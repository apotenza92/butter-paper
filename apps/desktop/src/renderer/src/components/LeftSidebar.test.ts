// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { LeftSidebar } from './LeftSidebar';

vi.mock('./PageThumbnailList', () => ({
  PageThumbnailList: () => null,
}));

vi.mock('./SidebarResizeHandle', () => ({
  SidebarResizeHandle: () => null,
}));

describe('LeftSidebar', () => {
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

  it('centres the Page Thumbnails heading', () => {
    act(() => {
      root.render(createElement(LeftSidebar, {
        session: null,
        pages: [],
        panel: 'pages',
        width: 300,
        onSelectPage: () => undefined,
        onSetPageScale: () => undefined,
        onRotatePage: () => undefined,
        onWidthChange: () => undefined,
      }));
    });

    const header = host.querySelector('[data-testid="left-sidebar-header"]');
    expect(header?.textContent).toBe('Page Thumbnails');
    expect(header?.className).toContain('justify-center');
    expect(header?.className).toContain('text-center');
    expect(host.querySelector('aside')?.id).toBe('left-sidebar-panel');
    expect(host.querySelector('aside')?.getAttribute('aria-label')).toBe('Page Thumbnails');
  });
});
