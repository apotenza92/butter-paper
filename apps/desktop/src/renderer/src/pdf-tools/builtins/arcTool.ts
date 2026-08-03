import { createArcMarkup, pdfPoint, rect as createRect, type ArcMarkup, type PdfPoint, type Rect } from '@butter-paper/core';
import type { ArcDraft } from '../annotationLifecycle';
import { getAnnotationContentStyle } from '../annotationStyles';
import { isPointNearLineSegment } from '../hitTesting';
import { getMoveCursor, getResizeCursor } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHandleDescriptor, ToolHit } from '../types';

const ARC_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const DEFAULT_ARC_ANGLE1 = 90;
export const DEFAULT_ARC_ANGLE2 = 180;

export const ARC_TOOL_DEFINITION: PdfToolDefinition<ArcMarkup, ArcDraft> & { readonly id: 'arc' } = {
  id: 'arc',
  label: 'Arc',
  shortcut: 'Shift+C',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-arc',
  implemented: true,
  properties: ARC_PROPERTIES,
  defaults: {
    strokeColor: '#ff0000',
    strokeWidthPt: 1,
    opacity: 1,
  },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      const points = arcControlPoints(markup);
      return {
        bounds: markup.rect,
        components: [
          {
            id: 'arc.body',
            role: 'shape',
            geometry: { kind: 'polyline', points: sampleArcPoints(markup, 24) },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: arcHandles(points),
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      const points = sampleArcPoints(markup, 24);
      for (let index = 1; index < points.length; index += 1) {
        if (isPointNearLineSegment(point, points[index - 1], points[index], context.tolerance)) {
          return {
            markupId: markup.id,
            componentId: 'arc.body',
            region: 'edge',
            bodyDrag: 'moveSelf',
            cursor: getMoveCursor(),
          };
        }
      }
      return null;
    },
  },
  render: {
    getContentPrimitives(markup) {
      const style = getAnnotationContentStyle(markup);
      return [
        {
          kind: 'path',
          d: arcSvgPath(markup),
          style: {
            stroke: style.stroke,
            fill: 'none',
            strokeWidth: style.strokeWidth,
            opacity: style.opacity,
          },
          pointerEvents: 'visibleStroke',
        },
      ];
    },
    getDraftPrimitives(draft) {
      const points = draft.phase === 'end'
        ? [draft.start, draft.current]
        : [draft.start, draft.current, draft.end ?? draft.current];
      if (draft.phase === 'end') {
        return [
          {
            kind: 'polyline',
            points,
            style: { stroke: '#ff0000', fill: 'none', strokeWidth: 1, dashArray: '4 3' },
            pointerEvents: 'none',
          },
        ];
      }
      const markup = createArcMarkupFromThreePoints('draft-arc', 0, draft.start, draft.end ?? draft.current, draft.current);
      return markup
        ? [{
            kind: 'path',
            d: arcSvgPath(markup),
            style: { stroke: '#ff0000', fill: 'none', strokeWidth: 1 },
            pointerEvents: 'none',
          }]
        : [{
            kind: 'polyline',
            points,
            style: { stroke: '#ff0000', fill: 'none', strokeWidth: 1, dashArray: '4 3' },
            pointerEvents: 'none',
          }];
    },
  },
  selection: {
    getSelectionChrome(markup): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: markup.rect,
          kind: 'child',
        },
        handles: arcHandles(arcControlPoints(markup)),
        controlPaths: [{ id: 'arc.path', points: sampleArcPoints(markup, 24), closed: false }],
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      const rect = draftBounds(draft);
      return {
        bounds: rect ? { rect, kind: 'child' } : undefined,
        handles: [],
      };
    },
  },
  interaction: {
    transformMarkup(markup, input) {
      if (input.handleBehavior !== 'reshapeArc') {
        return markup;
      }
      const controls = arcControlPoints(markup);
      if (input.handleId === 'arc.point.start') {
        return createArcMarkupFromThreePoints(markup.id, markup.pageIndex, input.currentPoint, controls.end, controls.mid) ?? markup;
      }
      if (input.handleId === 'arc.point.end') {
        return createArcMarkupFromThreePoints(markup.id, markup.pageIndex, controls.start, input.currentPoint, controls.mid) ?? markup;
      }
      if (input.handleId === 'arc.point.mid') {
        const snapped = snapArcMidpoint(controls.start, controls.end, input.currentPoint);
        return createArcMarkupFromThreePoints(markup.id, markup.pageIndex, controls.start, controls.end, snapped) ?? markup;
      }
      return markup;
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      const intent = String(annotation.intent ?? annotation.fields?.IT ?? '').toLowerCase();
      return subtype === 'circle' && intent === 'circlearc';
    },
    import(annotation, context) {
      const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = annotation.rect ?? [];
      const rect = createRect(Math.min(x1, x2), Math.min(y1, y2), Math.abs(x2 - x1), Math.abs(y2 - y1));
      const angle1 = numberField(annotation.fields?.Angle1, DEFAULT_ARC_ANGLE1);
      const angle2 = numberField(annotation.fields?.Angle2, DEFAULT_ARC_ANGLE2);
      const base = createArcMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        rect,
        angle1,
        angle2,
        source: { source: 'imported' },
      });
      const controls = arcControlPoints(base);
      return createArcMarkup({ ...base, ...controls });
    },
  },
};

