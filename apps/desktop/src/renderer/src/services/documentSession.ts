import type { Markup, PageModel, PageScale } from '@butter-paper/core';
import type { PdfPageGeometryIndex } from '@butter-paper/pdf';
import type {
  DocumentOpenStageTimings,
  LoadedDocumentPayload,
  PdfRect,
  PdfRenderRequestClass,
  PdfSaveTargetDescriptor,
} from '../../../shared/protocol';
import {
  createDesktopSessionBackend,
  type PdfSessionBackend,
  type PdfSessionDocumentHandle,
} from './documentSessionBackend';
import {
  recordCacheReuseFromPrefetch,
  recordPromotedTaskAbortBeforeStart,
  recordPromotedTaskStart,
  recordQueuedTaskAbortAfterStart,
  recordQueuedTaskAbortBeforeStart,
  recordQueuedTaskAdoption,
  recordQueuedTaskStart,
  recordQueuedTaskTransition,
  recordRasterAbort,
  recordRasterComplete,
  recordRasterError,
  recordRasterRequest,
  recordRasterStart,
} from './perfTracker';

interface ThumbnailCacheEntry {
  pageIndex: number;
  rotation?: PageModel['rotation'];
  objectUrl: string;
  byteSize: number;
  renderedWidth: number;
  renderedHeight: number;
  cropPdfRect?: PdfRect;
  sourceRequestClass?: RenderRequestClass;
  lastUsedAt: number;
  protectedUntil: number;
}

interface PageRenderCacheEntry {
  cacheKey: string;
  pageIndex: number;
  rotation?: PageModel['rotation'];
  bitmap: ImageBitmap;
  byteSize: number;
  renderedWidth: number;
  renderedHeight: number;
  cropPdfRect?: PdfRect;
  sourceRequestClass?: RenderRequestClass;
  lastUsedAt: number;
  refCount: number;
  pendingClose: boolean;
}

export type ReusablePageImageSource = 'page-bitmap' | 'page-url' | 'thumbnail';

export type ReusablePageImage =
  | {
      kind: 'surface';
      pageIndex: number;
      renderedWidth: number;
      renderedHeight: number;
      source: 'page-bitmap';
      sourceRequestClass?: RenderRequestClass;
      surface: PageRenderSurface;
    }
  | {
      kind: 'object-url';
      pageIndex: number;
      renderedWidth: number;
      renderedHeight: number;
      source: 'page-url' | 'thumbnail';
      sourceRequestClass?: RenderRequestClass;
      objectUrl: string;
    };

export interface PageRenderSurface {
  cacheKey: string;
  pageIndex: number;
  bitmap: ImageBitmap;
  renderedWidth: number;
  renderedHeight: number;
  cropPdfRect?: PdfRect;
}

export interface ReusablePagePreviewInfo {
  objectUrl: string;
  renderedWidth: number;
  renderedHeight: number;
  source: 'thumbnail' | 'page-url';
  sourceRequestClass?: RenderRequestClass;
}

interface ThumbnailRenderOptions {
  maxWidth: number;
  maxHeight: number;
  pixelRatio?: number;
  minScale?: number;
  pageWidth?: number;
  pageHeight?: number;
}

type NormalisedThumbnailRenderOptions = ThumbnailRenderOptions & {
  pixelRatio: number;
  minScale: number;
};

type RenderUrgency = 'visible' | 'prefetch';
type NavigationIntentSource = 'generic' | 'thumbnail';
export type RenderRequestClass = PdfRenderRequestClass;
type AdaptivePerformanceLevel = 0 | 1 | 2 | 3;

export function resolveAdaptiveRenderConcurrency(level: AdaptivePerformanceLevel): {
  page: number;
  thumbnail: number;
  total: number;
  prefetch: number;
  overviewThumbnailCeiling: number | null;
} {
  if (level === 3) {
    return { page: 1, thumbnail: 1, total: 1, prefetch: 0, overviewThumbnailCeiling: 1 };
  }
  if (level === 2) {
    return { page: 2, thumbnail: 1, total: 2, prefetch: 0, overviewThumbnailCeiling: 2 };
  }
  return { page: 2, thumbnail: 2, total: 3, prefetch: 1, overviewThumbnailCeiling: null };
}

interface RenderRequestOptions {
  signal?: AbortSignal;
  priority?: number;
  urgency?: RenderUrgency;
  requestClass?: RenderRequestClass;
  abortStartedRender?: boolean;
  cropPdfRect?: PdfRect;
  rotation?: PageModel['rotation'];
}

interface QueuedRenderTask {
  cacheKey: string;
  priority: number;
  urgency: RenderUrgency;
  requestClass: RenderRequestClass;
  promotionOriginClass: RenderRequestClass | null;
  enqueuedAt: number;
  startedAt: number | null;
  retainedAt: number | null;
  retainedUntil: number | null;
  signal?: AbortSignal;
  startedAbortController: AbortController;
  started: boolean;
  run: (task: QueuedRenderTask) => Promise<unknown>;
  resolve: (value: any) => void;
  reject: (error: unknown) => void;
  abortHandler?: () => void;
}

export interface DiagnosticsSnapshot {
  pageRendererMode: 'raster' | 'tiled-raster' | 'vector-spike';
  cadRenderExperiment: string | null;
  renderCacheEntries: number;
  renderCacheBytes: number;
  thumbnailCacheEntries: number;
  thumbnailCacheBytes: number;
  pageRenderReady: boolean;
  thumbnailRenderReady: boolean;
  firstVisiblePageIndex: number | null;
  firstVisiblePageReady: boolean;
  firstVisiblePageReadyAtMs: number | null;
  firstVisiblePageReadyRequestClass: RenderRequestClass | null;
  firstVisiblePageWarmupStatus: 'idle' | 'queued' | 'ready' | 'aborted' | 'error';
  lastPageRenderError: string | null;
  lastThumbnailRenderError: string | null;
  sessionBackendKind: 'pdfjs' | null;
  surfaceTransportKind: 'pdfjs-blob-url' | null;
  openStageTimings: DocumentOpenStageTimings | null;
  inactive: boolean;
  viewportInMotion: boolean;
  thumbnailListInMotion: boolean;
  queuedPageRenders: number;
  queuedThumbnailRenders: number;
  inflightPageRenders: number;
  inflightThumbnailRenders: number;
  overviewThumbnailConcurrencyLimit: number;
  overviewThumbnailConcurrencyCeiling: number;
  overviewThumbnailLastThroughputPerSecond: number | null;
  overviewThumbnailBestThroughputPerSecond: number | null;
  snapIndexBuilds: number;
  snapIndexPrimitiveCount: number;
  totalSnapIndexBuildMs: number;
  lastSnapIndexBuildMs: number | null;
  lastSnapIndexPageIndex: number | null;
  deepZoomRenderCount: number;
  lastDeepZoomRenderMs: number | null;
}

export function isRenderBacklogIdle(diagnostics: Pick<
  DiagnosticsSnapshot,
  | 'pageRenderReady'
  | 'thumbnailRenderReady'
  | 'queuedPageRenders'
  | 'queuedThumbnailRenders'
  | 'inflightPageRenders'
  | 'inflightThumbnailRenders'
>): boolean {
  return diagnostics.pageRenderReady
    && diagnostics.thumbnailRenderReady
    && diagnostics.queuedPageRenders === 0
    && diagnostics.queuedThumbnailRenders === 0
    && diagnostics.inflightPageRenders === 0
    && diagnostics.inflightThumbnailRenders === 0;
}

export function isRenderTaskAllowedDuringActivationCriticalWindow(
  kind: 'page' | 'thumbnail',
  task: { readonly urgency: 'visible' | 'prefetch'; readonly requestClass: RenderRequestClass },
  schedulerIsolation = false,
): boolean {
  if (kind === 'page') {
    if (
      task.requestClass === 'target-page-preview'
      || task.requestClass === 'target-page-hq'
      || task.requestClass === 'target-page-crop'
    ) {
      return true;
    }

    if (schedulerIsolation) {
      return task.urgency === 'visible' && task.requestClass === 'visible-page-preview';
    }

    return task.urgency !== 'prefetch' && task.requestClass !== 'warming';
  }

  return task.requestClass === 'visible-thumbnail' || task.requestClass === 'overview-thumbnail';
}

export class LocalPdfSession {
  private filePath: string;
  private fileName: string;
  private documentHandle: PdfSessionDocumentHandle | null = null;
  private documentAccessHandle: string | null = null;
  private readonly renderCache = new Map<string, PageRenderCacheEntry>();
  private readonly retiredPageRenderCache = new Map<string, PageRenderCacheEntry>();
  private readonly pageUrlCache = new Map<string, ThumbnailCacheEntry>();
  private readonly thumbnailCache = new Map<string, ThumbnailCacheEntry>();
  private readonly inflightPageRenders = new Map<string, Promise<PageRenderCacheEntry>>();
  private readonly inflightPageUrlRenders = new Map<string, Promise<string>>();
  private readonly pendingThumbnailRenders = new Map<string, Promise<string>>();
  private readonly inflightThumbnailRenders = new Map<string, Promise<string>>();
  private readonly pageGeometryIndexCache = new Map<number, Promise<PdfPageGeometryIndex>>();
  private readonly heldPageSurfaces = new WeakSet<PageRenderSurface>();
  private readonly heldObjectUrlCounts = new Map<string, number>();
  private readonly primedPagePreviews = new Map<number, string>();
  private readonly pageRenderQueue: QueuedRenderTask[] = [];
  private readonly thumbnailRenderQueue: QueuedRenderTask[] = [];
  private readonly activeThumbnailRenderTasks = new Set<QueuedRenderTask>();
  private inactive = false;
  private disposed = false;
  private readonly maxRenderCacheEntries = 8;
  private readonly maxRenderCacheBytes = 120 * 1024 * 1024;
  private readonly maxBitmapRenderCacheEntries = 2;
  private readonly maxBitmapRenderCacheBytes = 32 * 1024 * 1024;
  private readonly maxThumbnailCacheEntries = 1200;
  private readonly maxThumbnailCacheBytes = 64 * 1024 * 1024;
  private readonly maxConcurrentThumbnailRenders = 2;
  private readonly maxConcurrentTotalRenders = 3;
  private readonly maxConcurrentPrefetchPageRenders = 1;
  private readonly maxConcurrentPrefetchThumbnailRenders = 1;
  private overviewThumbnailConcurrencyLimit: number | null = null;
  private adaptivePerformanceLevel: AdaptivePerformanceLevel = 0;
  private overviewThumbnailWindowStartedAt: number | null = null;
  private overviewThumbnailWindowCompletions = 0;
  private overviewThumbnailWindowDurationMs = 0;
  private overviewThumbnailLastThroughputPerSecond: number | null = null;
  private overviewThumbnailBestThroughputPerSecond: number | null = null;
  private readonly listeners = new Set<() => void>();
  private versionCounter = 0;
  private activePageRenderCount = 0;
  private activeThumbnailRenderCount = 0;
  private activePrefetchPageRenderCount = 0;
  private activePrefetchThumbnailRenderCount = 0;
  private resourceEpoch = 0;
  private previewReuseEnabled = false;
  private viewportInMotion = false;
  private thumbnailListInMotion = false;
  private activationCriticalUntil = 0;
  private activationCriticalTimer: ReturnType<typeof setTimeout> | null = null;
  private thumbnailQueuePumpScheduled = false;
  private navigationIntentPageIndex: number | null = null;
  private navigationIntentSource: NavigationIntentSource = 'generic';
  private navigationIntentExpiresAt = 0;
  private pageRenderReady = false;
  private thumbnailRenderReady = false;
  private firstVisiblePageIndex: number | null = null;
  private firstVisiblePageReady = false;
  private firstVisiblePageReadyAtMs: number | null = null;
  private firstVisiblePageReadyRequestClass: RenderRequestClass | null = null;
  private firstVisiblePageWarmupStatus: DiagnosticsSnapshot['firstVisiblePageWarmupStatus'] = 'idle';
  private openStartedAt: number | null = null;
  private lastPageRenderError: string | null = null;
  private lastThumbnailRenderError: string | null = null;
  private sessionBackendKind: 'pdfjs' | null = null;
  private surfaceTransportKind: 'pdfjs-blob-url' | null = null;
  private openStageTimings: DocumentOpenStageTimings | null = null;
  private snapIndexBuilds = 0;
  private snapIndexPrimitiveCount = 0;
  private totalSnapIndexBuildMs = 0;
  private lastSnapIndexBuildMs: number | null = null;
  private lastSnapIndexPageIndex: number | null = null;
  private deepZoomRenderCount = 0;
  private lastDeepZoomRenderMs: number | null = null;

