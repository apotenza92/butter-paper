import { expect, test } from '@playwright/test';
import { getDiagnostics, launchButterPaper, openFixturePdf, resolveDesktopEntryPoint } from './helpers/electron.mjs';

test.describe.skip('Page scale foundation', () => {
  test('sets a preset scale from the Document menu and applies it to all pages', async () => {
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

    await expect(page.getByTestId('status-page-scale')).toHaveText('Scale Not Set');
    await page.getByTestId('menu-trigger-document').click();
    await page.getByTestId('menu-document-set-page-scale').click();
    await expect(page.getByTestId('page-scale-dialog')).toBeVisible();
    await page.getByTestId('page-scale-pages').selectOption('all');
    await page.getByTestId('page-scale-apply').click();

    await expect(page.getByTestId('status-page-scale')).toHaveText('1:100');

    await page.getByTestId('page-thumbnail-item-2').click();
    await expect.poll(async () => {
      const diagnostics = await getDiagnostics(page);
      return diagnostics?.currentPage ?? 0;
    }).toBe(1);
    await expect(page.getByTestId('status-page-scale')).toHaveText('1:100');

    await app.close();
  });

  test('saves and deletes a custom scale preset through the dialog', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'single-page');
    await expect(page.getByTestId('status-page-scale')).toHaveText('Scale Not Set');

    await openPageScaleDialog(page);
    await page.getByTestId('page-scale-method-custom').click();
    await page.getByTestId('page-scale-custom-pdf-length').fill('0.5');
    await page.getByTestId('page-scale-custom-real-length').fill('10');
    await page.getByTestId('page-scale-save-preset').check();
    await page.getByTestId('page-scale-apply').click();

    await expect(page.getByTestId('status-page-scale')).toHaveText('0.5 cm = 10 m');

    await openPageScaleDialog(page);
    await expect(page.getByTestId('page-scale-preset-select')).toContainText('0.5 cm = 10 m (saved)');
    await page.getByRole('button', { name: 'Delete' }).click();
    await expect(page.getByTestId('page-scale-preset-select')).not.toContainText('0.5 cm = 10 m (saved)');
    await page.getByRole('button', { name: 'Cancel' }).click();

    await app.close();
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
    await expect(page.getByTestId('page-scale-dialog')).toHaveCount(0);
    await expect(page.getByTestId('page-scale-calibration-instructions')).toContainText('Click the first point');

    const pageBox = await page.getByTestId('page-1').boundingBox();
    expect(pageBox).not.toBeNull();
    await page.mouse.click(pageBox.x + 100, pageBox.y + 100);
    await expect(page.getByTestId('page-scale-calibration-instructions')).toContainText('Click the second point');
    await page.mouse.click(pageBox.x + 200, pageBox.y + 100);

    await expect(page.getByTestId('page-scale-dialog')).toBeVisible();
    await expect(page.getByTestId('page-scale-calibrate-start-x')).not.toHaveValue('0');
    await expect(page.getByTestId('page-scale-calibrate-end-x')).not.toHaveValue('72');
    await page.getByTestId('page-scale-calibrate-real-length').fill('25');
    await page.getByTestId('page-scale-calibrate-real-units').selectOption('ft');
    await page.getByTestId('page-scale-apply').click();

    await expect(page.getByTestId('status-page-scale')).toHaveText('Calibrated 25 ft');

    await app.close();
  });
});

async function openPageScaleDialog(page) {
  await page.getByTestId('menu-trigger-document').click();
  await page.getByTestId('menu-document-set-page-scale').click();
  await expect(page.getByTestId('page-scale-dialog')).toBeVisible();
}
