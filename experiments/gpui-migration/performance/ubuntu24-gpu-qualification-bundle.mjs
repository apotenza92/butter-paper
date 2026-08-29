#!/usr/bin/env node

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { lstat, readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const sha256Pattern = /^[a-f0-9]{64}$/;
const entrypointPattern = /^RUN-QUALIFICATION\.sh$/;

function unsafePath(value) {
  return (
    typeof value !== "string" ||
    value.length === 0 ||
    value.startsWith("/") ||
    value.startsWith("-") ||
    value.includes("\\") ||
    value.includes("//") ||
    /[\0-\x1f\x7f]/.test(value) ||
    value
      .split("/")
      .some((part) => part === "" || part === "." || part === "..") ||
    path.posix.normalize(value) !== value
  );
}

export function validateArchivePaths(paths) {
  const observed = new Set();
  for (const candidate of paths) {
    if (unsafePath(candidate)) {
      throw new Error(`unsafe archive path: ${JSON.stringify(candidate)}`);
    }
    if (observed.has(candidate)) {
      throw new Error(`duplicate archive path: ${candidate}`);
    }
    observed.add(candidate);
  }
  return [...paths];
}

export function parseEntrypointArguments(argv) {
  if (
    argv.length !== 2 ||
    argv[0] !== "--output" ||
    !path.isAbsolute(argv[1] ?? "") ||
    /[\0-\x1f\x7f;&|`$<>]/.test(argv[1] ?? "")
  ) {
    throw new Error(
      "fixed entrypoint usage: RUN-QUALIFICATION.sh --output /absolute/fresh/path",
    );
  }
  return { output: path.resolve(argv[1]) };
}

async function sha256File(file) {
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const stream = createReadStream(file);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", rejectPromise);
    stream.on("end", resolvePromise);
  });
  return hash.digest("hex");
}

async function regularFiles(root) {
  const files = [];
  async function visit(directory, relativeDirectory = "") {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const relativePath = relativeDirectory
        ? `${relativeDirectory}/${entry.name}`
        : entry.name;
      validateArchivePaths([relativePath]);
      const absolutePath = path.join(root, ...relativePath.split("/"));
      const metadata = await lstat(absolutePath);
      if (metadata.isSymbolicLink()) {
        throw new Error(`bundle contains a symbolic link: ${relativePath}`);
      }
      if (metadata.isDirectory()) await visit(absolutePath, relativePath);
      else if (metadata.isFile())
        files.push({ absolutePath, relativePath, metadata });
      else
        throw new Error(`bundle contains a non-regular entry: ${relativePath}`);
    }
  }
  await visit(root);
  return files;
}

export async function buildBundleManifest(
  root,
  { entrypoint, classification, excludedPaths = [] },
) {
  validateArchivePaths([entrypoint, ...excludedPaths]);
  const excluded = new Set(excludedPaths);
  const files = [];
  for (const file of await regularFiles(root)) {
    if (excluded.has(file.relativePath)) continue;
    files.push({
      path: file.relativePath,
      bytes: file.metadata.size,
      mode: file.metadata.mode & 0o777,
      sha256: await sha256File(file.absolutePath),
    });
  }
  return {
    schema_version: 1,
    profile: "bp-ubuntu24-gpu-qualification-bundle-v1",
    target: { os: "ubuntu", version: "24.04", architecture: "x86_64" },
    entrypoint,
    qualification_classification: classification,
    files,
  };
}

export async function validateBundleDirectory(root, manifest) {
  if (
    manifest?.schema_version !== 1 ||
    manifest?.profile !== "bp-ubuntu24-gpu-qualification-bundle-v1" ||
    !entrypointPattern.test(manifest?.entrypoint ?? "") ||
    ![
      "runnable",
      "runnable-development-qualification",
      "blocked-contract-mismatch",
    ].includes(manifest?.qualification_classification) ||
    !Array.isArray(manifest?.files)
  ) {
    throw new Error("bundle manifest classification or schema is invalid");
  }
  const manifestPaths = validateArchivePaths(
    manifest.files.map((entry) => entry.path),
  );
  const expected = new Map(manifest.files.map((entry) => [entry.path, entry]));
  for (const entry of manifest.files) {
    if (
      !Number.isInteger(entry.bytes) ||
      entry.bytes < 0 ||
      !Number.isInteger(entry.mode) ||
      !sha256Pattern.test(entry.sha256 ?? "")
    ) {
      throw new Error(`invalid manifest file record: ${entry.path}`);
    }
  }
  const actual = await regularFiles(root);
  for (const file of actual) {
    if (file.relativePath === "BUNDLE-MANIFEST.json") continue;
    const expectedEntry = expected.get(file.relativePath);
    if (!expectedEntry) throw new Error(`unsealed file: ${file.relativePath}`);
    const digest = await sha256File(file.absolutePath);
    if (
      digest !== expectedEntry.sha256 ||
      file.metadata.size !== expectedEntry.bytes
    ) {
      throw new Error(`hash or byte count changed: ${file.relativePath}`);
    }
    if ((file.metadata.mode & 0o777) !== expectedEntry.mode) {
      throw new Error(`mode changed: ${file.relativePath}`);
    }
  }
  const actualPaths = new Set(
    actual
      .map(({ relativePath }) => relativePath)
      .filter((relativePath) => relativePath !== "BUNDLE-MANIFEST.json"),
  );
  for (const expectedPath of manifestPaths) {
    if (!actualPaths.has(expectedPath))
      throw new Error(`missing sealed file: ${expectedPath}`);
  }
  return { status: "verified", file_count: manifest.files.length };
}

async function main() {
  if (process.argv.length !== 4 || process.argv[2] !== "--validate") {
    throw new Error(
      "usage: node ubuntu24-gpu-qualification-bundle.mjs --validate <bundle-root>",
    );
  }
  const root = path.resolve(process.argv[3]);
  const manifest = JSON.parse(
    await readFile(path.join(root, "BUNDLE-MANIFEST.json"), "utf8"),
  );
  process.stdout.write(
    `${JSON.stringify(await validateBundleDirectory(root, manifest))}\n`,
  );
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1])
) {
  await main();
}
