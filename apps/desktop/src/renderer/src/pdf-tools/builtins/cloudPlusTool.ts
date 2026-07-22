import { createCloudPlusMarkup, pdfPoint, rect, translatePoint, translateRect, type CloudPlusMarkup, type PdfPoint, type Rect } from '@butter-paper/core';
import {
  createLineDraft,
  resizeRectFromHandle,
  shouldCommitLine,
  updateLineDraft,
  type LineDraft,
  type RectResizeHandle,
} from '../annotationLifecycle';
import { getAnnotationContentStyle, getAnnotationTextContentStyle } from '../annotationStyles';
import { isPointInRect, isPointNearPolygonEdge, isPointNearPolyline } from '../hitTesting';
import { getMoveCursor, getResizeHandles } from '../interactionChrome';
import { CLOUD_LINE_TYPE_RENDERER, DEFAULT_CLOUD_LINE_OPTIONS } from '../lineTypes';
import type { PdfToolDefinition, SelectionChromeDescriptor, ToolGeometryDescriptor, ToolHit } from '../types';

const DEFAULT_TEXT = 'Cloud+';
const DEFAULT_TEXT_BOX_WIDTH = 150;
const DEFAULT_TEXT_BOX_HEIGHT = 44;

const CLOUD_PLUS_PROPERTIES = {
  properties: [
    { kind: 'color', key: 'strokeColor', label: 'Stroke', default: '#ff0000' },
    { kind: 'color', key: 'textColor', label: 'Text', default: '#ff0000' },
    { kind: 'number', key: 'fontSizePt', label: 'Font size', default: 12, min: 6, max: 72, step: 1 },
    { kind: 'number', key: 'cloudIntensity', label: 'Cloud intensity', default: 2, min: 0, max: 4, step: 0.25 },
    { kind: 'number', key: 'opacity', label: 'Opacity', default: 1, min: 0, max: 1, step: 0.05 },
  ],
} as const;

export const CLOUD_PLUS_TOOL_DEFINITION: PdfToolDefinition<CloudPlusMarkup, LineDraft> & { readonly id: 'cloud-plus' } = {
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
      const tip = points[0] ?? pdfPoint(markup.textBox.x, markup.textBox.y);
      const afterTip = points[1] ?? tip;
      return [
        {
          kind: 'path',
          d: visible.d,
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
          points,
          style: { stroke: style.stroke, fill: 'none', strokeWidth: style.strokeWidth, opacity: style.opacity },
          pointerEvents: 'visibleStroke',
        },
        {
          kind: 'polyline',
          points: openArrowHeadPoints(afterTip, tip, 10, 7),
          style: { stroke: style.stroke, fill: 'none', strokeWidth: style.strokeWidth, opacity: style.opacity },
          pointerEvents: 'none',
        },
        {
          kind: 'textBox',
          rect: markup.textBox,
          text: markup.text,
          style: getAnnotationTextContentStyle(markup),
          pointerEvents: 'all',
        },
      ];
    },
    getDraftPrimitives(draft) {
      return CLOUD_PLUS_TOOL_DEFINITION.render?.getContentPrimitives(draftToCloudPlusPreview(draft, 'cloud-plus-draft', 0), { page: { id: 'draft', index: 0, size: { width: 0, height: 0 }, rotation: 0 }, phase: 'draft' }) ?? [];
    },
  },
  selection: {
    getSelectionChrome(markup): SelectionChromeDescriptor {
      return {
        bounds: {
          rect: cloudPlusBounds(markup),
          kind: 'group',
          canResize: false,
          canRotate: false,
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
        bounds: { rect: cloudPlusBounds(markup), kind: 'group', canResize: false, canRotate: false },
        handles: [],
        controlPaths: [
          { id: 'cloud-plus.draftCloudPath', points: markup.cloud.controlPath, closed: true },
          { id: 'cloud-plus.draftLeaderPath', points: markup.leader.points, closed: false },
        ],
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
      return draftToCloudPlusPreview(draft, context.createMarkupId('cloud-plus'), context.page.index);
    },
    transformMarkup(markup, input) {
      if (input.handleBehavior === 'reshapeVertex') {
        const vertexIndex = Number(input.handleId.split('.').at(-1));
        if (!Number.isInteger(vertexIndex) || vertexIndex < 0 || vertexIndex >= markup.cloud.controlPath.length) {
          return markup;
        }
        return createCloudPlusMarkup({
          ...markup,
          cloud: {
            ...markup.cloud,
            controlPath: markup.cloud.controlPath.map((point, index) => index === vertexIndex ? input.currentPoint : point),
            appearancePath: undefined,
          },
        });
      }

      if (input.handleBehavior === 'resizeSelf' && input.handleId.startsWith('cloud-plus.textBox.resize.')) {
        const handle = resizeHandleFromId(input.handleId);
        if (!handle) {
          return markup;
        }
        return createCloudPlusMarkup({ ...markup, textBox: resizeRectFromHandle(markup.textBox, handle, input.currentPoint) });
      }

      if (input.handleBehavior === 'moveEndpoint' || input.handleBehavior === 'moveKnee') {
        const pointIndex = leaderPointIndexFromHandle(input.handleId, markup.leader.points.length);
        if (pointIndex === null) {
          return markup;
        }
        return createCloudPlusMarkup({
          ...markup,
          leader: { points: markup.leader.points.map((point, index) => index === pointIndex ? input.currentPoint : point) },
        });
      }

      return markup;
    },
    dragMarkup(markup, input) {
      if (input.componentId === 'cloud-plus.cloud' || input.bodyDrag === 'moveGroup') {
        return createCloudPlusMarkup({
          ...markup,
          cloud: {
            ...markup.cloud,
            controlPath: markup.cloud.controlPath.map((point) => translatePoint(point, input.delta)),
            appearancePath: undefined,
          },
          textBox: translateRect(markup.textBox, input.delta),
          leader: { points: markup.leader.points.map((point) => translatePoint(point, input.delta)) },
        });
      }

      if (input.componentId === 'cloud-plus.textBox') {
        return createCloudPlusMarkup({
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

function draftToCloudPlusPreview(draft: LineDraft, id: string, pageIndex: number): CloudPlusMarkup {
  const cloudPath = rectangleControlPath(draft.start, draft.current);
  const cloudBounds = pointsBounds(cloudPath);
  const textBox = rect(cloudBounds.x + cloudBounds.width + 24, cloudBounds.y + cloudBounds.height * 0.5 - DEFAULT_TEXT_BOX_HEIGHT * 0.5, DEFAULT_TEXT_BOX_WIDTH, DEFAULT_TEXT_BOX_HEIGHT);
  const tip = pdfPoint(cloudBounds.x + cloudBounds.width, cloudBounds.y + cloudBounds.height * 0.5);
  const connection = pdfPoint(textBox.x, textBox.y + textBox.height * 0.5);
  const knee = pdfPoint((connection.x + tip.x) * 0.5, connection.y);
  return createCloudPlusMarkup({
    id,
    pageIndex,
    cloud: {
      controlPath: cloudPath,
      strokeWidth: 1,
      borderEffectIntensity: 2,
      scallopRadius: DEFAULT_CLOUD_LINE_OPTIONS.scallopRadius,
    },
    leader: { points: [tip, knee, connection] },
    textBox,
    text: DEFAULT_TEXT,
    color: '#ff0000',
    source: { source: 'butter' },
  });
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
    return { ...generated, d: markup.cloud.appearancePath };
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
