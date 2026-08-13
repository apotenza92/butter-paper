import { describe, expect, it } from 'vitest';
import { ANNOTATION_FONT_OPTIONS, annotationFontCssFamily, annotationFontDisplayName, compatibleAnnotationFontId } from './fonts.js';

describe('annotation font catalog', () => {
  it('keeps the short compatible-font list alphabetical by its visible name', () => {
    const labels = ANNOTATION_FONT_OPTIONS.map((font) => font.label);
    expect(labels).toEqual([...labels].sort((left, right) => left.localeCompare(right)));
    expect(labels).toEqual([
      'Arial',
      'Courier New',
      'Helvetica',
      'Times New Roman',
    ]);
  });

  it.each([
    ['ArialMT', 'Arimo'],
    ['Calibri', 'Arimo'],
    ['CourierNewPSMT', 'Roboto Mono'],
    ['Helvetica-Bold', 'Helvetica'],
    ['TimesNewRomanPS-BoldMT', 'Tinos'],
    ['Uninstalled Project Font', 'Noto Sans'],
  ] as const)('maps %s to the quiet compatible family %s', (source, expected) => {
    expect(compatibleAnnotationFontId(source)).toBe(expected);
  });

  it('prefers installed system fonts and keeps bundled compatible fallbacks', () => {
    expect(annotationFontDisplayName('Arial')).toBe('Arial');
    expect(annotationFontCssFamily('Arial')).toBe('Arial, Arimo, sans-serif');
    expect(annotationFontCssFamily('Courier New')).toBe('"Courier New", "Roboto Mono", monospace');
    expect(annotationFontCssFamily('Helvetica')).toBe('Helvetica, Arial, Arimo, sans-serif');
    expect(annotationFontCssFamily('Times New Roman')).toBe('"Times New Roman", Tinos, serif');
  });

  it('keeps Noto Sans available as a hidden Unicode and import fallback', () => {
    expect(ANNOTATION_FONT_OPTIONS.some((font) => font.id === 'Noto Sans')).toBe(false);
    expect(annotationFontDisplayName('Noto Sans')).toBe('Noto Sans');
  });
});
