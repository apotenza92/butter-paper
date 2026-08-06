import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { ancestorFileCandidates } from './applicationPaths';

describe('application path candidates', () => {
  it('finds repository artifacts from a direct Vite main-process entry', () => {
    const repositoryRoot = resolve('virtual-butter-paper-workspace');
    const buildRoot = join(repositoryRoot, 'apps', 'desktop', '.vite', 'build');

    expect(ancestorFileCandidates(
      buildRoot,
      'test-results/desktop-dev-provenance.json',
    )).toContain(join(repositoryRoot, 'test-results', 'desktop-dev-provenance.json'));
    expect(ancestorFileCandidates(buildRoot, 'package.json')).toContain(
      join(repositoryRoot, 'apps', 'desktop', 'package.json'),
    );
  });

  it('keeps Forge desktop roots and repository roots supported', () => {
    const repositoryRoot = resolve('virtual-butter-paper-workspace');
    const desktopRoot = join(repositoryRoot, 'apps', 'desktop');

    expect(ancestorFileCandidates(
      desktopRoot,
      'test-results/desktop-dev-provenance.json',
    )).toContain(join(repositoryRoot, 'test-results', 'desktop-dev-provenance.json'));
    expect(ancestorFileCandidates(
      repositoryRoot,
      'test-results/desktop-dev-provenance.json',
    )[0]).toBe(join(repositoryRoot, 'test-results', 'desktop-dev-provenance.json'));
  });
});
