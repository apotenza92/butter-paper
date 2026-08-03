import type { PageTransform, Rect } from '@butter-paper/core';
import type { GeometryPrimitive, ToolGeometryDescriptor } from './types';

export type SelectionMarqueeKind = 'window' | 'crossing';
export type SelectionMarqueeShape = 'box' | 'lasso';
export type SelectionMarqueeOperation = 'replace' | 'add' | 'remove';

export interface ViewportPoint {
  readonly x: number;
  readonly y: number;
}

export interface SelectionMarqueeState {
  readonly pointerId: number | null;
  readonly shape: SelectionMarqueeShape;
  readonly operation: SelectionMarqueeOperation;
  readonly start: ViewportPoint;
  readonly current: ViewportPoint;
  readonly points: readonly ViewportPoint[];
  readonly active: boolean;
}

export interface ViewportBounds {
  readonly left: number;
  readonly right: number;
  readonly top: number;
  readonly bottom: number;
}

const SELECTION_MARQUEE_THRESHOLD_PX = 6;

export function createSelectionMarquee(
  pointerId: number,
  start: ViewportPoint,
  operation: SelectionMarqueeOperation = 'replace',
): SelectionMarqueeState {
  return { pointerId, shape: 'lasso', operation, start, current: start, points: [start], active: false };
}

export function createArmedBoxSelectionMarquee(
  start: ViewportPoint,
  operation: SelectionMarqueeOperation = 'replace',
): SelectionMarqueeState {
  return { pointerId: null, shape: 'box', operation, start, current: start, points: [start], active: false };
}

export function selectionMarqueeOperationFromModifiers(modifiers: {
  readonly shiftKey: boolean;
  readonly altKey: boolean;
}): SelectionMarqueeOperation {
  if (modifiers.altKey) {
    return 'remove';
  }
  return modifiers.shiftKey ? 'add' : 'replace';
}

export function updateSelectionMarquee(
  marquee: SelectionMarqueeState,
  current: ViewportPoint,
  thresholdPx = SELECTION_MARQUEE_THRESHOLD_PX,
): SelectionMarqueeState {
  return {
    ...marquee,
    current,
    points: marquee.shape === 'lasso' ? [...marquee.points, current] : [marquee.start, current],
    active: marquee.active || Math.hypot(current.x - marquee.start.x, current.y - marquee.start.y) > thresholdPx,
  };
}

export function selectionMarqueeKind(start: ViewportPoint, end: ViewportPoint): SelectionMarqueeKind {
  return end.x >= start.x ? 'window' : 'crossing';
}

export function selectionMarqueeBounds(start: ViewportPoint, end: ViewportPoint): ViewportBounds {
  return {
    left: Math.min(start.x, end.x),
    right: Math.max(start.x, end.x),
    top: Math.min(start.y, end.y),
    bottom: Math.max(start.y, end.y),
  };
}

export function selectionAfterMarquee(
  selectedMarkupIds: readonly string[],
  hitMarkupIds: readonly string[],
  operation: SelectionMarqueeOperation,
): string[] {
  if (operation === 'replace') {
    return [...hitMarkupIds];
  }

  if (operation === 'remove') {
    const hitMarkupIdSet = new Set(hitMarkupIds);
    return selectedMarkupIds.filter((markupId) => !hitMarkupIdSet.has(markupId));
  }

  return [...selectedMarkupIds, ...hitMarkupIds.filter((markupId) => !selectedMarkupIds.includes(markupId))];
}

export function isGeometrySelectedByMarquee(
  geometry: ToolGeometryDescriptor,
  marquee: Pick<SelectionMarqueeState, 'start' | 'current' | 'shape' | 'points'>,
  transform: Pick<PageTransform, 'pdfToViewport' | 'pdfRectToViewport'>,
): boolean {
  const kind = selectionMarqueeKind(marquee.start, marquee.current);
  const selectionPath = marquee.shape === 'box'
    ? boxPath(marquee.start, marquee.current)
    : marquee.points;
  if (marquee.shape === 'lasso' && selectionPath.length < 3) {
    return false;
  }
  const componentPaths = geometry.components
    .map((component) => viewportPathForGeometry(component.geometry, transform))
    .filter((path) => path.points.length > 0);

  if (kind === 'window') {
    return componentPaths.length > 0
      && componentPaths.every((path) => path.points.every((point) => pointInSelection(point, selectionPath, marquee.shape)));
  }

  return componentPaths.some((path) => pathsIntersect(path.points, path.closed, selectionPath, marquee.shape));
}

function boxPath(start: ViewportPoint, end: ViewportPoint): ViewportPoint[] {
  const bounds = selectionMarqueeBounds(start, end);
  return [
    { x: bounds.left, y: bounds.top },
    { x: bounds.right, y: bounds.top },
    { x: bounds.right, y: bounds.bottom },
    { x: bounds.left, y: bounds.bottom },
  ];
}

