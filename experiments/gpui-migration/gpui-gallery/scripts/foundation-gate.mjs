#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const galleryDirectory = resolve(scriptDirectory, "..");

export function parseGitSource(source) {
  if (!source.startsWith("git+")) return null;
  const url = new URL(source.slice(4));
  const revision = url.searchParams.get("rev");
  const resolved = url.hash.slice(1);
  url.search = "";
  url.hash = "";
  return { url: url.toString().replace(/\/$/, ""), revision, resolved };
}

export function licenseIsForbiddenOnly(expression) {
  const branches = expression
    .replaceAll("(", "")
    .replaceAll(")", "")
    .split(/\s+OR\s+/i)
    .map((branch) => branch.trim());
  return branches.every((branch) => /(?:A?GPL|LGPL)-/i.test(branch));
}

export function collectReachable(metadata) {
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const reachable = new Set();
  const pending = [metadata.resolve.root];

  while (pending.length > 0) {
    const id = pending.pop();
    if (reachable.has(id)) continue;
    reachable.add(id);
    const node = nodes.get(id);
    if (!node) continue;
    for (const dependency of node.deps) {
      if (dependency.dep_kinds.some((kind) => kind.kind !== "dev")) {
        pending.push(dependency.pkg);
      }
    }
  }

  return [...reachable].map((id) => packages.get(id)).filter(Boolean);
}

export function parseCargoLock(lockfile) {
  const packages = new Map();
  for (const block of lockfile.split("[[package]]").slice(1)) {
    const field = (name) => block.match(new RegExp(`^${name} = "([^"]+)"`, "m"))?.[1] ?? null;
    const name = field("name");
    const version = field("version");
    const source = field("source");
    if (!name || !version) continue;
    packages.set(`${name}\u0000${version}\u0000${source ?? ""}`, {
      name,
      version,
      source,
      checksum: field("checksum"),
    });
  }
  return packages;
}

export function sha256(contents) {
  return createHash("sha256").update(contents).digest("hex");
}

export function missingReleasePlatformFeatures(manifest, requiredFeaturesByPackage) {
  const releaseDependencyLines = manifest
    .slice(manifest.indexOf("[dependencies]"), manifest.indexOf("[dev-dependencies]"))
    .split("\n");
  const missing = [];
  for (const [packageName, requiredFeatures] of Object.entries(requiredFeaturesByPackage)) {
    const dependencyLine = releaseDependencyLines.find((line) =>
      line.startsWith(`${packageName} =`),
    );
    for (const feature of requiredFeatures) {
      if (!dependencyLine?.includes(`"${feature}"`)) {
        missing.push({ packageName, feature });
      }
    }
  }
  return missing;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: galleryDirectory,
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
    ...options,
  });
  if (result.error) throw result.error;
  return result;
}

function validatePins(policy, errors) {
  const manifest = readFileSync(resolve(galleryDirectory, "Cargo.toml"), "utf8");
  const toolchain = readFileSync(resolve(galleryDirectory, "rust-toolchain.toml"), "utf8");

  const gpuiPins = [
    ...manifest.matchAll(
      /(?:gpui|gpui_platform)\s*=\s*\{[^\n]*git\s*=\s*"([^"]+)"[^\n]*rev\s*=\s*"([0-9a-f]{40})"/g,
    ),
  ].map((match) => ({ source: match[1], revision: match[2] }));
  if (gpuiPins.length !== 4) {
    errors.push(`expected four direct GPUI source/revision pins, found ${gpuiPins.length}`);
  }
  for (const { source, revision } of gpuiPins) {
    if (source !== policy.gpuiSource) {
      errors.push(`direct GPUI source ${source} does not match ${policy.gpuiSource}`);
    }
    if (revision !== policy.gpuiRevision) {
      errors.push(`direct GPUI revision ${revision} does not match ${policy.gpuiRevision}`);
    }
  }
  if (policy.allowedGitSources[policy.gpuiSource] !== policy.gpuiRevision) {
    errors.push("accepted GPUI source is not bound to its exact revision in allowedGitSources");
  }

  if (!toolchain.includes(`channel = "${policy.rustToolchain}"`)) {
    errors.push(`rust-toolchain.toml does not pin Rust ${policy.rustToolchain}`);
  }
  for (const target of policy.targets) {
    if (!toolchain.includes(`"${target}"`)) {
      errors.push(`rust-toolchain.toml does not include ${target}`);
    }
  }

  const releaseDependencyLines = manifest
    .slice(manifest.indexOf("[dependencies]"), manifest.indexOf("[dev-dependencies]"))
    .split("\n");
  for (const feature of policy.forbiddenReleaseFeatures) {
    if (releaseDependencyLines.some((line) => line.includes(`"${feature}"`))) {
      errors.push(`release dependency graph enables forbidden feature ${feature}`);
    }
  }
  for (const { packageName, feature } of missingReleasePlatformFeatures(
    manifest,
    policy.requiredPlatformFeatures,
  )) {
    errors.push(`release ${packageName} dependency does not enable ${feature}`);
  }
  for (const packageName of ["gpui", "gpui_platform"]) {
    const dependencyLine = releaseDependencyLines.find((line) =>
      line.startsWith(`${packageName} =`),
    );
    if (!dependencyLine?.includes("default-features = false")) {
      errors.push(`release ${packageName} dependency must disable default features`);
    }
  }

  if (
    policy.advisoryPolicy.vulnerabilities !== "deny" ||
    policy.advisoryPolicy.yanked !== "deny" ||
    policy.advisoryPolicy.unmaintained !== "warn" ||
    policy.advisoryPolicy.ignoredAdvisories.length !== 0
  ) {
    errors.push("foundation advisory policy must deny vulnerabilities/yanked, warn unmaintained, and ignore none");
  }
}

