import { createCloudMarkup, pdfPoint, rect, type CloudMarkup, type PdfPoint, type Rect } from '@butter-paper/core';
import {
  createLineDraft,
  updateCloudNodeDraft,
  shouldCommitLine,
  updateLineDraft,
  type CloudNodeDraft,
  type LineDraft,
} from '../annotationLifecycle';
import { getAnnotationContentStyle } from '../annotationStyles';
import { isPointNearPolygonEdge } from '../hitTesting';
import { CLOUD_LINE_TYPE_RENDERER, DEFAULT_CLOUD_LINE_OPTIONS, generateCloudScallopPoints } from '../lineTypes';
import { getMoveCursor } from '../interactionChrome';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const CLOUD_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'number', key: 'strokeWidthPt', label: 'Stroke width', default: 1, min: 0.25, max: 24, step: 0.25 },
    { kind: 'number', key: 'cloudIntensity', label: 'Cloud intensity', default: 2, min: 0, max: 4, step: 0.25 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

type CloudDraft = LineDraft | CloudNodeDraft;

export const CLOUD_TOOL_DEFINITION: PdfToolDefinition<CloudMarkup, CloudDraft> & { readonly id: 'cloud' } = {
  id: 'cloud',
  label: 'Cloud',
  shortcut: 'C',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-cloud',
  implemented: true,
  properties: CLOUD_PROPERTIES,
  defaults: {
    strokeColor: '#ff0000',
    strokeWidthPt: 1,
    fillColor: null,
    cloudIntensity: 2,
    opacity: 1,
  },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: pointsBounds(markup.controlPath),
        components: [
          {
            id: 'cloud.body',
            role: 'shape',
            geometry: {
              kind: 'generatedPath',
              controlPath: markup.controlPath,
              closed: true,
              lineType: {
                id: 'cloud',
                options: {
                  ...cloudLineOptions(markup),
                  pdfBorderEffectIntensity: markup.borderEffectIntensity ?? 2,
                },
                pdfCompatibility: { borderEffect: { style: 'cloud', intensity: markup.borderEffectIntensity ?? 2 } },
              },
            },
            bodyDrag: 'moveSelf',
          },
        ],
        handles: vertexHandles(markup.controlPath),
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      const visiblePath = cloudVisiblePath(markup);
      if (!isPointNearPolygonEdge(point, markup.controlPath, context.tolerance) && !isPointNearPolygonEdge(point, visiblePath, context.tolerance)) {
        return null;
      }

      return {
        markupId: markup.id,
        componentId: 'cloud.body',
        region: 'edge',
        bodyDrag: 'moveSelf',
        cursor: getMoveCursor(),
      };
    },
  },
  render: {
    getContentPrimitives(markup) {
      const style = getAnnotationContentStyle(markup);
      const visible = cloudVisible(markup);
      return [
        {
          kind: 'path',
          d: visible.d,
          style: {
            stroke: style.stroke,
            fill: 'none',
            strokeWidth: style.strokeWidth,
            opacity: style.opacity,
            lineCap: 'round',
            lineJoin: 'round',
          },
          pointerEvents: 'visibleStroke',
        },
      ];
    },
    getDraftPrimitives(draft) {
      const controlPath = draftControlPath(draft);
      const generated = CLOUD_LINE_TYPE_RENDERER.render({
        controlPath,
        closed: true,
        strokeWidth: 1,
        options: DEFAULT_CLOUD_LINE_OPTIONS,
      });
      return [
        {
          kind: 'path',
          d: generated.d,
          style: {
            stroke: '#ff0000',
            fill: 'none',
            strokeWidth: 1,
            opacity: 1,
            lineCap: 'round',
            lineJoin: 'round',
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
          rect: pointsBounds(markup.controlPath),
          kind: 'child',
        },
        handles: vertexHandles(markup.controlPath),
        controlPaths: [{ id: 'cloud.controlPath', points: markup.controlPath, closed: true }],
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      const controlPath = draftControlPath(draft);
      return {
        bounds: {
          rect: pointsBounds(controlPath),
          kind: 'child',
        },
        handles: draft.kind === 'cloud-node' ? vertexHandles(draft.points) : [],
        controlPaths: draft.kind === 'cloud-node' ? [{ id: 'cloud.draftControlPath', points: controlPath, closed: true }] : undefined,
      };
    },
  },
  interaction: {
    createDraft(session) {
      return createLineDraft(session.startPoint);
    },
    updateDraft(draft, point) {
      return draft.kind === 'cloud-node' ? updateCloudNodeDraft(draft, point) : updateLineDraft(draft, point);
    },
    commitDraft(draft, context) {
      if (draft.kind === 'cloud-node') {
        if (draft.points.length < 3) {
          return null;
        }

        return createCloudMarkup({
          id: context.createMarkupId('cloud'),
          pageIndex: context.page.index,
          controlPath: draft.points,
          strokeWidth: 1,
          borderEffectIntensity: 2,
          scallopRadius: DEFAULT_CLOUD_LINE_OPTIONS.scallopRadius,
          source: { source: 'butter' },
        });
      }

      if (!context.hasExceededDragThreshold || !shouldCommitLine(draft.start, draft.current)) {
        return null;
      }

      return createCloudMarkup({
        id: context.createMarkupId('cloud'),
        pageIndex: context.page.index,
        controlPath: rectangleControlPath(draft.start, draft.current),
        strokeWidth: 1,
        borderEffectIntensity: 2,
        scallopRadius: DEFAULT_CLOUD_LINE_OPTIONS.scallopRadius,
        source: { source: 'butter' },
      });
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior !== 'reshapeVertex') {
        return markup;
      }
      const vertexIndex = Number(input.handleId.split('.').at(-1));
      if (!Number.isInteger(vertexIndex) || vertexIndex < 0 || vertexIndex >= markup.controlPath.length) {
        return markup;
      }

      return createCloudMarkup({
        ...markup,
        controlPath: markup.controlPath.map((point, index) => index === vertexIndex ? input.currentPoint : point),
      });
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      const intent = String(annotation.intent ?? annotation.fields?.IT ?? '').toLowerCase();
      return subtype === 'polygon' && intent === 'polygoncloud';
    },
    import(annotation, context) {
      return createCloudMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        controlPath: readVertices(annotation.fields?.Vertices, annotation.rect),
        scallopRadius: readScallopRadius(annotation.rect, readVertices(annotation.fields?.Vertices, annotation.rect)),
        borderEffectIntensity: readBorderEffectIntensity(annotation.fields?.BE),
        source: { source: 'imported' },
      });
    },
  },
};

