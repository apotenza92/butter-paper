import { createCloudPlusMarkup, pdfPoint, rect, translateMarkup, translatePoint, translateRect, type CloudPlusMarkup, type Markup, type PdfPoint, type Rect } from '@butter-paper/core';
import {
  createLineDraft,
  resizeRectFromHandle,
  shouldCommitLine,
  updateCloudNodeDraft,
  updateLineDraft,
  type CloudNodeDraft,
  type LineDraft,
  type RectResizeHandle,
} from '../annotationLifecycle';
import { getAnnotationContentStyle, getVerticallyCenteredAnnotationTextContentStyle } from '../annotationStyles';
import { placeInitialCloudPlusTextBox, routeCloudPlusLeader, snapCloudPlusLeaderTip, type CloudPlusObstacle, type CloudPlusRoutingContext } from '../cloudPlusRouting';
import { isPointInRect, isPointNearPolygonEdge, isPointNearPolyline } from '../hitTesting';
import { getMoveCursor, getResizeHandles } from '../interactionChrome';
import { CLOUD_LINE_TYPE_RENDERER, DEFAULT_CLOUD_LINE_OPTIONS, sampleAbsoluteSvgPath } from '../lineTypes';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit, ToolInteractionContext } from '../types';
import { dimensionCaptionRect } from './dimensionTool';

const DEFAULT_TEXT = 'Cloud+';
const DEFAULT_TEXT_BOX_WIDTH = 150;
const DEFAULT_TEXT_BOX_HEIGHT = 44;

type CloudPlusDraft = LineDraft | CloudNodeDraft;

