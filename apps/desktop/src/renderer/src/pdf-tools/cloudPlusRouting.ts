import { pdfPoint, rect, type PdfPoint, type Rect } from '@butter-paper/core';

export type CloudPlusLeaderSide = 'left' | 'right';

export type CloudPlusObstacle =
  | { readonly id?: string; readonly kind: 'rect'; readonly rect: Rect }
  | { readonly id?: string; readonly kind: 'polyline'; readonly points: readonly PdfPoint[] }
  | { readonly id?: string; readonly kind: 'polygon'; readonly points: readonly PdfPoint[] };

export interface CloudPlusRoutingContext {
  readonly pageBounds?: Rect;
  readonly obstacles?: readonly CloudPlusObstacle[];
}

export interface CloudPlusLeaderRouteInput extends CloudPlusRoutingContext {
  readonly controlPath: readonly PdfPoint[];
  /** Sampled points from the generated scalloped appearance, including its closing point. */
  readonly visiblePath: readonly PdfPoint[];
  readonly textBox: Rect;
  readonly previousLeader?: readonly PdfPoint[];
}

export interface CloudPlusLeaderRoute {
  readonly points: readonly PdfPoint[];
  readonly side?: CloudPlusLeaderSide;
  readonly score: number;
}

export interface InitialCloudPlusTextPlacementInput extends CloudPlusRoutingContext {
  readonly controlPath: readonly PdfPoint[];
  readonly visiblePath: readonly PdfPoint[];
  readonly width: number;
  readonly height: number;
  readonly gap?: number;
}

const EPSILON = 0.000001;
const ROUTE_SIDES: readonly CloudPlusLeaderSide[] = ['left', 'right'];
const SIDE_SWITCH_PENALTY = 24;
const WRONG_FACING_PENALTY = 20_000;
const CLOUD_CROSSING_PENALTY = 100_000;
const OBSTACLE_CROSSING_PENALTY = 25_000;
const PAGE_OVERFLOW_PENALTY = 50_000;

/**
 * Produces a Revu-compatible leader: either no points for inline text, or
 * exactly three points ordered cloud tip, knee, text-box connection.
 */
export function routeCloudPlusLeader(input: CloudPlusLeaderRouteInput): CloudPlusLeaderRoute {
  if (isRectWhollyInsidePolygon(input.textBox, input.controlPath)) {
    return { points: [], score: 0 };
  }

  const previousSide = inferLeaderSide(input.previousLeader, input.textBox);
  const cloudBounds = pointsBounds(input.controlPath);
  const detour = Math.max(32, Math.min(96, Math.max(cloudBounds.width, cloudBounds.height) * 0.75));
  const kneeOffsets = input.obstacles && input.obstacles.length > 0 ? [0, -detour, detour] : [0];
  const candidates = ROUTE_SIDES.flatMap((side) => kneeOffsets.map((kneeOffset) =>
    buildRouteCandidate(input, cloudBounds, side, previousSide, kneeOffset)));
  return candidates.reduce((best, candidate) => candidate.score < best.score ? candidate : best);
}

