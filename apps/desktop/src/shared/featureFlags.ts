/** Keep the unfinished CAD View out of releases unless a developer opts in. */
export function resolveCadViewEnabled(value: string | undefined): boolean {
  return value === '1';
}
