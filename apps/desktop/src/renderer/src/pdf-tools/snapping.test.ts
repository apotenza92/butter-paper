import { describe, expect, it } from 'vitest';
import { createLineMarkup, createPageTransform, createRectangleMarkup, pdfPoint, rect, type PageModel } from '@butter-paper/core';
import { findNearestSnapPoint, getAnnotationSnapCandidates, getPdfContentSnapCandidates } from './snapping';

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
});
