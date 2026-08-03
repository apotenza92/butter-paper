import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  loadWindowBounds,
  resolveRestoredWindowBounds,
  WINDOW_STATE_FILE_NAME,
  writeWindowBoundsAtomic,
} from './windowState';

const temporaryDirectories: string[] = [];

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

describe('window state persistence', () => {
  it('returns no saved bounds on first launch', () => {
    const statePath = createStatePath();

    expect(loadWindowBounds(statePath)).toBeNull();
  });

  it('atomically saves and reloads changed size and position', () => {
    const statePath = createStatePath();
    const initialBounds = { x: 20, y: 30, width: 1200, height: 800 };
    const changedBounds = { x: 140, y: 90, width: 1440, height: 960 };

    writeWindowBoundsAtomic(statePath, initialBounds);
    writeWindowBoundsAtomic(statePath, changedBounds);

    expect(loadWindowBounds(statePath)).toEqual(changedBounds);
    expect(JSON.parse(readFileSync(statePath, 'utf8'))).toEqual({
      schemaVersion: 1,
      bounds: changedBounds,
    });
    expect(readdirSync(dirname(statePath)).filter((name) => name.endsWith('.tmp'))).toEqual([]);
  });

  it('ignores corrupt or invalid saved state', () => {
    const statePath = createStatePath();
    writeFileSync(statePath, '{broken json', 'utf8');
    expect(loadWindowBounds(statePath)).toBeNull();

    writeFileSync(statePath, JSON.stringify({
      schemaVersion: 1,
      bounds: { x: 10, y: 20, width: -1, height: 700 },
    }), 'utf8');
    expect(loadWindowBounds(statePath)).toBeNull();
  });

  it('restores saved bounds when their position remains on a display', () => {
    const bounds = { x: 1500, y: 100, width: 1200, height: 800 };

    expect(resolveRestoredWindowBounds(bounds, [
      { x: 0, y: 0, width: 1440, height: 900 },
      { x: 1440, y: 0, width: 1920, height: 1080 },
    ], { width: 900, height: 600 })).toEqual(bounds);
  });

  it('keeps the saved size but lets the OS place a window whose display was removed', () => {
    expect(resolveRestoredWindowBounds(
      { x: 1600, y: 100, width: 1200, height: 800 },
      [{ x: 0, y: 0, width: 1440, height: 900 }],
      { width: 900, height: 600 },
    )).toEqual({ width: 1200, height: 800 });
  });

  it('enforces the current minimum size on older saved bounds', () => {
    expect(resolveRestoredWindowBounds(
      { x: 100, y: 100, width: 800, height: 500 },
      [{ x: 0, y: 0, width: 1440, height: 900 }],
      { width: 900, height: 600 },
    )).toEqual({ x: 100, y: 100, width: 900, height: 600 });
  });
});

function createStatePath(): string {
  const directory = mkdtempSync(join(tmpdir(), 'butter-paper-window-state-'));
  temporaryDirectories.push(directory);
  return join(directory, WINDOW_STATE_FILE_NAME);
}
