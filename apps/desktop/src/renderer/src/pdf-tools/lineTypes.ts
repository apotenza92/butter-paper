import { pdfPoint, type PdfPoint } from '@butter-paper/core';

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

export function sampleAbsoluteSvgPath(path: string, cubicSamples = CLOUD_CURVE_HIT_TEST_SAMPLES): readonly PdfPoint[] {
  const tokens = path.match(/[MLCZ]|-?\d*\.?\d+(?:[eE][+-]?\d+)?/g) ?? [];
  const points: PdfPoint[] = [];
  let current: PdfPoint | undefined;
  let first: PdfPoint | undefined;
  let index = 0;
  while (index < tokens.length) {
    const command = tokens[index++];
    if (command === 'M' || command === 'L') {
      const x = Number(tokens[index++]);
      const y = Number(tokens[index++]);
      if (!Number.isFinite(x) || !Number.isFinite(y)) return [];
      const point = pdfPoint(x, y);
      current = point;
      first ??= point;
      points.push(point);
      continue;
    }
    if (command === 'C' && current) {
      const values = tokens.slice(index, index + 6).map(Number);
      index += 6;
      if (values.length !== 6 || values.some((value) => !Number.isFinite(value))) return [];
      const segment: CloudCubicSegment = {
        start: current,
        control1: pdfPoint(values[0], values[1]),
        control2: pdfPoint(values[2], values[3]),
        end: pdfPoint(values[4], values[5]),
      };
      const samples = Math.max(1, Math.floor(cubicSamples));
      for (let sample = 1; sample <= samples; sample += 1) {
        points.push(cubicPoint(segment, sample / samples));
      }
      current = segment.end;
      continue;
    }
    if (command === 'Z' && first) {
      if (!current || current.x !== first.x || current.y !== first.y) points.push(first);
      current = first;
      continue;
    }
    return [];
  }
  return points;
}

interface CloudCubicSegment {
  readonly start: PdfPoint;
  readonly control1: PdfPoint;
  readonly control2: PdfPoint;
  readonly end: PdfPoint;
}

const CLOUD_CURVE_HIT_TEST_SAMPLES = 32;
const CLOUD_PERIMETER_DISTANCE_EPSILON = 0.000001;
// Bluebeam's native cloud appearance does not draw half-ellipses directly on
// the control polygon. Its scallops ride on a shallow outward baseline and
// briefly curl back toward the control polygon between lobes.
const CLOUD_BASELINE_OFFSET_RATIO = 0.3309;
const CLOUD_REFERENCE_RADIUS = 14.28;
// Revu distributes whole scallops around the perimeter, then scales their
// height by the resulting lobe length. This reference is the measured lobe
// length of the canonical Revu cloud fixture. Keeping the profile relative to
// it preserves that fixture while avoiding taller/shorter lobes when rounding
// produces a slightly different perimeter phase.
const CLOUD_REFERENCE_LOBE_LENGTH = 14.24667;
const CLOUD_EXACT_CORNER_JOINT_RATIO = 0.27976;

function generateCloudCubicSegments(
  controlPath: readonly PdfPoint[],
  closed: boolean,
  options: CloudLineTypeOptions,
): readonly CloudCubicSegment[] {
  const orderedControlPath = closed
    ? removeRepeatedClosingPoint(normalizeClosedCloudControlPath(controlPath))
    : controlPath;
  const spacing = Math.max(3, options.scallopSpacing);
  const radius = Math.max(1, options.scallopRadius);
  const baselineOffset = Math.max(0, radius * CLOUD_BASELINE_OFFSET_RATIO + options.offset);

  if (closed && orderedControlPath.length >= 3) {
    if (orderedControlPath.every((point, index) => {
        const next = orderedControlPath[(index + 1) % orderedControlPath.length];
        return Math.hypot(next.x - point.x, next.y - point.y) >= spacing * 1.25;
      })) {
      return generatePerimeterCloudCubicSegments(orderedControlPath, spacing, radius, baselineOffset);
    }
    return generateClosedCloudCubicSegments(orderedControlPath, spacing, radius, baselineOffset);
  }

  const path = closed ? [...orderedControlPath, orderedControlPath[0]] : [...orderedControlPath];
  const segments: CloudCubicSegment[] = [];

  for (let edgeIndex = 0; edgeIndex < path.length - 1; edgeIndex += 1) {
    const edgeStart = path[edgeIndex];
    const edgeEnd = path[edgeIndex + 1];
    const edgeVector = unitVector(edgeStart, edgeEnd);
    if (edgeVector) {
      const normal = outwardNormal(edgeVector.x, edgeVector.y, 1);
      appendEdgeCloudLobes(
        segments,
        offsetPoint(edgeStart, normal, baselineOffset),
        offsetPoint(edgeEnd, normal, baselineOffset),
        normal,
        spacing,
        radius,
      );
    }
  }

  return segments;
}

