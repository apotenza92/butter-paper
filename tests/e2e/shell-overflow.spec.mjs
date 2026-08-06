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
      await expect(rightRail).not.toContainText('General');
      await expect(rightRail).toContainText('Markup');
      await expect(propertiesTrigger).toContainText('Properties');
      await expect(snapTrigger).toContainText('Snap');
      await expect(selectTrigger).toHaveText('');
      await expect(panTrigger).toHaveText('');
      await expect(propertiesTrigger).toHaveCSS('border-top-width', '1px');
      await expect(snapTrigger).toHaveCSS('border-top-width', '1px');
      await expect(selectTrigger).toHaveCSS('border-top-width', '1px');
      await expect(panTrigger).toHaveCSS('border-top-width', '1px');
      const initialTopControlSlotBounds = await topControlSlot.boundingBox();
      const initialPropertiesBounds = await propertiesTrigger.boundingBox();
      const initialSnapBounds = await snapTrigger.boundingBox();
      const initialPropertiesIconBounds = await propertiesTrigger.locator('svg').boundingBox();
      const initialPropertiesLabelBounds = await page.getByTestId('properties-sidebar-label').boundingBox();
      const initialSnapIconBounds = await snapTrigger.locator('svg').boundingBox();
      const initialSnapLabelBounds = await page.getByTestId('viewer-snap-label').boundingBox();
      const initialSelectBounds = await selectTrigger.boundingBox();
      const initialPanBounds = await panTrigger.boundingBox();
      const initialSelectIconBounds = await selectTrigger.locator('svg').boundingBox();
      const initialPanIconBounds = await panTrigger.locator('svg').boundingBox();
      const initialMarkupBounds = await markupGroup.boundingBox();
      expect(initialTopControlSlotBounds?.height).toBeCloseTo(168, 0);
      expect(initialPropertiesBounds?.x).toBeCloseTo(initialSnapBounds.x, 0);
      expect(initialPropertiesBounds?.width).toBeCloseTo(initialSnapBounds.width, 0);
      expect(initialSelectBounds?.width).toBeCloseTo(32, 0);
      expect(initialPanBounds?.width).toBeCloseTo(32, 0);
      expect(initialSnapBounds.y - (initialPropertiesBounds.y + initialPropertiesBounds.height)).toBeCloseTo(8, 0);
      expect((initialPropertiesIconBounds.x + initialPropertiesLabelBounds.x + initialPropertiesLabelBounds.width) / 2).toBeCloseTo(initialPropertiesBounds.x + initialPropertiesBounds.width / 2, 0);
      expect((initialSnapIconBounds.x + initialSnapLabelBounds.x + initialSnapLabelBounds.width) / 2).toBeCloseTo(initialSnapBounds.x + initialSnapBounds.width / 2, 0);
      expect(initialSelectIconBounds.x + initialSelectIconBounds.width / 2).toBeCloseTo(initialSelectBounds.x + initialSelectBounds.width / 2, 0);
      expect(initialPanIconBounds.x + initialPanIconBounds.width / 2).toBeCloseTo(initialPanBounds.x + initialPanBounds.width / 2, 0);
      expect(initialSelectBounds.y - (initialSnapBounds.y + initialSnapBounds.height)).toBeCloseTo(8, 0);
      expect(initialPanBounds.y - (initialSelectBounds.y + initialSelectBounds.height)).toBeCloseTo(8, 0);
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
      for (const divider of await dividers.all()) {
        const dividerBounds = await divider.boundingBox();
        expect(dividerBounds?.width).toBeGreaterThanOrEqual(71);
        expect(dividerBounds?.width).toBeLessThanOrEqual(73);
      }

      const resizeHandle = page.getByTestId('right-rail-resize-handle');
      await resizeHandle.focus();
      await page.keyboard.press('ArrowRight');
      await expect(rightRail).toHaveAttribute('data-column-count', '3');
      const widePropertiesBounds = await propertiesTrigger.boundingBox();
      const widePropertiesIconBounds = await propertiesTrigger.locator('svg').boundingBox();
      const widePropertiesLabelBounds = await page.getByTestId('properties-sidebar-label').boundingBox();
      const wideSnapBounds = await snapTrigger.boundingBox();
      const wideSnapIconBounds = await snapTrigger.locator('svg').boundingBox();
      const wideSnapLabelBounds = await page.getByTestId('viewer-snap-label').boundingBox();
      const wideSelectBounds = await selectTrigger.boundingBox();
      const wideSelectIconBounds = await selectTrigger.locator('svg').boundingBox();
      const widePanBounds = await panTrigger.boundingBox();
      const widePanIconBounds = await panTrigger.locator('svg').boundingBox();
      expect((widePropertiesIconBounds.x + widePropertiesLabelBounds.x + widePropertiesLabelBounds.width) / 2).toBeCloseTo(widePropertiesBounds.x + widePropertiesBounds.width / 2, 0);
      expect((wideSnapIconBounds.x + wideSnapLabelBounds.x + wideSnapLabelBounds.width) / 2).toBeCloseTo(wideSnapBounds.x + wideSnapBounds.width / 2, 0);
      expect(wideSelectBounds?.width).toBeCloseTo(32, 0);
      expect(widePanBounds?.width).toBeCloseTo(32, 0);
      expect(wideSelectIconBounds.x + wideSelectIconBounds.width / 2).toBeCloseTo(wideSelectBounds.x + wideSelectBounds.width / 2, 0);
      expect(widePanIconBounds.x + widePanIconBounds.width / 2).toBeCloseTo(widePanBounds.x + widePanBounds.width / 2, 0);
      const textBoxBounds = await markupGroup.getByTestId('tool-text-box').boundingBox();
      const arrowBounds = await markupGroup.getByTestId('tool-arrow').boundingBox();
      const penBounds = await markupGroup.getByTestId('tool-pen').boundingBox();
      const imageBounds = await markupGroup.getByTestId('tool-image').boundingBox();
      const snapshotBounds = await markupGroup.getByTestId('tool-snapshot').boundingBox();
      expect(textBoxBounds).not.toBeNull();
      expect(arrowBounds?.x).toBeGreaterThan(textBoxBounds.x);
      expect(penBounds?.x).toBeGreaterThan(arrowBounds.x);
      expect(imageBounds?.y).toBeGreaterThan(textBoxBounds.y);
      expect(snapshotBounds?.x).toBeGreaterThan(imageBounds.x);
      for (const divider of await dividers.all()) {
        const widenedDividerBounds = await divider.boundingBox();
        expect(widenedDividerBounds?.width).toBeGreaterThanOrEqual(111);
        expect(widenedDividerBounds?.width).toBeLessThanOrEqual(113);
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
      expect(compactTopControlSlotBounds?.height).toBeCloseTo(168, 0);
      expect(compactPropertiesBounds?.y).toBeCloseTo(initialPropertiesBounds.y, 0);
      expect(compactSnapBounds?.y).toBeCloseTo(initialSnapBounds.y, 0);
      expect(compactSelectBounds?.y).toBeCloseTo(initialSelectBounds.y, 0);
      expect(compactPanBounds?.y).toBeCloseTo(initialPanBounds.y, 0);
      expect(compactMarkupBounds?.y).toBeCloseTo(initialMarkupBounds.y, 0);

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
