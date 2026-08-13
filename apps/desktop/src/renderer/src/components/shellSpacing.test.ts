import { describe, expect, it } from 'vitest';
import {
  PRIMARY_BAND_HEIGHT,
  SHELL_BAND_BORDER_BOTTOM,
  SHELL_HORIZONTAL_SEPARATOR,
  SHELL_PANEL_BORDER_LEFT,
  SHELL_PANEL_BORDER_RIGHT,
} from './shellSpacing';

describe('shell border contracts', () => {
  it('uses one border token for every panel edge and band divider', () => {
    expect(SHELL_PANEL_BORDER_LEFT.split(' ')).toEqual(['border-l', 'border-border']);
    expect(SHELL_PANEL_BORDER_RIGHT.split(' ')).toEqual(['border-r', 'border-border']);
    expect(SHELL_BAND_BORDER_BOTTOM.split(' ')).toEqual(['border-b', 'border-border']);
  });

  it('keeps toolbar-aligned bands at one shared height', () => {
    expect(PRIMARY_BAND_HEIGHT).toBe('h-12');
  });

  it('requires horizontal group separators to span their container', () => {
    expect(SHELL_HORIZONTAL_SEPARATOR).toBe('w-full');
  });
});
