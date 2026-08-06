import { createCalloutMarkup, pdfPoint, rect, rectFromPoints, translatePoint, translateRect, type CalloutMarkup, type PdfPoint, type Rect } from '@butter-paper/core';
import {
  createLineDraft,
  resizeRectFromHandle,
  shouldCommitLine,
  updateLineDraft,
  type LineDraft,
  type RectResizeHandle,
} from '../annotationLifecycle';
import { getAnnotationContentStyle, getVerticallyCenteredAnnotationTextContentStyle } from '../annotationStyles';
import { isPointInRect, isPointNearPolyline } from '../hitTesting';
import { getMoveCursor, getResizeHandles } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const DEFAULT_TEXT = 'Callout';
const DEFAULT_TEXT_BOX_WIDTH = 150;
const DEFAULT_TEXT_BOX_HEIGHT = 44;

const CALLOUT_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'color', key: 'textColor', label: 'Text', default: '#ff0000' },
    { kind: 'number', key: 'fontSizePt', label: 'Font size', default: 12, min: 6, max: 72, step: 1 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const CALLOUT_TOOL_DEFINITION: PdfToolDefinition<CalloutMarkup, LineDraft> & { readonly id: 'callout' } = {
  id: 'callout',
  label: 'Callout',
  shortcut: 'Q',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-callout',
  implemented: true,
  properties: CALLOUT_PROPERTIES,
  defaults: {
    strokeColor: '#ff0000',
    textColor: '#ff0000',
    fontSizePt: 12,
    opacity: 1,
  },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: calloutBounds(markup),
        components: [
          {
            id: 'callout.textBox',
            role: 'textBox',
            geometry: { kind: 'textBox', rect: markup.textBox },
            bodyDrag: 'moveSelf',
          },
          {
            id: 'callout.leader',
            role: 'leader',
            geometry: { kind: 'polyline', points: markup.leader.points },
            bodyDrag: 'moveGroup',
          },
        ],
        handles: calloutHandles(markup),
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      if (isPointInRect(point, markup.textBox)) {
        return {
          markupId: markup.id,
          componentId: 'callout.textBox',
          region: 'interior',
          bodyDrag: 'moveSelf',
          cursor: getMoveCursor(),
        };
      }

      if (isPointNearPolyline(point, markup.leader.points, context.tolerance)) {
        return {
          markupId: markup.id,
          componentId: 'callout.leader',
          region: 'leader',
          bodyDrag: 'moveGroup',
          cursor: getMoveCursor(),
        };
      }

      return null;
    },
  },
  render: {
    getContentPrimitives(markup) {
      const style = getAnnotationContentStyle(markup);
      const points = markup.leader.points;
      const tip = points[0] ?? pdfPoint(markup.textBox.x, markup.textBox.y);
      const afterTip = points[1] ?? tip;
      return [
        {
          kind: 'polyline',
          points,
          style: {
            stroke: style.stroke,
            fill: 'none',
            strokeWidth: style.strokeWidth,
            opacity: style.opacity,
          },
          pointerEvents: 'visibleStroke',
        },
        {
          kind: 'polyline',
          points: openArrowHeadPoints(afterTip, tip, 10, 7),
          style: {
            stroke: style.stroke,
            fill: 'none',
            strokeWidth: style.strokeWidth,
            opacity: style.opacity,
          },
          pointerEvents: 'none',
        },
        {
          kind: 'textBox',
          rect: markup.textBox,
          text: markup.text,
          style: getVerticallyCenteredAnnotationTextContentStyle(markup),
          pointerEvents: 'all',
        },
      ];
    },
    getDraftPrimitives(draft) {
      const markup = draftToCalloutPreview(draft, 'callout-draft', 0);
      return CALLOUT_TOOL_DEFINITION.render?.getContentPrimitives(markup, { page: { id: 'draft', index: 0, size: { width: 0, height: 0 }, rotation: 0 }, phase: 'draft' }) ?? [];
    },
  },
  selection: {
    getSelectionChrome(markup): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: calloutBounds(markup),
          kind: 'group',
        },
        handles: calloutHandles(markup),
        controlPaths: [{ id: 'callout.leaderPath', points: markup.leader.points, closed: false }],
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      const markup = draftToCalloutPreview(draft, 'callout-draft', 0);
      return {
        bounds: {
          rect: calloutBounds(markup),
          kind: 'group',
        },
        handles: [],
        controlPaths: [{ id: 'callout.draftLeaderPath', points: markup.leader.points, closed: false }],
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

      return draftToCalloutPreview(draft, context.createMarkupId('callout'), context.page.index);
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior === 'resizeSelf' && input.handleId.startsWith('callout.textBox.resize.')) {
        const handle = resizeHandleFromId(input.handleId);
        if (!handle) {
          return markup;
        }
        const textBox = resizeRectFromHandle(markup.textBox, handle, input.currentPoint);
        return createCalloutMarkup({
          ...markup,
          textBox,
          leader: { points: leaderPointsForResizedTextBox(markup.leader.points, markup.textBox, textBox) },
        });
      }

      if (input.handleBehavior === 'moveEndpoint' || input.handleBehavior === 'moveKnee') {
        const pointIndex = leaderPointIndexFromHandle(input.handleId, markup.leader.points.length);
        if (pointIndex === null) {
          return markup;
        }
        return createCalloutMarkup({
          ...markup,
          leader: {
            points: markup.leader.points.map((point, index) => index === pointIndex ? input.currentPoint : point),
          },
        });
      }

      return markup;
    },
    dragMarkup(markup, input) {
      if (input.bodyDrag === 'moveGroup') {
        return createCalloutMarkup({
          ...markup,
          textBox: translateRect(markup.textBox, input.delta),
          leader: { points: markup.leader.points.map((point) => translatePoint(point, input.delta)) },
        });
      }

      if (input.componentId === 'callout.textBox') {
        return createCalloutMarkup({
          ...markup,
          textBox: translateRect(markup.textBox, input.delta),
          leader: {
            points: markup.leader.points.map((point, index) => index === markup.leader.points.length - 1 ? translatePoint(point, input.delta) : point),
          },
        });
      }

      return markup;
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      const intent = String(annotation.intent ?? annotation.fields?.IT ?? '').toLowerCase();
      return subtype === 'freetext' && intent === 'freetextcallout';
    },
    import(annotation, context) {
      const box = rectFromPdfRect(annotation.rect);
      return createCalloutMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        leader: { points: readLeaderPoints(annotation.fields?.CL ?? annotation.fields?.calloutLine, box) },
        textBox: box,
        text: String(annotation.fields?.Contents ?? DEFAULT_TEXT),
        color: '#ff0000',
        source: { source: 'imported' },
      });
    },
  },
};

