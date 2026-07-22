import { describe, expect, it } from 'vitest';
import {
  getScrollbarAxisMetrics,
  mapThumbDragDeltaToScrollOffset,
  mapTrackClickToScrollOffset,
} from './scrollbarMath';

describe('scrollbarMath', () => {
  it('returns no-overflow metrics when content fits', () => {
    expect(
      getScrollbarAxisMetrics({
        clientSize: 200,
        scrollSize: 200,
        scrollOffset: 0,
        trackSize: 140,
        minThumbSize: 28,
      }),
    ).toEqual({
      hasOverflow: false,
      maxScrollOffset: 0,
      trackSize: 140,
      thumbSize: 0,
      thumbOffset: 0,
      maxThumbOffset: 0,
    });
  });

  it('calculates proportional thumb size and offset', () => {
    expect(
      getScrollbarAxisMetrics({
        clientSize: 200,
        scrollSize: 800,
        scrollOffset: 300,
        trackSize: 160,
        minThumbSize: 28,
      }),
    ).toEqual({
      hasOverflow: true,
      maxScrollOffset: 600,
      trackSize: 160,
      thumbSize: 40,
      thumbOffset: 60,
      maxThumbOffset: 120,
    });
  });

  it('detects overflow before a custom track is mounted', () => {
    expect(
      getScrollbarAxisMetrics({
        clientSize: 200,
        scrollSize: 800,
        scrollOffset: 0,
        trackSize: 0,
        minThumbSize: 28,
      }),
    ).toMatchObject({
      hasOverflow: true,
      maxScrollOffset: 600,
      trackSize: 0,
      thumbSize: 0,
    });
  });

  it('respects the minimum thumb size', () => {
    expect(
      getScrollbarAxisMetrics({
        clientSize: 100,
        scrollSize: 10_000,
        scrollOffset: 0,
        trackSize: 90,
        minThumbSize: 32,
      }).thumbSize,
    ).toBe(32);
  });

  it('maps track clicks by centered click ratio', () => {
    const metrics = getScrollbarAxisMetrics({
      clientSize: 200,
      scrollSize: 800,
      scrollOffset: 0,
      trackSize: 160,
      minThumbSize: 28,
    });

    expect(mapTrackClickToScrollOffset(80, metrics)).toBe(300);
    expect(mapTrackClickToScrollOffset(0, metrics)).toBe(0);
    expect(mapTrackClickToScrollOffset(160, metrics)).toBe(600);
  });

  it('maps thumb drag deltas back to scroll offset', () => {
    const metrics = getScrollbarAxisMetrics({
      clientSize: 200,
      scrollSize: 800,
      scrollOffset: 150,
      trackSize: 160,
      minThumbSize: 28,
    });

    expect(mapThumbDragDeltaToScrollOffset(20, 150, metrics)).toBe(250);
    expect(mapThumbDragDeltaToScrollOffset(-1000, 150, metrics)).toBe(0);
    expect(mapThumbDragDeltaToScrollOffset(1000, 150, metrics)).toBe(600);
  });
});
