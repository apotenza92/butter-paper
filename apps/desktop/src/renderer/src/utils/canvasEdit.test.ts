import { describe, expect, it } from 'vitest';
import { pdfPoint, type Markup } from '@butter-paper/core';
import { markupIdsOnPage, pasteCanvasMarkups, selectedCanvasMarkups } from './canvasEdit';

const markups: Markup[] = [
  { id: 'one', kind: 'rectangle', pageIndex: 0, rect: { x: 10, y: 20, width: 30, height: 40 } },
  { id: 'two', kind: 'line', pageIndex: 1, start: pdfPoint(0, 0), end: pdfPoint(10, 10) },
];

describe('canvas edit commands', () => {
  it('copies only selected canvas markups and selects all on the current page', () => {
    expect(selectedCanvasMarkups(markups, ['two'])).toEqual([markups[1]]);
    expect(markupIdsOnPage(markups, 0)).toEqual(['one']);
  });

  it('pastes independent markups onto the current page with new ids and an offset', () => {
    const pasted = pasteCanvasMarkups([markups[0]], 2, 1, (kind) => `${kind}-copy`);
    expect(pasted).toEqual([{
      ...markups[0],
      id: 'rectangle-copy',
      pageIndex: 2,
      source: undefined,
      rect: { x: 22, y: 8, width: 30, height: 40 },
    }]);
  });
});
