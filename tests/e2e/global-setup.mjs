import { existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const moduleDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(moduleDir, '../..');
const fixtureManifestPath = resolve(repoRoot, 'tests/fixtures/generated/manifest.json');
const desktopEntryPoint = resolve(repoRoot, 'apps/desktop/.vite/build/main.js');
const rendererEntryPoint = resolve(repoRoot, 'apps/desktop/.vite/renderer/main_window/index.html');
const developmentProvenance = resolve(repoRoot, 'test-results/desktop-dev-provenance.json');

export default async function globalSetup() {
  ensurePnpmShim();

  if (!existsSync(fixtureManifestPath)) {
    execFileSync(process.execPath, [resolve(repoRoot, 'scripts/generate-fixtures.mjs')], {
      cwd: repoRoot,
      stdio: 'inherit',
      env: process.env,
    });
  }

  const pnpm = resolvePnpmCommand();
  if (!existsSync(developmentProvenance)) {
    execFileSync(pnpm.command, [...pnpm.args, 'predev:desktop'], {
      cwd: repoRoot,
      stdio: 'inherit',
      env: process.env,
    });
  }

  if (!existsSync(desktopEntryPoint) || !existsSync(rendererEntryPoint)) {
    execFileSync(pnpm.command, [...pnpm.args, 'build:desktop'], {
      cwd: repoRoot,
      stdio: 'inherit',
      env: process.env,
    });
  }

  execFileSync(process.execPath, [resolve(repoRoot, 'scripts/ensure-native-deps.mjs')], {
    cwd: repoRoot,
    stdio: 'inherit',
    env: process.env,
  });
}

function ensurePnpmShim() {
  try {
    execFileSync('corepack', ['enable'], {
      cwd: repoRoot,
      stdio: 'ignore',
      env: process.env,
    });
  } catch {
    // Some local environments install pnpm directly and do not need Corepack.
  }
}

function resolvePnpmCommand() {
  const directPnpmCommand = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
  try {
    execFileSync(directPnpmCommand, ['--version'], {
      cwd: repoRoot,
      stdio: 'ignore',
      env: process.env,
    });
    return { command: directPnpmCommand, args: [] };
  } catch {
    if (process.platform === 'win32') {
      return { command: 'cmd.exe', args: ['/d', '/s', '/c', 'corepack', 'pnpm'] };
    }
    return { command: 'corepack', args: ['pnpm'] };
  }
}
