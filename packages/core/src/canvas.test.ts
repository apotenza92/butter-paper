import { describe, expect, it } from 'vitest';
import {
  BUTTER_CANVAS_FILE_EXTENSION,
  BUTTER_CANVAS_SCHEMA_VERSION,
  createButterCanvasDocument,
  createLineMarkup,
  parseButterCanvasDocument,
  pdfPoint,
  serializeButterCanvasDocument,
  type ButterCanvasAsset,
  type ButterCanvasScale,
} from './index.js';

const imageAsset = {
  id: 'asset-1',
  kind: 'image',
  name: 'floor-plan.png',
  rect: { x: -120, y: 80, width: 640, height: 480 },
  dataUrl: 'data:image/png;base64,aW1hZ2U=',
  mimeType: 'image/png',
  rotation: 5,
  opacity: 0.72,
  visible: true,
  locked: false,
  source: {
    type: 'image-file',
    fileName: 'floor-plan.png',
  },
} as const satisfies ButterCanvasAsset;

const canvasScale = {
  source: 'calibrated',
  name: '1 px = 10 mm',
  canvasUnits: 'px',
  realUnits: 'mm',
  canvasUnitsPerRealUnit: 0.1,
  precision: {
    mode: 'decimal',
    value: 1,
  },
} as const satisfies ButterCanvasScale;

describe('Butter Canvas documents', () => {
  it('uses the .bpc extension for native canvas files', () => {
    expect(BUTTER_CANVAS_FILE_EXTENSION).toBe('.bpc');
  });

  it('round-trips a Butter Canvas document through native JSON', () => {
    const document = createButterCanvasDocument({
      id: 'canvas-1',
      title: 'Site sketch',
      createdAt: '2026-06-15T00:00:00.000Z',
      camera: { x: 120, y: -80, zoom: 1.5 },
      grid: { visible: true, snap: true, size: 25, pattern: 'dots' },
      snap: { grid: true, angleIncrementDeg: 30, sensitivityPx: 12 },
      scale: canvasScale,
      traceDefaults: {
        sensitivity: 68,
        zone: { x: 0.1, y: 0.2, width: 0.7, height: 0.5 },
      },
      assets: [imageAsset],
      markups: [
        createLineMarkup({
          id: 'line-1',
          pageIndex: 0,
          start: pdfPoint(0, 0),
          end: pdfPoint(100, 0),
        }),
      ],
    });

    const parsed = parseButterCanvasDocument(serializeButterCanvasDocument(document));

    expect(parsed).toEqual(document);
    expect(parsed.schemaVersion).toBe(BUTTER_CANVAS_SCHEMA_VERSION);
    expect(parsed.grid).toMatchObject({ visible: true, snap: true, size: 25, pattern: 'dots' });
    expect(parsed.snap).toMatchObject({ grid: true, angleIncrementDeg: 30, sensitivityPx: 12 });
    expect(parsed.scale).toEqual(canvasScale);
    expect(parsed.assets).toEqual([imageAsset]);
    expect(parsed.markups[0]).toMatchObject({ id: 'line-1', pageIndex: 0, kind: 'line' });
  });

  it('rejects unsupported schema versions clearly', () => {
    const document = createButterCanvasDocument({ id: 'canvas-1' });
    const encoded = JSON.stringify({ ...document, schemaVersion: 999 });

    expect(() => parseButterCanvasDocument(encoded)).toThrow(/Unsupported Butter Canvas schema version: 999/);
  });

  it('rejects malformed critical fields', () => {
    const document = createButterCanvasDocument({ id: 'canvas-1' });
    const encoded = JSON.stringify({
      ...document,
      grid: {
        ...document.grid,
        size: 0,
      },
    });

    expect(() => parseButterCanvasDocument(encoded)).toThrow(/grid\.size must be greater than 0/);
  });
});
