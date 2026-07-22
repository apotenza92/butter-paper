import {
  createAreaMarkup,
  createLengthMarkup,
  createPolylengthMarkup,
  formatScaledAreaLabel,
  formatScaledLengthLabel,
  measureScaledLength,
  measureScaledPolygonArea,
  measureScaledPolyline,
  pdfPoint,
  rect,
  translatePoint,
  type AreaMarkup,
  type LengthMarkup,
  type PageScale,
  type PdfPoint,
  type PolylengthMarkup,
  type Rect,
} from '@butter-paper/core';
import { getAnnotationContentStyle, getAnnotationTextContentStyle } from '../annotationStyles';
import { isPointInPolygon, isPointInRect, isPointNearLineSegment, isPointNearPolygonEdge, isPointNearPolyline } from '../hitTesting';
import { getMoveCursor } from '../interactionChrome';
import type { MeasurementPathDraft } from '../annotationLifecycle';
import { measurementPathPreviewPoints } from '../annotationLifecycle';
import { measureAnnotationText } from '../textLayout';
import type { PdfToolDefinition, RenderPrimitive, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

type MeasurementMarkup = LengthMarkup | PolylengthMarkup | AreaMarkup;

const MEASUREMENT_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'color', key: 'textColor', label: 'Text', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'number', key: 'fontSizePt', label: 'Font size', default: 12, min: 6, max: 72, step: 1 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const LENGTH_TOOL_DEFINITION: PdfToolDefinition<LengthMarkup, never> & { readonly id: 'length' } = {
  id: 'length',
  label: 'Length',
  shortcut: 'Shift+Alt+L',
  category: 'measurement',
  cursor: 'crosshair',
  testId: 'tool-length',
  implemented: true,
  properties: MEASUREMENT_PROPERTIES,
  defaults: measurementDefaults(),
  geometry: {
    getGeometry(markup, context): ToolGeometryDescriptor {
      const label = measurementLabel(markup, context.pageScale);
      return {
        bounds: lineBounds(markup.start, markup.end),
        components: [
          {
            id: 'length.body',
            role: 'measurement',
            geometry: { kind: 'line', start: markup.start, end: markup.end },
            bodyDrag: 'moveSelf',
          },
          {
            id: 'length.label',
            role: 'textBox',
            geometry: { kind: 'textBox', rect: labelRect(lengthLabelPoint(markup), label, markup) },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: endpointHandles('length', markup.start, markup.end),
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      const label = measurementLabel(markup, context.pageScale);
      if (isPointNearLineSegment(point, markup.start, markup.end, context.tolerance) || isPointInRect(point, labelRect(lengthLabelPoint(markup), label, markup))) {
        return measurementHit(markup.id, 'length.body');
      }
      return null;
    },
  },
  render: {
    getContentPrimitives(markup, context) {
      return renderLengthMarkup(markup, context.pageScale);
    },
  },
  selection: {
    getSelectionChrome(markup): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: lineBounds(markup.start, markup.end),
          kind: 'child',
          canResize: false,
          canRotate: false,
        },
        handles: endpointHandles('length', markup.start, markup.end),
        controlPaths: [{ id: 'length.path', points: [markup.start, markup.end], closed: false }],
      };
    },
  },
  interaction: {
    transformMarkup(markup, input) {
      if (input.handleBehavior !== 'moveEndpoint') {
        return markup;
      }
      return createLengthMarkup({
        ...markup,
        start: input.handleId.endsWith('.start') ? input.currentPoint : markup.start,
        end: input.handleId.endsWith('.end') ? input.currentPoint : markup.end,
      });
    },
    dragMarkup(markup, input) {
      if (input.bodyDrag !== 'moveSelf') {
        return markup;
      }
      return createLengthMarkup({
        ...markup,
        start: translatePoint(markup.start, input.delta),
        end: translatePoint(markup.end, input.delta),
      });
    },
  },
  pdf: {
    canImport(annotation) {
      const subject = String(annotation.subject ?? annotation.fields?.Subj ?? '').toLowerCase();
      return String(annotation.subtype ?? '').toLowerCase() === 'line' && (subject === 'length' || subject === 'length measurement');
    },
    import(annotation, context) {
      const line = readLine(annotation.fields?.L, annotation.rect);
      return createLengthMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        start: line.start,
        end: line.end,
        color: '#ff0000',
        source: { source: 'imported' },
      });
    },
  },
};

