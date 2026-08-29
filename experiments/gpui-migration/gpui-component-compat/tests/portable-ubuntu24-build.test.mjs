import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const base = new URL("../", import.meta.url);
const policyUrl = new URL("portable/ubuntu24-build-policy.json", base);
const containerfileUrl = new URL("portable/Containerfile.ubuntu24", base);
const runnerUrl = new URL("scripts/build-portable-ubuntu24.sh", base);

const packages = [
  "build-essential",
  "ca-certificates",
  "clang",
  "git",
  "libfontconfig-dev",
  "libvulkan-dev",
  "libx11-dev",
  "libxcb1-dev",
  "libxcb-xkb-dev",
  "libxkbcommon-dev",
  "libxkbcommon-x11-dev",
  "poppler-utils",
  "pkg-config",
  "qpdf",
  "xz-utils",
];

test("Ubuntu 24 portable inputs are immutable and package-minimal", async () => {
  const policy = JSON.parse(await readFile(policyUrl, "utf8"));
  assert.equal(
    policy.baseImage.reference,
    "ubuntu@sha256:1e0a86e57d247923571b75e0aaf48a1449cf8c543d51fb3e07a4a7d7bfa79316",
  );
  assert.equal(
    policy.baseImage.indexDigest,
    "sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517",
  );
  assert.equal(policy.snapshot.timestamp, "20260820T000000Z");
  assert.equal(
    policy.snapshot.url,
    "https://snapshot.ubuntu.com/ubuntu/20260820T000000Z",
  );
  assert.deepEqual(policy.snapshot.inReleaseSha256, {
    noble: "cdb2f31d809f589719a53c6ad15f255b27569c4059542ada282aaa21b8e164b0",
    "noble-backports": "835d68e98bf884be2dda56da61d8f17b9ac026dd2165ee12acb8719a365cfcea",
    "noble-security": "dd26a7efe93a3a3e3125c97e9f27f18a0fc357589f9b346dba72b158bd7e1e41",
    "noble-updates": "79d2a1c90ce4f14c98867053190c64a9018ac993702fe5146081873f3da526bf",
  });
  assert.deepEqual(policy.packages, packages);
  assert.deepEqual(policy.testUtilities, {
    qpdf: {
      version: "11.9.0-1.1ubuntu0.1",
      runtimeArchive: false,
    },
    "poppler-utils": {
      version: "24.02.0-1ubuntu9.9",
      runtimeArchive: false,
    },
  });
  assert.deepEqual(policy.snapshot.caCertificatesBootstrap, {
    filename: "pool/main/c/ca-certificates/ca-certificates_20240203_all.deb",
    sha256: "641de77d8f142cfd62a1a6f964ba67b20754d3337c480efb529d086075a06c9a",
  });
  assert.equal(policy.rust.version, "1.97.1");
});

test("container recipe verifies the signed snapshot and installs only the allowlist", async () => {
  const recipe = await readFile(containerfileUrl, "utf8");
  assert.match(recipe, /^FROM ubuntu@sha256:[0-9a-f]{64}$/m);
  assert.match(recipe, /signed-by=\/usr\/share\/keyrings\/ubuntu-archive-keyring\.gpg/);
  assert.match(recipe, /snapshot\.ubuntu\.com\/ubuntu\/20260820T000000Z/);
  assert.match(recipe, /^COPY ca-certificates_20240203_all\.deb \/tmp\/$/m);
  for (const packageName of packages) {
    assert.match(recipe, new RegExp(`\\b${packageName.replaceAll("+", "\\+")}\\b`));
  }
  assert.doesNotMatch(recipe, /ubuntu:(?:latest|24\.04)\b/);
  assert.doesNotMatch(recipe, /trusted=yes|--allow-unauthenticated/);
});

