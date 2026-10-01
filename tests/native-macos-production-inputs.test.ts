import {
  access,
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  compileMacosIconAssetCatalog,
  prepareMacosProductionInputs,
} from "../experiments/gpui-migration/gpui-migration/scripts/prepare-macos-production-inputs.mjs";
import { bindMacosProductionManifest } from "../experiments/gpui-migration/gpui-migration/scripts/bind-macos-production-manifest.mjs";
import { macho } from "./helpers/native-macos-production-fixture";

const roots: string[] = [];

async function fixture() {
  const root = await realpath(
    await mkdtemp(join(tmpdir(), "bp-native-inputs-")),
  );
  roots.push(root);
  const sources = join(root, "sources");
  await mkdir(sources);
  const binaries: Record<string, string> = {};
  for (const name of ["application", "worker", "camera", "phone"]) {
    binaries[name] = join(sources, name);
    await writeFile(binaries[name], macho("aarch64-apple-darwin"));
    await chmod(binaries[name], 0o755);
  }
  const supportingFiles: [string, string, string][] = [
    ["icon", "resources/icon.icns", join(sources, "icon")],
    [
      "iconAssetCatalog",
      "resources/Assets.car",
      join(sources, "asset-catalog"),
    ],
    [
      "thirdPartyNotices",
      "resources/THIRD_PARTY_NOTICES.md",
      join(sources, "notices"),
    ],
    ...[
      "allura-font.txt",
      "arimo-font.txt",
      "expo-google-fonts.txt",
      "noto-fonts.txt",
      "phone-helper-go.txt",
      "qrcp.txt",
      "roboto-mono-font.txt",
      "signature-pad.txt",
      "tinos-font.txt",
    ].map(
      (name) =>
        [name, `licenses/${name}`, join(sources, name)] as [
          string,
          string,
          string,
        ],
    ),
  ];
  for (const [key, , path] of supportingFiles) await writeFile(path, key);
  return {
    root,
    binaries,
    supportingFiles,
    outputRoot: join(root, "input-root"),
    receiptPath: join(root, "input-receipt.json"),
  };
}

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

