import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { ItemGroup } from '@/components/ui/item';
import { getPageScale, type Markup, type PageModel, type PageRotationDirection } from '@butter-paper/core';
import { isRenderBacklogIdle, type DiagnosticsSnapshot, type LocalPdfSession } from '../services/documentSession';
import { useRenderCoordinator } from '../services/renderCoordinator';
import { useSessionVersion } from '../services/sessionHooks';
import { recordComponentRender, recordEvent } from '../services/perfTracker';
import { useViewerStore } from '../state/viewerStore';
import {
  buildThumbnailLayouts,
  computeThumbnailPreviewWidth,
  computeVisibleThumbnailRange,
} from '../utils/thumbnailLayout';
import { capThumbnailPixelRatio } from '../utils/renderZoom';
import { CustomScrollArea } from './CustomScrollArea';
import { PageThumbnailItem } from './PageThumbnailItem';

const THUMBNAIL_VISIBLE_OVERSCAN = 6;
const THUMBNAIL_MOTION_SETTLE_MS = 180;
const THUMBNAIL_ACTIVE_SCROLL_MARGIN_PX = 24;
const QUICK_VISIBLE_THUMBNAIL_WIDTH = 64;
const EMPTY_MARKUPS: readonly Markup[] = [];
export const PAGE_THUMBNAIL_VIEWPORT_CLASS_NAME = 'overflow-y-auto overflow-x-hidden px-0 py-0';

interface PageThumbnailListProps {
  session: LocalPdfSession;
  pages: readonly PageModel[];
  mutationDisabled?: boolean;
  onSelectPage: (pageIndex: number, source?: 'thumbnail', previewUrl?: string | null) => void;
  onSetPageScale: (pageIndex: number) => void;
  onRotatePage: (pageIndex: number, direction: PageRotationDirection) => void;
}