test("portable plan fixes owned paths, isolation, resource guards, and cleanup", () => {
  const result = spawnSync(fileURLToPath(runnerUrl), ["--print-plan"], {
    cwd: fileURLToPath(new URL("..", base)),
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  const plan = JSON.parse(result.stdout);
  assert.equal(plan.targetRelative, ".build-targets/gpui-component-portable-u24");
  assert.equal(plan.sourceReadOnly, true);
  assert.equal(plan.rustReadOnly, true);
  assert.equal(plan.cargoOffline, true);
  assert.equal(plan.network, "none");
  assert.equal(plan.readOnlyRoot, true);
  assert.equal(plan.capDrop, "ALL");
  assert.equal(plan.noNewPrivileges, true);
  assert.deepEqual(plan.resources, {
    memory: "3g",
    memorySwap: "3g",
    cpus: "2",
    pids: 512,
    tmpfs: "1g",
  });
  assert.deepEqual(plan.diskKiB, {
    preflight: 30 * 1024 * 1024,
    runtimeStop: 20 * 1024 * 1024,
    absoluteMin: 18 * 1024 * 1024,
    targetMax: 5 * 1024 * 1024,
  });
  assert.equal(plan.cargoJobs, 1);
  assert.equal(plan.incremental, 0);
  assert.equal(plan.compileTimeoutSeconds, 45 * 60);
  assert.deepEqual(plan.cleanup, {
    ordinaryFailure: "retain",
    diskSafety: "remove-owned-target-only",
  });
  assert.deepEqual(plan.preparedOverlay, {
    uniquePerRun: true,
    seededFromFrozenSource: true,
    writable: true,
    immutableSourceRemainsReadOnly: true,
    priorFailureEvidencePreserved: true,
  });
  assert.deepEqual(plan.testUtilities, {
    qpdf: "11.9.0-1.1ubuntu0.1",
    "poppler-utils": "24.02.0-1ubuntu9.9",
    runtimeArchive: false,
  });
  assert.deepEqual(plan.licenseReceipts.fontconfig, {
    package: "libfontconfig-dev",
    sourcePackage: "fontconfig",
    version: "2.15.0-1.1ubuntu2",
    path: "/usr/share/doc/libfontconfig-dev/copyright",
    sha256: "b215a61cdd3e62b5b17cc28b1852c78acb3dd38be0fb30706f7efc050dba91db",
    provenance: "https://snapshot.ubuntu.com/ubuntu/20260820T000000Z",
  });
});

test("portable runner rejects shell-shaped arguments without executing them", () => {
  const result = spawnSync(fileURLToPath(runnerUrl), ["--print-plan;touch", "/tmp/no"], {
    cwd: fileURLToPath(new URL("..", base)),
    encoding: "utf8",
  });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /usage:/i);
});

test("portable worker build enables only the reviewed PDFium worker feature", async () => {
  const runner = await readFile(runnerUrl, "utf8");
  assert.match(
    runner,
    /cargo build --manifest-path \.\.\/gpui-gallery\/Cargo\.toml --locked --offline --no-default-features --features pdfium-worker --bin butter-paper-pdf-worker/,
  );
});

test("target sampler returns one integer when du prints then fails", () => {
  const result = spawnSync(fileURLToPath(runnerUrl), ["--test-target-kib-du-failure"], {
    cwd: fileURLToPath(new URL("..", base)),
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, "2409044\n");
});

test("portable run binds a unique reflink-seeded prepared overlay", async () => {
  const runner = await readFile(runnerUrl, "utf8");
  assert.match(runner, /run_id=/);
  assert.match(runner, /cp --reflink=auto -a .*source_snapshot.*\.prepared/);
  assert.match(
    runner,
    /src=\$prepared_overlay,dst=\/source\/gpui-component-compat\/\.prepared"/,
  );
  assert.doesNotMatch(
    runner,
    /src=\$real_worker_state,dst=\/source\/gpui-component-compat\/\.prepared\/real-document-spine-surfaces/,
  );
});

test("owned overlay cleanup accepts only the audited successful layouts", () => {
  for (const fixture of ["expected", "absent-surface"]) {
    const result = spawnSync(
      fileURLToPath(runnerUrl),
      ["--test-owned-overlay-cleanup", fixture],
      {
        cwd: fileURLToPath(new URL("..", base)),
        encoding: "utf8",
      },
    );
    assert.equal(result.status, 0, `${fixture}: ${result.stderr}`);
    assert.equal(result.stdout, "accepted sibling-preserved receipt-written\n");
  }
});

test("owned overlay cleanup rejects every unsafe audited layout", () => {
  const unsafeFixtures = [
    "payload",
    "symlink",
    "device",
    "deeper",
    "extra",
    "mismatched",
    "nonnum",
    "duplicate",
  ];
  for (const fixture of unsafeFixtures) {
    const result = spawnSync(
      fileURLToPath(runnerUrl),
      ["--test-owned-overlay-cleanup", fixture],
      {
        cwd: fileURLToPath(new URL("..", base)),
        encoding: "utf8",
      },
    );
    assert.equal(result.status, 0, `${fixture}: ${result.stderr}`);
    assert.equal(result.stdout, "rejected sibling-preserved\n");
  }
});

test("source snapshot deletion requires its exact canonical owner sentinel", () => {
  for (const fixture of ["wrong", "malformed", "symlink-sentinel"]) {
    const result = spawnSync(
      fileURLToPath(runnerUrl),
      ["--test-source-snapshot-deletion", fixture],
      {
        cwd: fileURLToPath(new URL("..", base)),
        encoding: "utf8",
      },
    );
    assert.equal(result.status, 0, `${fixture}: ${result.stderr}`);
    assert.equal(result.stdout, "rejected source-preserved sibling-preserved\n");
  }
  const accepted = spawnSync(
    fileURLToPath(runnerUrl),
    ["--test-source-snapshot-deletion", "canonical"],
    {
      cwd: fileURLToPath(new URL("..", base)),
      encoding: "utf8",
    },
  );
  assert.equal(accepted.status, 0, accepted.stderr);
  assert.equal(accepted.stdout, "accepted source-removed sibling-preserved\n");
});
