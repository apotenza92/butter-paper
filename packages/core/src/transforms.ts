import type { PdfPoint, Rect, ScreenPoint, Size, ViewportPoint } from './points.js';
import { pdfPoint, rectFromPoints, screenPoint, viewportPoint } from './points.js';

export interface PageGeometry {
  readonly size: Size;
  readonly rotation: 0 | 90 | 180 | 270;
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
  const pdfToViewport = (point: PdfPoint): ViewportPoint => {
    const rotated = rotatePdfToUnscaledViewport(point, geometry.size, rotation);
    return viewportPoint(rotated.x * normalizedZoom, rotated.y * normalizedZoom);
  };
  const viewportToPdf = (point: ViewportPoint): PdfPoint => {
    const unscaled = {
      x: point.x / normalizedZoom,
      y: point.y / normalizedZoom,
    };
    return rotateUnscaledViewportToPdf(unscaled, geometry.size, rotation);
  };
  return {
    geometry: {
      size: geometry.size,
      rotation,
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

function rotatePdfToUnscaledViewport(
  point: PdfPoint,
  size: Size,
  rotation: 0 | 90 | 180 | 270,
): { x: number; y: number } {
  switch (rotation) {
    case 0:
      return { x: point.x, y: size.height - point.y };
    case 90:
      return { x: point.y, y: point.x };
    case 180:
      return { x: size.width - point.x, y: point.y };
    case 270:
      return { x: size.height - point.y, y: size.width - point.x };
  }
}

function rotateUnscaledViewportToPdf(
  point: { x: number; y: number },
  size: Size,
  rotation: 0 | 90 | 180 | 270,
): PdfPoint {
  switch (rotation) {
    case 0:
      return pdfPoint(point.x, size.height - point.y);
    case 90:
      return pdfPoint(point.y, point.x);
    case 180:
      return pdfPoint(size.width - point.x, point.y);
    case 270:
      return pdfPoint(size.width - point.y, size.height - point.x);
  }
}
