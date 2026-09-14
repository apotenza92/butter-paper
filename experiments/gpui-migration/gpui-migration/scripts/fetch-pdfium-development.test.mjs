import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import test from "node:test";
import { resolve } from "node:path";
import { tmpdir } from "node:os";
import {
  artifactReceipt,
  assertArchiveEntriesSafe,
  hostTarget,
  sha256,
  validateManifest,
  verifyExtractedArtifact,
} from "./fetch-pdfium-development.mjs";

test("pins an explicitly non-production six-target PDFium supplier manifest", () => {
  const manifest = JSON.parse(readFileSync(resolve(import.meta.dirname, "..", "pdfium-development-binaries.json")));
  assert.doesNotThrow(() => validateManifest(manifest));
  assert.equal(manifest.productionApproved, false);
});

test("maps supported development hosts and computes checksums", () => {
  assert.equal(hostTarget("linux", "x64"), "x86_64-unknown-linux-gnu");
  assert.equal(hostTarget("darwin", "arm64"), "aarch64-apple-darwin");
  assert.equal(sha256(Buffer.from("butter-paper")), "f1adcc8e50b942e330e7bec1cf3086dcfc9dfc71740aaf92cdac14d8af6a097e");
});

test("rejects traversal and absolute archive entries", () => {
  assert.throws(() => assertArchiveEntriesSafe("lib/libpdfium.so\n../escape"), /unsafe archive/);
  assert.throws(() => assertArchiveEntriesSafe("/absolute"), /unsafe archive/);
  assert.doesNotThrow(() => assertArchiveEntriesSafe("lib/libpdfium.so\nVERSION"));
});

test("requires a receipt that binds reused extraction output to the archive pin", () => {
  const root = mkdtempSync(resolve(tmpdir(), "butter-paper-pdfium-receipt-"));
  const asset = {
    target: "x86_64-unknown-linux-gnu",
    sha256: "a".repeat(64),
    bytes: 123,
    library: "lib/libpdfium.so",
  };
  try {
    mkdirSync(resolve(root, "lib"));
    writeFileSync(resolve(root, asset.library), "development library");
    writeFileSync(resolve(root, "VERSION"), "MAJOR=151\nMINOR=0\nBUILD=7881\nPATCH=0\n");
    assert.throws(() => verifyExtractedArtifact(root, asset), /verification receipt/);
    writeFileSync(
      resolve(root, ".butter-paper-pdfium-development.json"),
      JSON.stringify(artifactReceipt(asset)),
    );
    assert.doesNotThrow(() => verifyExtractedArtifact(root, asset));
    writeFileSync(
      resolve(root, ".butter-paper-pdfium-development.json"),
      JSON.stringify({ ...artifactReceipt(asset), archiveSha256: "b".repeat(64) }),
    );
    assert.throws(() => verifyExtractedArtifact(root, asset), /does not match/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
