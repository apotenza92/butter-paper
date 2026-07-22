import { describe, expect, it } from 'vitest';
import {
  captureViewportCentreAnchor,
  clampWheelZoomFrameDelta,
  computeBluebeamWheelZoom,
  computePageLayoutGap,
  distributeStrictVisibleOverviewLayouts,
  getContinuousCurrentPageLayout,
  isViewportPanButtonPressed,
  resolveCadOverviewCenteredView,
  resolveCadOverviewEntryZoom,
  resolveMiddleDoubleClickZoomPreset,
  resolveMinimumZoomPanBounds,
  resolveAnchoredLayoutTransitionScroll,
  resolveColumnOverviewPreviewBatch,
  resolveColumnOverviewPreviewRenderWidth,
  resolveNearbyPagePreviewWarmCandidates,
  resolveViewportCentreAnchorScroll,
  resolvePageCenteredScroll,
  resolveVisiblePageViewportRect,
  resolveViewportVisibleOverscanPx,
  shouldWarmNearbyPagePreviews,
  shouldDeferColumnOverviewPreview,
  shouldAutoUpdateCurrentPageFromViewport,
  shouldPreserveColumnAnchorAfterLayoutChange,
  shouldPreserveViewportAnchorAfterLayoutChange,
  shouldRenderColumnOverviewPreview,
  shouldUseCanvasColumnOverview,
  shouldUseColumnOverviewMode,
} from './DocumentViewport';

describe('document viewport panning buttons', () => {
  it('tracks the pressed state for the initiating pan button', () => {
    expect(isViewportPanButtonPressed(1, 0)).toBe(true);
    expect(isViewportPanButtonPressed(4, 1)).toBe(true);
    expect(isViewportPanButtonPressed(2, 2)).toBe(true);
  });

  it('treats released or unrelated buttons as no longer panning', () => {
    expect(isViewportPanButtonPressed(0, 1)).toBe(false);
    expect(isViewportPanButtonPressed(1, 1)).toBe(false);
    expect(isViewportPanButtonPressed(4, 0)).toBe(false);
  });
});

describe('document viewport CAD centering', () => {
  const pages = Array.from({ length: 12 }, (_, index) => ({
    index,
    width: 100,
    height: 100,
  }));

  it('adds top canvas space so the first active preview can sit in the viewport centre', () => {
    const result = resolveCadOverviewCenteredView(pages, 1, 500, 500, 20, 'columns', 4, 0);

    expect(result.canvasPadding.top).toBeGreaterThan(0);
    expect(result.scroll.top + 250).toBeCloseTo(result.canvasPadding.top + 70);
  });

  it('adds bottom canvas space so the last active preview can sit in the viewport centre', () => {
    const result = resolveCadOverviewCenteredView(pages, 1, 500, 500, 20, 'columns', 4, 11);

    expect(result.canvasPadding.bottom).toBeGreaterThan(0);
    expect(result.scroll.top).toBeGreaterThan(0);
    expect(result.scroll.top + 250).toBeCloseTo(result.canvasPadding.top + 430);
  });
});

