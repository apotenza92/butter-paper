import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { mkdtemp, readFile, realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PDFDocument } from 'pdf-lib';
import { firstWindow, getDiagnostics, launchButterPaper, resolveDesktopEntryPoint, saveCurrentDocumentAs } from './helpers/electron.mjs';

const MILLIMETRES_TO_POINTS = 72 / 25.4;
const PAPER_SIZES = [
  ['A5', 148, 210],
  ['A4', 210, 297],
  ['A3', 297, 420],
  ['A2', 420, 594],
  ['A1', 594, 841],
  ['A0', 841, 1189],
];

test.describe('New blank PDF', () => {
  test('opens the size picker from the File menu and creates the selected page', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) test.skip(true, 'Desktop app could not be launched');

    const page = await firstWindow(app);
    const outputDirectory = await mkdtemp(join(tmpdir(), 'butter-paper-blank-menu-e2e-'));

    try {
      await page.getByTestId('menu-trigger-file').click();
      await page.getByTestId('menu-file-new-pdf').click();
      const dialog = page.getByTestId('new-blank-pdf-dialog');
      await expect(dialog).toBeVisible();
      await expect(page.getByTestId('new-blank-pdf-dialog-paper-size')).toHaveValue('a3');
      await page.getByTestId('new-blank-pdf-dialog-paper-size').selectOption('a4');
      await page.getByTestId('new-blank-pdf-dialog-portrait').click();
      await page.getByTestId('new-blank-pdf-dialog-create').click();

      await expect(dialog).toHaveCount(0);
      await expect.poll(async () => (await getDiagnostics(page))?.documentName).toBe('Untitled.pdf');
      await expect(page.getByTestId('document-tab-new-pdf')).toHaveAttribute('aria-label', 'New from Blank Paper');
      const outputPath = join(outputDirectory, 'file-menu-a4-portrait.pdf');
      await saveCurrentDocumentAs(page, outputPath);
      const document = await PDFDocument.load(await readFile(outputPath), { updateMetadata: false });
      expect(document.getPage(0).getWidth()).toBeCloseTo(210 * MILLIMETRES_TO_POINTS, 3);
      expect(document.getPage(0).getHeight()).toBeCloseTo(297 * MILLIMETRES_TO_POINTS, 3);

      await page.evaluate(async ({ path }) => {
        await window.__butterPaperTestHooks?.closeTab(path);
      }, { path: outputPath });
    } finally {
      await app.close();
      await rm(outputDirectory, { recursive: true, force: true });
    }
  });

  test('keeps equally spaced document actions after the tabs, uses the template picker, and protects a dirty tab', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) test.skip(true, 'Desktop app could not be launched');

    const page = await firstWindow(app);
    const openButton = page.getByTestId('document-tab-open');
    const newPdfButton = page.getByTestId('document-tab-new-pdf');
    const templatePickerButton = page.getByTestId('document-tab-template-picker');
    const initialOpenBounds = await openButton.boundingBox();
    const initialNewPdfBounds = await newPdfButton.boundingBox();
    expect(initialOpenBounds).not.toBeNull();
    expect(initialNewPdfBounds).not.toBeNull();
    expect(initialOpenBounds.x).toBeLessThan(100);
    expect(initialNewPdfBounds.x - (initialOpenBounds.x + initialOpenBounds.width)).toBeCloseTo(8, 0);

    await templatePickerButton.click();
    const templatePicker = page.getByTestId('template-picker');
    await expect(templatePicker).toBeVisible();
    await expect(page.getByTestId('template-picker-item-built-in-blank')).toBeVisible();
    await expect(page.getByTestId('template-preview-card')).toContainText('Blank Paper');

    const browserWindow = await app.browserWindow(page);
    await browserWindow.evaluate((window) => window.webContents.setZoomFactor(2));
    await expect.poll(async () => {
      const settingsBounds = await templatePicker.boundingBox();
      const constrainedViewportHeight = await page.evaluate(() => window.innerHeight);
      return settingsBounds !== null
        && settingsBounds.y >= 0
        && settingsBounds.y + settingsBounds.height <= constrainedViewportHeight;
    }).toBe(true);
    await page.keyboard.press('Escape');
    await expect(templatePicker).toHaveCount(0);
    await expect(templatePickerButton).toBeFocused();
    await browserWindow.evaluate((window) => window.webContents.setZoomFactor(1));

    await newPdfButton.click();
    await expect.poll(async () => (await getDiagnostics(page))?.documentName).toBe('Untitled.pdf');
    await expect.poll(async () => (await getDiagnostics(page))?.tabs?.[0]?.dirty).toBe(true);

    const dirtyMarker = page.locator('[data-document-tab-dirty]');
    const dirtyLabel = page.locator('.bp-document-tab-label');
    const dirtyClose = page.getByRole('button', { name: 'Close Untitled.pdf' });
    await expect(dirtyMarker).toHaveCount(1);
    const dirtyMarkerBounds = await dirtyMarker.boundingBox();
    const dirtyLabelBounds = await dirtyLabel.boundingBox();
    const dirtyCloseBounds = await dirtyClose.boundingBox();
    expect(dirtyMarkerBounds).not.toBeNull();
    expect(dirtyLabelBounds).not.toBeNull();
    expect(dirtyCloseBounds).not.toBeNull();
    expect(dirtyMarkerBounds.x + dirtyMarkerBounds.width).toBeLessThanOrEqual(dirtyLabelBounds.x);
    expect(dirtyMarkerBounds.x + dirtyMarkerBounds.width).toBeLessThan(dirtyCloseBounds.x);
    expect(Math.abs(
      dirtyMarkerBounds.y + dirtyMarkerBounds.height / 2
      - (dirtyLabelBounds.y + dirtyLabelBounds.height / 2),
    )).toBeLessThanOrEqual(1);

    const tabBounds = await page.getByTestId('document-tab-0').boundingBox();
    const separatorBounds = await page.getByTestId('document-tab-actions-separator').boundingBox();
    const populatedOpenBounds = await openButton.boundingBox();
    const populatedNewPdfBounds = await newPdfButton.boundingBox();
    expect(tabBounds).not.toBeNull();
    expect(separatorBounds).not.toBeNull();
    expect(populatedOpenBounds.x).toBeGreaterThanOrEqual(tabBounds.x + tabBounds.width);
    expect(populatedOpenBounds.x).toBeLessThan(populatedNewPdfBounds.x);
    const tabToSeparatorGap = separatorBounds.x - (tabBounds.x + tabBounds.width);
    const separatorToOpenGap = populatedOpenBounds.x - (separatorBounds.x + separatorBounds.width);
    const openToNewPdfGap = populatedNewPdfBounds.x - (populatedOpenBounds.x + populatedOpenBounds.width);
    expect(tabToSeparatorGap).toBeCloseTo(8, 0);
    expect(separatorToOpenGap).toBeCloseTo(8, 0);
    expect(openToNewPdfGap).toBeCloseTo(8, 0);
    expect(Math.abs(tabToSeparatorGap - separatorToOpenGap)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(separatorToOpenGap - openToNewPdfGap)).toBeLessThanOrEqual(0.5);

    const temporarySourcePath = (await getDiagnostics(page))?.documentPath;
    expect(temporarySourcePath && existsSync(temporarySourcePath)).toBe(true);
    await page.getByTestId('document-tab-0').hover();
    await page.getByRole('button', { name: 'Close Untitled.pdf' }).click();
    const closeConfirmation = page.getByTestId('confirmation-popover');
    await expect(closeConfirmation).toBeVisible();
    await closeConfirmation.getByRole('button', { name: 'Cancel' }).click();
    await expect(closeConfirmation).toHaveCount(0);
    await expect(page.getByTestId('document-tab-0')).toBeVisible();
    await page.getByTestId('document-tab-0').hover();
    await page.getByRole('button', { name: 'Close Untitled.pdf' }).click();
    await closeConfirmation.getByRole('button', { name: 'Discard' }).click();
    await expect(page.getByTestId('document-tab-0')).toHaveCount(0);
    expect(existsSync(temporarySourcePath)).toBe(false);

    await page.evaluate(() => {
      void window.butterPaper.application.confirmClose();
    });
    await app.close();
  });

  test('creates exact A0-A5 portrait and landscape PDFs through the normal session', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) test.skip(true, 'Desktop app could not be launched');
    const page = await firstWindow(app);
    const outputDirectory = await mkdtemp(join(tmpdir(), 'butter-paper-blank-e2e-'));

    try {
      for (const [label, portraitWidthMm, portraitHeightMm] of PAPER_SIZES) {
        for (const orientation of ['portrait', 'landscape']) {
          const widthMm = orientation === 'portrait' ? portraitWidthMm : portraitHeightMm;
          const heightMm = orientation === 'portrait' ? portraitHeightMm : portraitWidthMm;
          await page.evaluate(async ({ widthMm: width, heightMm: height }) => {
            await window.__butterPaperTestHooks?.createBlankPdf({ widthMm: width, heightMm: height });
          }, { widthMm, heightMm });
          await expect.poll(async () => (await getDiagnostics(page))?.pageCount).toBe(1);

          const outputPath = join(outputDirectory, `${label}-${orientation}.pdf`);
          await saveCurrentDocumentAs(page, outputPath);
          const document = await PDFDocument.load(await readFile(outputPath), { updateMetadata: false });
          expect(document.getPageCount()).toBe(1);
          expect(document.getCreator()).toBe('Butter Paper');
          expect(document.getProducer()).toBe('Butter Paper');
          expect(document.getPage(0).getWidth()).toBeCloseTo(widthMm * MILLIMETRES_TO_POINTS, 3);
          expect(document.getPage(0).getHeight()).toBeCloseTo(heightMm * MILLIMETRES_TO_POINTS, 3);

          const canonicalOutputPath = await realpath(outputPath);
          await expect.poll(async () => (await getDiagnostics(page))?.documentPath).toBe(canonicalOutputPath);
          await page.evaluate(async () => {
            await window.__butterPaperTestHooks?.closeTab(0);
          });
          await expect.poll(async () => (await getDiagnostics(page))?.tabs?.length ?? 0).toBe(0);
        }
      }
    } finally {
      await app.close();
      await rm(outputDirectory, { recursive: true, force: true });
    }
  });

  test('round-trips annotations and coordinates application shutdown across dirty PDFs', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) test.skip(true, 'Desktop app could not be launched');
    const childProcess = app.process();
    const page = await firstWindow(app);
    const outputDirectory = await mkdtemp(join(tmpdir(), 'butter-paper-blank-annotation-e2e-'));

    try {
      await page.evaluate(async () => {
        await window.__butterPaperTestHooks?.createBlankPdf({ widthMm: 210, heightMm: 297 });
      });
      await expect.poll(async () => (await getDiagnostics(page))?.pageCount).toBe(1);
      const layer = page.getByTestId('annotation-layer-1');
      await expect(layer).toBeVisible();
      await page.getByTestId('tool-rectangle').click();
      await expect.poll(async () => (await getDiagnostics(page))?.activeTool).toBe('rectangle');
      await layer.click({ position: { x: 50, y: 60 } });
      await layer.click({ position: { x: 160, y: 130 } });
      await expect.poll(async () => (await getDiagnostics(page))?.markupCount).toBe(1);

      const temporarySourcePath = (await getDiagnostics(page))?.documentPath;
      expect(typeof temporarySourcePath).toBe('string');
      const savedPath = join(outputDirectory, 'annotated-blank.pdf');
      await saveCurrentDocumentAs(page, savedPath);
      expect(existsSync(savedPath)).toBe(true);
      expect(existsSync(temporarySourcePath)).toBe(false);
      await page.evaluate(async () => {
        await window.__butterPaperTestHooks?.closeTab(0);
      });
      await expect.poll(async () => (await getDiagnostics(page))?.tabs?.length ?? 0).toBe(0);
      await page.evaluate(async ({ path }) => {
        await window.__butterPaperTestHooks?.openDocumentPath(path);
      }, { path: savedPath });
      await expect.poll(async () => (await getDiagnostics(page))?.markupCount).toBe(1);

      await page.evaluate(async () => {
        await window.__butterPaperTestHooks?.createBlankPdf({ widthMm: 297, heightMm: 210 });
      });
      await expect.poll(async () => (await getDiagnostics(page))?.tabs?.filter((tab) => tab.dirty).length).toBe(1);
      await page.evaluate(async () => {
        await window.__butterPaperTestHooks?.createBlankPdf({ widthMm: 420, heightMm: 297 });
      });
      await expect.poll(async () => (await getDiagnostics(page))?.tabs?.filter((tab) => tab.dirty).length).toBe(2);
      await page.getByTestId('menu-trigger-butter-paper').click();
      await page.getByTestId('menu-quit').click();
      await expect(page.getByTestId('unsaved-changes-dialog')).toContainText('Save All');
      await page.getByRole('button', { name: 'Cancel' }).click();
      await expect(page.getByTestId('app-root')).toBeVisible();
      const closed = new Promise((resolve) => app.once('close', resolve));
      await page.getByTestId('menu-trigger-butter-paper').click();
      await page.getByTestId('menu-quit').click();
      await expect(page.getByTestId('unsaved-changes-dialog')).toContainText('Save All');
      await page.getByTestId('unsaved-discard').click();
      await closed;
    } finally {
      if (childProcess.exitCode === null) childProcess.kill();
      await rm(outputDirectory, { recursive: true, force: true });
    }
  });
});