interface CloudCorner {
  readonly start: PdfPoint;
  readonly end: PdfPoint;
}

function generatePerimeterCloudCubicSegments(
  controlPath: readonly PdfPoint[],
  spacing: number,
  radius: number,
  baselineOffset: number,
): readonly CloudCubicSegment[] {
  const orientation = polygonSignedArea(controlPath);
  const edgeLengths = controlPath.map((point, index) => {
    const next = controlPath[(index + 1) % controlPath.length];
    return Math.hypot(next.x - point.x, next.y - point.y);
  });
  const perimeter = edgeLengths.reduce((sum, length) => sum + length, 0);
  const lobeCount = Math.max(4, Math.round(perimeter / spacing));
  const lobeLength = perimeter / lobeCount;
  const perimeterBaselineOffset = baselineOffset * lobeLength / CLOUD_REFERENCE_LOBE_LENGTH;
  const firstLobeStart = lobeLength * 0.5;
  const segments: CloudCubicSegment[] = [];

  for (let lobeIndex = 0; lobeIndex < lobeCount; lobeIndex += 1) {
    const startDistance = firstLobeStart + lobeIndex * lobeLength;
    const endDistance = startDistance + lobeLength;
    const start = pointOnOffsetPerimeter(controlPath, edgeLengths, perimeter, startDistance, orientation, perimeterBaselineOffset);
    const end = pointOnOffsetPerimeter(controlPath, edgeLengths, perimeter, endDistance, orientation, perimeterBaselineOffset);
    if (!start || !end) {
      continue;
    }
    const endApproach = pointOnOffsetPerimeter(
      controlPath,
      edgeLengths,
      perimeter,
      endDistance - Math.min(0.001, lobeLength * 0.001),
      orientation,
      perimeterBaselineOffset,
    );
    // A phase boundary at a vertex needs its own diagonal corner bump. Joining
    // two independently offset edge lobes here leaves a kink (and can even
    // disconnect the outline); Revu instead uses two symmetric lobes sharing a
    // shallow cusp on the vertex bisector.
    if (
      lobeIndex < lobeCount - 1
      && endApproach?.edgeIndex === start.edgeIndex
      && end.edgeIndex !== start.edgeIndex
    ) {
      const nextEnd = pointOnOffsetPerimeter(
        controlPath,
        edgeLengths,
        perimeter,
        endDistance + lobeLength,
        orientation,
        perimeterBaselineOffset,
      );
      if (nextEnd) {
        const vertex = controlPath[end.edgeIndex];
        const joint = roundedPoint(
          vertex.x + (start.normal.x + end.normal.x) * perimeterBaselineOffset * CLOUD_EXACT_CORNER_JOINT_RATIO,
          vertex.y + (start.normal.y + end.normal.y) * perimeterBaselineOffset * CLOUD_EXACT_CORNER_JOINT_RATIO,
          vertex,
        );
        appendExactCornerLobe(segments, start.point, joint, orientation, radius, lobeLength);
        appendExactCornerLobe(segments, joint, nextEnd.point, orientation, radius, lobeLength);
        lobeIndex += 1;
        continue;
      }
    }
    if (endApproach?.edgeIndex === start.edgeIndex) {
      appendBluebeamEdgeLobe(segments, start.point, end.point, start.normal, radius, lobeLength);
    } else {
      const incomingFraction = (start.edgeLength - start.distanceOnEdge) / lobeLength;
      const lateCornerAmount = Math.max(0, Math.min(1, (incomingFraction - 0.5) / (0.76726 - 0.5)));
      if (lobeIndex < lobeCount - 1 && lateCornerAmount > CLOUD_PERIMETER_DISTANCE_EPSILON) {
        const adjustedEnd = roundedPoint(
          end.point.x + (-end.normal.x * 0.52593 + start.normal.x * 0.32216)
            * lobeLength / CLOUD_REFERENCE_LOBE_LENGTH * lateCornerAmount,
          end.point.y + (-end.normal.y * 0.52593 + start.normal.y * 0.32216)
            * lobeLength / CLOUD_REFERENCE_LOBE_LENGTH * lateCornerAmount,
          end.point,
        );
        const nextEnd = pointOnOffsetPerimeter(
          controlPath,
          edgeLengths,
          perimeter,
          endDistance + lobeLength,
          orientation,
          perimeterBaselineOffset,
        );
        if (nextEnd) {
          appendPerimeterCloudCornerLobe(
            segments,
            start.point,
            adjustedEnd,
            orientation,
            radius,
            lobeLength,
            incomingFraction,
          );
          appendLateCornerTransitionLobe(
            segments,
            adjustedEnd,
            nextEnd.point,
            orientation,
            radius,
            lobeLength,
          );
          lobeIndex += 1;
          continue;
        }
      }
      appendPerimeterCloudCornerLobe(
        segments,
        start.point,
        end.point,
        orientation,
        radius,
        lobeLength,
        incomingFraction,
      );
    }
  }

  return segments;
}

