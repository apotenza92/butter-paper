import { describe, expect, it } from 'vitest';
import { resolveDocumentKeyboardNavigation } from './documentKeyboardNavigation';

describe('document keyboard navigation', () => {
  const pageIndices = [0, 1, 2, 3];

  it('maps Home and End to the document edges', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'home', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'document-edge', edge: 'top' });
    expect(resolveDocumentKeyboardNavigation({ action: 'end', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'document-edge', edge: 'bottom' });
  });

  it('always pans with unmodified arrow keys', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'arrow-up', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'viewport-scroll', axis: 'vertical', direction: -1, distance: 'step' });
    expect(resolveDocumentKeyboardNavigation({ action: 'arrow-down', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'viewport-scroll', axis: 'vertical', direction: 1, distance: 'step' });
    expect(resolveDocumentKeyboardNavigation({ action: 'arrow-left', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'viewport-scroll', axis: 'horizontal', direction: -1, distance: 'step' });
    expect(resolveDocumentKeyboardNavigation({ action: 'arrow-right', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'viewport-scroll', axis: 'horizontal', direction: 1, distance: 'step' });
  });

  it('scrolls Page Up and Page Down by a viewport', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'page-up', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'viewport-scroll', axis: 'vertical', direction: -1, distance: 'page' });
    expect(resolveDocumentKeyboardNavigation({ action: 'page-down', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'viewport-scroll', axis: 'vertical', direction: 1, distance: 'page' });
  });

  it('changes pages only for explicit previous-page and next-page shortcuts', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'previous-page', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'page', pageIndex: 1 });
    expect(resolveDocumentKeyboardNavigation({ action: 'next-page', pageIndices, currentPage: 2 }))
      .toEqual({ kind: 'page', pageIndex: 3 });
  });

  it('does nothing when explicit page navigation reaches a document boundary', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'previous-page', pageIndices, currentPage: 0 }))
      .toBeNull();
    expect(resolveDocumentKeyboardNavigation({ action: 'next-page', pageIndices, currentPage: 3 }))
      .toBeNull();
  });

  it('does nothing when there are no pages', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'home', pageIndices: [], currentPage: 0 })).toBeNull();
  });
});
