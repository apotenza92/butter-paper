import { writeFileSync } from "node:fs";
import { mkdir, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { assembleMacosProductionApp } from "../experiments/gpui-migration/gpui-migration/scripts/assemble-macos-production.mjs";
import {
  expectedNativeSignedCodeObjects,
  expectedNativeSignedInventory,
  parseNativeCodesignMetadata,
  validateNativeArchitecture,
  validateNativeCodesignMetadata,
  validateNativeEntitlements,
  verifySignedNativeMacosApp,
} from "../experiments/gpui-migration/gpui-migration/scripts/verify-signed-macos-production.mjs";
import {
  createNativeMacosProductionFixture,
  digest,
  type NativeMacosChannel,
  type NativeMacosTarget,
} from "./helpers/native-macos-production-fixture";

const roots: string[] = [];
const signingIdentity =
  "Developer ID Application: Alexander Potenza (27JL2VERNC)";
const certificateBytes = Buffer.from("synthetic trusted leaf certificate");

type RunMutation = {
  architectureRole?: string;
  entitlementRole?: string;
  failCommand?: "xcrun" | "spctl";
  metadata?: {
    authority?: string;
    cdHash?: string;
    identifier?: string;
    runtime?: boolean;
    team?: string;
    ticket?: string;
    timestamp?: string;
  };
  missingApplicationChain?: boolean;
};

async function signedFixture(
  channel: NativeMacosChannel = "stable",
  target: NativeMacosTarget = "aarch64-apple-darwin",
  mutation: RunMutation = {},
) {
  const setup = await createNativeMacosProductionFixture(channel, target);
  roots.push(setup.root);
  await assembleMacosProductionApp({
    manifestPath: setup.manifestPath,
    inputRoot: setup.inputRoot,
    pdfiumStage: setup.pdfiumStage,
    outputDirectory: setup.output,
  });
  await mkdir(join(setup.output, "Contents/_CodeSignature"), {
    recursive: true,
  });
  await writeFile(
    join(setup.output, "Contents/_CodeSignature/CodeResources"),
    "synthetic code resources",
  );

  const objects = expectedNativeSignedCodeObjects(channel).map((object) => ({
    ...object,
    absolutePath: join(setup.output, object.path),
  }));
  const calls: Array<{ command: string; args: string[]; input?: string }> = [];
  const objectFor = (path: string) => {
    const object = objects.find((candidate) => candidate.absolutePath === path);
    if (!object) throw new Error(`unexpected synthetic code path ${path}`);
    return object;
  };
  const run = (
    command: string,
    args: string[],
    options: { input?: string } = {},
  ) => {
    calls.push({ command, args, input: options.input });
    if (mutation.failCommand === command) {
      throw new Error(`${command} rejected synthetic package`);
    }
    if (command === "lipo") {
      const object = objectFor(args.at(-1)!);
      if (mutation.architectureRole === object.role) return "x86_64";
      return target === "aarch64-apple-darwin" ? "arm64" : "x86_64";
    }
    if (command === "plutil") {
      return JSON.stringify(
        mutation.entitlementRole === "camera-helper"
          ? { "com.apple.security.cs.allow-jit": true }
          : { "com.apple.security.device.camera": true },
      );
    }
    if (command === "codesign" && args[0] === "-dvvv") {
      const object = objectFor(args.at(-1)!);
      const metadata = object.role === "application" ? mutation.metadata : {};
      return [
        `Identifier=${metadata?.identifier ?? object.identifier}`,
        `Authority=${metadata?.authority ?? signingIdentity}`,
        `TeamIdentifier=${metadata?.team ?? "27JL2VERNC"}`,
        `CDHash=${metadata?.cdHash ?? "abc123"}`,
        `Timestamp=${metadata?.timestamp ?? "28 Sep 2026 at 10:00:00"}`,
        `Notarization Ticket=${metadata?.ticket ?? "stapled"}`,
        metadata?.runtime === false
          ? "CodeDirectory v=20500 flags=0x0(none)"
          : "CodeDirectory v=20500 flags=0x10000(runtime)",
      ].join("\n");
    }
    if (
      command === "codesign" &&
      args[0] === "-d" &&
      args.includes("--entitlements")
    ) {
      const object = objectFor(args.at(-1)!);
      if (object.role === "camera-helper") return "<?xml version=\"1.0\"?><plist/>";
      if (mutation.entitlementRole === object.role) {
        return "<?xml version=\"1.0\"?><plist/>";
      }
      return "";
    }
    if (
      command === "codesign" &&
      args[0] === "-d" &&
      args.some((arg) => arg.startsWith("--extract-certificates="))
    ) {
      const object = objectFor(args.at(-1)!);
      const prefix = args
        .find((arg) => arg.startsWith("--extract-certificates="))!
        .slice("--extract-certificates=".length);
      writeFileSync(`${prefix}0`, certificateBytes);
      if (object.role === "application" && !mutation.missingApplicationChain) {
        writeFileSync(`${prefix}1`, "synthetic intermediate certificate");
        writeFileSync(`${prefix}2`, "synthetic root certificate");
      }
      return "";
    }
    if (command === "codesign" && args[0] === "--verify") return "";
    if (command === "security" && args[0] === "verify-cert") return "";
    if (command === "xcrun" && args[0] === "stapler") return "";
    if (command === "spctl" && args[0] === "--assess") return "";
    throw new Error(`unexpected synthetic command: ${command} ${args.join(" ")}`);
  };
  return {
    ...setup,
    calls,
    fingerprint: digest(certificateBytes).toUpperCase(),
    objects,
    run,
  };
}

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

const validMetadata = (identifier: string) => ({
  authorities: ["Developer ID Application: Alexander Potenza (27JL2VERNC)"],
  cdHash: "abc",
  flags: "CodeDirectory v=20500 size=1 flags=0x10000(runtime)",
  identifier,
  teamIdentifier: "27JL2VERNC",
  timestamp: "28 Sep 2026 at 10:00:00",
  ticket: "stapled",
});

describe("signed native macOS production verifier", () => {
  it("runs the complete stable and beta signing gate across all five code objects", async () => {
    for (const [channel, target] of [
      ["stable", "aarch64-apple-darwin"],
      ["beta", "x86_64-apple-darwin"],
    ] as const) {
      const setup = await signedFixture(channel, target);
      await expect(
        verifySignedNativeMacosApp({
          appPath: setup.output,
          manifestPath: setup.manifestPath,
          fingerprint: setup.fingerprint,
          run: setup.run,
        }),
      ).resolves.toEqual({ channel, target, codeObjectCount: 5 });

      for (const object of setup.objects) {
        expect(setup.calls).toContainEqual(
          expect.objectContaining({
            command: "codesign",
            args: ["--verify", "--strict", "--verbose=2", object.absolutePath],
          }),
        );
        expect(setup.calls).toContainEqual(
          expect.objectContaining({
            command: "codesign",
            args: ["-dvvv", object.absolutePath],
          }),
        );
        expect(setup.calls).toContainEqual(
          expect.objectContaining({
            command: "lipo",
            args: ["-archs", object.absolutePath],
          }),
        );
        expect(setup.calls).toContainEqual(
          expect.objectContaining({
            command: "codesign",
            args: [
              "-d",
              "--xml",
              "--entitlements",
              "-",
              object.absolutePath,
            ],
          }),
        );
        expect(
          setup.calls.some(
            ({ command, args }) =>
              command === "codesign" &&
              args.some((arg) =>
                arg.startsWith("--extract-certificates="),
              ) &&
              args.at(-1) === object.absolutePath,
          ),
        ).toBe(true);
      }
      expect(
        setup.calls.filter(({ command }) => command === "security"),
      ).toHaveLength(1);
      expect(
        setup.calls.filter(({ command }) => command === "plutil"),
      ).toHaveLength(1);
      expect(
        setup.calls.filter(
          ({ command, args }) => command === "xcrun" && args[0] === "stapler",
        ),
      ).toHaveLength(1);
      expect(
        setup.calls.filter(({ command }) => command === "spctl"),
      ).toHaveLength(1);
    }
  });

  it("rejects immutable-resource changes and unexpected signed inventory", async () => {
    const changed = await signedFixture();
    await writeFile(
      join(changed.output, "Contents/Resources/icon.icns"),
      "changed icon",
    );
    await expect(
      verifySignedNativeMacosApp({
        appPath: changed.output,
        manifestPath: changed.manifestPath,
        fingerprint: changed.fingerprint,
        run: changed.run,
      }),
    ).rejects.toThrow("changed immutable resource");
    expect(changed.calls).toHaveLength(0);

    const extra = await signedFixture();
    await writeFile(join(extra.output, "Contents/Resources/unexpected"), "extra");
    await expect(
      verifySignedNativeMacosApp({
        appPath: extra.output,
        manifestPath: extra.manifestPath,
        fingerprint: extra.fingerprint,
        run: extra.run,
      }),
    ).rejects.toThrow("missing or extra files");
    expect(extra.calls).toHaveLength(0);
  });

  it("rejects integrated identity, trust and hardened-runtime failures", async () => {
    for (const [metadata, message] of [
      [{ authority: "Apple Development: Someone Else" }, "untrusted signing authority"],
      [{ team: "OTHERTEAM" }, "unexpected signing team"],
      [{ identifier: "com.example.substitution" }, "has identifier"],
      [{ runtime: false }, "hardened runtime"],
      [{ timestamp: "" }, "secure timestamp"],
      [{ cdHash: "" }, "CDHash"],
    ] as const) {
      const setup = await signedFixture("stable", "aarch64-apple-darwin", {
        metadata,
      });
      await expect(
        verifySignedNativeMacosApp({
          appPath: setup.output,
          manifestPath: setup.manifestPath,
          fingerprint: setup.fingerprint,
          run: setup.run,
        }),
      ).rejects.toThrow(message);
    }
  });

  it("rejects integrated architecture and entitlement widening", async () => {
    const architecture = await signedFixture("stable", "aarch64-apple-darwin", {
      architectureRole: "pdf-worker",
    });
    await expect(
      verifySignedNativeMacosApp({
        appPath: architecture.output,
        manifestPath: architecture.manifestPath,
        fingerprint: architecture.fingerprint,
        run: architecture.run,
      }),
    ).rejects.toThrow("pdf-worker architecture does not exactly match");

    const entitlement = await signedFixture("stable", "aarch64-apple-darwin", {
      entitlementRole: "camera-helper",
    });
    await expect(
      verifySignedNativeMacosApp({
        appPath: entitlement.output,
        manifestPath: entitlement.manifestPath,
        fingerprint: entitlement.fingerprint,
        run: entitlement.run,
      }),
    ).rejects.toThrow("camera-helper has missing or unexpected entitlements");
  });

  it("rejects leaf fingerprint and application certificate-chain failures", async () => {
    const fingerprint = await signedFixture();
    await expect(
      verifySignedNativeMacosApp({
        appPath: fingerprint.output,
        manifestPath: fingerprint.manifestPath,
        fingerprint: "0".repeat(64),
        run: fingerprint.run,
      }),
    ).rejects.toThrow("leaf signing certificate is not trusted");

    const chain = await signedFixture("stable", "aarch64-apple-darwin", {
      missingApplicationChain: true,
    });
    await expect(
      verifySignedNativeMacosApp({
        appPath: chain.output,
        manifestPath: chain.manifestPath,
        fingerprint: chain.fingerprint,
        run: chain.run,
      }),
    ).rejects.toThrow();
    expect(chain.calls.some(({ command }) => command === "security")).toBe(false);
  });

  it("rejects missing notarisation ticket, Stapler and Gatekeeper failures", async () => {
    const ticket = await signedFixture("stable", "aarch64-apple-darwin", {
      metadata: { ticket: "" },
    });
    await expect(
      verifySignedNativeMacosApp({
        appPath: ticket.output,
        manifestPath: ticket.manifestPath,
        fingerprint: ticket.fingerprint,
        run: ticket.run,
      }),
    ).rejects.toThrow("stapled notarisation ticket");

    for (const command of ["xcrun", "spctl"] as const) {
      const setup = await signedFixture("stable", "aarch64-apple-darwin", {
        failCommand: command,
      });
      await expect(
        verifySignedNativeMacosApp({
          appPath: setup.output,
          manifestPath: setup.manifestPath,
          fingerprint: setup.fingerprint,
          run: setup.run,
        }),
      ).rejects.toThrow(`${command} rejected synthetic package`);
    }
  });

  it("defines exact stable and beta code identities with camera access only on the camera helper", () => {
    for (const [channel, bundleId, product] of [
      ["stable", "com.butterpaper.desktop", "Butter Paper"],
      ["beta", "com.butterpaper.desktop.beta", "Butter Paper Beta"],
    ] as const) {
      const objects = expectedNativeSignedCodeObjects(channel);
      expect(objects).toHaveLength(5);
      expect(objects[0]).toMatchObject({
        path: `Contents/MacOS/${product}`,
        identifier: bundleId,
        entitlements: {},
      });
      expect(objects.find(({ role }) => role === "camera-helper")?.entitlements).toEqual({
        "com.apple.security.device.camera": true,
      });
      expect(objects.filter(({ role }) => role !== "camera-helper").every(({ entitlements }) => Object.keys(entitlements).length === 0)).toBe(true);
      expect(objects.some(({ identifier }) => identifier.includes("electron"))).toBe(false);
    }
  });

  it("accepts only exact Developer ID metadata, hardened runtime and channel identifier", () => {
    const output = [
      "Identifier=com.butterpaper.desktop",
      "Authority=Developer ID Application: Alexander Potenza (27JL2VERNC)",
      "TeamIdentifier=27JL2VERNC",
      "CDHash=abc",
      "Timestamp=28 Sep 2026 at 10:00:00",
      "Notarization Ticket=stapled",
      "CodeDirectory v=20500 size=1 flags=0x10000(runtime)",
    ].join("\n");
    const parsed = parseNativeCodesignMetadata(output);
    expect(() => validateNativeCodesignMetadata(parsed, "com.butterpaper.desktop", "app")).not.toThrow();
    for (const change of [
      { authorities: ["Apple Development: Someone Else"] },
      { teamIdentifier: "OTHER" },
      { identifier: "com.butterpaper.desktop.beta" },
      { flags: "CodeDirectory flags=0x0(none)" },
      { timestamp: null },
      { cdHash: null },
    ]) {
      expect(() => validateNativeCodesignMetadata({ ...validMetadata("com.butterpaper.desktop"), ...change }, "com.butterpaper.desktop", "app")).toThrow();
    }
  });

  it("rejects Electron runtime entitlements and architecture widening", () => {
    expect(() => validateNativeEntitlements(
      { "com.apple.security.device.camera": true },
      { "com.apple.security.device.camera": true },
      "camera",
    )).not.toThrow();
    expect(() => validateNativeEntitlements(
      { "com.apple.security.cs.allow-jit": true },
      {},
      "application",
    )).toThrow("unexpected entitlements");
    expect(() => validateNativeArchitecture("arm64", "aarch64-apple-darwin", "app")).not.toThrow();
    expect(() => validateNativeArchitecture("x86_64", "x86_64-apple-darwin", "app")).not.toThrow();
    expect(() => validateNativeArchitecture("arm64 x86_64", "aarch64-apple-darwin", "app")).toThrow("exactly match");
  });

  it("allows exactly the pre-sign files plus the signing CodeResources file", () => {
    const receipt = {
      files: [
        { file: "Contents/Info.plist" },
        { file: "Contents/MacOS/Butter Paper" },
      ],
    };
    expect(expectedNativeSignedInventory(receipt)).toEqual([
      "Contents/Info.plist",
      "Contents/MacOS/Butter Paper",
      "Contents/Resources/native-assembly-receipt.json",
      "Contents/_CodeSignature/CodeResources",
    ]);
    expect(() => expectedNativeSignedInventory({
      files: [{ file: "Contents/Info.plist" }, { file: "Contents/Info.plist" }],
    })).toThrow("duplicate");
    expect(() => expectedNativeSignedInventory({
      files: [{ file: "Contents/../../outside" }],
    })).toThrow("unsafe file path");
  });
});
