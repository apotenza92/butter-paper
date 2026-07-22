import { describe, expect, it } from 'vitest';
import { resolveAnchoredZoomAxis } from './viewportZoom';

describe('viewport zoom helpers', () => {
  it('uses native scroll when the cursor anchor is inside scroll bounds', () => {
    expect(resolveAnchoredZoomAxis(420, 120, 600)).toEqual({
      scrollOffset: 300,
      contentOffset: 0,
    });
  });

  it('shifts content forward when the desired scroll would go before the start', () => {
    expect(resolveAnchoredZoomAxis(80, 180, 600)).toEqual({
      scrollOffset: 0,
      contentOffset: 100,
    });
  });

  it('shifts content backward when the desired scroll exceeds the end', () => {
    expect(resolveAnchoredZoomAxis(980, 180, 600)).toEqual({
      scrollOffset: 600,
      contentOffset: -200,
    });
  });

  it('anchors without overflow by translating the content instead of scrolling', () => {
    expect(resolveAnchoredZoomAxis(260, 460, 0)).toEqual({
      scrollOffset: 0,
      contentOffset: 200,
    });
    expect(resolveAnchoredZoomAxis(660, 460, 0)).toEqual({
      scrollOffset: 0,
      contentOffset: -200,
    });
  });
});
