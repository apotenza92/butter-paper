import type { ButterCanvasDocument, DocumentModel, Markup } from '@butter-paper/core';
import type { PdfPageGeometryIndex, PdfSaveMode, PdfSaveResult } from '@butter-paper/pdf';

export type ToolMode = 'select' | 'pan' | 'text-box' | 'rectangle' | 'ellipse' | 'arc' | 'line' | 'arrow' | 'dimension' | 'length' | 'polylength' | 'area' | 'polyline' | 'polygon' | 'pen' | 'highlight' | 'cloud' | 'cloud-plus' | 'callout' | 'image' | 'snapshot';
export type ScrollMode = 'continuous' | 'single-page';
export type ScrollWheelMode = 'zoom' | 'scroll';
export type ZoomPreset = 'manual' | 'fit-width' | 'fit-page';
export type SidebarPanel = 'pages' | 'tools';
export type ThemeMode = 'light' | 'dark';

export interface ThemeSnapshot {
  mode: ThemeMode;
}

export type UpdateChannel = 'stable' | 'beta';
export interface ApplicationMetadata {
  productName: 'Butter Paper' | 'Butter Paper Beta';
  channel: UpdateChannel;
}
export interface DefaultPdfAppResult {
  outcome: 'changed' | 'requires-confirmation';
  message: string;
}
export type UpdateFrequency =
  | 'never'
  | 'startup'
  | 'hourly'
  | 'sixHours'
  | 'twelveHours'
  | 'daily'
  | 'weekly'
  | 'monthly';
export type UpdatePhase =
  | 'disabled'
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'downloaded'
  | 'error';
export type UpdateDisabledReason = 'development' | 'test-mode' | 'configuration' | 'platform-policy';

export interface UpdateStatus {
  phase: UpdatePhase;
  channel: UpdateChannel | null;
  frequency: UpdateFrequency;
  enabled: boolean;
  automaticChecksEnabled: boolean;
  currentVersion: string;
  availableVersion: string | null;
  releaseNotes: string | null;
  downloadPercent: number | null;
  lastSuccessfulCheckAt: string | null;
  disabledReason: UpdateDisabledReason | null;
  errorMessage: string | null;
}

export interface WindowBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface WindowState {
  bounds: WindowBounds;
  focused: boolean;
  maximized: boolean;
  title: string;
}

export interface DocumentOpenStageTimings {
  mainPayloadMs: number;
  mainMetadataMs?: number;
  mainPageModelMs?: number;
  mainAnnotationReadMs?: number;
  rendererFileReadMs?: number;
  rendererBrowserOpenMs?: number;
  totalOpenMs?: number;
}

