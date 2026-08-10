import { readFileSync } from 'node:fs';

const stableLightPath = 'assets/butter-paper-icon.svg';
const stableDarkPath = 'apps/desktop/assets/macos/Butter Paper.icon/Assets/01-artwork-dark.svg';
const betaLightPath = 'assets/butter-paper-icon-beta.svg';
const betaDarkPath = 'apps/desktop/assets/beta/macos/Butter Paper Beta.icon/Assets/01-artwork-dark.svg';

const nativeStableLightPath = 'apps/desktop/assets/macos/Butter Paper.icon/Assets/01-artwork.svg';
const nativeBetaLightPath = 'apps/desktop/assets/beta/macos/Butter Paper Beta.icon/Assets/01-artwork.svg';

const artworkPaths = [stableLightPath, stableDarkPath, betaLightPath, betaDarkPath];
const artwork = Object.fromEntries(
  artworkPaths.map(filePath => [filePath, readFileSync(filePath, 'utf8')]),
);

describe('Butter Paper icon artwork', () => {
  it.each(artworkPaths)('%s is a high-fidelity vector source', (filePath) => {
    const source = artwork[filePath];

    expect(source).toContain('width="1024" height="1024" viewBox="0 0 1024 1024"');
    expect(source).toContain('data-layout="corner-equal-padding"');
    expect(source).toContain('data-artwork-scale="0.87"');
    expect(source).toContain('data-cube-padding="58"');
    expect(source).toContain('data-feather-padding="46"');
    expect(source).toContain('data-feather-to-pot="58/42"');
    expect(source).toContain('data-source-artwork="approved-raster-direct-trace"');
    expect(source).toContain('data-efficiency-pass="none"');
    expect(source).toContain('<g id="quill-ink-mark"');
    expect(source).not.toMatch(/<rect\b|<clipPath\b|<filter\b|filter=|<image\b|pencil|sketch/i);
    expect(source.match(/<path\b/g)?.length).toBeGreaterThan(1000);
  });

  it('keeps dark artwork geometry identical to light artwork', () => {
    expect(geometry(artwork[stableDarkPath])).toEqual(geometry(artwork[stableLightPath]));
    expect(geometry(artwork[betaDarkPath])).toEqual(geometry(artwork[betaLightPath]));
  });

  it('keeps light artwork black-contour-only and dark artwork white-outer-contour-only', () => {
    for (const filePath of [stableLightPath, betaLightPath]) {
      const source = artwork[filePath];
      expect(source).not.toContain('stroke="#ffffff"');
      expect(source).not.toContain('fill="#ffffff"');
      expect(source).toContain('data-dark-outline="black"');
    }

    for (const filePath of [stableDarkPath, betaDarkPath]) {
      const source = artwork[filePath];
      expect(source).toContain('fill="#ffffff"');
      expect(source).toContain('data-dark-outline="white-pure-black-swap"');
      expect(source).not.toContain('dark-outer-outline');
    }
  });

  it('shares the cube treatment while separating stable and beta in the feather and ink', () => {
    const stable = artwork[stableLightPath];
    const beta = artwork[betaLightPath];

    expect(stable).toContain('#ee2224');
    expect(stable).toContain('#bc0302');
    expect(beta).toContain('#1b57f3');
    expect(beta).toContain('#001fc4');
    expect(stable).toContain('data-source-artwork="approved-raster-direct-trace"');
    expect(beta).toContain('data-source-artwork="approved-raster-direct-trace"');
  });

  it('keeps generated light artwork identical to the macOS adaptive sources', () => {
    expect(readFileSync(nativeStableLightPath, 'utf8')).toBe(artwork[stableLightPath]);
    expect(readFileSync(nativeBetaLightPath, 'utf8')).toBe(artwork[betaLightPath]);
  });
});

function geometry(source: string) {
  return {
    paths: [...source.matchAll(/<path\b[^>]*\bd="([^"]+)"[^>]*>/g)].map(match => match[1]),
    ellipses: [...source.matchAll(/<ellipse\b([^>]*)>/g)].map(match =>
      [...match[1].matchAll(/\b(cx|cy|rx|ry)="([^"]+)"/g)]
        .map(attribute => `${attribute[1]}=${attribute[2]}`),
    ),
  };
}