export function PageThumbnailList({ session, pages, mutationDisabled = false, onSelectPage, onSetPageScale, onRotatePage }: PageThumbnailListProps) {
  recordComponentRender('PageThumbnailList');
  const renderCoordinator = useRenderCoordinator(session);
  const currentPage = useViewerStore((state) => state.currentPage);
  const pendingThumbnailScroll = useViewerStore((state) => state.pendingThumbnailScroll);
  const consumeThumbnailScroll = useViewerStore((state) => state.consumeThumbnailScroll);
  const documentMarkups = useViewerStore((state) => state.document?.document.markups ?? EMPTY_MARKUPS);
  const documentPageScales = useViewerStore((state) => state.document?.document.pageScales ?? []);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);
  const [viewportWidth, setViewportWidth] = useState(0);
  const [thumbnailListInMotion, setThumbnailListInMotionState] = useState(false);
  const resizeFrameRef = useRef<number | null>(null);
  const scrollFrameRef = useRef<number | null>(null);
  const thumbnailMotionTimeoutRef = useRef<number | null>(null);
  const sessionRef = useRef(session);
  const pendingViewportMetricsRef = useRef({ width: 0, height: 0 });
  const pendingScrollTopRef = useRef(0);
  const sessionVersion = useSessionVersion(session);

  useEffect(() => {
    sessionRef.current = session;
  }, [session]);

  const markThumbnailListInMotion = () => {
    sessionRef.current.setThumbnailListInMotion(true);
    setThumbnailListInMotionState(true);
    if (thumbnailMotionTimeoutRef.current !== null) {
      window.clearTimeout(thumbnailMotionTimeoutRef.current);
    }
    thumbnailMotionTimeoutRef.current = window.setTimeout(() => {
      thumbnailMotionTimeoutRef.current = null;
      sessionRef.current.setThumbnailListInMotion(false);
      setThumbnailListInMotionState(false);
    }, THUMBNAIL_MOTION_SETTLE_MS);
  };

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const applyViewportMetrics = () => {
      const nextViewportMetrics = {
        height: Math.round(container.clientHeight),
        width: Math.round(container.clientWidth),
      };
      pendingViewportMetricsRef.current = nextViewportMetrics;
      setViewportHeight((current) => (current === nextViewportMetrics.height ? current : nextViewportMetrics.height));
      setViewportWidth((current) => (current === nextViewportMetrics.width ? current : nextViewportMetrics.width));
    };

    const observer = new ResizeObserver((entries) => {
      recordEvent('PageThumbnailList.resizeObserver');
      const entry = entries[0];
      if (!entry) {
        return;
      }

      pendingViewportMetricsRef.current = {
        height: Math.round(entry.contentRect.height),
        width: Math.round(entry.contentRect.width),
      };

      if (resizeFrameRef.current !== null) {
        return;
      }

      resizeFrameRef.current = window.requestAnimationFrame(() => {
        resizeFrameRef.current = null;
        const nextViewportMetrics = pendingViewportMetricsRef.current;
        setViewportHeight((current) => (current === nextViewportMetrics.height ? current : nextViewportMetrics.height));
        setViewportWidth((current) => (current === nextViewportMetrics.width ? current : nextViewportMetrics.width));
      });
    });

    const handleScroll = () => {
      recordEvent('PageThumbnailList.scroll');
      markThumbnailListInMotion();
      pendingScrollTopRef.current = container.scrollTop;

      if (scrollFrameRef.current !== null) {
        return;
      }

      scrollFrameRef.current = window.requestAnimationFrame(() => {
        scrollFrameRef.current = null;
        const nextScrollTop = pendingScrollTopRef.current;
        setScrollTop((current) => (current === nextScrollTop ? current : nextScrollTop));
      });
    };

    applyViewportMetrics();
    pendingScrollTopRef.current = container.scrollTop;
    setScrollTop((current) => (current === container.scrollTop ? current : container.scrollTop));

    observer.observe(container);
    container.addEventListener('scroll', handleScroll, { passive: true });

    return () => {
      observer.disconnect();
      container.removeEventListener('scroll', handleScroll);
      if (resizeFrameRef.current !== null) {
        window.cancelAnimationFrame(resizeFrameRef.current);
        resizeFrameRef.current = null;
      }
      if (scrollFrameRef.current !== null) {
        window.cancelAnimationFrame(scrollFrameRef.current);
        scrollFrameRef.current = null;
      }
      if (thumbnailMotionTimeoutRef.current !== null) {
        window.clearTimeout(thumbnailMotionTimeoutRef.current);
        thumbnailMotionTimeoutRef.current = null;
      }
      sessionRef.current.setThumbnailListInMotion(false);
      setThumbnailListInMotionState(false);
    };
  }, []);

  const previewWidth = useMemo(() => computeThumbnailPreviewWidth(viewportWidth), [viewportWidth]);
  const { layouts, totalHeight } = useMemo(() => buildThumbnailLayouts(pages, previewWidth), [pages, previewWidth]);
  const visibleRange = useMemo(
    () => computeVisibleThumbnailRange(layouts, scrollTop, viewportHeight, THUMBNAIL_VISIBLE_OVERSCAN, 0),
    [layouts, scrollTop, viewportHeight],
  );
  const strictVisibleRange = useMemo(
    () => computeVisibleThumbnailRange(layouts, scrollTop, viewportHeight, 0, 0),
    [layouts, scrollTop, viewportHeight],
  );
  const visiblePriorityAnchor = useMemo(() => {
    return getRangeAnchorIndex(layouts, strictVisibleRange, currentPage);
  }, [currentPage, layouts, strictVisibleRange]);
  const markupsByPage = useMemo(() => {
    const grouped = new Map<number, Markup[]>();
    for (const markup of documentMarkups) {
      const pageMarkups = grouped.get(markup.pageIndex);
      if (pageMarkups) {
        pageMarkups.push(markup);
      } else {
        grouped.set(markup.pageIndex, [markup]);
      }
    }
    return grouped;
  }, [documentMarkups]);

  useEffect(() => {
    const container = containerRef.current;
    if (!pendingThumbnailScroll || !container || viewportHeight <= 0 || layouts.length === 0) {
      return;
    }

    const activeLayout = layouts.find((layout) => layout.index === pendingThumbnailScroll.pageIndex);
    if (!activeLayout) {
      consumeThumbnailScroll(pendingThumbnailScroll.requestId);
      return;
    }

    const margin = Math.min(THUMBNAIL_ACTIVE_SCROLL_MARGIN_PX, Math.max(0, viewportHeight * 0.2));
    const viewportTop = container.scrollTop;
    const viewportBottom = viewportTop + viewportHeight;
    const activeTop = activeLayout.top;
    const activeBottom = activeLayout.top + activeLayout.itemHeight;
    let nextScrollTop: number | null = null;

    if (activeTop < viewportTop + margin) {
      nextScrollTop = activeTop - margin;
    } else if (activeBottom > viewportBottom - margin) {
      nextScrollTop = activeBottom - viewportHeight + margin;
    }

    if (nextScrollTop === null) {
      consumeThumbnailScroll(pendingThumbnailScroll.requestId);
      return;
    }

    const maxScrollTop = Math.max(0, totalHeight - viewportHeight);
    const clampedScrollTop = Math.max(0, Math.min(maxScrollTop, nextScrollTop));
    if (Math.abs(clampedScrollTop - container.scrollTop) < 1) {
      consumeThumbnailScroll(pendingThumbnailScroll.requestId);
      return;
    }

    container.scrollTo({ top: clampedScrollTop, behavior: 'auto' });
    const frameId = window.requestAnimationFrame(() => {
      consumeThumbnailScroll(pendingThumbnailScroll.requestId);
    });
    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [consumeThumbnailScroll, layouts, pendingThumbnailScroll, totalHeight, viewportHeight]);

  useEffect(() => {
    const diagnostics = session.diagnostics();
    if (!shouldWarmNearbyThumbnails({
      diagnostics,
      thumbnailListInMotion,
      hasStrictVisibleThumbnails: strictVisibleRange.endIndex >= strictVisibleRange.startIndex,
    })) {
      return;
    }

    const candidates = resolveNearbyThumbnailWarmCandidates({
      startIndex: strictVisibleRange.startIndex,
      endIndex: strictVisibleRange.endIndex,
      pageCount: pages.length,
    });

    const abortController = new AbortController();
    for (const layoutIndex of candidates) {
      const page = pages[layoutIndex];
      const layout = layouts[layoutIndex];
      if (!page || !layout || session.getReusablePagePreviewInfo(page.index, 0, page.rotation)) {
        continue;
      }

      void renderCoordinator.renderThumbnailUrl('sidebar-thumbnail', page.index, {
        maxWidth: previewWidth,
        maxHeight: layout.previewHeight,
        pageWidth: page.size.width,
        pageHeight: page.size.height,
      }, capThumbnailPixelRatio(window.devicePixelRatio || 1), {
        priority: 50 - Math.abs(page.index - currentPage),
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
    layouts,
    pages,
    previewWidth,
    renderCoordinator,
    session,
    sessionVersion,
    strictVisibleRange.endIndex,
    strictVisibleRange.startIndex,
    thumbnailListInMotion,
  ]);

  useEffect(() => {
    if (visibleRange.endIndex < visibleRange.startIndex) {
      return;
    }

    for (let layoutIndex = visibleRange.startIndex; layoutIndex <= visibleRange.endIndex; layoutIndex += 1) {
      const page = pages[layoutIndex];
      const layout = layouts[layoutIndex];
      if (!page || !layout) {
        continue;
      }

      const isStrictlyVisible =
        layoutIndex >= strictVisibleRange.startIndex && layoutIndex <= strictVisibleRange.endIndex;
      if (isStrictlyVisible) {
        void renderCoordinator.renderThumbnailUrl('sidebar-thumbnail', page.index, {
          maxWidth: previewWidth,
          maxHeight: layout.previewHeight,
          pageWidth: page.size.width,
          pageHeight: page.size.height,
        }, capThumbnailPixelRatio(window.devicePixelRatio || 1), {
          priority: 6000 - Math.abs(page.index - visiblePriorityAnchor),
          urgency: 'visible',
          requestClass: 'visible-thumbnail',
          rotation: page.rotation,
          abortStartedRender: false,
        }).catch(() => undefined);
      }

      const quickWidth = Math.max(24, Math.min(QUICK_VISIBLE_THUMBNAIL_WIDTH, previewWidth));
      const quickScale = quickWidth / Math.max(1, previewWidth);
      void renderCoordinator.renderThumbnailUrl('sidebar-thumbnail', page.index, {
        maxWidth: quickWidth,
        maxHeight: Math.max(32, layout.previewHeight * quickScale),
        pageWidth: page.size.width,
        pageHeight: page.size.height,
        minScale: 0.025,
      }, 1, {
        priority: (isStrictlyVisible ? 5000 : 1000) - Math.abs(page.index - visiblePriorityAnchor),
        urgency: 'visible',
        requestClass: 'visible-thumbnail',
        rotation: page.rotation,
        abortStartedRender: false,
      }).catch(() => undefined);
    }
  }, [
    layouts,
    pages,
    previewWidth,
    renderCoordinator,
    session,
    strictVisibleRange.endIndex,
    strictVisibleRange.startIndex,
    visibleRange.endIndex,
    visibleRange.startIndex,
    visiblePriorityAnchor,
  ]);

  return (
    <CustomScrollArea
      ref={containerRef}
      className="min-h-0 flex-1"
      viewportClassName={PAGE_THUMBNAIL_VIEWPORT_CLASS_NAME}
      viewportTestId="page-thumbnail-list"
      verticalTrackTestId="page-thumbnail-scrollbar-track"
      verticalThumbTestId="page-thumbnail-scrollbar-thumb"
    >
      <ItemGroup className="relative" style={{ height: `${totalHeight}px` }}>
        {pages.slice(visibleRange.startIndex, visibleRange.endIndex + 1).map((page, offset) => {
          const layoutPosition = visibleRange.startIndex + offset;
          const layout = layouts[layoutPosition];
          if (!layout) {
            return null;
          }

          const isStrictlyVisible =
            layoutPosition >= strictVisibleRange.startIndex && layoutPosition <= strictVisibleRange.endIndex;

          return (
            <PageThumbnailItem
              key={page.index}
              session={session}
              page={page}
              top={layout.top}
              previewWidth={previewWidth}
              previewHeight={layout.previewHeight}
              itemHeight={layout.itemHeight}
              markups={markupsByPage.get(page.index) ?? EMPTY_MARKUPS}
              pageScale={getPageScale({ pageScales: documentPageScales }, page.index)}
              mutationDisabled={mutationDisabled}
              isActive={page.index === currentPage}
              renderPriority={(isStrictlyVisible ? 2000 : 1000) - Math.abs(page.index - visiblePriorityAnchor)}
              renderUrgency={isStrictlyVisible ? 'visible' : 'prefetch'}
              sessionVersion={sessionVersion}
              onSelect={(previewUrl) => onSelectPage(page.index, 'thumbnail', previewUrl)}
              onSetPageScale={() => onSetPageScale(page.index)}
              onRotate={(direction) => onRotatePage(page.index, direction)}
            />
          );
        })}
      </ItemGroup>
    </CustomScrollArea>
  );
}

function getRangeAnchorIndex(
  layouts: readonly { index: number }[],
  range: { startIndex: number; endIndex: number },
  fallbackIndex: number,
): number {
  if (range.endIndex < range.startIndex) {
    return fallbackIndex;
  }

  const middleIndex = Math.floor((range.startIndex + range.endIndex) * 0.5);
  return layouts[middleIndex]?.index ?? fallbackIndex;
}

type ThumbnailWarmDiagnostics = Pick<
  DiagnosticsSnapshot,
  | 'pageRenderReady'
  | 'thumbnailRenderReady'
  | 'queuedPageRenders'
  | 'queuedThumbnailRenders'
  | 'inflightPageRenders'
  | 'inflightThumbnailRenders'
  | 'viewportInMotion'
>;

export function shouldWarmNearbyThumbnails({
  diagnostics,
  thumbnailListInMotion,
  hasStrictVisibleThumbnails,
}: {
  diagnostics: ThumbnailWarmDiagnostics;
  thumbnailListInMotion: boolean;
  hasStrictVisibleThumbnails: boolean;
}): boolean {
  return hasStrictVisibleThumbnails
    && !thumbnailListInMotion
    && !diagnostics.viewportInMotion
    && isRenderBacklogIdle(diagnostics);
}

export function resolveNearbyThumbnailWarmCandidates({
  startIndex,
  endIndex,
  pageCount,
  limit = 2,
}: {
  startIndex: number;
  endIndex: number;
  pageCount: number;
  limit?: number;
}): number[] {
  const candidates: number[] = [];
  for (let offset = 1; offset <= 2; offset += 1) {
    const before = startIndex - offset;
    const after = endIndex + offset;
    if (before >= 0) {
      candidates.push(before);
    }
    if (after < pageCount) {
      candidates.push(after);
    }
  }

  return [...new Set(candidates)].slice(0, Math.max(0, limit));
}
