import test from "node:test";
import assert from "node:assert/strict";
import {
  isolatedAppEnvironment,
  validateDisposableMacosRunner,
  validateExtractedInventory,
  validateFixturePdf,
  validatePackageReceipts,
} from "../scripts/smoke-signed-macos-package.mjs";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const bytes = Buffer.from("archive");
const artifact = {
  path: "butter-paper-macos-arm64-1.2.3.zip",
  bytes: bytes.length,
  sha256: createHash("sha256").update(bytes).digest("hex"),
};
const common = {
  schemaVersion: 1,
  target: "macos-arm64",
  channel: "stable",
  version: "1.2.3",
  sourceRevision: "a".repeat(40),
  artifact,
  signingCertificateSha256: "b".repeat(64),
  assemblyManifestSha256: "c".repeat(64),
  signingReceiptSha256: "d".repeat(64),
  notarisation: { status: "Accepted", submissionId: "submit-123" },
  stapled: true,
};
const pkg = {
  ...common,
  schema: "butter-paper/package-manifest",
  appBundle: "Butter Paper.app",
  archiveFormat: "zip",
};
const verification = {
  ...common,
  schema: "butter-paper/package-verification",
  verified: true,
  verification: "strict-signed-macos-production",
  extractedAppVerified: true,
};

test("accepts matching package and verification receipts for exact archive bytes", () => {
  assert.deepEqual(
    validatePackageReceipts(artifact.path, bytes, pkg, verification),
    {
      target: "macos-arm64",
      version: "1.2.3",
      sourceRevision: "a".repeat(40),
      archiveSha256: artifact.sha256,
      signingCertificateSha256: "B".repeat(64),
    },
  );
});

test("rejects archive byte, receipt identity, and verification mutations", () => {
  assert.throws(
    () =>
      validatePackageReceipts(
        artifact.path,
        Buffer.from("different"),
        pkg,
        verification,
      ),
    /identity/,
  );
  assert.throws(
    () =>
      validatePackageReceipts(
        artifact.path,
        bytes,
        { ...pkg, sourceRevision: "e".repeat(40) },
        verification,
      ),
    /disagree/,
  );
  assert.throws(
    () =>
      validatePackageReceipts(artifact.path, bytes, pkg, {
        ...verification,
        verified: false,
      }),
    /disagree/,
  );
  assert.throws(
    () => validatePackageReceipts("renamed.zip", bytes, pkg, verification),
    /identity/,
  );
});

test("receipt validation requires bounded-format identity fields and both verified digests", () => {
  assert.throws(
    () =>
      validatePackageReceipts(
        artifact.path,
        bytes,
        { ...pkg, archiveFormat: "tar" },
        verification,
      ),
    /disagree/,
  );
  assert.throws(
    () =>
      validatePackageReceipts(
        artifact.path,
        bytes,
        { ...pkg, assemblyManifestSha256: "bad" },
        verification,
      ),
    /disagree/,
  );
  assert.throws(
    () =>
      validatePackageReceipts(artifact.path, bytes, pkg, {
        ...verification,
        stapled: false,
      }),
    /disagree/,
  );
  assert.throws(
    () =>
      validatePackageReceipts(artifact.path, bytes, pkg, {
        ...verification,
        extractedAppVerified: false,
      }),
    /disagree/,
  );
});

test("fixture must have a bounded PDF header and EOF marker", () => {
  assert.deepEqual(validateFixturePdf(Buffer.from("%PDF-1.7\nbody\n%%EOF\n")), {
    bytes: 20,
    sha256: createHash("sha256")
      .update("%PDF-1.7\nbody\n%%EOF\n")
      .digest("hex"),
  });
  assert.throws(() => validateFixturePdf(Buffer.from("not a pdf")), /PDF/);
  assert.throws(
    () => validateFixturePdf(Buffer.from("%PDF-1.7 incomplete")),
    /PDF/,
  );
  assert.throws(
    () => validateFixturePdf(Buffer.alloc(32 * 1024 * 1024 + 1, 65)),
    /PDF/,
  );
});

