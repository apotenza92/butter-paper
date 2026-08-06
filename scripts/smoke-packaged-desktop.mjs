import { _electron as electron } from '@playwright/test';
import { existsSync } from 'node:fs';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { assertIsolatedGuiTestEnvironment } from './gui-test-environment.mjs';

assertIsolatedGuiTestEnvironment('Packaged desktop smoke test');

const repoRoot = resolve(import.meta.dirname, '..');
const configuredReleaseDir = process.env.BP_RELEASE_OUTPUT_DIR?.trim();
const releaseDir = configuredReleaseDir
  ? resolve(repoRoot, 'apps/desktop', configuredReleaseDir)
  : resolve(repoRoot, 'apps/desktop/release');
const fixturePath = resolve(repoRoot, 'tests/fixtures/generated/multi-page.pdf');
const releaseChannel = process.env.BP_RELEASE_CHANNEL || 'stable';
const expectedProductName = releaseChannel === 'beta' ? 'Butter Paper Beta' : 'Butter Paper';
const expectedExecutableName = releaseChannel === 'beta' ? 'butter-paper-beta' : 'butter-paper';

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

  const packageDirectories = process.platform === 'darwin'
    ? (process.arch === 'arm64' ? ['mac-arm64'] : ['mac', 'mac-x64'])
    : process.platform === 'win32'
      ? (process.arch === 'arm64' ? ['win-arm64-unpacked'] : ['win-unpacked', 'win-x64-unpacked'])
      : (process.arch === 'arm64' ? ['linux-arm64-unpacked'] : ['linux-unpacked', 'linux-x64-unpacked']);
  const candidates = packageDirectories
    .map((directory) => {
      if (process.platform === 'darwin') {
        return join(releaseDir, directory, `${expectedProductName}.app`, 'Contents', 'MacOS', expectedProductName);
      }
      if (process.platform === 'win32') {
        return join(releaseDir, directory, `${expectedProductName}.exe`);
      }
      return join(releaseDir, directory, expectedExecutableName);
    })
    .filter((path) => existsSync(path));

  if (candidates.length !== 1) {
    throw new Error(
      `Expected one current host package in ${releaseDir}, found ${candidates.length}: ${candidates.join(', ')}`,
    );
  }
  return candidates[0];
}

const executablePath = findPackagedExecutable();
const appErrors = [];
let app;
let page;
let temporaryDirectory;

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

async function getDiagnostics(page) {
  return await page.evaluate(() => window.__butterPaperTestHooks?.getDiagnostics() ?? null);
}

async function waitForDiagnostics(page, expected) {
  await page.waitForFunction((nextExpected) => {
    const diagnostics = window.__butterPaperTestHooks?.getDiagnostics();
    return diagnostics && Object.entries(nextExpected).every(([key, value]) => diagnostics[key] === value);
  }, expected, { timeout: 30_000 });
}

async function verifyCustomIcons(page) {
  for (const testId of ['icon-fit-width', 'icon-fit-page']) {
    const icon = page.getByTestId(testId);
    await icon.waitFor({ state: 'visible' });
    const bounds = await icon.boundingBox();
    assert(
      bounds
        && bounds.width >= 15.5
        && bounds.width <= 16.5
        && bounds.height >= 15.5
        && bounds.height <= 16.5,
      `${testId} did not render at its reviewed size`,
    );
    assert(await icon.locator('svg').count() === 0, `${testId} unexpectedly fell back to a library SVG`);
  }
}

async function verifyPdfWorkflow(page, outputDirectory) {
  await verifyCustomIcons(page);
  await page.getByTestId('viewer-fit-page').click();
  await waitForDiagnostics(page, { zoomPreset: 'fit-page' });
  await page.getByTestId('viewer-fit-width').click();
  await waitForDiagnostics(page, { zoomPreset: 'fit-width' });

  const initialMarkupCount = (await getDiagnostics(page))?.markupCount ?? 0;
  const annotationLayer = page.getByTestId('annotation-layer-1');
  const layerBounds = await annotationLayer.boundingBox();
  const viewportBounds = await page.getByTestId('document-viewport').boundingBox();
  assert(layerBounds && viewportBounds, 'Packaged PDF annotation surface did not render');
  const startY = Math.max(layerBounds.y + 90, viewportBounds.y + 90);
  await page.getByTestId('tool-rectangle').click();
  await page.mouse.click(layerBounds.x + 80, startY);
  await page.mouse.click(layerBounds.x + 220, startY + 80);
  await waitForDiagnostics(page, { markupCount: initialMarkupCount + 1 });

  const savedPdfPath = join(outputDirectory, 'packaged-annotation-round-trip.pdf');
  await page.evaluate(async ({ filePath }) => {
    await window.__butterPaperTestHooks?.saveCurrentDocumentAs(filePath);
  }, { filePath: savedPdfPath });
  assert(existsSync(savedPdfPath), 'Packaged PDF save did not create its output file');
  await page.evaluate(async ({ filePath }) => {
    await window.__butterPaperTestHooks?.openDocumentPath(filePath);
  }, { filePath: savedPdfPath });
  await waitForDiagnostics(page, { pageCount: 6, markupCount: initialMarkupCount + 1 });
  await page.locator('[data-testid^="markup-rect-"]').first().waitFor({ state: 'visible' });
}

