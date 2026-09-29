import {
  chmod,
  link,
  mkdir,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  assembleMacosProductionApp,
  validateNativeAssemblyManifest,
  verifyUnsignedNativeMacosApp,
} from "../experiments/gpui-migration/gpui-migration/scripts/assemble-macos-production.mjs";
import {
  createNativeMacosProductionFixture,
  digest,
  macho,
} from "./helpers/native-macos-production-fixture";

const roots: string[] = [];
const fixture = async (
  ...args: Parameters<typeof createNativeMacosProductionFixture>
) => {
  const setup = await createNativeMacosProductionFixture(...args);
  roots.push(setup.root);
  return setup;
};

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

describe("unsigned native macOS production assembly", () => {
  it("assembles exact stable and beta app identities for both supported Mac architectures", async () => {
    for (const channel of ["stable", "beta"] as const) {
      for (const target of [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
      ] as const) {
        const setup = await fixture(channel, target);
        const receipt = await assembleMacosProductionApp({
          manifestPath: setup.manifestPath,
          inputRoot: setup.inputRoot,
          pdfiumStage: setup.pdfiumStage,
          outputDirectory: setup.output,
        });
        const productName =
          channel === "stable" ? "Butter Paper" : "Butter Paper Beta";
        const bundleId =
          channel === "stable"
            ? "com.butterpaper.desktop"
            : "com.butterpaper.desktop.beta";
        expect(receipt).toMatchObject({
          signed: false,
          channel,
          productName,
          bundleIdentifier: bundleId,
          applicationVersion: setup.packageVersion,
          target,
        });
        expect(receipt.files.map(({ file }) => file)).toContain(
          `Contents/MacOS/${productName}`,
        );
        const plist = await readFile(
          join(setup.output, "Contents/Info.plist"),
          "utf8",
        );
        expect(plist).toContain(`<string>${bundleId}</string>`);
        expect(plist).toContain(`<string>${productName}</string>`);
        expect(plist).toContain(
          "<key>CFBundleIconFile</key>\n  <string>icon.icns</string>",
        );
        expect(plist).toContain(
          "<key>NSLocalNetworkUsageDescription</key>\n  <string>Butter Paper uses your local network only when you choose to transfer a signature from your phone.</string>",
        );
        const saved = JSON.parse(
          await readFile(
            join(
              setup.output,
              "Contents/Resources/native-assembly-receipt.json",
            ),
            "utf8",
          ),
        );
        expect(saved).toEqual(receipt);
        await expect(
          verifyUnsignedNativeMacosApp({
            appPath: setup.output,
            manifestPath: setup.manifestPath,
          }),
        ).resolves.toEqual(receipt);
      }
    }
  });

  it("rejects schema, package-version, architecture, receipt and inventory drift", async () => {
    const setup = await fixture();
    expect(() =>
      validateNativeAssemblyManifest(
        { ...setup.manifest, version: "0.0.0" },
        setup.packageVersion,
      ),
    ).toThrow("root package version");

    await writeFile(join(setup.inputRoot, "bin/application"), macho("x86_64-apple-darwin"));
    setup.manifest.artifacts.application = {
      path: "bin/application",
      bytes: macho("x86_64-apple-darwin").length,
      sha256: digest(macho("x86_64-apple-darwin")),
    };
    await writeFile(
      setup.manifestPath,
      `${JSON.stringify(setup.manifest, null, 2)}\n`,
    );
    await expect(
      assembleMacosProductionApp({
        manifestPath: setup.manifestPath,
        inputRoot: setup.inputRoot,
        pdfiumStage: setup.pdfiumStage,
        outputDirectory: setup.output,
      }),
    ).rejects.toThrow("architecture does not match");

    const receiptDrift = await fixture();
    receiptDrift.manifest.pdfiumStageReceiptSha256 = "0".repeat(64);
    await writeFile(
      receiptDrift.manifestPath,
      `${JSON.stringify(receiptDrift.manifest, null, 2)}\n`,
    );
    await expect(
      assembleMacosProductionApp({
        manifestPath: receiptDrift.manifestPath,
        inputRoot: receiptDrift.inputRoot,
        pdfiumStage: receiptDrift.pdfiumStage,
        outputDirectory: receiptDrift.output,
      }),
    ).rejects.toThrow("receipt digest changed");

    const extra = await fixture();
    await writeFile(join(extra.inputRoot, "unexpected"), "extra");
    await expect(
      assembleMacosProductionApp({
        manifestPath: extra.manifestPath,
        inputRoot: extra.inputRoot,
        pdfiumStage: extra.pdfiumStage,
        outputDirectory: extra.output,
      }),
    ).rejects.toThrow("missing, extra or duplicated");
  });

  it("rejects a Mach-O that requires a newer macOS than the package declares", async () => {
    const setup = await fixture("stable", "aarch64-apple-darwin", "12.0");
    const phone = macho("aarch64-apple-darwin", "13.0");
    await writeFile(join(setup.inputRoot, "bin/phone"), phone);
    setup.manifest.artifacts.phoneHelper = {
      path: "bin/phone",
      bytes: phone.length,
      sha256: digest(phone),
    };
    await writeFile(setup.manifestPath, `${JSON.stringify(setup.manifest, null, 2)}\n`);

    await expect(
      assembleMacosProductionApp({
        manifestPath: setup.manifestPath,
        inputRoot: setup.inputRoot,
        pdfiumStage: setup.pdfiumStage,
        outputDirectory: setup.output,
      }),
    ).rejects.toThrow("artifacts.phoneHelper requires macOS 13.0 but the package declares 12.0");
  });

  it("rejects a dylib substituted for an application executable", async () => {
    const setup = await fixture();
    const application = macho("aarch64-apple-darwin", "13.0", 6);
    await writeFile(join(setup.inputRoot, "bin/application"), application);
    setup.manifest.artifacts.application = {
      path: "bin/application",
      bytes: application.length,
      sha256: digest(application),
    };
    await writeFile(setup.manifestPath, `${JSON.stringify(setup.manifest, null, 2)}\n`);

    await expect(
      assembleMacosProductionApp({
        manifestPath: setup.manifestPath,
        inputRoot: setup.inputRoot,
        pdfiumStage: setup.pdfiumStage,
        outputDirectory: setup.output,
      }),
    ).rejects.toThrow("artifacts.application has the wrong Mach-O file type");
  });

  it("rejects unsafe links, non-executable helpers and pre-existing output", async () => {
    const linked = await fixture();
    await rm(join(linked.inputRoot, "bin/camera"));
    await symlink("application", join(linked.inputRoot, "bin/camera"));
    await expect(
      assembleMacosProductionApp({
        manifestPath: linked.manifestPath,
        inputRoot: linked.inputRoot,
        pdfiumStage: linked.pdfiumStage,
        outputDirectory: linked.output,
      }),
    ).rejects.toThrow("symlink");

    const hardLinked = await fixture();
    await rm(join(hardLinked.inputRoot, "licenses/qrcp.txt"));
    await link(
      join(hardLinked.inputRoot, "licenses/signature-pad.txt"),
      join(hardLinked.inputRoot, "licenses/qrcp.txt"),
    );
    await expect(
      assembleMacosProductionApp({
        manifestPath: hardLinked.manifestPath,
        inputRoot: hardLinked.inputRoot,
        pdfiumStage: hardLinked.pdfiumStage,
        outputDirectory: hardLinked.output,
      }),
    ).rejects.toThrow("hard link");

    const mode = await fixture();
    await chmod(join(mode.inputRoot, "bin/phone"), 0o644);
    await expect(
      assembleMacosProductionApp({
        manifestPath: mode.manifestPath,
        inputRoot: mode.inputRoot,
        pdfiumStage: mode.pdfiumStage,
        outputDirectory: mode.output,
      }),
    ).rejects.toThrow("must be executable");

    const existing = await fixture();
    await mkdir(existing.output);
    await expect(
      assembleMacosProductionApp({
        manifestPath: existing.manifestPath,
        inputRoot: existing.inputRoot,
        pdfiumStage: existing.pdfiumStage,
        outputDirectory: existing.output,
      }),
    ).rejects.toMatchObject({ code: "EEXIST" });
  });

  it("independently rejects assembled byte and inventory drift", async () => {
    const tampered = await fixture();
    await assembleMacosProductionApp({
      manifestPath: tampered.manifestPath,
      inputRoot: tampered.inputRoot,
      pdfiumStage: tampered.pdfiumStage,
      outputDirectory: tampered.output,
    });
    await writeFile(
      join(tampered.output, "Contents/Resources/icon.icns"),
      "changed",
    );
    await expect(
      verifyUnsignedNativeMacosApp({
        appPath: tampered.output,
        manifestPath: tampered.manifestPath,
      }),
    ).rejects.toThrow("input receipt");

    const extra = await fixture();
    await assembleMacosProductionApp({
      manifestPath: extra.manifestPath,
      inputRoot: extra.inputRoot,
      pdfiumStage: extra.pdfiumStage,
      outputDirectory: extra.output,
    });
    await writeFile(join(extra.output, "Contents/unexpected"), "extra");
    await expect(
      verifyUnsignedNativeMacosApp({
        appPath: extra.output,
        manifestPath: extra.manifestPath,
      }),
    ).rejects.toThrow("missing or extra files");
  });
});
