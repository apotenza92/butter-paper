#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  chmod,
  copyFile,
  lstat,
  mkdir,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const repoRoot = resolve(dirname(scriptPath), "../../../..");
const cpuByTarget = new Map([
  ["aarch64-apple-darwin", 0x0100000c],
  ["x86_64-apple-darwin", 0x01000007],
]);
const licenseDestinations = [
  "allura-font.txt",
  "arimo-font.txt",
  "expo-google-fonts.txt",
  "noto-fonts.txt",
  "phone-helper-go.txt",
  "qrcp.txt",
  "roboto-mono-font.txt",
  "signature-pad.txt",
  "tinos-font.txt",
];

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

function safeRelativePath(value, label) {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    isAbsolute(value) ||
    /^[A-Za-z]:[\\/]/.test(value) ||
    value.includes("\\") ||
    value.split("/").some((part) => !part || part === "." || part === "..")
  ) {
    throw new Error(`${label} must be a safe relative path`);
  }
  return value;
}

function validateRecord(record, label) {
  exactKeys(record, ["path", "bytes", "sha256"], label);
  safeRelativePath(record.path, `${label}.path`);
  if (!Number.isSafeInteger(record.bytes) || record.bytes <= 0) {
    throw new Error(`${label}.bytes must be a positive safe integer`);
  }
  if (!/^[0-9a-f]{64}$/.test(record.sha256)) {
    throw new Error(`${label}.sha256 must be a lowercase SHA-256 digest`);
  }
}

function releaseContract(channel) {
  if (channel === "stable") {
    return {
      productName: "Butter Paper",
      bundleIdentifier: "com.butterpaper.desktop",
    };
  }
  if (channel === "beta") {
    return {
      productName: "Butter Paper Beta",
      bundleIdentifier: "com.butterpaper.desktop.beta",
    };
  }
  throw new Error("native assembly channel must be stable or beta");
}

export function validateNativeAssemblyManifest(manifest, packageVersion) {
  exactKeys(
    manifest,
    [
      "schemaVersion",
      "purpose",
      "channel",
      "target",
      "version",
      "buildVersion",
      "minimumSystemVersion",
      "pdfiumStageReceiptSha256",
      "artifacts",
      "licenses",
    ],
    "native assembly manifest",
  );
  if (
    manifest.schemaVersion !== 1 ||
    manifest.purpose !== "unsigned-native-macos-production-assembly"
  ) {
    throw new Error("unsupported native assembly manifest schema");
  }
  releaseContract(manifest.channel);
  if (!cpuByTarget.has(manifest.target)) {
    throw new Error("native assembly target must be a macOS release target");
  }
  if (manifest.version !== packageVersion) {
    throw new Error("native assembly version must match the root package version");
  }
  if (!/^[0-9]+$/.test(manifest.buildVersion)) {
    throw new Error("native assembly buildVersion must contain only digits");
  }
  if (!/^(?:1[0-9]|2[0-9])\.[0-9]+$/.test(manifest.minimumSystemVersion)) {
    throw new Error("native assembly minimumSystemVersion is invalid");
  }
  if (!/^[0-9a-f]{64}$/.test(manifest.pdfiumStageReceiptSha256)) {
    throw new Error("native assembly PDFium receipt digest is invalid");
  }
  exactKeys(
    manifest.artifacts,
    [
      "application",
      "worker",
      "cameraHelper",
      "phoneHelper",
      "icon",
      "iconAssetCatalog",
      "thirdPartyNotices",
    ],
    "native assembly artifacts",
  );
  for (const [name, record] of Object.entries(manifest.artifacts)) {
    validateRecord(record, `artifacts.${name}`);
  }
  if (!Array.isArray(manifest.licenses) || manifest.licenses.length !== licenseDestinations.length) {
    throw new Error("native assembly license inventory is incomplete");
  }
  const destinations = [];
  for (const [index, license] of manifest.licenses.entries()) {
    exactKeys(license, ["destination", "path", "bytes", "sha256"], `licenses[${index}]`);
    if (!licenseDestinations.includes(license.destination)) {
      throw new Error("native assembly license destination is unsupported");
    }
    destinations.push(license.destination);
    validateRecord(
      { path: license.path, bytes: license.bytes, sha256: license.sha256 },
      `licenses[${index}]`,
    );
  }
  if (JSON.stringify(destinations.sort()) !== JSON.stringify([...licenseDestinations].sort())) {
    throw new Error("native assembly license inventory is incomplete or duplicated");
  }
  return manifest;
}

