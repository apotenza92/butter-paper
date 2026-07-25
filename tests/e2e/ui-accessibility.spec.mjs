import { expect, test } from '@playwright/test';
import { firstWindow, getDiagnostics, launchButterPaper, openFixturePdf, resolveDesktopEntryPoint } from './helpers/electron.mjs';

test.describe('shadcn Base UI shell accessibility', () => {
  test('supports keyboard menus, accessible tooltips, valid tabs, and protected custom icons', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) test.skip(true, 'Desktop app could not be launched');

    const page = await firstWindow(app);
    const fileTrigger = page.getByTestId('menu-trigger-file');
    await fileTrigger.focus();
    await page.keyboard.press('Enter');
    const openItem = page.getByTestId('menu-file-open');
    await expect(openItem).toBeVisible();
    await expect(openItem).toHaveRole('menuitem');
    await expect(openItem).toBeFocused();
    const activeToolBeforeMenuShortcut = (await getDiagnostics(page))?.activeTool;
    await page.keyboard.press('r');
    await expect.poll(async () => (await getDiagnostics(page))?.activeTool).toBe(activeToolBeforeMenuShortcut);
    await page.keyboard.press('ArrowDown');
    await expect(page.getByTestId('menu-file-open-canvas')).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(openItem).toHaveCount(0);
    await expect(fileTrigger).toBeFocused();

    const appTrigger = page.getByTestId('menu-trigger-butter-paper');
    await appTrigger.click();
    const checkForUpdates = page.getByTestId('menu-check-for-updates');
    await expect(checkForUpdates).toBeVisible();
    await expect(checkForUpdates).toHaveAttribute('aria-disabled', 'true');
    await expect(page.getByTestId('menu-open-release-page')).toBeVisible();
    const updateFrequency = page.getByTestId('menu-update-frequency');
    await updateFrequency.hover();
    const dailyFrequency = page.getByTestId('menu-update-frequency-daily');
    await expect(dailyFrequency).toBeVisible();
    await expect(dailyFrequency).toHaveAttribute('aria-checked', 'true');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');

    const { page: documentPage } = await openFixturePdf(app, 'zoom-target');
    const viewerToolbar = documentPage.getByTestId('viewer-toolbar');
    await expect(viewerToolbar.locator('[data-slot="separator"]')).toHaveCount(0);
    for (const groupName of ['Zoom controls', 'Fit controls', 'Page view controls', 'Snapping controls']) {
      await expect(viewerToolbar.getByRole('group', { name: groupName })).toBeVisible();
    }
    await openFixturePdf(app, 'single-page');
    const tablist = documentPage.getByRole('tablist', { name: 'Open documents' });
    await expect(tablist).toBeVisible();
    const zoomTab = documentPage.getByRole('tab', { name: /zoom-target\.pdf/i });
    const singlePageTab = documentPage.getByRole('tab', { name: /single-page\.pdf/i });
    await expect(tablist.getByRole('tab')).toHaveCount(2);
    await expect(singlePageTab).toHaveAttribute('aria-selected', 'true');
    await expect(zoomTab).toHaveAttribute('tabindex', '-1');
    await expect(singlePageTab).toHaveAttribute('tabindex', '0');
    await expect(singlePageTab).toHaveAttribute('aria-controls', 'document-tab-panel');
    await expect(documentPage.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'document-tab-trigger-1');
    await expect(singlePageTab.locator('button')).toHaveCount(0);
    await singlePageTab.focus();
    await page.keyboard.press('Home');
    await expect(zoomTab).toBeFocused();
    await expect(zoomTab).toHaveAttribute('aria-selected', 'true');
    await expect(zoomTab).toHaveAttribute('tabindex', '0');
    await expect(singlePageTab).toHaveAttribute('tabindex', '-1');
    await page.keyboard.press('End');
    await expect(singlePageTab).toBeFocused();
    await page.keyboard.press('ArrowLeft');
    await expect(zoomTab).toBeFocused();
    await documentPage.getByRole('button', { name: 'Close zoom-target.pdf' }).click();
    await expect(singlePageTab).toBeFocused();
    await expect(documentPage.getByRole('button', { name: 'Close zoom-target.pdf' })).toHaveCount(0);

    const canvasButton = documentPage.getByTestId('document-tab-new-canvas');
    await documentPage.getByRole('button', { name: 'Close single-page.pdf' }).focus();
    await page.keyboard.press('Tab');
    await expect(canvasButton).toBeFocused();
    await expect(canvasButton).toHaveAccessibleName('New Butter Canvas');
    await expect(documentPage.getByTestId('document-tab-new-canvas-tooltip')).toContainText('New Butter Canvas');

    const thumbnailPreview = documentPage.getByTestId('page-thumbnail-preview-1');
    const thumbnailLabel = documentPage.getByTestId('page-thumbnail-item-1').getByText('Page 1', { exact: true });
    const previewBounds = await thumbnailPreview.boundingBox();
    const labelBounds = await thumbnailLabel.boundingBox();
    expect(previewBounds).not.toBeNull();
    expect(labelBounds).not.toBeNull();
    expect(labelBounds.y).toBeGreaterThanOrEqual(previewBounds.y + previewBounds.height);

    for (const testId of ['icon-fit-width', 'icon-fit-page', 'icon-continuous-view', 'icon-butter-canvas']) {
      const icon = documentPage.getByTestId(testId);
      const primaryGlyph = await icon.evaluate((element) => element instanceof SVGElement) ? icon : icon.locator('svg').first();
      const primaryGlyphBounds = await primaryGlyph.boundingBox();
      expect(primaryGlyphBounds, `${testId} primary glyph should render`).not.toBeNull();
      expect(primaryGlyphBounds.width, `${testId} primary glyph width`).toBeGreaterThanOrEqual(17.5);
      expect(primaryGlyphBounds.height, `${testId} primary glyph height`).toBeGreaterThanOrEqual(17.5);
    }

    for (const testId of ['icon-fit-width', 'icon-fit-page', 'icon-butter-canvas']) {
      await expect(documentPage.getByTestId(testId).locator('svg')).toHaveCount(0);
    }

    for (const testId of ['tool-cloud-plus', 'tool-callout']) {
      const glyphs = documentPage.getByTestId(testId).locator('svg');
      const primaryBounds = await glyphs.first().boundingBox();
      const overlayBounds = await glyphs.last().boundingBox();
      expect(primaryBounds, `${testId} primary glyph should render`).not.toBeNull();
      expect(primaryBounds.width).toBeGreaterThanOrEqual(17.5);
      expect(primaryBounds.height).toBeGreaterThanOrEqual(17.5);
      expect(overlayBounds, `${testId} overlay glyph should render`).not.toBeNull();
      expect(overlayBounds.width).toBeGreaterThanOrEqual(6.5);
      expect(overlayBounds.width).toBeLessThanOrEqual(8.5);
      expect(overlayBounds.height).toBeGreaterThanOrEqual(6.5);
      expect(overlayBounds.height).toBeLessThanOrEqual(8.5);
    }

    const cadTrigger = documentPage.getByTestId('viewer-cad-view');
    await cadTrigger.click();
    await expect(cadTrigger).toHaveAttribute('aria-pressed', 'true');
    await cadTrigger.click();
    const columnsToggle = documentPage.getByTestId('viewer-cad-organisation-columns');
    const rowsToggle = documentPage.getByTestId('viewer-cad-organisation-rows');
    await expect(columnsToggle).toBeVisible();
    await columnsToggle.focus();
    await page.keyboard.press('ArrowRight');
    await expect(rowsToggle).toBeFocused();
    await expect(rowsToggle).toHaveAttribute('aria-pressed', 'true');
    const zoomWheelToggle = documentPage.getByTestId('viewer-cad-wheel-zoom');
    const scrollWheelToggle = documentPage.getByTestId('viewer-cad-wheel-scroll');
    await zoomWheelToggle.focus();
    await page.keyboard.press('ArrowDown');
    await expect(scrollWheelToggle).toBeFocused();
    await expect(scrollWheelToggle).toHaveAttribute('aria-pressed', 'true');
    const activeToolBeforePopoverShortcut = (await getDiagnostics(documentPage))?.activeTool;
    await page.keyboard.press('r');
    await expect.poll(async () => (await getDiagnostics(documentPage))?.activeTool).toBe(activeToolBeforePopoverShortcut);
    await page.keyboard.press('Escape');
    await expect(cadTrigger).toBeFocused();

    await app.close();
  });

  test('contains the toolbar and portal menus at 200 percent zoom', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'dark' });
    if (!app) test.skip(true, 'Desktop app could not be launched');

    const { page } = await openFixturePdf(app, 'zoom-target');
    const browserWindow = await app.browserWindow(page);
    await browserWindow.evaluate((window) => window.webContents.setZoomFactor(2));

    await expect.poll(async () => page.evaluate(() => document.body.scrollWidth <= window.innerWidth)).toBe(true);
    const toolbar = page.getByTestId('viewer-toolbar');
    await expect.poll(async () => toolbar.evaluate((element) => element.scrollWidth > element.clientWidth)).toBe(true);

    const snapTrigger = page.getByTestId('viewer-snap-target-menu');
    await snapTrigger.evaluate((element) => element.scrollIntoView({ block: 'nearest', inline: 'nearest' }));
    await snapTrigger.hover();
    await expect(page.locator('[data-slot="tooltip-content"]').filter({ hasText: 'Snap' })).toBeVisible();
    await page.mouse.move(0, 0);
    await snapTrigger.click();
    const snapItem = page.getByTestId('viewer-snap-content');
    await expect(snapItem).toBeVisible();
    const popupBounds = await page.locator('[data-slot="dropdown-menu-content"]').boundingBox();
    const viewport = await page.evaluate(() => ({ width: window.innerWidth, height: window.innerHeight }));
    expect(popupBounds).not.toBeNull();
    expect(popupBounds.x).toBeGreaterThanOrEqual(0);
    expect(popupBounds.y).toBeGreaterThanOrEqual(0);
    expect(popupBounds.x + popupBounds.width).toBeLessThanOrEqual(viewport.width + 1);
    expect(popupBounds.y + popupBounds.height).toBeLessThanOrEqual(viewport.height + 1);

    await browserWindow.evaluate((window) => window.webContents.setZoomFactor(1));
    await app.close();
  });
});
