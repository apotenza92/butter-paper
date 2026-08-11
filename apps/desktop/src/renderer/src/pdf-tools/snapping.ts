import { normalizeRect, pdfPoint, type Markup, type PageModel, type PageTransform, type PdfPoint, type Rect } from '@butter-paper/core';
import type { PdfContentPrimitive, PdfPageGeometryIndex } from '@butter-paper/pdf';
import type { SnapTarget } from '../state/viewerStore';
import { getMarkupToolDefinition } from './toolRegistry';
import type { GeometryPrimitive } from './types';

export type SnapSource = 'annotation' | 'pdf-content';
export type SnapRole = 'vertex' | 'endpoint' | 'midpoint' | 'center' | 'intersection' | 'edge' | 'bounds';

export type SnapCandidate =
  | {
    readonly kind: 'point';
    readonly point: PdfPoint;
    readonly source: SnapSource;
    readonly role: SnapRole;
    readonly ownerId?: string;
  }
  | {
    readonly kind: 'edge';
    readonly start: PdfPoint;
    readonly end: PdfPoint;
    readonly source: SnapSource;
    readonly role: SnapRole;
    readonly ownerId?: string;
  };

export interface SnapResult {
  readonly point: PdfPoint;
  readonly candidate: SnapCandidate;
  readonly distancePx: number;
}

export type OrthogonalAxis = 'horizontal' | 'vertical';

export interface OrthogonalConstraint {
  readonly anchor: PdfPoint;
  readonly point: PdfPoint;
  readonly axis: OrthogonalAxis;
}

export type AcquiredTrackingPoint = Extract<SnapCandidate, { readonly kind: 'point' }>;

export interface ObjectSnapTrackingGuide {
  readonly origin: PdfPoint;
  readonly axis: OrthogonalAxis;
  readonly source: SnapSource;
  readonly role: SnapRole;
  readonly ownerId?: string;
}

export interface ObjectSnapTrackingResult {
  readonly point: PdfPoint;
  readonly guides: readonly ObjectSnapTrackingGuide[];
  readonly distancePx: number;
}

export interface SnapGuideRect {
  readonly ownerId: string;
  readonly rect: Rect;
}

export interface EqualSizeGuide {
  readonly kind: 'equal-size';
  readonly axis: OrthogonalAxis;
  readonly moving: Rect;
  readonly reference: SnapGuideRect;
}

export interface EqualSpacingGuide {
  readonly kind: 'equal-spacing';
  readonly axis: OrthogonalAxis;
  readonly placement: 'before' | 'between' | 'after';
  readonly before: SnapGuideRect;
  readonly moving: Rect;
  readonly after: SnapGuideRect;
}

export type RelationshipSnapGuide = EqualSizeGuide | EqualSpacingGuide;

export interface EqualSizeSnapResult {
  readonly width?: number;
  readonly height?: number;
  readonly guides: readonly EqualSizeGuide[];
}

export interface EqualSpacingSnapResult {
  readonly adjustment: PdfPoint;
  readonly guides: readonly EqualSpacingGuide[];
}

export interface AnnotationSnapOptions {
  readonly excludeMarkupIds?: ReadonlySet<string> | readonly string[];
}

export interface FindSnapOptions {
  readonly tolerancePx?: number;
  readonly excludeOwnerIds?: ReadonlySet<string> | readonly string[];
  readonly snapTargets?: readonly SnapTarget[];
}

const DEFAULT_SNAP_TOLERANCE_PX = 8;
const DEFAULT_TRACKING_TOLERANCE_PX = 6;
const MAX_ACQUIRED_TRACKING_POINTS = 4;
const MAX_INTERSECTION_EDGE_PAIRS = 50000;
const pdfContentSnapCandidateCache = new WeakMap<PdfPageGeometryIndex, readonly SnapCandidate[]>();