test("extraction accepts only the single signed app bundle at its top level", () => {
  assert.equal(validateExtractedInventory(["Butter Paper.app"]), true);
  assert.throws(
    () => validateExtractedInventory(["Butter Paper.app", "payload"]),
    /inventory/,
  );
  assert.throws(() => validateExtractedInventory(["other.app"]), /inventory/);
});

test("child environment redirects conventional roots and removes update/development authority", () => {
  const env = isolatedAppEnvironment({
    home: "/private/smoke/home",
    root: "/private/smoke",
  });
  assert.equal(env.HOME, "/private/smoke/home");
  assert.equal(env.CFFIXED_USER_HOME, env.HOME);
  assert.equal(env.TMPDIR, "/private/smoke/tmp");
  assert.equal(env.XDG_CACHE_HOME, "/private/smoke/cache");
  assert.equal(env.XDG_CONFIG_HOME, "/private/smoke/config");
  assert.equal(env.XDG_DATA_HOME, "/private/smoke/data");
  assert.equal(env.XDG_STATE_HOME, "/private/smoke/state");
  assert.equal(
    Object.keys(env).some((key) =>
      /UPDATE|FEED|PDFIUM|DEV|ELECTRON/i.test(key),
    ),
    false,
  );
});

test("macOS signed-package smoke requires a disposable hosted account with no local override claim", () => {
  assert.deepEqual(
    validateDisposableMacosRunner({
      environment: {
        GITHUB_ACTIONS: "true",
        CI: "true",
        RUNNER_TEMP: "/private/runner-temp",
      },
      effectiveHome: "/private/disposable-home",
    }),
    {
      effectiveHome: "/private/disposable-home",
      runnerTemp: "/private/runner-temp",
      stableNativeRoot:
        "/private/disposable-home/Library/Application Support/com.butterpaper.desktop/native-v1",
    },
  );
  assert.throws(
    () =>
      validateDisposableMacosRunner({
        environment: { CI: "true", RUNNER_TEMP: "/tmp" },
        effectiveHome: "/private/local-home",
      }),
    /disposable GitHub Actions runner/,
  );
  assert.throws(
    () =>
      validateDisposableMacosRunner({
        environment: {
          GITHUB_ACTIONS: "true",
          CI: "true",
          RUNNER_TEMP: "relative",
        },
        effectiveHome: "/private/disposable-home",
      }),
    /absolute RUNNER_TEMP/,
  );
});

test("native edit driver requires AX, posts real pointer events, and independently checks the saved PDF", async () => {
  const helperPath = join(
    dirname(fileURLToPath(import.meta.url)),
    "../scripts/PackagedEditSmoke.swift",
  );
  const helper = await readFile(helperPath, "utf8");
  assert.match(helper, /AXIsProcessTrusted\(\)/);
  assert.match(helper, /kAXPressAction/);
  assert.match(helper, /window-relative central document viewport/);
  assert.match(helper, /CGEvent\(mouseEventSource:/);
  assert.match(helper, /\.leftMouseDragged/);
  assert.match(helper, /PDFDocument\(url:/);
  assert.match(helper, /inspect-baseline/);
  assert.match(helper, /rectangle annotation/);
  assert.doesNotMatch(helper, /BP_TEST|SMOKE_AUTOMATION|automation-backdoor/i);
});

test("packaged smoke performs edit, normal save and quit, reopen, PDF inspection, and retains temp data on cleanup failure", async () => {
  const source = await readFile(
    new URL("../scripts/smoke-signed-macos-package.mjs", import.meta.url),
    "utf8",
  );
  assert.match(source, /driver\("probe"\)/);
  assert.match(source, /driver\("edit", child\.pid\)/);
  assert.match(source, /driver\("close", child\.pid\)/);
  assert.match(source, /driver\("reopened", child\.pid, outputPath\)/);
  assert.match(source, /driver\("inspect", outputPath\)/);
  assert.match(source, /if \(root && safeToRemoveRoot\)/);
  assert.match(source, /retainedTempRoot = root/);
  assert.match(source, /rectangleCount === 1/);
});
