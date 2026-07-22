import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  isRenderCoordinatorV2Enabled,
  renderRequestClassForRole,
  resolveCoordinatorStateAfterSource,
  resolveRenderCoordinatorTier,
  resolveStateAfterRenderAbort,
  sourceKindForReusableImage,
} from './renderCoordinator';
import type { ReusablePageImage } from './documentSession';

describe('render coordinator policy', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('reads the v2 rollout flag from the desktop environment', () => {
    vi.stubGlobal('window', {
      butterPaper: {
        environment: {
          renderCoordinatorV2: true,
        },
      },
    });

    expect(isRenderCoordinatorV2Enabled()).toBe(true);
  });

  it('keeps a current visible source after aborts instead of blanking', () => {
    expect(resolveStateAfterRenderAbort('showing-preview')).toBe('showing-preview');
    expect(resolveStateAfterRenderAbort('showing-full')).toBe('showing-full');
  });

  it('does not clear valid state when no replacement source is ready', () => {
    expect(resolveCoordinatorStateAfterSource({
      currentState: 'showing-preview',
      hasDisplayedSource: true,
      nextQuality: null,
    })).toBe('showing-preview');
  });

  it('prioritises visible blank content ahead of upgrades and warming', () => {
    expect(resolveRenderCoordinatorTier({
      role: 'target-page',
      pageIndex: 4,
      isVisible: true,
      hasDisplayedSource: false,
      displayedQuality: null,
      viewportInMotion: true,
      renderUrgency: 'visible',
    })).toBe(1);
    expect(resolveRenderCoordinatorTier({
      role: 'overview-page',
      pageIndex: 4,
      isVisible: true,
      hasDisplayedSource: true,
      displayedQuality: 'preview',
      viewportInMotion: true,
      renderUrgency: 'visible',
    })).toBe(4);
    expect(resolveRenderCoordinatorTier({
      role: 'main-page',
      pageIndex: 4,
      isVisible: false,
      hasDisplayedSource: false,
      displayedQuality: null,
      viewportInMotion: false,
      renderUrgency: 'prefetch',
    })).toBe(5);
  });

  it('maps roles to existing session request classes', () => {
    expect(renderRequestClassForRole({ role: 'target-page', quality: 'preview', urgency: 'visible' })).toBe('target-page-preview');
    expect(renderRequestClassForRole({ role: 'main-page', quality: 'full', urgency: 'visible' })).toBe('visible-page-hq-upgrade');
    expect(renderRequestClassForRole({ role: 'overview-page', quality: 'preview', urgency: 'visible' })).toBe('overview-thumbnail');
    expect(renderRequestClassForRole({ role: 'sidebar-thumbnail', quality: 'preview', urgency: 'visible' })).toBe('visible-thumbnail');
    expect(renderRequestClassForRole({ role: 'target-page', quality: 'full', urgency: 'prefetch' })).toBe('nearby-prefetch');
  });

  it('normalises reusable source kinds across page, thumbnail, and overview caches', () => {
    const surface = {
      kind: 'surface',
      source: 'page-bitmap',
    } as ReusablePageImage;
    const thumbnail = {
      kind: 'object-url',
      source: 'thumbnail',
      sourceRequestClass: 'visible-thumbnail',
    } as ReusablePageImage;
    const overview = {
      kind: 'object-url',
      source: 'thumbnail',
      sourceRequestClass: 'overview-thumbnail',
    } as ReusablePageImage;

    expect(sourceKindForReusableImage(surface)).toBe('page-surface');
    expect(sourceKindForReusableImage(thumbnail)).toBe('thumbnail-url');
    expect(sourceKindForReusableImage(overview)).toBe('overview-url');
  });
});
