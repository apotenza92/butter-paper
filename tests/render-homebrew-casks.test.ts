import { renderCasks } from '../scripts/render-homebrew-casks.mjs';
import { readFileSync } from 'node:fs';

const file = (arch: string) => ({
  sha256: arch === 'arm64' ? 'a'.repeat(64) : 'b'.repeat(64),
  url: `https://github.com/apotenza92/butter-paper/releases/download/v0.0.1/Butter-Paper-macOS-${arch}.zip`,
});

describe('Homebrew cask renderer', () => {
  it('renders native ARM and Intel branches without credentials', () => {
    const casks = renderCasks({
      tag: 'v0.0.1',
      version: '0.0.1',
      channels: {
        stable: { app: 'Butter Paper.app', files: { arm64: file('arm64'), x64: file('x64') } },
        beta: { app: 'Butter Paper Beta.app', files: { arm64: file('arm64'), x64: file('x64') } },
      },
    });
    expect(Object.keys(casks).sort()).toEqual(['butter-paper.rb', 'butter-paper@beta.rb']);
    expect(casks['butter-paper.rb']).toContain('on_arm do');
    expect(casks['butter-paper.rb']).toContain('on_intel do');
    expect(casks['butter-paper@beta.rb']).toContain('Butter Paper Beta.app');
  });

  it('rejects beta zero so every prerelease has a positive sequence number', () => {
    expect(() => renderCasks({
      tag: 'v0.0.1-beta.0',
      version: '0.0.1-beta.0',
      channels: {
        beta: { app: 'Butter Paper Beta.app', files: { arm64: file('arm64'), x64: file('x64') } },
      },
    })).toThrow(/identity is invalid/);
  });

  it('requires native ARM and Intel validation before sealing the publication bundle', () => {
    const workflow = readFileSync('.github/workflows/release.yml', 'utf8');
    expect(workflow).toContain("runs-on: ${{ matrix.arch == 'arm64' && 'macos-15' || 'macos-15-intel' }}");
    expect(workflow).toContain('arch: [arm64, x64]');
    expect(workflow).toContain('beta\\.[1-9]\\d*');
    expect(workflow).toContain('needs: [prepare, test-homebrew]');
    expect(workflow).toContain('diff --recursive --unified validated/arm64 validated/x64');
    expect(workflow).toContain('butter-paper-homebrew-publication-');
  });

  it('never commits, pushes, or exposes a Homebrew token', () => {
    const workflow = readFileSync('.github/workflows/release.yml', 'utf8');
    const publication = workflow.split('  prepare-homebrew-publication:', 2)[1];
    expect(publication).toContain('actions/upload-artifact');
    expect(publication).toContain('SHA256SUMS');
    expect(publication).toContain('Apply these exact');
    expect(publication).not.toContain('git commit');
    expect(publication).not.toContain('git push');
    expect(workflow).not.toContain('HOMEBREW_TAP_TOKEN');
  });
});
