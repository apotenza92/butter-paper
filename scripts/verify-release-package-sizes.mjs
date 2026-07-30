#!/usr/bin/env node

import { existsSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';

const MAX_ARM64_INSTALLER_BYTES = 150 * 1024 * 1024;
const MAX_ARM64_TO_X64_RATIO = 1.25;

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
    for (const [arch, candidate] of Object.entries(paths)) {
      if (!existsSync(candidate) || !statSync(candidate).isFile()) {
        throw new Error(`Windows ${variant}/${arch} installer is missing: ${candidate}`);
      }
    }
    const arm64Bytes = statSync(paths.arm64).size;
    const x64Bytes = statSync(paths.x64).size;
    if (arm64Bytes > MAX_ARM64_INSTALLER_BYTES) {
      throw new Error(
        `Windows ${variant}/arm64 installer is unexpectedly large: ${arm64Bytes} bytes `
        + `(maximum ${MAX_ARM64_INSTALLER_BYTES}).`,
      );
    }
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
  const directory = resolve(option('--directory') ?? 'publish/assets');
  const variantsOption = option('--variants');
  if (!variantsOption) {
    throw new Error(
      'Usage: node scripts/verify-release-package-sizes.mjs '
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
