#!/usr/bin/env node

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmod,
  copyFile,
  cp,
  lstat,
  mkdir,
  readFile,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import {
  buildBundleManifest,
  validateArchivePaths,
  validateBundleDirectory,
} from "./ubuntu24-gpu-qualification-bundle.mjs";

const execFileAsync = promisify(execFile);
const performanceDirectory = path.dirname(fileURLToPath(import.meta.url));
const repositoryDirectory = path.resolve(performanceDirectory, "../../..");
const bundleName = "bp-ubuntu24-gpu-qualification-v1";
const oldCandidate = path.resolve(
  repositoryDirectory,
  "test-results/gpui-component-gpu-candidate-20260828T022500Z",
);
const portableCandidateDirectory = path.resolve(
  repositoryDirectory,
  "test-results/portable-ubuntu24-candidate-provenance-20260828T0649Z",
);
const electronCandidatePath = path.resolve(
  repositoryDirectory,
  "test-results/gpui-v6-candidates-gpu-native-20260824/electron-optimized-candidate-v4.json",
);

async function exists(file) {
  try {
    await stat(file);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

async function copyRegular(source, destination, mode = null) {
  const sourceMetadata = await lstat(source);
  if (!sourceMetadata.isFile() || sourceMetadata.isSymbolicLink()) {
    throw new Error(`source is not a regular file: ${source}`);
  }
  await mkdir(path.dirname(destination), { recursive: true });
  await copyFile(source, destination);
  await chmod(
    destination,
    mode ?? (sourceMetadata.mode & 0o111 ? 0o755 : 0o644),
  );
}

async function copyArtifactRecord(record, root, { executable = false } = {}) {
  const source = path.resolve(repositoryDirectory, record.path);
  const metadata = await stat(source);
  if (
    metadata.size !== record.bytes ||
    (await sha256(source)) !== record.sha256
  ) {
    throw new Error(`candidate artifact changed: ${record.path}`);
  }
  await copyRegular(
    source,
    destinationForRepositoryPath(root, record.path),
    executable ? 0o755 : null,
  );
}

async function copyTree(source, destination) {
  await cp(source, destination, {
    recursive: true,
    dereference: true,
    filter: (candidate) => !candidate.includes("/results/"),
  });
}

function destinationForRepositoryPath(root, relativePath) {
  validateArchivePaths([relativePath]);
  return path.join(root, ...relativePath.split("/"));
}

async function copyManifestArtifacts(manifest, root) {
  const records = [
    manifest.executable,
    ...manifest.bundle.files,
    ...manifest.runtime_dependency_closure.packages.flatMap(
      ({ files }) => files,
    ),
    manifest.runtime_artifact_closure.desktop_dev_provenance.artifact,
  ];
  const observed = new Set();
  for (const record of records) {
    if (observed.has(record.path)) continue;
    observed.add(record.path);
    await copyArtifactRecord(record, root, {
      executable: record.path === manifest.executable.path,
    });
  }
}

async function copyComponentManifestArtifacts(manifest, root) {
  const executablePaths = new Set([
    manifest.runtime.binary.path,
    manifest.runtime.worker.path,
  ]);
  const records = [
    manifest.runtime.binary,
    manifest.runtime.worker,
    manifest.runtime.pdfium,
    manifest.source.cargoLock,
    manifest.source.rustToolchain,
    manifest.source.preparationPolicy,
    ...manifest.source.compatFiles,
    manifest.build,
  ];
  for (const record of records) {
    await copyArtifactRecord(record, root, {
      executable: executablePaths.has(record.path),
    });
  }
}

async function sha256(file) {
  return createHash("sha256")
    .update(await readFile(file))
    .digest("hex");
}

const entrypoint = `#!/usr/bin/env sh
set -eu
bundle_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
exec "$bundle_root/runtime/node" "$bundle_root/scripts/qualification-entrypoint.mjs" "$@"
`;

const qualificationEntrypoint = `#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseEntrypointArguments, validateBundleDirectory } from "./ubuntu24-gpu-qualification-bundle.mjs";
import { runComponentShortQualification } from "../experiments/gpui-migration/performance/component-short-qualification.mjs";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDirectory, "..");
const options = parseEntrypointArguments(process.argv.slice(2));
const manifest = JSON.parse(await readFile(path.join(root, "BUNDLE-MANIFEST.json"), "utf8"));
await validateBundleDirectory(root, manifest);
const result = await runComponentShortQualification({ root, output: options.output });
process.stdout.write(JSON.stringify(result, null, 2) + "\\n");
if (result.status !== "PASSED") process.exitCode = 2;
`;

const readme = `# Ubuntu 24 GPUI GPU qualification bundle

This is an experiment-only, unpackaged x86_64 Ubuntu 24 runtime closure. It
does not modify, install, or publish Butter Paper. The archive includes the
accepted portable component candidate, the frozen Electron candidate closure,
the reviewed V6 short-pair scripts, fixtures, an Ubuntu-24-compatible Node
runtime, and deterministic SHA-256 file seals.

The old Ubuntu 26 archive was not portable. Its component_story required
GLIBC_2.43 and it omitted Electron and Node dependencies. This bundle replaces
that story binary with SHA-256
\`c1f28ef31f3f6da6ce8373d7e78edca34abef15212fbee7c51504b4cb382e26a\`
(maximum GLIBC_2.39), retains the accepted worker
\`b441c721a01fcfb137289ff86b61b6f3bb9cd3338523178b355d91af3528cb6e\`
(maximum GLIBC_2.34), and retains PDFium
\`f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64\`
(maximum GLIBC_2.16). Electron 43's retained ELF objects require at most
GLIBC_2.25. The self-contained bundled Node executable requires at most
GLIBC_2.28; the Ubuntu 26 system Node launcher and its GLIBC_2.43 libnode are
explicitly excluded.

Run the only supported entrypoint after verifying the detached archive hash:

\`\`\`sh
./RUN-QUALIFICATION.sh --output /absolute/fresh/output
\`\`\`

The entrypoint validates every sealed file and rejects extra files, symlinks,
changed modes, path traversal, control characters, and shell metacharacters in
the output argument. It authenticates the exact Electron and component
candidate closures, then checks the GPU desktop and cgroup prerequisites before
starting a process. Missing prerequisites produce a retained zero-launch
BLOCKED receipt.

On a ready host, the additive adapter runs one Electron open-pdf launch and one
GPUI Component open-pdf launch in that order. The GPUI argv includes the
authoritative reviewed profile \`--compat-profile
longbridge-gpui-component-v1\`. Both runners request the existing V6 XDamage
observation, GPU adapter evidence, and matched view state. The result remains
development-only with \`timing_eligible: false\` and \`v6_acceptance: false\`.
It is not an authenticated V6 qualification receipt and cannot unlock the
frozen 624-launch executor.

The host still supplies the NVIDIA driver, X11 desktop, D-Bus session, cgroup
v2 delegation, Vulkan tools, xdotool, ImageMagick, a C compiler, and X11
development libraries used by the reviewed observers. No provider credentials,
provisioning logic, or package-fetch command is included.
`;

async function assemble(outputDirectory, portableNodePath) {
  if (
    !path
      .resolve(outputDirectory)
      .startsWith(path.resolve(repositoryDirectory, "test-results") + path.sep)
  ) {
    throw new Error("output must be below test-results");
  }
  if (!(await exists(oldCandidate)))
    throw new Error("old candidate extraction is missing");
  const staging = path.join(outputDirectory, bundleName);
  await rm(outputDirectory, { recursive: true, force: true });
  await mkdir(staging, { recursive: true });

  await copyTree(path.join(oldCandidate, "runner-root"), staging);
  await copyTree(
    path.join(oldCandidate, "fixtures"),
    path.join(staging, "fixtures"),
  );
  await copyTree(
    path.join(oldCandidate, "fixtures/public-fixtures-v1"),
    path.join(
      staging,
      "experiments/gpui-migration/performance/results/public-fixtures-v1",
    ),
  );
  await copyTree(
    path.join(oldCandidate, "runtime/pdfium-development/licenses"),
    path.join(staging, "runtime/pdfium-development/licenses"),
  );

  const electronCandidate = JSON.parse(
    await readFile(electronCandidatePath, "utf8"),
  );
  const componentCandidatePath = path.join(
    portableCandidateDirectory,
    "candidate.json",
  );
  const componentCandidate = JSON.parse(
    await readFile(componentCandidatePath, "utf8"),
  );
  await copyManifestArtifacts(electronCandidate, staging);
  await copyComponentManifestArtifacts(componentCandidate, staging);
  await copyRegular(
    path.resolve(repositoryDirectory, "apps/desktop/package.json"),
    path.join(staging, "apps/desktop/package.json"),
  );
  await copyRegular(
    electronCandidatePath,
    path.join(staging, "candidates/electron-optimized-candidate-v4.json"),
  );
  await copyRegular(
    componentCandidatePath,
    path.join(staging, "candidates/component-development-candidate.json"),
  );
  await copyRegular(
    portableNodePath,
    path.join(staging, "runtime/node"),
    0o755,
  );

  for (const relativePath of [
    "experiments/gpui-migration/.build-targets/gpui-component-portable-u24/debug/component_story",
    "experiments/gpui-migration/.build-targets/gpui-component-portable-u24/debug/butter-paper-pdf-worker",
    "experiments/gpui-migration/.build-targets/gpui-component-portable-u24/portable-ubuntu24-build-summary.json",
    "experiments/gpui-migration/.build-targets/gpui-component-portable-u24/portable-ubuntu24-receipt.txt",
    "experiments/gpui-migration/gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
  ]) {
    await copyRegular(
      path.resolve(repositoryDirectory, relativePath),
      destinationForRepositoryPath(staging, relativePath),
    );
  }

  await copyTree(
    path.resolve(repositoryDirectory, "node_modules/electron/dist"),
    path.join(staging, "node_modules/electron/dist"),
  );
  await copyRegular(
    path.join(performanceDirectory, "ubuntu24-gpu-qualification-bundle.mjs"),
    path.join(staging, "scripts/ubuntu24-gpu-qualification-bundle.mjs"),
    0o755,
  );
  await copyRegular(
    path.join(performanceDirectory, "component-short-qualification.mjs"),
    path.join(
      staging,
      "experiments/gpui-migration/performance/component-short-qualification.mjs",
    ),
    0o755,
  );
  await writeFile(
    path.join(staging, "scripts/qualification-entrypoint.mjs"),
    qualificationEntrypoint,
    { mode: 0o755 },
  );
  await writeFile(path.join(staging, "RUN-QUALIFICATION.sh"), entrypoint, {
    mode: 0o755,
  });
  await writeFile(path.join(staging, "README.md"), readme, { mode: 0o644 });

  const manifest = await buildBundleManifest(staging, {
    entrypoint: "RUN-QUALIFICATION.sh",
    classification: "runnable-development-qualification",
    excludedPaths: ["BUNDLE-MANIFEST.json"],
  });
  await writeFile(
    path.join(staging, "BUNDLE-MANIFEST.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
    { mode: 0o644 },
  );
  await validateBundleDirectory(staging, manifest);
  return { staging, manifest, electronCandidate };
}

async function archive(outputDirectory, staging) {
  const archivePath = `${outputDirectory}.tar.zst`;
  await rm(archivePath, { force: true });
  await rm(`${archivePath}.sha256`, { force: true });
  const shell = `set -o pipefail; tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner --format=posix --pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime -cf - -- "$1" | zstd -19 -T1 -q -o "$2"`;
  await execFileAsync("bash", ["-c", shell, "bash", bundleName, archivePath], {
    cwd: path.dirname(staging),
    maxBuffer: 4 * 1024 * 1024,
  });
  const { stdout } = await execFileAsync(
    "tar",
    ["--zstd", "-tf", archivePath],
    { maxBuffer: 32 * 1024 * 1024 },
  );
  const listed = stdout.trim().split("\n").filter(Boolean);
  validateArchivePaths(
    listed.map((entry) => (entry.endsWith("/") ? entry.slice(0, -1) : entry)),
  );
  const digest = await sha256(archivePath);
  await writeFile(
    `${archivePath}.sha256`,
    `${digest}  ${path.basename(archivePath)}\n`,
    { mode: 0o644 },
  );
  return { archivePath, digest, archive_entries: listed.length };
}

async function main() {
  if (
    process.argv.length !== 6 ||
    process.argv[2] !== "--output" ||
    process.argv[4] !== "--node"
  ) {
    throw new Error(
      "usage: node build-ubuntu24-gpu-qualification-bundle.mjs --output test-results/<name> --node <self-contained-ubuntu24-compatible-node>",
    );
  }
  const outputDirectory = path.resolve(process.argv[3]);
  const portableNodePath = path.resolve(process.argv[5]);
  const assembled = await assemble(outputDirectory, portableNodePath);
  const archived = await archive(outputDirectory, assembled.staging);
  process.stdout.write(
    `${JSON.stringify({ status: "prepared", classification: "runnable-development-qualification", file_count: assembled.manifest.files.length, ...archived }, null, 2)}\n`,
  );
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1])
)
  await main();
