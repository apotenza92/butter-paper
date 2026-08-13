import { describe, expect, it } from 'vitest';
import { COLOR_PRESETS_STORAGE_KEY, DEFAULT_COLOR_PRESETS, hexToHsl, hexToRgb, hslToHex, loadColorPresets, normalizeHexColor, rgbToHex, saveColorPresets } from './colorPresets';

function memoryStorage(initialValue: string | null = null) {
  let value = initialValue;
  return {
    getItem: (key: string) => (key === COLOR_PRESETS_STORAGE_KEY ? value : null),
    setItem: (key: string, nextValue: string) => {
      if (key === COLOR_PRESETS_STORAGE_KEY) value = nextValue;
    },
  };
}

describe('color presets', () => {
  it('provides the common palette and normalizes short hex colors', () => {
    expect(DEFAULT_COLOR_PRESETS).toHaveLength(12);
    expect(DEFAULT_COLOR_PRESETS).toEqual(['#000000', '#808080', '#ff0000', '#ffa500', '#ffff00', '#00ff00', '#008080', '#00ffff', '#0000ff', '#800080', '#ff00ff', '#ffffff']);
    expect(normalizeHexColor('#3AF')).toBe('#33aaff');
  });

  it('rejects invalid colors', () => {
    expect(normalizeHexColor('nope')).toBeNull();
  });

  it('converts RGB and HSL entry values to the saved hex format', () => {
    expect(hexToRgb('#336699')).toEqual({ r: 51, g: 102, b: 153 });
    expect(rgbToHex({ r: 51, g: 102, b: 153 })).toBe('#336699');
    expect(hexToHsl('#336699')).toEqual({ h: 210, s: 50, l: 40 });
    expect(hslToHex({ h: 210, s: 50, l: 40 })).toBe('#336699');
  });

  it('always preserves defaults while persisting added and deleted custom presets', () => {
    const storage = memoryStorage();
    expect(saveColorPresets(storage, ['#123456', '#3af', '#123456'])).toEqual([...DEFAULT_COLOR_PRESETS, '#123456', '#33aaff']);
    expect(loadColorPresets(storage)).toEqual([...DEFAULT_COLOR_PRESETS, '#123456', '#33aaff']);
    expect(saveColorPresets(storage, [...DEFAULT_COLOR_PRESETS, '#33aaff'])).toEqual([...DEFAULT_COLOR_PRESETS, '#33aaff']);
    expect(loadColorPresets(storage)).toEqual([...DEFAULT_COLOR_PRESETS, '#33aaff']);
  });

  it('restores missing defaults from an older saved palette', () => {
    expect(loadColorPresets(memoryStorage('["#123456"]'))).toEqual([...DEFAULT_COLOR_PRESETS, '#123456']);
  });

  it('replaces the previous Butter Paper defaults without treating them as custom presets', () => {
    expect(loadColorPresets(memoryStorage('["#3b82f6","#ef4444","#123456"]'))).toEqual([...DEFAULT_COLOR_PRESETS, '#123456']);
  });
});