export function getAnnotationGuideRects(
  markups: readonly Markup[],
  page: PageModel,
  options: AnnotationSnapOptions = {},
): readonly SnapGuideRect[] {
  const excludedIds = options.excludeMarkupIds instanceof Set
    ? options.excludeMarkupIds
    : new Set(options.excludeMarkupIds ?? []);

  return markups.flatMap((markup) => {
    if (markup.pageIndex !== page.index || excludedIds.has(markup.id)) {
      return [];
    }
    const bounds = getMarkupToolDefinition(markup)?.geometry?.getGeometry(markup as never, { page }).bounds;
    return bounds ? [{ ownerId: markup.id, rect: normalizeRect(bounds) }] : [];
  });
}

export function findEqualSizeSnap(
  moving: Rect,
  references: readonly SnapGuideRect[],
  transform: Pick<PageTransform, 'zoom'>,
  tolerancePx = DEFAULT_TRACKING_TOLERANCE_PX,
): EqualSizeSnapResult | null {
  const normalizedMoving = normalizeRect(moving);
  const zoom = Math.max(transform.zoom, Number.EPSILON);
  const widthMatch = nearestSizeMatch(normalizedMoving.width, references, 'horizontal', zoom, tolerancePx);
  const heightMatch = nearestSizeMatch(normalizedMoving.height, references, 'vertical', zoom, tolerancePx);
  if (!widthMatch && !heightMatch) {
    return null;
  }

  return {
    ...(widthMatch ? { width: widthMatch.reference.rect.width } : {}),
    ...(heightMatch ? { height: heightMatch.reference.rect.height } : {}),
    guides: [
      ...(widthMatch ? [{ kind: 'equal-size' as const, axis: 'horizontal' as const, moving: normalizedMoving, reference: widthMatch.reference }] : []),
      ...(heightMatch ? [{ kind: 'equal-size' as const, axis: 'vertical' as const, moving: normalizedMoving, reference: heightMatch.reference }] : []),
    ],
  };
}

export function findEqualSpacingSnap(
  moving: Rect,
  references: readonly SnapGuideRect[],
  transform: Pick<PageTransform, 'zoom'>,
  tolerancePx = DEFAULT_TRACKING_TOLERANCE_PX,
): EqualSpacingSnapResult | null {
  const normalizedMoving = normalizeRect(moving);
  const zoom = Math.max(transform.zoom, Number.EPSILON);
  const horizontal = nearestSpacingMatch(normalizedMoving, references, 'horizontal', zoom, tolerancePx);
  const vertical = nearestSpacingMatch(normalizedMoving, references, 'vertical', zoom, tolerancePx);
  if (!horizontal && !vertical) {
    return null;
  }

  const adjustment = pdfPoint(horizontal?.adjustment ?? 0, vertical?.adjustment ?? 0);
  const adjustedMoving = normalizeRect({
    ...normalizedMoving,
    x: normalizedMoving.x + adjustment.x,
    y: normalizedMoving.y + adjustment.y,
  });
  return {
    adjustment,
    guides: [
      ...(horizontal ? [{ ...horizontal.guide, moving: adjustedMoving }] : []),
      ...(vertical ? [{ ...vertical.guide, moving: adjustedMoving }] : []),
    ],
  };
}

export function getAnnotationSnapCandidates(
  markups: readonly Markup[],
  page: PageModel,
  options: AnnotationSnapOptions = {},
): readonly SnapCandidate[] {
  const excludedIds = options.excludeMarkupIds instanceof Set
    ? options.excludeMarkupIds
    : new Set(options.excludeMarkupIds ?? []);
  const candidates: SnapCandidate[] = [];

  for (const markup of markups) {
    if (markup.pageIndex !== page.index || excludedIds.has(markup.id)) {
      continue;
    }

    const geometry = getMarkupToolDefinition(markup)?.geometry?.getGeometry(markup as never, { page });
    if (!geometry) {
      continue;
    }

    for (const component of geometry.components) {
      candidates.push(...snapCandidatesForPrimitive(component.geometry, {
        source: 'annotation',
        ownerId: markup.id,
      }));
    }
  }

  return withIntersectionCandidates(candidates);
}