describe('document viewport column overview crossover', () => {
  it('uses overview mode through zoomed-out column navigation', () => {
    expect(shouldUseColumnOverviewMode({
      layoutMode: 'columns',
      zoom: 0.3,
      mountedPageCount: 40,
    })).toBe(true);
  });

  it('uses overview mode when overscan would mount too many full pages', () => {
    expect(shouldUseColumnOverviewMode({
      layoutMode: 'columns',
      zoom: 0.4,
      mountedPageCount: 120,
    })).toBe(true);
  });

  it('restores full rendering once the column view is inspectable', () => {
    expect(shouldUseColumnOverviewMode({
      layoutMode: 'columns',
      zoom: 0.4,
      mountedPageCount: 72,
    })).toBe(false);
  });

  it('keeps overview active through the zoom crossover until the view is clearly inspectable', () => {
    expect(shouldUseColumnOverviewMode({
      layoutMode: 'columns',
      zoom: 0.35,
      mountedPageCount: 72,
      currentlyActive: true,
    })).toBe(true);
  });

  it('keeps column overview active during zoom gestures before settling back to full pages', () => {
    expect(shouldUseColumnOverviewMode({
      layoutMode: 'columns',
      zoom: 0.5,
      mountedPageCount: 72,
      viewportInMotion: true,
    })).toBe(true);
  });

  it('uses the same overview path for very zoomed-out continuous view', () => {
    expect(shouldUseColumnOverviewMode({
      layoutMode: 'continuous',
      zoom: 0.1,
      mountedPageCount: 200,
    })).toBe(true);
  });

  it('does not affect single-page layouts', () => {
    expect(shouldUseColumnOverviewMode({
      layoutMode: 'single-page',
      zoom: 0.1,
      mountedPageCount: 1,
    })).toBe(false);
  });
});