function draftToCalloutPreview(draft: LineDraft, id: string, pageIndex: number): CalloutMarkup {
  const textBox = calloutTextBoxForPoint(draft.current);
  const connection = pdfPoint(textBox.x, textBox.y + textBox.height * 0.5);
  const knee = pdfPoint((connection.x + draft.start.x) * 0.5, connection.y);
  return createCalloutMarkup({
    id,
    pageIndex,
    leader: { points: [draft.start, knee, connection] },
    textBox,
    text: DEFAULT_TEXT,
    color: '#ff0000',
    source: { source: 'butter' },
  });
}

function calloutTextBoxForPoint(point: PdfPoint): Rect {
  return rect(point.x, point.y - DEFAULT_TEXT_BOX_HEIGHT * 0.5, DEFAULT_TEXT_BOX_WIDTH, DEFAULT_TEXT_BOX_HEIGHT);
}

function calloutBounds(markup: CalloutMarkup): Rect {
  const points = [
    ...markup.leader.points,
    pdfPoint(markup.textBox.x, markup.textBox.y),
    pdfPoint(markup.textBox.x + markup.textBox.width, markup.textBox.y + markup.textBox.height),
  ];
  return pointsBounds(points);
}

function calloutHandles(markup: CalloutMarkup) {
  const resizeHandles = getResizeHandles(markup.textBox).map((handle) => ({
    id: `callout.textBox.resize.${handle.kind}`,
    componentId: 'callout.textBox',
    point: pdfPoint(handle.x, handle.y),
    behavior: 'resizeSelf' as const,
    cursor: handle.cursor,
  }));
  const leaderHandles = markup.leader.points.map((point, index) => ({
    id: index === 0
      ? 'callout.leader.tip'
      : index === markup.leader.points.length - 1
        ? 'callout.leader.connection'
        : `callout.leader.knee.${index}`,
    componentId: 'callout.leader',
    point,
    behavior: index === 0 || index === markup.leader.points.length - 1 ? 'moveEndpoint' as const : 'moveKnee' as const,
    cursor: getMoveCursor(),
  }));
  return [...resizeHandles, ...leaderHandles];
}

