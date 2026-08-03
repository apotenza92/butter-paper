import { createDimensionMarkup, pdfPoint, rect, resolveMarkupAppearance, translatePoint, type DimensionMarkup, type PdfPoint, type Rect } from '@butter-paper/core';
import {
  createLineDraft,
  shouldCommitLine,
  updateLineDraft,
  type LineDraft,
} from '../annotationLifecycle';
import { annotationFontCssFamily, getAnnotationContentStyle, getAnnotationTextContentStyle } from '../annotationStyles';
import { isPointInRect, isPointNearLineSegment } from '../hitTesting';
import { getMoveCursor } from '../interactionChrome';
import { measureAnnotationText } from '../textLayout';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const DEFAULT_DIMENSION_TEXT = 'Dimension';
const DEFAULT_DIMENSION_OFFSET = 24;
const EXTENSION_OVERHANG = 4;
const CAPTION_LINE_GAP = 4;

const DIMENSION_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'color', key: 'textColor', label: 'Text', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'number', key: 'fontSizePt', label: 'Font size', default: 12, min: 6, max: 72, step: 1 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const DIMENSION_TOOL_DEFINITION: PdfToolDefinition<DimensionMarkup, LineDraft> & { readonly id: 'dimension' } = {
  id: 'dimension',
  label: 'Dimension',
  shortcut: 'Shift+L',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-dimension',
  implemented: true,
  properties: DIMENSION_PROPERTIES,
  defaults: {
    strokeColor: '#ff0000',
    textColor: '#ff0000',
    strokeWidthPt: 1,
    fontSizePt: 12,
    opacity: 1,
  },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: dimensionBounds(markup),
        components: [
          {
            id: 'dimension.body',
            role: 'measurement',
            geometry: { kind: 'polyline', points: dimensionHitPath(markup) },
            bodyDrag: 'moveSelf',
          },
          {
            id: 'dimension.caption',
            role: 'textBox',
            geometry: { kind: 'textBox', rect: dimensionCaptionRect(markup) },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: dimensionHandles(markup),
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      if (isPointInRect(point, dimensionCaptionRect(markup))) {
        return {
          markupId: markup.id,
          componentId: 'dimension.caption',
          region: 'interior',
          bodyDrag: 'moveSelf',
          cursor: getMoveCursor(),
        };
      }

      const parts = dimensionSegments(markup);
      if (parts.some((segment) => isPointNearLineSegment(point, segment.start, segment.end, context.tolerance))) {
        return {
          markupId: markup.id,
          componentId: 'dimension.body',
          region: 'edge',
          bodyDrag: 'moveSelf',
          cursor: getMoveCursor(),
        };
      }

      return null;
    },
  },
  render: {
    getContentPrimitives(markup) {
      const style = getAnnotationContentStyle(markup);
      const geometry = dimensionGeometry(markup);
      const strokeStyle = {
        stroke: style.stroke,
        fill: 'none',
        strokeWidth: style.strokeWidth,
        opacity: style.opacity,
      };
      return [
        ...geometry.dimensionLineSegments.map((segment) => ({
          kind: 'polyline',
          points: [segment.start, segment.end],
          style: strokeStyle,
          pointerEvents: 'visibleStroke',
        } as const)),
        {
          kind: 'polyline',
          points: [geometry.extensionStartOuter, markup.start, geometry.extensionStartInner],
          style: strokeStyle,
          pointerEvents: 'visibleStroke',
        },
        {
          kind: 'polyline',
          points: [geometry.extensionEndOuter, markup.end, geometry.extensionEndInner],
          style: strokeStyle,
          pointerEvents: 'visibleStroke',
        },
        {
          kind: 'polygon',
          points: closedArrowHeadPoints(geometry.dimensionEnd, geometry.dimensionStart, 9, 7),
          style: { ...strokeStyle, fill: style.stroke },
          pointerEvents: 'none',
        },
        {
          kind: 'polygon',
          points: closedArrowHeadPoints(geometry.dimensionStart, geometry.dimensionEnd, 9, 7),
          style: { ...strokeStyle, fill: style.stroke },
          pointerEvents: 'none',
        },
        {
          kind: 'textBox',
          rect: dimensionCaptionRect(markup),
          text: markup.text,
          style: getAnnotationTextContentStyle(markup, 13 / 12),
          pointerEvents: 'all',
        },
      ];
    },
    getDraftPrimitives(draft) {
      const markup = draftToDimension(draft, 'dimension-draft', 0);
      return DIMENSION_TOOL_DEFINITION.render?.getContentPrimitives(markup, { page: { id: 'draft', index: 0, size: { width: 0, height: 0 }, rotation: 0 }, phase: 'draft' }) ?? [];
    },
  },
  selection: {
    getSelectionChrome(markup): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: dimensionBounds(markup),
          kind: 'group',
        },
        handles: dimensionHandles(markup),
        controlPaths: [{ id: 'dimension.path', points: dimensionHitPath(markup), closed: false }],
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      const markup = draftToDimension(draft, 'dimension-draft', 0);
      return {
        bounds: {
          rect: dimensionBounds(markup),
          kind: 'group',
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

      return draftToDimension(draft, context.createMarkupId('dimension'), context.page.index);
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior === 'moveEndpoint') {
        if (input.handleId === 'dimension.endpoint.start') {
          return createDimensionMarkup({ ...markup, start: input.currentPoint });
        }
        if (input.handleId === 'dimension.endpoint.end') {
          return createDimensionMarkup({ ...markup, end: input.currentPoint });
        }
      }

      if (input.handleBehavior === 'moveKnee' && input.handleId === 'dimension.offset') {
        const basis = dimensionBasis(markup.start, markup.end);
        const baselineCenter = midpoint(markup.start, markup.end);
        const offset = subtractPoint(input.currentPoint, baselineCenter);
        return createDimensionMarkup({
          ...markup,
          dimensionLineOffset: dotPoint(offset, basis.normal),
        });
      }

      return markup;
    },
    dragMarkup(markup, input) {
      if (input.bodyDrag !== 'moveSelf') {
        return markup;
      }
      return createDimensionMarkup({
        ...markup,
        start: translatePoint(markup.start, input.delta),
        end: translatePoint(markup.end, input.delta),
      });
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      const intent = String(annotation.intent ?? annotation.fields?.IT ?? '').toLowerCase();
      return subtype === 'line' && intent === 'linedimension';
    },
    import(annotation, context) {
      const line = readLinePoints(annotation.fields?.L, annotation.rect);
      const lineLeader = readLineLeader(annotation.fields?.LL);
      const caption = String(annotation.fields?.Cap ?? annotation.fields?.Contents ?? DEFAULT_DIMENSION_TEXT);
      return createDimensionMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        start: line.start,
        end: line.end,
        dimensionLineOffset: lineLeader ?? defaultOffsetForDirection(line.start, line.end),
        text: caption,
        color: '#ff0000',
        source: { source: 'imported' },
      });
    },
  },
};

