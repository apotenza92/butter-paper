import { createPolygonMarkup, pdfPoint, rect, type PdfPoint, type PolygonMarkup, type Rect } from '@butter-paper/core';
import {
  createVertexPathDraft,
  updateVertexPathDraft,
  vertexPathPreviewPoints,
  type VertexPathDraft,
} from '../annotationLifecycle';
import { getAnnotationContentStyle } from '../annotationStyles';
import { isPointInPolygon, isPointNearPolygonEdge } from '../hitTesting';
import { getMoveCursor } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const POLYGON_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'color', key: 'fillColor', label: 'Fill', default: null },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const POLYGON_TOOL_DEFINITION: PdfToolDefinition<PolygonMarkup, VertexPathDraft> & { readonly id: 'polygon' } = {
  id: 'polygon',
  label: 'Polygon',
  shortcut: 'Shift+P',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-polygon',
  implemented: true,
  properties: POLYGON_PROPERTIES,
  defaults: {
    strokeColor: '#ff0000',
    strokeWidthPt: 1,
    fillColor: null,
    opacity: 1,
  },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: pointsBounds(markup.points),
        components: [
          {
            id: 'polygon.body',
            role: 'shape',
            geometry: { kind: 'vertexPath', points: markup.points, closed: true },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: markup.points.map((point, index) => ({
          id: `polygon.vertex.${index}`,
          componentId: 'polygon.body',
          point,
          behavior: 'reshapeVertex' as const,
          cursor: getMoveCursor(),
        })),
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      const style = getAnnotationContentStyle(markup);
      if (style.fill !== 'none' && isPointInPolygon(point, markup.points)) {
        return {
          markupId: markup.id,
          componentId: 'polygon.body',
          region: 'interior',
          bodyDrag: 'moveSelf',
          cursor: getMoveCursor(),
        };
      }
      if (!isPointNearPolygonEdge(point, markup.points, context.tolerance)) {
        return null;
      }

      return {
        markupId: markup.id,
        componentId: 'polygon.body',
        region: 'edge',
        bodyDrag: 'moveSelf',
        cursor: getMoveCursor(),
      };
    },
  },
  render: {
    getContentPrimitives(markup) {
      const style = getAnnotationContentStyle(markup);
      return [
        {
          kind: 'polygon',
          points: markup.points,
          style: {
            stroke: style.stroke,
            fill: style.fill,
            strokeWidth: style.strokeWidth,
            opacity: style.opacity,
          },
          pointerEvents: style.fill === 'none' ? 'visibleStroke' : 'all',
        },
      ];
    },
    getDraftPrimitives(draft) {
      return [
        {
          kind: 'polyline',
          points: vertexPathPreviewPoints(draft),
          style: {
            stroke: '#ff0000',
            fill: 'none',
            strokeWidth: 1,
            opacity: 1,
          },
          pointerEvents: 'none',
        },
      ];
    },
  },
  selection: {
    getSelectionChrome(markup): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: pointsBounds(markup.points),
          kind: 'child',
        },
        handles: markup.points.map((point, index) => ({
          id: `polygon.vertex.${index}`,
          componentId: 'polygon.body',
          point,
          behavior: 'reshapeVertex' as const,
          cursor: getMoveCursor(),
        })),
        controlPaths: [{ id: 'polygon.path', points: markup.points, closed: true }],
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      const points = vertexPathPreviewPoints(draft);
      return {
        bounds: {
          rect: pointsBounds(points),
          kind: 'child',
        },
        handles: [],
      };
    },
  },
  interaction: {
    placement: 'click',
    createDraft(session) {
      return createVertexPathDraft('polygon', session.startPoint);
    },
    updateDraft(draft, point) {
      return updateVertexPathDraft(draft, point);
    },
    commitDraft(draft, context) {
      if (draft.points.length < 3) {
        return null;
      }

      return createPolygonMarkup({
        id: context.createMarkupId('polygon'),
        pageIndex: context.page.index,
        points: draft.points,
        source: { source: 'butter' },
      });
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior !== 'reshapeVertex') {
        return markup;
      }
      const vertexIndex = Number(input.handleId.split('.').at(-1));
      if (!Number.isInteger(vertexIndex) || vertexIndex < 0 || vertexIndex >= markup.points.length) {
        return markup;
      }

      return createPolygonMarkup({
        ...markup,
        points: markup.points.map((point, index) => index === vertexIndex ? input.currentPoint : point),
      });
    },
  },
  pdf: {
    canImport(annotation) {
      return String(annotation.subtype ?? '').toLowerCase() === 'polygon';
    },
    import(annotation, context) {
      return createPolygonMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        points: readVertices(annotation.fields?.Vertices, annotation.rect),
        source: { source: 'imported' },
      });
    },
  },
};

function pointsBounds(points: readonly PdfPoint[]): Rect {
  const bounds = rectFromPoints(points);
  return rect(bounds[0], bounds[1], bounds[2] - bounds[0], bounds[3] - bounds[1]);
}

function rectFromPoints(points: readonly PdfPoint[]): readonly number[] {
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

function readVertices(value: unknown, fallbackRect: readonly number[] | undefined): readonly PdfPoint[] {
  if (Array.isArray(value) && value.length >= 6) {
    const points: PdfPoint[] = [];
    for (let index = 0; index < value.length - 1; index += 2) {
      points.push(pdfPoint(Number(value[index]), Number(value[index + 1])));
    }
    return points;
  }
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = fallbackRect ?? [];
  return [pdfPoint(x1, y1), pdfPoint(x2, y1), pdfPoint(x2, y2)];
}