interface OffsetPerimeterPoint {
  readonly point: PdfPoint;
  readonly edgeIndex: number;
  readonly distanceOnEdge: number;
  readonly edgeLength: number;
  readonly normal: { readonly x: number; readonly y: number };
}

function pointOnOffsetPerimeter(
  controlPath: readonly PdfPoint[],
  edgeLengths: readonly number[],
  perimeter: number,
  rawDistance: number,
  orientation: number,
  baselineOffset: number,
): OffsetPerimeterPoint | null {
  let distance = ((rawDistance % perimeter) + perimeter) % perimeter;
  for (let edgeIndex = 0; edgeIndex < controlPath.length; edgeIndex += 1) {
    const edgeLength = edgeLengths[edgeIndex];
    if (distance < edgeLength - CLOUD_PERIMETER_DISTANCE_EPSILON || edgeIndex === controlPath.length - 1) {
      const start = controlPath[edgeIndex];
      const end = controlPath[(edgeIndex + 1) % controlPath.length];
      const vector = unitVector(start, end);
      if (!vector) {
        return null;
      }
      const normal = outwardNormal(vector.x, vector.y, orientation);
      return {
        point: offsetPoint(edgePoint(start, vector.x, vector.y, distance), normal, baselineOffset),
        edgeIndex,
        distanceOnEdge: distance,
        edgeLength,
        normal,
      };
    }
    distance -= edgeLength;
  }
  return null;
}

function generateClosedCloudCubicSegments(
  controlPath: readonly PdfPoint[],
  spacing: number,
  radius: number,
  baselineOffset: number,
): readonly CloudCubicSegment[] {
  const orientation = polygonSignedArea(controlPath);
  const corners = controlPath.map((vertex, index) => {
    const previous = controlPath[(index - 1 + controlPath.length) % controlPath.length];
    const next = controlPath[(index + 1) % controlPath.length];
    return cloudCorner(previous, vertex, next, orientation, spacing, baselineOffset);
  });
  const segments: CloudCubicSegment[] = [];

  for (let edgeIndex = 0; edgeIndex < controlPath.length; edgeIndex += 1) {
    const nextIndex = (edgeIndex + 1) % controlPath.length;
    const edgeStart = corners[edgeIndex].end;
    const edgeEnd = corners[nextIndex].start;
    const edgeVector = unitVector(edgeStart, edgeEnd);
    if (edgeVector) {
      appendEdgeCloudLobes(
        segments,
        edgeStart,
        edgeEnd,
        outwardNormal(edgeVector.x, edgeVector.y, orientation),
        spacing,
        radius,
      );
    }
    appendCloudCornerLobe(segments, corners[nextIndex].start, corners[nextIndex].end, orientation, radius);
  }

  return segments;
}

