import { useEffect, useMemo, useRef, useState, type ComponentType } from 'react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Separator } from '@/components/ui/separator';
import { Spinner } from '@/components/ui/spinner';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import {
  createPageTransform,
  formatPageScaleRatio,
  type Markup,
  type PageModel,
  type PageRotationDirection,
  type PageScale,
} from '@butter-paper/core';
import { MoreHorizontal, RotateCcw, RotateCw, ScanLine } from 'lucide-react';
import { isRenderUnavailableError, type LocalPdfSession, type PageRenderSurface } from '../services/documentSession';
import { recordComponentRender, recordPlaceholderShow } from '../services/perfTracker';
import { useRenderCoordinator } from '../services/renderCoordinator';
import { capThumbnailPixelRatio } from '../utils/renderZoom';
import { computeThumbnailContentSize } from '../utils/thumbnailLayout';
import { ReadOnlyAnnotationLayer } from './AnnotationLayer';

interface PageThumbnailItemProps {
  session: LocalPdfSession;
  page: PageModel;
  top: number;
  previewWidth: number;
  previewHeight: number;
  itemHeight: number;
  markups: readonly Markup[];
  pageScale?: PageScale;
  mutationDisabled?: boolean;
  isActive: boolean;
  showInlineRotationActions: boolean;
  renderPriority: number;
  renderUrgency: 'visible' | 'prefetch';
  sessionVersion: number;
  onSelect: (previewUrl: string | null) => void;
  onSetPageScale: () => void;
  onRotate: (direction: PageRotationDirection) => void;
}