  constructor(filePath: string, private readonly backend: PdfSessionBackend = createDesktopSessionBackend()) {
    this.filePath = filePath;
    this.fileName = filePath.split(/[/\\]/).pop() ?? filePath;
  }

  get version(): number {
    return this.versionCounter;
  }

  getDocumentAccessHandle(): string {
    return this.requireDocumentAccessHandle();
  }

  async open(): Promise<LoadedDocumentPayload> {
    this.disposed = false;
    await this.disposeDocumentResources();
    const openEpoch = this.resourceEpoch;

    const openStartedAt = performance.now();
    this.openStartedAt = openStartedAt;
    const { payload, handle, backendInfo, openStageTimings } = await this.backend.open(this.filePath);
    this.filePath = payload.filePath;
    this.fileName = payload.fileName;

    if (!this.isResourceEpochCurrent(openEpoch)) {
      await handle.close();
      await window.butterPaper.pdf.releaseDocument({
        documentHandle: payload.documentAccess.handle,
      }).catch(() => undefined);
      throw createAbortError();
    }

    this.documentHandle = handle;
    this.documentAccessHandle = payload.documentAccess.handle;
    this.previewReuseEnabled = false;
    this.viewportInMotion = false;
    this.thumbnailListInMotion = false;
    this.clearActivationCriticalWindow();
    this.navigationIntentPageIndex = null;
    this.navigationIntentSource = 'generic';
    this.navigationIntentExpiresAt = 0;
    this.pageRenderReady = false;
    this.thumbnailRenderReady = false;
    this.firstVisiblePageIndex = payload.document.pages[0]?.index ?? null;
    this.firstVisiblePageReady = false;
    this.firstVisiblePageReadyAtMs = null;
    this.firstVisiblePageReadyRequestClass = null;
    this.firstVisiblePageWarmupStatus = 'idle';
    this.lastPageRenderError = null;
    this.lastThumbnailRenderError = null;
    this.sessionBackendKind = backendInfo.sessionBackendKind;
    this.surfaceTransportKind = backendInfo.surfaceTransportKind;
    this.openStageTimings = {
      mainPayloadMs: roundDuration(openStageTimings.mainPayloadMs),
      mainMetadataMs: payload.openStageTimings?.mainMetadataMs,
      mainPageModelMs: payload.openStageTimings?.mainPageModelMs,
      mainAnnotationReadMs: payload.openStageTimings?.mainAnnotationReadMs,
      rendererFileReadMs: roundDuration(openStageTimings.rendererFileReadMs),
      rendererBrowserOpenMs: roundDuration(openStageTimings.rendererBrowserOpenMs),
      totalOpenMs: roundDuration(performance.now() - openStartedAt),
    };
    this.pageGeometryIndexCache.clear();
    this.snapIndexBuilds = 0;
    this.snapIndexPrimitiveCount = 0;
    this.totalSnapIndexBuildMs = 0;
    this.lastSnapIndexBuildMs = null;
    this.lastSnapIndexPageIndex = null;
    this.deepZoomRenderCount = 0;
    this.lastDeepZoomRenderMs = null;
    this.bumpVersion();
    return {
      ...payload,
      openStageTimings: this.openStageTimings,
    };
  }

  async save(
    markups: readonly Markup[],
    target?: PdfSaveTargetDescriptor,
    pageScales?: readonly PageScale[],
    pages?: readonly PageModel[],
  ): Promise<LoadedDocumentPayload> {
    const result = await this.backend.save({
      documentHandle: this.requireDocumentAccessHandle(),
      target,
      markups,
      pageScales,
      pageRotations: pages?.map((page) => ({ pageIndex: page.index, rotation: page.rotation })),
    });

    await this.disposeDocumentResources();
    this.filePath = result.path;
    this.fileName = result.path.split(/[/\\]/).pop() ?? result.path;
    return await this.open();
  }

  async renderPage(
    pageIndex: number,
    zoom: number,
    pixelRatio = window.devicePixelRatio || 1,
    options: RenderRequestOptions = {},
  ): Promise<string> {
    const scale = Math.max(0.1, zoom) * pixelRatio;
    const cacheKey = this.createPageCacheKey(pageIndex, zoom, pixelRatio, 'url', options.cropPdfRect, options.rotation);
    const cached = this.pageUrlCache.get(cacheKey);
    if (cached) {
      cached.lastUsedAt = performance.now();
      if (cached.sourceRequestClass && options.requestClass) {
        recordCacheReuseFromPrefetch('page', cached.sourceRequestClass, options.requestClass);
      }
      recordRasterRequest('page', 'hit', options.requestClass);
      return cached.objectUrl;
    }

    const inflight = this.inflightPageUrlRenders.get(cacheKey);
    if (inflight) {
      recordRasterRequest('page', 'hit', options.requestClass);
      return inflight;
    }

    const retainedQueuedTask = this.reattachRetainedQueuedRenderTask<string>('page', cacheKey, options);
    if (retainedQueuedTask) {
      recordRasterRequest('page', 'hit', options.requestClass);
      return retainedQueuedTask;
    }

    const adoptedQueuedTask = this.adoptCompatibleQueuedRenderTask<string>('page', cacheKey, options);
    if (adoptedQueuedTask) {
      recordRasterRequest('page', 'hit', options.requestClass);
      return adoptedQueuedTask;
    }

    recordRasterRequest('page', 'miss', options.requestClass);

    const requestEpoch = this.resourceEpoch;
    const renderStartedAt = performance.now();
    let renderPromise!: Promise<string>;
    renderPromise = this.enqueueRenderTask<string>(
      'page',
      cacheKey,
      async (task) => {
        this.inflightPageUrlRenders.set(cacheKey, renderPromise);
        const requestClass = task.requestClass;

        try {
          const handle = this.requireDocumentHandle();
          this.throwIfResourceStale(requestEpoch, handle);
          const rendered = await handle.renderPageToBlob({
            pageIndex,
            scale,
            rotation: options.rotation,
            signal: options.abortStartedRender ? resolveStartedRenderSignal(task) : undefined,
            requestClass,
            cropPdfRect: options.cropPdfRect,
          });
          this.throwIfResourceStale(requestEpoch, handle);
          const objectUrl = URL.createObjectURL(rendered.blob);
          if (!this.isResourceEpochCurrent(requestEpoch, handle)) {
            URL.revokeObjectURL(objectUrl);
            throw createAbortError();
          }
          this.pageUrlCache.set(cacheKey, {
            pageIndex,
            rotation: options.rotation,
            objectUrl,
            byteSize: rendered.blob.size,
            renderedWidth: rendered.width,
            renderedHeight: rendered.height,
            cropPdfRect: options.cropPdfRect,
            sourceRequestClass: requestClass,
            lastUsedAt: performance.now(),
            protectedUntil: resolveFreshImageUrlProtectionUntil(requestClass),
          });
          this.enforceCacheBudget(
            this.pageUrlCache,
            this.maxRenderCacheEntries,
            this.maxRenderCacheBytes,
            (entry) => this.isProtectedPageUrlCacheEntry(entry),
          );
          if (
            (requestClass === 'target-page-hq' || requestClass === 'target-page-crop')
            && this.navigationIntentPageIndex === pageIndex
          ) {
            this.navigationIntentPageIndex = null;
            this.navigationIntentExpiresAt = 0;
          }
          this.pageRenderReady = true;
          this.markFirstVisiblePageReady(pageIndex, requestClass ?? null);
          this.lastPageRenderError = null;
          recordRasterComplete('page', requestClass);
          this.recordDeepZoomRender(zoom, renderStartedAt);
          this.bumpVersion();
          return objectUrl;
        } catch (error) {
          if (!isExpectedRenderInterruption(error) && this.isResourceEpochCurrent(requestEpoch)) {
            this.lastPageRenderError = toErrorMessage(error, 'Unable to render page.');
            recordRasterError('page', requestClass);
            this.bumpVersion();
          }
          throw error;
        } finally {
          if (this.inflightPageUrlRenders.get(cacheKey) === renderPromise) {
            this.inflightPageUrlRenders.delete(cacheKey);
          }
        }
      },
      options,
    );

    return renderPromise;
  }

  async renderPageBitmap(
    pageIndex: number,
    zoom: number,
    pixelRatio = window.devicePixelRatio || 1,
    options: RenderRequestOptions = {},
  ): Promise<PageRenderSurface> {
    const scale = Math.max(0.1, zoom) * pixelRatio;
    const cacheKey = this.createPageCacheKey(pageIndex, zoom, pixelRatio, 'bitmap', options.cropPdfRect, options.rotation);
    const cached = this.renderCache.get(cacheKey);
    if (cached) {
      cached.lastUsedAt = performance.now();
      if (cached.sourceRequestClass && options.requestClass) {
        recordCacheReuseFromPrefetch('page', cached.sourceRequestClass, options.requestClass);
      }
      recordRasterRequest('page', 'hit', options.requestClass);
      return this.acquirePageRenderSurfaceWithBudget(cached);
    }

    const inflight = this.inflightPageRenders.get(cacheKey);
    if (inflight) {
      recordRasterRequest('page', 'hit', options.requestClass);
      return inflight.then((entry) => this.acquirePageRenderSurfaceWithBudget(entry));
    }

    const retainedQueuedTask = this.reattachRetainedQueuedRenderTask<PageRenderCacheEntry>('page', cacheKey, options);
    if (retainedQueuedTask) {
      recordRasterRequest('page', 'hit', options.requestClass);
      return retainedQueuedTask.then((entry) => this.acquirePageRenderSurfaceWithBudget(entry));
    }

    const adoptedQueuedTask = this.adoptCompatibleQueuedRenderTask<PageRenderCacheEntry>('page', cacheKey, options);
    if (adoptedQueuedTask) {
      recordRasterRequest('page', 'hit', options.requestClass);
      return adoptedQueuedTask.then((entry) => this.acquirePageRenderSurfaceWithBudget(entry));
    }

    recordRasterRequest('page', 'miss', options.requestClass);

    const requestEpoch = this.resourceEpoch;
    const renderStartedAt = performance.now();
    let inflightPromise!: Promise<PageRenderCacheEntry>;
    const entryPromise = this.enqueueRenderTask<PageRenderCacheEntry>(
      'page',
      cacheKey,
      async (task) => {
        this.inflightPageRenders.set(cacheKey, inflightPromise);
        const requestClass = task.requestClass;
        let renderedBitmap: ImageBitmap | null = null;

        try {
          const handle = this.requireDocumentHandle();
          this.throwIfResourceStale(requestEpoch, handle);
          const rendered = await handle.renderPageToBitmap({
            pageIndex,
            scale,
            rotation: options.rotation,
            signal: options.abortStartedRender ? resolveStartedRenderSignal(task) : undefined,
            requestClass,
            cropPdfRect: options.cropPdfRect,
          });
          renderedBitmap = rendered.bitmap;
          this.throwIfResourceStale(requestEpoch, handle);
          if (!this.isResourceEpochCurrent(requestEpoch, handle)) {
            renderedBitmap.close();
            renderedBitmap = null;
            throw createAbortError();
          }

          const entry: PageRenderCacheEntry = {
            cacheKey,
            pageIndex,
            rotation: options.rotation,
            bitmap: renderedBitmap,
            byteSize: estimateBitmapByteSize(rendered.width, rendered.height),
            renderedWidth: rendered.width,
            renderedHeight: rendered.height,
            cropPdfRect: options.cropPdfRect,
            sourceRequestClass: requestClass,
            lastUsedAt: performance.now(),
            refCount: 0,
            pendingClose: false,
          };
          this.renderCache.set(cacheKey, entry);
          renderedBitmap = null;
          if (
            (requestClass === 'target-page-hq' || requestClass === 'target-page-crop')
            && this.navigationIntentPageIndex === pageIndex
          ) {
            this.navigationIntentPageIndex = null;
            this.navigationIntentExpiresAt = 0;
          }
          this.pageRenderReady = true;
          this.markFirstVisiblePageReady(pageIndex, requestClass ?? null);
          this.lastPageRenderError = null;
          recordRasterComplete('page', requestClass);
          this.recordDeepZoomRender(zoom, renderStartedAt);
          this.bumpVersion();
          return entry;
        } catch (error) {
          if (renderedBitmap) {
            renderedBitmap.close();
          }
          if (!isExpectedRenderInterruption(error) && this.isResourceEpochCurrent(requestEpoch)) {
            this.lastPageRenderError = toErrorMessage(error, 'Unable to render page.');
            recordRasterError('page', requestClass);
            this.bumpVersion();
          }
          throw error;
        } finally {
          if (this.inflightPageRenders.get(cacheKey) === inflightPromise) {
            this.inflightPageRenders.delete(cacheKey);
          }
        }
      },
      options,
    );
    inflightPromise = entryPromise;

    return entryPromise.then((entry) => this.acquirePageRenderSurfaceWithBudget(entry));
  }

