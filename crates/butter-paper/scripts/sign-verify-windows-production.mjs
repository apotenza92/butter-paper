#!/usr/bin/env node

import { createHash } from "node:crypto";
import { link, lstat, mkdir, mkdtemp, readFile, rm, unlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve, sep } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const TARGETS = {
  x86_64: { target: "x86_64-pc-windows-msvc", label: "windows-x64" },
  arm64: { target: "aarch64-pc-windows-msvc", label: "windows-arm64" },
};
const SIGNED_FILES = [
  "gpui-migration.exe",
  "butter-paper-pdf-worker.exe",
  "butter-paper-signature-phone.exe",
  "pdfium.dll",
];
const SHA256 = /^[0-9a-f]{64}$/;

function fail(message) { throw new Error(message); }
function sha256(bytes) { return createHash("sha256").update(bytes).digest("hex"); }
function jsonBytes(value) { return Buffer.from(`${JSON.stringify(value, null, 2)}\n`); }

function run(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
  if (result.error || result.status !== 0) {
    throw new Error(`${command} failed (${result.status ?? result.error?.message}): ${String(result.stdout ?? "")}${String(result.stderr ?? "")}`.trim());
  }
  return `${result.stdout ?? ""}${result.stderr ?? ""}`;
}

function parseZip(bytes) {
  const files = new Map();
  let offset = 0;
  while (offset + 4 <= bytes.length && bytes.readUInt32LE(offset) === 0x04034b50) {
    if (offset + 30 > bytes.length) fail("package ZIP has a truncated local header");
    const flags = bytes.readUInt16LE(offset + 6);
    const method = bytes.readUInt16LE(offset + 8);
    const crc = bytes.readUInt32LE(offset + 14);
    const compressed = bytes.readUInt32LE(offset + 18);
    const uncompressed = bytes.readUInt32LE(offset + 22);
    const nameLength = bytes.readUInt16LE(offset + 26);
    const extraLength = bytes.readUInt16LE(offset + 28);
    if (flags !== 0x0800 || method !== 0 || compressed !== uncompressed) fail("package ZIP is not in the production package format");
    const start = offset + 30 + nameLength + extraLength;
    const end = start + compressed;
    if (end > bytes.length) fail("package ZIP has a truncated file");
    const name = bytes.toString("utf8", offset + 30, offset + 30 + nameLength);
    if (!name || name.includes("/") || name.includes("\\") || name === "." || name === ".." || files.has(name)) fail("package ZIP contains an unsafe or duplicate path");
    const content = bytes.subarray(start, end);
    if (crc32(content) !== crc) fail(`package ZIP CRC mismatch for ${name}`);
    files.set(name, Buffer.from(content));
    offset = end;
  }
  if (!files.size || offset + 4 > bytes.length || bytes.readUInt32LE(offset) !== 0x02014b50) fail("package ZIP has no valid central directory");
  const centralNames = [];
  while (offset + 4 <= bytes.length && bytes.readUInt32LE(offset) === 0x02014b50) {
    if (offset + 46 > bytes.length) fail("package ZIP has a truncated central directory");
    const nameLength = bytes.readUInt16LE(offset + 28), extraLength = bytes.readUInt16LE(offset + 30), commentLength = bytes.readUInt16LE(offset + 32);
    const nameEnd = offset + 46 + nameLength;
    const next = nameEnd + extraLength + commentLength;
    if (next > bytes.length || bytes.readUInt16LE(offset + 8) !== 0x0800 || bytes.readUInt16LE(offset + 10) !== 0 || bytes.readUInt32LE(offset + 42) >= offset) fail("package ZIP central directory is not in the production package format");
    const name = bytes.toString("utf8", offset + 46, nameEnd);
    if (!name || centralNames.includes(name)) fail("package ZIP central directory has duplicate or empty paths");
    centralNames.push(name);
    offset = next;
  }
  if (offset + 22 > bytes.length || bytes.readUInt32LE(offset) !== 0x06054b50 || offset + 22 !== bytes.length || bytes.readUInt16LE(offset + 8) !== centralNames.length || bytes.readUInt16LE(offset + 10) !== centralNames.length || JSON.stringify([...centralNames].sort()) !== JSON.stringify([...files.keys()].sort())) {
    fail("package ZIP central directory inventory does not match local files");
  }
  return files;
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function deterministicZip(files) {
  const local = [], central = [];
  let offset = 0;
  for (const [name, content] of [...files].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) {
    const filename = Buffer.from(name, "utf8"), crc = crc32(content);
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0); header.writeUInt16LE(20, 4); header.writeUInt16LE(0x0800, 6);
    header.writeUInt16LE(0, 8); header.writeUInt16LE(0, 10); header.writeUInt16LE(0x21, 12);
    header.writeUInt32LE(crc, 14); header.writeUInt32LE(content.length, 18); header.writeUInt32LE(content.length, 22);
    header.writeUInt16LE(filename.length, 26);
    local.push(header, filename, content);
    const directory = Buffer.alloc(46);
    directory.writeUInt32LE(0x02014b50, 0); directory.writeUInt16LE(0x0314, 4); directory.writeUInt16LE(20, 6);
    directory.writeUInt16LE(0x0800, 8); directory.writeUInt16LE(0, 10); directory.writeUInt16LE(0, 12); directory.writeUInt16LE(0x21, 14);
    directory.writeUInt32LE(crc, 16); directory.writeUInt32LE(content.length, 20); directory.writeUInt32LE(content.length, 24);
    directory.writeUInt16LE(filename.length, 28); directory.writeUInt32LE(offset, 42);
    central.push(directory, filename); offset += header.length + filename.length + content.length;
  }
  const centralBytes = Buffer.concat(central), localBytes = Buffer.concat(local), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(0, 4); end.writeUInt16LE(0, 6);
  end.writeUInt16LE(files.size, 8); end.writeUInt16LE(files.size, 10); end.writeUInt32LE(centralBytes.length, 12);
  end.writeUInt32LE(localBytes.length, 16);
  return Buffer.concat([localBytes, centralBytes, end]);
}

