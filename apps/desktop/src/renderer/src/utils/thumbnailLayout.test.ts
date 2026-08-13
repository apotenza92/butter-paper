import { describe, expect, it } from 'vitest';
import {
  buildThumbnailLayouts,
  computeThumbnailContentSize,
  computeThumbnailItemHeight,
  computeThumbnailPreviewHeight,
  computeThumbnailPreviewWidth,
  computeVisibleThumbnailRange,
} from './thumbnailLayout';

describe('thumbnail layout helpers', () => {
  const a3LandscapePreviewHeight = Math.round((188 * 297) / 420);

  it('uses an A3 landscape preview box for every page shape', () => {
    const portraitHeight = computeThumbnailPreviewHeight({ size: { width: 612, height: 792 } }, 188);
    const landscapeHeight = computeThumbnailPreviewHeight({ size: { width: 1584, height: 1224 } }, 188);

    expect(portraitHeight).toBe(a3LandscapePreviewHeight);
    expect(landscapeHeight).toBe(portraitHeight);
  });

  it('fits A3 landscape exactly inside the fixed preview box', () => {
    const previewHeight = computeThumbnailPreviewHeight({ size: { width: 420, height: 297 } }, 188);
    const contentSize = computeThumbnailContentSize({ size: { width: 420, height: 297 } }, 188, previewHeight);

    expect(contentSize.width).toBe(188);
    expect(contentSize.height).toBeCloseTo((188 * 297) / 420);
  });

  it('fits portrait pages inside the fixed A3 landscape preview box', () => {
    const previewHeight = computeThumbnailPreviewHeight({ size: { width: 612, height: 792 } }, 188);
    const contentSize = computeThumbnailContentSize({ size: { width: 612, height: 792 } }, 188, previewHeight);

    expect(contentSize.height).toBe(a3LandscapePreviewHeight);
    expect(contentSize.width).toBeCloseTo((a3LandscapePreviewHeight * 612) / 792);
  });

  it('builds fixed item heights for different page dimensions', () => {
    const previewWidth = computeThumbnailPreviewWidth(220);
    const previewHeight = Math.round((previewWidth * 297) / 420);
    const { layouts, totalHeight } = buildThumbnailLayouts([
      { index: 0, size: { width: 612, height: 792 } },
      { index: 1, size: { width: 1584, height: 1224 } },
    ], previewWidth);

    expect(layouts).toHaveLength(2);
    expect(layouts[0]?.previewHeight).toBe(previewHeight);
    expect(layouts[0]?.itemHeight).toBe(computeThumbnailItemHeight(previewHeight));
    expect(layouts[1]?.previewHeight).toBe(previewHeight);
    expect(layouts[1]?.itemHeight).toBe(computeThumbnailItemHeight(previewHeight));
    expect(layouts[0]?.top).toBe(8);
    expect(layouts[1]?.top).toBe(layouts[0]!.top + layouts[0]!.itemHeight + 16);
    expect(totalHeight).toBe(layouts[1]!.top + layouts[1]!.itemHeight + 16);
    for (const layout of layouts) {
      expect(Number.isInteger(layout.top)).toBe(true);
      expect(Number.isInteger(layout.previewHeight)).toBe(true);
      expect(Number.isInteger(layout.itemHeight)).toBe(true);
    }
  });

  it('uses the default item header, content gap, and vertical padding around every preview', () => {
    expect(computeThumbnailItemHeight(220)).toBe(32 + 10 + 220 + 20);
  });

  it('computes the visible range against fixed item heights', () => {
    const { layouts } = buildThumbnailLayouts([
      { index: 0, size: { width: 612, height: 792 } },
      { index: 1, size: { width: 1584, height: 1224 } },
      { index: 2, size: { width: 612, height: 792 } },
    ], 188);

    const visible = computeVisibleThumbnailRange(layouts, layouts[1]!.top + 10, layouts[1]!.itemHeight - 20, 0);

    expect(visible).toMatchObject({ startIndex: 1, endIndex: 1 });
  });

  it('returns an empty bootstrap range before the viewport is measured', () => {
    const { layouts } = buildThumbnailLayouts([
      { index: 0, size: { width: 612, height: 792 } },
      { index: 1, size: { width: 612, height: 792 } },
    ], 188);

    expect(computeVisibleThumbnailRange(layouts, 0, 0, 2, 0)).toEqual({ startIndex: 0, endIndex: -1 });
  });
});