export function getPdfContentSnapCandidates(index: PdfPageGeometryIndex | null | undefined): readonly SnapCandidate[] {
  if (!index) {
    return [];
  }

  const cached = pdfContentSnapCandidateCache.get(index);
  if (cached) {
    return cached;
  }

  const candidates = withIntersectionCandidates(index.primitives.flatMap((primitive) => snapCandidatesForPdfContentPrimitive(primitive)));
  pdfContentSnapCandidateCache.set(index, candidates);
  return candidates;
}

export function findNearestSnapPoint(
  point: PdfPoint,
  candidates: readonly SnapCandidate[],
  transform: Pick<PageTransform, 'zoom'>,
  options: FindSnapOptions = {},
): SnapResult | null {
  const tolerancePx = options.tolerancePx ?? DEFAULT_SNAP_TOLERANCE_PX;
  const excludedIds = !options.excludeOwnerIds
    ? null
    : (options.excludeOwnerIds instanceof Set ? options.excludeOwnerIds : new Set(options.excludeOwnerIds));
  const enabledTargets = options.snapTargets ? new Set(options.snapTargets) : null;
  const zoom = Math.max(transform.zoom, Number.EPSILON);
  const tolerancePdf = tolerancePx / zoom;
  const tolerancePdfSquared = tolerancePdf * tolerancePdf;
  let best: SnapResult | null = null;
  let bestScore = Number.POSITIVE_INFINITY;
  let bestDistancePdfSquared = Number.POSITIVE_INFINITY;

  for (const candidate of candidates) {
    if (candidate.ownerId && excludedIds?.has(candidate.ownerId)) {
      continue;
    }
    if (enabledTargets && !enabledTargets.has(snapTargetForRole(candidate.role))) {
      continue;
    }

    const snapPoint = candidate.kind === 'point'
      ? candidate.point
      : projectPointToSegment(point, candidate.start, candidate.end);
    const distancePdfSquared = squaredDistance(point, snapPoint);
    if (distancePdfSquared > tolerancePdfSquared) {
      continue;
    }

    const score = distancePdfSquared + rolePriority(candidate.role) * tolerancePdfSquared * 0.015;
    if (score >= bestScore) {
      continue;
    }

    bestScore = score;
    bestDistancePdfSquared = distancePdfSquared;
    best = {
      point: snapPoint,
      candidate,
      distancePx: Math.sqrt(distancePdfSquared) * zoom,
    };
  }

  return best;
}

export function constrainPointOrthogonally(anchor: PdfPoint, point: PdfPoint): OrthogonalConstraint {
  const dx = point.x - anchor.x;
  const dy = point.y - anchor.y;
  if (Math.abs(dx) >= Math.abs(dy)) {
    return {
      anchor,
      point: pdfPoint(point.x, anchor.y),
      axis: 'horizontal',
    };
  }

  return {
    anchor,
    point: pdfPoint(anchor.x, point.y),
    axis: 'vertical',
  };
}

export function isPointOnOrthogonalConstraint(
  point: PdfPoint,
  constraint: Pick<OrthogonalConstraint, 'anchor' | 'axis'>,
  epsilon = 0.0001,
): boolean {
  return constraint.axis === 'horizontal'
    ? Math.abs(point.y - constraint.anchor.y) <= epsilon
    : Math.abs(point.x - constraint.anchor.x) <= epsilon;
}

export function trackingPointKey(point: Pick<AcquiredTrackingPoint, 'point'>): string {
  return `${Math.round(point.point.x * 1000)}:${Math.round(point.point.y * 1000)}`;
}

export function toggleAcquiredTrackingPoint(
  acquired: readonly AcquiredTrackingPoint[],
  candidate: AcquiredTrackingPoint,
  maximum = MAX_ACQUIRED_TRACKING_POINTS,
): readonly AcquiredTrackingPoint[] {
  const key = trackingPointKey(candidate);
  const existingIndex = acquired.findIndex((point) => trackingPointKey(point) === key);
  if (existingIndex >= 0) {
    return acquired.filter((_, index) => index !== existingIndex);
  }

  const next = [...acquired, candidate];
  return next.length > maximum ? next.slice(next.length - maximum) : next;
}

