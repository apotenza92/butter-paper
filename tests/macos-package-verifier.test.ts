import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  createAppManifest,
  createSmokeEnvironment,
  normalizeFingerprint,
  parseCodesignMetadata,
  resolveReleaseContract,
  resolveConfiguredReleaseContract,
  validateBlockmap,
  validateChecksumText,
  validateEntitlements,
  validateExactArchitecture,
  validateSignatureMetadata,
  validateZipEntries,
} from '../scripts/verify-macos-package.mjs';

const fingerprint = 'C20E3A100252224861FF8474DEBB21E5A120210E7CD61905EFDA0B6464E18594';
const identity = 'Developer ID Application: Alexander Potenza (27JL2VERNC)';

describe('macOS release contract', () => {
  it('builds before credential materialization and launches only after credential cleanup', () => {
    const workflow = readFileSync('.github/workflows/release.yml', 'utf8');
    const packageJob = workflow.slice(
      workflow.indexOf('  package-macos:'),
      workflow.indexOf('\n  package-windows:'),
    );
    expect(packageJob.indexOf('Build desktop application without release credentials')).toBeLessThan(
      packageJob.indexOf('Build, sign, notarize, staple, and verify macOS package'),
    );
    expect(packageJob).toContain('--skip-build');
    expect(packageJob.indexOf('Verify and launch signed package without release credentials')).toBeGreaterThan(
      packageJob.indexOf('Build, sign, notarize, staple, and verify macOS package'),
    );

    const buildScript = readFileSync('scripts/build-signed-macos.mjs', 'utf8');
    expect(buildScript).toContain("'--skip-launch'");
    const stapleDmg = buildScript.indexOf("['stapler', 'staple', dmgPath]");
    const rebuildDmgBlockmap = buildScript.indexOf("buildBlockMap(dmgPath, 'gzip'");
    const refreshDmgMetadata = buildScript.indexOf("refreshUpdateMetadataArtifact(join(releaseDir, 'latest-mac.yml'), dmgPath)");
    const checksumDmg = buildScript.indexOf('writeChecksum(dmgPath)');
    expect(stapleDmg).toBeGreaterThan(-1);
    expect(rebuildDmgBlockmap).toBeGreaterThan(stapleDmg);
    expect(refreshDmgMetadata).toBeGreaterThan(rebuildDmgBlockmap);
    expect(checksumDmg).toBeGreaterThan(refreshDmgMetadata);

    const verifier = readFileSync('scripts/verify-macos-package.mjs', 'utf8');
    expect(verifier).toContain("readPlistValue(infoPlistPath, 'CFBundleIconName')");
    expect(verifier).toContain('packaged Icon Composer catalog is missing');
    expect(verifier).toContain('Icon Composer catalog is missing its dark appearance');
    expect(verifier).toContain('appearance is missing ${expectedBackground}');
    expect(verifier).toContain('Icon_Assets/system-dark');
    expect(verifier).toContain('Icon_Assets/01-artwork-dark');
    expect(verifier).toContain('does not fill the 1024x1024 icon canvas');
  });

  it('allowlists smoke variables without passing release credentials to the app', () => {
    const environment = createSmokeEnvironment({
      HOME: '/tmp/home',
      PATH: '/usr/bin',
      APPLE_API_KEY: '/tmp/AuthKey.p8',
      APPLE_SIGNING_CERTIFICATE_P12_BASE64: 'secret',
      GH_TOKEN: 'token',
    }, { BP_TEST_MODE: '1' });
    expect(environment).toEqual({
      HOME: '/tmp/home',
      PATH: '/usr/bin',
      BP_TEST_MODE: '1',
    });
  });

  it('keeps stable and beta identities and artifact names separate', () => {
    expect(resolveReleaseContract('stable', 'arm64')).toMatchObject({
      appName: 'Butter Paper.app',
      artifactPrefix: 'Butter-Paper-macOS',
      bundleId: 'com.butterpaper.desktop',
      executableName: 'Butter Paper',
      packageName: 'butter-paper',
    });
    expect(resolveReleaseContract('beta', 'x64')).toMatchObject({
      appName: 'Butter Paper Beta.app',
      artifactPrefix: 'Butter-Paper-Beta-macOS',
      bundleId: 'com.butterpaper.desktop.beta',
      executableName: 'Butter Paper Beta',
      packageName: 'butter-paper-beta',
    });
  });

  it('derives the active contract from Electron Builder configuration', () => {
    const stableContract = resolveConfiguredReleaseContract('stable', 'arm64', '/tmp/butter-paper-stable-test');
    const betaContract = resolveConfiguredReleaseContract('beta', 'x64', '/tmp/butter-paper-beta-test');
    expect(stableContract).toMatchObject({
      appName: 'Butter Paper.app',
      iconName: 'Icon',
    });
    expect(stableContract.iconSourcePath).toMatch(/apps\/desktop\/assets\/macos\/Butter Paper\.icon$/);
    expect(stableContract.legacyIconSourcePath).toMatch(/apps\/desktop\/assets\/icon\.icns$/);
    expect(betaContract).toMatchObject({
      appName: 'Butter Paper Beta.app',
      artifactPrefix: 'Butter-Paper-Beta-macOS',
      bundleId: 'com.butterpaper.desktop.beta',
      executableName: 'Butter Paper Beta',
      iconName: 'Icon',
      packageName: 'butter-paper-beta',
      updateFeedUrl: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/beta/darwin/x64',
    });
    expect(betaContract.iconSourcePath).toMatch(/apps\/desktop\/assets\/beta\/macos\/Butter Paper Beta\.icon$/);
    expect(betaContract.legacyIconSourcePath).toMatch(/apps\/desktop\/assets\/beta\/icon\.icns$/);
  });

  it('maintains complete light and dark Icon Composer sources for both channels', () => {
    for (const channel of ['stable', 'beta'] as const) {
      const contract = resolveConfiguredReleaseContract(channel, 'arm64', `/tmp/butter-paper-${channel}-icon-test`);
      const definition = JSON.parse(readFileSync(join(contract.iconSourcePath, 'icon.json'), 'utf8'));
      expect(definition['fill-specializations']).toEqual([
        { value: 'system-light' },
        { appearance: 'dark', value: 'system-dark' },
      ]);
      const layers = definition.groups.flatMap((group: { layers: Array<{
        'fill-specializations': Array<{ appearance?: string }>;
        'image-name-specializations': Array<{ appearance?: string; value: string }>;
      }> }) => group.layers);
      expect(layers).toHaveLength(1);
      for (const layer of layers) {
        expect(layer['fill-specializations']).toEqual([
          { value: 'none' },
          { appearance: 'dark', value: 'none' },
        ]);
        expect(layer['image-name-specializations']).toEqual([
          { value: '01-artwork.svg' },
          { appearance: 'dark', value: '01-artwork-dark.svg' },
        ]);
        for (const image of layer['image-name-specializations']) {
          const layerPath = join(contract.iconSourcePath, 'Assets', image.value);
          expect(existsSync(layerPath)).toBe(true);
          const layerSource = readFileSync(layerPath, 'utf8');
          expect(layerSource).toContain('viewBox="0 0 1024 1024"');
          expect(layerSource).toContain('linearGradient id="roll-edge"');
          expect(layerSource).toContain('fill="url(#roll-edge)"');
          expect(layerSource).toContain('d="M839 0V1024"');
          expect(layerSource).toContain('d="M1019 0V1024"');
          expect(layerSource).toContain('id="fold-shadow"');
          expect(layerSource).toContain(
            'id="fold" d="M0 0H270C237 49 244 226 244 226C244 226 39 246 0 270V0Z"',
          );
          const rollGradient = layerSource.match(
            /<linearGradient id="roll"[\s\S]*?<\/linearGradient>/,
          )?.[0];
          expect(rollGradient).toBeDefined();
          const edgeColor = image.appearance === 'dark'
            ? (channel === 'stable' ? '#050505' : '#010507')
            : (channel === 'stable' ? '#d69f3e' : '#043f6f');
          expect(rollGradient).toContain(`<stop offset="0" stop-color="${edgeColor}"/>`);
          expect(rollGradient).toContain(`<stop offset="1" stop-color="${edgeColor}"/>`);
          if (channel === 'stable') {
            expect(layerSource).toContain('stroke="#e2433d"');
            expect(layerSource).toContain(
              image.appearance === 'dark' ? 'fill="#090909"' : 'fill="#e3ab53"',
            );
          } else {
            expect(layerSource).toContain('stroke="#e5f7ff"');
            expect(layerSource).toContain(
              image.appearance === 'dark' ? 'fill="#02070b"' : 'fill="#043f6f"',
            );
          }
        }
      }
    }
  });

  it('rejects unknown channels and architectures', () => {
    expect(() => resolveReleaseContract('nightly', 'arm64')).toThrow(/release channel/);
    expect(() => resolveReleaseContract('stable', 'universal')).toThrow(/release architecture/);
  });
});

