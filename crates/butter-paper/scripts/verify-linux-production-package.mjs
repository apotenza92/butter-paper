#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstat, mkdir, mkdtemp, readFile, rm, unlink, writeFile, link } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const TARGETS = {
  arm64: { label: "linux-arm64", rust: "aarch64-unknown-linux-gnu", machine: 183 },
  x86_64: { label: "linux-x64", rust: "x86_64-unknown-linux-gnu", machine: 62 },
};
const COMMON = ["gpui-migration", "butter-paper-pdf-worker", "butter-paper-signature-phone", "libpdfium.so", "README.md", "THIRD_PARTY_NOTICES.md", "PHONE_HELPER_THIRD_PARTY_NOTICES.md", "QRCP_LICENSE", "SIGNATURE_PAD_LICENSE", "butter-paper.png", "butter-paper.desktop", "install-user.sh", "uninstall-user.sh"];
const FORBIDDEN = /(?:development|dev)[-_\s.]*pdfium|pdfium[-_\s.]*(?:development|dev)|(?:pdfium.{0,80}(?<![a-z0-9])override(?![a-z0-9])|(?<![a-z0-9])override(?![a-z0-9]).{0,80}pdfium)|\b(?:BP_UPDATE_TEST_MODE|PDFIUM_(?:DEV|DEVELOPMENT|OVERRIDE)|DEV_PDFIUM|PDFIUM_OVERRIDE)\b/i;
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const jsonBytes = (value) => Buffer.from(`${JSON.stringify(value, null, 2)}\n`);
function fail(message) { throw new Error(message); }

function safeArtifactPath(value) {
  if (typeof value !== "string" || !value || value.includes("\\") || isAbsolute(value) || value.split("/").some((part) => !part || part === "." || part === "..")) fail("artifact path must be a safe relative POSIX path");
  return value;
}

function parseTar(bytes, expectedRoot) {
  const entries = new Map();
  let offset = 0;
  let ended = false;
  while (offset + 512 <= bytes.length) {
    const header = bytes.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) {
      if (offset + 1024 !== bytes.length || !bytes.subarray(offset + 512).every((byte) => byte === 0)) fail("tar archive must end with exactly two zero blocks");
      ended = true;
      break;
    }
    const storedChecksum = parseInt(header.toString("ascii", 148, 156).replace(/\0.*$/, "").trim(), 8);
    let checksum = 0;
    for (let i = 0; i < 512; i += 1) checksum += i >= 148 && i < 156 ? 0x20 : header[i];
    if (!Number.isInteger(storedChecksum) || checksum !== storedChecksum) fail("tar header checksum is invalid");
    const nul = (start, end) => {
      const value = header.subarray(start, end);
      const stop = value.indexOf(0);
      return value.subarray(0, stop < 0 ? value.length : stop).toString("utf8");
    };
    const name = nul(0, 100);
    const prefix = nul(345, 500);
    const path = prefix ? `${prefix}/${name}` : name;
    if (header.toString("ascii", 257, 263) !== "ustar\0" || header.toString("ascii", 263, 265) !== "00") fail("tar entry must use the supported ustar format");
    if (header[156] !== 48 && header[156] !== 53) fail(`tar entry has unsupported type: ${path}`);
    const type = String.fromCharCode(header[156]);
    const mode = parseInt(header.toString("ascii", 100, 108).replace(/\0.*$/, "").trim(), 8);
    const size = parseInt(header.toString("ascii", 124, 136).replace(/\0.*$/, "").trim(), 8);
    if (!Number.isSafeInteger(mode) || !Number.isSafeInteger(size) || mode < 0 || size < 0) fail(`tar entry has invalid numeric fields: ${path}`);
    if (header.subarray(108, 124).some((byte) => byte !== 48 && byte !== 0) || header.subarray(136, 148).some((byte) => byte !== 48 && byte !== 0)) fail(`tar entry has unexpected ownership or timestamp: ${path}`);
    if (type === "5") {
      if (path !== `${expectedRoot}/` || size !== 0 || mode !== 0o755 || entries.has(path)) fail("tar archive has an unexpected root directory entry");
      entries.set(path, { type, mode, bytes: Buffer.alloc(0) });
    } else {
      if (!path.startsWith(`${expectedRoot}/`)) fail(`tar entry escapes its package root: ${path}`);
      const name = path.slice(expectedRoot.length + 1);
      if (!name || name.includes("/") || name === "." || name === ".." || entries.has(path)) fail(`tar archive is not a flat one-root package: ${path}`);
      const start = offset + 512;
      const end = start + size;
      const paddedEnd = start + Math.ceil(size / 512) * 512;
      if (end > bytes.length || paddedEnd > bytes.length) fail(`tar entry is truncated: ${path}`);
      if (!bytes.subarray(end, paddedEnd).every((byte) => byte === 0)) fail(`tar entry padding is invalid: ${path}`);
      entries.set(path, { type, mode, bytes: Buffer.from(bytes.subarray(start, end)) });
      offset = paddedEnd;
      continue;
    }
    offset += 512;
  }
  if (!ended || !entries.has(`${expectedRoot}/`)) fail("tar archive is truncated or has no package root");
  return entries;
}

