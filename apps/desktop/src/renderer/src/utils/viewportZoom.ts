export interface AnchoredZoomAxisResolution {
  readonly scrollOffset: number;
  readonly contentOffset: number;
}

export function resolveAnchoredZoomAxis(
  anchorContentPosition: number,
  viewportLocalPosition: number,
  maxScrollOffset: number,
): AnchoredZoomAxisResolution {
  const desiredScrollOffset = anchorContentPosition - viewportLocalPosition;
  const scrollOffset = clampScrollOffset(desiredScrollOffset, maxScrollOffset);

  return {
    scrollOffset,
    contentOffset: viewportLocalPosition + scrollOffset - anchorContentPosition,
  };
}

function clampScrollOffset(value: number, maxScrollOffset: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }

  const maxOffset = Number.isFinite(maxScrollOffset) ? Math.max(0, maxScrollOffset) : 0;
  return Math.min(maxOffset, Math.max(0, value));
}
