import { describe, expect, it } from 'vitest';
import { EMPTY_CANVAS_PADDING, getEffectiveCanvasPadding } from './canvasPadding';

describe('canvas padding helpers', () => {
  const pages = [{ index: 0, width: 1200, height: 1600 }];
  const padding = { left: 400, right: 500, top: 300, bottom: 600 };

  it('keeps manual pan padding even when the PDF is larger than the viewport', () => {
    expect(getEffectiveCanvasPadding(pages, 800, 600, 'manual', padding)).toEqual(padding);
  });

  it('keeps explicit pan padding while fit presets are active', () => {
    expect(getEffectiveCanvasPadding(pages, 800, 600, 'fit-width', padding)).toEqual(padding);
    expect(getEffectiveCanvasPadding(pages, 800, 600, 'fit-page', padding)).toEqual(padding);
  });

  it('suppresses padding until a document and measured viewport exist', () => {
    expect(getEffectiveCanvasPadding([], 800, 600, 'manual', padding)).toEqual(EMPTY_CANVAS_PADDING);
    expect(getEffectiveCanvasPadding(pages, 0, 600, 'manual', padding)).toEqual(EMPTY_CANVAS_PADDING);
    expect(getEffectiveCanvasPadding(pages, 800, 0, 'manual', padding)).toEqual(EMPTY_CANVAS_PADDING);
  });
});
