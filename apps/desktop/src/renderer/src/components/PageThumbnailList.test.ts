import { describe, expect, it } from 'vitest';
import {
  resolveNearbyThumbnailWarmCandidates,
  shouldWarmNearbyThumbnails,
} from './PageThumbnailList';

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
