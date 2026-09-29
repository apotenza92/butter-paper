#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  validateNativeAssemblyManifest,
  validateNativeMachO,
} from "./assemble-macos-production.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const repoRoot = resolve(dirname(scriptPath), "../../../..");
const teamId = "27JL2VERNC";
const identity = `Developer ID Application: Alexander Potenza (${teamId})`;
export const nativeSigningCertificateSha256 =
  "C20E3A100252224861FF8474DEBB21E5A120210E7CD61905EFDA0B6464E18594";

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function exactKeys(value, expected, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  if (JSON.stringify(Object.keys(value).sort()) !== JSON.stringify([...expected].sort())) {
    throw new Error(`${label} has missing or unknown fields`);
  }
}

function safeReceiptPath(value) {
  if (
    typeof value !== "string" ||
    !value.startsWith("Contents/") ||
    isAbsolute(value) ||
    value.includes("\\") ||
    value.split("/").some((part) => !part || part === "." || part === "..")
  ) {
    throw new Error("signed app assembly receipt contains an unsafe file path");
  }
  return value;
}

function releaseContract(channel) {
  if (channel === "stable") {
    return { productName: "Butter Paper", bundleIdentifier: "com.butterpaper.desktop" };
  }
  if (channel === "beta") {
    return { productName: "Butter Paper Beta", bundleIdentifier: "com.butterpaper.desktop.beta" };
  }
  throw new Error("native signed package channel must be stable or beta");
}

export function expectedNativeSignedCodeObjects(channel) {
  const contract = releaseContract(channel);
  return [
    {
      role: "application",
      path: `Contents/MacOS/${contract.productName}`,
      identifier: contract.bundleIdentifier,
      entitlements: {},
    },
    {
      role: "pdf-worker",
      path: "Contents/MacOS/butter-paper-pdf-worker",
      identifier: `${contract.bundleIdentifier}.pdf-worker`,
      entitlements: {},
    },
    {
      role: "camera-helper",
      path: "Contents/MacOS/butter-paper-signature-camera",
      identifier: `${contract.bundleIdentifier}.signature-camera`,
      entitlements: { "com.apple.security.device.camera": true },
    },
    {
      role: "phone-helper",
      path: "Contents/MacOS/butter-paper-signature-phone",
      identifier: `${contract.bundleIdentifier}.signature-phone`,
      entitlements: {},
    },
    {
      role: "pdfium",
      path: "Contents/Frameworks/libpdfium.dylib",
      identifier: `${contract.bundleIdentifier}.pdfium`,
      entitlements: {},
    },
  ];
}

export function parseNativeCodesignMetadata(output) {
  const values = new Map();
  const authorities = [];
  for (const line of String(output).split(/\r?\n/)) {
    if (line.startsWith("CodeDirectory ")) {
      values.set("CodeDirectory", line);
      continue;
    }
    const separator = line.indexOf("=");
    if (separator < 1) continue;
    const key = line.slice(0, separator).trim();
    const value = line.slice(separator + 1).trim();
    if (key === "Authority") authorities.push(value);
    else if (!values.has(key)) values.set(key, value);
  }
  return {
    authorities,
    cdHash: values.get("CDHash") ?? null,
    flags: values.get("CodeDirectory") ?? "",
    identifier: values.get("Identifier") ?? null,
    teamIdentifier: values.get("TeamIdentifier") ?? null,
    timestamp: values.get("Timestamp") ?? null,
    ticket: values.get("Notarization Ticket") ?? null,
  };
}

export function validateNativeCodesignMetadata(metadata, expectedIdentifier, label) {
  if (metadata.authorities[0] !== identity) {
    throw new Error(`${label} has an untrusted signing authority`);
  }
  if (metadata.teamIdentifier !== teamId) {
    throw new Error(`${label} has an unexpected signing team`);
  }
  if (metadata.identifier !== expectedIdentifier) {
    throw new Error(`${label} has identifier ${metadata.identifier ?? "missing"}`);
  }
  if (!metadata.flags.includes("runtime")) {
    throw new Error(`${label} does not have hardened runtime enabled`);
  }
  if (!metadata.timestamp) throw new Error(`${label} has no secure timestamp`);
  if (!metadata.cdHash) throw new Error(`${label} has no CDHash`);
}

export function validateNativeEntitlements(actual, expected, label) {
  if (!actual || typeof actual !== "object" || Array.isArray(actual)) {
    throw new Error(`${label} entitlements must be a dictionary`);
  }
  const actualKeys = Object.keys(actual).sort();
  const expectedKeys = Object.keys(expected).sort();
  if (JSON.stringify(actualKeys) !== JSON.stringify(expectedKeys)) {
    throw new Error(`${label} has missing or unexpected entitlements`);
  }
  for (const key of expectedKeys) {
    if (actual[key] !== true) throw new Error(`${label} entitlement ${key} must be true`);
  }
}

