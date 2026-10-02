#!/usr/bin/env node
// pnpm release:check [--skip-native-tests]
// Everything the release workflow would reject, checked locally first: the
// repository state, versions, changelog, PDFium approval, the workflow, the
// Homebrew bundle, a macOS packaging dry run, pnpm check and the native tests.

import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildHomebrewPublication } from "./build-homebrew-publication.mjs";
import { VERSION, versionFiles } from "./release-version.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const crate = join(root, "experiments/gpui-migration/gpui-migration");
const phone = join(root, "experiments/phone-signature-prototype");
const developerDir = process.env.DEVELOPER_DIR ?? "/Applications/Xcode-beta.app/Contents/Developer";

function run(command, args, { cwd = root, env = {}, quiet = false } = {}) {
  const result = spawnSync(command, args, {
    cwd,
    env: { ...process.env, ...env },
    encoding: "utf8",
    stdio: quiet ? "pipe" : "inherit",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed${quiet ? `:\n${result.stdout}${result.stderr}` : ""}`);
  }
  return (result.stdout ?? "").trim();
}

const step = (name) => process.stdout.write(`\n▸ ${name}\n`);

export async function releaseCheck({ skipNativeTests = false } = {}) {
  step("Repository state");
  if (run("git", ["branch", "--show-current"], { quiet: true }) !== "main") throw new Error("release from main");
  if (run("git", ["status", "--porcelain"], { quiet: true })) throw new Error("the working tree has uncommitted changes");
  run("git", ["fetch", "--quiet", "origin", "main", "--tags"], { quiet: true });
  if (run("git", ["rev-parse", "HEAD"], { quiet: true }) !== run("git", ["rev-parse", "origin/main"], { quiet: true }))
    throw new Error("HEAD must equal origin/main (push or pull first)");

  step("Version and changelog");
  const versions = await versionFiles(root);
  const version = versions["package.json"];
  if (!VERSION.test(version ?? "")) throw new Error(`package.json version ${version} is not X.Y.Z or X.Y.Z-beta.N`);
  for (const [file, value] of Object.entries(versions)) {
    if (value !== version) throw new Error(`${file} has ${value}, expected ${version} (run pnpm release:version ${version})`);
  }
  const tag = `v${version}`;
  if (run("git", ["tag", "--list", tag], { quiet: true }) || run("git", ["ls-remote", "--tags", "origin", tag], { quiet: true }))
    throw new Error(`${tag} already exists; releases are immutable, so bump the version`);
  const changelog = await readFile(join(root, "CHANGELOG.md"), "utf8");
  const section = changelog.split(`\n## [${version}]\n`)[1]?.split(/\n## \[/)[0]?.trim();
  if (!section) throw new Error(`CHANGELOG.md needs a non-empty ## [${version}] section`);
  process.stdout.write(`${tag}: ${Object.keys(versions).join(", ")} agree; changelog section present.\n`);

  step("PDFium approval");
  const runId = run("gh", ["variable", "get", "BP_PDFIUM_APPROVAL_RUN_ID"], { quiet: true });
  const artifacts = JSON.parse(run("gh", ["api", `repos/{owner}/{repo}/actions/runs/${runId}/artifacts`], { quiet: true }));
  const approved = artifacts.artifacts.find((artifact) => artifact.name === "gpui-pdfium-production-approved");
  if (!approved || approved.expired) throw new Error(`the PDFium approval in run ${runId} has expired; re-run the approval workflow`);
  const daysLeft = (Date.parse(approved.expires_at) - Date.now()) / 86_400_000;
  process.stdout.write(`Run ${runId}: approved PDFium artifact expires in ${daysLeft.toFixed(1)} days.\n`);
  if (daysLeft < 7) process.stdout.write("⚠ Re-run the PDFium approval soon.\n");

  step("Release workflow");
  run("actionlint", [".github/workflows/release.yml"]);

  const scratch = await realpath(await mkdtemp(join(tmpdir(), "bp-release-check-")));
  try {
    step("Homebrew bundle");
    const channels = version.includes("-beta.") ? ["Beta-"] : ["", "Beta-"];
    for (const prefix of channels) {
      for (const arch of ["arm64", "x64"]) {
        await writeFile(join(scratch, `Butter-Paper-${prefix}macOS-${arch}.zip`), `placeholder ${prefix}${arch}\n`);
      }
    }
    const manifest = await buildHomebrewPublication({
      tag,
      commit: run("git", ["rev-parse", "HEAD"], { quiet: true }),
      assetsDir: scratch,
      outputDir: join(scratch, "publication"),
      runId: 1,
      runAttempt: 1,
      jobs: ["package macos-arm64"],
    });
    process.stdout.write(`Casks: ${manifest.casks.join(", ")}\n`);

    step("macOS packaging dry run (stub binaries)");
    run("python3", ["prepare.py"], { cwd: phone, quiet: true });
    run("node", [
      "collect-go-notices.mjs",
      "--module-root",
      "dist/qrcp-3b176183de83bdb945e733e978497fb727ede2c5",
      "--output",
      "dist/PHONE_HELPER_THIRD_PARTY_NOTICES.md",
    ], {
      cwd: phone,
      quiet: true,
      env: { GOCACHE: join(phone, "dist/gocache"), GOMODCACHE: join(phone, "dist/gomodcache"), GOTOOLCHAIN: "local" },
    });
    await writeFile(join(scratch, "stub.c"), "int main(void) { return 0; }\n");
    const channelsFor = version.includes("-beta.") ? ["beta"] : ["stable", "beta"];
    for (const [arch, target] of [["arm64", "aarch64-apple-darwin"], ["x86_64", "x86_64-apple-darwin"]]) {
      const stub = join(scratch, `stub-${arch}`);
      run("clang", ["-target", `${arch}-apple-macos13.0`, join(scratch, "stub.c"), "-o", stub], { quiet: true });
      for (const channel of channelsFor) {
        const work = join(scratch, `${target}-${channel}`);
        run("node", [
          join(crate, "scripts/prepare-macos-production-inputs.mjs"),
          "--application", stub, "--worker", stub, "--camera-helper", stub, "--phone-helper", stub,
          "--target", target, "--minimum-system-version", "13.0", "--build-version", "1",
          "--channel", channel, "--output-root", join(work, "input"), "--receipt", join(work, "receipt.json"),
        ], { quiet: true, env: { DEVELOPER_DIR: developerDir } });
        process.stdout.write(`${target} ${channel}: inputs prepared\n`);
      }
    }
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }

  step("pnpm check");
  run("pnpm", ["check"]);

  if (!skipNativeTests) {
    step("Native tests");
    run("cargo", ["test", "--quiet"], {
      cwd: crate,
      env: { DEVELOPER_DIR: developerDir, CARGO_TARGET_DIR: join(crate, "../.build-targets/gpui-migration") },
    });
  }
  return { tag, version };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  releaseCheck({ skipNativeTests: process.argv.includes("--skip-native-tests") })
    .then(({ tag }) => process.stdout.write(`\n✓ ${tag} is ready to release.\n`))
    .catch((error) => {
      process.stderr.write(`\n✗ ${error.message}\n`);
      process.exitCode = 1;
    });
}
