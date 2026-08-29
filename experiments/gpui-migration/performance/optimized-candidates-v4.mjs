#!/usr/bin/env node

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, readFileSync } from "node:fs";
import {
  lstat,
  mkdir,
  readFile,
  readdir,
  realpath,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { arch, platform } from "node:os";
import { dirname, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import {
  fetchDevelopmentPdfium,
  hostTarget as pdfiumHostTarget,
  validateManifest as validatePdfiumDevelopmentManifest,
} from "../gpui-gallery/scripts/fetch-pdfium-development.mjs";

const execFileAsync = promisify(execFile);
const performanceDirectory = dirname(fileURLToPath(import.meta.url));
export const repositoryDirectoryV4 = resolve(performanceDirectory, "../../..");
const desktopDirectory = resolve(repositoryDirectoryV4, "apps/desktop");
const galleryDirectory = resolve(
  repositoryDirectoryV4,
  "experiments/gpui-migration/gpui-gallery",
);
const pdfiumDevelopmentManifestPath = resolve(
  galleryDirectory,
  "pdfium-development-binaries.json",
);
const manifestSchemaVersion = 1;
export const optimizedCandidateProfileV4 = "bp-perf-v4-optimized-candidate-1";
export const electronRuntimeProfileV4 =
  "vite-production-bundles-unpackaged-electron-runtime";
export const gpuiRuntimeProfileV4 = "cargo-release-with-benchmark-evidence";
export const gpuiReleaseFeaturesV4 = Object.freeze([
  "gallery",
  "benchmark-evidence",
  "pdfium-worker",
]);
export const gpuiReleaseBuildJobsV4 = 1;
export const electronRuntimeRootPackagesV4 = Object.freeze([
  "@butter-paper/core",
  "@butter-paper/pdf",
  "pdf-lib",
  "tuf-js",
]);

function gpuiReleaseBuildArgumentsV4() {
  return [
    "build",
    "--locked",
    "--release",
    "--no-default-features",
    "--features",
    gpuiReleaseFeaturesV4.join(","),
    "--jobs",
    String(gpuiReleaseBuildJobsV4),
    "--bin",
    "butter-paper-gpui-gallery",
    "--bin",
    "butter-paper-pdf-worker",
  ];
}

function defaultElectronExecutable() {
  if (platform() === "darwin") {
    return resolve(
      repositoryDirectoryV4,
      "node_modules/electron/dist/Electron.app/Contents/MacOS/Electron",
    );
  }
  return resolve(
    repositoryDirectoryV4,
    platform() === "win32"
      ? "node_modules/electron/dist/electron.exe"
      : "node_modules/electron/dist/electron",
  );
}

function executableName(name) {
  return platform() === "win32" ? `${name}.exe` : name;
}

export function optimizedCandidatePathsV4() {
  const releaseDirectory = resolve(galleryDirectory, "target/release");
  const pdfiumTarget = pdfiumHostTarget();
  const pdfiumManifest = JSON.parse(
    readFileSync(pdfiumDevelopmentManifestPath, "utf8"),
  );
  validatePdfiumDevelopmentManifest(pdfiumManifest);
  const pdfiumAsset = pdfiumManifest.assets.find(
    ({ target }) => target === pdfiumTarget,
  );
  if (!pdfiumAsset) {
    throw new Error(`no pinned development PDFium asset for ${pdfiumTarget}`);
  }
  return {
    electron_executable: defaultElectronExecutable(),
    electron_bundle_directory: resolve(desktopDirectory, ".vite"),
    desktop_dev_provenance: resolve(
      repositoryDirectoryV4,
      "test-results/desktop-dev-provenance.json",
    ),
    gpui_binary: resolve(
      releaseDirectory,
      executableName("butter-paper-gpui-gallery"),
    ),
    pdf_worker: resolve(
      releaseDirectory,
      executableName("butter-paper-pdf-worker"),
    ),
    pdfium_target: pdfiumTarget,
    pdfium_library: resolve(
      galleryDirectory,
      "target/pdfium-development",
      pdfiumTarget,
      pdfiumAsset.library,
    ),
    pdfium_pin_manifest: pdfiumDevelopmentManifestPath,
    electron_runtime_package_roots: {
      "@butter-paper/core": resolve(repositoryDirectoryV4, "packages/core"),
      "@butter-paper/pdf": resolve(repositoryDirectoryV4, "packages/pdf"),
      "pdf-lib": resolve(repositoryDirectoryV4, "node_modules/pdf-lib"),
      "tuf-js": resolve(repositoryDirectoryV4, "node_modules/tuf-js"),
    },
  };
}

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
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", rejectPromise);
    stream.on("end", resolvePromise);
  });
  return hash.digest("hex");
}