function draftToDimension(draft: LineDraft, id: string, pageIndex: number): DimensionMarkup {
  return createDimensionMarkup({
    id,
    pageIndex,
    start: draft.start,
    end: draft.current,
    dimensionLineOffset: defaultOffsetForDirection(draft.start, draft.current),
    text: DEFAULT_DIMENSION_TEXT,
    color: '#ff0000',
    source: { source: 'butter' },
  });
}

function defaultOffsetForDirection(start: PdfPoint, end: PdfPoint): number {
  return end.x >= start.x ? DEFAULT_DIMENSION_OFFSET : -DEFAULT_DIMENSION_OFFSET;
}

function dimensionGeometry(markup: DimensionMarkup) {
  const basis = dimensionBasis(markup.start, markup.end);
  const offset = scalePoint(basis.normal, markup.dimensionLineOffset);
  const dimensionStart = translatePoint(markup.start, offset);
  const dimensionEnd = translatePoint(markup.end, offset);
  const sign = markup.dimensionLineOffset >= 0 ? 1 : -1;
  const extensionStartInner = translatePoint(markup.start, scalePoint(basis.normal, sign * 2));
  const extensionEndInner = translatePoint(markup.end, scalePoint(basis.normal, sign * 2));
  const extensionStartOuter = translatePoint(dimensionStart, scalePoint(basis.normal, sign * EXTENSION_OVERHANG));
  const extensionEndOuter = translatePoint(dimensionEnd, scalePoint(basis.normal, sign * EXTENSION_OVERHANG));
  const captionCenter = midpoint(dimensionStart, dimensionEnd);
  const dimensionLineSegments = splitLineAroundCaption(dimensionStart, dimensionEnd, captionCenter, basis, dimensionCaptionSize(markup).width);
  return {
    dimensionStart,
    dimensionEnd,
    dimensionLineSegments,
    extensionStartInner,
    extensionEndInner,
    extensionStartOuter,
    extensionEndOuter,
    captionCenter,
  };
}

