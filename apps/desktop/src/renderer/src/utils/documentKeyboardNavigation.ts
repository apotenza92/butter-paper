import type { ScrollMode } from '../../../shared/protocol';

export type DocumentKeyboardAction = 'home' | 'end' | 'page-up' | 'page-down';

export type DocumentKeyboardNavigation =
  | { readonly kind: 'document-edge'; readonly edge: 'top' | 'bottom' }
  | { readonly kind: 'page'; readonly pageIndex: number; readonly resetZoom: boolean }
  | null;

export function resolveDocumentKeyboardNavigation(options: {
  readonly action: DocumentKeyboardAction;
  readonly pageIndices: readonly number[];
  readonly currentPage: number;
  readonly scrollMode: ScrollMode;
}): DocumentKeyboardNavigation {
  const { action, pageIndices, currentPage, scrollMode } = options;
  if (pageIndices.length === 0) {
    return null;
  }

  if (action === 'home') {
    return { kind: 'document-edge', edge: 'top' };
  }
  if (action === 'end') {
    return { kind: 'document-edge', edge: 'bottom' };
  }

  const currentPosition = Math.max(0, pageIndices.indexOf(currentPage));
  const direction = action === 'page-down' ? 1 : -1;
  const targetPosition = Math.min(pageIndices.length - 1, Math.max(0, currentPosition + direction));
  if (targetPosition === currentPosition) {
    return scrollMode === 'continuous'
      ? { kind: 'document-edge', edge: direction < 0 ? 'top' : 'bottom' }
      : null;
  }

  return {
    kind: 'page',
    pageIndex: pageIndices[targetPosition]!,
    resetZoom: scrollMode === 'single-page',
  };
}