async function verifiedFile(root, record, label, {
  executable = false,
  machoTarget,
  maximumMacosVersion,
  machoFileType,
} = {}) {
  const path = resolve(root, safeRelativePath(record.path, `${label}.path`));
  const within = relative(root, path);
  if (!within || within.startsWith("..") || isAbsolute(within)) {
    throw new Error(`${label} escapes its input root`);
  }
  let current = root;
  for (const part of record.path.split("/")) {
    current = join(current, part);
    const metadata = await lstat(current);
    if (metadata.isSymbolicLink()) throw new Error(`${label} must not traverse a symlink`);
  }
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.nlink !== 1) {
    throw new Error(`${label} must be a regular single-link file`);
  }
  if (executable && (metadata.mode & 0o111) === 0) {
    throw new Error(`${label} must be executable`);
  }
  const bytes = await readFile(path);
  if (bytes.length !== record.bytes || sha256(bytes) !== record.sha256) {
    throw new Error(`${label} does not match its input receipt`);
  }
  if (machoTarget) {
    validateNativeMachO(bytes, machoTarget, maximumMacosVersion, label, machoFileType);
  }
  return { path, bytes };
}

export function validateNativeMachO(bytes, target, maximumMacosVersion, label, expectedFileType) {
  if (bytes.length < 32 || bytes.readUInt32LE(0) !== 0xfeedfacf) {
    throw new Error(`${label} must be a thin 64-bit Mach-O`);
  }
  if (bytes.readUInt32LE(4) !== cpuByTarget.get(target)) {
    throw new Error(`${label} architecture does not match ${target}`);
  }
  if (expectedFileType !== undefined && bytes.readUInt32LE(12) !== expectedFileType) {
    throw new Error(`${label} has the wrong Mach-O file type`);
  }
  const commandCount = bytes.readUInt32LE(16);
  const commandBytes = bytes.readUInt32LE(20);
  if (commandCount > 4096 || commandBytes > bytes.length - 32) {
    throw new Error(`${label} has malformed Mach-O load commands`);
  }
  const versions = [];
  let offset = 32;
  const commandEnd = offset + commandBytes;
  for (let index = 0; index < commandCount; index += 1) {
    if (offset + 8 > commandEnd) throw new Error(`${label} has malformed Mach-O load commands`);
    const command = bytes.readUInt32LE(offset);
    const size = bytes.readUInt32LE(offset + 4);
    if (size < 8 || offset + size > commandEnd) {
      throw new Error(`${label} has malformed Mach-O load commands`);
    }
    if (command === 0x32) {
      if (size < 24 || bytes.readUInt32LE(offset + 8) !== 1) {
        throw new Error(`${label} has an invalid macOS build-version command`);
      }
      versions.push(bytes.readUInt32LE(offset + 12));
    } else if (command === 0x24) {
      if (size < 16) throw new Error(`${label} has an invalid minimum-macOS command`);
      versions.push(bytes.readUInt32LE(offset + 8));
    }
    offset += size;
  }
  if (offset !== commandEnd || versions.length !== 1) {
    throw new Error(`${label} must declare exactly one minimum macOS version`);
  }
  const maximum = encodedMacosVersion(maximumMacosVersion);
  if (versions[0] > maximum) {
    throw new Error(
      `${label} requires macOS ${formattedMacosVersion(versions[0])} but the package declares ${maximumMacosVersion}`,
    );
  }
}

function encodedMacosVersion(value) {
  const [major, minor] = value.split(".").map(Number);
  return (major << 16) | (minor << 8);
}

function formattedMacosVersion(value) {
  const major = value >>> 16;
  const minor = (value >>> 8) & 0xff;
  const patch = value & 0xff;
  return patch === 0 ? `${major}.${minor}` : `${major}.${minor}.${patch}`;
}

async function inventory(root, prefix = "") {
  const files = [];
  for (const entry of await readdir(resolve(root, prefix), { withFileTypes: true })) {
    const child = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isSymbolicLink()) throw new Error(`input inventory contains symlink ${child}`);
    if (entry.isDirectory()) files.push(...(await inventory(root, child)));
    else if (entry.isFile()) {
      const metadata = await lstat(resolve(root, child));
      if (metadata.nlink !== 1) throw new Error(`input inventory contains hard link ${child}`);
      files.push(child);
    } else throw new Error(`input inventory contains special file ${child}`);
  }
  return files.sort();
}

