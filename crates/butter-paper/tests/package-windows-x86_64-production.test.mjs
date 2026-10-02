import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { packageWindowsProduction } from "../scripts/package-windows-x86_64-production.mjs";

const revision = "0123456789abcdef0123456789abcdef01234567";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function pe(machine, dll = false) {
  const bytes = Buffer.alloc(0x80);
  bytes.write("MZ", 0, "ascii");
  bytes.writeUInt32LE(0x40, 0x3c);
  bytes.write("PE\0\0", 0x40, "ascii");
  bytes.writeUInt16LE(machine, 0x44);
  bytes.writeUInt16LE(2, 0x54);
  bytes.writeUInt16LE(dll ? 0x2000 : 0x0002, 0x56);
  bytes.writeUInt16LE(0x20b, 0x58);
  return bytes;
}

async function fixture(root, { approved = true, runtimeNote = true, architecture = "x86_64" } = {}) {
  const inputDir = join(root, "input");
  await mkdir(inputDir, { recursive: true });
  const machine = architecture === "arm64" ? 0xaa64 : 0x8664;
  const files = {
    "gpui-migration.exe": pe(machine),
    "butter-paper-pdf-worker.exe": pe(machine),
    "butter-paper-signature-phone.exe": pe(machine),
    "pdfium.dll": pe(machine, true),
    "README.md": Buffer.from(runtimeNote ? "Runtime dependencies: Microsoft Visual C++ runtime." : "Run the app."),
    "THIRD_PARTY_NOTICES.md": Buffer.from("Third-party notices and licences."),
    "PHONE_HELPER_THIRD_PARTY_NOTICES.md": Buffer.from("Go dependency licences."),
    "QRCP_LICENSE": Buffer.from("qrcp licence."),
    "SIGNATURE_PAD_LICENSE": Buffer.from("Signature Pad licence."),
  };
  for (const [name, bytes] of Object.entries(files)) await writeFile(join(inputDir, name), bytes);
  const receipt = {
    schemaVersion: 1,
    purpose: "production-distribution",
    productionApproved: approved,
    target: architecture === "arm64" ? "aarch64-pc-windows-msvc" : "x86_64-pc-windows-msvc",
    source: { revision },
    build: { provenance: "reviewed offline production build record" },
    redistributionReview: { reference: "review record" },
    library: { path: "pdfium.dll", bytes: files["pdfium.dll"].length, sha256: sha256(files["pdfium.dll"]) },
  };
  await writeFile(join(inputDir, `production-pdfium-windows-${architecture}.json`), `${JSON.stringify(receipt)}\n`);
  return inputDir;
}

async function iconFixture(root) {
  const bytes = Buffer.alloc(23);
  bytes.writeUInt16LE(1, 2);
  bytes.writeUInt16LE(1, 4);
  bytes.writeUInt32LE(1, 14);
  bytes.writeUInt32LE(22, 18);
  bytes[22] = 0;
  const path = join(root, "app.ico");
  await writeFile(path, bytes);
  return path;
}

function zipNames(bytes) {
  const names = [];
  let offset = 0;
  while (bytes.readUInt32LE(offset) === 0x04034b50) {
    const nameLength = bytes.readUInt16LE(offset + 26);
    const extraLength = bytes.readUInt16LE(offset + 28);
    const size = bytes.readUInt32LE(offset + 18);
    names.push(bytes.toString("utf8", offset + 30, offset + 30 + nameLength));
    offset += 30 + nameLength + extraLength + size;
  }
  return names;
}

function zipFile(bytes, wanted) {
  let offset = 0;
  while (bytes.readUInt32LE(offset) === 0x04034b50) {
    const nameLength = bytes.readUInt16LE(offset + 26);
    const extraLength = bytes.readUInt16LE(offset + 28);
    const size = bytes.readUInt32LE(offset + 18);
    const name = bytes.toString("utf8", offset + 30, offset + 30 + nameLength);
    const start = offset + 30 + nameLength + extraLength;
    if (name === wanted) return bytes.subarray(start, start + size).toString("utf8");
    offset = start + size;
  }
  throw new Error(`missing ZIP member: ${wanted}`);
}