export function validateNativeArchitecture(output, target, label) {
  const expected = target === "aarch64-apple-darwin"
    ? "arm64"
    : target === "x86_64-apple-darwin"
      ? "x86_64"
      : null;
  const actual = String(output).trim().split(/\s+/).filter(Boolean);
  if (!expected || actual.length !== 1 || actual[0] !== expected) {
    throw new Error(`${label} architecture does not exactly match ${target}`);
  }
}

export function expectedNativeSignedInventory(receipt) {
  if (!Array.isArray(receipt?.files)) throw new Error("native assembly receipt files are invalid");
  const files = receipt.files.map(({ file }) => safeReceiptPath(file));
  if (new Set(files).size !== files.length) {
    throw new Error("native assembly receipt contains duplicate files");
  }
  return [
    ...files,
    "Contents/CodeResources",
    "Contents/Resources/native-assembly-receipt.json",
    "Contents/_CodeSignature/CodeResources",
  ].sort();
}

async function fileInventory(root, prefix = "") {
  const files = [];
  for (const entry of await readdir(join(root, prefix), { withFileTypes: true })) {
    const child = prefix ? `${prefix}/${entry.name}` : entry.name;
    const path = join(root, child);
    if (entry.isSymbolicLink()) throw new Error(`signed app contains symlink ${child}`);
    if (entry.isDirectory()) files.push(...await fileInventory(root, child));
    else if (entry.isFile()) {
      const metadata = await lstat(path);
      if (metadata.nlink !== 1) throw new Error(`signed app contains hard link ${child}`);
      files.push(child);
    } else throw new Error(`signed app contains special file ${child}`);
  }
  return files.sort();
}

