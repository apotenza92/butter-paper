// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { createPageTransform, type DocumentModel } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ToolMode } from '../../../shared/protocol';
import { AnnotationLayer } from './AnnotationLayer';

describe('annotation tool cursor icon', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('shows the toolbar icon below and right of a crosshair and hides it for navigation tools', () => {
    renderLayer('rectangle');

    const layer = host.querySelector<SVGSVGElement>('[data-testid="annotation-layer-1"]')!;
    vi.spyOn(layer, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 612,
      bottom: 792,
      width: 612,
      height: 792,
      toJSON: () => ({}),
    });

    const cursorIcon = host.querySelector<HTMLDivElement>('[data-testid="tool-cursor-icon"]')!;
    expect(cursorIcon.style.opacity).toBe('0');

    act(() => layer.dispatchEvent(pointerEvent('pointermove', 200, 300)));

    expect(cursorIcon.style.transform).toBe('translate3d(220px, 320px, 0)');
    expect(cursorIcon.style.opacity).toBe('1');
    expect(cursorIcon.querySelector('.lucide-square')).not.toBeNull();
    expect(cursorIcon.classList.contains('size-4')).toBe(true);
    expect(cursorIcon.classList.contains('text-white')).toBe(true);
    expect(cursorIcon.classList.contains('mix-blend-difference')).toBe(true);
    expect(cursorIcon.querySelector('div')).toBeNull();
    expect(layer.style.cursor).toBe('crosshair');

    renderLayer('ellipse');
    const switchedCursorIcon = host.querySelector<HTMLDivElement>('[data-testid="tool-cursor-icon"]')!;
    expect(switchedCursorIcon.style.transform).toBe('translate3d(220px, 320px, 0)');
    expect(switchedCursorIcon.style.opacity).toBe('1');
    expect(switchedCursorIcon.querySelector('.lucide-circle')).not.toBeNull();

    renderLayer('select');
    expect(host.querySelector('[data-testid="tool-cursor-icon"]')).toBeNull();
  });

  function renderLayer(activeTool: ToolMode): void {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: [],
      transform: createPageTransform(page, 1),
      activeTool,
      selectedMarkupIds: [],
      postPlacement: null,
      pendingImageAsset: null,
      setSelectedMarkupIds: vi.fn(),
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => null),
      updateDocument: vi.fn<(updater: (document: DocumentModel) => DocumentModel) => void>(),
    })));
  }
});

function pointerEvent(type: 'pointermove', clientX: number, clientY: number): MouseEvent {
  const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY });
  Object.defineProperty(event, 'pointerId', { value: 1 });
  return event;
}