export interface PerfSnapshot {
  elapsedMs: number;
  componentRenders: Record<string, number>;
  detailedComponentRenders: Record<string, number>;
  eventCounts: Record<string, number>;
  placeholderShows: {
    page: number;
    thumbnail: number;
  };
  detailedPlaceholderShows: {
    page: Record<string, number>;
    thumbnail: Record<string, number>;
  };
  pageImageVisibility: Record<string, {
    firstVisibleMs?: number;
    previewVisibleMs?: number;
    acceptablePreviewVisibleMs?: number;
    mediumVisibleMs?: number;
    fullVisibleMs?: number;
    detailVisibleMs?: number;
    bestRenderedWidthRatio?: number;
  }>;
  overviewVisiblePreviewFill?: {
    totalVisible: number;
    filledVisible: number;
    acceptableVisible: number;
    filledRatio: number;
    acceptableRatio: number;
    firstFilledMs: number | null;
    firstAcceptableMs: number | null;
    reached25Ms: number | null;
    reached50Ms: number | null;
    reached75Ms: number | null;
    reached100Ms: number | null;
    acceptableReached25Ms: number | null;
    acceptableReached50Ms: number | null;
    acceptableReached75Ms: number | null;
    acceptableReached100Ms: number | null;
  } | null;
  overviewFocusPreview?: {
    pageIndex: number | null;
    source: 'pointer' | 'viewport-focus';
    requiredWidth: number;
    renderedWidth: number;
    acceptable: boolean;
    firstSeenMs: number | null;
    firstAcceptableMs: number | null;
  } | null;
  firstPageImageVisibleMs: number | null;
  firstPagePreviewVisibleMs: number | null;
  firstPageAcceptablePreviewVisibleMs?: number | null;
  firstPageFullVisibleMs: number | null;
  obsoleteRenderCompletions?: Record<string, number>;
  queueStability?: {
    classTransitions: Record<string, number>;
    transitionAgeMs: Record<string, {
      count: number;
      totalMs: number;
      maxMs: number;
    }>;
    startAgeMs: Record<string, {
      count: number;
      totalMs: number;
      maxMs: number;
    }>;
    abortedBeforeStartAgeMs: Record<string, {
      count: number;
      totalMs: number;
      maxMs: number;
    }>;
    abortedAfterStartAgeMs: Record<string, {
      count: number;
      totalMs: number;
      maxMs: number;
    }>;
    adoptions: Record<string, number>;
    adoptionAgeMs: Record<string, {
      count: number;
      totalMs: number;
      maxMs: number;
    }>;
    promotedTaskStarts: Record<string, number>;
    promotedTaskStartAgeMs: Record<string, {
      count: number;
      totalMs: number;
      maxMs: number;
    }>;
    promotedTaskAbortsBeforeStart: Record<string, number>;
    promotedTaskAbortBeforeStartAgeMs: Record<string, {
      count: number;
      totalMs: number;
      maxMs: number;
    }>;
    cacheReuseFromPrefetch: Record<string, number>;
  };
  renderPage: {
    requests: number;
    hits: number;
    misses: number;
    started: number;
    completed: number;
    abortedBeforeStart: number;
    abortedAfterStart: number;
    errors: number;
    byRequestClass?: Record<string, {
      requests: number;
      hits: number;
      misses: number;
      started: number;
      completed: number;
      abortedBeforeStart: number;
      abortedAfterStart: number;
      errors: number;
    }>;
  };
  renderThumbnail: {
    requests: number;
    hits: number;
    misses: number;
    started: number;
    completed: number;
    abortedBeforeStart: number;
    abortedAfterStart: number;
    errors: number;
    byRequestClass?: Record<string, {
      requests: number;
      hits: number;
      misses: number;
      started: number;
      completed: number;
      abortedBeforeStart: number;
      abortedAfterStart: number;
      errors: number;
    }>;
  };
  longTasks: {
    count: number;
    totalDuration: number;
    maxDuration: number;
  };
  lastWindowBounds: WindowBounds | null;
}

export interface LoadedDocumentPayload {
  filePath: string;
  fileName: string;
  document: DocumentModel;
  openStageTimings?: DocumentOpenStageTimings;
}

export interface SaveDocumentRequest {
  sourcePath: string;
  targetPath?: string;
  markups: readonly Markup[];
  pageScales?: DocumentModel['pageScales'];
  mode: PdfSaveMode;
}

export interface PageGeometryRequest {
  filePath: string;
  pageIndex: number;
}

export type DesktopRenderBackend = 'pdfjs' | 'pdfium';
export type DesktopRenderBackendSelectionSource = 'default' | 'env';
export type RenderCoreErrorCode =
  | 'not-implemented'
  | 'backend-unavailable'
  | 'invalid-request'
  | 'not-found'
  | 'process-failed'
  | 'decode-failed';
export type RenderCorePixelFormat = 'rgba8';
export type RenderCoreSurfaceByteFormat = 'png';
export type RenderCoreCliCommand = 'document-info' | 'page-info' | 'render-page' | 'other';
export type RenderCoreRenderMode = 'full' | 'preview';
export type RenderCoreRenderRequestClass =
  | 'target-page-hq'
  | 'target-page-crop'
  | 'target-page-preview'
  | 'visible-page-preview'
  | 'visible-page-hq-upgrade'
  | 'overview-thumbnail'
  | 'visible-thumbnail'
  | 'nearby-prefetch'
  | 'warming';

export interface DesktopRenderBackendConfig {
  requestedBackend: DesktopRenderBackend;
  selectionSource: DesktopRenderBackendSelectionSource;
  envOverride: string | null;
}