/** Chooses a deterministic initial label position while respecting page edges and nearby content. */
export function placeInitialCloudPlusTextBox(input: InitialCloudPlusTextPlacementInput): {
  readonly textBox: Rect;
  readonly leader: CloudPlusLeaderRoute;
} {
  const cloudBounds = pointsBounds(input.controlPath);
  const gap = Math.max(0, input.gap ?? 24);
  const candidates: readonly Rect[] = [
    rect(cloudBounds.x + cloudBounds.width + gap, cloudBounds.y + (cloudBounds.height - input.height) * 0.5, input.width, input.height),
    rect(cloudBounds.x - gap - input.width, cloudBounds.y + (cloudBounds.height - input.height) * 0.5, input.width, input.height),
    rect(cloudBounds.x + (cloudBounds.width - input.width) * 0.5, cloudBounds.y + cloudBounds.height + gap, input.width, input.height),
    rect(cloudBounds.x + (cloudBounds.width - input.width) * 0.5, cloudBounds.y - gap - input.height, input.width, input.height),
  ];

  return candidates
    .map((textBox, index) => {
      const leader = routeCloudPlusLeader({ ...input, textBox });
      const obstacleOverlap = (input.obstacles ?? []).reduce((sum, obstacle) => sum + rectObstacleOverlap(textBox, obstacle), 0);
      const pageOverflow = input.pageBounds ? rectOverflowArea(textBox, input.pageBounds) : 0;
      // The index is a deterministic tie-breaker and makes right-side placement the default.
      const collisionClass = obstacleOverlap > EPSILON ? 1_000_000 : 0;
      const score = leader.score + collisionClass + obstacleOverlap * 100 + pageOverflow * PAGE_OVERFLOW_PENALTY + index * EPSILON;
      return { textBox, leader, score };
    })
    .reduce((best, candidate) => candidate.score < best.score ? candidate : best);
}

/** Snaps a user-adjusted tip to the sampled scalloped outline. */
export function snapCloudPlusLeaderTip(visiblePath: readonly PdfPoint[], target: PdfPoint): PdfPoint {
  return closestPointOnPolyline(visiblePath, target) ?? target;
}

/**
 * Center-only tests hide the leader too early for concave clouds and large text
 * boxes. This requires the complete rectangle to be contained by the polygon.
 */
export function isRectWhollyInsidePolygon(box: Rect, polygon: readonly PdfPoint[]): boolean {
  if (polygon.length < 3 || box.width < 0 || box.height < 0) {
    return false;
  }
  const corners = rectCorners(box);
  if (!corners.every((point) => isPointInsideOrOnPolygon(point, polygon))) {
    return false;
  }

  // A concave notch can enter the rectangle without excluding a corner.
  const boxEdges = closedSegments(corners);
  const polygonEdges = closedSegments(polygon);
  return !polygonEdges.some(([start, end]) => boxEdges.some(([boxStart, boxEnd]) =>
    segmentsProperlyIntersect(start, end, boxStart, boxEnd)));
}

function buildRouteCandidate(
  input: CloudPlusLeaderRouteInput,
  cloudBounds: Rect,
  side: CloudPlusLeaderSide,
  previousSide: CloudPlusLeaderSide | undefined,
  kneeOffset: number,
): CloudPlusLeaderRoute {
  const connection = connectionPoint(input.textBox, side);
  const direction = outwardDirection(side);
  const tip = directionalAttachmentPoint(input.visiblePath, connection, direction)
    ?? closestPointOnPolyline(input.visiblePath, connection)
    ?? rectCenter(cloudBounds);
  const knee = cleanKnee(tip, connection, side, kneeOffset);
  const points = [tip, knee, connection] as const;
  const textCenter = rectCenter(input.textBox);
  const cloudCenter = rectCenter(cloudBounds);

  let score = polylineLength(points);
  if (!isSideFacingCloud(side, textCenter, cloudCenter)) {
    score += WRONG_FACING_PENALTY;
  }
  score += axisPreferencePenalty(side, input.textBox, cloudBounds);
  if (leaderCrossesCloudInterior(points, input.controlPath)) {
    score += CLOUD_CROSSING_PENALTY;
  }
  score += routeObstacleCrossings(points, input.obstacles ?? []) * OBSTACLE_CROSSING_PENALTY;
  if (input.pageBounds) {
    score += rectOverflowArea(input.textBox, input.pageBounds) * PAGE_OVERFLOW_PENALTY;
    score += points.reduce((sum, point) => sum + pointOutsideDistance(point, input.pageBounds!), 0) * 5_000;
  }
  if (previousSide && previousSide !== side) {
    score += SIDE_SWITCH_PENALTY;
  }

  return { points, side, score };
}

function connectionPoint(box: Rect, side: CloudPlusLeaderSide): PdfPoint {
  const center = rectCenter(box);
  switch (side) {
    case 'left': return pdfPoint(box.x, center.y);
    case 'right': return pdfPoint(box.x + box.width, center.y);
  }
}

