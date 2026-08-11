import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

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

describe('Butter Paper icon artwork', () => {
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
