// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ViewerToolbar } from './ViewerToolbar';

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

function hover(element: HTMLElement): void {
  const pointerOver = new MouseEvent('pointerover', { bubbles: true });
  Object.defineProperty(pointerOver, 'pointerType', { value: 'mouse' });
  element.dispatchEvent(pointerOver);
  element.dispatchEvent(new MouseEvent('mouseenter'));
}

const TOOLBAR_PROPS = {
  cadViewEnabled: true,
  zoom: 1,
  zoomPreset: 'manual' as const,
  scrollMode: 'continuous' as const,
  continuousScrollWheelMode: 'scroll' as const,
  singlePageScrollWheelMode: 'scroll' as const,
  pageColumnsEnabled: false,
  cadViewOrganisation: 'columns' as const,
  pagesPerColumn: 2,
  onZoomOut: () => undefined,
  onZoomReset: () => undefined,
  onZoomIn: () => undefined,
  onZoomChange: () => undefined,
  onFitWidth: () => undefined,
  onFitPage: () => undefined,
  onScrollModeChange: () => undefined,
  onContinuousScrollWheelModeChange: () => undefined,
  onSinglePageScrollWheelModeChange: () => undefined,
  onPageColumnsEnabledChange: () => undefined,
  onCadViewOrganisationChange: () => undefined,
  onPagesPerColumnChange: () => undefined,
};

describe('ViewerToolbar tooltip interactions', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', TestResizeObserver);
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    act(() => {
      root.render(
        createElement(
          TooltipProvider,
          { delay: 0 },
          createElement(ViewerToolbar, TOOLBAR_PROPS),
        ),
      );
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('keeps only the currently hovered toolbar tooltip visible after another control retained focus', () => {
    const fitWidth = host.querySelector<HTMLButtonElement>('[data-testid="viewer-fit-width"]');
    const fitPage = host.querySelector<HTMLButtonElement>('[data-testid="viewer-fit-page"]');
    const continuous = host.querySelector<HTMLButtonElement>('[data-testid="viewer-scroll-continuous"]');
    expect(fitWidth).toBeTruthy();
    expect(fitPage).toBeTruthy();
    expect(continuous).toBeTruthy();
    expect(fitWidth?.hasAttribute('data-toolbar-tooltip-trigger')).toBe(true);
    expect(fitPage?.hasAttribute('data-toolbar-tooltip-trigger')).toBe(true);
    expect(continuous?.hasAttribute('data-toolbar-tooltip-trigger')).toBe(true);

    act(() => fitWidth?.focus());
    act(() => fitPage && hover(fitPage));
    expect(document.activeElement).not.toBe(fitWidth);
    act(() => continuous && hover(continuous));

    const visibleTooltips = Array.from(
      document.body.querySelectorAll<HTMLElement>('[data-slot="tooltip-content"]'),
    ).filter((tooltip) => tooltip.getAttribute('data-closed') === null);
    expect(visibleTooltips.map((tooltip) => tooltip.textContent)).toEqual(['Continuous View']);
  });
});
