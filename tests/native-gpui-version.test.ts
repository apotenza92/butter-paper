import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

const repositoryRoot = join(import.meta.dirname, '..');
const nativeCrateRoot = join(
  repositoryRoot,
  'experiments',
  'gpui-migration',
  'gpui-migration',
);

function packageVersionFromCargoToml(contents: string): string {
  const packageSection = contents.match(/^\[package\]\s*\n([\s\S]*?)(?=^\[)/m)?.[1];
  const version = packageSection?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) throw new Error('native Cargo.toml has no package version');
  return version;
}

describe('native GPUI release version', () => {
  it('matches every native package, bundle and notice identity', () => {
    const rootVersion = JSON.parse(
      readFileSync(join(repositoryRoot, 'package.json'), 'utf8'),
    ).version;
    const cargoVersion = packageVersionFromCargoToml(
      readFileSync(join(nativeCrateRoot, 'Cargo.toml'), 'utf8'),
    );
    const cargoLock = readFileSync(join(nativeCrateRoot, 'Cargo.lock'), 'utf8');
    const developmentInfoPlist = readFileSync(
      join(nativeCrateRoot, 'bundle', 'Info.plist'),
      'utf8',
    );
    const thirdPartyNotices = readFileSync(
      join(nativeCrateRoot, 'THIRD_PARTY_NOTICES.md'),
      'utf8',
    );

    expect(cargoVersion).toBe(rootVersion);
    expect(cargoLock).toContain(
      `name = "butter-paper-gpui-migration"\nversion = "${rootVersion}"`,
    );
    expect(developmentInfoPlist).toContain(
      `<key>CFBundleShortVersionString</key>\n  <string>${rootVersion}</string>`,
    );
    expect(developmentInfoPlist).toContain(
      `<key>CFBundleVersion</key>\n  <string>${rootVersion}</string>`,
    );
    expect(thirdPartyNotices).toContain(
      `- Package: \`butter-paper-gpui-migration\` ${rootVersion}`,
    );
  });
});
