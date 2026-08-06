import { describe, expect, it } from 'vitest';
import { createLineMarkup, createPageTransform, createRectangleMarkup, pdfPoint, rect, type PageModel } from '@butter-paper/core';
import {
  constrainPointOrthogonally,
  findNearestSnapPoint,
  findObjectSnapTrackingPoint,
  getAnnotationSnapCandidates,
  getPdfContentSnapCandidates,
  isPointOnOrthogonalConstraint,
  toggleAcquiredTrackingPoint,
  type AcquiredTrackingPoint,
} from './snapping';

const page: PageModel = {
  id: 'page-1',
  index: 0,
  size: { width: 200, height: 200 },
  rotation: 0,
};

describe('snapping helpers', () => {
  it('indexes annotation corners, midpoints, centers, and edges', () => {
    const markup = createRectangleMarkup({
      id: 'rect-1',
      pageIndex: 0,
      rect: rect(10, 20, 40, 30),
    });

    const candidates = getAnnotationSnapCandidates([markup], page);

    expect(candidates).toHaveLength(13);
    expect(candidates).toContainEqual({
      kind: 'point',
      point: pdfPoint(30, 35),
      source: 'annotation',
      role: 'center',
      ownerId: 'rect-1',
    });
    expect(candidates).toContainEqual({
      kind: 'point',
      point: pdfPoint(30, 20),
      source: 'annotation',
      role: 'midpoint',
      ownerId: 'rect-1',
    });
    expect(candidates.filter((candidate) => candidate.kind === 'edge')).toHaveLength(4);
  });

  it('can exclude markups from the annotation snap index', () => {
    const visible = createLineMarkup({
      id: 'line-1',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(20, 10),
    });
    const excluded = createLineMarkup({
      id: 'line-2',
      pageIndex: 0,
      start: pdfPoint(30, 30),
      end: pdfPoint(40, 30),
    });

    const candidates = getAnnotationSnapCandidates([visible, excluded], page, {
      excludeMarkupIds: ['line-2'],
    });

    expect(candidates).toHaveLength(4);
    expect(candidates.every((candidate) => candidate.ownerId !== 'line-2')).toBe(true);
  });

  it('adds intersections for crossing annotation edges', () => {
    const horizontal = createLineMarkup({
      id: 'line-1',
      pageIndex: 0,
      start: pdfPoint(10, 50),
      end: pdfPoint(90, 50),
    });
    const vertical = createLineMarkup({
      id: 'line-2',
      pageIndex: 0,
      start: pdfPoint(40, 20),
      end: pdfPoint(40, 80),
    });

    const candidates = getAnnotationSnapCandidates([horizontal, vertical], page);

    expect(candidates).toContainEqual({
      kind: 'point',
      point: pdfPoint(40, 50),
      source: 'annotation',
      role: 'intersection',
    });
  });

  it('can exclude owners while finding the nearest snap point', () => {
    const transform = createPageTransform(page, 1);
    const visible = createLineMarkup({
      id: 'line-1',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(20, 10),
    });
    const excluded = createLineMarkup({
      id: 'line-2',
      pageIndex: 0,
      start: pdfPoint(30, 10),
      end: pdfPoint(40, 10),
    });
    const candidates = getAnnotationSnapCandidates([visible, excluded], page);

    const result = findNearestSnapPoint(pdfPoint(31, 10), candidates, transform, {
      tolerancePx: 25,
      excludeOwnerIds: ['line-2'],
    });

    expect(result?.candidate).toMatchObject({ ownerId: 'line-1' });
  });

  it('reuses PDF content snap candidates for the same geometry index object', () => {
    const index = {
      pageIndex: 0,
      buildMs: 1,
      primitives: [
        { kind: 'line' as const, start: pdfPoint(10, 20), end: pdfPoint(30, 20) },
      ],
    };

    expect(getPdfContentSnapCandidates(index)).toBe(getPdfContentSnapCandidates(index));
  });

  it('snaps to the nearest projected point on an edge within viewport tolerance', () => {
    const transform = createPageTransform(page, 1);
    const markup = createLineMarkup({
      id: 'line-1',
      pageIndex: 0,
      start: pdfPoint(10, 50),
      end: pdfPoint(90, 50),
    });
    const candidates = getAnnotationSnapCandidates([markup], page);

    const result = findNearestSnapPoint(pdfPoint(42, 53), candidates, transform, { tolerancePx: 5 });

    expect(result?.point).toEqual(pdfPoint(42, 50));
    expect(result?.candidate).toMatchObject({ kind: 'edge', ownerId: 'line-1' });
    expect(result?.distancePx).toBeCloseTo(3);
  });

  it('can limit snapping to configured target roles', () => {
    const transform = createPageTransform(page, 1);
    const candidates = getPdfContentSnapCandidates({
      pageIndex: 0,
      buildMs: 1,
      primitives: [
        { kind: 'line', start: pdfPoint(10, 50), end: pdfPoint(90, 50) },
      ],
    });

    expect(findNearestSnapPoint(pdfPoint(42, 53), candidates, transform, {
      tolerancePx: 5,
      snapTargets: ['endpoint', 'midpoint'],
    })).toBeNull();
    expect(findNearestSnapPoint(pdfPoint(42, 53), candidates, transform, {
      tolerancePx: 5,
      snapTargets: ['nearest'],
    })?.candidate).toMatchObject({ role: 'edge' });
  });

  it('adds intersections for crossing PDF content edges', () => {
    const candidates = getPdfContentSnapCandidates({
      pageIndex: 0,
      buildMs: 1,
      primitives: [
        { kind: 'line', start: pdfPoint(10, 50), end: pdfPoint(90, 50) },
        { kind: 'line', start: pdfPoint(40, 20), end: pdfPoint(40, 80) },
      ],
    });

    expect(candidates).toContainEqual({
      kind: 'point',
      point: pdfPoint(40, 50),
      source: 'pdf-content',
      role: 'intersection',
    });
  });

  it('biases precise snap roles ahead of a slightly closer nearest edge', () => {
    const transform = createPageTransform(page, 1);
    const candidates = [
      {
        kind: 'edge' as const,
        start: pdfPoint(0, 0),
        end: pdfPoint(20, 0),
        source: 'pdf-content' as const,
        role: 'edge' as const,
      },
      {
        kind: 'point' as const,
        point: pdfPoint(10, 0.6),
        source: 'pdf-content' as const,
        role: 'endpoint' as const,
      },
    ];

    const result = findNearestSnapPoint(pdfPoint(10, 0.1), candidates, transform, {
      tolerancePx: 8,
      snapTargets: ['endpoint', 'nearest'],
    });

    expect(result?.candidate).toMatchObject({ role: 'endpoint' });
  });

  it('ignores candidates outside the viewport tolerance', () => {
    const transform = createPageTransform(page, 1);
    const markup = createLineMarkup({
      id: 'line-1',
      pageIndex: 0,
      start: pdfPoint(10, 50),
      end: pdfPoint(90, 50),
    });
    const candidates = getAnnotationSnapCandidates([markup], page);

    expect(findNearestSnapPoint(pdfPoint(42, 60), candidates, transform, { tolerancePx: 5 })).toBeNull();
  });

  it('indexes PDF content geometry as snap candidates', () => {
    const candidates = getPdfContentSnapCandidates({
      pageIndex: 0,
      buildMs: 1,
      primitives: [
        { kind: 'line', start: pdfPoint(10, 20), end: pdfPoint(30, 20) },
        { kind: 'rect', rect: rect(40, 50, 20, 10) },
      ],
    });

    expect(candidates).toEqual(expect.arrayContaining([
      {
        kind: 'point',
        point: pdfPoint(10, 20),
        source: 'pdf-content',
        role: 'endpoint',
      },
      {
        kind: 'point',
        point: pdfPoint(50, 55),
        source: 'pdf-content',
        role: 'center',
      },
    ]));
    expect(candidates.every((candidate) => candidate.source === 'pdf-content')).toBe(true);
  });

  it('uses viewport-pixel sensitivity for content snapping', () => {
    const transform = createPageTransform(page, 2);
    const candidates = getPdfContentSnapCandidates({
      pageIndex: 0,
      buildMs: 1,
      primitives: [
        { kind: 'line', start: pdfPoint(10, 50), end: pdfPoint(90, 50) },
      ],
    });

    expect(findNearestSnapPoint(pdfPoint(42, 53), candidates, transform, { tolerancePx: 5 })).toBeNull();
    expect(findNearestSnapPoint(pdfPoint(42, 53), candidates, transform, { tolerancePx: 7 })?.point).toEqual(pdfPoint(42, 50));
  });

  it('constrains a point to the nearest orthogonal direction from its anchor', () => {
    expect(constrainPointOrthogonally(pdfPoint(10, 10), pdfPoint(42, 18))).toEqual({
      anchor: pdfPoint(10, 10),
      point: pdfPoint(42, 10),
      axis: 'horizontal',
    });
    expect(constrainPointOrthogonally(pdfPoint(10, 10), pdfPoint(14, 42))).toEqual({
      anchor: pdfPoint(10, 10),
      point: pdfPoint(10, 42),
      axis: 'vertical',
    });
  });

  it('rejects object snap points that would break an active orthogonal constraint', () => {
    const constraint = constrainPointOrthogonally(pdfPoint(10, 10), pdfPoint(42, 18));

    expect(isPointOnOrthogonalConstraint(pdfPoint(50, 10), constraint)).toBe(true);
    expect(isPointOnOrthogonalConstraint(pdfPoint(50, 11), constraint)).toBe(false);
  });

  it('acquires and removes temporary tracking points by revisiting their location', () => {
    const first = trackingPoint(pdfPoint(20, 30), 'endpoint');
    const second = trackingPoint(pdfPoint(80, 90), 'midpoint');

    expect(toggleAcquiredTrackingPoint([], first)).toEqual([first]);
    expect(toggleAcquiredTrackingPoint([first], second)).toEqual([first, second]);
    expect(toggleAcquiredTrackingPoint([first, second], first)).toEqual([second]);
  });

  it('keeps only the most recently acquired tracking points', () => {
    const points = [10, 20, 30, 40].map((x) => trackingPoint(pdfPoint(x, 10), 'endpoint'));
    const next = toggleAcquiredTrackingPoint(points, trackingPoint(pdfPoint(50, 10), 'endpoint'));

    expect(next.map((candidate) => candidate.point.x)).toEqual([20, 30, 40, 50]);
  });

  it('tracks along a distant horizontal or vertical alignment path in viewport pixels', () => {
    const transform = createPageTransform(page, 2);
    const acquired = [trackingPoint(pdfPoint(20, 30), 'endpoint')];

    expect(findObjectSnapTrackingPoint(pdfPoint(80, 32), acquired, transform, { tolerancePx: 5 })).toEqual({
      point: pdfPoint(80, 30),
      guides: [{ origin: pdfPoint(20, 30), axis: 'horizontal' }],
      distancePx: 4,
    });
    expect(findObjectSnapTrackingPoint(pdfPoint(80, 34), acquired, transform, { tolerancePx: 5 })).toBeNull();
  });

  it('combines alignment paths from two acquired points into a virtual intersection', () => {
    const transform = createPageTransform(page, 1);
    const acquired = [
      trackingPoint(pdfPoint(20, 30), 'endpoint'),
      trackingPoint(pdfPoint(80, 90), 'midpoint'),
    ];

    expect(findObjectSnapTrackingPoint(pdfPoint(78, 33), acquired, transform, { tolerancePx: 5 })).toEqual({
      point: pdfPoint(80, 30),
      guides: [
        { origin: pdfPoint(20, 30), axis: 'horizontal' },
        { origin: pdfPoint(80, 90), axis: 'vertical' },
      ],
      distancePx: Math.sqrt(13),
    });
  });

  it('limits tracking to the guide axis compatible with a Shift constraint', () => {
    const transform = createPageTransform(page, 1);
    const acquired = [trackingPoint(pdfPoint(80, 90), 'endpoint')];

    expect(findObjectSnapTrackingPoint(pdfPoint(78, 30), acquired, transform, {
      tolerancePx: 5,
      allowedGuideAxes: ['vertical'],
    })).toEqual({
      point: pdfPoint(80, 30),
      guides: [{ origin: pdfPoint(80, 90), axis: 'vertical' }],
      distancePx: 2,
    });
  });
});

function trackingPoint(point: ReturnType<typeof pdfPoint>, role: AcquiredTrackingPoint['role']): AcquiredTrackingPoint {
  return {
    kind: 'point',
    point,
    source: 'annotation',
    role,
    ownerId: `owner-${point.x}-${point.y}`,
  };
}
