// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { createPageTransform, createRectangleMarkup, pdfPoint, rect } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AnnotationLayer } from './AnnotationLayer';

describe('page scale calibration interaction', () => {
  let host: HTMLDivElement;
  let root: Root;

  const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
  const source = createRectangleMarkup({
    id: 'snap-source',
    pageIndex: 0,
    rect: rect(100, 650, 100, 100),
  });

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

  it('picks a snapped point without starting a selection marquee', () => {
    const onCalibrationPoint = vi.fn();
    const layer = renderLayer(null, onCalibrationPoint);

    dispatchPointer(layer, 'pointerdown', 104, 45);
    dispatchPointer(layer, 'pointermove', 160, 100);

    expect(onCalibrationPoint).toHaveBeenCalledWith(0, pdfPoint(100, 750));
    expect(host.querySelector('[data-testid="selection-marquee"]')).toBeNull();
  });

  it('draws a line and holds its second point orthogonal while Shift is pressed', () => {
    const onCalibrationPoint = vi.fn();
    const layer = renderLayer(pdfPoint(50, 750), onCalibrationPoint);

    dispatchPointer(layer, 'pointermove', 130, 60, true);

    const line = host.querySelector<SVGLineElement>('[data-testid="page-scale-calibration-line"]')!;
    expect(line).toBeTruthy();
    expect(line.getAttribute('x1')).toBe('50');
    expect(line.getAttribute('y1')).toBe('42');
    expect(line.getAttribute('x2')).toBe('130');
    expect(line.getAttribute('y2')).toBe('42');

    dispatchPointer(layer, 'pointerdown', 130, 60, true);
    expect(onCalibrationPoint).toHaveBeenCalledWith(0, pdfPoint(130, 750));
    expect(host.querySelector('[data-testid="selection-marquee"]')).toBeNull();
  });

  function renderLayer(
    calibrationStartPoint: ReturnType<typeof pdfPoint> | null,
    onCalibrationPoint: (pageIndex: number, point: ReturnType<typeof pdfPoint>) => void,
  ): SVGSVGElement {
    act(() => root.render(createElement(AnnotationLayer, {
      page,
      markups: [source],
      transform: createPageTransform(page, 1),
      activeTool: 'select',
      snapToContent: true,
      snapToMarkup: true,
      snapTargets: ['endpoint', 'midpoint', 'center', 'intersection'],
      selectedMarkupIds: [],
      postPlacement: null,
      pendingImageAsset: null,
      setSelectedMarkupIds: vi.fn(),
      setPostPlacement: vi.fn(),
      consumePendingImageAsset: vi.fn(() => null),
      updateDocument: vi.fn(),
      calibrationPickActive: true,
      calibrationStartPoint,
      onCalibrationPoint,
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
    return layer;
  }
});

function dispatchPointer(
  layer: SVGSVGElement,
  type: 'pointerdown' | 'pointermove',
  clientX: number,
  clientY: number,
  shiftKey = false,
): void {
  const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY, shiftKey });
  Object.defineProperty(event, 'pointerId', { value: 1 });
  act(() => layer.dispatchEvent(event));
}
