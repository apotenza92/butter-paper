import type { DocumentModel, Markup, MarkupId, SelectionState } from './document.js';
import type { PointLike } from './points.js';
import { replaceSelection as selectMarkupIds } from './selection.js';
import { setCalloutText, translateMarkup } from './markup.js';

export function createMarkup(document: DocumentModel, markup: Markup): DocumentModel {
  return {
    ...document,
    markups: [...document.markups, markup],
  };
}

export function moveMarkup(document: DocumentModel, markupId: MarkupId, delta: PointLike): DocumentModel {
  return {
    ...document,
    markups: document.markups.map((markup) =>
      markup.id === markupId ? translateMarkup(markup, delta) : markup,
    ),
  };
}

export function updateMarkupText(document: DocumentModel, markupId: MarkupId, text: string): DocumentModel {
  return {
    ...document,
    markups: document.markups.map((markup) =>
      markup.id === markupId ? setCalloutText(markup, text) : markup,
    ),
  };
}

export function selectMarkups(selection: SelectionState, markupIds: readonly MarkupId[]): SelectionState {
  return selectMarkupIds(selection, markupIds);
}
