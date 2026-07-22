import type { MarkupId, SelectionState } from './document.js';
import { uniqueIds } from './document.js';

export function createSelection(markupIds: readonly MarkupId[] = [], focusedMarkupId?: MarkupId): SelectionState {
  return {
    markupIds: uniqueIds(markupIds),
    focusedMarkupId: focusedMarkupId ?? markupIds[0],
  };
}

export function replaceSelection(selection: SelectionState, markupIds: readonly MarkupId[]): SelectionState {
  const ids = uniqueIds(markupIds);
  return {
    markupIds: ids,
    focusedMarkupId: ids[0],
  };
}

export function addToSelection(selection: SelectionState, markupId: MarkupId): SelectionState {
  const ids = uniqueIds([...selection.markupIds, markupId]);
  return {
    markupIds: ids,
    focusedMarkupId: markupId,
  };
}

export function clearSelection(): SelectionState {
  return {
    markupIds: [],
  };
}
