import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { link, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { packageWindowsProduction } from "../scripts/package-windows-x86_64-production.mjs";
import { signVerifyWindowsProduction, verifyUnsignedWindowsProduction } from "../scripts/sign-verify-windows-production.mjs";

const revision = "0123456789abcdef0123456789abcdef01234567";
const thumbprint = "0123456789ABCDEF0123456789ABCDEF01234567";
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

async function inputFixture(root, architecture = "x86_64") {
  const input = join(root, "staged");
  await mkdir(input, { recursive: true });
  const machine = architecture === "arm64" ? 0xaa64 : 0x8664;
  const files = {
    "gpui-migration.exe": pe(machine),
    "butter-paper-pdf-worker.exe": pe(machine),
    "butter-paper-signature-phone.exe": pe(machine),
    "pdfium.dll": pe(machine, true),
    "README.md": Buffer.from("Runtime dependencies: MSVC runtime."),
    "THIRD_PARTY_NOTICES.md": Buffer.from("Notices."),
    "PHONE_HELPER_THIRD_PARTY_NOTICES.md": Buffer.from("Go dependency licences."),
    "QRCP_LICENSE": Buffer.from("qrcp licence."),
    "SIGNATURE_PAD_LICENSE": Buffer.from("Signature Pad licence."),
  };
  for (const [name, content] of Object.entries(files)) await writeFile(join(input, name), content);
  const receipt = {
    schemaVersion: 1, purpose: "production-distribution", productionApproved: true,
    target: architecture === "arm64" ? "aarch64-pc-windows-msvc" : "x86_64-pc-windows-msvc",
    source: { revision }, build: { provenance: "production fixture" },
    redistributionReview: { reference: "review fixture" },
    library: { path: "pdfium.dll", bytes: files["pdfium.dll"].length, sha256: sha256(files["pdfium.dll"]) },
  };
  await writeFile(join(input, `production-pdfium-windows-${architecture}.json`), JSON.stringify(receipt));
  const icon = Buffer.alloc(23);
  icon.writeUInt16LE(1, 2); icon.writeUInt16LE(1, 4); icon.writeUInt32LE(1, 14); icon.writeUInt32LE(22, 18); icon[22] = 0;
  const iconPath = join(root, "app.ico");
  await writeFile(iconPath, icon);
  return { input, iconPath };
}

async function packageFixture(root, architecture = "x86_64") {
  const { input: inputDir, iconPath } = await inputFixture(root, architecture);
  const result = await packageWindowsProduction({ inputDir, iconPath, outputDir: root, version: "1.2.3", revision, architecture });
  return result.archive;
}

function fakeRunner({ status = "Valid", signer = thumbprint, timestamped = true, timestampThumbprint = "89ABCDEF0123456789ABCDEF0123456789ABCDEF" } = {}) {
  const calls = [];
  const runner = async (command, args) => {
    calls.push({ command, args });
    if (args[0] === "sign") {
      const path = args.at(-1);
      await writeFile(path, Buffer.concat([await readFile(path), Buffer.from("\nFAKE-SIGNATURE") ]));
      return "signed";
    }
    return JSON.stringify({ Status: status, SignerThumbprint: signer, Timestamped: timestamped, TimestampThumbprint: timestampThumbprint });
  };
  return { calls, runner };
}

async function sign(root, archive, runner, architecture = "x86_64") {
  return signVerifyWindowsProduction({
    inputArchive: archive, outputArchive: join(root, "signed.zip"), packageManifestPath: join(root, "package.json"),
    verificationReceiptPath: join(root, "verification.json"), architecture, version: "1.2.3", revision,
    certificateThumbprint: thumbprint, timestampUrl: "https://timestamp.example.test/rfc3161", runner,
  });
}

test("signs and independently verifies all Windows production code objects for x64 and arm64", async () => {
  for (const architecture of ["x86_64", "arm64"]) {
    const root = await mkdtemp(join(tmpdir(), "bp-sign-windows-"));
    try {
      const archive = await packageFixture(root, architecture);
      const { calls, runner } = fakeRunner();
      await sign(root, archive, runner, architecture);
      const receipt = JSON.parse(await readFile(join(root, "verification.json"), "utf8"));
      const manifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
      assert.equal(receipt.schema, "butter-paper/package-verification");
      assert.equal(receipt.target, architecture === "arm64" ? "windows-arm64" : "windows-x64");
      assert.equal(receipt.verified, true);
      assert.equal(receipt.artifact.sha256, sha256(await readFile(join(root, "signed.zip"))));
      assert.deepEqual(receipt.signatures.map(({ path }) => path).sort(), ["butter-paper-pdf-worker.exe", "butter-paper-signature-phone.exe", "gpui-migration.exe", "pdfium.dll"]);
      assert.equal(calls.filter(({ args }) => args[0] === "sign").length, 4);
      assert.equal(manifest.artifact.sha256, receipt.artifact.sha256);
      assert.equal(manifest.artifact.path, "signed.zip");
      assert.equal(manifest.package.target, architecture === "arm64" ? "aarch64-pc-windows-msvc" : "x86_64-pc-windows-msvc");
      assert.equal(manifest.package.pdfiumReceiptSha256.length, 64);
      assert.equal(calls.filter(({ args }) => args[0] === "sign").length, 4);
      assert.ok(calls.filter(({ args }) => args[0] === "sign").every(({ args }) => args.includes("/fd") && args.includes("SHA256") && args.includes("/td") && args.includes("/tr")));
      assert.equal(calls.filter(({ command }) => command === "powershell.exe").length, 4);
      const zip = await readFile(join(root, "signed.zip"));
      assert.match(zip.toString("utf8"), /OpenWithProgids/);
      assert.match(zip.toString("utf8"), /install\.ps1/);
      assert.match(zip.toString("utf8"), /uninstall\.ps1/);
    } finally { await rm(root, { recursive: true, force: true }); }
  }
});

test("verifies and publishes explicitly unsigned Windows packages without changing their bytes", async () => {
  for (const architecture of ["x86_64", "arm64"]) {
    const root = await mkdtemp(join(tmpdir(), "bp-unsigned-windows-"));
    try {
      const archive = await packageFixture(root, architecture);
      const sourceBytes = await readFile(archive);
      const output = join(root, "unsigned-release.zip");
      await verifyUnsignedWindowsProduction({
        inputArchive: archive,
        outputArchive: output,
        packageManifestPath: join(root, "package.json"),
        verificationReceiptPath: join(root, "verification.json"),
        architecture,
        version: "1.2.3",
        revision,
      });
      assert.deepEqual(await readFile(output), sourceBytes);
      const manifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
      const receipt = JSON.parse(await readFile(join(root, "verification.json"), "utf8"));
      assert.equal(manifest.signaturePolicy, "unsigned-user-authorised");
      assert.deepEqual(manifest.signatures, []);
      assert.equal(receipt.verified, true);
      assert.equal(receipt.integrityVerified, true);
      assert.equal(receipt.signaturePolicy, "unsigned-user-authorised");
      assert.deepEqual(receipt.signatures, []);
      assert.equal(receipt.artifact.sha256, sha256(sourceBytes));
    } finally { await rm(root, { recursive: true, force: true }); }
  }
});

test("fails closed for missing timestamp and mismatched signer or invalid signature status", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-sign-windows-invalid-"));
  try {
    const archive = await packageFixture(root);
    await assert.rejects(signVerifyWindowsProduction({ inputArchive: archive, outputArchive: join(root, "a"), packageManifestPath: join(root, "b"), verificationReceiptPath: join(root, "c"), architecture: "x86_64", version: "1.2.3", revision, certificateThumbprint: thumbprint }), /RFC3161 timestamp URL/);
    await assert.rejects(sign(root, archive, fakeRunner({ signer: "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF" }).runner), /signer thumbprint/);
    await assert.rejects(sign(root, archive, fakeRunner({ status: "NotSigned" }).runner), /status is NotSigned/);
    await assert.rejects(sign(root, archive, fakeRunner({ timestampThumbprint: "G".repeat(40) }).runner), /valid independently verified RFC3161 timestamp certificate thumbprint/);
    for (const path of ["signed.zip", "package.json", "verification.json"]) {
      await assert.rejects(readFile(join(root, path)), { code: "ENOENT" });
    }
  } finally { await rm(root, { recursive: true, force: true }); }
});

