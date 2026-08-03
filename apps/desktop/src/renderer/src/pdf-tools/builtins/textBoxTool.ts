import { createTextBoxMarkup, pdfPoint, rect as createRect, type PdfPoint, type Rect, type TextBoxMarkup } from '@butter-paper/core';
import {
  createTextBoxDraft,
  resizeRotatedRectFromHandle,
  shouldCommitRectangle,
  textBoxDraftToRect,
  updateTextBoxDraft,
  type RectResizeHandle,
  type TextBoxDraft,
} from '../annotationLifecycle';
import { getAnnotationContentStyle, getAnnotationTextContentStyle } from '../annotationStyles';
import { isPointInRect } from '../hitTesting';
import { getMoveCursor, getResizeHandles, getRotationHandle } from '../interactionChrome';
import { isPointInSelectionChromeBounds } from '../selectionHitZones';
import { splitAnnotationTextLines } from '../textLayout';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

export const TEXT_BOX_TEXT_INSET_PT = 5;
export const TEXT_BOX_FIRST_BASELINE_RATIO = 14.3146 / 12;

export function getTextBoxRenderMetrics(markup: Pick<TextBoxMarkup, 'fontFamily' | 'fontSizePt' | 'lineHeightPt'>) {
  const candidate = {
    id: 'text-metrics',
    pageIndex: 0,
    kind: 'text-box',
    rect: createRect(0, 0, 1, 1),
    text: '',
    ...markup,
  } as TextBoxMarkup;
  const metrics = getAnnotationTextContentStyle(candidate);
  const fontSizePt = metrics.fontSizePt ?? 12;
  return {
    fontFamily: metrics.fontFamily ?? 'Helvetica, Arial, sans-serif',
    fontSizePt,
    lineHeightPt: metrics.lineHeightPt ?? fontSizePt * 1.15,
    textInsetPt: metrics.textInsetPt ?? TEXT_BOX_TEXT_INSET_PT,
    firstBaselineOffsetPt: metrics.firstBaselineOffsetPt ?? fontSizePt * TEXT_BOX_FIRST_BASELINE_RATIO,
  };
}

