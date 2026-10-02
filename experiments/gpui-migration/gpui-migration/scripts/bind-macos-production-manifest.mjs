#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { validateNativeAssemblyManifest } from "./assemble-macos-production.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const repoRoot = resolve(dirname(scriptPath), "../../../..");
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function exactKeys(value, expected, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) {
    throw new Error(`${label} has missing or unknown fields`);
  }
}

function validateInputReceipt(receipt, packageVersion) {
  exactKeys(
    receipt,
    [
      "schema",
      "version",
      "channel",
      "target",
      "applicationVersion",
      "buildVersion",
      "minimumSystemVersion",
      "readyForAssembly",
      "blockedOn",
      "artifacts",
      "licenses",
    ],
    "native production input receipt",
  );
  if (
    receipt.schema !== "butter-paper/native-macos-production-inputs" ||
    receipt.version !== 1 ||
    (receipt.channel !== "stable" && receipt.channel !== "beta") ||
    receipt.applicationVersion !== packageVersion ||
    receipt.readyForAssembly !== false ||
    JSON.stringify(receipt.blockedOn) !==
      JSON.stringify(["production-pdfium-stage-receipt"])
  ) {
    throw new Error("native production input receipt is invalid");
  }
  return receipt;
}

async function readPdfiumReceipt(stage, target) {
  const root = resolve(stage);
  const metadata = await lstat(root);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw new Error("production PDFium stage must be a real directory");
  }
  const bytes = await readFile(join(root, "Resources/PDFium/receipt.json"));
  const receipt = JSON.parse(bytes);
  if (
    receipt?.schema !== "butter-paper/pdfium-production-stage" ||
    receipt?.version !== 1 ||
    receipt?.target !== target ||
    !Array.isArray(receipt?.files) ||
    !receipt.files.some(({ file }) => file === "Frameworks/libpdfium.dylib")
  ) {
    throw new Error(
      "production PDFium stage receipt is invalid or targets another architecture",
    );
  }
  return bytes;
}

export async function bindMacosProductionManifest({
  inputReceiptPath,
  pdfiumStage,
  manifestPath,
}) {
  const packageVersion = JSON.parse(
    await readFile(join(repoRoot, "package.json"), "utf8"),
  ).version;
  const inputReceipt = validateInputReceipt(
    JSON.parse(await readFile(inputReceiptPath)),
    packageVersion,
  );
  const pdfiumReceiptBytes = await readPdfiumReceipt(
    pdfiumStage,
    inputReceipt.target,
  );
  const manifest = {
    schemaVersion: 1,
    purpose: "unsigned-native-macos-production-assembly",
    channel: inputReceipt.channel,
    target: inputReceipt.target,
    version: inputReceipt.applicationVersion,
    buildVersion: inputReceipt.buildVersion,
    minimumSystemVersion: inputReceipt.minimumSystemVersion,
    pdfiumStageReceiptSha256: sha256(pdfiumReceiptBytes),
    artifacts: inputReceipt.artifacts,
    licenses: inputReceipt.licenses,
  };
  validateNativeAssemblyManifest(manifest, packageVersion);
  const destination = resolve(manifestPath);
  await mkdir(dirname(destination), { recursive: true });
  await writeFile(destination, `${JSON.stringify(manifest, null, 2)}\n`, {
    flag: "wx",
    mode: 0o644,
  });
  return manifest;
}

function argumentsMap(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    if (
      !argv[index]?.startsWith("--") ||
      !argv[index + 1] ||
      values.has(argv[index])
    ) {
      throw new Error(
        "usage: bind-macos-production-manifest.mjs --input-receipt FILE --pdfium-stage DIR --manifest FILE",
      );
    }
    values.set(argv[index], argv[index + 1]);
  }
  for (const key of ["--input-receipt", "--pdfium-stage", "--manifest"]) {
    if (!values.has(key)) throw new Error(`${key} is required`);
  }
  return values;
}

if (process.argv[1] === scriptPath) {
  const values = argumentsMap(process.argv.slice(2));
  const manifest = await bindMacosProductionManifest({
    inputReceiptPath: resolve(values.get("--input-receipt")),
    pdfiumStage: resolve(values.get("--pdfium-stage")),
    manifestPath: resolve(values.get("--manifest")),
  });
  process.stdout.write(`${JSON.stringify(manifest, null, 2)}\n`);
}
