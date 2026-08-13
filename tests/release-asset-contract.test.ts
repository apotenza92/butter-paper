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
    expect(stable).toHaveLength(63);
    expect(beta).toHaveLength(33);
    expect(stable).toEqual(expect.arrayContaining(beta));
    expect(beta).toContain('Butter-Paper-Beta-macOS-arm64.zip');
    expect(beta).toContain('Butter-Paper-Beta-Windows-arm64-Setup.exe');
    expect(beta).toContain('Butter-Paper-Beta-Linux-arm64.AppImage');
    expect(beta).toContain('homebrew-publication.tar.gz');
    expect(beta).toContain('update-beta-win32-arm64.yml');
    expect(beta).toContain('update-beta-linux-arm64.yml');
    expect(beta).toContain('Butter-Paper-Beta-Windows-arm64-Setup.exe.blockmap');
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
    expect(publishJob).toContain('comm -13 existing-assets.txt expected-assets.txt > missing-assets.txt');
    expect(publishJob).toContain('test "$remote_size" = "$local_size"');
    expect(publishJob).toContain('test "$remote_digest" = "$local_digest"');
    expect(publishJob).toContain('releases/$RELEASE_ID/assets?name=$encoded_name');
    expect(publishJob).toContain('find_release_record()');
    expect(publishJob).toContain('for attempt in {1..10}; do');
    expect(publishJob).toContain('Waiting for GitHub to return the new draft release');
    expect(publishJob).not.toContain('--clobber');
    expect(publishJob).not.toContain('--method DELETE');
  });

  it('keeps release simulations disposable and non-publishing', () => {
    const workflow = readFileSync('.github/workflows/release-pipeline-simulation.yml', 'utf8');
    expect(workflow).toContain('workflow_dispatch:');
    expect(workflow).toContain('Prepare immutable disposable candidate');
    expect(workflow).toContain('Verify exact candidate for promotion');
    expect(workflow).toContain("test \"$CANDIDATE_SHA\" = \"$GITHUB_SHA\"");
    expect(workflow).toContain('subject-checksums: promotion-input/release-manifest/assets/SHA256SUMS');
    expect(workflow).toContain('name: simulation-release-feed-sealed');
    expect(workflow).toContain('Promote without publishing');
    expect(workflow).not.toContain('contents: write');
    expect(workflow).not.toContain('gh release create');
    expect(workflow).not.toContain('git push');
    expect(workflow).not.toContain('repository_dispatch');
  });
});