async function verifyBlankPdfWorkflow(page, outputDirectory) {
  await page.getByTestId('document-tab-new-pdf-settings').click();
  await page.getByTestId('new-blank-pdf-settings').waitFor({ state: 'visible' });
  assert(await page.getByTestId('new-blank-pdf-paper-size').inputValue() === 'a3', 'Blank PDF settings did not default to A3');
  assert(await page.getByTestId('new-blank-pdf-landscape').getAttribute('aria-pressed') === 'true', 'Blank PDF settings did not default to landscape');
  await page.keyboard.press('Escape');
  await page.getByTestId('document-tab-new-pdf').click();
  await waitForDiagnostics(page, { pageCount: 1, documentName: 'Untitled.pdf' });
  await page.waitForFunction(() => window.__butterPaperTestHooks?.getDiagnostics()?.tabs?.at(-1)?.dirty === true);

  const savedPdfPath = join(outputDirectory, 'packaged-blank-pdf.pdf');
  await page.evaluate(async ({ filePath }) => {
    await window.__butterPaperTestHooks?.saveCurrentDocumentAs(filePath);
  }, { filePath: savedPdfPath });
  assert(existsSync(savedPdfPath), 'Packaged blank PDF save did not create its output file');
  await page.evaluate(async ({ filePath }) => {
    await window.__butterPaperTestHooks?.openDocumentPath(filePath);
  }, { filePath: savedPdfPath });
  await waitForDiagnostics(page, { pageCount: 1, documentName: 'packaged-blank-pdf.pdf' });
}

try {
  temporaryDirectory = await mkdtemp(join(tmpdir(), 'butter-paper-packaged-smoke-'));
  app = await electron.launch({
    executablePath,
    args: [
      ...(process.platform === 'linux' ? ['--no-sandbox'] : []),
      fixturePath,
    ],
    env: {
      ...process.env,
      BP_TEST_MODE: '1',
      BP_DISABLE_RENDERER_DEV_SERVER: '1',
      BP_TEST_FIXTURE_DIR: resolve(repoRoot, 'tests/fixtures/generated'),
      BP_TEST_USER_DATA_DIR: process.env.BP_TEST_USER_DATA_DIR
        || join(temporaryDirectory, 'user-data'),
    },
    timeout: 60_000,
  });

  app.process().stderr?.on('data', (chunk) => appErrors.push(String(chunk)));

  page = await app.firstWindow({ timeout: 60_000 });
  await page.waitForFunction(
    (productName) => document.title === productName,
    expectedProductName,
    { timeout: 60_000 },
  );
  const productMenuItem = page.getByRole('menuitem', { name: expectedProductName, exact: true });
  await productMenuItem.waitFor({ state: 'visible', timeout: 30_000 });
  assert(
    await productMenuItem.count() === 1,
    `Packaged app menu did not expose the ${expectedProductName} identity`,
  );
  await page.getByTestId('menu-trigger-butter-paper').click();
  await page.getByTestId('menu-set-default-pdf-app').waitFor({ state: 'visible' });
  await page.keyboard.press('Escape');
  await page.waitForFunction(
    () => Boolean(window.__butterPaperTestHooks?.openDocumentPath),
    undefined,
    { timeout: 30_000 },
  );
  await page.waitForFunction(
    () => window.__butterPaperTestHooks?.getDiagnostics()?.pageCount === 6,
    undefined,
    { timeout: 30_000 },
  );

  await verifyPdfWorkflow(page, temporaryDirectory);
  await verifyBlankPdfWorkflow(page, temporaryDirectory);
  const diagnostics = await getDiagnostics(page);
  console.log(
    `Packaged ${expectedProductName} smoke test passed: channel identity, PDF annotation round-trip, blank PDF creation, custom icons, and fit controls (${diagnostics.sessionBackendKind} backend).`,
  );
} catch (error) {
  if (page) {
    console.error(`Packaged app page at failure: title=${JSON.stringify(await page.title())} url=${page.url()}`);
  }
  const stderr = appErrors.join('').trim();
  if (stderr) {
    console.error(`Packaged app stderr:\n${stderr}`);
  }
  throw error;
} finally {
  await app?.close();
  if (temporaryDirectory) {
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}
