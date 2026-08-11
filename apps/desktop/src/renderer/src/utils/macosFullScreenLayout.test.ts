import { describe, expect, it } from 'vitest';
import { resolveMacosFullScreenLayout } from './macosFullScreenLayout';

describe('macOS fullscreen layout', () => {
  it('hides the custom title bar but preserves the requested app menu bar', () => {
    expect(resolveMacosFullScreenLayout({ platform: 'MacIntel', fullScreen: true, menuBarVisible: true })).toEqual({
      showWindowTitleBar: false,
      showAppMenuBar: true,
    });
  });

  it('keeps the custom title bar on other platforms', () => {
    expect(resolveMacosFullScreenLayout({ platform: 'Win32', fullScreen: true, menuBarVisible: false })).toEqual({
      showWindowTitleBar: true,
      showAppMenuBar: false,
    });
  });

});
