import { describe, expect, it } from 'vitest';
import {
  analyzeOutlineContinuity, basicSsim, boundaryDistance, comparisonHeatmap, comparisonOverlay,
  connectedComponents, createExclusionMask, createInkMask, cropImage, encodePgm, encodePpm,
  excludeMaskPixels, luminanceDifference, maskIoU, registerMasks, translateImage,
} from '../../scripts/bluebeam-compat/image-analysis.mjs';

function mask(width: number, height: number, points: Array<[number, number]>) {
  const data = new Uint8Array(width * height);
  for (const [x, y] of points) data[y * width + x] = 1;
  return { width, height, data };
}

function image(width: number, height: number, pixels: Array<[number, number, number, number]> = []) {
  const data = new Uint8Array(width * height * 4).fill(255);
  pixels.forEach((rgba, index) => data.set(rgba, index * 4));
  return { width, height, data };
}

describe('Bluebeam visual analysis', () => {
  it('extracts ink and connected components', () => {
    const source = image(4, 2);
    source.data.set([0, 0, 0, 255], 0);
    source.data.set([0, 0, 0, 255], 4);
    source.data.set([0, 0, 0, 255], 7 * 4);
    const result = createInkMask(source);
    expect([...result.data]).toEqual([1, 1, 0, 0, 0, 0, 0, 1]);
    expect(connectedComponents(result)).toEqual([
      { area: 2, bounds: { x: 0, y: 0, width: 2, height: 1 } },
      { area: 1, bounds: { x: 3, y: 1, width: 1, height: 1 } },
    ]);
  });

  it('registers translated masks deterministically', () => {
    const reference = mask(5, 5, [[1, 1], [2, 1], [1, 2]]);
    const candidate = mask(5, 5, [[2, 2], [3, 2], [2, 3]]);
    expect(registerMasks(reference, candidate, { maximumOffset: 2 })).toEqual({ x: -1, y: -1, iou: 1 });
    expect(maskIoU(reference, candidate, { x: -1, y: -1 })).toBe(1);
    expect(boundaryDistance(reference, candidate, { x: -1, y: -1 })).toEqual({ mean: 0, p95: 0, hausdorff: 0 });
  });

  it('reports perfect SSIM for identical images and visible differences in a heatmap', () => {
    const left = image(2, 1, [[0, 0, 0, 255], [255, 255, 255, 255]]);
    const right = image(2, 1, [[0, 0, 0, 255], [255, 0, 255, 255]]);
    expect(basicSsim(left, left)).toBeCloseTo(1, 12);
    expect(basicSsim(left, right)).toBeLessThan(1);
    expect([...comparisonHeatmap(left, right).data]).toEqual([0, 255, 0, 255, 255, 0, 0, 255]);
  });

  it('crops ROIs and emits portable binary image formats', () => {
    const source = image(2, 2, [[1, 2, 3, 255], [4, 5, 6, 255], [7, 8, 9, 255], [10, 11, 12, 255]]);
    const cropped = cropImage(source, { x: 1, y: 0, width: 1, height: 2 });
    expect([...cropped.data]).toEqual([4, 5, 6, 255, 10, 11, 12, 255]);
    expect(encodePpm(cropped).subarray(0, 11).toString()).toBe('P6\n1 2\n255\n');
    expect(encodePgm(mask(1, 1, [[0, 0]])).subarray(-1)[0]).toBe(255);
  });

  it('reports disconnected cloud fragments, their gap, and closed interiors', () => {
    const closed = mask(7, 7, [
      [1, 1], [2, 1], [3, 1], [4, 1], [5, 1],
      [1, 2], [5, 2], [1, 3], [5, 3], [1, 4], [5, 4],
      [1, 5], [2, 5], [3, 5], [4, 5], [5, 5],
    ]);
    expect(analyzeOutlineContinuity(closed)).toMatchObject({
      componentCount: 1,
      disconnectedPixels: 0,
      maximumNearestComponentGap: 0,
      enclosedBackgroundRegions: 1,
    });
    const broken = mask(7, 3, [[1, 1], [2, 1], [5, 1]]);
    expect(analyzeOutlineContinuity(broken)).toMatchObject({
      componentCount: 2,
      disconnectedPixels: 1,
      maximumNearestComponentGap: 2,
      minimumIntercomponentGap: 2,
      enclosedBackgroundRegions: 0,
    });
  });

  it('applies deterministic exclusion masks to ink and pixel metrics', () => {
    const source = image(2, 1, [[0, 0, 0, 255], [0, 0, 0, 255]]);
    const exclusionSource = image(2, 1, [[0, 0, 0, 255], [255, 255, 255, 255]]);
    const exclusion = createExclusionMask(exclusionSource);
    expect([...exclusion.data]).toEqual([1, 0]);
    expect([...excludeMaskPixels(createInkMask(source), exclusion).data]).toEqual([0, 1]);
    const changed = image(2, 1, [[255, 255, 255, 255], [0, 0, 0, 255]]);
    expect(basicSsim(source, changed, { exclusionMask: exclusion })).toBe(1);
    expect(luminanceDifference(source, changed, { exclusionMask: exclusion })).toEqual({
      meanAbsoluteError: 0,
      rootMeanSquareError: 0,
      maximumAbsoluteError: 0,
    });
  });

  it('aligns translated pixels before luminance and heatmap comparison', () => {
    const source = image(3, 1, [[255, 255, 255, 255], [0, 0, 0, 255], [255, 255, 255, 255]]);
    const translated = image(3, 1, [[255, 255, 255, 255], [255, 255, 255, 255], [0, 0, 0, 255]]);
    const registered = translateImage(translated, { x: -1, y: 0 });
    expect([...registered.data]).toEqual([...source.data]);
    expect(basicSsim(source, registered)).toBeCloseTo(1, 12);
    expect(luminanceDifference(source, registered).meanAbsoluteError).toBe(0);
  });

  it('creates a color-coded registered mask overlay', () => {
    const left = mask(3, 1, [[0, 0], [1, 0]]);
    const right = mask(3, 1, [[1, 0], [2, 0]]);
    expect([...comparisonOverlay(left, right, { x: -1, y: 0 }).data]).toEqual([
      0, 0, 0, 255,
      0, 0, 0, 255,
      255, 255, 255, 255,
    ]);
  });
});
