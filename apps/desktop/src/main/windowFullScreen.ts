import type { BrowserWindow } from 'electron';

type FullScreenWindow = Pick<BrowserWindow, 'isDestroyed' | 'webContents'> & {
  on(event: 'enter-full-screen' | 'leave-full-screen', listener: () => void): unknown;
};

export function registerWindowFullScreenNotifications(
  window: FullScreenWindow,
  channel: string,
): void {
  const notify = (fullScreen: boolean) => {
    if (!window.isDestroyed() && !window.webContents.isDestroyed()) {
      window.webContents.send(channel, fullScreen);
    }
  };

  window.on('enter-full-screen', () => notify(true));
  window.on('leave-full-screen', () => notify(false));
}
