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
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
} from "node:path";
import { fileURLToPath } from "node:url";
import { validateNativeMachO } from "./assemble-macos-production.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const PINNED_PDFIUM_REVISION = "91b9d569b34be4f38eed7b3c49b227356c3aadad";
const PINNED_SHARED_LIBRARY_PATCH_SHA256 = "1e521b48561c51a63425baeec7c74c1edaf65956b6e6297d98aff57f2cc2ee40";
const PINNED_DEPENDENCY_POLICY_PATCH_SHA256 = "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b";
const macTargets = new Map([
  ["aarch64-apple-darwin", 0x0100000c],
  ["x86_64-apple-darwin", 0x01000007],
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function safeRelativePath(value, label) {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    isAbsolute(value) ||
    /^[A-Za-z]:[\\/]/.test(value) ||
    value.includes("\\") ||
    value
      .split("/")
      .some((part) => part === "" || part === "." || part === "..")
  ) {
    throw new Error(`${label} must be a safe relative path`);
  }
  return value;
}

function requireHash(value, label) {
  if (!/^[0-9a-f]{64}$/.test(value ?? "")) {
    throw new Error(`${label} must be a lowercase SHA-256 digest`);
  }
}

function requireRecord(record, label) {
  if (!record || typeof record !== "object" || Array.isArray(record)) {
    throw new Error(`${label} must be an object`);
  }
  safeRelativePath(record.path, `${label}.path`);
  requireHash(record.sha256, `${label}.sha256`);
  if (!Number.isSafeInteger(record.bytes) || record.bytes <= 0) {
    throw new Error(`${label}.bytes must be a positive safe integer`);
  }
}

function containsDownloadField(value) {
  if (!value || typeof value !== "object") return false;
  return Object.entries(value).some(
    ([key, child]) =>
      /^(?:url|download|downloadUrl)$/i.test(key) ||
      containsDownloadField(child),
  );
}

export function validateProductionManifest(manifest) {
  if (
    manifest?.schemaVersion !== 1 ||
    manifest?.purpose !== "production-distribution" ||
    manifest?.productionApproved !== true
  ) {
    throw new Error(
      "production PDFium manifest is not explicitly approved for distribution",
    );
  }
  if (
    manifest.wrapper?.package !== "pdfium-render" ||
    manifest.wrapper?.version !== "0.9.4" ||
    manifest.wrapper?.revision !== "6cee8b9a3951832ac0ff62ce4c32800278001cb8" ||
    manifest.wrapper?.feature !== "pdfium_7881"
  ) {
    throw new Error(
      "production PDFium wrapper pins do not match the reviewed Rust graph",
    );
  }
  if (
    manifest.source?.repository !== "https://pdfium.googlesource.com/pdfium" ||
    manifest.source?.revision !== PINNED_PDFIUM_REVISION
  ) {
    throw new Error("production PDFium source revision does not match the app-reviewed pin");
  }
  if (
    manifest.build?.apiBuild !== 7881 ||
    manifest.build?.v8 !== false ||
    manifest.build?.xfa !== false ||
    manifest.build?.sharedLibraryPatchSha256 !== PINNED_SHARED_LIBRARY_PATCH_SHA256 ||
    manifest.build?.dependencyPolicyPatchSha256 !== PINNED_DEPENDENCY_POLICY_PATCH_SHA256 ||
    !/^(?:1[0-9]|2[0-9])\.[0-9]+$/.test(
      manifest.build?.minimumSystemVersion ?? "",
    ) ||
    !manifest.build?.toolchain ||
    Object.keys(manifest.build.toolchain).length === 0 ||
    Object.values(manifest.build.toolchain).some(
      (value) => typeof value !== "string" || value.length === 0,
    )
  ) {
    throw new Error(
      "production PDFium build policy or toolchain provenance is incomplete",
    );
  }
  requireRecord(manifest.redistributionReview, "redistributionReview");
  requireRecord(manifest.supplierReview, "supplierReview");
  if (containsDownloadField(manifest.artifacts)) {
    throw new Error(
      "production PDFium artifacts must be local inputs, not download locations",
    );
  }
  if (!Array.isArray(manifest.artifacts)) {
    throw new Error("production PDFium artifacts must be an array");
  }
  const targets = manifest.artifacts.map(({ target }) => target);
  if (targets.length === 0 || new Set(targets).size !== targets.length) {
    throw new Error(
      "production PDFium manifest must contain a nonempty set of unique macOS targets",
    );
  }
  for (const artifact of manifest.artifacts) {
    if (!macTargets.has(artifact.target)) {
      throw new Error(
        `unsupported production PDFium target ${artifact.target}`,
      );
    }
    if (basename(artifact.library?.path ?? "") !== "libpdfium.dylib") {
      throw new Error(
        `${artifact.target}.library must be named libpdfium.dylib`,
      );
    }
    for (const [name, record] of Object.entries({
      library: artifact.library,
      sbom: artifact.sbom,
      provenance: artifact.provenance,
      gnArgs: artifact.gnArgs,
    })) {
      requireRecord(record, `${artifact.target}.${name}`);
    }
    safeRelativePath(artifact.noticeRoot, `${artifact.target}.noticeRoot`);
    if (!Array.isArray(artifact.notices) || artifact.notices.length === 0) {
      throw new Error(
        `${artifact.target}.notices must contain the exact notice inventory`,
      );
    }
    const noticePaths = artifact.notices.map((notice, index) => {
      requireRecord(notice, `${artifact.target}.notices[${index}]`);
      return notice.path;
    });
    if (new Set(noticePaths).size !== noticePaths.length) {
      throw new Error(`${artifact.target}.notices contains duplicate paths`);
    }
  }
  return manifest;
}

async function verifiedFile(root, record, label) {
  const path = resolve(root, safeRelativePath(record.path, `${label}.path`));
  const withinRoot = relative(root, path);
  if (!withinRoot || withinRoot.startsWith("..") || isAbsolute(withinRoot)) {
    throw new Error(`${label} escapes the artifact root`);
  }
  let current = root;
  for (const segment of record.path.split("/")) {
    current = join(current, segment);
    const metadata = await lstat(current);
    if (metadata.isSymbolicLink())
      throw new Error(`${label} must not traverse a symlink`);
  }
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.nlink !== 1) {
    throw new Error(`${label} must be a regular single-link file`);
  }
  const bytes = await readFile(path);
  if (bytes.length !== record.bytes || sha256(bytes) !== record.sha256) {
    throw new Error(`${label} does not match its reviewed byte receipt`);
  }
  return { path, bytes };
}

