#!/usr/bin/env node

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, readdir, stat, writeFile } from "node:fs/promises";
import { dirname, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const performanceDirectory = dirname(fileURLToPath(import.meta.url));
export const repositoryDirectory = resolve(performanceDirectory, "../../..");
export const componentDevelopmentCandidateProfile =
  "bp-perf-longbridge-component-development-candidate-1";
export const componentOptimizedCandidateProfile =
  "bp-perf-longbridge-component-optimized-candidate-1";
export const reviewedLongbridgeRevision =
  "c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4";
export const reviewedZedRevision = "8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc";

const compatDirectory = resolve(
  repositoryDirectory,
  "experiments/gpui-migration/gpui-component-compat",
);

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalize(value[key])]),
    );
  }
  return value;
}

function canonicalSha256(value) {
  return createHash("sha256")
    .update(JSON.stringify(canonicalize(value)))
    .digest("hex");
}

async function sha256File(path) {
  return createHash("sha256")
    .update(await readFile(path))
    .digest("hex");
}

function repositoryRelative(path) {
  const candidate = relative(repositoryDirectory, resolve(path));
  if (!candidate || candidate === ".." || candidate.startsWith(`..${sep}`)) {
    throw new Error(`candidate input is outside the repository: ${path}`);
  }
  return candidate.split(sep).join("/");
}

async function artifact(path, { executable = false } = {}) {
  const metadata = await stat(path);
  if (!metadata.isFile())
    throw new Error(`candidate input is not a file: ${path}`);
  if (
    executable &&
    process.platform !== "win32" &&
    (metadata.mode & 0o111) === 0
  ) {
    throw new Error(`candidate input is not executable: ${path}`);
  }
  return {
    path: repositoryRelative(path),
    bytes: metadata.size,
    sha256: await sha256File(path),
  };
}

async function sourceFilesBelow(directory) {
  const files = [];
  async function visit(current) {
    const entries = await readdir(current, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const path = resolve(current, entry.name);
      if (entry.isDirectory()) await visit(path);
      else if (entry.isFile()) files.push(path);
      else
        throw new Error(`candidate source contains a non-file entry: ${path}`);
    }
  }
  await visit(directory);
  return files;
}

async function currentGraphReceipt() {
  const { stdout } = await execFileAsync(
    process.execPath,
    [resolve(compatDirectory, "scripts/verify-cargo-graph.mjs")],
    { cwd: compatDirectory, maxBuffer: 4 * 1024 * 1024 },
  );
  return JSON.parse(stdout);
}

async function currentMeasuredFeatureReceipt() {
  const args = [
    "tree",
    "--manifest-path",
    resolve(compatDirectory, "Cargo.toml"),
    "--locked",
    "--offline",
    "--features",
    "benchmark-evidence",
    "-e",
    "features,no-dev",
    "-i",
    "gpui",
  ];
  const { stdout } = await execFileAsync("cargo", args, {
    cwd: compatDirectory,
    maxBuffer: 4 * 1024 * 1024,
  });
  return {
    status: "verified",
    command:
      "cargo tree --locked --offline --features benchmark-evidence -e features,no-dev -i gpui",
    sha256: createHash("sha256").update(stdout).digest("hex"),
    testSupportEnabled: stdout.includes('gpui feature "test-support"'),
    profilerEnabled: stdout.includes('gpui feature "profiler"'),
  };
}

