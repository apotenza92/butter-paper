#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  chmod,
  lstat,
  mkdir,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const PINNED_PDFIUM_REVISION = "91b9d569b34be4f38eed7b3c49b227356c3aadad";
const PINNED_SHARED_LIBRARY_PATCH_SHA256 = "1e521b48561c51a63425baeec7c74c1edaf65956b6e6297d98aff57f2cc2ee40";
const PINNED_DEPENDENCY_POLICY_PATCH_SHA256 = "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b";
const targets = new Map([
  ["aarch64-pc-windows-msvc", { os: "windows", arch: "arm64", machine: 0xaa64, library: "pdfium.dll", receipt: "production-pdfium-windows-arm64.json" }],
  ["x86_64-pc-windows-msvc", { os: "windows", arch: "x86_64", machine: 0x8664, library: "pdfium.dll", receipt: "production-pdfium-windows-x86_64.json" }],
  ["aarch64-unknown-linux-gnu", { os: "linux", arch: "arm64", machine: 183, library: "libpdfium.so", receipt: "production-pdfium-linux-arm64.json" }],
  ["x86_64-unknown-linux-gnu", { os: "linux", arch: "x86_64", machine: 62, library: "libpdfium.so", receipt: "production-pdfium-linux-x86_64.json" }],
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function safeRelativePath(value, label) {
  if (
    typeof value !== "string" || value.length === 0 || isAbsolute(value) ||
    /^[A-Za-z]:[\\/]/.test(value) || value.includes("\\") ||
    value.split("/").some((part) => part === "" || part === "." || part === "..")
  ) throw new Error(`${label} must be a safe relative path`);
  return value;
}

function requireHash(value, label) {
  if (!/^[0-9a-f]{64}$/.test(value ?? "")) throw new Error(`${label} must be a lowercase SHA-256 digest`);
}

function requireRecord(record, label) {
  if (!record || typeof record !== "object" || Array.isArray(record)) throw new Error(`${label} must be an object`);
  safeRelativePath(record.path, `${label}.path`);
  requireHash(record.sha256, `${label}.sha256`);
  if (!Number.isSafeInteger(record.bytes) || record.bytes <= 0) throw new Error(`${label}.bytes must be a positive safe integer`);
}

function hasDownloadField(value) {
  if (!value || typeof value !== "object") return false;
  return Object.entries(value).some(([key, child]) => /^(?:url|download|downloadUrl)$/i.test(key) || hasDownloadField(child));
}

export function validateNonMacProductionManifest(manifest) {
  if (manifest?.schemaVersion !== 1 || manifest?.purpose !== "production-distribution" || manifest?.productionApproved !== true) {
    throw new Error("production PDFium manifest is not explicitly approved for distribution");
  }
  if (
    manifest.wrapper?.package !== "pdfium-render" || manifest.wrapper?.version !== "0.9.4" ||
    manifest.wrapper?.revision !== "6cee8b9a3951832ac0ff62ce4c32800278001cb8" ||
    manifest.wrapper?.feature !== "pdfium_7881"
  ) throw new Error("production PDFium wrapper pins do not match the reviewed Rust graph");
  if (manifest.source?.repository !== "https://pdfium.googlesource.com/pdfium" || manifest.source?.revision !== PINNED_PDFIUM_REVISION) {
    throw new Error("production PDFium source revision does not match the app-reviewed pin");
  }
  if (
    manifest.build?.apiBuild !== 7881 || manifest.build?.v8 !== false || manifest.build?.xfa !== false ||
    manifest.build?.debug !== false || manifest.build?.sharedLibraryPatchSha256 !== PINNED_SHARED_LIBRARY_PATCH_SHA256 ||
    manifest.build?.dependencyPolicyPatchSha256 !== PINNED_DEPENDENCY_POLICY_PATCH_SHA256 ||
    !manifest.build?.toolchain || Object.keys(manifest.build.toolchain).length === 0 ||
    Object.values(manifest.build.toolchain).some((value) => typeof value !== "string" || value.length === 0)
  ) throw new Error("production PDFium build policy or toolchain provenance is incomplete");
  requireRecord(manifest.redistributionReview, "redistributionReview");
  requireRecord(manifest.supplierReview, "supplierReview");
  if (!Array.isArray(manifest.artifacts) || hasDownloadField(manifest.artifacts)) {
    throw new Error("production PDFium artifacts must be local inputs, not download locations");
  }
  const names = manifest.artifacts.map((artifact) => artifact?.target);
  if (names.length === 0 || new Set(names).size !== names.length) throw new Error("production PDFium manifest must contain a nonempty set of unique targets");
  for (const artifact of manifest.artifacts) {
    const target = targets.get(artifact.target);
    if (!target) throw new Error(`unsupported production PDFium target ${artifact.target}`);
    if (basename(artifact.library?.path ?? "") !== target.library) throw new Error(`${artifact.target}.library must be named ${target.library}`);
    for (const [name, record] of Object.entries({ library: artifact.library, sbom: artifact.sbom, provenance: artifact.provenance, gnArgs: artifact.gnArgs })) {
      requireRecord(record, `${artifact.target}.${name}`);
    }
    safeRelativePath(artifact.noticeRoot, `${artifact.target}.noticeRoot`);
    if (!Array.isArray(artifact.notices) || artifact.notices.length === 0) throw new Error(`${artifact.target}.notices must contain the exact notice inventory`);
    const noticePaths = artifact.notices.map((notice, index) => {
      requireRecord(notice, `${artifact.target}.notices[${index}]`);
      return safeRelativePath(notice.path, `${artifact.target}.notices[${index}].path`);
    });
    if (new Set(noticePaths).size !== noticePaths.length) throw new Error(`${artifact.target}.notices contains duplicate paths`);
  }
  return manifest;
}

async function verifiedFile(root, record, label) {
  const safePath = safeRelativePath(record.path, `${label}.path`);
  const path = resolve(root, safePath);
  const within = relative(root, path);
  if (!within || within === ".." || within.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`) || isAbsolute(within)) throw new Error(`${label} escapes the artifact root`);
  let current = root;
  for (const segment of safePath.split("/")) {
    current = join(current, segment);
    const metadata = await lstat(current);
    if (metadata.isSymbolicLink()) throw new Error(`${label} must not traverse a symlink`);
  }
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.nlink !== 1) throw new Error(`${label} must be a regular single-link file`);
  const bytes = await readFile(path);
  if (bytes.length !== record.bytes || sha256(bytes) !== record.sha256) throw new Error(`${label} does not match its reviewed byte receipt`);
  return { path, bytes };
}

async function inventoryDirectory(root, prefix = "") {
  const directory = resolve(root, prefix);
  const metadata = await lstat(directory);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) throw new Error("PDFium notice root must be a real directory");
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const child = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isSymbolicLink()) throw new Error(`PDFium notice inventory contains symlink ${child}`);
    if (entry.isDirectory()) files.push(...await inventoryDirectory(root, child));
    else if (entry.isFile()) {
      const childMetadata = await lstat(resolve(root, child));
      if (childMetadata.nlink !== 1) throw new Error(`PDFium notice inventory contains hard link ${child}`);
      files.push(child);
    } else throw new Error(`PDFium notice inventory contains special file ${child}`);
  }
  return files.sort();
}

async function assertDirectory(root, path, label) {
  let current = root;
  for (const segment of safeRelativePath(path, label).split("/")) {
    current = join(current, segment);
    const metadata = await lstat(current);
    if (metadata.isSymbolicLink()) throw new Error(`${label} must not traverse a symlink`);
  }
  if (!(await lstat(current)).isDirectory()) throw new Error(`${label} must name a directory`);
  return current;
}

function validateBinary(bytes, target, label) {
  if (target.os === "windows") {
    if (bytes.length < 0x40 || bytes.toString("ascii", 0, 2) !== "MZ") throw new Error(`${label} is not a valid PE DLL`);
    const peOffset = bytes.readUInt32LE(0x3c);
    if (peOffset < 0x40 || peOffset + 24 > bytes.length || bytes.toString("ascii", peOffset, peOffset + 4) !== "PE\0\0") throw new Error(`${label} is not a valid PE DLL`);
    if (bytes.readUInt16LE(peOffset + 4) !== target.machine) throw new Error(`${label} PE machine does not match target`);
    const optionalSize = bytes.readUInt16LE(peOffset + 20);
    const characteristics = bytes.readUInt16LE(peOffset + 22);
    if (optionalSize < 2 || peOffset + 24 + optionalSize > bytes.length || bytes.readUInt16LE(peOffset + 24) !== 0x20b || (characteristics & 0x2000) === 0) {
      throw new Error(`${label} must be a 64-bit PE DLL`);
    }
    return;
  }
  if (bytes.length < 64 || bytes[0] !== 0x7f || bytes.toString("ascii", 1, 4) !== "ELF") throw new Error(`${label} is not a valid ELF shared object`);
  if (bytes[4] !== 2 || bytes[5] !== 1) throw new Error(`${label} must be a 64-bit little-endian ELF shared object`);
  if (bytes.readUInt16LE(16) !== 3 || bytes.readUInt16LE(18) !== target.machine) throw new Error(`${label} ELF type or machine does not match target shared object`);
}

function assertGnPolicy(bytes) {
  const lines = bytes.toString("utf8").split(/\r?\n/).filter((line) => !/^\s*#/.test(line));
  for (const name of ["pdf_enable_v8", "pdf_enable_xfa", "is_debug"]) {
    if (!lines.some((line) => new RegExp(`^\\s*${name}\\s*=\\s*false\\s*(?:#.*)?$`).test(line))) {
      throw new Error("production PDFium GN args do not enforce the reviewed feature policy");
    }
  }
}

function noticeDocument(notices) {
  const blocks = [];
  for (const { record, bytes } of notices) {
    let text;
    try {
      text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } catch {
      throw new Error(`notice ${record.path} must be UTF-8 text`);
    }
    blocks.push(Buffer.from(`## ${record.path}\n\n`, "utf8"), bytes);
    if (!text.endsWith("\n")) blocks.push(Buffer.from("\n", "utf8"));
    blocks.push(Buffer.from("\n", "utf8"));
  }
  return Buffer.concat(blocks);
}

export async function stageNonMacProductionPdfium({ manifestPath, artifactRoot, target: targetName, outputDirectory }) {
  const manifestBytes = await readFile(manifestPath);
  const manifest = validateNonMacProductionManifest(JSON.parse(manifestBytes));
  const artifact = manifest.artifacts.find((candidate) => candidate.target === targetName);
  if (!artifact) throw new Error(`production PDFium target is not approved: ${targetName}`);
  const target = targets.get(targetName);
  const root = resolve(artifactRoot);
  const rootMetadata = await lstat(root);
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) throw new Error("production PDFium artifact root must be a real directory");
  const verified = {};
  for (const [name, record] of Object.entries({ redistributionReview: manifest.redistributionReview, supplierReview: manifest.supplierReview, library: artifact.library, sbom: artifact.sbom, provenance: artifact.provenance, gnArgs: artifact.gnArgs })) {
    verified[name] = await verifiedFile(root, record, name);
  }
  validateBinary(verified.library.bytes, target, "production PDFium library");
  assertGnPolicy(verified.gnArgs.bytes);

  const noticeRoot = await assertDirectory(root, artifact.noticeRoot, "noticeRoot");
  const actualNotices = await inventoryDirectory(noticeRoot);
  const expectedNotices = artifact.notices.map(({ path }) => path).sort();
  if (JSON.stringify(actualNotices) !== JSON.stringify(expectedNotices)) throw new Error("production PDFium notice inventory has missing or extra files");
  const notices = [];
  for (const record of [...artifact.notices].sort((a, b) => a.path.localeCompare(b.path))) notices.push({ record, ...(await verifiedFile(noticeRoot, record, `notice ${record.path}`)) });

  const destination = resolve(outputDirectory);
  const inputRelative = relative(root, destination);
  const destinationRelative = relative(destination, root);
  if (!inputRelative.startsWith("..") && inputRelative !== "" || destinationRelative === "" || (!destinationRelative.startsWith("..") && !isAbsolute(destinationRelative))) {
    throw new Error("output directory must be outside the artifact root");
  }
  await mkdir(dirname(destination), { recursive: true });
  await mkdir(destination, { recursive: false, mode: 0o700 });
  let complete = false;
  try {
    const noticeBytes = noticeDocument(notices);
    const staged = new Map([[target.library, verified.library.bytes], ["THIRD_PARTY_NOTICES.md", noticeBytes]]);
    const files = [];
    for (const [name, bytes] of [...staged].sort(([a], [b]) => a.localeCompare(b))) {
      const outputPath = join(destination, name);
      await writeFile(outputPath, bytes, { flag: "wx", mode: 0o644 });
      await chmod(outputPath, 0o644);
      const written = await readFile(outputPath);
      if (!written.equals(bytes)) throw new Error(`staged PDFium file changed while writing: ${name}`);
      files.push({ file: name, bytes: written.length, sha256: sha256(written) });
    }
    files.sort((a, b) => a.file.localeCompare(b.file));
    const receipt = {
      schemaVersion: 1,
      purpose: "production-distribution",
      productionApproved: true,
      target: targetName,
      library: { path: target.library, bytes: verified.library.bytes.length, sha256: sha256(verified.library.bytes) },
      source: { revision: manifest.source.revision },
      build: { provenance: `sha256:${sha256(verified.provenance.bytes)}` },
      redistributionReview: { reference: `sha256:${sha256(verified.redistributionReview.bytes)}` },
      supplierReview: { reference: `sha256:${sha256(verified.supplierReview.bytes)}` },
      manifestSha256: sha256(manifestBytes),
      apiBuild: manifest.build.apiBuild,
      inputs: {
        sbom: { path: artifact.sbom.path, bytes: verified.sbom.bytes.length, sha256: sha256(verified.sbom.bytes) },
        provenance: { path: artifact.provenance.path, bytes: verified.provenance.bytes.length, sha256: sha256(verified.provenance.bytes) },
        gnArgs: { path: artifact.gnArgs.path, bytes: verified.gnArgs.bytes.length, sha256: sha256(verified.gnArgs.bytes) },
        notices: notices.map(({ record, bytes }) => ({ path: record.path, bytes: bytes.length, sha256: sha256(bytes) })),
      },
      files,
    };
    await writeFile(join(destination, target.receipt), `${JSON.stringify(receipt, null, 2)}\n`, { flag: "wx", mode: 0o644 });
    complete = true;
    return receipt;
  } finally {
    if (!complete) {
      await rm(destination, { recursive: true, force: true });
    }
  }
}

function parseArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || !value || values.has(key)) throw new Error("usage: stage-nonmac-pdfium-production.mjs --manifest FILE --artifact-root DIR --target TRIPLE --output-dir DIR");
    values.set(key, value);
  }
  for (const required of ["--manifest", "--artifact-root", "--target", "--output-dir"]) if (!values.has(required)) throw new Error(`${required} is required`);
  if (values.size !== 4) throw new Error("unexpected command-line option");
  return values;
}

if (process.argv[1] === scriptPath) {
  const values = parseArguments(process.argv.slice(2));
  const receipt = await stageNonMacProductionPdfium({ manifestPath: resolve(values.get("--manifest")), artifactRoot: resolve(values.get("--artifact-root")), target: values.get("--target"), outputDirectory: resolve(values.get("--output-dir")) });
  process.stdout.write(`${JSON.stringify(receipt, null, 2)}\n`);
}
