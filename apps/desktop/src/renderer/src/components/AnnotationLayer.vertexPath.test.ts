// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { createPageTransform, type DocumentModel, type Markup } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ToolMode } from '../../../shared/protocol';
import { AnnotationLayer } from './AnnotationLayer';

describe('polyline and polygon placement', () => {
  let host: HTMLDivElement;
  let root: Root;
  let documentModel: DocumentModel;

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
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('adds every clicked Polyline node and finishes on Enter', () => {
    const layer = renderLayer('polyline');
    clickPoint(layer, 10, 10);
    clickPoint(layer, 40, 20);
    clickPoint(layer, 70, 40);
    clickPoint(layer, 100, 20);

    pressFinishKey('Enter');

    expect(vertexPoints(documentModel.markups[0])).toEqual([
      { x: 10, y: 782 },
      { x: 40, y: 772 },
      { x: 70, y: 752 },
      { x: 100, y: 772 },
    ]);
    expect(documentModel.markups[0]?.kind).toBe('polyline');
  });

  it('finishes a multi-node Polygon on Escape and closes it when rendered', () => {
    const layer = renderLayer('polygon');
    clickPoint(layer, 20, 20);
    clickPoint(layer, 80, 20);
    clickPoint(layer, 90, 70);
    clickPoint(layer, 40, 90);

    pressFinishKey('Escape');

    expect(vertexPoints(documentModel.markups[0])).toHaveLength(4);
    expect(documentModel.markups[0]?.kind).toBe('polygon');
  });

  it('shows the Polygon start node and finishes when it is clicked within the close threshold', () => {
    const layer = renderLayer('polygon');
    clickPoint(layer, 20, 20);
    clickPoint(layer, 100, 20);
    clickPoint(layer, 70, 90);

    movePointer(layer, 24, 23);

    const marker = host.querySelector<SVGCircleElement>('[data-testid="polygon-start-marker"]');
    expect(marker?.getAttribute('data-close-active')).toBe('true');

    clickPoint(layer, 24, 23);

    expect(vertexPoints(documentModel.markups[0])).toEqual([
      { x: 20, y: 772 },
      { x: 100, y: 772 },
      { x: 70, y: 702 },
    ]);
    expect(host.querySelector('[data-testid="polygon-start-marker"]')).toBeNull();
  });

  function renderLayer(activeTool: Extract<ToolMode, 'polyline' | 'polygon'>): SVGSVGElement {
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
      updateDocument: (updater) => {
        documentModel = updater(documentModel);
      },
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
    return layer;
  }
});

function clickPoint(layer: SVGSVGElement, clientX: number, clientY: number): void {
  dispatchPointer(layer, 'pointerdown', clientX, clientY);
  dispatchPointer(layer, 'pointerup', clientX, clientY);
}

function movePointer(layer: SVGSVGElement, clientX: number, clientY: number): void {
  dispatchPointer(layer, 'pointermove', clientX, clientY);
}

function dispatchPointer(
  layer: SVGSVGElement,
  type: 'pointerdown' | 'pointermove' | 'pointerup',
  clientX: number,
  clientY: number,
): void {
  const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY });
  Object.defineProperty(event, 'pointerId', { value: 1 });
  act(() => layer.dispatchEvent(event));
}

function pressFinishKey(key: 'Enter' | 'Escape'): void {
  act(() => window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true })));
}

function vertexPoints(markup: Markup | undefined) {
  return markup?.kind === 'polyline' || markup?.kind === 'polygon' ? markup.points : [];
}
