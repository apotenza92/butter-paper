import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const moduleDir = dirname(fileURLToPath(import.meta.url));

export const fixturesRoot = resolve(moduleDir);
export const generatedFixturesDir = join(fixturesRoot, 'generated');
export const fixtureManifestPath = join(generatedFixturesDir, 'manifest.json');
