import type { WindowBounds } from '../../../shared/protocol';
import { resolveCadRenderExperimentConfig } from '../utils/cadRenderExperiment';

interface RasterStats {
  requests: number;
  hits: number;
  misses: number;
  started: number;
  completed: number;
  abortedBeforeStart: number;
  abortedAfterStart: number;
  errors: number;
  byRequestClass?: Record<string, RasterStats>;
}

interface LongTaskStats {
  count: number;
  totalDuration: number;
  maxDuration: number;
}

interface PageImageVisibilityStats {
  firstVisibleMs?: number;
  previewVisibleMs?: number;
  acceptablePreviewVisibleMs?: number;
  mediumVisibleMs?: number;
  fullVisibleMs?: number;
  detailVisibleMs?: number;
  bestRenderedWidthRatio?: number;
}

interface OverviewVisiblePreviewFillStats {
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
}

interface OverviewFocusPreviewStats {
  pageIndex: number | null;
  source: 'pointer' | 'viewport-focus';
  requiredWidth: number;
  renderedWidth: number;
  acceptable: boolean;
  firstSeenMs: number | null;
  firstAcceptableMs: number | null;
}

interface DurationAggregate {
  count: number;
  totalMs: number;
  maxMs: number;
}

interface QueueStabilityStats {
  classTransitions: Record<string, number>;
  transitionAgeMs: Record<string, DurationAggregate>;
  startAgeMs: Record<string, DurationAggregate>;
  abortedBeforeStartAgeMs: Record<string, DurationAggregate>;
  abortedAfterStartAgeMs: Record<string, DurationAggregate>;
  adoptions: Record<string, number>;
  adoptionAgeMs: Record<string, DurationAggregate>;
  promotedTaskStarts: Record<string, number>;
  promotedTaskStartAgeMs: Record<string, DurationAggregate>;
  promotedTaskAbortsBeforeStart: Record<string, number>;
  promotedTaskAbortBeforeStartAgeMs: Record<string, DurationAggregate>;
  cacheReuseFromPrefetch: Record<string, number>;
}

export interface PerfSnapshot {
  cadRenderExperiment: string | null;
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
  pageImageVisibility: Record<string, PageImageVisibilityStats>;
  overviewVisiblePreviewFill: OverviewVisiblePreviewFillStats | null;
  overviewFocusPreview: OverviewFocusPreviewStats | null;
  firstPageImageVisibleMs: number | null;
  firstPagePreviewVisibleMs: number | null;
  firstPageAcceptablePreviewVisibleMs: number | null;
  firstPageFullVisibleMs: number | null;
  obsoleteRenderCompletions: Record<string, number>;
  queueStability: QueueStabilityStats;
  renderPage: RasterStats;
  renderThumbnail: RasterStats;
  longTasks: LongTaskStats;
  lastWindowBounds: WindowBounds | null;
}

interface PerfState {
  startedAt: number;
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
  pageImageVisibility: Record<string, PageImageVisibilityStats>;
  overviewVisiblePreviewFill: OverviewVisiblePreviewFillStats | null;
  overviewFocusPreview: OverviewFocusPreviewStats | null;
  firstPageImageVisibleMs: number | null;
  firstPagePreviewVisibleMs: number | null;
  firstPageAcceptablePreviewVisibleMs: number | null;
  firstPageFullVisibleMs: number | null;
  obsoleteRenderCompletions: Record<string, number>;
  queueStability: QueueStabilityStats;
  renderPage: RasterStats;
  renderThumbnail: RasterStats;
  longTasks: LongTaskStats;
  lastWindowBounds: WindowBounds | null;
}

let observer: PerformanceObserver | null = null;
let initialised = false;
let state = createInitialState();

function createRasterStats(includeClasses = false): RasterStats {
  return {
    requests: 0,
    hits: 0,
    misses: 0,
    started: 0,
    completed: 0,
    abortedBeforeStart: 0,
    abortedAfterStart: 0,
    errors: 0,
    ...(includeClasses ? { byRequestClass: {} } : {}),
  };
}

