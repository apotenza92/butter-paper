import { pdfPoint, rect, rectFromPoints, type Markup, type PdfPoint, type Rect } from '@butter-paper/core';
import type { BodyDragBehavior, HandleBehavior } from './types';

export const MIN_RECTANGLE_SIZE_PDF = 2;
export const POINTER_DRAG_THRESHOLD_PX = 3;

export type RectangleDraft = {
  readonly kind: 'rectangle';
  readonly start: PdfPoint;
  readonly current: PdfPoint;
};

export type TextBoxDraft = {
  readonly kind: 'text-box';
  readonly start: PdfPoint;
  readonly current: PdfPoint;
};

export type LineDraft = {
  readonly kind: 'line';
  readonly start: PdfPoint;
  readonly current: PdfPoint;
};

export type ArcDraft = {
  readonly kind: 'arc';
  readonly phase: 'end' | 'mid';
  readonly start: PdfPoint;
  readonly end?: PdfPoint;
  readonly current: PdfPoint;
};

export type CloudNodeDraft = {
  readonly kind: 'cloud-node';
  readonly start: PdfPoint;
  readonly current: PdfPoint;
  readonly points: readonly PdfPoint[];
};

export type InkDraft = {
  readonly kind: 'ink';
  readonly start: PdfPoint;
  readonly current: PdfPoint;
  readonly points: readonly PdfPoint[];
};

export type MeasurementPathDraft = {
  readonly kind: 'measurement-path';
  readonly tool: 'polylength' | 'area';
  readonly start: PdfPoint;
  readonly current: PdfPoint;
  readonly points: readonly PdfPoint[];
};

export type MoveDraft = {
  readonly kind: 'move';
  readonly pointerId: number;
  readonly markupIds: readonly string[];
  readonly lastPoint: PdfPoint;
  readonly componentId?: string;
  readonly bodyDrag?: BodyDragBehavior;
};

export type TransformDraft = {
  readonly kind: 'transform';
  readonly pointerId: number;
  readonly markupId: string;
  readonly originalMarkup: Markup;
  readonly handleId: string;
  readonly handleBehavior: HandleBehavior;
  readonly startPoint: PdfPoint;
  readonly currentPoint: PdfPoint;
  readonly dragStarted: boolean;
};

export type AnnotationDraft = RectangleDraft | TextBoxDraft | LineDraft | ArcDraft | CloudNodeDraft | InkDraft | MeasurementPathDraft | MoveDraft | TransformDraft;

export function createRectangleDraft(start: PdfPoint): RectangleDraft {
  return { kind: 'rectangle', start, current: start };
}

export function updateRectangleDraft(draft: RectangleDraft, current: PdfPoint): RectangleDraft {
  return { ...draft, current };
}

export function createTextBoxDraft(start: PdfPoint): TextBoxDraft {
  return { kind: 'text-box', start, current: start };
}

export function updateTextBoxDraft(draft: TextBoxDraft, current: PdfPoint): TextBoxDraft {
  return { ...draft, current };
}

export function createLineDraft(start: PdfPoint): LineDraft {
  return { kind: 'line', start, current: start };
}

export function updateLineDraft(draft: LineDraft, current: PdfPoint): LineDraft {
  return { ...draft, current };
}

export function createArcDraft(start: PdfPoint): ArcDraft {
  return { kind: 'arc', phase: 'end', start, current: start };
}

export function setArcDraftEnd(draft: ArcDraft, end: PdfPoint): ArcDraft {
  return { ...draft, phase: 'mid', end, current: midpoint(draft.start, end) };
}

export function updateArcDraft(draft: ArcDraft, current: PdfPoint): ArcDraft {
  return { ...draft, current };
}

export function createCloudNodeDraft(start: PdfPoint): CloudNodeDraft {
  return { kind: 'cloud-node', start, current: start, points: [start] };
}

export function beginCloudNodeDraftPoint(draft: CloudNodeDraft, point: PdfPoint): CloudNodeDraft {
  return { ...draft, start: point, current: point };
}

export function updateCloudNodeDraft(draft: CloudNodeDraft, current: PdfPoint): CloudNodeDraft {
  return { ...draft, current };
}

export function addCloudNodeDraftPoint(draft: CloudNodeDraft, point: PdfPoint): CloudNodeDraft {
  const lastPoint = draft.points[draft.points.length - 1] ?? draft.start;
  const points = Math.hypot(point.x - lastPoint.x, point.y - lastPoint.y) < 0.5
    ? draft.points
    : [...draft.points, point];

  return { ...draft, start: point, current: point, points };
}

export function createInkDraft(start: PdfPoint): InkDraft {
  return { kind: 'ink', start, current: start, points: [start] };
}

export function updateInkDraft(draft: InkDraft, current: PdfPoint): InkDraft {
  const lastPoint = draft.points[draft.points.length - 1] ?? draft.start;
  const shouldAppend = Math.hypot(current.x - lastPoint.x, current.y - lastPoint.y) >= 0.5;
  return {
    ...draft,
    current,
    points: shouldAppend ? [...draft.points, current] : draft.points,
  };
}

export function createMeasurementPathDraft(tool: MeasurementPathDraft['tool'], start: PdfPoint): MeasurementPathDraft {
  return { kind: 'measurement-path', tool, start, current: start, points: [start] };
}

export function updateMeasurementPathDraft(draft: MeasurementPathDraft, current: PdfPoint): MeasurementPathDraft {
  return { ...draft, current };
}

