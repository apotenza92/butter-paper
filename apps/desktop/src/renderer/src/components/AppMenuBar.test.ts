import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  APP_MENU_CONTENT_CLASS_NAME,
  APP_MENU_KEYS,
  APP_MENU_SHORTCUT_CLASS_NAME,
  formatMenuAccelerator,
} from './AppMenuBar';
import {
  APPLICATION_MENU_BAR_VISIBILITY_LABEL,
  APPLICATION_MENU_COMMANDS,
  updateCheckMenuLabel,
} from '../../../shared/applicationMenu';

describe('application menu sizing', () => {
  it('sizes every menu to its longest item without wrapping labels', () => {
    expect(APP_MENU_CONTENT_CLASS_NAME).toContain('w-max');
    expect(APP_MENU_CONTENT_CLASS_NAME).toContain('whitespace-nowrap');
  });

  it('reserves a fixed right-hand column for keyboard shortcuts', () => {
    expect(APP_MENU_SHORTCUT_CLASS_NAME).toContain('ml-auto');
    expect(APP_MENU_SHORTCUT_CLASS_NAME).toContain('min-w-24');
    expect(APP_MENU_SHORTCUT_CLASS_NAME).toContain('text-right');
  });

  it('formats shared Electron accelerators for the current menu style', () => {
    expect(formatMenuAccelerator('CommandOrControl+S', true)).toBe('⌘S');
    expect(formatMenuAccelerator('CommandOrControl+Shift+S', true)).toBe('⇧⌘S');
    expect(formatMenuAccelerator('CommandOrControl+S', false)).toBe('Ctrl+S');
    expect(formatMenuAccelerator('CommandOrControl+Shift+S', false)).toBe('Ctrl+Shift+S');
  });
});

describe('application menu state rendering', () => {
  it('only renders the menu bar visibility control when the platform allows it', () => {
    const source = readFileSync('apps/desktop/src/renderer/src/components/AppMenuBar.tsx', 'utf8');
    expect(source).toContain('{showMenuBarVisibilityOption ? (');
    expect(source).toContain('<MenubarCheckboxItem');
  });

  it('keeps renderer command labels sourced from the shared menu model', () => {
    expect(APPLICATION_MENU_COMMANDS.save.label).toBe('Save');
    expect(APPLICATION_MENU_COMMANDS.saveAs.label).toBe('Save As...');
    expect(APP_MENU_KEYS).toEqual(['butter-paper', 'file', 'edit', 'view']);
    expect(APPLICATION_MENU_COMMANDS.selectAll.label).toBe('Select All');
    expect(APPLICATION_MENU_BAR_VISIBILITY_LABEL).toBe('Show Menu Bar in App Windows');
  });

  it('renders updater labels from the shared state formatter', () => {
    expect(updateCheckMenuLabel('downloading', 42.5)).toBe('Downloading Update (43%)');
  });
});
