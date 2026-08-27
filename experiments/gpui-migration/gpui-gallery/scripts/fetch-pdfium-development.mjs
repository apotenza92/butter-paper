#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const galleryDirectory = resolve(scriptDirectory, "..");
const manifestPath = resolve(galleryDirectory, "pdfium-development-binaries.json");

export function sha256(contents) {
  return createHash("sha256").update(contents).digest("hex");
}

export function hostTarget(platform = process.platform, arch = process.arch) {
  const key = `${platform}/${arch}`;
  const targets = {
    "linux/arm64": "aarch64-unknown-linux-gnu",
    "linux/x64": "x86_64-unknown-linux-gnu",
    "darwin/arm64": "aarch64-apple-darwin",
    "darwin/x64": "x86_64-apple-darwin",
    "win32/arm64": "aarch64-pc-windows-msvc",
    "win32/x64": "x86_64-pc-windows-msvc",
  };
  if (!targets[key]) throw new Error(`unsupported development host ${key}`);
  return targets[key];
}

export function validateManifest(manifest) {
  if (
    manifest.schemaVersion !== 1 ||
    manifest.purpose !== "prototype-development-only" ||
    manifest.productionApproved !== false ||
    manifest.apiBuild !== 7881 ||
    manifest.wrapper?.version !== "0.9.4" ||
    manifest.wrapper?.revision !== "6cee8b9a3951832ac0ff62ce4c32800278001cb8" ||
    manifest.wrapper?.feature !== "pdfium_7881"
  ) {
    throw new Error("development PDFium manifest policy fields do not match the reviewed pins");
  }
  if (manifest.assets.length !== 6 || new Set(manifest.assets.map(({ target }) => target)).size !== 6) {
    throw new Error("development PDFium manifest must contain six unique supported targets");
  }
  for (const asset of manifest.assets) {
    if (
      !asset.url.startsWith("https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/7881/") ||
      !/^[0-9a-f]{64}$/.test(asset.sha256) ||
      !Number.isSafeInteger(asset.bytes) ||
      asset.bytes <= 0
    ) {
      throw new Error(`invalid development PDFium asset pin for ${asset.target}`);
    }
  }
}

export function assertArchiveEntriesSafe(entries) {
  for (const entry of entries.split("\n").filter(Boolean)) {
    if (entry.startsWith("/") || entry.split("/").includes("..") || entry.includes("\\")) {
      throw new Error(`unsafe archive entry: ${entry}`);
    }
  }
}

const receiptName = ".butter-paper-pdfium-development.json";

export function artifactReceipt(asset, apiBuild = 7881) {
  return {
    schemaVersion: 1,
    target: asset.target,
    archiveSha256: asset.sha256,
    archiveBytes: asset.bytes,
    apiBuild,
    library: asset.library,
  };
}

export function verifyExtractedArtifact(root, asset, apiBuild = 7881, requireReceipt = true) {
  if (!existsSync(resolve(root, asset.library))) {
    throw new Error(`archive is missing ${asset.library}`);
  }
  const versionPath = resolve(root, "VERSION");
  if (!existsSync(versionPath) || !readFileSync(versionPath, "utf8").includes(`BUILD=${apiBuild}\n`)) {
    throw new Error(`archive VERSION does not identify PDFium build ${apiBuild}`);
  }
  if (requireReceipt) {
    let receipt;
    try {
      receipt = JSON.parse(readFileSync(resolve(root, receiptName), "utf8"));
    } catch {
      throw new Error("extracted PDFium artifact has no valid verification receipt");
    }
    if (JSON.stringify(receipt) !== JSON.stringify(artifactReceipt(asset, apiBuild))) {
      throw new Error("extracted PDFium verification receipt does not match the reviewed archive pin");
    }
  }
}

function runTar(args) {
  const result = spawnSync("tar", args, { encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr || "tar failed");
  return result.stdout;
}

export async function fetchDevelopmentPdfium(target = hostTarget()) {
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  validateManifest(manifest);
  const asset = manifest.assets.find((candidate) => candidate.target === target);
  if (!asset) throw new Error(`unsupported PDFium development target ${target}`);

  const outputRoot = resolve(galleryDirectory, "target", "pdfium-development");
  const destination = resolve(outputRoot, target);
  if (!relative(outputRoot, destination) || relative(outputRoot, destination).startsWith("..")) {
    throw new Error("refusing an output path outside target/pdfium-development");
  }
  if (existsSync(destination)) {
    try {
      verifyExtractedArtifact(destination, asset, manifest.apiBuild);
      return resolve(destination, asset.library);
    } catch {
      // This directory is disposable ignored output. Replace old or unverifiable
      // extraction state rather than claiming provenance it cannot prove.
      rmSync(destination, { recursive: true, force: true });
    }
  }

  mkdirSync(outputRoot, { recursive: true });
  const staging = mkdtempSync(resolve(outputRoot, `.staging-${target}-`));
  try {
    const response = await fetch(asset.url, { redirect: "follow" });
    if (!response.ok) throw new Error(`download failed: HTTP ${response.status}`);
    const archive = Buffer.from(await response.arrayBuffer());
    if (archive.byteLength !== asset.bytes) {
      throw new Error(`size mismatch: expected ${asset.bytes}, received ${archive.byteLength}`);
    }
    const actualHash = sha256(archive);
    if (actualHash !== asset.sha256) {
      throw new Error(`SHA-256 mismatch: expected ${asset.sha256}, received ${actualHash}`);
    }
    const archivePath = resolve(staging, asset.name);
    writeFileSync(archivePath, archive, { flag: "wx" });
    assertArchiveEntriesSafe(runTar(["-tzf", archivePath]));
    const extracted = resolve(staging, "extracted");
    mkdirSync(extracted);
    runTar(["-xzf", archivePath, "-C", extracted]);
    verifyExtractedArtifact(extracted, asset, manifest.apiBuild, false);
    writeFileSync(
      resolve(extracted, receiptName),
      `${JSON.stringify(artifactReceipt(asset, manifest.apiBuild), null, 2)}\n`,
      { flag: "wx" },
    );
    verifyExtractedArtifact(extracted, asset, manifest.apiBuild);
    renameSync(extracted, destination);
    return resolve(destination, asset.library);
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const target = process.argv[2] ?? hostTarget();
  const library = await fetchDevelopmentPdfium(target);
  process.stdout.write(`${library}\n`);
}