test("packages exact Windows x64 production inventory into byte-reproducible ZIP", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-windows-package-"));
  try {
    const inputDir = await fixture(root);
    const iconPath = await iconFixture(root);
    const first = await packageWindowsProduction({ inputDir, iconPath, outputDir: join(root, "out-a"), version: "1.2.3", revision });
    const second = await packageWindowsProduction({ inputDir, iconPath, outputDir: join(root, "out-b"), version: "1.2.3", revision });
    const firstBytes = await readFile(first.archive);
    const secondBytes = await readFile(second.archive);
    assert.deepEqual(firstBytes, secondBytes);
    assert.equal(first.sha256, second.sha256);
    assert.deepEqual(zipNames(firstBytes), [
      "MANIFEST.json", "PHONE_HELPER_THIRD_PARTY_NOTICES.md", "QRCP_LICENSE", "README.md", "SIGNATURE_PAD_LICENSE", "THIRD_PARTY_NOTICES.md", "butter-paper-pdf-worker.exe",
      "butter-paper-signature-phone.exe", "butter-paper.ico", "gpui-migration.exe", "install.ps1", "pdfium.dll", "production-pdfium-windows-x86_64.json", "uninstall.ps1",
    ]);
    assert.equal(first.manifest.target, "x86_64-pc-windows-msvc");
    assert.equal(first.manifest.files["pdfium.dll"].sha256, sha256(pe(0x8664, true)));
    assert.equal(first.manifest.pdfiumReceiptSha256, sha256(await readFile(join(inputDir, "production-pdfium-windows-x86_64.json"))));
    assert.equal(first.manifest.files["butter-paper.ico"].sha256, sha256(await readFile(iconPath)));
    assert.ok(first.manifest.files["install.ps1"].sha256);
    assert.ok(first.manifest.files["uninstall.ps1"].sha256);
    const install = zipFile(firstBytes, "install.ps1");
    const uninstall = zipFile(firstBytes, "uninstall.ps1");
    assert.match(install, /LocalApplicationData/);
    assert.match(install, /CreateShortcut/);
    assert.match(install, /OpenWithProgids/);
    assert.match(install, /ButterPaper\.PDF\.1\.2\.3\.x86_64/);
    assert.match(install, /default choice was not changed/i);
    assert.match(install, /"%1"/);
    assert.match(install, /Join-Path \$installRoot 'butter-paper\.ico'/);
    assert.doesNotMatch(install, /UserChoice|SetValue\(''\s*,\s*\$progId/);
    assert.match(install, /Shortcut already exists/);
    assert.match(install, /ProgID already exists/);
    assert.match(install, /OpenWith value already exists/);
    assert.match(install, /\$createdShortcut = \$false/);
    assert.match(install, /\$createdOpenWithValue = \$false/);
    assert.match(install, /\$createdProgId = \$false/);
    assert.match(install, /if \(\$createdShortcut\).*Remove-Item/s);
    assert.match(install, /if \(\$createdOpenWithValue\).*DeleteValue/s);
    assert.match(install, /if \(\$createdProgId\).*DeleteSubKeyTree/s);
    assert.match(install, /Copied package manifest identity is invalid/);
    assert.match(install, /Get-FileHash.*SHA256/);
    assert.ok(install.indexOf("Copied package manifest identity is invalid") < install.indexOf("Set-Content -LiteralPath (Join-Path $installRoot '.butter-paper-install.json')"));
    assert.match(uninstall, /DeleteValue\(\$progId, \$false\)/);
    assert.match(uninstall, /shortcutMatches/);
    assert.match(uninstall, /DeleteSubKeyTree\(\$progId, \$false\)/);
    assert.doesNotMatch(uninstall, /DeleteSubKeyTree\([^)]*\.pdf/);
    assert.match(uninstall, /Ownership marker is missing; preserving the install directory/);
    assert.match(uninstall, /Ownership marker does not match this exact package install/);
    assert.ok(uninstall.indexOf("Ownership marker does not match this exact package install") < uninstall.lastIndexOf("Remove-Item -LiteralPath $expectedRoot -Recurse -Force"));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("fails closed for unapproved PDFium or missing runtime documentation", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-windows-package-"));
  try {
    const unapproved = await fixture(join(root, "unapproved"), { approved: false });
    const iconPath = await iconFixture(root);
    await assert.rejects(packageWindowsProduction({ inputDir: unapproved, iconPath, outputDir: join(root, "out-1"), version: "1.2.3", revision }), /production Windows PDFium staging receipt/);
    const undocumented = await fixture(join(root, "undocumented"), { runtimeNote: false });
    await assert.rejects(packageWindowsProduction({ inputDir: undocumented, iconPath, outputDir: join(root, "out-2"), version: "1.2.3", revision }), /README.md must document Windows runtime dependencies/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("rejects extra staged inputs and nested output paths", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-windows-package-"));
  try {
    const inputDir = await fixture(root);
    const iconPath = await iconFixture(root);
    await writeFile(join(inputDir, "unexpected.dll"), "extra");
    await assert.rejects(packageWindowsProduction({ inputDir, iconPath, outputDir: join(root, "out"), version: "1.2.3", revision }), /input inventory must contain exactly/);
    await rm(join(inputDir, "unexpected.dll"));
    await assert.rejects(packageWindowsProduction({ inputDir, iconPath, outputDir: join(inputDir, "nested"), version: "1.2.3", revision }), /outside the input directory/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("packages Windows arm64 only with the matching PDFium receipt", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-windows-package-arm64-"));
  try {
    const inputDir = await fixture(join(root, "arm64"), { architecture: "arm64" });
    const iconPath = await iconFixture(root);
    const result = await packageWindowsProduction({ inputDir, iconPath, outputDir: join(root, "out"), version: "1.2.3", revision, architecture: "arm64" });
    assert.equal(result.manifest.target, "aarch64-pc-windows-msvc");
    assert.match(result.archive, /butter-paper-windows-arm64-1\.2\.3\.zip$/);
    assert.equal(result.manifest.pdfiumReceiptSha256, sha256(await readFile(join(inputDir, "production-pdfium-windows-arm64.json"))));
    const x64 = await fixture(join(root, "x64"));
    await assert.rejects(packageWindowsProduction({ inputDir: x64, iconPath, outputDir: join(root, "wrong-out"), version: "1.2.3", revision, architecture: "arm64" }), /exactly:.*production-pdfium-windows-arm64\.json/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("rejects a phone helper built for the other Windows architecture", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-windows-helper-architecture-"));
  try {
    const inputDir = await fixture(root);
    const iconPath = await iconFixture(root);
    await writeFile(join(inputDir, "butter-paper-signature-phone.exe"), pe(0xaa64));
    await assert.rejects(
      packageWindowsProduction({ inputDir, iconPath, outputDir: join(root, "out"), version: "1.2.3", revision }),
      /phone\.exe is not a 64-bit PE file for x86_64-pc-windows-msvc/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("rejects missing or malformed app icons", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-windows-icon-"));
  try {
    const inputDir = await fixture(root);
    await assert.rejects(packageWindowsProduction({ inputDir, outputDir: join(root, "missing"), version: "1.2.3", revision }), /app \.ico input is required/);
    const iconPath = join(root, "bad.ico");
    await writeFile(iconPath, "not an icon");
    await assert.rejects(packageWindowsProduction({ inputDir, iconPath, outputDir: join(root, "bad"), version: "1.2.3", revision }), /valid ICO file/);
  } finally { await rm(root, { recursive: true, force: true }); }
});
