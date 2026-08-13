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
          pageScale: {
            pageIndex: 0,
            source: 'preset',
            name: '1:100',
            pdfUnits: 'cm',
            realUnits: 'm',
            scaleX: 1,
            scaleY: 1,
            precision: { mode: 'decimal', value: 0.01 },
          },
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
    const scalePosition = host.querySelector<HTMLElement>('[data-testid="page-thumbnail-scale-position-1"]');
    const scaleBadge = host.querySelector<HTMLElement>('[data-testid="page-thumbnail-scale-badge-1"]');
    const preview = host.querySelector<HTMLElement>('[data-testid="page-thumbnail-preview-1"]');
    const setScale = host.querySelector<HTMLButtonElement>('[data-testid="page-thumbnail-set-scale-1"]');
    const rotateLeft = host.querySelector<HTMLButtonElement>('[data-testid="page-thumbnail-rotate-left-1"]');
    const rotateRight = host.querySelector<HTMLButtonElement>('[data-testid="page-thumbnail-rotate-right-1"]');

    expect(item?.getAttribute('data-variant')).toBe('outline');
    expect(item?.getAttribute('data-selected')).toBeNull();
    expect(item?.className).toContain('left-2');
    expect(item?.className).toContain('right-2');
    expect(item?.className).toContain('rounded-lg');
    expect(item?.className).toContain('border-border');
    expect(item?.className).not.toContain('border-[0.5px]');
    expect(selection?.className).toContain('absolute');
    expect(selection?.className).toContain('inset-0');
    expect(selection?.className).toContain('z-0');
    expect(selection?.className).toContain('rounded-lg');
    expect(actions?.className).toContain('relative');
    expect(actions?.className).toContain('z-10');
    expect(preview?.className).toContain('relative');
    expect(preview?.className).toContain('z-10');
    expect(host.querySelector('[data-testid="page-thumbnail-content-1"]')?.className).not.toContain('bp-current-page-outline');
    expect(item?.contains(label)).toBe(true);
    expect(scalePosition?.className).toContain('flex-1');
    expect(scalePosition?.className).toContain('justify-center');
    expect(scalePosition?.contains(scaleBadge ?? null)).toBe(true);
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
    expect(host.querySelector('[data-testid="page-thumbnail-separator-1"]')).toBeNull();
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

  it('uses the stock muted item state for the active page without an extra preview outline', () => {
    act(() => {
      root.render(
        createElement(PageThumbnailItem, {
          session: createSessionStub(),
          page: {
            id: 'page-1',
            index: 0,
            rotation: 0,
            size: { width: 612, height: 792 },
          } as PageModel,
          top: 0,
          previewWidth: 188,
          previewHeight: 133,
          itemHeight: 195,
          markups: [],
          isActive: true,
          renderPriority: 2000,
          renderUrgency: 'visible',
          sessionVersion: 0,
          onSelect: () => undefined,
          onSetPageScale: () => undefined,
          onRotate: () => undefined,
        }),
      );
    });

    const item = host.querySelector<HTMLElement>('[data-testid="page-thumbnail-item-1"]');
    const selection = host.querySelector<HTMLButtonElement>('[data-testid="page-thumbnail-select-1"]');
    const content = host.querySelector<HTMLElement>('[data-testid="page-thumbnail-content-1"]');

    expect(item?.getAttribute('data-variant')).toBe('muted');
    expect(item?.getAttribute('data-selected')).toBe('true');
    expect(item?.className).toContain('bg-muted/50');
    expect(item?.className).toContain('border-transparent');
    expect(selection?.getAttribute('aria-current')).toBe('page');
    expect(content?.className).not.toContain('bp-current-page-outline');
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