describe("native macOS production input preparation", () => {
  it("compiles the Icon Composer source into a macOS asset catalog", async () => {
    const root = await realpath(
      await mkdtemp(join(tmpdir(), "bp-native-icon-")),
    );
    roots.push(root);
    const iconPath = join(root, "Butter Paper.icon");
    await mkdir(iconPath);
    await writeFile(join(iconPath, "icon.json"), "{}");
    const calls: [string, string[]][] = [];
    const outputDirectory = join(root, "catalog");
    const catalog = await compileMacosIconAssetCatalog({
      iconPath,
      outputDirectory,
      execute: async (command: string, args: string[]) => {
        calls.push([command, args]);
        await writeFile(
          args[args.indexOf("--output-partial-info-plist") + 1],
          "<dict>\n\t<key>CFBundleIconName</key>\n\t<string>Icon</string>\n</dict>",
        );
      },
    });
    expect(catalog).toBe(join(outputDirectory, "Assets.car"));
    expect(calls).toHaveLength(1);
    const [command, args] = calls[0];
    expect(command).toBe("xcrun");
    expect(args.slice(0, 4)).toEqual([
      "actool",
      join(outputDirectory, "Icon.icon"),
      "--compile",
      outputDirectory,
    ]);
    expect(args[args.indexOf("--app-icon") + 1]).toBe("Icon");
    expect(args[args.indexOf("--platform") + 1]).toBe("macosx");
    await access(join(outputDirectory, "Icon.icon/icon.json"));
  });

  it("rejects an asset catalog that does not declare the application icon", async () => {
    const root = await realpath(
      await mkdtemp(join(tmpdir(), "bp-native-icon-")),
    );
    roots.push(root);
    const iconPath = join(root, "Butter Paper.icon");
    await mkdir(iconPath);
    await expect(
      compileMacosIconAssetCatalog({
        iconPath,
        outputDirectory: join(root, "catalog"),
        execute: async (_command: string, args: string[]) => {
          await writeFile(
            args[args.indexOf("--output-partial-info-plist") + 1],
            "<dict></dict>",
          );
        },
      }),
    ).rejects.toThrow("actool did not compile the macOS application icon");
  });

  it("copies and receipts the exact arm64 non-PDFium production inventory", async () => {
    const setup = await fixture();
    const receipt = await prepareMacosProductionInputs({
      applicationPath: setup.binaries.application,
      workerPath: setup.binaries.worker,
      cameraHelperPath: setup.binaries.camera,
      phoneHelperPath: setup.binaries.phone,
      target: "aarch64-apple-darwin",
      minimumSystemVersion: "13.0",
      buildVersion: "25",
      outputRoot: setup.outputRoot,
      receiptPath: setup.receiptPath,
      supportingFiles: setup.supportingFiles,
    });
    expect(receipt).toMatchObject({
      schema: "butter-paper/native-macos-production-inputs",
      readyForAssembly: false,
      blockedOn: ["production-pdfium-stage-receipt"],
      target: "aarch64-apple-darwin",
    });
    expect(Object.keys(receipt.artifacts)).toEqual([
      "application",
      "worker",
      "cameraHelper",
      "phoneHelper",
      "icon",
      "iconAssetCatalog",
      "thirdPartyNotices",
    ]);
    expect(receipt.licenses).toHaveLength(9);
    expect(await readFile(setup.receiptPath, "utf8")).toBe(
      `${JSON.stringify(receipt, null, 2)}\n`,
    );

    const pdfiumStage = join(setup.root, "pdfium-stage");
    await mkdir(join(pdfiumStage, "Resources/PDFium"), { recursive: true });
    const pdfiumReceipt = {
      schema: "butter-paper/pdfium-production-stage",
      version: 1,
      target: "aarch64-apple-darwin",
      apiBuild: 7881,
      sourceRevision: "a".repeat(40),
      manifestSha256: "b".repeat(64),
      files: [
        {
          file: "Frameworks/libpdfium.dylib",
          bytes: 1,
          sha256: "c".repeat(64),
        },
      ],
    };
    await writeFile(
      join(pdfiumStage, "Resources/PDFium/receipt.json"),
      `${JSON.stringify(pdfiumReceipt, null, 2)}\n`,
    );
    const manifestPath = join(setup.root, "manifest.json");
    const manifest = await bindMacosProductionManifest({
      inputReceiptPath: setup.receiptPath,
      pdfiumStage,
      manifestPath,
    });
    expect(manifest).toMatchObject({
      purpose: "unsigned-native-macos-production-assembly",
      target: "aarch64-apple-darwin",
      version: receipt.applicationVersion,
      buildVersion: "25",
      artifacts: receipt.artifacts,
      licenses: receipt.licenses,
    });
    expect(await readFile(manifestPath, "utf8")).toBe(
      `${JSON.stringify(manifest, null, 2)}\n`,
    );
  });

  it("fails closed and removes partial output for the wrong architecture", async () => {
    const setup = await fixture();
    await writeFile(setup.binaries.worker, macho("x86_64-apple-darwin"));
    await expect(
      prepareMacosProductionInputs({
        applicationPath: setup.binaries.application,
        workerPath: setup.binaries.worker,
        cameraHelperPath: setup.binaries.camera,
        phoneHelperPath: setup.binaries.phone,
        target: "aarch64-apple-darwin",
        minimumSystemVersion: "13.0",
        buildVersion: "25",
        outputRoot: setup.outputRoot,
        receiptPath: setup.receiptPath,
        supportingFiles: setup.supportingFiles,
      }),
    ).rejects.toThrow("architecture does not match");
    await expect(access(setup.outputRoot)).rejects.toMatchObject({
      code: "ENOENT",
    });
    await expect(access(setup.receiptPath)).rejects.toMatchObject({
      code: "ENOENT",
    });
  });

  it("rejects incomplete support files and receipts inside the input root", async () => {
    const incomplete = await fixture();
    await expect(
      prepareMacosProductionInputs({
        applicationPath: incomplete.binaries.application,
        workerPath: incomplete.binaries.worker,
        cameraHelperPath: incomplete.binaries.camera,
        phoneHelperPath: incomplete.binaries.phone,
        target: "aarch64-apple-darwin",
        minimumSystemVersion: "13.0",
        buildVersion: "25",
        outputRoot: incomplete.outputRoot,
        receiptPath: incomplete.receiptPath,
        supportingFiles: incomplete.supportingFiles.slice(1),
      }),
    ).rejects.toThrow("supporting-file inventory is incomplete");

    const nested = await fixture();
    await expect(
      prepareMacosProductionInputs({
        applicationPath: nested.binaries.application,
        workerPath: nested.binaries.worker,
        cameraHelperPath: nested.binaries.camera,
        phoneHelperPath: nested.binaries.phone,
        target: "aarch64-apple-darwin",
        minimumSystemVersion: "13.0",
        buildVersion: "25",
        outputRoot: nested.outputRoot,
        receiptPath: join(nested.outputRoot, "receipt.json"),
        supportingFiles: nested.supportingFiles,
      }),
    ).rejects.toThrow("receipt must be outside");
  });

  it("rejects a source path with a symlinked ancestor", async () => {
    const setup = await fixture();
    const sourceAlias = join(setup.root, "source-alias");
    await symlink(join(setup.root, "sources"), sourceAlias);
    await expect(
      prepareMacosProductionInputs({
        applicationPath: join(sourceAlias, "application"),
        workerPath: setup.binaries.worker,
        cameraHelperPath: setup.binaries.camera,
        phoneHelperPath: setup.binaries.phone,
        target: "aarch64-apple-darwin",
        minimumSystemVersion: "13.0",
        buildVersion: "25",
        outputRoot: setup.outputRoot,
        receiptPath: setup.receiptPath,
        supportingFiles: setup.supportingFiles,
      }),
    ).rejects.toThrow("must not traverse a symlink");
    await expect(access(setup.outputRoot)).rejects.toMatchObject({
      code: "ENOENT",
    });
  });

  it("rejects a receipt parent symlink that aliases the input root", async () => {
    const setup = await fixture();
    const receiptAlias = join(setup.root, "receipt-alias");
    await symlink(setup.outputRoot, receiptAlias);
    await expect(
      prepareMacosProductionInputs({
        applicationPath: setup.binaries.application,
        workerPath: setup.binaries.worker,
        cameraHelperPath: setup.binaries.camera,
        phoneHelperPath: setup.binaries.phone,
        target: "aarch64-apple-darwin",
        minimumSystemVersion: "13.0",
        buildVersion: "25",
        outputRoot: setup.outputRoot,
        receiptPath: join(receiptAlias, "receipt.json"),
        supportingFiles: setup.supportingFiles,
      }),
    ).rejects.toThrow("receipt must be outside");
    await expect(access(setup.outputRoot)).rejects.toMatchObject({
      code: "ENOENT",
    });
  });
});