export const POLYLENGTH_TOOL_DEFINITION: PdfToolDefinition<PolylengthMarkup, MeasurementPathDraft> & { readonly id: 'polylength' } = {
  id: 'polylength',
  label: 'Polylength',
  shortcut: 'Shift+Alt+Q',
  category: 'measurement',
  cursor: 'crosshair',
  testId: 'tool-polylength',
  implemented: true,
  properties: MEASUREMENT_PROPERTIES,
  defaults: measurementDefaults(),
  geometry: {
    getGeometry(markup, context): ToolGeometryDescriptor {
      return pathGeometry('polylength', markup, false, context.pageScale);
    },
    hitTest(markup, point, context): ToolHit | null {
      const label = measurementLabel(markup, context.pageScale);
      if (isPointNearPolyline(point, markup.points, context.tolerance) || isPointInRect(point, labelRect(pathLabelPoint(markup.points), label, markup))) {
        return measurementHit(markup.id, 'polylength.body');
      }
      return null;
    },
  },
  render: {
    getContentPrimitives(markup, context) {
      return renderPolylengthMarkup(markup, context.pageScale);
    },
    getDraftPrimitives(draft, context) {
      return renderPolylengthPreview(measurementPathPreviewPoints(draft), context.pageScale);
    },
  },
  selection: pathSelection('polylength', false),
  interaction: pathInteraction('polylength', createPolylengthMarkup),
  pdf: {
    canImport(annotation) {
      const subject = String(annotation.subject ?? annotation.fields?.Subj ?? '').toLowerCase();
      return String(annotation.subtype ?? '').toLowerCase() === 'polyline' && (subject === 'polylength' || subject === 'polylength measurement');
    },
    import(annotation, context) {
      return createPolylengthMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        points: readVertices(annotation.fields?.Vertices, annotation.rect, false),
        color: '#ff0000',
        source: { source: 'imported' },
      });
    },
  },
};

export const AREA_TOOL_DEFINITION: PdfToolDefinition<AreaMarkup, MeasurementPathDraft> & { readonly id: 'area' } = {
  id: 'area',
  label: 'Area',
  shortcut: 'Shift+Alt+A',
  category: 'measurement',
  cursor: 'crosshair',
  testId: 'tool-area',
  implemented: true,
  properties: MEASUREMENT_PROPERTIES,
  defaults: measurementDefaults(),
  geometry: {
    getGeometry(markup, context): ToolGeometryDescriptor {
      return pathGeometry('area', markup, true, context.pageScale);
    },
    hitTest(markup, point, context): ToolHit | null {
      const label = measurementLabel(markup, context.pageScale);
      if (isPointInPolygon(point, markup.points) || isPointNearPolygonEdge(point, markup.points, context.tolerance) || isPointInRect(point, labelRect(areaLabelPoint(markup.points), label, markup))) {
        return measurementHit(markup.id, 'area.body');
      }
      return null;
    },
  },
  render: {
    getContentPrimitives(markup, context) {
      return renderAreaMarkup(markup, context.pageScale);
    },
    getDraftPrimitives(draft, context) {
      return renderAreaPreview(measurementPathPreviewPoints(draft), context.pageScale);
    },
  },
  selection: pathSelection('area', true),
  interaction: pathInteraction('area', createAreaMarkup),
  pdf: {
    canImport(annotation) {
      const subject = String(annotation.subject ?? annotation.fields?.Subj ?? '').toLowerCase();
      return String(annotation.subtype ?? '').toLowerCase() === 'polygon' && (subject === 'area' || subject === 'area measurement');
    },
    import(annotation, context) {
      return createAreaMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        points: readVertices(annotation.fields?.Vertices, annotation.rect, true),
        color: '#ff0000',
        source: { source: 'imported' },
      });
    },
  },
};

export function createLengthMeasurementMarkup(id: string, pageIndex: number, start: PdfPoint, end: PdfPoint): LengthMarkup {
  return createLengthMarkup({
    id,
    pageIndex,
    start,
    end,
    color: '#ff0000',
    source: { source: 'butter' },
  });
}