export function findObjectSnapTrackingPoint(
  point: PdfPoint,
  acquired: readonly AcquiredTrackingPoint[],
  transform: Pick<PageTransform, 'zoom'>,
  options: {
    readonly tolerancePx?: number;
    readonly allowedGuideAxes?: readonly OrthogonalAxis[];
  } = {},
): ObjectSnapTrackingResult | null {
  if (acquired.length === 0) {
    return null;
  }

  const tolerancePx = options.tolerancePx ?? DEFAULT_TRACKING_TOLERANCE_PX;
  const zoom = Math.max(transform.zoom, Number.EPSILON);
  const allowedAxes = new Set<OrthogonalAxis>(options.allowedGuideAxes ?? ['horizontal', 'vertical']);
  let nearestHorizontal: { readonly guide: ObjectSnapTrackingGuide; readonly distancePx: number } | null = null;
  let nearestVertical: { readonly guide: ObjectSnapTrackingGuide; readonly distancePx: number } | null = null;

  for (const acquiredPoint of acquired) {
    if (allowedAxes.has('horizontal')) {
      const distancePx = Math.abs(point.y - acquiredPoint.point.y) * zoom;
      if (distancePx <= tolerancePx && (!nearestHorizontal || distancePx < nearestHorizontal.distancePx)) {
        nearestHorizontal = {
          guide: {
            origin: acquiredPoint.point,
            axis: 'horizontal',
            source: acquiredPoint.source,
            role: acquiredPoint.role,
            ownerId: acquiredPoint.ownerId,
          },
          distancePx,
        };
      }
    }

    if (allowedAxes.has('vertical')) {
      const distancePx = Math.abs(point.x - acquiredPoint.point.x) * zoom;
      if (distancePx <= tolerancePx && (!nearestVertical || distancePx < nearestVertical.distancePx)) {
        nearestVertical = {
          guide: {
            origin: acquiredPoint.point,
            axis: 'vertical',
            source: acquiredPoint.source,
            role: acquiredPoint.role,
            ownerId: acquiredPoint.ownerId,
          },
          distancePx,
        };
      }
    }
  }

  if (
    nearestHorizontal
    && nearestVertical
    && trackingPointKey({ point: nearestHorizontal.guide.origin }) !== trackingPointKey({ point: nearestVertical.guide.origin })
  ) {
    const trackedPoint = pdfPoint(nearestVertical.guide.origin.x, nearestHorizontal.guide.origin.y);
    return {
      point: trackedPoint,
      guides: [nearestHorizontal.guide, nearestVertical.guide],
      distancePx: Math.sqrt(squaredDistance(point, trackedPoint)) * zoom,
    };
  }

  if (nearestHorizontal && nearestVertical) {
    if (nearestHorizontal.distancePx <= nearestVertical.distancePx) {
      nearestVertical = null;
    } else {
      nearestHorizontal = null;
    }
  }

  if (nearestHorizontal) {
    return {
      point: pdfPoint(point.x, nearestHorizontal.guide.origin.y),
      guides: [nearestHorizontal.guide],
      distancePx: nearestHorizontal.distancePx,
    };
  }

  if (nearestVertical) {
    return {
      point: pdfPoint(nearestVertical.guide.origin.x, point.y),
      guides: [nearestVertical.guide],
      distancePx: nearestVertical.distancePx,
    };
  }

  return null;
}

function nearestSizeMatch(
  size: number,
  references: readonly SnapGuideRect[],
  axis: OrthogonalAxis,
  zoom: number,
  tolerancePx: number,
): { readonly reference: SnapGuideRect; readonly distancePx: number } | null {
  let best: { readonly reference: SnapGuideRect; readonly distancePx: number } | null = null;
  for (const reference of references) {
    const referenceSize = axis === 'horizontal' ? reference.rect.width : reference.rect.height;
    const distancePx = Math.abs(size - referenceSize) * zoom;
    if (distancePx <= tolerancePx && (!best || distancePx < best.distancePx)) {
      best = { reference, distancePx };
    }
  }
  return best;
}

