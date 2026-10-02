#!/usr/bin/env node
// pnpm release [--skip-native-tests]
// Runs pnpm release:check, then pushes the vX.Y.Z (or vX.Y.Z-beta.N) tag.
// The Release workflow builds, signs, publishes and updates Homebrew.

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { releaseCheck } from "./release-check.mjs";

function git(...args) {
  const result = spawnSync("git", args, { stdio: "inherit" });
  if (result.status !== 0) throw new Error(`git ${args.join(" ")} failed`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  releaseCheck({ skipNativeTests: process.argv.includes("--skip-native-tests") })
    .then(({ tag, version }) => {
      git("tag", "-a", tag, "-m", `Butter Paper ${version}`);
      git("push", "origin", tag);
      process.stdout.write(
        `\n✓ Pushed ${tag}. Follow the build: gh run watch $(gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId')\n`,
      );
    })
    .catch((error) => {
      process.stderr.write(`\n✗ ${error.message}\n`);
      process.exitCode = 1;
    });
}