function defaultRun(command, args, { input } = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    input,
    maxBuffer: 16 * 1024 * 1024,
  });
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed (${result.status}): ${output.trim()}`);
  }
  return output;
}

function parseEntitlements(output, run) {
  const start = output.indexOf("<?xml");
  if (start < 0) return {};
  return JSON.parse(run("plutil", ["-convert", "json", "-o", "-", "--", "-"], {
    input: output.slice(start),
  }));
}

function normalizeFingerprint(value) {
  const fingerprint = String(value).replace(/[^0-9a-f]/gi, "").toUpperCase();
  if (!/^[0-9A-F]{64}$/.test(fingerprint)) {
    throw new Error("trusted signing fingerprint must be a SHA-256 digest");
  }
  return fingerprint;
}

export async function verifySignedNativeMacosApp({
  appPath,
  manifestPath,
  fingerprint = nativeSigningCertificateSha256,
  run = defaultRun,
}) {
  const destination = resolve(appPath);
  const manifestBytes = await readFile(manifestPath);
  const unvalidatedManifest = JSON.parse(manifestBytes);
  const packageVersion = JSON.parse(await readFile(join(repoRoot, "package.json"), "utf8")).version;
  const manifest = validateNativeAssemblyManifest(
    unvalidatedManifest,
    packageVersion,
  );
  const contract = releaseContract(manifest.channel);
  if (basename(destination) !== `${contract.productName}.app`) {
    throw new Error("signed native app filename does not match its channel");
  }
  const receiptPath = join(destination, "Contents/Resources/native-assembly-receipt.json");
  const receipt = JSON.parse(await readFile(receiptPath, "utf8"));
  exactKeys(receipt, [
    "schema", "version", "signed", "channel", "target", "productName",
    "bundleIdentifier", "applicationVersion", "buildVersion",
    "minimumSystemVersion", "inputManifestSha256",
    "pdfiumStageReceiptSha256", "files",
  ], "pre-sign native assembly receipt");
  if (
    receipt?.schema !== "butter-paper/unsigned-native-macos-app" ||
    receipt?.version !== 1 ||
    receipt?.signed !== false ||
    receipt?.channel !== manifest.channel ||
    receipt?.target !== manifest.target ||
    receipt?.productName !== contract.productName ||
    receipt?.bundleIdentifier !== contract.bundleIdentifier ||
    receipt?.applicationVersion !== manifest.version ||
    receipt?.buildVersion !== manifest.buildVersion ||
    receipt?.minimumSystemVersion !== manifest.minimumSystemVersion ||
    receipt?.inputManifestSha256 !== sha256(manifestBytes) ||
    receipt?.pdfiumStageReceiptSha256 !== manifest.pdfiumStageReceiptSha256 ||
    !Array.isArray(receipt.files)
  ) {
    throw new Error("signed app does not contain its valid pre-sign assembly receipt");
  }
  const codeObjects = expectedNativeSignedCodeObjects(manifest.channel);
  const codePaths = new Set(codeObjects.map(({ path }) => path));
  const receiptPaths = new Set();
  for (const record of receipt.files) {
    exactKeys(record, ["file", "bytes", "sha256"], "signed app assembly file record");
    const file = safeReceiptPath(record.file);
    if (receiptPaths.has(file)) throw new Error("native assembly receipt contains duplicate files");
    receiptPaths.add(file);
    if (!Number.isSafeInteger(record.bytes) || record.bytes <= 0 || !/^[0-9a-f]{64}$/.test(record.sha256)) {
      throw new Error("signed app assembly receipt contains an invalid file record");
    }
    if (!codePaths.has(file)) {
      const bytes = await readFile(join(destination, file));
      if (bytes.length !== record.bytes || sha256(bytes) !== record.sha256) {
        throw new Error(`signed app changed immutable resource ${file}`);
      }
    }
  }
  for (const object of codeObjects) {
    if (!receiptPaths.has(object.path)) {
      throw new Error(`signed app assembly receipt is missing ${object.path}`);
    }
  }
  if (JSON.stringify(await fileInventory(destination)) !== JSON.stringify(expectedNativeSignedInventory(receipt))) {
    throw new Error("signed native app has missing or extra files");
  }

  run("codesign", ["--verify", "--deep", "--strict", "--verbose=4", destination]);
  const trustedFingerprint = normalizeFingerprint(fingerprint);
  const certificateDirectory = await mkdtemp(join(tmpdir(), "bp-native-cert-"));
  try {
    for (const object of codeObjects) {
      const path = join(destination, object.path);
      run("codesign", ["--verify", "--strict", "--verbose=2", path]);
      const metadata = parseNativeCodesignMetadata(run("codesign", ["-dvvv", path]));
      validateNativeCodesignMetadata(metadata, object.identifier, object.role);
      validateNativeArchitecture(run("lipo", ["-archs", path]), manifest.target, object.role);
      validateNativeMachO(
        await readFile(path),
        manifest.target,
        manifest.minimumSystemVersion,
        object.role,
        object.role === "pdfium" ? 6 : 2,
      );
      validateNativeEntitlements(
        parseEntitlements(run("codesign", ["-d", "--xml", "--entitlements", "-", path]), run),
        object.entitlements,
        object.role,
      );
      const prefix = join(certificateDirectory, `${object.role}-`);
      run("codesign", ["-d", `--extract-certificates=${prefix}`, path]);
      const leaf = `${prefix}0`;
      if (sha256(await readFile(leaf)).toUpperCase() !== trustedFingerprint) {
        throw new Error(`${object.role} leaf signing certificate is not trusted`);
      }
      if (object.role === "application") {
        const intermediate = `${prefix}1`;
        const root = `${prefix}2`;
        await Promise.all([readFile(intermediate), readFile(root)]);
        run("security", [
          "verify-cert", "-N", "-L", "-p", "codeSign",
          "-c", leaf, "-c", intermediate, "-r", root,
        ]);
        if (metadata.ticket !== "stapled") {
          throw new Error("application does not report a stapled notarisation ticket");
        }
      }
    }
  } finally {
    await rm(certificateDirectory, { recursive: true, force: true });
  }
  run("xcrun", ["stapler", "validate", destination]);
  run("spctl", ["--assess", "--type", "execute", "--verbose=4", destination]);
  return { channel: manifest.channel, target: manifest.target, codeObjectCount: codeObjects.length };
}

function argumentMap(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    if (!argv[index]?.startsWith("--") || !argv[index + 1] || values.has(argv[index])) {
      throw new Error("usage: verify-signed-macos-production.mjs --app APP --manifest FILE [--fingerprint SHA256]");
    }
    values.set(argv[index], argv[index + 1]);
  }
  for (const key of ["--app", "--manifest"]) {
    if (!values.has(key)) throw new Error(`${key} is required`);
  }
  return values;
}

if (process.argv[1] === scriptPath) {
  const values = argumentMap(process.argv.slice(2));
  const result = await verifySignedNativeMacosApp({
    appPath: resolve(values.get("--app")),
    manifestPath: resolve(values.get("--manifest")),
    fingerprint: values.get("--fingerprint") ?? nativeSigningCertificateSha256,
  });
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
}