function nearestSpacingMatch(
  moving: Rect,
  references: readonly SnapGuideRect[],
  axis: OrthogonalAxis,
  zoom: number,
  tolerancePx: number,
): {
  readonly adjustment: number;
  readonly guide: Omit<EqualSpacingGuide, 'moving'>;
  readonly distancePx: number;
} | null {
  const movingStart = axis === 'horizontal' ? moving.x : moving.y;
  const movingEnd = movingStart + (axis === 'horizontal' ? moving.width : moving.height);
  const crossStart = axis === 'horizontal' ? moving.y : moving.x;
  const crossEnd = crossStart + (axis === 'horizontal' ? moving.height : moving.width);
  const eligible = references.filter((reference) => {
    const referenceCrossStart = axis === 'horizontal' ? reference.rect.y : reference.rect.x;
    const referenceCrossEnd = referenceCrossStart + (axis === 'horizontal' ? reference.rect.height : reference.rect.width);
    return Math.min(crossEnd, referenceCrossEnd) >= Math.max(crossStart, referenceCrossStart);
  });
  let best: {
    readonly adjustment: number;
    readonly guide: Omit<EqualSpacingGuide, 'moving'>;
    readonly distancePx: number;
  } | null = null;

  for (const before of eligible) {
    const beforeEnd = axis === 'horizontal'
      ? before.rect.x + before.rect.width
      : before.rect.y + before.rect.height;
    for (const after of eligible) {
      const afterStart = axis === 'horizontal' ? after.rect.x : after.rect.y;
      const afterEnd = afterStart + (axis === 'horizontal' ? after.rect.width : after.rect.height);
      if (afterStart < beforeEnd) continue;
      const existingGap = afterStart - beforeEnd;
      const candidates = [
        ...(beforeEnd <= movingStart && afterStart >= movingEnd
          ? [{ placement: 'between' as const, adjustment: ((afterStart - movingEnd) - (movingStart - beforeEnd)) / 2 }]
          : []),
        ...(afterEnd <= movingStart
          ? [{ placement: 'after' as const, adjustment: existingGap - (movingStart - afterEnd) }]
          : []),
        ...(movingEnd <= before.rect[axis === 'horizontal' ? 'x' : 'y']
          ? [{ placement: 'before' as const, adjustment: (before.rect[axis === 'horizontal' ? 'x' : 'y'] - movingEnd) - existingGap }]
          : []),
      ];
      for (const candidate of candidates) {
        const distancePx = Math.abs(candidate.adjustment) * zoom;
        if (distancePx <= tolerancePx && (!best || distancePx < best.distancePx)) {
          best = {
            adjustment: candidate.adjustment,
            distancePx,
            guide: {
              kind: 'equal-spacing',
              axis,
              placement: candidate.placement,
              before,
              after,
            },
          };
        }
      }
    }
  }

  return best;
}

function snapCandidatesForPdfContentPrimitive(primitive: PdfContentPrimitive): readonly SnapCandidate[] {
  if (primitive.kind === 'rect') {
    return snapCandidatesForRect(primitive.rect, 0, { source: 'pdf-content' });
  }

  if (primitive.kind === 'line') {
    return snapCandidatesForOpenPath([primitive.start, primitive.end], {
      source: 'pdf-content',
      endpointRole: 'endpoint',
    });
  }

  return snapCandidatesForPath(primitive.points, primitive.closed, { source: 'pdf-content' });
}

