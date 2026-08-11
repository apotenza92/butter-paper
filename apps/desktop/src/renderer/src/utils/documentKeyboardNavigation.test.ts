import { describe, expect, it } from 'vitest';
import { resolveDocumentKeyboardNavigation } from './documentKeyboardNavigation';

describe('document keyboard navigation', () => {
  const pageIndices = [0, 1, 2, 3];

  it('moves Home and End to the document edges', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'home', pageIndices, currentPage: 2, scrollMode: 'continuous' }))
      .toEqual({ kind: 'document-edge', edge: 'top' });
    expect(resolveDocumentKeyboardNavigation({ action: 'end', pageIndices, currentPage: 2, scrollMode: 'single-page' }))
      .toEqual({ kind: 'document-edge', edge: 'bottom' });
  });

  it('moves continuous mode to the neighbouring page top', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'page-up', pageIndices, currentPage: 2, scrollMode: 'continuous' }))
      .toEqual({ kind: 'page', pageIndex: 1, resetZoom: false });
    expect(resolveDocumentKeyboardNavigation({ action: 'page-down', pageIndices, currentPage: 2, scrollMode: 'continuous' }))
      .toEqual({ kind: 'page', pageIndex: 3, resetZoom: false });
  });

  it('moves single-page mode and requests a zoom reset', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'page-up', pageIndices, currentPage: 2, scrollMode: 'single-page' }))
      .toEqual({ kind: 'page', pageIndex: 1, resetZoom: true });
  });

  it('uses document edges at continuous-mode boundaries', () => {
    expect(resolveDocumentKeyboardNavigation({ action: 'page-up', pageIndices, currentPage: 0, scrollMode: 'continuous' }))
      .toEqual({ kind: 'document-edge', edge: 'top' });
    expect(resolveDocumentKeyboardNavigation({ action: 'page-down', pageIndices, currentPage: 3, scrollMode: 'continuous' }))
      .toEqual({ kind: 'document-edge', edge: 'bottom' });
  });
});
