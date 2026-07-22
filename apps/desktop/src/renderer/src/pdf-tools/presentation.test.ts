import { describe, expect, it } from 'vitest';
import { defaultMarkupAppearance, pdfPoint, rect, type Markup } from '@butter-paper/core';
import { getMarkupToolDefinition } from './toolRegistry';

const CUSTOM_STROKE = '#123456';
const CUSTOM_FILL = '#abcdef';
const CUSTOM_TEXT = '#654321';

describe('data-driven markup presentation', () => {
  it.each(presentationGallery())('$kind primitives reflect stored appearance', (markup) => {
    const defaults = defaultMarkupAppearance(markup.kind);
    const customized = {
      ...markup,
      appearance: {
        ...(defaults.stroke ? { stroke: { color: CUSTOM_STROKE, widthPt: 3.25 } } : {}),
        ...(defaults.fill ? { fill: { color: CUSTOM_FILL } } : {}),
        ...(defaults.text ? {
          text: {
            color: CUSTOM_TEXT,
            fontId: 'FutureFont',
            fontSizePt: 27,
            lineHeightPt: 31,
            align: 'right' as const,
            insetPt: 7,
          },
        } : {}),
        opacity: 0.35,
      },
    } as Markup;
    const primitives = getMarkupToolDefinition(customized)?.render?.getContentPrimitives(customized as never, {
      page: { id: 'page-1', index: 0, size: { width: 600, height: 800 }, rotation: 0 },
      phase: 'idle',
    }) ?? [];
    const styles = primitives.flatMap((primitive) => 'style' in primitive && primitive.style ? [primitive.style] : []);

    expect(primitives.length).toBeGreaterThan(0);
    expect(primitives.some((primitive) => ('opacity' in primitive && primitive.opacity === 0.35)
      || ('style' in primitive && primitive.style?.opacity === 0.35))).toBe(true);
    if (defaults.stroke) {
      expect(styles.some((style) => style.stroke === CUSTOM_STROKE && style.strokeWidth === 3.25)).toBe(true);
    }
    if (defaults.fill) {
      expect(styles.some((style) => style.fill === CUSTOM_FILL)).toBe(true);
    }
    if (defaults.text) {
      const textPrimitive = primitives.find((primitive) => primitive.kind === 'textBox');
      expect(textPrimitive).toMatchObject({
        style: {
          textColor: CUSTOM_TEXT,
          fontSizePt: 27,
          lineHeightPt: 31,
          textAlign: 'right',
          textInsetPt: 7,
          opacity: 0.35,
        },
      });
    }
  });
});

function presentationGallery(): readonly Markup[] {
  const base = { pageIndex: 0 };
  const points = [pdfPoint(10, 10), pdfPoint(110, 10), pdfPoint(80, 70)];
  return [
    { ...base, id: 'rectangle', kind: 'rectangle', rect: rect(10, 10, 100, 60) },
    { ...base, id: 'ellipse', kind: 'ellipse', rect: rect(10, 10, 100, 60) },
    { ...base, id: 'arc', kind: 'arc', rect: rect(10, 10, 100, 60), angle1: 0, angle2: 90 },
    { ...base, id: 'line', kind: 'line', start: points[0], end: points[1] },
    { ...base, id: 'arrow', kind: 'arrow', start: points[0], end: points[1] },
    { ...base, id: 'dimension', kind: 'dimension', start: points[0], end: points[1], dimensionLineOffset: 24, text: '100 ft' },
    { ...base, id: 'length', kind: 'length', start: points[0], end: points[1] },
    { ...base, id: 'polylength', kind: 'polylength', points },
    { ...base, id: 'area', kind: 'area', points },
    { ...base, id: 'polyline', kind: 'polyline', points },
    { ...base, id: 'polygon', kind: 'polygon', points },
    { ...base, id: 'pen', kind: 'pen', paths: [points] },
    { ...base, id: 'highlight', kind: 'highlight', paths: [points] },
    { ...base, id: 'cloud', kind: 'cloud', controlPath: points },
    { ...base, id: 'cloud-plus', kind: 'cloud-plus', cloud: { controlPath: points }, leader: { points }, textBox: rect(120, 20, 80, 40), text: 'Cloud+' },
    { ...base, id: 'text-box', kind: 'text-box', rect: rect(10, 10, 100, 40), text: 'Text' },
    { ...base, id: 'callout', kind: 'callout', leader: { points }, textBox: rect(120, 20, 80, 40), text: 'Callout' },
    { ...base, id: 'image', kind: 'image', rect: rect(10, 10, 100, 60), dataUrl: 'data:image/png;base64,', mimeType: 'image/png' },
    { ...base, id: 'snapshot', kind: 'snapshot', rect: rect(10, 10, 100, 60), dataUrl: 'data:image/png;base64,', mimeType: 'image/png' },
  ];
}
