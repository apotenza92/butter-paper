import { createHash } from "node:crypto";
import { chmod, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const digest = (bytes: Uint8Array | string) =>
  createHash("sha256").update(bytes).digest("hex");

const targetCpu = {
  "aarch64-apple-darwin": 0x0100000c,
  "x86_64-apple-darwin": 0x01000007,
};

export type NativeMacosTarget = keyof typeof targetCpu;
export type NativeMacosChannel = "stable" | "beta";

export function macho(
  target: NativeMacosTarget,
  minimumSystemVersion = "13.0",
  fileType = 2,
) {
  const bytes = Buffer.alloc(56);
  bytes.writeUInt32LE(0xfeedfacf, 0);
  bytes.writeUInt32LE(targetCpu[target], 4);
  bytes.writeUInt32LE(fileType, 12);
  bytes.writeUInt32LE(1, 16);
  bytes.writeUInt32LE(24, 20);
  bytes.writeUInt32LE(0x32, 32);
  bytes.writeUInt32LE(24, 36);
  bytes.writeUInt32LE(1, 40);
  const [major, minor] = minimumSystemVersion.split(".").map(Number);
  bytes.writeUInt32LE((major << 16) | (minor << 8), 44);
  bytes.writeUInt32LE((26 << 16) | (2 << 8), 48);
  return bytes;
}

export async function createNativeMacosProductionFixture(
  channel: NativeMacosChannel = "stable",
  target: NativeMacosTarget = "aarch64-apple-darwin",
  minimumSystemVersion = "13.0",
) {
  const root = await mkdtemp(join(tmpdir(), "bp-native-package-"));
  const inputRoot = join(root, "inputs");
  const pdfiumStage = join(root, "pdfium");
  await mkdir(inputRoot, { recursive: true });
  await mkdir(join(pdfiumStage, "Frameworks"), { recursive: true });
  await mkdir(join(pdfiumStage, "Resources/PDFium"), { recursive: true });
  await mkdir(join(pdfiumStage, "Resources/Licenses/PDFium"), {
    recursive: true,
  });

  const files = new Map<string, Buffer>();
  for (const name of ["application", "worker", "camera", "phone"]) {
    files.set(`bin/${name}`, macho(target, minimumSystemVersion));
  }
  files.set("resources/icon.icns", Buffer.from("icon"));
  files.set("resources/Assets.car", Buffer.from("asset catalog"));
  files.set("resources/THIRD_PARTY_NOTICES.md", Buffer.from("notices"));
  const licenseNames = [
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
  for (const name of licenseNames) {
    files.set(`licenses/${name}`, Buffer.from(`licence ${name}`));
  }
  for (const [path, bytes] of files) {
    await mkdir(join(inputRoot, path, ".."), { recursive: true });
    await writeFile(join(inputRoot, path), bytes);
    if (path.startsWith("bin/")) await chmod(join(inputRoot, path), 0o755);
  }

  const pdfiumFiles = new Map<string, Buffer>([
    ["Frameworks/libpdfium.dylib", macho(target, minimumSystemVersion, 6)],
    ["Resources/PDFium/sbom", Buffer.from("sbom")],
    ["Resources/PDFium/provenance", Buffer.from("provenance")],
    ["Resources/PDFium/gn-args.txt", Buffer.from("args")],
    ["Resources/PDFium/redistribution-review", Buffer.from("redistribution")],
    ["Resources/PDFium/supplier-review", Buffer.from("supplier")],
    ["Resources/Licenses/PDFium/LICENSE", Buffer.from("pdfium licence")],
  ]);
  for (const [path, bytes] of pdfiumFiles) {
    await mkdir(join(pdfiumStage, path, ".."), { recursive: true });
    await writeFile(join(pdfiumStage, path), bytes);
  }
  const pdfiumReceipt = {
    schema: "butter-paper/pdfium-production-stage",
    version: 1,
    target,
    apiBuild: 7881,
    sourceRevision: "a".repeat(40),
    manifestSha256: "b".repeat(64),
    files: [...pdfiumFiles]
      .map(([file, bytes]) => ({ file, bytes: bytes.length, sha256: digest(bytes) }))
      .sort((left, right) => left.file.localeCompare(right.file)),
  };
  const pdfiumReceiptBytes = Buffer.from(
    `${JSON.stringify(pdfiumReceipt, null, 2)}\n`,
  );
  await writeFile(
    join(pdfiumStage, "Resources/PDFium/receipt.json"),
    pdfiumReceiptBytes,
  );

  const packageVersion = JSON.parse(
    await readFile(new URL("../../package.json", import.meta.url), "utf8"),
  ).version;
  const record = (path: string) => {
    const bytes = files.get(path)!;
    return { path, bytes: bytes.length, sha256: digest(bytes) };
  };
  const manifest = {
    schemaVersion: 1,
    purpose: "unsigned-native-macos-production-assembly",
    channel,
    target,
    version: packageVersion,
    buildVersion: "25",
    minimumSystemVersion,
    pdfiumStageReceiptSha256: digest(pdfiumReceiptBytes),
    artifacts: {
      application: record("bin/application"),
      worker: record("bin/worker"),
      cameraHelper: record("bin/camera"),
      phoneHelper: record("bin/phone"),
      icon: record("resources/icon.icns"),
      iconAssetCatalog: record("resources/Assets.car"),
      thirdPartyNotices: record("resources/THIRD_PARTY_NOTICES.md"),
    },
    licenses: licenseNames.map((destination) => ({
      destination,
      ...record(`licenses/${destination}`),
    })),
  };
  const manifestPath = join(root, "manifest.json");
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  const productName = channel === "stable" ? "Butter Paper" : "Butter Paper Beta";
  return {
    root,
    inputRoot,
    pdfiumStage,
    manifest,
    manifestPath,
    packageVersion,
    output: join(root, `${productName}.app`),
  };
}
