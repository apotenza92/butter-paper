import type { PageModel } from '@butter-paper/core';

export interface ThumbnailLayout {
  index: number;
  top: number;
  itemHeight: number;
  previewHeight: number;
}

export interface VisibleThumbnailRange {
  startIndex: number;
  endIndex: number;
}

export const THUMBNAIL_MAX_WIDTH = 188;
export const THUMBNAIL_MAX_HEIGHT = 220;

const MIN_THUMBNAIL_WIDTH = 120;
const CONTAINER_GAP = 0;
const ITEM_GAP = 0;
const ITEM_HEADER_HEIGHT = 48;
const ITEM_BOTTOM_PADDING = 16;
const ITEM_DIVIDER_HEIGHT = 1;
const CARD_HORIZONTAL_INSET = 16;
const CARD_HORIZONTAL_PADDING = 16;

export function computeThumbnailPreviewWidth(viewportWidth: number): number {
  return Math.max(
    MIN_THUMBNAIL_WIDTH,
    Math.min(THUMBNAIL_MAX_WIDTH, viewportWidth - CARD_HORIZONTAL_INSET - CARD_HORIZONTAL_PADDING),
  );
}

export function computeThumbnailPreviewHeight(
  page: Pick<PageModel, 'size'>,
  previewWidth: number,
  maxPreviewHeight = THUMBNAIL_MAX_HEIGHT,
): number {
  const safePageWidth = Math.max(1, page.size.width);
  const safePageHeight = Math.max(1, page.size.height);
  const scaledHeight = Math.round((previewWidth * safePageHeight) / safePageWidth);

  return Math.max(1, Math.min(maxPreviewHeight, scaledHeight));
}

export function computeThumbnailContentSize(
  page: Pick<PageModel, 'size'>,
  previewWidth: number,
  previewHeight: number,
): { width: number; height: number; scale: number } {
  const safePageWidth = Math.max(1, page.size.width);
  const safePageHeight = Math.max(1, page.size.height);
  const scale = Math.max(
    0.001,
    Math.min(
      previewWidth / safePageWidth,
      previewHeight / safePageHeight,
    ),
  );

  return {
    width: safePageWidth * scale,
    height: safePageHeight * scale,
    scale,
  };
}

export function computeThumbnailItemHeight(previewHeight: number): number {
  return ITEM_HEADER_HEIGHT + previewHeight + ITEM_BOTTOM_PADDING + ITEM_DIVIDER_HEIGHT;
}

export function buildThumbnailLayouts(
  pages: readonly Pick<PageModel, 'index' | 'size'>[],
  previewWidth: number,
): { layouts: ThumbnailLayout[]; totalHeight: number } {
  let top = CONTAINER_GAP;
  const layouts = pages.map((page) => {
    const previewHeight = computeThumbnailPreviewHeight(page, previewWidth);
    const itemHeight = computeThumbnailItemHeight(previewHeight);
    const layout = {
      index: page.index,
      top,
      itemHeight,
      previewHeight,
    };

    top += itemHeight + ITEM_GAP;
    return layout;
  });

  return {
    layouts,
    totalHeight: top,
  };
}

export function computeVisibleThumbnailRange(
  layouts: readonly ThumbnailLayout[],
  scrollTop: number,
  viewportHeight: number,
  overscan = 2,
  bootstrapCount = 0,
): VisibleThumbnailRange {
  if (layouts.length === 0) {
    return { startIndex: 0, endIndex: -1 };
  }

  if (viewportHeight <= 0) {
    return {
      startIndex: 0,
      endIndex: Math.min(layouts.length - 1, Math.max(-1, bootstrapCount - 1)),
    };
  }

  const visibleTop = Math.max(0, scrollTop);
  const visibleBottom = visibleTop + Math.max(0, viewportHeight);

  let startIndex = 0;
  let endIndex = layouts.length - 1;

  for (let index = 0; index < layouts.length; index += 1) {
    const layout = layouts[index];
    if (layout.top + layout.itemHeight >= visibleTop) {
      startIndex = Math.max(0, index - overscan);
      break;
    }
  }

  for (let index = layouts.length - 1; index >= 0; index -= 1) {
    const layout = layouts[index];
    if (layout.top <= visibleBottom) {
      endIndex = Math.min(layouts.length - 1, index + overscan);
      break;
    }
  }

  return { startIndex, endIndex };
}
