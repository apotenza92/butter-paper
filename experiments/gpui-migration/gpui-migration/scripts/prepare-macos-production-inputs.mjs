#!/usr/bin/env node

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmod,
  copyFile,
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import {
  basename,
  dirname,
  isAbsolute,
  join,
  parse,
  relative,
  resolve,
  sep,
} from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { validateNativeMachO } from "./assemble-macos-production.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const migrationRoot = resolve(dirname(scriptPath), "..");
const repoRoot = resolve(migrationRoot, "../../..");
const macosIconComposerSource = join(
  repoRoot,
  "assets/app/macos/Butter Paper.icon",
);
const macosIconName = "Icon";
const supportedTargets = new Set([
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
]);
const requiredSupportingEntries = [
  ["allura-font.txt", "licenses/allura-font.txt"],
  ["arimo-font.txt", "licenses/arimo-font.txt"],
  ["expo-google-fonts.txt", "licenses/expo-google-fonts.txt"],
  ["noto-fonts.txt", "licenses/noto-fonts.txt"],
  ["phone-helper-go.txt", "licenses/phone-helper-go.txt"],
  ["qrcp.txt", "licenses/qrcp.txt"],
  ["roboto-mono-font.txt", "licenses/roboto-mono-font.txt"],
  ["signature-pad.txt", "licenses/signature-pad.txt"],
  ["tinos-font.txt", "licenses/tinos-font.txt"],
  ["thirdPartyNotices", "resources/THIRD_PARTY_NOTICES.md"],
  ["icon", "resources/icon.icns"],
  ["iconAssetCatalog", "resources/Assets.car"],
].sort((left, right) => left[1].localeCompare(right[1]));

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

// macOS 26 only draws the system icon background and full-size artwork from a
// compiled Icon Composer catalog; a bare icon.icns is shrunk onto a grey tile.
export async function compileMacosIconAssetCatalog({
  iconPath = macosIconComposerSource,
  outputDirectory,
  execute = promisify(execFile),
}) {
  const output = resolve(outputDirectory);
  await mkdir(output, { recursive: true, mode: 0o700 });
  const source = join(output, `${macosIconName}.icon`);
  await cp(iconPath, source, { recursive: true });
  const partialInfoPlist = join(output, "assetcatalog_generated_info.plist");
  await execute("xcrun", [
    "actool",
    source,
    "--compile",
    output,
    "--output-format",
    "human-readable-text",
    "--output-partial-info-plist",
    partialInfoPlist,
    "--app-icon",
    macosIconName,
    "--include-all-app-icons",
    "--target-device",
    "mac",
    "--minimum-deployment-target",
    "26.0",
    "--platform",
    "macosx",
  ]);
  const generatedInfo = await readFile(partialInfoPlist, "utf8");
  if (
    !new RegExp(
      `<key>CFBundleIconName</key>\\s*<string>${macosIconName}</string>`,
    ).test(generatedInfo)
  ) {
    throw new Error("actool did not compile the macOS application icon");
  }
  return join(output, "Assets.car");
}

function defaultSupportingFiles(iconAssetCatalogPath) {
  const phoneRoot = join(repoRoot, "experiments/phone-signature-prototype");
  return [
    [
      "icon",
      "resources/icon.icns",
      join(repoRoot, "assets/app/icon.icns"),
    ],
    ["iconAssetCatalog", "resources/Assets.car", iconAssetCatalogPath],
    [
      "thirdPartyNotices",
      "resources/THIRD_PARTY_NOTICES.md",
      join(migrationRoot, "THIRD_PARTY_NOTICES.md"),
    ],
    [
      "allura-font.txt",
      "licenses/allura-font.txt",
      join(migrationRoot, "assets/fonts/Allura-OFL.txt"),
    ],
    [
      "arimo-font.txt",
      "licenses/arimo-font.txt",
      join(migrationRoot, "assets/fonts/Arimo-OFL.txt"),
    ],
    [
      "expo-google-fonts.txt",
      "licenses/expo-google-fonts.txt",
      join(migrationRoot, "assets/fonts/Expo-Google-Fonts-MIT.txt"),
    ],
    [
      "noto-fonts.txt",
      "licenses/noto-fonts.txt",
      join(
        migrationRoot,
        ".prepared/gpui-component-c27f5d5c/crates/story-web/fonts/OFL.txt",
      ),
    ],
    ["qrcp.txt", "licenses/qrcp.txt", join(phoneRoot, "dist/QRCP_LICENSE")],
    [
      "phone-helper-go.txt",
      "licenses/phone-helper-go.txt",
      join(phoneRoot, "dist/PHONE_HELPER_THIRD_PARTY_NOTICES.md"),
    ],
    [
      "roboto-mono-font.txt",
      "licenses/roboto-mono-font.txt",
      join(migrationRoot, "assets/fonts/RobotoMono-OFL.txt"),
    ],
    [
      "signature-pad.txt",
      "licenses/signature-pad.txt",
      join(phoneRoot, "dist/package/LICENSE"),
    ],
    [
      "tinos-font.txt",
      "licenses/tinos-font.txt",
      join(migrationRoot, "assets/fonts/Tinos-OFL.txt"),
    ],
  ];
}