function validateElf(bytes, machine, label, allowedTypes = [3]) {
  const type = bytes.length >= 64 ? bytes.readUInt16LE(16) : -1;
  if (bytes.length < 64 || bytes[0] !== 0x7f || bytes.toString("ascii", 1, 4) !== "ELF" || bytes[4] !== 2 || bytes[5] !== 1 || !allowedTypes.includes(type) || bytes.readUInt16LE(18) !== machine) fail(`${label} is not a permitted 64-bit little-endian ELF binary for the requested architecture`);
}

function validatePng(bytes) {
  if (bytes.length < 45 || !bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) fail("archive product icon is not a valid 1024x1024 PNG");
  let offset = 8;
  let first = true;
  let sawImageData = false;
  while (offset + 12 <= bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const end = offset + 12 + length;
    if (end > bytes.length) fail("archive product icon PNG chunk is truncated");
    const type = bytes.toString("ascii", offset + 4, offset + 8);
    if (first && (type !== "IHDR" || length !== 13 || bytes.readUInt32BE(offset + 8) !== 1024 || bytes.readUInt32BE(offset + 12) !== 1024)) fail("archive product icon is not a valid 1024x1024 PNG");
    first = false;
    if (type === "IDAT") sawImageData = true;
    let crc = 0xffffffff;
    for (let i = offset + 4; i < end - 4; i += 1) {
      crc ^= bytes[i];
      for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
    if (((crc ^ 0xffffffff) >>> 0) !== bytes.readUInt32BE(end - 4)) fail("archive product icon PNG checksum is invalid");
    if (type === "IEND") {
      if (length !== 0 || end !== bytes.length || !sawImageData) fail("archive product icon PNG is incomplete");
      return;
    }
    offset = end;
  }
  fail("archive product icon PNG is incomplete");
}

function validatePdfiumReceipt(receipt, bytes, target) {
  if (receipt?.schemaVersion !== 1 || receipt?.purpose !== "production-distribution" || receipt?.productionApproved !== true || receipt?.target !== target.rust || receipt?.library?.path !== "libpdfium.so" || receipt.library.bytes !== bytes.length || receipt.library.sha256 !== sha256(bytes) || !/^[0-9a-f]{40}$/.test(receipt?.source?.revision ?? "") || typeof receipt?.build?.provenance !== "string" || !receipt.build.provenance || typeof receipt?.redistributionReview?.reference !== "string" || !receipt.redistributionReview.reference) fail("archive does not contain a matching approved production PDFium receipt");
}

function parseManifest(bytes, { target, version, revision, names }) {
  let manifest;
  try { manifest = JSON.parse(bytes.toString("utf8")); } catch { fail("archive MANIFEST.json is invalid"); }
  const expectedFiles = [...names.keys()].filter((name) => name !== "MANIFEST.json").sort();
  if (manifest?.schemaVersion !== 1 || manifest?.product !== "Butter Paper" || manifest?.target !== target.rust || manifest?.version !== version || manifest?.sourceRevision !== revision || !manifest.files || typeof manifest.files !== "object" || Array.isArray(manifest.files) || JSON.stringify(Object.keys(manifest.files).sort()) !== JSON.stringify(expectedFiles)) fail("package manifest identity or inventory does not match the requested stable candidate");
  for (const name of expectedFiles) {
    const actual = manifest.files[name];
    const entry = names.get(name);
    if (entry.type !== "0" || actual?.bytes !== entry.bytes.length || actual?.sha256 !== sha256(entry.bytes)) fail(`package manifest claim does not match ${name}`);
    const expectedMode = name === "gpui-migration" || name === "butter-paper-pdf-worker" || name === "butter-paper-signature-phone" || name.endsWith(".sh") ? 0o755 : 0o644;
    if (entry.mode !== expectedMode) fail(`tar entry has an unsafe or unexpected mode: ${name}`);
  }
  const receipt = names.get(`production-pdfium-linux-${target === TARGETS.arm64 ? "arm64" : "x86_64"}.json`);
  if (!receipt || manifest.pdfiumReceiptSha256 !== sha256(receipt.bytes)) fail("package PDFium receipt claim is invalid");
  return manifest;
}

export async function verifyLinuxProductionPackage({ inputArchive, packageManifestPath, verificationReceiptPath, architecture, version, revision, artifactPath = basename(resolve(inputArchive)) }) {
  const target = TARGETS[architecture];
  if (!target) fail("architecture must be x86_64 or arm64");
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\+[0-9A-Za-z.-]+)?$/.test(version ?? "")) fail("version must be a stable release version");
  if (!/^[0-9a-f]{40}$/.test(revision ?? "")) fail("revision must be a full lowercase Git commit");
  safeArtifactPath(artifactPath);
  const inputPath = resolve(inputArchive);
  const manifestOutput = resolve(packageManifestPath);
  const receiptOutput = resolve(verificationReceiptPath);
  if (manifestOutput === receiptOutput || inputPath === manifestOutput || inputPath === receiptOutput) fail("output paths must be distinct and must not overwrite the package archive");
  for (const path of [manifestOutput, receiptOutput]) {
    try { await lstat(path); fail(`output already exists: ${path}`); } catch (error) { if (error.code !== "ENOENT") throw error; }
  }
  const archiveStat = await lstat(inputPath);
  if (!archiveStat.isFile() || archiveStat.isSymbolicLink() || archiveStat.nlink !== 1) fail("input archive must be a regular single-link file");
  const archiveBytes = await readFile(inputPath);
  let tarBytes;
  try { tarBytes = execFileSync("xz", ["--decompress", "--stdout", "--check=crc64"], { input: archiveBytes, maxBuffer: 256 * 1024 * 1024 }); } catch (error) { fail(`cannot independently decompress Linux production archive: ${error.message}`); }
  const rootName = `butter-paper-linux-${architecture}-${version}`;
  const parsed = parseTar(tarBytes, rootName);
  const files = new Map([...parsed].filter(([path]) => path !== `${rootName}/`).map(([path, entry]) => [path.slice(rootName.length + 1), entry]));
  const receiptName = `production-pdfium-linux-${architecture}.json`;
  const expected = [...COMMON, receiptName, "MANIFEST.json"].sort();
  if (JSON.stringify([...files.keys()].sort()) !== JSON.stringify(expected)) fail("archive inventory is mixed, partial, or unexpected");
  const markerData = Buffer.concat([...files.values()].map(({ bytes }) => bytes));
  if (FORBIDDEN.test(markerData.toString("latin1"))) fail("archive contains a development PDFium or override marker");
  for (const name of ["PHONE_HELPER_THIRD_PARTY_NOTICES.md", "QRCP_LICENSE", "SIGNATURE_PAD_LICENSE"]) {
    if (!files.get(name)?.bytes.toString("utf8").trim()) fail(`archive ${name} must not be empty`);
  }
  let manifest;
  try { manifest = parseManifest(files.get("MANIFEST.json").bytes, { target, version, revision, names: files }); } catch (error) { throw error; }
  validatePdfiumReceipt(JSON.parse(files.get(receiptName).bytes.toString("utf8")), files.get("libpdfium.so").bytes, target);
  for (const name of ["gpui-migration", "butter-paper-pdf-worker", "libpdfium.so"]) validateElf(files.get(name).bytes, target.machine, name);
  validateElf(files.get("butter-paper-signature-phone").bytes, target.machine, "butter-paper-signature-phone", [2, 3]);
  const readme = files.get("README.md").bytes.toString("utf8");
  if (!/runtime dependencies/i.test(readme) || !/libc|glibc/i.test(readme)) fail("archive README does not document Linux runtime dependencies");
  const desktop = files.get("butter-paper.desktop").bytes.toString("utf8");
  if (!/^\[Desktop Entry\]$/m.test(desktop) || !/^Exec=butter-paper-gpui %F$/m.test(desktop) || !/^Icon=butter-paper$/m.test(desktop) || !/^MimeType=application\/pdf;$/m.test(desktop)) fail("archive desktop entry is missing the app launcher or PDF association");
  validatePng(files.get("butter-paper.png").bytes);
  for (const name of ["install-user.sh", "uninstall-user.sh"]) {
    const script = files.get(name).bytes.toString("utf8");
    if (!script.startsWith("#!/bin/sh\nset -eu\n") || !script.includes("XDG_DATA_HOME") || !script.includes("$HOME/.local/bin") || !script.includes("$data/butter-paper/") || !script.includes("update-desktop-database") || !script.includes("update-mime-database") || script.includes("sudo ") || script.includes("pkexec")) fail(`archive ${name} is not a safe per-user integration script`);
  }
  const installer = files.get("install-user.sh").bytes.toString("utf8");
  const uninstaller = files.get("uninstall-user.sh").bytes.toString("utf8");
  if (
    !installer.includes("cp -p -- \"$root/$name\" \"$stage/$name\"")
    || !installer.includes("mv -T -n -- \"$stage\" \"$target\"")
    || !installer.includes("BP_EXEC=\"$launcher\" awk")
    || !uninstaller.includes("actual_count=0")
    || !uninstaller.includes("Versioned install directory contains unrelated files; preserved it.")
    || !uninstaller.includes("cmp -s \"$root/$name\" \"$target/$name\"")
    || !uninstaller.includes("cmp -s \"$launcher_tmp\" \"$launcher\"")
    || !uninstaller.includes("rmdir -- \"$target\"")
  ) fail("Linux desktop scripts do not install or remove the identified versioned package safely");
  const artifact = { path: artifactPath, bytes: archiveBytes.length, sha256: sha256(archiveBytes) };
  const identity = { target: target.label, channel: "stable", version, sourceRevision: revision };
  const packageManifest = { schema: "butter-paper/package-manifest", schemaVersion: 1, ...identity, artifact, package: { target: target.rust, pdfiumReceiptSha256: manifest.pdfiumReceiptSha256 } };
  const verificationReceipt = { schema: "butter-paper/package-verification", schemaVersion: 1, ...identity, artifact, verified: true };
  const outputs = [[manifestOutput, jsonBytes(packageManifest)], [receiptOutput, jsonBytes(verificationReceipt)]];
  const stages = [];
  const published = [];
  try {
    for (const [destination] of outputs) {
      await mkdir(dirname(destination), { recursive: true });
      const dir = await mkdtemp(join(dirname(destination), ".bp-linux-verify-stage-"));
      stages.push(dir);
    }
    for (let i = 0; i < outputs.length; i += 1) await writeFile(join(stages[i], basename(outputs[i][0])), outputs[i][1], { flag: "wx", mode: 0o644 });
    for (let i = 0; i < outputs.length; i += 1) {
      await link(join(stages[i], basename(outputs[i][0])), outputs[i][0]);
      published.push(outputs[i][0]);
    }
    return { artifact, packageManifest: manifestOutput, verificationReceipt: receiptOutput };
  } catch (error) {
    for (const path of published.reverse()) { try { await unlink(path); } catch {} }
    throw error;
  } finally {
    for (const dir of stages) await rm(dir, { recursive: true, force: true });
  }
}

