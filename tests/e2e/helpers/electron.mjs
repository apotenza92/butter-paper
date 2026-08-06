import { _electron as electron } from '@playwright/test';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertIsolatedGuiTestEnvironment } from '../../../scripts/gui-test-environment.mjs';

assertIsolatedGuiTestEnvironment('Electron E2E');

const moduleDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(moduleDir, '../../..');
const require = createRequire(import.meta.url);
const launchDiagnostics = new WeakMap();
const firstWindowTimeoutMs = readPositiveInteger(
  process.env.BP_E2E_FIRST_WINDOW_TIMEOUT_MS,
  15_000,
);

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
  const { fixtureDirectory, theme } = options;
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
  const userDataDirectory = mkdtempSync(join(tmpdir(), 'butter-paper-e2e-'));

  const env = {
    ...process.env,
    BP_TEST_MODE: '1',
    BP_TEST_USER_DATA_DIR: userDataDirectory,
    BP_DISABLE_RENDERER_DEV_SERVER: '1',
    BP_TEST_FIXTURE_DIR: fixtureDirectory
      ? resolve(fixtureDirectory)
      : resolve(repoRoot, 'tests/fixtures/generated'),
    ...(theme ? { BP_TEST_THEME: theme } : {}),
  };
  delete env.ELECTRON_RUN_AS_NODE;

  try {
    const app = await electron.launch({
      ...(executablePath ? { executablePath } : {}),
      args: launchArgs,
      env,
      timeout: firstWindowTimeoutMs,
    });
    captureLaunchDiagnostics(app);
    app.once('close', () => {
      rmSync(userDataDirectory, { recursive: true, force: true });
    });
    return app;
  } catch (error) {
    rmSync(userDataDirectory, { recursive: true, force: true });
    throw error;
  }
}

export async function openFixturePdf(app, fixtureName = 'single-page') {
  const filePath = join(
    repoRoot,
    'tests/fixtures/generated',
    fixtureName.endsWith('.pdf') ? fixtureName : `${fixtureName}.pdf`,
  );
  return await openPdfPath(app, filePath);
}

export async function openPdfPath(app, filePath) {
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
  let page = app.windows()[0];
  if (!page) {
    let handleClose;
    const closedBeforeWindow = new Promise((_, reject) => {
      handleClose = () => reject(new Error(
        'Butter Paper exited before creating its first window.',
      ));
      app.once('close', handleClose);
    });
    try {
      page = await Promise.race([
        app.firstWindow({ timeout: firstWindowTimeoutMs }),
        closedBeforeWindow,
      ]);
    } catch (error) {
      throw new Error(formatFirstWindowFailure(error, app), { cause: error });
    } finally {
      if (handleClose) {
        app.off('close', handleClose);
      }
    }
  }
  const browserWindow = await app.browserWindow(page);
  await browserWindow.evaluate((window) => window.webContents.setZoomFactor(1));
  await page.waitForLoadState('domcontentloaded');
  return page;
}

function captureLaunchDiagnostics(app) {
  const output = [];
  launchDiagnostics.set(app, output);
  const childProcess = app.process();
  captureStream(childProcess.stdout, 'stdout', output);
  captureStream(childProcess.stderr, 'stderr', output);
}

function captureStream(stream, label, output) {
  stream?.on('data', (chunk) => {
    output.push(`[${label}] ${String(chunk).trimEnd()}`);
    while (output.join('\n').length > 20_000) {
      output.shift();
    }
  });
}

function formatFirstWindowFailure(error, app) {
  const childProcess = app.process();
  const output = launchDiagnostics.get(app)?.join('\n').trim();
  const status = childProcess.exitCode == null
    ? `still running (pid ${childProcess.pid ?? 'unknown'})`
    : `exited with code ${childProcess.exitCode}`;
  return [
    `Butter Paper did not create its first window within ${firstWindowTimeoutMs}ms; process is ${status}.`,
    error instanceof Error ? error.message : String(error),
    output ? `Main-process output:\n${output}` : 'Main-process output: <none captured>',
  ].join('\n');
}

function readPositiveInteger(value, fallback) {
  const parsed = Number.parseInt(value ?? '', 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : fallback;
}