function outwardDirection(side: CloudPlusLeaderSide): PdfPoint {
  switch (side) {
    case 'left': return pdfPoint(-1, 0);
    case 'right': return pdfPoint(1, 0);
  }
}

function cleanKnee(tip: PdfPoint, connection: PdfPoint, side: CloudPlusLeaderSide, offset: number): PdfPoint {
  return pdfPoint((tip.x + connection.x) * 0.5, connection.y + offset);
}

function inferLeaderSide(points: readonly PdfPoint[] | undefined, box: Rect): CloudPlusLeaderSide | undefined {
  const connection = points?.at(-1);
  if (!connection) return undefined;
  const distances: readonly [CloudPlusLeaderSide, number][] = [
    ['left', Math.abs(connection.x - box.x)],
    ['right', Math.abs(connection.x - (box.x + box.width))],
  ];
  return distances.reduce((best, candidate) => candidate[1] < best[1] ? candidate : best)[0];
}

function isSideFacingCloud(side: CloudPlusLeaderSide, textCenter: PdfPoint, cloudCenter: PdfPoint): boolean {
  switch (side) {
    case 'left': return cloudCenter.x <= textCenter.x + EPSILON;
    case 'right': return cloudCenter.x >= textCenter.x - EPSILON;
  }
}

function axisPreferencePenalty(side: CloudPlusLeaderSide, textBox: Rect, cloudBounds: Rect): number {
  const textCenter = rectCenter(textBox);
  const cloudCenter = rectCenter(cloudBounds);
  const horizontalDistance = Math.abs(textCenter.x - cloudCenter.x)
    / Math.max(1, (cloudBounds.width + textBox.width) * 0.5);
  const verticalDistance = Math.abs(textCenter.y - cloudCenter.y)
    / Math.max(1, (cloudBounds.height + textBox.height) * 0.5);
  const wrongAxisDistance = Math.max(0, verticalDistance - horizontalDistance);
  return wrongAxisDistance * 5_000;
}

function directionalAttachmentPoint(
  points: readonly PdfPoint[],
  target: PdfPoint,
  direction: PdfPoint,
): PdfPoint | undefined {
  const intersections: PdfPoint[] = [];
  for (const [start, end] of openSegments(points)) {
    if (Math.abs(direction.x) > 0) {
      if (target.y < Math.min(start.y, end.y) - EPSILON || target.y > Math.max(start.y, end.y) + EPSILON) continue;
      if (Math.abs(end.y - start.y) <= EPSILON) {
        if (Math.abs(target.y - start.y) <= EPSILON) intersections.push(start, end);
      } else {
        const amount = (target.y - start.y) / (end.y - start.y);
        if (amount >= -EPSILON && amount <= 1 + EPSILON) {
          intersections.push(pdfPoint(start.x + (end.x - start.x) * amount, target.y));
        }
      }
    } else {
      if (target.x < Math.min(start.x, end.x) - EPSILON || target.x > Math.max(start.x, end.x) + EPSILON) continue;
      if (Math.abs(end.x - start.x) <= EPSILON) {
        if (Math.abs(target.x - start.x) <= EPSILON) intersections.push(start, end);
      } else {
        const amount = (target.x - start.x) / (end.x - start.x);
        if (amount >= -EPSILON && amount <= 1 + EPSILON) {
          intersections.push(pdfPoint(target.x, start.y + (end.y - start.y) * amount));
        }
      }
    }
  }

  return intersections
    .filter((point) => dot(subtract(point, target), direction) >= -EPSILON)
    .reduce<PdfPoint | undefined>((best, point) => !best || squaredDistance(point, target) < squaredDistance(best, target) ? point : best, undefined);
}

