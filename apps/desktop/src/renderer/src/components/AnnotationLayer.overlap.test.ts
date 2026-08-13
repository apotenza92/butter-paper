// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { createPageTransform, createRectangleMarkup, rect, type DocumentModel } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AnnotationLayer } from './AnnotationLayer';

describe('overlapping annotation targeting', () => {
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

  it('routes a click and move drag to the markup previewed by the layer hit test', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const behind = createRectangleMarkup({
      id: 'behind',
      pageIndex: 0,
      rect: rect(20, 692, 100, 80),
      appearance: { fill: { color: '#ffffff' } },
    });
    const front = createRectangleMarkup({
      id: 'front',
      pageIndex: 0,
      rect: rect(40, 712, 100, 80),
      appearance: { fill: { color: '#ffffff' } },
    });
    let documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [behind, front],
    };
    const setSelectedMarkupIds = vi.fn();
    const updateDocument = vi.fn((updater: (document: DocumentModel) => DocumentModel) => {
      documentModel = updater(documentModel);
    });

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: documentModel.markups,
      transform: createPageTransform(page, 1),
      activeTool: 'select',
      selectedMarkupIds: [],
      postPlacement: null,
      pendingImageAsset: null,
      setSelectedMarkupIds,
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => null),
      updateDocument,
    })));

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
    Object.defineProperties(layer, {
      setPointerCapture: { value: vi.fn() },
      hasPointerCapture: { value: vi.fn(() => true) },
      releasePointerCapture: { value: vi.fn() },
    });

    dispatchPointer(layer, 'pointermove', 75, 60);
    expect(host.querySelector('[data-testid="markup-front"]')?.getAttribute('data-interaction-state')).toBeNull();
    expect(host.querySelector('[data-testid="markup-front"] > g')?.nextElementSibling?.getAttribute('data-interaction-state')).toBe('hovered');

    const behindPrimitive = host.querySelector<SVGRectElement>('[data-testid="markup-behind"] rect')!;
    dispatchPointer(behindPrimitive, 'pointerdown', 75, 60);
    expect(setSelectedMarkupIds).toHaveBeenLastCalledWith(['front']);

    dispatchPointer(layer, 'pointermove', 85, 60);
    expect(updateDocument).not.toHaveBeenCalled();
    expect(host.querySelector<SVGRectElement>('[data-testid="markup-front"] rect')?.getAttribute('x')).toBe('50');

    dispatchPointer(layer, 'pointermove', 95, 60);
    expect(updateDocument).not.toHaveBeenCalled();
    expect(host.querySelector<SVGRectElement>('[data-testid="markup-front"] rect')?.getAttribute('x')).toBe('60');

    dispatchPointer(layer, 'pointerup', 95, 60);
    expect(updateDocument).toHaveBeenCalledOnce();
    expect(documentModel.markups.find((markup) => markup.id === 'front')).toMatchObject({
      rect: { x: 60, y: 712, width: 100, height: 80 },
    });
    expect(documentModel.markups.find((markup) => markup.id === 'behind')).toEqual(behind);
  });

  it('discards a transient move preview when the pointer is cancelled', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const markup = createRectangleMarkup({
      id: 'rectangle-1',
      pageIndex: 0,
      rect: rect(40, 712, 100, 80),
      appearance: { fill: { color: '#ffffff' } },
    });
    const updateDocument = vi.fn();

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: [markup],
      transform: createPageTransform(page, 1),
      activeTool: 'select',
      selectedMarkupIds: [markup.id],
      postPlacement: null,
      pendingImageAsset: null,
      setSelectedMarkupIds: vi.fn(),
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => null),
      updateDocument,
    })));

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
    Object.defineProperties(layer, {
      setPointerCapture: { value: vi.fn() },
      hasPointerCapture: { value: vi.fn(() => true) },
      releasePointerCapture: { value: vi.fn() },
    });

    const primitive = host.querySelector<SVGRectElement>('[data-testid="markup-rectangle-1"] rect')!;
    dispatchPointer(primitive, 'pointerdown', 75, 60);
    dispatchPointer(layer, 'pointermove', 95, 60);
    expect(host.querySelector<SVGRectElement>('[data-testid="markup-rectangle-1"] rect')?.getAttribute('x')).toBe('60');

    dispatchPointer(layer, 'pointercancel', 95, 60);
    expect(updateDocument).not.toHaveBeenCalled();
    expect(host.querySelector<SVGRectElement>('[data-testid="markup-rectangle-1"] rect')?.getAttribute('x')).toBe('40');
  });

  it('selects a locked markup without moving it or showing transform handles', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const markup = {
      ...createRectangleMarkup({
        id: 'locked-rectangle',
        pageIndex: 0,
        rect: rect(40, 712, 100, 80),
        appearance: { fill: { color: '#ffffff' } },
      }),
      locked: true,
    };
    const setSelectedMarkupIds = vi.fn();
    const updateDocument = vi.fn();

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: [markup],
      transform: createPageTransform(page, 1),
      activeTool: 'select',
      selectedMarkupIds: [markup.id],
      postPlacement: null,
      pendingImageAsset: null,
      setSelectedMarkupIds,
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => null),
      updateDocument,
    })));

    const layer = host.querySelector<SVGSVGElement>('[data-testid="annotation-layer-1"]')!;
    vi.spyOn(layer, 'getBoundingClientRect').mockReturnValue({
      x: 0, y: 0, left: 0, top: 0, right: 612, bottom: 792, width: 612, height: 792, toJSON: () => ({}),
    });
    Object.defineProperties(layer, {
      setPointerCapture: { value: vi.fn() },
      hasPointerCapture: { value: vi.fn(() => false) },
      releasePointerCapture: { value: vi.fn() },
    });

    expect(host.querySelector('[data-handle-id]')).toBeNull();
    const primitive = host.querySelector<SVGRectElement>('[data-testid="markup-locked-rectangle"] rect')!;
    dispatchPointer(primitive, 'pointerdown', 75, 60);
    dispatchPointer(layer, 'pointermove', 95, 60);
    dispatchPointer(layer, 'pointerup', 95, 60);

    expect(setSelectedMarkupIds).toHaveBeenLastCalledWith([markup.id]);
    expect(updateDocument).not.toHaveBeenCalled();
    expect(primitive.getAttribute('x')).toBe('40');
  });

  it('commits a resize once after rendering it as a transient preview', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const markup = createRectangleMarkup({
      id: 'rectangle-1',
      pageIndex: 0,
      rect: rect(40, 712, 100, 80),
      appearance: { fill: { color: '#ffffff' } },
    });
    let documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [markup],
    };
    const updateDocument = vi.fn((updater: (document: DocumentModel) => DocumentModel) => {
      documentModel = updater(documentModel);
    });

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: documentModel.markups,
      transform: createPageTransform(page, 1),
      activeTool: 'select',
      selectedMarkupIds: [markup.id],
      postPlacement: null,
      pendingImageAsset: null,
      setSelectedMarkupIds: vi.fn(),
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => null),
      updateDocument,
    })));

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
    Object.defineProperties(layer, {
      setPointerCapture: { value: vi.fn() },
      hasPointerCapture: { value: vi.fn(() => true) },
      releasePointerCapture: { value: vi.fn() },
    });

    const handle = host.querySelector<SVGRectElement>('[data-handle-id="rectangle.resize.se"]')!;
    const startX = Number(handle.getAttribute('x')) + Number(handle.getAttribute('width')) * 0.5;
    const startY = Number(handle.getAttribute('y')) + Number(handle.getAttribute('height')) * 0.5;
    dispatchPointer(handle, 'pointerdown', startX, startY);
    dispatchPointer(layer, 'pointermove', startX + 20, startY + 20);
    expect(updateDocument).not.toHaveBeenCalled();

    dispatchPointer(layer, 'pointerup', startX + 20, startY + 20);
    expect(updateDocument).toHaveBeenCalledOnce();
    expect(documentModel.markups[0]).not.toEqual(markup);
  });

  it('selects the hit markup and opens properties on double-click without starting a move', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const markup = createRectangleMarkup({
      id: 'rectangle-1',
      pageIndex: 0,
      rect: rect(40, 712, 100, 80),
      appearance: { fill: { color: '#ffffff' } },
    });
    const setSelectedMarkupIds = vi.fn();
    const updateDocument = vi.fn();
    const onToggleProperties = vi.fn();

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: [markup],
      transform: createPageTransform(page, 1),
      activeTool: 'select',
      selectedMarkupIds: [],
      postPlacement: null,
      pendingImageAsset: null,
      setSelectedMarkupIds,
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => null),
      updateDocument,
      onToggleProperties,
    })));

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
    Object.defineProperties(layer, {
      setPointerCapture: { value: vi.fn() },
      hasPointerCapture: { value: vi.fn(() => false) },
      releasePointerCapture: { value: vi.fn() },
    });

    const primitive = host.querySelector<SVGRectElement>('[data-testid="markup-rectangle-1"] rect')!;
    dispatchPointer(primitive, 'pointerdown', 75, 60, 2);
    dispatchDoubleClick(primitive, 75, 60);

    expect(setSelectedMarkupIds).toHaveBeenLastCalledWith(['rectangle-1']);
    expect(onToggleProperties).toHaveBeenCalledWith(false);
    expect(updateDocument).not.toHaveBeenCalled();
  });

  it('reports when the double-clicked markup was already selected', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const markup = createRectangleMarkup({
      id: 'rectangle-1',
      pageIndex: 0,
      rect: rect(40, 712, 100, 80),
      appearance: { fill: { color: '#ffffff' } },
    });
    const onToggleProperties = vi.fn();

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: [markup],
      transform: createPageTransform(page, 1),
      activeTool: 'select',
      selectedMarkupIds: [markup.id],
      postPlacement: null,
      pendingImageAsset: null,
      setSelectedMarkupIds: vi.fn(),
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => null),
      updateDocument: vi.fn(),
      onToggleProperties,
    })));

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
    const primitive = host.querySelector<SVGRectElement>('[data-testid="markup-rectangle-1"] rect')!;

    dispatchPointer(primitive, 'pointerdown', 75, 60, 2);
    dispatchDoubleClick(primitive, 75, 60);

    expect(onToggleProperties).toHaveBeenCalledWith(true);
  });
});

function dispatchPointer(
  target: Element,
  type: 'pointerdown' | 'pointermove' | 'pointerup' | 'pointercancel',
  clientX: number,
  clientY: number,
  detail = 1,
): void {
  const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY, detail });
  Object.defineProperty(event, 'pointerId', { value: 1 });
  act(() => target.dispatchEvent(event));
}

function dispatchDoubleClick(target: Element, clientX: number, clientY: number): void {
  const event = new MouseEvent('dblclick', { bubbles: true, button: 0, clientX, clientY, detail: 2 });
  act(() => target.dispatchEvent(event));
}