function repositoryRelativePath(path) {
  const candidate = relative(repositoryDirectoryV4, resolve(path));
  if (
    candidate.length === 0 ||
    candidate === ".." ||
    candidate.startsWith(`..${sep}`)
  ) {
    throw new Error(`candidate artifact is outside the repository: ${path}`);
  }
  return candidate.split(sep).join("/");
}

function resolveRepositoryPath(path) {
  if (typeof path !== "string" || path.length === 0 || path.startsWith("/")) {
    throw new Error("candidate manifest paths must be repository-relative");
  }
  const resolved = resolve(repositoryDirectoryV4, path);
  const candidate = relative(repositoryDirectoryV4, resolved);
  if (candidate === ".." || candidate.startsWith(`..${sep}`)) {
    throw new Error(`candidate manifest path escapes the repository: ${path}`);
  }
  return resolved;
}

async function artifact(path, { executable = false } = {}) {
  const metadata = await stat(path);
  if (!metadata.isFile())
    throw new Error(`candidate artifact is not a file: ${path}`);
  if (executable && platform() !== "win32" && (metadata.mode & 0o111) === 0) {
    throw new Error(`candidate artifact is not executable: ${path}`);
  }
  return {
    path: repositoryRelativePath(path),
    bytes: metadata.size,
    sha256: await sha256File(path),
  };
}

async function treeFiles(directory) {
  const files = [];
  async function visit(current) {
    const entries = await readdir(current, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const path = resolve(current, entry.name);
      if (entry.isDirectory()) await visit(path);
      else if (entry.isFile()) files.push(path);
      else
        throw new Error(`candidate bundle contains a non-file entry: ${path}`);
    }
  }
  await visit(directory);
  return files;
}

function runtimePackageNameFromImportV4(specifier) {
  if (
    specifier === "electron" ||
    specifier.startsWith("node:") ||
    specifier.startsWith(".") ||
    specifier.startsWith("/")
  ) {
    return null;
  }
  if (specifier.startsWith("@")) {
    return specifier.split("/").slice(0, 2).join("/");
  }
  return specifier.split("/")[0];
}

