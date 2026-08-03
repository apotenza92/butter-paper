import { expect, test } from '@playwright/test';
import { copyFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { firstWindow, getDiagnostics, launchButterPaper, resolveDesktopEntryPoint } from './helpers/electron.mjs';

const NORMAL_TOOL_TEST_IDS = [
  'tool-select',
  'tool-pan',
  'tool-text-box',
  'tool-arrow',
  'tool-pen',
  'tool-highlight',
  'tool-cloud',
  'tool-cloud-plus',
  'tool-callout',
  'tool-image',
  'tool-snapshot',
];
const CAD_TOOL_TEST_IDS = [
  'tool-rectangle',
  'tool-ellipse',
  'tool-line',
  'tool-arc',
  'tool-polyline',
  'tool-polygon',
  'tool-dimension',
  'tool-length',
  'tool-polylength',
  'tool-area',
];

test.describe('shell overflow controls', () => {
  test('scrolls overflowing tabs with either wheel axis and keeps both actions available', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) test.skip(true, 'Desktop app could not be launched');
    const temporaryDirectory = await mkdtemp(join(tmpdir(), 'butter-paper-tab-overflow-e2e-'));

    try {
      const sourcePath = resolve(import.meta.dirname, '../fixtures/generated/single-page.pdf');
      const paths = await Promise.all(Array.from({ length: 10 }, async (_value, index) => {
        const path = join(temporaryDirectory, `overflow-document-${String(index + 1).padStart(2, '0')}.pdf`);
        await copyFile(sourcePath, path);
        return path;
      }));
      const page = await firstWindow(app);
      await page.waitForFunction(() => Boolean(window.__butterPaperTestHooks?.openDocumentPaths));
      await page.evaluate(async ({ filePaths }) => {
        await window.__butterPaperTestHooks?.openDocumentPaths(filePaths);
      }, { filePaths: paths });
      await expect.poll(async () => (await getDiagnostics(page))?.tabs?.length).toBe(paths.length);

      const tabList = page.getByTestId('document-tab-list');
      await expect.poll(async () => tabList.evaluate((element) => element.scrollWidth > element.clientWidth)).toBe(true);
      const tabListBounds = await tabList.boundingBox();
      expect(tabListBounds).not.toBeNull();
      await page.mouse.move(tabListBounds.x + tabListBounds.width / 2, tabListBounds.y + tabListBounds.height / 2);

      await page.mouse.wheel(0, 240);
      await expect.poll(async () => tabList.evaluate((element) => element.scrollLeft)).toBeGreaterThan(0);
      await tabList.evaluate((element) => { element.scrollLeft = 0; });
      await page.mouse.wheel(240, 0);
      await expect.poll(async () => tabList.evaluate((element) => element.scrollLeft)).toBeGreaterThan(0);

      await tabList.evaluate((element) => { element.scrollLeft = 0; });
      const firstTab = page.getByTestId('document-tab-0');
      const lastTab = page.getByTestId(`document-tab-${paths.length - 1}`);
      await firstTab.focus();
      await page.keyboard.press('End');
      await expect(lastTab).toBeFocused();
      await expect.poll(async () => tabList.evaluate((element) => element.scrollLeft)).toBeGreaterThan(0);
      await expect(page.getByTestId('document-tab-open')).toBeVisible();
      await expect(page.getByTestId('document-tab-new-pdf')).toBeVisible();
    } finally {
      await app.close().catch(() => undefined);
      await rm(temporaryDirectory, { recursive: true, force: true });
    }
  });

  test('keeps normal and CAD tools grouped while the rail snaps between flowing column widths', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) test.skip(true, 'Desktop app could not be launched');

    try {
      const page = await firstWindow(app);
      await page.waitForFunction(() => Boolean(window.__butterPaperTestHooks?.openFixturePdf));
      await page.evaluate(async () => {
        await window.__butterPaperTestHooks?.openFixturePdf('single-page');
      });
      await expect.poll(async () => (await getDiagnostics(page))?.pageCount).toBe(1);

      const rightRail = page.getByTestId('right-rail');
      const normalGroup = page.getByTestId('right-rail-normal');
      const cadGroup = page.getByTestId('right-rail-cad');
      const rightRailBounds = await rightRail.boundingBox();
      expect(rightRailBounds?.width).toBeGreaterThanOrEqual(87);
      expect(rightRailBounds?.width).toBeLessThanOrEqual(89);
      await expect(rightRail).toHaveAttribute('data-column-count', '2');
      await expect(rightRail).toHaveText('');
      for (const testId of NORMAL_TOOL_TEST_IDS) {
        await expect(normalGroup.getByTestId(testId)).toBeVisible();
        await expect(cadGroup.getByTestId(testId)).toHaveCount(0);
      }
      for (const testId of CAD_TOOL_TEST_IDS) {
        await expect(cadGroup.getByTestId(testId)).toBeVisible();
        await expect(normalGroup.getByTestId(testId)).toHaveCount(0);
      }
      await expect(normalGroup.getByTestId('tool-select')).toHaveAccessibleName('Select');
      const rectangleTool = cadGroup.getByTestId('tool-rectangle');
      await expect(rectangleTool).toHaveAccessibleName('Rectangle');
      await rectangleTool.hover();
      await expect(page.locator('[data-slot="tooltip-content"]').filter({ hasText: 'Rectangle (R)' })).toBeVisible();
      await rectangleTool.click();
      await expect(rectangleTool).toHaveAttribute('aria-pressed', 'true');

      const divider = page.getByTestId('right-rail-group-divider');
      const dividerBounds = await divider.boundingBox();
      expect(dividerBounds?.width).toBeGreaterThanOrEqual(71);
      expect(dividerBounds?.width).toBeLessThanOrEqual(73);

      const resizeHandle = page.getByTestId('right-rail-resize-handle');
      await resizeHandle.focus();
      await page.keyboard.press('ArrowRight');
      await expect(rightRail).toHaveAttribute('data-column-count', '3');
      await expect.poll(async () => (await rightRail.boundingBox())?.width).toBeGreaterThanOrEqual(127);
      const selectBounds = await normalGroup.getByTestId('tool-select').boundingBox();
      const panBounds = await normalGroup.getByTestId('tool-pan').boundingBox();
      const textBoxBounds = await normalGroup.getByTestId('tool-text-box').boundingBox();
      const arrowBounds = await normalGroup.getByTestId('tool-arrow').boundingBox();
      expect(selectBounds).not.toBeNull();
      expect(panBounds?.x).toBeGreaterThan(selectBounds.x);
      expect(textBoxBounds?.x).toBeGreaterThan(panBounds.x);
      expect(arrowBounds?.y).toBeGreaterThan(selectBounds.y);
      const widenedDividerBounds = await divider.boundingBox();
      expect(widenedDividerBounds?.width).toBeGreaterThanOrEqual(111);
      expect(widenedDividerBounds?.width).toBeLessThanOrEqual(113);

      await resizeHandle.focus();
      await page.keyboard.press('Home');
      await expect(rightRail).toHaveAttribute('data-column-count', '1');
      await page.getByTestId('document-viewport').hover();
      await expect.poll(async () => (await rightRail.boundingBox())?.width).toBeLessThanOrEqual(49);

      await normalGroup.getByTestId('tool-select').hover();
      await expect(rightRail).toHaveAttribute('data-expanded', '');
      await expect.poll(async () => (await rightRail.boundingBox())?.width).toBeGreaterThanOrEqual(183);
      await expect(normalGroup.getByTestId('tool-select')).toContainText('Select');
      await expect(page.locator('[data-slot="tooltip-content"]').filter({ hasText: 'Select' })).toHaveCount(0);
      const rightRailSettings = page.getByTestId('right-rail-settings-trigger');
      await rightRailSettings.click();
      await expect(page.getByTestId('right-rail-settings-popover')).toBeVisible();
      await expect(page.getByTestId('right-rail-expand-on-hover')).toHaveAttribute('data-checked', '');
      await page.keyboard.press('Escape');
      await page.getByTestId('document-viewport').hover();
      await expect(rightRail).not.toHaveAttribute('data-expanded', '');
      await expect.poll(async () => (await rightRail.boundingBox())?.width).toBeLessThanOrEqual(49);

      const browserWindow = await app.browserWindow(page);
      await browserWindow.evaluate((window) => window.webContents.setZoomFactor(2));
      await expect(page.getByTestId('right-rail-overflow-indicator')).toBeVisible();

      const railViewport = page.getByTestId('right-rail-viewport');
      const railViewportBounds = await railViewport.boundingBox();
      expect(railViewportBounds).not.toBeNull();
      await page.mouse.move(
        railViewportBounds.x + railViewportBounds.width / 2,
        railViewportBounds.y + railViewportBounds.height / 2,
      );
      await page.mouse.wheel(0, 240);
      await expect.poll(async () => railViewport.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
    } finally {
      await app.close().catch(() => undefined);
    }
  });
});
