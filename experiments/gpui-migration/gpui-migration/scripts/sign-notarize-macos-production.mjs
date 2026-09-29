#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  validateNativeAssemblyManifest,
} from "./assemble-macos-production.mjs";
import {
  expectedNativeSignedCodeObjects,
  verifySignedNativeMacosApp,
} from "./verify-signed-macos-production.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const expectedIdentity = "Developer ID Application: Alexander Potenza (27JL2VERNC)";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function defaultRun(command, args, { input } = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    input,
    maxBuffer: 16 * 1024 * 1024,
  });
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed (${result.status}): ${output.trim()}`);
  }
  return output;
}

function validateInputs({ appPath, manifestPath, identity, fingerprint, notaryProfile, receiptPath }) {
  for (const [label, value] of Object.entries({ appPath, manifestPath, identity, fingerprint, notaryProfile, receiptPath })) {
    if (typeof value !== "string" || value.trim() === "") throw new Error(`${label} is required`);
  }
  if (identity !== expectedIdentity) throw new Error("Developer ID identity does not match the pinned production verifier");
  const normalisedFingerprint = String(fingerprint).replace(/[^0-9a-f]/gi, "").toUpperCase();
  if (!/^[0-9A-F]{64}$/.test(normalisedFingerprint)) throw new Error("trusted signing fingerprint must be a SHA-256 digest");
  if (/[\0\r\n]/.test(notaryProfile)) throw new Error("notary profile contains invalid characters");
  return { fingerprint: normalisedFingerprint };
}

function entitlementPlist(entitlements) {
  const entries = Object.entries(entitlements)
    .map(([key, enabled]) => `\t<key>${key}</key>\n\t<${enabled ? "true" : "false"}/>`)
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">\n<dict>${entries ? `\n${entries}\n` : ""}</dict>\n</plist>\n`;
}

function requireAcceptedNotarisation(output) {
  let result;
  try {
    result = JSON.parse(output);
  } catch {
    throw new Error("notarytool did not return valid JSON");
  }
  if (result?.status !== "Accepted" || typeof result.id !== "string" || result.id.length === 0) {
    throw new Error(`notarisation was not accepted (status: ${String(result?.status ?? "missing")})`);
  }
  return result.id;
}

