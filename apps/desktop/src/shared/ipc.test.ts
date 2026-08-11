import { describe, expect, it } from 'vitest';
import { ipcChannels } from './ipc';

describe('application menu IPC contract', () => {
  it('keeps every IPC channel name distinct', () => {
    const channels = Object.values(ipcChannels);
    expect(new Set(channels).size).toBe(channels.length);
  });

  it('keeps command, visibility, fullscreen, state, and close-tab channels distinct', () => {
    const channels = [
      ipcChannels.applicationMenuCommand,
      ipcChannels.applicationMenuBarVisibilityChanged,
      ipcChannels.applicationGetWindowFullScreen,
      ipcChannels.applicationWindowFullScreenChanged,
      ipcChannels.applicationSetMenuState,
      ipcChannels.applicationCloseTabRequested,
    ];
    expect(new Set(channels).size).toBe(channels.length);
    expect(channels).toEqual([
      'application:menu-command',
      'application:menu-bar-visibility-changed',
      'application:get-window-fullscreen',
      'application:window-fullscreen-changed',
      'application:set-menu-state',
      'application:close-tab-requested',
    ]);
  });

  it('uses separate one-time phone signature lifecycle channels', () => {
    expect([
      ipcChannels.signaturePhoneStart,
      ipcChannels.signaturePhonePoll,
      ipcChannels.signaturePhoneStop,
    ]).toEqual([
      'signature-phone:start',
      'signature-phone:poll',
      'signature-phone:stop',
    ]);
  });

  it('uses separate recent signature storage channels', () => {
    expect([
      ipcChannels.signatureRecentList,
      ipcChannels.signatureRecentRemember,
      ipcChannels.signatureRecentRemove,
      ipcChannels.signatureRecentClear,
    ]).toEqual([
      'signature-recent:list',
      'signature-recent:remember',
      'signature-recent:remove',
      'signature-recent:clear',
    ]);
  });
});
