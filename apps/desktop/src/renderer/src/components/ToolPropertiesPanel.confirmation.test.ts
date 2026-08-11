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

    const resetProperties = findButton('Reset properties');
    act(() => resetProperties?.click());
    await act(async () => { await Promise.resolve(); });

    expect(document.body.textContent).toContain('Reset Pen properties?');
    expect(document.body.textContent).toContain('Other tools will not change.');
    expect(document.querySelector('[data-slot="alert-dialog-overlay"]')).toBeNull();
    expect(useViewerStore.getState().toolPropertyValues.pen?.smoothCurves).toBe(false);

    const confirmReset = findButton('Reset');
    act(() => confirmReset?.click());
    expect(useViewerStore.getState().toolPropertyValues.pen?.smoothCurves).toBe(true);
  });
});

function findButton(label: string): HTMLButtonElement | undefined {
  return [...document.querySelectorAll<HTMLButtonElement>('button')]
    .find((button) => button.textContent?.trim() === label);
}