function leaderPointIndexFromHandle(handleId: string, pointCount: number): number | null {
  if (handleId === 'callout.leader.connection') {
    return pointCount - 1;
  }
  if (handleId === 'callout.leader.tip') {
    return 0;
  }
  const index = Number(handleId.replace('callout.leader.knee.', ''));
  return Number.isInteger(index) && index > 0 && index < pointCount - 1 ? index : null;
}

function resizeHandleFromId(handleId: string): RectResizeHandle | null {
  const handle = handleId.replace('callout.textBox.resize.', '');
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

function pointsBounds(points: readonly PdfPoint[]): Rect {
  const minX = Math.min(...points.map((point) => point.x));
  const minY = Math.min(...points.map((point) => point.y));
  const maxX = Math.max(...points.map((point) => point.x));
  const maxY = Math.max(...points.map((point) => point.y));
  return rect(minX, minY, Math.max(1, maxX - minX), Math.max(1, maxY - minY));
}

function leaderPointsForResizedTextBox(
  points: readonly PdfPoint[],
  previousTextBox: Rect,
  textBox: Rect,
): readonly PdfPoint[] {
  const connection = points[points.length - 1];
  if (!connection) {
    return points;
  }
  const attachedToLeft = Math.abs(connection.x - previousTextBox.x)
    <= Math.abs(connection.x - (previousTextBox.x + previousTextBox.width));
  const nextConnection = pdfPoint(
    attachedToLeft ? textBox.x : textBox.x + textBox.width,
    textBox.y + textBox.height * 0.5,
  );
  return points.map((point, index) => index === points.length - 1 ? nextConnection : point);
}

function openArrowHeadPoints(start: PdfPoint, end: PdfPoint, length: number, width: number): readonly PdfPoint[] {
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
    pdfPoint(base.x + perpendicular.x * width * 0.5, base.y + perpendicular.y * width * 0.5),
    end,
    pdfPoint(base.x - perpendicular.x * width * 0.5, base.y - perpendicular.y * width * 0.5),
  ];
}

function rectFromPdfRect(rectValue: readonly number[] | undefined): Rect {
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = rectValue ?? [];
  return rectFromPoints(pdfPoint(x1, y1), pdfPoint(x2, y2));
}

function readLeaderPoints(value: unknown, fallbackBox: Rect): readonly PdfPoint[] {
  if (Array.isArray(value) && value.length >= 4) {
    const points: PdfPoint[] = [];
    for (let index = 0; index < value.length - 1; index += 2) {
      points.push(pdfPoint(Number(value[index]), Number(value[index + 1])));
    }
    return points;
  }
  return [
    pdfPoint(fallbackBox.x, fallbackBox.y + fallbackBox.height * 0.5),
    pdfPoint(fallbackBox.x - 60, fallbackBox.y - 30),
  ];
}
