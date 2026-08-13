import { describe, expect, it } from 'vitest';
import { canHideMenuBar, resolveMenuBarVisibility } from './menuBarVisibility';

describe('menu bar visibility', () => {
  it('shows the menu bar by default on macOS', () => {
    expect(resolveMenuBarVisibility('MacIntel', null)).toBe(true);
  });

  it('preserves an explicit macOS choice to hide the menu bar', () => {
    expect(resolveMenuBarVisibility('MacIntel', '0')).toBe(false);
  });

  it('keeps the menu bar visible on other platforms', () => {
    expect(resolveMenuBarVisibility('Win32', '0')).toBe(true);
    expect(resolveMenuBarVisibility('Linux x86_64', '0')).toBe(true);
  });

  it('only offers menu bar visibility as a macOS preference', () => {
    expect(canHideMenuBar('MacIntel')).toBe(true);
    expect(canHideMenuBar('Win32')).toBe(false);
    expect(canHideMenuBar('Linux x86_64')).toBe(false);
  });
});