async function compiledExternalImportsV4(files) {
  const imports = new Set();
  const callPattern = /\b(?:require|import)\(\s*["']([^"'\r\n]+)["']\s*\)/g;
  const declarationPattern =
    /^\s*(?:import(?:\s+[^"']+?\s+from)?|export\s+[^"']+?\s+from)\s*["']([^"']+)["']/;
  for (const { path } of files) {
    if (!/apps\/desktop\/\.vite\/build\/.*\.(?:c?js|mjs)$/.test(path)) {
      continue;
    }
    const source = await readFile(resolveRepositoryPath(path), "utf8");
    for (const line of source.split("\n")) {
      const match = line.match(declarationPattern);
      const packageName = match
        ? runtimePackageNameFromImportV4(match[1])
        : null;
      if (packageName) imports.add(packageName);
    }
    for (const match of source.matchAll(callPattern)) {
      const packageName = runtimePackageNameFromImportV4(match[1]);
      if (packageName) imports.add(packageName);
    }
  }
  return [...imports].sort();
}

function authorizedElectronBundleOutputPathV4(
  bundleDirectory,
  { testOnlyCandidateRoot = null } = {},
) {
  const resolved = resolve(bundleDirectory);
  const canonicalOutput = resolve(desktopDirectory, ".vite");
  const testOutput = testOnlyCandidateRoot
    ? resolve(testOnlyCandidateRoot, "apps/desktop/.vite")
    : null;
  if (
    testOnlyCandidateRoot &&
    !repositoryRelativePath(testOnlyCandidateRoot).startsWith("test-results/")
  ) {
    throw new Error("test-only candidate root must be below test-results");
  }
  if (resolved !== canonicalOutput && resolved !== testOutput) {
    throw new Error(
      `refusing to reset non-disposable Electron output: ${repositoryRelativePath(resolved)}`,
    );
  }
  return repositoryRelativePath(resolved);
}

function authorizedDesktopDevProvenancePathV4(
  provenancePath,
  { testOnlyCandidateRoot = null } = {},
) {
  const resolved = resolve(provenancePath);
  const canonicalOutput = resolve(
    repositoryDirectoryV4,
    "test-results/desktop-dev-provenance.json",
  );
  const testOutput = testOnlyCandidateRoot
    ? resolve(testOnlyCandidateRoot, "test-results/desktop-dev-provenance.json")
    : null;
  if (
    testOnlyCandidateRoot &&
    !repositoryRelativePath(testOnlyCandidateRoot).startsWith("test-results/")
  ) {
    throw new Error("test-only candidate root must be below test-results");
  }
  if (resolved !== canonicalOutput && resolved !== testOutput) {
    throw new Error(
      `desktop dev provenance runtime path is not exact: ${repositoryRelativePath(resolved)}`,
    );
  }
  return resolved;
}

async function parseDesktopDevProvenanceV4(path) {
  const bytes = await readFile(path);
  let value;
  try {
    value = JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error("desktop dev provenance is not valid JSON");
  }
  const desktopPackage = JSON.parse(
    await readFile(resolve(desktopDirectory, "package.json"), "utf8"),
  );
  if (
    value?.schemaVersion !== 1 ||
    value?.version !== desktopPackage.version ||
    typeof value?.commit !== "string" ||
    !/^[a-f0-9]{40}$/i.test(value.commit) ||
    typeof value?.branch !== "string" ||
    value.branch.length === 0 ||
    typeof value?.dirty !== "boolean" ||
    typeof value?.checkoutId !== "string" ||
    !/^[a-f0-9]{64}$/i.test(value.checkoutId) ||
    typeof value?.statusFingerprint !== "string" ||
    !/^[a-f0-9]{64}$/i.test(value.statusFingerprint)
  ) {
    throw new Error(
      "desktop dev provenance is invalid or does not match the desktop package version",
    );
  }
  return {
    bytes,
    identity: {
      schema_version: value.schemaVersion,
      version: value.version,
      commit: value.commit,
      branch: value.branch,
      dirty: value.dirty,
      checkout_id: value.checkoutId,
      status_fingerprint: value.statusFingerprint,
    },
  };
}

async function installDesktopDevProvenanceV4({
  inputPath,
  runtimePath,
  testOnlyCandidateRoot,
}) {
  if (!inputPath) {
    throw new Error(
      "optimized Electron candidate preparation requires an explicit desktop dev provenance input",
    );
  }
  const parsed = await parseDesktopDevProvenanceV4(resolve(inputPath));
  const target = authorizedDesktopDevProvenancePathV4(runtimePath, {
    testOnlyCandidateRoot,
  });
  if (resolve(inputPath) !== target) {
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, parsed.bytes, { mode: 0o600 });
  }
  const installed = await parseDesktopDevProvenanceV4(target);
  if (!parsed.bytes.equals(installed.bytes)) {
    throw new Error(
      "installed desktop dev provenance bytes do not match input",
    );
  }
  return {
    artifact: await artifact(target),
    identity: installed.identity,
  };
}

async function pdfiumCandidateEvidenceV4(
  paths,
  { testOnlyCandidateRoot = null } = {},
) {
  if (resolve(paths.pdfium_pin_manifest) !== pdfiumDevelopmentManifestPath) {
    throw new Error("GPUI candidate must use the reviewed PDFium pin manifest");
  }
  const manifest = JSON.parse(
    await readFile(paths.pdfium_pin_manifest, "utf8"),
  );
  validatePdfiumDevelopmentManifest(manifest);
  const asset = manifest.assets.find(
    ({ target }) => target === paths.pdfium_target,
  );
  if (!asset) {
    throw new Error(
      `GPUI candidate has no pinned PDFium asset for ${paths.pdfium_target}`,
    );
  }
  const expectedLibrary = resolve(
    galleryDirectory,
    "target/pdfium-development",
    paths.pdfium_target,
    asset.library,
  );
  const testLibrary = testOnlyCandidateRoot
    ? resolve(
        testOnlyCandidateRoot,
        "gpui-gallery/target/pdfium-development",
        paths.pdfium_target,
        asset.library,
      )
    : null;
  if (
    resolve(paths.pdfium_library) !== expectedLibrary &&
    resolve(paths.pdfium_library) !== testLibrary
  ) {
    throw new Error(
      `GPUI candidate PDFium library path does not match the pinned target: ${expectedLibrary}`,
    );
  }
  return {
    target: paths.pdfium_target,
    api_build: manifest.apiBuild,
    archive_sha256: asset.sha256,
    archive_bytes: asset.bytes,
    library_relative_path: asset.library,
    pin_manifest: await artifact(paths.pdfium_pin_manifest),
    library: await artifact(paths.pdfium_library),
  };
}

function runtimeArtifactClosureV4(runtimeDependencies, devProvenance) {
  const payload = {
    dependency_tree_sha256: runtimeDependencies.tree_sha256,
    desktop_dev_provenance_sha256: devProvenance.artifact.sha256,
  };
  return {
    ...payload,
    tree_sha256: canonicalSha256(payload),
    desktop_dev_provenance: devProvenance,
  };
}

export async function resetElectronBundleOutputV4(
  bundleDirectory,
  { testOnlyCandidateRoot = null } = {},
) {
  const resolved = resolve(bundleDirectory);
  const repositoryRelative = authorizedElectronBundleOutputPathV4(resolved, {
    testOnlyCandidateRoot,
  });
  try {
    const metadata = await lstat(resolved);
    if (metadata.isSymbolicLink()) {
      throw new Error(
        `refusing to reset symlinked Electron output: ${repositoryRelative}`,
      );
    }
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  await rm(resolved, { recursive: true, force: true });
  return {
    performed: true,
    path: repositoryRelative,
    scope: "disposable-vite-output-only",
  };
}

async function packageRootIfPresent(path) {
  try {
    const resolved = await realpath(path);
    const packageJson = JSON.parse(
      await readFile(resolve(resolved, "package.json"), "utf8"),
    );
    return { root: resolved, packageJson };
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}

async function resolveRuntimeDependencyRootV4(
  packageName,
  parentRoot,
  explicitRoots,
) {
  const candidates = [
    explicitRoots[packageName],
    resolve(parentRoot, "node_modules", packageName),
    resolve(repositoryDirectoryV4, "node_modules", packageName),
    resolve(desktopDirectory, "node_modules", packageName),
  ].filter(Boolean);
  for (const candidate of candidates) {
    const resolved = await packageRootIfPresent(candidate);
    if (resolved) return resolved;
  }
  return null;
}

async function runtimePackageFilesV4(root) {
  const files = [];
  async function visit(current) {
    const entries = await readdir(current, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      if (entry.name === "node_modules") continue;
      const path = resolve(current, entry.name);
      if (entry.isDirectory()) await visit(path);
      else if (entry.isSymbolicLink()) {
        throw new Error(`runtime package contains a symlink: ${path}`);
      } else if (
        entry.isFile() &&
        !entry.name.endsWith(".d.ts") &&
        !entry.name.endsWith(".d.ts.map") &&
        !entry.name.endsWith(".js.map")
      ) {
        files.push(path);
      }
    }
  }
  await visit(root);
  return files;
}

export async function electronRuntimeClosureEvidenceV4(packageRoots) {
  const explicitRoots = { ...packageRoots };
  if (
    JSON.stringify(Object.keys(explicitRoots).sort()) !==
    JSON.stringify([...electronRuntimeRootPackagesV4].sort())
  ) {
    throw new Error("Electron runtime root package set is not exact");
  }
  const pending = electronRuntimeRootPackagesV4.map((name) => ({
    name,
    parentRoot: desktopDirectory,
    optional: false,
  }));
  const packages = [];
  const observedRoots = new Set();
  while (pending.length > 0) {
    const dependency = pending.shift();
    const resolved = await resolveRuntimeDependencyRootV4(
      dependency.name,
      dependency.parentRoot,
      explicitRoots,
    );
    if (!resolved) {
      if (dependency.optional) continue;
      throw new Error(
        `Electron runtime dependency is missing: ${dependency.name}`,
      );
    }
    if (observedRoots.has(resolved.root)) continue;
    observedRoots.add(resolved.root);
    if (resolved.packageJson.name !== dependency.name) {
      throw new Error(
        `Electron runtime dependency name mismatch: expected ${dependency.name}, got ${resolved.packageJson.name}`,
      );
    }
    const files = await Promise.all(
      (await runtimePackageFilesV4(resolved.root)).map((path) =>
        artifact(path),
      ),
    );
    files.sort((left, right) => left.path.localeCompare(right.path));
    packages.push({
      name: dependency.name,
      version: resolved.packageJson.version ?? null,
      root: repositoryRelativePath(resolved.root),
      files,
      tree_sha256: canonicalSha256(files),
    });
    const required = Object.keys(
      resolved.packageJson.dependencies ?? {},
    ).sort();
    const optional = Object.keys(
      resolved.packageJson.optionalDependencies ?? {},
    ).sort();
    for (const name of required) {
      pending.push({ name, parentRoot: resolved.root, optional: false });
    }
    for (const name of optional) {
      if (!required.includes(name)) {
        pending.push({ name, parentRoot: resolved.root, optional: true });
      }
    }
  }
  packages.sort((left, right) => left.name.localeCompare(right.name));
  const files = packages.flatMap((entry) => entry.files);
  const duplicatePaths = files.filter(
    ({ path }, index) =>
      files.findIndex((entry) => entry.path === path) !== index,
  );
  if (duplicatePaths.length > 0) {
    throw new Error("Electron runtime closure contains duplicate file paths");
  }
  return {
    root_packages: [...electronRuntimeRootPackagesV4],
    package_count: packages.length,
    file_count: files.length,
    tree_sha256: canonicalSha256(files),
    packages,
  };
}

async function electronBundleEvidence(bundleDirectory) {
  const required = [
    resolve(bundleDirectory, "build/main.js"),
    resolve(bundleDirectory, "build/preload.cjs"),
    resolve(bundleDirectory, "renderer/main_window/index.html"),
  ];
  for (const path of required) await artifact(path);
  const indexHtml = await readFile(required[2], "utf8");
  for (const marker of ["/@vite/client", "react-refresh"]) {
    if (indexHtml.includes(marker)) {
      throw new Error(
        `Electron renderer contains a development-server marker: ${marker}`,
      );
    }
  }
  const localhostAsset = indexHtml.match(
    /<(?:script|link)\b[^>]*\b(?:src|href)\s*=\s*["']https?:\/\/localhost(?=[:/"'])[^"']*["']/i,
  );
  if (localhostAsset) {
    throw new Error(
      `Electron renderer contains a development-server asset URL: ${localhostAsset[0]}`,
    );
  }
  const files = await Promise.all(
    (await treeFiles(bundleDirectory)).map((path) => artifact(path)),
  );
  const javascript = files.filter(({ path }) => /\.(?:c?js|mjs)$/.test(path));
  const rendererJavascript = files.filter(({ path }) =>
    /apps\/desktop\/\.vite\/renderer\/main_window\/assets\/.*\.js$/.test(path),
  );
  if (javascript.length < 3 || rendererJavascript.length === 0) {
    throw new Error(
      "Electron production bundle is missing compiled JavaScript assets",
    );
  }
  return {
    files,
    file_count: files.length,
    tree_sha256: canonicalSha256(files),
    compiled_external_imports: await compiledExternalImportsV4(files),
    development_server_markers_observed: false,
  };
}

function assertCompiledImportsAreSealedV4(bundle, runtimeClosure) {
  const sealedPackageNames = new Set(
    runtimeClosure.packages.map(({ name }) => name),
  );
  const missing = bundle.compiled_external_imports.filter(
    (name) => !sealedPackageNames.has(name),
  );
  if (missing.length > 0) {
    throw new Error(
      `Electron compiled import is outside the sealed runtime dependency closure: ${missing.join(", ")}`,
    );
  }
}

async function toolVersion(executable, args, cwd) {
  const { stdout } = await execFileAsync(executable, args, { cwd });
  return stdout.trim();
}

export async function prepareOptimizedCandidatesV4({
  output,
  devProvenancePath,
  electronExecutable = defaultElectronExecutable(),
  runBuilds = true,
  candidatePaths = optimizedCandidatePathsV4(),
  commandRunner = execFileAsync,
  pdfiumFetcher = fetchDevelopmentPdfium,
  testOnlyCandidateRoot = null,
} = {}) {
  if (!output)
    throw new Error("optimized candidate output directory is required");
  const paths = { ...candidatePaths };
  paths.electron_executable = resolve(electronExecutable);
  const desktopDevProvenance = await installDesktopDevProvenanceV4({
    inputPath: devProvenancePath,
    runtimePath: paths.desktop_dev_provenance,
    testOnlyCandidateRoot,
  });
  let electronOutputReset = {
    performed: false,
    path: repositoryRelativePath(paths.electron_bundle_directory),
    scope: "builds-not-run-by-this-invocation",
  };
  if (runBuilds) {
    await commandRunner("pnpm", ["--filter", "@butter-paper/core", "build"], {
      cwd: repositoryDirectoryV4,
    });
    await commandRunner("pnpm", ["--filter", "@butter-paper/pdf", "build"], {
      cwd: repositoryDirectoryV4,
    });
    electronOutputReset = await resetElectronBundleOutputV4(
      paths.electron_bundle_directory,
      { testOnlyCandidateRoot },
    );
    await commandRunner(
      "pnpm",
      ["--filter", "@butter-paper/desktop", "build"],
      {
        cwd: repositoryDirectoryV4,
        env: { ...process.env, NODE_ENV: "production" },
      },
    );
    await commandRunner("cargo", gpuiReleaseBuildArgumentsV4(), {
      cwd: galleryDirectory,
    });
    const fetchedPdfium = await pdfiumFetcher(paths.pdfium_target);
    if (resolve(fetchedPdfium) !== resolve(paths.pdfium_library)) {
      throw new Error(
        "pinned PDFium fetch did not return the candidate library path",
      );
    }
  }

  const electronBundle = await electronBundleEvidence(
    paths.electron_bundle_directory,
  );
  const electronRuntimeClosure = await electronRuntimeClosureEvidenceV4(
    paths.electron_runtime_package_roots,
  );
  assertCompiledImportsAreSealedV4(electronBundle, electronRuntimeClosure);
  const electronRuntimeArtifactClosure = runtimeArtifactClosureV4(
    electronRuntimeClosure,
    desktopDevProvenance,
  );
  const pdfium = await pdfiumCandidateEvidenceV4(paths, {
    testOnlyCandidateRoot,
  });
  const electron = {
    schema_version: manifestSchemaVersion,
    candidate_profile: optimizedCandidateProfileV4,
    implementation: "electron",
    runtime_profile: electronRuntimeProfileV4,
    build: {
      command: "pnpm --filter @butter-paper/desktop build",
      node_env: "production",
      packaged: false,
      signed: false,
      installed: false,
      output_tree_reset_before_build: electronOutputReset,
    },
    executable: await artifact(paths.electron_executable, { executable: true }),
    bundle: electronBundle,
    runtime_dependency_closure: electronRuntimeClosure,
    runtime_artifact_closure: electronRuntimeArtifactClosure,
  };
  const gpui = {
    schema_version: manifestSchemaVersion,
    candidate_profile: optimizedCandidateProfileV4,
    implementation: "gpui",
    runtime_profile: gpuiRuntimeProfileV4,
    build: {
      cargo_profile: "release",
      default_features: false,
      features: [...gpuiReleaseFeaturesV4],
      jobs: gpuiReleaseBuildJobsV4,
      command: `cargo ${gpuiReleaseBuildArgumentsV4().join(" ")}`,
      packaged: false,
      signed: false,
      installed: false,
    },
    executable: await artifact(paths.gpui_binary, { executable: true }),
    pdf_worker: await artifact(paths.pdf_worker, { executable: true }),
    pdfium,
  };
  const versions = {
    platform: platform(),
    architecture: arch(),
    node: process.version,
    pnpm: await toolVersion("pnpm", ["--version"], repositoryDirectoryV4),
    rustc: await toolVersion("rustc", ["--version"], galleryDirectory),
  };
  electron.tool_versions = versions;
  gpui.tool_versions = versions;

  await mkdir(output, { recursive: true });
  const electronManifestPath = resolve(
    output,
    "electron-optimized-candidate-v4.json",
  );
  const gpuiManifestPath = resolve(output, "gpui-optimized-candidate-v4.json");
  await writeFile(
    electronManifestPath,
    `${JSON.stringify(electron, null, 2)}\n`,
  );
  await writeFile(gpuiManifestPath, `${JSON.stringify(gpui, null, 2)}\n`);
  return { electronManifestPath, gpuiManifestPath, electron, gpui };
}

async function validateManifestArtifact(
  entry,
  label,
  { executable = false } = {},
) {
  if (
    !/^[0-9a-f]{64}$/.test(entry?.sha256 ?? "") ||
    !Number.isInteger(entry?.bytes)
  ) {
    throw new Error(`${label} manifest artifact is incomplete`);
  }
  const current = await artifact(resolveRepositoryPath(entry.path), {
    executable,
  });
  if (current.sha256 !== entry.sha256 || current.bytes !== entry.bytes) {
    throw new Error(
      `${label} no longer matches its optimized candidate manifest`,
    );
  }
  return current;
}

async function loadManifest(path, implementation, runtimeProfile) {
  const manifest = JSON.parse(await readFile(path, "utf8"));
  if (
    manifest.schema_version !== manifestSchemaVersion ||
    manifest.candidate_profile !== optimizedCandidateProfileV4 ||
    manifest.implementation !== implementation ||
    manifest.runtime_profile !== runtimeProfile
  ) {
    throw new Error(
      `${implementation} candidate manifest has the wrong optimized profile`,
    );
  }
  return manifest;
}

export async function validateOptimizedCandidatesV4({
  electronManifestPath,
  gpuiManifestPath,
  electronExecutable,
  gpuiBinary,
  candidatePaths = optimizedCandidatePathsV4(),
  testOnlyCandidateRoot = null,
}) {
  const electron = await loadManifest(
    electronManifestPath,
    "electron",
    electronRuntimeProfileV4,
  );
  const gpui = await loadManifest(
    gpuiManifestPath,
    "gpui",
    gpuiRuntimeProfileV4,
  );
  if (
    gpui.build?.cargo_profile !== "release" ||
    gpui.build?.default_features !== false ||
    gpui.build?.jobs !== gpuiReleaseBuildJobsV4 ||
    JSON.stringify(gpui.build?.features) !==
      JSON.stringify(gpuiReleaseFeaturesV4)
  ) {
    throw new Error(
      "GPUI candidate is not a benchmark-enabled Cargo release build",
    );
  }
  if (
    electron.build?.node_env !== "production" ||
    electron.build?.packaged !== false
  ) {
    throw new Error(
      "Electron candidate is not an unpackaged production Vite build",
    );
  }
  const outputReset = electron.build?.output_tree_reset_before_build;
  const expectedOutputResetPath = authorizedElectronBundleOutputPathV4(
    candidatePaths.electron_bundle_directory,
    { testOnlyCandidateRoot },
  );
  if (
    outputReset?.performed !== true ||
    outputReset?.path !== expectedOutputResetPath ||
    outputReset?.scope !== "disposable-vite-output-only"
  ) {
    throw new Error(
      "Electron decision candidate requires an exact performed clean Vite build",
    );
  }
  const expectedReleaseBinary = candidatePaths.gpui_binary;
  if (resolve(gpuiBinary) !== expectedReleaseBinary) {
    throw new Error(
      `GPUI v4 binary must be the Cargo release artifact: ${expectedReleaseBinary}`,
    );
  }
  const expectedWorker = candidatePaths.pdf_worker;
  if (resolveRepositoryPath(gpui.pdf_worker?.path) !== expectedWorker) {
    throw new Error(
      "GPUI candidate manifest does not name the Cargo release PDF worker",
    );
  }
  const currentPdfium = await pdfiumCandidateEvidenceV4(candidatePaths, {
    testOnlyCandidateRoot,
  });
  if (canonicalSha256(currentPdfium) !== canonicalSha256(gpui.pdfium)) {
    throw new Error(
      "GPUI pinned PDFium library no longer matches its candidate manifest",
    );
  }
  const [
    electronExecutableArtifact,
    gpuiExecutableArtifact,
    pdfWorkerArtifact,
  ] = await Promise.all([
    validateManifestArtifact(electron.executable, "Electron executable", {
      executable: true,
    }),
    validateManifestArtifact(gpui.executable, "GPUI executable", {
      executable: true,
    }),
    validateManifestArtifact(gpui.pdf_worker, "GPUI PDF worker", {
      executable: true,
    }),
  ]);
  if (
    resolveRepositoryPath(electron.executable.path) !==
    resolve(electronExecutable)
  ) {
    throw new Error(
      "Electron executable does not match its optimized candidate manifest",
    );
  }
  if (resolveRepositoryPath(gpui.executable.path) !== resolve(gpuiBinary)) {
    throw new Error(
      "GPUI executable does not match its optimized candidate manifest",
    );
  }
  const currentBundle = await electronBundleEvidence(
    candidatePaths.electron_bundle_directory,
  );
  if (
    currentBundle.tree_sha256 !== electron.bundle?.tree_sha256 ||
    currentBundle.file_count !== electron.bundle?.file_count
  ) {
    throw new Error(
      "Electron production Vite bundle no longer matches its candidate manifest",
    );
  }
  const currentRuntimeClosure = await electronRuntimeClosureEvidenceV4(
    candidatePaths.electron_runtime_package_roots,
  );
  assertCompiledImportsAreSealedV4(currentBundle, currentRuntimeClosure);
  if (
    canonicalSha256(currentRuntimeClosure) !==
    canonicalSha256(electron.runtime_dependency_closure)
  ) {
    throw new Error(
      "Electron runtime dependency closure no longer matches its candidate manifest",
    );
  }
  const expectedDevProvenancePath = authorizedDesktopDevProvenancePathV4(
    candidatePaths.desktop_dev_provenance,
    { testOnlyCandidateRoot },
  );
  if (
    resolveRepositoryPath(
      electron.runtime_artifact_closure?.desktop_dev_provenance?.artifact?.path,
    ) !== expectedDevProvenancePath
  ) {
    throw new Error(
      "Electron candidate does not name the exact desktop dev provenance runtime artifact",
    );
  }
  const currentDevProvenance = await parseDesktopDevProvenanceV4(
    expectedDevProvenancePath,
  );
  const devProvenanceArtifact = await validateManifestArtifact(
    electron.runtime_artifact_closure.desktop_dev_provenance.artifact,
    "Electron desktop dev provenance",
  );
  const currentRuntimeArtifactClosure = runtimeArtifactClosureV4(
    currentRuntimeClosure,
    {
      artifact: devProvenanceArtifact,
      identity: currentDevProvenance.identity,
    },
  );
  if (
    canonicalSha256(currentRuntimeArtifactClosure) !==
    canonicalSha256(electron.runtime_artifact_closure)
  ) {
    throw new Error(
      "Electron runtime artifact closure no longer matches its candidate manifest",
    );
  }
  const manifestArtifacts = await Promise.all(
    [electronManifestPath, gpuiManifestPath].map(async (path) => {
      const metadata = await stat(path);
      return {
        path: resolve(path),
        bytes: metadata.size,
        sha256: await sha256File(path),
      };
    }),
  );
  return {
    electron: {
      ...manifestArtifacts[0],
      candidate_profile: electron.candidate_profile,
      runtime_profile: electron.runtime_profile,
      bundle_tree_sha256: electron.bundle.tree_sha256,
      runtime_dependency_closure_tree_sha256:
        electron.runtime_dependency_closure.tree_sha256,
      runtime_artifact_closure_tree_sha256:
        electron.runtime_artifact_closure.tree_sha256,
      desktop_dev_provenance: devProvenanceArtifact,
      executable: electronExecutableArtifact,
    },
    gpui: {
      ...manifestArtifacts[1],
      candidate_profile: gpui.candidate_profile,
      runtime_profile: gpui.runtime_profile,
      executable: gpuiExecutableArtifact,
      pdf_worker: pdfWorkerArtifact,
      pdfium: currentPdfium,
    },
  };
}

export async function revalidateOptimizedCandidateLaunchV4({
  launchId,
  ...candidateOptions
}) {
  if (typeof launchId !== "string" || launchId.length === 0) {
    throw new Error("candidate launch revalidation requires a launch ID");
  }
  const startedMonotonicMs = Number(process.hrtime.bigint()) / 1e6;
  const candidates = await validateOptimizedCandidatesV4(candidateOptions);
  const payload = {
    schema_version: 1,
    candidate_profile: optimizedCandidateProfileV4,
    launch_id: launchId,
    revalidated_immediately_before_launch: true,
    electron_manifest_sha256: candidates.electron.sha256,
    electron_executable_sha256: candidates.electron.executable.sha256,
    electron_bundle_tree_sha256: candidates.electron.bundle_tree_sha256,
    electron_runtime_dependency_closure_tree_sha256:
      candidates.electron.runtime_dependency_closure_tree_sha256,
    electron_runtime_artifact_closure_tree_sha256:
      candidates.electron.runtime_artifact_closure_tree_sha256,
    electron_desktop_dev_provenance_sha256:
      candidates.electron.desktop_dev_provenance.sha256,
    gpui_manifest_sha256: candidates.gpui.sha256,
    gpui_executable_sha256: candidates.gpui.executable.sha256,
    gpui_pdf_worker_sha256: candidates.gpui.pdf_worker.sha256,
    gpui_pdfium_library_sha256: candidates.gpui.pdfium.library.sha256,
    gpui_pdfium_pin_manifest_sha256: candidates.gpui.pdfium.pin_manifest.sha256,
    started_monotonic_ms: startedMonotonicMs,
    ended_monotonic_ms: Number(process.hrtime.bigint()) / 1e6,
  };
  return {
    ...payload,
    evidence_sha256: canonicalSha256(payload),
  };
}

function parseArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (
      option === "--output" ||
      option === "--electron" ||
      option === "--dev-provenance"
    ) {
      const value = argv[++index];
      if (!value || value.startsWith("--"))
        throw new Error(`${option} requires a value`);
      if (option === "--output") options.output = resolve(value);
      else if (option === "--electron") {
        options.electronExecutable = resolve(value);
      } else {
        options.devProvenancePath = resolve(value);
      }
    } else if (option === "--help" || option === "-h") {
      return { help: true };
    } else {
      throw new Error(`unknown option: ${option}`);
    }
  }
  if (!options.output) throw new Error("--output is required");
  if (!options.devProvenancePath) {
    throw new Error("--dev-provenance is required");
  }
  return options;
}

function usage() {
  return `Usage: node optimized-candidates-v4.mjs --output <directory> --dev-provenance <file> [--electron <executable>]

Builds production Vite bundles and Cargo release gallery/PDF-worker binaries.
It installs and seals the explicit desktop development provenance input, then
writes hash-verified candidate manifests for run-paired-v4.mjs. It does not
package, sign, install, publish, or modify an installed application.\n`;
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  const options = parseArguments(process.argv.slice(2));
  if (options.help) process.stdout.write(usage());
  else {
    const prepared = await prepareOptimizedCandidatesV4(options);
    process.stdout.write(
      `${JSON.stringify(
        {
          electron_candidate_artifact: prepared.electronManifestPath,
          gpui_candidate_artifact: prepared.gpuiManifestPath,
          electron_executable: resolveRepositoryPath(
            prepared.electron.executable.path,
          ),
          gpui_binary: resolveRepositoryPath(prepared.gpui.executable.path),
        },
        null,
        2,
      )}\n`,
    );
  }
}