function createDurationAggregate(): DurationAggregate {
  return {
    count: 0,
    totalMs: 0,
    maxMs: 0,
  };
}

function createQueueStabilityStats(): QueueStabilityStats {
  return {
    classTransitions: {},
    transitionAgeMs: {},
    startAgeMs: {},
    abortedBeforeStartAgeMs: {},
    abortedAfterStartAgeMs: {},
    adoptions: {},
    adoptionAgeMs: {},
    promotedTaskStarts: {},
    promotedTaskStartAgeMs: {},
    promotedTaskAbortsBeforeStart: {},
    promotedTaskAbortBeforeStartAgeMs: {},
    cacheReuseFromPrefetch: {},
  };
}

function createInitialState(): PerfState {
  return {
    startedAt: now(),
    componentRenders: {},
    detailedComponentRenders: {},
    eventCounts: {},
    placeholderShows: {
      page: 0,
      thumbnail: 0,
    },
    detailedPlaceholderShows: {
      page: {},
      thumbnail: {},
    },
    pageImageVisibility: {},
    overviewVisiblePreviewFill: null,
    overviewFocusPreview: null,
    firstPageImageVisibleMs: null,
    firstPagePreviewVisibleMs: null,
    firstPageAcceptablePreviewVisibleMs: null,
    firstPageFullVisibleMs: null,
    obsoleteRenderCompletions: {},
    queueStability: createQueueStabilityStats(),
    renderPage: createRasterStats(true),
    renderThumbnail: createRasterStats(true),
    longTasks: {
      count: 0,
      totalDuration: 0,
      maxDuration: 0,
    },
    lastWindowBounds: null,
  };
}

function now(): number {
  return typeof performance !== 'undefined' ? performance.now() : Date.now();
}

function isEnabled(): boolean {
  return typeof window !== 'undefined' && Boolean(window.butterPaper?.environment.testMode);
}

function increment(record: Record<string, number>, key: string): void {
  record[key] = (record[key] ?? 0) + 1;
}

export function initialisePerfTracking(): void {
  if (!isEnabled() || initialised) {
    return;
  }

  initialised = true;

  if (typeof PerformanceObserver === 'undefined' || !PerformanceObserver.supportedEntryTypes?.includes('longtask')) {
    return;
  }

  observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      state.longTasks.count += 1;
      state.longTasks.totalDuration += entry.duration;
      state.longTasks.maxDuration = Math.max(state.longTasks.maxDuration, entry.duration);
    }
  });

  observer.observe({ entryTypes: ['longtask'] });
}

export function resetPerfTracking(): void {
  if (!isEnabled()) {
    return;
  }

  state = createInitialState();
}

export function getPerfSnapshot(): PerfSnapshot {
  return {
    cadRenderExperiment: resolveCadRenderExperimentConfig().name,
    elapsedMs: Math.round((now() - state.startedAt) * 1000) / 1000,
    componentRenders: { ...state.componentRenders },
    detailedComponentRenders: { ...state.detailedComponentRenders },
    eventCounts: { ...state.eventCounts },
    placeholderShows: {
      page: state.placeholderShows.page,
      thumbnail: state.placeholderShows.thumbnail,
    },
    detailedPlaceholderShows: {
      page: { ...state.detailedPlaceholderShows.page },
      thumbnail: { ...state.detailedPlaceholderShows.thumbnail },
    },
    pageImageVisibility: Object.fromEntries(
      Object.entries(state.pageImageVisibility).map(([pageIndex, timings]) => [pageIndex, { ...timings }]),
    ),
    overviewVisiblePreviewFill: state.overviewVisiblePreviewFill
      ? { ...state.overviewVisiblePreviewFill }
      : null,
    overviewFocusPreview: state.overviewFocusPreview
      ? { ...state.overviewFocusPreview }
      : null,
    firstPageImageVisibleMs: state.firstPageImageVisibleMs,
    firstPagePreviewVisibleMs: state.firstPagePreviewVisibleMs,
    firstPageAcceptablePreviewVisibleMs: state.firstPageAcceptablePreviewVisibleMs,
    firstPageFullVisibleMs: state.firstPageFullVisibleMs,
    obsoleteRenderCompletions: { ...state.obsoleteRenderCompletions },
    queueStability: cloneQueueStabilityStats(state.queueStability),
    renderPage: cloneRasterStats(state.renderPage),
    renderThumbnail: cloneRasterStats(state.renderThumbnail),
    longTasks: { ...state.longTasks },
    lastWindowBounds: state.lastWindowBounds ? { ...state.lastWindowBounds } : null,
  };
}

