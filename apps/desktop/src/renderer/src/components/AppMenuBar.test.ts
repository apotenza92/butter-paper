import { describe, expect, it } from 'vitest';
import { APP_MENU_CONTENT_CLASS_NAME } from './AppMenuBar';

describe('application menu sizing', () => {
  it('sizes every menu to its longest item without wrapping labels', () => {
    expect(APP_MENU_CONTENT_CLASS_NAME).toContain('w-max');
    expect(APP_MENU_CONTENT_CLASS_NAME).toContain('whitespace-nowrap');
  });
});
