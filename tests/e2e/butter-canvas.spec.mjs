import { expect, test } from '@playwright/test';
import { mkdtemp, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { firstWindow, getDiagnostics, launchButterPaper, openFixturePdf, resolveDesktopEntryPoint } from './helpers/electron.mjs';

const repoRoot = resolve(fileURLToPath(new URL('../..', import.meta.url)));

test.describe('Butter Canvas workflows', () => {
  test('creates, annotates, imports assets, saves, reopens, and switches canvas tabs', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const page = await firstWindow(app);
    await page.waitForFunction(() => Boolean(window.__butterPaperTestHooks?.createButterCanvas));
    await expect(page.getByTestId('document-tab-new-canvas')).toBeVisible();
    await page.getByTestId('document-tab-new-canvas').click();
    await expect(page.getByTestId('butter-canvas-toolbar')).toBeVisible();
    await expect(page.getByTestId('butter-canvas-viewport')).toBeVisible();
    await expect(page.getByTestId('viewer-toolbar')).toHaveCount(0);
    const canvasZoomOut = page.getByRole('button', { name: 'Zoom Out' });
    await canvasZoomOut.hover();
    await expect(page.locator('[data-slot="tooltip-content"]').filter({ hasText: 'Zoom Out' })).toBeVisible();
    await page.mouse.move(0, 0);

    const viewportBox = await page.getByTestId('butter-canvas-viewport').boundingBox();
    expect(viewportBox).not.toBeNull();
    await page.getByTestId('tool-rectangle').click();
    await drag(page, viewportBox.x + 80, viewportBox.y + 80, viewportBox.x + 220, viewportBox.y + 170);
    await waitForCanvasDiagnostics(page, { markupCount: 1 });
    await page.getByTestId('tool-select').click();
    await page.mouse.click(viewportBox.x + 150, viewportBox.y + 125);
    await expect(page.getByTestId('butter-canvas-markup-selection')).toBeVisible();
    await drag(page, viewportBox.x + 150, viewportBox.y + 125, viewportBox.x + 190, viewportBox.y + 155);
    await page.keyboard.press('Delete');
    await waitForCanvasDiagnostics(page, { markupCount: 0 });
    await page.getByTestId('tool-rectangle').click();
    await drag(page, viewportBox.x + 80, viewportBox.y + 80, viewportBox.x + 220, viewportBox.y + 170);
    await waitForCanvasDiagnostics(page, { markupCount: 1 });
    await page.getByRole('button', { name: 'Undo' }).click();
    await waitForCanvasDiagnostics(page, { markupCount: 0 });
    await page.getByRole('button', { name: 'Redo' }).click();
    await waitForCanvasDiagnostics(page, { markupCount: 1 });

    await page.getByRole('button', { name: 'Canvas Scale' }).click();
    await insertGeneratedCanvasImage(page);
    await expect(page.locator('[data-testid^="butter-canvas-asset-"]').first()).toBeVisible();

    await page.getByRole('button', { name: 'Trace Image' }).click();
    await expect(page.getByTestId('butter-canvas-trace-panel')).toBeVisible();
    await expect(page.getByTestId('butter-canvas-trace-zone-preview')).toBeVisible();
    const sensitivity = page.getByTestId('butter-canvas-trace-sensitivity');
    await expect(sensitivity.locator('[data-slot="slider-thumb"]')).toHaveCount(1);
    await sensitivity.locator('input[type="range"]').fill('70');
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await waitForCanvasDiagnostics(page, { markupCount: 1 });

    await page.getByRole('button', { name: 'Trace Image' }).click();
    await expect(page.getByTestId('butter-canvas-trace-panel')).toBeVisible();
    await page.getByTestId('butter-canvas-trace-output').selectOption('line');
    await page.getByTestId('butter-canvas-trace-apply').click();
    await expect.poll(async () => (await getDiagnostics(page))?.markupCount ?? 0).toBeGreaterThan(1);
    const tracedMarkupCount = (await getDiagnostics(page))?.markupCount ?? 0;
    await page.getByRole('button', { name: 'Trace Image' }).click();
    await page.getByTestId('butter-canvas-trace-apply').click();
    await expect.poll(async () => (await getDiagnostics(page))?.markupCount ?? 0).toBe(tracedMarkupCount);

    const pdfFixture = join(repoRoot, 'tests/fixtures/generated/multi-page.pdf');
    await page.evaluate(async ({ filePath }) => {
      await window.__butterPaperTestHooks?.importCanvasPdfPath(filePath, '1');
    }, { filePath: pdfFixture });
    await expect.poll(async () => {
      return await page.evaluate(() => window.__butterPaperTestHooks?.getActiveCanvasDocument()?.assets.length ?? 0);
    }).toBeGreaterThanOrEqual(2);
    await page.evaluate(async ({ filePath }) => {
      await window.__butterPaperTestHooks?.importCanvasPdfPath(filePath, 'all');
    }, { filePath: pdfFixture });
    await expect.poll(async () => {
      return await page.evaluate(() => window.__butterPaperTestHooks?.getActiveCanvasDocument()?.assets.filter((asset) => asset.kind === 'pdf-page-snapshot').length ?? 0);
    }).toBeGreaterThanOrEqual(7);

    const tempDir = await mkdtemp(join(tmpdir(), 'butter-canvas-e2e-'));
    const savePath = join(tempDir, 'workspace.bpc');
    await page.evaluate(async ({ filePath }) => {
      await window.__butterPaperTestHooks?.saveCurrentDocumentAs(filePath);
    }, { filePath: savePath });

    const saved = JSON.parse(await readFile(savePath, 'utf8'));
    expect(saved.kind).toBe('butter-canvas');
    expect(saved.scale?.name).toBe('1:100');
    expect(saved.markups.length).toBeGreaterThan(1);
    expect(saved.assets.some((asset) => asset.kind === 'image')).toBe(true);
    const savedPdfSnapshots = saved.assets.filter((asset) => asset.kind === 'pdf-page-snapshot');
    expect(savedPdfSnapshots.length).toBeGreaterThanOrEqual(7);
    expect(new Set(savedPdfSnapshots.map((asset) => Math.round(asset.rect.y))).size).toBeGreaterThan(1);

    await page.evaluate(async ({ filePath }) => {
      await window.__butterPaperTestHooks?.closeTab(filePath);
      await window.__butterPaperTestHooks?.openCanvasPath(filePath);
    }, { filePath: savePath });
    await expect(page.getByTestId('butter-canvas-toolbar')).toBeVisible();
    await waitForCanvasDiagnostics(page, { markupCount: saved.markups.length });

    await openFixturePdf(app, 'single-page');
    await expect(page.getByTestId('viewer-toolbar')).toBeVisible();
    await expect(page.getByTestId('butter-canvas-toolbar')).toHaveCount(0);
    await page.evaluate(async ({ filePath }) => {
      await window.__butterPaperTestHooks?.switchToTab(filePath);
    }, { filePath: savePath });
    await expect(page.getByTestId('butter-canvas-toolbar')).toBeVisible();

    await app.close();
  });
});

