// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetViewerStore, useViewerStore } from '../state/viewerStore';
import { ToolPropertiesPanel } from './ToolPropertiesPanel';

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

describe('ToolPropertiesPanel reset confirmation', () => {
  let host: HTMLDivElement;
  let root: Root;
  let originalGetAnimations: PropertyDescriptor | undefined;

  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', TestResizeObserver);
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    originalGetAnimations = Object.getOwnPropertyDescriptor(Element.prototype, 'getAnimations');
    Object.defineProperty(Element.prototype, 'getAnimations', {
      configurable: true,
      value: vi.fn(() => []),
    });
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    document.querySelectorAll('[data-slot="popover-content"]').forEach((element) => element.remove());
    resetViewerStore();
    vi.unstubAllGlobals();
    if (originalGetAnimations) Object.defineProperty(Element.prototype, 'getAnimations', originalGetAnimations);
    else Reflect.deleteProperty(Element.prototype, 'getAnimations');
  });

  it('does not reset the active tool until the user confirms', async () => {
    useViewerStore.getState().setToolPropertyValue('pen', 'smoothCurves', false);
    act(() => root.render(createElement(ToolPropertiesPanel, { activeTool: 'pen' })));

    const resetAccordion = findButton('Reset');
    expect(resetAccordion?.getAttribute('aria-expanded')).toBe('false');
    act(() => resetAccordion?.click());

    const resetProperties = findButton('Reset properties');
    act(() => resetProperties?.click());
    await act(async () => {
      await Promise.resolve();
    });

    expect(document.body.textContent).toContain('Reset Pen properties?');
    expect(document.body.textContent).toContain('Other tools will not change.');
    expect(document.querySelector('[data-slot="alert-dialog-overlay"]')).toBeNull();
    expect(useViewerStore.getState().toolPropertyValues.pen?.smoothCurves).toBe(false);

    const confirmReset = [...document.querySelectorAll<HTMLButtonElement>('[data-slot="popover-content"] button')]
      .find((button) => button.textContent?.trim() === 'Reset');
    act(() => confirmReset?.click());
    expect(useViewerStore.getState().toolPropertyValues.pen?.smoothCurves).toBe(true);
  });

  it('uses collapsible property groups in the shared order with reset last', () => {
    act(() => root.render(createElement(ToolPropertiesPanel, { activeTool: 'text-box' })));

    const headings = [...host.querySelectorAll<HTMLButtonElement>('[data-slot="accordion-trigger"]')];
    expect(headings.map((heading) => heading.textContent?.trim())).toEqual(['Text', 'Appearance', 'Reset']);
    expect(headings.map((heading) => heading.getAttribute('aria-expanded'))).toEqual(['true', 'true', 'false']);
    expect([...host.querySelectorAll('[data-slot="field-label"]')].map((label) => label.textContent?.trim())).toContain('Color');
    expect([...host.querySelectorAll('[data-slot="field-label"]')].map((label) => label.textContent?.trim())).not.toContain('Text');
  });
});

function findButton(label: string): HTMLButtonElement | undefined {
  return [...document.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent?.trim() === label);
}
