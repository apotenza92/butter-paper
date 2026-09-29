import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { packageSignedMacosProduction } from "../experiments/gpui-migration/gpui-migration/scripts/package-macos-signed-production.mjs";

const roots: string[] = [];
const fingerprint = "A".repeat(64);
const revision = "b".repeat(40);
const digest = (bytes: Buffer | string) => createHash("sha256").update(bytes).digest("hex");

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "native-macos-package-test-"));
  roots.push(root);
  const appPath = join(root, "Butter Paper.app");
  await mkdir(join(appPath, "Contents"), { recursive: true });
  await writeFile(join(appPath, "Contents", "marker"), "synthetic signed app");
  const manifestPath = join(root, "assembly.json");
  const manifest = { channel: "stable", target: "aarch64-apple-darwin", version: "1.2.3" };
  const manifestBytes = Buffer.from(JSON.stringify(manifest));
  await writeFile(manifestPath, manifestBytes);
  const signingReceiptPath = join(root, "signing.json");
  await writeFile(signingReceiptPath, JSON.stringify({
    schema: "butter-paper/signed-native-macos-production", version: 1,
    channel: "stable", target: manifest.target,
    identity: "Developer ID Application: Alexander Potenza (27JL2VERNC)",
    signingCertificateSha256: fingerprint, codeObjectCount: 5, manifestSha256: digest(manifestBytes),
    notarisation: { status: "Accepted", submissionId: "synthetic-submission" }, stapled: true, verified: true,
  }));
  const outputDir = join(root, "release");
  const verifierPaths: string[] = [];
  const fakeRun = async (command: string, args: string[]) => {
    if (command !== "/fake/ditto") throw new Error(`unexpected platform command ${command}`);
    if (args[0] === "-c") {
      expect(args.slice(1, 4)).toEqual(["-k", "--keepParent", appPath]);
      await writeFile(args[4], "synthetic zip bytes");
    } else if (args[0] === "-x") {
      const extractDir = args[3];
      const extracted = join(extractDir, basename(appPath));
      await mkdir(join(extracted, "Contents"), { recursive: true });
      await writeFile(join(extracted, "Contents", "marker"), "synthetic signed app");
    } else throw new Error(`unexpected archive arguments ${args.join(" ")}`);
  };
  const verify = async ({ appPath: path, manifestPath: manifest, fingerprint: supplied }: { appPath: string; manifestPath: string; fingerprint: string }) => {
    expect(manifest).toBe(manifestPath);
    expect(supplied).toBe(fingerprint);
    verifierPaths.push(path);
    return { channel: "stable", target: "aarch64-apple-darwin", codeObjectCount: 5 };
  };
  return { root, appPath, manifestPath, signingReceiptPath, outputDir, verifierPaths, fakeRun, verify };
}

afterEach(async () => Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true }))));

describe("signed macOS production release packaging", () => {
  it("verifies before and after ZIP extraction and emits stable-candidate receipts", async () => {
    const f = await fixture();
    const result = await packageSignedMacosProduction({
      ...f, sourceRevision: revision, fingerprint, dittoPath: "/fake/ditto", run: f.fakeRun, verify: f.verify,
    });
    expect(f.verifierPaths[0]).toBe(f.appPath);
    expect(basename(f.verifierPaths[1])).toBe("Butter Paper.app");
    expect(f.verifierPaths[1]).not.toBe(f.appPath);
    expect(result.artifact).toEqual({ path: basename(result.archive), bytes: 19, sha256: digest("synthetic zip bytes") });
    expect(result.packageManifest).toMatchObject({
      schema: "butter-paper/package-manifest", schemaVersion: 1, target: "macos-arm64",
      channel: "stable", version: "1.2.3", sourceRevision: revision, artifact: result.artifact,
    });
    expect(result.verificationReceipt).toMatchObject({
      schema: "butter-paper/package-verification", schemaVersion: 1, verified: true,
      extractedAppVerified: true, artifact: result.artifact,
    });
    expect((await readFile(result.archive)).toString()).toBe("synthetic zip bytes");
  });

  it("rejects mismatched signing evidence before archive work", async () => {
    const f = await fixture();
    const receipt = JSON.parse(await readFile(f.signingReceiptPath, "utf8"));
    receipt.target = "x86_64-apple-darwin";
    await writeFile(f.signingReceiptPath, JSON.stringify(receipt));
    await expect(packageSignedMacosProduction({
      ...f, sourceRevision: revision, fingerprint, dittoPath: "/fake/ditto", run: f.fakeRun, verify: f.verify,
    })).rejects.toThrow("signing receipt does not match");
    expect(f.verifierPaths).toHaveLength(0);
    await expect(readdir(f.outputDir)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("cleans every output if extracted-app verification fails", async () => {
    const f = await fixture();
    let count = 0;
    const verify = async (args: Parameters<typeof f.verify>[0]) => {
      count += 1;
      if (count === 2) throw new Error("extracted signature rejected");
      return f.verify(args);
    };
    await expect(packageSignedMacosProduction({
      ...f, sourceRevision: revision, fingerprint, dittoPath: "/fake/ditto", run: f.fakeRun, verify,
    })).rejects.toThrow("extracted signature rejected");
    expect(await readdir(f.outputDir)).toEqual([]);
  });
});
