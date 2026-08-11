// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import type { PageModel } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LocalPdfSession } from '../services/documentSession';
import { PageThumbnailItem } from './PageThumbnailItem';

describe('PageThumbnailItem interactions', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  });

  it('uses one full-item selection button while keeping the label and page actions separate', () => {
    const onSelect = vi.fn();
    const onSetPageScale = vi.fn();
    const onRotate = vi.fn();
    const session = createSessionStub();

    act(() => {
      root.render(
        createElement(PageThumbnailItem, {
          session,
          page: {
            index: 0,
            rotation: 0,
            size: { width: 612, height: 792 },
          } as PageModel,
          top: 0,
          previewWidth: 188,
          previewHeight: 220,
          itemHeight: 284,
          markups: [],
          isActive: false,
          renderPriority: 2000,
          renderUrgency: 'visible',
          sessionVersion: 0,
          onSelect,
          onSetPageScale,
          onRotate,
        }),
      );
    });

    const item = host.querySelector<HTMLElement>('[data-testid="page-thumbnail-item-1"]');
    const selection = host.querySelector<HTMLButtonElement>('[data-testid="page-thumbnail-select-1"]');
    const actions = host.querySelector<HTMLElement>('[data-testid="page-thumbnail-actions-1"]');
    const label = host.querySelector<HTMLElement>('[data-testid="page-thumbnail-label-1"]');
    const preview = host.querySelector<HTMLElement>('[data-testid="page-thumbnail-preview-1"]');
    const setScale = host.querySelector<HTMLButtonElement>('[data-testid="page-thumbnail-set-scale-1"]');
    const rotateLeft = host.querySelector<HTMLButtonElement>('[data-testid="page-thumbnail-rotate-left-1"]');
    const rotateRight = host.querySelector<HTMLButtonElement>('[data-testid="page-thumbnail-rotate-right-1"]');

    expect(item?.getAttribute('data-variant')).toBe('outline');
    expect(selection?.className).toContain('absolute');
    expect(selection?.className).toContain('inset-0');
    expect(actions?.className).toContain('relative');
    expect(preview?.className).toContain('relative');
    expect(item?.contains(label)).toBe(true);
    expect(item?.contains(preview)).toBe(true);
    expect(label?.closest('button')).toBeNull();
    expect(selection?.contains(label)).toBe(false);
    expect(selection?.contains(preview)).toBe(false);
    expect(selection?.contains(setScale)).toBe(false);
    expect(selection?.contains(rotateLeft)).toBe(false);
    expect(selection?.contains(rotateRight)).toBe(false);
    expect(setScale?.className).toContain('size-8');
    expect(rotateLeft?.className).toContain('size-8');
    expect(rotateRight?.className).toContain('size-8');
    expect(host.querySelector('[data-testid="page-thumbnail-more-1"]')).toBeNull();

    act(() => selection?.click());
    act(() => setScale?.click());
    act(() => rotateLeft?.click());
    act(() => rotateRight?.click());

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenNthCalledWith(1, null);
    expect(onSetPageScale).toHaveBeenCalledOnce();
    expect(onRotate).toHaveBeenNthCalledWith(1, 'left');
    expect(onRotate).toHaveBeenNthCalledWith(2, 'right');
  });
});

function createSessionStub(): LocalPdfSession {
  return {
    getBestReusableThumbnailImage: vi.fn(() => null),
    getBestReusablePageImage: vi.fn(() => null),
    renderThumbnail: vi.fn(() => new Promise<string>(() => undefined)),
    updateThumbnailRenderPriority: vi.fn(),
    releasePageSurface: vi.fn(),
    retainPageImageUrl: vi.fn(),
    releasePageImageUrl: vi.fn(),
  } as unknown as LocalPdfSession;
}