async function buildComponentCandidate(
  {
    binary,
    worker,
    pdfium,
    buildSummary,
    policy = resolve(compatDirectory, "source-preparation-policy.json"),
    lock = resolve(compatDirectory, "Cargo.lock"),
    toolchain = resolve(compatDirectory, "rust-toolchain.toml"),
    graphReceipt,
    measuredGraphReceipt,
    sourceFiles,
  },
  { profile, optimized },
) {
  const policyDocument = JSON.parse(await readFile(policy, "utf8"));
  if (policyDocument.component?.revision !== reviewedLongbridgeRevision) {
    throw new Error("Longbridge revision is not the reviewed pin");
  }
  if (policyDocument.zed?.revision !== reviewedZedRevision) {
    throw new Error("Zed revision is not the reviewed pin");
  }
  const graph = graphReceipt ?? (await currentGraphReceipt());
  if (
    graph.status !== "verified" ||
    graph.forbiddenPackages?.length !== 0 ||
    !String(graph.gpui?.source).includes(reviewedZedRevision)
  ) {
    throw new Error(
      "Cargo graph receipt does not prove the reviewed single-GPUI graph",
    );
  }
  const buildReceipt = JSON.parse(await readFile(buildSummary, "utf8"));
  if (
    optimized &&
    (buildReceipt.status !== 0 ||
      buildReceipt.runnerMode !== "perf-story-release" ||
      buildReceipt.cargoSubcommand !== "build" ||
      JSON.stringify(buildReceipt.cargoArguments) !==
        JSON.stringify([
          "--release",
          "--features",
          "benchmark-evidence",
          "--bin",
          "component_story",
          "--bin",
          "butter-paper-pdf-worker",
        ]) ||
      buildReceipt.targetDisposition?.state !== "retained")
  ) {
    throw new Error(
      "optimized candidate requires the fixed guarded release mode",
    );
  }
  const measuredGraph = optimized
    ? (measuredGraphReceipt ?? (await currentMeasuredFeatureReceipt()))
    : null;
  if (
    optimized &&
    (measuredGraph?.status !== "verified" ||
      measuredGraph?.testSupportEnabled !== false ||
      measuredGraph?.profilerEnabled !== true ||
      !/^[a-f0-9]{64}$/.test(measuredGraph?.sha256 ?? ""))
  ) {
    throw new Error(
      "optimized candidate measured feature graph is not the reviewed profiler-only GPUI graph",
    );
  }
  const manifest = {
    schemaVersion: 1,
    profile,
    developmentOnly: true,
    productionApproved: false,
    packaged: false,
    optimized,
    timingEligible: false,
    runtime: {
      binary: await artifact(binary, { executable: true }),
      worker: await artifact(worker, { executable: true }),
      pdfium: await artifact(pdfium),
    },
    source: {
      cargoLock: await artifact(lock),
      rustToolchain: await artifact(toolchain),
      preparationPolicy: await artifact(policy),
      preparedTreeSha256: policyDocument.prepared?.treeSha256,
      preparationPatchSha256: policyDocument.patch?.sha256,
      longbridgeRevision: reviewedLongbridgeRevision,
      zedRevision: reviewedZedRevision,
      compatFiles: await Promise.all(
        (
          sourceFiles ?? [
            resolve(compatDirectory, "Cargo.toml"),
            ...(await sourceFilesBelow(resolve(compatDirectory, "src"))),
          ]
        ).map((path) => artifact(path)),
      ),
    },
    build: await artifact(buildSummary),
    graph,
    ...(optimized ? { measuredGraph } : {}),
    limitations: [
      ...(optimized ? [] : ["development-profile-binary"]),
      "not-packaged",
      "pdfium-development-binary-not-approved-for-production",
      "native-visual-and-accessibility-not-run",
    ],
  };
  return { ...manifest, manifestSha256: canonicalSha256(manifest) };
}

export async function buildComponentDevelopmentCandidate(options) {
  return buildComponentCandidate(options, {
    profile: componentDevelopmentCandidateProfile,
    optimized: false,
  });
}

export async function buildComponentOptimizedCandidate(options) {
  return buildComponentCandidate(options, {
    profile: componentOptimizedCandidateProfile,
    optimized: true,
  });
}

