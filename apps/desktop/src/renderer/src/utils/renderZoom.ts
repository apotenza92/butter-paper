const FIT_ZOOM_STEP = 0.02;
const RASTER_ZOOM_STEP = 0.05;
const THUMBNAIL_MAX_PIXEL_RATIO = 1.25;
const MIN_PREVIEW_RASTER_ZOOM = 0.35;
const PREVIEW_RASTER_RATIO = 0.5;
const IMPORTANT_PREVIEW_RASTER_RATIO = 0.68;
const MAX_RASTER_PIXEL_DIMENSION = 8192;
const MAX_RASTER_PIXELS = 48_000_000;
const MAX_DETAIL_RASTER_PIXEL_DIMENSION = 12288;
const MAX_DETAIL_RASTER_PIXELS = 72_000_000;
const DISPLAY_LOD_MOTION_RATIO = 0.5;
const DISPLAY_LOD_PREFETCH_RATIO = 0.5;
const MIN_STABLE_REUSABLE_DISPLAY_RATIO = 0.55;
const MIN_MOTION_REUSABLE_DISPLAY_RATIO = 0.18;

export const MIN_VIEWER_ZOOM = 0.0625;
export const MAX_VIEWER_ZOOM = 64;
export const MAX_RASTER_ZOOM = 16;
export const MAX_DETAIL_RASTER_ZOOM = 24;

export interface RasterPageSize {
  readonly width: number;
  readonly height: number;
}

export interface DisplayRasterLodInput {
  readonly pageSize?: RasterPageSize;
  readonly cssWidth: number;
  readonly cssHeight: number;
  readonly zoom: number;
  readonly pixelRatio?: number;
  readonly isStrictlyVisible: boolean;
  readonly isTargetPage: boolean;
  readonly viewportInMotion: boolean;
}

export interface DisplayRasterLod {
  readonly desiredDisplayWidth: number;
  readonly desiredDisplayHeight: number;
  readonly desiredRasterZoom: number;
  readonly minimumReusableDisplayWidth: number;
  readonly upgradeDisplayWidth: number;
}

export interface InitialPreviewRasterZoomInput {
  readonly zoom: number;
  readonly pageSize?: RasterPageSize;
  readonly pixelRatio?: number;
  readonly importantPreviewRatio?: number;
  readonly isStrictlyVisible: boolean;
  readonly isTargetPage: boolean;
  readonly renderUrgency: 'visible' | 'prefetch';
  readonly viewportInMotion: boolean;
  readonly renderBacklogIdle: boolean;
}

export function clampViewerZoom(zoom: number): number {
  if (!Number.isFinite(zoom)) {
    return MIN_VIEWER_ZOOM;
  }

  return Math.min(MAX_VIEWER_ZOOM, Math.max(MIN_VIEWER_ZOOM, Number(zoom.toFixed(3))));
}

export function quantizeFitZoom(zoom: number): number {
  return quantizeZoom(zoom, FIT_ZOOM_STEP);
}

export function quantizeFitZoomDown(zoom: number): number {
  return quantizeZoomDown(zoom, FIT_ZOOM_STEP);
}

export function quantizeRasterZoom(zoom: number): number {
  return quantizeRasterZoomForMax(zoom, MAX_RASTER_ZOOM);
}

export function computeFullQualityRasterZoom(
  zoom: number,
  pageSize?: RasterPageSize,
  pixelRatio = 1,
): number {
  return quantizeRasterZoomForMax(Math.min(zoom, computeMaxRasterZoomForPage(
    pageSize,
    pixelRatio,
    {
      maxZoom: MAX_RASTER_ZOOM,
      maxPixelDimension: MAX_RASTER_PIXEL_DIMENSION,
      maxPixels: MAX_RASTER_PIXELS,
    },
  )), MAX_RASTER_ZOOM);
}

export function computeDetailRasterZoom(
  zoom: number,
  pageSize?: RasterPageSize,
  pixelRatio = 1,
): number {
  return quantizeRasterZoomForMax(Math.min(zoom, computeMaxRasterZoomForPage(
    pageSize,
    pixelRatio,
    {
      maxZoom: MAX_DETAIL_RASTER_ZOOM,
      maxPixelDimension: MAX_DETAIL_RASTER_PIXEL_DIMENSION,
      maxPixels: MAX_DETAIL_RASTER_PIXELS,
    },
  )), MAX_DETAIL_RASTER_ZOOM);
}

export function computePreviewRasterZoom(
  zoom: number,
  pageSize?: RasterPageSize,
  pixelRatio = 1,
): number {
  const rasterZoom = computeFullQualityRasterZoom(zoom, pageSize, pixelRatio);
  const scaledPreviewZoom = quantizeRasterZoom(Math.max(MIN_PREVIEW_RASTER_ZOOM, rasterZoom * PREVIEW_RASTER_RATIO));
  return Math.min(rasterZoom, scaledPreviewZoom);
}