export function createPolylengthMeasurementMarkup(id: string, pageIndex: number, points: readonly PdfPoint[]): PolylengthMarkup {
  return createPolylengthMarkup({
    id,
    pageIndex,
    points,
    color: '#ff0000',
    source: { source: 'butter' },
  });
}

export function createAreaMeasurementMarkup(id: string, pageIndex: number, points: readonly PdfPoint[]): AreaMarkup {
  return createAreaMarkup({
    id,
    pageIndex,
    points,
    color: '#ff0000',
    source: { source: 'butter' },
  });
}

export function measurementLabel(markup: MeasurementMarkup, pageScale: PageScale | undefined): string {
  if (!pageScale) {
    return 'Scale not set';
  }
  if (markup.kind === 'length') {
    return formatScaledLengthLabel(measureScaledLength(markup.start, markup.end, pageScale), pageScale, markup.displayUnit);
  }
  if (markup.kind === 'polylength') {
    return formatScaledLengthLabel(measureScaledPolyline(markup.points, pageScale), pageScale, markup.displayUnit);
  }
  return formatScaledAreaLabel(measureScaledPolygonArea(markup.points, pageScale), pageScale, markup.displayUnit);
}

function measurementDefaults(): Record<string, unknown> {
  return {
    strokeColor: '#ff0000',
    textColor: '#ff0000',
    strokeWidthPt: 1,
    fontSizePt: 12,
    opacity: 1,
  };
}

function renderLengthMarkup(markup: LengthMarkup, pageScale: PageScale | undefined): readonly RenderPrimitive[] {
  const style = getAnnotationContentStyle(markup);
  return [
    {
      kind: 'polyline',
      points: [markup.start, markup.end],
      style: { stroke: style.stroke, fill: 'none', strokeWidth: style.strokeWidth, opacity: style.opacity },
      pointerEvents: 'visibleStroke',
    },
    labelPrimitive(lengthLabelPoint(markup), measurementLabel(markup, pageScale), markup),
  ];
}

function renderPolylengthMarkup(markup: PolylengthMarkup, pageScale: PageScale | undefined): readonly RenderPrimitive[] {
  return renderMeasurementPath(markup, pageScale, false);
}

function renderAreaMarkup(markup: AreaMarkup, pageScale: PageScale | undefined): readonly RenderPrimitive[] {
  return renderMeasurementPath(markup, pageScale, true);
}

function renderPolylengthPreview(points: readonly PdfPoint[], pageScale: PageScale | undefined): readonly RenderPrimitive[] {
  if (points.length === 0) {
    return [];
  }
  const markup = createPolylengthMarkup({ id: 'polylength-preview', pageIndex: 0, points, color: '#ff0000' });
  return renderMeasurementPath(markup, pageScale, false);
}

function renderAreaPreview(points: readonly PdfPoint[], pageScale: PageScale | undefined): readonly RenderPrimitive[] {
  if (points.length === 0) {
    return [];
  }
  const markup = createAreaMarkup({ id: 'area-preview', pageIndex: 0, points, color: '#ff0000' });
  return renderMeasurementPath(markup, pageScale, true);
}

function renderMeasurementPath(markup: PolylengthMarkup | AreaMarkup, pageScale: PageScale | undefined, closed: boolean): readonly RenderPrimitive[] {
  const style = getAnnotationContentStyle(markup);
  const points = markup.points;
  return [
    {
      kind: closed && points.length >= 3 ? 'polygon' : 'polyline',
      points,
      style: {
        stroke: style.stroke,
        fill: closed && points.length >= 3 ? style.fill : 'none',
        strokeWidth: style.strokeWidth,
        opacity: style.opacity,
      },
      pointerEvents: closed && style.fill !== 'none' ? 'all' : 'visibleStroke',
    },
    labelPrimitive(closed ? areaLabelPoint(points) : pathLabelPoint(points), measurementLabel(markup, pageScale), markup),
  ];
}

function labelPrimitive(anchor: PdfPoint, text: string, markup: MeasurementMarkup): RenderPrimitive {
  return {
    kind: 'textBox',
    rect: labelRect(anchor, text, markup),
    text,
    style: getAnnotationTextContentStyle(markup, 13 / 12),
    pointerEvents: 'all',
  };
}

