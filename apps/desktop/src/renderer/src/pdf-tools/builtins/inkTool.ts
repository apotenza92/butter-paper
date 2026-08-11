import { createHighlightMarkup, createPenMarkup, pdfPoint, rect, type HighlightMarkup, type PenMarkup, type PdfPoint, type Rect } from '@butter-paper/core';
import {
  createInkDraft,
  hasExceededDragThreshold,
  updateInkDraft,
  type InkDraft,
} from '../annotationLifecycle';
import { getAnnotationContentStyle } from '../annotationStyles';
import { isPointNearInkPaths } from '../hitTesting';
import { getMoveCursor } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

type InkMarkup = PenMarkup | HighlightMarkup;

const PEN_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
    { kind: 'boolean', key: 'smoothCurves', label: 'Smooth curves', default: true },
  ],
} as const;

const HIGHLIGHT_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ffff00' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 12, min: 1, max: 48, step: 1 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const PEN_TOOL_DEFINITION: PdfToolDefinition<PenMarkup, InkDraft> & { readonly id: 'pen' } = createInkToolDefinition({
  id: 'pen',
  label: 'Pen',
  shortcut: 'P',
  testId: 'tool-pen',
  properties: PEN_PROPERTIES,
  defaultStroke: '#ff0000',
  defaultStrokeWidth: 1,
  createMarkup(params) {
    return createPenMarkup(params);
  },
});

export const HIGHLIGHT_TOOL_DEFINITION: PdfToolDefinition<HighlightMarkup, InkDraft> & { readonly id: 'highlight' } = createInkToolDefinition({
  id: 'highlight',
  label: 'Highlight',
  shortcut: 'H',
  testId: 'tool-highlight',
  properties: HIGHLIGHT_PROPERTIES,
  defaultStroke: '#ffff00',
  defaultStrokeWidth: 12,
  createMarkup(params) {
    return createHighlightMarkup({ ...params, blendMode: 'multiply' });
  },
});

function createInkToolDefinition<TMarkup extends InkMarkup, TId extends 'pen' | 'highlight'>({
  id,
  label,
  shortcut,
  testId,
  properties,
  defaultStroke,
  defaultStrokeWidth,
  createMarkup,
}: {
  readonly id: TId;
  readonly label: string;
  readonly shortcut: string;
  readonly testId: string;
  readonly properties: typeof PEN_PROPERTIES | typeof HIGHLIGHT_PROPERTIES;
  readonly defaultStroke: string;
  readonly defaultStrokeWidth: number;
  readonly createMarkup: (params: Omit<TMarkup, 'kind'>) => TMarkup;
}): PdfToolDefinition<TMarkup, InkDraft> & { readonly id: TId } {
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
      strokeColor: defaultStroke,
      strokeWidthPt: defaultStrokeWidth,
      opacity: 1,
      ...(id === 'pen' ? { smoothCurves: true } : {}),
      ...(id === 'highlight' ? { blendMode: 'multiply' } : {}),
    },
    geometry: {
      getGeometry(markup): ToolGeometryDescriptor {
        const style = getAnnotationContentStyle(markup);
        return {
          bounds: inkBounds(markup.paths, style.strokeWidth),
          components: [
            {
              id: `${id}.body`,
              role: 'shape',
              geometry: { kind: 'generatedPath', controlPath: markup.paths[0] ?? [], closed: false, lineType: { id: 'ink', options: { strokeWidth: style.strokeWidth } } },
              bodyDrag: 'moveSelf',
            },
          ],
        };
      },
      hitTest(markup, point, context): ToolHit | null {
        const style = getAnnotationContentStyle(markup);
        if (!isPointNearInkPaths(point, markup.paths, Math.max(context.tolerance, style.strokeWidth * 0.5))) {
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
        return markup.paths.map((path) => ({
          ...(markup.kind === 'pen' && markup.smoothCurves
            ? { kind: 'path' as const, d: interpolatingInkPath(path) }
            : { kind: 'polyline' as const, points: path }),
          style: {
            stroke: style.stroke,
            fill: 'none',
            strokeWidth: style.strokeWidth,
            opacity: style.opacity,
            lineCap: 'round' as const,
            lineJoin: 'round' as const,
            blendMode: style.blendMode,
          },
          pointerEvents: 'visibleStroke' as const,
        }));
      },
      getDraftPrimitives(draft) {
        return [
          {
            kind: 'polyline' as const,
            points: draft.points,
            style: {
              stroke: defaultStroke,
              fill: 'none',
              strokeWidth: defaultStrokeWidth,
              opacity: 1,
              lineCap: 'round' as const,
              lineJoin: 'round' as const,
              blendMode: id === 'highlight' ? 'multiply' as const : undefined,
            },
            pointerEvents: 'none' as const,
          },
        ];
      },
    },
    selection: {
      getSelectionChrome(markup): SelectionChromeDescriptor {
        const style = getAnnotationContentStyle(markup);
        return {
          bounds: {
            rect: inkBounds(markup.paths, style.strokeWidth),
            kind: 'child',
          },
          handles: [],
        };
      },
      getDraftChrome(draft): SelectionChromeDescriptor {
        return {
          bounds: {
            rect: inkBounds([draft.points], defaultStrokeWidth),
            kind: 'child',
          },
          handles: [],
        };
      },
    },
    interaction: {
      createDraft(session) {
        return createInkDraft(session.startPoint);
      },
      updateDraft(draft, point) {
        return updateInkDraft(draft, point);
      },
      commitDraft(draft, context) {
        if (!context.hasExceededDragThreshold && !hasExceededDragThreshold(draft.start, draft.current)) {
          return null;
        }

        return createMarkup({
          id: context.createMarkupId(id),
          pageIndex: context.page.index,
          paths: [dedupeInkPoints(draft.points)],
          strokeWidth: defaultStrokeWidth,
          color: defaultStroke,
          source: { source: 'butter' },
        } as unknown as Omit<TMarkup, 'kind'>);
      },
    },
    pdf: {
      canImport(annotation) {
        const subtype = String(annotation.subtype ?? '').toLowerCase();
        const subject = String(annotation.subject ?? annotation.fields?.Subj ?? '').toLowerCase();
        return subtype === 'ink' && (id === 'highlight' ? subject === 'highlight' : subject !== 'highlight');
      },
      import(annotation, context) {
        return createMarkup({
          id: context.fallbackId,
          pageIndex: context.pageIndex,
          paths: readInkList(annotation.fields?.InkList, annotation.rect),
          ...(id === 'pen' && typeof annotation.fields?.BPSmoothCurves === 'boolean'
            ? { smoothCurves: annotation.fields.BPSmoothCurves }
            : {}),
          source: { source: 'imported' },
        } as Omit<TMarkup, 'kind'>);
      },
    },
  };
}

