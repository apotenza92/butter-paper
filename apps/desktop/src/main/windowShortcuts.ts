export type ApplicationShortcutAction =
  | 'close-tab'
  | 'close-window'
  | 'quit-app'
  | 'zoom-in'
  | 'zoom-out'
  | 'zoom-reset';

export interface BeforeInputShortcut {
  readonly type: string;
  readonly key: string;
  readonly meta?: boolean;
  readonly control?: boolean;
  readonly alt?: boolean;
  readonly shift?: boolean;
  readonly isAutoRepeat?: boolean;
}

export function resolveApplicationShortcutAction(
  input: BeforeInputShortcut,
  platform: NodeJS.Platform,
): ApplicationShortcutAction | null {
  if (input.type !== 'keyDown' || input.isAutoRepeat) {
    return null;
  }

  const key = input.key.toLowerCase();
  const modifierPressed = platform === 'darwin' ? input.meta === true : input.control === true;
  const secondaryModifierPressed = platform === 'darwin' ? input.control === true : input.meta === true;
  if (!modifierPressed || secondaryModifierPressed || input.alt === true) {
    return null;
  }

  if (input.shift && (key === '+' || key === '=')) {
    return 'zoom-in';
  }

  if (input.shift && (key === '-' || key === '_')) {
    return 'zoom-out';
  }

  if (input.shift && key === '0') {
    return 'zoom-reset';
  }

  if (key === 'w') {
    return input.shift ? 'close-window' : 'close-tab';
  }

  if (platform === 'darwin' && key === 'q' && !input.shift) {
    return 'quit-app';
  }

  return null;
}
