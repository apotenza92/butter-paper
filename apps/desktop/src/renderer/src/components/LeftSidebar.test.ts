// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { LeftSidebar } from './LeftSidebar';

vi.mock('./PageThumbnailList', () => ({
  PageThumbnailList: () => null,
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

  it('matches the properties sidebar border ownership', () => {
    act(() => {
      root.render(createElement(LeftSidebar, {
        session: null,
        pages: [],
        panel: 'pages',
        onSelectPage: () => undefined,
        onSetPageScale: () => undefined,
        onRotatePage: () => undefined,
      }));
    });

    const header = host.querySelector('[data-testid="left-sidebar-header"]');
    expect(header?.textContent).toBe('Page Thumbnails');
    expect(header?.className).toContain('justify-center');
    expect(header?.className).toContain('text-center');
    expect(header?.className.split(' ')).toContain('border-b');
    expect(header?.className.split(' ')).toContain('border-border');
    expect(host.querySelector('aside')?.id).toBe('left-sidebar-panel');
    expect(host.querySelector('aside')?.getAttribute('aria-label')).toBe('Page Thumbnails');
    expect(host.querySelector('aside')?.className).toContain('border-r');
    expect(host.querySelector('aside')?.className).not.toContain('border-x');
    expect(host.querySelector('aside')?.className).toContain('border-border');
    expect((host.querySelector('aside') as HTMLElement | null)?.style.width).toBe('300px');
    expect(host.querySelector('[data-testid="left-sidebar-resize-handle"]')).toBeNull();
  });
});
