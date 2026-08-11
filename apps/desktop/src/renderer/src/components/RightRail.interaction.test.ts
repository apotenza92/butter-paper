// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SnapSettings } from '../state/viewerStore';
import { dismissToolShortcutPopup } from '../utils/toolShortcuts';
import { RightRail } from './RightRail';
import {
  enabledSnapSources,
  snapSourceSettingsForValues,
  snapTargetsAvailable,
} from './SnapSettingsMenu';

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const SNAP_SETTINGS: SnapSettings = {
  snapToContent: true,
  snapToMarkup: true,
  sensitivityPx: 8,
  snapTargets: ['endpoint', 'midpoint', 'center', 'intersection'],
  snapGuidesEnabled: true,
  snapGuideTypes: ['alignment', 'equal-size', 'equal-spacing'],
};

const SNAP_PROPS = {
  snapSettings: SNAP_SETTINGS,
  onSnapSettingsChange: () => undefined,
};

describe('RightRail properties interactions', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', TestResizeObserver);
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('uses one consistent icon-only grid for Properties, Snap, Select, and Pan', () => {
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          propertiesOpen: true,
          ...SNAP_PROPS,
          onSelectTool: () => undefined,
          onToggleProperties: () => undefined,
        }),
      ),
    );
    const trigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="properties-sidebar-trigger"]',
    );
    expect(trigger).toBeTruthy();
    expect(trigger?.getAttribute('aria-expanded')).toBe('true');
    expect(trigger?.getAttribute('aria-pressed')).toBe('true');
    expect(trigger?.getAttribute('aria-label')).toBe('Hide properties');
    expect(trigger?.className).toContain('size-8');
    expect(trigger?.className).toContain('p-0');
    expect(trigger?.className).not.toContain('border-input');
    expect(trigger?.className).toContain(
      "[&_svg:not([class*='size-'])]:size-4",
    );
    expect(trigger?.textContent).toBe('');
    expect(
      host.querySelector('[data-testid="properties-trigger-slot"]')?.className,
    ).toContain('gap-2');
    expect(
      host.querySelector('[data-testid="properties-trigger-slot"]')?.className,
    ).toContain('w-full');
    expect(
      host.querySelectorAll('[data-testid="properties-trigger-slot"]'),
    ).toHaveLength(1);
    const heading = host.querySelector<HTMLElement>(
      '[data-testid="right-rail-general-heading"]',
    );
    const controlGrid = host.querySelector<HTMLElement>(
      '[data-testid="top-rail-control-grid"]',
    );
    expect(heading?.textContent).toBe('General');
    expect(heading?.className).toContain('text-center');
    expect(heading?.className).toContain('text-sm');
    expect(controlGrid?.style.gridTemplateColumns).toBe('repeat(2, 32px)');
    expect(
      Array.from(controlGrid?.children ?? []).map((child) =>
        child.getAttribute('data-testid'),
      ),
    ).toEqual([
      'tool-select',
      'tool-pan',
      'properties-sidebar-trigger',
      'viewer-snap-controls',
    ]);

    const snapTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="viewer-snap-target-menu"]',
    );
    const snapControls = host.querySelector<HTMLElement>(
      '[data-testid="viewer-snap-controls"]',
    );
    const markupGroup = host.querySelector<HTMLElement>(
      '[data-testid="right-rail-markup"]',
    );
    const selectTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-select"]',
    );
    const panTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-pan"]',
    );
    const imageTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-image"]',
    );
    const signatureTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-signature"]',
    );
    expect(snapTrigger?.textContent).toBe('');
    expect(snapTrigger?.className).toContain('size-8');
    expect(snapTrigger?.className).toContain('p-0');
    expect(snapTrigger?.className).not.toContain('border-input');
    expect(snapTrigger?.getAttribute('aria-pressed')).toBe('false');
    expect(snapControls?.parentElement).toBe(controlGrid);
    expect(snapControls?.className).not.toContain('w-full');
    expect(selectTrigger?.parentElement).toBe(controlGrid);
    expect(panTrigger?.parentElement).toBe(controlGrid);
    expect(selectTrigger?.textContent).toBe('');
    expect(panTrigger?.textContent).toBe('');
    expect(panTrigger?.getAttribute('data-rail-tooltip')).toBe(
      'Pan (Hold Space)',
    );
    expect(selectTrigger?.className).toContain('size-8');
    expect(panTrigger?.className).toContain('size-8');
    expect(selectTrigger?.className).not.toContain('w-full');
    expect(panTrigger?.className).not.toContain('w-full');
    expect(selectTrigger?.className).not.toContain('border-input');
    expect(panTrigger?.className).not.toContain('border-input');
    expect(markupGroup?.contains(signatureTrigger ?? null)).toBe(true);
    expect(signatureTrigger?.getAttribute('data-rail-tooltip')).toBe('Signature');
    expect(signatureTrigger?.closest('[data-testid="signature-controls"]')?.nextElementSibling).toBe(imageTrigger);
    for (const control of [trigger, snapTrigger, selectTrigger, panTrigger]) {
      expect(control?.className).toContain('bg-transparent');
      expect(control?.className).toContain('hover:bg-muted');
      expect(control?.className).toBe(imageTrigger?.className);
    }
    expect(
      Boolean(
        snapControls &&
        markupGroup &&
        snapControls.compareDocumentPosition(markupGroup) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ),
    ).toBe(true);
    const railViewport = host.querySelector<HTMLElement>(
      '[data-testid="right-rail-viewport"]',
    );
    expect(
      railViewport?.contains(
        host.querySelector('[data-testid="properties-trigger-slot"]'),
      ),
    ).toBe(true);
    expect(
      host.querySelector('[data-testid="properties-trigger-slot"]')
        ?.parentElement,
    ).toBe(markupGroup?.parentElement);
    for (const groupHeading of host.querySelectorAll<HTMLElement>(
      'section > h2',
    )) {
      expect(groupHeading.className).toContain('text-sm');
    }
  });

  it('keeps select and pan available while disabling every annotation entry in read-only mode', () => {
    const onSelectTool = vi.fn();
    const onToggleProperties = vi.fn();
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          mutationDisabled: true,
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool,
          onToggleProperties,
        }),
      ),
    );

    const select = host.querySelector<HTMLButtonElement>('[data-testid="tool-select"]');
    const pan = host.querySelector<HTMLButtonElement>('[data-testid="tool-pan"]');
    const image = host.querySelector<HTMLButtonElement>('[data-testid="tool-image"]');
    const signature = host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]');
    const rectangle = host.querySelector<HTMLButtonElement>('[data-testid="tool-rectangle"]');
    const setPageScale = host.querySelector<HTMLButtonElement>('[data-testid="measure-set-page-scale"]');
    const properties = host.querySelector<HTMLButtonElement>('[data-testid="properties-sidebar-trigger"]');
    const snap = host.querySelector<HTMLButtonElement>('[data-testid="viewer-snap-target-menu"]');
    expect(select?.disabled).toBe(false);
    expect(pan?.disabled).toBe(false);
    expect(image?.disabled).toBe(true);
    expect(signature?.disabled).toBe(true);
    expect(rectangle?.disabled).toBe(true);
    expect(setPageScale?.disabled).toBe(true);
    expect(properties?.disabled).toBe(true);
    expect(snap?.disabled).toBe(true);

    act(() => {
      image?.click();
      rectangle?.click();
      properties?.click();
      select?.click();
      pan?.click();
    });
    expect(onSelectTool).toHaveBeenCalledTimes(2);
    expect(onSelectTool).toHaveBeenNthCalledWith(1, 'select', 0);
    expect(onSelectTool).toHaveBeenNthCalledWith(2, 'pan', 0);
    expect(onToggleProperties).not.toHaveBeenCalled();
  });

  it('puts Set page scale first in Measure and dispatches it as an action', () => {
    const onSetPageScale = vi.fn();
    const onSelectTool = vi.fn();
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool,
          onSetPageScale,
          onToggleProperties: () => undefined,
        }),
      ),
    );

    const measureGroup = host.querySelector<HTMLElement>('[data-testid="right-rail-measure"]');
    const setPageScale = host.querySelector<HTMLButtonElement>('[data-testid="measure-set-page-scale"]');
    const length = host.querySelector<HTMLButtonElement>('[data-testid="tool-length"]');

    expect(measureGroup?.contains(setPageScale)).toBe(true);
    expect(setPageScale?.nextElementSibling).toBe(length);
    expect(setPageScale?.getAttribute('aria-label')).toBe('Set page scale');
    expect(setPageScale?.getAttribute('data-rail-tooltip')).toBe('Set page scale');
    expect(setPageScale?.className).toContain('size-8');
    expect(setPageScale?.className).toContain('border-0');
    expect(setPageScale?.className).toContain('bg-transparent');
    expect(setPageScale?.className).toContain('p-0');
    expect(setPageScale?.className).not.toContain('border-input');

    act(() => setPageScale?.click());

    expect(onSetPageScale).toHaveBeenCalledOnce();
    expect(onSelectTool).not.toHaveBeenCalled();
  });

  it('stacks all four icon-only controls at one column', () => {
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool: () => undefined,
          onToggleProperties: () => undefined,
        }),
      ),
    );
    const slot = host.querySelector<HTMLElement>(
      '[data-testid="properties-trigger-slot"]',
    );
    const resizeHandle = host.querySelector<HTMLElement>(
      '[data-testid="right-rail-resize-handle"]',
    );

    act(() => {
      resizeHandle?.dispatchEvent(
        new KeyboardEvent('keydown', { bubbles: true, key: 'Home' }),
      );
    });

    const propertiesTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="properties-sidebar-trigger"]',
    );
    const snapTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="viewer-snap-target-menu"]',
    );
    const selectTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-select"]',
    );
    const panTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-pan"]',
    );
    const controlGrid = host.querySelector<HTMLElement>(
      '[data-testid="top-rail-control-grid"]',
    );
    expect(
      host
        .querySelector('[data-testid="right-rail"]')
        ?.getAttribute('data-column-count'),
    ).toBe('1');
    expect(controlGrid?.style.gridTemplateColumns).toBe('repeat(1, 32px)');
    expect(
      host.querySelector('[data-testid="right-rail-general-heading"]'),
    ).toBeNull();
    expect(slot?.className).toContain('gap-2');
    expect(propertiesTrigger?.textContent).toBe('');
    expect(
      host.querySelector('[data-testid="properties-sidebar-label"]'),
    ).toBeNull();
    expect(propertiesTrigger?.className).toContain('size-8');
    expect(propertiesTrigger?.className).not.toContain('w-full');
    expect(propertiesTrigger?.className).not.toContain('border-input');
    expect(snapTrigger?.textContent).toBe('');
    expect(host.querySelector('[data-testid="viewer-snap-label"]')).toBeNull();
    expect(snapTrigger?.className).toContain('size-8');
    expect(snapTrigger?.className).not.toContain('w-full');
    expect(snapTrigger?.className).not.toContain('border-input');
    expect(selectTrigger?.textContent).toBe('');
    expect(panTrigger?.textContent).toBe('');
    expect(selectTrigger?.className).toContain('size-8');
    expect(panTrigger?.className).toContain('size-8');
  });

  it('shows the compact, wider snap settings content', () => {
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool: () => undefined,
          onToggleProperties: () => undefined,
        }),
      ),
    );
    const snapTrigger = host.querySelector<HTMLButtonElement>(
      '[data-testid="viewer-snap-target-menu"]',
    );

    act(() => {
      snapTrigger?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });

    const popover = document.body.querySelector<HTMLElement>(
      '[data-testid="viewer-snap-popover"]',
    );
    const legends = Array.from(
      popover?.querySelectorAll<HTMLElement>('[data-slot="field-legend"]') ??
        [],
    );
    expect(popover?.className).toContain('w-80');
    expect(popover?.textContent).toContain('Snap to');
    expect(popover?.textContent).toContain('Intersections');
    expect(popover?.textContent).toContain('Show snap guides');
    expect(popover?.textContent).toContain('Equal spacing');
    expect(popover?.textContent).not.toContain('Snap settings');
    expect(popover?.textContent).not.toContain(
      'Choose snap sources and points.',
    );
    expect(legends.map((legend) => legend.textContent)).toEqual([
      'Snap to',
      'Snap points',
      'Snap guides',
    ]);
    expect(legends.every((legend) => legend.className.includes('w-full'))).toBe(
      true,
    );
    expect(
      legends.every((legend) => !legend.className.includes('text-center')),
    ).toBe(true);
  });

  it('changes the global snap-guide toggle and individual guide types independently', () => {
    const onSnapSettingsChange = vi.fn();
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          propertiesOpen: false,
          snapSettings: SNAP_SETTINGS,
          onSnapSettingsChange,
          onSelectTool: () => undefined,
          onToggleProperties: () => undefined,
        }),
      ),
    );
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="viewer-snap-target-menu"]')?.click());

    const globalToggle = document.body.querySelector<HTMLElement>('[data-testid="viewer-snap-guides-enabled"]');
    const equalSizeToggle = document.body.querySelector<HTMLButtonElement>('[data-testid="viewer-snap-guide-equal-size"]');
    expect(globalToggle?.getAttribute('data-checked')).not.toBeNull();
    expect(equalSizeToggle?.getAttribute('data-pressed')).not.toBeNull();

    act(() => globalToggle?.click());
    expect(onSnapSettingsChange).toHaveBeenCalledWith({ snapGuidesEnabled: false });

    act(() => equalSizeToggle?.click());
    expect(onSnapSettingsChange).toHaveBeenCalledWith({
      snapGuideTypes: ['alignment', 'equal-spacing'],
    });
  });

  it('closes snap settings for a tool shortcut and keeps the active tool on Escape', () => {
    const onSelectTool = vi.fn();
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'rectangle',
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool,
          onToggleProperties: () => undefined,
        }),
      ),
    );
    const snapTrigger = host.querySelector<HTMLButtonElement>('[data-testid="viewer-snap-target-menu"]');

    act(() => snapTrigger?.click());
    const popupControl = document.body.querySelector<HTMLButtonElement>('[data-testid="viewer-snap-content"]');
    act(() => {
      popupControl?.focus();
      popupControl?.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Escape' }));
    });
    expect(document.body.querySelector('[data-testid="viewer-snap-popover"][data-open]')).toBeNull();
    expect(onSelectTool).not.toHaveBeenCalled();

    act(() => snapTrigger?.click());
    expect(document.body.querySelector('[data-testid="viewer-snap-popover"][data-open]')).toBeTruthy();
    act(() => dismissToolShortcutPopup(
      document.body.querySelector<HTMLButtonElement>('[data-testid="viewer-snap-content"]'),
    ));
    expect(document.body.querySelector('[data-testid="viewer-snap-popover"][data-open]')).toBeNull();
  });

  it('selects once and toggles exactly once for a double-click sequence', () => {
    const onSelectTool = vi.fn();
    const onToggleProperties = vi.fn();
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool,
          onToggleProperties,
        }),
      ),
    );
    const image = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-image"]',
    );
    expect(image).toBeTruthy();
    act(() => {
      image?.dispatchEvent(
        new MouseEvent('click', { bubbles: true, detail: 1 }),
      );
      image?.dispatchEvent(
        new MouseEvent('click', { bubbles: true, detail: 2 }),
      );
      image?.dispatchEvent(
        new MouseEvent('dblclick', { bubbles: true, detail: 2 }),
      );
    });
    expect(onSelectTool).toHaveBeenCalledTimes(1);
    expect(onSelectTool).toHaveBeenCalledWith('image', 1);
    expect(onToggleProperties).toHaveBeenCalledTimes(1);
    expect(
      document.body.querySelector('[data-testid="rail-double-click-tooltip"]'),
    ).toBeNull();
  });

  it('shows a transient, state-aware properties hint after a tool click', () => {
    vi.useFakeTimers();
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool: () => undefined,
          onToggleProperties: () => undefined,
        }),
      ),
    );
    const rectangle = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-rectangle"]',
    );
    expect(rectangle?.getAttribute('data-rail-double-click-tooltip')).toBe(
      'Double click to show properties',
    );

    act(() => {
      rectangle?.dispatchEvent(
        new MouseEvent('click', { bubbles: true, detail: 1 }),
      );
    });
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'rectangle',
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool: () => undefined,
          onToggleProperties: () => undefined,
        }),
      ),
    );
    expect(
      document.body.querySelector('[data-testid="rail-double-click-tooltip"]')
        ?.textContent,
    ).toBe('Double click to show properties');

    act(() => {
      vi.advanceTimersByTime(2_000);
    });
    expect(
      document.body.querySelector('[data-testid="rail-double-click-tooltip"]'),
    ).toBeNull();

    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          propertiesOpen: true,
          ...SNAP_PROPS,
          onSelectTool: () => undefined,
          onToggleProperties: () => undefined,
        }),
      ),
    );
    expect(
      host
        .querySelector('[data-testid="tool-rectangle"]')
        ?.getAttribute('data-rail-double-click-tooltip'),
    ).toBe('Double click to hide properties');
  });

  it('preserves selection and properties gestures for tools moved into the top stack', () => {
    const onSelectTool = vi.fn();
    const onToggleProperties = vi.fn();
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'pan',
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool,
          onToggleProperties,
        }),
      ),
    );
    const select = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-select"]',
    );
    act(() => {
      select?.dispatchEvent(
        new MouseEvent('click', { bubbles: true, detail: 1 }),
      );
      select?.dispatchEvent(
        new MouseEvent('click', { bubbles: true, detail: 2 }),
      );
      select?.dispatchEvent(
        new MouseEvent('dblclick', { bubbles: true, detail: 2 }),
      );
    });
    expect(onSelectTool).toHaveBeenCalledTimes(1);
    expect(onSelectTool).toHaveBeenCalledWith('select', 1);
    expect(onToggleProperties).toHaveBeenCalledTimes(1);
  });

  it('supports keyboard activation and leaves disabled tools inert', () => {
    const onSelectTool = vi.fn();
    const onToggleProperties = vi.fn();
    act(() =>
      root.render(
        createElement(RightRail, {
          activeTool: 'select',
          disabled: true,
          propertiesOpen: false,
          ...SNAP_PROPS,
          onSelectTool,
          onToggleProperties,
        }),
      ),
    );
    const rectangle = host.querySelector<HTMLButtonElement>(
      '[data-testid="tool-rectangle"]',
    );
    act(() => {
      rectangle?.dispatchEvent(
        new MouseEvent('click', { bubbles: true, detail: 0 }),
      );
      rectangle?.dispatchEvent(
        new MouseEvent('dblclick', { bubbles: true, detail: 2 }),
      );
    });
    expect(onSelectTool).not.toHaveBeenCalled();
    expect(onToggleProperties).not.toHaveBeenCalled();
    expect(
      host.querySelector<HTMLButtonElement>(
        '[data-testid="viewer-snap-target-menu"]',
      )?.disabled,
    ).toBe(true);
    expect(
      host.querySelector<HTMLButtonElement>('[data-testid="tool-select"]')
        ?.disabled,
    ).toBe(true);
  });

  it('disables point-type snap controls when both snap sources are off', () => {
    expect(
      snapTargetsAvailable({ snapToContent: true, snapToMarkup: false }),
    ).toBe(true);
    expect(
      snapTargetsAvailable({ snapToContent: false, snapToMarkup: true }),
    ).toBe(true);
    expect(
      snapTargetsAvailable({ snapToContent: false, snapToMarkup: false }),
    ).toBe(false);
  });

  it('maps native snap-source toggle values without losing either setting', () => {
    expect(
      enabledSnapSources({ snapToContent: true, snapToMarkup: false }),
    ).toEqual(['content']);
    expect(
      enabledSnapSources({ snapToContent: true, snapToMarkup: true }),
    ).toEqual(['content', 'markup']);
    expect(snapSourceSettingsForValues(['markup'])).toEqual({
      snapToContent: false,
      snapToMarkup: true,
    });
    expect(snapSourceSettingsForValues([])).toEqual({
      snapToContent: false,
      snapToMarkup: false,
    });
  });
});