async function main(args) {
  const values = new Map();
  for (let i = 0; i < args.length; i += 1) {
    const key = args[i];
    if (!["--input", "--package-manifest", "--verification-receipt", "--architecture", "--version", "--revision", "--artifact-path"].includes(key) || !args[i + 1] || values.has(key)) fail("usage: verify-linux-production-package.mjs --input ARCHIVE --package-manifest JSON --verification-receipt JSON --architecture x86_64|arm64 --version VERSION --revision GIT_SHA [--artifact-path RELATIVE_PATH]");
    values.set(key, args[++i]);
  }
  for (const key of ["--input", "--package-manifest", "--verification-receipt", "--architecture", "--version", "--revision"]) if (!values.has(key)) fail("usage: verify-linux-production-package.mjs --input ARCHIVE --package-manifest JSON --verification-receipt JSON --architecture x86_64|arm64 --version VERSION --revision GIT_SHA [--artifact-path RELATIVE_PATH]");
  await verifyLinuxProductionPackage({ inputArchive: values.get("--input"), packageManifestPath: values.get("--package-manifest"), verificationReceiptPath: values.get("--verification-receipt"), architecture: values.get("--architecture"), version: values.get("--version"), revision: values.get("--revision"), artifactPath: values.get("--artifact-path") });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main(process.argv.slice(2)).catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
