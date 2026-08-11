// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { createPageTransform, pdfPoint, type DocumentModel } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createArcMarkupFromThreePoints } from '../pdf-tools/builtins/arcTool';
import { AnnotationLayer } from './AnnotationLayer';

describe('arc placement', () => {
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

  it('creates a stable circular arc from start, end, and bulge clicks', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    let documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [],
    };
    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: [],
      transform: createPageTransform(page, 1),
      activeTool: 'arc',
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

    clickPoint(layer, 50, 100);
    clickPoint(layer, 250, 100);
    movePointer(layer, 150, 40, { shiftKey: true });

    const previewPoints = layer.querySelector('polyline')?.getAttribute('points');
    expect(previewPoints).toBeTruthy();
    expect(previewPoints).not.toMatch(/NaN|Infinity/);

    clickPoint(layer, 150, 40, { shiftKey: true });

    const arc = documentModel.markups[0];
    expect(arc?.kind).toBe('arc');
    if (arc?.kind === 'arc') {
      expect(arc.start).toEqual({ x: 50, y: 692 });
      expect(arc.end).toEqual({ x: 250, y: 692 });
      expect(arc.mid?.x).toBeCloseTo(150);
      expect(arc.mid?.y).toBeCloseTo(733.421356);
      expect(Math.abs(arc.angle2 - arc.angle1)).toBeCloseTo(90);
      expect(Number.isFinite(arc.rect.width)).toBe(true);
      expect(arc.rect.width).toBeLessThan(1_000);
    }

    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: documentModel.markups,
      transform: createPageTransform(page, 1),
      activeTool: 'arc',
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

    const renderedArc = host.querySelector<SVGPolylineElement>('[data-testid^="markup-"] polyline');
    expect(renderedArc?.getAttribute('points')?.trim().split(/\s+/).length).toBeGreaterThan(2);
    expect(renderedArc?.getAttribute('stroke')).not.toBe('none');
  });

  it('selects and moves an arc by dragging its visible stroke', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const arc = createArcMarkupFromThreePoints(
      'arc-select',
      page.index,
      pdfPoint(50, 692),
      pdfPoint(250, 692),
      pdfPoint(150, 752),
    )!;
    let documentModel: DocumentModel = {
      id: 'document-1',
      path: '/fixture.pdf',
      metadata: {},
      pages: [page],
      markups: [arc],
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

    const visibleArc = host.querySelector<SVGPolylineElement>('[data-testid="markup-arc-select"] polyline')!;
    dispatchPointer(visibleArc, 'pointerdown', 150, 40);
    expect(setSelectedMarkupIds).toHaveBeenLastCalledWith(['arc-select']);

    dispatchPointer(layer, 'pointermove', 170, 40);
    expect(updateDocument).not.toHaveBeenCalled();

    dispatchPointer(layer, 'pointerup', 170, 40);
    expect(updateDocument).toHaveBeenCalledOnce();
    expect(documentModel.markups[0]).toMatchObject({
      start: { x: 70, y: 692 },
      end: { x: 270, y: 692 },
      mid: { x: 170, y: 752 },
    });
  });
});

function clickPoint(
  layer: SVGSVGElement,
  clientX: number,
  clientY: number,
  options: { readonly shiftKey?: boolean } = {},
): void {
  dispatchPointer(layer, 'pointerdown', clientX, clientY, options);
  dispatchPointer(layer, 'pointerup', clientX, clientY, options);
}

function movePointer(
  layer: SVGSVGElement,
  clientX: number,
  clientY: number,
  options: { readonly shiftKey?: boolean } = {},
): void {
  dispatchPointer(layer, 'pointermove', clientX, clientY, options);
}

function dispatchPointer(
  layer: Element,
  type: 'pointerdown' | 'pointermove' | 'pointerup',
  clientX: number,
  clientY: number,
  options: { readonly shiftKey?: boolean } = {},
): void {
  const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY, shiftKey: options.shiftKey });
  Object.defineProperty(event, 'pointerId', { value: 1 });
  act(() => layer.dispatchEvent(event));
}
