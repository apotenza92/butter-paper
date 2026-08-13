import { describe, expect, it } from 'vitest';
import {
  capThumbnailPixelRatio,
  clampViewerZoom,
  computeDetailRasterZoom,
  computeDisplayRasterLod,
  computeFullQualityRasterZoom,
  computeInitialPreviewRasterZoom,
  computePreviewRasterZoom,
  MAX_DETAIL_RASTER_ZOOM,
  MAX_RASTER_ZOOM,
  MAX_VIEWER_ZOOM,
  MIN_VIEWER_ZOOM,
  quantizeFitZoom,
  quantizeFitZoomDown,
  quantizeRasterZoom,
  quantizeZoom,
  quantizeZoomDown,
} from './renderZoom';

describe('render zoom helpers', () => {
  it('quantizes arbitrary zoom values to the provided step', () => {
    expect(quantizeZoom(0.892, 0.02)).toBe(0.9);
    expect(quantizeZoom(1.137, 0.05)).toBe(1.15);
  });

  it('uses a finer step for fit zoom', () => {
    expect(quantizeFitZoom(0.892)).toBe(0.9);
    expect(quantizeFitZoom(1.011)).toBe(1.02);
  });

  it('can quantize downward when a zoom must not overflow its bounds', () => {
    expect(quantizeZoomDown(0.899, 0.02)).toBe(0.88);
    expect(quantizeFitZoomDown(1.019)).toBe(1);
  });

  it('uses a coarser step for raster zoom bucketing', () => {
    expect(quantizeRasterZoom(0.892)).toBe(0.9);
    expect(quantizeRasterZoom(0.924)).toBe(0.9);
    expect(quantizeRasterZoom(0.926)).toBe(0.95);
  });

  it('keeps Bluebeam-style viewer zoom bounds while capping full-page raster cost', () => {
    expect(MIN_VIEWER_ZOOM).toBe(0.0625);
    expect(MAX_VIEWER_ZOOM).toBe(64);
    expect(MAX_RASTER_ZOOM).toBeLessThan(MAX_VIEWER_ZOOM);
    expect(clampViewerZoom(128)).toBe(64);
    expect(clampViewerZoom(0.001)).toBe(0.0625);
    expect(clampViewerZoom(1.2345)).toBe(1.234);
    expect(quantizeRasterZoom(MAX_VIEWER_ZOOM)).toBe(MAX_RASTER_ZOOM);
  });

  it('raises full-quality vector raster resolution when the page budget allows it', () => {
    expect(computeFullQualityRasterZoom(64, { width: 792, height: 612 }, 1)).toBe(9.95);
    expect(computeFullQualityRasterZoom(64, { width: 1584, height: 1224 }, 1)).toBe(5);
    expect(computeFullQualityRasterZoom(64, { width: 792, height: 612 }, 2)).toBe(5);
  });

  it('adds a higher detail raster bucket for the active zoomed page', () => {
    expect(MAX_DETAIL_RASTER_ZOOM).toBeGreaterThan(MAX_RASTER_ZOOM);
    expect(computeDetailRasterZoom(64, { width: 792, height: 612 }, 1)).toBe(12.2);
    expect(computeDetailRasterZoom(64, { width: 1584, height: 1224 }, 1)).toBe(6.1);
    expect(computeDetailRasterZoom(64, { width: 792, height: 612 }, 2)).toBe(6.1);
    expect(computeDetailRasterZoom(2, { width: 792, height: 612 }, 1)).toBe(2);
  });

  it('computes a cheaper preview raster zoom bucket', () => {
    expect(computePreviewRasterZoom(0.9)).toBe(0.45);
    expect(computePreviewRasterZoom(0.5)).toBe(0.35);
    expect(computePreviewRasterZoom(1.6)).toBe(0.8);
  });

  it('uses a stronger first preview only for important idle visible pages', () => {
    const baseInput = {
      zoom: 2,
      pageSize: { width: 320, height: 480 },
      pixelRatio: 1,
      isStrictlyVisible: true,
      isTargetPage: false,
      renderUrgency: 'visible' as const,
      viewportInMotion: false,
      renderBacklogIdle: true,
    };

    expect(computeInitialPreviewRasterZoom(baseInput)).toBe(1.35);
    expect(computeInitialPreviewRasterZoom({ ...baseInput, renderBacklogIdle: false })).toBe(1);
    expect(computeInitialPreviewRasterZoom({ ...baseInput, viewportInMotion: true })).toBe(1);
    expect(computeInitialPreviewRasterZoom({ ...baseInput, renderUrgency: 'prefetch' })).toBe(1);
  });

  it('caps stronger first previews by the full raster budget', () => {
    const cappedPreview = computeInitialPreviewRasterZoom({
      zoom: 64,
      pageSize: { width: 792, height: 612 },
      pixelRatio: 1,
      isStrictlyVisible: true,
      isTargetPage: true,
      renderUrgency: 'visible',
      viewportInMotion: false,
      renderBacklogIdle: true,
    });

    expect(cappedPreview).toBe(6.75);
    expect(cappedPreview).toBeLessThanOrEqual(computeFullQualityRasterZoom(64, { width: 792, height: 612 }, 1));
  });

  it('computes visibility-driven display LoD for stable visible pages', () => {
    expect(computeDisplayRasterLod({
      pageSize: { width: 320, height: 480 },
      cssWidth: 320,
      cssHeight: 480,
      zoom: 1,
      pixelRatio: 2,
      isStrictlyVisible: true,
      isTargetPage: false,
      viewportInMotion: false,
    })).toEqual({
      desiredDisplayWidth: 640,
      desiredDisplayHeight: 960,
      desiredRasterZoom: 1,
      minimumReusableDisplayWidth: 352,
      upgradeDisplayWidth: 576,
    });
  });

  it('accepts lower-resolution reusable rasters while the viewport is moving', () => {
    expect(computeDisplayRasterLod({
      pageSize: { width: 320, height: 480 },
      cssWidth: 160,
      cssHeight: 240,
      zoom: 0.5,
      pixelRatio: 1,
      isStrictlyVisible: true,
      isTargetPage: false,
      viewportInMotion: true,
    })).toEqual({
      desiredDisplayWidth: 160,
      desiredDisplayHeight: 240,
      desiredRasterZoom: 0.35,
      minimumReusableDisplayWidth: 80,
      upgradeDisplayWidth: 144,
    });
  });

  it('caps desired LoD scale for high zoom and high DPR pages', () => {
    const lod = computeDisplayRasterLod({
      pageSize: { width: 792, height: 612 },
      cssWidth: 792 * 64,
      cssHeight: 612 * 64,
      zoom: 64,
      pixelRatio: 2,
      isStrictlyVisible: true,
      isTargetPage: true,
      viewportInMotion: false,
    });

    expect(lod.desiredDisplayWidth).toBe(101376);
    expect(lod.desiredRasterZoom).toBe(6.1);
    expect(lod.minimumReusableDisplayWidth).toBe(55756.8);
  });

  it('caps thumbnail pixel ratio to a cheaper MQ-style ceiling', () => {
    expect(capThumbnailPixelRatio(0.5)).toBe(1);
    expect(capThumbnailPixelRatio(1)).toBe(1);
    expect(capThumbnailPixelRatio(1.2)).toBe(1.2);
    expect(capThumbnailPixelRatio(2)).toBe(1.25);
  });
});
