import { _electron as electron } from '@playwright/test';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const moduleDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(moduleDir, '../../..');
const require = createRequire(import.meta.url);

export function resolveDesktopEntryPoint() {
  const explicit = process.env.BP_DESKTOP_ENTRY;
  if (explicit && existsSync(explicit)) {
    return explicit;
  }

  const candidates = [
    resolve(repoRoot, 'apps/desktop/.vite/build/main.js'),
    resolve(repoRoot, 'apps/desktop/.vite/build/main.mjs'),
  ];

  return candidates.find((candidate) => existsSync(candidate));
}

export async function launchButterPaper(options = {}) {
  const { theme } = options;
  const entryPoint = resolveDesktopEntryPoint();
  if (!entryPoint) {
    return null;
  }
  const explicitExecutablePath = process.env.BP_ELECTRON_EXECUTABLE_PATH;
  const installedElectronPath = require('electron');
  const executablePath = explicitExecutablePath && existsSync(explicitExecutablePath)
    ? explicitExecutablePath
    : typeof installedElectronPath === 'string' && existsSync(installedElectronPath)
      ? installedElectronPath
      : undefined;
  const launchArgs = explicitExecutablePath && executablePath === explicitExecutablePath
    ? []
    : [entryPoint];

  const env = {
    ...process.env,
    BP_TEST_MODE: '1',
    BP_DISABLE_RENDERER_DEV_SERVER: '1',
    BP_TEST_FIXTURE_DIR: resolve(repoRoot, 'tests/fixtures/generated'),
    ...(theme ? { BP_TEST_THEME: theme } : {}),
  };
  delete env.ELECTRON_RUN_AS_NODE;

  return await electron.launch({
    ...(executablePath ? { executablePath } : {}),
    args: launchArgs,
    env,
    timeout: 60_000,
  });
}

export async function openFixturePdf(app, fixtureName = 'single-page') {
  const filePath = join(
    repoRoot,
    'tests/fixtures/generated',
    fixtureName.endsWith('.pdf') ? fixtureName : `${fixtureName}.pdf`,
  );
  const page = await firstWindow(app);
  await page.waitForFunction(() => Boolean(window.__butterPaperTestHooks?.openDocumentPath));
  await page.evaluate(async ({ path }) => {
    await window.__butterPaperTestHooks?.openDocumentPath(path);
  }, { path: filePath });
  return { page, filePath };
}

export async function saveCurrentDocumentAs(page, filePath) {
  await page.evaluate(async ({ filePath: nextPath }) => {
    await window.__butterPaperTestHooks?.saveCurrentDocumentAs(nextPath);
  }, { filePath });
}

export async function getDiagnostics(page) {
  return await page.evaluate(() => {
    return window.__butterPaperTestHooks?.getDiagnostics() ?? null;
  });
}

export async function firstWindow(app) {
  const page = app.windows()[0] ?? await app.firstWindow({ timeout: 60_000 });
  const browserWindow = await app.browserWindow(page);
  await browserWindow.evaluate((window) => window.webContents.setZoomFactor(1));
  await page.waitForLoadState('domcontentloaded');
  return page;
}
