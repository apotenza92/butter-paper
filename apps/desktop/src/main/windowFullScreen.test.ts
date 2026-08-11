import { describe, expect, it, vi } from 'vitest';
import { registerWindowFullScreenNotifications } from './windowFullScreen';

describe('window fullscreen notifications', () => {
  it('reports completed native fullscreen transitions', () => {
    const listeners = new Map<string, () => void>();
    const send = vi.fn();
    const window = {
      isDestroyed: () => false,
      webContents: { isDestroyed: () => false, send },
      on: (event: string, listener: () => void) => listeners.set(event, listener),
    };

    registerWindowFullScreenNotifications(window as never, 'window:fullscreen-changed');
    listeners.get('enter-full-screen')?.();
    listeners.get('leave-full-screen')?.();

    expect(send.mock.calls).toEqual([
      ['window:fullscreen-changed', true],
      ['window:fullscreen-changed', false],
    ]);
  });

  it('does not send after the renderer is destroyed', () => {
    const listeners = new Map<string, () => void>();
    const send = vi.fn();
    const window = {
      isDestroyed: () => false,
      webContents: { isDestroyed: () => true, send },
      on: (event: string, listener: () => void) => listeners.set(event, listener),
    };

    registerWindowFullScreenNotifications(window as never, 'window:fullscreen-changed');
    listeners.get('enter-full-screen')?.();

    expect(send).not.toHaveBeenCalled();
  });
});