const TEXT_BOX_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'textColor', label: 'Text', default: '#ff0000' },
    { kind: 'number', key: 'fontSizePt', label: 'Font size', default: 12, min: 6, max: 72, step: 1 },
    { kind: 'select', key: 'fontFamily', label: 'Font', default: 'Helvetica', options: [{ value: 'Helvetica', label: 'Helvetica' }] },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const TEXT_BOX_TOOL_DEFINITION: PdfToolDefinition<TextBoxMarkup, TextBoxDraft> & { readonly id: 'text-box' } = {
  id: 'text-box',
  label: 'Text Box',
  shortcut: 'T',
  category: 'markup',
  cursor: 'default',
  testId: 'tool-text-box',
  implemented: true,
  properties: TEXT_BOX_PROPERTIES,
  defaults: {
    textColor: '#ff0000',
    fontSizePt: 12,
    fontFamily: 'Helvetica',
    opacity: 1,
  },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: markup.rect,
        components: [
          {
            id: 'text-box.body',
            role: 'textBox',
            geometry: { kind: 'textBox', rect: markup.rect, rotation: markup.rotation },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: [
          ...getResizeHandles(markup.rect).map((handle) => ({
            id: `text-box.resize.${handle.kind}`,
            componentId: 'text-box.body',
            point: pdfPoint(handle.x, handle.y),
            behavior: 'resizeSelf' as const,
            cursor: handle.cursor,
          })),
          createRotationHandle(markup),
        ],
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      const isHit = context.transform
        ? isPointInSelectionChromeBounds(point, markup.rect, {
          transform: context.transform,
          rotation: markup.rotation,
          state: 'hovered',
        })
        : isPointInRect(markup.rotation ? rotatePointAroundRectCenter(point, markup.rect, markup.rotation) : point, markup.rect);

      if (!isHit) {
        return null;
      }

      return {
        markupId: markup.id,
        componentId: 'text-box.body',
        region: 'interior',
        bodyDrag: 'moveSelf',
        cursor: getMoveCursor(),
      };
    },
  },
  render: {
    getContentPrimitives(markup) {
      const style = getAnnotationContentStyle(markup);
      const textMetrics = getAnnotationTextContentStyle(markup);
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
          pointerEvents: 'all',
        },
        {
          kind: 'textBox',
          rect: markup.rect,
          text: markup.text,
          textLines: markup.appearanceTextLines ?? splitAnnotationTextLines(markup.text),
          richTextRuns: markup.richTextRuns,
          rotation: markup.rotation,
          style: {
            ...textMetrics,
          },
          pointerEvents: 'none',
        },
      ];
    },
    getDraftPrimitives(draft) {
      return [
        {
          kind: 'textBox',
          rect: textBoxDraftToRect(draft),
          text: '',
          style: {
            textColor: '#ff0000',
            fontFamily: 'Helvetica, Arial, sans-serif',
            fontSizePt: 12,
            lineHeightPt: 13.8,
            textInsetPt: 5,
            firstBaselineOffsetPt: 14.3146,
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
        handles: TEXT_BOX_TOOL_DEFINITION.geometry?.getGeometry(markup, context).handles,
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: textBoxDraftToRect(draft),
          kind: 'child',
        },
        handles: [],
      };
    },
  },
  interaction: {
    placement: 'click',
    createDraft(session) {
      return createTextBoxDraft(session.startPoint);
    },
    updateDraft(draft, point) {
      return updateTextBoxDraft(draft, point);
    },
    commitDraft(draft, context) {
      const rect = textBoxDraftToRect(draft);
      if (!shouldCommitRectangle(rect)) {
        return null;
      }

      return createTextBoxMarkup({
        id: context.createMarkupId('text'),
        pageIndex: context.page.index,
        rect,
        text: '',
        color: '#ff0000',
        fontFamily: 'Helvetica',
        fontSizePt: 12,
        lineHeightPt: 13.8,
        source: { source: 'butter' },
      });
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior === 'rotateSelf') {
        return createTextBoxMarkup({
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

      const { appearanceTextLines: _appearanceTextLines, ...resizableMarkup } = markup;
      return createTextBoxMarkup({
        ...resizableMarkup,
        rect: resizeRotatedRectFromHandle(markup.rect, markup.rotation, handle, input.currentPoint),
      });
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      const intent = String(annotation.intent ?? annotation.fields?.IT ?? '').toLowerCase();
      return subtype === 'freetext' && intent !== 'freetextcallout';
    },
    import(annotation, context) {
      const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = annotation.rect ?? [];
      return createTextBoxMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        rect: createRect(Math.min(x1, x2), Math.min(y1, y2), Math.abs(x2 - x1), Math.abs(y2 - y1)),
        text: String(annotation.fields?.Contents ?? ''),
        color: '#ff0000',
        source: { source: 'imported' },
      });
    },
  },
};

function createRotationHandle(markup: TextBoxMarkup) {
  const handle = getRotationHandle(markup.rect);
  return {
    id: 'text-box.rotate',
    componentId: 'text-box.body',
    point: pdfPoint(handle.x, handle.y),
    behavior: 'rotateSelf' as const,
    cursor: handle.cursor,
  };
}

function rotationFromDrag(markup: TextBoxMarkup, startPoint: PdfPoint, currentPoint: PdfPoint): number {
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
  const handle = handleId.replace('text-box.resize.', '');
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

function rotatePointAroundRectCenter(point: PdfPoint, box: Rect, degrees: number): PdfPoint {
  const center = {
    x: box.x + box.width * 0.5,
    y: box.y + box.height * 0.5,
  };
  const radians = (degrees * Math.PI) / 180;
  const dx = point.x - center.x;
  const dy = point.y - center.y;

  return pdfPoint(
    center.x + dx * Math.cos(radians) - dy * Math.sin(radians),
    center.y + dx * Math.sin(radians) + dy * Math.cos(radians),
  );
}
