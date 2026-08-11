import { createImageMarkup, pdfPoint, rect as createRect, type ImageMarkup } from '@butter-paper/core';
import {
  createRectangleDraft,
  resizeRotatedRectFromHandle,
  resizeRotatedRectFromHandlePreservingAspectRatio,
  shouldCommitRectangle,
  rectangleDraftToRect,
  updateRectangleDraft,
  type RectResizeHandle,
  type RectangleDraft,
} from '../annotationLifecycle';
import { isPointInRect } from '../hitTesting';
import { getAnnotationContentStyle } from '../annotationStyles';
import { getMoveCursor, getResizeHandles, getRotationHandle } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

export const DEFAULT_IMAGE_DATA_URL = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAAAoCAYAAABOzvzpAAAABHNCSVQICAgIfAhkiAAAAAFzUkdCAK7OHOkAAAIpSURBVGiB7Zi/TxNhHMafO0vvWiBBlIC2ooi1xkT5MZFAHEwMJCwMpotuDqZx4R9QEyWyOJkuYhxcTAcHEkYSEjc3fqXByo9AyJUCRh3s3XE1d06uvO8dtd8v6T3z8z7vk8/dffO+p6QeHXpoYKnUBagVAqAuQK0QAHUBaoUAqAtQKwRAXYBaSumXF54EG1khAOoC1IpQF5CR67n48nsFBWsT36wdFO1tKFCQjl3Gdf0KbsauYqjlNlTF//NkPwRLziFeGm+xaq4f67sVT+Fp4jEuRjt85QsBxO7f8xUokvVpXto79/Mz3pQ/wnJtKX9M1THZ9QDjZ+9I78H2E5j9sYDXex98rbFcG9Ol96h6fzDRfldqDcshaDgHyO3nA6/P7edhOAdSXnYAXHiYMmZgu0eBM2z3CFPGDFyIxxs7AGvWlnDgyWjVXMeatSX0sQNQMDfqmsUOwGLla82yCtam0MMOgExp6SzzFAKopRyvKvSwA5DSu+uaxQ7AtRoCkMliB6Dh34CR1gH06skT5/RoCYy0Dgp97ADoqoZniSyalODXlCYlgheXnkBXo0IvOwAA0Ksnke3MBF6f7cygR0tIedneBjPnRqGpUeTKeenrcMuZOCa7HmKsbVh6H/Y/RPaq3/HKeCc8IQ4038DzZBbnI22+8tkD+Kddp4ylShFLZhHLlSI8eOhvTqMvnkZ/PI1u7UKg3FMD4H+J5RCsp0IA1AWoFQKgLkCtv9cipgMsRDYAAAAAAElFTkSuQmCC';

const IMAGE_PROPERTIES = {
  properties: [
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const IMAGE_TOOL_DEFINITION: PdfToolDefinition<ImageMarkup, RectangleDraft> & { readonly id: 'image' } = {
  id: 'image',
  label: 'Insert Image',
  shortcut: 'I',
  category: 'media',
  cursor: 'crosshair',
  testId: 'tool-image',
  implemented: true,
  properties: IMAGE_PROPERTIES,
  defaults: { opacity: 1 },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: markup.rect,
        components: [
          {
            id: 'image.body',
            role: 'media',
            geometry: { kind: 'rect', rect: markup.rect, rotation: markup.rotation },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: [
          ...getResizeHandles(markup.rect)
            .filter((handle) => !markup.aspectRatioLocked || handle.kind.length === 2)
            .map((handle) => ({
              id: `image.resize.${handle.kind}`,
              componentId: 'image.body',
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
        componentId: 'image.body',
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
        handles: IMAGE_TOOL_DEFINITION.geometry?.getGeometry(markup, context).handles,
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
      return createImageMarkup({
        id: context.createMarkupId('image'),
        pageIndex: context.page.index,
        rect,
        dataUrl: DEFAULT_IMAGE_DATA_URL,
        mimeType: 'image/png',
        source: { source: 'butter' },
      });
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior === 'rotateSelf') {
        return createImageMarkup({
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
      const resizedRect = markup.aspectRatioLocked
        ? resizeRotatedRectFromHandlePreservingAspectRatio(markup.rect, markup.rotation, handle, input.currentPoint)
        : resizeRotatedRectFromHandle(markup.rect, markup.rotation, handle, input.currentPoint);
      return createImageMarkup({ ...markup, rect: resizedRect });
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      const subject = String(annotation.subject ?? annotation.fields?.Subj ?? '').toLowerCase();
      const intent = String(annotation.intent ?? annotation.fields?.IT ?? '').toLowerCase();
      return (subtype === 'square' || subtype === 'rect') && (subject === 'image' || intent === 'squareimage');
    },
    import(annotation, context) {
      const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = annotation.rect ?? [];
      return createImageMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        rect: createRect(Math.min(x1, x2), Math.min(y1, y2), Math.abs(x2 - x1), Math.abs(y2 - y1)),
        dataUrl: String(annotation.fields?.BPImageData ?? DEFAULT_IMAGE_DATA_URL),
        mimeType: String(annotation.fields?.BPImageMimeType ?? 'image/png') === 'image/jpeg' ? 'image/jpeg' : 'image/png',
        aspectRatioLocked: annotation.fields?.BPAspectRatioLocked === true,
        source: { source: 'imported' },
      });
    },
  },
};

function createRotationHandle(markup: ImageMarkup) {
  const handle = getRotationHandle(markup.rect);
  return {
    id: 'image.rotate',
    componentId: 'image.body',
    point: pdfPoint(handle.x, handle.y),
    behavior: 'rotateSelf' as const,
    cursor: handle.cursor,
  };
}

function rotationFromDrag(markup: ImageMarkup, startPoint: { readonly x: number; readonly y: number }, currentPoint: { readonly x: number; readonly y: number }): number {
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
  const handle = handleId.replace('image.resize.', '');
  if (handle === 'nw' || handle === 'n' || handle === 'ne' || handle === 'e' || handle === 'se' || handle === 's' || handle === 'sw' || handle === 'w') {
    return handle;
  }
  return null;
}

function unrotatePoint(point: { readonly x: number; readonly y: number }, markup: ImageMarkup) {
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