export function recordComponentRender(componentName: string, detail?: string | number): void {
  if (!isEnabled()) {
    return;
  }

  increment(state.componentRenders, componentName);
  if (detail !== undefined) {
    increment(state.detailedComponentRenders, `${componentName}:${detail}`);
  }
}

export function recordEvent(name: string): void {
  if (!isEnabled()) {
    return;
  }

  increment(state.eventCounts, name);
}

export function recordPlaceholderShow(kind: 'page' | 'thumbnail', pageIndex: number): void {
  if (!isEnabled()) {
    return;
  }

  state.placeholderShows[kind] += 1;
  increment(state.detailedPlaceholderShows[kind], String(pageIndex));
}

export function recordPageImageVisible(
  pageIndex: number,
  quality: 'preview' | 'medium' | 'full' | 'detail',
  renderedWidth?: number,
  displayedWidth?: number,
): void {
  if (!isEnabled()) {
    return;
  }

  const visibleAtMs = Math.round((now() - state.startedAt) * 1000) / 1000;
  const pageKey = String(pageIndex);
  const target = state.pageImageVisibility[pageKey] ??= {};

  target.firstVisibleMs ??= visibleAtMs;
  if (state.firstPageImageVisibleMs == null) {
    state.firstPageImageVisibleMs = visibleAtMs;
    performance.mark('bp-startup:first-page-image-visible');
  }
  const renderedWidthRatio =
    renderedWidth !== undefined && displayedWidth !== undefined && displayedWidth > 0
      ? Number((renderedWidth / displayedWidth).toFixed(3))
      : undefined;
  if (renderedWidthRatio !== undefined) {
    target.bestRenderedWidthRatio = Math.max(target.bestRenderedWidthRatio ?? 0, renderedWidthRatio);
  }

  if (quality === 'preview') {
    target.previewVisibleMs ??= visibleAtMs;
    state.firstPagePreviewVisibleMs ??= visibleAtMs;
    if (renderedWidthRatio !== undefined && renderedWidthRatio >= 0.65) {
      target.acceptablePreviewVisibleMs ??= visibleAtMs;
      state.firstPageAcceptablePreviewVisibleMs ??= visibleAtMs;
    }
    return;
  }

  if (quality === 'medium') {
    target.mediumVisibleMs ??= visibleAtMs;
    target.acceptablePreviewVisibleMs ??= visibleAtMs;
    state.firstPageAcceptablePreviewVisibleMs ??= visibleAtMs;
    return;
  }

  if (quality === 'detail') {
    target.detailVisibleMs ??= visibleAtMs;
    target.fullVisibleMs ??= visibleAtMs;
    if (state.firstPageFullVisibleMs == null) {
      state.firstPageFullVisibleMs = visibleAtMs;
      performance.mark('bp-startup:first-page-full-visible');
    }
    return;
  }

  target.fullVisibleMs ??= visibleAtMs;
  if (state.firstPageFullVisibleMs == null) {
    state.firstPageFullVisibleMs = visibleAtMs;
    performance.mark('bp-startup:first-page-full-visible');
  }
}