export interface DesktopRenderBackendSelection {
  configuredBackend: DesktopRenderBackend;
  activeBackend: DesktopRenderBackend | null;
  selectionSource: DesktopRenderBackendSelectionSource;
}

export interface DesktopRenderCapabilities {
  backend: DesktopRenderBackend;
  available: boolean;
  canOpenDocument: boolean;
  canGetPageInfo: boolean;
  canRenderPage: boolean;
  canReadSurface: boolean;
  canReleaseSurface: boolean;
  canCloseDocument: boolean;
  notes: string[];
}

export interface RenderCoreCliCommandStats {
  count: number;
  failures: number;
  totalMs: number;
  maxMs: number;
  lastMs: number | null;
}

export interface RenderCoreWorkerPoolStats {
  queued: number;
  active: number;
  assignments: number;
  totalQueueWaitMs: number;
  maxQueueWaitMs: number;
  lastQueueWaitMs: number | null;
}

export interface RenderCoreNativeStageStats {
  count: number;
  totalMs: number;
  maxMs: number;
  lastMs: number | null;
}

export interface RenderCoreNativeRenderPageStages {
  resolvePdfPathMs: RenderCoreNativeStageStats;
  loadDocumentMs: RenderCoreNativeStageStats;
  getPageMs: RenderCoreNativeStageStats;
  buildRenderConfigMs: RenderCoreNativeStageStats;
  pdfiumRenderMs: RenderCoreNativeStageStats;
  bitmapToImageMs: RenderCoreNativeStageStats;
  pngEncodeMs: RenderCoreNativeStageStats;
  nativeTotalMs: RenderCoreNativeStageStats;
}

export interface RenderCoreNativeRenderPageDiagnostics {
  count: number;
  failures: number;
  totalMs: number;
  maxMs: number;
  lastMs: number | null;
  stages: RenderCoreNativeRenderPageStages;
  byRenderMode: Record<RenderCoreRenderMode, {
    count: number;
    failures: number;
    totalMs: number;
    maxMs: number;
    lastMs: number | null;
    stages: RenderCoreNativeRenderPageStages;
  }>;
  byRequestClass: Partial<Record<RenderCoreRenderRequestClass, {
    count: number;
    failures: number;
    totalMs: number;
    maxMs: number;
    lastMs: number | null;
    stages: RenderCoreNativeRenderPageStages;
  }>>;
}

export interface RenderCoreDiagnostics {
  backend: DesktopRenderBackend;
  activeDocuments: number;
  activeSurfaces: number;
  activeSurfaceBytes: number;
  surfacesByDocument?: Array<{ documentId: string; count: number; bytes: number }>;
  cli: Record<RenderCoreCliCommand, RenderCoreCliCommandStats>;
  renderPageWorkerPool: RenderCoreWorkerPoolStats | null;
  renderPageNative: RenderCoreNativeRenderPageDiagnostics;
}

export interface RenderCoreError {
  code: RenderCoreErrorCode;
  message: string;
  backend: DesktopRenderBackend;
}

export type RenderCoreResult<T> =
  | {
      ok: true;
      value: T;
    }
  | {
      ok: false;
      error: RenderCoreError;
    };

export interface RenderCoreOpenDocumentRequest {
  filePath: string;
  password?: string | null;
}

export interface RenderCoreOpenDocumentResponse {
  documentId: string;
  pageCount: number;
  backend: DesktopRenderBackend;
}

export interface RenderCoreGetPageInfoRequest {
  documentId: string;
  pageIndex: number;
}

export interface RenderCorePageInfo {
  documentId: string;
  pageIndex: number;
  width: number;
  height: number;
  rotation: number;
}

export interface RenderCoreRenderTarget {
  width: number;
  height: number;
  scale: number;
}

export interface RenderCorePdfRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface RenderCoreRenderPageRequest {
  documentId: string;
  pageIndex: number;
  target: RenderCoreRenderTarget;
  cropPdfRect?: RenderCorePdfRect;
  rotation?: number;
  renderMode?: RenderCoreRenderMode;
  requestClass?: RenderCoreRenderRequestClass;
}

