import { describe, expect, it } from 'vitest';
import { buildPageLayouts, computeVisibleLayoutPositions, computeVisibleRange } from './virtualisation';

describe('virtualisation helpers', () => {
  it('builds stable page layouts and a visible range', () => {
    const pages = [
      { index: 0, width: 600, height: 800 },
      { index: 1, width: 600, height: 1000 },
      { index: 2, width: 600, height: 900 },
    ];

    const { layouts, totalHeight } = buildPageLayouts(pages, 1, 1200, 24);

    expect(layouts).toHaveLength(3);
    expect(totalHeight).toBeGreaterThan(0);
    expect(layouts[0]?.top).toBe(24);
    expect(layouts[1]?.top).toBeGreaterThan(layouts[0]!.top);
    expect(totalHeight).toBe(layouts[2]!.top + layouts[2]!.height + 24);
    expect(layouts[2]).toMatchObject({ columnIndex: 0, rowIndex: 2 });

    const visible = computeVisibleRange(layouts, 760, 600, 1);
    expect(visible.startIndex).toBeLessThanOrEqual(1);
    expect(visible.endIndex).toBeGreaterThanOrEqual(1);
  });

  it('returns an empty bootstrap range before the viewport is measured', () => {
    const { layouts } = buildPageLayouts([
      { index: 0, width: 600, height: 800 },
      { index: 1, width: 600, height: 1000 },
    ], 1, 1200, 24);

    expect(computeVisibleRange(layouts, 0, 0, 2, 0)).toEqual({ startIndex: 0, endIndex: -1 });
  });

  it('adds optional pan padding around the laid-out pages', () => {
    const { layouts, totalHeight, totalWidth } = buildPageLayouts(
      [{ index: 0, width: 600, height: 800 }],
      2,
      1000,
      24,
      { left: 500, right: 500, top: 300, bottom: 300 },
    );

    expect(layouts[0]?.left).toBe(524);
    expect(layouts[0]?.top).toBe(324);
    expect(totalWidth).toBe(2248);
    expect(totalHeight).toBe(2248);
  });

  it('can lay pages out in columns by pages per column', () => {
    const pages = Array.from({ length: 6 }, (_, index) => ({ index, width: 100, height: 200 }));
    const { layouts, totalWidth } = buildPageLayouts(
      pages,
      1,
      800,
      20,
      { left: 0, right: 0, top: 0, bottom: 0 },
      { mode: 'columns', pagesPerColumn: 2, viewportHeight: 600 },
    );

    expect(layouts).toHaveLength(6);
    expect(layouts.map((layout) => [layout.columnIndex, layout.rowIndex])).toEqual([
      [0, 0],
      [0, 1],
      [1, 0],
      [1, 1],
      [2, 0],
      [2, 1],
    ]);
    expect(layouts[2]!.left).toBeGreaterThan(layouts[0]!.left);
    expect(totalWidth).toBeGreaterThanOrEqual(800);
  });

  it('can lay pages out in rows by pages per row', () => {
    const pages = Array.from({ length: 6 }, (_, index) => ({ index, width: 100, height: 200 }));
    const { layouts, totalHeight } = buildPageLayouts(
      pages,
      1,
      800,
      20,
      { left: 0, right: 0, top: 0, bottom: 0 },
      { mode: 'columns', cadViewOrganisation: 'rows', pagesPerColumn: 3, viewportHeight: 600 },
    );

    expect(layouts).toHaveLength(6);
    expect(layouts.map((layout) => [layout.columnIndex, layout.rowIndex])).toEqual([
      [0, 0],
      [1, 0],
      [2, 0],
      [0, 1],
      [1, 1],
      [2, 1],
    ]);
    expect(layouts[1]!.left).toBeGreaterThan(layouts[0]!.left);
    expect(layouts[3]!.top).toBeGreaterThan(layouts[0]!.top);
    expect(totalHeight).toBe(20 + 200 + 20 + 200 + 20);
  });

  it('keeps column layout stable with differently sized pages', () => {
    const pages = [
      { index: 0, width: 100, height: 100 },
      { index: 1, width: 140, height: 80 },
      { index: 2, width: 90, height: 160 },
      { index: 3, width: 130, height: 120 },
      { index: 4, width: 110, height: 90 },
    ];
    const { layouts, totalHeight, totalWidth } = buildPageLayouts(
      pages,
      0.5,
      300,
      12,
      { left: 4, right: 6, top: 8, bottom: 10 },
      { mode: 'columns', pagesPerColumn: 2, viewportHeight: 260 },
    );

    expect(layouts.map((layout) => [layout.index, layout.columnIndex, layout.rowIndex])).toEqual([
      [0, 0, 0],
      [1, 0, 1],
      [2, 1, 0],
      [3, 1, 1],
      [4, 2, 0],
    ]);
    expect(layouts[0]?.top).toBe(layouts[2]?.top);
    expect(layouts[2]?.top).toBe(layouts[4]?.top);
    expect(layouts[1]?.top).toBe(layouts[3]?.top);
    expect(totalHeight).toBe(8 + 12 + 80 + 12 + 60 + 12 + 10);
    expect(totalWidth).toBeGreaterThanOrEqual(300);
  });

  it('aligns column rows to shared top edges when page sizes vary', () => {
    const { layouts } = buildPageLayouts(
      [
        { index: 0, width: 100, height: 80 },
        { index: 1, width: 100, height: 200 },
        { index: 2, width: 100, height: 120 },
        { index: 3, width: 100, height: 90 },
      ],
      1,
      500,
      10,
      { left: 0, right: 0, top: 0, bottom: 0 },
      { mode: 'columns', pagesPerColumn: 2, viewportHeight: 400 },
    );

    expect(layouts[0]?.top).toBe(layouts[2]?.top);
    expect(layouts[1]?.top).toBe(layouts[3]?.top);
    expect(layouts[1]?.top).toBe(10 + 120 + 10);
  });

  it('can lay out only the active page in single-page mode', () => {
    const { layouts, totalHeight, totalWidth } = buildPageLayouts(
      [
        { index: 0, width: 600, height: 800 },
        { index: 1, width: 400, height: 400 },
      ],
      1,
      1000,
      24,
      { left: 0, right: 0, top: 0, bottom: 0 },
      { mode: 'single-page', currentPageIndex: 1, viewportHeight: 900 },
    );

    expect(layouts).toHaveLength(1);
    expect(layouts[0]).toMatchObject({ index: 1, columnIndex: 0, rowIndex: 0 });
    expect(totalWidth).toBe(1000);
    expect(totalHeight).toBe(900);
  });

  it('finds visible layouts in two dimensions for column mode', () => {
    const { layouts } = buildPageLayouts(
      Array.from({ length: 6 }, (_, index) => ({ index, width: 100, height: 100 })),
      1,
      500,
      20,
      { left: 0, right: 0, top: 0, bottom: 0 },
      { mode: 'columns', pagesPerColumn: 2, viewportHeight: 400 },
    );

    const visiblePositions = computeVisibleLayoutPositions(layouts, {
      left: layouts[2]!.left - 5,
      top: 0,
      width: 120,
      height: 260,
    });

    expect(visiblePositions.map((position) => layouts[position]!.index)).toEqual([2, 3]);
  });
});
