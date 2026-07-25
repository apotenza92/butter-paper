import { existsSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';

export function expectedReleaseAssetNames(channel) {
  if (!['stable', 'beta'].includes(channel)) {
    throw new Error(`Release channel must be stable or beta, received: ${channel}`);
  }
  const variants = channel === 'stable' ? ['stable', 'beta'] : ['beta'];
  const names = ['SHA256SUMS', 'homebrew-release.json'];
  for (const variant of variants) {
    const prefix = variant === 'beta' ? 'Butter-Paper-Beta' : 'Butter-Paper';
    for (const arch of ['arm64', 'x64']) {
      const macStem = `${prefix}-macOS-${arch}`;
      names.push(
        `${macStem}.dmg`,
        `${macStem}.dmg.blockmap`,
        `${macStem}.dmg.sha256`,
        `${macStem}.zip`,
        `${macStem}.zip.blockmap`,
        `${macStem}.zip.sha256`,
        `notarization-${variant}-${arch}.json`,
        `update-${variant}-darwin-${arch}.yml`,
      );

      const windowsStem = `${prefix}-Windows-${arch}-Setup.exe`;
      names.push(windowsStem);

      const linuxStem = `${prefix}-Linux-${arch}`;
      names.push(
        `${linuxStem}.AppImage`,
        `${linuxStem}.deb`,
        `${linuxStem}.rpm`,
      );
    }
  }
  return names.sort();
}

export function validateReleaseAssetDirectory(channel, directory) {
  const expected = expectedReleaseAssetNames(channel);
  const actual = readdirSync(directory, { withFileTypes: true })
    .filter(entry => entry.isFile())
    .map(entry => entry.name)
    .sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    const missing = expected.filter(name => !actual.includes(name));
    const unexpected = actual.filter(name => !expected.includes(name));
    throw new Error(`Release asset contract mismatch. Missing: ${missing.join(', ') || 'none'}. Unexpected: ${unexpected.join(', ') || 'none'}.`);
  }
  return expected;
}

function readOption(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : null;
}

export function main() {
  const channel = readOption('--channel');
  const directory = resolve(readOption('--directory') ?? 'publish/assets');
  if (!channel || !existsSync(directory)) {
    throw new Error('Usage: node scripts/release-asset-contract.mjs --channel stable|beta --directory <assets>');
  }
  for (const name of validateReleaseAssetDirectory(channel, directory)) {
    console.log(name);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main();
}
