import { useEffect, useMemo, useRef, useState } from 'react';
import {
  createPageTransform,
  getPageScale,
  resolvePageViewBox,
  type PageModel,
  type PdfPoint,
  type Rect,
} from '@butter-paper/core';
import { isRenderBacklogIdle, isRenderUnavailableError, type LocalPdfSession, type PageRenderSurface } from '../services/documentSession';
import {
  recordComponentRender,
  recordObsoleteRenderCompletion,
  recordPageImageVisible,
  recordPlaceholderShow,
} from '../services/perfTracker';
import { useSessionVersion } from '../services/sessionHooks';
import { useRenderCoordinator } from '../services/renderCoordinator';
import { useViewerStore } from '../state/viewerStore';
import { getPdfContentSnapCandidates, type SnapCandidate } from '../pdf-tools/snapping';
import {
  computeDetailRasterZoom,
  computeDisplayRasterLod,
  computeFullQualityRasterZoom,
  computeInitialPreviewRasterZoom,
} from '../utils/renderZoom';
import { resolveCadRenderExperimentConfig } from '../utils/cadRenderExperiment';
import type { PageLayout } from '../utils/virtualisation';
import { AnnotationLayer } from './AnnotationLayer';
import { Spinner } from '@/components/ui/spinner';

interface PageViewProps {
  session: LocalPdfSession;
  page: PageModel;
  layout: PageLayout;
  zoom: number;
  renderPriority: number;
  renderUrgency: 'visible' | 'prefetch';
  isStrictlyVisible: boolean;
  isTargetPage: boolean;
  viewportInMotion: boolean;
  visiblePageViewportRect?: Rect | null;
  overviewLabel?: {
    pageNumber: number;
  } | null;
  calibrationPickActive?: boolean;
  onCalibrationPoint?: (pageIndex: number, point: PdfPoint) => void;
  onSelectPage?: (pageIndex: number) => void;
  onHoverPage?: (pageIndex: number) => void;
}

const FULL_QUALITY_DELAY_MS = 360;
const MAX_PLACEHOLDER_SPINNER_SIZE_PX = 28;
const MIN_PLACEHOLDER_SPINNER_SIZE_PX = 4;
const MIN_ANIMATED_PLACEHOLDER_SPINNER_SIZE_PX = 10;
type PageImageQuality = 'preview' | 'full' | 'detail';