function labelRect(anchor: PdfPoint, text: string, markup: MeasurementMarkup): Rect {
  const style = getAnnotationTextContentStyle(markup, 13 / 12);
  const fontSizePt = style.fontSizePt ?? 12;
  const lineHeightPt = style.lineHeightPt ?? fontSizePt * 1.15;
  const insetPt = style.textInsetPt ?? 0;
  const measuredWidth = measureAnnotationText(text, {
    fontFamily: style.fontFamily,
    fontSizePt,
  });
  const width = Math.max(fontSizePt * (56 / 12), measuredWidth + insetPt * 2);
  const height = Math.max(lineHeightPt, fontSizePt * 1.5);
  return rect(anchor.x + 6, anchor.y + 6, width, height);
}

function lengthLabelPoint(markup: Pick<LengthMarkup, 'start' | 'end'>): PdfPoint {
  return midpoint(markup.start, markup.end);
}

function pathLabelPoint(points: readonly PdfPoint[]): PdfPoint {
  if (points.length === 0) {
    return pdfPoint(0, 0);
  }
  if (points.length === 1) {
    return points[0];
  }
  const target = totalPdfPathLength(points, false) * 0.5;
  let travelled = 0;
  for (let index = 1; index < points.length; index += 1) {
    const start = points[index - 1];
    const end = points[index];
    const segmentLength = Math.hypot(end.x - start.x, end.y - start.y);
    if (travelled + segmentLength >= target && segmentLength > 0) {
      const t = (target - travelled) / segmentLength;
      return pdfPoint(start.x + (end.x - start.x) * t, start.y + (end.y - start.y) * t);
    }
    travelled += segmentLength;
  }
  return points[points.length - 1];
}

function areaLabelPoint(points: readonly PdfPoint[]): PdfPoint {
  if (points.length < 3) {
    return pathLabelPoint(points);
  }
  const total = points.reduce((sum, point) => pdfPoint(sum.x + point.x, sum.y + point.y), pdfPoint(0, 0));
  return pdfPoint(total.x / points.length, total.y / points.length);
}

function pathGeometry(
  id: 'polylength' | 'area',
  markup: PolylengthMarkup | AreaMarkup,
  closed: boolean,
  pageScale: PageScale | undefined,
): ToolGeometryDescriptor {
  const points = markup.points;
  const label = measurementLabel(markup, pageScale);
  return {
    bounds: pointsBounds(points),
    components: [
      {
        id: `${id}.body`,
        role: 'measurement',
        geometry: closed ? { kind: 'vertexPath', points, closed: true } : { kind: 'polyline', points },
        bodyDrag: 'moveSelf',
      },
      {
        id: `${id}.label`,
        role: 'textBox',
        geometry: { kind: 'textBox', rect: labelRect(closed ? areaLabelPoint(points) : pathLabelPoint(points), label, markup) },
        bodyDrag: 'moveSelf',
      },
    ],
    handles: points.map((point, index) => ({
      id: `${id}.vertex.${index}`,
      componentId: `${id}.body`,
      point,
      behavior: 'reshapeVertex' as const,
      cursor: getMoveCursor(),
    })),
  };
}

function pathSelection(id: 'polylength' | 'area', closed: boolean) {
  return {
    getSelectionChrome(markup: PolylengthMarkup | AreaMarkup): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: pointsBounds(markup.points),
          kind: 'child',
          canResize: false,
          canRotate: false,
        },
        handles: markup.points.map((point, index) => ({
          id: `${id}.vertex.${index}`,
          componentId: `${id}.body`,
          point,
          behavior: 'reshapeVertex' as const,
          cursor: getMoveCursor(),
        })),
        controlPaths: [{ id: `${id}.path`, points: markup.points, closed }],
      };
    },
    getDraftChrome(draft: MeasurementPathDraft): SelectionChromeDescriptor {
      const points = measurementPathPreviewPoints(draft);
      return {
        bounds: {
          rect: pointsBounds(points),
          kind: 'child',
          canResize: false,
          canRotate: false,
        },
        handles: [],
      };
    },
  };
}

