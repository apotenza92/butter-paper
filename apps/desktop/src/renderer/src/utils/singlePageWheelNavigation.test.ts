import { describe, expect, it } from 'vitest';
import { resolveSinglePageWheelNavigation } from './singlePageWheelNavigation';

describe('single-page wheel navigation', () => {
  const pageIndices = [0, 1, 2, 3, 4];

  it('snaps to the next or previous page from wheel direction', () => {
    expect(resolveSinglePageWheelNavigation({
      pageIndices,
      currentPage: 2,
      deltaX: 0,
      deltaY: 100,
      accumulatedDelta: 0,
    })).toEqual({ pageIndex: 3, accumulatedDelta: 0 });

    expect(resolveSinglePageWheelNavigation({
      pageIndices,
      currentPage: 2,
      deltaX: 0,
      deltaY: -100,
      accumulatedDelta: 0,
    })).toEqual({ pageIndex: 1, accumulatedDelta: 0 });
  });

  it('accumulates small trackpad deltas until they cross a page step', () => {
    const first = resolveSinglePageWheelNavigation({
      pageIndices,
      currentPage: 1,
      deltaX: 0,
      deltaY: 30,
      accumulatedDelta: 0,
    });

    expect(first).toEqual({ pageIndex: null, accumulatedDelta: 30 });
    expect(resolveSinglePageWheelNavigation({
      pageIndices,
      currentPage: 1,
      deltaX: 0,
      deltaY: 50,
      accumulatedDelta: first.accumulatedDelta,
    })).toEqual({ pageIndex: 2, accumulatedDelta: 0 });
  });

  it('uses horizontal wheel deltas when they dominate', () => {
    expect(resolveSinglePageWheelNavigation({
      pageIndices,
      currentPage: 2,
      deltaX: -120,
      deltaY: 10,
      accumulatedDelta: 0,
    })).toEqual({ pageIndex: 1, accumulatedDelta: 0 });
  });

  it('moves one page for a large wheel tick and clamps at document edges', () => {
    expect(resolveSinglePageWheelNavigation({
      pageIndices,
      currentPage: 0,
      deltaX: 0,
      deltaY: 300,
      accumulatedDelta: 0,
    })).toEqual({ pageIndex: 1, accumulatedDelta: 0 });

    expect(resolveSinglePageWheelNavigation({
      pageIndices,
      currentPage: 4,
      deltaX: 0,
      deltaY: 100,
      accumulatedDelta: 0,
    })).toEqual({ pageIndex: null, accumulatedDelta: 0 });
  });
});
