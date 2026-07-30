import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import YAML from 'yaml';

const require = createRequire(import.meta.url);
const {
  artifactName,
  prepareTarget,
} = require('../scripts/test-nonmac-update.cjs') as {
  artifactName(value: string): string;
  prepareTarget(options: {
    baseUrl: string;
    candidateDirectory: string;
    candidateMetadata: string;
    channel: 'stable' | 'beta';
  }): {
    bytes: Buffer;
    version: string;
  };
};

describe('native Windows and Linux updater audit', () => {
  it('rewrites only byte-verified updater artifacts to the loopback server', () => {
    const directory = mkdtempSync(join(tmpdir(), 'butter-paper-native-updater-'));
    try {
      const artifact = Buffer.from('candidate package');
      const artifactPath = join(directory, 'Butter-Paper-Windows-x64-Setup.exe');
      writeFileSync(artifactPath, artifact);
      const metadataPath = join(directory, 'latest.yml');
      const sha512 = createHash('sha512').update(artifact).digest('base64');
      writeFileSync(metadataPath, YAML.stringify({
        version: '0.0.12',
        files: [{ url: basename(artifactPath), sha512, size: artifact.length }],
        path: basename(artifactPath),
        sha512,
      }));

      const prepared = prepareTarget({
        baseUrl: 'http://127.0.0.1:43127',
        candidateDirectory: directory,
        candidateMetadata: metadataPath,
        channel: 'stable',
      });
      const rewritten = YAML.parse(prepared.bytes.toString('utf8'));
      expect(rewritten.files[0].url).toBe(
        'http://127.0.0.1:43127/assets/Butter-Paper-Windows-x64-Setup.exe',
      );
      expect(rewritten.path).toBe(rewritten.files[0].url);
      expect(rewritten.butterPaperChannel).toBe('stable');
      expect(prepared.version).toBe('0.0.12');

      writeFileSync(artifactPath, 'tampered');
      expect(() => prepareTarget({
        baseUrl: 'http://127.0.0.1:43127',
        candidateDirectory: directory,
        candidateMetadata: metadataPath,
        channel: 'stable',
      })).toThrow(/SHA-512 does not match/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it('rejects escaping or malformed artifact names', () => {
    expect(artifactName('https://example.invalid/Butter-Paper.AppImage')).toBe(
      'Butter-Paper.AppImage',
    );
    expect(() => artifactName('')).toThrow(/invalid artifact URL/);
    expect(() => artifactName('%2F')).toThrow(/Unsafe/);
  });

  it('requires real native ARM64 and x64 replacement on Windows and Linux', () => {
    const workflow = readFileSync(resolve('.github/workflows/nonmac-updater-audit.yml'), 'utf8');
    const release = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');
    const script = readFileSync(resolve('scripts/test-nonmac-update.cjs'), 'utf8');

    expect(workflow).toContain('windows-11-arm');
    expect(workflow).toContain('windows-2025');
    expect(workflow).toContain('ubuntu-24.04-arm');
    expect(workflow).toContain('Require the matching native runner');
    expect(workflow).toContain('BP_REQUIRE_TUF_ROOT: \'1\'');
    expect(workflow).toContain('create-test-tuf-repository.cjs');
    expect(workflow).toContain('test-nonmac-update.cjs');
    expect(workflow).not.toContain('BP_NSIS_ASSISTED_MIGRATION_FIXTURE');
    expect(workflow).toContain('--linux AppImage');
    expect(workflow).not.toContain('secrets.');
    expect(script).toContain("['wrong-signature', 'corrupt-payload', 'valid']");
    expect(script).toContain('updated-runtime-launched');
    expect(script).toContain('update-downloaded');
    expect(script).toContain('Updater changed existing user data');
    expect(script).toContain("path.join(userData, 'update-trust', 'metadata', 'root.json')");
    expect(script).toContain('AppImage updater did not replace the installed bytes');
    expect(script).toContain('waitForWindowsReplacement');
    expect(script).toContain('archivesUnderLocalPrograms');
    expect(script).toContain('uninstallEntries');
    expect(script).toContain('Updated Windows runtime did not relaunch at the candidate version');
    expect(script.indexOf("new Set(['updated-runtime-launched', 'error'])")).toBeLessThan(
      script.lastIndexOf('await waitForWindowsReplacement('),
    );
    expect(script).toContain('PACKAGE_SHA256SUMS');
    expect(script).not.toContain('copyFileSync(privateKeyPath');
    expect(release).toContain('uses: ./.github/workflows/nonmac-updater-audit.yml');
    expect(release).toContain(
      'needs: [prepare, package-macos, package-windows, package-linux, test-macos-updater, test-nonmac-updater]',
    );
  });
});
