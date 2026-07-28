import { resolveReleasePageUrl } from './releasePage';

describe('release page URL', () => {
  it('opens the matching release when an update is available', () => {
    expect(resolveReleasePageUrl('0.0.12-beta.1')).toBe(
      'https://github.com/apotenza92/butter-paper/releases/tag/v0.0.12-beta.1',
    );
  });

  it('opens the release list without an available update', () => {
    expect(resolveReleasePageUrl(null)).toBe(
      'https://github.com/apotenza92/butter-paper/releases',
    );
  });
});
