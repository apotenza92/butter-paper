import { createSnapshotMarkup, pdfPoint, rect as createRect, type SnapshotMarkup } from '@butter-paper/core';
import {
  createRectangleDraft,
  rectangleDraftToRect,
  resizeRotatedRectFromHandle,
  shouldCommitRectangle,
  updateRectangleDraft,
  type RectangleDraft,
  type RectResizeHandle,
} from '../annotationLifecycle';
import { isPointInRect } from '../hitTesting';
import { getAnnotationContentStyle } from '../annotationStyles';
import { getMoveCursor, getResizeHandles, getRotationHandle } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';
import { DEFAULT_IMAGE_DATA_URL } from './imageTool';

const SNAPSHOT_PROPERTIES = {
  properties: [
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const SNAPSHOT_TOOL_DEFINITION: PdfToolDefinition<SnapshotMarkup, RectangleDraft> & { readonly id: 'snapshot' } = {
  id: 'snapshot',
  label: 'Snapshot',
  shortcut: 'G',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-snapshot',
  implemented: true,
  properties: SNAPSHOT_PROPERTIES,
  defaults: { opacity: 1 },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: markup.rect,
        components: [
          {
            id: 'snapshot.body',
            role: 'media',
            geometry: { kind: 'rect', rect: markup.rect, rotation: markup.rotation },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: [
          ...getResizeHandles(markup.rect).map((handle) => ({
            id: `snapshot.resize.${handle.kind}`,
            componentId: 'snapshot.body',
            point: pdfPoint(handle.x, handle.y),
            behavior: 'resizeSelf' as const,
            cursor: handle.cursor,
          })),
          createRotationHandle(markup),
        ],
      };
    },
    hitTest(markup, point): ToolHit | null {
      if (!isPointInRect(unrotatePoint(point, markup), markup.rect)) {
        return null;
      }
      return {
        markupId: markup.id,
        componentId: 'snapshot.body',
        region: 'interior',
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
          kind: 'image',
          rect: markup.rect,
          assetId: markup.dataUrl,
          rotation: markup.rotation,
          opacity: style.opacity,
          pointerEvents: 'all',
        },
      ];
    },
    getDraftPrimitives(draft) {
      const rect = rectangleDraftToRect(draft);
      return [
        {
          kind: 'rect',
          rect,
          style: {
            stroke: '#2563eb',
            fill: 'rgba(37, 99, 235, 0.08)',
            strokeWidth: 1,
            dashArray: '4 3',
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
        handles: SNAPSHOT_TOOL_DEFINITION.geometry?.getGeometry(markup, context).handles,
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
      return createSnapshotMarkup({
        id: context.createMarkupId('snapshot'),
        pageIndex: context.page.index,
        rect,
        dataUrl: DEFAULT_IMAGE_DATA_URL,
        mimeType: 'image/png',
        source: { source: 'butter' },
      });
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior === 'rotateSelf') {
        return createSnapshotMarkup({
          ...markup,
          rotation: rotationFromDrag(markup, input.startPoint, input.currentPoint),
        });
      }
      if (input.handleBehavior !== 'resizeSelf') {
        return markup;
      }
      const handle = resizeHandleFromId(input.handleId);
      return handle
        ? createSnapshotMarkup({ ...markup, rect: resizeRotatedRectFromHandle(markup.rect, markup.rotation, handle, input.currentPoint) })
        : markup;
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      const subject = String(annotation.subject ?? annotation.fields?.Subj ?? '').toLowerCase();
      const intent = String(annotation.intent ?? annotation.fields?.IT ?? '').toLowerCase();
      return subtype === 'stamp' && (subject === 'snapshot' || intent === 'stampsnapshot');
    },
    import(annotation, context) {
      const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = annotation.rect ?? [];
      return createSnapshotMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        rect: createRect(Math.min(x1, x2), Math.min(y1, y2), Math.abs(x2 - x1), Math.abs(y2 - y1)),
        dataUrl: String(annotation.fields?.BPSnapshotData ?? DEFAULT_IMAGE_DATA_URL),
        mimeType: String(annotation.fields?.BPSnapshotMimeType ?? 'image/png') === 'image/jpeg' ? 'image/jpeg' : 'image/png',
        source: { source: 'imported' },
      });
    },
  },
};

function createRotationHandle(markup: SnapshotMarkup) {
  const handle = getRotationHandle(markup.rect);
  return {
    id: 'snapshot.rotate',
    componentId: 'snapshot.body',
    point: pdfPoint(handle.x, handle.y),
    behavior: 'rotateSelf' as const,
    cursor: handle.cursor,
  };
}

function rotationFromDrag(markup: SnapshotMarkup, startPoint: { readonly x: number; readonly y: number }, currentPoint: { readonly x: number; readonly y: number }): number {
  const center = {
    x: markup.rect.x + markup.rect.width * 0.5,
    y: markup.rect.y + markup.rect.height * 0.5,
  };
  const startAngle = Math.atan2(startPoint.y - center.y, startPoint.x - center.x);
  const currentAngle = Math.atan2(currentPoint.y - center.y, currentPoint.x - center.x);
  const deltaDegrees = ((startAngle - currentAngle) * 180) / Math.PI;
  return ((markup.rotation ?? 0) + deltaDegrees + 360) % 360;
}

function resizeHandleFromId(handleId: string): RectResizeHandle | null {
  const handle = handleId.replace('snapshot.resize.', '');
  if (handle === 'nw' || handle === 'n' || handle === 'ne' || handle === 'e' || handle === 'se' || handle === 's' || handle === 'sw' || handle === 'w') {
    return handle;
  }
  return null;
}

function unrotatePoint(point: { readonly x: number; readonly y: number }, markup: SnapshotMarkup) {
  const centerX = markup.rect.x + markup.rect.width * 0.5;
  const centerY = markup.rect.y + markup.rect.height * 0.5;
  const radians = -((markup.rotation ?? 0) * Math.PI) / 180;
  const dx = point.x - centerX;
  const dy = point.y - centerY;
  return pdfPoint(
    centerX + dx * Math.cos(radians) - dy * Math.sin(radians),
    centerY + dx * Math.sin(radians) + dy * Math.cos(radians),
  );
}
