import { describe, expect, it } from 'vitest';
import { pdfPoint, rect, type PageModel, type RectangleMarkup } from '@butter-paper/core';
import { buildMarkupSpatialIndex } from './markupSpatialIndex';

describe('maintained markup spatial index', () => {
  const page = { index: 1, size: { width: 612, height: 792 }, rotation: 0 } as PageModel;
  const markups = Array.from({ length: 100 }, (_, index): RectangleMarkup => ({
    id: `dense-${index}`,
    pageIndex: 1,
    kind: 'rectangle',
    rect: rect((index % 10) * 54, Math.floor(index / 10) * 54, 36, 24),
  }));

  it('records the exact generation and returns a bounded point-query candidate set', () => {
    const index = buildMarkupSpatialIndex(markups, page, 54);
    const query = index.query(pdfPoint(20, 12), 4);

    expect(index.generation).toHaveLength(100);
    expect(index.generation[0]).toEqual({ id: 'dense-0', bounds: rect(0, 0, 36, 24) });
    expect(query.totalMarkupCount).toBe(100);
    expect(query.indexedMarkupCount).toBe(100);
    expect(query.candidateMarkupIds).toContain('dense-0');
    expect(query.candidateMarkupIds.length).toBeLessThan(100);
    expect(query.queriedCellCount).toBeGreaterThan(0);
  });

  it('excludes another page and an out-of-range cell from the query receipt', () => {
    const index = buildMarkupSpatialIndex([
      ...markups,
      { ...markups[0], id: 'other-page', pageIndex: 0 },
    ], page, 54);

    expect(index.generation).toHaveLength(100);
    expect(index.query(pdfPoint(600, 780), 1).candidateMarkupIds).toEqual([]);
  });
});