async function validatePdfiumStage(stageRoot, expectedTarget, expectedDigest, maximumMacosVersion) {
  const receiptPath = join(stageRoot, "Resources/PDFium/receipt.json");
  const receiptBytes = await readFile(receiptPath);
  if (sha256(receiptBytes) !== expectedDigest) {
    throw new Error("production PDFium stage receipt digest changed");
  }
  const receipt = JSON.parse(receiptBytes);
  exactKeys(receipt, ["schema", "version", "target", "apiBuild", "sourceRevision", "manifestSha256", "files"], "PDFium stage receipt");
  if (
    receipt.schema !== "butter-paper/pdfium-production-stage" ||
    receipt.version !== 1 ||
    receipt.target !== expectedTarget ||
    receipt.apiBuild !== 7881 ||
    !/^[0-9a-f]{40}$/.test(receipt.sourceRevision) ||
    !/^[0-9a-f]{64}$/.test(receipt.manifestSha256) ||
    !Array.isArray(receipt.files)
  ) {
    throw new Error("production PDFium stage receipt is invalid");
  }
  const paths = [];
  const verified = [];
  for (const [index, record] of receipt.files.entries()) {
    exactKeys(record, ["file", "bytes", "sha256"], `PDFium files[${index}]`);
    const normalized = { path: record.file, bytes: record.bytes, sha256: record.sha256 };
    validateRecord(normalized, `PDFium files[${index}]`);
    paths.push(record.file);
    verified.push(await verifiedFile(stageRoot, normalized, `PDFium ${record.file}`, {
      machoTarget: record.file === "Frameworks/libpdfium.dylib" ? expectedTarget : undefined,
      maximumMacosVersion: record.file === "Frameworks/libpdfium.dylib" ? maximumMacosVersion : undefined,
      machoFileType: record.file === "Frameworks/libpdfium.dylib" ? 6 : undefined,
    }));
  }
  if (new Set(paths).size !== paths.length || !paths.includes("Frameworks/libpdfium.dylib")) {
    throw new Error("production PDFium stage inventory is incomplete or duplicated");
  }
  const actual = await inventory(stageRoot);
  const expected = [...paths, "Resources/PDFium/receipt.json"].sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error("production PDFium stage has missing or extra files");
  }
  return { receiptPath, files: paths.map((file, index) => ({ file, source: verified[index].path })) };
}