const CLOUD_PLUS_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'color', key: 'textColor', label: 'Text', default: '#ff0000' },
    { kind: 'number', key: 'fontSizePt', label: 'Font size', default: 12, min: 6, max: 72, step: 1 },
    { kind: 'number', key: 'cloudIntensity', label: 'Cloud intensity', default: 2, min: 0, max: 4, step: 0.25 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const CLOUD_PLUS_TOOL_DEFINITION: PdfToolDefinition<CloudPlusMarkup, CloudPlusDraft> & { readonly id: 'cloud-plus' } = {
  id: 'cloud-plus',
  label: 'Cloud+',
  shortcut: 'K',
  category: 'markup',
  cursor: 'crosshair',
  testId: 'tool-cloud-plus',
  implemented: true,
  properties: CLOUD_PLUS_PROPERTIES,
  defaults: {
    strokeColor: '#ff0000',
    textColor: '#ff0000',
    fontSizePt: 12,
    cloudIntensity: 2,
    opacity: 1,
  },
  geometry: {
    getGeometry(markup): ToolGeometryDescriptor {
      return {
        bounds: cloudPlusBounds(markup),
        components: [
          {
            id: 'cloud-plus.cloud',
            role: 'shape',
            geometry: {
              kind: 'generatedPath',
              controlPath: markup.cloud.controlPath,
              closed: true,
              lineType: {
                id: 'cloud',
                options: cloudLineOptions(markup),
                pdfCompatibility: { borderEffect: { style: 'cloud', intensity: markup.cloud.borderEffectIntensity ?? 2 } },
              },
            },
            bodyDrag: 'moveGroup',
          },
          {
            id: 'cloud-plus.textBox',
            role: 'textBox',
            geometry: { kind: 'textBox', rect: markup.textBox },
            bodyDrag: 'moveSelf',
          },
          {
            id: 'cloud-plus.leader',
            role: 'leader',
            geometry: { kind: 'polyline', points: markup.leader.points },
            bodyDrag: 'adjustOnly',
          },
        ],
        handles: cloudPlusHandles(markup),
      };
    },
    hitTest(markup, point, context): ToolHit | null {
      if (isPointInRect(point, markup.textBox)) {
        return {
          markupId: markup.id,
          componentId: 'cloud-plus.textBox',
          region: 'interior',
          bodyDrag: 'moveSelf',
          cursor: getMoveCursor(),
        };
      }

      if (isPointNearPolyline(point, markup.leader.points, context.tolerance)) {
        return {
          markupId: markup.id,
          componentId: 'cloud-plus.leader',
          region: 'leader',
          bodyDrag: 'adjustOnly',
          cursor: getMoveCursor(),
        };
      }

      const visible = cloudVisiblePath(markup);
      if (isPointNearPolygonEdge(point, markup.cloud.controlPath, context.tolerance) || isPointNearPolygonEdge(point, visible, context.tolerance)) {
        return {
          markupId: markup.id,
          componentId: 'cloud-plus.cloud',
          region: 'edge',
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
      const visible = cloudVisible(markup);
      const points = markup.leader.points;
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
        {
          kind: 'polyline',
          points,
          style: { stroke: style.stroke, fill: 'none', strokeWidth: style.strokeWidth, opacity: style.opacity },
          pointerEvents: 'visibleStroke',
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
    getDraftPrimitives(draft, context) {
      return CLOUD_PLUS_TOOL_DEFINITION.render?.getContentPrimitives(
        draftToCloudPlusPreview(draft, 'cloud-plus-draft', context.page.index, cloudPlusRoutingContext(context)),
        context,
      ) ?? [];
    },
  },
  selection: {
    getSelectionChrome(markup): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: cloudPlusBounds(markup),
          kind: 'group',
        },
        handles: cloudPlusHandles(markup),
        controlPaths: [
          { id: 'cloud-plus.cloudPath', points: markup.cloud.controlPath, closed: true },
          { id: 'cloud-plus.leaderPath', points: markup.leader.points, closed: false },
        ],
      };
    },
    getDraftChrome(draft): SelectionChromeDescriptor {
      const markup = draftToCloudPlusPreview(draft, 'cloud-plus-draft', 0);
      return {
        bounds: { rect: cloudPlusBounds(markup), kind: 'group' },
        handles: draft.kind === 'cloud-node' ? cloudPlusDraftVertexHandles(draft.points) : [],
        controlPaths: [
          { id: 'cloud-plus.draftCloudPath', points: markup.cloud.controlPath, closed: true },
          { id: 'cloud-plus.draftLeaderPath', points: markup.leader.points, closed: false },
        ],
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
        return cloudPlusFromControlPath(
          draft.points,
          context.createMarkupId('cloud-plus'),
          context.page.index,
          cloudPlusRoutingContext(context),
        );
      }

      if (!context.hasExceededDragThreshold || !shouldCommitLine(draft.start, draft.current)) {
        return null;
      }
      return cloudPlusFromControlPath(
        rectangleControlPath(draft.start, draft.current),
        context.createMarkupId('cloud-plus'),
        context.page.index,
        cloudPlusRoutingContext(context),
      );
    },
    transformMarkup(markup, input, context) {
      const routingContext = cloudPlusRoutingContext(context, markup.id);
      if (input.handleBehavior === 'reshapeVertex') {
        const vertexIndex = Number(input.handleId.split('.').at(-1));
        if (!Number.isInteger(vertexIndex) || vertexIndex < 0 || vertexIndex >= markup.cloud.controlPath.length) {
          return markup;
        }
        const controlPath = markup.cloud.controlPath.map((point, index) => index === vertexIndex ? input.currentPoint : point);
        const reshaped = createCloudPlusMarkup({
          ...markup,
          cloud: {
            ...markup.cloud,
            controlPath,
            appearancePath: generatedCloudPath(controlPath, markup),
          },
        });
        return createCloudPlusMarkup({
          ...reshaped,
          leader: { points: leaderPointsForTextBox(reshaped, reshaped.textBox, routingContext, markup.leader.points) },
        });
      }

      if (input.handleBehavior === 'resizeSelf' && input.handleId.startsWith('cloud-plus.textBox.resize.')) {
        const handle = resizeHandleFromId(input.handleId);
        if (!handle) {
          return markup;
        }
        const textBox = resizeRectFromHandle(markup.textBox, handle, input.currentPoint);
        return createCloudPlusMarkup({
          ...markup,
          textBox,
          leader: { points: leaderPointsForTextBox(markup, textBox, routingContext, markup.leader.points) },
        });
      }

      if (input.handleBehavior === 'moveEndpoint' || input.handleBehavior === 'moveKnee') {
        const pointIndex = leaderPointIndexFromHandle(input.handleId, markup.leader.points.length);
        if (pointIndex === null) {
          return markup;
        }
        if (pointIndex === markup.leader.points.length - 1) {
          const hintedLeader = markup.leader.points.map((point, index) => index === pointIndex ? input.currentPoint : point);
          return createCloudPlusMarkup({
            ...markup,
            leader: { points: leaderPointsForTextBox(markup, markup.textBox, routingContext, hintedLeader) },
          });
        }
        const nextPoint = pointIndex === 0
          ? snapCloudPlusLeaderTip(cloudVisiblePath(markup), input.currentPoint)
          : input.currentPoint;
        return createCloudPlusMarkup({
          ...markup,
          leader: { points: markup.leader.points.map((point, index) => index === pointIndex ? nextPoint : point) },
        });
      }

      return markup;
    },
    dragMarkup(markup, input, context) {
      if (input.componentId === 'cloud-plus.cloud' || input.bodyDrag === 'moveGroup') {
        return translateMarkup(markup, input.delta);
      }

      if (input.componentId === 'cloud-plus.textBox') {
        const textBox = translateRect(markup.textBox, input.delta);
        return createCloudPlusMarkup({
          ...markup,
          textBox,
          leader: { points: leaderPointsForTextBox(markup, textBox, cloudPlusRoutingContext(context, markup.id), markup.leader.points) },
        });
      }

      return markup;
    },
  },
  pdf: {
    canImport(annotation) {
      const subtype = String(annotation.subtype ?? '').toLowerCase();
      const subject = String(annotation.subject ?? annotation.fields?.Subj ?? '').toLowerCase();
      const intent = String(annotation.intent ?? annotation.fields?.IT ?? '').toLowerCase();
      const intentEx = String(annotation.fields?.ITEx ?? '').toLowerCase();
      return subject === 'cloud+' && intentEx === 'polytext' && (subtype === 'polygon' || subtype === 'freetext') && (intent === 'polygoncloud' || intent === 'freetextcallout');
    },
    import(annotation, context) {
      const box = rectFromPdfRect(annotation.rect);
      return createCloudPlusMarkup({
        id: context.fallbackId,
        pageIndex: context.pageIndex,
        cloud: {
          controlPath: readVertices(annotation.fields?.Vertices, annotation.rect),
          borderEffectIntensity: readBorderEffectIntensity(annotation.fields?.BE) ?? 2,
          scallopRadius: DEFAULT_CLOUD_LINE_OPTIONS.scallopRadius,
        },
        leader: { points: readLeaderPoints(annotation.fields?.CL, box) },
        textBox: box,
        text: String(annotation.fields?.Contents ?? DEFAULT_TEXT),
        color: '#ff0000',
        source: { source: 'imported' },
      });
    },
  },
};

function draftToCloudPlusPreview(
  draft: CloudPlusDraft,
  id: string,
  pageIndex: number,
  routingContext: CloudPlusRoutingContext = {},
): CloudPlusMarkup {
  const cloudPath = draft.kind === 'cloud-node'
    ? cloudNodePreviewPath(draft)
    : rectangleControlPath(draft.start, draft.current);
  return cloudPlusFromControlPath(cloudPath, id, pageIndex, routingContext);
}

function cloudPlusFromControlPath(
  cloudPath: readonly PdfPoint[],
  id: string,
  pageIndex: number,
  routingContext: CloudPlusRoutingContext = {},
): CloudPlusMarkup {
  const visible = CLOUD_LINE_TYPE_RENDERER.render({
    controlPath: cloudPath,
    closed: true,
    strokeWidth: 1,
    options: DEFAULT_CLOUD_LINE_OPTIONS,
  });
  const placement = placeInitialCloudPlusTextBox({
    controlPath: cloudPath,
    visiblePath: visible.points,
    width: DEFAULT_TEXT_BOX_WIDTH,
    height: DEFAULT_TEXT_BOX_HEIGHT,
    gap: 24,
    ...routingContext,
  });
  return createCloudPlusMarkup({
    id,
    pageIndex,
    cloud: {
      controlPath: cloudPath,
      strokeWidth: 1,
      borderEffectIntensity: 2,
      scallopRadius: DEFAULT_CLOUD_LINE_OPTIONS.scallopRadius,
      appearancePath: visible.d,
    },
    leader: { points: placement.leader.points },
    textBox: placement.textBox,
    text: DEFAULT_TEXT,
    color: '#ff0000',
    source: { source: 'butter' },
  });
}

function cloudNodePreviewPath(draft: CloudNodeDraft): readonly PdfPoint[] {
  const lastPoint = draft.points[draft.points.length - 1] ?? draft.start;
  return Math.hypot(draft.current.x - lastPoint.x, draft.current.y - lastPoint.y) < 0.5
    ? draft.points
    : [...draft.points, draft.current];
}

function cloudPlusBounds(markup: CloudPlusMarkup): Rect {
  return pointsBounds([
    ...markup.cloud.controlPath,
    ...markup.leader.points,
    pdfPoint(markup.textBox.x, markup.textBox.y),
    pdfPoint(markup.textBox.x + markup.textBox.width, markup.textBox.y + markup.textBox.height),
  ]);
}

function cloudPlusHandles(markup: CloudPlusMarkup) {
  const cloudHandles = markup.cloud.controlPath.map((point, index) => ({
    id: `cloud-plus.cloud.vertex.${index}`,
    componentId: 'cloud-plus.cloud',
    point,
    behavior: 'reshapeVertex' as const,
    cursor: getMoveCursor(),
  }));
  const textResizeHandles = getResizeHandles(markup.textBox).map((handle) => ({
    id: `cloud-plus.textBox.resize.${handle.kind}`,
    componentId: 'cloud-plus.textBox',
    point: pdfPoint(handle.x, handle.y),
    behavior: 'resizeSelf' as const,
    cursor: handle.cursor,
  }));
  const leaderHandles = markup.leader.points.map((point, index) => ({
    id: index === 0
      ? 'cloud-plus.leader.tip'
      : index === markup.leader.points.length - 1
        ? 'cloud-plus.leader.connection'
        : `cloud-plus.leader.knee.${index}`,
    componentId: 'cloud-plus.leader',
    point,
    behavior: index === 0 || index === markup.leader.points.length - 1 ? 'moveEndpoint' as const : 'moveKnee' as const,
    cursor: getMoveCursor(),
  }));
  return [...cloudHandles, ...textResizeHandles, ...leaderHandles];
}

function cloudPlusDraftVertexHandles(points: readonly PdfPoint[]) {
  return points.map((point, index) => ({
    id: `cloud-plus.cloud.vertex.${index}`,
    componentId: 'cloud-plus.cloud',
    point,
    behavior: 'reshapeVertex' as const,
    cursor: getMoveCursor(),
  }));
}

function leaderPointIndexFromHandle(handleId: string, pointCount: number): number | null {
  if (handleId === 'cloud-plus.leader.connection') {
    return pointCount - 1;
  }
  if (handleId === 'cloud-plus.leader.tip') {
    return 0;
  }
  const index = Number(handleId.replace('cloud-plus.leader.knee.', ''));
  return Number.isInteger(index) && index > 0 && index < pointCount - 1 ? index : null;
}

function resizeHandleFromId(handleId: string): RectResizeHandle | null {
  const handle = handleId.replace('cloud-plus.textBox.resize.', '');
  if (handle === 'nw' || handle === 'n' || handle === 'ne' || handle === 'e' || handle === 'se' || handle === 's' || handle === 'sw' || handle === 'w') {
    return handle;
  }
  return null;
}

function cloudVisible(markup: CloudPlusMarkup) {
  if (typeof markup.cloud.appearancePath === 'string' && markup.cloud.appearancePath.length > 0) {
    const generated = CLOUD_LINE_TYPE_RENDERER.render({
      controlPath: markup.cloud.controlPath,
      closed: true,
      strokeWidth: 1,
      options: cloudLineOptions(markup),
    });
    const appearancePoints = sampleAbsoluteSvgPath(markup.cloud.appearancePath);
    return {
      ...generated,
      d: markup.cloud.appearancePath,
      points: appearancePoints.length > 1 ? appearancePoints : generated.points,
    };
  }
  return CLOUD_LINE_TYPE_RENDERER.render({
    controlPath: markup.cloud.controlPath,
    closed: true,
    strokeWidth: 1,
    options: cloudLineOptions(markup),
  });
}

function cloudVisiblePath(markup: CloudPlusMarkup): readonly PdfPoint[] {
  return cloudVisible(markup).points;
}

function generatedCloudPath(controlPath: readonly PdfPoint[], markup: CloudPlusMarkup): string {
  return CLOUD_LINE_TYPE_RENDERER.render({
    controlPath,
    closed: true,
    strokeWidth: 1,
    options: cloudLineOptions(markup),
  }).d;
}

function cloudLineOptions(markup: CloudPlusMarkup) {
  const radius = markup.cloud.scallopRadius ?? DEFAULT_CLOUD_LINE_OPTIONS.scallopRadius;
  return {
    ...DEFAULT_CLOUD_LINE_OPTIONS,
    scallopRadius: radius,
    scallopSpacing: radius,
    pdfBorderEffectIntensity: markup.cloud.borderEffectIntensity ?? 2,
  };
}

function rectangleControlPath(start: PdfPoint, current: PdfPoint): readonly PdfPoint[] {
  return [
    start,
    pdfPoint(start.x, current.y),
    current,
    pdfPoint(current.x, start.y),
  ];
}

function pointsBounds(points: readonly PdfPoint[]): Rect {
  const bounds = pointBoundsArray(points);
  return rect(bounds[0], bounds[1], bounds[2] - bounds[0], bounds[3] - bounds[1]);
}

function rectFromPdfRect(rectValue: readonly number[] | undefined): Rect {
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = rectValue ?? [];
  const normalized = pointBoundsArray([pdfPoint(x1, y1), pdfPoint(x2, y2)]);
  return rect(normalized[0], normalized[1], normalized[2] - normalized[0], normalized[3] - normalized[1]);
}

function pointBoundsArray(points: readonly PdfPoint[]): readonly number[] {
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

function readLeaderPoints(value: unknown, fallbackBox: Rect): readonly PdfPoint[] {
  if (Array.isArray(value) && value.length >= 4) {
    const points: PdfPoint[] = [];
    for (let index = 0; index < value.length - 1; index += 2) {
      points.push(pdfPoint(Number(value[index]), Number(value[index + 1])));
    }
    return points;
  }
  return [
    pdfPoint(fallbackBox.x - 60, fallbackBox.y + fallbackBox.height * 0.5),
    pdfPoint(fallbackBox.x - 30, fallbackBox.y + fallbackBox.height * 0.5),
    pdfPoint(fallbackBox.x, fallbackBox.y + fallbackBox.height * 0.5),
  ];
}

function cloudPlusRoutingContext(
  context: ToolInteractionContext | undefined,
  excludeMarkupId?: string,
): CloudPlusRoutingContext {
  if (!context) {
    return {};
  }
  return {
    pageBounds: context.pageBounds ?? rect(0, 0, context.page.size.width, context.page.size.height),
    obstacles: cloudPlusRoutingObstacles(context.markups ?? [], excludeMarkupId),
  };
}

export function cloudPlusRoutingObstacles(
  markups: readonly Markup[],
  excludeMarkupId?: string,
): readonly CloudPlusObstacle[] {
  return [...markups]
    .filter((markup) => markup.id !== excludeMarkupId)
    .sort((left, right) => left.id.localeCompare(right.id))
    .flatMap((markup): readonly CloudPlusObstacle[] => {
      const id = markup.id;
      switch (markup.kind) {
        case 'text-box':
        case 'rectangle':
        case 'redact':
        case 'ellipse':
        case 'arc':
        case 'image':
        case 'snapshot':
        case 'imported-annotation':
          return [{ id, kind: 'rect', rect: markup.rect }];
        case 'callout':
          return [
            { id: `${id}:text`, kind: 'rect', rect: markup.textBox },
            { id: `${id}:leader`, kind: 'polyline', points: markup.leader.points },
          ];
        case 'cloud-plus':
          return [
            { id: `${id}:text`, kind: 'rect', rect: markup.textBox },
            { id: `${id}:leader`, kind: 'polyline', points: markup.leader.points },
            { id: `${id}:cloud`, kind: 'polygon', points: cloudVisiblePath(markup) },
          ];
        case 'dimension':
          return [
            { id: `${id}:caption`, kind: 'rect', rect: dimensionCaptionRect(markup) },
            { id: `${id}:line`, kind: 'polyline', points: [markup.start, markup.end] },
          ];
        case 'line':
        case 'arrow':
        case 'length':
          return [{ id, kind: 'polyline', points: [markup.start, markup.end] }];
        case 'polyline':
        case 'polylength':
          return [{ id, kind: 'polyline', points: markup.points }];
        case 'polygon':
        case 'area':
          return [{ id, kind: 'polygon', points: markup.points }];
        case 'cloud':
          return [{ id, kind: 'polygon', points: markup.controlPath }];
        case 'pen':
        case 'highlight':
          return markup.paths.map((points, index) => ({ id: `${id}:path:${index}`, kind: 'polyline' as const, points }));
      }
    });
}

function leaderPointsForTextBox(
  markup: CloudPlusMarkup,
  textBox: Rect,
  routingContext?: CloudPlusRoutingContext,
  previousLeader: readonly PdfPoint[] = markup.leader.points,
): readonly PdfPoint[] {
  return routeCloudPlusLeader({
    controlPath: markup.cloud.controlPath,
    visiblePath: cloudVisiblePath(markup),
    textBox,
    previousLeader,
    ...routingContext,
  }).points;
}

export function updateCloudPlusTextBox(
  markup: CloudPlusMarkup,
  textBox: Rect,
  context?: ToolInteractionContext,
): CloudPlusMarkup {
  return createCloudPlusMarkup({
    ...markup,
    textBox,
    leader: {
      points: leaderPointsForTextBox(
        markup,
        textBox,
        cloudPlusRoutingContext(context, markup.id),
        markup.leader.points,
      ),
    },
  });
}