  async renderThumbnail(
    pageIndex: number,
    optionsOrWidth: number | ThumbnailRenderOptions = 156,
    pixelRatio = window.devicePixelRatio || 1,
    requestOptions: RenderRequestOptions = {},
  ): Promise<string> {
    const thumbnailOptions = normaliseThumbnailOptions(optionsOrWidth, pixelRatio);
    const cacheKey = this.createThumbnailCacheKey(pageIndex, thumbnailOptions, requestOptions.rotation);
    const cached = this.thumbnailCache.get(cacheKey);
    if (cached) {
      cached.lastUsedAt = performance.now();
      if (cached.sourceRequestClass && requestOptions.requestClass) {
        recordCacheReuseFromPrefetch('thumbnail', cached.sourceRequestClass, requestOptions.requestClass);
      }
      recordRasterRequest('thumbnail', 'hit', requestOptions.requestClass);
      return cached.objectUrl;
    }

    const inflight = this.inflightThumbnailRenders.get(cacheKey);
    if (inflight) {
      recordRasterRequest('thumbnail', 'hit', requestOptions.requestClass);
      return inflight;
    }

    const pending = this.pendingThumbnailRenders.get(cacheKey);
    if (pending) {
      this.reprioritiseQueuedRenderTask('thumbnail', cacheKey, requestOptions);
      recordRasterRequest('thumbnail', 'hit', requestOptions.requestClass);
      return pending;
    }

    recordRasterRequest('thumbnail', 'miss', requestOptions.requestClass);

    const requestEpoch = this.resourceEpoch;
    let renderPromise!: Promise<string>;
    renderPromise = this.enqueueRenderTask(
      'thumbnail',
      cacheKey,
      async (task) => {
        this.inflightThumbnailRenders.set(cacheKey, renderPromise);
        const requestClass = task.requestClass;

        try {
          const handle = this.requireDocumentHandle();
          this.throwIfResourceStale(requestEpoch, handle);
          const pageWidth = thumbnailOptions.pageWidth;
          const pageHeight = thumbnailOptions.pageHeight;
          const fallbackPageInfo =
            pageWidth !== undefined && pageHeight !== undefined ? null : await handle.getPageInfo(pageIndex);
          this.throwIfResourceStale(requestEpoch, handle);
          const scale = Math.max(
            thumbnailOptions.minScale,
            Math.min(
              thumbnailOptions.maxWidth / Math.max(1, pageWidth ?? fallbackPageInfo?.width ?? 0),
              thumbnailOptions.maxHeight / Math.max(1, pageHeight ?? fallbackPageInfo?.height ?? 0),
            ) * thumbnailOptions.pixelRatio,
          );
          const rendered = await handle.renderPageToBlob({
            pageIndex,
            scale,
            rotation: requestOptions.rotation,
            signal: resolveStartedRenderSignal(task, requestOptions.abortStartedRender),
            requestClass,
          });
          this.throwIfResourceStale(requestEpoch, handle);
          const objectUrl = URL.createObjectURL(rendered.blob);
          if (!this.isResourceEpochCurrent(requestEpoch, handle)) {
            URL.revokeObjectURL(objectUrl);
            throw createAbortError();
          }
          this.thumbnailCache.set(cacheKey, {
            pageIndex,
            rotation: requestOptions.rotation,
            objectUrl,
            byteSize: rendered.blob.size,
            renderedWidth: rendered.width,
            renderedHeight: rendered.height,
            sourceRequestClass: requestClass,
            lastUsedAt: performance.now(),
            protectedUntil: resolveFreshImageUrlProtectionUntil(requestClass),
          });
          this.enforceCacheBudget(this.thumbnailCache, this.maxThumbnailCacheEntries, this.maxThumbnailCacheBytes);
          this.thumbnailRenderReady = true;
          this.lastThumbnailRenderError = null;
          recordRasterComplete('thumbnail', requestClass);
          this.bumpVersion();
          return objectUrl;
        } catch (error) {
          if (!isExpectedRenderInterruption(error) && this.isResourceEpochCurrent(requestEpoch)) {
            this.lastThumbnailRenderError = toErrorMessage(error, 'Unable to render thumbnail.');
            recordRasterError('thumbnail', requestClass);
            this.bumpVersion();
          }
          throw error;
        } finally {
          if (this.inflightThumbnailRenders.get(cacheKey) === renderPromise) {
            this.inflightThumbnailRenders.delete(cacheKey);
          }
        }
      },
      requestOptions,
    );
    this.pendingThumbnailRenders.set(cacheKey, renderPromise);
    renderPromise.then(() => {
      if (this.pendingThumbnailRenders.get(cacheKey) === renderPromise) {
        this.pendingThumbnailRenders.delete(cacheKey);
      }
    }, () => {
      if (this.pendingThumbnailRenders.get(cacheKey) === renderPromise) {
        this.pendingThumbnailRenders.delete(cacheKey);
      }
    });

    return renderPromise;
  }

  updatePageRenderPriority(
    pageIndex: number,
    zoom: number,
    pixelRatio = window.devicePixelRatio || 1,
    options: Pick<RenderRequestOptions, 'priority' | 'urgency' | 'requestClass' | 'rotation'> = {},
  ): void {
    this.reprioritiseQueuedRenderTask(
      'page',
      this.createPageCacheKey(pageIndex, zoom, pixelRatio, 'url', undefined, options.rotation),
      options,
    );
  }

  updatePageBitmapRenderPriority(
    pageIndex: number,
    zoom: number,
    pixelRatio = window.devicePixelRatio || 1,
    options: Pick<RenderRequestOptions, 'priority' | 'urgency' | 'requestClass' | 'rotation'> = {},
  ): void {
    this.reprioritiseQueuedRenderTask(
      'page',
      this.createPageCacheKey(pageIndex, zoom, pixelRatio, 'bitmap', undefined, options.rotation),
      options,
    );
  }

  updateThumbnailRenderPriority(
    pageIndex: number,
    optionsOrWidth: number | ThumbnailRenderOptions = 156,
    pixelRatio = window.devicePixelRatio || 1,
    options: Pick<RenderRequestOptions, 'priority' | 'urgency' | 'requestClass' | 'rotation'> = {},
  ): void {
    const thumbnailOptions = normaliseThumbnailOptions(optionsOrWidth, pixelRatio);
    this.reprioritiseQueuedRenderTask(
      'thumbnail',
      this.createThumbnailCacheKey(pageIndex, thumbnailOptions, options.rotation),
      options,
    );
  }

  setViewportInMotion(inMotion: boolean): void {
    if (this.viewportInMotion === inMotion) {
      return;
    }

    this.viewportInMotion = inMotion;
    this.pumpAllRenderQueues();
  }

  setThumbnailListInMotion(inMotion: boolean): void {
    if (this.thumbnailListInMotion === inMotion) {
      return;
    }

    this.thumbnailListInMotion = inMotion;
    this.pumpAllRenderQueues();
  }

  setAdaptivePerformanceLevel(level: AdaptivePerformanceLevel): void {
    if (this.adaptivePerformanceLevel === level) {
      return;
    }
    this.adaptivePerformanceLevel = level;
    if (level >= 2) {
      this.clearQueuedRenderTasks(this.pageRenderQueue, (task) => task.urgency === 'prefetch' || task.requestClass === 'warming');
      this.clearQueuedRenderTasks(this.thumbnailRenderQueue, (task) => task.urgency === 'prefetch' || task.requestClass === 'warming');
    }
    this.pumpAllRenderQueues();
  }

  setNavigationIntent(pageIndex: number, durationMs = 2500, source: NavigationIntentSource = 'generic'): void {
    this.navigationIntentPageIndex = pageIndex;
    this.navigationIntentSource = source;
    this.navigationIntentExpiresAt = performance.now() + durationMs;
    if (source === 'thumbnail') {
      this.startActivationCriticalWindow();
      return;
    }
    this.pumpAllRenderQueues();
  }

  async warmFirstVisiblePage(
    pageIndex = this.firstVisiblePageIndex ?? 0,
    zoom = 0.5,
    pixelRatio = window.devicePixelRatio || 1,
  ): Promise<void> {
    if (this.disposed || !this.documentHandle || this.inactive === false || this.firstVisiblePageReady) {
      return;
    }

    if (this.getReusablePagePreviewInfo(pageIndex) || this.findReusablePageSurface(pageIndex, 0)) {
      this.markFirstVisiblePageReady(pageIndex, 'warming');
      this.firstVisiblePageWarmupStatus = 'ready';
      return;
    }

    this.firstVisiblePageWarmupStatus = 'queued';
    this.bumpVersion();

    try {
      await this.renderPage(pageIndex, zoom, pixelRatio, {
        priority: -100,
        urgency: 'prefetch',
        requestClass: 'warming',
      });
      this.firstVisiblePageWarmupStatus = 'ready';
      this.markFirstVisiblePageReady(pageIndex, 'warming');
    } catch (error) {
      this.firstVisiblePageWarmupStatus = isAbortError(error) ? 'aborted' : 'error';
    } finally {
      this.bumpVersion();
    }
  }

  isNavigationIntentPage(pageIndex: number): boolean {
    this.clearExpiredNavigationIntent();
    return this.navigationIntentPageIndex === pageIndex;
  }

  isThumbnailNavigationIntentPage(pageIndex: number): boolean {
    this.clearExpiredNavigationIntent();
    return this.navigationIntentPageIndex === pageIndex && this.navigationIntentSource === 'thumbnail';
  }

  enablePreviewReuse(): void {
    this.previewReuseEnabled = true;
  }

  isPreviewReuseEnabled(): boolean {
    return this.previewReuseEnabled;
  }

  primePagePreview(pageIndex: number, objectUrl: string | null): void {
    if (!objectUrl) {
      return;
    }

    this.primedPagePreviews.set(pageIndex, objectUrl);
  }

  consumePrimedPagePreview(pageIndex: number): string | null {
    const objectUrl = this.primedPagePreviews.get(pageIndex) ?? null;
    if (objectUrl) {
      this.primedPagePreviews.delete(pageIndex);
    }
    return objectUrl;
  }

  hasReusablePagePreview(pageIndex: number, minimumWidth = 0, rotation?: PageModel['rotation']): boolean {
    return this.findReusablePagePreview(pageIndex, minimumWidth, rotation) !== null;
  }

  getReusablePagePreview(pageIndex: number, minimumWidth = 0, rotation?: PageModel['rotation']): string | null {
    return this.getReusablePagePreviewInfo(pageIndex, minimumWidth, rotation)?.objectUrl ?? null;
  }

  getReusablePageSurface(pageIndex: number, minimumWidth = 0, rotation?: PageModel['rotation']): PageRenderSurface | null {
    const selectedEntry = this.findReusablePageSurface(pageIndex, minimumWidth, rotation);
    if (!selectedEntry) {
      return null;
    }

    selectedEntry.lastUsedAt = performance.now();
    return this.acquirePageRenderSurface(selectedEntry);
  }

  getBestReusablePageImage(pageIndex: number, desiredWidth: number, rotation?: PageModel['rotation']): ReusablePageImage | null {
    const safeDesiredWidth = Number.isFinite(desiredWidth) && desiredWidth > 0 ? desiredWidth : 0;
    const bitmapEntry = this.findBestPageBitmapImage(pageIndex, safeDesiredWidth, rotation);
    if (bitmapEntry) {
      bitmapEntry.lastUsedAt = performance.now();
      return {
        kind: 'surface',
        pageIndex,
        renderedWidth: bitmapEntry.renderedWidth,
        renderedHeight: bitmapEntry.renderedHeight,
        source: 'page-bitmap',
        sourceRequestClass: bitmapEntry.sourceRequestClass,
        surface: this.acquirePageRenderSurface(bitmapEntry),
      };
    }

    const pageUrlEntry = this.findBestPageUrlImage(pageIndex, safeDesiredWidth, rotation);
    if (pageUrlEntry) {
      pageUrlEntry.lastUsedAt = performance.now();
      return {
        kind: 'object-url',
        pageIndex,
        renderedWidth: pageUrlEntry.renderedWidth,
        renderedHeight: pageUrlEntry.renderedHeight,
        source: 'page-url',
        sourceRequestClass: pageUrlEntry.sourceRequestClass,
        objectUrl: pageUrlEntry.objectUrl,
      };
    }

    const thumbnailEntry = this.findBestThumbnailImage(pageIndex, rotation);
    if (!thumbnailEntry) {
      return null;
    }

    thumbnailEntry.lastUsedAt = performance.now();
    return {
      kind: 'object-url',
      pageIndex,
      renderedWidth: thumbnailEntry.renderedWidth,
      renderedHeight: thumbnailEntry.renderedHeight,
      source: 'thumbnail',
      sourceRequestClass: thumbnailEntry.sourceRequestClass,
      objectUrl: thumbnailEntry.objectUrl,
    };
  }

