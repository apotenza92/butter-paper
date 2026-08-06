import { expect, test } from '@playwright/test';
import { copyFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { firstWindow, getDiagnostics, launchButterPaper, resolveDesktopEntryPoint } from './helpers/electron.mjs';

const MARKUP_TOOL_TEST_IDS = [
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
const DRAW_TOOL_TEST_IDS = [
  'tool-rectangle',
  'tool-ellipse',
  'tool-line',
  'tool-arc',
  'tool-polyline',
  'tool-polygon',
  'tool-dimension',
];
const MEASURE_TOOL_TEST_IDS = [
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

  test('keeps tools semantically grouped while the rail snaps between flowing column widths', async () => {
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
      const markupGroup = page.getByTestId('right-rail-markup');
      const drawGroup = page.getByTestId('right-rail-draw');
      const measureGroup = page.getByTestId('right-rail-measure');
      const groups = [markupGroup, drawGroup, measureGroup];
      const topControlSlot = page.getByTestId('properties-trigger-slot');
      const propertiesTrigger = page.getByTestId('properties-sidebar-trigger');
      const snapTrigger = page.getByTestId('viewer-snap-target-menu');
      const selectTrigger = page.getByTestId('tool-select');
      const panTrigger = page.getByTestId('tool-pan');
      await expect(rightRail).toHaveAttribute('data-column-count', '2');
      await expect(page.getByTestId('right-rail-general-heading')).toHaveText('General');
      await expect(rightRail).toContainText('Markup');
      await expect(propertiesTrigger).toHaveAccessibleName('Show properties');
      await expect(snapTrigger).toHaveAccessibleName('Snap settings');
      await expect(selectTrigger).toHaveText('');
      await expect(panTrigger).toHaveText('');
      await expect(propertiesTrigger).toBeVisible();
      await expect(snapTrigger).toBeVisible();
      await expect(selectTrigger).toBeVisible();
      await expect(panTrigger).toBeVisible();
      const initialRightRailBounds = await rightRail.boundingBox();
      const initialTopControlSlotBounds = await topControlSlot.boundingBox();
      const initialPropertiesBounds = await propertiesTrigger.boundingBox();
      const initialSnapBounds = await snapTrigger.boundingBox();
      const initialSelectBounds = await selectTrigger.boundingBox();
      const initialPanBounds = await panTrigger.boundingBox();
      const initialMarkupBounds = await markupGroup.boundingBox();
      expect(initialRightRailBounds).not.toBeNull();
      expect(initialTopControlSlotBounds).not.toBeNull();
      expect(initialPropertiesBounds).not.toBeNull();
      expect(initialSnapBounds).not.toBeNull();
      expect(initialSelectBounds).not.toBeNull();
      expect(initialPanBounds).not.toBeNull();
      expect(initialMarkupBounds).not.toBeNull();
      expect(initialSnapBounds.x).toBeGreaterThan(initialPropertiesBounds.x);
      expect(initialSelectBounds.y).toBeGreaterThan(initialPropertiesBounds.y);
      expect(initialPanBounds.x).toBeGreaterThan(initialSelectBounds.x);
      expect(initialMarkupBounds.y).toBeGreaterThan(initialSelectBounds.y);
      for (const [group, testIds] of [
        [markupGroup, MARKUP_TOOL_TEST_IDS],
        [drawGroup, DRAW_TOOL_TEST_IDS],
        [measureGroup, MEASURE_TOOL_TEST_IDS],
      ]) {
        for (const testId of testIds) {
          await expect(group.getByTestId(testId)).toBeVisible();
          for (const otherGroup of groups.filter((candidate) => candidate !== group)) {
            await expect(otherGroup.getByTestId(testId)).toHaveCount(0);
          }
        }
      }
      await expect(selectTrigger).toHaveAccessibleName('Select');
      const rectangleTool = drawGroup.getByTestId('tool-rectangle');
      await expect(rectangleTool).toHaveAccessibleName('Rectangle');
      await rectangleTool.hover();
      await expect(page.locator('[data-slot="tooltip-content"]').filter({ hasText: 'Rectangle (R)' })).toBeVisible();
      await rectangleTool.click();
      await expect(rectangleTool).toHaveAttribute('aria-pressed', 'true');

      const dividers = page.locator('[data-testid^="right-rail-group-divider-"]');
      await expect(dividers).toHaveCount(2);
      for (const [group, heading] of [
        [markupGroup, 'Markup'],
        [drawGroup, 'Draw'],
        [measureGroup, 'Measure'],
      ]) {
        await expect(group.getByRole('heading', { name: heading })).toBeVisible();
      }
      const initialDividerWidths = [];
      for (const divider of await dividers.all()) {
        const dividerBounds = await divider.boundingBox();
        expect(dividerBounds).not.toBeNull();
        expect(dividerBounds.width).toBeGreaterThan(0);
        expect(dividerBounds.width).toBeLessThanOrEqual(initialRightRailBounds.width);
        initialDividerWidths.push(dividerBounds.width);
      }

      const resizeHandle = page.getByTestId('right-rail-resize-handle');
      await resizeHandle.focus();
      await page.keyboard.press('ArrowRight');
      await expect(rightRail).toHaveAttribute('data-column-count', '3');
      const wideRightRailBounds = await rightRail.boundingBox();
      expect(wideRightRailBounds).not.toBeNull();
      expect(wideRightRailBounds.width).toBeGreaterThan(initialRightRailBounds.width);
      const textBoxBounds = await markupGroup.getByTestId('tool-text-box').boundingBox();
      const arrowBounds = await markupGroup.getByTestId('tool-arrow').boundingBox();
      const penBounds = await markupGroup.getByTestId('tool-pen').boundingBox();
      const imageBounds = await markupGroup.getByTestId('tool-image').boundingBox();
      const snapshotBounds = await markupGroup.getByTestId('tool-snapshot').boundingBox();
      expect(textBoxBounds).not.toBeNull();
      expect(arrowBounds).not.toBeNull();
      expect(penBounds).not.toBeNull();
      expect(imageBounds).not.toBeNull();
      expect(snapshotBounds).not.toBeNull();
      expect(arrowBounds.x).toBeGreaterThan(textBoxBounds.x);
      expect(penBounds.x).toBeGreaterThan(arrowBounds.x);
      expect(imageBounds.y).toBeGreaterThan(textBoxBounds.y);
      expect(snapshotBounds.x).toBeGreaterThan(imageBounds.x);
      let dividerIndex = 0;
      for (const divider of await dividers.all()) {
        const widenedDividerBounds = await divider.boundingBox();
        expect(widenedDividerBounds).not.toBeNull();
        expect(widenedDividerBounds.width).toBeGreaterThan(initialDividerWidths[dividerIndex]);
        expect(widenedDividerBounds.width).toBeLessThanOrEqual(wideRightRailBounds.width);
        dividerIndex += 1;
      }

      await resizeHandle.focus();
      await page.keyboard.press('Home');
      await expect(rightRail).toHaveAttribute('data-column-count', '1');
      await page.getByTestId('document-viewport').hover();

      for (const group of groups) {
        await expect(group.getByRole('heading')).toHaveCount(0);
      }
      await expect(dividers).toHaveCount(2);
      await expect(propertiesTrigger).toHaveText('');
      await expect(snapTrigger).toHaveText('');
      await expect(selectTrigger).toHaveText('');
      await expect(panTrigger).toHaveText('');
      const compactTopControlSlotBounds = await topControlSlot.boundingBox();
      const compactPropertiesBounds = await propertiesTrigger.boundingBox();
      const compactSnapBounds = await snapTrigger.boundingBox();
      const compactSelectBounds = await selectTrigger.boundingBox();
      const compactPanBounds = await panTrigger.boundingBox();
      const compactMarkupBounds = await markupGroup.boundingBox();
      const compactRightRailBounds = await rightRail.boundingBox();
      expect(compactRightRailBounds).not.toBeNull();
      expect(compactRightRailBounds.width).toBeLessThan(initialRightRailBounds.width);
      expect(compactTopControlSlotBounds).not.toBeNull();
      expect(compactPropertiesBounds).not.toBeNull();
      expect(compactSnapBounds).not.toBeNull();
      expect(compactSelectBounds).not.toBeNull();
      expect(compactPanBounds).not.toBeNull();
      expect(compactMarkupBounds).not.toBeNull();
      expect(compactSnapBounds.y).toBeGreaterThan(compactPropertiesBounds.y);
      expect(compactSelectBounds.y).toBeGreaterThan(compactSnapBounds.y);
      expect(compactPanBounds.y).toBeGreaterThan(compactSelectBounds.y);
      expect(compactMarkupBounds.y).toBeGreaterThan(compactPropertiesBounds.y);

      await selectTrigger.hover();
      await expect(selectTrigger).not.toContainText('Select');
      await expect(page.locator('[data-slot="tooltip-content"]').filter({ hasText: 'Select' })).toBeVisible();

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
