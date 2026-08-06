import { describe, expect, it } from 'vitest';
import { pdfPoint, rect } from '@butter-paper/core';
import {
  createArmedBoxSelectionMarquee,
  createSelectionMarquee,
  isGeometrySelectedByMarquee,
  resolvedSelectionMarqueeKind,
  selectionAfterMarquee,
  selectionMarqueeKind,
  selectionMarqueeOperationFromModifiers,
  updateSelectionMarquee,
} from './selectionMarquee';

const identityTransform = {
  pdfToViewport: (point: { readonly x: number; readonly y: number }) => point,
  pdfRectToViewport: (box: { readonly x: number; readonly y: number; readonly width: number; readonly height: number }) => box,
};

function boxMarquee(start: { x: number; y: number }, current: { x: number; y: number }) {
  return updateSelectionMarquee(createArmedBoxSelectionMarquee(start), current, 0);
}

describe('CAD selection marquee', () => {
  it('uses left-to-right window selection and right-to-left crossing selection', () => {
    expect(selectionMarqueeKind({ x: 10, y: 10 }, { x: 100, y: 80 })).toBe('window');
    expect(selectionMarqueeKind({ x: 100, y: 10 }, { x: 10, y: 80 })).toBe('crossing');
  });

  it('latches after Fraia-compatible six-pixel activation movement', () => {
    const started = createSelectionMarquee(1, { x: 10, y: 10 });
    const belowThreshold = updateSelectionMarquee(started, { x: 16, y: 10 });
    const active = updateSelectionMarquee(belowThreshold, { x: 17, y: 10 });

    expect(belowThreshold.active).toBe(false);
    expect(active.active).toBe(true);
    expect(updateSelectionMarquee(active, { x: 10, y: 10 }).active).toBe(true);
  });

  it('locks a drag lasso to its first clear horizontal direction', () => {
    let windowLasso = createSelectionMarquee(1, { x: 10, y: 10 });
    windowLasso = updateSelectionMarquee(windowLasso, { x: 18, y: 30 });
    windowLasso = updateSelectionMarquee(windowLasso, { x: 0, y: 40 });

    let crossingLasso = createSelectionMarquee(1, { x: 40, y: 10 });
    crossingLasso = updateSelectionMarquee(crossingLasso, { x: 32, y: 30 });
    crossingLasso = updateSelectionMarquee(crossingLasso, { x: 50, y: 40 });

    expect(resolvedSelectionMarqueeKind(windowLasso)).toBe('window');
    expect(resolvedSelectionMarqueeKind(crossingLasso)).toBe('crossing');
  });

  it('keeps lasso selection semantics stable after the pointer crosses its origin', () => {
    const geometry = {
      bounds: rect(5, 15, 25, 0),
      components: [{
        id: 'line.body',
        role: 'shape' as const,
        geometry: { kind: 'line' as const, start: pdfPoint(5, 15), end: pdfPoint(30, 15) },
        bodyDrag: 'moveSelf' as const,
      }],
    };
    let lasso = createSelectionMarquee(1, { x: 10, y: 10 });
    lasso = updateSelectionMarquee(lasso, { x: 40, y: 10 }, 0);
    lasso = updateSelectionMarquee(lasso, { x: 40, y: 40 }, 0);
    lasso = updateSelectionMarquee(lasso, { x: 0, y: 40 }, 0);

    expect(resolvedSelectionMarqueeKind(lasso)).toBe('window');
    expect(isGeometrySelectedByMarquee(geometry, lasso, identityTransform as never)).toBe(false);
  });

  it('requires complete containment for window selection but accepts intersections when crossing', () => {
    const geometry = {
      bounds: rect(10, 10, 90, 10),
      components: [{
        id: 'line.body',
        role: 'shape' as const,
        geometry: { kind: 'line' as const, start: pdfPoint(10, 15), end: pdfPoint(100, 15) },
        bodyDrag: 'moveSelf' as const,
      }],
    };

    expect(isGeometrySelectedByMarquee(
      geometry,
      boxMarquee({ x: 0, y: 0 }, { x: 50, y: 30 }),
      identityTransform as never,
    )).toBe(false);
    expect(isGeometrySelectedByMarquee(
      geometry,
      boxMarquee({ x: 50, y: 30 }, { x: 0, y: 0 }),
      identityTransform as never,
    )).toBe(true);
  });

  it('selects every component of a composite only when the window contains all of them', () => {
    const geometry = {
      bounds: rect(10, 10, 50, 30),
      components: [
        {
          id: 'callout.text',
          role: 'textBox' as const,
          geometry: { kind: 'textBox' as const, rect: rect(40, 10, 20, 20) },
          bodyDrag: 'moveSelf' as const,
        },
        {
          id: 'callout.leader',
          role: 'leader' as const,
          geometry: { kind: 'polyline' as const, points: [pdfPoint(10, 20), pdfPoint(40, 20)] },
          bodyDrag: 'moveGroup' as const,
        },
      ],
    };

    expect(isGeometrySelectedByMarquee(
      geometry,
      boxMarquee({ x: 35, y: 5 }, { x: 65, y: 35 }),
      identityTransform as never,
    )).toBe(false);
    expect(isGeometrySelectedByMarquee(
      geometry,
      boxMarquee({ x: 5, y: 5 }, { x: 65, y: 35 }),
      identityTransform as never,
    )).toBe(true);
  });

  it('uses explicit replace, add, and remove operations for zonal selection', () => {
    expect(selectionAfterMarquee(['rect-1'], ['line-1', 'ellipse-1'], 'replace')).toEqual(['line-1', 'ellipse-1']);
    expect(selectionAfterMarquee(['rect-1', 'line-1'], ['line-1', 'ellipse-1'], 'add')).toEqual(['rect-1', 'line-1', 'ellipse-1']);
    expect(selectionAfterMarquee(['rect-1', 'line-1'], ['line-1', 'ellipse-1'], 'remove')).toEqual(['rect-1']);
    expect(selectionMarqueeOperationFromModifiers({ shiftKey: false, altKey: false })).toBe('replace');
    expect(selectionMarqueeOperationFromModifiers({ shiftKey: true, altKey: false })).toBe('add');
    expect(selectionMarqueeOperationFromModifiers({ shiftKey: false, altKey: true })).toBe('remove');
  });

  it('uses the freehand polygon for drag-lasso selection', () => {
    const geometry = {
      bounds: rect(20, 20, 10, 10),
      components: [{
        id: 'rect.body',
        role: 'shape' as const,
        geometry: { kind: 'rect' as const, rect: rect(20, 20, 10, 10) },
        bodyDrag: 'moveSelf' as const,
      }],
    };
    let lasso = createSelectionMarquee(1, { x: 10, y: 10 });
    lasso = updateSelectionMarquee(lasso, { x: 40, y: 10 }, 0);
    lasso = updateSelectionMarquee(lasso, { x: 40, y: 40 }, 0);
    lasso = updateSelectionMarquee(lasso, { x: 10, y: 40 }, 0);

    expect(lasso.shape).toBe('lasso');
    expect(isGeometrySelectedByMarquee(geometry, lasso, identityTransform as never)).toBe(true);
  });
});
