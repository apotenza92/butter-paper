import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fixtureManifestPath } from './paths.js';

const moduleDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(moduleDir, '../..');
const generatorPath = resolve(repoRoot, 'scripts/generate-fixtures.mjs');

export async function ensureFixturesGenerated() {
  if (existsSync(fixtureManifestPath)) {
    return;
  }

  const result = spawnSync(process.execPath, [generatorPath], {
    cwd: repoRoot,
    stdio: 'inherit',
    env: {
      ...process.env,
      BP_FIXTURE_FORCE: '1',
    },
  });

  if (result.status !== 0) {
    throw new Error('Failed to generate PDF fixtures');
  }
}
