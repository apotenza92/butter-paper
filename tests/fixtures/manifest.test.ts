import { describe, expect, it } from 'vitest';
import { fixtureExists, getFixturePath, loadFixtureManifest } from './index.js';

describe('fixture manifest', () => {
  it('exposes deterministic fixture entries', async () => {
    const manifest = await loadFixtureManifest();

    expect(manifest.fixtures.length).toBeGreaterThanOrEqual(4);
    expect(manifest.fixtures.some((entry) => entry.name === 'engineering-large')).toBe(true);
    expect(await fixtureExists('single-page')).toBe(true);
    expect(await getFixturePath('single-page')).toContain('single-page.pdf');
  });
});
