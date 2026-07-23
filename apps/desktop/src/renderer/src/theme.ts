import type { ThemeMode } from '../../shared/protocol';

export function applyThemeMode(mode: ThemeMode): void {
  document.documentElement.dataset.theme = mode;
  document.documentElement.classList.toggle('dark', mode === 'dark');
  document.documentElement.style.colorScheme = mode;
}

export function getAppliedThemeMode(): ThemeMode {
  return document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
}

export async function bootstrapThemeMode(): Promise<ThemeMode> {
  try {
    const snapshot = await window.butterPaper.theme.getSnapshot();
    applyThemeMode(snapshot.mode);
    return snapshot.mode;
  } catch {
    const fallbackMode = getAppliedThemeMode();
    applyThemeMode(fallbackMode);
    return fallbackMode;
  }
}

export function subscribeToThemeMode(listener: (mode: ThemeMode) => void): () => void {
  return window.butterPaper.theme.subscribe((snapshot) => {
    applyThemeMode(snapshot.mode);
    listener(snapshot.mode);
  });
}
