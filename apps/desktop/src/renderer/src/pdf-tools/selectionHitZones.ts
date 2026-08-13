import type { PageTransform, PdfPoint, Rect } from '@butter-paper/core';
import { getChromeStyle, type ChromeBoundsKind, type InteractionState } from './interactionChrome';

export interface SelectionHitZoneOptions {
  readonly rotation?: number;
  readonly transform: PageTransform;
  readonly state?: InteractionState;
  readonly boundsKind?: ChromeBoundsKind;
  readonly edgeTolerancePx?: number;
}

export function isPointInSelectionChromeBounds(point: PdfPoint, rect: Rect, options: SelectionHitZoneOptions): boolean {
  const { chromeBox, testPoint } = getSelectionChromeViewportHitZone(point, rect, options);
  return isViewportPointInRect(testPoint, chromeBox);
}

export function isPointNearSelectionChromeEdge(point: PdfPoint, rect: Rect, options: SelectionHitZoneOptions): boolean {
  const { chromeBox, style, testPoint } = getSelectionChromeViewportHitZone(point, rect, options);
  const tolerance = options.edgeTolerancePx ?? 6;
  const expanded = expandViewportRect(chromeBox, tolerance);
  const contracted = expandViewportRect(chromeBox, -(style.boundsOutsetPx + tolerance));
  return isViewportPointInRect(testPoint, expanded) && !isViewportPointInRect(testPoint, contracted);
}

export function getSelectionChromeViewportHitZone(point: PdfPoint, rect: Rect, options: SelectionHitZoneOptions) {
  const box = options.transform.pdfRectToViewport(rect);
  const style = getChromeStyle(options.state ?? 'hovered', options.boundsKind ?? 'child');
  const chromeBox = expandViewportRect(box, style.boundsOutsetPx);
  const viewportPoint = options.transform.pdfToViewport(point);
  const testPoint = options.rotation
    ? unrotateViewportPointAroundBounds(viewportPoint, box, options.rotation)
    : viewportPoint;

  return {
    box,
    chromeBox,
    style,
    testPoint,
  };
}

export function expandViewportRect(
  rect: { readonly x: number; readonly y: number; readonly width: number; readonly height: number },
  amount: number,
) {
  return {
    x: rect.x - amount,
    y: rect.y - amount,
    width: Math.max(0, rect.width + amount * 2),
    height: Math.max(0, rect.height + amount * 2),
  };
}

export function projectChromeHandlePoint(
  point: { readonly x: number; readonly y: number },
  source: { readonly x: number; readonly y: number; readonly width: number; readonly height: number },
  target: { readonly x: number; readonly y: number; readonly width: number; readonly height: number },
  outsideOffsetPx?: number,
) {
  const sourceRight = source.x + source.width;
  const sourceBottom = source.y + source.height;
  const targetRight = target.x + target.width;
  const targetBottom = target.y + target.height;

  return {
    x: projectChromeAxis(point.x, source.x, sourceRight, target.x, targetRight, outsideOffsetPx),
    y: projectChromeAxis(point.y, source.y, sourceBottom, target.y, targetBottom, outsideOffsetPx),
  };
}

export function unrotateViewportPointAroundBounds(
  point: { readonly x: number; readonly y: number },
  bounds: { readonly x: number; readonly y: number; readonly width: number; readonly height: number },
  rotation: number,
) {
  const center = {
    x: bounds.x + bounds.width * 0.5,
    y: bounds.y + bounds.height * 0.5,
  };
  const radians = (-rotation * Math.PI) / 180;
  const dx = point.x - center.x;
  const dy = point.y - center.y;

  return {
    x: center.x + dx * Math.cos(radians) - dy * Math.sin(radians),
    y: center.y + dx * Math.sin(radians) + dy * Math.cos(radians),
  };
}

function projectChromeAxis(
  value: number,
  sourceStart: number,
  sourceEnd: number,
  targetStart: number,
  targetEnd: number,
  outsideOffsetPx?: number,
): number {
  if (value < sourceStart) {
    return targetStart - (outsideOffsetPx ?? sourceStart - value);
  }

  if (value > sourceEnd) {
    return targetEnd + (outsideOffsetPx ?? value - sourceEnd);
  }

  const sourceSize = sourceEnd - sourceStart;
  if (sourceSize === 0) {
    return targetStart + (targetEnd - targetStart) * 0.5;
  }

  return targetStart + (targetEnd - targetStart) * ((value - sourceStart) / sourceSize);
}

function isViewportPointInRect(point: { readonly x: number; readonly y: number }, rect: { readonly x: number; readonly y: number; readonly width: number; readonly height: number }): boolean {
  return point.x >= rect.x
    && point.x <= rect.x + rect.width
    && point.y >= rect.y
    && point.y <= rect.y + rect.height;
}
