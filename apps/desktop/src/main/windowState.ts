import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

export const WINDOW_STATE_FILE_NAME = 'window-state.json';

export interface WindowBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface PersistedWindowState {
  schemaVersion: 1;
  bounds: WindowBounds;
}

let windowStateWriteSequence = 0;

export function loadWindowBounds(statePath: string): WindowBounds | null {
  try {
    return parseWindowState(JSON.parse(readFileSync(statePath, 'utf8')))?.bounds ?? null;
  } catch {
    return null;
  }
}

export function writeWindowBoundsAtomic(statePath: string, bounds: WindowBounds): void {
  const state: PersistedWindowState = {
    schemaVersion: 1,
    bounds,
  };
  mkdirSync(dirname(statePath), { recursive: true });
  windowStateWriteSequence += 1;
  const temporaryPath = `${statePath}.${process.pid}.${Date.now()}.${windowStateWriteSequence}.tmp`;
  try {
    writeFileSync(temporaryPath, `${JSON.stringify(state, null, 2)}\n`, {
      encoding: 'utf8',
      mode: 0o600,
    });
    renameSync(temporaryPath, statePath);
  } finally {
    rmSync(temporaryPath, { force: true });
  }
}

export function resolveRestoredWindowBounds(
  bounds: WindowBounds | null,
  displayWorkAreas: readonly WindowBounds[],
  minimumSize: Readonly<{ width: number; height: number }>,
): Partial<WindowBounds> {
  if (bounds == null) {
    return {};
  }

  const restoredBounds = {
    ...bounds,
    width: Math.max(bounds.width, minimumSize.width),
    height: Math.max(bounds.height, minimumSize.height),
  };
  const positionIsVisible = displayWorkAreas.some((workArea) => rectanglesIntersect(restoredBounds, workArea));
  return positionIsVisible
    ? restoredBounds
    : { width: restoredBounds.width, height: restoredBounds.height };
}

function parseWindowState(value: unknown): PersistedWindowState | null {
  if (!isRecord(value) || value.schemaVersion !== 1 || !isRecord(value.bounds)) {
    return null;
  }

  const bounds = value.bounds;
  if (
    !isSafeInteger(bounds.x)
    || !isSafeInteger(bounds.y)
    || !isPositiveSafeInteger(bounds.width)
    || !isPositiveSafeInteger(bounds.height)
  ) {
    return null;
  }

  return {
    schemaVersion: 1,
    bounds: {
      x: bounds.x,
      y: bounds.y,
      width: bounds.width,
      height: bounds.height,
    },
  };
}

function rectanglesIntersect(left: WindowBounds, right: WindowBounds): boolean {
  return left.x < right.x + right.width
    && left.x + left.width > right.x
    && left.y < right.y + right.height
    && left.y + left.height > right.y;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value != null && !Array.isArray(value);
}

function isSafeInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value);
}

function isPositiveSafeInteger(value: unknown): value is number {
  return isSafeInteger(value) && value > 0;
}
