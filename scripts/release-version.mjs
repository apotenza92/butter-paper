#!/usr/bin/env node
// pnpm release:version X.Y.Z[-beta.N]
// Sets the version everywhere a release checks it and refreshes the
// source-preparation checksums of the files that changed. If CHANGELOG.md has
// no section for the version yet, the [Unreleased] notes become it.

import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const crate = join(root, "experiments/gpui-migration/gpui-migration");
export const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-beta\.([1-9]\d*))?$/;

async function replaceOnce(path, pattern, replacement, label) {
  const text = await readFile(path, "utf8");
  const matches = text.match(new RegExp(pattern.source, `${pattern.flags.replace("g", "")}g`));
  if (!matches || matches.length !== 1) {
    throw new Error(`${label}: expected exactly one version in ${path}`);
  }
  await writeFile(path, text.replace(pattern, replacement));
}

export async function versionFiles(rootDir = root) {
  const crateDir = join(rootDir, "experiments/gpui-migration/gpui-migration");
  const read = (path) => readFile(path, "utf8");
  const packageVersion = JSON.parse(await read(join(rootDir, "package.json"))).version;
  const cargo = /^version = "([^"]+)"$/m.exec(await read(join(crateDir, "Cargo.toml")))?.[1];
  const lock = /\[\[package\]\]\nname = "butter-paper-gpui-migration"\nversion = "([^"]+)"/.exec(
    await read(join(crateDir, "Cargo.lock")),
  )?.[1];
  const notices = /- Package: `butter-paper-gpui-migration` (\S+)/.exec(
    await read(join(crateDir, "THIRD_PARTY_NOTICES.md")),
  )?.[1];
  return { "package.json": packageVersion, "Cargo.toml": cargo, "Cargo.lock": lock, "THIRD_PARTY_NOTICES.md": notices };
}

async function refreshPreparationChecksums() {
  const policyPath = join(crate, "source-preparation-policy.json");
  let raw = await readFile(policyPath, "utf8");
  const refreshed = [];
  for (const entry of JSON.parse(raw).sharedExperimentSources) {
    const digest = createHash("sha256")
      .update(await readFile(normalize(join(crate, entry.path))))
      .digest("hex");
    if (digest !== entry.sha256) {
      raw = raw.replace(entry.sha256, digest);
      refreshed.push(entry.path);
    }
  }
  await writeFile(policyPath, raw);
  return refreshed;
}

async function ensureChangelogSection(version) {
  const path = join(root, "CHANGELOG.md");
  const text = await readFile(path, "utf8");
  if (text.includes(`\n## [${version}]\n`)) return false;
  const marker = "## [Unreleased]\n";
  if (!text.includes(marker)) throw new Error("CHANGELOG.md has no ## [Unreleased] section");
  await writeFile(path, text.replace(marker, `${marker}\n## [${version}]\n`));
  return true;
}

export async function setVersion(version) {
  const match = VERSION.exec(version ?? "");
  if (!match) throw new Error("usage: pnpm release:version X.Y.Z or X.Y.Z-beta.N");
  const core = `${match[1]}.${match[2]}.${match[3]}`;
  await replaceOnce(join(root, "package.json"), /^  "version": "[^"]+",$/m, `  "version": "${version}",`, "package.json");
  await replaceOnce(join(crate, "Cargo.toml"), /^version = "[^"]+"$/m, `version = "${version}"`, "Cargo.toml");
  await replaceOnce(
    join(crate, "Cargo.lock"),
    /(\[\[package\]\]\nname = "butter-paper-gpui-migration"\nversion = )"[^"]+"/,
    `$1"${version}"`,
    "Cargo.lock",
  );
  await replaceOnce(
    join(crate, "THIRD_PARTY_NOTICES.md"),
    /(- Package: `butter-paper-gpui-migration` )\S+/,
    `$1${version}`,
    "THIRD_PARTY_NOTICES.md",
  );
  // The development bundle template carries macOS's X.Y.Z form.
  const plistPath = join(crate, "bundle/Info.plist");
  const plist = await readFile(plistPath, "utf8");
  await writeFile(
    plistPath,
    plist.replace(
      /(<key>CFBundle(?:Short)?Version(?:String)?<\/key>\s*<string>)[^<]+(<\/string>)/g,
      `$1${core}$2`,
    ),
  );
  const refreshed = await refreshPreparationChecksums();
  const changelog = await ensureChangelogSection(version);
  return { refreshed, changelog };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const version = process.argv[2];
  setVersion(version)
    .then(({ refreshed, changelog }) => {
      process.stdout.write(`Version set to ${version}.\n`);
      if (refreshed.length) process.stdout.write(`Refreshed checksums: ${refreshed.join(", ")}\n`);
      process.stdout.write(
        changelog
          ? `Started CHANGELOG.md ## [${version}] from [Unreleased]; check the notes, commit, then run pnpm release.\n`
          : `CHANGELOG.md already has ## [${version}]; commit, then run pnpm release.\n`,
      );
    })
    .catch((error) => {
      process.stderr.write(`${error.message}\n`);
      process.exitCode = 1;
    });
}
