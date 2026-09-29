import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { lstat, readFile, readdir, readlink } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
export const probeDirectory = resolve(scriptDirectory, "..");

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

export async function fileSha256(path) {
  return sha256(await readFile(path));
}

export function pathIsInside(root, candidate, pathApi = { relative, isAbsolute, sep }) {
  const within = pathApi.relative(root, candidate);
  return within !== ".." && !within.startsWith(`..${pathApi.sep}`) && !pathApi.isAbsolute(within);
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

export function validateAnnotationFontReceipts(expected, actual) {
  if (actual.length !== expected.length) {
    throw new Error("annotation font receipt coverage drifted");
  }
  for (let index = 0; index < expected.length; index += 1) {
    const wanted = expected[index];
    const observed = actual[index];
    if (observed?.asset !== wanted.asset || observed?.source !== wanted.source) {
      throw new Error(`annotation font path drifted at receipt ${index}`);
    }
    if (observed.bytes !== wanted.bytes || observed.sourceBytes !== wanted.bytes) {
      throw new Error(`annotation font size drifted for ${wanted.asset}`);
    }
    if (observed.sha256 !== wanted.sha256 || observed.sourceSha256 !== wanted.sha256) {
      throw new Error(`annotation font checksum drifted for ${wanted.asset}`);
    }
  }
}

export async function verifyAnnotationFontInputs(policy, repositoryRoot = resolve(probeDirectory, "../../..")) {
  const fontPolicy = policy.annotationFonts;
  const actual = [];
  for (const face of fontPolicy.faces) {
    const asset = await readFile(resolve(probeDirectory, face.asset));
    const source = await readFile(resolve(repositoryRoot, face.source));
    actual.push({
      asset: face.asset,
      source: face.source,
      bytes: asset.length,
      sourceBytes: source.length,
      sha256: sha256(asset),
      sourceSha256: sha256(source),
    });
  }
  validateAnnotationFontReceipts(fontPolicy.faces, actual);

  for (const pkg of fontPolicy.packages) {
    const packageRoot = resolve(repositoryRoot, "node_modules", pkg.name);
    const manifest = JSON.parse(await readFile(resolve(packageRoot, "package.json"), "utf8"));
    if (manifest.version !== pkg.version || manifest.license !== pkg.license) {
      throw new Error(`annotation font package metadata drifted for ${pkg.name}`);
    }
    const sourceLicense = await fileSha256(resolve(packageRoot, "LICENSE_FONT"));
    const assetLicense = await fileSha256(resolve(probeDirectory, pkg.licenseAsset));
    if (sourceLicense !== pkg.licenseSha256 || assetLicense !== pkg.licenseSha256) {
      throw new Error(`annotation font licence drifted for ${pkg.name}`);
    }
  }
  const wrapperLicense = await fileSha256(resolve(probeDirectory, fontPolicy.wrapperLicense.asset));
  if (wrapperLicense !== fontPolicy.wrapperLicense.sha256) {
    throw new Error("annotation font wrapper licence drifted");
  }
  return actual;
}

export async function verifySharedSourceInputs(policy) {
  const expected = policy.sharedExperimentSources ?? [];
  const migrationDirectory = resolve(probeDirectory, "..");
  const actual = [];
  for (const input of expected) {
    const path = resolve(probeDirectory, input.path);
    if (!pathIsInside(migrationDirectory, path)) {
      throw new Error(`shared experiment source escapes the migration boundary: ${input.path}`);
    }
    actual.push({ path: input.path, sha256: await fileSha256(path) });
  }
  validateSharedSourceReceipts(expected, actual);
  return actual;
}

async function treeEntries(root, directory = root, indexModes = new Map()) {
  const entries = [];
  for (const name of (await readdir(directory)).sort()) {
    if (name === ".git") continue;
    const path = join(directory, name);
    const stat = await lstat(path);
    const nameInTree = relative(root, path).split("\\").join("/");
    const indexedMode = indexModes.get(nameInTree);
    if (stat.isDirectory()) {
      entries.push(...await treeEntries(root, path, indexModes));
    } else if (stat.isSymbolicLink()) {
      entries.push(`120000\0${nameInTree}\0${await readlink(path)}\n`);
    } else if (stat.isFile() && indexedMode === "120000") {
      // Git for Windows materialises symlinks as regular files when symlink
      // creation is unavailable. The reviewed index mode remains authoritative
      // and the file bytes are the link target.
      entries.push(`120000\0${nameInTree}\0${await readFile(path, "utf8")}\n`);
    } else if (stat.isFile()) {
      const mode = indexedMode ?? (stat.mode & 0o111 ? "100755" : "100644");
      const contents = await readFile(path);
      entries.push(`${mode}\0${nameInTree}\0${contents.length}\0${sha256(contents)}\n`);
    }
  }
  return entries;
}

export async function deterministicTreeDigest(root) {
  const resolvedRoot = resolve(root);
  const indexed = spawnSync("git", ["-C", resolvedRoot, "ls-files", "--stage", "-z"], {
    encoding: "utf8",
  });
  if (indexed.status !== 0) {
    try {
      await lstat(join(resolvedRoot, ".git"));
      throw new Error(`cannot read prepared tree modes from Git: ${indexed.stderr || indexed.stdout}`);
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
      return sha256((await treeEntries(resolvedRoot)).join(""));
    }
  }
  const indexEntries = indexed.stdout.split("\0").filter(Boolean).map((entry) => {
    const match = entry.match(/^(100644|100755|120000) ([0-9a-f]+) 0\t(.+)$/s);
    if (!match) throw new Error(`invalid prepared Git index entry: ${entry}`);
    return `${match[1]}\0${match[3].split("\\").join("/")}\0${match[2]}\n`;
  });
  return sha256(indexEntries.join(""));
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

export function validateCargoMetadata(metadata, policy, preparedRoot = policy.zedPrepared && resolve(probeDirectory, policy.zedPrepared.directory)) {
  const gpuiPackages = metadata.packages.filter((pkg) => pkg.name === "gpui");
  if (gpuiPackages.length !== 1) {
    throw new Error(`expected exactly one gpui package identity, found ${gpuiPackages.length}`);
  }
  if (policy.zedPrepared) {
    for (const [name, path] of Object.entries(policy.zedPrepared.packages)) {
      const packages = metadata.packages.filter(pkg => pkg.name === name);
      if (packages.length !== 1 || packages[0].source !== null
          || resolve(packages[0].manifest_path) !== resolve(preparedRoot, path, "Cargo.toml")) {
        throw new Error(`prepared Zed package identity drifted: ${name}`);
      }
    }
    if (metadata.packages.some(pkg => pkg.source?.startsWith(`git+${policy.zed.url}?`))) {
      throw new Error("mixed prepared and Git Zed package identities");
    }
  } else if (!gpuiPackages[0].source?.includes(policy.zed.revision)) {
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
    if (clarification && !pkg.source?.includes(clarification.revision)
        && !(policy.zedPrepared?.packages[pkg.name] && clarification.revision === policy.zed.revision)) {
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

export function validateZedPatch(contents, policy) {
  if (sha256(contents) !== policy.zedPrepared.patch.sha256) throw new Error("renderer patch checksum drifted");
  const paths = [...contents.toString().matchAll(/^diff --git a\/\S+ b\/(\S+)$/gm)].map(match => match[1]).sort();
  const expected = policy.zedPrepared.changedFiles.filter(path => path !== "Cargo.toml").sort();
  if (JSON.stringify(paths) !== JSON.stringify(expected)) throw new Error("renderer patch file scope drifted");
}

export function validateZedLockTransition(before, after, policy) {
  const source = `source = "git+${policy.zed.url}?rev=${policy.zed.revision}#${policy.zed.revision}"\n`;
  const names = before.split("[[package]]").slice(1).filter(block => block.includes(source))
    .map(block => block.match(/^name = "([^"]+)"/m)?.[1]).sort();
  if (JSON.stringify(names) !== JSON.stringify(Object.keys(policy.zedPrepared.packages).sort())
      || before.split(source).join("") !== after) {
    throw new Error("Zed lock transition changed package versions, dependency edges, or unrelated sources");
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
