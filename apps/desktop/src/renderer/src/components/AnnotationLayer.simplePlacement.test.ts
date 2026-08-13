// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { createPageTransform, type DocumentModel } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AnnotationLayer } from './AnnotationLayer';

describe('simple shape placement', () => {
  let host: HTMLDivElement;
  let root: Root;
  let documentModel: DocumentModel;
  let updateDocument: ReturnType<typeof vi.fn<(updater: (document: DocumentModel) => DocumentModel) => void>>;

  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    documentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [],
      markups: [],
    };
    updateDocument = vi.fn((updater) => {
      documentModel = updater(documentModel);
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('creates a rectangle with two clicks', () => {
    const layer = renderRectangleLayer();

    dispatchPointer(layer, 'pointerdown', 10, 10);
    dispatchPointer(layer, 'pointerup', 10, 10);
    expect(documentModel.markups).toHaveLength(0);

    dispatchPointer(layer, 'pointerdown', 40, 40);

    expect(documentModel.markups).toEqual([
      expect.objectContaining({ kind: 'rectangle', rect: { x: 10, y: 752, width: 30, height: 30 } }),
    ]);
  });

  it('creates a rectangle with one drag', () => {
    const onMarkupPlaced = vi.fn();
    const layer = renderRectangleLayer('rectangle', onMarkupPlaced);

    dispatchPointer(layer, 'pointerdown', 10, 10);
    dispatchPointer(layer, 'pointermove', 40, 40);
    dispatchPointer(layer, 'pointerup', 40, 40);

    expect(documentModel.markups).toEqual([
      expect.objectContaining({ kind: 'rectangle', rect: { x: 10, y: 752, width: 30, height: 30 } }),
    ]);
    expect(onMarkupPlaced).toHaveBeenCalledOnce();
  });

  it('creates a perfect circle when Shift is held while dragging an ellipse', () => {
    const layer = renderRectangleLayer('ellipse');

    dispatchPointer(layer, 'pointerdown', 10, 10);
    dispatchPointer(layer, 'pointermove', 50, 30, { shiftKey: true });
    dispatchPointer(layer, 'pointerup', 50, 30, { shiftKey: true });

    expect(documentModel.markups).toEqual([
      expect.objectContaining({ kind: 'ellipse', rect: { x: 10, y: 742, width: 40, height: 40 } }),
    ]);
  });

  function renderRectangleLayer(
    activeTool: 'rectangle' | 'ellipse' = 'rectangle',
    onMarkupPlaced = vi.fn(),
  ): SVGSVGElement {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    documentModel = { ...documentModel, pages: [page] };
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
      onMarkupPlaced,
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
    let capturedPointerId: number | null = null;
    Object.defineProperties(layer, {
      setPointerCapture: { value: vi.fn((pointerId: number) => { capturedPointerId = pointerId; }) },
      hasPointerCapture: { value: vi.fn((pointerId: number) => capturedPointerId === pointerId) },
      releasePointerCapture: { value: vi.fn(() => { capturedPointerId = null; }) },
    });
    return layer;
  }
});

function dispatchPointer(
  layer: SVGSVGElement,
  type: 'pointerdown' | 'pointermove' | 'pointerup',
  clientX: number,
  clientY: number,
  options: { readonly shiftKey?: boolean } = {},
): void {
  const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY, shiftKey: options.shiftKey });
  Object.defineProperty(event, 'pointerId', { value: 1 });
  act(() => layer.dispatchEvent(event));
}