describe('document viewport column overview previews', () => {
  it('uses the canvas overview before large documents can queue hundreds of preview tiles', () => {
    expect(shouldUseCanvasColumnOverview({
      useColumnOverviewMode: true,
      strictVisiblePageCount: 270,
    })).toBe(true);
  });

  it('keeps small column overviews as interactive tiles', () => {
    expect(shouldUseCanvasColumnOverview({
      useColumnOverviewMode: true,
      strictVisiblePageCount: 6,
    })).toBe(false);
  });

  it('does not use the canvas overview outside overview mode', () => {
    expect(shouldUseCanvasColumnOverview({
      useColumnOverviewMode: false,
      strictVisiblePageCount: 270,
    })).toBe(false);
  });

  it('allows strict visible overview previews during motion', () => {
    expect(shouldRenderColumnOverviewPreview(0.5, true, true)).toBe(true);
    expect(shouldDeferColumnOverviewPreview(true, true)).toBe(false);
  });

  it('allows strict visible overview previews while settled above the overview entry threshold', () => {
    expect(shouldRenderColumnOverviewPreview(0.35, false, true)).toBe(true);
  });

  it('defers non-strict overview preview renders during motion', () => {
    expect(shouldRenderColumnOverviewPreview(0.5, true, false)).toBe(true);
    expect(shouldDeferColumnOverviewPreview(true, false)).toBe(true);
  });

  it('keeps low-zoom overview previews enabled after motion settles', () => {
    expect(shouldRenderColumnOverviewPreview(0.2, false, false)).toBe(true);
    expect(shouldDeferColumnOverviewPreview(false, false)).toBe(false);
  });

  it('prioritises missing strict visible canvas previews before overscan previews', () => {
    const layouts = Array.from({ length: 80 }, (_, index) => ({
      index,
      left: index * 10,
      top: 0,
      width: 8,
      height: 10,
      columnIndex: index,
      rowIndex: 0,
    }));

    const batch = resolveColumnOverviewPreviewBatch({
      layouts,
      renderablePageIndices: new Set(layouts.map((layout) => layout.index)),
      availablePreviewPageIndices: new Set([18]),
      requestedPreviewPageIndices: new Set([19]),
      strictVisiblePageIndices: new Set([17, 18, 19, 20, 21]),
    });

    expect(batch.slice(0, 3).map((layout) => layout.index)).toEqual([17, 20, 21]);
    expect(batch).toHaveLength(64);
  });

  it('uses a small canvas overview prefetch batch after visible previews are filled', () => {
    const layouts = Array.from({ length: 40 }, (_, index) => ({
      index,
      left: index * 10,
      top: 0,
      width: 8,
      height: 10,
      columnIndex: index,
      rowIndex: 0,
    }));

    const batch = resolveColumnOverviewPreviewBatch({
      layouts,
      renderablePageIndices: new Set(layouts.map((layout) => layout.index)),
      availablePreviewPageIndices: new Set([17, 18, 19, 20, 21]),
      requestedPreviewPageIndices: new Set(),
      strictVisiblePageIndices: new Set([17, 18, 19, 20, 21]),
    });

    expect(batch).toHaveLength(12);
    expect(batch[0]?.index).toBe(0);
  });

  it('re-requests visible canvas previews when an existing preview is below required width', () => {
    const layouts = Array.from({ length: 8 }, (_, index) => ({
      index,
      left: index * 10,
      top: 0,
      width: 120,
      height: 160,
      columnIndex: index,
      rowIndex: 0,
    }));

    const batch = resolveColumnOverviewPreviewBatch({
      layouts,
      renderablePageIndices: new Set(layouts.map((layout) => layout.index)),
      availablePreviewPageIndices: new Set(layouts.map((layout) => layout.index)),
      requestedPreviewPageIndices: new Set(),
      requiredPreviewWidths: new Map([[3, 162]]),
      strictVisiblePageIndices: new Set([3]),
    });

    expect(batch[0]?.index).toBe(3);
  });

  it('does not re-request visible canvas previews when a requested upgrade is already large enough', () => {
    const layouts = Array.from({ length: 8 }, (_, index) => ({
      index,
      left: index * 10,
      top: 0,
      width: 120,
      height: 160,
      columnIndex: index,
      rowIndex: 0,
    }));

    const batch = resolveColumnOverviewPreviewBatch({
      layouts,
      renderablePageIndices: new Set(layouts.map((layout) => layout.index)),
      availablePreviewPageIndices: new Set(),
      requestedPreviewPageIndices: new Set([3]),
      requestedPreviewWidths: new Map([[3, 162]]),
      requiredPreviewWidths: new Map([[3, 162]]),
      strictVisiblePageIndices: new Set([3]),
    });

    expect(batch.map((layout) => layout.index)).not.toContain(3);
  });

  it('re-requests the pointer page when a requested visible upgrade is not large enough', () => {
    const layouts = Array.from({ length: 8 }, (_, index) => ({
      index,
      left: index * 10,
      top: 0,
      width: 120,
      height: 160,
      columnIndex: index,
      rowIndex: 0,
    }));

    const batch = resolveColumnOverviewPreviewBatch({
      layouts,
      renderablePageIndices: new Set(layouts.map((layout) => layout.index)),
      availablePreviewPageIndices: new Set(),
      requestedPreviewPageIndices: new Set([3]),
      requestedPreviewWidths: new Map([[3, 162]]),
      requiredPreviewWidths: new Map([[3, 240]]),
      strictVisiblePageIndices: new Set([3]),
      pointerPageIndex: 3,
    });

    expect(batch[0]?.index).toBe(3);
  });

  it('orders strict visible overview previews in screen order by default', () => {
    const layouts = Array.from({ length: 16 }, (_, index) => ({
      index,
      left: index * 10,
      top: 0,
      width: 8,
      height: 10,
      columnIndex: index,
      rowIndex: 0,
    }));

    const batch = resolveColumnOverviewPreviewBatch({
      layouts,
      renderablePageIndices: new Set(layouts.map((layout) => layout.index)),
      availablePreviewPageIndices: new Set(),
      requestedPreviewPageIndices: new Set(),
      strictVisiblePageIndices: new Set(layouts.map((layout) => layout.index)),
    });

    expect(batch.slice(0, 8).map((layout) => layout.index)).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
  });

  it('prioritises the pointer page before other visible overview previews', () => {
    const layouts = Array.from({ length: 8 }, (_, index) => ({
      index,
      left: index * 10,
      top: 0,
      width: 8,
      height: 10,
      columnIndex: index,
      rowIndex: 0,
    }));

    const batch = resolveColumnOverviewPreviewBatch({
      layouts,
      renderablePageIndices: new Set(layouts.map((layout) => layout.index)),
      availablePreviewPageIndices: new Set(),
      requestedPreviewPageIndices: new Set(),
      strictVisiblePageIndices: new Set(layouts.map((layout) => layout.index)),
      pointerPageIndex: 5,
    });

    expect(batch[0]?.index).toBe(5);
  });

  it('prioritises the viewport focus page before other visible overview previews', () => {
    const layouts = Array.from({ length: 8 }, (_, index) => ({
      index,
      left: index * 10,
      top: 0,
      width: 8,
      height: 10,
      columnIndex: index,
      rowIndex: 0,
    }));

    const batch = resolveColumnOverviewPreviewBatch({
      layouts,
      renderablePageIndices: new Set(layouts.map((layout) => layout.index)),
      availablePreviewPageIndices: new Set(),
      requestedPreviewPageIndices: new Set(),
      strictVisiblePageIndices: new Set(layouts.map((layout) => layout.index)),
      focusPageIndex: 4,
    });

    expect(batch[0]?.index).toBe(4);
  });

  it('renders visible and pointer overview previews at screen-aware quality', () => {
    expect(resolveColumnOverviewPreviewRenderWidth({
      displayWidth: 180,
      isStrictlyVisible: true,
      isPointerPage: false,
      isFocusPage: true,
    })).toBe(306);
    expect(resolveColumnOverviewPreviewRenderWidth({
      displayWidth: 180,
      isStrictlyVisible: true,
      isPointerPage: false,
    })).toBe(243);
    expect(resolveColumnOverviewPreviewRenderWidth({
      displayWidth: 180,
      isStrictlyVisible: true,
      isPointerPage: true,
    })).toBe(360);
    expect(resolveColumnOverviewPreviewRenderWidth({
      displayWidth: 180,
      isStrictlyVisible: false,
      isPointerPage: false,
    })).toBe(48);
  });

  it('can still distribute strict visible overview previews across viewport regions', () => {
    const layouts = Array.from({ length: 16 }, (_, index) => ({
      index,
      left: index * 10,
      top: 0,
      width: 8,
      height: 10,
      columnIndex: index,
      rowIndex: 0,
    }));

    expect(distributeStrictVisibleOverviewLayouts(
      layouts,
      new Set(layouts.map((layout) => layout.index)),
      4,
    ).slice(0, 8).map((layout) => layout.index)).toEqual([0, 4, 8, 12, 1, 5, 9, 13]);
  });
});

