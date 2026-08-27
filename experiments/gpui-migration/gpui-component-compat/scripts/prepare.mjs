#!/usr/bin/env node

import { access, mkdir, rename, rm } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

import {
  fileSha256,
  loadPolicy,
  probeDirectory,
  validatePreparedTree,
  verifySharedSourceInputs,
} from "./source-preparation.mjs";

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    encoding: "utf8",
    stdio: options.capture ? ["ignore", "pipe", "pipe"] : "inherit",
  });
  if (result.status !== 0) {
    const detail = options.capture ? `\n${result.stderr || result.stdout}` : "";
    throw new Error(`${command} ${args.join(" ")} failed${detail}`);
  }
  return options.capture ? result.stdout.trim() : "";
}

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function verifyCheckout(root, policy) {
  const revision = run("git", ["rev-parse", "HEAD"], { cwd: root, capture: true });
  if (revision !== policy.component.revision) {
    throw new Error(`component revision drifted: ${revision}`);
  }
  const tree = run("git", ["rev-parse", "HEAD^{tree}"], { cwd: root, capture: true });
  if (tree !== policy.component.tree) {
    throw new Error(`component source tree drifted: ${tree}`);
  }

  const changed = run("git", ["status", "--short"], { cwd: root, capture: true });
  if (changed !== "M  Cargo.toml") {
    throw new Error(`prepared source changed outside the reviewed patch: ${changed || "clean tree"}`);
  }

  const patchDigest = await fileSha256(join(probeDirectory, policy.patch.path));
  if (patchDigest !== policy.patch.sha256) {
    throw new Error(`preparation patch checksum drifted: ${patchDigest}`);
  }
  return validatePreparedTree(root, policy);
}

async function prepare(source) {
  const policy = await loadPolicy();
  const output = join(probeDirectory, policy.prepared.directory);
  if (await exists(output)) {
    const digest = await verifyCheckout(output, policy);
    return { status: "reused", output, digest };
  }

  await mkdir(dirname(output), { recursive: true });
  const temporary = `${output}.tmp-${process.pid}`;
  try {
    const cloneArgs = source
      ? ["clone", "--no-hardlinks", "--no-checkout", resolve(source), temporary]
      : ["clone", "--filter=blob:none", "--no-checkout", policy.component.url, temporary];
    run("git", cloneArgs);
    run("git", ["checkout", "--detach", policy.component.revision], { cwd: temporary });
    run("git", ["apply", "--index", join(probeDirectory, policy.patch.path)], { cwd: temporary });

    const digest = await verifyCheckout(temporary, policy);
    await rename(temporary, output);
    return { status: "prepared", output, digest };
  } catch (error) {
    await rm(temporary, { recursive: true, force: true });
    throw error;
  }
}

const command = process.argv[2] ?? "prepare";
const sourceFlag = process.argv.indexOf("--source");
const source = sourceFlag >= 0 ? process.argv[sourceFlag + 1] : undefined;
const policy = await loadPolicy();
const output = join(probeDirectory, policy.prepared.directory);

if (command === "prepare") {
  process.stdout.write(`${JSON.stringify(await prepare(source), null, 2)}\n`);
} else if (command === "verify") {
  process.stdout.write(`${JSON.stringify({
    status: "verified",
    output,
    digest: await verifyCheckout(output, policy),
    sharedSources: await verifySharedSourceInputs(policy),
  }, null, 2)}\n`);
} else {
  throw new Error(`unknown command ${command}; use prepare or verify`);
}