export function addMeasurementPathDraftPoint(draft: MeasurementPathDraft, point: PdfPoint): MeasurementPathDraft {
  const lastPoint = draft.points[draft.points.length - 1] ?? draft.start;
  const points = Math.hypot(point.x - lastPoint.x, point.y - lastPoint.y) < 0.5
    ? draft.points
    : [...draft.points, point];
  return { ...draft, start: point, current: point, points };
}

export function measurementPathPreviewPoints(draft: MeasurementPathDraft): readonly PdfPoint[] {
  const lastPoint = draft.points[draft.points.length - 1] ?? draft.start;
  if (Math.hypot(draft.current.x - lastPoint.x, draft.current.y - lastPoint.y) < 0.5) {
    return draft.points;
  }
  return [...draft.points, draft.current];
}

export function textBoxDraftToRect(draft: TextBoxDraft): Rect {
  return rectFromPoints(draft.start, draft.current);
}

export function rectangleDraftToRect(draft: RectangleDraft): Rect {
  return rectFromPoints(draft.start, draft.current);
}

export function shouldCommitRectangle(rect: Rect, minimumSize = MIN_RECTANGLE_SIZE_PDF): boolean {
  return rect.width > minimumSize && rect.height > minimumSize;
}

export function shouldCommitLine(start: PdfPoint, end: PdfPoint, minimumLength = MIN_RECTANGLE_SIZE_PDF): boolean {
  return Math.hypot(end.x - start.x, end.y - start.y) > minimumLength;
}

function midpoint(start: PdfPoint, end: PdfPoint): PdfPoint {
  return pdfPoint((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
}

export function createMoveDraft(
  pointerId: number,
  markupIds: readonly string[],
  point: PdfPoint,
  options: Pick<MoveDraft, 'componentId' | 'bodyDrag'> = {},
): MoveDraft {
  return { kind: 'move', pointerId, markupIds, lastPoint: point, ...options };
}

export function moveDelta(draft: MoveDraft, point: PdfPoint): PdfPoint {
  return pdfPoint(point.x - draft.lastPoint.x, point.y - draft.lastPoint.y);
}

export function hasExceededDragThreshold(start: { x: number; y: number }, current: { x: number; y: number }, threshold = POINTER_DRAG_THRESHOLD_PX): boolean {
  return Math.hypot(current.x - start.x, current.y - start.y) >= threshold;
}

export function createTransformDraft(
  pointerId: number,
  originalMarkup: Markup,
  handleId: string,
  handleBehavior: HandleBehavior,
  point: PdfPoint,
): TransformDraft {
  return {
    kind: 'transform',
    pointerId,
    markupId: originalMarkup.id,
    originalMarkup,
    handleId,
    handleBehavior,
    startPoint: point,
    currentPoint: point,
    dragStarted: false,
  };
}

export function updateTransformDraft(draft: TransformDraft, currentPoint: PdfPoint, dragStarted = draft.dragStarted): TransformDraft {
  return { ...draft, currentPoint, dragStarted: draft.dragStarted || dragStarted };
}

export type RectResizeHandle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w';

export function resizeRectFromHandle(original: Rect, handle: RectResizeHandle, point: PdfPoint, minimumSize = MIN_RECTANGLE_SIZE_PDF): Rect {
  const originalLeft = original.x;
  const originalBottom = original.y;
  const originalRight = original.x + original.width;
  const originalTop = original.y + original.height;
  let left = originalLeft;
  let bottom = originalBottom;
  let right = originalRight;
  let top = originalTop;

  if (handle.includes('w')) {
    left = Math.min(point.x, originalRight - minimumSize);
  }
  if (handle.includes('e')) {
    right = Math.max(point.x, originalLeft + minimumSize);
  }
  if (handle.includes('n')) {
    top = Math.max(point.y, originalBottom + minimumSize);
  }
  if (handle.includes('s')) {
    bottom = Math.min(point.y, originalTop - minimumSize);
  }

  return rect(left, bottom, right - left, top - bottom);
}

export function resizeRotatedRectFromHandle(
  original: Rect,
  rotation: number | undefined,
  handle: RectResizeHandle,
  point: PdfPoint,
  minimumSize = MIN_RECTANGLE_SIZE_PDF,
): Rect {
  if (!rotation) {
    return resizeRectFromHandle(original, handle, point, minimumSize);
  }

  const anchorBefore = getOppositeResizeAnchor(original, handle);
  const anchorBeforeWorld = rotatePointAroundRectCenter(anchorBefore, original, -rotation);
  const localPoint = rotatePointAroundRectCenter(point, original, rotation);
  const resized = resizeRectFromHandle(original, handle, localPoint, minimumSize);
  const anchorAfter = getOppositeResizeAnchor(resized, handle);
  const anchorAfterWorld = rotatePointAroundRectCenter(anchorAfter, resized, -rotation);
  const delta = pdfPoint(anchorBeforeWorld.x - anchorAfterWorld.x, anchorBeforeWorld.y - anchorAfterWorld.y);

  return rect(resized.x + delta.x, resized.y + delta.y, resized.width, resized.height);
}

function getOppositeResizeAnchor(original: Rect, handle: RectResizeHandle): PdfPoint {
  const left = original.x;
  const right = original.x + original.width;
  const bottom = original.y;
  const top = original.y + original.height;
  const centerX = original.x + original.width * 0.5;
  const centerY = original.y + original.height * 0.5;
  const x = handle.includes('w') ? right : handle.includes('e') ? left : centerX;
  const y = handle.includes('s') ? top : handle.includes('n') ? bottom : centerY;

  return pdfPoint(x, y);
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