describe('certificate and signature validation', () => {
  it('normalizes canonical SHA-256 fingerprints', () => {
    const colonSeparated = fingerprint.match(/.{2}/g)?.join(':') ?? '';
    expect(normalizeFingerprint(colonSeparated.toLowerCase())).toBe(fingerprint);
    expect(() => normalizeFingerprint('abcd')).toThrow(/SHA-256/);
  });

  it('parses and validates exact Developer ID metadata', () => {
    const metadata = parseCodesignMetadata([
      'Identifier=com.butterpaper.desktop',
      'CodeDirectory v=20500 size=451 flags=0x10000(runtime) hashes=3+7 location=embedded',
      `Authority=${identity}`,
      'Authority=Developer ID Certification Authority',
      'Authority=Apple Root CA',
      'Timestamp=22 Jul 2026 at 6:31:50 pm',
      'TeamIdentifier=27JL2VERNC',
      'CDHash=fd3b6521696731432d4c40f1e41ab8b41430f354',
    ].join('\n'));
    expect(() => validateSignatureMetadata(metadata, {
      identity,
      teamId: '27JL2VERNC',
    }, 'fixture')).not.toThrow();
    expect(metadata.flags).toContain('runtime');
  });

  it('rejects the wrong signer, team, runtime, timestamp, or CDHash', () => {
    const base = {
      authorities: [identity],
      cdHash: 'abc',
      flags: 'CodeDirectory flags=runtime',
      identifier: 'com.butterpaper.desktop',
      teamIdentifier: '27JL2VERNC',
      ticket: 'stapled',
      timestamp: 'now',
    };
    const expected = { identity, teamId: '27JL2VERNC' };
    expect(() => validateSignatureMetadata({ ...base, authorities: ['wrong'] }, expected, 'fixture')).toThrow(/signed by/);
    expect(() => validateSignatureMetadata({ ...base, teamIdentifier: 'wrong' }, expected, 'fixture')).toThrow(/team/);
    expect(() => validateSignatureMetadata({ ...base, flags: '' }, expected, 'fixture')).toThrow(/hardened-runtime/);
    expect(() => validateSignatureMetadata({ ...base, timestamp: null }, expected, 'fixture')).toThrow(/timestamp/);
    expect(() => validateSignatureMetadata({ ...base, cdHash: null }, expected, 'fixture')).toThrow(/CDHash/);
  });

  it('requires one exact thin architecture', () => {
    expect(validateExactArchitecture('arm64\n', 'arm64', 'fixture')).toBe('arm64');
    expect(validateExactArchitecture('x86_64\n', 'x64', 'fixture')).toBe('x86_64');
    expect(() => validateExactArchitecture('arm64 x86_64', 'arm64', 'fixture')).toThrow(/exactly/);
    expect(() => validateExactArchitecture('arm64', 'x64', 'fixture')).toThrow(/exactly/);
  });

  it('allows only the maintained Electron release entitlements', () => {
    expect(() => validateEntitlements({
      'com.apple.security.cs.allow-jit': true,
      'com.apple.security.cs.allow-unsigned-executable-memory': true,
    }, 'fixture')).not.toThrow();
    expect(() => validateEntitlements({
      'com.apple.security.get-task-allow': true,
    }, 'fixture')).toThrow(/get-task-allow/);
    expect(() => validateEntitlements({
      'com.apple.security.files.user-selected.read-write': true,
    }, 'fixture')).toThrow(/unexpected entitlement/);
  });
});

