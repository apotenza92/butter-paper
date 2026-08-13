import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Alert, AlertAction, AlertDescription } from '@/components/ui/alert';
import { Spinner } from '@/components/ui/spinner';
import type { PointerEvent as ReactPointerEvent } from 'react';
import type { PageModel, PdfPoint, Rect } from '@butter-paper/core';
import type { PdfOpenProgress, ToolMode, ZoomPreset } from '../../../shared/protocol';
import { isRenderBacklogIdle, type DiagnosticsSnapshot, type LocalPdfSession } from '../services/documentSession';
import { useRenderCoordinator } from '../services/renderCoordinator';
import { useSessionVersion } from '../services/sessionHooks';
import { useAdaptivePerformance } from '../services/useAdaptivePerformance';
import { useViewerStore, type ScrollWheelMode } from '../state/viewerStore';
import { buildPageLayouts, computeVisibleLayoutPositions } from '../utils/virtualisation';
import type { PageLayout } from '../utils/virtualisation';
import { clampViewerZoom, computePreviewRasterZoom, MIN_VIEWER_ZOOM, quantizeFitZoomDown } from '../utils/renderZoom';
import { resolveAnchoredZoomAxis } from '../utils/viewportZoom';
import {
  EMPTY_CANVAS_PADDING,
  areCanvasPaddingsEqual,
  getEffectiveCanvasPadding,
} from '../utils/canvasPadding';
import type { CanvasPadding } from '../utils/canvasPadding';
import { resolveSinglePageWheelNavigation } from '../utils/singlePageWheelNavigation';
import { resolveDocumentKeyboardNavigation, type DocumentKeyboardAction } from '../utils/documentKeyboardNavigation';
import { isInteractiveShortcutTarget } from '../utils/toolShortcuts';
import { resolveAdaptiveMotionOverscanPx, shouldAllowAdaptivePrefetch, type AdaptivePerformanceLevel } from '../utils/adaptivePerformance';
import { recordComponentRender, recordEvent, recordOverviewFocusPreviewQuality, recordOverviewVisiblePreviewFill } from '../services/perfTracker';
import { CustomScrollArea } from './CustomScrollArea';
import { PageView, shouldRetryBrokenPageImageSource } from './PageView';

const DOCUMENT_VISIBLE_OVERSCAN_PX = 1200;
const DOCUMENT_MOTION_OVERSCAN_PX = 1200;
const COLUMN_VISIBLE_OVERSCAN_PX = 320;
const COLUMN_OVERVIEW_ZOOM_THRESHOLD = 0.3;
const COLUMN_OVERVIEW_ZOOM_EXIT_THRESHOLD = 0.42;
const COLUMN_MOTION_OVERVIEW_ZOOM_THRESHOLD = 1.2;
const COLUMN_OVERVIEW_MOUNTED_PAGE_THRESHOLD = 96;
const COLUMN_OVERVIEW_MOUNTED_PAGE_EXIT_THRESHOLD = 72;
const PAGE_LAYOUT_GAP = 24;
const COLUMN_OVERVIEW_PREVIEW_MAX_WIDTH_PX = 32;
const COLUMN_OVERVIEW_PREVIEW_MAX_HEIGHT_PX = 42;
const COLUMN_OVERVIEW_PREVIEW_MIN_SCALE = 0.025;
const COLUMN_OVERVIEW_PREVIEW_MIN_RENDER_WIDTH_PX = 48;
const COLUMN_OVERVIEW_VISIBLE_PREVIEW_RENDER_SCALE = 1.35;
const COLUMN_OVERVIEW_FOCUS_PREVIEW_RENDER_SCALE = 1.7;
const COLUMN_OVERVIEW_POINTER_PREVIEW_RENDER_SCALE = 2;
const COLUMN_OVERVIEW_PREVIEW_MAX_RENDER_WIDTH_PX = 512;
const COLUMN_CANVAS_OVERVIEW_VISIBLE_PAGE_THRESHOLD = 128;
const COLUMN_CANVAS_OVERVIEW_VISIBLE_PREVIEW_BATCH_SIZE = 64;
const COLUMN_CANVAS_OVERVIEW_PREFETCH_PREVIEW_BATCH_SIZE = 12;
const COLUMN_CANVAS_OVERVIEW_PREVIEW_BATCH_DELAY_MS = 120;
const FIT_VIEWPORT_SETTLE_MS = 140;
const VIEWPORT_MOTION_SETTLE_MS = 180;
const RAPID_VIEWPORT_ENTER_PX_PER_MS = 1.5;
const RAPID_VIEWPORT_EXIT_PX_PER_MS = 0.75;
const CONTINUOUS_CURRENT_PAGE_SCROLL_SUPPRESSION_MS = 500;
const BLUEBEAM_TRACKPAD_ZOOM_RATE = 0.00165;
const WHEEL_ZOOM_MAX_DELTA_PER_FRAME = 120;
const MIDDLE_DOUBLE_CLICK_MS = 400;
const MIDDLE_CLICK_DRAG_TOLERANCE_PX = 4;
const MIDDLE_DOUBLE_CLICK_TOLERANCE_PX = 8;
const MINIMUM_ZOOM_PAN_VISIBLE_RATIO = 0.5;
const DOCUMENT_KEYBOARD_SCROLL_STEP_PX = 48;
export const DOCUMENT_OPENING_INDICATOR_DELAY_MS = 700;

interface ViewportPanState {
  readonly pointerId: number;
  readonly button: number;
  x: number;
  y: number;
  readonly startClientX: number;
  readonly startClientY: number;
  moved: boolean;
}

interface MiddleClickState {
  readonly timeStamp: number;
  readonly clientX: number;
  readonly clientY: number;
}

interface ViewportContentOffset {
  readonly x: number;
  readonly y: number;
}

interface PendingPanScroll {
  readonly left: number;
  readonly top: number;
}

type WheelZoomAnchor =
  | {
      readonly kind: 'page';
      readonly pageIndex: number;
      readonly pageX: number;
      readonly pageY: number;
    }
  | {
      readonly kind: 'content';
      readonly contentX: number;
      readonly contentY: number;
    };

interface PendingWheelZoomAnchor {
  readonly anchor: WheelZoomAnchor;
  readonly localX: number;
  readonly localY: number;
  readonly zoom: number;
  readonly zoomRatio: number;
}

export interface LayoutTransitionSnapshot {
  readonly layoutMode: 'continuous' | 'columns' | 'single-page';
  readonly cadViewOrganisation?: 'columns' | 'rows';
  readonly pagesPerColumn: number;
  readonly zoom: number;
  readonly layouts: readonly PageLayout[];
  readonly effectiveScrollLeft: number;
  readonly effectiveScrollTop: number;
  readonly viewportSize: {
    readonly width: number;
    readonly height: number;
  };
}

export interface AnchoredLayoutTransitionScroll {
  readonly left: number;
  readonly top: number;
  readonly contentOffset: ViewportContentOffset;
}

export interface ViewportCentreAnchor {
  readonly pageIndex: number;
  readonly ratioX: number;
  readonly ratioY: number;
}

interface WheelZoomPoint {
  readonly localX: number;
  readonly localY: number;
}

type ScrollDirection = 'down' | 'up' | 'none';

interface DocumentViewportProps {
  session: LocalPdfSession | null;
  opening?: boolean;
  openingProgress?: PdfOpenProgress | null;
  onOpenDocument: () => void;
  calibrationPick?: {
    active: boolean;
    pointCount: number;
    pageIndex: number | null;
    startPoint: PdfPoint | null;
  } | null;
  onCalibrationPoint?: (pageIndex: number, point: PdfPoint) => void;
  onCancelCalibrationPick?: () => void;
}

