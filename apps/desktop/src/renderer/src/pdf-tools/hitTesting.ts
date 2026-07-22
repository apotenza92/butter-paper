import { resolveMarkupAppearance, type Markup, type PdfPoint, type Rect } from '@butter-paper/core';
import type { ToolHandleDescriptor } from './types';

export interface HitTestOptions {
  readonly tolerance: number;
}

export interface MarkupHit {
  readonly markupId: string;
  readonly region: 'edge' | 'interior';
}

export function isPointInRect(point: PdfPoint, rect: Rect): boolean {
  return point.x >= rect.x
    && point.x <= rect.x + rect.width
    && point.y >= rect.y
    && point.y <= rect.y + rect.height;
}

export function isPointNearRectEdge(point: PdfPoint, rect: Rect, tolerance: number): boolean {
  const expanded = expandRect(rect, tolerance);
  const contracted = contractRect(rect, tolerance);
  return isPointInRect(point, expanded) && !isPointInRect(point, contracted);
}

export function isPointNearRotatedRectEdge(point: PdfPoint, rect: Rect, rotation: number | undefined, tolerance: number): boolean {
  const testPoint = rotation ? rotatePointAroundRectCenter(point, rect, rotation) : point;
  return isPointNearRectEdge(testPoint, rect, tolerance);
}

export function isPointInRotatedRect(point: PdfPoint, rect: Rect, rotation: number | undefined): boolean {
  return isPointInRect(rotation ? rotatePointAroundRectCenter(point, rect, rotation) : point, rect);
}

export function isPointInRotatedEllipse(point: PdfPoint, rect: Rect, rotation: number | undefined): boolean {
  const testPoint = rotation ? rotatePointAroundRectCenter(point, rect, rotation) : point;
  const radiusX = rect.width * 0.5;
  const radiusY = rect.height * 0.5;
  if (radiusX <= 0 || radiusY <= 0) return false;
  const centerX = rect.x + radiusX;
  const centerY = rect.y + radiusY;
  return Math.hypot((testPoint.x - centerX) / radiusX, (testPoint.y - centerY) / radiusY) <= 1;
}

export function isPointInPolygon(point: PdfPoint, points: readonly PdfPoint[]): boolean {
  let inside = false;
  for (let current = 0, previous = points.length - 1; current < points.length; previous = current, current += 1) {
    const a = points[current];
    const b = points[previous];
    if (((a.y > point.y) !== (b.y > point.y))
      && point.x < ((b.x - a.x) * (point.y - a.y)) / (b.y - a.y) + a.x) {
      inside = !inside;
    }
  }
  return inside;
}

export function hitTestMarkup(markup: Markup, point: PdfPoint, options: HitTestOptions): MarkupHit | null {
  if (markup.kind === 'rectangle') {
    if (resolveMarkupAppearance(markup).fill?.color && isPointInRotatedRect(point, markup.rect, markup.rotation)) {
      return { markupId: markup.id, region: 'interior' };
    }
    if (isPointNearRotatedRectEdge(point, markup.rect, markup.rotation, options.tolerance)) {
      return { markupId: markup.id, region: 'edge' };
    }

  }

  if (markup.kind === 'ellipse') {
    if (resolveMarkupAppearance(markup).fill?.color && isPointInRotatedEllipse(point, markup.rect, markup.rotation)) {
      return { markupId: markup.id, region: 'interior' };
    }
    if (isPointNearRotatedEllipseEdge(point, markup.rect, markup.rotation, options.tolerance)) {
      return { markupId: markup.id, region: 'edge' };
    }
  }

  if (markup.kind === 'line' || markup.kind === 'arrow') {
    if (isPointNearLineSegment(point, markup.start, markup.end, options.tolerance)) {
      return { markupId: markup.id, region: 'edge' };
    }
  }

  if (markup.kind === 'dimension') {
    const dimensionStart = { ...markup.start, y: markup.start.y + markup.dimensionLineOffset } as PdfPoint;
    const dimensionEnd = { ...markup.end, y: markup.end.y + markup.dimensionLineOffset } as PdfPoint;
    if (isPointNearLineSegment(point, dimensionStart, dimensionEnd, options.tolerance)) {
      return { markupId: markup.id, region: 'edge' };
    }
  }

  if (markup.kind === 'polyline') {
    if (isPointNearPolyline(point, markup.points, options.tolerance)) {
      return { markupId: markup.id, region: 'edge' };
    }
  }

  if (markup.kind === 'polygon') {
    if (resolveMarkupAppearance(markup).fill?.color && isPointInPolygon(point, markup.points)) {
      return { markupId: markup.id, region: 'interior' };
    }
    if (isPointNearPolygonEdge(point, markup.points, options.tolerance)) {
      return { markupId: markup.id, region: 'edge' };
    }
  }

  if (markup.kind === 'cloud') {
    if (isPointNearPolygonEdge(point, markup.controlPath, options.tolerance)) {
      return { markupId: markup.id, region: 'edge' };
    }
  }

  if (markup.kind === 'pen' || markup.kind === 'highlight') {
    if (isPointNearInkPaths(point, markup.paths, Math.max(options.tolerance, (markup.strokeWidth ?? (markup.kind === 'highlight' ? 12 : 1)) * 0.5))) {
      return { markupId: markup.id, region: 'edge' };
    }
  }

  if (markup.kind === 'callout') {
    if (isPointInRect(point, markup.textBox)) {
      return { markupId: markup.id, region: 'interior' };
    }
  }

  if (markup.kind === 'text-box') {
    if (isPointInRect(point, markup.rect)) {
      return { markupId: markup.id, region: 'interior' };
    }
  }

  return null;
}