test("rejects invalid timestamp evidence and mismatched package identity before signing", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-sign-windows-inventory-"));
  try {
    const archive = await packageFixture(root);
    await assert.rejects(sign(root, archive, fakeRunner({ timestamped: false, timestampThumbprint: "" }).runner), /valid independently verified RFC3161 timestamp certificate thumbprint/);
    await assert.rejects(signVerifyWindowsProduction({
      inputArchive: archive, outputArchive: join(root, "x"), packageManifestPath: join(root, "y"), verificationReceiptPath: join(root, "z"),
      architecture: "arm64", version: "1.2.3", revision, certificateThumbprint: thumbprint,
      timestampUrl: "https://timestamp.example.test", runner: async () => "{}",
    }), /identity does not match/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test("cleans only this invocation's outputs when a final publication link fails", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-sign-windows-publish-failure-"));
  try {
    const archive = await packageFixture(root);
    const unrelated = join(root, "keep.txt");
    await writeFile(unrelated, "user data");
    let links = 0;
    await assert.rejects(signVerifyWindowsProduction({
      inputArchive: archive, outputArchive: join(root, "signed.zip"), packageManifestPath: join(root, "package.json"),
      verificationReceiptPath: join(root, "verification.json"), architecture: "x86_64", version: "1.2.3", revision,
      certificateThumbprint: thumbprint, timestampUrl: "https://timestamp.example.test", runner: fakeRunner().runner,
      publishLink: async (source, destination) => {
        links += 1;
        if (links === 2) throw new Error("simulated final publication failure");
        await link(source, destination);
      },
    }), /simulated final publication failure/);
    assert.equal(await readFile(unrelated, "utf8"), "user data");
    for (const path of ["signed.zip", "package.json", "verification.json"]) {
      await assert.rejects(readFile(join(root, path)), { code: "ENOENT" });
    }
  } finally { await rm(root, { recursive: true, force: true }); }
});