describe('document viewport CAD entry zoom', () => {
  const pages = Array.from({ length: 20 }, (_, index) => ({
    id: `page-${index}`,
    index,
    size: { width: 100, height: 200 },
    rotation: 0 as const,
  }));

  it('fits the whole CAD column layout into the viewport when possible', () => {
    expect(resolveCadOverviewEntryZoom({
      pages,
      viewportSize: { width: 500, height: 500 },
      pagesPerColumn: 10,
      cadViewOrganisation: 'columns',
      gap: 24,
    })).toBe(0.22);
  });

  it('clamps CAD entry zoom to the minimum supported viewer zoom', () => {
    expect(resolveCadOverviewEntryZoom({
      pages: Array.from({ length: 200 }, (_, index) => ({
        id: `page-${index}`,
        index,
        size: { width: 1000, height: 2000 },
        rotation: 0 as const,
      })),
      viewportSize: { width: 500, height: 500 },
      pagesPerColumn: 100,
      cadViewOrganisation: 'columns',
      gap: 24,
    })).toBe(0.0625);
  });
});

describe('document viewport nearby page warming', () => {
  const idleDiagnostics = {
    pageRenderReady: true,
    thumbnailRenderReady: true,
    queuedPageRenders: 0,
    queuedThumbnailRenders: 0,
    inflightPageRenders: 0,
    inflightThumbnailRenders: 0,
  };

  it('warms only when both render paths are idle and the viewport is settled', () => {
    expect(shouldWarmNearbyPagePreviews({
      diagnostics: idleDiagnostics,
      viewportInMotion: false,
      strictVisiblePageCount: 1,
    })).toBe(true);
    expect(shouldWarmNearbyPagePreviews({
      diagnostics: { ...idleDiagnostics, queuedThumbnailRenders: 1 },
      viewportInMotion: false,
      strictVisiblePageCount: 1,
    })).toBe(false);
    expect(shouldWarmNearbyPagePreviews({
      diagnostics: idleDiagnostics,
      viewportInMotion: true,
      strictVisiblePageCount: 1,
    })).toBe(false);
  });

  it('caps nearby page warming candidates to two pages', () => {
    expect(resolveNearbyPagePreviewWarmCandidates({
      visiblePageIndices: [4, 5],
      pageCount: 12,
      layoutMode: 'continuous',
    })).toEqual([3, 6]);
  });
});

