export type DocumentKeyboardAction =
  | 'home'
  | 'end'
  | 'previous-page'
  | 'next-page'
  | 'page-up'
  | 'page-down'
  | 'arrow-up'
  | 'arrow-down'
  | 'arrow-left'
  | 'arrow-right';

export type DocumentKeyboardNavigation =
  | { readonly kind: 'document-edge'; readonly edge: 'top' | 'bottom' }
  | { readonly kind: 'page'; readonly pageIndex: number }
  | {
      readonly kind: 'viewport-scroll';
      readonly axis: 'horizontal' | 'vertical';
      readonly direction: -1 | 1;
      readonly distance: 'step' | 'page';
    }
  | null;

export function resolveDocumentKeyboardNavigation(options: {
  readonly action: DocumentKeyboardAction;
  readonly pageIndices: readonly number[];
  readonly currentPage: number;
}): DocumentKeyboardNavigation {
  const { action, pageIndices, currentPage } = options;
  if (pageIndices.length === 0) {
    return null;
  }

  if (action === 'home') {
    return { kind: 'document-edge', edge: 'top' };
  }
  if (action === 'end') {
    return { kind: 'document-edge', edge: 'bottom' };
  }

  if (action === 'arrow-left' || action === 'arrow-right') {
    return {
      kind: 'viewport-scroll',
      axis: 'horizontal',
      direction: action === 'arrow-left' ? -1 : 1,
      distance: 'step',
    };
  }
  if (action === 'arrow-up' || action === 'arrow-down' || action === 'page-up' || action === 'page-down') {
    return {
      kind: 'viewport-scroll',
      axis: 'vertical',
      direction: action === 'arrow-up' || action === 'page-up' ? -1 : 1,
      distance: action === 'page-up' || action === 'page-down' ? 'page' : 'step',
    };
  }

  const currentPosition = Math.max(0, pageIndices.indexOf(currentPage));
  const direction = action === 'previous-page' ? -1 : 1;
  const targetPosition = Math.min(pageIndices.length - 1, Math.max(0, currentPosition + direction));
  if (targetPosition === currentPosition) {
    return null;
  }

  return { kind: 'page', pageIndex: pageIndices[targetPosition]! };
}
