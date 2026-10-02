#!/usr/bin/env node

import { createHash, randomUUID } from "node:crypto";
import {
  link,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import { basename, isAbsolute, join, relative, resolve, sep } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { nativeSigningCertificateSha256, verifySignedNativeMacosApp } from "./verify-signed-macos-production.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const defaultDitto = "/usr/bin/ditto";
const targetNames = new Map([
  ["aarch64-apple-darwin", "macos-arm64"],
  ["x86_64-apple-darwin", "macos-x64"],
]);
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function fail(message) { throw new Error(message); }

export function defaultRun(command, args, { input } = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    input,
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed (${result.status}): ${`${result.stdout ?? ""}${result.stderr ?? ""}`.trim()}`);
  return `${result.stdout ?? ""}${result.stderr ?? ""}`;
}

function readJson(bytes, label) {
  try { return JSON.parse(bytes.toString("utf8")); }
  catch { fail(`${label} must contain valid JSON`); }
}

function outside(path, directory) {
  const rel = relative(directory, path);
  return rel === ".." || rel.startsWith(`..${sep}`) || isAbsolute(rel);
}

export async function packageSignedMacosProduction({
  appPath,
  signingReceiptPath,
  manifestPath,
  outputDir,
  sourceRevision,
  fingerprint = nativeSigningCertificateSha256,
  dittoPath = defaultDitto,
  run = defaultRun,
  verify = verifySignedNativeMacosApp,
}) {
  for (const [name, value] of Object.entries({ appPath, signingReceiptPath, manifestPath, outputDir, sourceRevision })) {
    if (typeof value !== "string" || !value.trim()) fail(`${name} is required`);
  }
  if (!/^[0-9a-f]{40}$/.test(sourceRevision)) fail("sourceRevision must be a full lowercase Git commit");
  const trustedFingerprint = String(fingerprint).replace(/[^0-9a-f]/gi, "").toUpperCase();
  if (!/^[0-9A-F]{64}$/.test(trustedFingerprint)) fail("trusted signing fingerprint must be a SHA-256 digest");

  const app = resolve(appPath);
  const manifestFile = resolve(manifestPath);
  const receiptFile = resolve(signingReceiptPath);
  const output = resolve(outputDir);
  const manifestBytes = await readFile(manifestFile);
  const receiptBytes = await readFile(receiptFile);
  const manifest = readJson(manifestBytes, "native assembly manifest");
  const signingReceipt = readJson(receiptBytes, "signed app receipt");
  const channel = manifest.channel;
  if (channel !== "stable" && channel !== "beta") fail("signed macOS production apps must be stable or beta");
  const platform = targetNames.get(manifest.target);
  if (!platform) fail("native assembly target must be a supported macOS release target");
  // Beta packages install Butter Paper Beta beside the stable app.
  const target = channel === "beta" ? `${platform}-beta` : platform;
  const fileStem = `butter-paper-${target}`;
  const version = manifest.version;
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\+([0-9A-Za-z.-]+))?$/.test(version ?? "")) fail("version must be stable semver");
  const appName = channel === "beta" ? "Butter Paper Beta.app" : "Butter Paper.app";
  if (basename(app) !== appName) fail(`signed app filename does not match the ${channel} channel`);
  if (
    signingReceipt.schema !== "butter-paper/signed-native-macos-production" || signingReceipt.version !== 1 ||
    signingReceipt.channel !== channel || signingReceipt.target !== manifest.target ||
    signingReceipt.identity !== "Developer ID Application: Alexander Potenza (27JL2VERNC)" ||
    signingReceipt.signingCertificateSha256 !== trustedFingerprint ||
    signingReceipt.codeObjectCount !== 5 ||
    signingReceipt.manifestSha256 !== sha256(manifestBytes) || signingReceipt.notarisation?.status !== "Accepted" ||
    typeof signingReceipt.notarisation?.submissionId !== "string" || !signingReceipt.notarisation.submissionId ||
    signingReceipt.stapled !== true || signingReceipt.verified !== true
  ) fail(`signing receipt does not match the ${channel} assembly manifest and trusted notarised identity`);

  await mkdir(output, { recursive: true });
  const realApp = await realpath(app);
  const realOutput = await realpath(output);
  if (!outside(realOutput, realApp)) fail("output directory must be outside the signed app bundle");
  const archive = join(realOutput, `${fileStem}-${version}.zip`);
  const packageManifestPath = join(realOutput, `${fileStem}-${version}.package.json`);
  const verificationReceiptPath = join(realOutput, `${fileStem}-${version}.verification.json`);
  const destinations = [archive, packageManifestPath, verificationReceiptPath];
  for (const path of destinations) {
    try { await lstat(path); fail(`package output already exists: ${path}`); }
    catch (error) { if (error?.code !== "ENOENT") throw error; }
  }

  const temporaryRoot = await mkdtemp(join(tmpdir(), "bp-signed-macos-package-"));
  const pending = destinations.map((path) => `${path}.tmp-${process.pid}-${randomUUID()}`);
  const installed = [];
  try {
    const verification = await verify({ appPath: app, manifestPath: manifestFile, fingerprint: trustedFingerprint, run });
    if (verification.channel !== channel || verification.target !== manifest.target || verification.codeObjectCount !== signingReceipt.codeObjectCount) fail("strict signed verifier returned mismatched channel, target, or code-object inventory");
    await run(dittoPath, ["-c", "-k", "--keepParent", app, pending[0]]);
    const extractedRoot = join(temporaryRoot, "extracted");
    await mkdir(extractedRoot, { mode: 0o700 });
    await run(dittoPath, ["-x", "-k", pending[0], extractedRoot]);
    const extractedApp = join(extractedRoot, appName);
    const extractedVerification = await verify({ appPath: extractedApp, manifestPath: manifestFile, fingerprint: trustedFingerprint, run });
    if (extractedVerification.channel !== channel || extractedVerification.target !== manifest.target || extractedVerification.codeObjectCount !== signingReceipt.codeObjectCount) fail("extracted signed verifier returned mismatched channel, target, or code-object inventory");

    const archiveBytes = await readFile(pending[0]);
    if (archiveBytes.length === 0) fail("release ZIP is empty");
    const artifact = { path: basename(archive), bytes: archiveBytes.length, sha256: sha256(archiveBytes) };
    const identity = { target, channel, version, sourceRevision };
    const packageManifest = {
      schema: "butter-paper/package-manifest", schemaVersion: 1, ...identity, artifact,
      archiveFormat: "zip", appBundle: appName, assemblyManifestSha256: sha256(manifestBytes),
      signingReceiptSha256: sha256(receiptBytes), signingCertificateSha256: trustedFingerprint,
      notarisation: signingReceipt.notarisation, stapled: true,
    };
    const verificationReceipt = {
      schema: "butter-paper/package-verification", schemaVersion: 1, ...identity, artifact,
      verified: true, verification: "strict-signed-macos-production",
      signingCertificateSha256: trustedFingerprint, assemblyManifestSha256: sha256(manifestBytes),
      signingReceiptSha256: sha256(receiptBytes), notarisation: signingReceipt.notarisation,
      stapled: true, extractedAppVerified: true,
    };
    await writeFile(pending[1], `${JSON.stringify(packageManifest, null, 2)}\n`, { flag: "wx", mode: 0o644 });
    await writeFile(pending[2], `${JSON.stringify(verificationReceipt, null, 2)}\n`, { flag: "wx", mode: 0o644 });
    for (let index = 0; index < destinations.length; index += 1) {
      await link(pending[index], destinations[index]);
      installed.push(destinations[index]);
    }
    return { archive, packageManifestPath, verificationReceiptPath, artifact, packageManifest, verificationReceipt };
  } catch (error) {
    await Promise.all(installed.map((path) => rm(path, { force: true })));
    throw error;
  } finally {
    await Promise.all(pending.map((path) => rm(path, { force: true })));
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

function argumentsMap(args) {
  const values = new Map();
  for (let index = 0; index < args.length; index += 2) {
    if (!args[index]?.startsWith("--") || !args[index + 1] || values.has(args[index])) fail("usage: package-macos-signed-production.mjs --app APP --receipt FILE --manifest FILE --output DIR --revision GIT_SHA [--fingerprint SHA256] [--ditto PATH]");
    values.set(args[index], args[index + 1]);
  }
  for (const key of ["--app", "--receipt", "--manifest", "--output", "--revision"]) if (!values.has(key)) fail(`${key} is required`);
  for (const key of values.keys()) if (!["--app", "--receipt", "--manifest", "--output", "--revision", "--fingerprint", "--ditto"].includes(key)) fail(`unknown argument ${key}`);
  return values;
}

if (process.argv[1] === scriptPath) {
  const args = argumentsMap(process.argv.slice(2));
  packageSignedMacosProduction({
    appPath: resolve(args.get("--app")), signingReceiptPath: resolve(args.get("--receipt")),
    manifestPath: resolve(args.get("--manifest")), outputDir: resolve(args.get("--output")),
    sourceRevision: args.get("--revision"), fingerprint: args.get("--fingerprint"), dittoPath: args.get("--ditto") ?? defaultDitto,
  }).then((result) => process.stdout.write(`${JSON.stringify(result, null, 2)}\n`)).catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
