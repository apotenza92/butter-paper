import { describe, expect, it } from 'vitest';
import { getWindowTitleBarOptions } from './windowTitleBar';

describe('window title bar', () => {
  it('uses transparent native controls at the native platform height', () => {
    const options = getWindowTitleBarOptions();

    expect(options).toEqual({
      titleBarStyle: 'hidden',
      titleBarOverlay: {
        color: '#00000000',
      },
    });
    expect(options.titleBarOverlay).not.toHaveProperty('height');
  });
});