export interface RenderCoreRenderPageResponse {
  surfaceId: string;
  pixelWidth: number;
  pixelHeight: number;
  pixelFormat: RenderCorePixelFormat;
  surfaceByteFormat: RenderCoreSurfaceByteFormat;
  byteLength: number;
}

export interface RenderCoreReadSurfaceRequest {
  surfaceId: string;
}

export interface RenderCoreReadSurfaceResponse {
  surfaceId: string;
  byteFormat: RenderCoreSurfaceByteFormat;
  bytes: Uint8Array;
  byteLength: number;
}

export interface RenderCoreReleaseSurfaceRequest {
  surfaceId: string;
}

export interface RenderCoreCloseDocumentRequest {
  documentId: string;
}

export interface ButterPaperBridge {
  readonly environment: {
    readonly testMode: boolean;
    readonly defaultSamplePdfPath: string | null;
    readonly cadRenderExperiment: string | null;
    readonly renderCoordinatorV2: boolean;
  };
  readonly application: {
    getMetadata(): Promise<ApplicationMetadata>;
    setAsDefaultPdfApp(): Promise<DefaultPdfAppResult>;
    takePendingPdfPaths(): Promise<string[]>;
    onOpenPdfPaths(listener: (filePaths: string[]) => void): () => void;
  };
  readonly theme: {
    getSnapshot(): Promise<ThemeSnapshot>;
    subscribe(listener: (snapshot: ThemeSnapshot) => void): () => void;
  };
  readonly updates: {
    getStatus(): Promise<UpdateStatus>;
    setFrequency(frequency: UpdateFrequency): Promise<UpdateStatus>;
    checkNow(): Promise<UpdateStatus>;
    installDownloaded(): Promise<void>;
    setRestartBlocked(blocked: boolean): Promise<void>;
    openReleasePage(): Promise<void>;
    onStatusChanged(listener: (status: UpdateStatus) => void): () => void;
  };
  readonly dialogs: {
    openPdfDialog(): Promise<string[] | null>;
    savePdfAsDialog(defaultPath?: string): Promise<string | null>;
    openCanvasDialog(): Promise<string[] | null>;
    saveCanvasAsDialog(defaultPath?: string): Promise<string | null>;
  };
  readonly files: {
    readFile(filePath: string): Promise<Uint8Array>;
    writeFile(filePath: string, bytes: Uint8Array): Promise<void>;
  };
  readonly pdf: {
    loadDocument(filePath: string): Promise<LoadedDocumentPayload>;
    getPageGeometry(request: PageGeometryRequest): Promise<PdfPageGeometryIndex>;
    saveDocument(request: SaveDocumentRequest): Promise<PdfSaveResult>;
  };
  readonly canvas: {
    readDocument(filePath: string): Promise<ButterCanvasDocument>;
    writeDocument(filePath: string, document: ButterCanvasDocument): Promise<void>;
  };
  readonly renderCore: {
    getBackendConfig(): Promise<DesktopRenderBackendConfig>;
    getBackendSelection(): Promise<DesktopRenderBackendSelection>;
    getCapabilities(): Promise<DesktopRenderCapabilities>;
    getDiagnostics(): Promise<RenderCoreDiagnostics>;
    openDocument(request: RenderCoreOpenDocumentRequest): Promise<RenderCoreResult<RenderCoreOpenDocumentResponse>>;
    getPageInfo(request: RenderCoreGetPageInfoRequest): Promise<RenderCoreResult<RenderCorePageInfo>>;
    renderPage(request: RenderCoreRenderPageRequest): Promise<RenderCoreResult<RenderCoreRenderPageResponse>>;
    readSurface(request: RenderCoreReadSurfaceRequest): Promise<RenderCoreResult<RenderCoreReadSurfaceResponse>>;
    releaseSurface(request: RenderCoreReleaseSurfaceRequest): Promise<RenderCoreResult<null>>;
    closeDocument(request: RenderCoreCloseDocumentRequest): Promise<RenderCoreResult<null>>;
  };
  readonly test: {
    resolveFixturePath(name: string): Promise<string | null>;
    getWindowState(): Promise<WindowState | null>;
    setWindowBounds(bounds: Partial<WindowBounds>): Promise<WindowState | null>;
  } | null;
}

