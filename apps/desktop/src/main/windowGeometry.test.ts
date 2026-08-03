import { describe, expect, it } from 'vitest';
import { MAIN_WINDOW_GEOMETRY } from './windowGeometry';

describe('main window geometry', () => {
  it('opens at a compact desktop size while retaining a usable minimum', () => {
    expect(MAIN_WINDOW_GEOMETRY).toEqual({
      width: 1200,
      height: 800,
      minWidth: 900,
      minHeight: 600,
    });
    expect(MAIN_WINDOW_GEOMETRY.width).toBeGreaterThan(MAIN_WINDOW_GEOMETRY.minWidth);
    expect(MAIN_WINDOW_GEOMETRY.height).toBeGreaterThan(MAIN_WINDOW_GEOMETRY.minHeight);
    expect(MAIN_WINDOW_GEOMETRY.width / MAIN_WINDOW_GEOMETRY.height).toBe(1.5);
    expect(MAIN_WINDOW_GEOMETRY.minWidth / MAIN_WINDOW_GEOMETRY.minHeight).toBe(1.5);
  });
});
