import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  getPerfSnapshot,
  recordCacheReuseFromPrefetch,
  recordOverviewFocusPreviewQuality,
  recordOverviewVisiblePreviewFill,
  recordPageImageVisible,
  recordObsoleteRenderCompletion,
  recordQueuedTaskAbortAfterStart,
  recordQueuedTaskAbortBeforeStart,
  recordQueuedTaskAdoption,
  recordPromotedTaskAbortBeforeStart,
  recordPromotedTaskStart,
  recordQueuedTaskStart,
  recordQueuedTaskTransition,
  recordRasterAbort,
  recordRasterComplete,
  recordRasterError,
  recordRasterRequest,
  recordRasterStart,
  resetPerfTracking,
} from './perfTracker';

describe('perfTracker raster lifecycle accounting', () => {
  const originalWindow = globalThis.window;

  beforeEach(() => {
    Object.defineProperty(globalThis, 'window', {
      value: {
        butterPaper: {
          environment: {
            testMode: true,
            cadRenderExperiment: 'current-control',
          },
        },
      },
      configurable: true,
      writable: true,
    });
    resetPerfTracking();
  });

  afterEach(() => {
    if (originalWindow === undefined) {
      delete (globalThis as { window?: Window }).window;
      return;
    }

    Object.defineProperty(globalThis, 'window', {
      value: originalWindow,
      configurable: true,
      writable: true,
    });
  });

  it('tracks started, completed, and aborted-after-start renders separately', () => {
    recordRasterRequest('page', 'miss');
    recordRasterStart('page');
    recordRasterAbort('page', 'after-start');
    recordRasterRequest('page', 'miss');
    recordRasterStart('page');
    recordRasterComplete('page');
    recordRasterError('page');

    expect(getPerfSnapshot().renderPage).toMatchObject({
      requests: 2,
      hits: 0,
      misses: 2,
      started: 2,
      completed: 1,
      abortedBeforeStart: 0,
      abortedAfterStart: 1,
      errors: 1,
    });
  });

  it('tracks queued aborts before render start', () => {
    recordRasterRequest('thumbnail', 'miss');
    recordRasterAbort('thumbnail', 'before-start');
    recordRasterRequest('thumbnail', 'hit');

    expect(getPerfSnapshot().renderThumbnail).toMatchObject({
      requests: 2,
      hits: 1,
      misses: 1,
      started: 0,
      completed: 0,
      abortedBeforeStart: 1,
      abortedAfterStart: 0,
      errors: 0,
    });
  });

  it('tracks per-request-class raster lifecycle counts', () => {
    recordRasterRequest('page', 'miss', 'nearby-prefetch');
    recordRasterAbort('page', 'before-start', 'nearby-prefetch');
    recordRasterRequest('page', 'miss', 'target-page-hq');
    recordRasterStart('page', 'target-page-hq');
    recordRasterComplete('page', 'target-page-hq');

    expect(getPerfSnapshot().renderPage.byRequestClass).toMatchObject({
      'nearby-prefetch': {
        requests: 1,
        misses: 1,
        abortedBeforeStart: 1,
      },
      'target-page-hq': {
        requests: 1,
        misses: 1,
        started: 1,
        completed: 1,
      },
    });
  });

  it('tracks queue-stability transitions, ages, and prefetch cache reuse', () => {
    recordQueuedTaskTransition('page', 'nearby-prefetch', 'visible-page-preview', 12);
    recordQueuedTaskAdoption('page', 'nearby-prefetch', 'target-page-preview', 9);
    recordPromotedTaskStart('page', 'nearby-prefetch', 'visible-page-preview', 18);
    recordPromotedTaskAbortBeforeStart('page', 'nearby-prefetch', 'target-page-preview', 11);
    recordQueuedTaskStart('page', 'visible-page-preview', 18);
    recordQueuedTaskAbortBeforeStart('page', 'nearby-prefetch', 7);
    recordQueuedTaskAbortAfterStart('page', 'visible-page-preview', 23);
    recordCacheReuseFromPrefetch('page', 'nearby-prefetch', 'target-page-preview');

    expect(getPerfSnapshot().queueStability).toMatchObject({
      classTransitions: {
        'page:nearby-prefetch->visible-page-preview': 1,
      },
      transitionAgeMs: {
        'page:nearby-prefetch->visible-page-preview': {
          count: 1,
          totalMs: 12,
          maxMs: 12,
        },
      },
      startAgeMs: {
        'page:visible-page-preview': {
          count: 1,
          totalMs: 18,
          maxMs: 18,
        },
      },
      abortedBeforeStartAgeMs: {
        'page:nearby-prefetch': {
          count: 1,
          totalMs: 7,
          maxMs: 7,
        },
      },
      abortedAfterStartAgeMs: {
        'page:visible-page-preview': {
          count: 1,
          totalMs: 23,
          maxMs: 23,
        },
      },
      adoptions: {
        'page:nearby-prefetch->target-page-preview': 1,
      },
      adoptionAgeMs: {
        'page:nearby-prefetch->target-page-preview': {
          count: 1,
          totalMs: 9,
          maxMs: 9,
        },
      },
      promotedTaskStarts: {
        'page:nearby-prefetch->visible-page-preview': 1,
      },
      promotedTaskStartAgeMs: {
        'page:nearby-prefetch->visible-page-preview': {
          count: 1,
          totalMs: 18,
          maxMs: 18,
        },
      },
      promotedTaskAbortsBeforeStart: {
        'page:nearby-prefetch->target-page-preview': 1,
      },
      promotedTaskAbortBeforeStartAgeMs: {
        'page:nearby-prefetch->target-page-preview': {
          count: 1,
          totalMs: 11,
          maxMs: 11,
        },
      },
      cacheReuseFromPrefetch: {
        'page:nearby-prefetch->target-page-preview': 1,
      },
    });
  });

  it('tracks acceptable preview timing, medium timing, and obsolete render completions', () => {
    recordPageImageVisible(0, 'preview', 640, 1000);
    recordPageImageVisible(1, 'preview', 700, 1000);
    recordPageImageVisible(2, 'medium', 720, 1000);
    recordObsoleteRenderCompletion('page', 'visible-page-preview');

    const snapshot = getPerfSnapshot();
    expect(snapshot.cadRenderExperiment).toBe('current-control');
    expect(snapshot.firstPageAcceptablePreviewVisibleMs).not.toBeNull();
    expect(snapshot.pageImageVisibility['0']?.acceptablePreviewVisibleMs).toBeUndefined();
    expect(snapshot.pageImageVisibility['1']?.acceptablePreviewVisibleMs).not.toBeUndefined();
    expect(snapshot.pageImageVisibility['2']?.mediumVisibleMs).not.toBeUndefined();
    expect(snapshot.obsoleteRenderCompletions['page:visible-page-preview']).toBe(1);
  });

  it('tracks first visible preview and HQ timings per page', () => {
    recordPageImageVisible(12, 'preview');
    recordPageImageVisible(12, 'full');
    recordPageImageVisible(12, 'full');

    const snapshot = getPerfSnapshot();
    expect(snapshot.firstPageImageVisibleMs).not.toBeNull();
    expect(snapshot.firstPagePreviewVisibleMs).toBe(snapshot.pageImageVisibility['12']?.previewVisibleMs);
    expect(snapshot.firstPageFullVisibleMs).toBe(snapshot.pageImageVisibility['12']?.fullVisibleMs);
    expect(snapshot.pageImageVisibility['12']?.firstVisibleMs).toBe(snapshot.pageImageVisibility['12']?.previewVisibleMs);
  });

  it('tracks CAD overview visible preview fill milestones', () => {
    recordOverviewVisiblePreviewFill(8, 0);
    recordOverviewVisiblePreviewFill(8, 2);
    recordOverviewVisiblePreviewFill(8, 4);
    recordOverviewVisiblePreviewFill(8, 6);
    recordOverviewVisiblePreviewFill(8, 8);

    const fill = getPerfSnapshot().overviewVisiblePreviewFill;
    expect(fill).toMatchObject({
      totalVisible: 8,
      filledVisible: 8,
      acceptableVisible: 8,
      filledRatio: 1,
      acceptableRatio: 1,
    });
    expect(fill?.firstFilledMs).not.toBeNull();
    expect(fill?.firstAcceptableMs).not.toBeNull();
    expect(fill?.reached25Ms).not.toBeNull();
    expect(fill?.reached50Ms).not.toBeNull();
    expect(fill?.reached75Ms).not.toBeNull();
    expect(fill?.reached100Ms).not.toBeNull();
    expect(fill?.acceptableReached25Ms).not.toBeNull();
    expect(fill?.acceptableReached50Ms).not.toBeNull();
    expect(fill?.acceptableReached75Ms).not.toBeNull();
    expect(fill?.acceptableReached100Ms).not.toBeNull();
  });

  it('tracks CAD overview acceptable preview fill separately from any-image fill', () => {
    recordOverviewVisiblePreviewFill(8, 8, 2);

    const fill = getPerfSnapshot().overviewVisiblePreviewFill;
    expect(fill).toMatchObject({
      totalVisible: 8,
      filledVisible: 8,
      acceptableVisible: 2,
      filledRatio: 1,
      acceptableRatio: 0.25,
    });
    expect(fill?.reached100Ms).not.toBeNull();
    expect(fill?.acceptableReached25Ms).not.toBeNull();
    expect(fill?.acceptableReached50Ms).toBeNull();
  });

  it('tracks focused CAD overview acceptable preview timing', () => {
    recordOverviewFocusPreviewQuality({
      pageIndex: 7,
      source: 'viewport-focus',
      requiredWidth: 200,
      renderedWidth: 80,
    });
    recordOverviewFocusPreviewQuality({
      pageIndex: 7,
      source: 'pointer',
      requiredWidth: 200,
      renderedWidth: 160,
    });

    const focus = getPerfSnapshot().overviewFocusPreview;
    expect(focus).toMatchObject({
      pageIndex: 7,
      source: 'pointer',
      requiredWidth: 200,
      renderedWidth: 160,
      acceptable: true,
    });
    expect(focus?.firstSeenMs).not.toBeNull();
    expect(focus?.firstAcceptableMs).not.toBeNull();
  });
});