  getBestReusablePageImageAtLeast(pageIndex: number, minimumWidth: number, rotation?: PageModel['rotation']): ReusablePageImage | null {
    const safeMinimumWidth = Number.isFinite(minimumWidth) && minimumWidth > 0 ? minimumWidth : 0;
    const bitmapEntry = this.findBestPageBitmapImageAtLeast(pageIndex, safeMinimumWidth, rotation);
    if (bitmapEntry) {
      bitmapEntry.lastUsedAt = performance.now();
      return {
        kind: 'surface',
        pageIndex,
        renderedWidth: bitmapEntry.renderedWidth,
        renderedHeight: bitmapEntry.renderedHeight,
        source: 'page-bitmap',
        sourceRequestClass: bitmapEntry.sourceRequestClass,
        surface: this.acquirePageRenderSurface(bitmapEntry),
      };
    }

    const pageUrlEntry = this.findBestPageUrlImageAtLeast(pageIndex, safeMinimumWidth, rotation);
    if (!pageUrlEntry) {
      return null;
    }

    pageUrlEntry.lastUsedAt = performance.now();
    return {
      kind: 'object-url',
      pageIndex,
      renderedWidth: pageUrlEntry.renderedWidth,
      renderedHeight: pageUrlEntry.renderedHeight,
      source: 'page-url',
      sourceRequestClass: pageUrlEntry.sourceRequestClass,
      objectUrl: pageUrlEntry.objectUrl,
    };
  }

  getBestReusableThumbnailImage(pageIndex: number, desiredWidth: number, rotation?: PageModel['rotation']): ReusablePageImage | null {
    const safeDesiredWidth = Number.isFinite(desiredWidth) && desiredWidth > 0 ? desiredWidth : 0;
    const thumbnailEntry = this.findBestReusableImageEntry(this.thumbnailCache.values(), pageIndex, safeDesiredWidth, rotation);
    if (!thumbnailEntry) {
      return null;
    }

    thumbnailEntry.lastUsedAt = performance.now();
    return {
      kind: 'object-url',
      pageIndex,
      renderedWidth: thumbnailEntry.renderedWidth,
      renderedHeight: thumbnailEntry.renderedHeight,
      source: 'thumbnail',
      sourceRequestClass: thumbnailEntry.sourceRequestClass,
      objectUrl: thumbnailEntry.objectUrl,
    };
  }

  releasePageSurface(surface: PageRenderSurface | null): void {
    if (!surface || !this.heldPageSurfaces.has(surface)) {
      return;
    }

    this.heldPageSurfaces.delete(surface);
    const activeEntry = this.renderCache.get(surface.cacheKey);
    const retiredEntry = this.retiredPageRenderCache.get(surface.cacheKey);
    const entry = activeEntry?.bitmap === surface.bitmap
      ? activeEntry
      : (retiredEntry?.bitmap === surface.bitmap ? retiredEntry : null);
    if (!entry) {
      return;
    }

    entry.refCount = Math.max(0, entry.refCount - 1);
    if (entry.refCount === 0 && entry.pendingClose) {
      entry.bitmap.close();
      this.retiredPageRenderCache.delete(entry.cacheKey);
    }
  }

  retainPageImageUrl(objectUrl: string | null): void {
    if (!objectUrl) {
      return;
    }

    this.heldObjectUrlCounts.set(objectUrl, (this.heldObjectUrlCounts.get(objectUrl) ?? 0) + 1);
  }

  releasePageImageUrl(objectUrl: string | null): void {
    if (!objectUrl) {
      return;
    }

    const nextCount = (this.heldObjectUrlCounts.get(objectUrl) ?? 0) - 1;
    if (nextCount > 0) {
      this.heldObjectUrlCounts.set(objectUrl, nextCount);
      return;
    }

    this.heldObjectUrlCounts.delete(objectUrl);
    this.enforceCacheBudget(
      this.pageUrlCache,
      this.maxRenderCacheEntries,
      this.maxRenderCacheBytes,
      (entry) => this.isProtectedPageUrlCacheEntry(entry),
    );
    this.enforceCacheBudget(this.thumbnailCache, this.maxThumbnailCacheEntries, this.maxThumbnailCacheBytes);
  }

  discardPageImageUrl(objectUrl: string | null): void {
    if (!objectUrl) {
      return;
    }

    const nextCount = (this.heldObjectUrlCounts.get(objectUrl) ?? 0) - 1;
    if (nextCount > 0) {
      this.heldObjectUrlCounts.set(objectUrl, nextCount);
    } else {
      this.heldObjectUrlCounts.delete(objectUrl);
    }

    let removed = false;
    for (const [key, entry] of this.pageUrlCache.entries()) {
      if (entry.objectUrl === objectUrl) {
        this.pageUrlCache.delete(key);
        removed = true;
      }
    }
    for (const [key, entry] of this.thumbnailCache.entries()) {
      if (entry.objectUrl === objectUrl) {
        this.thumbnailCache.delete(key);
        removed = true;
      }
    }

    if (removed && !this.isHeldObjectUrl(objectUrl)) {
      URL.revokeObjectURL(objectUrl);
    }
    if (removed) {
      this.bumpVersion();
    }
  }

  getReusablePagePreviewInfo(
    pageIndex: number,
    minimumWidth = 0,
    rotation?: PageModel['rotation'],
  ): ReusablePagePreviewInfo | null {
    const thumbnailEntry = this.findReusablePagePreview(pageIndex, minimumWidth, rotation);
    if (thumbnailEntry) {
      thumbnailEntry.lastUsedAt = performance.now();
      return {
        objectUrl: thumbnailEntry.objectUrl,
        renderedWidth: thumbnailEntry.renderedWidth,
        renderedHeight: thumbnailEntry.renderedHeight,
        source: 'thumbnail',
        sourceRequestClass: thumbnailEntry.sourceRequestClass,
      };
    }

    const pageUrlEntry = this.findReusablePageUrlPreview(pageIndex, minimumWidth, rotation);
    if (!pageUrlEntry) {
      return null;
    }

    pageUrlEntry.lastUsedAt = performance.now();
    return {
      objectUrl: pageUrlEntry.objectUrl,
      renderedWidth: pageUrlEntry.renderedWidth,
      renderedHeight: pageUrlEntry.renderedHeight,
      source: 'page-url',
      sourceRequestClass: pageUrlEntry.sourceRequestClass,
    };
  }

  getReusablePagePreviewInfoAtLeast(
    pageIndex: number,
    minimumWidth: number,
    rotation?: PageModel['rotation'],
  ): ReusablePagePreviewInfo | null {
    const thumbnailEntry = this.findReusablePagePreviewAtLeast(pageIndex, minimumWidth, rotation);
    if (thumbnailEntry) {
      thumbnailEntry.lastUsedAt = performance.now();
      return {
        objectUrl: thumbnailEntry.objectUrl,
        renderedWidth: thumbnailEntry.renderedWidth,
        renderedHeight: thumbnailEntry.renderedHeight,
        source: 'thumbnail',
        sourceRequestClass: thumbnailEntry.sourceRequestClass,
      };
    }

    const pageUrlEntry = this.findReusablePageUrlPreviewAtLeast(pageIndex, minimumWidth, rotation);
    if (!pageUrlEntry) {
      return null;
    }

    pageUrlEntry.lastUsedAt = performance.now();
    return {
      objectUrl: pageUrlEntry.objectUrl,
      renderedWidth: pageUrlEntry.renderedWidth,
      renderedHeight: pageUrlEntry.renderedHeight,
      source: 'page-url',
      sourceRequestClass: pageUrlEntry.sourceRequestClass,
    };
  }

  async getPageGeometryIndex(pageIndex: number): Promise<PdfPageGeometryIndex> {
    const cached = this.pageGeometryIndexCache.get(pageIndex);
    if (cached) {
      return cached;
    }

    const buildStartedAt = performance.now();
    const promise = window.butterPaper.pdf.getPageGeometry({
      documentHandle: this.requireDocumentAccessHandle(),
      pageIndex,
    }).then((index) => {
      this.snapIndexBuilds += 1;
      this.snapIndexPrimitiveCount += index.primitives.length;
      this.lastSnapIndexBuildMs = index.buildMs || roundDuration(performance.now() - buildStartedAt);
      this.totalSnapIndexBuildMs = roundDuration(this.totalSnapIndexBuildMs + this.lastSnapIndexBuildMs);
      this.lastSnapIndexPageIndex = pageIndex;
      this.bumpVersion();
      return index;
    }).catch((error) => {
      this.pageGeometryIndexCache.delete(pageIndex);
      throw error;
    });

    this.pageGeometryIndexCache.set(pageIndex, promise);
    return promise;
  }

  diagnostics(): DiagnosticsSnapshot {
    return {
      pageRendererMode: resolvePageRendererMode(),
      cadRenderExperiment: window.butterPaper?.environment.cadRenderExperiment ?? null,
      renderCacheEntries: this.pageUrlCache.size + this.renderCache.size + this.retiredPageRenderCache.size,
      renderCacheBytes: this.pageCacheBytes(),
      thumbnailCacheEntries: this.thumbnailCache.size,
      thumbnailCacheBytes: this.cacheBytes(this.thumbnailCache),
      pageRenderReady: this.pageRenderReady,
      thumbnailRenderReady: this.thumbnailRenderReady,
      firstVisiblePageIndex: this.firstVisiblePageIndex,
      firstVisiblePageReady: this.firstVisiblePageReady,
      firstVisiblePageReadyAtMs: this.firstVisiblePageReadyAtMs,
      firstVisiblePageReadyRequestClass: this.firstVisiblePageReadyRequestClass,
      firstVisiblePageWarmupStatus: this.firstVisiblePageWarmupStatus,
      lastPageRenderError: this.lastPageRenderError,
      lastThumbnailRenderError: this.lastThumbnailRenderError,
      sessionBackendKind: this.sessionBackendKind,
      surfaceTransportKind: this.surfaceTransportKind,
      openStageTimings: this.openStageTimings,
      inactive: this.inactive,
      viewportInMotion: this.viewportInMotion,
      thumbnailListInMotion: this.thumbnailListInMotion,
      queuedPageRenders: this.pageRenderQueue.length,
      queuedThumbnailRenders: this.thumbnailRenderQueue.length,
      inflightPageRenders: this.inflightPageRenders.size + this.inflightPageUrlRenders.size,
      inflightThumbnailRenders: this.inflightThumbnailRenders.size,
      overviewThumbnailConcurrencyLimit: this.resolveOverviewThumbnailConcurrencyLimit(),
      overviewThumbnailConcurrencyCeiling: this.resolveOverviewThumbnailConcurrencyCeiling(),
      overviewThumbnailLastThroughputPerSecond: this.overviewThumbnailLastThroughputPerSecond,
      overviewThumbnailBestThroughputPerSecond: this.overviewThumbnailBestThroughputPerSecond,
      snapIndexBuilds: this.snapIndexBuilds,
      snapIndexPrimitiveCount: this.snapIndexPrimitiveCount,
      totalSnapIndexBuildMs: this.totalSnapIndexBuildMs,
      lastSnapIndexBuildMs: this.lastSnapIndexBuildMs,
      lastSnapIndexPageIndex: this.lastSnapIndexPageIndex,
      deepZoomRenderCount: this.deepZoomRenderCount,
      lastDeepZoomRenderMs: this.lastDeepZoomRenderMs,
    };
  }

  activate(): void {
    this.inactive = false;
    this.clearQueuedRenderTasks(this.pageRenderQueue, (task) => task.urgency === 'prefetch' || task.requestClass === 'warming');
    this.clearQueuedRenderTasks(this.thumbnailRenderQueue, (task) => task.urgency === 'prefetch' || task.requestClass === 'warming');
    this.startActivationCriticalWindow();
    if (this.firstVisiblePageIndex !== null) {
      this.setNavigationIntent(this.firstVisiblePageIndex, 1800, 'generic');
    } else {
      this.pumpAllRenderQueues();
    }
  }

  deactivate(): void {
    this.inactive = true;
    this.clearActivationCriticalWindow();
    this.clearQueuedRenderTasks(this.pageRenderQueue);
    this.clearQueuedRenderTasks(this.thumbnailRenderQueue);
  }

