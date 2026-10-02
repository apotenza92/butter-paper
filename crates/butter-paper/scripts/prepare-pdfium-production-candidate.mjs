#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  lstat,
  mkdir,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { platform, arch, release } from "node:os";

const scriptPath = fileURLToPath(import.meta.url);
const targetLibraries = new Map([
  ["aarch64-apple-darwin", "libpdfium.dylib"],
  ["x86_64-apple-darwin", "libpdfium.dylib"],
  ["aarch64-pc-windows-msvc", "pdfium.dll"],
  ["x86_64-pc-windows-msvc", "pdfium.dll"],
  ["aarch64-unknown-linux-gnu", "libpdfium.so"],
  ["x86_64-unknown-linux-gnu", "libpdfium.so"],
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function safeRelativePath(value, label) {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    isAbsolute(value) ||
    /^[A-Za-z]:/.test(value) ||
    value.includes("\\") ||
    value.split("/").some((part) => part === "" || part === "." || part === "..")
  ) {
    throw new Error(`${label} must be a safe relative path`);
  }
  return value;
}

async function safeFile(root, path, label) {
  const safePath = safeRelativePath(path, label);
  const absolute = resolve(root, ...safePath.split("/"));
  const withinRoot = relative(root, absolute);
  if (!withinRoot || withinRoot.startsWith("..") || isAbsolute(withinRoot)) {
    throw new Error(`${label} escapes the candidate root`);
  }
  let current = root;
  for (const part of safePath.split("/")) {
    current = join(current, part);
    const metadata = await lstat(current);
    if (metadata.isSymbolicLink()) {
      throw new Error(`${label} must not traverse a symlink`);
    }
  }
  const metadata = await lstat(absolute);
  if (!metadata.isFile() || metadata.nlink !== 1) {
    throw new Error(`${label} must be a regular single-link file`);
  }
  return { absolute, bytes: await readFile(absolute) };
}

async function readEvidenceFile(path, label, encoding) {
  const absolute = resolve(requireNonempty(path, label));
  const metadata = await lstat(absolute);
  if (metadata.isSymbolicLink() || !metadata.isFile() || metadata.nlink !== 1) {
    throw new Error(`${label} must be a regular single-link file`);
  }
  return readFile(absolute, encoding);
}

async function inventoryNotices(root, prefix = "notices") {
  const absolute = resolve(root, ...prefix.split("/"));
  const metadata = await lstat(absolute);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw new Error("notice root must be a real directory");
  }
  const files = [];
  for (const entry of await readdir(absolute, { withFileTypes: true })) {
    const child = `${prefix}/${entry.name}`;
    if (entry.isSymbolicLink()) {
      throw new Error(`notice inventory contains symlink ${child}`);
    }
    if (entry.isDirectory()) {
      files.push(...(await inventoryNotices(root, child)));
    } else if (entry.isFile()) {
      const item = await safeFile(root, child, `notice ${child}`);
      files.push({ path: child, bytes: item.bytes });
    } else {
      throw new Error(`notice inventory contains special file ${child}`);
    }
  }
  return files.sort((a, b) => a.path.localeCompare(b.path));
}

async function assertCandidateInputs(root, library) {
  const rootMetadata = await lstat(root);
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw new Error("candidate root must be a real directory");
  }
  const entries = (await readdir(root)).sort();
  const expected = ["gn-args.txt", "notices", library].sort();
  if (JSON.stringify(entries) !== JSON.stringify(expected)) {
    throw new Error("candidate root contains missing or extra inputs");
  }
  const libraryFile = await safeFile(root, library, "library");
  const gnArgs = await safeFile(root, "gn-args.txt", "GN arguments");
  const notices = await inventoryNotices(root);
  if (libraryFile.bytes.length === 0 || gnArgs.bytes.length === 0) {
    throw new Error("candidate library and GN arguments must not be empty");
  }
  if (notices.length === 0) throw new Error("no upstream notice files were collected");
  return { libraryFile, gnArgs, notices };
}

