import { describe, expect, it } from 'vitest';
import { resolveApplicationShortcutAction } from './windowShortcuts';

describe('application window shortcuts', () => {
  it('closes the active tab for Cmd/Ctrl+W', () => {
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'W', meta: true }, 'darwin')).toBe('close-tab');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', control: true }, 'win32')).toBe('close-tab');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', control: true }, 'linux')).toBe('close-tab');
  });

  it('closes only the window for Cmd/Ctrl+Shift+W', () => {
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', meta: true, shift: true }, 'darwin')).toBe('close-window');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', control: true, shift: true }, 'win32')).toBe('close-window');
  });

  it('quits the app for Cmd+Q on macOS', () => {
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'q', meta: true }, 'darwin')).toBe('quit-app');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'q', control: true }, 'win32')).toBeNull();
  });

  it('zooms and resets the whole app with Shift-modified shortcuts', () => {
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '+', meta: true, shift: true }, 'darwin')).toBe('zoom-in');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '=', meta: true, shift: true }, 'darwin')).toBe('zoom-in');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '_', meta: true, shift: true }, 'darwin')).toBe('zoom-out');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '-', meta: true, shift: true }, 'darwin')).toBe('zoom-out');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '+', control: true, shift: true }, 'win32')).toBe('zoom-in');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '-', control: true, shift: true }, 'linux')).toBe('zoom-out');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '0', meta: true, shift: true }, 'darwin')).toBe('zoom-reset');
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '0', control: true, shift: true }, 'win32')).toBe('zoom-reset');
  });

  it('does not replace document zoom shortcuts without Shift', () => {
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '=', meta: true }, 'darwin')).toBeNull();
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '-', meta: true }, 'darwin')).toBeNull();
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: '0', meta: true }, 'darwin')).toBeNull();
  });

  it('does not repeat destructive actions for auto-repeat or unrelated modifiers', () => {
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', meta: true, isAutoRepeat: true }, 'darwin')).toBeNull();
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', meta: true }, 'win32')).toBeNull();
    expect(resolveApplicationShortcutAction({ type: 'keyUp', key: 'w', meta: true }, 'darwin')).toBeNull();
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', meta: true, alt: true }, 'darwin')).toBeNull();
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', meta: true, control: true }, 'darwin')).toBeNull();
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', control: true, meta: true }, 'win32')).toBeNull();
    expect(resolveApplicationShortcutAction({ type: 'keyDown', key: 'w', control: true, shift: true, alt: true }, 'linux')).toBeNull();
  });
});
