// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { createPageTransform, createRectangleMarkup, rect, type DocumentModel } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SnapGuideType } from '../state/viewerStore';
import { AnnotationLayer } from './AnnotationLayer';

describe('annotation snap source feedback', () => {
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

  it('highlights only the annotation that drives the active guide and hides inactive acquired points', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const source = createRectangleMarkup({
      id: 'signature-source',
      pageIndex: 0,
      rect: rect(100, 650, 100, 100),
    });
    const documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [source],
    };

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: documentModel.markups,
      transform: createPageTransform(page, 1),
      activeTool: 'line',
      selectedMarkupIds: [],
      postPlacement: null,
      pendingImageAsset: null,
      setSelectedMarkupIds: vi.fn(),
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => null),
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

    dispatchPointerMove(layer, 100, 42);
    const hoverChrome = host.querySelector<SVGGElement>('[data-testid="markup-signature-source"] [data-interaction-state="hovered"]')!;
    expect(hoverChrome).toBeTruthy();
    expect(hoverChrome.querySelector('rect[stroke="#93c5fd"]')?.getAttribute('x')).toBe('92');
    expect(hoverChrome.querySelector('[data-handle-id]')).toBeTruthy();
    expect(hoverChrome.querySelector('[data-handle-id="rectangle.rotate"]')).toBeNull();
    const endpointMarker = host.querySelector<SVGRectElement>('[data-testid="snap-indicator"] rect')!;
    expect(endpointMarker.getAttribute('width')).toBe('14');
    expect(endpointMarker.getAttribute('stroke-width')).toBe('2.25');
    expect(endpointMarker.getAttribute('stroke')).toBe('#16a34a');

    dispatchPointerMove(layer, 260, 45);
    expect(host.querySelector('[data-testid="object-snap-tracking-source"]')?.getAttribute('data-snap-owner-id'))
      .toBe('signature-source');
    expect(host.querySelector('[data-testid="markup-signature-source"] [data-interaction-state="hovered"]')).toBeTruthy();
    expect(host.querySelector('[data-testid="acquired-tracking-point"]')).toBeNull();
    const trackingMarkerLine = host.querySelector<SVGLineElement>('[data-testid="object-snap-tracking-point"] line')!;
    expect(Number(trackingMarkerLine.getAttribute('x2')) - Number(trackingMarkerLine.getAttribute('x1'))).toBe(14);
    expect(trackingMarkerLine.getAttribute('stroke-width')).toBe('2.25');
    expect(trackingMarkerLine.getAttribute('stroke')).toBe('#16a34a');
    expect(host.querySelector('[data-testid="object-snap-tracking-guide"]')?.getAttribute('stroke')).toBe('#16a34a');

    dispatchPointerMove(layer, 260, 100);
    expect(host.querySelector('[data-testid="object-snap-tracking-source"]')).toBeNull();
    expect(host.querySelector('[data-testid="markup-signature-source"] [data-interaction-state="hovered"]')).toBeNull();
  });

  it('tracks a moved object from its own geometry instead of its arbitrary grab point', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const moving = createRectangleMarkup({
      id: 'moving',
      pageIndex: 0,
      rect: rect(100, 650, 100, 100),
      appearance: { fill: { color: '#ffffff' } },
    });
    const source = createRectangleMarkup({ id: 'source', pageIndex: 0, rect: rect(300, 650, 100, 100) });
    let documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [moving, source],
    };
    const updateDocument = vi.fn((updater: (document: DocumentModel) => DocumentModel) => {
      documentModel = updater(documentModel);
    });
    const layer = renderLayer(page, documentModel.markups, [], updateDocument);

    dispatchPointer(layer, 'pointerdown', 150, 92);
    dispatchPointer(layer, 'pointermove', 250, 92);
    expect(updateDocument).not.toHaveBeenCalled();
    expect(host.querySelector<SVGRectElement>('[data-testid="markup-moving"] rect')?.getAttribute('x')).toBe('200');
    expect(host.querySelector('[data-testid="markup-moving"] [data-interaction-state]')).toBeNull();

    dispatchPointer(layer, 'pointermove', 220, 95);
    expect(host.querySelector('[data-testid="object-snap-tracking-source"]')?.getAttribute('data-snap-owner-id'))
      .toBe('source');
    expect(host.querySelector('[data-testid="markup-source"] [data-interaction-state="hovered"]')).toBeNull();
    expect(host.querySelector('[data-testid="markup-source"] [data-handle-id]')).toBeNull();
    expect(updateDocument).not.toHaveBeenCalled();

    dispatchPointer(layer, 'pointerup', 220, 95);
    expect(updateDocument).toHaveBeenCalledOnce();
    expect(documentModel.markups.find((markup) => markup.id === 'moving')).toMatchObject({
      rect: { x: 170, y: 650, width: 100, height: 100 },
    });
  });

  it('acquires and tracks a source point while resizing from a handle', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const moving = createRectangleMarkup({ id: 'moving', pageIndex: 0, rect: rect(100, 650, 100, 100) });
    const source = createRectangleMarkup({ id: 'source', pageIndex: 0, rect: rect(300, 650, 100, 100) });
    let documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [moving, source],
    };
    const updateDocument = vi.fn((updater: (document: DocumentModel) => DocumentModel) => {
      documentModel = updater(documentModel);
    });
    const layer = renderLayer(page, documentModel.markups, ['moving'], updateDocument);

    expect(host.querySelector('[data-testid="markup-moving"] [data-interaction-state="focused"]')).toBeTruthy();
    expect(host.querySelector('[data-testid="markup-moving"] [data-handle-id]')).toBeTruthy();

    dispatchPointer(layer, 'pointerdown', 200, 92);
    dispatchPointer(layer, 'pointermove', 300, 92);
    expect(updateDocument).not.toHaveBeenCalled();
    expect(layer.style.cursor).toBe('none');
    expect(host.querySelector('[data-testid="markup-moving"] [data-interaction-state]')).toBeNull();

    dispatchPointer(layer, 'pointermove', 302, 120);
    expect(host.querySelector('[data-testid="object-snap-tracking-source"]')?.getAttribute('data-snap-owner-id'))
      .toBe('source');
    expect(host.querySelector('[data-testid="markup-source"] [data-interaction-state="hovered"]')).toBeNull();
    expect(host.querySelector('[data-testid="markup-source"] [data-handle-id]')).toBeNull();
    expect(updateDocument).not.toHaveBeenCalled();
    expect(layer.style.cursor).toBe('none');

    dispatchPointer(layer, 'pointerup', 302, 120);
    expect(updateDocument).toHaveBeenCalledOnce();
    expect(documentModel.markups.find((markup) => markup.id === 'moving')).toMatchObject({
      rect: { x: 100, y: 650, width: 200, height: 100 },
    });
    expect(host.querySelector('[data-testid="markup-moving"] [data-interaction-state="focused"]')).toBeTruthy();
    expect(host.querySelector('[data-testid="markup-moving"] [data-handle-id]')).toBeTruthy();
  });

  it('keeps real-anchor resize nodes visible when snapping is disabled', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const markup = createRectangleMarkup({ id: 'moving', pageIndex: 0, rect: rect(100, 650, 100, 100) });
    renderLayer(page, [markup], [markup.id], vi.fn(), false);

    const eastHandle = host.querySelector<SVGRectElement>('[data-testid="markup-moving"] [data-handle-id="rectangle.resize.e"]')!;
    expect(eastHandle.getAttribute('x')).toBe('196.5');
    expect(host.querySelector('[data-testid="markup-moving"] [data-interaction-state="focused"]')).toBeTruthy();
  });

  it('snaps a resize to another annotation width and shows equal-size bars', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const moving = createRectangleMarkup({ id: 'moving', pageIndex: 0, rect: rect(100, 650, 100, 100) });
    const source = createRectangleMarkup({ id: 'source', pageIndex: 0, rect: rect(300, 650, 150, 80) });
    let documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [moving, source],
    };
    const updateDocument = vi.fn((updater: (document: DocumentModel) => DocumentModel) => {
      documentModel = updater(documentModel);
    });
    const layer = renderLayer(page, documentModel.markups, ['moving'], updateDocument);

    dispatchPointer(layer, 'pointerdown', 200, 92);
    dispatchPointer(layer, 'pointermove', 248, 92);

    const guide = host.querySelector('[data-testid="equal-size-guide"]');
    expect(guide?.getAttribute('data-guide-axis')).toBe('horizontal');
    expect(guide?.getAttribute('data-reference-owner-id')).toBe('source');
    expect(guide?.querySelectorAll('[data-testid="relationship-measurement"]')).toHaveLength(2);
    expect(host.querySelector<SVGRectElement>('[data-testid="markup-moving"] rect')?.getAttribute('width')).toBe('150');

    dispatchPointer(layer, 'pointerup', 248, 92);
    expect(documentModel.markups.find((markup) => markup.id === 'moving')).toMatchObject({
      rect: { x: 100, y: 650, width: 150, height: 100 },
    });
  });

  it('nudges a newly drawn rectangle to an existing annotation size', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const source = createRectangleMarkup({ id: 'source', pageIndex: 0, rect: rect(300, 650, 150, 80) });
    let documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [source],
    };
    const updateDocument = vi.fn((updater: (document: DocumentModel) => DocumentModel) => {
      documentModel = updater(documentModel);
    });
    const layer = renderLayer(
      page,
      documentModel.markups,
      [],
      updateDocument,
      true,
      true,
      ['alignment', 'equal-size', 'equal-spacing'],
      'rectangle',
    );

    dispatchPointer(layer, 'pointerdown', 100, 142);
    dispatchPointer(layer, 'pointermove', 248, 62);

    expect(host.querySelectorAll('[data-testid="equal-size-guide"]')).toHaveLength(2);
    expect(host.querySelector('[data-reference-owner-id="source"]')).toBeTruthy();

    dispatchPointer(layer, 'pointerup', 248, 62);
    expect(documentModel.markups.at(-1)).toMatchObject({
      kind: 'rectangle',
      rect: { x: 100, y: 650, width: 150, height: 80 },
    });
  });

  it('snaps a moved annotation between two annotations with equal gaps', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const before = createRectangleMarkup({ id: 'before', pageIndex: 0, rect: rect(50, 650, 50, 50) });
    const moving = createRectangleMarkup({
      id: 'moving',
      pageIndex: 0,
      rect: rect(150, 650, 50, 50),
      appearance: { fill: { color: '#ffffff' } },
    });
    const after = createRectangleMarkup({ id: 'after', pageIndex: 0, rect: rect(260, 650, 50, 50) });
    let documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [before, moving, after],
    };
    const updateDocument = vi.fn((updater: (document: DocumentModel) => DocumentModel) => {
      documentModel = updater(documentModel);
    });
    const layer = renderLayer(page, documentModel.markups, [], updateDocument);

    dispatchPointer(layer, 'pointerdown', 175, 117);
    dispatchPointer(layer, 'pointermove', 179, 117);

    const guide = host.querySelector('[data-testid="equal-spacing-guide"]');
    expect(guide?.getAttribute('data-guide-axis')).toBe('horizontal');
    expect(guide?.getAttribute('data-before-owner-id')).toBe('before');
    expect(guide?.getAttribute('data-after-owner-id')).toBe('after');
    expect(guide?.querySelectorAll('[data-testid="relationship-measurement"]')).toHaveLength(2);
    expect(host.querySelector('[data-testid="equal-size-guide"]')).toBeTruthy();
    expect(host.querySelector<SVGRectElement>('[data-testid="markup-moving"] rect')?.getAttribute('x')).toBe('155');

    dispatchPointer(layer, 'pointerup', 179, 117);
    expect(documentModel.markups.find((markup) => markup.id === 'moving')).toMatchObject({
      rect: { x: 155, y: 650, width: 50, height: 50 },
    });
  });

  it('extends the spacing from an existing annotation pair', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const first = createRectangleMarkup({ id: 'first', pageIndex: 0, rect: rect(50, 650, 50, 50) });
    const second = createRectangleMarkup({ id: 'second', pageIndex: 0, rect: rect(150, 650, 50, 50) });
    const moving = createRectangleMarkup({
      id: 'moving',
      pageIndex: 0,
      rect: rect(240, 650, 50, 50),
      appearance: { fill: { color: '#ffffff' } },
    });
    const layer = renderLayer(page, [first, second, moving], [], vi.fn());

    dispatchPointer(layer, 'pointerdown', 265, 117);
    dispatchPointer(layer, 'pointermove', 272, 117);

    const guide = host.querySelector('[data-testid="equal-spacing-guide"]');
    expect(guide?.getAttribute('data-guide-placement')).toBe('after');
    expect(guide?.getAttribute('data-before-owner-id')).toBe('first');
    expect(guide?.getAttribute('data-after-owner-id')).toBe('second');
    expect(host.querySelector<SVGRectElement>('[data-testid="markup-moving"] rect')?.getAttribute('x')).toBe('250');
  });

  it('keeps equal-size snapping active when that guide type is hidden', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const moving = createRectangleMarkup({ id: 'moving', pageIndex: 0, rect: rect(100, 650, 100, 100) });
    const source = createRectangleMarkup({ id: 'source', pageIndex: 0, rect: rect(300, 650, 150, 80) });
    const layer = renderLayer(page, [moving, source], ['moving'], vi.fn(), true, true, ['equal-spacing']);

    dispatchPointer(layer, 'pointerdown', 200, 92);
    dispatchPointer(layer, 'pointermove', 248, 92);

    expect(host.querySelector<SVGRectElement>('[data-testid="markup-moving"] rect')?.getAttribute('width')).toBe('150');
    expect(host.querySelector('[data-testid="equal-size-guide"]')).toBeNull();
    expect(host.querySelector('[data-testid="relationship-snap-guides"]')).toBeNull();
  });

  it('keeps snapping active when snap guides are globally hidden', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const moving = createRectangleMarkup({ id: 'moving', pageIndex: 0, rect: rect(100, 650, 100, 100) });
    const source = createRectangleMarkup({ id: 'source', pageIndex: 0, rect: rect(300, 650, 150, 80) });
    const layer = renderLayer(page, [moving, source], ['moving'], vi.fn(), true, false);

    dispatchPointer(layer, 'pointerdown', 200, 92);
    dispatchPointer(layer, 'pointermove', 248, 92);

    expect(host.querySelector<SVGRectElement>('[data-testid="markup-moving"] rect')?.getAttribute('width')).toBe('150');
    expect(host.querySelector('[data-testid="equal-size-guide"]')).toBeNull();
  });

  function renderLayer(
    page: { readonly id: string; readonly index: number; readonly size: { readonly width: number; readonly height: number }; readonly rotation: 0 },
    markups: DocumentModel['markups'],
    selectedMarkupIds: readonly string[],
    updateDocument: (updater: (document: DocumentModel) => DocumentModel) => void,
    snapEnabled = true,
    snapGuidesEnabled = true,
    snapGuideTypes: readonly SnapGuideType[] = ['alignment', 'equal-size', 'equal-spacing'],
    activeTool: 'select' | 'rectangle' = 'select',
  ): SVGSVGElement {
    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups,
      transform: createPageTransform(page, 1),
      activeTool,
      snapToContent: snapEnabled,
      snapToMarkup: snapEnabled,
      snapTargets: ['endpoint', 'midpoint', 'center', 'intersection'],
      snapGuidesEnabled,
      snapGuideTypes,
      selectedMarkupIds: [...selectedMarkupIds],
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
): void {
  const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY });
  Object.defineProperty(event, 'pointerId', { value: 1 });
  act(() => layer.dispatchEvent(event));
}

function dispatchPointerMove(layer: SVGSVGElement, clientX: number, clientY: number): void {
  dispatchPointer(layer, 'pointermove', clientX, clientY);
}