  trimInactiveCaches(): void {
    this.deactivate();
    for (const [key, entry] of this.renderCache.entries()) {
      this.renderCache.delete(key);
      if (entry.refCount > 0) {
        entry.pendingClose = true;
        this.retiredPageRenderCache.set(key, entry);
      } else {
        entry.bitmap.close();
      }
    }
    for (const entry of this.pageUrlCache.values()) {
      URL.revokeObjectURL(entry.objectUrl);
    }
    this.pageUrlCache.clear();
    for (const entry of this.thumbnailCache.values()) {
      URL.revokeObjectURL(entry.objectUrl);
    }
    this.thumbnailCache.clear();
    this.heldObjectUrlCounts.clear();
    this.primedPagePreviews.clear();
    this.pageGeometryIndexCache.clear();
    this.bumpVersion();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  dispose(): void {
    this.disposed = true;
    this.previewReuseEnabled = false;
    void this.disposeDocumentResources();
    this.bumpVersion();
  }

  private async disposeDocumentResources(): Promise<void> {
    this.resourceEpoch += 1;

    for (const entry of this.renderCache.values()) {
      entry.bitmap.close();
    }
    this.renderCache.clear();
    for (const entry of this.retiredPageRenderCache.values()) {
      entry.bitmap.close();
    }
    this.retiredPageRenderCache.clear();
    for (const entry of this.pageUrlCache.values()) {
      URL.revokeObjectURL(entry.objectUrl);
    }
    this.pageUrlCache.clear();
    for (const entry of this.thumbnailCache.values()) {
      URL.revokeObjectURL(entry.objectUrl);
    }
    this.thumbnailCache.clear();
    this.heldObjectUrlCounts.clear();
    this.clearQueuedRenderTasks(this.pageRenderQueue);
    this.clearQueuedRenderTasks(this.thumbnailRenderQueue);
    this.primedPagePreviews.clear();
    this.inflightPageRenders.clear();
    this.inflightPageUrlRenders.clear();
    this.pendingThumbnailRenders.clear();
    this.inflightThumbnailRenders.clear();
    for (const task of this.activeThumbnailRenderTasks) {
      task.startedAbortController.abort(createAbortError());
    }
    this.activeThumbnailRenderTasks.clear();
    this.pageGeometryIndexCache.clear();
    this.activePageRenderCount = 0;
    this.activeThumbnailRenderCount = 0;
    this.activePrefetchPageRenderCount = 0;
    this.activePrefetchThumbnailRenderCount = 0;
    await this.documentHandle?.close();
    this.documentHandle = null;
    const documentAccessHandle = this.documentAccessHandle;
    this.documentAccessHandle = null;
    if (documentAccessHandle) {
      await window.butterPaper.pdf.releaseDocument({ documentHandle: documentAccessHandle }).catch(() => undefined);
    }
    this.previewReuseEnabled = false;
    this.viewportInMotion = false;
    this.thumbnailListInMotion = false;
    this.clearActivationCriticalWindow();
    this.navigationIntentPageIndex = null;
    this.navigationIntentSource = 'generic';
    this.navigationIntentExpiresAt = 0;
    this.pageRenderReady = false;
    this.thumbnailRenderReady = false;
    this.firstVisiblePageIndex = null;
    this.firstVisiblePageReady = false;
    this.firstVisiblePageReadyAtMs = null;
    this.firstVisiblePageReadyRequestClass = null;
    this.firstVisiblePageWarmupStatus = 'idle';
    this.openStartedAt = null;
    this.lastPageRenderError = null;
    this.lastThumbnailRenderError = null;
    this.sessionBackendKind = null;
    this.surfaceTransportKind = null;
    this.openStageTimings = null;
    this.snapIndexBuilds = 0;
    this.snapIndexPrimitiveCount = 0;
    this.totalSnapIndexBuildMs = 0;
    this.lastSnapIndexBuildMs = null;
    this.lastSnapIndexPageIndex = null;
    this.deepZoomRenderCount = 0;
    this.lastDeepZoomRenderMs = null;
  }

  private requireDocumentAccessHandle(): string {
    if (!this.documentAccessHandle) {
      throw new Error('PDF document access is unavailable.');
    }
    return this.documentAccessHandle;
  }

  private recordDeepZoomRender(zoom: number, startedAt: number): void {
    if (zoom < 64) {
      return;
    }

    this.deepZoomRenderCount += 1;
    this.lastDeepZoomRenderMs = roundDuration(performance.now() - startedAt);
  }

  private requireDocumentHandle(): PdfSessionDocumentHandle {
    if (!this.documentHandle) {
      throw createRenderUnavailableError();
    }

    return this.documentHandle;
  }

  private isResourceEpochCurrent(epoch: number, handle?: PdfSessionDocumentHandle): boolean {
    return epoch === this.resourceEpoch && (!handle || this.documentHandle === handle);
  }

  private throwIfResourceStale(epoch: number, handle?: PdfSessionDocumentHandle): void {
    if (!this.isResourceEpochCurrent(epoch, handle)) {
      throw createAbortError();
    }
  }

  private findReusablePagePreview(
    pageIndex: number,
    minimumWidth: number,
    rotation?: PageModel['rotation'],
  ): ThumbnailCacheEntry | null {
    let fallbackEntry: ThumbnailCacheEntry | null = null;
    let bestEntry: ThumbnailCacheEntry | null = null;

    for (const entry of this.thumbnailCache.values()) {
      if (entry.pageIndex !== pageIndex || (rotation !== undefined && entry.rotation !== rotation)) {
        continue;
      }

      if (!fallbackEntry || entry.renderedWidth > fallbackEntry.renderedWidth) {
        fallbackEntry = entry;
      }

      if (entry.renderedWidth < minimumWidth) {
        continue;
      }

      if (!bestEntry || entry.renderedWidth > bestEntry.renderedWidth) {
        bestEntry = entry;
      }
    }

    return bestEntry ?? fallbackEntry;
  }

  private findReusablePagePreviewAtLeast(
    pageIndex: number,
    minimumWidth: number,
    rotation?: PageModel['rotation'],
  ): ThumbnailCacheEntry | null {
    let bestEntry: ThumbnailCacheEntry | null = null;

    for (const entry of this.thumbnailCache.values()) {
      if (
        entry.pageIndex !== pageIndex
        || (rotation !== undefined && entry.rotation !== rotation)
        || entry.renderedWidth < minimumWidth
      ) {
        continue;
      }

      if (!bestEntry || entry.renderedWidth > bestEntry.renderedWidth) {
        bestEntry = entry;
      }
    }

    return bestEntry;
  }

  private findReusablePageUrlPreview(
    pageIndex: number,
    minimumWidth: number,
    rotation?: PageModel['rotation'],
  ): ThumbnailCacheEntry | null {
    let fallbackEntry: ThumbnailCacheEntry | null = null;
    let bestEntry: ThumbnailCacheEntry | null = null;

    for (const entry of this.pageUrlCache.values()) {
      if (entry.pageIndex !== pageIndex || (rotation !== undefined && entry.rotation !== rotation)) {
        continue;
      }

      if (!fallbackEntry || entry.renderedWidth > fallbackEntry.renderedWidth) {
        fallbackEntry = entry;
      }

      if (entry.renderedWidth < minimumWidth) {
        continue;
      }

      if (!bestEntry || entry.renderedWidth > bestEntry.renderedWidth) {
        bestEntry = entry;
      }
    }

    return bestEntry ?? fallbackEntry;
  }

  private findReusablePageUrlPreviewAtLeast(
    pageIndex: number,
    minimumWidth: number,
    rotation?: PageModel['rotation'],
  ): ThumbnailCacheEntry | null {
    let bestEntry: ThumbnailCacheEntry | null = null;

    for (const entry of this.pageUrlCache.values()) {
      if (
        entry.pageIndex !== pageIndex
        || (rotation !== undefined && entry.rotation !== rotation)
        || entry.renderedWidth < minimumWidth
      ) {
        continue;
      }
      if (entry.cropPdfRect) {
        continue;
      }

      if (!bestEntry || entry.renderedWidth > bestEntry.renderedWidth) {
        bestEntry = entry;
      }
    }

    return bestEntry;
  }

  private findReusablePageSurface(
    pageIndex: number,
    minimumWidth: number,
    rotation?: PageModel['rotation'],
  ): PageRenderCacheEntry | null {
    let fallbackEntry: PageRenderCacheEntry | null = null;
    let bestEntry: PageRenderCacheEntry | null = null;

    for (const entry of this.renderCache.values()) {
      if (entry.pageIndex !== pageIndex || (rotation !== undefined && entry.rotation !== rotation)) {
        continue;
      }
      if (entry.cropPdfRect) {
        continue;
      }

      if (!fallbackEntry || entry.renderedWidth > fallbackEntry.renderedWidth) {
        fallbackEntry = entry;
      }

      if (entry.renderedWidth < minimumWidth) {
        continue;
      }

      if (!bestEntry || entry.renderedWidth > bestEntry.renderedWidth) {
        bestEntry = entry;
      }
    }

    return bestEntry ?? fallbackEntry;
  }

  private findBestPageBitmapImage(pageIndex: number, desiredWidth: number, rotation?: PageModel['rotation']): PageRenderCacheEntry | null {
    return this.findBestReusableImageEntry(this.renderCache.values(), pageIndex, desiredWidth, rotation);
  }

  private findBestPageBitmapImageAtLeast(pageIndex: number, minimumWidth: number, rotation?: PageModel['rotation']): PageRenderCacheEntry | null {
    return this.findBestReusableImageEntryAtLeast(this.renderCache.values(), pageIndex, minimumWidth, rotation);
  }

  private findBestPageUrlImage(pageIndex: number, desiredWidth: number, rotation?: PageModel['rotation']): ThumbnailCacheEntry | null {
    return this.findBestReusableImageEntry(this.pageUrlCache.values(), pageIndex, desiredWidth, rotation);
  }

  private findBestPageUrlImageAtLeast(pageIndex: number, minimumWidth: number, rotation?: PageModel['rotation']): ThumbnailCacheEntry | null {
    return this.findBestReusableImageEntryAtLeast(this.pageUrlCache.values(), pageIndex, minimumWidth, rotation);
  }

  private findBestThumbnailImage(pageIndex: number, rotation?: PageModel['rotation']): ThumbnailCacheEntry | null {
    let bestEntry: ThumbnailCacheEntry | null = null;
    for (const entry of this.thumbnailCache.values()) {
      if (entry.pageIndex !== pageIndex || (rotation !== undefined && entry.rotation !== rotation)) {
        continue;
      }

      if (!bestEntry || entry.renderedWidth > bestEntry.renderedWidth) {
        bestEntry = entry;
      }
    }

    return bestEntry;
  }

  private findBestReusableImageEntry<T extends { pageIndex: number; renderedWidth: number; rotation?: PageModel['rotation'] }>(
    entries: Iterable<T>,
    pageIndex: number,
    desiredWidth: number,
    rotation?: PageModel['rotation'],
  ): T | null {
    let smallestAdequateEntry: T | null = null;
    let widestLowerEntry: T | null = null;

    for (const entry of entries) {
      if (entry.pageIndex !== pageIndex || (rotation !== undefined && entry.rotation !== rotation)) {
        continue;
      }
      if ('cropPdfRect' in entry && entry.cropPdfRect) {
        continue;
      }

      if (entry.renderedWidth >= desiredWidth) {
        if (!smallestAdequateEntry || entry.renderedWidth < smallestAdequateEntry.renderedWidth) {
          smallestAdequateEntry = entry;
        }
        continue;
      }

      if (!widestLowerEntry || entry.renderedWidth > widestLowerEntry.renderedWidth) {
        widestLowerEntry = entry;
      }
    }

    return smallestAdequateEntry ?? widestLowerEntry;
  }

  private findBestReusableImageEntryAtLeast<T extends { pageIndex: number; renderedWidth: number; rotation?: PageModel['rotation'] }>(
    entries: Iterable<T>,
    pageIndex: number,
    minimumWidth: number,
    rotation?: PageModel['rotation'],
  ): T | null {
    let smallestAdequateEntry: T | null = null;

    for (const entry of entries) {
      if (
        entry.pageIndex !== pageIndex
        || (rotation !== undefined && entry.rotation !== rotation)
        || entry.renderedWidth < minimumWidth
      ) {
        continue;
      }
      if ('cropPdfRect' in entry && entry.cropPdfRect) {
        continue;
      }

      if (!smallestAdequateEntry || entry.renderedWidth < smallestAdequateEntry.renderedWidth) {
        smallestAdequateEntry = entry;
      }
    }

    return smallestAdequateEntry;
  }

  private acquirePageRenderSurface(entry: PageRenderCacheEntry): PageRenderSurface {
    entry.refCount += 1;
    entry.lastUsedAt = performance.now();
    const surface: PageRenderSurface = {
      cacheKey: entry.cacheKey,
      pageIndex: entry.pageIndex,
      bitmap: entry.bitmap,
      renderedWidth: entry.renderedWidth,
      renderedHeight: entry.renderedHeight,
      cropPdfRect: entry.cropPdfRect,
    };
    this.heldPageSurfaces.add(surface);
    return surface;
  }

  private acquirePageRenderSurfaceWithBudget(entry: PageRenderCacheEntry): PageRenderSurface {
    const surface = this.acquirePageRenderSurface(entry);
    this.enforcePageCacheBudget();
    return surface;
  }

  private pageCacheBytes(): number {
    let bytes = this.cacheBytes(this.pageUrlCache);
    bytes += this.bitmapPageCacheBytes();
    return bytes;
  }

  private bitmapPageCacheBytes(): number {
    let bytes = this.cacheBytes(this.renderCache);
    bytes += this.cacheBytes(this.retiredPageRenderCache);
    return bytes;
  }

  private cacheBytes(cache: Map<string, { byteSize: number }>): number {
    let bytes = 0;
    for (const entry of cache.values()) {
      bytes += entry.byteSize;
    }
    return bytes;
  }

  private createPageCacheKey(
    pageIndex: number,
    zoom: number,
    pixelRatio: number,
    transport: 'url' | 'bitmap' = 'url',
    cropPdfRect?: PdfRect,
    rotation?: PageModel['rotation'],
  ): string {
    const cropKey = cropPdfRect
      ? [
        'crop',
        cropPdfRect.x.toFixed(2),
        cropPdfRect.y.toFixed(2),
        cropPdfRect.width.toFixed(2),
        cropPdfRect.height.toFixed(2),
      ].join(':')
      : 'full';
    return `${transport}:${pageIndex}:${zoom.toFixed(3)}:${pixelRatio.toFixed(2)}:${rotation ?? 'source'}:${cropKey}`;
  }

  private createThumbnailCacheKey(
    pageIndex: number,
    options: NormalisedThumbnailRenderOptions,
    rotation?: PageModel['rotation'],
  ): string {
    return [
      pageIndex,
      rotation ?? 'source',
      options.maxWidth.toFixed(1),
      options.maxHeight.toFixed(1),
      options.pixelRatio.toFixed(2),
      options.minScale.toFixed(3),
    ].join(':');
  }

  private enqueueRenderTask<T>(
    kind: 'page' | 'thumbnail',
    cacheKey: string,
    run: (task: QueuedRenderTask) => Promise<T>,
    options: RenderRequestOptions,
  ): Promise<T> {
    if (options.signal?.aborted) {
      return Promise.reject(createAbortError());
    }

    return new Promise<T>((resolve, reject) => {
      const queue = kind === 'page' ? this.pageRenderQueue : this.thumbnailRenderQueue;
      const task: QueuedRenderTask = {
        cacheKey,
        priority: options.priority ?? 0,
        urgency: options.urgency ?? 'visible',
        requestClass: options.requestClass ?? (options.urgency === 'prefetch' ? 'nearby-prefetch' : 'visible-page-preview'),
        promotionOriginClass: null,
        enqueuedAt: performance.now(),
        startedAt: null,
        retainedAt: null,
        retainedUntil: null,
        signal: undefined,
        startedAbortController: new AbortController(),
        started: false,
        run,
        resolve,
        reject,
      };

      this.bindQueuedRenderTaskAbortHandler(kind, queue, task, options.signal);

      queue.push(task);
      queue.sort(compareQueuedRenderTasks);
      this.scheduleRenderQueuePump(kind);
    });
  }

  private scheduleRenderQueuePump(kind: 'page' | 'thumbnail'): void {
    if (kind === 'page') {
      this.pumpAllRenderQueues();
      return;
    }

    if (this.thumbnailQueuePumpScheduled) {
      return;
    }

    this.thumbnailQueuePumpScheduled = true;
    void Promise.resolve().then(() => {
      this.thumbnailQueuePumpScheduled = false;
      this.pumpAllRenderQueues();
    });
  }

  private reprioritiseQueuedRenderTask(
    kind: 'page' | 'thumbnail',
    cacheKey: string,
    options: Pick<RenderRequestOptions, 'priority' | 'urgency' | 'requestClass'>,
  ): void {
    const queue = kind === 'page' ? this.pageRenderQueue : this.thumbnailRenderQueue;
    const task = queue.find((entry) => entry.cacheKey === cacheKey);
    if (!task) {
      return;
    }

    this.applyQueuedTaskPriorityUpdate(kind, task, options);
    queue.sort(compareQueuedRenderTasks);
    this.pumpAllRenderQueues();
  }

  private reattachRetainedQueuedRenderTask<T>(
    kind: 'page' | 'thumbnail',
    cacheKey: string,
    options: RenderRequestOptions,
  ): Promise<T> | null {
    const queue = kind === 'page' ? this.pageRenderQueue : this.thumbnailRenderQueue;
    const task = queue.find((entry) => entry.cacheKey === cacheKey && this.isReusableRetainedQueuedTask(entry));
    if (!task) {
      return null;
    }

    const retainedAgeMs = performance.now() - (task.retainedAt ?? task.enqueuedAt);
    recordQueuedTaskTransition(kind, `${task.requestClass}:retained`, task.requestClass, retainedAgeMs);
    task.retainedAt = null;
    task.retainedUntil = null;

    return new Promise<T>((resolve, reject) => {
      task.resolve = resolve;
      task.reject = reject;
      this.applyQueuedTaskPriorityUpdate(kind, task, options);
      this.bindQueuedRenderTaskAbortHandler(kind, queue, task, options.signal);
      queue.sort(compareQueuedRenderTasks);
      this.pumpAllRenderQueues();
    });
  }

  private adoptCompatibleQueuedRenderTask<T>(
    kind: 'page' | 'thumbnail',
    cacheKey: string,
    options: RenderRequestOptions,
  ): Promise<T> | null {
    if (kind !== 'page') {
      return null;
    }

    const queue = this.pageRenderQueue;
    const task = queue.find((entry) => (
      entry.cacheKey === cacheKey
      && !entry.started
      && entry.retainedUntil === null
      && entry.requestClass === 'nearby-prefetch'
    ));
    if (!task) {
      return null;
    }

    const nextRequestClass = options.requestClass ?? task.requestClass;
    const nextUrgency = options.urgency ?? task.urgency;
    if (nextUrgency !== 'visible' || REQUEST_CLASS_RANK[nextRequestClass] >= REQUEST_CLASS_RANK[task.requestClass]) {
      return null;
    }

    const adoptionAgeMs = performance.now() - task.enqueuedAt;
    recordQueuedTaskAdoption(kind, task.requestClass, nextRequestClass, adoptionAgeMs);

    const previousReject = task.reject;
    previousReject(createAbortError());

    return new Promise<T>((resolve, reject) => {
      task.resolve = resolve;
      task.reject = reject;
      this.applyQueuedTaskPriorityUpdate(kind, task, options);
      this.bindQueuedRenderTaskAbortHandler(kind, queue, task, options.signal);
      queue.sort(compareQueuedRenderTasks);
      this.pumpAllRenderQueues();
    });
  }

  private applyQueuedTaskPriorityUpdate(
    kind: 'page' | 'thumbnail',
    task: QueuedRenderTask,
    options: Pick<RenderRequestOptions, 'priority' | 'urgency' | 'requestClass'>,
  ): void {
    task.priority = Math.max(task.priority, options.priority ?? task.priority);
    if (options.urgency === 'visible') {
      task.urgency = 'visible';
    }
    const nextRequestClass = options.requestClass ?? task.requestClass;
    if (REQUEST_CLASS_RANK[nextRequestClass] < REQUEST_CLASS_RANK[task.requestClass]) {
      recordQueuedTaskTransition(kind, task.requestClass, nextRequestClass, performance.now() - task.enqueuedAt);
      task.promotionOriginClass ??= task.requestClass;
      task.requestClass = nextRequestClass;
    }
  }

  private bindQueuedRenderTaskAbortHandler(
    kind: 'page' | 'thumbnail',
    queue: QueuedRenderTask[],
    task: QueuedRenderTask,
    signal?: AbortSignal,
  ): void {
    if (task.signal && task.abortHandler) {
      task.signal.removeEventListener('abort', task.abortHandler);
    }

    task.signal = signal;
    task.abortHandler = undefined;
    if (!signal) {
      return;
    }

    task.abortHandler = () => {
      if (task.started) {
        recordQueuedTaskAbortAfterStart(kind, task.requestClass, performance.now() - task.enqueuedAt);
        return;
      }

      const queuedAgeMs = performance.now() - task.enqueuedAt;
      if (this.shouldRetainQueuedTaskOnAbort(kind, task)) {
        recordQueuedTaskTransition(kind, task.requestClass, `${task.requestClass}:retained`, queuedAgeMs);
        recordQueuedTaskAbortBeforeStart(kind, task.requestClass, queuedAgeMs);
        recordRasterAbort(kind, 'before-start', task.requestClass);
        const currentReject = task.reject;
        task.retainedAt = performance.now();
        task.retainedUntil = task.retainedAt + DETACHED_NEARBY_PREFETCH_RETENTION_MS;
        task.signal = undefined;
        task.abortHandler = undefined;
        task.resolve = () => undefined;
        task.reject = () => undefined;
        currentReject(createAbortError());
        return;
      }

      const taskIndex = queue.indexOf(task);
      if (taskIndex >= 0) {
        queue.splice(taskIndex, 1);
        recordQueuedTaskAbortBeforeStart(kind, task.requestClass, queuedAgeMs);
        if (task.promotionOriginClass) {
          recordPromotedTaskAbortBeforeStart(kind, task.promotionOriginClass, task.requestClass, queuedAgeMs);
        }
        recordRasterAbort(kind, 'before-start', task.requestClass);
      }
      rejectAndSilenceTask(task, createAbortError());
    };
    signal.addEventListener('abort', task.abortHandler, { once: true });
  }

  private shouldRetainQueuedTaskOnAbort(kind: 'page' | 'thumbnail', task: QueuedRenderTask): boolean {
    return false && kind === 'page' && task.requestClass === 'nearby-prefetch';
  }

  private isReusableRetainedQueuedTask(task: QueuedRenderTask): boolean {
    if (task.retainedUntil === null) {
      return false;
    }

    return task.retainedUntil > performance.now();
  }

  private pumpAllRenderQueues(): void {
    this.pumpRenderQueue('page');
    this.pumpRenderQueue('thumbnail');
  }

  private pumpRenderQueue(kind: 'page' | 'thumbnail'): void {
    const queue = kind === 'page' ? this.pageRenderQueue : this.thumbnailRenderQueue;
    const maxConcurrent = this.getMaxPotentialConcurrentRenderCount(kind);

    if (kind === 'thumbnail' && this.getActiveRenderCount(kind) >= maxConcurrent) {
      const visibleTask = queue.find((task) => (
        task.urgency === 'visible'
        && task.requestClass === 'visible-thumbnail'
        && this.isQueuedTaskAllowedDuringMotion(kind, task)
      ));
      if (visibleTask) {
        this.preemptActiveThumbnailRenderForQueuedTask(visibleTask);
      }
      return;
    }

    while (this.getActiveRenderCount(kind) < maxConcurrent) {
      const nextTaskIndex = this.findNextQueuedRenderTaskIndex(kind, queue);
      if (nextTaskIndex < 0) {
        return;
      }

      const [nextTask] = queue.splice(nextTaskIndex, 1);
      if (!nextTask) {
        return;
      }

      if (nextTask.signal?.aborted) {
        const queuedAgeMs = performance.now() - nextTask.enqueuedAt;
        recordQueuedTaskAbortBeforeStart(kind, nextTask.requestClass, queuedAgeMs);
        if (nextTask.promotionOriginClass) {
          recordPromotedTaskAbortBeforeStart(kind, nextTask.promotionOriginClass, nextTask.requestClass, queuedAgeMs);
        }
        recordRasterAbort(kind, 'before-start', nextTask.requestClass);
        nextTask.reject(createAbortError());
        continue;
      }

      nextTask.started = true;
      nextTask.startedAt = performance.now();
      const queuedAgeMs = nextTask.startedAt - nextTask.enqueuedAt;
      recordQueuedTaskStart(kind, nextTask.requestClass, queuedAgeMs);
      if (nextTask.promotionOriginClass) {
        recordPromotedTaskStart(kind, nextTask.promotionOriginClass, nextTask.requestClass, queuedAgeMs);
      }
      recordRasterStart(kind, nextTask.requestClass);
      this.incrementActiveRenderCount(kind, nextTask.urgency);
      if (kind === 'thumbnail') {
        this.activeThumbnailRenderTasks.add(nextTask);
      }

      void nextTask.run(nextTask)
        .then((value) => {
          if (kind === 'thumbnail' && isAdaptiveBurstThumbnailTask(nextTask)) {
            this.recordOverviewThumbnailCompletion(performance.now() - (nextTask.startedAt ?? performance.now()));
          }
          nextTask.resolve(value);
        })
        .catch((error) => {
          if (isAbortError(error)) {
            recordRasterAbort(kind, 'after-start', nextTask.requestClass);
          }
          nextTask.reject(error);
        })
        .finally(() => {
          if (nextTask.signal && nextTask.abortHandler) {
            nextTask.signal.removeEventListener('abort', nextTask.abortHandler);
          }

          this.decrementActiveRenderCount(kind, nextTask.urgency);
          if (kind === 'thumbnail') {
            this.activeThumbnailRenderTasks.delete(nextTask);
          }
          this.pumpAllRenderQueues();
        });
    }
  }

  private findNextQueuedRenderTaskIndex(kind: 'page' | 'thumbnail', queue: QueuedRenderTask[]): number {
    const adaptiveConcurrency = resolveAdaptiveRenderConcurrency(this.adaptivePerformanceLevel);
    const maxPrefetchConcurrent = adaptiveConcurrency.prefetch === 0
      ? 0
      : kind === 'page' ? this.maxConcurrentPrefetchPageRenders : this.maxConcurrentPrefetchThumbnailRenders;
    const activePrefetchCount =
      kind === 'page' ? this.activePrefetchPageRenderCount : this.activePrefetchThumbnailRenderCount;

    for (let index = 0; index < queue.length; index += 1) {
      const task = queue[index];
      if (task.retainedUntil !== null && task.retainedUntil <= performance.now()) {
        queue.splice(index, 1);
        index -= 1;
        continue;
      }
      if (task.urgency === 'prefetch' && activePrefetchCount >= maxPrefetchConcurrent) {
        continue;
      }
      if (!this.isQueuedTaskAllowedDuringMotion(kind, task)) {
        continue;
      }
      if (!this.canStartQueuedRenderTask(kind, task)) {
        if (kind === 'page') {
          this.preemptActiveThumbnailRenderForQueuedPageTask(task);
        } else {
          this.preemptActiveThumbnailRenderForQueuedTask(task);
        }
        continue;
      }
      if (
        kind === 'thumbnail'
        && !isUserVisibleThumbnailTask(task)
        && this.hasQueuedCriticalPageRender()
        && this.getActiveTotalRenderCount() >= this.getAdaptiveMaxTotalRenderCount() - 1
      ) {
        continue;
      }
      return index;
    }

    return -1;
  }

  private preemptActiveThumbnailRenderForQueuedTask(task: QueuedRenderTask): boolean {
    if (!this.thumbnailRenderReady) {
      return false;
    }

    if (!isUserVisibleThumbnailTask(task)) {
      return false;
    }

    const candidates = [...this.activeThumbnailRenderTasks]
      .filter((activeTask) => (
        activeTask.cacheKey !== task.cacheKey
        && !activeTask.startedAbortController.signal.aborted
        && (
          activeTask.urgency === 'prefetch'
          || activeTask.requestClass === 'nearby-prefetch'
          || activeTask.requestClass === 'warming'
        )
      ))
      .sort((left, right) => left.priority - right.priority || left.enqueuedAt - right.enqueuedAt);
    const candidate = candidates[0];
    if (!candidate) {
      return false;
    }

    candidate.startedAbortController.abort(createAbortError());
    return true;
  }

  private preemptActiveThumbnailRenderForQueuedPageTask(task: QueuedRenderTask): boolean {
    if (!this.thumbnailRenderReady || !isTargetPageRenderTask(task)) {
      return false;
    }

    const candidates = [...this.activeThumbnailRenderTasks]
      .filter((activeTask) => !activeTask.startedAbortController.signal.aborted)
      .sort((left, right) => left.priority - right.priority || left.enqueuedAt - right.enqueuedAt);
    const candidate = candidates[0];
    if (!candidate) {
      return false;
    }

    candidate.startedAbortController.abort(createAbortError());
    return true;
  }

  private isQueuedTaskAllowedDuringMotion(kind: 'page' | 'thumbnail', task: QueuedRenderTask): boolean {
    const inActivationCriticalWindow = performance.now() < this.activationCriticalUntil;
    const useSchedulerIsolation = this.navigationIntentSource === 'thumbnail';
    if (
      inActivationCriticalWindow
      && !isRenderTaskAllowedDuringActivationCriticalWindow(kind, task, useSchedulerIsolation)
    ) {
      return false;
    }

    if (kind === 'page') {
      if (this.thumbnailListInMotion && task.requestClass === 'warming') {
        return false;
      }

      if (!this.viewportInMotion) {
        return true;
      }

      return task.requestClass === 'target-page-preview'
        || task.requestClass === 'visible-page-preview'
        || task.requestClass === 'target-page-hq'
        || task.requestClass === 'target-page-crop';
    }

    if (this.viewportInMotion) {
      return task.requestClass === 'overview-thumbnail' || task.requestClass === 'visible-thumbnail';
    }

    if (this.thumbnailListInMotion) {
      return task.requestClass === 'overview-thumbnail' || task.requestClass === 'visible-thumbnail';
    }

    return true;
  }

  private canStartQueuedRenderTask(kind: 'page' | 'thumbnail', task: QueuedRenderTask): boolean {
    return this.getActiveRenderCount(kind) < this.getMaxConcurrentRenderCount(kind, task)
      && this.getActiveTotalRenderCount() < this.getMaxTotalRenderCount(kind, task);
  }

  private clearExpiredNavigationIntent(): void {
    if (this.navigationIntentPageIndex === null || performance.now() <= this.navigationIntentExpiresAt) {
      return;
    }

    this.navigationIntentPageIndex = null;
    this.navigationIntentSource = 'generic';
    this.navigationIntentExpiresAt = 0;
  }

  private startActivationCriticalWindow(durationMs = 450): void {
    this.activationCriticalUntil = performance.now() + durationMs;
    if (this.activationCriticalTimer !== null) {
      clearTimeout(this.activationCriticalTimer);
    }
    this.activationCriticalTimer = setTimeout(() => {
      this.activationCriticalTimer = null;
      this.activationCriticalUntil = 0;
      this.pumpAllRenderQueues();
    }, durationMs);
  }

  private clearActivationCriticalWindow(): void {
    this.activationCriticalUntil = 0;
    if (this.activationCriticalTimer !== null) {
      clearTimeout(this.activationCriticalTimer);
      this.activationCriticalTimer = null;
    }
  }

  private markFirstVisiblePageReady(pageIndex: number, requestClass: RenderRequestClass | null): void {
    if (this.firstVisiblePageIndex !== pageIndex || this.firstVisiblePageReady) {
      return;
    }

    this.firstVisiblePageReady = true;
    this.firstVisiblePageReadyAtMs = this.openStartedAt === null ? null : roundDuration(performance.now() - this.openStartedAt);
    this.firstVisiblePageReadyRequestClass = requestClass;
  }

  private hasQueuedCriticalPageRender(): boolean {
    return this.pageRenderQueue.some((task) => (
      task.requestClass === 'target-page-preview'
      || task.requestClass === 'target-page-hq'
      || task.requestClass === 'target-page-crop'
      || task.requestClass === 'visible-page-preview'
    ));
  }

  private getMaxPotentialConcurrentRenderCount(kind: 'page' | 'thumbnail'): number {
    const adaptiveConcurrency = resolveAdaptiveRenderConcurrency(this.adaptivePerformanceLevel);
    if (kind === 'page') {
      return adaptiveConcurrency.page;
    }
    const overviewLimit = Math.max(this.resolveOverviewThumbnailConcurrencyCeiling(), this.resolveOverviewThumbnailConcurrencyLimit());
    return adaptiveConcurrency.overviewThumbnailCeiling === null
      ? overviewLimit
      : Math.min(overviewLimit, adaptiveConcurrency.overviewThumbnailCeiling);
  }

  private getMaxConcurrentRenderCount(kind: 'page' | 'thumbnail', task?: QueuedRenderTask): number {
    if (kind === 'page') {
      return resolveAdaptiveRenderConcurrency(this.adaptivePerformanceLevel).page;
    }

    if (!task || !isAdaptiveBurstThumbnailTask(task)) {
      return resolveAdaptiveRenderConcurrency(this.adaptivePerformanceLevel).thumbnail;
    }

    const adaptiveConcurrency = resolveAdaptiveRenderConcurrency(this.adaptivePerformanceLevel);
    const overviewLimit = this.resolveOverviewThumbnailConcurrencyLimit();
    return adaptiveConcurrency.overviewThumbnailCeiling === null
      ? overviewLimit
      : Math.min(overviewLimit, adaptiveConcurrency.overviewThumbnailCeiling);
  }

  private getMaxTotalRenderCount(kind: 'page' | 'thumbnail', task: QueuedRenderTask): number {
    if (
      kind === 'thumbnail'
      && isAdaptiveBurstThumbnailTask(task)
      && (!this.hasRenderInputPressure() || task.requestClass === 'visible-thumbnail')
    ) {
      const adaptiveConcurrency = resolveAdaptiveRenderConcurrency(this.adaptivePerformanceLevel);
      const overviewLimit = adaptiveConcurrency.overviewThumbnailCeiling === null
        ? this.resolveOverviewThumbnailConcurrencyLimit()
        : Math.min(this.resolveOverviewThumbnailConcurrencyLimit(), adaptiveConcurrency.overviewThumbnailCeiling);
      return Math.max(adaptiveConcurrency.total, overviewLimit);
    }

    return this.getAdaptiveMaxTotalRenderCount();
  }

  private getAdaptiveMaxTotalRenderCount(): number {
    return resolveAdaptiveRenderConcurrency(this.adaptivePerformanceLevel).total;
  }

  private resolveOverviewThumbnailConcurrencyLimit(): number {
    if (this.overviewThumbnailConcurrencyLimit === null) {
      this.overviewThumbnailConcurrencyLimit = this.resolveOverviewThumbnailConcurrencyFloor();
    }

    return this.overviewThumbnailConcurrencyLimit;
  }

  private resolveOverviewThumbnailConcurrencyCeiling(): number {
    return resolveOverviewThumbnailConcurrencyCeiling(
      resolveHardwareConcurrency(),
      this.maxConcurrentTotalRenders,
      this.maxConcurrentThumbnailRenders,
    );
  }

  private recordOverviewThumbnailCompletion(durationMs: number): void {
    const now = performance.now();
    this.overviewThumbnailWindowStartedAt ??= now;
    this.overviewThumbnailWindowCompletions += 1;
    this.overviewThumbnailWindowDurationMs += durationMs;

    const currentLimit = this.resolveOverviewThumbnailConcurrencyLimit();
    if (this.overviewThumbnailWindowCompletions < Math.max(this.maxConcurrentThumbnailRenders, currentLimit)) {
      return;
    }

    const elapsedMs = Math.max(1, now - this.overviewThumbnailWindowStartedAt);
    const throughputPerSecond = roundDuration((this.overviewThumbnailWindowCompletions / elapsedMs) * 1000);
    this.overviewThumbnailLastThroughputPerSecond = throughputPerSecond;
    const hasPagePressure = this.hasRenderInputPressure() || this.activePageRenderCount > 0;

    if (hasPagePressure) {
      this.resetOverviewThumbnailTuningWindow();
      return;
    } else if (
      this.overviewThumbnailBestThroughputPerSecond === null
      || throughputPerSecond >= this.overviewThumbnailBestThroughputPerSecond
    ) {
      this.overviewThumbnailBestThroughputPerSecond = Math.max(
        this.overviewThumbnailBestThroughputPerSecond ?? 0,
        throughputPerSecond,
      );
      this.overviewThumbnailConcurrencyLimit = Math.min(
        this.resolveOverviewThumbnailConcurrencyCeiling(),
        currentLimit + 2,
      );
    } else {
      this.overviewThumbnailConcurrencyLimit = Math.max(
        this.resolveOverviewThumbnailConcurrencyFloor(),
        currentLimit - 1,
      );
    }

    this.resetOverviewThumbnailTuningWindow();
  }

  private resolveOverviewThumbnailConcurrencyFloor(): number {
    return resolveInitialOverviewThumbnailConcurrencyLimit(
      resolveHardwareConcurrency(),
      this.maxConcurrentTotalRenders,
      this.maxConcurrentThumbnailRenders,
    );
  }

  private resetOverviewThumbnailTuningWindow(): void {
    this.overviewThumbnailWindowStartedAt = null;
    this.overviewThumbnailWindowCompletions = 0;
    this.overviewThumbnailWindowDurationMs = 0;
  }

  private hasRenderInputPressure(): boolean {
    return this.viewportInMotion
      || this.thumbnailListInMotion
      || performance.now() < this.activationCriticalUntil
      || this.hasQueuedCriticalPageRender();
  }

  private getActiveRenderCount(kind: 'page' | 'thumbnail'): number {
    return kind === 'page' ? this.activePageRenderCount : this.activeThumbnailRenderCount;
  }

  private getActiveTotalRenderCount(): number {
    return this.activePageRenderCount + this.activeThumbnailRenderCount;
  }

  private incrementActiveRenderCount(kind: 'page' | 'thumbnail', urgency: RenderUrgency): void {
    if (kind === 'page') {
      this.activePageRenderCount += 1;
      if (urgency === 'prefetch') {
        this.activePrefetchPageRenderCount += 1;
      }
      return;
    }

    this.activeThumbnailRenderCount += 1;
    if (urgency === 'prefetch') {
      this.activePrefetchThumbnailRenderCount += 1;
    }
  }

  private decrementActiveRenderCount(kind: 'page' | 'thumbnail', urgency: RenderUrgency): void {
    if (kind === 'page') {
      this.activePageRenderCount = Math.max(0, this.activePageRenderCount - 1);
      if (urgency === 'prefetch') {
        this.activePrefetchPageRenderCount = Math.max(0, this.activePrefetchPageRenderCount - 1);
      }
      return;
    }

    this.activeThumbnailRenderCount = Math.max(0, this.activeThumbnailRenderCount - 1);
    if (urgency === 'prefetch') {
      this.activePrefetchThumbnailRenderCount = Math.max(0, this.activePrefetchThumbnailRenderCount - 1);
    }
  }

  private clearQueuedRenderTasks(queue: QueuedRenderTask[], predicate?: (task: QueuedRenderTask) => boolean): void {
    const remaining: QueuedRenderTask[] = [];
    for (const task of queue.splice(0, queue.length)) {
      if (predicate && !predicate(task)) {
        remaining.push(task);
        continue;
      }
      if (task.signal && task.abortHandler) {
        task.signal.removeEventListener('abort', task.abortHandler);
      }
      task.reject(createAbortError());
    }
    queue.push(...remaining);
  }

  private enforcePageCacheBudget(): void {
    if (this.renderCache.size === 0) {
      return;
    }

    const orderedEntries = [...this.renderCache.entries()].sort((left, right) => left[1].lastUsedAt - right[1].lastUsedAt);
    let bytes = this.bitmapPageCacheBytes();

    while (this.renderCache.size > this.maxBitmapRenderCacheEntries || bytes > this.maxBitmapRenderCacheBytes) {
      const oldest = orderedEntries.shift();
      if (!oldest) {
        break;
      }

      const [key, entry] = oldest;
      this.renderCache.delete(key);
      if (entry.refCount > 0) {
        entry.pendingClose = true;
        this.retiredPageRenderCache.set(key, entry);
      } else {
        entry.bitmap.close();
      }
      bytes -= entry.byteSize;
    }
  }

  private enforceCacheBudget(
    cache: Map<string, ThumbnailCacheEntry>,
    maxEntries: number,
    maxBytes: number,
    isProtectedEntry?: (entry: ThumbnailCacheEntry) => boolean,
  ): void {
    if (cache.size === 0) {
      return;
    }

    const orderedEntries = [...cache.entries()].sort((left, right) => left[1].lastUsedAt - right[1].lastUsedAt);
    let bytes = this.cacheBytes(cache);

    while (cache.size > maxEntries || bytes > maxBytes) {
      const oldest = orderedEntries.shift();
      if (!oldest) {
        break;
      }

      const [key, entry] = oldest;
      if (isProtectedImageUrlCacheEntry(entry) || isProtectedEntry?.(entry) || this.isHeldObjectUrl(entry.objectUrl)) {
        continue;
      }

      cache.delete(key);
      URL.revokeObjectURL(entry.objectUrl);
      bytes -= entry.byteSize;
    }
  }

  private isProtectedPageUrlCacheEntry(entry: ThumbnailCacheEntry): boolean {
    const visiblePreview =
      entry.sourceRequestClass === 'target-page-preview'
      || entry.sourceRequestClass === 'visible-page-preview';
    return visiblePreview && performance.now() - entry.lastUsedAt <= RECENT_VISIBLE_PAGE_PREVIEW_RETENTION_MS;
  }

  private isHeldObjectUrl(objectUrl: string): boolean {
    return (this.heldObjectUrlCounts.get(objectUrl) ?? 0) > 0;
  }

  private bumpVersion(): void {
    this.versionCounter += 1;
    for (const listener of this.listeners) {
      listener();
    }
  }
}

function estimateBitmapByteSize(width: number, height: number): number {
  return Math.max(1, Math.ceil(width)) * Math.max(1, Math.ceil(height)) * 4;
}

function normaliseThumbnailOptions(
  optionsOrWidth: number | ThumbnailRenderOptions,
  fallbackPixelRatio: number,
): NormalisedThumbnailRenderOptions {
  if (typeof optionsOrWidth === 'number') {
    if (optionsOrWidth <= 0) {
      throw new RangeError('Thumbnail width must be greater than zero.');
    }

    return {
      maxWidth: optionsOrWidth,
      maxHeight: 220,
      pixelRatio: fallbackPixelRatio,
      minScale: 0.1,
    };
  }

  if (optionsOrWidth.maxWidth <= 0 || optionsOrWidth.maxHeight <= 0) {
    throw new RangeError('Thumbnail bounds must be greater than zero.');
  }

  return {
    maxWidth: optionsOrWidth.maxWidth,
    maxHeight: optionsOrWidth.maxHeight,
    pixelRatio: optionsOrWidth.pixelRatio ?? fallbackPixelRatio,
    minScale: Math.max(0.005, optionsOrWidth.minScale ?? 0.1),
    pageWidth: optionsOrWidth.pageWidth,
    pageHeight: optionsOrWidth.pageHeight,
  };
}

function toErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function resolveStartedRenderSignal(task: QueuedRenderTask, includeExternalSignal = true): AbortSignal {
  const internalSignal = task.startedAbortController.signal;
  const externalSignal = includeExternalSignal ? task.signal : undefined;
  if (!externalSignal) {
    return internalSignal;
  }
  if (externalSignal.aborted) {
    return externalSignal;
  }
  if (internalSignal.aborted) {
    return internalSignal;
  }

  const abortSignalWithAny = AbortSignal as typeof AbortSignal & {
    any?: (signals: AbortSignal[]) => AbortSignal;
  };
  if (typeof abortSignalWithAny.any === 'function') {
    return abortSignalWithAny.any([externalSignal, internalSignal]);
  }

  const controller = new AbortController();
  const abort = () => {
    if (!controller.signal.aborted) {
      controller.abort(createAbortError());
    }
  };
  externalSignal.addEventListener('abort', abort, { once: true });
  internalSignal.addEventListener('abort', abort, { once: true });
  return controller.signal;
}

function isVisibleOverviewThumbnailTask(task: Pick<QueuedRenderTask, 'requestClass' | 'urgency'>): boolean {
  return task.requestClass === 'overview-thumbnail' && task.urgency === 'visible';
}

function isUserVisibleThumbnailTask(task: Pick<QueuedRenderTask, 'requestClass' | 'urgency'>): boolean {
  return task.urgency === 'visible'
    && (task.requestClass === 'visible-thumbnail' || task.requestClass === 'overview-thumbnail');
}

function isAdaptiveBurstThumbnailTask(task: Pick<QueuedRenderTask, 'requestClass' | 'urgency'>): boolean {
  return task.urgency === 'visible'
    && (task.requestClass === 'overview-thumbnail' || task.requestClass === 'visible-thumbnail');
}

function isTargetPageRenderTask(task: Pick<QueuedRenderTask, 'requestClass'>): boolean {
  return task.requestClass === 'target-page-preview'
    || task.requestClass === 'target-page-hq'
    || task.requestClass === 'target-page-crop';
}

function resolveHardwareConcurrency(): number {
  const hardwareConcurrency = typeof navigator === 'undefined' ? 0 : navigator.hardwareConcurrency;
  return Number.isFinite(hardwareConcurrency) && hardwareConcurrency > 0
    ? Math.floor(Number(hardwareConcurrency))
    : 0;
}

export function resolveOverviewThumbnailConcurrencyCeiling(
  hardwareConcurrency: number,
  fallbackTotalConcurrency: number,
  baseThumbnailConcurrency: number,
): number {
  return Math.max(baseThumbnailConcurrency, fallbackTotalConcurrency, hardwareConcurrency);
}

export function resolveInitialOverviewThumbnailConcurrencyLimit(
  hardwareConcurrency: number,
  fallbackTotalConcurrency: number,
  baseThumbnailConcurrency: number,
): number {
  const ceiling = resolveOverviewThumbnailConcurrencyCeiling(
    hardwareConcurrency,
    fallbackTotalConcurrency,
    baseThumbnailConcurrency,
  );
  const hardwareDerivedStart = hardwareConcurrency > 0
    ? Math.ceil(hardwareConcurrency * 0.5)
    : fallbackTotalConcurrency;

  return Math.max(baseThumbnailConcurrency, Math.min(ceiling, hardwareDerivedStart));
}

const DETACHED_NEARBY_PREFETCH_RETENTION_MS = 180;
const FRESH_IMAGE_URL_PROTECTION_MS = 2_000;
const RECENT_VISIBLE_PAGE_PREVIEW_RETENTION_MS = 10_000;

export function shouldProtectFreshImageUrlCacheEntry(
  sourceRequestClass: RenderRequestClass | undefined,
): boolean {
  return sourceRequestClass === 'target-page-preview'
    || sourceRequestClass === 'target-page-crop'
    || sourceRequestClass === 'visible-page-preview'
    || sourceRequestClass === 'target-page-hq'
    || sourceRequestClass === 'visible-page-hq-upgrade'
    || sourceRequestClass === 'overview-thumbnail';
}

function resolveFreshImageUrlProtectionUntil(sourceRequestClass: RenderRequestClass | undefined): number {
  return shouldProtectFreshImageUrlCacheEntry(sourceRequestClass)
    ? performance.now() + FRESH_IMAGE_URL_PROTECTION_MS
    : 0;
}

function isProtectedImageUrlCacheEntry(entry: ThumbnailCacheEntry): boolean {
  return entry.protectedUntil > performance.now();
}

const REQUEST_CLASS_RANK: Record<RenderRequestClass, number> = {
  'target-page-preview': 0,
  'target-page-crop': 1,
  'target-page-hq': 1,
  'visible-page-preview': 2,
  'visible-thumbnail': 3,
  'overview-thumbnail': 3,
  'visible-page-hq-upgrade': 4,
  'nearby-prefetch': 5,
  warming: 6,
};

function resolvePageRendererMode(): DiagnosticsSnapshot['pageRendererMode'] {
  const requestedMode = String(import.meta.env.VITE_BP_PAGE_RENDERER_MODE ?? '').trim();
  if (requestedMode === 'tiled-raster' || requestedMode === 'vector-spike') {
    return requestedMode;
  }

  return 'raster';
}

function compareQueuedRenderTasks(left: QueuedRenderTask, right: QueuedRenderTask): number {
  const classDelta = REQUEST_CLASS_RANK[left.requestClass] - REQUEST_CLASS_RANK[right.requestClass];
  if (classDelta !== 0) {
    return classDelta;
  }

  if (left.priority !== right.priority) {
    return right.priority - left.priority;
  }

  return left.enqueuedAt - right.enqueuedAt;
}

function rejectAndSilenceTask(task: QueuedRenderTask, error: unknown): void {
  const currentReject = task.reject;
  task.resolve = () => undefined;
  task.reject = () => undefined;
  currentReject(error);
}

function roundDuration(value: number): number {
  return Math.round(value * 1000) / 1000;
}

function createAbortError(): Error {
  const error = new Error('Render request aborted.');
  error.name = 'AbortError';
  return error;
}

function createRenderUnavailableError(): Error {
  const error = new Error('The PDF render session is temporarily unavailable.');
  error.name = 'RenderUnavailableError';
  return error;
}

export function isRenderUnavailableError(error: unknown): boolean {
  return error instanceof Error && error.name === 'RenderUnavailableError';
}

function isAbortError(error: unknown): boolean {
  return error instanceof Error && error.name === 'AbortError';
}

function isExpectedRenderInterruption(error: unknown): boolean {
  return isAbortError(error) || isRenderUnavailableError(error);
}