async function inventoryDirectory(root, prefix = "") {
  const directory = resolve(root, prefix);
  const metadata = await lstat(directory);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw new Error("PDFium notice root must be a real directory");
  }
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const child = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isSymbolicLink())
      throw new Error(`PDFium notice inventory contains symlink ${child}`);
    if (entry.isDirectory())
      files.push(...(await inventoryDirectory(root, child)));
    else if (entry.isFile()) {
      const childMetadata = await lstat(resolve(root, child));
      if (childMetadata.nlink !== 1) {
        throw new Error(`PDFium notice inventory contains hard link ${child}`);
      }
      files.push(child);
    } else
      throw new Error(`PDFium notice inventory contains special file ${child}`);
  }
  return files.sort();
}

async function assertDirectoryComponentsSafe(root, path, label) {
  let current = root;
  for (const segment of safeRelativePath(path, label).split("/")) {
    current = join(current, segment);
    const metadata = await lstat(current);
    if (metadata.isSymbolicLink()) {
      throw new Error(`${label} must not traverse a symlink`);
    }
  }
  const metadata = await lstat(current);
  if (!metadata.isDirectory())
    throw new Error(`${label} must name a directory`);
  return current;
}

function assertGnPolicy(bytes) {
  const text = bytes.toString("utf8");
  for (const assignment of [
    /(?:^|\s)pdf_enable_v8\s*=\s*false(?:\s|$)/,
    /(?:^|\s)pdf_enable_xfa\s*=\s*false(?:\s|$)/,
    /(?:^|\s)is_debug\s*=\s*false(?:\s|$)/,
  ]) {
    if (!assignment.test(text)) {
      throw new Error(
        "production PDFium GN args do not enforce the reviewed feature policy",
      );
    }
  }
}