export interface ViewerDiagnostics {
  documentPath: string | null;
  documentName: string | null;
  themeMode?: ThemeMode;
  pageCount: number;
  zoom: number;
  activeTool: ToolMode;
  leftSidebarOpen?: boolean;
  rightSidebarOpen?: boolean;
  leftSidebarWidth?: number;
  rightSidebarWidth?: number;
  scrollMode?: ScrollMode;
  scrollWheelMode?: ScrollWheelMode;
  continuousScrollWheelMode?: ScrollWheelMode;
  singlePageScrollWheelMode?: ScrollWheelMode;
  cadScrollWheelMode?: ScrollWheelMode;
  pageColumnsEnabled?: boolean;
  cadViewOrganisation?: 'columns' | 'rows';
  pagesPerColumn?: number;
  zoomPreset?: ZoomPreset;
  currentPage: number;
  visiblePageIndices: number[];
  selectedMarkupIds: string[];
  markupCount: number;
  renderCacheEntries: number;
  renderCacheBytes: number;
  pageRenderReady?: boolean;
  thumbnailRenderReady?: boolean;
  lastPageRenderError?: string | null;
  lastThumbnailRenderError?: string | null;
  thumbnailCacheEntries?: number;
  thumbnailCacheBytes?: number;
  sessionBackendKind?: 'pdfjs' | 'pdfium-render-core' | null;
  surfaceTransportKind?: 'pdfjs-blob-url' | 'pdfium-png-bridge' | null;
  openStageTimings?: DocumentOpenStageTimings | null;
  pageRendererMode?: 'raster' | 'tiled-raster' | 'vector-spike';
  cadRenderExperiment?: string | null;
  overviewThumbnailConcurrencyLimit?: number;
  overviewThumbnailConcurrencyCeiling?: number;
  overviewThumbnailLastThroughputPerSecond?: number | null;
  overviewThumbnailBestThroughputPerSecond?: number | null;
  snapIndexBuilds?: number;
  snapIndexPrimitiveCount?: number;
  totalSnapIndexBuildMs?: number;
  lastSnapIndexBuildMs?: number | null;
  lastSnapIndexPageIndex?: number | null;
  deepZoomRenderCount?: number;
  lastDeepZoomRenderMs?: number | null;
  renderCoordinator?: {
    enabled: boolean;
    sourceSelections: number;
    staleRetentions: number;
    blankAvoidanceSelections: number;
    directRenderFallbacks: number;
    requestsByRole: Record<string, number>;
    requestsByTier: Record<string, number>;
  };
  tabs?: Array<{
    id: string;
    filePath: string;
    fileName: string;
    active: boolean;
    dirty: boolean;
    diagnostics: {
      renderCacheEntries: number;
      renderCacheBytes: number;
      thumbnailCacheEntries: number;
      thumbnailCacheBytes: number;
      firstVisiblePageIndex?: number | null;
      firstVisiblePageReady?: boolean;
      firstVisiblePageReadyAtMs?: number | null;
      firstVisiblePageReadyRequestClass?: RenderCoreRenderRequestClass | null;
      firstVisiblePageWarmupStatus?: 'idle' | 'queued' | 'ready' | 'aborted' | 'error';
      inactive?: boolean;
      queuedPageRenders?: number;
      queuedThumbnailRenders?: number;
      pageRendererMode?: 'raster' | 'tiled-raster' | 'vector-spike';
      cadRenderExperiment?: string | null;
      overviewThumbnailConcurrencyLimit?: number;
      overviewThumbnailConcurrencyCeiling?: number;
      overviewThumbnailLastThroughputPerSecond?: number | null;
      overviewThumbnailBestThroughputPerSecond?: number | null;
      snapIndexBuilds?: number;
      snapIndexPrimitiveCount?: number;
      totalSnapIndexBuildMs?: number;
      lastSnapIndexBuildMs?: number | null;
      deepZoomRenderCount?: number;
      lastDeepZoomRenderMs?: number | null;
    };
  }>;
  activeTabId?: string | null;
  activeTabIndex?: number;
}