describe('document viewport visible overscan', () => {
  it('holds continuous overscan at the strict viewport while motion or critical render work is active', () => {
    expect(resolveViewportVisibleOverscanPx({
      layoutMode: 'continuous',
      viewportInMotion: true,
      renderBacklogIdle: true,
    })).toBe(0);
    expect(resolveViewportVisibleOverscanPx({
      layoutMode: 'continuous',
      viewportInMotion: false,
      renderBacklogIdle: false,
    })).toBe(0);
  });

  it('restores continuous overscan after the render backlog is idle', () => {
    expect(resolveViewportVisibleOverscanPx({
      layoutMode: 'continuous',
      viewportInMotion: false,
      renderBacklogIdle: true,
    })).toBe(1200);
  });

  it('keeps column overview overscan independent from page render backlog', () => {
    expect(resolveViewportVisibleOverscanPx({
      layoutMode: 'columns',
      viewportInMotion: true,
      renderBacklogIdle: false,
    })).toBe(320);
  });
});

describe('document viewport visible page crop rect', () => {
  it('returns the page-local visible rectangle for a partially clipped page', () => {
    expect(resolveVisiblePageViewportRect(
      { index: 2, left: 100, top: 200, width: 400, height: 600, columnIndex: 0, rowIndex: 0 },
      250,
      450,
      { width: 300, height: 300 },
    )).toEqual({
      x: 150,
      y: 250,
      width: 250,
      height: 300,
    });
  });

  it('returns null when the page is outside the viewport', () => {
    expect(resolveVisiblePageViewportRect(
      { index: 2, left: 100, top: 200, width: 400, height: 600, columnIndex: 0, rowIndex: 0 },
      600,
      900,
      { width: 300, height: 300 },
    )).toBeNull();
  });
});

describe('document viewport wheel zoom', () => {
  it('caps accelerated trackpad deltas per frame', () => {
    expect(clampWheelZoomFrameDelta(40)).toBe(40);
    expect(clampWheelZoomFrameDelta(900)).toBe(120);
    expect(clampWheelZoomFrameDelta(-900)).toBe(-120);
    expect(clampWheelZoomFrameDelta(Number.NaN)).toBe(0);
  });

  it('applies wheel zoom with the measured Bluebeam-style exponential curve', () => {
    expect(computeBluebeamWheelZoom(1, -120)).toBeCloseTo(1.219, 3);
    expect(computeBluebeamWheelZoom(1, 120)).toBeCloseTo(0.82, 3);
    expect(computeBluebeamWheelZoom(1.2233, -60)).toBeCloseTo(1.351, 3);
    expect(computeBluebeamWheelZoom(0.07, 120)).toBe(0.0625);
  });
});

describe('document viewport middle-button double click', () => {
  it('enters fit page from manual zoom', () => {
    expect(resolveMiddleDoubleClickZoomPreset('manual')).toBe('fit-page');
  });

  it('enters fit width from manual zoom in continuous view', () => {
    expect(resolveMiddleDoubleClickZoomPreset('manual', 'continuous')).toBe('fit-width');
  });

  it('preserves active fit presets', () => {
    expect(resolveMiddleDoubleClickZoomPreset('fit-width', 'continuous')).toBe('fit-width');
    expect(resolveMiddleDoubleClickZoomPreset('fit-page', 'continuous')).toBe('fit-page');
  });
});

