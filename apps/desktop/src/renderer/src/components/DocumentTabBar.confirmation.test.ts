// @vitest-environment jsdom

import { act, createElement, useState } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DocumentTabBar } from './DocumentTabBar';

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

describe('DocumentTabBar close confirmation', () => {
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
    vi.unstubAllGlobals();
    if (originalGetAnimations) Object.defineProperty(Element.prototype, 'getAnimations', originalGetAnimations);
    else Reflect.deleteProperty(Element.prototype, 'getAnimations');
  });

  it('anchors save and discard actions to the modified tab without a page backdrop', async () => {
    const onDiscard = vi.fn();

    function Harness() {
      const [pendingTabId, setPendingTabId] = useState<string | null>(null);
      return createElement(DocumentTabBar, {
        tabs: [{ id: 'tab-1', documentName: 'site-plan.pdf', dirty: true }],
        activeTabId: 'tab-1',
        onSelectTab: () => undefined,
        onCloseTab: setPendingTabId,
        onReorderTabs: () => undefined,
        onOpenTab: () => undefined,
        onNewPdf: () => undefined,
        closeConfirmation: {
            tabId: pendingTabId,
            busy: false,
            onCancel: () => setPendingTabId(null),
            onDiscard: () => {
              onDiscard();
              setPendingTabId(null);
            },
            onSave: () => undefined,
        },
      });
    }

    act(() => root.render(createElement(Harness)));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="document-tab-close-0"]')?.click());
    await act(async () => { await Promise.resolve(); });

    const popup = document.querySelector('[data-testid="confirmation-popover"]');
    expect(popup?.textContent).toContain('Save changes to site-plan.pdf?');
    expect(document.querySelector('[data-slot="dialog-overlay"]')).toBeNull();
    expect(document.querySelector('[data-slot="alert-dialog-overlay"]')).toBeNull();
    expect(host.querySelector('[data-testid="document-tab-0"]')).toBeTruthy();

    act(() => findButton(popup, 'Discard')?.click());
    expect(onDiscard).toHaveBeenCalledOnce();
  });
});

function findButton(scope: Element | null, label: string): HTMLButtonElement | undefined {
  return [...(scope?.querySelectorAll<HTMLButtonElement>('button') ?? [])]
    .find((button) => button.textContent?.trim() === label);
}
