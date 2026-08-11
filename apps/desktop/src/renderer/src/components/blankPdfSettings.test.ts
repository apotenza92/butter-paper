import { describe, expect, it } from 'vitest';
import {
  BLANK_PDF_SETTINGS_STORAGE_KEY,
  DEFAULT_BLANK_PDF_SETTINGS,
  formatBlankPdfPaperPresetOption,
  formatBlankPdfSettings,
  loadBlankPdfSettings,
  resolveBlankPdfDimensions,
  saveBlankPdfSettings,
  type BlankPdfSettings,
} from './blankPdfSettings';

function settings(overrides: Partial<BlankPdfSettings> = {}): BlankPdfSettings {
  return { ...DEFAULT_BLANK_PDF_SETTINGS, ...overrides };
}

function memoryStorage(initialValue: string | null = null) {
  let value = initialValue;
  return {
    getItem: (key: string) => key === BLANK_PDF_SETTINGS_STORAGE_KEY ? value : null,
    setItem: (key: string, nextValue: string) => {
      if (key === BLANK_PDF_SETTINGS_STORAGE_KEY) value = nextValue;
    },
  };
}

describe('blank PDF defaults', () => {
  it('starts new users with A3 landscape', () => {
    expect(loadBlankPdfSettings(memoryStorage())).toEqual(DEFAULT_BLANK_PDF_SETTINGS);
    expect(resolveBlankPdfDimensions(DEFAULT_BLANK_PDF_SETTINGS)).toEqual({ widthMm: 420, heightMm: 297 });
    expect(formatBlankPdfSettings(DEFAULT_BLANK_PDF_SETTINGS)).toBe('A3 · Landscape');
  });

  it('persists and restores the chosen default', () => {
    const storage = memoryStorage();
    const selected = settings({ preset: 'a4', orientation: 'portrait' });
    saveBlankPdfSettings(storage, selected);
    expect(loadBlankPdfSettings(storage)).toEqual(selected);
  });

  it('adds pattern defaults when it loads settings saved before patterns existed', () => {
    const legacySettings = {
      preset: 'a4',
      orientation: 'portrait',
      customWidth: '210',
      customHeight: '297',
    };
    expect(loadBlankPdfSettings(memoryStorage(JSON.stringify(legacySettings)))).toEqual({
      ...DEFAULT_BLANK_PDF_SETTINGS,
      ...legacySettings,
    });
  });

  it('falls back safely when persisted settings are malformed or invalid', () => {
    expect(loadBlankPdfSettings(memoryStorage('{not-json'))).toEqual(DEFAULT_BLANK_PDF_SETTINGS);
    expect(loadBlankPdfSettings(memoryStorage(JSON.stringify(settings({ preset: 'custom', customWidth: '9' }))))).toEqual(DEFAULT_BLANK_PDF_SETTINGS);
  });
});

describe('resolveBlankPdfDimensions', () => {
  it('uses A4 portrait by its exact metric dimensions', () => {
    expect(resolveBlankPdfDimensions(settings({ preset: 'a4', orientation: 'portrait' }))).toEqual({ widthMm: 210, heightMm: 297 });
  });

  it('swaps preset dimensions for landscape', () => {
    expect(resolveBlankPdfDimensions(settings({ preset: 'a0', orientation: 'landscape' }))).toEqual({ widthMm: 1189, heightMm: 841 });
  });

  it('adds a 10 mm grey grid to the create request', () => {
    expect(resolveBlankPdfDimensions(settings({ patternType: 'grid' }))).toEqual({
      widthMm: 420,
      heightMm: 297,
      pattern: {
        type: 'grid',
        spacingMm: 10,
        color: '#d1d5db',
      },
    });
  });

  it('resolves custom dot spacing and colour', () => {
    expect(resolveBlankPdfDimensions(settings({
      patternType: 'dots',
      patternSpacingPreset: 'custom',
      customPatternSpacing: '7.5',
      patternColorPreset: 'custom',
      customPatternColor: '#4E95CC',
    }))).toMatchObject({
      pattern: {
        type: 'dots',
        spacingMm: 7.5,
        color: '#4e95cc',
      },
    });
  });

  it('resolves the light blue preset and includes the pattern in the default label', () => {
    const selected = settings({ patternType: 'isometric', patternColorPreset: 'blue' });
    expect(resolveBlankPdfDimensions(selected)).toMatchObject({
      pattern: { color: '#4e95cc' },
    });
    expect(formatBlankPdfSettings(selected)).toBe('A3 · Landscape · Isometric');
  });

  it('uses explicit custom width and height without applying orientation', () => {
    expect(resolveBlankPdfDimensions(settings({
      preset: 'custom',
      orientation: 'landscape',
      customWidth: '320.5',
      customHeight: '450',
    }))).toEqual({ widthMm: 320.5, heightMm: 450 });
  });

  it.each([
    ['', '297'],
    ['9', '297'],
    ['210', '5001'],
  ])('rejects invalid custom dimensions', (customWidth, customHeight) => {
    expect(() => resolveBlankPdfDimensions(settings({ preset: 'custom', customWidth, customHeight }))).toThrow(/between 10 and 5000 mm/);
  });
});

describe('formatBlankPdfPaperPresetOption', () => {
  it.each([
    ['a5', 'A5 — 148 mm wide × 210 mm high', 'A5 — 210 mm wide × 148 mm high'],
    ['a4', 'A4 — 210 mm wide × 297 mm high', 'A4 — 297 mm wide × 210 mm high'],
    ['a3', 'A3 — 297 mm wide × 420 mm high', 'A3 — 420 mm wide × 297 mm high'],
    ['a2', 'A2 — 420 mm wide × 594 mm high', 'A2 — 594 mm wide × 420 mm high'],
    ['a1', 'A1 — 594 mm wide × 841 mm high', 'A1 — 841 mm wide × 594 mm high'],
    ['a0', 'A0 — 841 mm wide × 1189 mm high', 'A0 — 1189 mm wide × 841 mm high'],
  ] as const)('shows %s dimensions in the selected orientation', (preset, portrait, landscape) => {
    expect(formatBlankPdfPaperPresetOption(preset, 'portrait')).toBe(portrait);
    expect(formatBlankPdfPaperPresetOption(preset, 'landscape')).toBe(landscape);
  });
});