function requireNonempty(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} is required`);
  }
  return value;
}

function record(path, bytes) {
  return { path, bytes: bytes.length, sha256: sha256(bytes) };
}

async function writeNew(path, value) {
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, {
    flag: "wx",
    mode: 0o644,
  });
}

export async function preparePdfiumProductionCandidate(options) {
  const target = requireNonempty(options.target, "target");
  const expectedLibrary = targetLibraries.get(target);
  if (!expectedLibrary) throw new Error(`unsupported target ${target}`);
  const library = safeRelativePath(options.library, "library");
  if (library !== expectedLibrary) {
    throw new Error(`${target} library must be named ${expectedLibrary}`);
  }

  const root = resolve(requireNonempty(options.candidateRoot, "candidate root"));
  const input = await assertCandidateInputs(root, library);
  const dependencyRevisions = JSON.parse(
    await readEvidenceFile(options.dependencyRevisions, "dependency revisions", "utf8"),
  );
  if (
    !dependencyRevisions ||
    typeof dependencyRevisions !== "object" ||
    Array.isArray(dependencyRevisions) ||
    Object.keys(dependencyRevisions).length === 0
  ) {
    throw new Error("gclient dependency revision inventory is empty or malformed");
  }
  const components = [];
  for (const [dependencyPath, identity] of Object.entries(dependencyRevisions).sort(([a], [b]) => a.localeCompare(b))) {
    if (!identity || typeof identity !== "object" || Array.isArray(identity) || typeof identity.url !== "string" || !identity.url) {
      throw new Error(`dependency identity is malformed: ${dependencyPath}`);
    }
    let revision = identity.rev;
    const properties = [
      { name: "dependency.path", value: dependencyPath },
      { name: "source.repository", value: identity.url },
    ];
    if (typeof revision === "string" && revision) {
      properties.push({ name: "source.revision", value: revision });
    } else {
      revision = "identity-embedded-in-source-url";
      properties.push({ name: "source.revision.status", value: "review source URL identity" });
    }
    components.push({ type: "library", name: dependencyPath, version: revision, properties });
  }

  const [sharedPatch, dependencyPatch] = await Promise.all([
    readEvidenceFile(options.sharedLibraryPatch, "shared library patch"),
    readEvidenceFile(options.dependencyPolicyPatch, "dependency policy patch"),
  ]);
  const toolchain = {};
  for (const [key, path] of Object.entries({
    clang: options.clangVersionFile,
    gn: options.gnVersionFile,
    ninja: options.ninjaVersionFile,
    siso: options.sisoVersionFile,
  })) {
    const value = (await readEvidenceFile(path, `${key} version file`, "utf8")).trim();
    if (!value) throw new Error("build tool version evidence is missing");
    toolchain[key] = value;
  }

  const pdfiumRevision = requireNonempty(options.pdfiumRevision, "PDFium revision");
  const depotToolsRevision = requireNonempty(options.depotToolsRevision, "depot_tools revision");
  const repositoryRevision = requireNonempty(options.repositoryRevision, "repository revision");
  const runnerImage = requireNonempty(options.runnerImage, "runner image");
  const ninjaJobs = Number(options.ninjaJobs);
  if (!Number.isSafeInteger(ninjaJobs) || ninjaJobs < 1) throw new Error("ninja jobs must be a positive integer");
  const buildCommand = requireNonempty(options.buildCommand, "build command");

  const reviewsDirectory = join(root, "reviews");
  await mkdir(reviewsDirectory, { mode: 0o755 });
  const sbomBytes = Buffer.from(`${JSON.stringify({
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    version: 1,
    metadata: { component: { type: "application", name: "PDFium", version: "chromium/7881" } },
    components,
    properties: [{ name: "review.status", value: "PENDING REVIEW: confirm dependency identities, component mapping, licences and notices before distribution" }],
  }, null, 2)}\n`);
  const provenance = {
    schema: "butter-paper/pdfium-build-provenance",
    version: 1,
    source: { repository: "https://pdfium.googlesource.com/pdfium", revision: pdfiumRevision },
    builder: {
      workflow: ".github/workflows/build-gpui-pdfium-production.yml",
      repositoryRevision,
      runnerImage,
      runnerImageOS: options.runnerImageOS || "unknown",
      runnerImageVersion: options.runnerImageVersion || "unknown",
      runnerOS: options.runnerOS || "unknown",
      runnerArch: options.runnerArch || "unknown",
      host: `${platform()} ${release()} ${arch()}`,
    },
    toolchain: {
      depotToolsRevision,
      clang: toolchain.clang,
      gn: toolchain.gn,
      ninja: toolchain.ninja,
      siso: toolchain.siso,
    },
    build: {
      apiBuild: 7881,
      v8: false,
      xfa: false,
      debug: false,
      clangUseChromePlugins: false,
      remoteExec: false,
      skia: false,
      fontations: false,
      sharedLibraryPatchSha256: sha256(sharedPatch),
      dependencyPolicyPatchSha256: sha256(dependencyPatch),
      command: buildCommand,
      ninjaJobs,
    },
  };

  const sbomPath = join(root, "sbom.json");
  const provenancePath = join(root, "provenance.json");
  const redistributionPath = join(reviewsDirectory, "redistribution-review.txt");
  const supplierPath = join(reviewsDirectory, "supplier-review.txt");
  const redistributionBytes = Buffer.from("UNREVIEWED PLACEHOLDER. Confirm source and dependency redistribution rights before release.\n");
  const supplierBytes = Buffer.from("UNREVIEWED PLACEHOLDER. Confirm Google PDFium source identity, dependency origins, and notices before release.\n");
  const sbomBytesFile = sbomBytes;
  const provenanceBytes = Buffer.from(`${JSON.stringify(provenance, null, 2)}\n`);

  await Promise.all([
    writeFile(sbomPath, sbomBytesFile, { flag: "wx", mode: 0o644 }),
    writeFile(provenancePath, provenanceBytes, { flag: "wx", mode: 0o644 }),
    writeFile(redistributionPath, redistributionBytes, { flag: "wx", mode: 0o644 }),
    writeFile(supplierPath, supplierBytes, { flag: "wx", mode: 0o644 }),
  ]);
  const artifact = {
    target,
    library: record(library, input.libraryFile.bytes),
    sbom: record("sbom.json", sbomBytesFile),
    provenance: record("provenance.json", provenanceBytes),
    gnArgs: record("gn-args.txt", input.gnArgs.bytes),
    noticeRoot: "notices",
    notices: input.notices.map(({ path, bytes }) => ({ ...record(path, bytes), path: path.slice("notices/".length) })),
  };
  const manifestBuild = {
    apiBuild: 7881,
    v8: false,
    xfa: false,
    debug: false,
    clangUseChromePlugins: false,
    remoteExec: false,
    skia: false,
    fontations: false,
    sharedLibraryPatchSha256: sha256(sharedPatch),
    dependencyPolicyPatchSha256: sha256(dependencyPatch),
    toolchain: {
      depot_tools: depotToolsRevision,
      runner: runnerImage,
      clang: toolchain.clang,
      gn: toolchain.gn,
      ninja: toolchain.ninja,
      siso: toolchain.siso,
    },
  };
  if (target.endsWith("-apple-darwin")) manifestBuild.minimumSystemVersion = "13.0";
  const manifest = {
    schemaVersion: 1,
    purpose: "production-distribution",
    productionApproved: false,
    wrapper: {
      package: "pdfium-render",
      version: "0.9.4",
      revision: "6cee8b9a3951832ac0ff62ce4c32800278001cb8",
      feature: "pdfium_7881",
    },
    source: { repository: "https://pdfium.googlesource.com/pdfium", revision: pdfiumRevision },
    build: manifestBuild,
    redistributionReview: record("reviews/redistribution-review.txt", redistributionBytes),
    supplierReview: record("reviews/supplier-review.txt", supplierBytes),
    artifacts: [artifact],
  };
  const manifestPath = join(root, "production-pdfium-candidate.json");
  await writeNew(manifestPath, manifest);
  return manifest;
}

function parseArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || !value || values.has(key)) {
      throw new Error("invalid command line arguments");
    }
    values.set(key, value);
  }
  const names = {
    "--candidate-root": "candidateRoot",
    "--target": "target",
    "--library": "library",
    "--dependency-revisions": "dependencyRevisions",
    "--shared-library-patch": "sharedLibraryPatch",
    "--dependency-policy-patch": "dependencyPolicyPatch",
    "--gn-version-file": "gnVersionFile",
    "--ninja-version-file": "ninjaVersionFile",
    "--siso-version-file": "sisoVersionFile",
    "--clang-version-file": "clangVersionFile",
    "--pdfium-revision": "pdfiumRevision",
    "--depot-tools-revision": "depotToolsRevision",
    "--repository-revision": "repositoryRevision",
    "--runner-image": "runnerImage",
    "--runner-image-os": "runnerImageOS",
    "--runner-image-version": "runnerImageVersion",
    "--runner-os": "runnerOS",
    "--runner-arch": "runnerArch",
    "--ninja-jobs": "ninjaJobs",
    "--build-command": "buildCommand",
  };
  const options = {};
  for (const [key, value] of values) {
    if (!names[key]) throw new Error(`unknown argument ${key}`);
    options[names[key]] = value;
  }
  for (const key of [
    "candidateRoot", "target", "library", "dependencyRevisions", "sharedLibraryPatch",
    "dependencyPolicyPatch", "gnVersionFile", "ninjaVersionFile", "sisoVersionFile",
    "clangVersionFile", "pdfiumRevision", "depotToolsRevision", "repositoryRevision",
    "runnerImage", "ninjaJobs", "buildCommand",
  ]) {
    if (options[key] === undefined) throw new Error(`--${Object.keys(names).find((name) => names[name] === key)} is required`);
  }
  return options;
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(scriptPath)) {
  preparePdfiumProductionCandidate(parseArguments(process.argv.slice(2))).catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