function escapeXml(value) {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

function infoPlist(manifest, contract) {
  const values = {
    CFBundleDisplayName: contract.productName,
    CFBundleExecutable: contract.productName,
    CFBundleIconFile: "icon.icns",
    CFBundleIconName: "Icon",
    CFBundleIdentifier: contract.bundleIdentifier,
    CFBundleName: contract.productName,
    // macOS wants X.Y.Z here; a beta is told apart by its build number.
    CFBundleShortVersionString: manifest.version.replace(/-beta\.[1-9][0-9]*$/, ""),
    CFBundleVersion: manifest.buildVersion,
    LSMinimumSystemVersion: manifest.minimumSystemVersion,
  };
  const entries = Object.entries(values)
    .map(([key, value]) => `  <key>${key}</key>\n  <string>${escapeXml(value)}</string>`)
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">\n<dict>\n  <key>CFBundleDevelopmentRegion</key>\n  <string>en</string>\n${entries}\n  <key>CFBundleDocumentTypes</key>\n  <array>\n    <dict>\n      <key>CFBundleTypeName</key>\n      <string>PDF Document</string>\n      <key>CFBundleTypeRole</key>\n      <string>Editor</string>\n      <key>LSHandlerRank</key>\n      <string>Alternate</string>\n      <key>LSItemContentTypes</key>\n      <array>\n        <string>com.adobe.pdf</string>\n      </array>\n    </dict>\n  </array>\n  <key>CFBundleInfoDictionaryVersion</key>\n  <string>6.0</string>\n  <key>CFBundlePackageType</key>\n  <string>APPL</string>\n  <key>NSHighResolutionCapable</key>\n  <true/>\n  <key>NSCameraUsageDescription</key>\n  <string>Butter Paper uses the camera only when you choose to take a signature photo.</string>\n  <key>NSLocalNetworkUsageDescription</key>\n  <string>Butter Paper uses your local network only when you choose to transfer a signature from your phone.</string>\n</dict>\n</plist>\n`;
}

async function verifyUnsignedApp(appPath, manifestBytes, manifest) {
  const destination = resolve(appPath);
  const contract = releaseContract(manifest.channel);
  const receiptPath = join(
    destination,
    "Contents/Resources/native-assembly-receipt.json",
  );
  const receiptBytes = await readFile(receiptPath);
  const receipt = JSON.parse(receiptBytes);
  exactKeys(
    receipt,
    [
      "schema",
      "version",
      "signed",
      "channel",
      "target",
      "productName",
      "bundleIdentifier",
      "applicationVersion",
      "buildVersion",
      "minimumSystemVersion",
      "inputManifestSha256",
      "pdfiumStageReceiptSha256",
      "files",
    ],
    "native assembly receipt",
  );
  if (
    receipt.schema !== "butter-paper/unsigned-native-macos-app" ||
    receipt.version !== 1 ||
    receipt.signed !== false ||
    receipt.channel !== manifest.channel ||
    receipt.target !== manifest.target ||
    receipt.productName !== contract.productName ||
    receipt.bundleIdentifier !== contract.bundleIdentifier ||
    receipt.applicationVersion !== manifest.version ||
    receipt.buildVersion !== manifest.buildVersion ||
    receipt.minimumSystemVersion !== manifest.minimumSystemVersion ||
    receipt.inputManifestSha256 !== sha256(manifestBytes) ||
    receipt.pdfiumStageReceiptSha256 !== manifest.pdfiumStageReceiptSha256 ||
    !Array.isArray(receipt.files)
  ) {
    throw new Error("native assembly receipt does not match its manifest");
  }
  const receiptRecords = new Map();
  for (const [index, record] of receipt.files.entries()) {
    exactKeys(record, ["file", "bytes", "sha256"], `native receipt files[${index}]`);
    const normalized = { path: record.file, bytes: record.bytes, sha256: record.sha256 };
    validateRecord(normalized, `native receipt files[${index}]`);
    if (receiptRecords.has(record.file)) {
      throw new Error("native assembly receipt contains duplicate files");
    }
    receiptRecords.set(record.file, normalized);
  }
  const pdfiumReceiptBytes = await readFile(
    join(destination, "Contents/Resources/PDFium/receipt.json"),
  );
  if (sha256(pdfiumReceiptBytes) !== manifest.pdfiumStageReceiptSha256) {
    throw new Error("assembled PDFium receipt changed");
  }
  const pdfiumReceipt = JSON.parse(pdfiumReceiptBytes);
  if (
    pdfiumReceipt.schema !== "butter-paper/pdfium-production-stage" ||
    pdfiumReceipt.version !== 1 ||
    pdfiumReceipt.target !== manifest.target ||
    !Array.isArray(pdfiumReceipt.files)
  ) {
    throw new Error("assembled PDFium receipt is invalid");
  }
  const expectedFiles = [
    "Contents/Info.plist",
    `Contents/MacOS/${contract.productName}`,
    "Contents/MacOS/butter-paper-pdf-worker",
    "Contents/MacOS/butter-paper-signature-camera",
    "Contents/MacOS/butter-paper-signature-phone",
    "Contents/Resources/Assets.car",
    "Contents/Resources/icon.icns",
    "Contents/Resources/THIRD_PARTY_NOTICES.md",
    ...licenseDestinations.map(
      (name) => `Contents/Resources/Licenses/${name}`,
    ),
    ...pdfiumReceipt.files.map(({ file }) => `Contents/${safeRelativePath(file, "assembled PDFium file")}`),
    "Contents/Resources/PDFium/receipt.json",
  ].sort();
  if (
    new Set(expectedFiles).size !== expectedFiles.length ||
    JSON.stringify([...receiptRecords.keys()].sort()) !==
      JSON.stringify(expectedFiles)
  ) {
    throw new Error("native assembly receipt has missing or extra package files");
  }
  const expectedRecords = new Map([
    [
      `Contents/MacOS/${contract.productName}`,
      manifest.artifacts.application,
    ],
    ["Contents/MacOS/butter-paper-pdf-worker", manifest.artifacts.worker],
    [
      "Contents/MacOS/butter-paper-signature-camera",
      manifest.artifacts.cameraHelper,
    ],
    [
      "Contents/MacOS/butter-paper-signature-phone",
      manifest.artifacts.phoneHelper,
    ],
    ["Contents/Resources/icon.icns", manifest.artifacts.icon],
    ["Contents/Resources/Assets.car", manifest.artifacts.iconAssetCatalog],
    [
      "Contents/Resources/THIRD_PARTY_NOTICES.md",
      manifest.artifacts.thirdPartyNotices,
    ],
    ...manifest.licenses.map((license) => [
      `Contents/Resources/Licenses/${license.destination}`,
      license,
    ]),
    ...pdfiumReceipt.files.map((record) => [
      `Contents/${record.file}`,
      record,
    ]),
    [
      "Contents/Resources/PDFium/receipt.json",
      { bytes: pdfiumReceiptBytes.length, sha256: manifest.pdfiumStageReceiptSha256 },
    ],
  ]);
  const expectedPlist = Buffer.from(infoPlist(manifest, contract));
  expectedRecords.set("Contents/Info.plist", {
    bytes: expectedPlist.length,
    sha256: sha256(expectedPlist),
  });
  for (const [file, expected] of expectedRecords) {
    const actualRecord = receiptRecords.get(file);
    if (
      !actualRecord ||
      actualRecord.bytes !== expected.bytes ||
      actualRecord.sha256 !== expected.sha256
    ) {
      throw new Error(`native assembly receipt changed ${file}`);
    }
  }
  const actual = await inventory(destination);
  if (
    JSON.stringify(actual) !==
    JSON.stringify(
      [...expectedFiles, "Contents/Resources/native-assembly-receipt.json"].sort(),
    )
  ) {
    throw new Error("unsigned native app has missing or extra files");
  }
  const executableFiles = new Set([
    `Contents/MacOS/${contract.productName}`,
    "Contents/MacOS/butter-paper-pdf-worker",
    "Contents/MacOS/butter-paper-signature-camera",
    "Contents/MacOS/butter-paper-signature-phone",
  ]);
  for (const [file, record] of receiptRecords) {
    await verifiedFile(destination, record, `assembled ${file}`, {
      executable: executableFiles.has(file),
      machoTarget:
        executableFiles.has(file) || file === "Contents/Frameworks/libpdfium.dylib"
          ? manifest.target
          : undefined,
      maximumMacosVersion:
        executableFiles.has(file) || file === "Contents/Frameworks/libpdfium.dylib"
          ? manifest.minimumSystemVersion
          : undefined,
      machoFileType: executableFiles.has(file)
        ? 2
        : file === "Contents/Frameworks/libpdfium.dylib"
          ? 6
          : undefined,
    });
  }
  if (
    (await readFile(join(destination, "Contents/Info.plist"), "utf8")) !==
    infoPlist(manifest, contract)
  ) {
    throw new Error("unsigned native app Info.plist changed");
  }
  return receipt;
}

export async function verifyUnsignedNativeMacosApp({ appPath, manifestPath }) {
  const packageVersion = JSON.parse(
    await readFile(join(repoRoot, "package.json"), "utf8"),
  ).version;
  const manifestBytes = await readFile(manifestPath);
  const manifest = validateNativeAssemblyManifest(
    JSON.parse(manifestBytes),
    packageVersion,
  );
  return verifyUnsignedApp(appPath, manifestBytes, manifest);
}

export async function assembleMacosProductionApp({ manifestPath, inputRoot, pdfiumStage, outputDirectory }) {
  const packageVersion = JSON.parse(await readFile(join(repoRoot, "package.json"), "utf8")).version;
  const manifestBytes = await readFile(manifestPath);
  const manifest = validateNativeAssemblyManifest(JSON.parse(manifestBytes), packageVersion);
  const contract = releaseContract(manifest.channel);
  const root = resolve(inputRoot);
  const rootMetadata = await lstat(root);
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw new Error("native assembly input root must be a real directory");
  }
  const expectedInputFiles = [
    ...Object.values(manifest.artifacts).map(({ path }) => path),
    ...manifest.licenses.map(({ path }) => path),
  ].sort();
  if (new Set(expectedInputFiles).size !== expectedInputFiles.length || JSON.stringify(await inventory(root)) !== JSON.stringify(expectedInputFiles)) {
    throw new Error("native assembly input root has missing, extra or duplicated files");
  }
  const executables = {};
  for (const name of ["application", "worker", "cameraHelper", "phoneHelper"]) {
    executables[name] = await verifiedFile(root, manifest.artifacts[name], `artifacts.${name}`, {
      executable: true,
      machoTarget: manifest.target,
      maximumMacosVersion: manifest.minimumSystemVersion,
      machoFileType: 2,
    });
  }
  const resources = {};
  for (const name of ["icon", "iconAssetCatalog", "thirdPartyNotices"]) {
    resources[name] = await verifiedFile(root, manifest.artifacts[name], `artifacts.${name}`);
  }
  const licenses = [];
  for (const license of manifest.licenses) {
    licenses.push({ destination: license.destination, ...(await verifiedFile(root, license, `license ${license.destination}`)) });
  }
  const pdfium = await validatePdfiumStage(
    resolve(pdfiumStage),
    manifest.target,
    manifest.pdfiumStageReceiptSha256,
    manifest.minimumSystemVersion,
  );

  const destination = resolve(outputDirectory);
  await mkdir(dirname(destination), { recursive: true });
  await mkdir(destination, { recursive: false, mode: 0o700 });
  let complete = false;
  const staged = [];
  try {
    const stage = async (source, relativePath, mode = 0o644) => {
      const target = join(destination, relativePath);
      await mkdir(dirname(target), { recursive: true });
      await copyFile(source, target);
      await chmod(target, mode);
      const bytes = await readFile(target);
      staged.push({ file: relativePath, bytes: bytes.length, sha256: sha256(bytes) });
    };
    await stage(executables.application.path, `Contents/MacOS/${contract.productName}`, 0o755);
    await stage(executables.worker.path, "Contents/MacOS/butter-paper-pdf-worker", 0o755);
    await stage(executables.cameraHelper.path, "Contents/MacOS/butter-paper-signature-camera", 0o755);
    await stage(executables.phoneHelper.path, "Contents/MacOS/butter-paper-signature-phone", 0o755);
    for (const file of pdfium.files) await stage(file.source, `Contents/${file.file}`, file.file.startsWith("Frameworks/") ? 0o755 : 0o644);
    await stage(pdfium.receiptPath, "Contents/Resources/PDFium/receipt.json");
    await stage(resources.icon.path, "Contents/Resources/icon.icns");
    await stage(resources.iconAssetCatalog.path, "Contents/Resources/Assets.car");
    await stage(resources.thirdPartyNotices.path, "Contents/Resources/THIRD_PARTY_NOTICES.md");
    for (const license of licenses) await stage(license.path, `Contents/Resources/Licenses/${license.destination}`);
    const plistPath = join(destination, "Contents/Info.plist");
    await mkdir(dirname(plistPath), { recursive: true });
    await writeFile(plistPath, infoPlist(manifest, contract), { flag: "wx", mode: 0o644 });
    const plistBytes = await readFile(plistPath);
    staged.push({ file: "Contents/Info.plist", bytes: plistBytes.length, sha256: sha256(plistBytes) });
    staged.sort((a, b) => a.file.localeCompare(b.file));
    const receipt = {
      schema: "butter-paper/unsigned-native-macos-app",
      version: 1,
      signed: false,
      channel: manifest.channel,
      target: manifest.target,
      productName: contract.productName,
      bundleIdentifier: contract.bundleIdentifier,
      applicationVersion: manifest.version,
      buildVersion: manifest.buildVersion,
      minimumSystemVersion: manifest.minimumSystemVersion,
      inputManifestSha256: sha256(manifestBytes),
      pdfiumStageReceiptSha256: manifest.pdfiumStageReceiptSha256,
      files: staged,
    };
    await writeFile(join(destination, "Contents/Resources/native-assembly-receipt.json"), `${JSON.stringify(receipt, null, 2)}\n`, { flag: "wx", mode: 0o644 });
    await verifyUnsignedApp(destination, manifestBytes, manifest);
    complete = true;
    return receipt;
  } finally {
    if (!complete) await rm(destination, { recursive: true, force: true });
  }
}

function argumentsMap(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    if (!argv[index]?.startsWith("--") || !argv[index + 1] || values.has(argv[index])) {
      throw new Error("usage: assemble-macos-production.mjs --manifest FILE --input-root DIR --pdfium-stage DIR --output-dir APP");
    }
    values.set(argv[index], argv[index + 1]);
  }
  for (const key of ["--manifest", "--input-root", "--pdfium-stage", "--output-dir"]) {
    if (!values.has(key)) throw new Error(`${key} is required`);
  }
  return values;
}

if (process.argv[1] === scriptPath) {
  const values = argumentsMap(process.argv.slice(2));
  const receipt = await assembleMacosProductionApp({
    manifestPath: resolve(values.get("--manifest")),
    inputRoot: resolve(values.get("--input-root")),
    pdfiumStage: resolve(values.get("--pdfium-stage")),
    outputDirectory: resolve(values.get("--output-dir")),
  });
  process.stdout.write(`${JSON.stringify(receipt, null, 2)}\n`);
}