export function recordOverviewVisiblePreviewFill(
  totalVisible: number,
  filledVisible: number,
  acceptableVisible = filledVisible,
): void {
  if (!isEnabled() || totalVisible <= 0) {
    return;
  }

  const safeTotal = Math.max(0, Math.floor(totalVisible));
  const safeFilled = Math.max(0, Math.min(safeTotal, Math.floor(filledVisible)));
  const safeAcceptable = Math.max(0, Math.min(safeTotal, Math.floor(acceptableVisible)));
  const filledRatio = Number((safeFilled / Math.max(1, safeTotal)).toFixed(3));
  const acceptableRatio = Number((safeAcceptable / Math.max(1, safeTotal)).toFixed(3));
  const visibleAtMs = Math.round((now() - state.startedAt) * 1000) / 1000;
  const existing = state.overviewVisiblePreviewFill;
  const target = existing && existing.totalVisible === safeTotal
    ? existing
    : {
        totalVisible: safeTotal,
        filledVisible: 0,
        acceptableVisible: 0,
        filledRatio: 0,
        acceptableRatio: 0,
        firstFilledMs: null,
        firstAcceptableMs: null,
        reached25Ms: null,
        reached50Ms: null,
        reached75Ms: null,
        reached100Ms: null,
        acceptableReached25Ms: null,
        acceptableReached50Ms: null,
        acceptableReached75Ms: null,
        acceptableReached100Ms: null,
      };

  target.filledVisible = Math.max(target.filledVisible, safeFilled);
  target.acceptableVisible = Math.max(target.acceptableVisible, safeAcceptable);
  target.filledRatio = Math.max(target.filledRatio, filledRatio);
  target.acceptableRatio = Math.max(target.acceptableRatio, acceptableRatio);
  if (safeFilled > 0) {
    target.firstFilledMs ??= visibleAtMs;
  }
  if (safeAcceptable > 0) {
    target.firstAcceptableMs ??= visibleAtMs;
  }
  if (filledRatio >= 0.25) {
    target.reached25Ms ??= visibleAtMs;
  }
  if (filledRatio >= 0.5) {
    target.reached50Ms ??= visibleAtMs;
  }
  if (filledRatio >= 0.75) {
    target.reached75Ms ??= visibleAtMs;
  }
  if (filledRatio >= 1) {
    target.reached100Ms ??= visibleAtMs;
  }
  if (acceptableRatio >= 0.25) {
    target.acceptableReached25Ms ??= visibleAtMs;
  }
  if (acceptableRatio >= 0.5) {
    target.acceptableReached50Ms ??= visibleAtMs;
  }
  if (acceptableRatio >= 0.75) {
    target.acceptableReached75Ms ??= visibleAtMs;
  }
  if (acceptableRatio >= 1) {
    target.acceptableReached100Ms ??= visibleAtMs;
  }

  state.overviewVisiblePreviewFill = target;
}

export function recordOverviewFocusPreviewQuality({
  pageIndex,
  source,
  requiredWidth,
  renderedWidth,
}: {
  pageIndex: number | null;
  source: 'pointer' | 'viewport-focus';
  requiredWidth: number;
  renderedWidth: number;
}): void {
  if (!isEnabled() || pageIndex === null || requiredWidth <= 0) {
    return;
  }

  const visibleAtMs = Math.round((now() - state.startedAt) * 1000) / 1000;
  const acceptable = renderedWidth >= requiredWidth * 0.75;
  const existing = state.overviewFocusPreview;
  const target = existing && existing.pageIndex === pageIndex
    ? existing
    : {
        pageIndex,
        source,
        requiredWidth,
        renderedWidth: 0,
        acceptable: false,
        firstSeenMs: null,
        firstAcceptableMs: null,
      };

  if (source === 'pointer' || target.source !== 'pointer') {
    target.source = source;
  }
  target.requiredWidth = Math.max(target.requiredWidth, requiredWidth);
  target.renderedWidth = Math.max(target.renderedWidth, renderedWidth);
  target.firstSeenMs ??= visibleAtMs;
  target.acceptable = target.acceptable || acceptable;
  if (acceptable) {
    target.firstAcceptableMs ??= visibleAtMs;
  }

  state.overviewFocusPreview = target;
}

export function recordObsoleteRenderCompletion(kind: 'page' | 'thumbnail', requestClass?: string): void {
  if (!isEnabled()) {
    return;
  }

  increment(state.obsoleteRenderCompletions, `${kind}:${requestClass ?? 'unknown'}`);
}