describe('document viewport panning bounds', () => {
  it('keeps horizontal pan within the minimum-zoom visible overlap', () => {
    expect(resolveMinimumZoomPanBounds([
      { index: 0, left: 256, top: 24, width: 800, height: 1000, columnIndex: 0, rowIndex: 0 },
    ], 600, 900, 1, 'x')).toEqual({ min: -319, max: 1031 });
  });

  it('allows all but a sliver of the minimum-zoom page footprint to leave the viewport', () => {
    expect(resolveMinimumZoomPanBounds([
      { index: 0, left: 256, top: 24, width: 800, height: 1000, columnIndex: 0, rowIndex: 0 },
    ], 600, 2000, 0.0625, 'x')).toEqual({ min: 56, max: 656 });
  });

  it('supports vertical pan bounds for single-page view', () => {
    expect(resolveMinimumZoomPanBounds([
      { index: 0, left: 24, top: 128, width: 800, height: 1000, columnIndex: 0, rowIndex: 0 },
    ], 700, 2000, 0.0625, 'y')).toEqual({ min: -72, max: 628 });
  });

  it('still returns expandable bounds when the current canvas has no overflow', () => {
    expect(resolveMinimumZoomPanBounds([
      { index: 0, left: 24, top: 24, width: 400, height: 600, columnIndex: 0, rowIndex: 0 },
    ], 600, 0, 1, 'x')).toEqual({ min: -563.5, max: 411.5 });
  });
});

describe('document viewport page spacing', () => {
  it('keeps the page gap proportional to zoomed page size', () => {
    expect(computePageLayoutGap(0.05)).toBe(1.2);
    expect(computePageLayoutGap(0.3)).toBe(7.2);
  });

  it('keeps normal reading zoom spacing close to the existing layout', () => {
    expect(computePageLayoutGap(1)).toBe(24);
  });

  it('scales the page gap up at high zoom', () => {
    expect(computePageLayoutGap(4)).toBe(96);
  });
});

