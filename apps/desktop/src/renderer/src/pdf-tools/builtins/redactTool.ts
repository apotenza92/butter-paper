import { createRedactMarkup, pdfPoint, type RedactMarkup } from '@butter-paper/core';
import {
  createRectangleDraft,
  rectangleDraftToRect,
  resizeRectFromHandle,
  shouldCommitRectangle,
  updateRectangleDraft,
  type RectResizeHandle,
  type RectangleDraft,
} from '../annotationLifecycle';
import { isPointInRect } from '../hitTesting';
import { getMoveCursor, getResizeHandles } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const REDACT_PROPERTIES = { properties: [] } as const;

export const REDACT_TOOL_DEFINITION: PdfToolDefinition<RedactMarkup, RectangleDraft> & { readonly id: 'redact' } = {
  id: 'redact',
  label: 'Redact',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-redact',
  implemented: true,
  properties: REDACT_PROPERTIES,
  defaults: {},
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: markup.rect,
        components: [{
          id: 'redact.body',
          role: 'shape',
          geometry: { kind: 'rect', rect: markup.rect },
          bodyDrag: 'moveSelf',
        }],
        handles: getResizeHandles(markup.rect).map((handle) => ({
          id: `redact.resize.${handle.kind}`,
          componentId: 'redact.body',
          point: pdfPoint(handle.x, handle.y),
          behavior: 'resizeSelf' as const,
          cursor: handle.cursor,
        })),
      };
    },
    hitTest(markup, point): ToolHit | null {
      if (!isPointInRect(point, markup.rect)) return null;
      return {
        markupId: markup.id,
        componentId: 'redact.body',
        region: 'interior',
        bodyDrag: 'moveSelf',
        cursor: getMoveCursor(),
      };
    },
  },
  render: {
    getContentPrimitives(markup) {
      return [{
        kind: 'rect',
        rect: markup.rect,
        style: { stroke: '#ff0000', fill: '#000000', strokeWidth: 1, opacity: 0.35 },
        pointerEvents: 'all',
      }];
    },
    getDraftPrimitives(draft) {
      return [{
        kind: 'rect',
        rect: rectangleDraftToRect(draft),
        style: { stroke: '#ff0000', fill: '#000000', strokeWidth: 1, opacity: 0.35 },
        pointerEvents: 'none',
      }];
    },
  },
  selection: {
    getSelectionChrome(markup, context): SelectionChromeDescriptor {
      return {
        bounds: { rect: markup.rect, kind: 'child' },
        handles: REDACT_TOOL_DEFINITION.geometry?.getGeometry(markup, context).handles,
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      return { bounds: { rect: rectangleDraftToRect(draft), kind: 'child' }, handles: [] };
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
      if (!shouldCommitRectangle(rect)) return null;
      return createRedactMarkup({
        id: context.createMarkupId('redact'),
        pageIndex: context.page.index,
        rect,
        source: { source: 'butter' },
      });
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior !== 'resizeSelf') return markup;
      const handle = resizeHandleFromId(input.handleId);
      if (!handle) return markup;
      return createRedactMarkup({ ...markup, rect: resizeRectFromHandle(markup.rect, handle, input.currentPoint) });
    },
  },
};

function resizeHandleFromId(handleId: string): RectResizeHandle | null {
  const kind = handleId.split('.').at(-1);
  return kind === 'nw' || kind === 'n' || kind === 'ne' || kind === 'e'
    || kind === 'se' || kind === 's' || kind === 'sw' || kind === 'w'
    ? kind
    : null;
}
