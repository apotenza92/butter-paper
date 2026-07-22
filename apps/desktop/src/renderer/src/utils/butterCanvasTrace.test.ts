import { describe, expect, it } from 'vitest';
import { traceImageToMarkups, type TraceImageSource } from './butterCanvasTrace';

function whiteImage(width: number, height: number): Uint8ClampedArray {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let index = 0; index < data.length; index += 4) {
    data[index] = 255;
    data[index + 1] = 255;
    data[index + 2] = 255;
    data[index + 3] = 255;
  }
  return data;
}

function setBlack(data: Uint8ClampedArray, width: number, x: number, y: number): void {
  const index = (y * width + x) * 4;
  data[index] = 0;
  data[index + 1] = 0;
  data[index + 2] = 0;
  data[index + 3] = 255;
}

describe('Butter Canvas tracing', () => {
  it('converts detected dark runs into editable polyline markups', () => {
    const width = 12;
    const height = 8;
    const data = whiteImage(width, height);
    for (let x = 2; x <= 10; x += 1) {
      setBlack(data, width, x, 3);
    }
    const image: TraceImageSource = { width, height, data };

    const markups = traceImageToMarkups({
      image,
      assetRect: { x: 100, y: 200, width: 110, height: 70 },
      sensitivity: 50,
      minSegmentPixels: 4,
      maxSegments: 10,
      idPrefix: 'test-trace',
    });

    expect(markups.length).toBeGreaterThan(0);
    expect(markups[0]).toMatchObject({
      id: 'test-trace-1',
      kind: 'polyline',
      pageIndex: 0,
      color: '#2563eb',
    });
    if (markups[0].kind !== 'polyline') {
      throw new Error('Expected polyline trace markup.');
    }
    expect(markups[0].points[0]).toMatchObject({ x: 120, y: 230 });
    expect(markups[0].points[1]).toMatchObject({ x: 200, y: 230 });
  });

  it('limits tracing to the selected normalized zone', () => {
    const width = 20;
    const height = 10;
    const data = whiteImage(width, height);
    for (let x = 0; x <= 19; x += 1) {
      setBlack(data, width, x, 5);
    }
    const image: TraceImageSource = { width, height, data };

    const markups = traceImageToMarkups({
      image,
      assetRect: { x: 0, y: 0, width: 190, height: 90 },
      sensitivity: 50,
      minSegmentPixels: 3,
      zone: { x: 0.25, y: 0, width: 0.5, height: 1 },
    });

    expect(markups.length).toBeGreaterThan(0);
    if (markups[0].kind !== 'polyline') {
      throw new Error('Expected polyline trace markup.');
    }
    expect(markups[0].points[0].x).toBeGreaterThanOrEqual(50);
    expect(markups[0].points[1].x).toBeLessThanOrEqual(140);
  });

  it('ignores fully transparent dark pixels', () => {
    const width = 10;
    const height = 4;
    const data = whiteImage(width, height);
    for (let x = 1; x <= 8; x += 1) {
      const index = (2 * width + x) * 4;
      data[index] = 0;
      data[index + 1] = 0;
      data[index + 2] = 0;
      data[index + 3] = 0;
    }

    expect(traceImageToMarkups({
      image: { width, height, data },
      assetRect: { x: 0, y: 0, width: 90, height: 30 },
      sensitivity: 100,
      minSegmentPixels: 3,
    })).toHaveLength(0);
  });

  it('can emit line or pen markups for the selected output mode', () => {
    const width = 12;
    const height = 8;
    const data = whiteImage(width, height);
    for (let x = 2; x <= 10; x += 1) {
      setBlack(data, width, x, 3);
    }
    const image: TraceImageSource = { width, height, data };

    const lineMarkups = traceImageToMarkups({
      image,
      assetRect: { x: 0, y: 0, width: 110, height: 70 },
      sensitivity: 50,
      minSegmentPixels: 4,
      maxSegments: 1,
      outputMode: 'line',
    });
    const penMarkups = traceImageToMarkups({
      image,
      assetRect: { x: 0, y: 0, width: 110, height: 70 },
      sensitivity: 50,
      minSegmentPixels: 4,
      maxSegments: 1,
      outputMode: 'pen',
    });

    expect(lineMarkups[0]?.kind).toBe('line');
    expect(penMarkups[0]?.kind).toBe('pen');
  });
});
