export interface CanvasPadding {
  readonly left: number;
  readonly right: number;
  readonly top: number;
  readonly bottom: number;
}

export const EMPTY_CANVAS_PADDING: CanvasPadding = { left: 0, right: 0, top: 0, bottom: 0 };

export function getEffectiveCanvasPadding(
  pages: readonly { index: number; width: number; height: number }[],
  viewportWidth: number,
  viewportHeight: number,
  zoomPreset: 'manual' | 'fit-width' | 'fit-page',
  canvasPadding: CanvasPadding,
): CanvasPadding {
  if (pages.length === 0 || viewportWidth <= 0 || viewportHeight <= 0) {
    return EMPTY_CANVAS_PADDING;
  }

  return canvasPadding;
}

export function areCanvasPaddingsEqual(left: CanvasPadding, right: CanvasPadding): boolean {
  return left.left === right.left
    && left.right === right.right
    && left.top === right.top
    && left.bottom === right.bottom;
}
