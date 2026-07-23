import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  expectedReleaseAssetNames,
  validateReleaseAssetDirectory,
} from '../scripts/release-asset-contract.mjs';

describe('release asset contract', () => {
  it('defines an exact stable superset and beta-only contract', () => {
    const stable = expectedReleaseAssetNames('stable');
    const beta = expectedReleaseAssetNames('beta');
    expect(stable).toHaveLength(50);
    expect(beta).toHaveLength(26);
    expect(stable).toEqual(expect.arrayContaining(beta));
    expect(beta).toContain('Butter-Paper-Beta-macOS-arm64.zip');
    expect(beta).toContain('Butter-Paper-Beta-Windows-arm64-Setup.exe');
    expect(beta).toContain('Butter-Paper-Beta-Linux-arm64.AppImage');
    expect(beta).not.toContain('update-beta-win32-arm64.yml');
    expect(beta).not.toContain('update-beta-linux-arm64.yml');
    expect(beta).not.toContain('Butter-Paper-macOS-arm64.zip');
    expect(() => expectedReleaseAssetNames('nightly')).toThrow(/stable or beta/);
  });

  it('rejects missing or unexpected assets', () => {
    const directory = mkdtempSync(join(tmpdir(), 'butter-paper-assets-'));
    try {
      for (const name of expectedReleaseAssetNames('beta')) {
        writeFileSync(join(directory, name), name);
      }
      expect(validateReleaseAssetDirectory('beta', directory)).toEqual(expectedReleaseAssetNames('beta'));
      rmSync(join(directory, 'SHA256SUMS'));
      writeFileSync(join(directory, 'unexpected.txt'), 'unexpected');
      expect(() => validateReleaseAssetDirectory('beta', directory)).toThrow(/Missing: SHA256SUMS.*Unexpected: unexpected.txt/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it('refuses to rewrite an existing draft asset', () => {
    const workflow = readFileSync('.github/workflows/release.yml', 'utf8');
    const publishJob = workflow.slice(
      workflow.indexOf('  publish:'),
      workflow.indexOf('\n  verify-publication:'),
    );
    expect(publishJob).toContain('release-asset-contract.mjs');
    expect(publishJob).toContain('cmp "publish/assets/$asset_name" "existing-release/$asset_name"');
    expect(publishJob).not.toContain('--clobber');
    expect(publishJob).not.toContain('--method DELETE');
  });
});