export async function stageProductionPdfium({
  manifestPath,
  artifactRoot,
  target,
  outputDirectory,
}) {
  const manifestBytes = await readFile(manifestPath);
  const manifest = validateProductionManifest(JSON.parse(manifestBytes));
  const artifact = manifest.artifacts.find(
    (candidate) => candidate.target === target,
  );
  if (!artifact)
    throw new Error(`production PDFium target is not approved: ${target}`);
  const root = resolve(artifactRoot);
  const rootMetadata = await lstat(root);
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw new Error("production PDFium artifact root must be a real directory");
  }

  const commonRecords = {
    redistributionReview: manifest.redistributionReview,
    supplierReview: manifest.supplierReview,
  };
  const verified = {};
  for (const [name, record] of Object.entries({
    ...commonRecords,
    library: artifact.library,
    sbom: artifact.sbom,
    provenance: artifact.provenance,
    gnArgs: artifact.gnArgs,
  })) {
    verified[name] = await verifiedFile(root, record, name);
  }
  validateNativeMachO(
    verified.library.bytes,
    target,
    manifest.build.minimumSystemVersion,
    "production PDFium library",
    6,
  );
  assertGnPolicy(verified.gnArgs.bytes);

  const noticeRoot = await assertDirectoryComponentsSafe(
    root,
    artifact.noticeRoot,
    "noticeRoot",
  );
  const actualNoticePaths = await inventoryDirectory(noticeRoot);
  const expectedNoticePaths = artifact.notices.map(({ path }) => path).sort();
  if (
    JSON.stringify(actualNoticePaths) !== JSON.stringify(expectedNoticePaths)
  ) {
    throw new Error(
      "production PDFium notice inventory has missing or extra files",
    );
  }
  const verifiedNotices = [];
  for (const notice of [...artifact.notices].sort((a, b) =>
    a.path.localeCompare(b.path),
  )) {
    verifiedNotices.push({
      record: notice,
      ...(await verifiedFile(noticeRoot, notice, `notice ${notice.path}`)),
    });
  }

  const destination = resolve(outputDirectory);
  await mkdir(dirname(destination), { recursive: true });
  await mkdir(destination, { recursive: false, mode: 0o700 });
  let complete = false;
  try {
    const frameworkDirectory = join(destination, "Frameworks");
    const resourceDirectory = join(destination, "Resources/PDFium");
    const licenseDirectory = join(destination, "Resources/Licenses/PDFium");
    await mkdir(frameworkDirectory, { recursive: true });
    await mkdir(resourceDirectory, { recursive: true });
    await mkdir(licenseDirectory, { recursive: true });
    const stagedFiles = [];
    const stage = async (bytes, targetPath, mode = 0o644) => {
      await mkdir(dirname(targetPath), { recursive: true });
      await writeFile(targetPath, bytes, { flag: "wx", mode });
      await chmod(targetPath, mode);
      const stagedBytes = await readFile(targetPath);
      if (!stagedBytes.equals(bytes)) {
        throw new Error("staged PDFium bytes changed while writing");
      }
      stagedFiles.push({
        file: relative(destination, targetPath),
        bytes: stagedBytes.length,
        sha256: sha256(stagedBytes),
      });
    };
    await stage(
      verified.library.bytes,
      join(frameworkDirectory, "libpdfium.dylib"),
      0o755,
    );
    await stage(verified.sbom.bytes, join(resourceDirectory, "sbom"));
    await stage(
      verified.provenance.bytes,
      join(resourceDirectory, "provenance"),
    );
    await stage(verified.gnArgs.bytes, join(resourceDirectory, "gn-args.txt"));
    await stage(
      verified.redistributionReview.bytes,
      join(resourceDirectory, "redistribution-review"),
    );
    await stage(
      verified.supplierReview.bytes,
      join(resourceDirectory, "supplier-review"),
    );
    for (const notice of verifiedNotices) {
      await stage(notice.bytes, join(licenseDirectory, notice.record.path));
    }
    stagedFiles.sort((a, b) => a.file.localeCompare(b.file));
    const receipt = {
      schema: "butter-paper/pdfium-production-stage",
      version: 1,
      target,
      apiBuild: manifest.build.apiBuild,
      sourceRevision: manifest.source.revision,
      manifestSha256: sha256(manifestBytes),
      files: stagedFiles,
    };
    await writeFile(
      join(resourceDirectory, "receipt.json"),
      `${JSON.stringify(receipt, null, 2)}\n`,
      { flag: "wx", mode: 0o644 },
    );
    complete = true;
    return receipt;
  } finally {
    if (!complete) await rm(destination, { recursive: true, force: true });
  }
}

function parseArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || !value || values.has(key)) {
      throw new Error(
        "usage: stage-pdfium-production.mjs --manifest FILE --artifact-root DIR --target TRIPLE --output-dir DIR",
      );
    }
    values.set(key, value);
  }
  for (const required of [
    "--manifest",
    "--artifact-root",
    "--target",
    "--output-dir",
  ]) {
    if (!values.has(required)) throw new Error(`${required} is required`);
  }
  return values;
}

if (process.argv[1] === scriptPath) {
  const values = parseArguments(process.argv.slice(2));
  const receipt = await stageProductionPdfium({
    manifestPath: resolve(values.get("--manifest")),
    artifactRoot: resolve(values.get("--artifact-root")),
    target: values.get("--target"),
    outputDirectory: resolve(values.get("--output-dir")),
  });
  process.stdout.write(`${JSON.stringify(receipt, null, 2)}\n`);
}