function dimensionSegments(markup: DimensionMarkup) {
  const geometry = dimensionGeometry(markup);
  return [
    ...geometry.dimensionLineSegments,
    { start: geometry.extensionStartInner, end: geometry.extensionStartOuter },
    { start: geometry.extensionEndInner, end: geometry.extensionEndOuter },
  ];
}

function dimensionHitPath(markup: DimensionMarkup): readonly PdfPoint[] {
  const geometry = dimensionGeometry(markup);
  return [
    geometry.extensionStartOuter,
    geometry.dimensionStart,
    geometry.dimensionEnd,
    geometry.extensionEndOuter,
  ];
}

export function dimensionCaptionRect(markup: DimensionMarkup): Rect {
  const basis = dimensionBasis(markup.start, markup.end);
  const center = midpoint(
    translatePoint(markup.start, scalePoint(basis.normal, markup.dimensionLineOffset)),
    translatePoint(markup.end, scalePoint(basis.normal, markup.dimensionLineOffset)),
  );
  const size = dimensionCaptionSize(markup);
  return rect(center.x - size.width * 0.5, center.y - size.height * 0.5, size.width, size.height);
}

function dimensionCaptionSize(markup: DimensionMarkup): { width: number; height: number } {
  const text = resolveMarkupAppearance(markup).text!;
  const measuredWidth = measureAnnotationText(markup.text, {
    fontFamily: annotationFontCssFamily(text.fontId),
    fontSizePt: text.fontSizePt,
  });
  return {
    width: Math.max(text.fontSizePt, measuredWidth + text.insetPt * 2),
    height: Math.max(text.fontSizePt, text.lineHeightPt),
  };
}

function dimensionBounds(markup: DimensionMarkup): Rect {
  const caption = dimensionCaptionRect(markup);
  const points = [
    markup.start,
    markup.end,
    ...dimensionHitPath(markup),
    pdfPoint(caption.x, caption.y),
    pdfPoint(caption.x + caption.width, caption.y + caption.height),
  ];
  const minX = Math.min(...points.map((point) => point.x));
  const minY = Math.min(...points.map((point) => point.y));
  const maxX = Math.max(...points.map((point) => point.x));
  const maxY = Math.max(...points.map((point) => point.y));
  return rect(minX, minY, Math.max(1, maxX - minX), Math.max(1, maxY - minY));
}

