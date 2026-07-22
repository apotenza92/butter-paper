import type { ImportedAnnotationMarkup } from '@butter-paper/core';
import { isPointInRect } from '../hitTesting';
import { getMoveCursor } from '../interactionChrome';
import { isPointInSelectionChromeBounds } from '../selectionHitZones';
import type { PdfToolDefinition, RenderPrimitive, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const IMPORTED_ANNOTATION_MIN_RENDER_ZOOM = 0.35;

export const IMPORTED_ANNOTATION_TOOL_DEFINITION: PdfToolDefinition<ImportedAnnotationMarkup> & { readonly id: 'imported-annotation' } = {
  id: 'imported-annotation',
  label: 'Imported Annotation',
  category: 'markup',
  cursor: getMoveCursor(),
  testId: 'tool-imported-annotation',
  implemented: true,
  properties: { properties: [] },
  defaults: {},
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: markup.rect,
        components: [
          {
            id: 'imported.body',
            role: 'bounds',
            geometry: { kind: 'rect', rect: markup.rect },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: [],
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      if (isLinkAnnotation(markup)) {
        return null;
      }

      const isHit = context.transform
        ? isPointInSelectionChromeBounds(point, markup.rect, {
          transform: context.transform,
          state: 'hovered',
        })
        : isPointInRect(point, markup.rect);

      if (!isHit) {
        return null;
      }

      return {
        markupId: markup.id,
        componentId: 'imported.body',
        region: 'interior',
        bodyDrag: 'moveSelf',
        cursor: getMoveCursor(),
      };
    },
  },
  render: {
    getContentPrimitives(markup) {
      if (isLinkAnnotation(markup)) {
        return [];
      }

      const primitives: RenderPrimitive[] = [
        {
          kind: 'rect',
          rect: markup.rect,
          style: {
            stroke: '#ef4444',
            fill: 'rgba(239,68,68,0.04)',
            strokeWidth: 1.25,
            opacity: 1,
            dashArray: '6 3',
            hideBelowZoom: IMPORTED_ANNOTATION_MIN_RENDER_ZOOM,
          },
          pointerEvents: 'all',
        },
      ];

      primitives.push({
        kind: 'textBox',
        rect: markup.rect,
        text: labelForImportedAnnotation(markup),
        style: {
          textColor: '#991b1b',
          fontFamily: 'Helvetica, Arial, sans-serif',
          fontSizePt: 10,
          opacity: 0.95,
          hideBelowZoom: IMPORTED_ANNOTATION_MIN_RENDER_ZOOM,
        },
        pointerEvents: 'none',
      });

      return primitives;
    },
  },
  selection: {
    getSelectionChrome(markup): SelectionChromeDescriptor {
      if (isLinkAnnotation(markup)) {
        return {};
      }

      return {
        bounds: {
          rect: markup.rect,
          kind: 'child',
          canResize: false,
          canRotate: false,
        },
        handles: [],
      };
    },
  },
};

function isLinkAnnotation(markup: ImportedAnnotationMarkup): boolean {
  return markup.subtype.toLowerCase() === 'link';
}

function labelForImportedAnnotation(markup: ImportedAnnotationMarkup): string {
  const subject = markup.subject?.trim();
  if (subject) {
    return subject;
  }

  if (markup.intent) {
    return `${markup.subtype} / ${markup.intent}`;
  }

  return markup.subtype;
}
