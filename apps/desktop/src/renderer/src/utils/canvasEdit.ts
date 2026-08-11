import { translateMarkup, type Markup } from '@butter-paper/core';

const PASTE_OFFSET_PT = 12;

export function selectedCanvasMarkups(
  markups: readonly Markup[],
  selectedMarkupIds: readonly string[],
): Markup[] {
  const selected = new Set(selectedMarkupIds);
  return markups.filter((markup) => selected.has(markup.id));
}

export function markupIdsOnPage(markups: readonly Markup[], pageIndex: number): string[] {
  return markups.filter((markup) => markup.pageIndex === pageIndex).map((markup) => markup.id);
}

export function pasteCanvasMarkups(
  clipboard: readonly Markup[],
  pageIndex: number,
  pasteSequence: number,
  createId: (kind: Markup['kind']) => string,
): Markup[] {
  const offset = PASTE_OFFSET_PT * Math.max(1, pasteSequence);
  return clipboard.map((markup) => translateMarkup({
    ...markup,
    id: createId(markup.kind),
    pageIndex,
    source: undefined,
  }, { x: offset, y: -offset }));
}
