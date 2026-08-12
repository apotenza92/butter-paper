import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

const brandPalette = JSON.parse(readFileSync('assets/brand/palette.json', 'utf8')) as {
  brand: Record<string, string>;
  channels: Record<string, Record<string, string>>;
};

const origamiSources = {
  stable: 'assets/icon-source/butter-paper-origami.svg',
  beta: 'assets/icon-source/butter-paper-origami-beta.svg',
} as const;

const variants = {
  stable: {
    light: 'assets/butter-paper-icon.png',
    dark: 'assets/butter-paper-icon-dark.png',
    nativeLight: 'apps/desktop/assets/macos/Butter Paper.icon/Assets/01-artwork.png',
    nativeDark: 'apps/desktop/assets/macos/Butter Paper.icon/Assets/01-artwork-dark.png',
  },
  beta: {
    light: 'assets/butter-paper-icon-beta.png',
    dark: 'assets/butter-paper-icon-beta-dark.png',
    nativeLight: 'apps/desktop/assets/beta/macos/Butter Paper Beta.icon/Assets/01-artwork.png',
    nativeDark: 'apps/desktop/assets/beta/macos/Butter Paper Beta.icon/Assets/01-artwork-dark.png',
  },
} as const;

const renderSource = readFileSync('assets/icon-source/render_icon_variants.py', 'utf8');
const sceneSource = readFileSync('assets/icon-source/build_glass_document_icon.py', 'utf8');
const brandIconRenderer = readFileSync('scripts/render-brand-icons.mjs', 'utf8');

describe('Butter Paper icon artwork', () => {
  it('derives stable and beta identity from the canonical brand palette', () => {
    const stable = readFileSync(origamiSources.stable, 'utf8').toUpperCase();
    const beta = readFileSync(origamiSources.beta, 'utf8').toUpperCase();

    expect(brandPalette.brand).toMatchObject({
      markupCoral: '#E7654C',
      studioViolet: '#6756B3',
      traceTeal: '#08756C',
    });
    expect(stable).toContain(brandPalette.channels.stable.primary);
    expect(stable).toContain(brandPalette.channels.stable.fold);
    expect(beta).toContain(brandPalette.channels.beta.primary);
    expect(beta).toContain(brandPalette.channels.beta.fold);
  });

  it('optically sizes the canonical SVG artwork for native icon canvases', () => {
    expect(brandIconRenderer).toContain('const visibleWidthRatio = 0.80;');
    expect(brandIconRenderer).toContain("source: 'butter-paper-origami.svg'");
    expect(brandIconRenderer).toContain("source: 'butter-paper-origami-beta.svg'");
    expect(brandIconRenderer).toContain('context.drawImage(image, renderedLeft, renderedTop, renderedWidth, renderedHeight);');
  });

  it.each(Object.entries(origamiSources))(
    'keeps the %s origami source gap-free and geometrically shared',
    (_channel, filePath) => {
      const source = readFileSync(filePath, 'utf8');
      expect(source).toContain('viewBox="0 0 256 200"');
      expect(source).toContain('clip-path="url(#silhouette)"');
      expect(source).toContain('M10 10 L71 24 L71 96');
      expect(source).toContain('M246 10 L185 24 L185 96');
      expect(source).toContain('M71 96 L121 76 L128 127');
      expect(source).toContain('M185 96 L135 76 L128 127');
      expect(source).not.toContain('stroke=');
      expect(source).not.toContain('<circle');
      expect(source).not.toContain('<ellipse');
      expect(source.match(/<path[^>]+fill="url\(#/g)?.length).toBe(11);
      expect(source).not.toMatch(/<path[^>]+fill="#[0-9a-f]{6}"/i);
    },
  );

  it('keeps stable and beta origami geometry byte-identical apart from copy and palette', () => {
    const normalize = (filePath: string) => readFileSync(filePath, 'utf8')
      .replace('Butter Paper Beta', 'Butter Paper')
      .replace('violet', 'teal')
      .replace('teal central fold', 'coral central fold')
      .replace(/#[0-9a-f]{6}/gi, '#palette');
    expect(normalize(origamiSources.beta)).toBe(normalize(origamiSources.stable));
  });

  it.each(Object.values(variants).flatMap(variant => [variant.light, variant.dark]))(
    '%s is a full-canvas 1024 px RGBA PNG',
    (filePath) => {
      const source = readFileSync(filePath);
      expect(source.subarray(0, 8).toString('hex')).toBe('89504e470d0a1a0a');
      expect(source.readUInt32BE(16)).toBe(1024);
      expect(source.readUInt32BE(20)).toBe(1024);
      expect(source.readUInt8(24)).toBe(8);
      expect(source.readUInt8(25)).toBe(6);
    },
  );

  it('keeps each macOS adaptive source byte-identical to its canonical artwork', () => {
    for (const variant of Object.values(variants)) {
      expect(sha256(variant.nativeLight)).toBe(sha256(variant.light));
      expect(sha256(variant.nativeDark)).toBe(sha256(variant.dark));
    }
  });

  it('keeps stable and beta geometry shared while changing only the annotation palette', () => {
    expect(renderSource).toContain('(\"stable\", \"light\")');
    expect(renderSource).toContain('(\"beta\", \"light\")');
    expect(renderSource).toContain('if channel == "stable"');
    expect(renderSource).toContain('surface = (0.96, 0.004, 0.012)');
    expect(renderSource).toContain('surface = (0.30, 0.025, 0.92)');
    expect(sha256(variants.stable.light)).not.toBe(sha256(variants.beta.light));
    expect(sha256(variants.stable.dark)).not.toBe(sha256(variants.beta.dark));
  });

  it('preserves the reviewed glass-document composition and icon-scale topology', () => {
    expect(sceneSource).toContain('Raised transparent yellow trace layer');
    expect(sceneSource).toContain('Vibrant ruby annotation glass');
    expect(sceneSource).toContain('horizontal_lobes=10');
    expect(sceneSource).toContain('vertical_lobes=8');
    expect(sceneSource).toContain('Filled oxblood review arrow');
    expect(sceneSource).toContain('Transparent ruby markup text box outline');
    expect(sceneSource).toContain('Expanded monochrome document content');
    expect(sceneSource).not.toContain('Loupe');
  });
});

function sha256(filePath: string) {
  return createHash('sha256').update(readFileSync(filePath)).digest('hex');
}