describe('document viewport layout transitions', () => {
  it('captures the page point at the viewport centre', () => {
    expect(captureViewportCentreAnchor(
      [{ index: 1, left: 100, top: 200, width: 400, height: 600, columnIndex: 0, rowIndex: 0 }],
      0,
      0,
      { width: 600, height: 800 },
    )).toEqual({
      pageIndex: 1,
      ratioX: 0.5,
      ratioY: 1 / 3,
    });
  });

  it('captures the nearest page point when the viewport centre is whitespace', () => {
    expect(captureViewportCentreAnchor(
      [
        { index: 0, left: 0, top: 0, width: 100, height: 100, columnIndex: 0, rowIndex: 0 },
        { index: 1, left: 0, top: 200, width: 100, height: 100, columnIndex: 0, rowIndex: 1 },
      ],
      0,
      110,
      { width: 100, height: 100 },
    )).toEqual({
      pageIndex: 1,
      ratioX: 0.5,
      ratioY: 0,
    });
  });

  it('does not resolve a viewport centre anchor when its page is missing', () => {
    expect(resolveViewportCentreAnchorScroll(
      { pageIndex: 7, ratioX: 0.5, ratioY: 0.5 },
      [{ index: 1, left: 0, top: 0, width: 100, height: 100, columnIndex: 0, rowIndex: 0 }],
      100,
      100,
      200,
      200,
    )).toBeNull();
  });

  it('clamps a resolved viewport centre anchor at scroll bounds', () => {
    expect(resolveViewportCentreAnchorScroll(
      { pageIndex: 1, ratioX: 0.5, ratioY: 0.5 },
      [{ index: 1, left: 100, top: 100, width: 200, height: 200, columnIndex: 0, rowIndex: 0 }],
      500,
      500,
      600,
      600,
    )).toEqual({
      left: 0,
      top: 0,
      contentOffset: {
        x: 50,
        y: 50,
      },
    });
  });

  it('centres the current page when entering column view', () => {
    expect(resolvePageCenteredScroll(
      { left: 600, top: 240, width: 120, height: 160 },
      300,
      200,
      1000,
      700,
    )).toEqual({ left: 510, top: 220 });
  });

  it('clamps centred column transition scroll to available bounds', () => {
    expect(resolvePageCenteredScroll(
      { left: 20, top: 10, width: 80, height: 100 },
      300,
      200,
      500,
      260,
    )).toEqual({ left: 0, top: 0 });
  });

  it('preserves the viewport anchor when entering columns near the top scroll bound', () => {
    expect(resolveAnchoredLayoutTransitionScroll(
      {
        layoutMode: 'continuous',
        pagesPerColumn: 10,
        zoom: 0.8,
        layouts: [
          { index: 0, left: 100, top: 20, width: 400, height: 520, columnIndex: 0, rowIndex: 0 },
          { index: 1, left: 100, top: 560, width: 400, height: 520, columnIndex: 0, rowIndex: 1 },
          { index: 2, left: 100, top: 1100, width: 400, height: 520, columnIndex: 0, rowIndex: 2 },
        ],
        effectiveScrollLeft: 0,
        effectiveScrollTop: 900,
        viewportSize: { width: 800, height: 600 },
      },
      [
        { index: 0, left: 20, top: 20, width: 400, height: 520, columnIndex: 0, rowIndex: 0 },
        { index: 1, left: 20, top: 560, width: 400, height: 520, columnIndex: 0, rowIndex: 1 },
        { index: 2, left: 460, top: 20, width: 400, height: 520, columnIndex: 1, rowIndex: 0 },
      ],
      800,
      600,
      900,
      1120,
    )).toEqual({
      left: 100,
      top: 0,
      contentOffset: {
        x: -260,
        y: 180,
      },
    });
  });

  it('preserves the viewport anchor when leaving columns', () => {
    expect(resolveAnchoredLayoutTransitionScroll(
      {
        layoutMode: 'columns',
        pagesPerColumn: 2,
        zoom: 0.8,
        layouts: [
          { index: 0, left: 20, top: 20, width: 400, height: 520, columnIndex: 0, rowIndex: 0 },
          { index: 1, left: 20, top: 560, width: 400, height: 520, columnIndex: 0, rowIndex: 1 },
          { index: 2, left: 460, top: 20, width: 400, height: 520, columnIndex: 1, rowIndex: 0 },
        ],
        effectiveScrollLeft: 100,
        effectiveScrollTop: 0,
        viewportSize: { width: 800, height: 600 },
      },
      [
        { index: 0, left: 100, top: 20, width: 400, height: 520, columnIndex: 0, rowIndex: 0 },
        { index: 1, left: 100, top: 560, width: 400, height: 520, columnIndex: 0, rowIndex: 1 },
        { index: 2, left: 100, top: 1100, width: 400, height: 520, columnIndex: 0, rowIndex: 2 },
      ],
      800,
      600,
      900,
      1700,
    )).toEqual({
      left: 0,
      top: 1080,
      contentOffset: {
        x: 260,
        y: 0,
      },
    });
  });

  it('preserves the normalised page anchor through column zoom layout changes', () => {
    expect(resolveAnchoredLayoutTransitionScroll(
      {
        layoutMode: 'columns',
        pagesPerColumn: 2,
        zoom: 0.8,
        layouts: [
          { index: 2, left: 460, top: 20, width: 400, height: 520, columnIndex: 1, rowIndex: 0 },
        ],
        effectiveScrollLeft: 100,
        effectiveScrollTop: 0,
        viewportSize: { width: 800, height: 600 },
      },
      [
        { index: 2, left: 520, top: 20, width: 500, height: 650, columnIndex: 1, rowIndex: 0 },
      ],
      800,
      600,
      1100,
      700,
    )).toEqual({
      left: 170,
      top: 70,
      contentOffset: {
        x: 0,
        y: 0,
      },
    });
  });

  it('lets column wheel zoom keep the cursor anchor instead of applying the centre anchor', () => {
    expect(shouldPreserveColumnAnchorAfterLayoutChange({
      previousLayoutMode: 'columns',
      nextLayoutMode: 'columns',
      previousPagesPerColumn: 2,
      nextPagesPerColumn: 2,
      previousZoom: 0.8,
      nextZoom: 1,
      didHandleColumnWheelZoom: true,
    })).toBe(false);
  });

  it('still preserves the column anchor for non-wheel column zoom changes', () => {
    expect(shouldPreserveColumnAnchorAfterLayoutChange({
      previousLayoutMode: 'columns',
      nextLayoutMode: 'columns',
      previousPagesPerColumn: 2,
      nextPagesPerColumn: 2,
      previousZoom: 0.8,
      nextZoom: 1,
    })).toBe(true);
  });

  it('preserves the viewport anchor for normal view and fit transitions', () => {
    expect(shouldPreserveViewportAnchorAfterLayoutChange({
      previousLayoutMode: 'continuous',
      nextLayoutMode: 'single-page',
      previousPagesPerColumn: 2,
      nextPagesPerColumn: 2,
      previousZoom: 1,
      nextZoom: 1,
    })).toBe(true);

    expect(shouldPreserveViewportAnchorAfterLayoutChange({
      previousLayoutMode: 'continuous',
      nextLayoutMode: 'continuous',
      previousPagesPerColumn: 2,
      nextPagesPerColumn: 2,
      previousZoom: 1,
      nextZoom: 0.8,
    })).toBe(true);
  });
});

