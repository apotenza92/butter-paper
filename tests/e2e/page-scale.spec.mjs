import { expect, test } from '@playwright/test';
import { closeButterPaperDiscardingUnsaved, getDiagnostics, launchButterPaper, openFixturePdf, resolveDesktopEntryPoint } from './helpers/electron.mjs';

test.describe('Page scale foundation', () => {
  test('sets a preset scale from the page thumbnail actions and applies it to all pages', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await expect.poll(async () => {
      const diagnostics = await getDiagnostics(page);
      return diagnostics?.pageCount ?? 0;
    }).toBe(6);

    await expect.poll(async () => (await getActiveDocument(page))?.pageScales?.length ?? 0).toBe(0);
    await page.getByTestId('page-thumbnail-set-scale-1').click();
    await expect(page.getByTestId('page-scale-dialog')).toBeVisible();
    await chooseSelectOption(page, 'page-scale-preset-select', '1:100');
    await chooseSelectOption(page, 'page-scale-pages', 'All Pages');
    await page.getByTestId('page-scale-apply').click();

    await expect.poll(async () => (await getActiveDocument(page))?.pageScales ?? []).toEqual([
      expect.objectContaining({ pageIndex: 0, name: '1:100', source: 'preset' }),
      expect.objectContaining({ pageIndex: 1, name: '1:100', source: 'preset' }),
      expect.objectContaining({ pageIndex: 2, name: '1:100', source: 'preset' }),
      expect.objectContaining({ pageIndex: 3, name: '1:100', source: 'preset' }),
      expect.objectContaining({ pageIndex: 4, name: '1:100', source: 'preset' }),
      expect.objectContaining({ pageIndex: 5, name: '1:100', source: 'preset' }),
    ]);

    await page.getByTestId('page-thumbnail-item-2').click();
    await expect.poll(async () => {
      const diagnostics = await getDiagnostics(page);
      return diagnostics?.currentPage ?? 0;
    }).toBe(1);
    await expect.poll(async () => (await getActiveDocument(page))?.pageScales?.[1]?.name).toBe('1:100');

    await closeButterPaperDiscardingUnsaved(app, page);
  });

  test('saves and deletes a custom scale preset through the dialog', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'single-page');
    await expect.poll(async () => (await getActiveDocument(page))?.pageScales?.length ?? 0).toBe(0);

    await openPageScaleDialog(page);
    await page.getByTestId('page-scale-method-custom').click();
    await page.getByTestId('page-scale-custom-pdf-length').fill('0.5');
    await page.getByTestId('page-scale-custom-real-length').fill('10');
    await page.getByTestId('page-scale-save-preset').focus();
    await page.keyboard.press('Space');
    await expect(page.getByTestId('page-scale-save-preset')).toHaveAttribute('aria-checked', 'true');
    await page.getByTestId('page-scale-apply').click();

    await expect.poll(async () => (await getActiveDocument(page))?.pageScales?.[0]).toEqual(
      expect.objectContaining({ pageIndex: 0, name: '0.5 cm = 10 m', source: 'custom' }),
    );
    await expect.poll(async () => (await getActiveDocument(page))?.scalePresets?.[0]).toEqual(
      expect.objectContaining({ name: '0.5 cm = 10 m', source: 'custom', builtIn: false }),
    );

    await openPageScaleDialog(page);
    await expect(page.getByTestId('page-scale-preset-select')).toContainText('0.5 cm = 10 m (saved)');
    await page.getByRole('button', { name: 'Delete' }).click();
    await expect(page.getByTestId('page-scale-preset-select')).not.toContainText('0.5 cm = 10 m (saved)');
    await expect.poll(async () => (await getActiveDocument(page))?.scalePresets?.length ?? 0).toBe(0);
    await page.getByRole('button', { name: 'Cancel' }).click();

    await closeButterPaperDiscardingUnsaved(app, page);
  });

  test('applies a calibrated scale through the dialog', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'single-page');
    await openPageScaleDialog(page);
    await page.getByTestId('page-scale-method-calibrate').click();
    await expect(page.getByTestId('page-scale-dialog')).toBeVisible();
    await expect(page.getByTestId('page-scale-calibrate-real-length')).toBeVisible();
    await page.getByTestId('page-scale-pick-calibration').click();
    await expect(page.getByTestId('page-scale-dialog')).toHaveCount(0);
    await expect(page.getByTestId('page-scale-calibration-instructions')).toContainText('Click the first point');

    const pageBox = await page.getByTestId('annotation-layer-1').boundingBox();
    expect(pageBox).not.toBeNull();
    const viewportBox = await page.getByTestId('document-viewport').boundingBox();
    expect(viewportBox).not.toBeNull();
    const calibrationY = Math.max(pageBox.y + 100, viewportBox.y + 100);
    await page.mouse.click(pageBox.x + 100, calibrationY);
    await expect(page.getByTestId('page-scale-calibration-instructions')).toContainText('Click the second point');
    await page.mouse.click(pageBox.x + 200, calibrationY);

    await expect(page.getByTestId('page-scale-dialog')).toBeVisible();
    await expect(page.getByTestId('page-scale-calibrate-start-x')).not.toHaveValue('0');
    await expect(page.getByTestId('page-scale-calibrate-end-x')).not.toHaveValue('72');
    await page.getByTestId('page-scale-calibrate-real-length').fill('25');
    await chooseSelectOption(page, 'page-scale-calibrate-real-units', 'ft');
    await page.getByTestId('page-scale-apply').click();

    await expect.poll(async () => (await getActiveDocument(page))?.pageScales?.[0]).toEqual(
      expect.objectContaining({ pageIndex: 0, name: 'Calibrated 25 ft', source: 'calibrated', realUnits: 'ft' }),
    );

    await closeButterPaperDiscardingUnsaved(app, page);
  });

  test('provides modal focus, keyboard controls, validation, and constrained zoom layout', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'dark' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'single-page');
    const browserWindow = await app.browserWindow(page);
    await browserWindow.evaluate((window) => window.webContents.setZoomFactor(2));

    await openPageScaleDialog(page);
    const dialog = page.getByRole('dialog', { name: 'Set Page Scale' });
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveAccessibleDescription(/Page 1 of 1.*Choose a scale/);
    await expect.poll(async () => dialog.evaluate((element) => element.contains(document.activeElement))).toBe(true);

    const dialogBox = await dialog.boundingBox();
    expect(dialogBox).not.toBeNull();
    const viewport = await page.evaluate(() => ({
      width: window.innerWidth,
      height: window.innerHeight,
    }));
    expect(dialogBox.x).toBeGreaterThanOrEqual(0);
    expect(dialogBox.y).toBeGreaterThanOrEqual(0);
    expect(dialogBox.x + dialogBox.width).toBeLessThanOrEqual(viewport.width);
    expect(dialogBox.y + dialogBox.height).toBeLessThanOrEqual(viewport.height);
    await page.getByTestId('page-scale-method-preset').focus();
    await page.keyboard.press('ArrowRight');
    await expect(page.getByTestId('page-scale-method-custom')).toBeFocused();
    await expect(page.getByTestId('page-scale-method-custom')).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByRole('combobox', { name: 'PDF units' })).toHaveCount(1);
    await expect(page.getByRole('combobox', { name: 'Real units' })).toHaveCount(1);
    await page.getByTestId('page-scale-separate-y').focus();
    await page.keyboard.press('Space');
    await expect(page.getByTestId('page-scale-separate-y')).toHaveAttribute('aria-checked', 'true');
    await expect(page.getByRole('combobox', { name: 'PDF units' })).toHaveCount(2);
    await expect(page.getByRole('combobox', { name: 'Real units' })).toHaveCount(2);
    await expect.poll(async () => page.getByTestId('page-scale-dialog-body').evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
    await expectDialogWithinViewport(dialog, viewport);

    await page.getByTestId('page-scale-pages').focus();
    await page.keyboard.press('Enter');
    const allPagesOption = page.getByRole('option', { name: 'All Pages' });
    await expect(allPagesOption).toBeVisible();
    await allPagesOption.click();
    await expect(page.getByTestId('page-scale-pages')).toContainText('All Pages');

    await page.getByTestId('page-scale-custom-pdf-length').fill('0');
    await page.getByTestId('page-scale-apply').click();
    await expect(page.getByTestId('page-scale-error')).toHaveRole('alert');
    await expect(page.getByTestId('page-scale-error')).toContainText('PDF length must be a positive number.');
    await expect(dialog).toBeVisible();
    await expectDialogWithinViewport(dialog, viewport);
    await expect.poll(async () => (await getActiveDocument(page))?.pageScales?.length ?? 0).toBe(0);

    await page.getByTestId('page-scale-apply').focus();
    await page.keyboard.press('Tab');
    await expect(page.getByTestId('page-scale-close')).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(page.getByTestId('page-scale-apply')).toBeFocused();

    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(page.getByTestId('page-thumbnail-set-scale-1')).toBeFocused();
    await expect.poll(async () => (await getActiveDocument(page))?.pageScales?.length ?? 0).toBe(0);

    await app.close();
  });
});

async function openPageScaleDialog(page) {
  await page.getByTestId('page-thumbnail-set-scale-1').click();
  await expect(page.getByTestId('page-scale-dialog')).toBeVisible();
}

async function chooseSelectOption(page, testId, optionName) {
  await page.getByTestId(testId).click();
  await page.getByRole('option', { name: optionName, exact: true }).click();
}

async function getActiveDocument(page) {
  return page.evaluate(() => window.__butterPaperTestHooks?.getActiveDocument() ?? null);
}

async function expectDialogWithinViewport(dialog, viewport) {
  const bounds = await dialog.boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds.x).toBeGreaterThanOrEqual(0);
  expect(bounds.y).toBeGreaterThanOrEqual(0);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(viewport.width + 1);
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(viewport.height + 1);
}
