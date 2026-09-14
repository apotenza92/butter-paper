import { createHash } from "node:crypto";
import { lstat, readFile, readdir, readlink } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
export const probeDirectory = resolve(scriptDirectory, "..");

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

export async function fileSha256(path) {
  return sha256(await readFile(path));
}

export function validateSharedSourceReceipts(expected, actual) {
  if (actual.length !== expected.length) {
    throw new Error("shared experiment source receipt coverage drifted");
  }
  for (let index = 0; index < expected.length; index += 1) {
    if (actual[index]?.path !== expected[index].path) {
      throw new Error(`shared experiment source path drifted at receipt ${index}`);
    }
    if (actual[index]?.sha256 !== expected[index].sha256) {
      throw new Error(`shared experiment source checksum drifted for ${expected[index].path}`);
    }
  }
}

export async function verifySharedSourceInputs(policy) {
  const expected = policy.sharedExperimentSources ?? [];
  const migrationDirectory = resolve(probeDirectory, "..");
  const actual = [];
  for (const input of expected) {
    const path = resolve(probeDirectory, input.path);
    if (path !== migrationDirectory && !path.startsWith(`${migrationDirectory}/`)) {
      throw new Error(`shared experiment source escapes the migration boundary: ${input.path}`);
    }
    actual.push({ path: input.path, sha256: await fileSha256(path) });
  }
  validateSharedSourceReceipts(expected, actual);
  return actual;
}

async function treeEntries(root, directory = root) {
  const entries = [];
  for (const name of (await readdir(directory)).sort()) {
    if (name === ".git") continue;
    const path = join(directory, name);
    const stat = await lstat(path);
    const nameInTree = relative(root, path).split("\\").join("/");
    if (stat.isDirectory()) {
      entries.push(...await treeEntries(root, path));
    } else if (stat.isSymbolicLink()) {
      entries.push(`120000\0${nameInTree}\0${await readlink(path)}\n`);
    } else if (stat.isFile()) {
      const mode = stat.mode & 0o111 ? "100755" : "100644";
      const contents = await readFile(path);
      entries.push(`${mode}\0${nameInTree}\0${contents.length}\0${sha256(contents)}\n`);
    }
  }
  return entries;
}

export async function deterministicTreeDigest(root) {
  return sha256((await treeEntries(resolve(root))).join(""));
}

export function validatePreparedManifest(manifest, policy) {
  const gitLines = manifest.split("\n").filter((line) => /\bgit\s*=/.test(line));
  for (const line of gitLines) {
    if (/\bbranch\s*=/.test(line) || !/\brev\s*=\s*"[0-9a-f]{40}"/.test(line)) {
      throw new Error(`moving Git input: ${line.trim()}`);
    }
  }

  const zedLines = gitLines.filter((line) => line.includes(policy.zed.url));
  if (zedLines.length === 0 || zedLines.some((line) => !line.includes(`rev = "${policy.zed.revision}"`))) {
    throw new Error("prepared manifest must pin every Zed dependency to the reviewed revision");
  }

  for (const feature of policy.forbiddenFeatures) {
    if (manifest.includes(`"${feature}"`) || manifest.includes(`'${feature}'`)) {
      throw new Error(`prepared manifest contains forbidden feature ${feature}`);
    }
  }
}

export function validateCargoMetadata(metadata, policy) {
  const gpuiPackages = metadata.packages.filter((pkg) => pkg.name === "gpui");
  if (gpuiPackages.length !== 1) {
    throw new Error(`expected exactly one gpui package identity, found ${gpuiPackages.length}`);
  }
  if (!gpuiPackages[0].source?.includes(policy.zed.revision)) {
    throw new Error("gpui package does not use the reviewed Zed revision");
  }

  for (const pkg of metadata.packages) {
    if (policy.forbiddenPackages.includes(pkg.name)) {
      throw new Error(`resolved forbidden package ${pkg.name}`);
    }
    const replacement = policy.replacementPackages?.[pkg.name];
    if (replacement && (pkg.source !== replacement.source || pkg.license !== replacement.license)) {
      throw new Error(`replacement package ${pkg.name} has unreviewed source or license`);
    }
    const clarification = policy.licenseClarifications?.[pkg.name];
    if (!pkg.license && !pkg.license_file && !clarification) {
      throw new Error(`package ${pkg.name} is missing license metadata`);
    }
    if (clarification && !pkg.source?.includes(clarification.revision)) {
      throw new Error(`license clarification for ${pkg.name} does not match its source revision`);
    }
    for (const expression of policy.rejectedLicenseExpressions ?? []) {
      if (pkg.license === expression) {
        throw new Error(`package ${pkg.name} has rejected license ${pkg.license}`);
      }
    }
    if (pkg.source?.startsWith("git+")) {
      const [requested, precise] = pkg.source.slice(4).split("#");
      const parsed = new URL(requested);
      const sourceUrl = `${parsed.origin}${parsed.pathname}`;
      const expected = policy.allowedGitSources?.[sourceUrl];
      if (!expected) {
        throw new Error(`unapproved Git source ${sourceUrl}`);
      }
      if (precise !== expected || parsed.searchParams.get("rev") !== expected) {
        throw new Error(`Git source ${sourceUrl} is not pinned to ${expected}`);
      }
    }
  }

  for (const node of metadata.resolve?.nodes ?? []) {
    for (const feature of node.features ?? []) {
      if (policy.forbiddenFeatures.includes(feature)) {
        throw new Error(`resolved forbidden feature ${feature} on ${node.id}`);
      }
    }
  }
}

export async function loadPolicy() {
  return JSON.parse(await readFile(join(probeDirectory, "source-preparation-policy.json"), "utf8"));
}

export async function validatePreparedTree(root, policy) {
  const manifest = await readFile(join(root, "Cargo.toml"), "utf8");
  validatePreparedManifest(manifest, policy);

  const licenseDigest = await fileSha256(join(root, policy.component.licenseFile));
  if (licenseDigest !== policy.component.licenseSha256) {
    throw new Error(`component license checksum drifted: ${licenseDigest}`);
  }

  const digest = await deterministicTreeDigest(root);
  if (policy.prepared.treeSha256 !== "PENDING" && digest !== policy.prepared.treeSha256) {
    throw new Error(`prepared tree checksum drifted: ${digest}`);
  }
  return digest;
}