function validatePackage(files, { architecture, version, revision }) {
  const target = TARGETS[architecture];
  if (!target) fail("architecture must be x86_64 or arm64");
  const manifestBytes = files.get("MANIFEST.json");
  if (!manifestBytes) fail("package is missing MANIFEST.json");
  let manifest;
  try { manifest = JSON.parse(manifestBytes.toString("utf8")); } catch { fail("package MANIFEST.json is invalid"); }
  if (manifest.schemaVersion !== 1 || manifest.product !== "Butter Paper" || manifest.target !== target.target || manifest.version !== version || manifest.sourceRevision !== revision || !manifest.files || typeof manifest.files !== "object") {
    fail("package identity does not match requested Windows production target");
  }
  const expectedNames = [...Object.keys(manifest.files), "MANIFEST.json"].sort();
  if (JSON.stringify([...files.keys()].sort()) !== JSON.stringify(expectedNames)) fail("package inventory does not match its manifest");
  const required = [...SIGNED_FILES, "README.md", "THIRD_PARTY_NOTICES.md", "PHONE_HELPER_THIRD_PARTY_NOTICES.md", "QRCP_LICENSE", "SIGNATURE_PAD_LICENSE", "butter-paper.ico", "install.ps1", "uninstall.ps1", `production-pdfium-windows-${architecture}.json`].sort();
  if (JSON.stringify(Object.keys(manifest.files).sort()) !== JSON.stringify(required)) fail("package manifest has a mixed, partial, or unexpected inventory");
  for (const [name, record] of Object.entries(manifest.files)) {
    const content = files.get(name);
    if (!content || record.bytes !== content.length || record.sha256 !== sha256(content)) fail(`package manifest claim does not match ${name}`);
  }
  const receiptHash = sha256(files.get(`production-pdfium-windows-${architecture}.json`));
  if (manifest.pdfiumReceiptSha256 !== receiptHash) fail("package PDFium receipt claim is invalid");
  return { target, manifest };
}