function withIntersectionCandidates(candidates: readonly SnapCandidate[]): readonly SnapCandidate[] {
  const edges = candidates.filter((candidate): candidate is Extract<SnapCandidate, { kind: 'edge' }> => candidate.kind === 'edge');
  if (edges.length < 2 || edges.length * (edges.length - 1) * 0.5 > MAX_INTERSECTION_EDGE_PAIRS) {
    return candidates;
  }

  const intersections: SnapCandidate[] = [];
  const seen = new Set<string>();
  for (let leftIndex = 0; leftIndex < edges.length - 1; leftIndex += 1) {
    for (let rightIndex = leftIndex + 1; rightIndex < edges.length; rightIndex += 1) {
      const point = segmentIntersection(edges[leftIndex], edges[rightIndex]);
      if (!point) {
        continue;
      }

      const key = `${Math.round(point.x * 1000)}:${Math.round(point.y * 1000)}`;
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      intersections.push({
        kind: 'point',
        point,
        source: edges[leftIndex].source === edges[rightIndex].source ? edges[leftIndex].source : 'annotation',
        role: 'intersection',
      });
    }
  }

  return intersections.length > 0 ? [...candidates, ...intersections] : candidates;
}

export function snapCandidatesForPrimitive(
  primitive: GeometryPrimitive,
  metadata: Pick<SnapCandidate, 'source' | 'ownerId'>,
): readonly SnapCandidate[] {
  if (primitive.kind === 'rect' || primitive.kind === 'textBox') {
    return snapCandidatesForRect(primitive.rect, primitive.rotation, metadata);
  }

  if (primitive.kind === 'line') {
    return snapCandidatesForOpenPath([primitive.start, primitive.end], {
      ...metadata,
      endpointRole: 'endpoint',
    });
  }

  if (primitive.kind === 'polyline') {
    return snapCandidatesForOpenPath(primitive.points, {
      ...metadata,
      endpointRole: 'vertex',
    });
  }

  if (primitive.kind === 'vertexPath') {
    return snapCandidatesForPath(primitive.points, primitive.closed, metadata);
  }

  return snapCandidatesForPath(primitive.controlPath, primitive.closed, metadata);
}

function snapCandidatesForRect(
  inputRect: Rect,
  rotation = 0,
  metadata: Pick<SnapCandidate, 'source' | 'ownerId'>,
): readonly SnapCandidate[] {
  const box = normalizeRect(inputRect);
  const center = pdfPoint(box.x + box.width * 0.5, box.y + box.height * 0.5);
  const corners = [
    pdfPoint(box.x, box.y),
    pdfPoint(box.x + box.width, box.y),
    pdfPoint(box.x + box.width, box.y + box.height),
    pdfPoint(box.x, box.y + box.height),
  ].map((point) => rotatePdfPoint(point, center, rotation));

  const candidates: SnapCandidate[] = [
    { kind: 'point', point: center, source: metadata.source, role: 'center', ownerId: metadata.ownerId },
  ];

  for (const corner of corners) {
    candidates.push({ kind: 'point', point: corner, source: metadata.source, role: 'vertex', ownerId: metadata.ownerId });
  }

  addSegmentCandidates(candidates, corners, true, metadata, 'bounds');
  return candidates;
}

function snapCandidatesForOpenPath(
  points: readonly PdfPoint[],
  metadata: Pick<SnapCandidate, 'source' | 'ownerId'> & { readonly endpointRole: SnapRole },
): readonly SnapCandidate[] {
  if (points.length === 0) {
    return [];
  }

  const candidates = points.map((point, index): SnapCandidate => ({
    kind: 'point',
    point,
    source: metadata.source,
    role: index === 0 || index === points.length - 1 ? metadata.endpointRole : 'vertex',
    ownerId: metadata.ownerId,
  }));
  addSegmentCandidates(candidates, points, false, metadata, 'edge');
  return candidates;
}

function snapCandidatesForPath(
  points: readonly PdfPoint[],
  closed: boolean,
  metadata: Pick<SnapCandidate, 'source' | 'ownerId'>,
): readonly SnapCandidate[] {
  const candidates = points.map((point): SnapCandidate => ({
    kind: 'point',
    point,
    source: metadata.source,
    role: 'vertex',
    ownerId: metadata.ownerId,
  }));
  addSegmentCandidates(candidates, points, closed, metadata, 'edge');
  return candidates;
}