function viewportPathForGeometry(
  geometry: GeometryPrimitive,
  transform: Pick<PageTransform, 'pdfToViewport' | 'pdfRectToViewport'>,
): { readonly points: readonly ViewportPoint[]; readonly closed: boolean } {
  if (geometry.kind === 'rect' || geometry.kind === 'textBox') {
    return { points: viewportRectCorners(geometry.rect, geometry.rotation, transform), closed: true };
  }
  if (geometry.kind === 'line') {
    return { points: [transform.pdfToViewport(geometry.start), transform.pdfToViewport(geometry.end)], closed: false };
  }
  if (geometry.kind === 'generatedPath') {
    return { points: geometry.controlPath.map((point) => transform.pdfToViewport(point)), closed: geometry.closed };
  }
  return {
    points: geometry.points.map((point) => transform.pdfToViewport(point)),
    closed: geometry.kind === 'vertexPath' ? geometry.closed : false,
  };
}

function viewportRectCorners(
  rect: Rect,
  rotation: number | undefined,
  transform: Pick<PageTransform, 'pdfRectToViewport'>,
): ViewportPoint[] {
  const box = transform.pdfRectToViewport(rect);
  const center = { x: box.x + box.width * 0.5, y: box.y + box.height * 0.5 };
  const corners = [
    { x: box.x, y: box.y },
    { x: box.x + box.width, y: box.y },
    { x: box.x + box.width, y: box.y + box.height },
    { x: box.x, y: box.y + box.height },
  ];
  if (!rotation) {
    return corners;
  }

  const radians = (rotation * Math.PI) / 180;
  const cosine = Math.cos(radians);
  const sine = Math.sin(radians);
  return corners.map((point) => {
    const dx = point.x - center.x;
    const dy = point.y - center.y;
    return {
      x: center.x + dx * cosine - dy * sine,
      y: center.y + dx * sine + dy * cosine,
    };
  });
}

function pointInSelection(point: ViewportPoint, selectionPath: readonly ViewportPoint[], shape: SelectionMarqueeShape): boolean {
  return shape === 'box'
    ? pointInBounds(point, selectionMarqueeBounds(selectionPath[0], selectionPath[2]))
    : pointInPolygon(point, selectionPath);
}

function pathsIntersect(
  points: readonly ViewportPoint[],
  closed: boolean,
  selectionPath: readonly ViewportPoint[],
  selectionShape: SelectionMarqueeShape,
): boolean {
  if (points.some((point) => pointInSelection(point, selectionPath, selectionShape))) {
    return true;
  }

  const segmentCount = closed ? points.length : Math.max(0, points.length - 1);
  const selectionSegmentCount = selectionPath.length;
  for (let index = 0; index < segmentCount; index += 1) {
    const start = points[index];
    const end = points[(index + 1) % points.length];
    for (let selectionIndex = 0; selectionIndex < selectionSegmentCount; selectionIndex += 1) {
      if (segmentsIntersect(
        start,
        end,
        selectionPath[selectionIndex],
        selectionPath[(selectionIndex + 1) % selectionPath.length],
      )) {
        return true;
      }
    }
  }
  return false;
}

function pointInBounds(point: ViewportPoint, bounds: ViewportBounds): boolean {
  return point.x >= bounds.left && point.x <= bounds.right && point.y >= bounds.top && point.y <= bounds.bottom;
}

function segmentsIntersect(a: ViewportPoint, b: ViewportPoint, c: ViewportPoint, d: ViewportPoint): boolean {
  const orientation = (p: ViewportPoint, q: ViewportPoint, r: ViewportPoint) => (
    (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
  );
  const abC = orientation(a, b, c);
  const abD = orientation(a, b, d);
  const cdA = orientation(c, d, a);
  const cdB = orientation(c, d, b);
  const epsilon = 1e-8;
  const onSegment = (p: ViewportPoint, q: ViewportPoint, r: ViewportPoint) => (
    Math.abs(orientation(p, q, r)) <= epsilon
    && r.x >= Math.min(p.x, q.x) - epsilon
    && r.x <= Math.max(p.x, q.x) + epsilon
    && r.y >= Math.min(p.y, q.y) - epsilon
    && r.y <= Math.max(p.y, q.y) + epsilon
  );

  if ((abC > epsilon && abD < -epsilon || abC < -epsilon && abD > epsilon)
    && (cdA > epsilon && cdB < -epsilon || cdA < -epsilon && cdB > epsilon)) {
    return true;
  }
  return onSegment(a, b, c) || onSegment(a, b, d) || onSegment(c, d, a) || onSegment(c, d, b);
}

function pointInPolygon(point: ViewportPoint, polygon: readonly ViewportPoint[]): boolean {
  let inside = false;
  for (let current = 0, previous = polygon.length - 1; current < polygon.length; previous = current, current += 1) {
    const a = polygon[current];
    const b = polygon[previous];
    const crosses = (a.y > point.y) !== (b.y > point.y)
      && point.x < ((b.x - a.x) * (point.y - a.y)) / ((b.y - a.y) || 1e-9) + a.x;
    if (crosses) {
      inside = !inside;
    }
  }
  return inside;
}
