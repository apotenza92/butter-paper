import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  assertFreshSource,
  assertVersionParity,
  classifySourceFreshness,
  prepareGeneratedOutput,
  readPackageVersion,
} from '../scripts/prepare-desktop-dev.mjs';

describe('desktop development preflight', () => {
  it('accepts the remote commit and local commits based on it', () => {
    expect(classifySourceFreshness({ head: 'a', remoteHead: 'a', remoteIsAncestor: true })).toBe('current');
    expect(classifySourceFreshness({ head: 'b', remoteHead: 'a', remoteIsAncestor: true })).toBe('ahead');
  });

  it('rejects a checkout that is behind or diverged from the remote baseline', () => {
    expect(() => assertFreshSource({
      head: '1111111111111111',
      remoteHead: '2222222222222222',
      remoteIsAncestor: false,
      remoteLabel: 'origin/main',
    })).toThrow(/behind or diverged from origin\/main/);
  });

  it('requires repository and desktop versions to agree', () => {
    expect(assertVersionParity('0.0.18', '0.0.18')).toBe('0.0.18');
    expect(() => assertVersionParity('0.0.18', '0.0.17')).toThrow(/does not match/);
  });

  it('reads a package version and rejects a missing version', () => {
    const directory = mkdtempSync(join(tmpdir(), 'butter-paper-dev-preflight-'));
    const validPath = join(directory, 'valid.json');
    const invalidPath = join(directory, 'invalid.json');
    writeFileSync(validPath, JSON.stringify({ version: '0.0.18' }));
    writeFileSync(invalidPath, JSON.stringify({ name: 'missing-version' }));

    expect(readPackageVersion(validPath)).toBe('0.0.18');
    expect(() => readPackageVersion(invalidPath)).toThrow(/Missing package version/);
  });

  it('removes only disposable desktop Vite output', () => {
    const root = mkdtempSync(join(tmpdir(), 'butter-paper-dev-output-'));
    const viteDirectory = join(root, 'apps/desktop/.vite');
    const sourceDirectory = join(root, 'apps/desktop/src');
    mkdirSync(viteDirectory, { recursive: true });
    mkdirSync(sourceDirectory, { recursive: true });
    writeFileSync(join(viteDirectory, 'stale.js'), 'stale');
    writeFileSync(join(sourceDirectory, 'keep.ts'), 'keep');

    prepareGeneratedOutput(root);

    expect(() => readFileSync(join(viteDirectory, 'stale.js'))).toThrow();
    expect(readFileSync(join(sourceDirectory, 'keep.ts'), 'utf8')).toBe('keep');
  });

  it('wires freshness verification and dependency builds into the default dev command', () => {
    const rootPackage = JSON.parse(readFileSync('package.json', 'utf8'));
    expect(rootPackage.scripts['predev:desktop']).toContain('prepare-desktop-dev.mjs');
    expect(rootPackage.scripts['predev:desktop']).toContain('@butter-paper/core build');
    expect(rootPackage.scripts['predev:desktop']).toContain('@butter-paper/pdf build');
  });

  it('keeps checkout-specific development provenance outside packaged app inputs', () => {
    const preflight = readFileSync('scripts/prepare-desktop-dev.mjs', 'utf8');
    const builderConfig = readFileSync('apps/desktop/electron-builder.config.cjs', 'utf8');
    const electronE2eSetup = readFileSync('tests/e2e/global-setup.mjs', 'utf8');
    const electronE2eHelper = readFileSync('tests/e2e/helpers/electron.mjs', 'utf8');
    expect(preflight).toContain("'test-results/desktop-dev-provenance.json'");
    expect(builderConfig).not.toContain('desktop-dev-provenance.json');
    expect(electronE2eSetup).toContain("'predev:desktop'");
    expect(electronE2eSetup).toContain('test-results/desktop-dev-provenance.json');
    expect(electronE2eHelper).toContain("app.once('close'");
  });
});