export async function validateComponentDevelopmentCandidate(manifest) {
  const expectedHash = manifest.manifestSha256;
  const unsigned = { ...manifest };
  delete unsigned.manifestSha256;
  if (canonicalSha256(unsigned) !== expectedHash) {
    throw new Error("candidate manifest hash changed");
  }
  if (
    ![
      componentDevelopmentCandidateProfile,
      componentOptimizedCandidateProfile,
    ].includes(manifest.profile) ||
    manifest.optimized !==
      (manifest.profile === componentOptimizedCandidateProfile) ||
    manifest.developmentOnly !== true ||
    manifest.productionApproved !== false ||
    manifest.packaged !== false ||
    manifest.timingEligible !== false
  ) {
    throw new Error("candidate classification changed");
  }
  if (
    manifest.source.longbridgeRevision !== reviewedLongbridgeRevision ||
    manifest.source.zedRevision !== reviewedZedRevision ||
    manifest.graph.status !== "verified" ||
    manifest.graph.forbiddenPackages?.length !== 0
  ) {
    throw new Error("candidate source or graph identity changed");
  }
  if (
    manifest.optimized &&
    (manifest.measuredGraph?.status !== "verified" ||
      manifest.measuredGraph?.testSupportEnabled !== false ||
      manifest.measuredGraph?.profilerEnabled !== true)
  ) {
    throw new Error("candidate measured feature graph changed");
  }
  for (const receipt of [
    manifest.runtime.binary,
    manifest.runtime.worker,
    manifest.runtime.pdfium,
    manifest.source.cargoLock,
    manifest.source.rustToolchain,
    manifest.source.preparationPolicy,
    manifest.build,
    ...manifest.source.compatFiles,
  ]) {
    const path = resolve(repositoryDirectory, receipt.path);
    const actual = await artifact(path, {
      executable:
        receipt === manifest.runtime.binary ||
        receipt === manifest.runtime.worker,
    });
    if (actual.bytes !== receipt.bytes || actual.sha256 !== receipt.sha256) {
      throw new Error(`candidate artifact changed: ${receipt.path}`);
    }
  }
  return true;
}

async function main(argv) {
  const command = argv[0];
  if (command === "validate" && argv.length === 2) {
    const manifest = JSON.parse(await readFile(resolve(argv[1]), "utf8"));
    await validateComponentDevelopmentCandidate(manifest);
    process.stdout.write(
      `${JSON.stringify({ status: "verified", profile: manifest.profile })}\n`,
    );
    return;
  }
  if (command === "prepare") {
    const options = Object.fromEntries(
      argv.slice(1).reduce((pairs, value, index, values) => {
        if (index % 2 === 0) pairs.push([value, values[index + 1]]);
        return pairs;
      }, []),
    );
    for (const name of [
      "--output",
      "--binary",
      "--worker",
      "--pdfium",
      "--build-summary",
    ]) {
      if (!options[name]) throw new Error(`missing ${name}`);
    }
    const kind = options["--kind"] ?? "development";
    if (!["development", "optimized"].includes(kind)) {
      throw new Error("--kind must be development or optimized");
    }
    const buildCandidate =
      kind === "optimized"
        ? buildComponentOptimizedCandidate
        : buildComponentDevelopmentCandidate;
    const manifest = await buildCandidate({
      binary: resolve(options["--binary"]),
      worker: resolve(options["--worker"]),
      pdfium: resolve(options["--pdfium"]),
      buildSummary: resolve(options["--build-summary"]),
    });
    await writeFile(
      resolve(options["--output"]),
      `${JSON.stringify(manifest, null, 2)}\n`,
    );
    process.stdout.write(
      `${JSON.stringify({ status: "prepared", profile: manifest.profile, manifestSha256: manifest.manifestSha256 })}\n`,
    );
    return;
  }
  throw new Error(
    "usage: component-development-candidate.mjs prepare [--kind development|optimized] --output <path> --binary <path> --worker <path> --pdfium <path> --build-summary <path> | validate <manifest>",
  );
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
