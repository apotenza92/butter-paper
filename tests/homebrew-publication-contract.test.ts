import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildHomebrewPublication } from '../scripts/build-homebrew-publication.mjs';

describe('standard Homebrew publication contract', () => {
  it('seals exact stable and beta identities with native run provenance', () => {
    const root = mkdtempSync(join(tmpdir(), 'butter-paper-homebrew-'));
    try {
      const assets = join(root, 'assets');
      const output = join(root, 'publication');
      mkdirSync(assets);
      const channels = Object.fromEntries(['stable', 'beta'].map(channel => {
        const prefix = channel === 'beta' ? 'Butter-Paper-Beta' : 'Butter-Paper';
        const files = Object.fromEntries(['arm64', 'x64'].map(arch => {
          const name = `${prefix}-macOS-${arch}.zip`;
          writeFileSync(join(assets, name), `${channel}-${arch}`);
          const digest = createHash('sha256').update(`${channel}-${arch}`).digest('hex');
          return [arch, {url: `https://github.com/apotenza92/butter-paper/releases/download/v1.2.3/${name}`, sha256: digest}];
        }));
        return [channel, {app: channel === 'beta' ? 'Butter Paper Beta.app' : 'Butter Paper.app', cask: channel === 'beta' ? 'butter-paper@beta' : 'butter-paper', files}];
      }));
      const manifest = buildHomebrewPublication({release: {tag: 'v1.2.3', version: '1.2.3', channels}, assetsDirectory: assets, outputDirectory: output, repository: 'apotenza92/butter-paper', commit: 'a'.repeat(40), runId: '42', runAttempt: '2'});
      expect(manifest.casks).toEqual(['butter-paper.rb', 'butter-paper@beta.rb']);
      expect(manifest.native_validation).toEqual({workflow_run_id: 42, workflow_run_attempt: 2, jobs: ['Test Homebrew casks (arm64)', 'Test Homebrew casks (x64)']});
      expect(manifest.artifacts).toHaveLength(4);
      expect(readFileSync(join(output, 'Casks', 'butter-paper.rb'), 'utf8')).toContain('version "1.2.3"');
    } finally {
      rmSync(root, {recursive: true, force: true});
    }
  });

  it('requires the dispatch credential only in the protected tag environment', () => {
    const workflow = readFileSync('.github/workflows/release.yml', 'utf8');
    const dispatch = workflow.split('  dispatch-homebrew-publication:', 2)[1];
    expect(dispatch).toContain('environment: homebrew-dispatch');
    expect(dispatch).toContain('HOMEBREW_DISPATCHER_PRIVATE_KEY');
    expect(dispatch).toContain('repos/$TAP_REPOSITORY/dispatches');
    expect(dispatch).not.toContain('gh run watch');
    expect(dispatch).not.toContain('permission-actions: read');
    expect(workflow).not.toContain('HOMEBREW_TAP_DEPLOY_KEY');
  });
});
