import { describe, expect, it } from 'vitest';
import {
  APPLICATION_MENU_COMMANDS,
  APPLICATION_MENU_UPDATE_FREQUENCIES,
  updateCheckMenuLabel,
} from './applicationMenu';
import type { ApplicationMenuState } from './protocol';

describe('application menu definitions', () => {
  it('keeps command labels and accelerators stable', () => {
    expect(Object.values(APPLICATION_MENU_COMMANDS)).toEqual([
      { command: 'new-pdf', label: 'New from Template...', accelerator: 'CommandOrControl+N' },
      { command: 'open-pdf', label: 'Open...', accelerator: 'CommandOrControl+O' },
      { command: 'save', label: 'Save', accelerator: 'CommandOrControl+S' },
      { command: 'save-as', label: 'Save As...', accelerator: 'CommandOrControl+Shift+S' },
      { command: 'save-document-as-template', label: 'Save Document as Template...' },
      { command: 'undo', label: 'Undo', accelerator: 'CommandOrControl+Z' },
      { command: 'redo', label: 'Redo', accelerator: 'CommandOrControl+Shift+Z' },
      { command: 'cut', label: 'Cut', accelerator: 'CommandOrControl+X' },
      { command: 'copy', label: 'Copy', accelerator: 'CommandOrControl+C' },
      { command: 'paste', label: 'Paste', accelerator: 'CommandOrControl+V' },
      { command: 'select-all', label: 'Select All', accelerator: 'CommandOrControl+A' },
      { command: 'set-default-pdf-app', label: 'Set as Default PDF App...' },
      { command: 'check-for-updates', label: 'Check for Updates...' },
      { command: 'open-release-page', label: 'View Releases...' },
    ]);
  });

  it('keeps update frequencies ordered and uniquely labelled', () => {
    expect(APPLICATION_MENU_UPDATE_FREQUENCIES.map(({ value }) => value)).toEqual([
      'never', 'startup', 'hourly', 'sixHours', 'twelveHours', 'daily', 'weekly', 'monthly',
    ]);
    expect(new Set(APPLICATION_MENU_UPDATE_FREQUENCIES.map(({ label }) => label)).size)
      .toBe(APPLICATION_MENU_UPDATE_FREQUENCIES.length);
  });

  it('maps updater phases and rounds download progress in the menu label', () => {
    expect(updateCheckMenuLabel('idle', null)).toBe('Check for Updates...');
    expect(updateCheckMenuLabel('checking', null)).toBe('Checking for Updates...');
    expect(updateCheckMenuLabel('available', null)).toBe('Downloading Update...');
    expect(updateCheckMenuLabel('downloading', 42.5)).toBe('Downloading Update (43%)');
    expect(updateCheckMenuLabel('downloaded', 100)).toBe('Update Ready...');
  });

  it('accepts the complete renderer-to-main menu state contract', () => {
    const state: ApplicationMenuState = {
      canSave: false,
      canUndo: false,
      canRedo: false,
      canCut: false,
      canCopy: false,
      canPaste: false,
      canSelectAll: false,
      updateStatus: null,
      menuBarVisible: true,
    };
    expect(state).toMatchObject({ canSave: false, canUndo: false, canRedo: false, updateStatus: null, menuBarVisible: true });
  });
});
