import { afterEach, describe, expect, it, vi } from 'vitest';
import { isToolShortcutBlockedTarget, parseToolShortcut, resolveToolShortcut } from './toolShortcuts';

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

  constructor(tagName: string, options: { contentEditable?: boolean; overlayRole?: boolean } = {}) {
    this.tagName = tagName.toUpperCase();
    this.isContentEditable = options.contentEditable ?? false;
    this.overlayRole = options.overlayRole ?? false;
  }

  closest(): TestHTMLElement | null {
    return this.overlayRole ? this : null;
  }
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('tool shortcuts', () => {
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
