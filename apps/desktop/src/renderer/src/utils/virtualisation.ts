export interface PageLayout {
  index: number;
  top: number;
  left: number;
  width: number;
  height: number;
  columnIndex: number;
  rowIndex: number;
}

export interface VisibleRange {
  startIndex: number;
  endIndex: number;
}

export interface PageLayoutPanPadding {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

export interface PageLayoutOptions {
  mode?: 'continuous' | 'columns' | 'single-page';
  cadViewOrganisation?: 'columns' | 'rows';
  pagesPerColumn?: number;
  currentPageIndex?: number;
  viewportHeight?: number;
}

export interface ViewportRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function computeVisibleRange(
  pageLayouts: PageLayout[],
  scrollTop: number,
  viewportHeight: number,
  overscan = 2,
  bootstrapCount = 0,
): VisibleRange {
  if (pageLayouts.length === 0) {
    return { startIndex: 0, endIndex: -1 };
  }

  if (viewportHeight <= 0) {
    return {
      startIndex: 0,
      endIndex: Math.min(pageLayouts.length - 1, Math.max(-1, bootstrapCount - 1)),
    };
  }

  const visibleTop = Math.max(0, scrollTop);
  const visibleBottom = visibleTop + Math.max(0, viewportHeight);

  let startIndex = 0;
  let endIndex = pageLayouts.length - 1;

  for (let index = 0; index < pageLayouts.length; index += 1) {
    const layout = pageLayouts[index];
    if (layout.top + layout.height >= visibleTop) {
      startIndex = Math.max(0, index - overscan);
      break;
    }
  }

  for (let index = pageLayouts.length - 1; index >= 0; index -= 1) {
    const layout = pageLayouts[index];
    if (layout.top <= visibleBottom) {
      endIndex = Math.min(pageLayouts.length - 1, index + overscan);
      break;
    }
  }

  return { startIndex, endIndex };
}

export function computeVisibleLayoutPositions(
  pageLayouts: readonly PageLayout[],
  viewport: ViewportRect,
  overscanPx = 0,
  bootstrapCount = 0,
): number[] {
  if (pageLayouts.length === 0) {
    return [];
  }

  if (viewport.width <= 0 || viewport.height <= 0) {
    return Array.from(
      { length: Math.min(pageLayouts.length, Math.max(0, bootstrapCount)) },
      (_, index) => index,
    );
  }

  const visibleLeft = Math.max(0, viewport.left - overscanPx);
  const visibleTop = Math.max(0, viewport.top - overscanPx);
  const visibleRight = viewport.left + viewport.width + overscanPx;
  const visibleBottom = viewport.top + viewport.height + overscanPx;
  const positions: number[] = [];

  for (let index = 0; index < pageLayouts.length; index += 1) {
    const layout = pageLayouts[index];
    const intersects = layout.left + layout.width >= visibleLeft
      && layout.left <= visibleRight
      && layout.top + layout.height >= visibleTop
      && layout.top <= visibleBottom;
    if (intersects) {
      positions.push(index);
    }
  }

  return positions;
}

export function buildPageLayouts(
  pages: Array<{ index: number; width: number; height: number }>,
  zoom: number,
  viewportWidth: number,
  gap = 24,
  panPadding: PageLayoutPanPadding = { left: 0, right: 0, top: 0, bottom: 0 },
  options: PageLayoutOptions = {},
): { layouts: PageLayout[]; totalHeight: number; totalWidth: number; panPadding: PageLayoutPanPadding } {
  const mode = options.mode ?? 'continuous';
  const pagesPerColumn = clampPagesPerColumn(options.pagesPerColumn ?? 10);
  const layoutPages = mode === 'single-page'
    ? pages.filter((page) => page.index === (options.currentPageIndex ?? pages[0]?.index ?? 0))
    : pages;
  const layouts: PageLayout[] = [];
  const safePanPadding = {
    left: Math.max(0, panPadding.left),
    right: Math.max(0, panPadding.right),
    top: Math.max(0, panPadding.top),
    bottom: Math.max(0, panPadding.bottom),
  };
  const viewportHeight = Math.max(1, options.viewportHeight ?? 1);
  const safeViewportWidth = Math.max(1, viewportWidth);

  if (mode === 'columns') {
    return buildCadPageLayouts(
      layoutPages,
      zoom,
      safeViewportWidth,
      gap,
      safePanPadding,
      pagesPerColumn,
      options.cadViewOrganisation ?? 'columns',
    );
  }

  if (mode === 'single-page') {
    return buildSinglePageLayout(layoutPages, zoom, safeViewportWidth, viewportHeight, gap, safePanPadding);
  }

  let top = safePanPadding.top + gap;
  let totalHeight = safePanPadding.top + gap;
  let totalWidth = safeViewportWidth;
  const contentWidth = Math.max(0, safeViewportWidth - gap * 2);

  for (let position = 0; position < layoutPages.length; position += 1) {
    const page = layoutPages[position];
    const width = Math.max(1, page.width * zoom);
    const height = Math.max(1, page.height * zoom);
    const left = safePanPadding.left + Math.max(gap, (contentWidth - width) / 2 + gap);

    layouts.push({
      index: page.index,
      top,
      left,
      width,
      height,
      columnIndex: 0,
      rowIndex: position,
    });

    top += height + gap;
    totalHeight = top;
    totalWidth = Math.max(totalWidth, left + width + gap + safePanPadding.right);
  }

  totalHeight += safePanPadding.bottom;

  return {
    layouts,
    totalHeight,
    totalWidth,
    panPadding: safePanPadding,
  };
}

function buildSinglePageLayout(
  pages: Array<{ index: number; width: number; height: number }>,
  zoom: number,
  viewportWidth: number,
  viewportHeight: number,
  gap: number,
  panPadding: PageLayoutPanPadding,
): { layouts: PageLayout[]; totalHeight: number; totalWidth: number; panPadding: PageLayoutPanPadding } {
  const page = pages[0];
  if (!page) {
    return {
      layouts: [],
      totalHeight: viewportHeight,
      totalWidth: viewportWidth,
      panPadding,
    };
  }

  const width = Math.max(1, page.width * zoom);
  const height = Math.max(1, page.height * zoom);
  const usableWidth = Math.max(1, viewportWidth - panPadding.left - panPadding.right);
  const usableHeight = Math.max(1, viewportHeight - panPadding.top - panPadding.bottom);
  const left = panPadding.left + Math.max(gap, (usableWidth - width) / 2);
  const top = panPadding.top + Math.max(gap, (usableHeight - height) / 2);

  return {
    layouts: [{
      index: page.index,
      top,
      left,
      width,
      height,
      columnIndex: 0,
      rowIndex: 0,
    }],
    totalHeight: Math.max(viewportHeight, top + height + gap + panPadding.bottom),
    totalWidth: Math.max(viewportWidth, left + width + gap + panPadding.right),
    panPadding,
  };
}

function buildCadPageLayouts(
  pages: Array<{ index: number; width: number; height: number }>,
  zoom: number,
  viewportWidth: number,
  gap: number,
  panPadding: PageLayoutPanPadding,
  pagesPerColumn: number,
  organisation: 'columns' | 'rows',
): { layouts: PageLayout[]; totalHeight: number; totalWidth: number; panPadding: PageLayoutPanPadding } {
  const pagesPerGroup = pagesPerColumn;
  const columnCount = organisation === 'columns'
    ? Math.max(1, Math.ceil(pages.length / pagesPerGroup))
    : Math.min(pages.length || 1, pagesPerGroup);
  const rowCount = organisation === 'columns'
    ? Math.min(pages.length || 1, pagesPerGroup)
    : Math.max(1, Math.ceil(pages.length / pagesPerGroup));
  const columnWidths = Array.from({ length: columnCount }, () => 1);
  const rowHeights = Array.from({ length: rowCount }, () => 1);
  const scaledPages = pages.map((page, position) => ({
    index: page.index,
    width: Math.max(1, page.width * zoom),
    height: Math.max(1, page.height * zoom),
    columnIndex: organisation === 'columns' ? Math.floor(position / pagesPerGroup) : position % pagesPerGroup,
    rowIndex: organisation === 'columns' ? position % pagesPerGroup : Math.floor(position / pagesPerGroup),
  }));

  for (const page of scaledPages) {
    columnWidths[page.columnIndex] = Math.max(columnWidths[page.columnIndex] ?? 1, page.width);
    rowHeights[page.rowIndex] = Math.max(rowHeights[page.rowIndex] ?? 1, page.height);
  }
  const gridWidth = columnWidths.reduce((width, columnWidth) => width + columnWidth, gap * (columnWidths.length + 1));
  const gridHeight = rowHeights.reduce((height, rowHeight) => height + rowHeight + gap, gap);
  const contentWidth = Math.max(1, viewportWidth - panPadding.left - panPadding.right);
  const baseLeft = panPadding.left + Math.max(0, (contentWidth - gridWidth) / 2);
  const columnLefts: number[] = [];
  let columnLeft = baseLeft + gap;

  for (const columnWidth of columnWidths) {
    columnLefts.push(columnLeft);
    columnLeft += columnWidth + gap;
  }

  const layouts: PageLayout[] = [];
  const rowTops: number[] = [];
  let rowTop = panPadding.top + gap;
  for (const rowHeight of rowHeights) {
    rowTops.push(rowTop);
    rowTop += rowHeight + gap;
  }

  for (const page of scaledPages) {
    const columnWidth = columnWidths[page.columnIndex] ?? page.width;
    const left = (columnLefts[page.columnIndex] ?? gap) + (columnWidth - page.width) / 2;
    const top = rowTops[page.rowIndex] ?? gap;
    layouts.push({
      index: page.index,
      top,
      left,
      width: page.width,
      height: page.height,
      columnIndex: page.columnIndex,
      rowIndex: page.rowIndex,
    });
  }

  return {
    layouts,
    totalHeight: Math.max(1, panPadding.top + gridHeight + panPadding.bottom),
    totalWidth: Math.max(viewportWidth, baseLeft + gridWidth + panPadding.right),
    panPadding,
  };
}

function clampPagesPerColumn(value: number): number {
  if (!Number.isFinite(value)) {
    return 10;
  }
  return Math.min(100, Math.max(1, Math.round(value)));
}