function validateLicense(policy, pkg, target, errors) {
  if (pkg.license) {
    if (licenseIsForbiddenOnly(pkg.license)) {
      errors.push(`${target}: ${pkg.name} ${pkg.version} is ${pkg.license}`);
    }
    return { expression: pkg.license, evidence: "manifest" };
  }

  const clarification = policy.licenseClarifications?.[`${pkg.name}@${pkg.version}`];
  if (!clarification) {
    errors.push(`${target}: ${pkg.name} ${pkg.version} has no declared license`);
    return null;
  }

  const git = pkg.source ? parseGitSource(pkg.source) : null;
  if (
    !git ||
    git.url !== clarification.sourceUrl ||
    git.revision !== clarification.sourceRevision ||
    git.resolved !== clarification.sourceRevision
  ) {
    errors.push(`${target}: ${pkg.name} ${pkg.version} license clarification source mismatch`);
    return null;
  }
  if (licenseIsForbiddenOnly(clarification.expression)) {
    errors.push(
      `${target}: ${pkg.name} ${pkg.version} clarification is ${clarification.expression}`,
    );
    return null;
  }

  const licensePath = resolve(dirname(pkg.manifest_path), clarification.licensePath);
  let actualHash;
  try {
    actualHash = sha256(readFileSync(licensePath));
  } catch (error) {
    errors.push(`${target}: ${pkg.name} ${pkg.version} license evidence unreadable: ${error.message}`);
    return null;
  }
  if (actualHash !== clarification.licenseFileSha256) {
    errors.push(`${target}: ${pkg.name} ${pkg.version} license evidence checksum mismatch`);
    return null;
  }
  return {
    expression: clarification.expression,
    evidence: `${clarification.licensePath} sha256:${actualHash}`,
  };
}

function validatePackage(policy, pkg, lockedPackage, target, errors) {
  if (pkg.source === null) return;
  const license = validateLicense(policy, pkg, target, errors);

  if (pkg.source.startsWith("registry+")) {
    if (!policy.allowedRegistrySources.includes(pkg.source)) {
      errors.push(`${target}: ${pkg.name} uses unapproved registry ${pkg.source}`);
    }
    if (!lockedPackage?.checksum) {
      errors.push(`${target}: ${pkg.name} ${pkg.version} has no locked registry checksum`);
    }
    return license;
  }

  const git = parseGitSource(pkg.source);
  if (!git) {
    errors.push(`${target}: ${pkg.name} uses unknown source ${pkg.source}`);
    return license;
  }
  const expectedRevision = policy.allowedGitSources[git.url];
  if (!expectedRevision) {
    errors.push(`${target}: ${pkg.name} uses unapproved Git source ${git.url}`);
    return license;
  }
  if (git.revision !== expectedRevision || git.resolved !== expectedRevision) {
    errors.push(
      `${target}: ${pkg.name} Git pin ${git.revision ?? "missing"}#${git.resolved || "missing"} does not match ${expectedRevision}`,
    );
  }
  return license;
}

function metadataForTarget(target) {
  const result = run("cargo", [
    "metadata",
    "--format-version",
    "1",
    "--locked",
    "--offline",
    "--all-features",
    "--filter-platform",
    target,
  ]);
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || `cargo metadata failed for ${target}`);
  }
  return JSON.parse(result.stdout);
}

function validateToolVersion(policy, errors) {
  const result = run("rustc", ["--version"]);
  if (result.status !== 0) {
    errors.push("rustc --version failed");
    return null;
  }
  const version = result.stdout.trim();
  if (!version.startsWith(`rustc ${policy.rustToolchain} `)) {
    errors.push(`active compiler is ${version}; expected rustc ${policy.rustToolchain}`);
  }
  return version;
}