export function recordRasterRequest(
  kind: 'page' | 'thumbnail',
  cacheResult: 'hit' | 'miss',
  requestClass?: string,
): void {
  if (!isEnabled()) {
    return;
  }

  mutateRasterStats(kind, requestClass, (target) => {
    target.requests += 1;
    if (cacheResult === 'hit') {
      target.hits += 1;
    } else {
      target.misses += 1;
    }
  });
}

export function recordRasterStart(kind: 'page' | 'thumbnail', requestClass?: string): void {
  if (!isEnabled()) {
    return;
  }

  mutateRasterStats(kind, requestClass, (target) => {
    target.started += 1;
  });
}

export function recordRasterComplete(kind: 'page' | 'thumbnail', requestClass?: string): void {
  if (!isEnabled()) {
    return;
  }

  mutateRasterStats(kind, requestClass, (target) => {
    target.completed += 1;
  });
}

export function recordRasterAbort(
  kind: 'page' | 'thumbnail',
  phase: 'before-start' | 'after-start',
  requestClass?: string,
): void {
  if (!isEnabled()) {
    return;
  }

  mutateRasterStats(kind, requestClass, (target) => {
    if (phase === 'before-start') {
      target.abortedBeforeStart += 1;
      return;
    }

    target.abortedAfterStart += 1;
  });
}

export function recordRasterError(kind: 'page' | 'thumbnail', requestClass?: string): void {
  if (!isEnabled()) {
    return;
  }

  mutateRasterStats(kind, requestClass, (target) => {
    target.errors += 1;
  });
}

export function recordQueuedTaskTransition(
  kind: 'page' | 'thumbnail',
  fromClass: string,
  toClass: string,
  ageMs: number,
): void {
  if (!isEnabled() || fromClass === toClass) {
    return;
  }

  const transitionKey = `${kind}:${fromClass}->${toClass}`;
  increment(state.queueStability.classTransitions, transitionKey);
  updateDurationAggregate(state.queueStability.transitionAgeMs, transitionKey, ageMs);
}

export function recordQueuedTaskStart(kind: 'page' | 'thumbnail', requestClass: string, ageMs: number): void {
  if (!isEnabled()) {
    return;
  }

  updateDurationAggregate(state.queueStability.startAgeMs, `${kind}:${requestClass}`, ageMs);
}

export function recordQueuedTaskAbortBeforeStart(kind: 'page' | 'thumbnail', requestClass: string, ageMs: number): void {
  if (!isEnabled()) {
    return;
  }

  updateDurationAggregate(state.queueStability.abortedBeforeStartAgeMs, `${kind}:${requestClass}`, ageMs);
}

export function recordQueuedTaskAbortAfterStart(kind: 'page' | 'thumbnail', requestClass: string, ageMs: number): void {
  if (!isEnabled()) {
    return;
  }

  updateDurationAggregate(state.queueStability.abortedAfterStartAgeMs, `${kind}:${requestClass}`, ageMs);
}

export function recordQueuedTaskAdoption(
  kind: 'page' | 'thumbnail',
  fromClass: string,
  toClass: string,
  ageMs: number,
): void {
  if (!isEnabled() || fromClass === toClass) {
    return;
  }

  const adoptionKey = `${kind}:${fromClass}->${toClass}`;
  increment(state.queueStability.adoptions, adoptionKey);
  updateDurationAggregate(state.queueStability.adoptionAgeMs, adoptionKey, ageMs);
}

export function recordPromotedTaskStart(
  kind: 'page' | 'thumbnail',
  fromClass: string,
  toClass: string,
  ageMs: number,
): void {
  if (!isEnabled() || fromClass === toClass) {
    return;
  }

  const key = `${kind}:${fromClass}->${toClass}`;
  increment(state.queueStability.promotedTaskStarts, key);
  updateDurationAggregate(state.queueStability.promotedTaskStartAgeMs, key, ageMs);
}

