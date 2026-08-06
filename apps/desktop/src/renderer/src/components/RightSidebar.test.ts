// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RightSidebar } from './RightSidebar';

vi.mock('./ToolPropertiesPanel', () => ({
  ToolPropertiesPanel: () => null,
}));

vi.mock('./SidebarResizeHandle', () => ({
  SidebarResizeHandle: () => null,
}));

describe('RightSidebar', () => {
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

  it('centres the tool heading without a preview badge', () => {
    act(() => {
      root.render(createElement(RightSidebar, {
        activeTool: 'cloud',
        width: 300,
        onWidthChange: () => undefined,
      }));
    });

    const header = host.querySelector('[data-testid="right-sidebar-header"]');
    const heading = host.querySelector('[data-testid="right-sidebar-heading"]');

    expect(heading?.textContent).toBe('Cloud');
    expect(heading?.className).toContain('text-center');
    expect(header?.className).toContain('justify-center');
    expect(header?.textContent).not.toContain('Preview');
    expect(header?.children).toHaveLength(1);
  });
});
