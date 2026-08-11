export function resolveMenuBarVisibility(platform: string, storedValue: string | null): boolean {
  if (!platform.toLowerCase().includes('mac')) {
    return true;
  }
  return storedValue !== '0';
}