export function recordPromotedTaskAbortBeforeStart(
  kind: 'page' | 'thumbnail',
  fromClass: string,
  toClass: string,
  ageMs: number,
): void {
  if (!isEnabled() || fromClass === toClass) {
    return;
  }

  const key = `${kind}:${fromClass}->${toClass}`;
  increment(state.queueStability.promotedTaskAbortsBeforeStart, key);
  updateDurationAggregate(state.queueStability.promotedTaskAbortBeforeStartAgeMs, key, ageMs);
}

export function recordCacheReuseFromPrefetch(
  kind: 'page' | 'thumbnail',
  sourceRequestClass: string,
  requestClass: string,
): void {
  if (!isEnabled() || sourceRequestClass !== 'nearby-prefetch' || requestClass === sourceRequestClass) {
    return;
  }

  increment(state.queueStability.cacheReuseFromPrefetch, `${kind}:${sourceRequestClass}->${requestClass}`);
}

function cloneRasterStats(stats: RasterStats): RasterStats {
  return {
    requests: stats.requests,
    hits: stats.hits,
    misses: stats.misses,
    started: stats.started,
    completed: stats.completed,
    abortedBeforeStart: stats.abortedBeforeStart,
    abortedAfterStart: stats.abortedAfterStart,
    errors: stats.errors,
    ...(stats.byRequestClass
      ? {
          byRequestClass: Object.fromEntries(
            Object.entries(stats.byRequestClass).map(([requestClass, classStats]) => [requestClass, cloneRasterStats(classStats)]),
          ),
        }
      : {}),
  };
}

function cloneQueueStabilityStats(stats: QueueStabilityStats): QueueStabilityStats {
  return {
    classTransitions: { ...stats.classTransitions },
    transitionAgeMs: cloneDurationAggregateRecord(stats.transitionAgeMs),
    startAgeMs: cloneDurationAggregateRecord(stats.startAgeMs),
    abortedBeforeStartAgeMs: cloneDurationAggregateRecord(stats.abortedBeforeStartAgeMs),
    abortedAfterStartAgeMs: cloneDurationAggregateRecord(stats.abortedAfterStartAgeMs),
    adoptions: { ...stats.adoptions },
    adoptionAgeMs: cloneDurationAggregateRecord(stats.adoptionAgeMs),
    promotedTaskStarts: { ...stats.promotedTaskStarts },
    promotedTaskStartAgeMs: cloneDurationAggregateRecord(stats.promotedTaskStartAgeMs),
    promotedTaskAbortsBeforeStart: { ...stats.promotedTaskAbortsBeforeStart },
    promotedTaskAbortBeforeStartAgeMs: cloneDurationAggregateRecord(stats.promotedTaskAbortBeforeStartAgeMs),
    cacheReuseFromPrefetch: { ...stats.cacheReuseFromPrefetch },
  };
}

function cloneDurationAggregateRecord(record: Record<string, DurationAggregate>): Record<string, DurationAggregate> {
  return Object.fromEntries(
    Object.entries(record).map(([key, value]) => [key, { ...value }]),
  );
}

function updateDurationAggregate(record: Record<string, DurationAggregate>, key: string, durationMs: number): void {
  const target = record[key] ??= createDurationAggregate();
  target.count += 1;
  target.totalMs += durationMs;
  target.maxMs = Math.max(target.maxMs, durationMs);
}

function mutateRasterStats(
  kind: 'page' | 'thumbnail',
  requestClass: string | undefined,
  mutate: (target: RasterStats) => void,
): void {
  const aggregate = kind === 'page' ? state.renderPage : state.renderThumbnail;
  mutate(aggregate);

  if (!requestClass) {
    return;
  }

  const byRequestClass = aggregate.byRequestClass ??= {};
  const classStats = byRequestClass[requestClass] ??= createRasterStats();
  mutate(classStats);
}

export function recordWindowBounds(bounds: WindowBounds): void {
  if (!isEnabled()) {
    return;
  }

  state.lastWindowBounds = { ...bounds };
}
