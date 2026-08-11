// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { createPageTransform, type DocumentModel } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AnnotationLayer } from './AnnotationLayer';

describe('image placement interaction', () => {
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

  it('shows the pending image at its final size and completes in Select mode', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const asset = {
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png' as const,
      width: 200,
      height: 80,
      fileName: 'signature.png',
      aspectRatioLocked: true,
      selectAfterPlacement: true,
    };
    const consumePendingImageAsset = vi.fn(() => asset);
    const onImagePlaced = vi.fn();
    const updateDocument = vi.fn<(updater: (document: DocumentModel) => DocumentModel) => void>();

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: [],
      transform: createPageTransform(page, 1),
      activeTool: 'image',
      selectedMarkupIds: [],
      postPlacement: null,
      pendingImageAsset: asset,
      setSelectedMarkupIds: vi.fn(),
      setPostPlacement: vi.fn(),
      consumePendingImageAsset,
      onImagePlaced,
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

    act(() => layer.dispatchEvent(pointerEvent('pointermove', 200, 300)));

    const preview = host.querySelector<SVGImageElement>('[data-testid="pending-image-preview"]');
    expect(preview?.getAttribute('href')).toBe(asset.dataUrl);
    expect(preview?.getAttribute('x')).toBe('100');
    expect(preview?.getAttribute('y')).toBe('260');
    expect(preview?.getAttribute('width')).toBe('200');
    expect(preview?.getAttribute('height')).toBe('80');
    expect(preview?.getAttribute('opacity')).toBe('0.45');
    expect(layer.style.cursor).toBe('none');

    act(() => layer.dispatchEvent(pointerEvent('pointerdown', 200, 300)));

    expect(consumePendingImageAsset).toHaveBeenCalledOnce();
    expect(updateDocument).toHaveBeenCalledOnce();
    const update = updateDocument.mock.calls[0]?.[0];
    const updatedDocument = update?.({
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [],
    });
    expect(updatedDocument?.markups).toEqual([
      expect.objectContaining({
        kind: 'image',
        aspectRatioLocked: true,
      }),
    ]);
    expect(onImagePlaced).toHaveBeenCalledOnce();
    expect(host.querySelector('[data-testid="pending-image-preview"]')).toBeNull();
  });

  it('does not return to Select after placing a regular image', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const asset = {
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png' as const,
      width: 200,
      height: 80,
      fileName: 'image.png',
    };
    const onImagePlaced = vi.fn();

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: [],
      transform: createPageTransform(page, 1),
      activeTool: 'image',
      selectedMarkupIds: [],
      postPlacement: null,
      pendingImageAsset: asset,
      setSelectedMarkupIds: vi.fn(),
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => asset),
      onImagePlaced,
      updateDocument: vi.fn(),
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
    act(() => layer.dispatchEvent(pointerEvent('pointerdown', 200, 300)));

    expect(onImagePlaced).not.toHaveBeenCalled();
  });
});

function pointerEvent(type: 'pointermove' | 'pointerdown', clientX: number, clientY: number): MouseEvent {
  const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY });
  Object.defineProperty(event, 'pointerId', { value: 1 });
  return event;
}
