import electron from 'electron';
import { resolve } from 'node:path';

const { BrowserWindow } = electron;

export function isTestModeEnabled(): boolean {
  return process.env.BP_TEST_MODE === '1';
}

export function resolveFixturePath(name: string): string {
  const baseDir = process.env.BP_TEST_FIXTURE_DIR?.trim() || resolve(process.cwd(), 'tests/fixtures/generated');
  return resolve(baseDir, name);
}

export function getFocusedWindowState() {
  const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
  if (!window) {
    return null;
  }

  const bounds = window.getBounds();
  return {
    bounds,
    focused: window.isFocused(),
    maximized: window.isMaximized(),
    title: window.getTitle(),
  };
}

export function setFocusedWindowBounds(nextBounds: Partial<{ x: number; y: number; width: number; height: number }>) {
  const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
  if (!window) {
    return null;
  }

  const bounds = window.getBounds();
  window.setBounds({
    x: nextBounds.x ?? bounds.x,
    y: nextBounds.y ?? bounds.y,
    width: nextBounds.width ?? bounds.width,
    height: nextBounds.height ?? bounds.height,
  });

  return getFocusedWindowState();
}
