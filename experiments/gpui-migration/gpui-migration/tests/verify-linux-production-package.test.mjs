import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstat, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { packageLinuxProduction } from "../scripts/package-linux-x86_64-production.mjs";
import { verifyLinuxProductionPackage } from "../scripts/verify-linux-production-package.mjs";

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const productIconPath = resolve(dirname(fileURLToPath(import.meta.url)), "../../../../assets/butter-paper-icon.png");

function elf(machine) {
  const bytes = Buffer.alloc(64);
  bytes.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1]);
  bytes.writeUInt16LE(3, 16);
  bytes.writeUInt16LE(machine, 18);
  return bytes;
}

async function fixture(root, architecture, { marker = false } = {}) {
  const inputDir = join(root, `input-${architecture}`);
  await mkdir(inputDir, { recursive: true });
  const machine = architecture === "arm64" ? 183 : 62;
  const content = {
    "gpui-migration": elf(machine),
    "butter-paper-pdf-worker": elf(machine),
    "butter-paper-signature-phone": elf(machine),
    "libpdfium.so": elf(machine),
    "README.md": Buffer.from("Runtime dependencies include glibc and system libraries.\n"),
    "THIRD_PARTY_NOTICES.md": Buffer.from(marker ? "development-pdfium override marker\n" : "Reviewed third-party notices.\n"),
    "PHONE_HELPER_THIRD_PARTY_NOTICES.md": Buffer.from("Go dependency licences.\n"),
    "QRCP_LICENSE": Buffer.from("qrcp licence.\n"),
    "SIGNATURE_PAD_LICENSE": Buffer.from("Signature Pad licence.\n"),
  };
  for (const [name, bytes] of Object.entries(content)) await writeFile(join(inputDir, name), bytes, { mode: name.startsWith("gpui-") || name.startsWith("butter-paper-") ? 0o755 : 0o644 });
  const target = architecture === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu";
  const receipt = {
    schemaVersion: 1, purpose: "production-distribution", productionApproved: true, target,
    source: { revision: "a".repeat(40) }, build: { provenance: `sha256:${"b".repeat(64)}` },
    redistributionReview: { reference: `sha256:${"c".repeat(64)}` },
    library: { path: "libpdfium.so", bytes: content["libpdfium.so"].length, sha256: sha256(content["libpdfium.so"]) },
  };
  await writeFile(join(inputDir, `production-pdfium-linux-${architecture}.json`), `${JSON.stringify(receipt, null, 2)}\n`);
  const out = join(root, `package-${architecture}`);
  const iconPath = join(root, "product.png");
  await writeFile(iconPath, await readFile(productIconPath));
  const packaged = await packageLinuxProduction({ inputDir, outputDir: out, version: "1.2.3", revision: "d".repeat(40), iconPath, architecture });
  return { archive: packaged.archive, rootName: `butter-paper-linux-${architecture}-1.2.3`, packageDir: join(out, `butter-paper-linux-${architecture}-1.2.3`), iconPath };
}

async function temporary(t, prefix) {
  const root = await mkdtemp(join(tmpdir(), prefix));
  t.after(() => rm(root, { recursive: true, force: true }));
  return root;
}

