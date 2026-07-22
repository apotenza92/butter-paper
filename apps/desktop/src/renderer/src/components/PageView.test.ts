import { describe, expect, it } from 'vitest';
import {
  computePagePlaceholderSpinner,
  isDetailCropSurfaceGeometryCompatible,
  resolveCachePromotionMinimumWidth,
  resolveNextPageImageQualityRequest,
  resolvePageImageQuality,
  shouldClearDetailCropSurfaceForViewport,
  shouldRequestColdPrefetchPageRender,
  shouldReplaceDisplayedPageWithRenderError,
  shouldRetryBrokenPageImageSource,
  shouldRetryTargetQualityAfterAbort,
} from './PageView';

describe('page view placeholder', () => {
  it('caps loading spinner size on normal pages', () => {
    expect(computePagePlaceholderSpinner(612, 792)).toEqual({
      size: 28,
      borderWidth: 3,
      animated: true,
    });
  });

  it('shrinks loading spinner for zoomed-out pages', () => {
    expect(computePagePlaceholderSpinner(30.6, 39.6)).toEqual({
      size: 7.92,
      borderWidth: 1,
      animated: false,
    });
  });
});

describe('page view render failures', () => {
  it('keeps the displayed page image mounted when an upgrade render fails', () => {
    expect(shouldReplaceDisplayedPageWithRenderError(true)).toBe(false);
    expect(shouldReplaceDisplayedPageWithRenderError(false)).toBe(true);
  });

  it('retries only when a broken image still has a source URL to discard', () => {
    expect(shouldRetryBrokenPageImageSource('blob:stale')).toBe(true);
    expect(shouldRetryBrokenPageImageSource(null)).toBe(false);
  });

  it('retries aborted target quality only when stable visible content remains mounted', () => {
    expect(shouldRetryTargetQualityAfterAbort({
      isTargetPage: true,
      isStrictlyVisible: true,
      renderUrgency: 'visible',
      viewportInMotion: false,
      hasDisplayedImage: true,
    })).toBe(true);
    expect(shouldRetryTargetQualityAfterAbort({
      isTargetPage: true,
      isStrictlyVisible: true,
      renderUrgency: 'visible',
      viewportInMotion: true,
      hasDisplayedImage: true,
    })).toBe(false);
    expect(shouldRetryTargetQualityAfterAbort({
      isTargetPage: true,
      isStrictlyVisible: true,
      renderUrgency: 'visible',
      viewportInMotion: false,
      hasDisplayedImage: false,
    })).toBe(false);
  });
});

describe('page view image quality classification', () => {
  it('classifies reusable rasters as preview or full', () => {
    const thresholds = {
      upgradeDisplayWidth: 900,
    };

    expect(resolvePageImageQuality(320, thresholds)).toBe('preview');
    expect(resolvePageImageQuality(899, thresholds)).toBe('preview');
    expect(resolvePageImageQuality(900, thresholds)).toBe('full');
  });

  it('promotes only to cached rasters that are better than the current displayed image', () => {
    expect(resolveCachePromotionMinimumWidth({
      currentRenderedWidth: 500,
      desiredDisplayWidth: 1200,
    })).toBe(505);
    expect(resolveCachePromotionMinimumWidth({
      currentRenderedWidth: 1200,
      desiredDisplayWidth: 900,
    })).toBe(1201);
    expect(resolveCachePromotionMinimumWidth({
      currentRenderedWidth: 0,
      desiredDisplayWidth: 900,
    })).toBe(1);
  });
});

describe('page view progressive quality promotion', () => {
  it('promotes visible preview pages to full quality after the dwell delay', () => {
    expect(resolveNextPageImageQualityRequest({
      currentQuality: 'preview',
      renderUrgency: 'visible',
      viewportInMotion: false,
    })).toEqual({
      quality: 'full',
      delayMs: 360,
    });
  });

  it('keeps target preview promotion on the normal dwell delay by default', () => {
    expect(resolveNextPageImageQualityRequest({
      currentQuality: 'preview',
      renderUrgency: 'visible',
      viewportInMotion: false,
    })).toEqual({
      quality: 'full',
      delayMs: 360,
    });
  });

  it('does not promote prefetch pages until they become visible', () => {
    expect(resolveNextPageImageQualityRequest({
      currentQuality: 'preview',
      renderUrgency: 'prefetch',
      viewportInMotion: false,
    })).toBeNull();
  });

  it('suppresses full-quality promotion during viewport motion', () => {
    expect(resolveNextPageImageQualityRequest({
      currentQuality: 'preview',
      renderUrgency: 'visible',
      viewportInMotion: true,
    })).toBeNull();
  });

  it('immediately promotes thumbnail navigation targets even while the viewport is settling', () => {
    expect(resolveNextPageImageQualityRequest({
      currentQuality: 'preview',
      renderUrgency: 'visible',
      viewportInMotion: true,
      immediateTargetPromotion: true,
    })).toEqual({
      quality: 'full',
      delayMs: 0,
    });
  });

});

describe('page view cold prefetch gating', () => {
  it('allows cold page prefetch renders only after motion settles and the render backlog is idle', () => {
    expect(shouldRequestColdPrefetchPageRender({
      viewportInMotion: false,
      renderBacklogIdle: true,
    })).toBe(true);
    expect(shouldRequestColdPrefetchPageRender({
      viewportInMotion: true,
      renderBacklogIdle: true,
    })).toBe(false);
    expect(shouldRequestColdPrefetchPageRender({
      viewportInMotion: false,
      renderBacklogIdle: false,
    })).toBe(false);
  });
});

describe('page view detail crop overlay', () => {
  it('clears stale crop overlays while the viewport is moving', () => {
    expect(shouldClearDetailCropSurfaceForViewport({
      viewportInMotion: true,
      currentCropKey: '0:0:100:100',
      surfaceCropKey: '0:0:100:100',
    })).toBe(true);
  });

  it('clears crop overlays when the visible crop changes', () => {
    expect(shouldClearDetailCropSurfaceForViewport({
      viewportInMotion: false,
      currentCropKey: '0:120:100:100',
      surfaceCropKey: '0:0:100:100',
    })).toBe(true);
  });

  it('keeps crop overlays only when motion has settled and the crop still matches', () => {
    expect(shouldClearDetailCropSurfaceForViewport({
      viewportInMotion: false,
      currentCropKey: '0:0:100:100',
      surfaceCropKey: '0:0:100:100',
    })).toBe(false);
  });

  it('rejects crop surfaces that would be stretched into the overlay rectangle', () => {
    expect(isDetailCropSurfaceGeometryCompatible(
      { renderedWidth: 800, renderedHeight: 400 },
      { width: 400, height: 200 },
    )).toBe(true);
    expect(isDetailCropSurfaceGeometryCompatible(
      { renderedWidth: 800, renderedHeight: 400 },
      { width: 400, height: 300 },
    )).toBe(false);
  });
});
