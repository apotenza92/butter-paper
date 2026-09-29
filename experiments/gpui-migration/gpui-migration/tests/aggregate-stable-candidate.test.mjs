import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  readFile,
  rm,
  writeFile,
  symlink,
  link,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, test } from "node:test";
import { aggregateStableCandidate } from "../scripts/aggregate-stable-candidate.mjs";

const REVISION = "0123456789abcdef0123456789abcdef01234567";
const REQUIRED = [
  "macos-arm64",
  "macos-x64",
  "windows-arm64",
  "windows-x64",
  "linux-arm64",
  "linux-x64",
];
const roots = [];

async function fixture(targets = REQUIRED) {
  const root = await mkdtemp(join(tmpdir(), "stable-candidate-"));
  roots.push(root);
  const candidate = {
    schema: "butter-paper/stable-candidate-input",
    schemaVersion: 2,
    channel: "stable",
    version: "1.2.3",
    sourceRevision: REVISION,
    targets: [],
  };
  for (const target of targets) {
    const slug = target.replaceAll("-", "_");
    const artifact = `packages/${slug}.pkg`;
    const packageManifest = `receipts/${slug}.package.json`;
    const verificationReceipt = `receipts/${slug}.verified.json`;
    const runtimeEvidence = `runtime/${slug}.smoke.json`;
    const bytes = Buffer.from(`package bytes for ${target}\n`);
    const artifactClaim = {
      path: artifact,
      bytes: bytes.length,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    };
    const identity = {
      target,
      channel: "stable",
      version: "1.2.3",
      sourceRevision: REVISION,
    };
    await mkdir(join(root, "packages"), { recursive: true });
    await mkdir(join(root, "receipts"), { recursive: true });
    await mkdir(join(root, "runtime"), { recursive: true });
    await writeFile(join(root, artifact), bytes);
    await writeFile(
      join(root, packageManifest),
      JSON.stringify({
        schema: "butter-paper/package-manifest",
        schemaVersion: 1,
        ...identity,
        artifact: artifactClaim,
      }),
    );
    await writeFile(
      join(root, verificationReceipt),
      JSON.stringify({
        schema: "butter-paper/package-verification",
        schemaVersion: 1,
        ...identity,
        verified: true,
        artifact: artifactClaim,
      }),
    );
    await writeFile(
      join(root, runtimeEvidence),
      JSON.stringify({
        schema: "butter-paper/nonmac-runtime-smoke",
        schemaVersion: 1,
        passed: true,
        packageIdentity: { ...identity, archiveSha256: artifactClaim.sha256 },
        documentOpenEvidence: { documentId: "a".repeat(32) },
        cleanup: { status: "verified-clean", tempRootRemoved: true },
      }),
    );
    candidate.targets.push({
      target,
      artifact,
      packageManifest,
      verificationReceipt,
      runtimeEvidence,
    });
  }
  await writeFile(
    join(root, "candidate.json"),
    `${JSON.stringify(candidate, null, 2)}\n`,
  );
  return { root, candidate };
}

async function outputFor(root, name = "manifest.json") {
  const outputPath = join(
    root,
    "..",
    `${name}-${Math.random().toString(16).slice(2)}.json`,
  );
  return aggregateStableCandidate({ inputDir: root, outputPath });
}

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

test("aggregates exact required six-platform coverage deterministically", async () => {
  const { root } = await fixture(REQUIRED);
  const first = await outputFor(root, "first");
  const second = await outputFor(root, "second");
  assert.deepEqual(first.manifest.requiredTargets, [...REQUIRED].sort());
  assert.deepEqual(first.manifest.optionalTargets, []);
  assert.deepEqual(
    first.manifest.targets.map(({ target }) => target),
    [...REQUIRED].sort(),
  );
  assert.equal(first.sha256, second.sha256);
  assert.deepEqual(
    await readFile(first.outputPath),
    await readFile(second.outputPath),
  );
  assert.equal(
    (await readFile(first.outputPath)).toString(),
    `${JSON.stringify(first.manifest, null, 2)}\n`,
  );
  assert.equal(
    (await readFile(first.checksumPath, "utf8")).trim(),
    `${first.sha256}  ${first.outputPath.split("/").at(-1)}`,
  );
});

test("rejects missing, duplicate, and unexpected targets", async (t) => {
  await t.test("missing required target", async () => {
    const { root } = await fixture(REQUIRED.slice(1));
    await assert.rejects(
      outputFor(root),
      /missing required target: macos-arm64/,
    );
  });
  await t.test("macOS x64 cannot stand in for macOS arm64", async () => {
    const { root } = await fixture(REQUIRED.slice(1));
    await assert.rejects(
      outputFor(root),
      /missing required target: macos-arm64/,
    );
  });
  await t.test("duplicate target", async () => {
    const { root, candidate } = await fixture();
    candidate.targets.push({ ...candidate.targets[0] });
    await writeFile(join(root, "candidate.json"), JSON.stringify(candidate));
    await assert.rejects(outputFor(root), /duplicate target: macos-arm64/);
  });
  await t.test("unexpected target", async () => {
    const { root, candidate } = await fixture();
    candidate.targets[0].target = "linux-riscv64";
    await writeFile(join(root, "candidate.json"), JSON.stringify(candidate));
    await assert.rejects(outputFor(root), /unexpected target: linux-riscv64/);
  });
});

