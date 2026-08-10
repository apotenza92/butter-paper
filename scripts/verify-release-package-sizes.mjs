#!/usr/bin/env node

import { existsSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';

// The release now bundles one matching Java 21 sidecar runtime. Keep enough
// headroom for that intentional runtime while retaining the ARM64:x64 ratio
// guard and rejecting an additional/duplicated runtime footprint.
const MAX_ARM64_INSTALLER_BYTES = 250 * 1024 * 1024;
const MAX_ARM64_TO_X64_RATIO = 1.25;

export function validateWindowsArm64InstallerSize(filePath, label = 'Windows ARM64 installer') {
  const candidate = resolve(filePath);
  if (!existsSync(candidate) || !statSync(candidate).isFile()) {
    throw new Error(`${label} is missing: ${candidate}`);
  }
  const bytes = statSync(candidate).size;
  if (bytes > MAX_ARM64_INSTALLER_BYTES) {
    throw new Error(
      `${label} is unexpectedly large: ${bytes} bytes `
      + `(maximum ${MAX_ARM64_INSTALLER_BYTES}).`,
    );
  }
  return { bytes, filePath: candidate, label };
}

export function validateWindowsInstallerSizes(directory, variants) {
  const results = [];
  for (const variant of variants) {
    if (!['stable', 'beta'].includes(variant)) {
      throw new Error(`Release variant must be stable or beta, received: ${variant}`);
    }
    const prefix = variant === 'beta' ? 'Butter-Paper-Beta' : 'Butter-Paper';
    const paths = Object.fromEntries(['arm64', 'x64'].map((arch) => [
      arch,
      resolve(directory, `${prefix}-Windows-${arch}-Setup.exe`),
    ]));
    const { bytes: arm64Bytes } = validateWindowsArm64InstallerSize(
      paths.arm64,
      `Windows ${variant}/arm64 installer`,
    );
    if (!existsSync(paths.x64) || !statSync(paths.x64).isFile()) {
      throw new Error(`Windows ${variant}/x64 installer is missing: ${paths.x64}`);
    }
    const x64Bytes = statSync(paths.x64).size;
    if (arm64Bytes > x64Bytes * MAX_ARM64_TO_X64_RATIO) {
      throw new Error(
        `Windows ${variant}/arm64 installer is disproportionately large: ${arm64Bytes} bytes `
        + `versus ${x64Bytes} bytes for x64.`,
      );
    }
    results.push({ arm64Bytes, variant, x64Bytes });
  }
  return results;
}

function option(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : null;
}

export function main() {
  const arm64Installer = option('--arm64-installer');
  if (arm64Installer) {
    const result = validateWindowsArm64InstallerSize(
      arm64Installer,
      option('--label') ?? 'Windows ARM64 installer',
    );
    console.log(`${result.label} size passed: ${result.bytes} bytes.`);
    return;
  }

  const directory = resolve(option('--directory') ?? 'publish/assets');
  const variantsOption = option('--variants');
  if (!variantsOption) {
    throw new Error(
      'Usage: node scripts/verify-release-package-sizes.mjs '
      + '--arm64-installer <file> [--label <label>] or '
      + '--directory <assets> --variants stable,beta',
    );
  }
  const variants = variantsOption.split(',').filter(Boolean);
  for (const result of validateWindowsInstallerSizes(directory, variants)) {
    console.log(
      `Windows ${result.variant} installer sizes passed: `
      + `arm64=${result.arm64Bytes}, x64=${result.x64Bytes}.`,
    );
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main();
}
