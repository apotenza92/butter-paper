import { describe, expect, it } from 'vitest';
import { APP_MENU_CONTENT_CLASS_NAME, APP_MENU_KEYS } from './AppMenuBar';
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
});

describe('application menu state rendering', () => {
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