test("rejects mixed identities and artifact tampering", async (t) => {
  await t.test("mixed package revision", async () => {
    const { root, candidate } = await fixture();
    const manifest = join(root, candidate.targets[0].packageManifest);
    const value = JSON.parse(await readFile(manifest, "utf8"));
    value.sourceRevision = "fedcba9876543210fedcba9876543210fedcba98";
    await writeFile(manifest, JSON.stringify(value));
    await assert.rejects(outputFor(root), /identity does not match/);
  });
  await t.test("mixed version in verification receipt", async () => {
    const { root, candidate } = await fixture();
    const receipt = join(root, candidate.targets[0].verificationReceipt);
    const value = JSON.parse(await readFile(receipt, "utf8"));
    value.version = "1.2.4";
    await writeFile(receipt, JSON.stringify(value));
    await assert.rejects(outputFor(root), /identity does not match/);
  });
  await t.test("altered package bytes", async () => {
    const { root, candidate } = await fixture();
    await writeFile(join(root, candidate.targets[0].artifact), "altered bytes");
    await assert.rejects(outputFor(root), /artifact claim does not match/);
  });
  await t.test("runtime evidence from another package", async () => {
    const { root, candidate } = await fixture();
    const evidence = join(root, candidate.targets[0].runtimeEvidence);
    const value = JSON.parse(await readFile(evidence, "utf8"));
    value.packageIdentity.archiveSha256 = "f".repeat(64);
    await writeFile(evidence, JSON.stringify(value));
    await assert.rejects(outputFor(root), /clean exact-package runtime smoke/);
  });
});

test("fails closed on development markers, unverified receipts, and unexpected input files", async (t) => {
  await t.test("development PDFium marker", async () => {
    const { root, candidate } = await fixture();
    const receipt = join(root, candidate.targets[0].verificationReceipt);
    const value = JSON.parse(await readFile(receipt, "utf8"));
    value.notes = "development-pdfium override";
    await writeFile(receipt, JSON.stringify(value));
    await assert.rejects(
      outputFor(root),
      /development PDFium or override marker/,
    );
  });
  await t.test("PDFium override marker", async () => {
    const { root, candidate } = await fixture();
    const receipt = join(root, candidate.targets[0].verificationReceipt);
    const value = JSON.parse(await readFile(receipt, "utf8"));
    value.notes = "PDFium override input";
    await writeFile(receipt, JSON.stringify(value));
    await assert.rejects(
      outputFor(root),
      /development PDFium or override marker/,
    );
  });
  await t.test("ordinary PDFium GenerateObjectOverrides symbol", async () => {
    const { root, candidate } = await fixture();
    const record = candidate.targets.find(({ target }) => target === "linux-arm64");
    const bytes = Buffer.from(
      "_ZN18CPDF_FontSubsetter23GenerateObjectOverridesEN6pdfium4span",
    );
    const claim = {
      path: record.artifact,
      bytes: bytes.length,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    };
    await writeFile(join(root, record.artifact), bytes);
    for (const path of [record.packageManifest, record.verificationReceipt]) {
      const value = JSON.parse(await readFile(join(root, path), "utf8"));
      value.artifact = claim;
      await writeFile(join(root, path), JSON.stringify(value));
    }
    const runtimePath = join(root, record.runtimeEvidence);
    const runtime = JSON.parse(await readFile(runtimePath, "utf8"));
    runtime.packageIdentity.archiveSha256 = claim.sha256;
    await writeFile(runtimePath, JSON.stringify(runtime));
    const result = await outputFor(root);
    assert.equal(result.manifest.targets.find(({ target }) => target === "linux-arm64").artifact.sha256, claim.sha256);
  });
  await t.test("unverified receipt", async () => {
    const { root, candidate } = await fixture();
    const receipt = join(root, candidate.targets[0].verificationReceipt);
    const value = JSON.parse(await readFile(receipt, "utf8"));
    value.verified = false;
    await writeFile(receipt, JSON.stringify(value));
    await assert.rejects(outputFor(root), /not an approved/);
  });
  await t.test("unexpected file", async () => {
    const { root } = await fixture();
    await writeFile(join(root, "stray.bin"), "extra");
    await assert.rejects(outputFor(root), /unexpected file: stray.bin/);
  });
  await t.test("failed or incompletely cleaned runtime smoke", async () => {
    const { root, candidate } = await fixture();
    const evidence = join(root, candidate.targets[0].runtimeEvidence);
    const value = JSON.parse(await readFile(evidence, "utf8"));
    value.passed = false;
    value.cleanup.status = "unknown-or-failed";
    await writeFile(evidence, JSON.stringify(value));
    await assert.rejects(outputFor(root), /clean exact-package runtime smoke/);
  });
});

test("rejects symbolic links and hard links in the input tree", async (t) => {
  await t.test("symbolic link", async () => {
    const { root, candidate } = await fixture();
    const artifact = join(root, candidate.targets[0].artifact);
    const replacement = `${artifact}.real`;
    await writeFile(replacement, await readFile(artifact));
    await rm(artifact);
    await symlink(replacement, artifact);
    await assert.rejects(outputFor(root), /symbolic link/);
  });
  await t.test("hard link", async () => {
    const { root, candidate } = await fixture();
    const artifact = join(root, candidate.targets[0].artifact);
    await link(artifact, `${artifact}.alias`);
    await assert.rejects(outputFor(root), /hard-linked file/);
  });
});