function leaderCrossesCloudInterior(points: readonly PdfPoint[], polygon: readonly PdfPoint[]): boolean {
  const boundary = closedSegments(polygon);
  return openSegments(points).some(([start, end]) => {
    if (isPointInPolygon(start, polygon) || isPointInPolygon(end, polygon)) return true;
    if (boundary.some(([edgeStart, edgeEnd]) => segmentsProperlyIntersect(start, end, edgeStart, edgeEnd))) return true;
    return isPointInPolygon(pdfPoint((start.x + end.x) * 0.5, (start.y + end.y) * 0.5), polygon);
  });
}

function routeObstacleCrossings(points: readonly PdfPoint[], obstacles: readonly CloudPlusObstacle[]): number {
  return obstacles.reduce((sum, obstacle) => sum + openSegments(points).reduce((segmentSum, [start, end]) => {
    if (obstacle.kind === 'rect') {
      return segmentSum + (segmentIntersectsRectInterior(start, end, obstacle.rect) ? 1 : 0);
    }
    const obstacleSegments = obstacle.kind === 'polygon' ? closedSegments(obstacle.points) : openSegments(obstacle.points);
    return segmentSum + obstacleSegments.filter(([obstacleStart, obstacleEnd]) => segmentsIntersect(start, end, obstacleStart, obstacleEnd)).length;
  }, 0), 0);
}

function rectObstacleOverlap(box: Rect, obstacle: CloudPlusObstacle): number {
  if (obstacle.kind === 'rect') return intersectionArea(box, obstacle.rect);
  const obstacleBounds = pointsBounds(obstacle.points);
  return intersectionArea(box, obstacleBounds);
}

function rectOverflowArea(box: Rect, bounds: Rect): number {
  return Math.max(0, box.width * box.height - intersectionArea(box, bounds));
}

function pointOutsideDistance(point: PdfPoint, bounds: Rect): number {
  const dx = point.x < bounds.x
    ? bounds.x - point.x
    : point.x > bounds.x + bounds.width
      ? point.x - (bounds.x + bounds.width)
      : 0;
  const dy = point.y < bounds.y
    ? bounds.y - point.y
    : point.y > bounds.y + bounds.height
      ? point.y - (bounds.y + bounds.height)
      : 0;
  return Math.hypot(dx, dy);
}

function intersectionArea(a: Rect, b: Rect): number {
  const width = Math.max(0, Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x));
  const height = Math.max(0, Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y));
  return width * height;
}

function segmentIntersectsRectInterior(start: PdfPoint, end: PdfPoint, box: Rect): boolean {
  if (isPointStrictlyInRect(start, box) || isPointStrictlyInRect(end, box)) return true;
  return closedSegments(rectCorners(box)).some(([a, b]) => segmentsProperlyIntersect(start, end, a, b));
}

function isPointStrictlyInRect(point: PdfPoint, box: Rect): boolean {
  return point.x > box.x + EPSILON && point.x < box.x + box.width - EPSILON
    && point.y > box.y + EPSILON && point.y < box.y + box.height - EPSILON;
}

function isPointInsideOrOnPolygon(point: PdfPoint, polygon: readonly PdfPoint[]): boolean {
  return closedSegments(polygon).some(([start, end]) => isPointOnSegment(point, start, end)) || isPointInPolygon(point, polygon);
}

function isPointInPolygon(point: PdfPoint, polygon: readonly PdfPoint[]): boolean {
  let inside = false;
  for (let current = 0, previous = polygon.length - 1; current < polygon.length; previous = current, current += 1) {
    const a = polygon[current];
    const b = polygon[previous];
    if (((a.y > point.y) !== (b.y > point.y))
      && point.x < ((b.x - a.x) * (point.y - a.y)) / (b.y - a.y) + a.x) {
      inside = !inside;
    }
  }
  return inside;
}

function closestPointOnPolyline(points: readonly PdfPoint[], target: PdfPoint): PdfPoint | undefined {
  let closest: PdfPoint | undefined;
  let closestDistance = Infinity;
  for (const [start, end] of openSegments(points)) {
    const vector = subtract(end, start);
    const denominator = dot(vector, vector);
    const amount = denominator <= EPSILON ? 0 : Math.max(0, Math.min(1, dot(subtract(target, start), vector) / denominator));
    const point = pdfPoint(start.x + vector.x * amount, start.y + vector.y * amount);
    const distance = squaredDistance(point, target);
    if (distance < closestDistance) {
      closest = point;
      closestDistance = distance;
    }
  }
  return closest ?? points[0];
}