function safeOutputPath(root, path, label) {
  const destination = resolve(root, path);
  const within = relative(root, destination);
  if (!within || within.startsWith("..") || isAbsolute(within)) {
    throw new Error(`${label} escapes the output root`);
  }
  return destination;
}

async function verifiedSource(path, label) {
  const absolute = resolve(path);
  let current = parse(absolute).root;
  for (const component of relative(current, absolute).split(sep)) {
    current = join(current, component);
    if ((await lstat(current)).isSymbolicLink()) {
      throw new Error(`${label} must not traverse a symlink`);
    }
  }
  const metadata = await lstat(absolute);
  if (metadata.isSymbolicLink() || !metadata.isFile() || metadata.nlink !== 1) {
    throw new Error(`${label} must be a regular single-link file`);
  }
  return readFile(absolute);
}

async function inventory(root, prefix = "") {
  const files = [];
  for (const entry of await readdir(join(root, prefix), {
    withFileTypes: true,
  })) {
    const child = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isSymbolicLink())
      throw new Error(`input inventory contains symlink ${child}`);
    if (entry.isDirectory()) files.push(...(await inventory(root, child)));
    else if (entry.isFile()) {
      const metadata = await lstat(join(root, child));
      if (metadata.nlink !== 1)
        throw new Error(`input inventory contains hard link ${child}`);
      files.push(child);
    } else throw new Error(`input inventory contains special file ${child}`);
  }
  return files.sort();
}

