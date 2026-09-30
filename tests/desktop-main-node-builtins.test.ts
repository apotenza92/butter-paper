import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

// Vite replaces any Node built-in missing from the main-process externals with
// a browser stub, which crashes the packaged app at import time.
describe('desktop main-process build', () => {
  it('keeps every Node built-in imported by the main process external', () => {
    const config = readFileSync('apps/desktop/vite.main.config.ts', 'utf8');
    const directory = 'apps/desktop/src/main';
    const imported = new Set<string>();
    for (const name of readdirSync(directory)) {
      if (!name.endsWith('.ts') || name.endsWith('.test.ts')) continue;
      const source = readFileSync(join(directory, name), 'utf8');
      for (const match of source.matchAll(/^import[^;]*? from '(node:[a-z_/]+)';/gms)) imported.add(match[1]);
    }
    expect(imported.size).toBeGreaterThan(0);
    for (const builtin of imported) {
      expect(config, `${builtin} must be listed in vite.main.config.ts externals`).toContain(`'${builtin}'`);
    }
  });
});
