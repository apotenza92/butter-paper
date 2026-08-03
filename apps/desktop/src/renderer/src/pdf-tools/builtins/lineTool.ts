import { createArrowMarkup, createLineMarkup, pdfPoint, rect, type ArrowMarkup, type LineMarkup, type PdfPoint, type Rect } from '@butter-paper/core';
import {
  createLineDraft,
  shouldCommitLine,
  updateLineDraft,
  type LineDraft,
} from '../annotationLifecycle';
import { getAnnotationContentStyle } from '../annotationStyles';
import { isPointNearLineSegment } from '../hitTesting';
import { getMoveCursor } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

type StraightLineMarkup = LineMarkup | ArrowMarkup;

const LINE_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

const ARROW_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 0.5, min: 0.25, max: 24, step: 0.25 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const LINE_TOOL_DEFINITION: PdfToolDefinition<LineMarkup, LineDraft> & { readonly id: 'line' } = createLineToolDefinition({
  id: 'line',
  label: 'Line',
  shortcut: 'L',
  testId: 'tool-line',
  properties: LINE_PROPERTIES,
  defaultStrokeWidth: 1,
  createMarkup(params) {
    return createLineMarkup(params);
  },
});

export const ARROW_TOOL_DEFINITION: PdfToolDefinition<ArrowMarkup, LineDraft> & { readonly id: 'arrow' } = createLineToolDefinition({
  id: 'arrow',
  label: 'Arrow',
  shortcut: 'A',
  testId: 'tool-arrow',
  properties: ARROW_PROPERTIES,
  defaultStrokeWidth: 0.5,
  createMarkup(params) {
    return createArrowMarkup(params);
  },
});

