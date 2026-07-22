import { describe, expect, it } from 'vitest';
import type { Markup } from './document.js';
import {
  createArcMarkup,
  createAreaMarkup,
  createArrowMarkup,
  createCalloutMarkup,
  createCloudMarkup,
  createCloudPlusMarkup,
  createDimensionMarkup,
  createEllipseMarkup,
  createHighlightMarkup,
  createImageMarkup,
  createLengthMarkup,
  createLineMarkup,
  createPenMarkup,
  createPolygonMarkup,
  createPolylengthMarkup,
  createPolylineMarkup,
  createRectangleMarkup,
  createSnapshotMarkup,
  createTextBoxMarkup,
} from './markup.js';
import { defaultMarkupAppearance, resolveMarkupAppearance } from './appearance.js';
import { pdfPoint, rect } from './points.js';

const BUILT_IN_KINDS = [
  'rectangle', 'ellipse', 'arc', 'line', 'arrow', 'dimension', 'length', 'polylength',
  'area', 'polyline', 'polygon', 'pen', 'highlight', 'cloud', 'cloud-plus', 'text-box',
  'callout', 'image', 'snapshot',
] as const;

describe('canonical markup appearance', () => {
  it.each(BUILT_IN_KINDS)('stores a complete resolved default on new %s markups', (kind) => {
    const markup = createBuiltInMarkup(kind);
    expect(markup.appearance).toEqual(defaultMarkupAppearance(kind));
    expect(resolveMarkupAppearance(markup)).toEqual(defaultMarkupAppearance(kind));
  });

  it.each(BUILT_IN_KINDS)('resolves direct non-default %s data without customization state', (kind) => {
    const original = createBuiltInMarkup(kind);
    const defaults = defaultMarkupAppearance(kind);
    const markup = {
      ...original,
      color: '#111111',
      opacity: 0.8,
      appearance: {
        ...(defaults.stroke ? { stroke: { color: '#123456', widthPt: 3.25 } } : {}),
        ...(defaults.fill ? { fill: { color: '#abcdef' } } : {}),
        ...(defaults.text ? {
          text: {
            color: '#654321',
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

    const resolved = resolveMarkupAppearance(markup);
    expect(resolved.opacity).toBe(0.35);
    if (defaults.stroke) expect(resolved.stroke).toEqual({ color: '#123456', widthPt: 3.25 });
    if (defaults.fill) expect(resolved.fill).toEqual({ color: '#abcdef' });
    if (defaults.text) {
      expect(resolved.text).toEqual({
        color: '#654321',
        fontId: 'FutureFont',
        fontSizePt: 27,
        lineHeightPt: 31,
        align: 'right',
        insetPt: 7,
      });
    }
  });

  it('uses appearance, then legacy fields, then canonical defaults', () => {
    const markup = createTextBoxMarkup({
      id: 'precedence',
      pageIndex: 0,
      rect: rect(0, 0, 20, 20),
      text: 'Text',
      color: '#112233',
      borderColor: '#223344',
      borderWidth: 2,
      fontSizePt: 18,
      appearance: {
        stroke: { color: '#334455' },
        text: { color: '#445566', align: 'center' },
      },
    });

    expect(resolveMarkupAppearance(markup)).toEqual({
      stroke: { color: '#334455', widthPt: 2 },
      fill: { color: null },
      text: {
        color: '#445566',
        fontId: 'Helvetica',
        fontSizePt: 18,
        lineHeightPt: 20.7,
        align: 'center',
        insetPt: 5,
      },
      opacity: 1,
      blendMode: 'normal',
    });
  });
});

function createBuiltInMarkup(kind: typeof BUILT_IN_KINDS[number]): Exclude<Markup, { kind: 'imported-annotation' }> {
  const base = { id: `appearance-${kind}`, pageIndex: 0, source: { source: 'butter' as const } };
  const points = [pdfPoint(0, 0), pdfPoint(20, 0), pdfPoint(20, 20)];
  switch (kind) {
    case 'rectangle': return createRectangleMarkup({ ...base, rect: rect(0, 0, 20, 20) });
    case 'ellipse': return createEllipseMarkup({ ...base, rect: rect(0, 0, 20, 20) });
    case 'arc': return createArcMarkup({ ...base, rect: rect(0, 0, 20, 20), angle1: 0, angle2: 90 });
    case 'line': return createLineMarkup({ ...base, start: points[0], end: points[1] });
    case 'arrow': return createArrowMarkup({ ...base, start: points[0], end: points[1] });
    case 'dimension': return createDimensionMarkup({ ...base, start: points[0], end: points[1], dimensionLineOffset: 10, text: 'Dimension' });
    case 'length': return createLengthMarkup({ ...base, start: points[0], end: points[1] });
    case 'polylength': return createPolylengthMarkup({ ...base, points });
    case 'area': return createAreaMarkup({ ...base, points });
    case 'polyline': return createPolylineMarkup({ ...base, points });
    case 'polygon': return createPolygonMarkup({ ...base, points });
    case 'pen': return createPenMarkup({ ...base, paths: [points] });
    case 'highlight': return createHighlightMarkup({ ...base, paths: [points] });
    case 'cloud': return createCloudMarkup({ ...base, controlPath: points });
    case 'cloud-plus': return createCloudPlusMarkup({ ...base, cloud: { controlPath: points }, leader: { points }, textBox: rect(20, 20, 50, 30), text: 'Cloud+' });
    case 'text-box': return createTextBoxMarkup({ ...base, rect: rect(0, 0, 20, 20), text: 'Text' });
    case 'callout': return createCalloutMarkup({ ...base, leader: { points }, textBox: rect(20, 20, 50, 30), text: 'Callout' });
    case 'image': return createImageMarkup({ ...base, rect: rect(0, 0, 20, 20), dataUrl: 'data:image/png;base64,', mimeType: 'image/png' });
    case 'snapshot': return createSnapshotMarkup({ ...base, rect: rect(0, 0, 20, 20), dataUrl: 'data:image/png;base64,', mimeType: 'image/png' });
  }
}
