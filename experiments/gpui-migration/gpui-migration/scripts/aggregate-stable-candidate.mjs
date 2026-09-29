#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const REQUIRED_TARGETS = [
  "linux-arm64",
  "linux-x64",
  "macos-arm64",
  "macos-x64",
  "windows-arm64",
  "windows-x64",
];
const OPTIONAL_TARGETS = [];
const ALL_TARGETS = new Set([...REQUIRED_TARGETS, ...OPTIONAL_TARGETS]);
const REVISION = /^[0-9a-f]{40}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const STABLE_SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\+([0-9A-Za-z.-]+))?$/;
const FORBIDDEN_MARKER =
  /(?:development|dev)[-_\s.]*pdfium|pdfium[-_\s.]*(?:development|dev)|(?:pdfium.{0,80}override|override.{0,80}pdfium)|\b(?:BP_UPDATE_TEST_MODE|PDFIUM_(?:DEV|DEVELOPMENT|OVERRIDE)|DEV_PDFIUM|PDFIUM_OVERRIDE)\b/i;

function fail(message) {
  throw new Error(message);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function hasForbiddenMarker(bytesOrText) {
  const text = Buffer.isBuffer(bytesOrText)
    ? bytesOrText.toString("latin1")
    : bytesOrText;
  return FORBIDDEN_MARKER.test(text);
}

function safeRelativePath(value, label) {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.includes("\\") ||
    isAbsolute(value)
  ) {
    fail(`${label} must be a safe relative path`);
  }
  const parts = value.split("/");
  if (parts.some((part) => part === "" || part === "." || part === "..")) {
    fail(`${label} must be a safe relative path`);
  }
  return parts.join(sep);
}

async function inventoryDirectory(root) {
  const files = new Set();
  async function visit(directory, prefix = "") {
    const entries = (await readdir(directory)).sort();
    for (const entry of entries) {
      const rel = prefix ? `${prefix}/${entry}` : entry;
      const path = resolve(directory, entry);
      const stat = await lstat(path);
      if (stat.isSymbolicLink()) fail(`input contains a symbolic link: ${rel}`);
      if (stat.isDirectory()) {
        await visit(path, rel);
      } else if (stat.isFile()) {
        if (stat.nlink !== 1) fail(`input contains a hard-linked file: ${rel}`);
        files.add(rel);
      } else {
        fail(`input contains a special file: ${rel}`);
      }
    }
  }
  await visit(root);
  return files;
}

async function readRegularFile(root, relativePath, label) {
  const rel = safeRelativePath(relativePath, label);
  const path = resolve(root, rel);
  const fromRoot = relative(root, path);
  if (
    fromRoot === ".." ||
    fromRoot.startsWith(`..${sep}`) ||
    isAbsolute(fromRoot)
  ) {
    fail(`${label} escapes the input directory`);
  }
  const stat = await lstat(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) {
    fail(`${label} must be a regular single-link file`);
  }
  return { rel: rel.split(sep).join("/"), bytes: await readFile(path) };
}

function parseJson(bytes, label) {
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    fail(`${label} must contain valid JSON`);
  }
}

function validateIdentity(value, expected, label) {
  if (
    value?.target !== expected.target ||
    value?.channel !== "stable" ||
    value?.version !== expected.version ||
    value?.sourceRevision !== expected.sourceRevision
  ) {
    fail(`${label} identity does not match the stable candidate descriptor`);
  }
}

function validateArtifactClaim(value, actual, label) {
  if (
    value?.artifact?.path !== actual.path ||
    value?.artifact?.bytes !== actual.bytes ||
    value?.artifact?.sha256 !== actual.sha256
  ) {
    fail(`${label} artifact claim does not match the package bytes`);
  }
}

