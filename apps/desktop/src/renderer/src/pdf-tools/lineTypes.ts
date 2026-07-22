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

  const orderedControlPath = closed ? normalizeClosedCloudControlPath(controlPath) : controlPath;
  const path = closed ? [...orderedControlPath, orderedControlPath[0]] : [...orderedControlPath];
  const orientation = closed ? polygonSignedArea(orderedControlPath) : 1;
  const spacing = Math.max(3, options.scallopSpacing);
  const radius = Math.max(1, options.scallopRadius + options.offset);
  const outwardOffset = options.offset > 0 ? options.offset : radius * 0.3316;
  const generated: PdfPoint[] = [];

  for (let index = 0; index < path.length - 1; index += 1) {
    const start = path[index];
    const end = path[index + 1];
    const dx = end.x - start.x;
    const dy = end.y - start.y;
    const length = Math.hypot(dx, dy);
    if (length === 0) {
      continue;
    }

    const segmentCount = Math.max(1, Math.round(length / spacing));
    const ux = dx / length;
    const uy = dy / length;
    const normal = outwardNormal(ux, uy, orientation);

    for (let segmentIndex = 0; segmentIndex < segmentCount; segmentIndex += 1) {
      const lobeLength = spacing;
      const lobeStart = (segmentIndex + 0.5) * spacing;
      for (const sample of BLUEBEAM_CLOUD_LOBE_SAMPLES) {
        if (generated.length > 0 && segmentIndex > 0 && sample.t === 0 && sample.n === 0) continue;
        const distanceAlong = lobeStart + sample.t * lobeLength;
        const baseX = start.x + ux * distanceAlong;
        const baseY = start.y + uy * distanceAlong;
        const normalDistance = outwardOffset + sample.n * radius;
        generated.push({
          ...start,
          x: roundCloudCoordinate(baseX + normal.x * normalDistance),
          y: roundCloudCoordinate(baseY + normal.y * normalDistance),
        });
      }
    }
  }

  if (closed && generated.length > 0) {
    generated.push(generated[0]);
  }
  return generated;
}

export function generateCloudScallopPath(
  controlPath: readonly PdfPoint[],
  closed: boolean,
  options: CloudLineTypeOptions = DEFAULT_CLOUD_LINE_OPTIONS,
): string {
  if (controlPath.length < 2) {
    return pointsToPath(controlPath, closed);
  }

  const orderedControlPath = closed ? normalizeClosedCloudControlPath(controlPath) : controlPath;
  const path = closed ? [...orderedControlPath, orderedControlPath[0]] : [...orderedControlPath];
  const orientation = closed ? polygonSignedArea(orderedControlPath) : 1;
  const spacing = Math.max(3, options.scallopSpacing);
  const radius = Math.max(1, options.scallopRadius + options.offset);
  const outwardOffset = options.offset > 0 ? options.offset : radius * 0.3316;
  const commands: string[] = [];

  for (let index = 0; index < path.length - 1; index += 1) {
    const start = path[index];
    const end = path[index + 1];
    const dx = end.x - start.x;
    const dy = end.y - start.y;
    const length = Math.hypot(dx, dy);
    if (length === 0) {
      continue;
    }
    const ux = dx / length;
    const uy = dy / length;
    const normal = outwardNormal(ux, uy, orientation);
    const segmentCount = Math.max(1, Math.round(length / spacing));
    for (let segmentIndex = 0; segmentIndex < segmentCount; segmentIndex += 1) {
      const pointAt = (sample: { t: number; n: number }) => {
        const lobeLength = spacing;
        const lobeStart = (segmentIndex + 0.5) * spacing;
        const distanceAlong = lobeStart + sample.t * lobeLength;
        const baseX = start.x + ux * distanceAlong;
        const baseY = start.y + uy * distanceAlong;
        const normalDistance = outwardOffset + sample.n * radius;
        return {
          x: roundCloudCoordinate(baseX + normal.x * normalDistance),
          y: roundCloudCoordinate(baseY + normal.y * normalDistance),
        };
      };

      if (commands.length === 0) {
        const first = pointAt(BLUEBEAM_CLOUD_LOBE_POINTS[0]);
        commands.push(`M ${first.x} ${first.y}`);
      }

      const firstCurve = BLUEBEAM_CLOUD_LOBE_POINTS.slice(1, 4).map(pointAt);
      const secondCurve = BLUEBEAM_CLOUD_LOBE_POINTS.slice(4, 7).map(pointAt);
      const thirdCurve = BLUEBEAM_CLOUD_LOBE_POINTS.slice(7, 10).map(pointAt);
      commands.push(cubicCommand(firstCurve));
      commands.push(cubicCommand(secondCurve));
      commands.push(cubicCommand(thirdCurve));
    }
  }

  return commands.join(' ');
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

const BLUEBEAM_CLOUD_LOBE_POINTS = [
  // Fitted from Bluebeam `/PolygonCloud` AP streams in
  // `docs/pdf-tools/bluebeam-defaults-research.md`.
  // Each lobe is drawn as three cubic segments; `t` is distance along the
  // control edge, and `n` is outward normal distance in radius units.
  { t: 0, n: 0 },
  { t: 0.1832, n: 0.2761 },
  { t: 0.5555, n: 0.3515 },
  { t: 0.8317, n: 0.1683 },
  { t: 0.9435, n: 0.0942 },
  { t: 1.0574, n: -0.0722 },
  { t: 1.0854, n: -0.2033 },
  { t: 1.0721, n: -0.1427 },
  { t: 1.0335, n: -0.0517 },
  { t: 1, n: 0 },
] as const;

const BLUEBEAM_CLOUD_LOBE_SAMPLES = BLUEBEAM_CLOUD_LOBE_POINTS;

function cubicCommand(points: readonly { x: number; y: number }[]): string {
  const [first, second, third] = points;
  return `C ${first.x} ${first.y} ${second.x} ${second.y} ${third.x} ${third.y}`;
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
