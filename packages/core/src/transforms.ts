import type { PdfPoint, Rect, ScreenPoint, Size, ViewportPoint } from './points.js';
import { pdfPoint, rectFromPoints, screenPoint, viewportPoint } from './points.js';

export interface PageGeometry {
  readonly size: Size;
  readonly rotation: 0 | 90 | 180 | 270;
  /** Effective visible PDF page box in unrotated default-user-space coordinates. */
  readonly viewBox?: Rect;
  /** PDF /UserUnit multiplier. Defaults to 1 for legacy geometry. */
  readonly userUnit?: number;
}

export interface PageTransform {
  readonly geometry: PageGeometry;
  readonly zoom: number;
  readonly origin: ScreenPoint;
  pdfToViewport(point: PdfPoint): ViewportPoint;
  viewportToPdf(point: ViewportPoint): PdfPoint;
  pdfRectToViewport(box: Rect): Rect;
  viewportRectToPdf(box: Rect): Rect;
  pdfToScreen(point: PdfPoint): ScreenPoint;
  screenToPdf(point: ScreenPoint): PdfPoint;
}

export const MAX_PAGE_TRANSFORM_ZOOM = 64;

export function clampZoom(value: number, min = 0.1, max = MAX_PAGE_TRANSFORM_ZOOM): number {
  return Math.min(max, Math.max(min, value));
}

export function createPageTransform(
  geometry: PageGeometry,
  zoom: number,
  origin: ScreenPoint = screenPoint(0, 0),
): PageTransform {
  const normalizedZoom = clampZoom(zoom);
  const rotation = ((geometry.rotation % 360) + 360) % 360 as 0 | 90 | 180 | 270;
  const userUnit = resolvePageUserUnit(geometry);
  const viewBox = resolvePageViewBox({ ...geometry, rotation, userUnit });
  const viewportScale = normalizedZoom * userUnit;
  const pdfToViewport = (point: PdfPoint): ViewportPoint => {
    const rotated = rotatePdfToUnscaledViewport(point, viewBox, rotation);
    return viewportPoint(rotated.x * viewportScale, rotated.y * viewportScale);
  };
  const viewportToPdf = (point: ViewportPoint): PdfPoint => {
    const unscaled = {
      x: point.x / viewportScale,
      y: point.y / viewportScale,
    };
    return rotateUnscaledViewportToPdf(unscaled, viewBox, rotation);
  };
  return {
    geometry: {
      size: geometry.size,
      rotation,
      viewBox,
      userUnit,
    },
    zoom: normalizedZoom,
    origin,
    pdfToViewport,
    viewportToPdf,
    pdfRectToViewport(box: Rect): Rect {
      const topLeft = pdfToViewport(pdfPoint(box.x, box.y));
      const bottomRight = pdfToViewport(pdfPoint(box.x + box.width, box.y + box.height));
      return rectFromPoints(topLeft, bottomRight);
    },
    viewportRectToPdf(box: Rect): Rect {
      const topLeft = viewportToPdf(viewportPoint(box.x, box.y));
      const bottomRight = viewportToPdf(viewportPoint(box.x + box.width, box.y + box.height));
      return rectFromPoints(topLeft, bottomRight);
    },
    pdfToScreen(point: PdfPoint): ScreenPoint {
      const viewport = pdfToViewport(point);
      return screenPoint(viewport.x + origin.x, viewport.y + origin.y);
    },
    screenToPdf(point: ScreenPoint): PdfPoint {
      return viewportToPdf(viewportPoint(point.x - origin.x, point.y - origin.y));
    },
  };
}

export function resolvePageUserUnit(geometry: PageGeometry): number {
  return typeof geometry.userUnit === 'number' && Number.isFinite(geometry.userUnit) && geometry.userUnit > 0
    ? geometry.userUnit
    : 1;
}

export function resolvePageViewBox(geometry: PageGeometry): Rect {
  const candidate = geometry.viewBox;
  if (candidate
    && [candidate.x, candidate.y, candidate.width, candidate.height].every(Number.isFinite)
    && candidate.width > 0
    && candidate.height > 0) {
    return { ...candidate };
  }

  const userUnit = resolvePageUserUnit(geometry);
  const rotated = geometry.rotation === 90 || geometry.rotation === 270;
  return {
    x: 0,
    y: 0,
    width: (rotated ? geometry.size.height : geometry.size.width) / userUnit,
    height: (rotated ? geometry.size.width : geometry.size.height) / userUnit,
  };
}

function rotatePdfToUnscaledViewport(
  point: PdfPoint,
  viewBox: Rect,
  rotation: 0 | 90 | 180 | 270,
): { x: number; y: number } {
  const right = viewBox.x + viewBox.width;
  const top = viewBox.y + viewBox.height;
  switch (rotation) {
    case 0:
      return { x: point.x - viewBox.x, y: top - point.y };
    case 90:
      return { x: point.y - viewBox.y, y: point.x - viewBox.x };
    case 180:
      return { x: right - point.x, y: point.y - viewBox.y };
    case 270:
      return { x: top - point.y, y: right - point.x };
  }
}

function rotateUnscaledViewportToPdf(
  point: { x: number; y: number },
  viewBox: Rect,
  rotation: 0 | 90 | 180 | 270,
): PdfPoint {
  const right = viewBox.x + viewBox.width;
  const top = viewBox.y + viewBox.height;
  switch (rotation) {
    case 0:
      return pdfPoint(viewBox.x + point.x, top - point.y);
    case 90:
      return pdfPoint(viewBox.x + point.y, viewBox.y + point.x);
    case 180:
      return pdfPoint(right - point.x, viewBox.y + point.y);
    case 270:
      return pdfPoint(right - point.y, top - point.x);
  }
}
