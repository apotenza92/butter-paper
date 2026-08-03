import type { PdfPoint } from '@butter-paper/core';

export interface GeneratedLinePath {
  readonly d: string;
  readonly points: readonly PdfPoint[];
  readonly pdfCompatibility?: Record<string, unknown>;
}

export interface LineTypeRenderer<TOptions = Record<string, unknown>> {
  readonly id: string;
  render(input: {
    readonly controlPath: readonly PdfPoint[];
    readonly closed: boolean;
    readonly strokeWidth: number;
    readonly options: TOptions;
  }): GeneratedLinePath;
}

export interface CloudLineTypeOptions {
  readonly offset: number;
  readonly scallopRadius: number;
  readonly scallopSpacing: number;
  readonly pdfBorderEffectIntensity?: number;
}

export const DEFAULT_CLOUD_LINE_OPTIONS: CloudLineTypeOptions = {
  offset: 0,
  scallopRadius: 14.28,
  scallopSpacing: 14.28,
  pdfBorderEffectIntensity: 2,
};

export const CLOUD_LINE_TYPE_RENDERER: LineTypeRenderer<CloudLineTypeOptions> = {
  id: 'cloud',
  render(input) {
    const points = generateCloudScallopPoints(input.controlPath, input.closed, input.options);
    return {
      d: generateCloudScallopPath(input.controlPath, input.closed, input.options),
      points,
      pdfCompatibility: {
        borderEffect: {
          style: 'cloud',
          intensity: input.options.pdfBorderEffectIntensity ?? DEFAULT_CLOUD_LINE_OPTIONS.pdfBorderEffectIntensity,
        },
      },
    };
  },
};

export const LINE_TYPE_RENDERERS = [CLOUD_LINE_TYPE_RENDERER] as const;

export function generateCloudScallopPoints(
  controlPath: readonly PdfPoint[],
  closed: boolean,
  options: CloudLineTypeOptions = DEFAULT_CLOUD_LINE_OPTIONS,
): readonly PdfPoint[] {
  if (controlPath.length < 2) {
    return controlPath;
  }

  const segments = generateCloudCubicSegments(controlPath, closed, options);
  if (segments.length === 0) {
    return controlPath;
  }

  const points: PdfPoint[] = [segments[0].start];
  for (const segment of segments) {
    for (let sample = 1; sample <= CLOUD_CURVE_HIT_TEST_SAMPLES; sample += 1) {
      points.push(cubicPoint(segment, sample / CLOUD_CURVE_HIT_TEST_SAMPLES));
    }
  }
  return points;
}

export function generateCloudScallopPath(
  controlPath: readonly PdfPoint[],
  closed: boolean,
  options: CloudLineTypeOptions = DEFAULT_CLOUD_LINE_OPTIONS,
): string {
  if (controlPath.length < 2) {
    return pointsToPath(controlPath, closed);
  }

  const segments = generateCloudCubicSegments(controlPath, closed, options);
  if (segments.length === 0) {
    return pointsToPath(controlPath, closed);
  }
  const commands = [`M ${segments[0].start.x} ${segments[0].start.y}`];
  for (const segment of segments) {
    commands.push(cubicCommand(segment));
  }
  if (closed) {
    commands.push('Z');
  }
  return commands.join(' ');
}

interface CloudCubicSegment {
  readonly start: PdfPoint;
  readonly control1: PdfPoint;
  readonly control2: PdfPoint;
  readonly end: PdfPoint;
}

const CLOUD_CURVE_HIT_TEST_SAMPLES = 6;
const HALF_ELLIPSE_KAPPA = 0.5522847498;
const BLUEBEAM_CLOUD_PADDING_RATIO = 0.6831;

function generateCloudCubicSegments(
  controlPath: readonly PdfPoint[],
  closed: boolean,
  options: CloudLineTypeOptions,
): readonly CloudCubicSegment[] {
  const orderedControlPath = closed ? normalizeClosedCloudControlPath(controlPath) : controlPath;
  const path = closed ? [...orderedControlPath, orderedControlPath[0]] : [...orderedControlPath];
  const orientation = closed ? polygonSignedArea(orderedControlPath) : 1;
  const spacing = Math.max(3, options.scallopSpacing);
  const radius = Math.max(1, options.scallopRadius);
  const segments: CloudCubicSegment[] = [];

  for (let edgeIndex = 0; edgeIndex < path.length - 1; edgeIndex += 1) {
    const edgeStart = path[edgeIndex];
    const edgeEnd = path[edgeIndex + 1];
    const dx = edgeEnd.x - edgeStart.x;
    const dy = edgeEnd.y - edgeStart.y;
    const length = Math.hypot(dx, dy);
    if (length === 0) {
      continue;
    }

    const lobeCount = Math.max(1, Math.round(length / spacing));
    const lobeLength = length / lobeCount;
    const ux = dx / length;
    const uy = dy / length;
    const normal = outwardNormal(ux, uy, orientation);
    const depth = Math.max(0.5, Math.min(radius * BLUEBEAM_CLOUD_PADDING_RATIO, lobeLength * 0.8) + options.offset);

    for (let lobeIndex = 0; lobeIndex < lobeCount; lobeIndex += 1) {
      const startDistance = lobeIndex * lobeLength;
      const endDistance = (lobeIndex + 1) * lobeLength;
      const middleDistance = startDistance + lobeLength * 0.5;
      const start = lobeIndex === 0 ? edgeStart : edgePoint(edgeStart, ux, uy, startDistance);
      const middle = offsetPoint(edgePoint(edgeStart, ux, uy, middleDistance), normal, depth);
      const end = lobeIndex === lobeCount - 1 ? edgeEnd : edgePoint(edgeStart, ux, uy, endDistance);
      const tangentX = ux * lobeLength * 0.5 * HALF_ELLIPSE_KAPPA;
      const tangentY = uy * lobeLength * 0.5 * HALF_ELLIPSE_KAPPA;
      const normalX = normal.x * depth * HALF_ELLIPSE_KAPPA;
      const normalY = normal.y * depth * HALF_ELLIPSE_KAPPA;

      segments.push({
        start,
        control1: roundedPoint(start.x + normalX, start.y + normalY, start),
        control2: roundedPoint(middle.x - tangentX, middle.y - tangentY, start),
        end: middle,
      });
      segments.push({
        start: middle,
        control1: roundedPoint(middle.x + tangentX, middle.y + tangentY, start),
        control2: roundedPoint(end.x + normalX, end.y + normalY, start),
        end,
      });
    }
  }

  return segments;
}

