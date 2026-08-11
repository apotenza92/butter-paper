import type { BrowserWindowConstructorOptions } from 'electron';

export function getWindowTitleBarOptions(): Pick<
  BrowserWindowConstructorOptions,
  'titleBarOverlay' | 'titleBarStyle'
> {
  return {
    titleBarStyle: 'hidden',
    titleBarOverlay: {
      color: '#00000000',
    },
  };
}
