import type { Markup, PageModel, PdfPoint, Rect } from '@butter-paper/core';
import { getMarkupToolDefinition } from './toolRegistry';

export interface MarkupSpatialIndexGenerationEntry {
  readonly id: string;
  readonly bounds: Rect;
}

export interface MarkupSpatialQueryReceipt {
  readonly totalMarkupCount: number;
  readonly indexedMarkupCount: number;
  readonly queriedCellCount: number;
  readonly candidateMarkupIds: readonly string[];
}

export interface MarkupSpatialIndex {
  readonly generation: readonly MarkupSpatialIndexGenerationEntry[];
  query(point: PdfPoint, tolerance: number): MarkupSpatialQueryReceipt;
}

function cellRange(bounds: Rect, cellSize: number): readonly [number, number, number, number] {
  return [
    Math.floor(bounds.x / cellSize),
    Math.floor(bounds.y / cellSize),
    Math.floor((bounds.x + bounds.width) / cellSize),
    Math.floor((bounds.y + bounds.height) / cellSize),
  ];
}

export function buildMarkupSpatialIndex(
  markups: readonly Markup[],
  page: PageModel,
  cellSize = 72,
): MarkupSpatialIndex {
  if (!Number.isFinite(cellSize) || cellSize <= 0) {
    throw new Error('Markup spatial-index cell size must be positive.');
  }
  const generation = markups.flatMap((markup): MarkupSpatialIndexGenerationEntry[] => {
    if (markup.pageIndex !== page.index) return [];
    const geometry = getMarkupToolDefinition(markup)?.geometry?.getGeometry(markup as never, { page });
    return geometry?.bounds ? [{ id: markup.id, bounds: geometry.bounds }] : [];
  });
  const cells = new Map<string, Set<string>>();
  for (const entry of generation) {
    const [startX, startY, endX, endY] = cellRange(entry.bounds, cellSize);
    for (let y = startY; y <= endY; y += 1) {
      for (let x = startX; x <= endX; x += 1) {
        const key = `${x}:${y}`;
        const ids = cells.get(key) ?? new Set<string>();
        ids.add(entry.id);
        cells.set(key, ids);
      }
    }
  }
  return {
    generation,
    query(point, tolerance) {
      if (!Number.isFinite(tolerance) || tolerance < 0) {
        throw new Error('Markup spatial-index tolerance must be non-negative.');
      }
      const queryBounds = {
        x: point.x - tolerance,
        y: point.y - tolerance,
        width: tolerance * 2,
        height: tolerance * 2,
      } as Rect;
      const [startX, startY, endX, endY] = cellRange(queryBounds, cellSize);
      const candidates = new Set<string>();
      let queriedCellCount = 0;
      for (let y = startY; y <= endY; y += 1) {
        for (let x = startX; x <= endX; x += 1) {
          queriedCellCount += 1;
          for (const id of cells.get(`${x}:${y}`) ?? []) candidates.add(id);
        }
      }
      return {
        totalMarkupCount: markups.filter((markup) => markup.pageIndex === page.index).length,
        indexedMarkupCount: generation.length,
        queriedCellCount,
        candidateMarkupIds: generation.flatMap(({ id }) => candidates.has(id) ? [id] : []),
      };
    },
  };
}