describe('archive validation', () => {
  it('accepts only the expected stable or beta app root', () => {
    expect(() => validateZipEntries([
      'Butter Paper.app/',
      'Butter Paper.app/Contents/',
      'Butter Paper.app/Contents/MacOS/Butter Paper',
    ].join('\n'), resolveReleaseContract('stable', 'arm64'))).not.toThrow();
    expect(() => validateZipEntries([
      'Butter Paper Beta.app/',
      'Butter Paper Beta.app/Contents/MacOS/Butter Paper Beta',
    ].join('\n'), resolveReleaseContract('beta', 'x64'))).not.toThrow();
  });

  it('rejects traversal, absolute, duplicate, and unexpected ZIP entries', () => {
    const stable = resolveReleaseContract('stable', 'arm64');
    expect(() => validateZipEntries('../outside', stable)).toThrow(/unsafe traversal/);
    expect(() => validateZipEntries('/absolute', stable)).toThrow(/unsafe entry/);
    expect(() => validateZipEntries('other.txt', stable)).toThrow(/unexpected top-level/);
    expect(() => validateZipEntries([
      'Butter Paper.app/Contents/MacOS/Butter Paper',
      'Butter Paper.app/Contents/MacOS/Butter Paper',
    ].join('\n'), stable)).toThrow(/duplicate/);
  });

  it('validates blockmap coverage and checksum files', () => {
    const blockmap = {
      version: '2',
      files: [{ name: 'file', sizes: [4, 6], checksums: ['one', 'two'] }],
    };
    expect(() => validateBlockmap(blockmap, 10, 'fixture')).not.toThrow();
    expect(() => validateBlockmap(blockmap, 11, 'fixture')).toThrow(/represents 10 bytes/);
    expect(() => validateBlockmap({ ...blockmap, version: '1' }, 10, 'fixture')).toThrow(/blockmap v2/);
    expect(() => validateChecksumText(`${fingerprint.toLowerCase()}  artifact.zip\n`, 'artifact.zip', fingerprint)).not.toThrow();
    expect(() => validateChecksumText(`${fingerprint}  wrong.zip`, 'artifact.zip', fingerprint)).toThrow(/invalid content/);
  });

  it('creates deterministic byte and symlink manifests and rejects escaping links', () => {
    const root = mkdtempSync(join(tmpdir(), 'butter-paper-manifest-test-'));
    try {
      const contents = join(root, 'Contents');
      mkdirSync(contents);
      writeFileSync(join(contents, 'Info.plist'), 'fixture');
      symlinkSync('Info.plist', join(contents, 'Current'));
      expect(createAppManifest(root)).toEqual([
        { path: 'Contents', type: 'directory' },
        { path: 'Contents/Current', type: 'symlink', target: 'Info.plist' },
        expect.objectContaining({ path: 'Contents/Info.plist', type: 'file', size: 7 }),
      ]);
      symlinkSync('../../outside', join(contents, 'Escape'));
      expect(() => createAppManifest(root)).toThrow(/escaping symlink/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
