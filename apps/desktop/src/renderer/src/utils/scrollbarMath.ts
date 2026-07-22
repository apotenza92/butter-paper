export interface ScrollbarAxisInput {
  clientSize: number;
  scrollSize: number;
  scrollOffset: number;
  trackSize: number;
  minThumbSize: number;
}

export interface ScrollbarAxisMetrics {
  hasOverflow: boolean;
  maxScrollOffset: number;
  trackSize: number;
  thumbSize: number;
  thumbOffset: number;
  maxThumbOffset: number;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function getScrollbarAxisMetrics({
  clientSize,
  scrollSize,
  scrollOffset,
  trackSize,
  minThumbSize,
}: ScrollbarAxisInput): ScrollbarAxisMetrics {
  const safeClientSize = Math.max(0, clientSize);
  const safeScrollSize = Math.max(safeClientSize, scrollSize);
  const safeTrackSize = Math.max(0, trackSize);
  const maxScrollOffset = Math.max(0, safeScrollSize - safeClientSize);

  if (safeClientSize === 0 || maxScrollOffset === 0) {
    return {
      hasOverflow: false,
      maxScrollOffset,
      trackSize: safeTrackSize,
      thumbSize: 0,
      thumbOffset: 0,
      maxThumbOffset: 0,
    };
  }

  if (safeTrackSize === 0) {
    return {
      hasOverflow: true,
      maxScrollOffset,
      trackSize: safeTrackSize,
      thumbSize: 0,
      thumbOffset: 0,
      maxThumbOffset: 0,
    };
  }

  const proportionalThumbSize = (safeClientSize / safeScrollSize) * safeTrackSize;
  const thumbSize = clamp(Math.round(proportionalThumbSize), Math.min(minThumbSize, safeTrackSize), safeTrackSize);
  const maxThumbOffset = Math.max(0, safeTrackSize - thumbSize);
  const normalizedScrollOffset = clamp(scrollOffset, 0, maxScrollOffset);
  const thumbOffset =
    maxScrollOffset === 0 || maxThumbOffset === 0
      ? 0
      : Math.round((normalizedScrollOffset / maxScrollOffset) * maxThumbOffset);

  return {
    hasOverflow: true,
    maxScrollOffset,
    trackSize: safeTrackSize,
    thumbSize,
    thumbOffset,
    maxThumbOffset,
  };
}

export function mapTrackClickToScrollOffset(clickOffset: number, metrics: ScrollbarAxisMetrics): number {
  if (!metrics.hasOverflow || metrics.maxScrollOffset === 0 || metrics.trackSize === 0) {
    return 0;
  }

  const centeredOffset = clickOffset - metrics.thumbSize / 2;
  const normalizedThumbOffset = clamp(centeredOffset, 0, metrics.maxThumbOffset);
  if (metrics.maxThumbOffset === 0) {
    return 0;
  }

  return Math.round((normalizedThumbOffset / metrics.maxThumbOffset) * metrics.maxScrollOffset);
}

export function mapThumbDragDeltaToScrollOffset(delta: number, startScrollOffset: number, metrics: ScrollbarAxisMetrics): number {
  if (!metrics.hasOverflow || metrics.maxScrollOffset === 0 || metrics.maxThumbOffset === 0) {
    return 0;
  }

  const scrollPerPixel = metrics.maxScrollOffset / metrics.maxThumbOffset;
  return clamp(Math.round(startScrollOffset + delta * scrollPerPixel), 0, metrics.maxScrollOffset);
}
