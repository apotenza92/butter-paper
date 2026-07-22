import { _electron as electron } from '@playwright/test';
import { existsSync, readdirSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';

const repoRoot = resolve(import.meta.dirname, '..');
const releaseDir = resolve(repoRoot, 'apps/desktop/release');
const fixturePath = resolve(repoRoot, 'tests/fixtures/generated/multi-page.pdf');

function findPackagedExecutable() {
  const explicit = process.env.BP_ELECTRON_EXECUTABLE_PATH;
  if (explicit) {
    const resolved = resolve(explicit);
    if (!existsSync(resolved)) {
      throw new Error(`BP_ELECTRON_EXECUTABLE_PATH does not exist: ${resolved}`);
    }
    return resolved;
  }

  if (!existsSync(releaseDir)) {
    throw new Error(`Package output directory does not exist: ${releaseDir}`);
  }

  const entries = readdirSync(releaseDir, { recursive: true, withFileTypes: true });
  const candidates = entries
    .filter((entry) => entry.isFile())
    .map((entry) => join(entry.parentPath, entry.name))
    .filter((path) => {
      if (process.platform === 'darwin') {
        return path.endsWith('.app/Contents/MacOS/Butter Paper');
      }
      if (process.platform === 'win32') {
        return basename(path).toLowerCase() === 'butter paper.exe'
          && path.toLowerCase().includes('unpacked');
      }
      return basename(path) === 'butter-paper' && path.includes('unpacked');
    });

  if (candidates.length !== 1) {
    throw new Error(
      `Expected one packaged executable in ${releaseDir}, found ${candidates.length}: ${candidates.join(', ')}`,
    );
  }
  return candidates[0];
}

const executablePath = findPackagedExecutable();
const appErrors = [];
let app;

try {
  app = await electron.launch({
    executablePath,
    args: [],
    env: {
      ...process.env,
      BP_TEST_MODE: '1',
      BP_DISABLE_RENDERER_DEV_SERVER: '1',
      BP_TEST_FIXTURE_DIR: resolve(repoRoot, 'tests/fixtures/generated'),
    },
    timeout: 60_000,
  });

  app.process().stderr?.on('data', (chunk) => appErrors.push(String(chunk)));

  const page = await app.firstWindow({ timeout: 60_000 });
  await page.waitForFunction(
    () => Boolean(window.__butterPaperTestHooks?.openDocumentPath),
    undefined,
    { timeout: 30_000 },
  );
  await page.evaluate(
    async (path) => window.__butterPaperTestHooks.openDocumentPath(path),
    fixturePath,
  );
  await page.waitForFunction(
    () => window.__butterPaperTestHooks?.getDiagnostics()?.pageCount === 6,
    undefined,
    { timeout: 30_000 },
  );

  const diagnostics = await page.evaluate(
    () => window.__butterPaperTestHooks.getDiagnostics(),
  );
  console.log(
    `Packaged desktop smoke test passed: ${diagnostics.pageCount} pages, ${diagnostics.sessionBackendKind} backend.`,
  );
} catch (error) {
  const stderr = appErrors.join('').trim();
  if (stderr) {
    console.error(`Packaged app stderr:\n${stderr}`);
  }
  throw error;
} finally {
  await app?.close();
}
