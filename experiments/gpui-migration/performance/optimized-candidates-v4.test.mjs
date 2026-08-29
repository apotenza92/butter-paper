import assert from "node:assert/strict";
import { createHash } from "node:crypto";
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
  electronRuntimeProfileV4,
  electronRuntimeRootPackagesV4,
  gpuiReleaseBuildJobsV4,
  gpuiReleaseFeaturesV4,
  gpuiRuntimeProfileV4,
  optimizedCandidatePathsV4,
  optimizedCandidateProfileV4,
  prepareOptimizedCandidatesV4,
  revalidateOptimizedCandidateLaunchV4,
  repositoryDirectoryV4,
  resetElectronBundleOutputV4,
  validateOptimizedCandidatesV4,
} from "./optimized-candidates-v4.mjs";

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

async function writeFakeElectronBundle(bundle, assetName = "index.js") {
  await Promise.all([
    mkdir(resolve(bundle, "build"), { recursive: true }),
    mkdir(resolve(bundle, "renderer/main_window/assets"), { recursive: true }),
  ]);
  await Promise.all([
    writeFile(
      resolve(bundle, "build/main.js"),
      'import "@butter-paper/core";\n' +
        'import "@butter-paper/pdf/blank";\n' +
        'import("pdf-lib");\n' +
        'import "tuf-js/dist/error.js";\n' +
        "export const main = true;\n",
    ),
    writeFile(
      resolve(bundle, "build/preload.cjs"),
      "exports.preload = true;\n",
    ),
    writeFile(
      resolve(bundle, "renderer/main_window/index.html"),
      `<meta http-equiv="Content-Security-Policy" content="connect-src 'self' http://localhost:*">\n` +
        `<link rel="modulepreload" href="./assets/${assetName}">\n` +
        `<script type="module" src="./assets/${assetName}"></script>\n`,
    ),
    writeFile(
      resolve(bundle, `renderer/main_window/assets/${assetName}`),
      "export const renderer = true;\n",
    ),
  ]);
}

async function fakeCandidateTree() {
  const temporaryParent = resolve(repositoryDirectoryV4, "test-results");
  await mkdir(temporaryParent, { recursive: true });
  const directory = await mkdtemp(resolve(temporaryParent, "optimized-v4-"));
  const bundle = resolve(directory, "apps/desktop/.vite");
  const release = resolve(directory, "gpui-gallery/target/release");
  const electron = resolve(directory, "electron");
  const gpui = resolve(release, "butter-paper-gpui-gallery");
  const worker = resolve(release, "butter-paper-pdf-worker");
  const productionPaths = optimizedCandidatePathsV4();
  const pdfium = resolve(
    directory,
    "gpui-gallery/target/pdfium-development",
    productionPaths.pdfium_target,
    process.platform === "win32"
      ? "bin/pdfium.dll"
      : `lib/libpdfium.${process.platform === "darwin" ? "dylib" : "so"}`,
  );
  const runtimeRoot = resolve(directory, "runtime");
  const devProvenance = resolve(
    directory,
    "test-results/desktop-dev-provenance.json",
  );
  const electronRuntimePackageRoots = Object.fromEntries(
    electronRuntimeRootPackagesV4.map((name) => [
      name,
      resolve(runtimeRoot, name.replaceAll("/", "--")),
    ]),
  );
  await Promise.all([
    mkdir(release, { recursive: true }),
    mkdir(resolve(directory, "test-results"), { recursive: true }),
    mkdir(resolve(pdfium, ".."), { recursive: true }),
    ...Object.values(electronRuntimePackageRoots).map((root) =>
      mkdir(resolve(root, "dist"), { recursive: true }),
    ),
  ]);
  await writeFakeElectronBundle(bundle);
  await Promise.all([
    writeFile(electron, "optimized electron runtime\n"),
    writeFile(gpui, "cargo release gpui\n"),
    writeFile(worker, "cargo release pdf worker\n"),
    writeFile(pdfium, "pinned synthetic PDFium library\n"),
    writeFile(
      devProvenance,
      `${JSON.stringify({
        schemaVersion: 1,
        version: "0.0.25",
        commit: "1".repeat(40),
        branch: "synthetic-candidate",
        dirty: true,
        checkoutId: "2".repeat(64),
        statusFingerprint: "3".repeat(64),
      })}\n`,
    ),
    ...Object.entries(electronRuntimePackageRoots).flatMap(([name, root]) => [
      writeFile(
        resolve(root, "package.json"),
        `${JSON.stringify({ name, version: "1.0.0", dependencies: {} })}\n`,
      ),
      writeFile(
        resolve(root, "dist/index.js"),
        `export const name = ${JSON.stringify(name)};\n`,
      ),
    ]),
  ]);
  await Promise.all([electron, gpui, worker].map((path) => chmod(path, 0o755)));
  return {
    directory,
    output: resolve(directory, "manifests"),
    electron,
    gpui,
    worker,
    pdfium,
    bundle,
    devProvenance,
    candidatePaths: {
      electron_executable: electron,
      electron_bundle_directory: bundle,
      desktop_dev_provenance: devProvenance,
      gpui_binary: gpui,
      pdf_worker: worker,
      pdfium_target: productionPaths.pdfium_target,
      pdfium_library: pdfium,
      pdfium_pin_manifest: productionPaths.pdfium_pin_manifest,
      electron_runtime_package_roots: electronRuntimePackageRoots,
    },
  };
}