for (const architecture of ["arm64", "x86_64"]) {
  test(`Linux ${architecture} production archive emits stable-candidate receipts`, async (t) => {
    const root = await temporary(t, "bp-linux-verify-");
    const { archive, packageDir } = await fixture(root, architecture);
    const manifestPath = join(root, "receipts", "package-manifest.json");
    const verificationPath = join(root, "receipts", "package-verification.json");
    const result = await verifyLinuxProductionPackage({ inputArchive: archive, packageManifestPath: manifestPath, verificationReceiptPath: verificationPath, architecture, version: "1.2.3", revision: "d".repeat(40) });
    const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
    const receipt = JSON.parse(await readFile(verificationPath, "utf8"));
    assert.equal(manifest.schema, "butter-paper/package-manifest");
    assert.equal(manifest.target, architecture === "arm64" ? "linux-arm64" : "linux-x64");
    assert.equal(manifest.artifact.sha256, sha256(await readFile(archive)));
    assert.equal(result.artifact.bytes, (await readFile(archive)).length);
    assert.equal(manifest.package.target, architecture === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu");
    assert.equal(receipt.schema, "butter-paper/package-verification");
    assert.equal(receipt.verified, true);
    assert.deepEqual(receipt.artifact, manifest.artifact);
    assert.match(await readFile(join(packageDir, "butter-paper.desktop"), "utf8"), /MimeType=application\/pdf;/);
    assert.match(await readFile(join(packageDir, "butter-paper.desktop"), "utf8"), /Exec=butter-paper-gpui %F/);
    const installer = await readFile(join(packageDir, "install-user.sh"), "utf8");
    const uninstaller = await readFile(join(packageDir, "uninstall-user.sh"), "utf8");
    assert.match(installer, /target="\$data\/butter-paper\/1\.2\.3"/);
    assert.match(installer, /bin="\$HOME\/\.local\/bin"/);
    assert.match(installer, /mv -T -- "\$stage" "\$target"/);
    assert.match(installer, /awk/);
    assert.match(installer, /Select it as the default PDF application/);
    const rollbackTrap = installer.indexOf("trap rollback EXIT");
    const publishPayload = installer.indexOf('mv -T -n -- "$stage" "$target"');
    const createLauncher = installer.indexOf('ln -- "$launcher_tmp" "$launcher"');
    const createDesktop = installer.indexOf('ln -- "$desktop_tmp" "$desktop"');
    const createIcon = installer.indexOf('ln -- "$icon_tmp" "$icon"');
    const refreshDesktop = installer.indexOf('update-desktop-database "$data/applications"', createIcon);
    const commitInstall = installer.lastIndexOf("trap - EXIT HUP INT TERM");
    assert.ok(rollbackTrap >= 0 && rollbackTrap < publishPayload);
    assert.ok(publishPayload < createLauncher && createLauncher < createDesktop && createDesktop < createIcon && createIcon < refreshDesktop && refreshDesktop < commitInstall);
    assert.ok(installer.indexOf('cmp -s "$root/butter-paper.png" "$icon"') < installer.indexOf('rm -f -- "$icon"'));
    assert.ok(installer.indexOf('cmp -s "$desktop_tmp" "$desktop"') < installer.indexOf('rm -f -- "$desktop"'));
    assert.ok(installer.indexOf('cmp -s "$launcher_tmp" "$launcher"') < installer.indexOf('rm -f -- "$launcher"'));
    assert.ok(installer.indexOf('cmp -s "$root/$name" "$target/$name"') < installer.indexOf('rm -f -- "$target/$name"'));
    assert.match(installer, /rmdir -- "\$target" 2>\/dev\/null \|\| :/);
    assert.match(uninstaller, /Installed payload differs from the package/);
    assert.match(uninstaller, /rmdir -- "\$target"/);
    const inventoryPreflight = uninstaller.indexOf("actual_count=0");
    const unrelatedPreflight = uninstaller.indexOf("Versioned install directory contains unrelated files; preserved it.");
    const payloadIdentity = uninstaller.indexOf('cmp -s "$root/$name" "$target/$name"');
    const removePayload = uninstaller.indexOf('rm -f -- "$target/$name"');
    const removeIntegration = uninstaller.indexOf('rm -- "$launcher" "$desktop" "$icon"');
    assert.ok(inventoryPreflight >= 0);
    assert.ok(inventoryPreflight < unrelatedPreflight && unrelatedPreflight < payloadIdentity);
    assert.ok(payloadIdentity < removePayload && removePayload < removeIntegration);
    for (const name of ["install-user.sh", "uninstall-user.sh"]) {
      const scriptPath = join(packageDir, name);
      const script = await readFile(scriptPath, "utf8");
      assert.match(script, /XDG_DATA_HOME/);
      assert.match(script, /update-desktop-database/);
      assert.match(script, /update-mime-database/);
      assert.equal((await lstat(scriptPath)).mode & 0o777, 0o755);
      // `sh -n` parses syntax only; it does not execute the packaged script.
      execFileSync("sh", ["-n", scriptPath]);
    }
    assert.equal((await lstat(join(packageDir, "butter-paper.png"))).mode & 0o777, 0o644);
    const packageContents = JSON.parse(await readFile(join(packageDir, "MANIFEST.json"), "utf8"));
    assert.equal(packageContents.files["butter-paper.png"].sha256, sha256(await readFile(join(packageDir, "butter-paper.png"))));
  });
}

test("Linux packager rejects missing and unsafe product PNG inputs without executing integration scripts", async (t) => {
  const root = await temporary(t, "bp-linux-verify-");
  const { packageDir, iconPath } = await fixture(root, "x86_64");
  const inputDir = join(root, "input-x86_64");
  const outputDir = join(root, "rejected-output");
  await assert.rejects(packageLinuxProduction({ inputDir, outputDir, version: "1.2.3", revision: "d".repeat(40), iconPath: join(root, "absent.png") }), /ENOENT|product PNG icon/);
  const unsafePath = join(root, "bad.png");
  await writeFile(unsafePath, "not a png");
  await assert.rejects(packageLinuxProduction({ inputDir, outputDir, version: "1.2.3", revision: "d".repeat(40), iconPath: unsafePath }), /valid 1024x1024 PNG/);
  assert.equal(await readFile(join(packageDir, "install-user.sh"), "utf8").then((text) => text.startsWith("#!/bin/sh\nset -eu\n")), true);
  await assert.rejects(readFile(join(root, "does-not-exist")));
});

test("Linux verification rejects a target mismatch and does not publish receipts", async (t) => {
  const root = await temporary(t, "bp-linux-verify-");
  const { archive } = await fixture(root, "x86_64");
  const manifestPath = join(root, "manifest.json");
  const receiptPath = join(root, "verification.json");
  await assert.rejects(verifyLinuxProductionPackage({ inputArchive: archive, packageManifestPath: manifestPath, verificationReceiptPath: receiptPath, architecture: "arm64", version: "1.2.3", revision: "d".repeat(40) }), /root|inventory|target|manifest/i);
  await assert.rejects(readFile(manifestPath));
  await assert.rejects(readFile(receiptPath));
});

test("Linux verification refuses marker-bearing packages and existing outputs", async (t) => {
  const root = await temporary(t, "bp-linux-verify-");
  const { archive } = await fixture(root, "x86_64", { marker: true });
  const manifestPath = join(root, "manifest.json");
  const receiptPath = join(root, "verification.json");
  await writeFile(manifestPath, "occupied");
  await assert.rejects(verifyLinuxProductionPackage({ inputArchive: archive, packageManifestPath: manifestPath, verificationReceiptPath: receiptPath, architecture: "x86_64", version: "1.2.3", revision: "d".repeat(40) }), /already exists/);
  await rm(manifestPath);
  await assert.rejects(verifyLinuxProductionPackage({ inputArchive: archive, packageManifestPath: manifestPath, verificationReceiptPath: receiptPath, architecture: "x86_64", version: "1.2.3", revision: "d".repeat(40) }), /development PDFium or override marker/);
  await assert.rejects(readFile(manifestPath));
});