export function hitTestMarkups(markups: readonly Markup[], point: PdfPoint, options: HitTestOptions): MarkupHit | null {
  for (let index = markups.length - 1; index >= 0; index -= 1) {
    const hit = hitTestMarkup(markups[index], point, options);
    if (hit) {
      return hit;
    }
  }

  return null;
}

export function hitTestHandles(handles: readonly ToolHandleDescriptor[], point: PdfPoint, options: HitTestOptions): ToolHandleDescriptor | null {
  for (let index = handles.length - 1; index >= 0; index -= 1) {
    const handle = handles[index];
    if (distance(point, handle.point) <= options.tolerance) {
      return handle;
    }
  }

  return null;
}

function expandRect(rect: Rect, amount: number): Rect {
  return {
    x: rect.x - amount,
    y: rect.y - amount,
    width: rect.width + amount * 2,
    height: rect.height + amount * 2,
  } as Rect;
}

function contractRect(rect: Rect, amount: number): Rect {
  return {
    x: rect.x + amount,
    y: rect.y + amount,
    width: Math.max(0, rect.width - amount * 2),
    height: Math.max(0, rect.height - amount * 2),
  } as Rect;
}

function distance(a: PdfPoint, b: PdfPoint): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function rotatePointAroundRectCenter(point: PdfPoint, rect: Rect, degrees: number): PdfPoint {
  const center = {
    x: rect.x + rect.width * 0.5,
    y: rect.y + rect.height * 0.5,
  };
  const radians = (degrees * Math.PI) / 180;
  const dx = point.x - center.x;
  const dy = point.y - center.y;

  return {
    ...point,
    x: center.x + dx * Math.cos(radians) - dy * Math.sin(radians),
    y: center.y + dx * Math.sin(radians) + dy * Math.cos(radians),
  };
}

export function isPointNearRotatedEllipseEdge(point: PdfPoint, rect: Rect, rotation: number | undefined, tolerance: number): boolean {
  const testPoint = rotation ? rotatePointAroundRectCenter(point, rect, rotation) : point;
  const radiusX = rect.width * 0.5;
  const radiusY = rect.height * 0.5;
  if (radiusX <= 0 || radiusY <= 0) {
    return false;
  }

  const centerX = rect.x + radiusX;
  const centerY = rect.y + radiusY;
  const normalized = Math.hypot((testPoint.x - centerX) / radiusX, (testPoint.y - centerY) / radiusY);
  const toleranceRatio = Math.max(tolerance / radiusX, tolerance / radiusY);
  return Math.abs(normalized - 1) <= toleranceRatio;
}

export function isPointNearLineSegment(point: PdfPoint, start: PdfPoint, end: PdfPoint, tolerance: number): boolean {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const lengthSquared = dx * dx + dy * dy;
  if (lengthSquared === 0) {
    return distance(point, start) <= tolerance;
  }

  const t = Math.max(0, Math.min(1, ((point.x - start.x) * dx + (point.y - start.y) * dy) / lengthSquared));
  const projection = {
    x: start.x + t * dx,
    y: start.y + t * dy,
  };
  return distance(point, projection as PdfPoint) <= tolerance;
}

export function isPointNearPolyline(point: PdfPoint, points: readonly PdfPoint[], tolerance: number): boolean {
  for (let index = 0; index < points.length - 1; index += 1) {
    if (isPointNearLineSegment(point, points[index], points[index + 1], tolerance)) {
      return true;
    }
  }
  return false;
}

export function isPointNearPolygonEdge(point: PdfPoint, points: readonly PdfPoint[], tolerance: number): boolean {
  if (points.length < 2) {
    return false;
  }
  return isPointNearPolyline(point, [...points, points[0]], tolerance);
}

export function isPointNearInkPaths(point: PdfPoint, paths: readonly (readonly PdfPoint[])[], tolerance: number): boolean {
  return paths.some((path) => isPointNearPolyline(point, path, tolerance));
}
