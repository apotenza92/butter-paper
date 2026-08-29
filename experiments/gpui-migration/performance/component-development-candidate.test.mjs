import assert from "node:assert/strict";
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";

import {
  buildComponentDevelopmentCandidate,
  buildComponentOptimizedCandidate,
  componentDevelopmentCandidateProfile,
  componentOptimizedCandidateProfile,
  repositoryDirectory,
  validateComponentDevelopmentCandidate,
} from "./component-development-candidate.mjs";

test("seals a development-only Longbridge candidate and rejects artifact drift", async () => {
  const root = resolve(repositoryDirectory, "test-results");
  await mkdir(root, { recursive: true });
  const directory = await mkdtemp(
    resolve(root, "component-development-candidate-"),
  );
  try {
    const binary = resolve(directory, "component_story");
    const worker = resolve(directory, "pdf-worker");
    const pdfium = resolve(directory, "libpdfium.so");
    const policy = resolve(directory, "source-preparation-policy.json");
    const lock = resolve(directory, "Cargo.lock");
    const buildSummary = resolve(directory, "build-summary.json");
    const sourceFile = resolve(directory, "component_story.rs");
    await Promise.all([
      writeFile(binary, "component story"),
      writeFile(worker, "worker"),
      writeFile(pdfium, "pdfium"),
      writeFile(lock, "lock"),
      writeFile(buildSummary, "{}\n"),
      writeFile(sourceFile, "fn main() {}\n"),
      writeFile(
        policy,
        `${JSON.stringify({
          component: { revision: "c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4" },
          zed: { revision: "8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc" },
          prepared: { treeSha256: "a".repeat(64) },
          patch: { sha256: "b".repeat(64) },
        })}\n`,
      ),
    ]);
    await Promise.all([chmod(binary, 0o755), chmod(worker, 0o755)]);
    const manifest = await buildComponentDevelopmentCandidate({
      binary,
      worker,
      pdfium,
      policy,
      lock,
      buildSummary,
      graphReceipt: {
        status: "verified",
        packageCount: 870,
        gpui: {
          source:
            "git+https://github.com/zed-industries/zed?rev=8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc#8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc",
        },
        forbiddenPackages: [],
      },
      sourceFiles: [sourceFile],
    });
    assert.equal(manifest.profile, componentDevelopmentCandidateProfile);
    assert.equal(manifest.developmentOnly, true);
    assert.equal(manifest.timingEligible, false);
    assert.equal(await validateComponentDevelopmentCandidate(manifest), true);
    await writeFile(binary, "changed component story");
    await assert.rejects(
      validateComponentDevelopmentCandidate(manifest),
      /candidate artifact changed/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("seals an optimized release candidate only from the fixed guarded release mode", async () => {
  const root = resolve(repositoryDirectory, "test-results");
  await mkdir(root, { recursive: true });
  const directory = await mkdtemp(
    resolve(root, "component-optimized-candidate-"),
  );
  try {
    const binary = resolve(directory, "component_story");
    const worker = resolve(directory, "pdf-worker");
    const pdfium = resolve(directory, "libpdfium.so");
    const policy = resolve(directory, "source-preparation-policy.json");
    const lock = resolve(directory, "Cargo.lock");
    const toolchain = resolve(directory, "rust-toolchain.toml");
    const buildSummary = resolve(directory, "build-summary.json");
    const sourceFile = resolve(directory, "component_story.rs");
    await Promise.all([
      writeFile(binary, "optimized component story"),
      writeFile(worker, "optimized worker"),
      writeFile(pdfium, "pdfium"),
      writeFile(lock, "lock"),
      writeFile(toolchain, '[toolchain]\nchannel = "1.97.1"\n'),
      writeFile(sourceFile, "fn main() {}\n"),
      writeFile(
        buildSummary,
        `${JSON.stringify({
          status: 0,
          runnerMode: "perf-story-release",
          cargoSubcommand: "build",
          cargoArguments: [
            "--release",
            "--features",
            "benchmark-evidence",
            "--bin",
            "component_story",
            "--bin",
            "butter-paper-pdf-worker",
          ],
          targetDisposition: { state: "retained", reason: "successful-run" },
        })}\n`,
      ),
      writeFile(
        policy,
        `${JSON.stringify({
          component: { revision: "c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4" },
          zed: { revision: "8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc" },
          prepared: { treeSha256: "a".repeat(64) },
          patch: { sha256: "b".repeat(64) },
        })}\n`,
      ),
    ]);
    await Promise.all([chmod(binary, 0o755), chmod(worker, 0o755)]);
    const manifest = await buildComponentOptimizedCandidate({
      binary,
      worker,
      pdfium,
      policy,
      lock,
      toolchain,
      buildSummary,
      graphReceipt: {
        status: "verified",
        packageCount: 871,
        gpui: {
          source:
            "git+https://github.com/zed-industries/zed?rev=8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc#8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc",
        },
        forbiddenPackages: [],
      },
      measuredGraphReceipt: {
        status: "verified",
        command:
          "cargo tree --locked --offline --features benchmark-evidence -e features,no-dev -i gpui",
        sha256: "c".repeat(64),
        testSupportEnabled: false,
        profilerEnabled: true,
      },
      sourceFiles: [sourceFile],
    });
    assert.equal(manifest.profile, componentOptimizedCandidateProfile);
    assert.equal(manifest.optimized, true);
    assert.equal(manifest.timingEligible, false);
    assert.equal(manifest.measuredGraph.testSupportEnabled, false);
    assert.equal(await validateComponentDevelopmentCandidate(manifest), true);

    await writeFile(
      buildSummary,
      JSON.stringify({ status: 0, runnerMode: "perf-story-build" }),
    );
    await assert.rejects(
      buildComponentOptimizedCandidate({
        binary,
        worker,
        pdfium,
        policy,
        lock,
        toolchain,
        buildSummary,
        graphReceipt: manifest.graph,
        measuredGraphReceipt: manifest.measuredGraph,
        sourceFiles: [sourceFile],
      }),
      /guarded release mode/,
    );

    await writeFile(
      buildSummary,
      `${JSON.stringify({
        status: 0,
        runnerMode: "perf-story-release",
        cargoSubcommand: "build",
        cargoArguments: [
          "--release",
          "--features",
          "benchmark-evidence",
          "--bin",
          "component_story",
          "--bin",
          "butter-paper-pdf-worker",
        ],
        targetDisposition: { state: "retained" },
      })}\n`,
    );
    await assert.rejects(
      buildComponentOptimizedCandidate({
        binary,
        worker,
        pdfium,
        policy,
        lock,
        toolchain,
        buildSummary,
        graphReceipt: manifest.graph,
        measuredGraphReceipt: {
          ...manifest.measuredGraph,
          testSupportEnabled: true,
        },
        sourceFiles: [sourceFile],
      }),
      /measured feature graph/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("rejects a moving or mixed GPUI graph before sealing", async () => {
  const source = await readFile(
    resolve(
      repositoryDirectory,
      "experiments/gpui-migration/gpui-component-compat/source-preparation-policy.json",
    ),
    "utf8",
  );
  assert.match(source, /c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4/);
  await assert.rejects(
    buildComponentDevelopmentCandidate({
      binary: import.meta.filename,
      worker: import.meta.filename,
      pdfium: import.meta.filename,
      buildSummary: import.meta.filename,
      graphReceipt: {
        status: "verified",
        gpui: { source: "git+https://github.com/gpui-ce/gpui" },
        forbiddenPackages: [],
      },
      sourceFiles: [import.meta.filename],
    }),
    /single-GPUI graph/,
  );
});

test("measured component binaries keep GPUI test support out of normal dependencies", async () => {
  const manifest = await readFile(
    resolve(
      repositoryDirectory,
      "experiments/gpui-migration/gpui-component-compat/Cargo.toml",
    ),
    "utf8",
  );
  const dependencies = manifest.match(
    /\[dependencies\]([\s\S]*?)(?:\n\[|$)/,
  )?.[1];
  assert.ok(dependencies, "Cargo manifest must have dependencies");
  assert.doesNotMatch(dependencies, /test-support/);
  assert.match(manifest, /benchmark-evidence = \["gpui\/profiler"\]/);
});