function cloudCorner(
  previous: PdfPoint,
  vertex: PdfPoint,
  next: PdfPoint,
  orientation: number,
  spacing: number,
  baselineOffset: number,
): CloudCorner {
  const incoming = unitVector(previous, vertex) ?? { x: 1, y: 0, length: 0 };
  const outgoing = unitVector(vertex, next) ?? { x: 1, y: 0, length: 0 };
  const incomingNormal = outwardNormal(incoming.x, incoming.y, orientation);
  const outgoingNormal = outwardNormal(outgoing.x, outgoing.y, orientation);
  const inset = Math.min(Math.max(baselineOffset, spacing * 0.25), incoming.length * 0.25, outgoing.length * 0.25);
  const start = offsetPoint(
    roundedPoint(vertex.x - incoming.x * inset, vertex.y - incoming.y * inset, vertex),
    incomingNormal,
    baselineOffset,
  );
  const end = offsetPoint(
    roundedPoint(vertex.x + outgoing.x * inset, vertex.y + outgoing.y * inset, vertex),
    outgoingNormal,
    baselineOffset,
  );

  return { start, end };
}

function appendEdgeCloudLobes(
  segments: CloudCubicSegment[],
  edgeStart: PdfPoint,
  edgeEnd: PdfPoint,
  normal: { readonly x: number; readonly y: number },
  spacing: number,
  radius: number,
): void {
  const edgeVector = unitVector(edgeStart, edgeEnd);
  if (!edgeVector) {
    return;
  }

  const lobeCount = Math.max(1, Math.round(edgeVector.length / spacing));
  const lobeLength = edgeVector.length / lobeCount;

  for (let lobeIndex = 0; lobeIndex < lobeCount; lobeIndex += 1) {
    const startDistance = lobeIndex * lobeLength;
    const endDistance = (lobeIndex + 1) * lobeLength;
    const start = lobeIndex === 0 ? edgeStart : edgePoint(edgeStart, edgeVector.x, edgeVector.y, startDistance);
    const end = lobeIndex === lobeCount - 1 ? edgeEnd : edgePoint(edgeStart, edgeVector.x, edgeVector.y, endDistance);
    appendBluebeamEdgeLobe(segments, start, end, normal, radius, lobeLength);
  }
}

function appendBluebeamEdgeLobe(
  segments: CloudCubicSegment[],
  start: PdfPoint,
  end: PdfPoint,
  normal: { readonly x: number; readonly y: number },
  radius: number,
  lobeLength: number,
): void {
  const chord = unitVector(start, end);
  if (!chord) {
    return;
  }
  const normalScale = lobeLength / CLOUD_REFERENCE_LOBE_LENGTH;
  const point = (tangentRatio: number, normalRatio: number): PdfPoint => roundedPoint(
    start.x + chord.x * chord.length * tangentRatio + normal.x * radius * normalRatio * normalScale,
    start.y + chord.y * chord.length * tangentRatio + normal.y * radius * normalRatio * normalScale,
    start,
  );
  const firstEnd = point(0.8317, 0.1679);
  const secondEnd = point(1.0861, -0.2028);

  segments.push({ start, control1: point(0.1832, 0.2755), control2: point(0.5555, 0.3507), end: firstEnd });
  segments.push({ start: firstEnd, control1: point(0.9435, 0.0939), control2: point(1.0574, -0.0720), end: secondEnd });
  segments.push({ start: secondEnd, control1: point(1.0729, -0.1424), control2: point(1.0343, -0.0516), end });
}

const EXACT_CORNER_LOBE_PROFILE = [
  [[0.107253, 4.40065], [0.418061, 6.58248], [0.694205, 4.87317]],
  [[0.883093, 3.70399], [1.036292, 0.13844], [1.036389, -3.09080]],
  [[1.036369, -2.20737], [1.020071, -0.82356], [1, 0]],
] as const;

function appendExactCornerLobe(
  segments: CloudCubicSegment[],
  start: PdfPoint,
  end: PdfPoint,
  orientation: number,
  radius: number,
  lobeLength: number,
): void {
  const chord = unitVector(start, end);
  if (!chord) {
    return;
  }
  const normal = outwardNormal(chord.x, chord.y, orientation);
  const normalScale = radius / CLOUD_REFERENCE_RADIUS * lobeLength / CLOUD_REFERENCE_LOBE_LENGTH;
  const point = ([tangentRatio, normalDistance]: readonly [number, number]): PdfPoint => roundedPoint(
    start.x + chord.x * chord.length * tangentRatio + normal.x * normalDistance * normalScale,
    start.y + chord.y * chord.length * tangentRatio + normal.y * normalDistance * normalScale,
    start,
  );
  let segmentStart = start;
  for (const [control1, control2, segmentEnd] of EXACT_CORNER_LOBE_PROFILE) {
    const resolvedEnd = segmentEnd[0] === 1 && segmentEnd[1] === 0 ? end : point(segmentEnd);
    segments.push({
      start: segmentStart,
      control1: point(control1),
      control2: point(control2),
      end: resolvedEnd,
    });
    segmentStart = resolvedEnd;
  }
}

