import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { buildHomebrewPublication } from "../scripts/build-homebrew-publication.mjs";
import { VERSION, versionFiles } from "../scripts/release-version.mjs";

const commit = "0123456789abcdef0123456789abcdef01234567";
const roots: string[] = [];

async function assets(names: string[]) {
  const root = await mkdtemp(join(tmpdir(), "bp-homebrew-"));
  roots.push(root);
  for (const name of names) await writeFile(join(root, name), `${name}\n`);
  return root;
}

const macos = (prefix: string) =>
  ["arm64", "x64"].map((arch) => `Butter-Paper-${prefix}macOS-${arch}.zip`);

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe("Homebrew publication bundle", () => {
  it("advances both casks from a stable release, as the tap registry requires", async () => {
    const root = await assets([...macos(""), ...macos("Beta-")]);
    const manifest = await buildHomebrewPublication({
      tag: "v0.0.32",
      commit,
      assetsDir: root,
      outputDir: join(root, "publication"),
      runId: 7,
      runAttempt: 1,
      jobs: ["package macos-arm64"],
    });
    expect(manifest.channel).toBe("stable");
    expect(manifest.casks).toEqual(["butter-paper.rb", "butter-paper@beta.rb"]);
    expect(manifest.minimum_macos).toBe("13.0");
    expect(manifest.artifacts.map(({ name }: { name: string }) => name)).toEqual([
      ...macos(""),
      ...macos("Beta-"),
    ]);
    const stable = await readFile(join(root, "publication/Casks/butter-paper.rb"), "utf8");
    expect(stable).toContain('version "0.0.32"');
    expect(stable).toContain("download/v#{version}/Butter-Paper-macOS-arm64.zip");
    expect(stable).toContain('app "Butter Paper.app"');
    expect(stable).toContain("depends_on macos: :ventura");
    const sums = await readFile(join(root, "publication/SHA256SUMS"), "utf8");
    expect(sums.trim().split("\n").map((line) => line.split("  ")[1])).toEqual([
      "manifest.json",
      "Casks/butter-paper.rb",
      "Casks/butter-paper@beta.rb",
    ]);
  });

  it("updates only the Beta cask from a beta release", async () => {
    const root = await assets(macos("Beta-"));
    const manifest = await buildHomebrewPublication({
      tag: "v0.0.33-beta.2",
      commit,
      assetsDir: root,
      outputDir: join(root, "publication"),
      runId: 7,
      runAttempt: 1,
      jobs: ["package macos-arm64"],
    });
    expect(manifest.channel).toBe("beta");
    expect(manifest.casks).toEqual(["butter-paper@beta.rb"]);
    expect(manifest.applications).toEqual({ beta: "Butter Paper Beta.app" });
  });

  it("refuses a release missing a macOS package", async () => {
    const root = await assets(macos("").slice(0, 1));
    await expect(
      buildHomebrewPublication({
        tag: "v0.0.32",
        commit,
        assetsDir: root,
        outputDir: join(root, "publication"),
        runId: 7,
        runAttempt: 1,
        jobs: ["package macos-arm64"],
      }),
    ).rejects.toThrow(/missing release asset Butter-Paper-macOS-x64.zip/);
  });
});

describe("release versions", () => {
  it("accepts X.Y.Z and X.Y.Z-beta.N only", () => {
    for (const version of ["0.0.32", "1.2.3", "0.0.33-beta.1", "0.0.33-beta.12"]) {
      expect(VERSION.test(version)).toBe(true);
    }
    for (const version of ["0.0.32-beta", "0.0.32-beta.0", "0.0.32-rc.1", "v0.0.32", "0.32"]) {
      expect(VERSION.test(version)).toBe(false);
    }
  });

  it("finds one version in every file a release checks", async () => {
    const versions = await versionFiles();
    expect(new Set(Object.values(versions)).size).toBe(1);
  });
});