function addSegmentCandidates(
  candidates: SnapCandidate[],
  points: readonly PdfPoint[],
  closed: boolean,
  metadata: Pick<SnapCandidate, 'source' | 'ownerId'>,
  edgeRole: SnapRole,
): void {
  const segmentCount = closed ? points.length : Math.max(0, points.length - 1);
  for (let index = 0; index < segmentCount; index += 1) {
    const start = points[index];
    const end = points[(index + 1) % points.length];
    candidates.push({
      kind: 'point',
      point: midpoint(start, end),
      source: metadata.source,
      role: 'midpoint',
      ownerId: metadata.ownerId,
    });
    candidates.push({
      kind: 'edge',
      start,
      end,
      source: metadata.source,
      role: edgeRole,
      ownerId: metadata.ownerId,
    });
  }
}

function midpoint(a: PdfPoint, b: PdfPoint): PdfPoint {
  return pdfPoint((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
}

function projectPointToSegment(point: PdfPoint, start: PdfPoint, end: PdfPoint): PdfPoint {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const lengthSquared = dx * dx + dy * dy;
  if (lengthSquared === 0) {
    return start;
  }

  const t = Math.max(0, Math.min(1, ((point.x - start.x) * dx + (point.y - start.y) * dy) / lengthSquared));
  return pdfPoint(start.x + dx * t, start.y + dy * t);
}

function segmentIntersection(
  left: Pick<Extract<SnapCandidate, { kind: 'edge' }>, 'start' | 'end'>,
  right: Pick<Extract<SnapCandidate, { kind: 'edge' }>, 'start' | 'end'>,
): PdfPoint | null {
  const leftDx = left.end.x - left.start.x;
  const leftDy = left.end.y - left.start.y;
  const rightDx = right.end.x - right.start.x;
  const rightDy = right.end.y - right.start.y;
  const denominator = leftDx * rightDy - leftDy * rightDx;
  if (Math.abs(denominator) < 0.000001) {
    return null;
  }

  const startDx = right.start.x - left.start.x;
  const startDy = right.start.y - left.start.y;
  const leftT = (startDx * rightDy - startDy * rightDx) / denominator;
  const rightT = (startDx * leftDy - startDy * leftDx) / denominator;
  if (leftT < -0.0001 || leftT > 1.0001 || rightT < -0.0001 || rightT > 1.0001) {
    return null;
  }

  const point = pdfPoint(left.start.x + leftT * leftDx, left.start.y + leftT * leftDy);
  if (
    isNearPoint(point, left.start)
    || isNearPoint(point, left.end)
    || isNearPoint(point, right.start)
    || isNearPoint(point, right.end)
  ) {
    return null;
  }

  return point;
}

function snapTargetForRole(role: SnapRole): SnapTarget {
  if (role === 'vertex' || role === 'endpoint') {
    return 'endpoint';
  }
  if (role === 'edge' || role === 'bounds') {
    return 'nearest';
  }
  return role;
}

function rolePriority(role: SnapRole): number {
  switch (role) {
    case 'intersection':
      return 0;
    case 'endpoint':
    case 'vertex':
      return 1;
    case 'midpoint':
      return 2;
    case 'center':
      return 3;
    case 'edge':
    case 'bounds':
      return 8;
  }
}

function rotatePdfPoint(point: PdfPoint, center: PdfPoint, degrees: number): PdfPoint {
  if (!degrees) {
    return point;
  }

  const radians = degrees * Math.PI / 180;
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  const dx = point.x - center.x;
  const dy = point.y - center.y;
  return pdfPoint(center.x + dx * cos - dy * sin, center.y + dx * sin + dy * cos);
}

function squaredDistance(a: { readonly x: number; readonly y: number }, b: { readonly x: number; readonly y: number }): number {
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  return dx * dx + dy * dy;
}

function isNearPoint(a: PdfPoint, b: PdfPoint): boolean {
  return squaredDistance(a, b) < 0.000001;
}