function segmentsIntersect(a: PdfPoint, b: PdfPoint, c: PdfPoint, d: PdfPoint): boolean {
  const abC = orientation(a, b, c);
  const abD = orientation(a, b, d);
  const cdA = orientation(c, d, a);
  const cdB = orientation(c, d, b);
  if (((abC > EPSILON && abD < -EPSILON) || (abC < -EPSILON && abD > EPSILON))
    && ((cdA > EPSILON && cdB < -EPSILON) || (cdA < -EPSILON && cdB > EPSILON))) return true;
  return Math.abs(abC) <= EPSILON && isPointOnSegment(c, a, b)
    || Math.abs(abD) <= EPSILON && isPointOnSegment(d, a, b)
    || Math.abs(cdA) <= EPSILON && isPointOnSegment(a, c, d)
    || Math.abs(cdB) <= EPSILON && isPointOnSegment(b, c, d);
}

function segmentsProperlyIntersect(a: PdfPoint, b: PdfPoint, c: PdfPoint, d: PdfPoint): boolean {
  const abC = orientation(a, b, c);
  const abD = orientation(a, b, d);
  const cdA = orientation(c, d, a);
  const cdB = orientation(c, d, b);
  return ((abC > EPSILON && abD < -EPSILON) || (abC < -EPSILON && abD > EPSILON))
    && ((cdA > EPSILON && cdB < -EPSILON) || (cdA < -EPSILON && cdB > EPSILON));
}

function isPointOnSegment(point: PdfPoint, start: PdfPoint, end: PdfPoint): boolean {
  return Math.abs(orientation(start, end, point)) <= EPSILON
    && point.x >= Math.min(start.x, end.x) - EPSILON && point.x <= Math.max(start.x, end.x) + EPSILON
    && point.y >= Math.min(start.y, end.y) - EPSILON && point.y <= Math.max(start.y, end.y) + EPSILON;
}

function orientation(a: PdfPoint, b: PdfPoint, c: PdfPoint): number {
  return (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
}

function rectCorners(box: Rect): readonly PdfPoint[] {
  return [
    pdfPoint(box.x, box.y),
    pdfPoint(box.x + box.width, box.y),
    pdfPoint(box.x + box.width, box.y + box.height),
    pdfPoint(box.x, box.y + box.height),
  ];
}

function pointsBounds(points: readonly PdfPoint[]): Rect {
  if (points.length === 0) return rect(0, 0, 0, 0);
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return rect(x, y, Math.max(...xs) - x, Math.max(...ys) - y);
}

function rectCenter(box: Rect): PdfPoint {
  return pdfPoint(box.x + box.width * 0.5, box.y + box.height * 0.5);
}

function polylineLength(points: readonly PdfPoint[]): number {
  return openSegments(points).reduce((sum, [start, end]) => sum + Math.hypot(end.x - start.x, end.y - start.y), 0);
}

function openSegments(points: readonly PdfPoint[]): readonly (readonly [PdfPoint, PdfPoint])[] {
  return points.slice(0, -1).map((point, index) => [point, points[index + 1]] as const);
}

function closedSegments(points: readonly PdfPoint[]): readonly (readonly [PdfPoint, PdfPoint])[] {
  return points.map((point, index) => [point, points[(index + 1) % points.length]] as const);
}

function subtract(a: PdfPoint, b: PdfPoint): PdfPoint {
  return pdfPoint(a.x - b.x, a.y - b.y);
}

function dot(a: PdfPoint, b: PdfPoint): number {
  return a.x * b.x + a.y * b.y;
}

function squaredDistance(a: PdfPoint, b: PdfPoint): number {
  return (a.x - b.x) ** 2 + (a.y - b.y) ** 2;
}