export function interpolatingInkPath(points: readonly PdfPoint[]): string {
  if (points.length === 0) return '';
  if (points.length === 1) return `M ${points[0].x} ${points[0].y}`;
  if (points.length === 2) return `M ${points[0].x} ${points[0].y} L ${points[1].x} ${points[1].y}`;

  const segments = [`M ${points[0].x} ${points[0].y}`];
  for (let index = 0; index < points.length - 1; index += 1) {
    const p0 = points[Math.max(0, index - 1)];
    const p1 = points[index];
    const p2 = points[index + 1];
    const p3 = points[Math.min(points.length - 1, index + 2)];
    const c1 = pdfPoint(p1.x + (p2.x - p0.x) / 6, p1.y + (p2.y - p0.y) / 6);
    const c2 = pdfPoint(p2.x - (p3.x - p1.x) / 6, p2.y - (p3.y - p1.y) / 6);
    segments.push(`C ${c1.x} ${c1.y} ${c2.x} ${c2.y} ${p2.x} ${p2.y}`);
  }
  return segments.join(' ');
}

function inkBounds(paths: readonly (readonly PdfPoint[])[], strokeWidth: number): Rect {
  const points = paths.flat();
  if (points.length === 0) {
    return rect(0, 0, 0, 0);
  }

  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const padding = Math.max(1, strokeWidth * 0.5);
  const minX = Math.min(...xs) - padding;
  const minY = Math.min(...ys) - padding;
  const maxX = Math.max(...xs) + padding;
  const maxY = Math.max(...ys) + padding;
  return rect(minX, minY, maxX - minX, maxY - minY);
}

function rectToArray(box: Rect): readonly number[] {
  return [box.x, box.y, box.x + box.width, box.y + box.height];
}

function dedupeInkPoints(points: readonly PdfPoint[]): readonly PdfPoint[] {
  return points.filter((point, index) => index === 0 || point.x !== points[index - 1].x || point.y !== points[index - 1].y);
}

function readInkList(value: unknown, fallbackRect: readonly number[] | undefined): readonly (readonly PdfPoint[])[] {
  if (Array.isArray(value)) {
    const paths = value
      .filter((path): path is readonly number[] => Array.isArray(path))
      .map((path) => readPathNumbers(path))
      .filter((path) => path.length >= 2);
    if (paths.length > 0) {
      return paths;
    }
  }

  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = fallbackRect ?? [];
  return [[pdfPoint(x1, y1), pdfPoint(x2, y2)]];
}

function readPathNumbers(value: readonly number[]): readonly PdfPoint[] {
  const points: PdfPoint[] = [];
  for (let index = 0; index < value.length - 1; index += 2) {
    points.push(pdfPoint(Number(value[index]), Number(value[index + 1])));
  }
  return points;
}
