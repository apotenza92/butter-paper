import { createRectangleMarkup, pdfPoint, rect as createRect, type RectangleMarkup } from '@butter-paper/core';
import {
  createRectangleDraft,
  rectangleDraftToRect,
  resizeRectFromHandle,
  resizeRotatedRectFromHandle,
  shouldCommitRectangle,
  updateRectangleDraft,
  type RectResizeHandle,
  type RectangleDraft,
} from '../annotationLifecycle';
import { getAnnotationContentStyle } from '../annotationStyles';
import { isPointInRotatedRect, isPointNearRotatedRectEdge } from '../hitTesting';
import { getMoveCursor, getResizeHandles, getRotationHandle } from '../interactionChrome';
import { isPointNearSelectionChromeEdge } from '../selectionHitZones';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const RECTANGLE_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'color', key: 'fillColor', label: 'Fill', default: null },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const RECTANGLE_TOOL_DEFINITION: PdfToolDefinition<RectangleMarkup, RectangleDraft> & { readonly id: 'rectangle' } = {
  id: 'rectangle',
  label: 'Rectangle',
  shortcut: 'R',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-rectangle',
  implemented: true,
  properties: RECTANGLE_PROPERTIES,
  defaults: {
    strokeColor: '#ff0000',
    strokeWidthPt: 1,
    fillColor: null,
    opacity: 1,
  },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: markup.rect,
        components: [
          {
            id: 'rectangle.body',
            role: 'shape',
            geometry: { kind: 'rect', rect: markup.rect, rotation: markup.rotation },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: [
          ...getResizeHandles(markup.rect).map((handle) => ({
            id: `rectangle.resize.${handle.kind}`,
            componentId: 'rectangle.body',
            point: pdfPoint(handle.x, handle.y),
            behavior: 'resizeSelf' as const,
            cursor: handle.cursor,
          })),
          createRotationHandle(markup),
        ],
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      const style = getAnnotationContentStyle(markup);
      if (style.fill !== 'none' && isPointInRotatedRect(point, markup.rect, markup.rotation)) {
        return {
          markupId: markup.id,
          componentId: 'rectangle.body',
          region: 'interior',
          bodyDrag: 'moveSelf',
          cursor: getMoveCursor(),
        };
      }
      if (context.transform) {
        if (!isPointNearSelectionChromeEdge(point, markup.rect, {
          transform: context.transform,
          rotation: markup.rotation,
          state: 'hovered',
        })) {
          return null;
        }
      } else if (!isPointNearRotatedRectEdge(point, markup.rect, markup.rotation, context.tolerance)) {
        return null;
      }

      return {
        markupId: markup.id,
        componentId: 'rectangle.body',
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
          kind: 'rect',
          rect: markup.rect,
          rotation: markup.rotation,
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
          kind: 'rect',
          rect: rectangleDraftToRect(draft),
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
    getSelectionChrome(markup, context): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: markup.rect,
          rotation: markup.rotation,
          kind: 'child',
          canResize: true,
          canRotate: context.phase !== 'hovered',
        },
        handles: RECTANGLE_TOOL_DEFINITION.geometry?.getGeometry(markup, context).handles,
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: rectangleDraftToRect(draft),
          kind: 'child',
          canResize: false,
          canRotate: false,
        },
        handles: [],
      };
    },
  },
  interaction: {
    placement: 'click',
    createDraft(session) {
      return createRectangleDraft(session.startPoint);
    },
    updateDraft(draft, point) {
      return updateRectangleDraft(draft, point);
    },
    commitDraft(draft, context) {
      const rect = rectangleDraftToRect(draft);
      if (!shouldCommitRectangle(rect)) {
        return null;
      }

      return createRectangleMarkup({
        id: context.createMarkupId('rect'),
        pageIndex: context.page.index,
        rect,
        source: { source: 'butter' },
      });
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior === 'rotateSelf') {
        return createRectangleMarkup({
          ...markup,
          rotation: rotationFromDrag(markup, input.startPoint, input.currentPoint),
        });
      }

      if (input.handleBehavior !== 'resizeSelf') {
        return markup;
      }

      const handle = resizeHandleFromId(input.handleId);
      if (!handle) {
        return markup;
      }

      return createRectangleMarkup({
        ...markup,
        rect: resizeRotatedRectFromHandle(markup.rect, markup.rotation, handle, input.currentPoint),
      });
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      return subtype === 'square' || subtype === 'rect';
    },
    import(annotation, context) {
      const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = annotation.rect ?? [];
      return createRectangleMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        rect: createRect(Math.min(x1, x2), Math.min(y1, y2), Math.abs(x2 - x1), Math.abs(y2 - y1)),
        source: { source: 'imported' },
      });
    },
  },
};

function createRotationHandle(markup: RectangleMarkup) {
  const handle = getRotationHandle(markup.rect);
  return {
    id: 'rectangle.rotate',
    componentId: 'rectangle.body',
    point: pdfPoint(handle.x, handle.y),
    behavior: 'rotateSelf' as const,
    cursor: handle.cursor,
  };
}

function rotationFromDrag(markup: RectangleMarkup, startPoint: { x: number; y: number }, currentPoint: { x: number; y: number }): number {
  const center = {
    x: markup.rect.x + markup.rect.width * 0.5,
    y: markup.rect.y + markup.rect.height * 0.5,
  };
  const startAngle = Math.atan2(startPoint.y - center.y, startPoint.x - center.x);
  const currentAngle = Math.atan2(currentPoint.y - center.y, currentPoint.x - center.x);
  const deltaDegrees = ((startAngle - currentAngle) * 180) / Math.PI;
  return normalizeDegrees((markup.rotation ?? 0) + deltaDegrees);
}

function normalizeDegrees(value: number): number {
  return ((value % 360) + 360) % 360;
}

function resizeHandleFromId(handleId: string): RectResizeHandle | null {
  const handle = handleId.replace('rectangle.resize.', '');
  if (
    handle === 'nw'
    || handle === 'n'
    || handle === 'ne'
    || handle === 'e'
    || handle === 'se'
    || handle === 's'
    || handle === 'sw'
    || handle === 'w'
  ) {
    return handle;
  }

  return null;
}
