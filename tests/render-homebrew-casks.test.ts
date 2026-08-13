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
    expect(casks['butter-paper.rb']).toContain('depends_on macos: :monterey');
    expect(casks['butter-paper.rb']).toContain(
      'releases/download/v#{version}/Butter-Paper-macOS-arm64.zip',
    );
    expect(casks['butter-paper.rb']).toContain(`${'a'.repeat(64)}"\n\n    url`);
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
    expect(workflow).toContain('needs: [prepare, publish]');
    expect(workflow).toContain('needs: [prepare, verify-publication, test-homebrew]');
    expect(workflow).toContain('homebrew-publication.tar.gz');
    expect(workflow).toContain('build-homebrew-publication.mjs');
    expect(workflow).toContain('include-hidden-files: true');
    expect(workflow).toContain('brew tap "$TEST_TAP" "$TEST_TAP_REPO" --custom-remote');
    expect(workflow).toContain('HOMEBREW_GITHUB_API_TOKEN: ${{ github.token }}');
    expect(workflow).toContain('$TEST_TAP/butter-paper@beta');
    expect(workflow).toContain('brew install --cask "$FULL_TOKEN"');
    expect(workflow).toContain('releases/$RELEASE_ID/assets?name=$encoded_name');
    expect(workflow).toContain('Draft release assets match the verified bundle by name, size, and SHA-256.');
  });

  it('dispatches without directly committing or pushing the tap', () => {
    const workflow = readFileSync('.github/workflows/release.yml', 'utf8');
    const publication = workflow.split('  dispatch-homebrew-publication:', 2)[1];
    expect(publication).toContain('actions/create-github-app-token');
    expect(publication).toContain('publish-homebrew-v1');
    expect(publication).not.toContain('gh run watch');
    expect(publication).not.toContain('git commit');
    expect(publication).not.toContain('git push');
    expect(workflow).not.toContain('HOMEBREW_TAP_TOKEN');
  });
});
