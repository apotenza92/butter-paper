export function resolveMenuBarVisibility(platform: string, storedValue: string | null): boolean {
  if (!canHideMenuBar(platform)) {
    return true;
  }
  return storedValue !== '0';
}

export function canHideMenuBar(platform: string): boolean {
  return platform.toLowerCase().includes('mac');
}