function runCargoDeny(policy, errors, cargoDeny) {
  const version = run(cargoDeny, ["--version"]);
  if (version.status !== 0) {
    errors.push(`${cargoDeny} is unavailable; install cargo-deny ${policy.cargoDenyVersion}`);
    return;
  }
  if (!version.stdout.includes(`cargo-deny ${policy.cargoDenyVersion}`)) {
    errors.push(
      `${cargoDeny} reports ${version.stdout.trim()}; expected cargo-deny ${policy.cargoDenyVersion}`,
    );
    return;
  }
  const result = run(cargoDeny, [
    "--manifest-path",
    "Cargo.toml",
    "--config",
    "deny.toml",
    "--features",
    "gallery",
    "--no-default-features",
    "--exclude-dev",
    "--locked",
    "check",
    "--warn",
    "unmaintained",
    "advisories",
    "licenses",
    "sources",
  ]);
  if (result.status !== 0) {
    const details = [result.stdout, result.stderr].filter(Boolean).join("\n").trim();
    errors.push(`cargo-deny rejected the graph${details ? `:\n${details}` : ""}`);
  }
}

function compileTarget(target, errors) {
  const result = run(
    "cargo",
    [
      "check",
      "--locked",
      "--features",
      "gallery",
      "--bin",
      "butter-paper-gpui-gallery",
      "--target",
      target,
      "-j",
      "1",
    ],
    { stdio: "inherit" },
  );
  if (result.status !== 0) errors.push(`compile probe failed for ${target}`);
}

export function main(argv = process.argv.slice(2)) {
  const policy = JSON.parse(
    readFileSync(resolve(galleryDirectory, "foundation-policy.json"), "utf8"),
  );
  const lockPackages = parseCargoLock(
    readFileSync(resolve(galleryDirectory, "Cargo.lock"), "utf8"),
  );
  const errors = [];
  const inventory = new Map();
  validatePins(policy, errors);
  const compiler = validateToolVersion(policy, errors);

  for (const target of policy.targets) {
    let packages;
    try {
      packages = collectReachable(metadataForTarget(target));
    } catch (error) {
      errors.push(`${target}: metadata probe failed: ${error.message}`);
      continue;
    }
    for (const pkg of packages) {
      const lockedPackage = lockPackages.get(
        `${pkg.name}\u0000${pkg.version}\u0000${pkg.source ?? ""}`,
      );
      const license = validatePackage(policy, pkg, lockedPackage, target, errors);
      if (pkg.source === null) continue;
      const key = `${pkg.name} ${pkg.version} ${pkg.source}`;
      const entry = inventory.get(key) ?? {
        name: pkg.name,
        version: pkg.version,
        license: license?.expression ?? pkg.license,
        licenseEvidence: license?.evidence ?? null,
        repository: pkg.repository,
        source: pkg.source,
        checksum: lockedPackage?.checksum ?? null,
        targets: [],
      };
      entry.targets.push(target);
      inventory.set(key, entry);
    }
  }

  const cargoDenyIndex = argv.indexOf("--cargo-deny");
  const cargoDeny = cargoDenyIndex >= 0 ? argv[cargoDenyIndex + 1] : "cargo-deny";
  if (!argv.includes("--metadata-only")) runCargoDeny(policy, errors, cargoDeny);

  const compileIndex = argv.indexOf("--compile-target");
  if (compileIndex >= 0) {
    const target = argv[compileIndex + 1];
    if (!policy.targets.includes(target)) {
      errors.push(`compile target ${target ?? "missing"} is not in the foundation policy`);
    } else {
      compileTarget(target, errors);
    }
  }

  const report = {
    schemaVersion: policy.schemaVersion,
    gpuiRevision: policy.gpuiRevision,
    rustToolchain: policy.rustToolchain,
    activeCompiler: compiler,
    targets: policy.targets,
    packages: [...inventory.values()].sort((a, b) =>
      `${a.name} ${a.version}`.localeCompare(`${b.name} ${b.version}`),
    ),
    errors: [...new Set(errors)],
  };

  if (argv.includes("--json")) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else {
    process.stdout.write(
      `foundation graph: ${report.packages.length} packages across ${report.targets.length} targets\n`,
    );
    if (report.errors.length === 0) {
      process.stdout.write("foundation gate: passed\n");
    } else {
      process.stderr.write(`foundation gate: failed (${report.errors.length} unique errors)\n`);
      for (const error of report.errors) process.stderr.write(`- ${error}\n`);
    }
  }
  return report.errors.length === 0 ? 0 : 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = main();
}