function createLineToolDefinition<TMarkup extends StraightLineMarkup, TId extends 'line' | 'arrow'>({
  id,
  label,
  shortcut,
  testId,
  properties,
  defaultStrokeWidth,
  createMarkup,
}: {
  readonly id: TId;
  readonly label: string;
  readonly shortcut: string;
  readonly testId: string;
  readonly properties: typeof LINE_PROPERTIES | typeof ARROW_PROPERTIES;
  readonly defaultStrokeWidth: number;
  readonly createMarkup: (params: Omit<TMarkup, 'kind'>) => TMarkup;
}): PdfToolDefinition<TMarkup, LineDraft> & { readonly id: TId } {
  return {
    id,
    label,
    shortcut,
    category: 'markup',
    cursor: 'crosshair',
    testId,
    implemented: true,
    properties,
    defaults: {
      strokeColor: '#ff0000',
      strokeWidthPt: defaultStrokeWidth,
      opacity: 1,
    },
    geometry: {
      getGeometry(markup): ToolGeometryDescriptor {
        return {
          bounds: lineBounds(markup),
          components: [
            {
              id: `${id}.body`,
              role: 'shape',
              geometry: { kind: 'line', start: markup.start, end: markup.end },
              bodyDrag: 'moveSelf',
            },
          ],
          handles: getEndpointHandles(id, markup),
        };
      },
      hitTest(markup, point, context): ToolHit | null {
        if (!isPointNearLineSegment(point, markup.start, markup.end, context.tolerance)) {
          return null;
        }

        return {
          markupId: markup.id,
          componentId: `${id}.body`,
          region: 'edge',
          bodyDrag: 'moveSelf',
          cursor: getMoveCursor(),
        };
      },
    },
    render: {
      getContentPrimitives(markup) {
        const style = getAnnotationContentStyle(markup);
        const primitives = [
          {
            kind: 'polyline' as const,
            points: [markup.start, markup.end],
            style: {
              stroke: style.stroke,
              fill: 'none',
              strokeWidth: style.strokeWidth,
              opacity: style.opacity,
            },
            pointerEvents: 'visibleStroke' as const,
          },
        ];

        if (id !== 'arrow') {
          return primitives;
        }

        return [
          ...primitives,
          {
            kind: 'polygon' as const,
            points: arrowHeadPoints(markup.start, markup.end, Math.max(7, style.strokeWidth * 8), Math.max(4, style.strokeWidth * 5)),
            style: {
              stroke: style.stroke,
              fill: style.stroke,
              strokeWidth: style.strokeWidth,
              opacity: style.opacity,
            },
            pointerEvents: 'none' as const,
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
              strokeWidth: defaultStrokeWidth,
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
            rect: lineBounds(markup),
            kind: 'child',
          },
          handles: getEndpointHandles(id, markup),
          controlPaths: [{ id: `${id}.path`, points: [markup.start, markup.end], closed: false }],
        };
      },
      getDraftChrome(draft): SelectionChromeDescriptor {
        return {
          bounds: {
            rect: lineBounds({ start: draft.start, end: draft.current }),
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

        return createMarkup({
          id: context.createMarkupId(id),
          pageIndex: context.page.index,
          start: draft.start,
          end: draft.current,
          source: { source: 'butter' },
        } as Omit<TMarkup, 'kind'>);
      },
      transformMarkup(markup, input) {
        if (input.handleBehavior !== 'moveEndpoint') {
          return markup;
        }

        if (input.handleId.endsWith('.start')) {
          return createMarkup({
            ...markup,
            start: input.currentPoint,
          } as Omit<TMarkup, 'kind'>);
        }
        if (input.handleId.endsWith('.end')) {
          return createMarkup({
            ...markup,
            end: input.currentPoint,
          } as Omit<TMarkup, 'kind'>);
        }
        return markup;
      },
    },
    pdf: {
      canImport(annotation) {
        const subtype = String(annotation.subtype ?? '').toLowerCase();
        const intent = String(annotation.intent ?? '').toLowerCase();
        return subtype === 'line' && (id === 'arrow' ? intent === 'linearrow' : intent !== 'linearrow');
      },
      import(annotation, context) {
        const points = readLinePoints(annotation.rect);
        return createMarkup({
          id: context.fallbackId,
          pageIndex: context.pageIndex,
          start: points.start,
          end: points.end,
          source: { source: 'imported' },
        } as Omit<TMarkup, 'kind'>);
      },
    },
  };
}

function lineBounds(line: { readonly start: PdfPoint; readonly end: PdfPoint }): Rect {
  const minX = Math.min(line.start.x, line.end.x);
  const minY = Math.min(line.start.y, line.end.y);
  const maxX = Math.max(line.start.x, line.end.x);
  const maxY = Math.max(line.start.y, line.end.y);
  return rect(minX, minY, Math.max(1, maxX - minX), Math.max(1, maxY - minY));
}

function getEndpointHandles(id: 'line' | 'arrow', markup: StraightLineMarkup) {
  return [
    {
      id: `${id}.endpoint.start`,
      componentId: `${id}.body`,
      point: markup.start,
      behavior: 'moveEndpoint' as const,
      cursor: getMoveCursor(),
    },
    {
      id: `${id}.endpoint.end`,
      componentId: `${id}.body`,
      point: markup.end,
      behavior: 'moveEndpoint' as const,
      cursor: getMoveCursor(),
    },
  ];
}

function arrowHeadPoints(start: PdfPoint, end: PdfPoint, length: number, width: number): readonly PdfPoint[] {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const distance = Math.hypot(dx, dy);
  if (distance === 0) {
    return [end, end, end];
  }
  const ux = dx / distance;
  const uy = dy / distance;
  const base = pdfPoint(end.x - ux * length, end.y - uy * length);
  const perpendicular = { x: -uy, y: ux };
  return [
    end,
    pdfPoint(base.x + perpendicular.x * width * 0.5, base.y + perpendicular.y * width * 0.5),
    pdfPoint(base.x - perpendicular.x * width * 0.5, base.y - perpendicular.y * width * 0.5),
  ];
}

function readLinePoints(rectValue: readonly number[] | undefined): { start: PdfPoint; end: PdfPoint } {
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = rectValue ?? [];
  return {
    start: pdfPoint(x1, y1),
    end: pdfPoint(x2, y2),
  };
}