function validateRuntimeEvidence(value, expected, artifact, label) {
  const identity =
    value?.schema === "butter-paper/macos-runtime-smoke"
      ? value?.identity
      : value?.schema === "butter-paper/nonmac-runtime-smoke"
        ? value?.packageIdentity
        : null;
  if (
    value?.schemaVersion !== 1 ||
    !identity ||
    value?.passed !== true ||
    value?.error !== undefined ||
    value?.cleanup?.status !== "verified-clean" ||
    value?.cleanup?.tempRootRemoved !== true ||
    !value?.documentOpenEvidence ||
    identity.target !== expected.target ||
    identity.version !== expected.version ||
    identity.sourceRevision !== expected.sourceRevision ||
    identity.archiveSha256 !== artifact.sha256
  ) {
    fail(
      `${label} does not prove a clean exact-package runtime smoke for the candidate artifact`,
    );
  }
}

function checkNoDevelopmentMarkers(value, label) {
  const serialized = typeof value === "string" ? value : JSON.stringify(value);
  if (hasForbiddenMarker(serialized))
    fail(`${label} contains a development PDFium or override marker`);
}

export async function aggregateStableCandidate({ inputDir, outputPath }) {
  const root = resolve(inputDir);
  const output = resolve(outputPath);
  const rootStat = await lstat(root);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink())
    fail("input must be a real directory");
  const outputFromRoot = relative(root, output);
  if (
    outputFromRoot === "" ||
    (!isAbsolute(outputFromRoot) &&
      outputFromRoot !== ".." &&
      !outputFromRoot.startsWith(`..${sep}`))
  ) {
    fail("output must be outside the input directory");
  }

  const inputFiles = await inventoryDirectory(root);
  if (!inputFiles.has("candidate.json"))
    fail("input is missing candidate.json");
  const descriptorFile = await readRegularFile(
    root,
    "candidate.json",
    "candidate descriptor",
  );
  const descriptor = parseJson(descriptorFile.bytes, "candidate descriptor");
  if (
    descriptor?.schema !== "butter-paper/stable-candidate-input" ||
    descriptor?.schemaVersion !== 2 ||
    descriptor?.channel !== "stable" ||
    !STABLE_SEMVER.test(descriptor?.version ?? "") ||
    !REVISION.test(descriptor?.sourceRevision ?? "") ||
    !Array.isArray(descriptor?.targets)
  ) {
    fail(
      "candidate descriptor must declare schema version 2, stable channel, a stable semver, source revision, and targets",
    );
  }
  const seenTargets = new Set();
  const referencedFiles = new Set(["candidate.json"]);
  const results = [];
  for (const record of descriptor.targets) {
    const target = record?.target;
    if (!ALL_TARGETS.has(target)) fail(`unexpected target: ${String(target)}`);
    if (seenTargets.has(target)) fail(`duplicate target: ${target}`);
    seenTargets.add(target);

    const artifactFile = await readRegularFile(
      root,
      record.artifact,
      `${target} artifact`,
    );
    const packageManifestFile = await readRegularFile(
      root,
      record.packageManifest,
      `${target} package manifest`,
    );
    const verificationFile = await readRegularFile(
      root,
      record.verificationReceipt,
      `${target} verification receipt`,
    );
    const runtimeEvidenceFile = await readRegularFile(
      root,
      record.runtimeEvidence,
      `${target} runtime evidence`,
    );
    for (const file of [
      artifactFile,
      packageManifestFile,
      verificationFile,
      runtimeEvidenceFile,
    ]) {
      if (referencedFiles.has(file.rel))
        fail(`input file is referenced more than once: ${file.rel}`);
      referencedFiles.add(file.rel);
    }

    const artifact = {
      path: artifactFile.rel,
      bytes: artifactFile.bytes.length,
      sha256: sha256(artifactFile.bytes),
    };
    const expected = {
      target,
      version: descriptor.version,
      sourceRevision: descriptor.sourceRevision,
    };
    const packageManifest = parseJson(
      packageManifestFile.bytes,
      `${target} package manifest`,
    );
    const verificationReceipt = parseJson(
      verificationFile.bytes,
      `${target} verification receipt`,
    );
    const runtimeEvidence = parseJson(
      runtimeEvidenceFile.bytes,
      `${target} runtime evidence`,
    );
    validateIdentity(packageManifest, expected, `${target} package manifest`);
    validateIdentity(
      verificationReceipt,
      expected,
      `${target} verification receipt`,
    );
    validateArtifactClaim(
      packageManifest,
      artifact,
      `${target} package manifest`,
    );
    validateArtifactClaim(
      verificationReceipt,
      artifact,
      `${target} verification receipt`,
    );
    validateRuntimeEvidence(
      runtimeEvidence,
      expected,
      artifact,
      `${target} runtime evidence`,
    );
    if (
      packageManifest?.schema !== "butter-paper/package-manifest" ||
      packageManifest?.schemaVersion !== 1 ||
      verificationReceipt?.schema !== "butter-paper/package-verification" ||
      verificationReceipt?.schemaVersion !== 1 ||
      verificationReceipt?.verified !== true
    ) {
      fail(
        `${target} package or verification manifest is not an approved version 1 receipt`,
      );
    }
    checkNoDevelopmentMarkers(packageManifest, `${target} package manifest`);
    checkNoDevelopmentMarkers(
      verificationReceipt,
      `${target} verification receipt`,
    );
    if (hasForbiddenMarker(artifactFile.bytes))
      fail(
        `${target} artifact contains a development PDFium or override marker`,
      );

    results.push({
      target,
      artifact,
      packageManifestSha256: sha256(packageManifestFile.bytes),
      verificationReceiptSha256: sha256(verificationFile.bytes),
      runtimeEvidenceSha256: sha256(runtimeEvidenceFile.bytes),
    });
  }

  for (const target of REQUIRED_TARGETS) {
    if (!seenTargets.has(target))
      fail(`candidate descriptor is missing required target: ${target}`);
  }
  for (const file of inputFiles) {
    if (!referencedFiles.has(file))
      fail(`input contains an unexpected file: ${file}`);
  }
  results.sort((a, b) =>
    a.target < b.target ? -1 : a.target > b.target ? 1 : 0,
  );

  const manifest = {
    schema: "butter-paper/stable-candidate-manifest",
    schemaVersion: 1,
    channel: "stable",
    version: descriptor.version,
    sourceRevision: descriptor.sourceRevision,
    requiredTargets: [...REQUIRED_TARGETS],
    optionalTargets: results
      .filter(({ target }) => OPTIONAL_TARGETS.includes(target))
      .map(({ target }) => target),
    targets: results,
  };
  const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
  const digest = sha256(manifestBytes);
  const checksumBytes = Buffer.from(`${digest}  ${output.split(sep).at(-1)}\n`);
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, manifestBytes, { flag: "wx", mode: 0o644 });
  try {
    await writeFile(`${output}.sha256`, checksumBytes, {
      flag: "wx",
      mode: 0o644,
    });
  } catch (error) {
    throw new Error(
      `manifest was written but checksum creation failed: ${error.message}`,
    );
  }
  return {
    manifest,
    sha256: digest,
    outputPath: output,
    checksumPath: `${output}.sha256`,
  };
}

async function main() {
  const args = process.argv.slice(2);
  const values = new Map();
  for (let index = 0; index < args.length; index += 1) {
    if (
      !["--input", "--output"].includes(args[index]) ||
      !args[index + 1] ||
      values.has(args[index])
    ) {
      fail("usage: aggregate-stable-candidate.mjs --input DIR --output FILE");
    }
    values.set(args[index], args[++index]);
  }
  if (values.size !== 2)
    fail("usage: aggregate-stable-candidate.mjs --input DIR --output FILE");
  await aggregateStableCandidate({
    inputDir: values.get("--input"),
    outputPath: values.get("--output"),
  });
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  main().catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
