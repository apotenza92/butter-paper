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

  it('uses the active tool as the fixed sidebar heading', () => {
    act(() => {
      root.render(
        createElement(RightSidebar, {
          activeTool: 'cloud',
        }),
      );
    });

    expect(host.querySelector('[data-testid="right-sidebar-header"]')).toBeTruthy();
    expect(host.querySelector('[data-testid="right-sidebar-heading"]')?.textContent).toBe('Cloud');
    expect(host.querySelector<HTMLElement>('[data-testid="right-sidebar"]')?.style.width).toBe('300px');
    expect(host.querySelector('[data-testid="right-sidebar-resize-handle"]')).toBeNull();
  });

  it('resolves the focused markup from the selection order', () => {
    const first = { id: 'first', kind: 'rectangle' } as never;
    const focused = { id: 'focused', kind: 'ellipse' } as never;

    expect(findFocusedSelectedMarkup([first, focused], ['focused', 'first'])).toBe(focused);
    expect(findFocusedSelectedMarkup([first, focused], [])).toBeNull();
  });

  it('passes the focused markup to the properties panel while Select is active', () => {
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
      root.render(
        createElement(RightSidebar, {
          activeTool: 'select',
        }),
      );
    });

    expect(findFocusedSelectedMarkup([selectedMarkup], [selectedMarkup.id])).toBe(selectedMarkup);
    expect(host.querySelector('[data-testid="right-sidebar-heading"]')?.textContent).toBe('Rectangle');
  });
});