export function pointsToPath(points: readonly PdfPoint[], closed: boolean): string {
  if (points.length === 0) {
    return '';
  }

  const [firstPoint, ...rest] = points;
  const commands = [`M ${firstPoint.x} ${firstPoint.y}`];
  for (const point of rest) {
    commands.push(`L ${point.x} ${point.y}`);
  }
  if (closed) {
    commands.push('Z');
  }
  return commands.join(' ');
}

function cubicCommand(segment: CloudCubicSegment): string {
  return `C ${segment.control1.x} ${segment.control1.y} ${segment.control2.x} ${segment.control2.y} ${segment.end.x} ${segment.end.y}`;
}

function cubicPoint(segment: CloudCubicSegment, t: number): PdfPoint {
  if (t <= 0) {
    return segment.start;
  }
  if (t >= 1) {
    return segment.end;
  }
  const inverse = 1 - t;
  const x = inverse ** 3 * segment.start.x
    + 3 * inverse ** 2 * t * segment.control1.x
    + 3 * inverse * t ** 2 * segment.control2.x
    + t ** 3 * segment.end.x;
  const y = inverse ** 3 * segment.start.y
    + 3 * inverse ** 2 * t * segment.control1.y
    + 3 * inverse * t ** 2 * segment.control2.y
    + t ** 3 * segment.end.y;
  return roundedPoint(x, y, segment.start);
}

function edgePoint(start: PdfPoint, ux: number, uy: number, distance: number): PdfPoint {
  return roundedPoint(start.x + ux * distance, start.y + uy * distance, start);
}

function offsetPoint(point: PdfPoint, normal: { x: number; y: number }, distance: number): PdfPoint {
  return roundedPoint(point.x + normal.x * distance, point.y + normal.y * distance, point);
}

function roundedPoint(x: number, y: number, source: PdfPoint): PdfPoint {
  return { ...source, x: roundCloudCoordinate(x), y: roundCloudCoordinate(y) };
}

function outwardNormal(ux: number, uy: number, orientation: number): { x: number; y: number } {
  const rightNormal = { x: uy, y: -ux };
  return orientation >= 0 ? rightNormal : { x: -rightNormal.x, y: -rightNormal.y };
}

function normalizeClosedCloudControlPath(points: readonly PdfPoint[]): readonly PdfPoint[] {
  if (points.length !== 4 || !isAxisAlignedRectangle(points)) {
    return points;
  }

  const minX = Math.min(...points.map((point) => point.x));
  const maxX = Math.max(...points.map((point) => point.x));
  const minY = Math.min(...points.map((point) => point.y));
  const maxY = Math.max(...points.map((point) => point.y));
  return [
    pointAt(points, minX, maxY),
    pointAt(points, maxX, maxY),
    pointAt(points, maxX, minY),
    pointAt(points, minX, minY),
  ];
}

function isAxisAlignedRectangle(points: readonly PdfPoint[]): boolean {
  const xs = new Set(points.map((point) => roundCloudCoordinate(point.x)));
  const ys = new Set(points.map((point) => roundCloudCoordinate(point.y)));
  return xs.size === 2 && ys.size === 2;
}

function pointAt(points: readonly PdfPoint[], x: number, y: number): PdfPoint {
  return points.find((point) => point.x === x && point.y === y) ?? { ...points[0], x, y };
}

function polygonSignedArea(points: readonly PdfPoint[]): number {
  let area = 0;
  for (let index = 0; index < points.length; index += 1) {
    const current = points[index];
    const next = points[(index + 1) % points.length];
    area += current.x * next.y - next.x * current.y;
  }
  return area * 0.5;
}

function roundCloudCoordinate(value: number): number {
  return Math.round(value * 10000) / 10000;
}
