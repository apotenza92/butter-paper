import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  link,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { preparePdfiumProductionCandidate } from "../scripts/prepare-pdfium-production-candidate.mjs";

const scriptPath = fileURLToPath(new URL("../scripts/prepare-pdfium-production-candidate.mjs", import.meta.url));

async function fixture(t) {
  const base = await mkdtemp(join(tmpdir(), "pdfium-candidate-test-"));
  t.after(() => rm(base, { recursive: true, force: true }));
  const candidateRoot = join(base, "candidate");
  const notices = join(candidateRoot, "notices", "third_party", "example");
  await mkdir(notices, { recursive: true });
  await writeFile(join(candidateRoot, "libpdfium.so"), "binary fixture");
  await writeFile(join(candidateRoot, "gn-args.txt"), "is_debug = false\n");
  await writeFile(join(notices, "LICENSE"), "Example licence\n");
  const dependencyRevisions = join(base, "dependency-revisions.json");
  await writeFile(dependencyRevisions, JSON.stringify({
    "third_party/example": { url: "https://example.test/repo", rev: "abc123" },
    "third_party/url_only": { url: "https://example.test/url-only" },
  }));
  const inputs = {
    candidateRoot,
    target: "x86_64-unknown-linux-gnu",
    library: "libpdfium.so",
    dependencyRevisions,
    sharedLibraryPatch: join(base, "shared.patch"),
    dependencyPolicyPatch: join(base, "deps.patch"),
    pdfiumRevision: "91b9d569b34be4f38eed7b3c49b227356c3aadad",
    depotToolsRevision: "7575b8253e91eec32feb636c07fb515b234285da",
    repositoryRevision: "deadbeef",
    runnerImage: "ubuntu-24.04",
    runnerImageOS: "ubuntu24",
    runnerImageVersion: "20260920.1",
    runnerOS: "Linux",
    runnerArch: "X64",
    ninjaJobs: "2",
    buildCommand: "autoninja -C out/Production -j 2 pdfium",
  };
  await writeFile(inputs.sharedLibraryPatch, "shared patch evidence\n");
  await writeFile(inputs.dependencyPolicyPatch, "dependency policy evidence\n");
  for (const [key, value] of Object.entries({
    gnVersionFile: "2400",
    ninjaVersionFile: "1.12.1",
    sisoVersionFile: "siso 1",
    clangVersionFile: "clang 20",
  })) {
    inputs[key] = join(base, `${key}.txt`);
    await writeFile(inputs[key], `${value}\n`);
  }
  return { base, inputs };
}

test("writes pending metadata, pinned wrapper/source identity, provenance, notices and dependency SBOM", async (t) => {
  const { inputs } = await fixture(t);
  const manifest = await preparePdfiumProductionCandidate(inputs);
  assert.equal(manifest.schemaVersion, 1);
  assert.equal(manifest.purpose, "production-distribution");
  assert.equal(manifest.productionApproved, false);
  assert.deepEqual(manifest.wrapper, {
    package: "pdfium-render",
    version: "0.9.4",
    revision: "6cee8b9a3951832ac0ff62ce4c32800278001cb8",
    feature: "pdfium_7881",
  });
  assert.deepEqual(manifest.source, {
    repository: "https://pdfium.googlesource.com/pdfium",
    revision: inputs.pdfiumRevision,
  });
  assert.deepEqual(manifest.build.toolchain, {
    depot_tools: inputs.depotToolsRevision,
    runner: inputs.runnerImage,
    clang: "clang 20",
    gn: "2400",
    ninja: "1.12.1",
    siso: "siso 1",
  });
  assert.deepEqual(manifest.artifacts[0].notices.map(({ path }) => path), ["third_party/example/LICENSE"]);
  const sbom = JSON.parse(await readFile(join(inputs.candidateRoot, "sbom.json"), "utf8"));
  assert.equal(sbom.bomFormat, "CycloneDX");
  assert.equal(sbom.specVersion, "1.5");
  assert.deepEqual(sbom.components.map(({ name, version }) => [name, version]), [
    ["third_party/example", "abc123"],
    ["third_party/url_only", "identity-embedded-in-source-url"],
  ]);
  assert.deepEqual(sbom.components[1].properties.at(-1), {
    name: "source.revision.status",
    value: "review source URL identity",
  });
  assert.match(sbom.properties[0].value, /PENDING REVIEW/);
  const provenance = JSON.parse(await readFile(join(inputs.candidateRoot, "provenance.json"), "utf8"));
  assert.deepEqual(provenance.builder, {
    workflow: ".github/workflows/build-gpui-pdfium-production.yml",
    repositoryRevision: "deadbeef",
    runnerImage: "ubuntu-24.04",
    runnerImageOS: "ubuntu24",
    runnerImageVersion: "20260920.1",
    runnerOS: "Linux",
    runnerArch: "X64",
    host: provenance.builder.host,
  });
  assert.deepEqual(provenance.toolchain, {
    depotToolsRevision: inputs.depotToolsRevision,
    clang: "clang 20",
    gn: "2400",
    ninja: "1.12.1",
    siso: "siso 1",
  });
  assert.match(await readFile(join(inputs.candidateRoot, "reviews/redistribution-review.txt"), "utf8"), /UNREVIEWED PLACEHOLDER/);
  assert.match(await readFile(join(inputs.candidateRoot, "reviews/supplier-review.txt"), "utf8"), /UNREVIEWED PLACEHOLDER/);
  assert.equal(manifest.artifacts[0].library.sha256, createHash("sha256").update("binary fixture").digest("hex"));
  const manifestPath = join(inputs.candidateRoot, "production-pdfium-candidate.json");
  assert.equal((await lstat(manifestPath)).isFile(), true);
  const previousManifest = await readFile(manifestPath, "utf8");
  await assert.rejects(preparePdfiumProductionCandidate(inputs));
  assert.equal(await readFile(manifestPath, "utf8"), previousManifest);
});