function fakeBuildCommandRunner(fixture, assetName = "index.js") {
  return async (executable, args) => {
    if (executable === "pnpm" && args.includes("@butter-paper/desktop")) {
      await writeFakeElectronBundle(fixture.bundle, assetName);
    }
  };
}

function fakePdfiumFetcher(fixture) {
  return async () => fixture.pdfium;
}

test("requires an explicit valid desktop dev provenance input", async () => {
  const fixture = await fakeCandidateTree();
  try {
    await assert.rejects(
      prepareOptimizedCandidatesV4({
        output: fixture.output,
        electronExecutable: fixture.electron,
        runBuilds: false,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /requires an explicit desktop dev provenance input/,
    );
    const invalidProvenance = JSON.parse(
      await readFile(fixture.devProvenance, "utf8"),
    );
    invalidProvenance.version = "0.0.0";
    await writeFile(
      fixture.devProvenance,
      `${JSON.stringify(invalidProvenance)}\n`,
    );
    await assert.rejects(
      prepareOptimizedCandidatesV4({
        output: fixture.output,
        electronExecutable: fixture.electron,
        devProvenancePath: fixture.devProvenance,
        runBuilds: false,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /does not match the desktop package version/,
    );
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

test("prepares and revalidates only frozen optimized candidate artifacts", async () => {
  const fixture = await fakeCandidateTree();
  try {
    const prepared = await prepareOptimizedCandidatesV4({
      output: fixture.output,
      electronExecutable: fixture.electron,
      devProvenancePath: fixture.devProvenance,
      runBuilds: true,
      candidatePaths: fixture.candidatePaths,
      commandRunner: fakeBuildCommandRunner(fixture),
      pdfiumFetcher: fakePdfiumFetcher(fixture),
      testOnlyCandidateRoot: fixture.directory,
    });
    assert.equal(
      prepared.electron.candidate_profile,
      optimizedCandidateProfileV4,
    );
    assert.equal(prepared.electron.runtime_profile, electronRuntimeProfileV4);
    assert.equal(prepared.gpui.runtime_profile, gpuiRuntimeProfileV4);
    assert.equal(prepared.gpui.build.cargo_profile, "release");
    assert.equal(prepared.gpui.build.default_features, false);
    assert.equal(prepared.gpui.build.jobs, gpuiReleaseBuildJobsV4);
    assert.deepEqual(prepared.gpui.build.features, gpuiReleaseFeaturesV4);
    assert.match(prepared.gpui.build.command, /--no-default-features/);
    assert.match(prepared.gpui.build.command, /--jobs 1/);
    assert.deepEqual(prepared.electron.bundle.compiled_external_imports, [
      "@butter-paper/core",
      "@butter-paper/pdf",
      "pdf-lib",
      "tuf-js",
    ]);
    const validated = await validateOptimizedCandidatesV4({
      electronManifestPath: prepared.electronManifestPath,
      gpuiManifestPath: prepared.gpuiManifestPath,
      electronExecutable: fixture.electron,
      gpuiBinary: fixture.gpui,
      candidatePaths: fixture.candidatePaths,
      testOnlyCandidateRoot: fixture.directory,
    });
    assert.match(validated.electron.bundle_tree_sha256, /^[0-9a-f]{64}$/);
    assert.match(validated.gpui.pdf_worker.sha256, /^[0-9a-f]{64}$/);

    await writeFile(fixture.worker, "mutated worker\n");
    await assert.rejects(
      validateOptimizedCandidatesV4({
        electronManifestPath: prepared.electronManifestPath,
        gpuiManifestPath: prepared.gpuiManifestPath,
        electronExecutable: fixture.electron,
        gpuiBinary: fixture.gpui,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /PDF worker no longer matches/,
    );
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

test("rejects a compiled import outside the sealed runtime closure", async () => {
  const fixture = await fakeCandidateTree();
  try {
    await writeFile(
      resolve(fixture.bundle, "build/main.js"),
      'import "unsealed-runtime-package";\n',
    );
    await assert.rejects(
      prepareOptimizedCandidatesV4({
        output: fixture.output,
        electronExecutable: fixture.electron,
        devProvenancePath: fixture.devProvenance,
        runBuilds: false,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /compiled import is outside the sealed runtime dependency closure: unsealed-runtime-package/,
    );
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

test("permits the maintained localhost CSP but rejects a localhost module asset", async () => {
  const fixture = await fakeCandidateTree();
  try {
    await prepareOptimizedCandidatesV4({
      output: fixture.output,
      electronExecutable: fixture.electron,
      devProvenancePath: fixture.devProvenance,
      runBuilds: false,
      candidatePaths: fixture.candidatePaths,
      testOnlyCandidateRoot: fixture.directory,
    });
    await writeFile(
      resolve(fixture.bundle, "renderer/main_window/index.html"),
      '<meta http-equiv="Content-Security-Policy" content="connect-src http://localhost:*">\n' +
        '<script type="module" src="http://localhost:5173/src/main.tsx"></script>\n',
    );
    await assert.rejects(
      prepareOptimizedCandidatesV4({
        output: fixture.output,
        electronExecutable: fixture.electron,
        devProvenancePath: fixture.devProvenance,
        runBuilds: false,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /development-server asset URL/,
    );
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

test("rejects development-server Electron assets and a debug GPUI path", async () => {
  const fixture = await fakeCandidateTree();
  try {
    await writeFile(
      resolve(fixture.bundle, "renderer/main_window/index.html"),
      '<script type="module" src="/@vite/client"></script>\n',
    );
    await assert.rejects(
      prepareOptimizedCandidatesV4({
        output: fixture.output,
        electronExecutable: fixture.electron,
        devProvenancePath: fixture.devProvenance,
        runBuilds: false,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /development-server marker/,
    );

    await writeFile(
      resolve(fixture.bundle, "renderer/main_window/index.html"),
      '<script type="module" src="./assets/index.js"></script>\n',
    );
    const prepared = await prepareOptimizedCandidatesV4({
      output: fixture.output,
      electronExecutable: fixture.electron,
      devProvenancePath: fixture.devProvenance,
      runBuilds: true,
      candidatePaths: fixture.candidatePaths,
      commandRunner: fakeBuildCommandRunner(fixture),
      pdfiumFetcher: fakePdfiumFetcher(fixture),
      testOnlyCandidateRoot: fixture.directory,
    });
    await assert.rejects(
      validateOptimizedCandidatesV4({
        electronManifestPath: prepared.electronManifestPath,
        gpuiManifestPath: prepared.gpuiManifestPath,
        electronExecutable: fixture.electron,
        gpuiBinary: resolve(
          fixture.directory,
          "gpui-gallery/target/debug/butter-paper-gpui-gallery",
        ),
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /Cargo release artifact/,
    );
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

test("production candidate preparation cannot retain stale Vite assets", async () => {
  const fixture = await fakeCandidateTree();
  const staleAsset = resolve(
    fixture.bundle,
    "renderer/main_window/assets/pdfSession-stale.js",
  );
  try {
    await writeFile(staleAsset, "export const stale = true;\n");
    const commandRunner = async (executable, args) => {
      if (executable === "pnpm" && args.includes("@butter-paper/desktop")) {
        await Promise.all([
          mkdir(resolve(fixture.bundle, "build"), { recursive: true }),
          mkdir(resolve(fixture.bundle, "renderer/main_window/assets"), {
            recursive: true,
          }),
        ]);
        await Promise.all([
          writeFile(
            resolve(fixture.bundle, "build/main.js"),
            "export const main = 'fresh';\n",
          ),
          writeFile(
            resolve(fixture.bundle, "build/preload.cjs"),
            "exports.preload = 'fresh';\n",
          ),
          writeFile(
            resolve(fixture.bundle, "renderer/main_window/index.html"),
            '<script type="module" src="./assets/pdfSession-current.js"></script>\n',
          ),
          writeFile(
            resolve(
              fixture.bundle,
              "renderer/main_window/assets/pdfSession-current.js",
            ),
            "export const current = true;\n",
          ),
        ]);
      }
    };
    const prepared = await prepareOptimizedCandidatesV4({
      output: resolve(fixture.directory, "fresh-manifests"),
      electronExecutable: fixture.electron,
      devProvenancePath: fixture.devProvenance,
      runBuilds: true,
      candidatePaths: fixture.candidatePaths,
      commandRunner,
      pdfiumFetcher: fakePdfiumFetcher(fixture),
      testOnlyCandidateRoot: fixture.directory,
    });
    assert(
      prepared.electron.bundle.files.some(({ path }) =>
        path.endsWith("pdfSession-current.js"),
      ),
    );
    assert(
      !prepared.electron.bundle.files.some(({ path }) =>
        path.endsWith("pdfSession-stale.js"),
      ),
      "a stale output from an earlier Vite build entered the candidate manifest",
    );
    assert.equal(
      prepared.electron.build.output_tree_reset_before_build.performed,
      true,
    );
    assert.equal(
      prepared.electron.build.output_tree_reset_before_build.scope,
      "disposable-vite-output-only",
    );
    assert(
      prepared.electron.build.output_tree_reset_before_build.path.endsWith(
        "/apps/desktop/.vite",
      ),
    );
    await assert.rejects(
      resetElectronBundleOutputV4(fixture.directory),
      /refusing to reset non-disposable Electron output/,
    );
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

test("only resets the canonical output or an explicit test fixture output", async () => {
  const fixture = await fakeCandidateTree();
  const invalidTestRoot = resolve(
    repositoryDirectoryV4,
    "experiments/gpui-migration",
  );
  try {
    await assert.rejects(
      resetElectronBundleOutputV4(fixture.bundle),
      /refusing to reset non-disposable Electron output/,
    );
    await assert.rejects(
      resetElectronBundleOutputV4(
        resolve(invalidTestRoot, "apps/desktop/.vite"),
        { testOnlyCandidateRoot: invalidTestRoot },
      ),
      /test-only candidate root must be below test-results/,
    );
    const evidence = await resetElectronBundleOutputV4(fixture.bundle, {
      testOnlyCandidateRoot: fixture.directory,
    });
    assert.equal(evidence.performed, true);
    assert.equal(
      evidence.path,
      `${fixture.directory.slice(repositoryDirectoryV4.length + 1)}/apps/desktop/.vite`,
    );
    await assert.rejects(readFile(resolve(fixture.bundle, "build/main.js")), {
      code: "ENOENT",
    });
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

test("rejects candidate manifests when the Vite reset was not performed", async () => {
  const fixture = await fakeCandidateTree();
  try {
    const prepared = await prepareOptimizedCandidatesV4({
      output: fixture.output,
      electronExecutable: fixture.electron,
      devProvenancePath: fixture.devProvenance,
      runBuilds: false,
      candidatePaths: fixture.candidatePaths,
      testOnlyCandidateRoot: fixture.directory,
    });
    await assert.rejects(
      validateOptimizedCandidatesV4({
        electronManifestPath: prepared.electronManifestPath,
        gpuiManifestPath: prepared.gpuiManifestPath,
        electronExecutable: fixture.electron,
        gpuiBinary: fixture.gpui,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /requires an exact performed clean Vite build/,
    );
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

test("detects a mutation in the Electron runtime dependency closure", async () => {
  const fixture = await fakeCandidateTree();
  try {
    const prepared = await prepareOptimizedCandidatesV4({
      output: fixture.output,
      electronExecutable: fixture.electron,
      devProvenancePath: fixture.devProvenance,
      runBuilds: true,
      candidatePaths: fixture.candidatePaths,
      commandRunner: fakeBuildCommandRunner(fixture),
      pdfiumFetcher: fakePdfiumFetcher(fixture),
      testOnlyCandidateRoot: fixture.directory,
    });
    await writeFile(
      resolve(
        fixture.candidatePaths.electron_runtime_package_roots["tuf-js"],
        "dist/index.js",
      ),
      "export const mutated = true;\n",
    );
    await assert.rejects(
      validateOptimizedCandidatesV4({
        electronManifestPath: prepared.electronManifestPath,
        gpuiManifestPath: prepared.gpuiManifestPath,
        electronExecutable: fixture.electron,
        gpuiBinary: fixture.gpui,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /runtime dependency closure no longer matches/,
    );
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

test("returns hash-bound evidence for immediate pre-launch revalidation", async () => {
  const fixture = await fakeCandidateTree();
  try {
    const prepared = await prepareOptimizedCandidatesV4({
      output: fixture.output,
      electronExecutable: fixture.electron,
      devProvenancePath: fixture.devProvenance,
      runBuilds: true,
      candidatePaths: fixture.candidatePaths,
      commandRunner: fakeBuildCommandRunner(fixture),
      pdfiumFetcher: fakePdfiumFetcher(fixture),
      testOnlyCandidateRoot: fixture.directory,
    });
    const receipt = await revalidateOptimizedCandidateLaunchV4({
      launchId: "pair-001-electron",
      electronManifestPath: prepared.electronManifestPath,
      gpuiManifestPath: prepared.gpuiManifestPath,
      electronExecutable: fixture.electron,
      gpuiBinary: fixture.gpui,
      candidatePaths: fixture.candidatePaths,
      testOnlyCandidateRoot: fixture.directory,
    });
    const { evidence_sha256: evidenceSha256, ...payload } = receipt;
    assert.equal(receipt.revalidated_immediately_before_launch, true);
    assert.equal(receipt.launch_id, "pair-001-electron");
    assert.equal(evidenceSha256, canonicalSha256(payload));
    assert.match(
      receipt.electron_desktop_dev_provenance_sha256,
      /^[0-9a-f]{64}$/,
    );
    assert.equal(
      receipt.gpui_pdfium_library_sha256,
      prepared.gpui.pdfium.library.sha256,
    );
    assert(receipt.ended_monotonic_ms >= receipt.started_monotonic_ms);

    const originalPdfium = await readFile(fixture.pdfium);
    await writeFile(fixture.pdfium, "mutated PDFium library\n");
    await assert.rejects(
      revalidateOptimizedCandidateLaunchV4({
        launchId: "pair-002-gpui",
        electronManifestPath: prepared.electronManifestPath,
        gpuiManifestPath: prepared.gpuiManifestPath,
        electronExecutable: fixture.electron,
        gpuiBinary: fixture.gpui,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /pinned PDFium library no longer matches/,
    );
    await writeFile(fixture.pdfium, originalPdfium);
    const originalProvenance = await readFile(fixture.devProvenance);
    await writeFile(
      fixture.devProvenance,
      originalProvenance
        .toString("utf8")
        .replace("synthetic-candidate", "mutated-candidate"),
    );
    await assert.rejects(
      revalidateOptimizedCandidateLaunchV4({
        launchId: "pair-003-electron",
        electronManifestPath: prepared.electronManifestPath,
        gpuiManifestPath: prepared.gpuiManifestPath,
        electronExecutable: fixture.electron,
        gpuiBinary: fixture.gpui,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /desktop dev provenance no longer matches/,
    );
    await writeFile(fixture.devProvenance, originalProvenance);
    await writeFile(fixture.electron, "mutated Electron runtime\n");
    await assert.rejects(
      revalidateOptimizedCandidateLaunchV4({
        launchId: "pair-004-electron",
        electronManifestPath: prepared.electronManifestPath,
        gpuiManifestPath: prepared.gpuiManifestPath,
        electronExecutable: fixture.electron,
        gpuiBinary: fixture.gpui,
        candidatePaths: fixture.candidatePaths,
        testOnlyCandidateRoot: fixture.directory,
      }),
      /Electron executable no longer matches/,
    );
  } finally {
    await rm(fixture.directory, { recursive: true, force: true });
  }
});
