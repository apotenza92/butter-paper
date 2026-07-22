import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { ensureFixturesGenerated } from './ensure-fixtures.js';
import { fixtureManifestPath, generatedFixturesDir } from './paths.js';

export interface FixtureManifestEntry {
  name: string;
  file: string;
  pageCount: number;
  pages: Array<{
    width: number;
    height: number;
    rotation: number;
  }>;
}

export interface FixtureManifest {
  generatedAt: string;
  fixtures: FixtureManifestEntry[];
}

export async function loadFixtureManifest(): Promise<FixtureManifest> {
  await ensureFixturesGenerated();
  const raw = readFileSync(fixtureManifestPath, 'utf8');
  return JSON.parse(raw) as FixtureManifest;
}

export async function getFixturePath(name: string): Promise<string> {
  const manifest = await loadFixtureManifest();
  const entry = manifest.fixtures.find((fixture) => fixture.name === name);

  if (!entry) {
    throw new Error(`Unknown fixture: ${name}`);
  }

  return join(generatedFixturesDir, entry.file);
}

export async function fixtureExists(name: string): Promise<boolean> {
  const manifest = await loadFixtureManifest();
  const entry = manifest.fixtures.find((fixture) => fixture.name === name);
  return entry ? existsSync(join(generatedFixturesDir, entry.file)) : false;
}
