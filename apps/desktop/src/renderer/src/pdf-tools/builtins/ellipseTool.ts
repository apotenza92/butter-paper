import { createEllipseMarkup, pdfPoint, rect as createRect, type EllipseMarkup, type PdfPoint, type Rect } from '@butter-paper/core';
import {
  createRectangleDraft,
  rectangleDraftToRect,
  resizeRotatedRectFromHandle,
  shouldCommitRectangle,
  updateRectangleDraft,
  type RectResizeHandle,
  type RectangleDraft,
} from '../annotationLifecycle';
import { getAnnotationContentStyle } from '../annotationStyles';
import { isPointInRotatedEllipse, isPointNearRotatedEllipseEdge } from '../hitTesting';
import { getMoveCursor, getResizeHandles, getRotationHandle } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const ELLIPSE_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'color', key: 'fillColor', label: 'Fill', default: null },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const ELLIPSE_TOOL_DEFINITION: PdfToolDefinition<EllipseMarkup, RectangleDraft> & { readonly id: 'ellipse' } = {
  id: 'ellipse',
  label: 'Ellipse',
  shortcut: 'E',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-ellipse',
  implemented: true,
  properties: ELLIPSE_PROPERTIES,
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
            id: 'ellipse.body',
            role: 'shape',
            geometry: { kind: 'rect', rect: markup.rect, rotation: markup.rotation },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: [
          ...getEllipseResizeHandles(markup.rect).map((handle) => ({
            id: `ellipse.resize.${handle.kind}`,
            componentId: 'ellipse.body',
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
      if (style.fill !== 'none' && isPointInRotatedEllipse(point, markup.rect, markup.rotation)) {
        return {
          markupId: markup.id,
          componentId: 'ellipse.body',
          region: 'interior',
          bodyDrag: 'moveSelf',
          cursor: getMoveCursor(),
        };
      }
      if (!isPointNearRotatedEllipseEdge(point, markup.rect, markup.rotation, context.tolerance)) {
        return null;
      }

      return {
        markupId: markup.id,
        componentId: 'ellipse.body',
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
          kind: 'ellipse',
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
          kind: 'ellipse',
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
        },
        handles: ELLIPSE_TOOL_DEFINITION.geometry?.getGeometry(markup, context).handles,
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: rectangleDraftToRect(draft),
          kind: 'child',
        },
        handles: [],
      };
    },
  },
  interaction: {
    placement: 'click-or-drag',
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

      return createEllipseMarkup({
        id: context.createMarkupId('ellipse'),
        pageIndex: context.page.index,
        rect,
        source: { source: 'butter' },
      });
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior === 'rotateSelf') {
        return createEllipseMarkup({
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

      return createEllipseMarkup({
        ...markup,
        rect: resizeRotatedRectFromHandle(
          markup.rect,
          markup.rotation,
          handle,
          ellipseResizePointFromHandle(markup.rect, markup.rotation ?? 0, handle, input.currentPoint),
        ),
      });
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      const intent = String(annotation.intent ?? '').toLowerCase();
      return subtype === 'circle' && intent !== 'circlearc';
    },
    import(annotation, context) {
      const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = annotation.rect ?? [];
      return createEllipseMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        rect: createRect(Math.min(x1, x2), Math.min(y1, y2), Math.abs(x2 - x1), Math.abs(y2 - y1)),
        source: { source: 'imported' },
      });
    },
  },
};

export function constrainEllipseDraftPoint(start: PdfPoint, point: PdfPoint): PdfPoint {
  const deltaX = point.x - start.x;
  const deltaY = point.y - start.y;
  const diameter = Math.max(Math.abs(deltaX), Math.abs(deltaY));

  return pdfPoint(
    start.x + diameter * (Math.sign(deltaX) || 1),
    start.y + diameter * (Math.sign(deltaY) || 1),
  );
}

const ELLIPSE_DIAGONAL_RADIUS_FACTOR = Math.SQRT1_2;
const ELLIPSE_DIAGONAL_OPPOSITE_FACTOR = (1 + ELLIPSE_DIAGONAL_RADIUS_FACTOR) * 0.5;

function getEllipseResizeHandles(rect: Rect) {
  const centerX = rect.x + rect.width * 0.5;
  const centerY = rect.y + rect.height * 0.5;
  const diagonalX = rect.width * 0.5 * ELLIPSE_DIAGONAL_RADIUS_FACTOR;
  const diagonalY = rect.height * 0.5 * ELLIPSE_DIAGONAL_RADIUS_FACTOR;

  return getResizeHandles(rect).map((handle) => {
    if (handle.kind === 'nw') {
      return { ...handle, x: centerX - diagonalX, y: centerY + diagonalY };
    }
    if (handle.kind === 'ne') {
      return { ...handle, x: centerX + diagonalX, y: centerY + diagonalY };
    }
    if (handle.kind === 'se') {
      return { ...handle, x: centerX + diagonalX, y: centerY - diagonalY };
    }
    if (handle.kind === 'sw') {
      return { ...handle, x: centerX - diagonalX, y: centerY - diagonalY };
    }
    return handle;
  });
}

export function ellipseResizePointFromHandle(
  original: Rect,
  rotation: number,
  handle: RectResizeHandle,
  point: PdfPoint,
): PdfPoint {
  if (handle === 'n' || handle === 'e' || handle === 's' || handle === 'w') {
    return point;
  }

  const localPoint = rotatePointAroundRectCenter(point, original, rotation);
  const opposite = pdfPoint(
    handle.includes('w') ? original.x + original.width : original.x,
    handle.includes('n') ? original.y : original.y + original.height,
  );
  const boundsPoint = pdfPoint(
    opposite.x + (localPoint.x - opposite.x) / ELLIPSE_DIAGONAL_OPPOSITE_FACTOR,
    opposite.y + (localPoint.y - opposite.y) / ELLIPSE_DIAGONAL_OPPOSITE_FACTOR,
  );
  return rotatePointAroundRectCenter(boundsPoint, original, -rotation);
}

function rotatePointAroundRectCenter(point: PdfPoint, box: Rect, degrees: number): PdfPoint {
  const centerX = box.x + box.width * 0.5;
  const centerY = box.y + box.height * 0.5;
  const radians = (degrees * Math.PI) / 180;
  const dx = point.x - centerX;
  const dy = point.y - centerY;
  return pdfPoint(
    centerX + dx * Math.cos(radians) - dy * Math.sin(radians),
    centerY + dx * Math.sin(radians) + dy * Math.cos(radians),
  );
}

function createRotationHandle(markup: EllipseMarkup) {
  const handle = getRotationHandle(markup.rect);
  return {
    id: 'ellipse.rotate',
    componentId: 'ellipse.body',
    point: pdfPoint(handle.x, handle.y),
    behavior: 'rotateSelf' as const,
    cursor: handle.cursor,
  };
}

function rotationFromDrag(markup: EllipseMarkup, startPoint: { x: number; y: number }, currentPoint: { x: number; y: number }): number {
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
  const handle = handleId.replace('ellipse.resize.', '');
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