export function DocumentViewport({
  session,
  opening = false,
  openingProgress = null,
  onOpenDocument,
  calibrationPick = null,
  onCalibrationPoint,
  onCancelCalibrationPick,
}: DocumentViewportProps) {
  recordComponentRender('DocumentViewport');
  const [showOpeningIndicator, setShowOpeningIndicator] = useState(false);
  useEffect(() => {
    if (!opening) {
      setShowOpeningIndicator(false);
      return;
    }
    const timer = window.setTimeout(() => setShowOpeningIndicator(true), DOCUMENT_OPENING_INDICATOR_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [opening]);
  const documentState = useViewerStore((state) => state.document);
  const zoom = useViewerStore((state) => state.zoom);
  const zoomPreset = useViewerStore((state) => state.zoomPreset);
  const scrollMode = useViewerStore((state) => state.scrollMode);
  const continuousScrollWheelMode = useViewerStore((state) => state.continuousScrollWheelMode);
  const singlePageScrollWheelMode = useViewerStore((state) => state.singlePageScrollWheelMode);
  const pageColumnsEnabled = useViewerStore((state) => state.pageColumnsEnabled);
  const cadViewOrganisation = useViewerStore((state) => state.cadViewOrganisation);
  const pagesPerColumn = useViewerStore((state) => state.pagesPerColumn);
  const activeTool = useViewerStore((state) => state.activeTool);
  const currentPage = useViewerStore((state) => state.currentPage);
  const leftSidebarOpen = useViewerStore((state) => state.leftSidebarOpen);
  const rightSidebarOpen = useViewerStore((state) => state.rightSidebarOpen);
  const pendingPageScroll = useViewerStore((state) => state.pendingPageScroll);
  const pendingDocumentScroll = useViewerStore((state) => state.pendingDocumentScroll);
  const setZoom = useViewerStore((state) => state.setZoom);
  const setZoomPreset = useViewerStore((state) => state.setZoomPreset);
  const setActiveTool = useViewerStore((state) => state.setActiveTool);
  const consumePageScroll = useViewerStore((state) => state.consumePageScroll);
  const consumeDocumentScroll = useViewerStore((state) => state.consumeDocumentScroll);
  const requestPageScroll = useViewerStore((state) => state.requestPageScroll);
  const requestDocumentScroll = useViewerStore((state) => state.requestDocumentScroll);
  const requestThumbnailScroll = useViewerStore((state) => state.requestThumbnailScroll);
  const setCurrentPage = useViewerStore((state) => state.setCurrentPage);
  const setVisiblePageIndices = useViewerStore((state) => state.setVisiblePageIndices);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 });
  const [fitViewportSize, setFitViewportSize] = useState({ width: 0, height: 0 });
  const [scrollTop, setScrollTop] = useState(0);
  const [scrollLeft, setScrollLeft] = useState(0);
  const [viewportContentOffset, setViewportContentOffset] = useState<ViewportContentOffset>({ x: 0, y: 0 });
  const [canvasPadding, setCanvasPadding] = useState<CanvasPadding>(EMPTY_CANVAS_PADDING);
  const [startupPageRenderReady, setStartupPageRenderReady] = useState(false);
  const [isPanning, setIsPanning] = useState(false);
  const [viewportInMotion, setViewportInMotion] = useState(false);
  const [rapidViewportMotion, setRapidViewportMotion] = useState(false);
  const [columnOverviewModeActive, setColumnOverviewModeActive] = useState(false);
  const [pointerPageIndex, setPointerPageIndex] = useState<number | null>(null);
  const panState = useRef<ViewportPanState | null>(null);
  const lastMiddleClickRef = useRef<MiddleClickState | null>(null);
  const middlePanRestoreToolRef = useRef<ToolMode | null>(null);
  const activeToolRef = useRef<ToolMode>(activeTool);
  const resizeFrameRef = useRef<number | null>(null);
  const fitViewportTimeoutRef = useRef<number | null>(null);
  const scrollFrameRef = useRef<number | null>(null);
  const viewportMotionTimeoutRef = useRef<number | null>(null);
  const continuousCurrentPageScrollSuppressionTimeoutRef = useRef<number | null>(null);
  const sessionRef = useRef<LocalPdfSession | null>(session);
  const pendingViewportSizeRef = useRef({ width: 0, height: 0 });
  const pendingScrollTopRef = useRef(0);
  const pendingScrollLeftRef = useRef(0);
  const lastObservedScrollTopRef = useRef(0);
  const lastObservedScrollLeftRef = useRef(0);
  const lastObservedScrollAtRef = useRef<number | null>(null);
  const lastWheelMotionAtRef = useRef<number | null>(null);
  const previousContinuousCurrentPageScrollTopRef = useRef(0);
  const scrollDirectionRef = useRef<ScrollDirection>('none');
  const lastScrollWasUserScrollRef = useRef(true);
  const suppressContinuousCurrentPageScrollRef = useRef(false);
  const layoutTransitionSnapshotRef = useRef<LayoutTransitionSnapshot | null>(null);
  const viewportContentOffsetRef = useRef<ViewportContentOffset>({ x: 0, y: 0 });
  const wheelZoomTargetRef = useRef<number | null>(null);
  const pendingWheelZoomAnchorRef = useRef<PendingWheelZoomAnchor | null>(null);
  const fitZoomReferencePageRef = useRef<number | null>(null);
  const wheelZoomHandledColumnAnchorRef = useRef(false);
  const wheelZoomFrameRef = useRef<number | null>(null);
  const wheelZoomDeltaYRef = useRef(0);
  const latestWheelZoomPointRef = useRef<WheelZoomPoint | null>(null);
  const singlePageWheelDeltaRef = useRef(0);
  const canvasPaddingRef = useRef<CanvasPadding>(EMPTY_CANVAS_PADDING);
  const pendingPanScrollRef = useRef<PendingPanScroll | null>(null);
  const previousZoomRef = useRef(zoom);
  const viewportInMotionRef = useRef(false);
  const rapidViewportMotionRef = useRef(false);
  const previousPageColumnsEnabledRef = useRef(pageColumnsEnabled);
  const layoutModeRef = useRef<'continuous' | 'columns' | 'single-page'>('continuous');
  const pageLayoutListRef = useRef<readonly PageLayout[]>([]);
  const hasDocumentView = Boolean(documentState && session);
  const sessionVersion = useSessionVersion(session);
  const renderCoordinator = useRenderCoordinator(session);
  const adaptivePerformance = useAdaptivePerformance(session, hasDocumentView);

  const resetViewportContentOffset = () => {
    viewportContentOffsetRef.current = { x: 0, y: 0 };
    setViewportContentOffset((current) => (
      current.x === 0 && current.y === 0 ? current : { x: 0, y: 0 }
    ));
  };

  const updateCanvasPadding = (nextPadding: CanvasPadding) => {
    const currentPadding = canvasPaddingRef.current;
    if (areCanvasPaddingsEqual(currentPadding, nextPadding)) {
      return;
    }

    canvasPaddingRef.current = nextPadding;
    setCanvasPadding(nextPadding);
  };

  const resetCanvasPadding = () => {
    updateCanvasPadding(EMPTY_CANVAS_PADDING);
  };

  useEffect(() => {
    sessionRef.current = session;
  }, [session]);

  useEffect(() => {
    activeToolRef.current = activeTool;
  }, [activeTool]);

  useEffect(() => {
    setStartupPageRenderReady(false);
    resetViewportContentOffset();
    resetCanvasPadding();
  }, [documentState?.filePath, session]);

  useEffect(() => {
    if (zoomPreset !== 'manual') {
      resetViewportContentOffset();
      resetCanvasPadding();
    } else {
      fitZoomReferencePageRef.current = null;
    }
  }, [zoomPreset]);

  useEffect(() => {
    if (Math.abs(previousZoomRef.current - zoom) <= 0.001) {
      return;
    }

    previousZoomRef.current = zoom;
    const wheelZoomTarget = wheelZoomTargetRef.current;
    wheelZoomTargetRef.current = null;
    if (wheelZoomTarget !== null && Math.abs(wheelZoomTarget - zoom) <= 0.001) {
      return;
    }

    resetViewportContentOffset();
  }, [zoom]);

  const markViewportInMotion = (rapid = rapidViewportMotionRef.current) => {
    const activeSession = sessionRef.current;
    if (!activeSession) {
      return;
    }

    activeSession.setViewportInMotion(true);
    if (!viewportInMotionRef.current) {
      viewportInMotionRef.current = true;
      setViewportInMotion(true);
    }
    if (rapidViewportMotionRef.current !== rapid) {
      rapidViewportMotionRef.current = rapid;
      setRapidViewportMotion(rapid);
    }
    if (viewportMotionTimeoutRef.current !== null) {
      window.clearTimeout(viewportMotionTimeoutRef.current);
    }
    viewportMotionTimeoutRef.current = window.setTimeout(() => {
      viewportMotionTimeoutRef.current = null;
      sessionRef.current?.setViewportInMotion(false);
      viewportInMotionRef.current = false;
      rapidViewportMotionRef.current = false;
      setViewportInMotion(false);
      setRapidViewportMotion(false);
    }, VIEWPORT_MOTION_SETTLE_MS);
  };

  const suppressContinuousCurrentPageFromScroll = () => {
    suppressContinuousCurrentPageScrollRef.current = true;
    lastScrollWasUserScrollRef.current = false;
    if (continuousCurrentPageScrollSuppressionTimeoutRef.current !== null) {
      window.clearTimeout(continuousCurrentPageScrollSuppressionTimeoutRef.current);
    }
    continuousCurrentPageScrollSuppressionTimeoutRef.current = window.setTimeout(() => {
      continuousCurrentPageScrollSuppressionTimeoutRef.current = null;
      suppressContinuousCurrentPageScrollRef.current = false;
    }, CONTINUOUS_CURRENT_PAGE_SCROLL_SUPPRESSION_MS);
  };

  useLayoutEffect(() => {
    layoutTransitionSnapshotRef.current = null;
    pendingPanScrollRef.current = null;
    pendingWheelZoomAnchorRef.current = null;
    wheelZoomTargetRef.current = null;
    singlePageWheelDeltaRef.current = 0;
    scrollDirectionRef.current = 'none';
    lastObservedScrollTopRef.current = 0;

    const container = containerRef.current;
    if (!container) {
      return;
    }

    suppressContinuousCurrentPageFromScroll();
    resetViewportContentOffset();
    resetCanvasPadding();
    container.scrollTo({ left: 0, top: 0, behavior: 'auto' });
    pendingScrollLeftRef.current = 0;
    pendingScrollTopRef.current = 0;
    setScrollLeft(0);
    setScrollTop(0);
  }, [documentState?.filePath, session]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const observer = new ResizeObserver((entries) => {
      recordEvent('DocumentViewport.resizeObserver');
      const entry = entries[0];
      if (!entry) {
        return;
      }

      pendingViewportSizeRef.current = {
        width: Math.round(entry.contentRect.width),
        height: Math.round(entry.contentRect.height),
      };

      if (resizeFrameRef.current !== null) {
        return;
      }

      resizeFrameRef.current = window.requestAnimationFrame(() => {
        resizeFrameRef.current = null;
        const nextSize = pendingViewportSizeRef.current;
        setViewportSize((current) => (
          current.width === nextSize.width && current.height === nextSize.height ? current : nextSize
        ));
      });
    });

    observer.observe(container);
    return () => {
      observer.disconnect();
      if (resizeFrameRef.current !== null) {
        window.cancelAnimationFrame(resizeFrameRef.current);
        resizeFrameRef.current = null;
      }
    };
  }, [hasDocumentView]);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    // Sidebar visibility changes are discrete shell changes, so synchronise the
    // fitted size before paint instead of waiting for the resize-drag debounce.
    const nextSize = measureViewportSize(container);
    pendingViewportSizeRef.current = nextSize;
    setViewportSize((current) => (
      current.width === nextSize.width && current.height === nextSize.height ? current : nextSize
    ));
    setFitViewportSize((current) => (
      current.width === nextSize.width && current.height === nextSize.height ? current : nextSize
    ));
  }, [hasDocumentView, leftSidebarOpen, rightSidebarOpen]);

  const resolvePanScrollAndPadding = (
    container: HTMLDivElement,
    activePanState: ViewportPanState,
    event: PointerEvent,
  ): PendingPanScroll => {
    let nextLeft = activePanState.x - event.clientX;
    let nextTop = activePanState.y - event.clientY;
    const maxScrollLeft = Math.max(0, container.scrollWidth - container.clientWidth);
    const maxScrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
    const currentPadding = canvasPaddingRef.current;
    let nextPadding = currentPadding;
    const currentLayoutMode = layoutModeRef.current;
    const currentZoom = useViewerStore.getState().zoom;
    const horizontalBounds = resolveViewportPanBounds(
      pageLayoutListRef.current,
      container.clientWidth,
      maxScrollLeft,
      currentZoom,
      'x',
      currentLayoutMode,
    );
    const verticalBounds = resolveViewportPanBounds(
      pageLayoutListRef.current,
      container.clientHeight,
      maxScrollTop,
      currentZoom,
      'y',
      currentLayoutMode,
    );

    if (horizontalBounds) {
      const requestedLeft = nextLeft;
      nextLeft = clamp(requestedLeft, horizontalBounds.min, horizontalBounds.max);
      if (nextLeft < 0) {
        const expansion = Math.max(container.clientWidth, Math.ceil(-nextLeft));
        nextPadding = {
          ...nextPadding,
          left: nextPadding.left + expansion,
        };
        activePanState.x += expansion;
        nextLeft += expansion;
      } else if (nextLeft > maxScrollLeft) {
        const expansion = Math.max(container.clientWidth, Math.ceil(nextLeft - maxScrollLeft));
        nextPadding = {
          ...nextPadding,
          right: nextPadding.right + expansion,
        };
      }
      if (requestedLeft !== nextLeft) {
        activePanState.x = nextLeft + event.clientX;
      }
    } else if (nextLeft < 0) {
      const expansion = Math.max(container.clientWidth, Math.ceil(-nextLeft));
      nextPadding = {
        ...nextPadding,
        left: nextPadding.left + expansion,
      };
      activePanState.x += expansion;
      nextLeft += expansion;
    } else if (nextLeft > maxScrollLeft) {
      const expansion = Math.max(container.clientWidth, Math.ceil(nextLeft - maxScrollLeft));
      nextPadding = {
        ...nextPadding,
        right: nextPadding.right + expansion,
      };
    }

    if (verticalBounds) {
      const requestedTop = nextTop;
      nextTop = clamp(requestedTop, verticalBounds.min, verticalBounds.max);
      if (nextTop < 0) {
        const expansion = Math.max(container.clientHeight, Math.ceil(-nextTop));
        nextPadding = {
          ...nextPadding,
          top: nextPadding.top + expansion,
        };
        activePanState.y += expansion;
        nextTop += expansion;
      } else if (nextTop > maxScrollTop) {
        const expansion = Math.max(container.clientHeight, Math.ceil(nextTop - maxScrollTop));
        nextPadding = {
          ...nextPadding,
          bottom: nextPadding.bottom + expansion,
        };
      }
      if (requestedTop !== nextTop) {
        activePanState.y = nextTop + event.clientY;
      }
    } else if (nextTop < 0) {
      const expansion = Math.max(container.clientHeight, Math.ceil(-nextTop));
      nextPadding = {
        ...nextPadding,
        top: nextPadding.top + expansion,
      };
      activePanState.y += expansion;
      nextTop += expansion;
    } else if (nextTop > maxScrollTop) {
      const expansion = Math.max(container.clientHeight, Math.ceil(nextTop - maxScrollTop));
      nextPadding = {
        ...nextPadding,
        bottom: nextPadding.bottom + expansion,
      };
    }

    const nextScroll = { left: nextLeft, top: nextTop };
    if (!areCanvasPaddingsEqual(currentPadding, nextPadding)) {
      pendingPanScrollRef.current = nextScroll;
      updateCanvasPadding(nextPadding);
    }

    return nextScroll;
  };

  useEffect(() => {
    if (viewportSize.width === 0 || viewportSize.height === 0) {
      setFitViewportSize(viewportSize);
      return;
    }

    if (fitViewportSize.width === 0 || fitViewportSize.height === 0) {
      setFitViewportSize(viewportSize);
      return;
    }

    if (fitViewportTimeoutRef.current !== null) {
      window.clearTimeout(fitViewportTimeoutRef.current);
    }

    fitViewportTimeoutRef.current = window.setTimeout(() => {
      fitViewportTimeoutRef.current = null;
      setFitViewportSize((current) => (
        current.width === viewportSize.width && current.height === viewportSize.height ? current : viewportSize
      ));
    }, FIT_VIEWPORT_SETTLE_MS);

    return () => {
      if (fitViewportTimeoutRef.current !== null) {
        window.clearTimeout(fitViewportTimeoutRef.current);
        fitViewportTimeoutRef.current = null;
      }
    };
  }, [fitViewportSize.height, fitViewportSize.width, viewportSize]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const handleScroll = () => {
      recordEvent('DocumentViewport.scroll');
      const observedAt = performance.now();
      const elapsedMs = lastObservedScrollAtRef.current === null
        ? 0
        : observedAt - lastObservedScrollAtRef.current;
      const distancePx = Math.hypot(
        container.scrollLeft - lastObservedScrollLeftRef.current,
        container.scrollTop - lastObservedScrollTopRef.current,
      );
      const rapid = resolveRapidViewportMotion({
        previousRapid: rapidViewportMotionRef.current,
        distancePx,
        elapsedMs,
      });
      lastObservedScrollAtRef.current = observedAt;
      lastObservedScrollLeftRef.current = container.scrollLeft;
      markViewportInMotion(rapid);
      const isUserScrollForCurrentPage = !suppressContinuousCurrentPageScrollRef.current && !panState.current;
      if (isUserScrollForCurrentPage) {
        const nextScroll = constrainViewportScrollToPanBounds(
          pageLayoutListRef.current,
          container.clientWidth,
          container.clientHeight,
          Math.max(0, container.scrollWidth - container.clientWidth),
          Math.max(0, container.scrollHeight - container.clientHeight),
          useViewerStore.getState().zoom,
          layoutModeRef.current,
          container.scrollLeft,
          container.scrollTop,
          viewportContentOffsetRef.current,
        );
        if (nextScroll.left !== container.scrollLeft || nextScroll.top !== container.scrollTop) {
          container.scrollTo({ ...nextScroll, behavior: 'auto' });
        }
      }
      const nextObservedScrollTop = container.scrollTop;
      lastScrollWasUserScrollRef.current = isUserScrollForCurrentPage;
      if (isUserScrollForCurrentPage && nextObservedScrollTop > lastObservedScrollTopRef.current) {
        scrollDirectionRef.current = 'down';
      } else if (isUserScrollForCurrentPage && nextObservedScrollTop < lastObservedScrollTopRef.current) {
        scrollDirectionRef.current = 'up';
      }
      lastObservedScrollTopRef.current = nextObservedScrollTop;
      pendingScrollTopRef.current = container.scrollTop;
      pendingScrollLeftRef.current = container.scrollLeft;

      if (scrollFrameRef.current !== null) {
        return;
      }

      scrollFrameRef.current = window.requestAnimationFrame(() => {
        scrollFrameRef.current = null;
        const nextScrollTop = pendingScrollTopRef.current;
        const nextScrollLeft = pendingScrollLeftRef.current;
        setScrollTop((current) => (current === nextScrollTop ? current : nextScrollTop));
        setScrollLeft((current) => (current === nextScrollLeft ? current : nextScrollLeft));
      });
    };

    container.addEventListener('scroll', handleScroll, { passive: true });
    handleScroll();
    return () => {
      container.removeEventListener('scroll', handleScroll);
      if (scrollFrameRef.current !== null) {
        window.cancelAnimationFrame(scrollFrameRef.current);
        scrollFrameRef.current = null;
      }
      if (viewportMotionTimeoutRef.current !== null) {
        window.clearTimeout(viewportMotionTimeoutRef.current);
        viewportMotionTimeoutRef.current = null;
      }
      if (continuousCurrentPageScrollSuppressionTimeoutRef.current !== null) {
        window.clearTimeout(continuousCurrentPageScrollSuppressionTimeoutRef.current);
        continuousCurrentPageScrollSuppressionTimeoutRef.current = null;
      }
      sessionRef.current?.setViewportInMotion(false);
      viewportInMotionRef.current = false;
      rapidViewportMotionRef.current = false;
      suppressContinuousCurrentPageScrollRef.current = false;
      setViewportInMotion(false);
      setRapidViewportMotion(false);
    };
  }, [hasDocumentView]);

  useEffect(() => {
    if (!containerRef.current) {
      panState.current = null;
      return;
    }

    const container = containerRef.current;

    const restoreMiddlePanTool = () => {
      const restoreTool = middlePanRestoreToolRef.current;
      middlePanRestoreToolRef.current = null;
      if (restoreTool) {
        setActiveTool(restoreTool);
      }
    };

    const handlePointerDown = (event: PointerEvent) => {
      const currentTool = activeToolRef.current;
      const shouldPan = event.button === 1 || (event.button === 0 && currentTool === 'pan');
      if (!shouldPan) {
        return;
      }

      event.preventDefault();
      if (event.button === 1 && isMiddleDoubleClick(event, lastMiddleClickRef.current)) {
        lastMiddleClickRef.current = null;
        panState.current = null;
        setIsPanning(false);
        markViewportInMotion();
        if (useViewerStore.getState().pageColumnsEnabled) {
          const viewerState = useViewerStore.getState();
          const pages = documentState?.document.pages ?? [];
          const nextZoom = resolveCadOverviewEntryZoom({
            pages,
            viewportSize: pendingViewportSizeRef.current,
            pagesPerColumn: viewerState.pagesPerColumn,
            cadViewOrganisation: viewerState.cadViewOrganisation,
            gap: computePageLayoutGap(viewerState.zoom),
          });
          const nextLayoutGap = computePageLayoutGap(nextZoom);
          const pageInputs = pages.map((page) => ({
            index: page.index,
            width: page.size.width,
            height: page.size.height,
          }));
          const nextOverview = resolveCadOverviewCenteredView(
            pageInputs,
            nextZoom,
            pendingViewportSizeRef.current.width || 1,
            pendingViewportSizeRef.current.height || 1,
            nextLayoutGap,
            viewerState.cadViewOrganisation,
            viewerState.pagesPerColumn,
            viewerState.currentPage,
          );

          resetViewportContentOffset();
          updateCanvasPadding(nextOverview.canvasPadding);
          setZoomPreset('manual');
          setZoom(nextZoom);
          pendingPanScrollRef.current = nextOverview.scroll;
          container.scrollTo({ ...nextOverview.scroll, behavior: 'auto' });
          pendingScrollLeftRef.current = nextOverview.scroll.left;
          pendingScrollTopRef.current = nextOverview.scroll.top;
          setScrollLeft(nextOverview.scroll.left);
          setScrollTop(nextOverview.scroll.top);
          return;
        }
        const viewerState = useViewerStore.getState();
        const currentLayoutMode = resolveViewerLayoutMode(viewerState.scrollMode, viewerState.pageColumnsEnabled);
        const nextZoomPreset = resolveMiddleDoubleClickZoomPreset(viewerState.zoomPreset, currentLayoutMode);
        if (useViewerStore.getState().zoomPreset !== nextZoomPreset) {
          setZoomPreset(nextZoomPreset);
        }
        return;
      }

      markViewportInMotion();
      suppressContinuousCurrentPageFromScroll();
      let panStartScrollLeft = container.scrollLeft;
      let panStartScrollTop = container.scrollTop;
      if (useViewerStore.getState().zoomPreset !== 'manual') {
        if (layoutModeRef.current === 'single-page') {
          const currentLayout = pageLayoutListRef.current[0];
          if (currentLayout) {
            updateCanvasPadding(materializeSinglePagePanPadding(
              currentLayout,
              computePageLayoutGap(useViewerStore.getState().zoom),
              canvasPaddingRef.current,
            ));
          }
        } else if (layoutModeRef.current === 'continuous') {
          const currentPadding = canvasPaddingRef.current;
          const nextPadding = materializeContinuousPanPadding(
            currentPadding,
            { width: container.clientWidth, height: container.clientHeight },
          );
          panStartScrollLeft += nextPadding.left - currentPadding.left;
          panStartScrollTop += nextPadding.top - currentPadding.top;
          pendingPanScrollRef.current = {
            left: panStartScrollLeft,
            top: panStartScrollTop,
          };
          updateCanvasPadding(nextPadding);
        }
        setZoomPreset('manual');
      }
      panState.current = {
        pointerId: event.pointerId,
        button: event.button,
        x: event.clientX + panStartScrollLeft,
        y: event.clientY + panStartScrollTop,
        startClientX: event.clientX,
        startClientY: event.clientY,
        moved: false,
      };
      setIsPanning(true);
      if (event.button === 1 && currentTool !== 'pan' && middlePanRestoreToolRef.current === null) {
        middlePanRestoreToolRef.current = currentTool;
        setActiveTool('pan');
      }
      container.setPointerCapture(event.pointerId);
    };

    const finishPan = (event: PointerEvent | MouseEvent | FocusEvent, cancelled: boolean) => {
      const activePan = panState.current;
      if (!activePan) {
        return;
      }
      if ('pointerId' in event && activePan.pointerId !== event.pointerId) {
        return;
      }

      panState.current = null;
      setIsPanning(false);
      if (!cancelled) {
        markViewportInMotion();
      }
      if ('pointerId' in event && container.hasPointerCapture(event.pointerId)) {
        container.releasePointerCapture(event.pointerId);
      }
      if (activePan.button === 1 && !cancelled && event instanceof PointerEvent && !activePan.moved) {
        lastMiddleClickRef.current = {
          timeStamp: event.timeStamp,
          clientX: event.clientX,
          clientY: event.clientY,
        };
      } else if (activePan.button === 1) {
        lastMiddleClickRef.current = null;
      }
      if (activePan.button === 1) {
        restoreMiddlePanTool();
      }
    };

    const handlePointerMove = (event: PointerEvent) => {
      if (!panState.current || panState.current.pointerId !== event.pointerId) {
        return;
      }

      if (!isViewportPanButtonPressed(event.buttons, panState.current.button)) {
        finishPan(event, false);
        return;
      }

      event.preventDefault();
      markViewportInMotion();
      suppressContinuousCurrentPageFromScroll();
      if (hasExceededMiddleClickDragTolerance(event, panState.current)) {
        panState.current.moved = true;
      }
      const { left: nextLeft, top: nextTop } = resolvePanScrollAndPadding(container, panState.current, event);
      container.scrollTo({ left: nextLeft, top: nextTop, behavior: 'auto' });
    };

    const handlePointerUp = (event: PointerEvent) => {
      finishPan(event, false);
    };

    const handlePointerCancel = (event: PointerEvent) => {
      finishPan(event, true);
    };

    const handleLostPointerCapture = (event: PointerEvent) => {
      finishPan(event, true);
    };

    const handleWindowMouseUp = (event: MouseEvent) => {
      const activePan = panState.current;
      if (!activePan || event.button !== activePan.button) {
        return;
      }

      finishPan(event, false);
    };

    const handleWindowBlur = (event: FocusEvent) => {
      finishPan(event, true);
    };

    container.addEventListener('pointerdown', handlePointerDown, { capture: true });
    container.addEventListener('pointermove', handlePointerMove);
    container.addEventListener('pointerup', handlePointerUp);
    container.addEventListener('pointercancel', handlePointerCancel);
    container.addEventListener('lostpointercapture', handleLostPointerCapture);
    container.addEventListener('auxclick', preventMiddleClickDefault);
    window.addEventListener('pointerup', handlePointerUp, { capture: true });
    window.addEventListener('pointercancel', handlePointerCancel, { capture: true });
    window.addEventListener('mouseup', handleWindowMouseUp, { capture: true });
    window.addEventListener('blur', handleWindowBlur);

    return () => {
      container.removeEventListener('pointerdown', handlePointerDown, { capture: true });
      container.removeEventListener('pointermove', handlePointerMove);
      container.removeEventListener('pointerup', handlePointerUp);
      container.removeEventListener('pointercancel', handlePointerCancel);
      container.removeEventListener('lostpointercapture', handleLostPointerCapture);
      container.removeEventListener('auxclick', preventMiddleClickDefault);
      window.removeEventListener('pointerup', handlePointerUp, { capture: true });
      window.removeEventListener('pointercancel', handlePointerCancel, { capture: true });
      window.removeEventListener('mouseup', handleWindowMouseUp, { capture: true });
      window.removeEventListener('blur', handleWindowBlur);
      panState.current = null;
      restoreMiddlePanTool();
      setIsPanning(false);
    };
  }, [documentState, hasDocumentView, setActiveTool, setZoom, setZoomPreset]);

  const pages = documentState?.document.pages ?? [];
  const pageByIndex = useMemo(() => {
    return new Map(pages.map((page) => [page.index, page]));
  }, [pages]);
  const layoutMode = scrollMode === 'single-page'
    ? 'single-page'
    : pageColumnsEnabled
      ? 'columns'
      : 'continuous';
  layoutModeRef.current = layoutMode;
  const scrollWheelMode = layoutMode === 'single-page'
    ? singlePageScrollWheelMode
    : continuousScrollWheelMode;
  const pageLayoutGap = computePageLayoutGap(zoom);
  const pageLayouts = useMemo(() => {
    const pageInputs = pages.map((page) => ({
      index: page.index,
      width: page.size.width,
      height: page.size.height,
    }));
    const panPadding = getEffectiveCanvasPadding(
      pageInputs,
      viewportSize.width,
      viewportSize.height,
      zoomPreset,
      canvasPadding,
    );
    return buildPageLayouts(
      pageInputs,
      zoom,
      viewportSize.width || 1,
      pageLayoutGap,
      panPadding,
      {
        mode: layoutMode,
        cadViewOrganisation,
        pagesPerColumn,
        currentPageIndex: currentPage,
        viewportHeight: viewportSize.height || 1,
      },
    );
  }, [cadViewOrganisation, canvasPadding, currentPage, layoutMode, pageLayoutGap, pages, pagesPerColumn, viewportSize.height, viewportSize.width, zoom, zoomPreset]);
  pageLayoutListRef.current = pageLayouts.layouts;

  useEffect(() => {
    const wasPageColumnsEnabled = previousPageColumnsEnabledRef.current;
    previousPageColumnsEnabledRef.current = pageColumnsEnabled;

    if (
      wasPageColumnsEnabled
      || !pageColumnsEnabled
      || pages.length === 0
      || viewportSize.width <= 0
      || viewportSize.height <= 0
    ) {
      return;
    }

    const nextZoom = resolveCadOverviewEntryZoom({
      pages,
      viewportSize,
      pagesPerColumn,
      cadViewOrganisation,
      gap: pageLayoutGap,
    });
    const nextLayoutGap = computePageLayoutGap(nextZoom);
    const pageInputs = pages.map((page) => ({
      index: page.index,
      width: page.size.width,
      height: page.size.height,
    }));
    const nextOverview = resolveCadOverviewCenteredView(
      pageInputs,
      nextZoom,
      viewportSize.width || 1,
      viewportSize.height || 1,
      nextLayoutGap,
      cadViewOrganisation,
      pagesPerColumn,
      currentPage,
    );

    resetViewportContentOffset();
    updateCanvasPadding(nextOverview.canvasPadding);
    setZoomPreset('manual');
    setZoom(nextZoom);

    const container = containerRef.current;
    if (!container) {
      return;
    }

    const frameId = window.requestAnimationFrame(() => {
      container.scrollTo({ ...nextOverview.scroll, behavior: 'auto' });
      pendingScrollLeftRef.current = nextOverview.scroll.left;
      pendingScrollTopRef.current = nextOverview.scroll.top;
      setScrollLeft(nextOverview.scroll.left);
      setScrollTop(nextOverview.scroll.top);
    });

    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [
    cadViewOrganisation,
    currentPage,
    pageColumnsEnabled,
    pageLayoutGap,
    pages,
    pagesPerColumn,
    setZoom,
    setZoomPreset,
    viewportSize,
  ]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const handleWheel = (event: WheelEvent) => {
      if (!hasDocumentView) {
        return;
      }

      event.preventDefault();
      const wheelAt = performance.now();
      const wheelElapsedMs = lastWheelMotionAtRef.current === null
        ? 16
        : wheelAt - lastWheelMotionAtRef.current;
      const rapid = resolveRapidViewportMotion({
        previousRapid: rapidViewportMotionRef.current,
        distancePx: Math.hypot(event.deltaX, event.deltaY),
        elapsedMs: wheelElapsedMs,
      });
      lastWheelMotionAtRef.current = wheelAt;
      markViewportInMotion(rapid);
      const shouldScroll = shouldScrollViewportWheel(layoutMode, scrollWheelMode, event.ctrlKey);
      if (shouldScroll) {
        if (wheelZoomFrameRef.current !== null) {
          window.cancelAnimationFrame(wheelZoomFrameRef.current);
          wheelZoomFrameRef.current = null;
        }
        wheelZoomDeltaYRef.current = 0;
        latestWheelZoomPointRef.current = null;
        if (layoutMode === 'single-page') {
          const pageNavigation = resolveSinglePageWheelNavigation({
            pageIndices: pages.map((page) => page.index),
            currentPage: useViewerStore.getState().currentPage,
            deltaX: event.deltaX,
            deltaY: event.deltaY,
            accumulatedDelta: singlePageWheelDeltaRef.current,
          });
          singlePageWheelDeltaRef.current = pageNavigation.accumulatedDelta;

          if (pageNavigation.pageIndex !== null) {
            sessionRef.current?.setNavigationIntent(pageNavigation.pageIndex, 1200, 'generic');
            resetViewportContentOffset();
            resetCanvasPadding();
            setCurrentPage(pageNavigation.pageIndex);
            requestPageScroll(pageNavigation.pageIndex);
          }
          return;
        }

        singlePageWheelDeltaRef.current = 0;
        const nextScroll = {
          left: container.scrollLeft + event.deltaX,
          top: container.scrollTop + event.deltaY,
        };
        container.scrollTo({
          left: nextScroll.left,
          top: nextScroll.top,
          behavior: 'auto',
        });
        return;
      }

      singlePageWheelDeltaRef.current = 0;
      suppressContinuousCurrentPageFromScroll();
      const bounds = container.getBoundingClientRect();
      const localX = event.clientX - bounds.left;
      const localY = event.clientY - bounds.top;
      wheelZoomDeltaYRef.current += event.deltaY;
      latestWheelZoomPointRef.current = { localX, localY };
      if (wheelZoomFrameRef.current !== null) {
        return;
      }

      wheelZoomFrameRef.current = window.requestAnimationFrame(() => {
        wheelZoomFrameRef.current = null;
        const deltaY = clampWheelZoomFrameDelta(wheelZoomDeltaYRef.current);
        const point = latestWheelZoomPointRef.current;
        wheelZoomDeltaYRef.current = 0;
        latestWheelZoomPointRef.current = null;
        if (!point || Math.abs(deltaY) <= Number.EPSILON) {
          return;
        }

        const currentOffset = viewportContentOffsetRef.current;
        const contentX = container.scrollLeft - currentOffset.x + point.localX;
        const contentY = container.scrollTop - currentOffset.y + point.localY;
        const anchor = getWheelZoomAnchor(pageLayouts.layouts, zoom, contentX, contentY);
        const nextZoom = computeBluebeamWheelZoom(zoom, deltaY);
        if (Math.abs(nextZoom - zoom) <= 0.001) {
          return;
        }

        const zoomRatio = nextZoom / zoom;
        setZoomPreset('manual');
        wheelZoomTargetRef.current = nextZoom;
        pendingWheelZoomAnchorRef.current = {
          anchor,
          localX: point.localX,
          localY: point.localY,
          zoom: nextZoom,
          zoomRatio,
        };
        setZoom(nextZoom);
      });
    };

    container.addEventListener('wheel', handleWheel, { capture: true, passive: false });
    return () => {
      container.removeEventListener('wheel', handleWheel, { capture: true });
      if (wheelZoomFrameRef.current !== null) {
        window.cancelAnimationFrame(wheelZoomFrameRef.current);
        wheelZoomFrameRef.current = null;
      }
      wheelZoomDeltaYRef.current = 0;
      latestWheelZoomPointRef.current = null;
      singlePageWheelDeltaRef.current = 0;
      if (continuousCurrentPageScrollSuppressionTimeoutRef.current !== null) {
        window.clearTimeout(continuousCurrentPageScrollSuppressionTimeoutRef.current);
        continuousCurrentPageScrollSuppressionTimeoutRef.current = null;
      }
      suppressContinuousCurrentPageScrollRef.current = false;
    };
  }, [hasDocumentView, layoutMode, pageLayoutGap, pageLayouts.layouts, pages, requestPageScroll, scrollWheelMode, setCurrentPage, setZoom, setZoomPreset, zoom]);

  useLayoutEffect(() => {
    const pendingAnchor = pendingWheelZoomAnchorRef.current;
    const container = containerRef.current;
    if (!pendingAnchor || !container || Math.abs(pendingAnchor.zoom - zoom) > 0.001) {
      return;
    }

    pendingWheelZoomAnchorRef.current = null;
    const nextAnchorContent = getAnchorContentPosition(
      pendingAnchor.anchor,
      pageLayouts.layouts,
      zoom,
      pendingAnchor.zoomRatio,
    );
    const nextMaxScrollLeft = Math.max(0, container.scrollWidth - container.clientWidth);
    const nextMaxScrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
    const nextX = resolveAnchoredZoomAxis(nextAnchorContent.x, pendingAnchor.localX, nextMaxScrollLeft);
    const nextY = resolveAnchoredZoomAxis(nextAnchorContent.y, pendingAnchor.localY, nextMaxScrollTop);
    const nextOffset = { x: nextX.contentOffset, y: nextY.contentOffset };

    viewportContentOffsetRef.current = nextOffset;
    setViewportContentOffset(nextOffset);
    wheelZoomHandledColumnAnchorRef.current = true;
    suppressContinuousCurrentPageFromScroll();
    container.scrollTo({
      left: nextX.scrollOffset,
      top: nextY.scrollOffset,
      behavior: 'auto',
    });
  }, [layoutMode, pageLayouts.layouts, zoom]);

  useLayoutEffect(() => {
    const container = containerRef.current;
    const pendingPanScroll = pendingPanScrollRef.current;
    if (!container || !pendingPanScroll) {
      return;
    }

    pendingPanScrollRef.current = null;
    container.scrollTo({ left: pendingPanScroll.left, top: pendingPanScroll.top, behavior: 'auto' });
  }, [pageLayouts.totalHeight, pageLayouts.totalWidth]);

  useLayoutEffect(() => {
    const container = containerRef.current;
    const previousSnapshot = layoutTransitionSnapshotRef.current;
    const didHandleWheelZoom = wheelZoomHandledColumnAnchorRef.current;
    wheelZoomHandledColumnAnchorRef.current = false;

    const previousCentreAnchor = previousSnapshot
      ? captureViewportCentreAnchor(
        previousSnapshot.layouts,
        previousSnapshot.effectiveScrollLeft,
        previousSnapshot.effectiveScrollTop,
        previousSnapshot.viewportSize,
      )
      : null;
    const shouldPreserveCentreAnchor = previousSnapshot
      && previousCentreAnchor
      && shouldPreserveViewportAnchorAfterLayoutChange({
        previousLayoutMode: previousSnapshot.layoutMode,
        nextLayoutMode: layoutMode,
        previousCadViewOrganisation: previousSnapshot.cadViewOrganisation ?? 'columns',
        nextCadViewOrganisation: cadViewOrganisation,
        previousPagesPerColumn: previousSnapshot.pagesPerColumn,
        nextPagesPerColumn: pagesPerColumn,
        previousZoom: previousSnapshot.zoom,
        nextZoom: zoom,
        didHandleWheelZoom,
      });

    if (
      shouldPreserveCentreAnchor
      && container
      && layoutMode === 'single-page'
      && !pageLayouts.layouts.some((layout) => layout.index === previousCentreAnchor.pageIndex)
      && currentPage !== previousCentreAnchor.pageIndex
    ) {
      setCurrentPage(previousCentreAnchor.pageIndex);
      return;
    }

    if (shouldPreserveCentreAnchor && container) {
      const nextScroll = resolveViewportCentreAnchorScroll(
        previousCentreAnchor,
        pageLayouts.layouts,
        container.clientWidth,
        container.clientHeight,
        container.scrollWidth,
        container.scrollHeight,
      );

      if (nextScroll) {
        markViewportInMotion();
        suppressContinuousCurrentPageFromScroll();
        viewportContentOffsetRef.current = nextScroll.contentOffset;
        setViewportContentOffset(nextScroll.contentOffset);
        container.scrollTo({
          left: nextScroll.left,
          top: nextScroll.top,
          behavior: 'auto',
        });
      }
    }

    const currentOffset = viewportContentOffsetRef.current;
    layoutTransitionSnapshotRef.current = {
      layoutMode,
      cadViewOrganisation,
      pagesPerColumn,
      zoom,
      layouts: pageLayouts.layouts,
      effectiveScrollLeft: (container?.scrollLeft ?? scrollLeft) - currentOffset.x,
      effectiveScrollTop: (container?.scrollTop ?? scrollTop) - currentOffset.y,
      viewportSize,
    };
  }, [cadViewOrganisation, currentPage, layoutMode, pageLayouts.layouts, pagesPerColumn, scrollLeft, scrollTop, setCurrentPage, viewportSize, zoom]);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const nextScrollLeft = container.scrollLeft;
    const nextScrollTop = container.scrollTop;
    setScrollLeft((current) => (current === nextScrollLeft ? current : nextScrollLeft));
    setScrollTop((current) => (current === nextScrollTop ? current : nextScrollTop));
  }, [pageLayouts.totalHeight, pageLayouts.totalWidth]);

  const effectiveScrollLeft = scrollLeft - viewportContentOffset.x;
  const effectiveScrollTop = scrollTop - viewportContentOffset.y;
  const renderBacklogIdle = useMemo(() => {
    return session ? isRenderBacklogIdle(session.diagnostics()) : false;
  }, [session, sessionVersion]);
  const visibleOverscanPx = resolveViewportVisibleOverscanPx({
    layoutMode,
    viewportInMotion,
    renderBacklogIdle,
    adaptivePerformanceLevel: adaptivePerformance.level,
  });
  const visibleLayoutPositions = useMemo(() => {
    return computeVisibleLayoutPositions(pageLayouts.layouts, {
      left: effectiveScrollLeft,
      top: effectiveScrollTop,
      width: viewportSize.width,
      height: viewportSize.height,
    }, visibleOverscanPx, 0);
  }, [effectiveScrollLeft, effectiveScrollTop, pageLayouts.layouts, viewportSize.height, viewportSize.width, visibleOverscanPx]);
  const strictVisibleLayoutPositions = useMemo(() => {
    return computeVisibleLayoutPositions(pageLayouts.layouts, {
      left: effectiveScrollLeft,
      top: effectiveScrollTop,
      width: viewportSize.width,
      height: viewportSize.height,
    }, 0, 0);
  }, [effectiveScrollLeft, effectiveScrollTop, pageLayouts.layouts, viewportSize.height, viewportSize.width]);
  const strictVisibleLayoutPositionSet = useMemo(() => {
    return new Set(strictVisibleLayoutPositions);
  }, [strictVisibleLayoutPositions]);
  const visiblePriorityAnchor = useMemo(() => {
    return getViewportAnchorIndex(pageLayouts.layouts, effectiveScrollLeft, effectiveScrollTop, viewportSize, currentPage);
  }, [currentPage, effectiveScrollLeft, effectiveScrollTop, pageLayouts.layouts, viewportSize]);
  const useColumnOverviewMode = shouldUseColumnOverviewMode({
    layoutMode,
    zoom,
    mountedPageCount: visibleLayoutPositions.length,
    viewportInMotion,
    currentlyActive: columnOverviewModeActive,
  });
  useEffect(() => {
    if (columnOverviewModeActive === useColumnOverviewMode) {
      return;
    }

    setColumnOverviewModeActive(useColumnOverviewMode);
  }, [columnOverviewModeActive, useColumnOverviewMode]);
  const useCanvasColumnOverview = shouldUseCanvasColumnOverview({
    useColumnOverviewMode,
    strictVisiblePageCount: strictVisibleLayoutPositions.length,
  });
  const overviewCanvasLayouts = useMemo(() => {
    if (!useCanvasColumnOverview) {
      return [];
    }

    return visibleLayoutPositions
      .map((position) => pageLayouts.layouts[position])
      .filter((layout): layout is PageLayout => Boolean(layout));
  }, [pageLayouts.layouts, useCanvasColumnOverview, visibleLayoutPositions]);
  const overviewCanvasPreviewUrls = useMemo(() => {
    const urls = new Map<number, string>();
    if (!useCanvasColumnOverview || !session) {
      return urls;
    }

    for (const layout of overviewCanvasLayouts) {
      const reusablePreview = session.getReusablePagePreviewInfo(
        layout.index,
        0,
        pageByIndex.get(layout.index)?.rotation,
      );
      if (reusablePreview) {
        urls.set(layout.index, reusablePreview.objectUrl);
      }
    }

    return urls;
  }, [overviewCanvasLayouts, pageByIndex, session, sessionVersion, useCanvasColumnOverview]);
  const overviewCanvasStrictVisiblePageIndices = useMemo(() => {
    const pageIndices = new Set<number>();
    if (!useCanvasColumnOverview) {
      return pageIndices;
    }

    for (const position of strictVisibleLayoutPositions) {
      const pageIndex = pageLayouts.layouts[position]?.index;
      if (pageIndex !== undefined) {
        pageIndices.add(pageIndex);
      }
    }

    return pageIndices;
  }, [pageLayouts.layouts, strictVisibleLayoutPositions, useCanvasColumnOverview]);
  const columnOverviewTilePriorityByPageIndex = useMemo(() => {
    const priorities = new Map<number, number>();
    if (!useColumnOverviewMode || useCanvasColumnOverview) {
      return priorities;
    }

    const visibleLayouts = visibleLayoutPositions
      .map((position) => ({
        layout: pageLayouts.layouts[position],
        isStrictlyVisible: strictVisibleLayoutPositionSet.has(position),
      }))
      .filter((entry): entry is { layout: PageLayout; isStrictlyVisible: boolean } => Boolean(entry.layout))
      .sort((left, right) => {
        const strictDelta = Number(right.isStrictlyVisible) - Number(left.isStrictlyVisible);
        if (strictDelta !== 0) {
          return strictDelta;
        }

        return left.layout.top - right.layout.top
          || left.layout.left - right.layout.left
          || left.layout.index - right.layout.index;
      });

    visibleLayouts.forEach((entry, renderIndex) => {
      priorities.set(entry.layout.index, (entry.isStrictlyVisible ? 5000 : 1000) - renderIndex);
    });

    return priorities;
  }, [pageLayouts.layouts, strictVisibleLayoutPositionSet, useCanvasColumnOverview, useColumnOverviewMode, visibleLayoutPositions]);
  useLayoutEffect(() => {
    if (zoomPreset === 'manual') {
      if (!startupPageRenderReady) {
        setStartupPageRenderReady(true);
      }
      return;
    }

    if (pages.length === 0 || fitViewportSize.width === 0 || fitViewportSize.height === 0) {
      return;
    }

    const viewportFitReferencePageIndex = layoutMode === 'continuous' ? visiblePriorityAnchor : currentPage;
    const fitReferencePageIndex = fitZoomReferencePageRef.current ?? viewportFitReferencePageIndex;
    const referencePage = pages.find((page) => page.index === fitReferencePageIndex) ?? pages[0];
    if (!referencePage) {
      return;
    }

    const fitWidthZoom = resolveFitWidthZoom(fitViewportSize.width, referencePage.size.width);
    const fitPageZoom = Math.min(
      fitWidthZoom,
      fitViewportSize.height / Math.max(1, referencePage.size.height + PAGE_LAYOUT_GAP * 2),
    );
    const nextZoom = zoomPreset === 'fit-page'
      ? clampViewerZoom(quantizeFitZoomDown(fitPageZoom))
      : fitWidthZoom;

    if (Math.abs(nextZoom - zoom) > 0.001) {
      fitZoomReferencePageRef.current = referencePage.index;
      setZoom(nextZoom);
      return;
    }

    if (!startupPageRenderReady) {
      setStartupPageRenderReady(true);
    }

    fitZoomReferencePageRef.current = null;
  }, [currentPage, fitViewportSize.height, fitViewportSize.width, layoutMode, pages, setZoom, startupPageRenderReady, visiblePriorityAnchor, zoom, zoomPreset]);

  useEffect(() => {
    const nextVisiblePageIndices = strictVisibleLayoutPositions
      .map((position) => pageLayouts.layouts[position])
      .filter((layout): layout is PageLayout => Boolean(layout))
      .map((layout) => layout.index);
    const currentVisiblePageIndices = useViewerStore.getState().visiblePageIndices;

    if (!areNumberArraysEqual(currentVisiblePageIndices, nextVisiblePageIndices)) {
      setVisiblePageIndices(nextVisiblePageIndices);
    }
  }, [pageLayouts.layouts, setVisiblePageIndices, strictVisibleLayoutPositions]);

  useEffect(() => {
    if (!session || !renderCoordinator) {
      return;
    }

    const diagnostics = session.diagnostics();
    if (!shouldWarmNearbyPagePreviews({
      diagnostics,
      viewportInMotion,
      strictVisiblePageCount: strictVisibleLayoutPositions.length,
      adaptivePerformanceLevel: adaptivePerformance.level,
    })) {
      return;
    }

    const visiblePageIndexSet = new Set<number>();
    const visiblePageIndices: number[] = [];
    for (const position of strictVisibleLayoutPositions) {
      const pageIndex = pageLayouts.layouts[position]?.index;
      if (pageIndex === undefined || visiblePageIndexSet.has(pageIndex)) {
        continue;
      }
      visiblePageIndexSet.add(pageIndex);
      visiblePageIndices.push(pageIndex);
    }

    if (visiblePageIndices.length === 0) {
      return;
    }

    const uniqueCandidates = resolveNearbyPagePreviewWarmCandidates({
      visiblePageIndices,
      pageCount: pages.length,
      layoutMode,
    });
    if (uniqueCandidates.length === 0) {
      return;
    }

    const abortController = new AbortController();
    const pixelRatio = window.devicePixelRatio || 1;
    for (const pageIndex of uniqueCandidates) {
      const page = pageByIndex.get(pageIndex);
      if (!page || session.getReusablePagePreviewInfo(pageIndex, 0, page.rotation)) {
        continue;
      }

      void renderCoordinator.renderPageUrl('main-page', pageIndex, computePreviewRasterZoom(zoom, page.size, pixelRatio), pixelRatio, {
        priority: 100 - Math.abs(pageIndex - currentPage),
        urgency: 'prefetch',
        requestClass: 'warming',
        rotation: page.rotation,
        signal: abortController.signal,
      }).catch(() => undefined);
    }

    return () => {
      abortController.abort();
    };
  }, [
    currentPage,
    adaptivePerformance.level,
    layoutMode,
    pageByIndex,
    pageLayouts.layouts,
    pages.length,
    session,
    sessionVersion,
    strictVisibleLayoutPositions,
    viewportInMotion,
    zoom,
    renderCoordinator,
  ]);

  useEffect(() => {
    const previousContinuousScrollTop = previousContinuousCurrentPageScrollTopRef.current;
    previousContinuousCurrentPageScrollTopRef.current = effectiveScrollTop;

    if (!shouldAutoUpdateCurrentPageFromViewport(layoutMode)) {
      return;
    }

    if (!lastScrollWasUserScrollRef.current) {
      return;
    }

    const currentLayout = getContinuousCurrentPageLayout(
      pageLayouts.layouts,
      effectiveScrollLeft,
      previousContinuousScrollTop,
      effectiveScrollTop,
      viewportSize,
      scrollDirectionRef.current,
    );

    if (currentLayout && useViewerStore.getState().currentPage !== currentLayout.index) {
      setCurrentPage(currentLayout.index);
    }
  }, [effectiveScrollLeft, effectiveScrollTop, layoutMode, pageLayouts.layouts, setCurrentPage, viewportSize]);

  useEffect(() => {
    if (!pendingDocumentScroll || !containerRef.current) {
      return;
    }

    const container = containerRef.current;
    markViewportInMotion();
    suppressContinuousCurrentPageFromScroll();
    resetViewportContentOffset();
    resetCanvasPadding();
    container.scrollTo({
      left: pendingDocumentScroll.edge === 'top' ? 0 : container.scrollLeft,
      top: pendingDocumentScroll.edge === 'top'
        ? 0
        : Math.max(0, container.scrollHeight - container.clientHeight),
      behavior: 'auto',
    });

    const frameId = window.requestAnimationFrame(() => {
      consumeDocumentScroll(pendingDocumentScroll.requestId);
    });

    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [consumeDocumentScroll, pendingDocumentScroll]);

  useEffect(() => {
    if (!hasDocumentView) {
      return;
    }

    const handleKeyboardNavigation = (event: KeyboardEvent) => {
      const isMacPlatform = navigator.platform.toLowerCase().includes('mac');
      const primaryModifierPressed = isMacPlatform ? event.metaKey : event.ctrlKey;
      const secondaryModifierPressed = isMacPlatform ? event.ctrlKey : event.metaKey;
      const isPageShortcut = primaryModifierPressed && (event.key === 'ArrowLeft' || event.key === 'ArrowRight');
      if (
        event.defaultPrevented
        || event.altKey
        || event.shiftKey
        || secondaryModifierPressed
        || (primaryModifierPressed && !isPageShortcut)
        || isInteractiveShortcutTarget(event.target)
      ) {
        return;
      }

      const actionByKey: Partial<Record<string, DocumentKeyboardAction>> = {
        ArrowUp: 'arrow-up',
        ArrowDown: 'arrow-down',
        ArrowLeft: 'arrow-left',
        ArrowRight: 'arrow-right',
        PageUp: 'page-up',
        PageDown: 'page-down',
        Home: 'home',
        End: 'end',
      };
      const action = isPageShortcut
        ? event.key === 'ArrowLeft' ? 'previous-page' : 'next-page'
        : actionByKey[event.key];
      if (!action) {
        return;
      }

      const container = containerRef.current;
      if (!container) {
        return;
      }

      const navigation = resolveDocumentKeyboardNavigation({
        action,
        pageIndices: pages.map((page) => page.index),
        currentPage,
      });

      event.preventDefault();
      if (!navigation) {
        return;
      }

      if (navigation.kind === 'document-edge') {
        const pageIndex = navigation.edge === 'top' ? pages[0]?.index : pages.at(-1)?.index;
        if (pageIndex !== undefined && currentPage !== pageIndex) {
          setCurrentPage(pageIndex);
        }
        requestDocumentScroll(navigation.edge);
        return;
      }

      if (navigation.kind === 'page') {
        sessionRef.current?.setNavigationIntent(navigation.pageIndex, 1200, 'generic');
        setCurrentPage(navigation.pageIndex);
        requestPageScroll(navigation.pageIndex);
        return;
      }

      const distance = navigation.distance === 'page'
        ? Math.max(DOCUMENT_KEYBOARD_SCROLL_STEP_PX, container.clientHeight * 0.9)
        : DOCUMENT_KEYBOARD_SCROLL_STEP_PX;
      container.scrollBy({
        left: navigation.axis === 'horizontal' ? navigation.direction * distance : 0,
        top: navigation.axis === 'vertical' ? navigation.direction * distance : 0,
        behavior: 'auto',
      });
    };

    window.addEventListener('keydown', handleKeyboardNavigation);
    return () => window.removeEventListener('keydown', handleKeyboardNavigation);
  }, [currentPage, hasDocumentView, pages, requestDocumentScroll, requestPageScroll, setCurrentPage]);

  useEffect(() => {
    if (!pendingPageScroll || !containerRef.current) {
      return;
    }

    const targetLayout = pageLayouts.layouts.find((layout) => layout.index === pendingPageScroll.pageIndex);
    if (!targetLayout) {
      return;
    }

    markViewportInMotion();
    suppressContinuousCurrentPageFromScroll();
    resetViewportContentOffset();
    resetCanvasPadding();
    containerRef.current.scrollTo({
      left: Math.max(0, targetLayout.left - pageLayoutGap),
      top: Math.max(0, targetLayout.top - pageLayoutGap),
      behavior: 'auto',
    });

    const frameId = window.requestAnimationFrame(() => {
      consumePageScroll(pendingPageScroll.requestId);
    });

    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [consumePageScroll, pageLayoutGap, pageLayouts.layouts, pendingPageScroll]);

  const handleSelectPage = useCallback((pageIndex: number) => {
    const activeSession = sessionRef.current;
    activeSession?.setNavigationIntent(pageIndex, 1200, 'generic');
    if (useViewerStore.getState().currentPage !== pageIndex) {
      setCurrentPage(pageIndex);
    }
    requestThumbnailScroll(pageIndex);
  }, [requestThumbnailScroll, setCurrentPage]);

  if (opening && showOpeningIndicator) {
    return <DocumentOpeningIndicator progress={openingProgress} />;
  }

  if (!documentState || !session) {
    return (
      <section className="flex h-full items-center justify-center bg-background text-muted-foreground">
        <div className="flex flex-col items-center text-center">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className="mb-3"
            data-testid="viewport-open-document"
            onClick={onOpenDocument}
          >
            Open
          </Button>
          <div className="text-[14px] font-medium text-foreground">Open a PDF to begin</div>
        </div>
      </section>
    );
  }

  const viewportContent = (
    <div
      className="relative"
      onPointerMove={(event) => {
        const bounds = event.currentTarget.getBoundingClientRect();
        const localX = event.clientX - bounds.left;
        const localY = event.clientY - bounds.top;
        const pointerLayout = getLayoutAtPoint(pageLayouts.layouts, localX, localY);
        const nextPointerPageIndex = pointerLayout?.index ?? null;
        setPointerPageIndex((current) => (current === nextPointerPageIndex ? current : nextPointerPageIndex));
      }}
      onPointerLeave={() => setPointerPageIndex(null)}
      style={{
        height: `${pageLayouts.totalHeight}px`,
        width: `${pageLayouts.totalWidth}px`,
        minWidth: '100%',
        transform: viewportContentOffset.x !== 0 || viewportContentOffset.y !== 0
          ? `translate(${viewportContentOffset.x}px, ${viewportContentOffset.y}px)`
          : undefined,
      }}
    >
      {startupPageRenderReady && useCanvasColumnOverview ? (
        <ColumnOverviewCanvas
          session={session}
          pages={pages}
          layouts={overviewCanvasLayouts}
          previewUrls={overviewCanvasPreviewUrls}
          strictVisiblePageIndices={overviewCanvasStrictVisiblePageIndices}
          pointerPageIndex={pointerPageIndex}
          focusPageIndex={pointerPageIndex ?? visiblePriorityAnchor}
          totalWidth={pageLayouts.totalWidth}
          totalHeight={pageLayouts.totalHeight}
          currentPage={currentPage}
          zoom={zoom}
          onSelectPage={handleSelectPage}
        />
      ) : startupPageRenderReady ? visibleLayoutPositions.map((layoutPosition) => {
        const layout = pageLayouts.layouts[layoutPosition];
        const page = layout ? pageByIndex.get(layout.index) : null;
        if (!layout) {
          return null;
        }

        if (!page) {
          return null;
        }

        const isStrictlyVisible = strictVisibleLayoutPositionSet.has(layoutPosition);
        const visiblePageViewportRect = resolveVisiblePageViewportRect(
          layout,
          effectiveScrollLeft,
          effectiveScrollTop,
          viewportSize,
        );

        if (useColumnOverviewMode) {
          return (
            <PageOverviewTile
              key={`${documentState.filePath}:overview:${layout.index}`}
              session={session}
              page={page}
              layout={layout}
              renderPreview={shouldRenderColumnOverviewPreview(zoom, viewportInMotion, isStrictlyVisible)}
              deferPreviewImage={shouldDeferColumnOverviewPreview(viewportInMotion, isStrictlyVisible)}
              isCurrentPage={page.index === currentPage}
              isStrictlyVisible={isStrictlyVisible}
              isPointerPage={page.index === pointerPageIndex}
              isFocusPage={page.index === (pointerPageIndex ?? visiblePriorityAnchor)}
              renderPriority={columnOverviewTilePriorityByPageIndex.get(page.index) ?? 0}
              label={{
                pageNumber: layout.index + 1,
              }}
              onSelectPage={handleSelectPage}
            />
          );
        }

        return (
          <PageView
            key={`${documentState.filePath}:${layout.index}`}
            session={session}
            page={page}
            layout={layout}
            zoom={zoom}
            renderPriority={(isStrictlyVisible ? 2000 : 1000) - Math.abs(page.index - visiblePriorityAnchor)}
            renderUrgency={isStrictlyVisible ? 'visible' : 'prefetch'}
            isStrictlyVisible={isStrictlyVisible}
            isTargetPage={page.index === currentPage}
            viewportInMotion={viewportInMotion}
            deferHighQualityDuringMotion={viewportInMotion && (rapidViewportMotion || page.index !== currentPage)}
            adaptivePerformanceLevel={adaptivePerformance.level}
            visiblePageViewportRect={visiblePageViewportRect}
            overviewLabel={null}
            calibrationPickActive={Boolean(calibrationPick?.active)}
            calibrationStartPoint={calibrationPick?.pageIndex === page.index ? calibrationPick.startPoint : null}
            onCalibrationPoint={onCalibrationPoint}
            onSelectPage={handleSelectPage}
            onHoverPage={setPointerPageIndex}
          />
        );
      }) : (
        <div className="absolute inset-0 flex items-start justify-center pt-8">
          <Spinner className="size-7" />
        </div>
      )}
    </div>
  );

  return (
    <div className="relative h-full">
      <CustomScrollArea
        ref={containerRef}
        className="relative h-full bg-background"
        orientation="both"
        viewportClassName={isPanning ? 'cursor-grabbing' : activeTool === 'pan' ? 'cursor-grab active:cursor-grabbing' : 'cursor-default'}
        viewportProps={{
          'data-adaptive-performance-level': adaptivePerformance.level,
          'data-adaptive-performance-reason': adaptivePerformance.reason,
          'data-adaptive-refresh-hz': adaptivePerformance.detectedRefreshHz,
          'data-adaptive-target-frame-ms': adaptivePerformance.targetFrameMs,
        } as React.HTMLAttributes<HTMLDivElement>}
        viewportTestId="document-viewport"
        verticalTrackTestId="document-viewport-scrollbar-track-y"
        verticalThumbTestId="document-viewport-scrollbar-thumb-y"
        horizontalTrackTestId="document-viewport-scrollbar-track-x"
        horizontalThumbTestId="document-viewport-scrollbar-thumb-x"
        cornerTestId="document-viewport-scrollbar-corner"
      >
        {viewportContent}
      </CustomScrollArea>
      {calibrationPick?.active ? (
        <Alert className="pointer-events-auto absolute left-1/2 top-4 z-30 w-fit -translate-x-1/2 pr-20" data-testid="page-scale-calibration-instructions">
          <AlertDescription>
            {calibrationPick.pointCount === 0
              ? 'Click the first point of a known distance.'
              : 'Click the second point of the same distance. Hold Shift for horizontal or vertical.'}
          </AlertDescription>
          <AlertAction>
            <Button type="button" variant="outline" size="xs" onClick={onCancelCalibrationPick}>
              Cancel
            </Button>
          </AlertAction>
        </Alert>
      ) : null}
    </div>
  );
}

export function DocumentOpeningIndicator({ progress = null }: { readonly progress?: PdfOpenProgress | null }) {
  const size = progress?.totalBytes ? formatFileSize(progress.totalBytes) : null;
  const percentage = progress?.totalBytes && progress.phase === 'reading'
    ? Math.min(100, Math.round(progress.bytesRead / progress.totalBytes * 100))
    : null;
  const title = progress?.fileName ? `Opening “${progress.fileName}”${size ? ` · ${size}` : ''}` : 'Opening PDF';
  const source = progress?.sourceName;
  const detail = progress?.phase === 'processing'
    ? 'Finishing…'
    : percentage !== null && percentage > 0
      ? `${source ? `Downloading from ${source}` : 'Reading file'} · ${percentage}%${formatEta(progress?.estimatedSecondsRemaining)}`
      : source
        ? `Waiting for ${source} to download the file…`
        : 'If the file is stored online, your storage provider may need to download it first.';
  return (
    <section
      className="flex h-full items-center justify-center bg-background text-muted-foreground"
      aria-live="polite"
      data-testid="viewport-opening-document"
    >
      <div className="flex max-w-sm flex-col items-center gap-3 text-center">
        <Spinner className="size-6" />
        <div className="flex flex-col gap-1">
          <div className="text-[14px] font-medium text-foreground">{title}</div>
          <div className="text-sm">{detail}</div>
        </div>
      </div>
    </section>
  );
}

export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** unitIndex;
  const digits = unitIndex === 0 ? 0 : value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[unitIndex]}`;
}

function formatEta(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 2) return '';
  if (seconds < 60) return ` · About ${Math.max(2, Math.round(seconds / 5) * 5)} seconds remaining`;
  const minutes = Math.max(1, Math.round(seconds / 60));
  return ` · About ${minutes} ${minutes === 1 ? 'minute' : 'minutes'} remaining`;
}

export function resolveVisiblePageViewportRect(
  layout: PageLayout,
  scrollLeft: number,
  scrollTop: number,
  viewportSize: { readonly width: number; readonly height: number },
): Rect | null {
  const left = Math.max(layout.left, scrollLeft);
  const top = Math.max(layout.top, scrollTop);
  const right = Math.min(layout.left + layout.width, scrollLeft + Math.max(0, viewportSize.width));
  const bottom = Math.min(layout.top + layout.height, scrollTop + Math.max(0, viewportSize.height));
  if (right <= left || bottom <= top) {
    return null;
  }

  return {
    x: left - layout.left,
    y: top - layout.top,
    width: right - left,
    height: bottom - top,
  };
}

export function measureViewportSize(
  viewport: Pick<HTMLElement, 'clientWidth' | 'clientHeight'>,
): { width: number; height: number } {
  return {
    width: Math.round(viewport.clientWidth),
    height: Math.round(viewport.clientHeight),
  };
}

export function resolveViewportVisibleOverscanPx({
  layoutMode,
  viewportInMotion,
  renderBacklogIdle,
  adaptivePerformanceLevel = 0,
}: {
  layoutMode: 'continuous' | 'columns' | 'single-page';
  viewportInMotion: boolean;
  renderBacklogIdle: boolean;
  adaptivePerformanceLevel?: AdaptivePerformanceLevel;
}): number {
  if (layoutMode === 'columns') {
    return COLUMN_VISIBLE_OVERSCAN_PX;
  }

  if (viewportInMotion) {
    return Math.min(DOCUMENT_MOTION_OVERSCAN_PX, resolveAdaptiveMotionOverscanPx(adaptivePerformanceLevel));
  }

  if (!renderBacklogIdle) {
    return 0;
  }

  return DOCUMENT_VISIBLE_OVERSCAN_PX;
}

export function shouldUseColumnOverviewMode({
  layoutMode,
  zoom,
  mountedPageCount,
  viewportInMotion = false,
  currentlyActive = false,
}: {
  layoutMode: 'continuous' | 'columns' | 'single-page';
  zoom: number;
  mountedPageCount: number;
  viewportInMotion?: boolean;
  currentlyActive?: boolean;
}): boolean {
  if (layoutMode === 'single-page') {
    return false;
  }

  const shouldUseMotionOverview = viewportInMotion
    && zoom <= COLUMN_MOTION_OVERVIEW_ZOOM_THRESHOLD
    && mountedPageCount >= COLUMN_OVERVIEW_MOUNTED_PAGE_EXIT_THRESHOLD;

  if (currentlyActive) {
    return zoom <= COLUMN_OVERVIEW_ZOOM_EXIT_THRESHOLD
      || mountedPageCount > COLUMN_OVERVIEW_MOUNTED_PAGE_EXIT_THRESHOLD
      || shouldUseMotionOverview;
  }

  return zoom <= COLUMN_OVERVIEW_ZOOM_THRESHOLD
    || mountedPageCount > COLUMN_OVERVIEW_MOUNTED_PAGE_THRESHOLD
    || shouldUseMotionOverview;
}

export function shouldUseCanvasColumnOverview({
  useColumnOverviewMode,
  strictVisiblePageCount,
}: {
  useColumnOverviewMode: boolean;
  strictVisiblePageCount: number;
}): boolean {
  return useColumnOverviewMode
    && strictVisiblePageCount >= COLUMN_CANVAS_OVERVIEW_VISIBLE_PAGE_THRESHOLD;
}

export function shouldRenderColumnOverviewPreview(
  zoom: number,
  viewportInMotion: boolean,
  isStrictlyVisible: boolean,
): boolean {
  return isStrictlyVisible || zoom < COLUMN_OVERVIEW_ZOOM_THRESHOLD || viewportInMotion;
}

export function shouldDeferColumnOverviewPreview(viewportInMotion: boolean, isStrictlyVisible: boolean): boolean {
  return viewportInMotion && !isStrictlyVisible;
}

export function resolveRapidViewportMotion({
  previousRapid,
  distancePx,
  elapsedMs,
}: {
  previousRapid: boolean;
  distancePx: number;
  elapsedMs: number;
}): boolean {
  if (!Number.isFinite(distancePx) || !Number.isFinite(elapsedMs) || elapsedMs <= 0) {
    return previousRapid;
  }

  const speedPxPerMs = Math.max(0, distancePx) / elapsedMs;
  return previousRapid
    ? speedPxPerMs >= RAPID_VIEWPORT_EXIT_PX_PER_MS
    : speedPxPerMs >= RAPID_VIEWPORT_ENTER_PX_PER_MS;
}

export function resolveColumnOverviewPreviewBatch({
  layouts,
  renderablePageIndices,
  availablePreviewPageIndices,
  requestedPreviewPageIndices,
  requestedPreviewWidths,
  requiredPreviewWidths,
  strictVisiblePageIndices,
  pointerPageIndex = null,
  focusPageIndex = null,
  distributedVisibleOrder = false,
}: {
  layouts: readonly PageLayout[];
  renderablePageIndices: ReadonlySet<number>;
  availablePreviewPageIndices: ReadonlySet<number>;
  requestedPreviewPageIndices: ReadonlySet<number>;
  requestedPreviewWidths?: ReadonlyMap<number, number>;
  requiredPreviewWidths?: ReadonlyMap<number, number>;
  strictVisiblePageIndices: ReadonlySet<number>;
  pointerPageIndex?: number | null;
  focusPageIndex?: number | null;
  distributedVisibleOrder?: boolean;
}): PageLayout[] {
  const candidates = layouts
    .filter((layout) => renderablePageIndices.has(layout.index))
    .filter((layout) => !isOverviewPreviewWidthSatisfied(
      layout.index,
      availablePreviewPageIndices,
      requiredPreviewWidths,
    ))
    .filter((layout) => !isOverviewPreviewWidthSatisfied(
      layout.index,
      requestedPreviewPageIndices,
      requiredPreviewWidths,
      requestedPreviewWidths,
    ))
    .sort((left, right) => {
      const pointerDelta = Number(right.index === pointerPageIndex) - Number(left.index === pointerPageIndex);
      if (pointerDelta !== 0) {
        return pointerDelta;
      }

      const focusDelta = Number(right.index === focusPageIndex) - Number(left.index === focusPageIndex);
      if (focusDelta !== 0) {
        return focusDelta;
      }

      const strictDelta = Number(strictVisiblePageIndices.has(right.index)) - Number(strictVisiblePageIndices.has(left.index));
      if (strictDelta !== 0) {
        return strictDelta;
      }

      return left.top - right.top || left.left - right.left || left.index - right.index;
    });
  const orderedCandidates = distributedVisibleOrder
    ? distributeStrictVisibleOverviewLayouts(candidates, strictVisiblePageIndices)
    : candidates;

  const hasMissingStrictVisiblePreview = orderedCandidates.some((layout) => strictVisiblePageIndices.has(layout.index));
  return orderedCandidates.slice(
    0,
    hasMissingStrictVisiblePreview
      ? COLUMN_CANVAS_OVERVIEW_VISIBLE_PREVIEW_BATCH_SIZE
      : COLUMN_CANVAS_OVERVIEW_PREFETCH_PREVIEW_BATCH_SIZE,
  );
}

function isOverviewPreviewWidthSatisfied(
  pageIndex: number,
  pageIndices: ReadonlySet<number>,
  requiredPreviewWidths?: ReadonlyMap<number, number>,
  availablePreviewWidths?: ReadonlyMap<number, number>,
): boolean {
  if (!pageIndices.has(pageIndex)) {
    return false;
  }

  const requiredWidth = requiredPreviewWidths?.get(pageIndex);
  if (!requiredWidth) {
    return true;
  }

  const availableWidth = availablePreviewWidths?.get(pageIndex);
  return availableWidth !== undefined && availableWidth >= requiredWidth;
}

export function resolveCadOverviewEntryZoom({
  pages,
  viewportSize,
  pagesPerColumn,
  cadViewOrganisation,
  gap,
}: {
  pages: readonly PageModel[];
  viewportSize: { readonly width: number; readonly height: number };
  pagesPerColumn: number;
  cadViewOrganisation: 'columns' | 'rows';
  gap: number;
}): number {
  if (pages.length === 0 || viewportSize.width <= 0 || viewportSize.height <= 0) {
    return clampViewerZoom(0);
  }

  const pagesPerGroup = Math.max(1, Math.round(pagesPerColumn));
  const columnCount = cadViewOrganisation === 'columns'
    ? Math.max(1, Math.ceil(pages.length / pagesPerGroup))
    : Math.min(pages.length, pagesPerGroup);
  const rowCount = cadViewOrganisation === 'columns'
    ? Math.min(pages.length, pagesPerGroup)
    : Math.max(1, Math.ceil(pages.length / pagesPerGroup));
  const columnWidths = Array.from({ length: columnCount }, () => 1);
  const rowHeights = Array.from({ length: rowCount }, () => 1);

  pages.forEach((page, position) => {
    const columnIndex = cadViewOrganisation === 'columns'
      ? Math.floor(position / pagesPerGroup)
      : position % pagesPerGroup;
    const rowIndex = cadViewOrganisation === 'columns'
      ? position % pagesPerGroup
      : Math.floor(position / pagesPerGroup);
    columnWidths[columnIndex] = Math.max(columnWidths[columnIndex] ?? 1, page.size.width);
    rowHeights[rowIndex] = Math.max(rowHeights[rowIndex] ?? 1, page.size.height);
  });

  const unscaledGridWidth = columnWidths.reduce((width, columnWidth) => width + columnWidth, gap * (columnWidths.length + 1));
  const unscaledGridHeight = rowHeights.reduce((height, rowHeight) => height + rowHeight, gap * (rowHeights.length + 1));
  const fitZoom = Math.min(
    viewportSize.width / Math.max(1, unscaledGridWidth),
    viewportSize.height / Math.max(1, unscaledGridHeight),
  );

  return clampViewerZoom(quantizeFitZoomDown(fitZoom));
}

export function distributeStrictVisibleOverviewLayouts(
  layouts: readonly PageLayout[],
  strictVisiblePageIndices: ReadonlySet<number>,
  laneCount = 4,
): PageLayout[] {
  const strictVisibleLayouts = layouts.filter((layout) => strictVisiblePageIndices.has(layout.index));
  const remainingLayouts = layouts.filter((layout) => !strictVisiblePageIndices.has(layout.index));
  if (strictVisibleLayouts.length <= laneCount) {
    return [...strictVisibleLayouts, ...remainingLayouts];
  }

  const safeLaneCount = Math.max(1, laneCount);
  const laneSize = Math.ceil(strictVisibleLayouts.length / safeLaneCount);
  const lanes: PageLayout[][] = [];
  for (let laneIndex = 0; laneIndex < safeLaneCount; laneIndex += 1) {
    lanes.push(strictVisibleLayouts.slice(laneIndex * laneSize, (laneIndex + 1) * laneSize));
  }

  const distributed: PageLayout[] = [];
  const maxLaneLength = Math.max(...lanes.map((lane) => lane.length));
  for (let row = 0; row < maxLaneLength; row += 1) {
    for (const lane of lanes) {
      const layout = lane[row];
      if (layout) {
        distributed.push(layout);
      }
    }
  }

  return [...distributed, ...remainingLayouts];
}

type RenderBacklogDiagnostics = Pick<
  DiagnosticsSnapshot,
  | 'pageRenderReady'
  | 'thumbnailRenderReady'
  | 'queuedPageRenders'
  | 'queuedThumbnailRenders'
  | 'inflightPageRenders'
  | 'inflightThumbnailRenders'
>;

export function shouldWarmNearbyPagePreviews({
  diagnostics,
  viewportInMotion,
  strictVisiblePageCount,
  adaptivePerformanceLevel = 0,
}: {
  diagnostics: RenderBacklogDiagnostics;
  viewportInMotion: boolean;
  strictVisiblePageCount: number;
  adaptivePerformanceLevel?: AdaptivePerformanceLevel;
}): boolean {
  return !viewportInMotion
    && strictVisiblePageCount > 0
    && shouldAllowAdaptivePrefetch(adaptivePerformanceLevel)
    && isRenderBacklogIdle(diagnostics);
}

export function resolveNearbyPagePreviewWarmCandidates({
  visiblePageIndices,
  pageCount,
  layoutMode,
  limit = 2,
}: {
  visiblePageIndices: readonly number[];
  pageCount: number;
  layoutMode: 'continuous' | 'columns' | 'single-page';
  limit?: number;
}): number[] {
  if (visiblePageIndices.length === 0) {
    return [];
  }

  const visiblePageIndexSet = new Set(visiblePageIndices);
  const radius = layoutMode === 'columns' ? 1 : 2;
  const firstVisiblePageIndex = Math.min(...visiblePageIndices);
  const lastVisiblePageIndex = Math.max(...visiblePageIndices);
  const candidates: number[] = [];

  for (let offset = 1; offset <= radius; offset += 1) {
    const previousPageIndex = firstVisiblePageIndex - offset;
    const nextPageIndex = lastVisiblePageIndex + offset;
    if (previousPageIndex >= 0 && !visiblePageIndexSet.has(previousPageIndex)) {
      candidates.push(previousPageIndex);
    }
    if (nextPageIndex < pageCount && !visiblePageIndexSet.has(nextPageIndex)) {
      candidates.push(nextPageIndex);
    }
  }

  return [...new Set(candidates)].slice(0, Math.max(0, limit));
}

export function shouldAutoUpdateCurrentPageFromViewport(
  layoutMode: 'continuous' | 'columns' | 'single-page',
): boolean {
  return layoutMode === 'continuous';
}

export function shouldPreserveColumnAnchorAfterLayoutChange({
  previousLayoutMode,
  nextLayoutMode,
  previousCadViewOrganisation = 'columns',
  nextCadViewOrganisation = 'columns',
  previousPagesPerColumn,
  nextPagesPerColumn,
  previousZoom,
  nextZoom,
  didHandleColumnWheelZoom = false,
}: {
  previousLayoutMode: 'continuous' | 'columns' | 'single-page';
  nextLayoutMode: 'continuous' | 'columns' | 'single-page';
  previousCadViewOrganisation?: 'columns' | 'rows';
  nextCadViewOrganisation?: 'columns' | 'rows';
  previousPagesPerColumn: number;
  nextPagesPerColumn: number;
  previousZoom: number;
  nextZoom: number;
  didHandleColumnWheelZoom?: boolean;
}): boolean {
  return shouldPreserveViewportAnchorAfterLayoutChange({
    previousLayoutMode,
    nextLayoutMode,
    previousCadViewOrganisation,
    nextCadViewOrganisation,
    previousPagesPerColumn,
    nextPagesPerColumn,
    previousZoom,
    nextZoom,
    didHandleWheelZoom: didHandleColumnWheelZoom,
  });
}

export function shouldPreserveViewportAnchorAfterLayoutChange({
  previousLayoutMode,
  nextLayoutMode,
  previousCadViewOrganisation = 'columns',
  nextCadViewOrganisation = 'columns',
  previousPagesPerColumn,
  nextPagesPerColumn,
  previousZoom,
  nextZoom,
  didHandleWheelZoom = false,
}: {
  previousLayoutMode: 'continuous' | 'columns' | 'single-page';
  nextLayoutMode: 'continuous' | 'columns' | 'single-page';
  previousCadViewOrganisation?: 'columns' | 'rows';
  nextCadViewOrganisation?: 'columns' | 'rows';
  previousPagesPerColumn: number;
  nextPagesPerColumn: number;
  previousZoom: number;
  nextZoom: number;
  didHandleWheelZoom?: boolean;
}): boolean {
  const zoomChanged = Math.abs(previousZoom - nextZoom) > 0.001;
  if (didHandleWheelZoom && zoomChanged) {
    return false;
  }

  return previousLayoutMode !== nextLayoutMode
    || previousCadViewOrganisation !== nextCadViewOrganisation
    || previousPagesPerColumn !== nextPagesPerColumn
    || zoomChanged;
}

export function computePageLayoutGap(zoom: number): number {
  if (!Number.isFinite(zoom)) {
    return PAGE_LAYOUT_GAP;
  }

  return Number((PAGE_LAYOUT_GAP * Math.max(0.01, zoom)).toFixed(2));
}

export function resolveFitWidthZoom(viewportWidth: number, pageWidth: number): number {
  const rawFitZoom = viewportWidth / Math.max(1, pageWidth + PAGE_LAYOUT_GAP * 2);
  return clampViewerZoom(quantizeFitZoomDown(rawFitZoom));
}

export function materializeSinglePagePanPadding(
  layout: Pick<PageLayout, 'left' | 'top'>,
  gap: number,
  currentPadding: CanvasPadding,
): CanvasPadding {
  const horizontalCentering = Math.max(0, layout.left - currentPadding.left - gap);
  const verticalCentering = Math.max(0, layout.top - currentPadding.top - gap);

  return {
    left: currentPadding.left + horizontalCentering,
    right: currentPadding.right + horizontalCentering,
    top: currentPadding.top + verticalCentering,
    bottom: currentPadding.bottom + verticalCentering,
  };
}

export function materializeContinuousPanPadding(
  currentPadding: CanvasPadding,
  viewportSize: { readonly width: number; readonly height: number },
): CanvasPadding {
  const horizontalPanRoom = Math.max(0, viewportSize.width);
  const verticalPanRoom = Math.max(0, viewportSize.height);

  return {
    left: currentPadding.left + horizontalPanRoom,
    right: currentPadding.right + horizontalPanRoom,
    top: currentPadding.top + verticalPanRoom,
    bottom: currentPadding.bottom + verticalPanRoom,
  };
}

export function clampWheelZoomFrameDelta(deltaY: number): number {
  if (!Number.isFinite(deltaY)) {
    return 0;
  }

  return Math.max(-WHEEL_ZOOM_MAX_DELTA_PER_FRAME, Math.min(WHEEL_ZOOM_MAX_DELTA_PER_FRAME, deltaY));
}

export function shouldScrollViewportWheel(
  layoutMode: 'continuous' | 'columns' | 'single-page',
  scrollWheelMode: ScrollWheelMode,
  ctrlKey: boolean,
): boolean {
  if (layoutMode === 'columns') {
    return false;
  }

  return scrollWheelMode === 'scroll' ? !ctrlKey : ctrlKey;
}

export function computeBluebeamWheelZoom(currentZoom: number, deltaY: number): number {
  if (!Number.isFinite(currentZoom) || !Number.isFinite(deltaY)) {
    return clampViewportZoom(currentZoom);
  }

  return clampViewportZoom(currentZoom * Math.exp(-deltaY * BLUEBEAM_TRACKPAD_ZOOM_RATE));
}

export function resolvePageCenteredScroll(
  layout: Pick<PageLayout, 'left' | 'top' | 'width' | 'height'>,
  viewportWidth: number,
  viewportHeight: number,
  scrollWidth: number,
  scrollHeight: number,
): { left: number; top: number } {
  const maxScrollLeft = Math.max(0, scrollWidth - Math.max(0, viewportWidth));
  const maxScrollTop = Math.max(0, scrollHeight - Math.max(0, viewportHeight));
  return {
    left: clamp(layout.left + layout.width * 0.5 - Math.max(0, viewportWidth) * 0.5, 0, maxScrollLeft),
    top: clamp(layout.top + layout.height * 0.5 - Math.max(0, viewportHeight) * 0.5, 0, maxScrollTop),
  };
}

export function resolveCadOverviewCenteredView(
  pages: Array<{ index: number; width: number; height: number }>,
  zoom: number,
  viewportWidth: number,
  viewportHeight: number,
  gap: number,
  cadViewOrganisation: 'columns' | 'rows',
  pagesPerColumn: number,
  currentPageIndex: number,
): { canvasPadding: CanvasPadding; scroll: { left: number; top: number } } {
  const baseLayouts = buildPageLayouts(
    pages,
    zoom,
    viewportWidth,
    gap,
    EMPTY_CANVAS_PADDING,
    {
      mode: 'columns',
      cadViewOrganisation,
      pagesPerColumn,
      currentPageIndex,
      viewportHeight,
    },
  );
  const activeLayout = baseLayouts.layouts.find((layout) => layout.index === currentPageIndex);
  if (!activeLayout) {
    return {
      canvasPadding: EMPTY_CANVAS_PADDING,
      scroll: { left: 0, top: 0 },
    };
  }

  const activeCenterX = activeLayout.left + activeLayout.width * 0.5;
  const activeCenterY = activeLayout.top + activeLayout.height * 0.5;
  const halfViewportWidth = Math.max(0, viewportWidth) * 0.5;
  const halfViewportHeight = Math.max(0, viewportHeight) * 0.5;
  const canvasPadding = {
    left: Math.ceil(Math.max(0, halfViewportWidth - activeCenterX)),
    right: Math.ceil(Math.max(0, activeCenterX + halfViewportWidth - baseLayouts.totalWidth)),
    top: Math.ceil(Math.max(0, halfViewportHeight - activeCenterY)),
    bottom: Math.ceil(Math.max(0, activeCenterY + halfViewportHeight - baseLayouts.totalHeight)),
  };
  const paddedLayouts = buildPageLayouts(
    pages,
    zoom,
    viewportWidth,
    gap,
    canvasPadding,
    {
      mode: 'columns',
      cadViewOrganisation,
      pagesPerColumn,
      currentPageIndex,
      viewportHeight,
    },
  );
  const paddedActiveLayout = paddedLayouts.layouts.find((layout) => layout.index === currentPageIndex) ?? activeLayout;

  return {
    canvasPadding,
    scroll: resolvePageCenteredScroll(
      paddedActiveLayout,
      viewportWidth,
      viewportHeight,
      paddedLayouts.totalWidth,
      paddedLayouts.totalHeight,
    ),
  };
}

export function resolveAnchoredLayoutTransitionScroll(
  previousSnapshot: LayoutTransitionSnapshot,
  nextLayouts: readonly PageLayout[],
  viewportWidth: number,
  viewportHeight: number,
  scrollWidth: number,
  scrollHeight: number,
): AnchoredLayoutTransitionScroll | null {
  const anchor = captureViewportCentreAnchor(
    previousSnapshot.layouts,
    previousSnapshot.effectiveScrollLeft,
    previousSnapshot.effectiveScrollTop,
    previousSnapshot.viewportSize,
  );
  return anchor
    ? resolveViewportCentreAnchorScroll(anchor, nextLayouts, viewportWidth, viewportHeight, scrollWidth, scrollHeight)
    : null;
}

export function captureViewportCentreAnchor(
  layouts: readonly PageLayout[],
  effectiveScrollLeft: number,
  effectiveScrollTop: number,
  viewportSize: { readonly width: number; readonly height: number },
): ViewportCentreAnchor | null {
  if (layouts.length === 0) {
    return null;
  }

  const anchorContentX = effectiveScrollLeft + Math.max(0, viewportSize.width) * 0.5;
  const anchorContentY = effectiveScrollTop + Math.max(0, viewportSize.height) * 0.5;
  const anchorLayout = getLayoutAtPoint(layouts, anchorContentX, anchorContentY)
    ?? findNearestPageLayout(layouts, anchorContentX, anchorContentY);
  if (!anchorLayout) {
    return null;
  }

  const pageX = clamp(anchorContentX, anchorLayout.left, anchorLayout.left + anchorLayout.width);
  const pageY = clamp(anchorContentY, anchorLayout.top, anchorLayout.top + anchorLayout.height);

  return {
    pageIndex: anchorLayout.index,
    ratioX: (pageX - anchorLayout.left) / Math.max(1, anchorLayout.width),
    ratioY: (pageY - anchorLayout.top) / Math.max(1, anchorLayout.height),
  };
}

export function resolveViewportCentreAnchorScroll(
  anchor: ViewportCentreAnchor,
  nextLayouts: readonly PageLayout[],
  viewportWidth: number,
  viewportHeight: number,
  scrollWidth: number,
  scrollHeight: number,
): AnchoredLayoutTransitionScroll | null {
  const safeViewportWidth = Math.max(0, viewportWidth);
  const safeViewportHeight = Math.max(0, viewportHeight);
  const localX = safeViewportWidth * 0.5;
  const localY = safeViewportHeight * 0.5;
  const nextAnchorLayout = nextLayouts.find((layout) => layout.index === anchor.pageIndex);
  if (!nextAnchorLayout) {
    return null;
  }

  const nextAnchorContentX = nextAnchorLayout.left + anchor.ratioX * nextAnchorLayout.width;
  const nextAnchorContentY = nextAnchorLayout.top + anchor.ratioY * nextAnchorLayout.height;
  const maxScrollLeft = Math.max(0, scrollWidth - safeViewportWidth);
  const maxScrollTop = Math.max(0, scrollHeight - safeViewportHeight);
  const nextX = resolveAnchoredZoomAxis(nextAnchorContentX, localX, maxScrollLeft);
  const nextY = resolveAnchoredZoomAxis(nextAnchorContentY, localY, maxScrollTop);

  return {
    left: nextX.scrollOffset,
    top: nextY.scrollOffset,
    contentOffset: {
      x: nextX.contentOffset,
      y: nextY.contentOffset,
    },
  };
}

function ColumnOverviewCanvas({
  session,
  pages,
  layouts,
  previewUrls,
  strictVisiblePageIndices,
  pointerPageIndex,
  focusPageIndex,
  totalWidth,
  totalHeight,
  currentPage,
  zoom,
  onSelectPage,
}: {
  session: LocalPdfSession;
  pages: readonly PageModel[];
  layouts: readonly PageLayout[];
  previewUrls: ReadonlyMap<number, string>;
  strictVisiblePageIndices: ReadonlySet<number>;
  pointerPageIndex: number | null;
  focusPageIndex: number | null;
  totalWidth: number;
  totalHeight: number;
  currentPage: number;
  zoom: number;
  onSelectPage: (pageIndex: number) => void;
}) {
  const renderCoordinator = useRenderCoordinator(session);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [requestedPreviewUrls, setRequestedPreviewUrls] = useState<ReadonlyMap<number, string>>(() => new Map());
  const [imageVersion, setImageVersion] = useState(0);
  const requestedPreviewPagesRef = useRef(new Set<number>());
  const requestedPreviewWidthsRef = useRef(new Map<number, number>());
  const combinedPreviewUrlsRef = useRef<ReadonlyMap<number, string>>(new Map());
  const imageCacheRef = useRef(new Map<number, {
    image: HTMLImageElement;
    loaded: boolean;
    url: string;
  }>());
  const pageByIndex = useMemo(() => {
    return new Map(pages.map((page) => [page.index, page]));
  }, [pages]);
  const combinedPreviewUrls = useMemo(() => {
    const urls = new Map(requestedPreviewUrls);
    for (const [pageIndex, objectUrl] of previewUrls.entries()) {
      urls.set(pageIndex, objectUrl);
    }

    return urls;
  }, [previewUrls, requestedPreviewUrls]);

  useEffect(() => {
    combinedPreviewUrlsRef.current = combinedPreviewUrls;
  }, [combinedPreviewUrls]);

  useEffect(() => {
    requestedPreviewPagesRef.current.clear();
    requestedPreviewWidthsRef.current.clear();
    imageCacheRef.current.clear();
    setRequestedPreviewUrls(new Map());
    setImageVersion((version) => version + 1);
  }, [session]);

  useEffect(() => {
    const layoutPageIndices = new Set(layouts.map((layout) => layout.index));
    let changed = false;
    for (const pageIndex of requestedPreviewPagesRef.current) {
      if (!layoutPageIndices.has(pageIndex)) {
        requestedPreviewPagesRef.current.delete(pageIndex);
        requestedPreviewWidthsRef.current.delete(pageIndex);
      }
    }

    setRequestedPreviewUrls((current) => {
      const next = new Map<number, string>();
      for (const [pageIndex, objectUrl] of current.entries()) {
        if (layoutPageIndices.has(pageIndex)) {
          next.set(pageIndex, objectUrl);
        } else {
          changed = true;
        }
      }

      return changed ? next : current;
    });
  }, [layouts]);

  useEffect(() => {
    let cancelled = false;
    let timeoutId: number | null = null;

    const requestNextBatch = () => {
      if (cancelled) {
        return;
      }

      const availablePreviewUrls = combinedPreviewUrlsRef.current;
      const requiredPreviewWidths = resolveColumnOverviewPreviewRequiredWidths({
        layouts,
        pageByIndex,
        strictVisiblePageIndices,
        pointerPageIndex,
        focusPageIndex,
        session,
      });
      const availablePreviewPageIndices = new Set<number>();
      for (const layout of layouts) {
        const requiredWidth = requiredPreviewWidths.get(layout.index);
        if (!requiredWidth) {
          if (availablePreviewUrls.has(layout.index)) {
            availablePreviewPageIndices.add(layout.index);
          }
          continue;
        }

        if (session.getReusablePagePreviewInfo(
          layout.index,
          requiredWidth * 0.75,
          pageByIndex.get(layout.index)?.rotation,
        )) {
          availablePreviewPageIndices.add(layout.index);
        }
      }
      const batch = resolveColumnOverviewPreviewBatch({
        layouts,
        renderablePageIndices: new Set(pageByIndex.keys()),
        availablePreviewPageIndices,
        requestedPreviewPageIndices: requestedPreviewPagesRef.current,
        requestedPreviewWidths: requestedPreviewWidthsRef.current,
        requiredPreviewWidths,
        strictVisiblePageIndices,
        pointerPageIndex,
        focusPageIndex,
      });

      if (batch.length === 0) {
        return;
      }

      const hasStrictVisiblePreview = batch.some((layout) => strictVisiblePageIndices.has(layout.index));

      void Promise.allSettled(batch.map(async (layout, batchIndex) => {
        const page = pageByIndex.get(layout.index);
        if (!page) {
          return;
        }

        requestedPreviewPagesRef.current.add(layout.index);
        const isStrictlyVisible = strictVisiblePageIndices.has(layout.index);
        const previewBounds = getColumnOverviewPreviewBounds(page, layout, {
          isStrictlyVisible,
          isPointerPage: layout.index === pointerPageIndex,
          isFocusPage: layout.index === focusPageIndex,
        });
        requestedPreviewWidthsRef.current.set(layout.index, previewBounds.maxWidth);
        const objectUrl = await renderCoordinator.renderThumbnailUrl('overview-page', layout.index, previewBounds, 1, {
          priority: layout.index === pointerPageIndex
            ? 7000 - batchIndex
            : layout.index === focusPageIndex
              ? 6500 - batchIndex
            : isStrictlyVisible
              ? 6000 - batchIndex
              : 2000 - batchIndex,
          urgency: isStrictlyVisible ? 'visible' : 'prefetch',
          requestClass: 'overview-thumbnail',
          rotation: page.rotation,
        });
        if (cancelled) {
          return;
        }

        setRequestedPreviewUrls((current) => {
          if (current.get(layout.index) === objectUrl) {
            return current;
          }

          const next = new Map(current);
          next.set(layout.index, objectUrl);
          return next;
        });
      })).then(() => {
        if (!cancelled) {
          timeoutId = window.setTimeout(
            requestNextBatch,
            hasStrictVisiblePreview ? 0 : COLUMN_CANVAS_OVERVIEW_PREVIEW_BATCH_DELAY_MS,
          );
        }
      });
    };

    requestNextBatch();

    return () => {
      cancelled = true;
      if (timeoutId !== null) {
      window.clearTimeout(timeoutId);
      }
    };
  }, [focusPageIndex, layouts, pageByIndex, pointerPageIndex, renderCoordinator, session, strictVisiblePageIndices]);

  useEffect(() => {
    const cache = imageCacheRef.current;
    let changed = false;
    const allowedPageIndices = new Set(layouts.map((layout) => layout.index));

    for (const pageIndex of cache.keys()) {
      if (!allowedPageIndices.has(pageIndex)) {
        cache.delete(pageIndex);
        changed = true;
      }
    }

    for (const [pageIndex, objectUrl] of combinedPreviewUrls.entries()) {
      if (!allowedPageIndices.has(pageIndex)) {
        continue;
      }

      const cached = cache.get(pageIndex);
      if (cached?.url === objectUrl) {
        continue;
      }

      const image = new Image();
      image.decoding = 'async';
      cache.set(pageIndex, {
        image,
        loaded: false,
        url: objectUrl,
      });
      image.onload = () => {
        const entry = cache.get(pageIndex);
        if (entry?.url !== objectUrl) {
          return;
        }

        entry.loaded = true;
        setImageVersion((version) => version + 1);
      };
      image.src = objectUrl;
      changed = true;
    }

    if (changed) {
      setImageVersion((version) => version + 1);
    }
  }, [combinedPreviewUrls, layouts]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }

    if (strictVisiblePageIndices.size > 0) {
      let filledStrictVisibleCount = 0;
      let acceptableStrictVisibleCount = 0;
      for (const pageIndex of strictVisiblePageIndices) {
        const cachedImage = imageCacheRef.current.get(pageIndex);
        if (cachedImage?.loaded) {
          filledStrictVisibleCount += 1;
          const layout = layouts.find((candidate) => candidate.index === pageIndex);
          const page = pageByIndex.get(pageIndex);
          if (layout && page) {
            const requiredWidth = getColumnOverviewPreviewBounds(page, layout, {
              isStrictlyVisible: true,
              isPointerPage: pageIndex === pointerPageIndex,
              isFocusPage: pageIndex === focusPageIndex,
            }).maxWidth;
            const imageWidth = cachedImage.image.naturalWidth || cachedImage.image.width || 0;
            if (imageWidth >= requiredWidth * 0.75) {
              acceptableStrictVisibleCount += 1;
            }
            if (pageIndex === focusPageIndex) {
              recordOverviewFocusPreviewQuality({
                pageIndex,
                source: pageIndex === pointerPageIndex ? 'pointer' : 'viewport-focus',
                requiredWidth,
                renderedWidth: imageWidth,
              });
            }
          }
        }
      }
      recordOverviewVisiblePreviewFill(
        strictVisiblePageIndices.size,
        filledStrictVisibleCount,
        acceptableStrictVisibleCount,
      );
    }

	    const pixelRatio = Math.min(2, window.devicePixelRatio || 1);
    const width = Math.max(1, Math.ceil(totalWidth));
    const height = Math.max(1, Math.ceil(totalHeight));
    canvas.width = Math.ceil(width * pixelRatio);
    canvas.height = Math.ceil(height * pixelRatio);
    const context = canvas.getContext('2d');
    if (!context) {
      return;
    }

    context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    context.clearRect(0, 0, width, height);

    let cancelled = false;

    for (const layout of layouts) {
      drawOverviewTile(context, layout, {
        isCurrentPage: layout.index === currentPage,
      });

      const cachedImage = imageCacheRef.current.get(layout.index);
      if (cachedImage?.loaded) {
        drawOverviewPreviewImage(context, cachedImage.image, layout);
      }
    }
    return () => {
      cancelled = true;
    };
	  }, [currentPage, focusPageIndex, imageVersion, layouts, pageByIndex, pointerPageIndex, strictVisiblePageIndices, totalHeight, totalWidth, zoom]);

  const handleClick = useCallback((event: ReactPointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }

    const bounds = canvas.getBoundingClientRect();
    const scaleX = totalWidth / Math.max(1, bounds.width);
    const scaleY = totalHeight / Math.max(1, bounds.height);
    const x = (event.clientX - bounds.left) * scaleX;
    const y = (event.clientY - bounds.top) * scaleY;
    const layout = getLayoutAtPoint(layouts, x, y);
    if (layout) {
      onSelectPage(layout.index);
    }
  }, [layouts, onSelectPage, totalHeight, totalWidth]);

  return (
    <canvas
      ref={canvasRef}
      className="absolute left-0 top-0 z-10"
      data-testid="column-overview-canvas"
      onClick={handleClick}
      style={{
        width: `${totalWidth}px`,
        height: `${totalHeight}px`,
      }}
    />
  );
}

function drawOverviewTile(
  context: CanvasRenderingContext2D,
  layout: PageLayout,
  options: { isCurrentPage: boolean },
) {
  const radius = 0;
  drawRoundedRect(context, layout.left, layout.top, layout.width, layout.height, radius);
  context.fillStyle = '#f4f4f5';
  context.fill();
  context.strokeStyle = options.isCurrentPage ? '#3b82f6' : '#d4d4d8';
  context.lineWidth = options.isCurrentPage ? 2 : 1;
  context.stroke();

  const inset = Math.min(4, Math.max(2, layout.width * 0.08));
  const innerX = layout.left + inset;
  const innerY = layout.top + inset;
  const innerWidth = Math.max(1, layout.width - inset * 2);
  const innerHeight = Math.max(1, layout.height - inset * 2);
  drawRoundedRect(context, innerX, innerY, innerWidth, innerHeight, Math.max(2, radius - 2));
  context.fillStyle = '#e5e7eb';
  context.fill();
  context.strokeStyle = '#d4d4d8';
  context.lineWidth = 1;
  context.stroke();

}

function drawOverviewPreviewImage(
  context: CanvasRenderingContext2D,
  image: CanvasImageSource & { naturalWidth?: number; naturalHeight?: number },
  layout: PageLayout,
): void {
  const inset = Math.min(4, Math.max(2, layout.width * 0.08));
  const targetX = layout.left + inset;
  const targetY = layout.top + inset;
  const targetWidth = Math.max(1, layout.width - inset * 2);
  const targetHeight = Math.max(1, layout.height - inset * 2);
  const imageWidth = image.naturalWidth ?? targetWidth;
  const imageHeight = image.naturalHeight ?? targetHeight;
  const scale = Math.min(targetWidth / Math.max(1, imageWidth), targetHeight / Math.max(1, imageHeight));
  const drawWidth = Math.max(1, imageWidth * scale);
  const drawHeight = Math.max(1, imageHeight * scale);
  const drawX = targetX + (targetWidth - drawWidth) * 0.5;
  const drawY = targetY + (targetHeight - drawHeight) * 0.5;

  context.fillStyle = '#f4f4f5';
  context.fillRect(targetX, targetY, targetWidth, targetHeight);
  context.drawImage(image, drawX, drawY, drawWidth, drawHeight);
}

function drawRoundedRect(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) {
  const safeRadius = Math.min(radius, width * 0.5, height * 0.5);
  context.beginPath();
  context.moveTo(x + safeRadius, y);
  context.lineTo(x + width - safeRadius, y);
  context.quadraticCurveTo(x + width, y, x + width, y + safeRadius);
  context.lineTo(x + width, y + height - safeRadius);
  context.quadraticCurveTo(x + width, y + height, x + width - safeRadius, y + height);
  context.lineTo(x + safeRadius, y + height);
  context.quadraticCurveTo(x, y + height, x, y + height - safeRadius);
  context.lineTo(x, y + safeRadius);
  context.quadraticCurveTo(x, y, x + safeRadius, y);
  context.closePath();
}

function PageOverviewTile({
  session,
  page,
  layout,
  renderPreview,
  deferPreviewImage,
  renderPriority,
  label,
  isCurrentPage,
  isStrictlyVisible,
  isPointerPage,
  isFocusPage,
  onSelectPage,
}: {
  session: LocalPdfSession;
  page: PageModel;
  layout: PageLayout;
  renderPreview: boolean;
  deferPreviewImage: boolean;
  renderPriority: number;
  isCurrentPage: boolean;
  isStrictlyVisible: boolean;
  isPointerPage: boolean;
  isFocusPage: boolean;
  label: {
    pageNumber: number;
  };
  onSelectPage: (pageIndex: number) => void;
}) {
  const renderCoordinator = useRenderCoordinator(session);
  const [sourceUrl, setSourceUrl] = useState<string | null>(null);
  const [previewRetryTick, setPreviewRetryTick] = useState(0);
  const sourceUrlRef = useRef<string | null>(null);

  const replaceSourceUrl = (nextSourceUrl: string) => {
    if (sourceUrlRef.current === nextSourceUrl) {
      return;
    }

    session.retainPageImageUrl(nextSourceUrl);
    session.releasePageImageUrl(sourceUrlRef.current);
    sourceUrlRef.current = nextSourceUrl;
    setSourceUrl(nextSourceUrl);
  };

  const clearSourceUrl = () => {
    session.releasePageImageUrl(sourceUrlRef.current);
    sourceUrlRef.current = null;
    setSourceUrl(null);
  };

  const retryBrokenSourceUrl = () => {
    if (!shouldRetryBrokenPageImageSource(sourceUrlRef.current)) {
      return;
    }

    session.discardPageImageUrl(sourceUrlRef.current);
    sourceUrlRef.current = null;
    setSourceUrl(null);
    setPreviewRetryTick((tick) => tick + 1);
  };

  useEffect(() => {
    return () => {
      session.releasePageImageUrl(sourceUrlRef.current);
      sourceUrlRef.current = null;
    };
  }, [session]);

  useEffect(() => {
    if (!renderPreview) {
      return;
    }

    let cancelled = false;
    const previewBounds = getColumnOverviewPreviewBounds(page, layout, {
      isStrictlyVisible,
      isPointerPage,
      isFocusPage,
    });
    const minimumReusableWidth = previewBounds.maxWidth * 0.75;
    const reusablePreview = renderCoordinator.getReusablePreviewUrl(page.index, minimumReusableWidth, page.rotation);

    if (reusablePreview) {
      replaceSourceUrl(reusablePreview);
      return () => {
        cancelled = true;
      };
    }

    if (deferPreviewImage) {
      return () => {
        cancelled = true;
      };
    }

    void renderCoordinator.renderThumbnailUrl('overview-page', page.index, previewBounds, 1, {
      priority: isPointerPage ? renderPriority + 2000 : isFocusPage ? renderPriority + 1500 : renderPriority,
      urgency: 'visible',
      requestClass: 'overview-thumbnail',
      rotation: page.rotation,
    }).then((nextUrl) => {
      if (!cancelled) {
        replaceSourceUrl(nextUrl);
      }
    }).catch((error) => {
      if (!cancelled && !(error instanceof Error && error.name === 'AbortError')) {
        clearSourceUrl();
      }
    });

    return () => {
      cancelled = true;
    };
  }, [deferPreviewImage, isFocusPage, isPointerPage, isStrictlyVisible, layout, page, previewRetryTick, renderCoordinator, renderPreview, renderPriority, session]);

  useEffect(() => {
    if (!renderPreview) {
      return;
    }

    const previewBounds = getColumnOverviewPreviewBounds(page, layout, {
      isStrictlyVisible,
      isPointerPage,
      isFocusPage,
    });

    renderCoordinator.updateThumbnailPriority('overview-page', page.index, previewBounds, 1, {
      priority: isPointerPage ? renderPriority + 2000 : isFocusPage ? renderPriority + 1500 : renderPriority,
      urgency: 'visible',
      requestClass: 'overview-thumbnail',
      rotation: page.rotation,
    });
  }, [isFocusPage, isPointerPage, isStrictlyVisible, layout, page, renderCoordinator, renderPreview, renderPriority]);

  return (
    <div
      className={[
        'absolute border border-neutral-300 bg-neutral-100 transition-[border-color,box-shadow]',
        isCurrentPage ? 'bp-current-page-outline' : '',
      ].join(' ')}
      style={{
        top: `${layout.top}px`,
        left: `${layout.left}px`,
        width: `${layout.width}px`,
        height: `${layout.height}px`,
      }}
      data-page-index={layout.index}
      data-current-page={isCurrentPage ? 'true' : 'false'}
      data-pointer-page={isPointerPage ? 'true' : 'false'}
      data-focus-page={isFocusPage ? 'true' : 'false'}
      data-testid={`page-${layout.index + 1}`}
      data-overview-tile="true"
      onClick={() => onSelectPage(layout.index)}
    >
      <div className="absolute inset-[3px] border border-neutral-300 bg-neutral-200" />
      {sourceUrl ? (
        <img
          src={sourceUrl}
          alt=""
          className="absolute inset-0 h-full w-full object-contain"
          draggable={false}
          onError={retryBrokenSourceUrl}
          aria-hidden="true"
          data-testid={`page-overview-preview-${label.pageNumber}`}
        />
      ) : null}
    </div>
  );
}

function getColumnOverviewPreviewBounds(
  page: PageModel,
  layout?: Pick<PageLayout, 'width' | 'height'>,
  options: {
    isStrictlyVisible?: boolean;
    isPointerPage?: boolean;
    isFocusPage?: boolean;
  } = {},
) {
  const renderWidth = resolveColumnOverviewPreviewRenderWidth({
    displayWidth: layout?.width ?? COLUMN_OVERVIEW_PREVIEW_MAX_WIDTH_PX,
    isStrictlyVisible: Boolean(options.isStrictlyVisible),
    isPointerPage: Boolean(options.isPointerPage),
    isFocusPage: Boolean(options.isFocusPage),
  });
  const pageAspect = page.size.height / Math.max(1, page.size.width);
  return {
    maxWidth: renderWidth,
    maxHeight: Math.max(COLUMN_OVERVIEW_PREVIEW_MAX_HEIGHT_PX, Math.ceil(renderWidth * pageAspect)),
    pageWidth: page.size.width,
    pageHeight: page.size.height,
    pixelRatio: 1,
    minScale: COLUMN_OVERVIEW_PREVIEW_MIN_SCALE,
  };
}

function resolveColumnOverviewPreviewRequiredWidths({
  layouts,
  pageByIndex,
  strictVisiblePageIndices,
  pointerPageIndex,
  focusPageIndex,
  session,
}: {
  layouts: readonly PageLayout[];
  pageByIndex: ReadonlyMap<number, PageModel>;
  strictVisiblePageIndices: ReadonlySet<number>;
  pointerPageIndex: number | null;
  focusPageIndex: number | null;
  session: LocalPdfSession;
}): Map<number, number> {
  const requiredWidths = new Map<number, number>();
  for (const layout of layouts) {
    if (!strictVisiblePageIndices.has(layout.index) && layout.index !== pointerPageIndex) {
      continue;
    }

    const page = pageByIndex.get(layout.index);
    if (!page) {
      continue;
    }

    const previewBounds = getColumnOverviewPreviewBounds(page, layout, {
      isStrictlyVisible: strictVisiblePageIndices.has(layout.index),
      isPointerPage: layout.index === pointerPageIndex,
      isFocusPage: layout.index === focusPageIndex,
    });
    if (!session.getReusablePagePreviewInfo(layout.index, previewBounds.maxWidth * 0.75, page.rotation)) {
      requiredWidths.set(layout.index, previewBounds.maxWidth);
    }
  }

  return requiredWidths;
}

export function resolveColumnOverviewPreviewRenderWidth({
  displayWidth,
  isStrictlyVisible,
  isPointerPage,
  isFocusPage,
}: {
  displayWidth: number;
  isStrictlyVisible: boolean;
  isPointerPage: boolean;
  isFocusPage?: boolean;
}): number {
  const safeDisplayWidth = Number.isFinite(displayWidth) ? Math.max(1, displayWidth) : 1;
  const scale = isPointerPage
    ? COLUMN_OVERVIEW_POINTER_PREVIEW_RENDER_SCALE
    : isFocusPage
      ? COLUMN_OVERVIEW_FOCUS_PREVIEW_RENDER_SCALE
    : isStrictlyVisible
      ? COLUMN_OVERVIEW_VISIBLE_PREVIEW_RENDER_SCALE
      : 1;
  const targetWidth = isStrictlyVisible || isPointerPage || isFocusPage
    ? safeDisplayWidth * scale
    : COLUMN_OVERVIEW_PREVIEW_MIN_RENDER_WIDTH_PX;

  return Math.round(Math.max(
    COLUMN_OVERVIEW_PREVIEW_MIN_RENDER_WIDTH_PX,
    Math.min(COLUMN_OVERVIEW_PREVIEW_MAX_RENDER_WIDTH_PX, targetWidth),
  ));
}

function getViewportAnchorIndex(
  layouts: readonly PageLayout[],
  scrollLeft: number,
  scrollTop: number,
  viewportSize: { width: number; height: number },
  fallbackIndex: number,
): number {
  return getViewportAnchorLayout(layouts, scrollLeft, scrollTop, viewportSize)?.index ?? fallbackIndex;
}

export function getContinuousCurrentPageLayout(
  layouts: readonly PageLayout[],
  scrollLeft: number,
  previousScrollTop: number,
  scrollTop: number,
  viewportSize: { width: number; height: number },
  scrollDirection: ScrollDirection,
): PageLayout | null {
  if (layouts.length === 0) {
    return null;
  }

  if (scrollDirection === 'none' || previousScrollTop === scrollTop) {
    return null;
  }

  const viewportWidth = Math.max(0, viewportSize.width);
  const viewportHeight = Math.max(0, viewportSize.height);
  if (viewportWidth <= 0 || viewportHeight <= 0) {
    return null;
  }

  const viewportLeft = scrollLeft;
  const viewportRight = scrollLeft + viewportWidth;
  const viewportTop = scrollTop;
  const viewportBottom = scrollTop + viewportHeight;
  let majorityLayout: PageLayout | null = null;
  let largestVisibleHeight = 0;
  for (const layout of layouts) {
    const visibleLeft = Math.max(viewportLeft, layout.left);
    const visibleRight = Math.min(viewportRight, layout.left + layout.width);
    if (visibleRight <= visibleLeft) {
      continue;
    }

    const visibleTop = Math.max(viewportTop, layout.top);
    const visibleBottom = Math.min(viewportBottom, layout.top + layout.height);
    const visibleHeight = Math.max(0, visibleBottom - visibleTop);
    if (visibleHeight <= viewportHeight * 0.5 || visibleHeight <= largestVisibleHeight) {
      continue;
    }

    majorityLayout = layout;
    largestVisibleHeight = visibleHeight;
  }

  return majorityLayout;
}

function getViewportAnchorLayout(
  layouts: readonly PageLayout[],
  scrollLeft: number,
  scrollTop: number,
  viewportSize: { width: number; height: number },
): PageLayout | null {
  if (layouts.length === 0) {
    return null;
  }

  const anchorX = scrollLeft + Math.max(0, viewportSize.width) * 0.5;
  const anchorY = scrollTop + Math.max(0, viewportSize.height) * 0.5;
  return getLayoutAtPoint(layouts, anchorX, anchorY) ?? getNearestLayoutToPoint(layouts, anchorX, anchorY);
}

function getNearestLayoutToPoint(layouts: readonly PageLayout[], x: number, y: number): PageLayout | null {
  let nearestLayout: PageLayout | null = null;
  let nearestDistanceSquared = Number.POSITIVE_INFINITY;
  for (const layout of layouts) {
    const layoutCenterX = layout.left + layout.width * 0.5;
    const layoutCenterY = layout.top + layout.height * 0.5;
    const distanceSquared = (layoutCenterX - x) ** 2 + (layoutCenterY - y) ** 2;
    if (distanceSquared < nearestDistanceSquared) {
      nearestLayout = layout;
      nearestDistanceSquared = distanceSquared;
    }
  }

  return nearestLayout;
}

function getLayoutAtPoint(layouts: readonly PageLayout[], x: number, y: number): PageLayout | null {
  for (const layout of layouts) {
    if (
      x >= layout.left
      && x <= layout.left + layout.width
      && y >= layout.top
      && y <= layout.top + layout.height
    ) {
      return layout;
    }
  }

  return null;
}

function areNumberArraysEqual(left: readonly number[], right: readonly number[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) {
      return false;
    }
  }

  return true;
}

function clampViewportZoom(zoom: number): number {
  return clampViewerZoom(zoom);
}

function getWheelZoomAnchor(
  layouts: readonly PageLayout[],
  zoom: number,
  contentX: number,
  contentY: number,
): WheelZoomAnchor {
  const pageLayout = findNearestPageLayout(layouts, contentX, contentY);

  if (!pageLayout) {
    return { kind: 'content', contentX, contentY };
  }

  return {
    kind: 'page',
    pageIndex: pageLayout.index,
    pageX: (contentX - pageLayout.left) / Math.max(zoom, Number.EPSILON),
    pageY: (contentY - pageLayout.top) / Math.max(zoom, Number.EPSILON),
  };
}

function findNearestPageLayout(
  layouts: readonly PageLayout[],
  contentX: number,
  contentY: number,
): PageLayout | null {
  let nearestLayout: PageLayout | null = null;
  let nearestDistanceSquared = Number.POSITIVE_INFINITY;

  for (const layout of layouts) {
    const nearestX = clamp(contentX, layout.left, layout.left + layout.width);
    const nearestY = clamp(contentY, layout.top, layout.top + layout.height);
    const distanceSquared = (contentX - nearestX) ** 2 + (contentY - nearestY) ** 2;
    if (distanceSquared < nearestDistanceSquared) {
      nearestLayout = layout;
      nearestDistanceSquared = distanceSquared;
    }
  }

  return nearestLayout;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function getAnchorContentPosition(
  anchor: WheelZoomAnchor,
  layouts: readonly PageLayout[],
  zoom: number,
  fallbackZoomRatio: number,
): { x: number; y: number } {
  if (anchor.kind === 'content') {
    return {
      x: anchor.contentX * fallbackZoomRatio,
      y: anchor.contentY * fallbackZoomRatio,
    };
  }

  const layout = layouts.find((candidate) => candidate.index === anchor.pageIndex);
  if (!layout) {
    return { x: 0, y: 0 };
  }

  return {
    x: layout.left + anchor.pageX * zoom,
    y: layout.top + anchor.pageY * zoom,
  };
}

function resolveColumnWheelScroll(
  layouts: readonly PageLayout[],
  scrollLeft: number,
  scrollTop: number,
  viewportWidth: number,
  viewportHeight: number,
  scrollWidth: number,
  scrollHeight: number,
  deltaX: number,
  deltaY: number,
  gap: number,
): { left: number; top: number } {
  if (layouts.length === 0) {
    return { left: scrollLeft + deltaX, top: scrollTop + deltaY };
  }

  if (Math.abs(deltaX) > Math.abs(deltaY)) {
    return {
      left: scrollLeft + deltaX,
      top: scrollTop,
    };
  }

  const columns = getLayoutColumns(layouts, gap);
  const currentColumn = getNearestColumn(columns, scrollLeft + gap * 2);
  if (!currentColumn) {
    return { left: scrollLeft + deltaX, top: scrollTop + deltaY };
  }

  const maxScrollLeft = Math.max(0, scrollWidth - viewportWidth);
  const maxScrollTop = Math.max(0, scrollHeight - viewportHeight);
  const nextColumnState = walkColumnWheelDelta(
    columns,
    currentColumn.index,
    scrollTop,
    viewportHeight,
    deltaY,
    gap,
  );

  return {
    left: clamp(nextColumnState.column.left - gap, 0, maxScrollLeft),
    top: clamp(nextColumnState.top, 0, maxScrollTop),
  };
}

interface LayoutColumn {
  readonly index: number;
  readonly left: number;
  readonly right: number;
  readonly top: number;
  readonly bottom: number;
}

function getLayoutColumns(layouts: readonly PageLayout[], gap: number): readonly LayoutColumn[] {
  const columnMap = new Map<number, LayoutColumn>();

  for (const layout of layouts) {
    const current = columnMap.get(layout.columnIndex);
    const nextColumn = {
      index: layout.columnIndex,
      left: current ? Math.min(current.left, layout.left) : layout.left,
      right: current ? Math.max(current.right, layout.left + layout.width) : layout.left + layout.width,
      top: current ? Math.min(current.top, layout.top) : layout.top,
      bottom: current ? Math.max(current.bottom, layout.top + layout.height) : layout.top + layout.height,
    };
    columnMap.set(layout.columnIndex, nextColumn);
  }

  return Array.from(columnMap.values())
    .sort((left, right) => left.index - right.index)
    .map((column) => ({
      ...column,
      top: Math.max(0, column.top - gap),
      bottom: column.bottom + gap,
    }));
}

function getNearestColumn(columns: readonly LayoutColumn[], viewportCenterX: number): LayoutColumn | null {
  let nearestColumn: LayoutColumn | null = null;
  let nearestDistance = Number.POSITIVE_INFINITY;

  for (const column of columns) {
    const columnCenterX = (column.left + column.right) * 0.5;
    const distance = Math.abs(columnCenterX - viewportCenterX);
    if (distance < nearestDistance) {
      nearestColumn = column;
      nearestDistance = distance;
    }
  }

  return nearestColumn;
}

function walkColumnWheelDelta(
  columns: readonly LayoutColumn[],
  columnIndex: number,
  scrollTop: number,
  viewportHeight: number,
  deltaY: number,
  gap: number,
): { column: LayoutColumn; top: number } {
  let activeColumnIndex = Math.min(columns.length - 1, Math.max(0, columnIndex));
  let nextTop = scrollTop;
  let remainingDelta = deltaY;

  while (Math.abs(remainingDelta) > Number.EPSILON) {
    const column = columns[activeColumnIndex] ?? columns[0];
    const columnMinTop = column.top;
    const columnMaxTop = Math.max(columnMinTop, column.bottom - viewportHeight);

    if (remainingDelta > 0) {
      const availableInColumn = Math.max(0, columnMaxTop - nextTop);
      if (remainingDelta <= availableInColumn || activeColumnIndex >= columns.length - 1) {
        nextTop += remainingDelta;
        break;
      }

      remainingDelta -= availableInColumn;
      activeColumnIndex += 1;
      nextTop = columns[activeColumnIndex]?.top ?? columnMaxTop;
      remainingDelta = Math.max(0, remainingDelta - gap);
      continue;
    }

    const availableInColumn = Math.max(0, nextTop - columnMinTop);
    const upwardDelta = Math.abs(remainingDelta);
    if (upwardDelta <= availableInColumn || activeColumnIndex <= 0) {
      nextTop += remainingDelta;
      break;
    }

    remainingDelta += availableInColumn;
    activeColumnIndex -= 1;
    const previousColumn = columns[activeColumnIndex] ?? column;
    nextTop = Math.max(previousColumn.top, previousColumn.bottom - viewportHeight);
    remainingDelta = Math.min(0, remainingDelta + gap);
  }

  const activeColumn = columns[activeColumnIndex] ?? columns[0];
  return {
    column: activeColumn,
    top: clamp(nextTop, activeColumn.top, Math.max(activeColumn.top, activeColumn.bottom - viewportHeight)),
  };
}

export function resolveViewerLayoutMode(
  scrollMode: 'continuous' | 'single-page',
  pageColumnsEnabled: boolean,
): 'continuous' | 'columns' | 'single-page' {
  return scrollMode === 'single-page'
    ? 'single-page'
    : pageColumnsEnabled
      ? 'columns'
      : 'continuous';
}

export function resolveMinimumZoomPanBounds(
  layouts: readonly PageLayout[],
  viewportLength: number,
  maxScroll: number,
  zoom: number,
  axis: 'x' | 'y',
  minimumZoom = MIN_VIEWER_ZOOM,
  minimumVisibleRatio = MINIMUM_ZOOM_PAN_VISIBLE_RATIO,
): { min: number; max: number } | null {
  if (layouts.length === 0 || viewportLength <= 0 || maxScroll < 0 || zoom <= 0) {
    return null;
  }

  const targetLayout = layouts.reduce((largest, layout) => (
    getLayoutAxisLength(layout, axis) > getLayoutAxisLength(largest, axis) ? layout : largest
  ), layouts[0]);
  if (!targetLayout) {
    return null;
  }

  const layoutStart = axis === 'x' ? targetLayout.left : targetLayout.top;
  const layoutLength = getLayoutAxisLength(targetLayout, axis);
  const minimumVisibleLength = layoutLength
    * clamp(minimumZoom / zoom, 0, 1)
    * clamp(minimumVisibleRatio, 0, 1);
  const min = layoutStart + minimumVisibleLength - viewportLength;
  const max = layoutStart + layoutLength - minimumVisibleLength;
  return min <= max ? { min, max } : { min: max, max: min };
}

export function resolveViewportPanBounds(
  layouts: readonly PageLayout[],
  viewportLength: number,
  maxScroll: number,
  zoom: number,
  axis: 'x' | 'y',
  layoutMode: 'continuous' | 'columns' | 'single-page',
  minimumZoom = MIN_VIEWER_ZOOM,
  minimumVisibleRatio = MINIMUM_ZOOM_PAN_VISIBLE_RATIO,
): { min: number; max: number } | null {
  if (layoutMode === 'columns') {
    return null;
  }

  if (layoutMode !== 'continuous' || axis === 'x') {
    return resolveMinimumZoomPanBounds(
      layouts,
      viewportLength,
      maxScroll,
      zoom,
      axis,
      minimumZoom,
      minimumVisibleRatio,
    );
  }

  if (layouts.length === 0 || viewportLength <= 0 || maxScroll < 0 || zoom <= 0) {
    return null;
  }

  const firstLayout = layouts.reduce((first, layout) => (
    layout.top < first.top ? layout : first
  ), layouts[0]);
  const lastLayout = layouts.reduce((last, layout) => (
    layout.top + layout.height > last.top + last.height ? layout : last
  ), layouts[0]);
  if (!firstLayout || !lastLayout) {
    return null;
  }

  const visibleScale = clamp(minimumZoom / zoom, 0, 1) * clamp(minimumVisibleRatio, 0, 1);
  const min = firstLayout.top + firstLayout.height * visibleScale - viewportLength;
  const max = lastLayout.top + lastLayout.height * (1 - visibleScale);
  return min <= max ? { min, max } : { min: max, max: min };
}

export function constrainViewportScrollToPanBounds(
  layouts: readonly PageLayout[],
  viewportWidth: number,
  viewportHeight: number,
  maxScrollLeft: number,
  maxScrollTop: number,
  zoom: number,
  layoutMode: 'continuous' | 'columns' | 'single-page',
  scrollLeft: number,
  scrollTop: number,
  contentOffset: ViewportContentOffset = { x: 0, y: 0 },
): { left: number; top: number } {
  const horizontalBounds = resolveViewportPanBounds(
    layouts,
    viewportWidth,
    maxScrollLeft,
    zoom,
    'x',
    layoutMode,
  );
  const verticalBounds = resolveViewportPanBounds(
    layouts,
    viewportHeight,
    maxScrollTop,
    zoom,
    'y',
    layoutMode,
  );
  return {
    left: constrainScrollAxis(scrollLeft, maxScrollLeft, horizontalBounds, contentOffset.x),
    top: constrainScrollAxis(scrollTop, maxScrollTop, verticalBounds, contentOffset.y),
  };
}

function constrainScrollAxis(
  scrollOffset: number,
  maxScroll: number,
  bounds: { min: number; max: number } | null,
  contentOffset: number,
): number {
  const safeMaxScroll = Math.max(0, maxScroll);
  if (!bounds) {
    return clamp(scrollOffset, 0, safeMaxScroll);
  }

  const min = clamp(bounds.min + contentOffset, 0, safeMaxScroll);
  const max = clamp(bounds.max + contentOffset, min, safeMaxScroll);
  return clamp(scrollOffset, min, max);
}

function getLayoutAxisLength(layout: PageLayout, axis: 'x' | 'y'): number {
  return axis === 'x' ? layout.width : layout.height;
}

function preventMiddleClickDefault(event: MouseEvent): void {
  if (event.button === 1) {
    event.preventDefault();
  }
}

export function resolveMiddleDoubleClickZoomPreset(
  currentPreset: ZoomPreset,
  layoutMode: 'continuous' | 'columns' | 'single-page' = 'single-page',
): ZoomPreset {
  if (currentPreset !== 'manual') {
    return currentPreset;
  }

  return layoutMode === 'continuous' ? 'fit-width' : 'fit-page';
}

function isMiddleDoubleClick(event: PointerEvent, previousClick: MiddleClickState | null): boolean {
  if (!previousClick) {
    return false;
  }

  const elapsedMs = event.timeStamp - previousClick.timeStamp;
  if (elapsedMs < 0 || elapsedMs > MIDDLE_DOUBLE_CLICK_MS) {
    return false;
  }

  return distanceBetweenPoints(event.clientX, event.clientY, previousClick.clientX, previousClick.clientY)
    <= MIDDLE_DOUBLE_CLICK_TOLERANCE_PX;
}

function hasExceededMiddleClickDragTolerance(event: PointerEvent, panState: ViewportPanState): boolean {
  return distanceBetweenPoints(event.clientX, event.clientY, panState.startClientX, panState.startClientY)
    > MIDDLE_CLICK_DRAG_TOLERANCE_PX;
}

export function isViewportPanButtonPressed(buttons: number, button: number): boolean {
  if (button === 0) {
    return (buttons & 1) !== 0;
  }
  if (button === 1) {
    return (buttons & 4) !== 0;
  }
  if (button === 2) {
    return (buttons & 2) !== 0;
  }

  return buttons !== 0;
}

function distanceBetweenPoints(leftX: number, leftY: number, rightX: number, rightY: number): number {
  return Math.hypot(leftX - rightX, leftY - rightY);
}
