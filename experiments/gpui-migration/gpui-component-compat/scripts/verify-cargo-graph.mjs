#!/usr/bin/env node

import { spawnSync } from "node:child_process";

import {
  loadPolicy,
  probeDirectory,
  validateCargoMetadata,
} from "./source-preparation.mjs";

const result = spawnSync(
  "cargo",
  ["metadata", "--locked", "--offline", "--format-version", "1"],
  {
    cwd: probeDirectory,
    encoding: "utf8",
    env: { ...process.env, CARGO_NET_OFFLINE: "true" },
    maxBuffer: 128 * 1024 * 1024,
  },
);

if (result.status !== 0) {
  process.stderr.write(result.stderr || result.stdout);
  process.exit(result.status ?? 1);
}

const metadata = JSON.parse(result.stdout);
const policy = await loadPolicy();
validateCargoMetadata(metadata, policy);

const packages = metadata.packages;
const gpui = packages.find((pkg) => pkg.name === "gpui");
const component = packages.find((pkg) => pkg.name === "gpui-component");
const replacement = packages.find((pkg) => pkg.name === "ztracing");
if (!component || component.version !== "0.5.2" || component.source !== null) {
  throw new Error("gpui-component is not the reviewed prepared local package");
}

process.stdout.write(`${JSON.stringify({
  status: "verified",
  packageCount: packages.length,
  gpui: { version: gpui.version, source: gpui.source, license: gpui.license },
  component: { version: component.version, source: component.source, license: component.license },
  tracingReplacement: {
    version: replacement.version,
    source: replacement.source,
    license: replacement.license,
  },
  forbiddenPackages: policy.forbiddenPackages.filter((name) => packages.some((pkg) => pkg.name === name)),
}, null, 2)}\n`);