describe('document viewport current page tracking', () => {
  it('auto-tracks the visible page in normal continuous view only', () => {
    expect(shouldAutoUpdateCurrentPageFromViewport('continuous')).toBe(true);
    expect(shouldAutoUpdateCurrentPageFromViewport('columns')).toBe(false);
    expect(shouldAutoUpdateCurrentPageFromViewport('single-page')).toBe(false);
  });

  it('uses the page with the majority of the viewport while scrolling down in continuous view', () => {
    const layouts = [
      { index: 0, left: 0, top: 0, width: 100, height: 100, columnIndex: 0, rowIndex: 0 },
      { index: 1, left: 0, top: 120, width: 100, height: 100, columnIndex: 0, rowIndex: 1 },
    ];

    expect(getContinuousCurrentPageLayout(
      layouts,
      0,
      80,
      90,
      { width: 100, height: 80 },
      'down',
    )?.index).toBe(1);

    expect(getContinuousCurrentPageLayout(
      layouts,
      0,
      70,
      80,
      { width: 100, height: 80 },
      'down',
    )).toBeNull();
  });

  it('uses the page with the majority of the viewport while scrolling up in continuous view', () => {
    const layouts = [
      { index: 0, left: 0, top: 0, width: 100, height: 100, columnIndex: 0, rowIndex: 0 },
      { index: 1, left: 0, top: 120, width: 100, height: 100, columnIndex: 0, rowIndex: 1 },
      { index: 2, left: 0, top: 240, width: 100, height: 100, columnIndex: 0, rowIndex: 2 },
    ];

    expect(getContinuousCurrentPageLayout(
      layouts,
      0,
      150,
      125,
      { width: 100, height: 80 },
      'up',
    )?.index).toBe(1);

    expect(getContinuousCurrentPageLayout(
      layouts,
      0,
      150,
      210,
      { width: 100, height: 80 },
      'up',
    )?.index).toBe(2);

    expect(getContinuousCurrentPageLayout(
      layouts,
      0,
      270,
      165,
      { width: 100, height: 80 },
      'up',
    )?.index).toBe(1);
  });
});