export async function prepareMacosProductionInputs({
  applicationPath,
  workerPath,
  cameraHelperPath,
  phoneHelperPath,
  target,
  minimumSystemVersion,
  buildVersion,
  channel = "stable",
  outputRoot,
  receiptPath,
  supportingFiles,
}) {
  if (channel !== "stable" && channel !== "beta") {
    throw new Error("production input channel must be stable or beta");
  }
  if (!supportedTargets.has(target))
    throw new Error("unsupported macOS production target");
  if (!/^(?:1[0-9]|2[0-9])\.[0-9]+$/.test(minimumSystemVersion)) {
    throw new Error("minimum macOS version is invalid");
  }
  if (!/^[0-9]+$/.test(buildVersion)) {
    throw new Error("production build version must contain only digits");
  }
  const root = resolve(outputRoot);
  const receipt = resolve(receiptPath);
  const receiptWithinRoot = relative(root, receipt);
  if (
    !receiptWithinRoot ||
    (!receiptWithinRoot.startsWith("..") && !isAbsolute(receiptWithinRoot))
  ) {
    throw new Error("production input receipt must be outside the input root");
  }
  const binaries = [
    ["application", "bin/application", applicationPath],
    ["worker", "bin/worker", workerPath],
    ["cameraHelper", "bin/camera", cameraHelperPath],
    ["phoneHelper", "bin/phone", phoneHelperPath],
  ];
  const expectedPaths = [
    ...binaries.map(([, path]) => path),
    ...supportingFiles.map(([, path]) => path),
  ].sort();
  if (new Set(expectedPaths).size !== expectedPaths.length) {
    throw new Error("production input destinations must be unique");
  }
  if (
    JSON.stringify(
      supportingFiles
        .map(([key, path]) => [key, path])
        .sort((left, right) => left[1].localeCompare(right[1])),
    ) !== JSON.stringify(requiredSupportingEntries)
  ) {
    throw new Error("production supporting-file inventory is incomplete");
  }

  await mkdir(dirname(root), { recursive: true });
  await mkdir(root, { recursive: false, mode: 0o700 });
  let receiptWritten = false;
  let complete = false;
  try {
    const records = new Map();
    const stage = async (key, destinationPath, sourcePath, executable) => {
      const source = resolve(sourcePath);
      const sourceBytes = await verifiedSource(source, key);
      const destination = safeOutputPath(root, destinationPath, key);
      await mkdir(dirname(destination), { recursive: true });
      await copyFile(source, destination);
      await chmod(destination, executable ? 0o755 : 0o644);
      const bytes = await readFile(destination);
      if (!bytes.equals(sourceBytes))
        throw new Error(`${key} changed while copying`);
      const metadata = await lstat(destination);
      if (
        !metadata.isFile() ||
        metadata.isSymbolicLink() ||
        metadata.nlink !== 1
      ) {
        throw new Error(`${key} output must be a regular single-link file`);
      }
      if (executable) {
        validateNativeMachO(bytes, target, minimumSystemVersion, key, 2);
      }
      records.set(key, {
        path: destinationPath,
        bytes: bytes.length,
        sha256: sha256(bytes),
      });
    };
    for (const [key, destination, source] of binaries) {
      await stage(key, destination, source, true);
    }
    for (const [key, destination, source] of supportingFiles) {
      await stage(key, destination, source, false);
    }
    if (
      JSON.stringify(await inventory(root)) !== JSON.stringify(expectedPaths)
    ) {
      throw new Error("production input root has missing or extra files");
    }
    const packageVersion = JSON.parse(
      await readFile(join(repoRoot, "package.json"), "utf8"),
    ).version;
    const inputReceipt = {
      schema: "butter-paper/native-macos-production-inputs",
      version: 1,
      channel,
      target,
      applicationVersion: packageVersion,
      buildVersion,
      minimumSystemVersion,
      readyForAssembly: false,
      blockedOn: ["production-pdfium-stage-receipt"],
      artifacts: {
        application: records.get("application"),
        worker: records.get("worker"),
        cameraHelper: records.get("cameraHelper"),
        phoneHelper: records.get("phoneHelper"),
        icon: records.get("icon"),
        iconAssetCatalog: records.get("iconAssetCatalog"),
        thirdPartyNotices: records.get("thirdPartyNotices"),
      },
      licenses: supportingFiles
        .filter(([, destination]) => destination.startsWith("licenses/"))
        .map(([key]) => ({ destination: key, ...records.get(key) }))
        .sort((left, right) =>
          left.destination.localeCompare(right.destination),
        ),
    };
    await mkdir(dirname(receipt), { recursive: true });
    const realRoot = await realpath(root);
    const realReceipt = join(
      await realpath(dirname(receipt)),
      basename(receipt),
    );
    const resolvedReceiptWithinRoot = relative(realRoot, realReceipt);
    if (
      !resolvedReceiptWithinRoot ||
      (!resolvedReceiptWithinRoot.startsWith("..") &&
        !isAbsolute(resolvedReceiptWithinRoot))
    ) {
      throw new Error(
        "production input receipt must be outside the input root",
      );
    }
    await writeFile(receipt, `${JSON.stringify(inputReceipt, null, 2)}\n`, {
      flag: "wx",
      mode: 0o644,
    });
    receiptWritten = true;
    complete = true;
    return inputReceipt;
  } finally {
    if (!complete) {
      await rm(root, { recursive: true, force: true });
      if (receiptWritten) await rm(receipt, { force: true });
    }
  }
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
        "usage: prepare-macos-production-inputs.mjs --application FILE --worker FILE --camera-helper FILE --phone-helper FILE --target TRIPLE --minimum-system-version VERSION --build-version NUMBER --output-root DIR --receipt FILE [--channel stable|beta]",
      );
    }
    values.set(argv[index], argv[index + 1]);
  }
  for (const key of [
    "--application",
    "--worker",
    "--camera-helper",
    "--phone-helper",
    "--target",
    "--minimum-system-version",
    "--build-version",
    "--output-root",
    "--receipt",
  ]) {
    if (!values.has(key)) throw new Error(`${key} is required`);
  }
  return values;
}

if (process.argv[1] === scriptPath) {
  const values = argumentsMap(process.argv.slice(2));
  // macOS temporary directories sit behind the /var -> /private/var symlink,
  // which the staged-input checks reject.
  const iconCatalogRoot = await realpath(
    await mkdtemp(join(tmpdir(), "bp-icon-catalog-")),
  );
  let receipt;
  try {
    const iconAssetCatalogPath = await compileMacosIconAssetCatalog({
      outputDirectory: iconCatalogRoot,
    });
    receipt = await prepareMacosProductionInputs({
      applicationPath: values.get("--application"),
      workerPath: values.get("--worker"),
      cameraHelperPath: values.get("--camera-helper"),
      phoneHelperPath: values.get("--phone-helper"),
      target: values.get("--target"),
      minimumSystemVersion: values.get("--minimum-system-version"),
      buildVersion: values.get("--build-version"),
      outputRoot: values.get("--output-root"),
      receiptPath: values.get("--receipt"),
      channel: values.get("--channel") ?? "stable",
      supportingFiles: defaultSupportingFiles(iconAssetCatalogPath),
    });
  } finally {
    await rm(iconCatalogRoot, { recursive: true, force: true });
  }
  process.stdout.write(`${JSON.stringify(receipt, null, 2)}\n`);
}
