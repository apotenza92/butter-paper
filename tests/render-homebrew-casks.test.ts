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

  it('requires native ARM and Intel validation before the single publish job', () => {
    const workflow = readFileSync('.github/workflows/release.yml', 'utf8');
    expect(workflow).toContain("runs-on: ${{ matrix.arch == 'arm64' && 'macos-15' || 'macos-15-intel' }}");
    expect(workflow).toContain('arch: [arm64, x64]');
    expect(workflow).toContain('beta\\.[1-9]\\d*');
    expect(workflow).toContain('needs: [prepare, test-homebrew]');
    expect(workflow).toContain('diff --recursive --unified validated/arm64 validated/x64');
    expect(workflow).toContain('environment: homebrew-release');
  });

  it('exposes the Homebrew token only to the final authenticated push', () => {
    const workflow = readFileSync('.github/workflows/release.yml', 'utf8');
    const publish = workflow.split('  publish-homebrew:', 2)[1];
    const checkout = publish.split('      - name: Apply validated Homebrew casks', 1)[0];
    expect(checkout).toContain('persist-credentials: false');
    expect(checkout).not.toContain('HOMEBREW_TAP_TOKEN');
    expect(publish.match(/secrets\.HOMEBREW_TAP_TOKEN/g)).toHaveLength(1);
    expect(publish).toContain('GIT_ASKPASS="$ASKPASS"');
  });
});
