import { createPolylineMarkup, pdfPoint, rect, type PdfPoint, type PolylineMarkup, type Rect } from '@butter-paper/core';
import {
  createLineDraft,
  shouldCommitLine,
  updateLineDraft,
  type LineDraft,
} from '../annotationLifecycle';
import { getAnnotationContentStyle } from '../annotationStyles';
import { isPointNearPolyline } from '../hitTesting';
import { getMoveCursor } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const POLYLINE_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const POLYLINE_TOOL_DEFINITION: PdfToolDefinition<PolylineMarkup, LineDraft> & { readonly id: 'polyline' } = {
  id: 'polyline',
  label: 'Polyline',
  shortcut: 'Shift+N',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-polyline',
  implemented: true,
  properties: POLYLINE_PROPERTIES,
  defaults: {
    strokeColor: '#ff0000',
    strokeWidthPt: 1,
    opacity: 1,
  },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: pointsBounds(markup.points),
        components: [
          {
            id: 'polyline.body',
            role: 'shape',
            geometry: { kind: 'polyline', points: markup.points },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: markup.points.map((point, index) => ({
          id: `polyline.vertex.${index}`,
          componentId: 'polyline.body',
          point,
          behavior: 'reshapeVertex' as const,
          cursor: getMoveCursor(),
        })),
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      if (!isPointNearPolyline(point, markup.points, context.tolerance)) {
        return null;
      }

      return {
        markupId: markup.id,
        componentId: 'polyline.body',
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
          kind: 'polyline',
          points: markup.points,
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
      return [
        {
          kind: 'polyline',
          points: [draft.start, draft.current],
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
          id: `polyline.vertex.${index}`,
          componentId: 'polyline.body',
          point,
          behavior: 'reshapeVertex' as const,
          cursor: getMoveCursor(),
        })),
        controlPaths: [{ id: 'polyline.path', points: markup.points, closed: false }],
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: pointsBounds([draft.start, draft.current]),
          kind: 'child',
        },
        handles: [],
      };
    },
  },
  interaction: {
    placement: 'click',
    createDraft(session) {
      return createLineDraft(session.startPoint);
    },
    updateDraft(draft, point) {
      return updateLineDraft(draft, point);
    },
    commitDraft(draft, context) {
      if (!shouldCommitLine(draft.start, draft.current)) {
        return null;
      }

      return createPolylineMarkup({
        id: context.createMarkupId('polyline'),
        pageIndex: context.page.index,
        points: [draft.start, draft.current],
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

      return createPolylineMarkup({
        ...markup,
        points: markup.points.map((point, index) => index === vertexIndex ? input.currentPoint : point),
      });
    },
  },
  pdf: {
    canImport(annotation) {
      return String(annotation.subtype ?? '').toLowerCase() === 'polyline';
    },
    import(annotation, context) {
      return createPolylineMarkup({
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
  if (Array.isArray(value) && value.length >= 4) {
    const points: PdfPoint[] = [];
    for (let index = 0; index < value.length - 1; index += 2) {
      points.push(pdfPoint(Number(value[index]), Number(value[index + 1])));
    }
    return points;
  }
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = fallbackRect ?? [];
  return [pdfPoint(x1, y1), pdfPoint(x2, y2)];
}
