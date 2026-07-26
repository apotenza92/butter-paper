import { _electron as electron } from '@playwright/test';
import { existsSync } from 'node:fs';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const repoRoot = resolve(import.meta.dirname, '..');
const releaseDir = resolve(repoRoot, 'apps/desktop/release');
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
  for (const testId of ['icon-fit-width', 'icon-fit-page', 'icon-butter-canvas']) {
    const icon = page.getByTestId(testId);
    await icon.waitFor({ state: 'visible' });
    const bounds = await icon.boundingBox();
    assert(bounds && bounds.width >= 17.5 && bounds.height >= 17.5, `${testId} did not render at its reviewed size`);
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

async function verifyButterCanvasWorkflow(page, outputDirectory) {
  await page.getByTestId('document-tab-new-canvas').click();
  await page.getByTestId('butter-canvas-toolbar').waitFor({ state: 'visible' });
  const viewport = page.getByTestId('butter-canvas-viewport');
  const bounds = await viewport.boundingBox();
  assert(bounds, 'Packaged Butter Canvas viewport did not render');

  await page.getByTestId('tool-rectangle').click();
  await page.mouse.move(bounds.x + 80, bounds.y + 80);
  await page.mouse.down();
  await page.mouse.move(bounds.x + 220, bounds.y + 170, { steps: 12 });
  await page.mouse.up();
  await waitForDiagnostics(page, { pageCount: 0, markupCount: 1 });

  const savedCanvasPath = join(outputDirectory, 'packaged-butter-canvas-round-trip.bpc');
  await page.evaluate(async ({ filePath }) => {
    await window.__butterPaperTestHooks?.saveCurrentDocumentAs(filePath);
  }, { filePath: savedCanvasPath });
  const savedCanvas = JSON.parse(await readFile(savedCanvasPath, 'utf8'));
  assert(savedCanvas.kind === 'butter-canvas', 'Packaged Butter Canvas save used the wrong document kind');
  assert(savedCanvas.markups?.length === 1, 'Packaged Butter Canvas save lost its annotation');

  await page.evaluate(async ({ filePath }) => {
    await window.__butterPaperTestHooks?.closeTab(filePath);
    await window.__butterPaperTestHooks?.openCanvasPath(filePath);
  }, { filePath: savedCanvasPath });
  await page.getByTestId('butter-canvas-toolbar').waitFor({ state: 'visible' });
  await waitForDiagnostics(page, { pageCount: 0, markupCount: 1 });
}

try {
  temporaryDirectory = await mkdtemp(join(tmpdir(), 'butter-paper-packaged-smoke-'));
  app = await electron.launch({
    executablePath,
    args: process.platform === 'linux' ? ['--no-sandbox'] : [],
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
  assert(
    await page.getByRole('menuitem', { name: expectedProductName, exact: true }).count() === 1,
    `Packaged app menu did not expose the ${expectedProductName} identity`,
  );
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

  await verifyPdfWorkflow(page, temporaryDirectory);
  await verifyButterCanvasWorkflow(page, temporaryDirectory);
  const diagnostics = await getDiagnostics(page);
  console.log(
    `Packaged ${expectedProductName} smoke test passed: channel identity, PDF annotation round-trip, custom icons, fit controls, and Butter Canvas (${diagnostics.sessionBackendKind} backend).`,
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