export async function signNotariseVerifyNativeMacosApp({
  appPath,
  manifestPath,
  identity,
  fingerprint,
  notaryProfile,
  receiptPath,
  run = defaultRun,
}) {
  const validated = validateInputs({ appPath, manifestPath, identity, fingerprint, notaryProfile, receiptPath });
  const app = resolve(appPath);
  const receiptDestination = resolve(receiptPath);
  const realApp = await realpath(app);
  const realReceiptParent = await realpath(dirname(receiptDestination));
  const realReceipt = join(realReceiptParent, basename(receiptDestination));
  const receiptRelativeToApp = relative(realApp, realReceipt);
  if (!receiptRelativeToApp || (receiptRelativeToApp !== ".." && !receiptRelativeToApp.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`) && !isAbsolute(receiptRelativeToApp))) {
    throw new Error("signing receipt must be written outside the signed app bundle");
  }
  const manifestBytes = await readFile(manifestPath);
  const manifest = JSON.parse(manifestBytes);
  const packageVersion = JSON.parse(await readFile(resolve(dirname(scriptPath), "../../../..", "package.json"), "utf8")).version;
  validateNativeAssemblyManifest(manifest, packageVersion);
  const expectedAppName = manifest.channel === "stable" ? "Butter Paper.app" : "Butter Paper Beta.app";
  if (basename(app) !== expectedAppName) throw new Error("app filename does not match its release channel");
  const objects = expectedNativeSignedCodeObjects(manifest.channel);
  const entitlementsDirectory = await mkdtemp(resolve(tmpdir(), "bp-native-entitlements-"));
  try {
    // Sign embedded code first. Signing the bundle root last seals resources and its main executable.
    const signingOrder = objects.filter(({ role }) => role !== "application").sort((left, right) => {
      const rank = (object) => object.role === "pdfium" ? 0 : 1;
      return rank(left) - rank(right);
    });
    for (const [index, object] of signingOrder.entries()) {
      const entitlementsPath = resolve(entitlementsDirectory, `${index}-${object.role}.plist`);
      await writeFile(entitlementsPath, entitlementPlist(object.entitlements), { flag: "wx" });
      run("codesign", [
        "--force", "--sign", identity,
        "--options", "runtime",
        "--timestamp",
        "--identifier", object.identifier,
        "--entitlements", entitlementsPath,
        resolve(app, object.path),
      ]);
    }

    const application = objects.find(({ role }) => role === "application");
    if (!application) throw new Error("native signing inventory has no application bundle identity");
    const appEntitlementsPath = resolve(entitlementsDirectory, "application-bundle.plist");
    await writeFile(appEntitlementsPath, entitlementPlist(application.entitlements), { flag: "wx" });
    run("codesign", [
      "--force", "--sign", identity,
      "--options", "runtime",
      "--timestamp",
      "--identifier", application.identifier,
      "--entitlements", appEntitlementsPath,
      app,
    ]);

    const archivePath = resolve(entitlementsDirectory, "Butter Paper notarisation.zip");
    run("/usr/bin/ditto", ["-c", "-k", "--keepParent", app, archivePath]);
    const notarisationOutput = run("xcrun", [
      "notarytool", "submit", archivePath,
      "--keychain-profile", notaryProfile,
      "--wait",
      "--output-format", "json",
    ]);
    const submissionId = requireAcceptedNotarisation(notarisationOutput);
    run("xcrun", ["stapler", "staple", app]);

    const verification = await verifySignedNativeMacosApp({
      appPath: app,
      manifestPath: resolve(manifestPath),
      fingerprint: validated.fingerprint,
      run,
    });
    const receipt = {
      schema: "butter-paper/signed-native-macos-production",
      version: 1,
      channel: verification.channel,
      target: verification.target,
      codeObjectCount: verification.codeObjectCount,
      identity,
      signingCertificateSha256: validated.fingerprint,
      manifestSha256: sha256(manifestBytes),
      notarisation: { status: "Accepted", submissionId },
      stapled: true,
      verified: true,
    };
    const receiptBytes = `${JSON.stringify(receipt, null, 2)}\n`;
    await writeFile(receiptDestination, receiptBytes, { flag: "wx" });
    return receipt;
  } finally {
    await rm(entitlementsDirectory, { recursive: true, force: true });
  }
}

function argumentMap(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    if (!argv[index]?.startsWith("--") || !argv[index + 1] || values.has(argv[index])) {
      throw new Error("usage: sign-notarize-macos-production.mjs --app APP --manifest FILE --identity IDENTITY --fingerprint SHA256 --notary-profile PROFILE --receipt FILE");
    }
    values.set(argv[index], argv[index + 1]);
  }
  const required = ["--app", "--manifest", "--identity", "--fingerprint", "--notary-profile", "--receipt"];
  if (values.size !== required.length || required.some((key) => !values.has(key))) {
    throw new Error("all signing, notarisation, input, and receipt arguments are required");
  }
  return values;
}

if (process.argv[1] === scriptPath) {
  const values = argumentMap(process.argv.slice(2));
  const receipt = await signNotariseVerifyNativeMacosApp({
    appPath: resolve(values.get("--app")),
    manifestPath: resolve(values.get("--manifest")),
    identity: values.get("--identity"),
    fingerprint: values.get("--fingerprint"),
    notaryProfile: values.get("--notary-profile"),
    receiptPath: resolve(values.get("--receipt")),
  });
  process.stdout.write(`${JSON.stringify(receipt, null, 2)}\n`);
}
