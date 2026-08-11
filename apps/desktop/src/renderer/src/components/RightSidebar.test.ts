// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { rect, type RectangleMarkup } from '@butter-paper/core';
import { useViewerStore } from '../state/viewerStore';
import { findFocusedSelectedMarkup, RightSidebar } from './RightSidebar';

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
    useViewerStore.setState({ document: null, selectedMarkupIds: [] });
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

  it('resolves the focused markup from the selection order', () => {
    const first = { id: 'first', kind: 'rectangle' } as never;
    const focused = { id: 'focused', kind: 'ellipse' } as never;

    expect(findFocusedSelectedMarkup([first, focused], ['focused', 'first'])).toBe(focused);
    expect(findFocusedSelectedMarkup([first, focused], [])).toBeNull();
  });

  it('shows the selected markup heading while the Select tool is active', () => {
    const selectedMarkup: RectangleMarkup = {
      id: 'rectangle-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 20, 30, 40),
    };
    useViewerStore.setState({
      document: {
        dirty: false,
        filePath: '/tmp/example.pdf',
        document: {
          id: 'document-1',
          path: '/tmp/example.pdf',
          metadata: {},
          pages: [],
          markups: [selectedMarkup],
        },
      } as never,
      selectedMarkupIds: [selectedMarkup.id],
    });

    act(() => {
      root.render(createElement(RightSidebar, {
        activeTool: 'select',
        width: 300,
        onWidthChange: () => undefined,
      }));
    });

    expect(host.querySelector('[data-testid="right-sidebar-heading"]')?.textContent).toBe('Rectangle');
  });
});
