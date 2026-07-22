import { describe, expect, it } from 'vitest';
import {
  THUMBNAIL_MAX_HEIGHT,
  buildThumbnailLayouts,
  computeThumbnailContentSize,
  computeThumbnailItemHeight,
  computeThumbnailPreviewHeight,
  computeThumbnailPreviewWidth,
  computeVisibleThumbnailRange,
} from './thumbnailLayout';

describe('thumbnail layout helpers', () => {
  it('caps portrait thumbnails at the maximum preview height', () => {
    const previewHeight = computeThumbnailPreviewHeight({ size: { width: 612, height: 792 } }, 188);

    expect(previewHeight).toBe(THUMBNAIL_MAX_HEIGHT);
  });

  it('shrinks landscape thumbnails to match the page aspect ratio', () => {
    const previewHeight = computeThumbnailPreviewHeight({ size: { width: 1584, height: 1224 } }, 188);

    expect(previewHeight).toBe(145);
    expect(previewHeight).toBeLessThan(THUMBNAIL_MAX_HEIGHT);
  });

  it('fits annotation overlay content to capped portrait thumbnails', () => {
    const contentSize = computeThumbnailContentSize({ size: { width: 612, height: 792 } }, 188, THUMBNAIL_MAX_HEIGHT);

    expect(contentSize.height).toBe(THUMBNAIL_MAX_HEIGHT);
    expect(contentSize.width).toBeCloseTo(170);
    expect(contentSize.scale).toBeCloseTo(THUMBNAIL_MAX_HEIGHT / 792);
  });

  it('builds variable item heights from preview dimensions', () => {
    const previewWidth = computeThumbnailPreviewWidth(220);
    const { layouts, totalHeight } = buildThumbnailLayouts([
      { index: 0, size: { width: 612, height: 792 } },
      { index: 1, size: { width: 1584, height: 1224 } },
    ], previewWidth);

    expect(layouts).toHaveLength(2);
    expect(layouts[0]?.previewHeight).toBe(THUMBNAIL_MAX_HEIGHT);
    expect(layouts[0]?.itemHeight).toBe(computeThumbnailItemHeight(THUMBNAIL_MAX_HEIGHT));
    expect(layouts[1]?.previewHeight).toBe(145);
    expect(layouts[1]?.itemHeight).toBe(computeThumbnailItemHeight(145));
    expect(layouts[1]?.top).toBeGreaterThan(layouts[0]!.top + layouts[0]!.itemHeight);
    expect(totalHeight).toBeGreaterThan(layouts[1]!.top + layouts[1]!.itemHeight);
  });

  it('computes the visible range against variable item heights', () => {
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
