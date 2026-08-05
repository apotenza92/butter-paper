import { readFileSync } from 'node:fs';

const stableLightPath = 'assets/butter-paper-icon.svg';
const stableDarkPath = 'apps/desktop/assets/macos/Butter Paper.icon/Assets/01-artwork-dark.svg';
const betaLightPath = 'assets/butter-paper-icon-beta.svg';
const betaDarkPath = 'apps/desktop/assets/beta/macos/Butter Paper Beta.icon/Assets/01-artwork-dark.svg';

const nativeStableLightPath =
  'apps/desktop/assets/macos/Butter Paper.icon/Assets/01-artwork.svg';
const nativeBetaLightPath =
  'apps/desktop/assets/beta/macos/Butter Paper Beta.icon/Assets/01-artwork.svg';

const artworkPaths = [stableLightPath, stableDarkPath, betaLightPath, betaDarkPath];
const artwork = Object.fromEntries(
  artworkPaths.map(filePath => [filePath, readFileSync(filePath, 'utf8')]),
);

describe('Butter Paper icon artwork', () => {
  it.each(artworkPaths)('%s keeps the approved full-canvas icon contract', (filePath) => {
    const source = artwork[filePath];

    expect(source).toContain('width="1254" height="1254" viewBox="0 0 1254 1254"');
    expect(source).toContain('<rect width="1254" height="1254" rx="150"/>');
    expect(source).toContain('<g id="clean-stable-marker"');
    expect(source).toContain('<path d="M 627 627 C ');
    expect(source).not.toMatch(/<filter\b|filter=|<image\b|pencil|sketch/i);
  });

  it('uses identical geometry for stable, beta, light, and dark artwork', () => {
    const reference = geometry(artwork[stableLightPath]);

    for (const filePath of artworkPaths.slice(1)) {
      expect(geometry(artwork[filePath])).toEqual(reference);
    }
  });

  it('keeps generated light artwork identical to the macOS adaptive sources', () => {
    expect(readFileSync(nativeStableLightPath, 'utf8')).toBe(artwork[stableLightPath]);
    expect(readFileSync(nativeBetaLightPath, 'utf8')).toBe(artwork[betaLightPath]);
  });

  it.each([
    [stableLightPath, 'paper', ['#faeeb4', '#f3d875', '#dca84a', '#d31d17', '#d92735', '#e21c2d']],
    [stableDarkPath, 'darkButter', ['#6f5018', '#432d0d', '#251705', '#f3d875', '#d31d17', '#d92735', '#e21c2d']],
    [betaLightPath, 'betaPaper', ['#168bcb', '#0a5e9d', '#043f6f', '#e5f7ff']],
    [betaDarkPath, 'betaDarkPaper', ['#0d2b3d', '#091c28', '#050d13', '#e5f7ff']],
  ])('%s preserves its reviewed channel palette and background knockouts', (filePath, gradientId, colours) => {
    const source = artwork[filePath];

    for (const colour of colours) expect(source).toContain(colour);
    expect(source.match(new RegExp(`fill="url\\(#${gradientId}\\)"`, 'g'))).toHaveLength(9);
  });
});

function geometry(source: string) {
  return {
    paths: [...source.matchAll(/<path\b[^>]*\bd="([^"]+)"[^>]*>/g)].map(match => match[1]),
    rects: [...source.matchAll(/<rect\b([^>]*)\/?\s*>/g)].map(match =>
      [...match[1].matchAll(/\b(x|y|width|height|rx|ry)="([^"]+)"/g)]
        .map(attribute => `${attribute[1]}=${attribute[2]}`),
    ),
    transforms: [...source.matchAll(/<g\b[^>]*\btransform="([^"]+)"[^>]*>/g)]
      .map(match => match[1]),
  };
}
