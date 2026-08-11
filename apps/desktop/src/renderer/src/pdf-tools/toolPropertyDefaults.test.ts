import { describe, expect, it } from 'vitest';
import { createCloudPlusMarkup, createPenMarkup, createRectangleMarkup, pdfPoint, rect, resolveMarkupAppearance } from '@butter-paper/core';
import { applyToolPropertyValues, builtInToolPropertyValues } from './toolPropertyDefaults';

describe('tool property defaults', () => {
  it('derives each tool value set from its declared built-in defaults', () => {
    expect(builtInToolPropertyValues('rectangle')).toEqual({
      strokeColor: '#ff0000',
      strokeWidthPt: 1,
      fillColor: null,
      opacity: 1,
    });
    expect(builtInToolPropertyValues('pen')).toMatchObject({ smoothCurves: true });
  });

  it('applies visual defaults when a new markup is committed', () => {
    const markup = applyToolPropertyValues(createRectangleMarkup({
      id: 'rectangle-1',
      pageIndex: 0,
      rect: rect(0, 0, 20, 10),
    }), {
      strokeColor: '#123456',
      strokeWidthPt: 3,
      fillColor: '#abcdef',
      opacity: 0.4,
    });

    expect(resolveMarkupAppearance(markup)).toMatchObject({
      stroke: { color: '#123456', widthPt: 3 },
      fill: { color: '#abcdef' },
      opacity: 0.4,
    });
  });

  it('stores pen smoothing and compound cloud properties on their geometry', () => {
    const pen = applyToolPropertyValues(createPenMarkup({
      id: 'pen-1',
      pageIndex: 0,
      paths: [[pdfPoint(0, 0), pdfPoint(10, 10)]],
    }), { smoothCurves: false });
    expect(pen).toMatchObject({ kind: 'pen', smoothCurves: false });

    const cloudPlus = applyToolPropertyValues(createCloudPlusMarkup({
      id: 'cloud-plus-1',
      pageIndex: 0,
      cloud: { controlPath: [pdfPoint(0, 0), pdfPoint(10, 0), pdfPoint(10, 10)] },
      leader: { points: [pdfPoint(10, 10), pdfPoint(20, 20)] },
      textBox: rect(20, 20, 100, 40),
      text: '',
    }), { cloudIntensity: 3 });
    expect(cloudPlus).toMatchObject({ kind: 'cloud-plus', cloud: { borderEffectIntensity: 3 } });
  });
});