function dimensionHandles(markup: DimensionMarkup) {
  const geometry = dimensionGeometry(markup);
  return [
    {
      id: 'dimension.endpoint.start',
      componentId: 'dimension.body',
      point: markup.start,
      behavior: 'moveEndpoint' as const,
      cursor: getMoveCursor(),
    },
    {
      id: 'dimension.endpoint.end',
      componentId: 'dimension.body',
      point: markup.end,
      behavior: 'moveEndpoint' as const,
      cursor: getMoveCursor(),
    },
    {
      id: 'dimension.offset',
      componentId: 'dimension.body',
      point: geometry.captionCenter,
      behavior: 'moveKnee' as const,
      cursor: getMoveCursor(),
    },
  ];
}

function dimensionBasis(start: PdfPoint, end: PdfPoint): { unit: PdfPoint; normal: PdfPoint; length: number } {
  const delta = subtractPoint(end, start);
  const length = Math.hypot(delta.x, delta.y);
  if (length === 0) {
    return {
      unit: pdfPoint(1, 0),
      normal: pdfPoint(0, 1),
      length: 0,
    };
  }
  const unit = pdfPoint(delta.x / length, delta.y / length);
  return {
    unit,
    normal: pdfPoint(-unit.y, unit.x),
    length,
  };
}

function splitLineAroundCaption(start: PdfPoint, end: PdfPoint, center: PdfPoint, basis: { unit: PdfPoint; length: number }, captionWidth: number): readonly { start: PdfPoint; end: PdfPoint }[] {
  const halfGap = Math.min((captionWidth * 0.5) + CAPTION_LINE_GAP, Math.max(0, (basis.length * 0.5) - 1));
  if (halfGap <= 0) {
    return [{ start, end }];
  }
  const gapStart = translatePoint(center, scalePoint(basis.unit, -halfGap));
  const gapEnd = translatePoint(center, scalePoint(basis.unit, halfGap));
  return [
    { start, end: gapStart },
    { start: gapEnd, end },
  ];
}

function midpoint(start: PdfPoint, end: PdfPoint): PdfPoint {
  return pdfPoint((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
}

function subtractPoint(point: PdfPoint, origin: PdfPoint): PdfPoint {
  return pdfPoint(point.x - origin.x, point.y - origin.y);
}

function scalePoint(point: PdfPoint, scale: number): PdfPoint {
  return pdfPoint(point.x * scale, point.y * scale);
}

function dotPoint(first: PdfPoint, second: PdfPoint): number {
  return first.x * second.x + first.y * second.y;
}

function closedArrowHeadPoints(tip: PdfPoint, tail: PdfPoint, length: number, width: number): readonly PdfPoint[] {
  const dx = tail.x - tip.x;
  const dy = tail.y - tip.y;
  const distance = Math.hypot(dx, dy);
  if (distance === 0) {
    return [tip, tip, tip];
  }
  const ux = dx / distance;
  const uy = dy / distance;
  const base = pdfPoint(tip.x + ux * length, tip.y + uy * length);
  const perpendicular = { x: -uy, y: ux };
  return [
    tip,
    pdfPoint(base.x + perpendicular.x * width * 0.5, base.y + perpendicular.y * width * 0.5),
    pdfPoint(base.x - perpendicular.x * width * 0.5, base.y - perpendicular.y * width * 0.5),
  ];
}

function rectToPdfArray(box: Rect): readonly number[] {
  return [box.x, box.y, box.x + box.width, box.y + box.height];
}

function readLinePoints(value: unknown, fallbackRect: readonly number[] | undefined): { start: PdfPoint; end: PdfPoint } {
  if (Array.isArray(value) && value.length >= 4) {
    return {
      start: pdfPoint(Number(value[0]), Number(value[1])),
      end: pdfPoint(Number(value[2]), Number(value[3])),
    };
  }
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = fallbackRect ?? [];
  return {
    start: pdfPoint(x1, y1),
    end: pdfPoint(x2, y2),
  };
}

function readLineLeader(value: unknown): number | undefined {
  const numberValue = Number(value);
  return Number.isFinite(numberValue) ? numberValue : undefined;
}
