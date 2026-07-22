const SINGLE_PAGE_WHEEL_DELTA_PER_PAGE = 80;

export interface SinglePageWheelNavigationInput {
  readonly pageIndices: readonly number[];
  readonly currentPage: number;
  readonly deltaX: number;
  readonly deltaY: number;
  readonly accumulatedDelta: number;
}

export interface SinglePageWheelNavigationResult {
  readonly pageIndex: number | null;
  readonly accumulatedDelta: number;
}

export function resolveSinglePageWheelNavigation({
  pageIndices,
  currentPage,
  deltaX,
  deltaY,
  accumulatedDelta,
}: SinglePageWheelNavigationInput): SinglePageWheelNavigationResult {
  if (pageIndices.length <= 1) {
    return { pageIndex: null, accumulatedDelta: 0 };
  }

  const dominantDelta = Math.abs(deltaY) >= Math.abs(deltaX) ? deltaY : deltaX;
  if (!Number.isFinite(dominantDelta) || Math.abs(dominantDelta) <= Number.EPSILON) {
    return { pageIndex: null, accumulatedDelta };
  }

  const nextAccumulatedDelta = accumulatedDelta + dominantDelta;
  const absAccumulatedDelta = Math.abs(nextAccumulatedDelta);
  if (absAccumulatedDelta < SINGLE_PAGE_WHEEL_DELTA_PER_PAGE) {
    return { pageIndex: null, accumulatedDelta: nextAccumulatedDelta };
  }

  const direction = Math.sign(nextAccumulatedDelta);
  const currentPosition = Math.max(0, pageIndices.indexOf(currentPage));
  const targetPosition = clamp(currentPosition + direction, 0, pageIndices.length - 1);

  if (targetPosition === currentPosition) {
    return { pageIndex: null, accumulatedDelta: 0 };
  }

  return {
    pageIndex: pageIndices[targetPosition] ?? null,
    accumulatedDelta: 0,
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
