import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile, chmod } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { packageLinuxProduction } from "../scripts/package-linux-x86_64-production.mjs";

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const productIconPath = resolve(dirname(fileURLToPath(import.meta.url)), "../../../../assets/butter-paper-icon.png");

async function fixture(root, architecture = "x86_64") {
  const inputDir = join(root, "inputs");
  await mkdir(inputDir, { recursive: true });
  const content = {
    "gpui-migration": "ELF fixture app\n",
    "butter-paper-pdf-worker": "ELF fixture worker\n",
    "butter-paper-signature-phone": "ELF fixture phone helper\n",
    "libpdfium.so": "ELF fixture pdfium\n",
    "README.md": "Runtime dependencies: glibc, libfontconfig, X11/Wayland system libraries.\n",
    "THIRD_PARTY_NOTICES.md": "Reviewed third-party notices.\n",
    "PHONE_HELPER_THIRD_PARTY_NOTICES.md": "Go dependency licences.\n",
    "QRCP_LICENSE": "qrcp licence.\n",
    "SIGNATURE_PAD_LICENSE": "Signature Pad licence.\n",
  };
  for (const [name, value] of Object.entries(content)) {
    await writeFile(join(inputDir, name), value, { mode: 0o644 });
  }
  await chmod(join(inputDir, "gpui-migration"), 0o755);
  await chmod(join(inputDir, "butter-paper-pdf-worker"), 0o755);
  await chmod(join(inputDir, "butter-paper-signature-phone"), 0o755);
  const library = Buffer.from(content["libpdfium.so"]);
  const receipt = {
    schemaVersion: 1,
    purpose: "production-distribution",
    productionApproved: true,
    target: architecture === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu",
    source: { revision: "a".repeat(40) },
    build: { provenance: "CI build attestation sha256:" + "b".repeat(64) },
    redistributionReview: { reference: "review-123" },
    library: { path: "libpdfium.so", bytes: library.length, sha256: sha256(library) },
  };
  await writeFile(join(inputDir, `production-pdfium-linux-${architecture}.json`), `${JSON.stringify(receipt)}\n`);
  const iconPath = join(root, "product.png");
  await writeFile(iconPath, await readFile(productIconPath));
  return { inputDir, iconPath };
}

test("Linux x86_64 production archive has deterministic bytes and bound manifest", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "bp-linux-package-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const { inputDir, iconPath } = await fixture(root);
  const args = { inputDir, iconPath, version: "1.2.3", revision: "c".repeat(40) };
  const first = await packageLinuxProduction({ ...args, outputDir: join(root, "out-a") });
  const second = await packageLinuxProduction({ ...args, outputDir: join(root, "out-b") });
  assert.equal(first.sha256, second.sha256);
  assert.deepEqual(first.manifest.target, "x86_64-unknown-linux-gnu");
  assert.equal(first.manifest.sourceRevision, args.revision);
  assert.equal(first.manifest.files["libpdfium.so"].sha256, sha256(Buffer.from("ELF fixture pdfium\n")));
  const entries = execFileSync("tar", ["-tJf", first.archive], { encoding: "utf8" }).trim().split("\n");
  assert.deepEqual(entries, ["butter-paper-linux-x86_64-1.2.3/", ...Object.keys(first.manifest.files).sort().map((name) => `butter-paper-linux-x86_64-1.2.3/${name}`), "butter-paper-linux-x86_64-1.2.3/MANIFEST.json"].sort());
  const archivedManifest = execFileSync("tar", ["-xOJf", first.archive, "butter-paper-linux-x86_64-1.2.3/MANIFEST.json"], { encoding: "utf8" });
  assert.equal(JSON.parse(archivedManifest).pdfiumReceiptSha256, first.manifest.pdfiumReceiptSha256);
});

test("Linux production packaging fails closed without an approved matching PDFium receipt", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "bp-linux-package-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const { inputDir, iconPath } = await fixture(root);
  const receiptPath = join(inputDir, "production-pdfium-linux-x86_64.json");
  await rm(receiptPath);
  await assert.rejects(
    packageLinuxProduction({ inputDir, iconPath, outputDir: join(root, "out"), version: "1.2.3", revision: "c".repeat(40) }),
    /exactly:.*production-pdfium-linux-x86_64\.json/,
  );
  await writeFile(receiptPath, JSON.stringify({ schemaVersion: 1, productionApproved: false }));
  await assert.rejects(
    packageLinuxProduction({ inputDir, iconPath, outputDir: join(root, "out2"), version: "1.2.3", revision: "c".repeat(40) }),
    /missing or invalid production PDFium staging receipt/,
  );
});

test("Linux production packaging enforces exact sibling inventory and dependency note", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "bp-linux-package-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const { inputDir, iconPath } = await fixture(root);
  await writeFile(join(inputDir, "unexpected.txt"), "extra\n");
  await assert.rejects(
    packageLinuxProduction({ inputDir, iconPath, outputDir: join(root, "out"), version: "1.2.3", revision: "c".repeat(40) }),
    /input inventory must contain exactly/,
  );
  await rm(join(inputDir, "unexpected.txt"));
  await writeFile(join(inputDir, "README.md"), "Run the app.\n");
  await assert.rejects(
    packageLinuxProduction({ inputDir, iconPath, outputDir: join(root, "out2"), version: "1.2.3", revision: "c".repeat(40) }),
    /runtime dependencies/,
  );
});

test("Linux arm64 packaging binds the archive and receipt to the requested target", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "bp-linux-package-arm64-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const { inputDir, iconPath } = await fixture(root, "arm64");
  const result = await packageLinuxProduction({ inputDir, iconPath, outputDir: join(root, "out"), version: "1.2.3", revision: "c".repeat(40), architecture: "arm64" });
  assert.equal(result.manifest.target, "aarch64-unknown-linux-gnu");
  assert.match(result.archive, /butter-paper-linux-arm64-1\.2\.3\.tar\.xz$/);
  const mismatched = await fixture(join(root, "wrong"));
  await assert.rejects(packageLinuxProduction({ ...mismatched, outputDir: join(root, "wrong-out"), version: "1.2.3", revision: "c".repeat(40), architecture: "arm64" }), /exactly:.*production-pdfium-linux-arm64\.json/);
});