export function createArcMarkupFromThreePoints(id: string, pageIndex: number, start: PdfPoint, end: PdfPoint, mid: PdfPoint): ArcMarkup | null {
  const circle = circleFromThreePoints(start, mid, end);
  if (!circle || circle.radius < 1) {
    return null;
  }
  const rect = createRect(circle.center.x - circle.radius, circle.center.y - circle.radius, circle.radius * 2, circle.radius * 2);
  const angle1 = angleFromCenter(circle.center, start);
  const through = angleFromCenter(circle.center, mid);
  const rawEnd = angleFromCenter(circle.center, end);
  const angle2 = endAngleThrough(angle1, rawEnd, through);
  return createArcMarkup({
    id,
    pageIndex,
    rect,
    angle1,
    angle2,
    start,
    end,
    mid,
    source: { source: 'butter' },
  });
}

export function arcSvgPath(markup: ArcMarkup): string {
  const controls = arcControlPoints(markup);
  const delta = normalizedArcDelta(markup.angle1, markup.angle2);
  const largeArcFlag = Math.abs(delta) > 180 ? 1 : 0;
  const sweepFlag = delta >= 0 ? 0 : 1;
  const radius = markup.rect.width * 0.5;
  return `M ${controls.start.x} ${controls.start.y} A ${radius} ${radius} 0 ${largeArcFlag} ${sweepFlag} ${controls.end.x} ${controls.end.y}`;
}

export function arcControlPoints(markup: ArcMarkup): { start: PdfPoint; end: PdfPoint; mid: PdfPoint } {
  const start = markup.start ?? arcPoint(markup.rect, markup.angle1);
  const end = markup.end ?? arcPoint(markup.rect, markup.angle2);
  const midAngle = markup.angle1 + normalizedArcDelta(markup.angle1, markup.angle2) * 0.5;
  const mid = markup.mid ?? arcPoint(markup.rect, midAngle);
  return { start, end, mid };
}

function arcHandles(points: { start: PdfPoint; end: PdfPoint; mid: PdfPoint }): readonly ToolHandleDescriptor[] {
  return [
    { id: 'arc.point.start', componentId: 'arc.body', point: points.start, behavior: 'reshapeArc', cursor: getResizeCursor('nw') },
    { id: 'arc.point.mid', componentId: 'arc.body', point: points.mid, behavior: 'reshapeArc', cursor: getResizeCursor('n') },
    { id: 'arc.point.end', componentId: 'arc.body', point: points.end, behavior: 'reshapeArc', cursor: getResizeCursor('ne') },
  ];
}

function sampleArcPoints(markup: ArcMarkup, segments: number): readonly PdfPoint[] {
  const delta = normalizedArcDelta(markup.angle1, markup.angle2);
  return Array.from({ length: segments + 1 }, (_, index) => arcPoint(markup.rect, markup.angle1 + (delta * index) / segments));
}

function arcPoint(rect: Rect, angleDegrees: number): PdfPoint {
  const radius = rect.width * 0.5;
  const radians = (angleDegrees * Math.PI) / 180;
  return pdfPoint(
    rect.x + radius + Math.cos(radians) * radius,
    rect.y + radius + Math.sin(radians) * radius,
  );
}

function circleFromThreePoints(a: PdfPoint, b: PdfPoint, c: PdfPoint): { center: PdfPoint; radius: number } | null {
  const d = 2 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
  if (Math.abs(d) < 0.001) {
    return null;
  }
  const aSq = a.x * a.x + a.y * a.y;
  const bSq = b.x * b.x + b.y * b.y;
  const cSq = c.x * c.x + c.y * c.y;
  const center = pdfPoint(
    (aSq * (b.y - c.y) + bSq * (c.y - a.y) + cSq * (a.y - b.y)) / d,
    (aSq * (c.x - b.x) + bSq * (a.x - c.x) + cSq * (b.x - a.x)) / d,
  );
  return { center, radius: Math.hypot(a.x - center.x, a.y - center.y) };
}

function angleFromCenter(center: PdfPoint, point: PdfPoint): number {
  return normalizeAngle((Math.atan2(point.y - center.y, point.x - center.x) * 180) / Math.PI);
}

function endAngleThrough(start: number, end: number, through: number): number {
  const ccwEnd = start + positiveDelta(start, end);
  const ccwThrough = start + positiveDelta(start, through);
  if (ccwThrough <= ccwEnd) {
    return ccwEnd;
  }
  return start - positiveDelta(end, start);
}

function snapArcMidpoint(start: PdfPoint, end: PdfPoint, point: PdfPoint): PdfPoint {
  const chordMid = pdfPoint((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
  const chord = pdfPoint(end.x - start.x, end.y - start.y);
  const length = Math.hypot(chord.x, chord.y);
  if (length < 1) {
    return point;
  }
  const normal = pdfPoint(-chord.y / length, chord.x / length);
  const projection = (point.x - chordMid.x) * normal.x + (point.y - chordMid.y) * normal.y;
  return pdfPoint(chordMid.x + normal.x * projection, chordMid.y + normal.y * projection);
}

function draftBounds(draft: ArcDraft): Rect | null {
  const xs = [draft.start.x, draft.current.x, draft.end?.x].filter((value): value is number => typeof value === 'number');
  const ys = [draft.start.y, draft.current.y, draft.end?.y].filter((value): value is number => typeof value === 'number');
  if (xs.length === 0 || ys.length === 0) {
    return null;
  }
  const minX = Math.min(...xs);
  const minY = Math.min(...ys);
  return createRect(minX, minY, Math.max(...xs) - minX, Math.max(...ys) - minY);
}

function normalizedArcDelta(angle1: number, angle2: number): number {
  let delta = angle2 - angle1;
  while (delta <= -360) delta += 360;
  while (delta > 360) delta -= 360;
  return delta;
}

function positiveDelta(from: number, to: number): number {
  return (normalizeAngle(to) - normalizeAngle(from) + 360) % 360;
}

function normalizeAngle(angle: number): number {
  return ((angle % 360) + 360) % 360;
}

function numberField(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}
