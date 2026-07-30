import { mkdtempSync, rmSync, truncateSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { validateWindowsInstallerSizes } from '../scripts/verify-release-package-sizes.mjs';

function installerPath(directory: string, variant: 'stable' | 'beta', arch: 'arm64' | 'x64') {
  const prefix = variant === 'beta' ? 'Butter-Paper-Beta' : 'Butter-Paper';
  return join(directory, `${prefix}-Windows-${arch}-Setup.exe`);
}

function createSizedFile(path: string, size: number) {
  writeFileSync(path, '');
  truncateSync(path, size);
}

describe('release package size guard', () => {
  it('accepts proportionate stable and beta Windows installers', () => {
    const directory = mkdtempSync(join(tmpdir(), 'butter-paper-sizes-'));
    try {
      for (const variant of ['stable', 'beta'] as const) {
        createSizedFile(installerPath(directory, variant, 'arm64'), 95 * 1024 * 1024);
        createSizedFile(installerPath(directory, variant, 'x64'), 105 * 1024 * 1024);
      }
      expect(validateWindowsInstallerSizes(directory, ['stable', 'beta'])).toHaveLength(2);
    } finally {
      rmSync(directory, { force: true, recursive: true });
    }
  });

  it('rejects the duplicated ARM64 runtime footprint', () => {
    const directory = mkdtempSync(join(tmpdir(), 'butter-paper-sizes-'));
    try {
      createSizedFile(installerPath(directory, 'stable', 'arm64'), 200 * 1024 * 1024);
      createSizedFile(installerPath(directory, 'stable', 'x64'), 105 * 1024 * 1024);
      expect(() => validateWindowsInstallerSizes(directory, ['stable']))
        .toThrow(/unexpectedly large/);
    } finally {
      rmSync(directory, { force: true, recursive: true });
    }
  });
});