const LATE_CORNER_TRANSITION_PROFILE = [
  [[0.169146, 4.02569], [0.530132, 5.29018], [0.806274, 2.82421]],
  [[0.929862, 1.72057], [1.051031, -0.85928], [1.076916, -2.93806]],
  [[1.066093, -2.06879], [1.031659, -0.75340], [1, 0]],
] as const;

function appendLateCornerTransitionLobe(
  segments: CloudCubicSegment[],
  start: PdfPoint,
  end: PdfPoint,
  orientation: number,
  radius: number,
  lobeLength: number,
): void {
  const chord = unitVector(start, end);
  if (!chord) {
    return;
  }
  const normal = outwardNormal(chord.x, chord.y, orientation);
  const normalScale = radius / CLOUD_REFERENCE_RADIUS * lobeLength / CLOUD_REFERENCE_LOBE_LENGTH;
  const point = ([tangentRatio, normalDistance]: readonly [number, number]): PdfPoint => roundedPoint(
    start.x + chord.x * chord.length * tangentRatio + normal.x * normalDistance * normalScale,
    start.y + chord.y * chord.length * tangentRatio + normal.y * normalDistance * normalScale,
    start,
  );
  let segmentStart = start;
  for (const [control1, control2, segmentEnd] of LATE_CORNER_TRANSITION_PROFILE) {
    const resolvedEnd = segmentEnd[0] === 1 && segmentEnd[1] === 0 ? end : point(segmentEnd);
    segments.push({
      start: segmentStart,
      control1: point(control1),
      control2: point(control2),
      end: resolvedEnd,
    });
    segmentStart = resolvedEnd;
  }
}

function appendCloudCornerLobe(
  segments: CloudCubicSegment[],
  start: PdfPoint,
  end: PdfPoint,
  orientation: number,
  radius: number,
): void {
  const chord = unitVector(start, end);
  if (!chord) {
    return;
  }
  const normal = outwardNormal(chord.x, chord.y, orientation);
  const normalScale = radius / CLOUD_REFERENCE_RADIUS;
  const point = (tangentRatio: number, normalDistance: number): PdfPoint => roundedPoint(
    start.x + chord.x * chord.length * tangentRatio + normal.x * normalDistance * normalScale,
    start.y + chord.y * chord.length * tangentRatio + normal.y * normalDistance * normalScale,
    start,
  );
  const firstEnd = point(0.5301, 8.018);
  const secondEnd = point(1, -1.029);
  const thirdEnd = point(0.9771, -3.120);

  segments.push({ start, control1: point(0.0166, 4.712), control2: point(0.2540, 8.302), end: firstEnd });
  segments.push({ start: firstEnd, control1: point(0.8063, 7.734), control2: point(1.0166, 3.684), end: secondEnd });
  segments.push({ start: secondEnd, control1: point(0.9979, -1.620), control2: point(0.9876, -2.556), end: thirdEnd });
  segments.push({ start: thirdEnd, control1: point(0.9929, -2.279), control2: point(1.0031, -0.882), end });
}