async function insertGeneratedCanvasImage(page) {
  await page.evaluate(async () => {
    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 32;
    const context = canvas.getContext('2d');
    if (!context) {
      throw new Error('Canvas unavailable.');
    }
    context.fillStyle = '#ffffff';
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.strokeStyle = '#000000';
    context.lineWidth = 4;
    context.beginPath();
    context.moveTo(6, 16);
    context.lineTo(58, 16);
    context.stroke();
    const response = await fetch(canvas.toDataURL('image/png'));
    const blob = await response.blob();
    const file = new File([blob], 'trace-source.png', { type: 'image/png' });
    const input = document.querySelector('[data-testid="butter-canvas-image-file-input"]');
    if (!(input instanceof HTMLInputElement)) {
      throw new Error('Canvas image file input unavailable.');
    }
    const transfer = new DataTransfer();
    transfer.items.add(file);
    input.files = transfer.files;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  });
}

async function waitForCanvasDiagnostics(page, expected) {
  await expect.poll(async () => {
    const diagnostics = await getDiagnostics(page);
    return {
      pageCount: diagnostics?.pageCount ?? 0,
      markupCount: diagnostics?.markupCount ?? 0,
    };
  }).toMatchObject({ pageCount: 0, ...expected });
}

async function drag(page, startX, startY, endX, endY) {
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(endX, endY, { steps: 12 });
  await page.mouse.up();
}
