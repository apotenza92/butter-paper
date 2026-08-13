// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { LeftRail } from './LeftRail';

vi.mock('./RailScrollArea', () => ({
  RailScrollArea: ({ children }: { children: React.ReactNode }) => createElement('div', null, children),
}));

describe('LeftRail', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement('div');
    document.body.appendChild(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it('exposes a keyboard-focusable page panel toggle', () => {
    const onToggle = vi.fn();
    act(() => root.render(createElement(LeftRail, {
      activePanel: 'pages',
      onToggle,
    })));

    const pages = host.querySelector<HTMLButtonElement>('[data-testid="left-rail-pages"]');
    const rail = host.querySelector<HTMLElement>('[data-testid="left-rail"]');
    expect(pages?.getAttribute('aria-expanded')).toBe('true');
    expect(pages?.getAttribute('aria-controls')).toBe('left-sidebar-panel');
    expect(pages?.tabIndex).toBe(0);
    expect(rail?.className).toContain('border-r');
    expect(rail?.className).toContain('border-border');
    pages?.focus();
    expect(document.activeElement).toBe(pages);

    act(() => pages?.click());
    expect(onToggle).toHaveBeenCalledWith('pages');
  });
});
