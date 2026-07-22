import { afterEach, describe, expect, it } from 'vitest';
import {
  MAX_LEFT_SIDEBAR_WIDTH,
  MAX_RIGHT_SIDEBAR_WIDTH,
  MIN_LEFT_SIDEBAR_WIDTH,
  MIN_RIGHT_SIDEBAR_WIDTH,
  resetViewerStore,
  useViewerStore,
} from './viewerStore';

afterEach(() => {
  resetViewerStore();
});

describe('viewer store', () => {
  it('updates light UI state without heavy document data', () => {
    useViewerStore.getState().setZoom(1.5);
    useViewerStore.getState().setActiveTool('rectangle');
    useViewerStore.getState().setCurrentPage(3);
    useViewerStore.getState().setVisiblePageIndices([2, 3]);

    expect(useViewerStore.getState().zoom).toBe(1.5);
    expect(useViewerStore.getState().activeTool).toBe('rectangle');
    expect(useViewerStore.getState().currentPage).toBe(3);
    expect(useViewerStore.getState().visiblePageIndices).toEqual([2, 3]);
  });

  it('tracks sidebar toggles and page scroll requests', () => {
    const store = useViewerStore.getState();

    store.openLeftSidebar('pages');
    store.openRightSidebar('tools');
    expect(useViewerStore.getState().leftSidebarOpen).toBe(true);
    expect(useViewerStore.getState().rightSidebarOpen).toBe(true);

    store.toggleLeftSidebar();
    store.toggleRightSidebar();
    expect(useViewerStore.getState().leftSidebarOpen).toBe(false);
    expect(useViewerStore.getState().rightSidebarOpen).toBe(false);

    const requestId = store.requestPageScroll(7);
    expect(useViewerStore.getState().pendingPageScroll).toEqual({
      pageIndex: 7,
      requestId,
    });

    store.consumePageScroll(requestId);
    expect(useViewerStore.getState().pendingPageScroll).toBeNull();

    const thumbnailRequestId = store.requestThumbnailScroll(4);
    expect(useViewerStore.getState().pendingThumbnailScroll).toEqual({
      pageIndex: 4,
      requestId: thumbnailRequestId,
    });

    store.consumeThumbnailScroll(thumbnailRequestId);
    expect(useViewerStore.getState().pendingThumbnailScroll).toBeNull();
  });

  it('tracks a picked image until it is placed', () => {
    const asset = {
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png' as const,
      width: 320,
      height: 180,
      fileName: 'site-photo.png',
    };

    useViewerStore.getState().setPendingImageAsset(asset);
    expect(useViewerStore.getState().pendingImageAsset).toEqual(asset);
    expect(useViewerStore.getState().consumePendingImageAsset()).toEqual(asset);
    expect(useViewerStore.getState().pendingImageAsset).toBeNull();
  });

  it('tracks snap source toggles and clamps sensitivity', () => {
    const store = useViewerStore.getState();

    expect(store.snapSettings).toEqual({
      snapToContent: true,
      snapToMarkup: true,
      sensitivityPx: 8,
      snapTargets: ['endpoint', 'midpoint', 'center', 'intersection'],
    });

    store.setSnapSettings({ snapToContent: false, sensitivityPx: 99, snapTargets: ['nearest', 'midpoint', 'nearest'] });
    expect(useViewerStore.getState().snapSettings).toEqual({
      snapToContent: false,
      snapToMarkup: true,
      sensitivityPx: 24,
      snapTargets: ['nearest', 'midpoint'],
    });

    store.setSnapSettings({ snapToMarkup: false, sensitivityPx: 1, snapTargets: [] });
    expect(useViewerStore.getState().snapSettings).toEqual({
      snapToContent: false,
      snapToMarkup: false,
      sensitivityPx: 2,
      snapTargets: ['endpoint', 'midpoint', 'center', 'intersection'],
    });
  });

  it('tracks page column layout settings and clamps pages per column', () => {
    const store = useViewerStore.getState();

    expect(store.pageColumnsEnabled).toBe(false);
    expect(store.cadViewOrganisation).toBe('columns');
    expect(store.pagesPerColumn).toBe(10);
    expect(store.continuousScrollWheelMode).toBe('scroll');
    expect(store.singlePageScrollWheelMode).toBe('zoom');
    expect(store.cadScrollWheelMode).toBe('zoom');

    store.setContinuousScrollWheelMode('zoom');
    store.setSinglePageScrollWheelMode('scroll');
    store.setCadScrollWheelMode('scroll');
    expect(useViewerStore.getState().continuousScrollWheelMode).toBe('zoom');
    expect(useViewerStore.getState().singlePageScrollWheelMode).toBe('scroll');
    expect(useViewerStore.getState().cadScrollWheelMode).toBe('scroll');
    store.setZoomPreset('fit-width');
    store.setPageColumnsEnabled(true);
    store.setCadViewOrganisation('rows');
    store.setPagesPerColumn(0);
    expect(useViewerStore.getState().pageColumnsEnabled).toBe(true);
    expect(useViewerStore.getState().zoomPreset).toBe('manual');
    expect(useViewerStore.getState().cadViewOrganisation).toBe('rows');
    expect(useViewerStore.getState().pagesPerColumn).toBe(1);

    store.setPagesPerColumn(500);
    expect(useViewerStore.getState().pagesPerColumn).toBe(100);

    store.setPagesPerColumn(Number.NaN);
    expect(useViewerStore.getState().pagesPerColumn).toBe(10);
  });

  it('clamps sidebar widths and preserves them across document loads', () => {
    const store = useViewerStore.getState();
    store.setLeftSidebarWidth(MIN_LEFT_SIDEBAR_WIDTH - 40);
    store.setRightSidebarWidth(MAX_RIGHT_SIDEBAR_WIDTH + 40);

    expect(useViewerStore.getState().leftSidebarWidth).toBe(MIN_LEFT_SIDEBAR_WIDTH);
    expect(useViewerStore.getState().rightSidebarWidth).toBe(MAX_RIGHT_SIDEBAR_WIDTH);

    store.setLeftSidebarWidth(MAX_LEFT_SIDEBAR_WIDTH);
    store.setRightSidebarWidth(MIN_RIGHT_SIDEBAR_WIDTH);
    store.setDocument({
      filePath: '/tmp/example.pdf',
      fileName: 'example.pdf',
      document: {
        pages: [],
        markups: [],
      } as never,
    });

    expect(useViewerStore.getState().leftSidebarWidth).toBe(MAX_LEFT_SIDEBAR_WIDTH);
    expect(useViewerStore.getState().rightSidebarWidth).toBe(MIN_RIGHT_SIDEBAR_WIDTH);
  });

  it('resets sidebar and page-scroll state when a document loads', () => {
    const store = useViewerStore.getState();
    store.openLeftSidebar('pages');
    store.openRightSidebar('tools');
    store.requestPageScroll(3);
    store.setZoomPreset('fit-width');
    store.setScrollMode('single-page');
    store.setContinuousScrollWheelMode('zoom');
    store.setSinglePageScrollWheelMode('scroll');
    store.setCadScrollWheelMode('scroll');
    store.setPageColumnsEnabled(true);
    store.setPagesPerColumn(5);

    store.setDocument({
      filePath: '/tmp/example.pdf',
      fileName: 'example.pdf',
      document: {
        pages: [],
        markups: [],
      } as never,
    });

    expect(useViewerStore.getState().leftSidebarOpen).toBe(false);
    expect(useViewerStore.getState().rightSidebarOpen).toBe(false);
    expect(useViewerStore.getState().pendingPageScroll).toBeNull();
    expect(useViewerStore.getState().zoomPreset).toBe('manual');
    expect(useViewerStore.getState().scrollMode).toBe('continuous');
    expect(useViewerStore.getState().continuousScrollWheelMode).toBe('scroll');
    expect(useViewerStore.getState().singlePageScrollWheelMode).toBe('zoom');
    expect(useViewerStore.getState().cadScrollWheelMode).toBe('zoom');
    expect(useViewerStore.getState().pageColumnsEnabled).toBe(false);
    expect(useViewerStore.getState().cadViewOrganisation).toBe('columns');
    expect(useViewerStore.getState().pagesPerColumn).toBe(10);
  });
});
