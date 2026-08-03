import { describe, expect, it, vi } from 'vitest';
import { loadRailExpandOnHover, saveRailExpandOnHover, shouldExpandRail } from './railSettings';

describe('rail label settings', () => {
  it('enables hover expansion by default and remembers an explicit preference per rail', () => {
    const values = new Map<string, string>();
    const storage = {
      getItem: vi.fn((key: string) => values.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => { values.set(key, value); }),
    };

    expect(loadRailExpandOnHover(storage, 'left')).toBe(true);
    saveRailExpandOnHover(storage, 'left', false);
    expect(loadRailExpandOnHover(storage, 'left')).toBe(false);
    expect(loadRailExpandOnHover(storage, 'right')).toBe(true);
  });

  it('expands only single-column rails during hover or while their settings stay open', () => {
    expect(shouldExpandRail({ enabled: true, hovered: true, settingsOpen: false, singleColumn: true })).toBe(true);
    expect(shouldExpandRail({ enabled: true, hovered: false, settingsOpen: true, singleColumn: true })).toBe(true);
    expect(shouldExpandRail({ enabled: true, hovered: true, settingsOpen: false, singleColumn: false })).toBe(false);
    expect(shouldExpandRail({ enabled: false, hovered: true, settingsOpen: true, singleColumn: true })).toBe(false);
  });
});
