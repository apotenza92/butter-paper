#!/usr/bin/env node
// Builds the checksum-sealed Homebrew publication the tap
// (apotenza92/homebrew-tap, scripts/homebrew_publication.py) validates:
// manifest.json, Casks/*.rb and SHA256SUMS. A stable release also advances
// the Beta cask, as the tap registry declares (stable_advances_beta).

import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY = "apotenza92/butter-paper";
const MINIMUM_MACOS = "13.0";
const ARCHITECTURES = ["arm64", "x64"];
const CHANNELS = {
  stable: {
    cask: "butter-paper.rb",
    token: "butter-paper",
    name: "Butter Paper",
    desc: "Cross-platform PDF review and markup",
    application: "Butter Paper.app",
    bundleIdentifier: "com.butterpaper.desktop",
    asset: (arch) => `Butter-Paper-macOS-${arch}.zip`,
    zap: [
      "~/Library/Application Support/Butter Paper",
      "~/Library/Application Support/com.butterpaper.desktop",
      "~/Library/Caches/com.butterpaper.desktop",
      "~/Library/Preferences/com.butterpaper.desktop.plist",
      "~/Library/Saved Application State/com.butterpaper.desktop.savedState",
    ],
  },
  beta: {
    cask: "butter-paper@beta.rb",
    token: "butter-paper@beta",
    name: "Butter Paper Beta",
    desc: "Cross-platform PDF review and markup (beta channel)",
    application: "Butter Paper Beta.app",
    bundleIdentifier: "com.butterpaper.desktop.beta",
    asset: (arch) => `Butter-Paper-Beta-macOS-${arch}.zip`,
    zap: [
      "~/Library/Application Support/Butter Paper Beta",
      "~/Library/Application Support/com.butterpaper.desktop.beta",
      "~/Library/Caches/com.butterpaper.desktop.beta",
      "~/Library/Preferences/com.butterpaper.desktop.beta.plist",
      "~/Library/Saved Application State/com.butterpaper.desktop.beta.savedState",
    ],
  },
};

const TAG = /^v(\d+\.\d+\.\d+)(-beta\.[1-9]\d*)?$/;
const COMMIT = /^[0-9a-f]{40}$/;

function fail(message) {
  throw new Error(message);
}

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

export function renderCask(channel, version, digests) {
  const config = CHANNELS[channel];
  const block = (arch, condition) => `  ${condition} do
    sha256 "${digests[arch]}"

    url "https://github.com/${REPOSITORY}/releases/download/v#{version}/${config.asset(arch)}"
  end`;
  return `cask "${config.token}" do
  version "${version}"

${block("arm64", "on_arm")}
${block("x64", "on_intel")}

  name "${config.name}"
  desc "${config.desc}"
  homepage "https://github.com/${REPOSITORY}"

  livecheck do
    skip "Updated by the Butter Paper release workflow"
  end

  auto_updates true
  depends_on macos: :ventura

  app "${config.application}"

  zap trash: [
${config.zap.map((path) => `    "${path}",`).join("\n")}
  ]
end
`;
}

export async function buildHomebrewPublication({
  tag,
  commit,
  assetsDir,
  outputDir,
  runId,
  runAttempt,
  jobs,
}) {
  const match = TAG.exec(tag ?? "");
  if (!match) fail("tag must be vX.Y.Z or vX.Y.Z-beta.N");
  if (!COMMIT.test(commit ?? "")) fail("commit must be a full lowercase SHA");
  if (!/^[1-9]\d*$/.test(String(runId)) || !/^[1-9]\d*$/.test(String(runAttempt)))
    fail("run id and attempt must be positive integers");
  if (!Array.isArray(jobs) || jobs.length === 0) fail("native validation jobs are required");
  const version = tag.slice(1);
  const channel = match[2] ? "beta" : "stable";
  const channels = channel === "beta" ? ["beta"] : ["stable", "beta"];
  const artifacts = [];
  const casks = [];
  const output = resolve(outputDir);
  await mkdir(join(output, "Casks"), { recursive: true });
  for (const cardChannel of channels) {
    const config = CHANNELS[cardChannel];
    const digests = {};
    for (const arch of ARCHITECTURES) {
      const name = config.asset(arch);
      const path = join(resolve(assetsDir), name);
      const info = await stat(path).catch(() => fail(`missing release asset ${name}`));
      if (!info.isFile() || info.size === 0) fail(`release asset ${name} is not a file`);
      const digest = sha256(await readFile(path));
      digests[arch] = digest;
      artifacts.push({
        name,
        url: `https://github.com/${REPOSITORY}/releases/download/${tag}/${name}`,
        size: info.size,
        sha256: digest,
        channel: cardChannel,
        architecture: arch,
      });
    }
    await writeFile(join(output, "Casks", config.cask), renderCask(cardChannel, version, digests));
    casks.push(config.cask);
  }
  const pick = (field) =>
    Object.fromEntries(channels.map((value) => [value, CHANNELS[value][field]]));
  const manifest = {
    schema_version: 1,
    product: "butter-paper",
    source_repository: REPOSITORY,
    release_tag: tag,
    release_commit: commit,
    channel,
    casks,
    artifacts,
    applications: pick("application"),
    bundle_identifiers: pick("bundleIdentifier"),
    architectures: ARCHITECTURES,
    minimum_macos: MINIMUM_MACOS,
    native_validation: {
      workflow_run_id: String(runId),
      workflow_run_attempt: String(runAttempt),
      jobs,
    },
  };
  await writeFile(join(output, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  const lines = [];
  for (const file of ["manifest.json", ...casks.sort().map((cask) => `Casks/${cask}`)]) {
    lines.push(`${sha256(await readFile(join(output, file)))}  ${file}`);
  }
  await writeFile(join(output, "SHA256SUMS"), `${lines.join("\n")}\n`);
  const entries = (await readdir(output)).sort();
  if (JSON.stringify(entries) !== JSON.stringify(["Casks", "SHA256SUMS", "manifest.json"]))
    fail("publication output must contain only Casks, SHA256SUMS and manifest.json");
  return manifest;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = new Map();
  const argv = process.argv.slice(2);
  for (let index = 0; index < argv.length; index += 2) {
    if (!argv[index]?.startsWith("--") || argv[index + 1] === undefined) {
      fail("usage: build-homebrew-publication.mjs --tag TAG --commit SHA --assets DIR --output DIR --run-id N --run-attempt N --jobs a,b");
    }
    args.set(argv[index], argv[index + 1]);
  }
  buildHomebrewPublication({
    tag: args.get("--tag"),
    commit: args.get("--commit"),
    assetsDir: args.get("--assets"),
    outputDir: args.get("--output"),
    runId: args.get("--run-id"),
    runAttempt: args.get("--run-attempt"),
    jobs: (args.get("--jobs") ?? "").split(",").filter(Boolean),
  })
    .then((manifest) => process.stdout.write(`${JSON.stringify(manifest.casks)}\n`))
    .catch((error) => {
      process.stderr.write(`${error.message}\n`);
      process.exitCode = 1;
    });
}