function vertexHandles(points: readonly PdfPoint[]) {
  return points.map((point, index) => ({
    id: `cloud.vertex.${index}`,
    componentId: 'cloud.body',
    point,
    behavior: 'reshapeVertex' as const,
    cursor: getMoveCursor(),
  }));
}

function cloudVisible(markup: Pick<CloudMarkup, 'controlPath' | 'borderEffectIntensity' | 'scallopRadius' | 'appearancePath'>) {
  if ('appearancePath' in markup && typeof markup.appearancePath === 'string' && markup.appearancePath.length > 0) {
    const generated = CLOUD_LINE_TYPE_RENDERER.render({
      controlPath: markup.controlPath,
      closed: true,
      strokeWidth: 1,
      options: cloudLineOptions(markup),
    });
    return {
      ...generated,
      d: markup.appearancePath,
    };
  }
  return CLOUD_LINE_TYPE_RENDERER.render({
    controlPath: markup.controlPath,
    closed: true,
    strokeWidth: 1,
    options: cloudLineOptions(markup),
  });
}

function cloudVisiblePath(markup: Pick<CloudMarkup, 'controlPath' | 'borderEffectIntensity' | 'scallopRadius' | 'appearancePath'>): readonly PdfPoint[] {
  return cloudVisible(markup).points;
}

function rectangleControlPath(start: PdfPoint, current: PdfPoint): readonly PdfPoint[] {
  return [
    start,
    pdfPoint(start.x, current.y),
    current,
    pdfPoint(current.x, start.y),
  ];
}

function draftControlPath(draft: CloudDraft): readonly PdfPoint[] {
  if (draft.kind === 'cloud-node') {
    const lastPoint = draft.points[draft.points.length - 1] ?? draft.start;
    const previewPoint = Math.hypot(draft.current.x - lastPoint.x, draft.current.y - lastPoint.y) < 0.5
      ? []
      : [draft.current];
    return [...draft.points, ...previewPoint];
  }

  return rectangleControlPath(draft.start, draft.current);
}

function pointsBounds(points: readonly PdfPoint[]): Rect {
  const bounds = rectFromPoints(points);
  return rect(bounds[0], bounds[1], bounds[2] - bounds[0], bounds[3] - bounds[1]);
}

function rectFromPoints(points: readonly PdfPoint[]): readonly number[] {
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

function readVertices(value: unknown, fallbackRect: readonly number[] | undefined): readonly PdfPoint[] {
  if (Array.isArray(value) && value.length >= 6) {
    const points: PdfPoint[] = [];
    for (let index = 0; index < value.length - 1; index += 2) {
      points.push(pdfPoint(Number(value[index]), Number(value[index + 1])));
    }
    return points;
  }
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = fallbackRect ?? [];
  return [pdfPoint(x1, y2), pdfPoint(x1, y1), pdfPoint(x2, y1), pdfPoint(x2, y2)];
}

function readBorderEffectIntensity(value: unknown): number | undefined {
  if (value && typeof value === 'object' && 'I' in value) {
    return Number((value as { I?: unknown }).I);
  }
  return undefined;
}

function cloudLineOptions(markup: Pick<CloudMarkup, 'borderEffectIntensity' | 'scallopRadius'>) {
  const radius = markup.scallopRadius ?? DEFAULT_CLOUD_LINE_OPTIONS.scallopRadius;
  return {
    ...DEFAULT_CLOUD_LINE_OPTIONS,
    scallopRadius: radius,
    scallopSpacing: radius,
    pdfBorderEffectIntensity: markup.borderEffectIntensity ?? 2,
  };
}

function readScallopRadius(rectValue: readonly number[] | undefined, points: readonly PdfPoint[]): number | undefined {
  if (!rectValue || points.length === 0) {
    return undefined;
  }
  const controlBounds = rectFromPoints(points);
  const [rectMinX, rectMinY, rectMaxX, rectMaxY] = rectValue;
  const paddings = [
    controlBounds[0] - rectMinX,
    controlBounds[1] - rectMinY,
    rectMaxX - controlBounds[2],
    rectMaxY - controlBounds[3],
  ].filter((value) => Number.isFinite(value) && value > 0);
  if (paddings.length === 0) {
    return undefined;
  }
  const padding = paddings.sort((a, b) => a - b)[Math.floor(paddings.length / 2)];
  return padding / 0.6831;
}