function powershellVerifyScript(path) {
  const quoted = path.replaceAll("'", "''");
  return `$s=Get-AuthenticodeSignature -LiteralPath '${quoted}'; $ts=$null; if ($s.TimeStamperCertificate) { $ts=$s.TimeStamperCertificate.Thumbprint }; [pscustomobject]@{Status=[string]$s.Status;SignerThumbprint=[string]$s.SignerCertificate.Thumbprint;TimestampThumbprint=$ts;Timestamped=($null -ne $s.TimeStamperCertificate)} | ConvertTo-Json -Compress`;
}

function parseVerification(output) {
  let result;
  try { result = JSON.parse(String(output).trim()); } catch { fail("Authenticode verification runner returned invalid JSON"); }
  return result;
}

export async function signVerifyWindowsProduction({
  inputArchive, outputArchive, packageManifestPath, verificationReceiptPath, architecture,
  version, revision, certificateThumbprint, timestampUrl,
  artifactPath = basename(resolve(outputArchive)),
  signtool = "signtool.exe", powershell = "powershell.exe", runner = run, publishLink = link,
}) {
  if (!/^(?:[0-9a-f]{40})$/.test(revision ?? "")) fail("revision must be a full lowercase Git commit");
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\+[0-9A-Za-z.-]+)?$/.test(version ?? "")) fail("version must be a stable release version");
  const thumbprint = String(certificateThumbprint ?? "").replaceAll(" ", "").toUpperCase();
  if (!/^[0-9A-F]{40}$/.test(thumbprint)) fail("an explicit SHA-1 certificate thumbprint is required");
  let timestamp;
  try { timestamp = new URL(timestampUrl); } catch { fail("an explicit RFC3161 timestamp URL is required"); }
  if (timestamp.protocol !== "https:" || !timestamp.hostname || timestamp.username || timestamp.password) fail("timestamp URL must be an HTTPS RFC3161 endpoint");
  if (typeof artifactPath !== "string" || !artifactPath || artifactPath.includes("\\") || artifactPath.startsWith("/") || artifactPath.split("/").some((part) => !part || part === "." || part === "..")) fail("artifact path must be a safe relative POSIX path");
  if ([inputArchive, outputArchive, packageManifestPath, verificationReceiptPath].some((value) => typeof value !== "string" || !value)) fail("input archive and all output paths are required");
  const inputPath = resolve(inputArchive), archiveOutput = resolve(outputArchive), manifestOutput = resolve(packageManifestPath), receiptOutput = resolve(verificationReceiptPath);
  if (new Set([archiveOutput, manifestOutput, receiptOutput]).size !== 3) fail("output paths must be distinct");
  if ([archiveOutput, manifestOutput, receiptOutput].includes(inputPath)) fail("output paths must not overwrite the unsigned package");
  for (const path of [archiveOutput, manifestOutput, receiptOutput]) {
    try { await lstat(path); fail(`output already exists: ${path}`); } catch (error) { if (error.code !== "ENOENT") throw error; }
  }
  const stat = await lstat(inputPath);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) fail("input archive must be a regular single-link file");
  const files = parseZip(await readFile(inputPath));
  const { target, manifest } = validatePackage(files, { architecture, version, revision });
  const temp = await mkdtemp(join(tmpdir(), "bp-windows-sign-"));
  try {
    const signatures = [];
    for (const name of SIGNED_FILES) {
      const path = join(temp, name);
      await writeFile(path, files.get(name), { flag: "wx", mode: 0o600 });
      await runner(signtool, ["sign", "/sha1", thumbprint, "/fd", "SHA256", "/tr", timestamp.href, "/td", "SHA256", path]);
      const output = await runner(powershell, ["-NoProfile", "-NonInteractive", "-Command", powershellVerifyScript(path)]);
      const verified = parseVerification(output);
      if (verified.Status !== "Valid") fail(`${name} Authenticode status is ${verified.Status ?? "missing"}`);
      if (String(verified.SignerThumbprint ?? "").replaceAll(" ", "").toUpperCase() !== thumbprint) fail(`${name} signer thumbprint does not match the requested certificate`);
      const timestampThumbprint = String(verified.TimestampThumbprint ?? "");
      if (verified.Timestamped !== true || !/^[0-9a-f]{40}$/i.test(timestampThumbprint)) fail(`${name} has no valid independently verified RFC3161 timestamp certificate thumbprint`);
      const signed = await readFile(path);
      files.set(name, signed);
      signatures.push({ path: name, sha256: sha256(signed), bytes: signed.length, status: "Valid", signerThumbprint: thumbprint, timestamped: true, timestampThumbprint: timestampThumbprint.toUpperCase() });
    }
    for (const name of SIGNED_FILES) manifest.files[name] = { bytes: files.get(name).length, sha256: sha256(files.get(name)) };
    files.set("MANIFEST.json", jsonBytes(manifest));
    const archiveBytes = deterministicZip(files);
    const artifact = { path: artifactPath, bytes: archiveBytes.length, sha256: sha256(archiveBytes) };
    const packageManifest = {
      schema: "butter-paper/package-manifest", schemaVersion: 1, target: target.label, channel: "stable", version, sourceRevision: revision,
      artifact, package: { target: target.target, pdfiumReceiptSha256: manifest.pdfiumReceiptSha256 }, signatures,
    };
    const verificationReceipt = {
      schema: "butter-paper/package-verification", schemaVersion: 1, target: target.label, channel: "stable", version, sourceRevision: revision,
      artifact, verified: true, signerThumbprint: thumbprint, timestampUrl: timestamp.href, signatures,
    };
    const outputs = [[archiveOutput, archiveBytes], [manifestOutput, jsonBytes(packageManifest)], [receiptOutput, jsonBytes(verificationReceipt)]];
    const staging = [];
    const published = [];
    try {
      for (const [destination, bytes] of outputs) {
        await mkdir(dirname(destination), { recursive: true });
        const directory = await mkdtemp(join(dirname(destination), ".bp-windows-sign-stage-"));
        staging.push(directory);
        const stagedPath = join(directory, basename(destination));
        await writeFile(stagedPath, bytes, { flag: "wx", mode: 0o600 });
      }
      for (let index = 0; index < outputs.length; index += 1) {
        const [destination] = outputs[index];
        const stagedPath = join(staging[index], basename(destination));
        // Hard-link publication is atomic and fails if the destination already exists.
        await publishLink(stagedPath, destination);
        published.push(destination);
      }
      return { artifact, packageManifest: manifestOutput, verificationReceipt: receiptOutput };
    } catch (error) {
      for (const path of published.reverse()) {
        try { await unlink(path); } catch {}
      }
      throw error;
    } finally {
      for (const directory of staging) await rm(directory, { recursive: true, force: true });
    }
  } finally {
    await rm(temp, { recursive: true, force: true });
  }
}