export function PageView({
  session,
  page,
  layout,
  zoom,
  renderPriority,
  renderUrgency,
  isStrictlyVisible,
  viewportInMotion,
  isTargetPage,
  visiblePageViewportRect = null,
  overviewLabel = null,
  calibrationPickActive = false,
  onCalibrationPoint,
  onSelectPage,
  onHoverPage,
}: PageViewProps) {
  recordComponentRender('PageView', page.index);
  const renderCoordinator = useRenderCoordinator(session);
  const [sourceUrl, setSourceUrl] = useState<string | null>(null);
  const [pageSurface, setPageSurface] = useState<PageRenderSurface | null>(null);
  const [detailCropSurface, setDetailCropSurface] = useState<{
    surface: PageRenderSurface;
    viewportRect: Rect;
  } | null>(null);
  const [renderState, setRenderState] = useState<'loading' | 'ready' | 'error'>('loading');
  const [imageQuality, setImageQuality] = useState<PageImageQuality | null>(null);
  const [displayedRenderedWidth, setDisplayedRenderedWidth] = useState<number | null>(null);
  const [renderRetryTick, setRenderRetryTick] = useState(0);
  const [pdfContentSnapCandidates, setPdfContentSnapCandidates] = useState<readonly SnapCandidate[]>([]);
  const pageRef = useRef<HTMLDivElement | null>(null);
  const bitmapCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const detailCropCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  const sourceUrlRef = useRef<string | null>(null);
  const pageSurfaceRef = useRef<PageRenderSurface | null>(null);
  const detailCropSurfaceRef = useRef<PageRenderSurface | null>(null);
  const renderPriorityRef = useRef(renderPriority);
  const renderUrgencyRef = useRef(renderUrgency);
  const documentState = useViewerStore((state) => state.document);
  const activeTool = useViewerStore((state) => state.activeTool);
  const selectedMarkupIds = useViewerStore((state) => state.selectedMarkupIds);
  const rightSidebarOpen = useViewerStore((state) => state.rightSidebarOpen);
  const postPlacement = useViewerStore((state) => state.postPlacement);
  const pendingImageAsset = useViewerStore((state) => state.pendingImageAsset);
  const snapSettings = useViewerStore((state) => state.snapSettings);
  const toolPropertyValues = useViewerStore((state) => state.toolPropertyValues);
  const setSelectedMarkupIds = useViewerStore((state) => state.setSelectedMarkupIds);
  const setPostPlacement = useViewerStore((state) => state.setPostPlacement);
  const setActiveTool = useViewerStore((state) => state.setActiveTool);
  const openRightSidebar = useViewerStore((state) => state.openRightSidebar);
  const collapseRightSidebar = useViewerStore((state) => state.collapseRightSidebar);
  const consumePendingImageAsset = useViewerStore((state) => state.consumePendingImageAsset);
  const updateDocument = useViewerStore((state) => state.updateDocument);
  const setStatusMessage = useViewerStore((state) => state.setStatusMessage);
  const placeholderSpinner = computePagePlaceholderSpinner(layout.width, layout.height);
  const sessionVersion = useSessionVersion(session);

  const transform = useMemo(() => {
    return createPageTransform(
      {
        size: page.size,
        rotation: page.rotation,
        viewBox: page.viewBox,
        userUnit: page.userUnit,
      },
      zoom,
    );
  }, [page.rotation, page.size, page.userUnit, page.viewBox, zoom]);

  const pageMarkups = useMemo(() => {
    return documentState?.document.markups.filter((markup) => markup.pageIndex === page.index) ?? [];
  }, [documentState?.document.markups, page.index]);
  const pageScale = useMemo(() => {
    return documentState ? getPageScale(documentState.document, page.index) : undefined;
  }, [documentState?.document, page.index]);

  const devicePixelRatio = window.devicePixelRatio || 1;
  const cadRenderExperiment = useMemo(() => resolveCadRenderExperimentConfig(), [sessionVersion]);
  const renderBacklogIdle = useMemo(() => isRenderBacklogIdle(session.diagnostics()), [session, sessionVersion]);
  const previewRasterZoom = useMemo(
    () => computeInitialPreviewRasterZoom({
      zoom,
      pageSize: page.size,
      pixelRatio: devicePixelRatio,
      importantPreviewRatio: cadRenderExperiment.importantPreviewRatio,
      isStrictlyVisible,
      isTargetPage,
      renderUrgency,
      viewportInMotion,
      renderBacklogIdle,
    }),
    [
      devicePixelRatio,
      cadRenderExperiment.importantPreviewRatio,
      isStrictlyVisible,
      isTargetPage,
      page.size,
      renderBacklogIdle,
      renderUrgency,
      viewportInMotion,
      zoom,
    ],
  );
  const rasterZoom = useMemo(
    () => computeFullQualityRasterZoom(zoom, page.size, devicePixelRatio),
    [devicePixelRatio, page.size, zoom],
  );
  const detailRasterZoom = useMemo(
    () => isTargetPage ? computeDetailRasterZoom(zoom, page.size, devicePixelRatio) : rasterZoom,
    [devicePixelRatio, isTargetPage, page.size, rasterZoom, zoom],
  );
  const displayRasterLod = useMemo(
    () => computeDisplayRasterLod({
      pageSize: page.size,
      cssWidth: layout.width,
      cssHeight: layout.height,
      zoom,
      pixelRatio: devicePixelRatio,
      isStrictlyVisible,
      isTargetPage,
      viewportInMotion,
    }),
    [devicePixelRatio, isStrictlyVisible, isTargetPage, layout.height, layout.width, page.size, viewportInMotion, zoom],
  );

  function handlePageClick(event: React.MouseEvent<HTMLDivElement>): void {
    if (!calibrationPickActive || !onCalibrationPoint) {
      onSelectPage?.(page.index);
      return;
    }

    const bounds = pageRef.current?.getBoundingClientRect();
    if (!bounds) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    onCalibrationPoint(page.index, transform.viewportToPdf({
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top,
    } as never));
  }

  const replacePageSurface = (nextSurface: PageRenderSurface) => {
    const currentSurface = pageSurfaceRef.current;
    if (
      currentSurface
      && currentSurface.cacheKey === nextSurface.cacheKey
      && currentSurface.bitmap === nextSurface.bitmap
    ) {
      session.releasePageSurface(nextSurface);
      return;
    }

    pageSurfaceRef.current = nextSurface;
    setPageSurface(nextSurface);
    session.releasePageSurface(currentSurface);
  };

  const replaceDetailCropSurface = (nextCrop: { surface: PageRenderSurface; viewportRect: Rect } | null) => {
    const currentSurface = detailCropSurfaceRef.current;
    const nextSurface = nextCrop?.surface ?? null;
    if (
      currentSurface
      && nextSurface
      && currentSurface.cacheKey === nextSurface.cacheKey
      && currentSurface.bitmap === nextSurface.bitmap
    ) {
      session.releasePageSurface(nextSurface);
      return;
    }

    detailCropSurfaceRef.current = nextSurface;
    setDetailCropSurface(nextCrop);
    session.releasePageSurface(currentSurface);
  };

  const replaceSourceUrl = (nextSourceUrl: string) => {
    if (sourceUrlRef.current === nextSourceUrl) {
      return;
    }

    session.retainPageImageUrl(nextSourceUrl);
    session.releasePageImageUrl(sourceUrlRef.current);
    sourceUrlRef.current = nextSourceUrl;
    setSourceUrl(nextSourceUrl);
  };

  const clearPageSurface = () => {
    const currentSurface = pageSurfaceRef.current;
    pageSurfaceRef.current = null;
    setPageSurface(null);
    session.releasePageSurface(currentSurface);
  };

  const clearDetailCropSurface = () => {
    const currentSurface = detailCropSurfaceRef.current;
    detailCropSurfaceRef.current = null;
    setDetailCropSurface(null);
    session.releasePageSurface(currentSurface);
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
    setDisplayedQuality(null, null);
    setRenderState('loading');
    setRenderRetryTick((tick) => tick + 1);
  };

  const setReplacementRenderError = () => {
    if (shouldReplaceDisplayedPageWithRenderError(Boolean(pageSurfaceRef.current || sourceUrlRef.current))) {
      setRenderState('error');
    }
  };

  const retryTargetQualityAfterAbort = () => {
    if (!shouldRetryTargetQualityAfterAbort({
      isTargetPage,
      isStrictlyVisible,
      renderUrgency: renderUrgencyRef.current,
      viewportInMotion,
      hasDisplayedImage: Boolean(pageSurfaceRef.current || sourceUrlRef.current),
    })) {
      return;
    }

    setRenderRetryTick((tick) => tick + 1);
  };

  const resolveImageQuality = (renderedWidth: number): PageImageQuality => resolvePageImageQuality(renderedWidth, {
    upgradeDisplayWidth: displayRasterLod.upgradeDisplayWidth,
  });

  const estimateRenderedWidth = (rasterZoomValue: number) => {
    return Math.max(1, page.size.width * rasterZoomValue * devicePixelRatio);
  };

  const setDisplayedQuality = (quality: PageImageQuality | null, renderedWidth: number | null) => {
    setImageQuality(quality);
    setDisplayedRenderedWidth(renderedWidth);
  };

  const detailCrop = useMemo(() => resolveDetailCrop({
    visiblePageViewportRect,
    page,
    layout,
    transform,
  }), [layout, page, transform, visiblePageViewportRect]);
  const detailCropKey = detailCrop ? cropRectKey(detailCrop.pdfRect) : null;
  useEffect(() => {
    sourceUrlRef.current = sourceUrl;
  }, [sourceUrl]);

  useEffect(() => {
    pageSurfaceRef.current = pageSurface;
  }, [pageSurface]);

  useEffect(() => {
    return () => {
      session.releasePageSurface(pageSurfaceRef.current);
      session.releasePageSurface(detailCropSurfaceRef.current);
      session.releasePageImageUrl(sourceUrlRef.current);
      pageSurfaceRef.current = null;
      detailCropSurfaceRef.current = null;
      sourceUrlRef.current = null;
    };
  }, [session]);

  useEffect(() => {
    renderPriorityRef.current = renderPriority;
    renderUrgencyRef.current = renderUrgency;
  }, [renderPriority, renderUrgency]);

  useEffect(() => {
    if (!snapSettings.snapToContent) {
      setPdfContentSnapCandidates([]);
      return;
    }

    let cancelled = false;
    void session.getPageGeometryIndex(page.index).then((index) => {
      if (!cancelled) {
        setPdfContentSnapCandidates(getPdfContentSnapCandidates(index));
      }
    }).catch(() => {
      if (!cancelled) {
        setPdfContentSnapCandidates([]);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [page.index, session, snapSettings.snapToContent]);

  useEffect(() => {
    let cancelled = false;
    const abortController = new AbortController();
    const scheduledTimeouts = new Set<number>();
    const pixelRatio = devicePixelRatio;
    const desiredPreviewRasterZoom = displayRasterLod.desiredRasterZoom;
    const initialRasterZoom = Math.min(desiredPreviewRasterZoom, previewRasterZoom);
    const usesBitmapTargetPath = isTargetPage && renderUrgency !== 'prefetch';
    const isThumbnailNavigationTarget = isTargetPage && session.isThumbnailNavigationIntentPage(page.index);
    const usesBitmapTargetPreview = usesBitmapTargetPath && !isThumbnailNavigationTarget;

    const hasDisplayedImage = Boolean(pageSurfaceRef.current || sourceUrlRef.current);
    const coordinatorRole = isTargetPage ? 'target-page' : 'main-page';
    const bestReusableImage = renderCoordinator.selectBestReusableSource({
      pageIndex: page.index,
      minimumDisplayWidth: displayRasterLod.minimumReusableDisplayWidth,
      role: coordinatorRole,
      hasDisplayedSource: hasDisplayedImage,
      rotation: page.rotation,
    });
    const primedPreviewUrl = hasDisplayedImage || bestReusableImage
      ? null
      : renderCoordinator.consumePrimedPagePreview(page.index);
    const reusablePreviewQuality = bestReusableImage
      ? resolveImageQuality(bestReusableImage.renderedWidth)
      : (primedPreviewUrl ? 'preview' : null);

    if (bestReusableImage?.kind === 'surface') {
      clearSourceUrl();
      replacePageSurface(bestReusableImage.surface);
      setDisplayedQuality(reusablePreviewQuality, bestReusableImage.renderedWidth);
      setRenderState('ready');
    } else if (bestReusableImage?.kind === 'object-url') {
      clearPageSurface();
      replaceSourceUrl(bestReusableImage.objectUrl);
      setDisplayedQuality(reusablePreviewQuality, bestReusableImage.renderedWidth);
      setRenderState('ready');
    } else if (primedPreviewUrl) {
      clearPageSurface();
      replaceSourceUrl(primedPreviewUrl);
      setDisplayedQuality('preview', null);
      setRenderState('ready');
    }

    if (!pageSurfaceRef.current && !sourceUrlRef.current) {
      recordPlaceholderShow('page', page.index);
      setRenderState('loading');
    }

    const clearScheduledTimeouts = () => {
      for (const timeout of scheduledTimeouts) {
        window.clearTimeout(timeout);
      }
      scheduledTimeouts.clear();
    };

    const scheduleRender = (callback: () => void, delayMs: number) => {
      const timeout = window.setTimeout(() => {
        scheduledTimeouts.delete(timeout);
        callback();
      }, delayMs);
      scheduledTimeouts.add(timeout);
    };

    const requestQuality = (quality: Exclude<PageImageQuality, 'detail'>) => {
      const fullUrgency = renderUrgencyRef.current;
      const shouldUseTargetCropOverlay = quality === 'full'
        && isTargetPage
        && cadRenderExperiment.targetCropPrototype
        && Boolean(detailCrop)
        && Boolean(pageSurfaceRef.current || sourceUrlRef.current);
      const requestClass = fullUrgency === 'prefetch'
        ? 'nearby-prefetch'
        : shouldUseTargetCropOverlay
          ? 'target-page-crop'
        : quality === 'preview'
          ? (isTargetPage ? 'target-page-preview' : 'visible-page-preview')
          : (isTargetPage ? 'target-page-hq' : 'visible-page-hq-upgrade');
      const requestedRasterZoom = quality === 'preview'
        ? initialRasterZoom
        : rasterZoom;
      const priorityOffset = quality === 'preview' ? 250 : 0;
      const navigationTargetPriorityOffset = isThumbnailNavigationTarget ? 5000 : 0;

      const useBitmapTransport = usesBitmapTargetPath && (quality !== 'preview' || usesBitmapTargetPreview);
      const cropPdfRect = shouldUseTargetCropOverlay ? detailCrop?.pdfRect : undefined;
      const cropViewportRect = shouldUseTargetCropOverlay ? detailCrop?.viewportRect : undefined;

      if (useBitmapTransport) {
        void renderCoordinator.renderPageSurface(coordinatorRole, layout.index, requestedRasterZoom, pixelRatio, {
          priority: renderPriorityRef.current + priorityOffset + navigationTargetPriorityOffset,
          urgency: fullUrgency,
          requestClass,
          rotation: page.rotation,
          cropPdfRect,
          abortStartedRender: fullUrgency === 'prefetch',
          signal: abortController.signal,
        }).then((surface) => {
          if (cancelled) {
            session.releasePageSurface(surface);
            recordObsoleteRenderCompletion('page', requestClass);
            return;
          }

          if (cropViewportRect) {
            replaceDetailCropSurface({ surface, viewportRect: cropViewportRect });
            setDisplayedQuality(quality, surface.renderedWidth * (layout.width / Math.max(1, cropViewportRect.width)));
            setRenderState('ready');
            scheduleNextQuality(quality);
            return;
          }

          clearSourceUrl();
          clearDetailCropSurface();
          replacePageSurface(surface);
          setDisplayedQuality(quality, surface.renderedWidth);
          setRenderState('ready');
          scheduleNextQuality(quality);
        }).catch((error) => {
          if (!cancelled) {
            if (isRenderUnavailableError(error)) {
              return;
            }
            if (error instanceof Error && error.name === 'AbortError') {
              retryTargetQualityAfterAbort();
              return;
            }
            setReplacementRenderError();
          }
        });
        return;
      }

      void renderCoordinator.renderPageUrl(coordinatorRole, layout.index, requestedRasterZoom, pixelRatio, {
        priority: renderPriorityRef.current + priorityOffset + navigationTargetPriorityOffset,
        urgency: fullUrgency,
        requestClass,
        rotation: page.rotation,
        cropPdfRect,
        abortStartedRender: fullUrgency === 'prefetch',
        signal: abortController.signal,
      }).then((objectUrl) => {
        if (cancelled) {
          recordObsoleteRenderCompletion('page', requestClass);
          return;
        }

        clearPageSurface();
        clearDetailCropSurface();
        replaceSourceUrl(objectUrl);
        setDisplayedQuality(quality, estimateRenderedWidth(requestedRasterZoom));
        setRenderState('ready');
        scheduleNextQuality(quality);
      }).catch((error) => {
        if (!cancelled) {
          if (isRenderUnavailableError(error)) {
            return;
          }
          if (error instanceof Error && error.name === 'AbortError') {
            retryTargetQualityAfterAbort();
            return;
          }
          setReplacementRenderError();
        }
      });
    };

    const requestFullQuality = () => {
      requestQuality('full');
    };

    const scheduleNextQuality = (currentQuality: PageImageQuality | null) => {
      const nextRequest = resolveNextPageImageQualityRequest({
        currentQuality,
        renderUrgency: renderUrgencyRef.current,
        viewportInMotion,
        immediateTargetPromotion: isThumbnailNavigationTarget,
      });
      if (!nextRequest) {
        return;
      }
      scheduleRender(requestFullQuality, nextRequest.delayMs);
    };

    const requestPreviewThenMaybeFullQuality = () => {
      if (
        renderUrgencyRef.current === 'prefetch'
        && !shouldRequestColdPrefetchPageRender({
          viewportInMotion,
          renderBacklogIdle,
        })
      ) {
        return;
      }

      const shouldUsePreview = !pageSurfaceRef.current && !sourceUrlRef.current && initialRasterZoom < rasterZoom;
      if (!shouldUsePreview) {
        requestFullQuality();
        return;
      }

      requestQuality('preview');
    };

    if (hasDisplayedImage) {
      // Scrolling changes visibility and target-page priority, but it must not
      // tear down the image already on screen. Wait until motion settles before
      // scheduling a quality upgrade for the displayed raster.
      if (!viewportInMotion) {
        scheduleNextQuality(imageQuality);
      }
      return () => {
        cancelled = true;
        abortController.abort();
        clearScheduledTimeouts();
      };
    }

    if (reusablePreviewQuality === 'full') {
      return () => {
        cancelled = true;
        abortController.abort();
      };
    }

    if (reusablePreviewQuality === 'preview') {
      scheduleNextQuality(reusablePreviewQuality);
      return () => {
        cancelled = true;
        abortController.abort();
        clearScheduledTimeouts();
      };
    }

    requestPreviewThenMaybeFullQuality();

    return () => {
      cancelled = true;
      abortController.abort();
      clearScheduledTimeouts();
    };
  }, [
    devicePixelRatio,
    displayRasterLod,
    isStrictlyVisible,
    isTargetPage,
    layout.index,
    page.index,
    page.rotation,
    page.size,
    previewRasterZoom,
    rasterZoom,
    renderRetryTick,
    renderBacklogIdle,
    renderUrgency,
    renderCoordinator,
    session,
    viewportInMotion,
    imageQuality,
    cadRenderExperiment,
    detailCrop,
  ]);

  useEffect(() => {
    const currentRenderedWidth = displayedRenderedWidth ?? 0;
    const betterImage = session.getBestReusablePageImageAtLeast(
      page.index,
      resolveCachePromotionMinimumWidth({
        currentRenderedWidth,
        desiredDisplayWidth: displayRasterLod.desiredDisplayWidth,
      }),
      page.rotation,
    );
    if (!betterImage || betterImage.renderedWidth <= currentRenderedWidth) {
      return;
    }

    const nextQuality = resolveImageQuality(betterImage.renderedWidth);

    if (betterImage.kind === 'surface') {
      clearSourceUrl();
      clearDetailCropSurface();
      replacePageSurface(betterImage.surface);
    } else {
      clearPageSurface();
      clearDetailCropSurface();
      replaceSourceUrl(betterImage.objectUrl);
    }
    setDisplayedQuality(nextQuality, betterImage.renderedWidth);
    setRenderState('ready');
  }, [displayRasterLod.desiredDisplayWidth, displayRasterLod.upgradeDisplayWidth, displayedRenderedWidth, page.index, page.rotation, session, sessionVersion]);

  useEffect(() => {
    if (!detailCropSurface) {
      return;
    }

    const surfaceCropKey = detailCropSurface.surface.cropPdfRect
      ? cropRectKey(detailCropSurface.surface.cropPdfRect)
      : null;
    if (!shouldClearDetailCropSurfaceForViewport({
      viewportInMotion,
      currentCropKey: detailCropKey,
      surfaceCropKey,
    })) {
      return;
    }

    clearDetailCropSurface();
    setDisplayedQuality('full', pageSurfaceRef.current?.renderedWidth ?? displayedRenderedWidth);
  }, [detailCropKey, detailCropSurface, displayedRenderedWidth, viewportInMotion]);

  useEffect(() => {
    if (
      !isTargetPage ||
      !isStrictlyVisible ||
      renderUrgency === 'prefetch' ||
      viewportInMotion ||
      (imageQuality !== 'full' && imageQuality !== 'detail') ||
      detailRasterZoom <= rasterZoom
    ) {
      return;
    }

    let cancelled = false;
    const abortController = new AbortController();
    const requestClass = cadRenderExperiment.targetCropPrototype && detailCrop ? 'target-page-crop' : 'target-page-hq';
    const cropPdfRect = requestClass === 'target-page-crop' ? detailCrop?.pdfRect : undefined;
    const cropViewportRect = requestClass === 'target-page-crop' ? detailCrop?.viewportRect : undefined;

    if (
      imageQuality === 'detail'
      && detailCropKey
      && detailCropSurface?.surface.cropPdfRect
      && cropRectKey(detailCropSurface.surface.cropPdfRect) === detailCropKey
    ) {
      return;
    }
    const timeout = window.setTimeout(() => {
      void renderCoordinator.renderPageSurface('target-page', layout.index, detailRasterZoom, devicePixelRatio, {
        priority: renderPriority - 25,
        urgency: 'visible',
        requestClass,
        rotation: page.rotation,
        cropPdfRect,
        signal: abortController.signal,
      }).then((surface) => {
        if (cancelled) {
          session.releasePageSurface(surface);
          recordObsoleteRenderCompletion('page', requestClass);
          return;
        }

        if (cropViewportRect) {
          if (!isDetailCropSurfaceGeometryCompatible(surface, cropViewportRect)) {
            session.releasePageSurface(surface);
            void renderCoordinator.renderPageSurface('target-page', layout.index, detailRasterZoom, devicePixelRatio, {
              priority: renderPriority - 25,
              urgency: 'visible',
              requestClass: 'target-page-hq',
              rotation: page.rotation,
              signal: abortController.signal,
            }).then((fallbackSurface) => {
              if (cancelled) {
                session.releasePageSurface(fallbackSurface);
                recordObsoleteRenderCompletion('page', 'target-page-hq');
                return;
              }

              clearSourceUrl();
              replacePageSurface(fallbackSurface);
              clearDetailCropSurface();
              setDisplayedQuality('detail', fallbackSurface.renderedWidth);
              setRenderState('ready');
            }).catch((error) => {
              if (!cancelled && isRenderUnavailableError(error)) {
                return;
              }
              if (!cancelled && !(error instanceof Error && error.name === 'AbortError')) {
                setReplacementRenderError();
              } else if (!cancelled) {
                retryTargetQualityAfterAbort();
              }
            });
            return;
          }
          replaceDetailCropSurface({ surface, viewportRect: cropViewportRect });
          setDisplayedQuality('detail', surface.renderedWidth * (layout.width / Math.max(1, cropViewportRect.width)));
        } else {
          clearSourceUrl();
          replacePageSurface(surface);
          clearDetailCropSurface();
          setDisplayedQuality('detail', surface.renderedWidth);
        }
        setRenderState('ready');
      }).catch((error) => {
        if (!cancelled && isRenderUnavailableError(error)) {
          return;
        }
        if (!cancelled && !(error instanceof Error && error.name === 'AbortError')) {
          setReplacementRenderError();
        } else if (!cancelled) {
          retryTargetQualityAfterAbort();
        }
      });
    }, cadRenderExperiment.detailQualityDelayMs);

    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
      abortController.abort();
    };
  }, [
    detailRasterZoom,
    detailCrop,
    detailCropKey,
    detailCropSurface,
    devicePixelRatio,
    imageQuality,
    isStrictlyVisible,
    isTargetPage,
    layout.index,
    layout.width,
    page.size,
    page.rotation,
    rasterZoom,
    renderCoordinator,
    renderPriority,
    renderUrgency,
    session,
    viewportInMotion,
    cadRenderExperiment.targetCropPrototype,
    cadRenderExperiment.detailQualityDelayMs,
  ]);

  useEffect(() => {
    const pixelRatio = devicePixelRatio;
    const initialRasterZoom = Math.min(displayRasterLod.desiredRasterZoom, previewRasterZoom);
    const usesBitmapTargetPath = isTargetPage && renderUrgency !== 'prefetch';
    const usesBitmapTargetPreview = usesBitmapTargetPath && !session.isThumbnailNavigationIntentPage(page.index);

    if (usesBitmapTargetPath) {
      if (usesBitmapTargetPreview) {
        renderCoordinator.updatePageSurfacePriority('target-page', layout.index, initialRasterZoom, pixelRatio, {
          priority: renderPriority + 250,
          urgency: 'visible',
          requestClass: 'target-page-preview',
          rotation: page.rotation,
        });
      } else {
        renderCoordinator.updatePageUrlPriority('target-page', layout.index, initialRasterZoom, pixelRatio, {
          priority: renderPriority + 250,
          urgency: 'visible',
          requestClass: 'target-page-preview',
          rotation: page.rotation,
        });
      }
      renderCoordinator.updatePageSurfacePriority('target-page', layout.index, rasterZoom, pixelRatio, {
        priority: renderPriority,
        urgency: renderUrgency,
        requestClass: 'target-page-hq',
        rotation: page.rotation,
      });
      if (detailRasterZoom > rasterZoom) {
        renderCoordinator.updatePageSurfacePriority('target-page', layout.index, detailRasterZoom, pixelRatio, {
          priority: renderPriority - 25,
          urgency: 'visible',
          requestClass: cadRenderExperiment.targetCropPrototype ? 'target-page-crop' : 'target-page-hq',
          rotation: page.rotation,
        });
      }
      return;
    }

    renderCoordinator.updatePageUrlPriority(isTargetPage ? 'target-page' : 'main-page', layout.index, initialRasterZoom, pixelRatio, {
      priority: renderPriority + 250,
      urgency: renderUrgency === 'prefetch' ? 'prefetch' : 'visible',
      requestClass: renderUrgency === 'prefetch'
        ? 'nearby-prefetch'
        : (isTargetPage ? 'target-page-preview' : 'visible-page-preview'),
      rotation: page.rotation,
    });
    renderCoordinator.updatePageUrlPriority(isTargetPage ? 'target-page' : 'main-page', layout.index, rasterZoom, pixelRatio, {
      priority: renderPriority,
      urgency: renderUrgency,
      requestClass: renderUrgency === 'prefetch'
        ? 'nearby-prefetch'
        : (isTargetPage ? 'target-page-hq' : 'visible-page-hq-upgrade'),
      rotation: page.rotation,
    });
  }, [cadRenderExperiment.targetCropPrototype, detailRasterZoom, devicePixelRatio, displayRasterLod.desiredRasterZoom, isTargetPage, layout.index, page.index, page.rotation, previewRasterZoom, rasterZoom, renderCoordinator, renderPriority, renderUrgency, session]);

  useEffect(() => {
    const canvas = bitmapCanvasRef.current;
    if (!canvas || !pageSurface) {
      return;
    }

    canvas.width = Math.max(1, pageSurface.renderedWidth);
    canvas.height = Math.max(1, pageSurface.renderedHeight);
    const context = canvas.getContext('2d', { alpha: false });
    if (!context) {
      return;
    }

    context.clearRect(0, 0, canvas.width, canvas.height);
    try {
      context.drawImage(pageSurface.bitmap, 0, 0, canvas.width, canvas.height);
    } catch (error) {
      if (error instanceof DOMException && error.name === 'InvalidStateError') {
        const fallbackImage = session.getBestReusablePageImageAtLeast(page.index, displayedRenderedWidth ?? 0, page.rotation);
        if (fallbackImage?.kind === 'object-url') {
          clearPageSurface();
          replaceSourceUrl(fallbackImage.objectUrl);
          setDisplayedQuality(resolveImageQuality(fallbackImage.renderedWidth), fallbackImage.renderedWidth);
          setRenderState('ready');
        } else {
          clearPageSurface();
          setDisplayedQuality(sourceUrlRef.current ? imageQuality : null, sourceUrlRef.current ? displayedRenderedWidth : null);
          setRenderState(sourceUrlRef.current ? 'ready' : 'loading');
        }
        retryTargetQualityAfterAbort();
        return;
      }
      throw error;
    }
  }, [pageSurface]);

  useEffect(() => {
    const canvas = detailCropCanvasRef.current;
    const cropSurface = detailCropSurface?.surface ?? null;
    if (!canvas || !cropSurface) {
      return;
    }

    canvas.width = Math.max(1, cropSurface.renderedWidth);
    canvas.height = Math.max(1, cropSurface.renderedHeight);
    const context = canvas.getContext('2d', { alpha: false });
    if (!context) {
      return;
    }

    context.clearRect(0, 0, canvas.width, canvas.height);
    try {
      context.drawImage(cropSurface.bitmap, 0, 0, canvas.width, canvas.height);
    } catch (error) {
      if (error instanceof DOMException && error.name === 'InvalidStateError') {
        clearDetailCropSurface();
        if (pageSurfaceRef.current) {
          setDisplayedQuality('full', pageSurfaceRef.current.renderedWidth);
        }
        return;
      }
      throw error;
    }
  }, [detailCropSurface]);

  useEffect(() => {
    const hasVisibleImage = Boolean(
      pageSurface
      || sourceUrl
      || detailCropSurface,
    );
    if (!isStrictlyVisible || !hasVisibleImage || !imageQuality) {
      return;
    }

    recordPageImageVisible(
      page.index,
      imageQuality,
      displayedRenderedWidth ?? undefined,
      displayRasterLod.desiredDisplayWidth,
    );
  }, [
    displayRasterLod.desiredDisplayWidth,
    displayedRenderedWidth,
    imageQuality,
    isStrictlyVisible,
    page.index,
    pageSurface,
    sourceUrl,
    detailCropSurface,
  ]);

  function createSnapshotDataUrl(snapshotRect: Rect): string | null {
    const sourceCanvas = bitmapCanvasRef.current;
    const sourceImage = imageRef.current;
    const viewportRect = transform.pdfRectToViewport(snapshotRect);
    const sourceWidth = sourceCanvas?.width ?? sourceImage?.naturalWidth ?? 0;
    const sourceHeight = sourceCanvas?.height ?? sourceImage?.naturalHeight ?? 0;
    if (sourceWidth <= 0 || sourceHeight <= 0 || viewportRect.width <= 0 || viewportRect.height <= 0) {
      return null;
    }

    const scaleX = sourceWidth / Math.max(1, layout.width);
    const scaleY = sourceHeight / Math.max(1, layout.height);
    const cropX = Math.max(0, Math.round(viewportRect.x * scaleX));
    const cropY = Math.max(0, Math.round(viewportRect.y * scaleY));
    const cropWidth = Math.min(sourceWidth - cropX, Math.max(1, Math.round(viewportRect.width * scaleX)));
    const cropHeight = Math.min(sourceHeight - cropY, Math.max(1, Math.round(viewportRect.height * scaleY)));
    if (cropWidth <= 0 || cropHeight <= 0) {
      return null;
    }

    const canvas = document.createElement('canvas');
    canvas.width = cropWidth;
    canvas.height = cropHeight;
    const context = canvas.getContext('2d');
    if (!context) {
      return null;
    }
    const source = sourceCanvas ?? sourceImage;
    if (!source) {
      return null;
    }
    context.drawImage(source, cropX, cropY, cropWidth, cropHeight, 0, 0, cropWidth, cropHeight);
    return canvas.toDataURL('image/png');
  }

  return (
    <div
      ref={pageRef}
      className={[
        'absolute border border-neutral-300 bg-white transition-[border-color,box-shadow]',
        isTargetPage ? 'bp-current-page-outline' : '',
      ].join(' ')}
      style={{
        top: `${layout.top}px`,
        left: `${layout.left}px`,
        width: `${layout.width}px`,
        height: `${layout.height}px`,
      }}
      data-page-index={page.index}
      data-current-page={isTargetPage ? 'true' : 'false'}
      data-testid={`page-${page.index + 1}`}
      onClick={handlePageClick}
      onPointerEnter={() => onHoverPage?.(page.index)}
    >
      {overviewLabel ? (
        <div
          className="pointer-events-none absolute left-2 top-2 z-20 rounded-[5px] bg-neutral-950/85 px-2 py-1 text-[11px] font-semibold leading-tight text-white shadow-sm"
          data-testid={`page-overview-label-${overviewLabel.pageNumber}`}
        >
          <div>{`Page ${overviewLabel.pageNumber}`}</div>
        </div>
      ) : null}
      {pageSurface ? (
        <canvas
          ref={bitmapCanvasRef}
          aria-label={`Page ${layout.index + 1}`}
          className={[
            'block h-full w-full',
            imageQuality === 'preview' ? 'bp-page-preview-raster' : '',
          ].join(' ')}
          data-render-state={renderState}
          data-render-quality={imageQuality ?? 'unknown'}
        />
      ) : sourceUrl ? (
        <img
          ref={imageRef}
          src={sourceUrl}
          alt=""
          className={[
            'block h-full w-full object-contain',
            imageQuality === 'preview' ? 'bp-page-preview-raster' : '',
          ].join(' ')}
          draggable={false}
          onError={retryBrokenSourceUrl}
          aria-label={`Page ${layout.index + 1}`}
          data-render-state={renderState}
          data-render-quality={imageQuality ?? 'unknown'}
        />
      ) : (
        <div className="flex h-full items-center justify-center bg-neutral-50">
          {renderState === 'error' ? (
            <div className="text-[12px] text-neutral-400">Unable to render page</div>
          ) : (
            <Spinner
              className={placeholderSpinner.animated ? undefined : 'animate-none'}
              style={{
                width: placeholderSpinner.size,
                height: placeholderSpinner.size,
              }}
              data-render-placeholder="page"
            />
          )}
        </div>
      )}
      {detailCropSurface ? (
        <canvas
          ref={detailCropCanvasRef}
          aria-hidden="true"
          className="pointer-events-none absolute z-10 block"
          style={{
            left: `${detailCropSurface.viewportRect.x}px`,
            top: `${detailCropSurface.viewportRect.y}px`,
            width: `${detailCropSurface.viewportRect.width}px`,
            height: `${detailCropSurface.viewportRect.height}px`,
          }}
          data-render-quality="detail-crop"
        />
      ) : null}
      <AnnotationLayer
        page={page}
        pageScale={pageScale}
        markups={pageMarkups}
        transform={transform}
        pdfContentSnapCandidates={pdfContentSnapCandidates}
        snapToContent={snapSettings.snapToContent}
        snapToMarkup={snapSettings.snapToMarkup}
        snapTolerancePx={snapSettings.sensitivityPx}
        snapTargets={snapSettings.snapTargets}
        snapGuidesEnabled={snapSettings.snapGuidesEnabled}
        snapGuideTypes={snapSettings.snapGuideTypes}
        activeTool={activeTool}
        toolPropertyValues={toolPropertyValues}
        selectedMarkupIds={selectedMarkupIds}
        postPlacement={postPlacement}
        pendingImageAsset={pendingImageAsset}
        setSelectedMarkupIds={setSelectedMarkupIds}
        setPostPlacement={setPostPlacement}
        consumePendingImageAsset={consumePendingImageAsset}
        onImagePlaced={() => setActiveTool('select')}
        onToggleProperties={(wasSelectedBeforeDoubleClick) => {
          if (propertiesDoubleClickSidebarAction(wasSelectedBeforeDoubleClick, rightSidebarOpen) === 'collapse') {
            collapseRightSidebar();
            return;
          }
          openRightSidebar('tools');
        }}
        createSnapshotDataUrl={createSnapshotDataUrl}
        updateDocument={updateDocument}
        onToolError={setStatusMessage}
        calibrationPickActive={calibrationPickActive}
        onCalibrationPoint={onCalibrationPoint}
      />
    </div>
  );
}

export function propertiesDoubleClickSidebarAction(
  wasSelectedBeforeDoubleClick: boolean,
  rightSidebarOpen: boolean,
): 'open' | 'collapse' {
  return wasSelectedBeforeDoubleClick && rightSidebarOpen ? 'collapse' : 'open';
}

export function computePagePlaceholderSpinner(pageWidth: number, pageHeight: number): {
  size: number;
  borderWidth: number;
  animated: boolean;
} {
  const safeWidth = Number.isFinite(pageWidth) ? Math.max(1, pageWidth) : 1;
  const safeHeight = Number.isFinite(pageHeight) ? Math.max(1, pageHeight) : 1;
  const size = Math.max(
    MIN_PLACEHOLDER_SPINNER_SIZE_PX,
    Math.min(MAX_PLACEHOLDER_SPINNER_SIZE_PX, safeWidth * 0.28, safeHeight * 0.2),
  );

  return {
    size: Number(size.toFixed(2)),
    borderWidth: Number(Math.max(1, Math.min(3, size * 0.12)).toFixed(2)),
    animated: size >= MIN_ANIMATED_PLACEHOLDER_SPINNER_SIZE_PX,
  };
}

export function resolvePageImageQuality(
  renderedWidth: number,
  thresholds: {
    upgradeDisplayWidth: number;
  },
): PageImageQuality {
  if (renderedWidth >= thresholds.upgradeDisplayWidth) {
    return 'full';
  }
  return 'preview';
}

export function resolveNextPageImageQualityRequest({
  currentQuality,
  renderUrgency,
  viewportInMotion,
  immediateTargetPromotion = false,
}: {
  currentQuality: PageImageQuality | null;
  renderUrgency: 'visible' | 'prefetch';
  viewportInMotion: boolean;
  immediateTargetPromotion?: boolean;
}): { quality: 'full'; delayMs: number } | null {
  if (renderUrgency !== 'visible') {
    return null;
  }

  if (currentQuality === 'preview' && immediateTargetPromotion) {
    return {
      quality: 'full',
      delayMs: 0,
    };
  }

  if (currentQuality === 'preview' && !viewportInMotion) {
    return {
      quality: 'full',
      delayMs: FULL_QUALITY_DELAY_MS,
    };
  }

  return null;
}

export function shouldRequestColdPrefetchPageRender({
  viewportInMotion,
  renderBacklogIdle,
}: {
  viewportInMotion: boolean;
  renderBacklogIdle: boolean;
}): boolean {
  return !viewportInMotion && renderBacklogIdle;
}

export function shouldReplaceDisplayedPageWithRenderError(hasDisplayedImage: boolean): boolean {
  return !hasDisplayedImage;
}

export function shouldRetryTargetQualityAfterAbort({
  isTargetPage,
  isStrictlyVisible,
  renderUrgency,
  viewportInMotion,
  hasDisplayedImage,
}: {
  isTargetPage: boolean;
  isStrictlyVisible: boolean;
  renderUrgency: 'visible' | 'prefetch';
  viewportInMotion: boolean;
  hasDisplayedImage: boolean;
}): boolean {
  return isTargetPage
    && isStrictlyVisible
    && renderUrgency === 'visible'
    && !viewportInMotion
    && hasDisplayedImage;
}

export function resolveCachePromotionMinimumWidth({
  currentRenderedWidth,
  desiredDisplayWidth,
}: {
  currentRenderedWidth: number;
  desiredDisplayWidth: number;
}): number {
  const safeCurrentWidth = Number.isFinite(currentRenderedWidth) && currentRenderedWidth > 0
    ? currentRenderedWidth
    : 0;
  const safeDesiredWidth = Number.isFinite(desiredDisplayWidth) && desiredDisplayWidth > 0
    ? desiredDisplayWidth
    : 0;
  return Math.max(safeCurrentWidth + 1, Math.min(safeDesiredWidth, safeCurrentWidth * 1.01));
}

export function shouldRetryBrokenPageImageSource(sourceUrl: string | null): boolean {
  return Boolean(sourceUrl);
}

export function shouldClearDetailCropSurfaceForViewport({
  viewportInMotion,
  currentCropKey,
  surfaceCropKey,
}: {
  viewportInMotion: boolean;
  currentCropKey: string | null;
  surfaceCropKey: string | null;
}): boolean {
  return viewportInMotion || !currentCropKey || !surfaceCropKey || currentCropKey !== surfaceCropKey;
}

export function isDetailCropSurfaceGeometryCompatible(
  surface: Pick<PageRenderSurface, 'renderedWidth' | 'renderedHeight'>,
  viewportRect: Pick<Rect, 'width' | 'height'>,
): boolean {
  if (
    surface.renderedWidth <= 0
    || surface.renderedHeight <= 0
    || viewportRect.width <= 0
    || viewportRect.height <= 0
  ) {
    return false;
  }

  const surfaceAspect = surface.renderedWidth / surface.renderedHeight;
  const viewportAspect = viewportRect.width / viewportRect.height;
  const relativeDelta = Math.abs(surfaceAspect - viewportAspect) / Math.max(surfaceAspect, viewportAspect);
  return relativeDelta <= 0.02;
}

export function resolveDetailCrop({
  visiblePageViewportRect,
  page,
  layout,
  transform,
}: {
  visiblePageViewportRect: Rect | null | undefined;
  page: PageModel;
  layout: Pick<PageLayout, 'width' | 'height'>;
  transform: ReturnType<typeof createPageTransform>;
}): { pdfRect: Rect; viewportRect: Rect } | null {
  if (!visiblePageViewportRect || visiblePageViewportRect.width <= 0 || visiblePageViewportRect.height <= 0) {
    return null;
  }

  const padPx = Math.max(24, Math.min(96, Math.max(visiblePageViewportRect.width, visiblePageViewportRect.height) * 0.08));
  const viewportRect = {
    x: Math.max(0, visiblePageViewportRect.x - padPx),
    y: Math.max(0, visiblePageViewportRect.y - padPx),
    width: Math.min(layout.width, visiblePageViewportRect.x + visiblePageViewportRect.width + padPx) - Math.max(0, visiblePageViewportRect.x - padPx),
    height: Math.min(layout.height, visiblePageViewportRect.y + visiblePageViewportRect.height + padPx) - Math.max(0, visiblePageViewportRect.y - padPx),
  };
  if (viewportRect.width <= 0 || viewportRect.height <= 0) {
    return null;
  }

  const viewBox = resolvePageViewBox(page);
  const pdfRect = transform.viewportRectToPdf(viewportRect);
  const clampedX = Math.max(viewBox.x, pdfRect.x);
  const clampedY = Math.max(viewBox.y, pdfRect.y);
  const clampedPdfRect = {
    x: clampedX,
    y: clampedY,
    width: Math.min(viewBox.x + viewBox.width, pdfRect.x + pdfRect.width) - clampedX,
    height: Math.min(viewBox.y + viewBox.height, pdfRect.y + pdfRect.height) - clampedY,
  };
  if (clampedPdfRect.width <= 0 || clampedPdfRect.height <= 0) {
    return null;
  }

  return {
    pdfRect: clampedPdfRect,
    viewportRect: transform.pdfRectToViewport(clampedPdfRect),
  };
}

function cropRectKey(rect: Rect): string {
  return `${rect.x.toFixed(2)}:${rect.y.toFixed(2)}:${rect.width.toFixed(2)}:${rect.height.toFixed(2)}`;
}
