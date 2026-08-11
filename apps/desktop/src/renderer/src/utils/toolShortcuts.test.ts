import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  isToolShortcutBlockedTarget,
  parseToolShortcut,
  resolvePdfZoomShortcut,
  resolveToolShortcut,
  shouldResetToolOnEscape,
} from './toolShortcuts';

const shortcuts = [
  { tool: 'select', shortcut: parseToolShortcut('V') },
  { tool: 'rectangle', shortcut: parseToolShortcut('R') },
  { tool: 'length', shortcut: parseToolShortcut('Shift+Alt+L') },
] as const;

function shortcutEvent(overrides: Partial<Parameters<typeof resolveToolShortcut>[1]> = {}) {
  return {
    key: 'r',
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ctrlKey: false,
    blockedByFocus: false,
    ...overrides,
  };
}

class TestHTMLElement {
  readonly tagName: string;
  readonly isContentEditable: boolean;
  readonly overlayRole: boolean;
  readonly toolPopup: boolean;

  constructor(tagName: string, options: { contentEditable?: boolean; overlayRole?: boolean; toolPopup?: boolean } = {}) {
    this.tagName = tagName.toUpperCase();
    this.isContentEditable = options.contentEditable ?? false;
    this.overlayRole = options.overlayRole ?? false;
    this.toolPopup = options.toolPopup ?? false;
  }

  closest(selector: string): TestHTMLElement | null {
    if (selector.includes('[data-slot="popover-content"]')) {
      return this.toolPopup ? this : null;
    }
    return this.overlayRole ? this : null;
  }
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('tool shortcuts', () => {
  it('resolves PDF zoom only without Shift', () => {
    expect(resolvePdfZoomShortcut(shortcutEvent({ key: '=', metaKey: true }), true)).toBe('zoom-in');
    expect(resolvePdfZoomShortcut(shortcutEvent({ key: '-', metaKey: true }), true)).toBe('zoom-out');
    expect(resolvePdfZoomShortcut(shortcutEvent({ key: '0', metaKey: true }), true)).toBe('zoom-reset');
    expect(resolvePdfZoomShortcut(shortcutEvent({ key: '=', metaKey: true, shiftKey: true }), true)).toBeNull();
    expect(resolvePdfZoomShortcut(shortcutEvent({ key: '-', metaKey: true, shiftKey: true }), true)).toBeNull();
    expect(resolvePdfZoomShortcut(shortcutEvent({ key: '0', metaKey: true, shiftKey: true }), true)).toBeNull();
    expect(resolvePdfZoomShortcut(shortcutEvent({ key: '=', ctrlKey: true }), false)).toBe('zoom-in');
    expect(resolvePdfZoomShortcut(shortcutEvent({ key: '-', ctrlKey: true }), false)).toBe('zoom-out');
    expect(resolvePdfZoomShortcut(shortcutEvent({ key: '0', ctrlKey: true }), false)).toBe('zoom-reset');
  });

  it('resolves plain and modified tool shortcuts', () => {
    expect(resolveToolShortcut(shortcuts, shortcutEvent())).toBe('rectangle');
    expect(resolveToolShortcut(shortcuts, shortcutEvent({ key: 'L', shiftKey: true, altKey: true }))).toBe('length');
  });

  it('continues to resolve shortcuts when a non-editing application control has focus', () => {
    vi.stubGlobal('HTMLElement', TestHTMLElement);
    const toolbarButton = new TestHTMLElement('button') as unknown as EventTarget;
    const blockedByFocus = isToolShortcutBlockedTarget(toolbarButton);

    expect(blockedByFocus).toBe(false);
    expect(resolveToolShortcut(shortcuts, shortcutEvent({ blockedByFocus }))).toBe('rectangle');
  });

  it('resolves shortcuts from tool popup controls but not their editing fields', () => {
    vi.stubGlobal('HTMLElement', TestHTMLElement);
    const popupButton = new TestHTMLElement('button', { overlayRole: true, toolPopup: true }) as unknown as EventTarget;
    const popupInput = new TestHTMLElement('input', { overlayRole: true, toolPopup: true }) as unknown as EventTarget;

    expect(isToolShortcutBlockedTarget(popupButton)).toBe(false);
    expect(resolveToolShortcut(shortcuts, shortcutEvent({
      blockedByFocus: isToolShortcutBlockedTarget(popupButton),
    }))).toBe('rectangle');
    expect(shouldResetToolOnEscape(popupButton)).toBe(false);
    expect(isToolShortcutBlockedTarget(popupInput)).toBe(true);
  });

  it('does not steal shortcuts from editing, overlays, or system modifier chords', () => {
    vi.stubGlobal('HTMLElement', TestHTMLElement);
    const textInput = new TestHTMLElement('input') as unknown as EventTarget;
    const dialogButton = new TestHTMLElement('button', { overlayRole: true }) as unknown as EventTarget;

    expect(isToolShortcutBlockedTarget(textInput)).toBe(true);
    expect(isToolShortcutBlockedTarget(dialogButton)).toBe(true);
    expect(resolveToolShortcut(shortcuts, shortcutEvent({ blockedByFocus: true }))).toBeNull();
    expect(resolveToolShortcut(shortcuts, shortcutEvent({ metaKey: true }))).toBeNull();
    expect(resolveToolShortcut(shortcuts, shortcutEvent({ ctrlKey: true }))).toBeNull();
  });
});
