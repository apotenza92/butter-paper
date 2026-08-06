export interface ToolShortcut {
  readonly key: string;
  readonly shift: boolean;
  readonly alt: boolean;
}

export interface ToolShortcutEntry<TTool> {
  readonly tool: TTool;
  readonly shortcut: ToolShortcut;
}

export interface ToolShortcutEvent {
  readonly key: string;
  readonly shiftKey: boolean;
  readonly altKey: boolean;
  readonly metaKey: boolean;
  readonly ctrlKey: boolean;
  readonly blockedByFocus: boolean;
}

export function isEditableShortcutTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target.isContentEditable) {
    return true;
  }
  const tagName = target.tagName.toLowerCase();
  return tagName === 'input' || tagName === 'textarea' || tagName === 'select';
}

export function isToolShortcutBlockedTarget(target: EventTarget | null): boolean {
  if (isEditableShortcutTarget(target)) {
    return true;
  }
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  return Boolean(target.closest([
    '[role="combobox"]',
    '[role="dialog"]',
    '[role="grid"]',
    '[role="listbox"]',
    '[role="menu"]',
    '[role="tree"]',
  ].join(',')));
}

export function parseToolShortcut(shortcut: string): ToolShortcut {
  const parts = shortcut.split('+').map((part) => part.trim()).filter(Boolean);
  const key = parts.at(-1) ?? shortcut;
  const modifiers = parts.slice(0, -1).map((part) => part.toLowerCase());
  return {
    key: normalizeShortcutKey(key),
    shift: modifiers.includes('shift'),
    alt: modifiers.includes('alt') || modifiers.includes('option'),
  };
}

export function normalizeShortcutKey(key: string): string {
  if (key === ' ' || key.toLowerCase() === 'spacebar') {
    return 'space';
  }
  return key.toLowerCase();
}

export function resolveToolShortcut<TTool>(
  shortcuts: readonly ToolShortcutEntry<TTool>[],
  event: ToolShortcutEvent,
): TTool | null {
  if (event.metaKey || event.ctrlKey || event.blockedByFocus) {
    return null;
  }

  const key = normalizeShortcutKey(event.key);
  return shortcuts.find((entry) => (
    entry.shortcut.key === key
    && entry.shortcut.shift === event.shiftKey
    && entry.shortcut.alt === event.altKey
  ))?.tool ?? null;
}