function pathInteraction<TMarkup extends PolylengthMarkup | AreaMarkup>(
  id: 'polylength' | 'area',
  createMarkup: (params: Omit<TMarkup, 'kind'>) => TMarkup,
) {
  return {
    transformMarkup(markup: TMarkup, input: { readonly handleId: string; readonly handleBehavior: string; readonly currentPoint: PdfPoint }): TMarkup {
      if (input.handleBehavior !== 'reshapeVertex') {
        return markup;
      }
      const vertexIndex = Number(input.handleId.split('.').at(-1));
      if (!Number.isInteger(vertexIndex) || vertexIndex < 0 || vertexIndex >= markup.points.length) {
        return markup;
      }
      return createMarkup({
        ...markup,
        points: markup.points.map((point, index) => index === vertexIndex ? input.currentPoint : point),
      } as Omit<TMarkup, 'kind'>);
    },
    dragMarkup(markup: TMarkup, input: { readonly bodyDrag: string | undefined; readonly delta: PdfPoint }): TMarkup {
      if (input.bodyDrag !== 'moveSelf') {
        return markup;
      }
      return createMarkup({
        ...markup,
        points: markup.points.map((point) => translatePoint(point, input.delta)),
      } as Omit<TMarkup, 'kind'>);
    },
  };
}

function endpointHandles(id: 'length', start: PdfPoint, end: PdfPoint) {
  return [
    {
      id: `${id}.endpoint.start`,
      componentId: `${id}.body`,
      point: start,
      behavior: 'moveEndpoint' as const,
      cursor: getMoveCursor(),
    },
    {
      id: `${id}.endpoint.end`,
      componentId: `${id}.body`,
      point: end,
      behavior: 'moveEndpoint' as const,
      cursor: getMoveCursor(),
    },
  ];
}

function measurementHit(markupId: string, componentId: string): ToolHit {
  return {
    markupId,
    componentId,
    region: 'edge',
    bodyDrag: 'moveSelf',
    cursor: getMoveCursor(),
  };
}

function pointsBounds(points: readonly PdfPoint[], padding = 0): Rect {
  if (points.length === 0) {
    return rect(0, 0, 1, 1);
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const minX = Math.min(...xs) - padding;
  const minY = Math.min(...ys) - padding;
  const maxX = Math.max(...xs) + padding;
  const maxY = Math.max(...ys) + padding;
  return rect(minX, minY, Math.max(1, maxX - minX), Math.max(1, maxY - minY));
}

function lineBounds(start: PdfPoint, end: PdfPoint, padding = 0): Rect {
  return pointsBounds([start, end], padding);
}

function rectToPdfArray(box: Rect): readonly number[] {
  return [box.x, box.y, box.x + box.width, box.y + box.height];
}

function midpoint(start: PdfPoint, end: PdfPoint): PdfPoint {
  return pdfPoint((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
}

function totalPdfPathLength(points: readonly PdfPoint[], closed: boolean): number {
  const segmentCount = closed ? points.length : Math.max(0, points.length - 1);
  let total = 0;
  for (let index = 0; index < segmentCount; index += 1) {
    const start = points[index];
    const end = points[(index + 1) % points.length];
    total += Math.hypot(end.x - start.x, end.y - start.y);
  }
  return total;
}

function readLine(value: unknown, fallbackRect: readonly number[] | undefined): { start: PdfPoint; end: PdfPoint } {
  if (Array.isArray(value) && value.length >= 4) {
    return {
      start: pdfPoint(Number(value[0]), Number(value[1])),
      end: pdfPoint(Number(value[2]), Number(value[3])),
    };
  }
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = fallbackRect ?? [];
  return { start: pdfPoint(x1, y1), end: pdfPoint(x2, y2) };
}

function readVertices(value: unknown, fallbackRect: readonly number[] | undefined, closed: boolean): readonly PdfPoint[] {
  if (Array.isArray(value) && value.length >= (closed ? 6 : 4)) {
    const points: PdfPoint[] = [];
    for (let index = 0; index < value.length - 1; index += 2) {
      points.push(pdfPoint(Number(value[index]), Number(value[index + 1])));
    }
    return points;
  }
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = fallbackRect ?? [];
  return closed
    ? [pdfPoint(x1, y1), pdfPoint(x2, y1), pdfPoint(x2, y2)]
    : [pdfPoint(x1, y1), pdfPoint(x2, y2)];
}
