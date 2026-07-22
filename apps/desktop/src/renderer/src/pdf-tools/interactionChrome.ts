import type { Rect } from '@butter-paper/core';

export type InteractionState = 'idle' | 'hovered' | 'selected' | 'focused' | 'draft';
export type ChromeBoundsKind = 'child' | 'group';
export type ResizeHandleKind = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w';

export interface ChromeStyle {
  readonly boundsStroke: string;
  readonly haloStroke: string;
  readonly handleFill: string;
  readonly handleStroke: string;
  readonly strokeDasharray?: string;
  readonly boundsStrokeWidth: number;
  readonly haloStrokeWidth: number;
  readonly handleSize: number;
  readonly boundsOutsetPx: number;
}

export interface ResizeHandle {
  readonly kind: ResizeHandleKind;
  readonly x: number;
  readonly y: number;
  readonly cursor: string;
}

export const ROTATE_CURSOR = lucideCursor({
  paths: [
    'M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8',
    'M21 3v5h-5',
  ],
  fallback: 'none',
});

export const MOVE_CURSOR = lucideCursor({
  paths: [
    'M12 2v20',
    'm15 19-3 3-3-3',
    'm19 9 3 3-3 3',
    'M2 12h20',
    'm5 9-3 3 3 3',
    'm9 5 3-3 3 3',
  ],
  fallback: 'none',
});

export const CHROME_TOKENS = {
  selectedBounds: '#2563eb',
  groupBounds: '#60a5fa',
  neutralHalo: 'rgba(255,255,255,0.92)',
  hoverBounds: '#93c5fd',
  focusBounds: '#1d4ed8',
  draftBounds: '#0f766e',
  primaryHandle: '#facc15',
  secondaryHandle: '#ffffff',
  handleStroke: '#111827',
} as const;

export function getChromeStyle(state: InteractionState, boundsKind: ChromeBoundsKind = 'child'): ChromeStyle {
  if (state === 'draft') {
    return {
      boundsStroke: CHROME_TOKENS.draftBounds,
      haloStroke: CHROME_TOKENS.neutralHalo,
      handleFill: CHROME_TOKENS.secondaryHandle,
      handleStroke: CHROME_TOKENS.draftBounds,
      strokeDasharray: '6 4',
      boundsStrokeWidth: 1.5,
      haloStrokeWidth: 5,
      handleSize: 0,
      boundsOutsetPx: 0,
    };
  }

  if (state === 'hovered') {
    return {
      boundsStroke: CHROME_TOKENS.hoverBounds,
      haloStroke: CHROME_TOKENS.neutralHalo,
      handleFill: '#fef08a',
      handleStroke: CHROME_TOKENS.primaryHandle,
      strokeDasharray: '4 3',
      boundsStrokeWidth: 1.25,
      haloStrokeWidth: 4,
      handleSize: 6,
      boundsOutsetPx: 8,
    };
  }

  if (boundsKind === 'group') {
    return {
      boundsStroke: CHROME_TOKENS.groupBounds,
      haloStroke: CHROME_TOKENS.neutralHalo,
      handleFill: CHROME_TOKENS.secondaryHandle,
      handleStroke: CHROME_TOKENS.groupBounds,
      strokeDasharray: '8 5',
      boundsStrokeWidth: 1.25,
      haloStrokeWidth: 5,
      handleSize: 0,
      boundsOutsetPx: 8,
    };
  }

  return {
    boundsStroke: state === 'focused' ? CHROME_TOKENS.focusBounds : CHROME_TOKENS.selectedBounds,
    haloStroke: CHROME_TOKENS.neutralHalo,
    handleFill: state === 'focused' ? CHROME_TOKENS.primaryHandle : CHROME_TOKENS.secondaryHandle,
    handleStroke: CHROME_TOKENS.handleStroke,
    strokeDasharray: '5 4',
    boundsStrokeWidth: state === 'focused' ? 1.75 : 1.5,
    haloStrokeWidth: 5,
    handleSize: 7,
    boundsOutsetPx: 8,
  };
}

export function getResizeHandles(rect: Rect): readonly ResizeHandle[] {
  const centerX = rect.x + rect.width * 0.5;
  const centerY = rect.y + rect.height * 0.5;
  const right = rect.x + rect.width;
  const visualTop = rect.y + rect.height;
  const visualBottom = rect.y;

  return [
    { kind: 'nw', x: rect.x, y: visualTop, cursor: getResizeCursor('nw') },
    { kind: 'n', x: centerX, y: visualTop, cursor: getResizeCursor('n') },
    { kind: 'ne', x: right, y: visualTop, cursor: getResizeCursor('ne') },
    { kind: 'e', x: right, y: centerY, cursor: getResizeCursor('e') },
    { kind: 'se', x: right, y: visualBottom, cursor: getResizeCursor('se') },
    { kind: 's', x: centerX, y: visualBottom, cursor: getResizeCursor('s') },
    { kind: 'sw', x: rect.x, y: visualBottom, cursor: getResizeCursor('sw') },
    { kind: 'w', x: rect.x, y: centerY, cursor: getResizeCursor('w') },
  ];
}

export function getRotationHandle(rect: Rect, offset = 12): ResizeHandle {
  return {
    kind: 'n',
    x: rect.x + rect.width * 0.5,
    y: rect.y + rect.height + offset,
    cursor: ROTATE_CURSOR,
  };
}

export function getMoveCursor(rotation = 0): string {
  return lucideCursor({
    paths: [
      'M12 2v20',
      'm15 19-3 3-3-3',
      'm19 9 3 3-3 3',
      'M2 12h20',
      'm5 9-3 3 3 3',
      'm9 5 3-3 3 3',
    ],
    rotation,
    fallback: 'none',
  });
}

export function getResizeCursor(handle: ResizeHandleKind, rotation = 0): string {
  return lucideCursor({
    paths: ['M18 8 22 12 18 16', 'M6 8 2 12 6 16', 'M2 12H22'],
    rotation: baseResizeCursorRotation(handle) + rotation,
    fallback: 'none',
  });
}

export function getRotateCursor(rotation = 0): string {
  return lucideCursor({
    paths: [
      'M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8',
      'M21 3v5h-5',
    ],
    rotation,
    fallback: 'none',
  });
}

function baseResizeCursorRotation(handle: ResizeHandleKind): number {
  if (handle === 'n' || handle === 's') {
    return 90;
  }
  if (handle === 'nw' || handle === 'se') {
    return 45;
  }
  if (handle === 'ne' || handle === 'sw') {
    return -45;
  }
  return 0;
}

function lucideCursor({
  paths,
  rotation = 0,
  fallback,
}: {
  readonly paths: readonly string[];
  readonly rotation?: number;
  readonly fallback: string;
}): string {
  const haloMarkup = paths.map((d) => `<path d="${d}" stroke="#fff" stroke-width="7.333"/>`).join('');
  const pathMarkup = paths.map((d) => `<path d="${d}" stroke="#111827" stroke-width="2"/>`).join('');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke-linecap="round" stroke-linejoin="round"><g transform="rotate(${normalizeDegrees(rotation)} 12 12)"><g transform="translate(3 3) scale(0.75)">${haloMarkup}${pathMarkup}</g></g></svg>`;

  return `url("data:image/svg+xml,${encodeURIComponent(svg)}") 12 12, ${fallback}`;
}

function normalizeDegrees(value: number): number {
  return ((value % 360) + 360) % 360;
}
