import { describe, expect, it } from 'vitest';
import {
  PAGE_INLINE_ROTATION_ACTIONS_MIN_WIDTH,
  resolveNearbyThumbnailWarmCandidates,
  shouldShowInlinePageRotationActions,
  shouldWarmNearbyThumbnails,
} from './PageThumbnailList';

describe('page thumbnail actions', () => {
  it('shows both rotation buttons at normal sidebar widths and collapses them when narrow', () => {
    expect(shouldShowInlinePageRotationActions(PAGE_INLINE_ROTATION_ACTIONS_MIN_WIDTH)).toBe(true);
    expect(shouldShowInlinePageRotationActions(PAGE_INLINE_ROTATION_ACTIONS_MIN_WIDTH - 1)).toBe(false);
  });
});

describe('page thumbnail nearby warming', () => {
  const idleDiagnostics = {
    pageRenderReady: true,
    thumbnailRenderReady: true,
    queuedPageRenders: 0,
    queuedThumbnailRenders: 0,
    inflightPageRenders: 0,
    inflightThumbnailRenders: 0,
    viewportInMotion: false,
  };

  it('warms only when both render queues are idle and motion has settled', () => {
    expect(shouldWarmNearbyThumbnails({
      diagnostics: idleDiagnostics,
      thumbnailListInMotion: false,
      hasStrictVisibleThumbnails: true,
    })).toBe(true);
    expect(shouldWarmNearbyThumbnails({
      diagnostics: { ...idleDiagnostics, inflightPageRenders: 1 },
      thumbnailListInMotion: false,
      hasStrictVisibleThumbnails: true,
    })).toBe(false);
    expect(shouldWarmNearbyThumbnails({
      diagnostics: { ...idleDiagnostics, viewportInMotion: true },
      thumbnailListInMotion: false,
      hasStrictVisibleThumbnails: true,
    })).toBe(false);
    expect(shouldWarmNearbyThumbnails({
      diagnostics: idleDiagnostics,
      thumbnailListInMotion: true,
      hasStrictVisibleThumbnails: true,
    })).toBe(false);
  });

  it('caps nearby thumbnail warming candidates to two thumbnails', () => {
    expect(resolveNearbyThumbnailWarmCandidates({
      startIndex: 4,
      endIndex: 6,
      pageCount: 12,
    })).toEqual([3, 7]);
  });
});