export async function verifyUnsignedWindowsProduction({
  inputArchive, outputArchive, packageManifestPath, verificationReceiptPath,
  architecture, version, revision,
  artifactPath = basename(resolve(outputArchive)),
  publishLink = link,
}) {
  if (!/^(?:[0-9a-f]{40})$/.test(revision ?? "")) fail("revision must be a full lowercase Git commit");
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\+[0-9A-Za-z.-]+)?$/.test(version ?? "")) fail("version must be a stable release version");
  if (typeof artifactPath !== "string" || !artifactPath || artifactPath.includes("\\") || artifactPath.startsWith("/") || artifactPath.split("/").some((part) => !part || part === "." || part === "..")) fail("artifact path must be a safe relative POSIX path");
  if ([inputArchive, outputArchive, packageManifestPath, verificationReceiptPath].some((value) => typeof value !== "string" || !value)) fail("input archive and all output paths are required");
  const inputPath = resolve(inputArchive), archiveOutput = resolve(outputArchive), manifestOutput = resolve(packageManifestPath), receiptOutput = resolve(verificationReceiptPath);
  if (new Set([archiveOutput, manifestOutput, receiptOutput]).size !== 3) fail("output paths must be distinct");
  if ([archiveOutput, manifestOutput, receiptOutput].includes(inputPath)) fail("output paths must not overwrite the source package");
  for (const path of [archiveOutput, manifestOutput, receiptOutput]) {
    try { await lstat(path); fail(`output already exists: ${path}`); } catch (error) { if (error.code !== "ENOENT") throw error; }
  }
  const stat = await lstat(inputPath);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) fail("input archive must be a regular single-link file");
  const archiveBytes = await readFile(inputPath);
  const files = parseZip(archiveBytes);
  const { target, manifest } = validatePackage(files, { architecture, version, revision });
  const artifact = { path: artifactPath, bytes: archiveBytes.length, sha256: sha256(archiveBytes) };
  const signaturePolicy = "unsigned-user-authorised";
  const packageManifest = {
    schema: "butter-paper/package-manifest", schemaVersion: 1, target: target.label, channel: "stable", version, sourceRevision: revision,
    artifact, package: { target: target.target, pdfiumReceiptSha256: manifest.pdfiumReceiptSha256 },
    signaturePolicy, signatures: [],
  };
  const verificationReceipt = {
    schema: "butter-paper/package-verification", schemaVersion: 1, target: target.label, channel: "stable", version, sourceRevision: revision,
    artifact, verified: true, integrityVerified: true, signaturePolicy, signatures: [],
  };
  const outputs = [[archiveOutput, archiveBytes], [manifestOutput, jsonBytes(packageManifest)], [receiptOutput, jsonBytes(verificationReceipt)]];
  const staging = [];
  const published = [];
  try {
    for (const [destination, bytes] of outputs) {
      await mkdir(dirname(destination), { recursive: true });
      const directory = await mkdtemp(join(dirname(destination), ".bp-windows-unsigned-stage-"));
      staging.push(directory);
      await writeFile(join(directory, basename(destination)), bytes, { flag: "wx", mode: 0o600 });
    }
    for (let index = 0; index < outputs.length; index += 1) {
      const [destination] = outputs[index];
      await publishLink(join(staging[index], basename(destination)), destination);
      published.push(destination);
    }
    return { artifact, packageManifest: manifestOutput, verificationReceipt: receiptOutput, signaturePolicy };
  } catch (error) {
    for (const path of published.reverse()) {
      try { await unlink(path); } catch {}
    }
    throw error;
  } finally {
    for (const directory of staging) await rm(directory, { recursive: true, force: true });
  }
}