const ASYMMETRIC_CORNER_PROFILE = [
  [[0.01664, 4.7124], [0.25399, 8.3023], [0.53014, 8.0183]],
  [[0.80628, 7.7342], [1.01664, 3.6838], [1, -1.0285]],
  [[0.99791, -1.6198], [0.98764, -2.5562], [0.97706, -3.1203]],
  [[0.99285, -2.2788], [1.00311, -0.8818], [1, 0]],
] as const;
// The first canonical rectangle crosses a corner with 26.726% of the lobe on
// the incoming edge. Revu uses a distinct, shallower diagonal profile here;
// clamping it to the later 27.945% sample is the visible corner kink that used
// to remain on Cloud+ rectangles.
const EARLY_CORNER_PROFILE = [
  [[0.02054, 4.70792], [0.261048, 8.24054], [0.53719, 7.89036]],
  [[0.813333, 7.54019], [1.020538, 3.43978], [1.000001, -1.26805]],
  [[0.997714, -1.79166], [0.988588, -2.62325], [0.979609, -3.12549]],
  [[0.994715, -2.28038], [1.003841, -0.88105], [1, 0]],
] as const;
const CENTERED_CORNER_PROFILE = [
  [[-0.0559, 4.627], [0.12265, 9.1374], [0.39879, 10.074]],
  [[0.67493, 11.0106], [0.9441, 8.0189], [1, 3.3918]],
  [[1.02263, 1.5183], [0.9911, -1.3053], [0.92956, -2.9148]],
  [[0.95801, -2.1709], [0.98954, -0.8659], [1, 0]],
] as const;
const LATE_CORNER_PROFILE = [
  [[0.03161, 4.69029], [0.28108, 8.05739], [0.55722, 7.52062]],
  [[0.83336, 6.98394], [1.0316, 2.74656], [1, -1.94384]],
  [[0.99775, -2.27764], [0.99186, -2.81182], [0.98686, -3.13678]],
  [[1.00003, -2.28212], [1.00591, -0.87774], [1, 0]],
] as const;

function appendPerimeterCloudCornerLobe(
  segments: CloudCubicSegment[],
  start: PdfPoint,
  end: PdfPoint,
  orientation: number,
  radius: number,
  lobeLength: number,
  incomingFraction: number,
): void {
  const chord = unitVector(start, end);
  if (!chord) {
    return;
  }
  const normal = outwardNormal(chord.x, chord.y, orientation);
  const normalScale = radius / CLOUD_REFERENCE_RADIUS * lobeLength / CLOUD_REFERENCE_LOBE_LENGTH;
  const profileStops = [
    { fraction: 0.26726, profile: EARLY_CORNER_PROFILE },
    { fraction: 0.27945, profile: ASYMMETRIC_CORNER_PROFILE },
    { fraction: 0.5, profile: CENTERED_CORNER_PROFILE },
    { fraction: 0.76726, profile: LATE_CORNER_PROFILE },
  ] as const;
  const upperIndex = profileStops.findIndex((stop) => incomingFraction <= stop.fraction);
  const toIndex = upperIndex < 0 ? profileStops.length - 1 : upperIndex;
  const fromIndex = Math.max(0, toIndex - 1);
  const fromStop = profileStops[fromIndex];
  const toStop = profileStops[toIndex];
  const fractionRange = toStop.fraction - fromStop.fraction;
  const interpolation = fractionRange === 0
    ? 0
    : Math.max(0, Math.min(1, (incomingFraction - fromStop.fraction) / fractionRange));
  const fromProfile = fromStop.profile;
  const toProfile = toStop.profile;
  const profile = fromProfile.map((segment, segmentIndex) => segment.map((coordinate, coordinateIndex) => {
    const target = toProfile[segmentIndex][coordinateIndex];
    return [
      coordinate[0] + (target[0] - coordinate[0]) * interpolation,
      coordinate[1] + (target[1] - coordinate[1]) * interpolation,
    ] as const;
  }));
  const point = ([tangentRatio, normalDistance]: readonly [number, number]): PdfPoint => roundedPoint(
    start.x + chord.x * chord.length * tangentRatio + normal.x * normalDistance * normalScale,
    start.y + chord.y * chord.length * tangentRatio + normal.y * normalDistance * normalScale,
    start,
  );
  let segmentStart = start;
  for (const [control1, control2, segmentEnd] of profile) {
    const resolvedEnd = segmentEnd[0] === 1 && segmentEnd[1] === 0 ? end : point(segmentEnd);
    segments.push({
      start: segmentStart,
      control1: point(control1),
      control2: point(control2),
      end: resolvedEnd,
    });
    segmentStart = resolvedEnd;
  }
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

function unitVector(start: PdfPoint, end: PdfPoint): { x: number; y: number; length: number } | null {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const length = Math.hypot(dx, dy);
  return length === 0 ? null : { x: dx / length, y: dy / length, length };
}

function removeRepeatedClosingPoint(points: readonly PdfPoint[]): readonly PdfPoint[] {
  if (points.length < 2) {
    return points;
  }
  const first = points[0];
  const last = points[points.length - 1];
  return first.x === last.x && first.y === last.y ? points.slice(0, -1) : points;
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