test("records the macOS deployment target only for Apple targets", async (t) => {
  const { inputs } = await fixture(t);
  inputs.target = "aarch64-apple-darwin";
  inputs.library = "libpdfium.dylib";
  await rm(join(inputs.candidateRoot, "libpdfium.so"));
  await writeFile(join(inputs.candidateRoot, "libpdfium.dylib"), "binary fixture");
  const manifest = await preparePdfiumProductionCandidate(inputs);
  assert.equal(manifest.build.minimumSystemVersion, "13.0");
  assert.equal(manifest.artifacts[0].library.path, "libpdfium.dylib");
});

test("exposes the reusable metadata builder through explicit CLI inputs", async (t) => {
  const { inputs } = await fixture(t);
  const flags = {
    "--candidate-root": inputs.candidateRoot,
    "--target": inputs.target,
    "--library": inputs.library,
    "--dependency-revisions": inputs.dependencyRevisions,
    "--shared-library-patch": inputs.sharedLibraryPatch,
    "--dependency-policy-patch": inputs.dependencyPolicyPatch,
    "--gn-version-file": inputs.gnVersionFile,
    "--ninja-version-file": inputs.ninjaVersionFile,
    "--siso-version-file": inputs.sisoVersionFile,
    "--clang-version-file": inputs.clangVersionFile,
    "--pdfium-revision": inputs.pdfiumRevision,
    "--depot-tools-revision": inputs.depotToolsRevision,
    "--repository-revision": inputs.repositoryRevision,
    "--runner-image": inputs.runnerImage,
    "--runner-image-os": inputs.runnerImageOS,
    "--runner-image-version": inputs.runnerImageVersion,
    "--runner-os": inputs.runnerOS,
    "--runner-arch": inputs.runnerArch,
    "--ninja-jobs": inputs.ninjaJobs,
    "--build-command": inputs.buildCommand,
  };
  const result = spawnSync(process.execPath, [scriptPath, ...Object.entries(flags).flat()], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(await readFile(join(inputs.candidateRoot, "production-pdfium-candidate.json"), "utf8")).productionApproved, false);
});

test("rejects extra inputs, unsafe paths, malformed dependencies, and pre-existing output", async (t) => {
  const { inputs } = await fixture(t);
  const extra = join(inputs.candidateRoot, "unexpected.txt");
  await writeFile(extra, "not allowed");
  await assert.rejects(preparePdfiumProductionCandidate(inputs), /missing or extra inputs/);
  await rm(extra);
  inputs.library = "../libpdfium.so";
  await assert.rejects(preparePdfiumProductionCandidate(inputs), /safe relative path/);
  inputs.library = "libpdfium.so";
  await writeFile(inputs.dependencyRevisions, JSON.stringify({ dep: { url: "" } }));
  await assert.rejects(preparePdfiumProductionCandidate(inputs), /dependency identity is malformed/);
  await writeFile(extra, "sentinel");
  assert.equal(await readFile(extra, "utf8"), "sentinel");
});

test("rejects symlinked and hard-linked notice evidence", async (t) => {
  const symlinkFixture = await fixture(t);
  const notice = join(symlinkFixture.inputs.candidateRoot, "notices/third_party/example/LICENSE");
  await rm(notice);
  await symlink(symlinkFixture.inputs.gnVersionFile, notice);
  await assert.rejects(preparePdfiumProductionCandidate(symlinkFixture.inputs), /symlink/);

  const hardlinkFixture = await fixture(t);
  const hardlinkNotice = join(hardlinkFixture.inputs.candidateRoot, "notices/third_party/example/LICENSE");
  await rm(hardlinkNotice);
  await link(join(hardlinkFixture.inputs.candidateRoot, "gn-args.txt"), hardlinkNotice);
  await assert.rejects(preparePdfiumProductionCandidate(hardlinkFixture.inputs), /single-link/);
});