async function main(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    const key = args[index];
    if (!key?.startsWith("--") || options[key.slice(2)] !== undefined || args[index + 1] === undefined) fail("invalid or duplicate option");
    options[key.slice(2)] = args[index + 1];
  }
  const required = ["input", "output", "package-manifest", "verification-receipt", "architecture", "version", "revision", "certificate-thumbprint", "timestamp-url"];
  if (Object.keys(options).some((key) => ![...required, "artifact-path"].includes(key)) || required.some((key) => options[key] === undefined)) {
    fail("usage: sign-verify-windows-production.mjs --input ZIP --output ZIP --package-manifest JSON --verification-receipt JSON --architecture x86_64|arm64 --version VERSION --revision GIT_SHA --certificate-thumbprint SHA1 --timestamp-url HTTPS_URL [--artifact-path RELATIVE_PATH]");
  }
  console.log(JSON.stringify(await signVerifyWindowsProduction({
    inputArchive: options.input, outputArchive: options.output, packageManifestPath: options["package-manifest"],
    verificationReceiptPath: options["verification-receipt"], architecture: options.architecture,
    version: options.version, revision: options.revision, certificateThumbprint: options["certificate-thumbprint"], timestampUrl: options["timestamp-url"],
    artifactPath: options["artifact-path"],
  }), null, 2));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => { console.error(error.message); process.exitCode = 1; });
}
