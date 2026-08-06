import { afterEach, describe, expect, it } from 'vitest';
import {
  DEFAULT_LEFT_SIDEBAR_WIDTH,
  DEFAULT_RIGHT_SIDEBAR_WIDTH,
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
  it('starts both sidebars at 300 px', () => {
    expect(DEFAULT_LEFT_SIDEBAR_WIDTH).toBe(300);
    expect(DEFAULT_RIGHT_SIDEBAR_WIDTH).toBe(300);
    expect(useViewerStore.getState().leftSidebarWidth).toBe(300);
    expect(useViewerStore.getState().rightSidebarWidth).toBe(300);
  });

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

    store.toggleLeftSidebar('pages');
    expect(useViewerStore.getState().leftSidebarPanel).toBe('pages');
    expect(useViewerStore.getState().leftSidebarOpen).toBe(false);

    store.toggleLeftSidebar();
    store.toggleRightSidebar();
    expect(useViewerStore.getState().leftSidebarOpen).toBe(true);
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

  it('shares post-placement direct manipulation across pages until selection or tool changes', () => {
    const store = useViewerStore.getState();
    store.setActiveTool('rectangle');
    store.setSelectedMarkupIds(['rect-1']);
    store.setPostPlacement({ markupId: 'rect-1', tool: 'rectangle' });

    expect(useViewerStore.getState().postPlacement).toEqual({ markupId: 'rect-1', tool: 'rectangle' });
    store.setSelectedMarkupIds(['rect-1']);
    expect(useViewerStore.getState().postPlacement).toEqual({ markupId: 'rect-1', tool: 'rectangle' });
    store.setSelectedMarkupIds([]);
    expect(useViewerStore.getState().postPlacement).toBeNull();

    store.setSelectedMarkupIds(['rect-2']);
    store.setPostPlacement({ markupId: 'rect-2', tool: 'rectangle' });
    store.setActiveTool('line');
    expect(useViewerStore.getState().postPlacement).toBeNull();
  });

  it('resets selection state and returns to the selection tool', () => {
    const store = useViewerStore.getState();
    store.setActiveTool('rectangle');
    store.setSelectedMarkupIds(['rect-1', 'line-1']);
    store.setPostPlacement({ markupId: 'rect-1', tool: 'rectangle' });

    store.resetToSelectionTool();

    expect(useViewerStore.getState()).toMatchObject({
      activeTool: 'select',
      selectedMarkupIds: [],
      postPlacement: null,
    });
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

    store.setContinuousScrollWheelMode('zoom');
    store.setSinglePageScrollWheelMode('scroll');
    expect(useViewerStore.getState().continuousScrollWheelMode).toBe('zoom');
    expect(useViewerStore.getState().singlePageScrollWheelMode).toBe('scroll');
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
      documentAccess: { handle: `pdfdoc_${'b'.repeat(32)}` },
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
    store.setPageColumnsEnabled(true);
    store.setPagesPerColumn(5);

    store.setDocument({
      filePath: '/tmp/example.pdf',
      fileName: 'example.pdf',
      documentAccess: { handle: `pdfdoc_${'c'.repeat(32)}` },
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
    expect(useViewerStore.getState().pageColumnsEnabled).toBe(false);
    expect(useViewerStore.getState().cadViewOrganisation).toBe('columns');
    expect(useViewerStore.getState().pagesPerColumn).toBe(10);
  });
});
