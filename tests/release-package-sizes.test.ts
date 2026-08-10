import { mkdtempSync, rmSync, truncateSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  validateWindowsArm64InstallerSize,
  validateWindowsInstallerSizes,
} from '../scripts/verify-release-package-sizes.mjs';

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

  it('accepts a single native ARM64 package-smoke candidate', () => {
    const directory = mkdtempSync(join(tmpdir(), 'butter-paper-sizes-'));
    try {
      const candidate = installerPath(directory, 'stable', 'arm64');
      createSizedFile(candidate, 100 * 1024 * 1024);
      expect(validateWindowsArm64InstallerSize(candidate).bytes).toBe(100 * 1024 * 1024);
    } finally {
      rmSync(directory, { force: true, recursive: true });
    }
  });

  it('rejects an excessively duplicated ARM64 runtime footprint', () => {
    const directory = mkdtempSync(join(tmpdir(), 'butter-paper-sizes-'));
    try {
      const candidate = installerPath(directory, 'stable', 'arm64');
      createSizedFile(candidate, 260 * 1024 * 1024);
      expect(() => validateWindowsArm64InstallerSize(candidate))
        .toThrow(/unexpectedly large/);
    } finally {
      rmSync(directory, { force: true, recursive: true });
    }
  });

  it('rejects an ARM64 installer disproportionate to x64', () => {
    const directory = mkdtempSync(join(tmpdir(), 'butter-paper-sizes-'));
    try {
      createSizedFile(installerPath(directory, 'stable', 'arm64'), 130 * 1024 * 1024);
      createSizedFile(installerPath(directory, 'stable', 'x64'), 90 * 1024 * 1024);
      expect(() => validateWindowsInstallerSizes(directory, ['stable']))
        .toThrow(/disproportionately large/);
    } finally {
      rmSync(directory, { force: true, recursive: true });
    }
  });
});
