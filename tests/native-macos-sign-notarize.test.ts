import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { assembleMacosProductionApp } from "../experiments/gpui-migration/gpui-migration/scripts/assemble-macos-production.mjs";
import {
  expectedNativeSignedCodeObjects,
} from "../experiments/gpui-migration/gpui-migration/scripts/verify-signed-macos-production.mjs";
import {
  signNotariseVerifyNativeMacosApp,
} from "../experiments/gpui-migration/gpui-migration/scripts/sign-notarize-macos-production.mjs";
import { createNativeMacosProductionFixture } from "./helpers/native-macos-production-fixture";

const roots: string[] = [];
const identity = "Developer ID Application: Alexander Potenza (27JL2VERNC)";
const certificate = Buffer.from("synthetic trusted leaf certificate");
const fingerprint = createHash("sha256").update(certificate).digest("hex").toUpperCase();

async function unsignedFixture() {
  const setup = await createNativeMacosProductionFixture();
  roots.push(setup.root);
  await assembleMacosProductionApp({
    manifestPath: setup.manifestPath,
    inputRoot: setup.inputRoot,
    pdfiumStage: setup.pdfiumStage,
    outputDirectory: setup.output,
  });
  return setup;
}

function strictFakeRunner(setup: Awaited<ReturnType<typeof unsignedFixture>>, {
  notaryStatus = "Accepted",
  failSigningRole,
}: { notaryStatus?: string; failSigningRole?: string } = {}) {
  const objects = expectedNativeSignedCodeObjects("stable").map((object) => ({
    ...object,
    path: join(setup.output, object.path),
  }));
  const calls: Array<{ command: string; args: string[]; input?: string }> = [];
  let archivePath: string | undefined;
  const objectAt = (path: string) => {
    const object = objects.find((entry) => entry.path === path);
    if (!object) throw new Error(`unexpected code path ${path}`);
    return object;
  };
  const run = (command: string, args: string[], options: { input?: string } = {}) => {
    calls.push({ command, args, input: options.input });
    if (command === "codesign" && args[0] === "--force") {
      const signingPath = args.at(-1)!;
      const object = signingPath === setup.output
        ? objects.find((entry) => entry.role === "application")!
        : objectAt(signingPath);
      const signingEntitlementsPath = args.at(-2)!;
      expect(args).toEqual([
        "--force", "--sign", identity,
        "--options", "runtime", "--timestamp",
        "--identifier", object.identifier,
        "--entitlements", expect.stringMatching(/\.plist$/), signingPath,
      ]);
      expect(signingEntitlementsPath).toMatch(/\.plist$/);
      const plist = readFileSync(signingEntitlementsPath, "utf8");
      if (object.role === "camera-helper") {
        expect(plist).toContain("<key>com.apple.security.device.camera</key>");
      } else {
        expect(plist).toContain("<dict></dict>");
        expect(plist).not.toContain("com.apple.security.device.camera");
      }
      if (object.role === failSigningRole) throw new Error("synthetic signing failure");
      if (signingPath === setup.output) {
        mkdirSync(join(setup.output, "Contents/_CodeSignature"), { recursive: true });
        writeFileSync(join(setup.output, "Contents/_CodeSignature/CodeResources"), "synthetic resources");
      }
      return "";
    }
    if (command === "/usr/bin/ditto") {
      expect(args).toEqual(["-c", "-k", "--keepParent", setup.output, expect.stringMatching(/Butter Paper notarisation\.zip$/)]);
      archivePath = args.at(-1)!;
      writeFileSync(archivePath, "synthetic ZIP archive");
      return "";
    }
    if (command === "xcrun" && args[0] === "notarytool") {
      expect(archivePath).toBe(args[2]);
      expect(args[2]).toMatch(/\.zip$/);
      expect(existsSync(args[2])).toBe(true);
      expect(args).toEqual([
        "notarytool", "submit", archivePath,
        "--keychain-profile", "production-notary",
        "--wait", "--output-format", "json",
      ]);
      return JSON.stringify({ id: "submission-123", status: notaryStatus });
    }
    if (command === "xcrun" && args[0] === "stapler") {
      writeFileSync(join(setup.output, "Contents/CodeResources"), "synthetic notarisation ticket");
      return "";
    }
    if (command === "codesign" && args[0] === "--verify") return "";
    if (command === "codesign" && args[0] === "-dvvv") {
      const object = objectAt(args.at(-1)!);
      return [
        `Identifier=${object.identifier}`,
        `Authority=${identity}`,
        "TeamIdentifier=27JL2VERNC",
        "CDHash=abc123",
        "Timestamp=28 Sep 2026 at 10:00:00",
        "Notarization Ticket=stapled",
        "CodeDirectory v=20500 flags=0x10000(runtime)",
      ].join("\n");
    }
    if (command === "lipo") return "arm64";
    if (command === "codesign" && args[0] === "-d" && args.includes("--xml")) {
      const object = objectAt(args.at(-1)!);
      return object.role === "camera-helper" ? "<?xml version=\"1.0\"?><plist><dict><key>com.apple.security.device.camera</key><true/></dict></plist>" : "";
    }
    if (command === "plutil") {
      return options.input?.includes("com.apple.security.device.camera")
        ? JSON.stringify({ "com.apple.security.device.camera": true })
        : "{}";
    }
    if (command === "codesign" && args[0] === "-d" && args.some((arg) => arg.startsWith("--extract-certificates="))) {
      const object = objectAt(args.at(-1)!);
      const prefix = args.find((arg) => arg.startsWith("--extract-certificates="))!.slice("--extract-certificates=".length);
      writeFileSync(`${prefix}0`, certificate);
      if (object.role === "application") {
        writeFileSync(`${prefix}1`, "synthetic intermediate");
        writeFileSync(`${prefix}2`, "synthetic root");
      }
      return "";
    }
    if (command === "security" || command === "spctl") return "";
    throw new Error(`unexpected fake platform command: ${command} ${args.join(" ")}`);
  };
  return { calls, run, get archivePath() { return archivePath; } };
}

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe("macOS production signing and notarisation orchestration", () => {
  it("signs nested code first with exact runtime entitlements, waits, staples, verifies and writes a stable receipt", async () => {
    const setup = await unsignedFixture();
    const receiptPath = join(setup.root, "signing-receipt.json");
    const fake = strictFakeRunner(setup);
    const receipt = await signNotariseVerifyNativeMacosApp({
      appPath: setup.output,
      manifestPath: setup.manifestPath,
      identity,
      fingerprint,
      notaryProfile: "production-notary",
      receiptPath,
      run: fake.run,
    });

    expect(fake.calls.filter(({ command, args }) => command === "codesign" && args[0] === "--force").map(({ args }) => args.at(-1)))
      .toEqual([
        "libpdfium.dylib",
        "butter-paper-pdf-worker",
        "butter-paper-signature-camera",
        "butter-paper-signature-phone",
        setup.output,
      ].map((name) => name === setup.output ? name : expect.stringContaining(name)));
    const submitIndex = fake.calls.findIndex(({ command, args }) => command === "xcrun" && args[0] === "notarytool");
    const archiveIndex = fake.calls.findIndex(({ command }) => command === "/usr/bin/ditto");
    const stapleIndex = fake.calls.findIndex(({ command, args }) => command === "xcrun" && args[0] === "stapler" && args[1] === "staple");
    const verifyIndex = fake.calls.findIndex(({ command, args }) => command === "codesign" && args[0] === "--verify" && args.includes("--deep"));
    expect(submitIndex).toBeGreaterThan(0);
    expect(archiveIndex).toBeGreaterThan(0);
    expect(submitIndex).toBeGreaterThan(archiveIndex);
    expect(stapleIndex).toBeGreaterThan(submitIndex);
    expect(verifyIndex).toBeGreaterThan(stapleIndex);
    expect(receipt).toMatchObject({
      schema: "butter-paper/signed-native-macos-production",
      version: 1,
      channel: "stable",
      target: "aarch64-apple-darwin",
      codeObjectCount: 5,
      identity,
      signingCertificateSha256: fingerprint,
      notarisation: { status: "Accepted", submissionId: "submission-123" },
      stapled: true,
      verified: true,
    });
    expect(await readFile(receiptPath, "utf8")).toBe(`${JSON.stringify(receipt, null, 2)}\n`);
    expect(fake.archivePath).toBeDefined();
    expect(existsSync(fake.archivePath!)).toBe(false);
    await expect(signNotariseVerifyNativeMacosApp({
      appPath: setup.output,
      manifestPath: setup.manifestPath,
      identity,
      fingerprint,
      notaryProfile: "production-notary",
      receiptPath,
      run: fake.run,
    })).rejects.toThrow();
  });

  it("requires explicit trust and notarisation inputs before calling platform tools", async () => {
    const setup = await unsignedFixture();
    const fake = strictFakeRunner(setup);
    await expect(signNotariseVerifyNativeMacosApp({
      appPath: setup.output,
      manifestPath: setup.manifestPath,
      identity,
      fingerprint,
      notaryProfile: "",
      receiptPath: join(setup.root, "receipt.json"),
      run: fake.run,
    })).rejects.toThrow("notaryProfile is required");
    await expect(signNotariseVerifyNativeMacosApp({
      appPath: setup.output,
      manifestPath: setup.manifestPath,
      identity: "-",
      fingerprint,
      notaryProfile: "production-notary",
      receiptPath: join(setup.root, "receipt.json"),
      run: fake.run,
    })).rejects.toThrow("Developer ID identity");
    expect(fake.calls).toHaveLength(0);
  });

  it("stops before notarisation on signing failure and before stapling on a rejected submission", async () => {
    const signingSetup = await unsignedFixture();
    const signingFake = strictFakeRunner(signingSetup, { failSigningRole: "camera-helper" });
    await expect(signNotariseVerifyNativeMacosApp({
      appPath: signingSetup.output,
      manifestPath: signingSetup.manifestPath,
      identity,
      fingerprint,
      notaryProfile: "production-notary",
      receiptPath: join(signingSetup.root, "receipt.json"),
      run: signingFake.run,
    })).rejects.toThrow("synthetic signing failure");
    expect(signingFake.calls.some(({ command, args }) => command === "xcrun" && args[0] === "notarytool")).toBe(false);

    const rejectedSetup = await unsignedFixture();
    const rejectedFake = strictFakeRunner(rejectedSetup, { notaryStatus: "Invalid" });
    await expect(signNotariseVerifyNativeMacosApp({
      appPath: rejectedSetup.output,
      manifestPath: rejectedSetup.manifestPath,
      identity,
      fingerprint,
      notaryProfile: "production-notary",
      receiptPath: join(rejectedSetup.root, "receipt.json"),
      run: rejectedFake.run,
    })).rejects.toThrow("notarisation was not accepted");
    expect(rejectedFake.calls.some(({ command, args }) => command === "xcrun" && args[0] === "stapler")).toBe(false);
    expect(rejectedFake.calls.some(({ command, args }) => command === "codesign" && args[0] === "--verify")).toBe(false);
    expect(rejectedFake.archivePath).toBeDefined();
    expect(existsSync(rejectedFake.archivePath!)).toBe(false);
  });
});