export function PageThumbnailItem({
  session,
  page,
  top,
  previewWidth,
  previewHeight,
  itemHeight,
  markups,
  pageScale,
  mutationDisabled = false,
  isActive,
  showInlineRotationActions,
  renderPriority,
  renderUrgency,
  sessionVersion,
  onSelect,
  onSetPageScale,
  onRotate,
}: PageThumbnailItemProps) {
  recordComponentRender('PageThumbnailItem', page.index);
  const renderCoordinator = useRenderCoordinator(session);
  const [sourceUrl, setSourceUrl] = useState<string | null>(null);
  const [pageSurface, setPageSurface] = useState<PageRenderSurface | null>(null);
  const [hasError, setHasError] = useState(false);
  const [renderRetryTick, setRenderRetryTick] = useState(0);
  const bitmapCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const sourceUrlRef = useRef<string | null>(sourceUrl);
  const pageSurfaceRef = useRef<PageRenderSurface | null>(null);
  const displayedRenderedWidthRef = useRef(0);
  const renderPriorityRef = useRef(renderPriority);
  const renderUrgencyRef = useRef(renderUrgency);
  const thumbnailBounds = useMemo(() => ({
    maxWidth: previewWidth,
    maxHeight: previewHeight,
    pageWidth: page.size.width,
    pageHeight: page.size.height,
  }), [page.size.height, page.size.width, previewHeight, previewWidth]);
  const contentSize = useMemo(() => (
    computeThumbnailContentSize(page, previewWidth, previewHeight)
  ), [page, previewHeight, previewWidth]);
  const annotationTransform = useMemo(() => (
    createPageTransform(
      {
        size: page.size,
        rotation: page.rotation,
      },
      contentSize.scale,
    )
  ), [contentSize.scale, page.rotation, page.size]);

  useEffect(() => {
    sourceUrlRef.current = sourceUrl;
  }, [sourceUrl]);

  useEffect(() => {
    pageSurfaceRef.current = pageSurface;
  }, [pageSurface]);

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
    displayedRenderedWidthRef.current = nextSurface.renderedWidth;
    setPageSurface(nextSurface);
    session.releasePageSurface(currentSurface);
  };

  const replaceSourceUrl = (nextSourceUrl: string, renderedWidth: number) => {
    if (sourceUrlRef.current === nextSourceUrl) {
      displayedRenderedWidthRef.current = Math.max(displayedRenderedWidthRef.current, renderedWidth);
      return;
    }

    session.retainPageImageUrl(nextSourceUrl);
    session.releasePageImageUrl(sourceUrlRef.current);
    sourceUrlRef.current = nextSourceUrl;
    displayedRenderedWidthRef.current = renderedWidth;
    setSourceUrl(nextSourceUrl);
  };

  const clearPageSurface = () => {
    const currentSurface = pageSurfaceRef.current;
    pageSurfaceRef.current = null;
    if (!sourceUrlRef.current) {
      displayedRenderedWidthRef.current = 0;
    }
    setPageSurface(null);
    session.releasePageSurface(currentSurface);
  };

  const clearSourceUrl = () => {
    session.releasePageImageUrl(sourceUrlRef.current);
    sourceUrlRef.current = null;
    if (!pageSurfaceRef.current) {
      displayedRenderedWidthRef.current = 0;
    }
    setSourceUrl(null);
  };

  useEffect(() => {
    return () => {
      session.releasePageSurface(pageSurfaceRef.current);
      session.releasePageImageUrl(sourceUrlRef.current);
      pageSurfaceRef.current = null;
      sourceUrlRef.current = null;
      displayedRenderedWidthRef.current = 0;
    };
  }, [session]);

  useEffect(() => {
    renderPriorityRef.current = renderPriority;
    renderUrgencyRef.current = renderUrgency;
  }, [renderPriority, renderUrgency]);

  useEffect(() => {
    let cancelled = false;
    const abortController = new AbortController();
    const thumbnailPixelRatio = capThumbnailPixelRatio(window.devicePixelRatio || 1);
    const minimumReusableWidth = previewWidth * thumbnailPixelRatio * 0.9;
    const hasDisplayedImage = Boolean(pageSurfaceRef.current || sourceUrlRef.current);
    const reusableImage = renderCoordinator.selectBestReusableSource({
      pageIndex: page.index,
      minimumDisplayWidth: minimumReusableWidth,
      role: 'sidebar-thumbnail',
      hasDisplayedSource: hasDisplayedImage,
      rotation: page.rotation,
    });

    if (reusableImage?.kind === 'surface') {
      clearSourceUrl();
      replacePageSurface(reusableImage.surface);
      setHasError(false);

      return () => {
        cancelled = true;
        abortController.abort();
      };
    }

    if (reusableImage?.kind === 'object-url') {
      clearPageSurface();
      replaceSourceUrl(reusableImage.objectUrl, reusableImage.renderedWidth);
      setHasError(false);

      return () => {
        cancelled = true;
        abortController.abort();
      };
    }

    if (!pageSurfaceRef.current && !sourceUrlRef.current) {
      recordPlaceholderShow('thumbnail', page.index);
    }
    setHasError(false);

    const thumbnailUrgency = renderUrgencyRef.current;
    void renderCoordinator.renderThumbnailUrl('sidebar-thumbnail', page.index, thumbnailBounds, thumbnailPixelRatio, {
      priority: renderPriorityRef.current,
      urgency: thumbnailUrgency,
      requestClass: thumbnailUrgency === 'prefetch' ? 'nearby-prefetch' : 'visible-thumbnail',
      rotation: page.rotation,
      abortStartedRender: true,
      signal: abortController.signal,
    })
      .then((nextUrl) => {
        if (!cancelled) {
          clearPageSurface();
          const reusableImage = renderCoordinator.selectBestReusableSource({
            pageIndex: page.index,
            minimumDisplayWidth: previewWidth * thumbnailPixelRatio,
            role: 'sidebar-thumbnail',
            hasDisplayedSource: false,
            rotation: page.rotation,
          });
          if (reusableImage?.kind === 'surface') {
            clearSourceUrl();
            replacePageSurface(reusableImage.surface);
          } else if (reusableImage?.kind === 'object-url') {
            replaceSourceUrl(reusableImage.objectUrl, reusableImage.renderedWidth);
          } else {
            replaceSourceUrl(nextUrl, previewWidth * thumbnailPixelRatio);
          }
          setHasError(false);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          if (isRenderUnavailableError(error) || (error instanceof Error && error.name === 'AbortError')) {
            return;
          }
          setHasError(true);
        }
      });

    return () => {
      cancelled = true;
      abortController.abort();
    };
  }, [
    page.index,
    page.rotation,
    previewWidth,
    renderCoordinator,
    renderRetryTick,
    renderUrgency,
    session,
    thumbnailBounds,
  ]);

  useEffect(() => {
    renderCoordinator.updateThumbnailPriority('sidebar-thumbnail', page.index, thumbnailBounds, capThumbnailPixelRatio(window.devicePixelRatio || 1), {
      priority: renderPriority,
      urgency: renderUrgency,
      requestClass: renderUrgency === 'prefetch' ? 'nearby-prefetch' : 'visible-thumbnail',
      rotation: page.rotation,
    });
  }, [page.index, page.rotation, renderCoordinator, renderPriority, renderUrgency, thumbnailBounds]);

  useEffect(() => {
    const thumbnailPixelRatio = capThumbnailPixelRatio(window.devicePixelRatio || 1);
    const targetRenderedWidth = previewWidth * thumbnailPixelRatio;
    const reusableImage = renderCoordinator.selectBestReusableSource({
      pageIndex: page.index,
      minimumDisplayWidth: targetRenderedWidth,
      role: 'sidebar-thumbnail',
      hasDisplayedSource: false,
      rotation: page.rotation,
    });
    if (!reusableImage) {
      return;
    }

    if (
      (pageSurfaceRef.current || sourceUrlRef.current)
      && reusableImage.renderedWidth <= displayedRenderedWidthRef.current * 1.1
    ) {
      if (reusableImage.kind === 'surface') {
        session.releasePageSurface(reusableImage.surface);
      }
      return;
    }

    if (reusableImage.kind === 'surface') {
      clearSourceUrl();
      replacePageSurface(reusableImage.surface);
    } else {
      clearPageSurface();
      replaceSourceUrl(reusableImage.objectUrl, reusableImage.renderedWidth);
    }
    setHasError(false);
  }, [page.index, page.rotation, previewWidth, renderCoordinator, session, sessionVersion]);

  useEffect(() => {
    if (renderUrgency !== 'visible') {
      return;
    }

    const thumbnailPixelRatio = capThumbnailPixelRatio(window.devicePixelRatio || 1);
    const targetRenderedWidth = previewWidth * thumbnailPixelRatio;
    if (displayedRenderedWidthRef.current >= targetRenderedWidth * 0.9) {
      return;
    }

    let cancelled = false;
    void renderCoordinator.renderThumbnailUrl('sidebar-thumbnail', page.index, thumbnailBounds, thumbnailPixelRatio, {
      priority: renderPriority + 6000,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
      rotation: page.rotation,
      abortStartedRender: false,
    })
      .then((nextUrl) => {
        if (cancelled) {
          return;
        }

        const reusableImage = renderCoordinator.selectBestReusableSource({
          pageIndex: page.index,
          minimumDisplayWidth: targetRenderedWidth,
          role: 'sidebar-thumbnail',
          hasDisplayedSource: false,
          rotation: page.rotation,
        });
        if (reusableImage?.kind === 'surface') {
          clearSourceUrl();
          replacePageSurface(reusableImage.surface);
        } else if (reusableImage?.kind === 'object-url') {
          clearPageSurface();
          replaceSourceUrl(reusableImage.objectUrl, reusableImage.renderedWidth);
        } else {
          clearPageSurface();
          replaceSourceUrl(nextUrl, targetRenderedWidth);
        }
        setHasError(false);
      })
      .catch((error) => {
        if (!cancelled && !isRenderUnavailableError(error) && !(error instanceof Error && error.name === 'AbortError')) {
          setHasError(true);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [page.index, page.rotation, previewWidth, renderCoordinator, renderPriority, renderUrgency, thumbnailBounds]);

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
        clearPageSurface();
        setRenderRetryTick((tick) => tick + 1);
        return;
      }
      throw error;
    }
  }, [pageSurface]);

  return (
    <div
      className="absolute left-0 right-0 flex flex-col"
      data-testid={`page-thumbnail-item-${page.index + 1}`}
      style={{ top: `${top}px`, height: `${itemHeight}px` }}
    >
      <div className="flex h-12 shrink-0 items-center gap-1 px-2" data-testid={`page-thumbnail-actions-${page.index + 1}`}>
        <Button
          type="button"
          variant="ghost"
          className="min-w-0 flex-1 justify-start px-1.5"
          onClick={() => onSelect(sourceUrlRef.current)}
          data-testid={`page-thumbnail-label-${page.index + 1}`}
        >
          <span className="truncate text-[12px] font-medium tabular-nums">Page {page.index + 1}</span>
          {pageScale ? (
            <Badge variant="secondary" data-testid={`page-thumbnail-scale-badge-${page.index + 1}`}>
              {formatPageScaleRatio(pageScale)}
            </Badge>
          ) : null}
        </Button>
        <PageThumbnailActionButton
          icon={ScanLine}
          label="Set page scale"
          testId={`page-thumbnail-set-scale-${page.index + 1}`}
          onClick={onSetPageScale}
          disabled={mutationDisabled}
        />
        {showInlineRotationActions ? (
          <>
            <PageThumbnailActionButton
              icon={RotateCcw}
              label="Rotate left"
              testId={`page-thumbnail-rotate-left-${page.index + 1}`}
              onClick={() => onRotate('left')}
              disabled={mutationDisabled}
            />
            <PageThumbnailActionButton
              icon={RotateCw}
              label="Rotate right"
              testId={`page-thumbnail-rotate-right-${page.index + 1}`}
              onClick={() => onRotate('right')}
              disabled={mutationDisabled}
            />
          </>
        ) : null}
        <DropdownMenu disabled={mutationDisabled}>
          <Tooltip>
            <TooltipTrigger render={(
              <DropdownMenuTrigger render={(
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label={`Page ${page.index + 1} actions`}
                  data-testid={`page-thumbnail-more-${page.index + 1}`}
                  disabled={mutationDisabled}
                >
                  <MoreHorizontal aria-hidden="true" />
                </Button>
              )} />
            )} />
            <TooltipContent>More page actions</TooltipContent>
          </Tooltip>
          <DropdownMenuContent align="end" className="w-max min-w-40 whitespace-nowrap">
            <DropdownMenuGroup>
              <DropdownMenuItem disabled={mutationDisabled} onClick={mutationDisabled ? undefined : onSetPageScale}>
                <ScanLine aria-hidden="true" />
                Set page scale
              </DropdownMenuItem>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuGroup>
              <DropdownMenuItem disabled={mutationDisabled} onClick={mutationDisabled ? undefined : () => onRotate('left')}>
                <RotateCcw aria-hidden="true" />
                Rotate left
              </DropdownMenuItem>
              <DropdownMenuItem disabled={mutationDisabled} onClick={mutationDisabled ? undefined : () => onRotate('right')}>
                <RotateCw aria-hidden="true" />
                Rotate right
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <Button
        type="button"
        variant="ghost"
        className="flex w-full items-center justify-center rounded-none p-0"
        data-testid={`page-thumbnail-preview-${page.index + 1}`}
        aria-label={`Open page ${page.index + 1}`}
        onClick={() => onSelect(sourceUrlRef.current)}
        style={{ height: `${previewHeight}px` }}
      >
        <div
          className={[
            'relative overflow-hidden bg-white',
            isActive ? 'bp-current-page-outline' : '',
          ].join(' ')}
          data-testid={`page-thumbnail-content-${page.index + 1}`}
          style={{
            width: `${contentSize.width}px`,
            height: `${contentSize.height}px`,
          }}
        >
          {pageSurface ? (
            <>
              <canvas
                ref={bitmapCanvasRef}
                aria-label={`Page ${page.index + 1} thumbnail`}
                className="block h-full w-full"
              />
              {markups.length > 0 ? (
                <ReadOnlyAnnotationLayer
                  page={page}
                  markups={markups}
                  transform={annotationTransform}
                  testId={`thumbnail-annotation-layer-${page.index + 1}`}
                />
              ) : null}
            </>
          ) : sourceUrl ? (
            <>
              <img
                src={sourceUrl}
                alt={`Page ${page.index + 1} thumbnail`}
                className="block h-full w-full"
                draggable={false}
                onError={() => {
                  clearSourceUrl();
                  setRenderRetryTick((tick) => tick + 1);
                }}
              />
              {markups.length > 0 ? (
                <ReadOnlyAnnotationLayer
                  page={page}
                  markups={markups}
                  transform={annotationTransform}
                  testId={`thumbnail-annotation-layer-${page.index + 1}`}
                />
              ) : null}
            </>
          ) : (
            <div
              className="flex h-full w-full items-center justify-center bg-muted text-[11px] text-muted-foreground"
            >
              {hasError ? (
                'Preview unavailable'
              ) : (
                <Spinner className="size-5" data-render-placeholder="thumbnail" />
              )}
            </div>
          )}
        </div>
      </Button>
      <div className="h-4 shrink-0" aria-hidden="true" />
      <Separator data-testid={`page-thumbnail-separator-${page.index + 1}`} />
    </div>
  );
}

function PageThumbnailActionButton({
  icon: Icon,
  label,
  onClick,
  testId,
  disabled = false,
}: {
  icon: ComponentType<{ 'aria-hidden'?: boolean | 'true' | 'false' }>;
  label: string;
  onClick: () => void;
  testId: string;
  disabled?: boolean;
}) {
  const button = (
    <Button type="button" variant="ghost" size="icon" aria-label={label} data-testid={testId} disabled={disabled} onClick={disabled ? undefined : onClick}>
      <Icon aria-hidden="true" />
    </Button>
  );

  return (
    <Tooltip>
      <TooltipTrigger render={button} />
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