export function computeInitialPreviewRasterZoom({
  zoom,
  pageSize,
  pixelRatio = 1,
  importantPreviewRatio = IMPORTANT_PREVIEW_RASTER_RATIO,
  isStrictlyVisible,
  isTargetPage,
  renderUrgency,
  viewportInMotion,
  renderBacklogIdle,
}: InitialPreviewRasterZoomInput): number {
  const basePreviewZoom = computePreviewRasterZoom(zoom, pageSize, pixelRatio);
  const shouldUseImportantPreview = renderUrgency === 'visible'
    && renderBacklogIdle
    && !viewportInMotion
    && (isStrictlyVisible || isTargetPage);

  if (!shouldUseImportantPreview) {
    return basePreviewZoom;
  }

  const fullRasterZoom = computeFullQualityRasterZoom(zoom, pageSize, pixelRatio);
  const importantPreviewZoom = quantizeRasterZoomForMax(
    Math.max(basePreviewZoom, fullRasterZoom * importantPreviewRatio),
    MAX_RASTER_ZOOM,
  );

  return Math.min(fullRasterZoom, importantPreviewZoom);
}

export function computeDisplayRasterLod({
  pageSize,
  cssWidth,
  cssHeight,
  zoom,
  pixelRatio = 1,
  isStrictlyVisible,
  isTargetPage,
  viewportInMotion,
}: DisplayRasterLodInput): DisplayRasterLod {
  const safePixelRatio = Number.isFinite(pixelRatio) && pixelRatio > 0 ? pixelRatio : 1;
  const safeWidth = Number.isFinite(cssWidth) && cssWidth > 0 ? cssWidth : 1;
  const safeHeight = Number.isFinite(cssHeight) && cssHeight > 0 ? cssHeight : 1;
  const displayWidth = safeWidth * safePixelRatio;
  const displayHeight = safeHeight * safePixelRatio;
  const fullRasterZoom = isTargetPage
    ? computeDetailRasterZoom(zoom, pageSize, safePixelRatio)
    : computeFullQualityRasterZoom(zoom, pageSize, safePixelRatio);
  const displayRasterZoom = viewportInMotion || !isStrictlyVisible
    ? Math.max(MIN_PREVIEW_RASTER_ZOOM, fullRasterZoom * (isStrictlyVisible ? DISPLAY_LOD_MOTION_RATIO : DISPLAY_LOD_PREFETCH_RATIO))
    : fullRasterZoom;
  const desiredRasterZoom = Math.min(fullRasterZoom, quantizeRasterZoomForMax(displayRasterZoom, isTargetPage ? MAX_DETAIL_RASTER_ZOOM : MAX_RASTER_ZOOM));
  const reusableRatio = viewportInMotion ? MIN_MOTION_REUSABLE_DISPLAY_RATIO : MIN_STABLE_REUSABLE_DISPLAY_RATIO;

  return {
    desiredDisplayWidth: Number(displayWidth.toFixed(2)),
    desiredDisplayHeight: Number(displayHeight.toFixed(2)),
    desiredRasterZoom,
    minimumReusableDisplayWidth: Number((displayWidth * reusableRatio).toFixed(2)),
    upgradeDisplayWidth: Number((displayWidth * 0.9).toFixed(2)),
  };
}

function quantizeRasterZoomForMax(zoom: number, maxZoom: number): number {
  return quantizeZoom(Math.min(maxZoom, zoom), RASTER_ZOOM_STEP);
}

export function capThumbnailPixelRatio(pixelRatio: number): number {
  if (!Number.isFinite(pixelRatio) || pixelRatio <= 0) {
    return 1;
  }

  return Number(Math.min(THUMBNAIL_MAX_PIXEL_RATIO, Math.max(1, pixelRatio)).toFixed(2));
}

export function quantizeZoom(zoom: number, step: number): number {
  if (!Number.isFinite(zoom) || !Number.isFinite(step) || step <= 0) {
    return zoom;
  }

  return Number((Math.round(zoom / step) * step).toFixed(3));
}

export function quantizeZoomDown(zoom: number, step: number): number {
  if (!Number.isFinite(zoom) || !Number.isFinite(step) || step <= 0) {
    return zoom;
  }

  return Number((Math.floor(zoom / step) * step).toFixed(3));
}

interface RasterBudget {
  readonly maxZoom: number;
  readonly maxPixelDimension: number;
  readonly maxPixels: number;
}

function computeMaxRasterZoomForPage(
  pageSize: RasterPageSize | undefined,
  pixelRatio: number,
  budget: RasterBudget,
): number {
  if (
    !pageSize ||
    !Number.isFinite(pageSize.width) ||
    !Number.isFinite(pageSize.height) ||
    pageSize.width <= 0 ||
    pageSize.height <= 0
  ) {
    return budget.maxZoom;
  }

  const ratio = Number.isFinite(pixelRatio) && pixelRatio > 0 ? pixelRatio : 1;
  const maxByWidth = budget.maxPixelDimension / (pageSize.width * ratio);
  const maxByHeight = budget.maxPixelDimension / (pageSize.height * ratio);
  const maxByArea = Math.sqrt(budget.maxPixels / (pageSize.width * pageSize.height)) / ratio;
  return Math.max(MIN_VIEWER_ZOOM, Math.min(budget.maxZoom, maxByWidth, maxByHeight, maxByArea));
}
