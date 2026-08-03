import { expect, test } from '@playwright/test';
import { getDiagnostics, launchButterPaper, openFixturePdf, resolveDesktopEntryPoint } from './helpers/electron.mjs';

test.describe('viewer page layout controls', () => {
  test('supports page columns, overview labels, single-page mode, and split view controls', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await waitForDiagnostics(page, {
      pageCount: 6,
      scrollMode: 'continuous',
      zoomPreset: 'fit-width',
      pageColumnsEnabled: false,
      pagesPerColumn: 10,
    });
    await expectViewButtonWidthsToMatch(page);

    await enableColumns(page, 2);
    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(0.1));

    await waitForDiagnostics(page, {
      pageColumnsEnabled: true,
      pagesPerColumn: 2,
      zoomPreset: 'manual',
      scrollWheelMode: 'zoom',
    });
    await expectViewButtonWidthsToMatch(page);
    await expect(page.getByTestId('viewer-fit-width')).toBeDisabled();
    await expect(page.getByTestId('viewer-fit-page')).toBeDisabled();
    await expect.poll(async () => page.locator('[data-overview-tile="true"]').count()).toBeGreaterThan(0);
    await expect(page.getByTestId('page-1')).toHaveAttribute('data-current-page', 'true');

    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(0.8));
    await page.getByTestId('document-viewport').evaluate((element) => element.scrollTo({ left: 0, top: 0 }));
    const beforeColumnWheelDiagnostics = await getDiagnostics(page);
    await page.getByTestId('document-viewport').evaluate((element) => {
      element.dispatchEvent(new WheelEvent('wheel', {
        bubbles: true,
        cancelable: true,
        deltaY: 900,
      }));
    });
    await expect.poll(async () => {
      const diagnostics = await getDiagnostics(page);
      return diagnostics?.zoom ?? beforeColumnWheelDiagnostics?.zoom ?? 0;
    }).toBeLessThan((beforeColumnWheelDiagnostics?.zoom ?? 0.8) - 0.05);
    await expect.poll(async () => (await getDiagnostics(page))?.currentPage).toBe(beforeColumnWheelDiagnostics?.currentPage ?? 0);
    await openCadViewSettings(page);
    await expect(page.getByText('Mousewheel always zooms in CAD View.')).toBeVisible();
    await expect(page.getByTestId('viewer-cad-wheel-scroll')).toHaveCount(0);
    await expect(page.getByTestId('viewer-cad-wheel-zoom')).toHaveCount(0);
    await page.keyboard.press('Escape');
    await waitForDiagnostics(page, {
      pageColumnsEnabled: true,
      scrollWheelMode: 'zoom',
    });
    const beforeCadCtrlWheelDiagnostics = await getDiagnostics(page);
    await page.getByTestId('document-viewport').evaluate((element) => {
      element.dispatchEvent(new WheelEvent('wheel', {
        bubbles: true,
        cancelable: true,
        ctrlKey: true,
        deltaY: 900,
      }));
    });
    await expect.poll(async () => {
      const diagnostics = await getDiagnostics(page);
      return diagnostics?.zoom ?? beforeCadCtrlWheelDiagnostics?.zoom ?? 0;
    }).toBeLessThan((beforeCadCtrlWheelDiagnostics?.zoom ?? 0.8) - 0.05);
    await page.getByTestId('page-3').click();
    await waitForDiagnostics(page, {
      currentPage: 2,
      pageColumnsEnabled: true,
      pagesPerColumn: 2,
    });
    await expect(page.getByTestId('page-3')).toHaveAttribute('data-current-page', 'true');

    await openCadViewSettings(page);
    await page.getByTestId('viewer-cad-organisation-rows').click();
    await page.getByTestId('viewer-pages-per-row').fill('3');
    await page.keyboard.press('Escape');
    await waitForDiagnostics(page, {
      cadViewOrganisation: 'rows',
      pagesPerColumn: 3,
    });

    await page.getByTestId('viewer-scroll-single-page').click();
    await waitForDiagnostics(page, { scrollMode: 'single-page' });
    await expect.poll(async () => page.locator('[data-page-index]').count()).toBe(1);

    await page.getByTestId('page-thumbnail-item-3').click();
    await waitForDiagnostics(page, {
      currentPage: 2,
      scrollMode: 'single-page',
    });
    await expect(page.getByTestId('page-3')).toBeVisible();
    await expect(page.getByTestId('page-1')).toHaveCount(0);

    await page.getByTestId('viewer-fit-page').click();
    await waitForDiagnostics(page, {
      scrollMode: 'single-page',
      zoomPreset: 'fit-page',
    });

    await page.getByTestId('viewer-fit-width').click();
    await waitForDiagnostics(page, {
      scrollMode: 'single-page',
      zoomPreset: 'fit-width',
    });

    await page.getByTestId('viewer-scroll-continuous').click();
    await waitForDiagnostics(page, {
      scrollMode: 'continuous',
      zoomPreset: 'fit-width',
    });

    const singlePageSettings = page.getByTestId('viewer-scroll-single-page-settings');
    await singlePageSettings.click();
    await expect(singlePageSettings).toHaveAttribute('aria-expanded', 'true');
    await page.getByTestId('viewer-single-page-wheel-scroll').click();
    await page.keyboard.press('Escape');
    await waitForDiagnostics(page, {
      scrollMode: 'continuous',
      singlePageScrollWheelMode: 'scroll',
    });
    await expect(singlePageSettings).toBeFocused();

    await app.close();
  });

  test('uses lightweight overview tiles for zoomed-out page columns', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await enableColumns(page, 2);
    await page.evaluate(() => {
      window.__butterPaperTestHooks?.resetPerfSnapshot();
      window.__butterPaperTestHooks?.setZoom(0.05);
    });
    await waitForDiagnostics(page, {
      pageColumnsEnabled: true,
      pagesPerColumn: 2,
      zoomPreset: 'manual',
    });

    await expect.poll(async () => page.locator('[data-overview-tile="true"]').count()).toBeGreaterThan(0);
    const perf = await page.evaluate(() => window.__butterPaperTestHooks?.getPerfSnapshot());
    expect(perf?.renderPage.requests ?? 0).toBe(0);

    await page.evaluate(() => {
      window.__butterPaperTestHooks?.resetPerfSnapshot();
      window.__butterPaperTestHooks?.setZoom(0.8);
    });
    await expect.poll(async () => page.locator('[data-overview-tile="true"]').count()).toBe(0);

    await app.close();
  });

  test.skip('deactivates fit presets during middle-button pan and preserves them on double click', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await waitForDiagnostics(page, {
      scrollMode: 'continuous',
      zoomPreset: 'fit-width',
    });

    await middlePanViewport(page);
    await waitForDiagnostics(page, { zoomPreset: 'manual' });

    await page.getByTestId('viewer-fit-page').click();
    await waitForDiagnostics(page, { zoomPreset: 'fit-page' });
    await middlePanViewport(page);
    await waitForDiagnostics(page, { zoomPreset: 'manual' });

    await page.getByTestId('viewer-fit-page').click();
    await waitForDiagnostics(page, { zoomPreset: 'fit-page' });
    await middleDoubleClickViewport(page);
    await waitForDiagnostics(page, { zoomPreset: 'fit-page' });

    await page.getByTestId('viewer-fit-width').click();
    await waitForDiagnostics(page, { zoomPreset: 'fit-width' });
    await middleDoubleClickViewport(page);
    await waitForDiagnostics(page, { zoomPreset: 'fit-width' });

    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(1.15));
    await waitForDiagnostics(page, { zoomPreset: 'manual' });
    await middleDoubleClickViewport(page);
    await waitForDiagnostics(page, { zoomPreset: 'fit-width' });

    await app.close();
  });

  test.skip('allows middle-button panning in single-page fit page view', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await page.getByTestId('viewer-scroll-single-page').click();
    await page.getByTestId('viewer-fit-page').click();
    await waitForDiagnostics(page, {
      scrollMode: 'single-page',
      zoomPreset: 'fit-page',
    });
    const before = await measurePagePosition(page, 0);

    await middlePanViewport(page);
    await waitForDiagnostics(page, {
      scrollMode: 'single-page',
      zoomPreset: 'fit-page',
    });
    const after = await measurePagePosition(page, 0);

    expect(Math.abs(after.top - before.top) + Math.abs(after.left - before.left)).toBeGreaterThan(8);

    await app.close();
  });

  test.skip('updates fit presets from the page under the viewport', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'mixed-page-sizes');
    await waitForDiagnostics(page, {
      pageCount: 3,
      scrollMode: 'continuous',
      zoomPreset: 'fit-width',
    });
    await expect.poll(async () => (await getDiagnostics(page))?.zoom ?? 0).toBeGreaterThan(1.2);
    const firstPageFitWidthZoom = (await getDiagnostics(page))?.zoom ?? 1;

    await scrollViewportToRatio(page, 0.22);
    await expect.poll(async () => (await getDiagnostics(page))?.zoom ?? 0).toBeLessThan(firstPageFitWidthZoom - 0.05);
    const secondPageFitWidthAnchor = await measureViewportCentrePageAnchor(page);
    const secondPageFitWidthZoom = (await getDiagnostics(page))?.zoom ?? firstPageFitWidthZoom;

    await page.getByTestId('viewer-fit-page').click();
    await waitForDiagnostics(page, { zoomPreset: 'fit-page' });
    await expectCentreAnchorClose(page, secondPageFitWidthAnchor);
    const secondPageFitPageZoom = (await getDiagnostics(page))?.zoom ?? secondPageFitWidthZoom;

    await scrollViewportToRatio(page, 0.72);
    await expect.poll(async () => (await getDiagnostics(page))?.zoom ?? 0).toBeLessThan(secondPageFitPageZoom - 0.05);

    await app.close();
  });

  test.skip('preserves the viewport centre anchor across view mode changes', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(0.8));
    await waitForDiagnostics(page, {
      scrollMode: 'continuous',
      zoomPreset: 'manual',
    });

    await scrollPageToViewportCentre(page, 2);
    await expect.poll(async () => (await measureViewportCentrePageAnchor(page)).pageIndex).toBe(2);
    const initialAnchor = await measureViewportCentrePageAnchor(page);

    await page.getByTestId('viewer-scroll-single-page').click();
    await waitForDiagnostics(page, {
      currentPage: initialAnchor.pageIndex,
      scrollMode: 'single-page',
    });
    await expectCentreAnchorClose(page, initialAnchor);

    await page.getByTestId('viewer-scroll-continuous').click();
    await waitForDiagnostics(page, {
      pageColumnsEnabled: false,
      scrollMode: 'continuous',
    });
    await expectCentreAnchorClose(page, initialAnchor);

    await page.getByTestId('viewer-cad-view').click();
    await waitForDiagnostics(page, {
      pageColumnsEnabled: true,
      scrollMode: 'continuous',
    });
    await expectCentreAnchorClose(page, initialAnchor);

    await openCadViewSettings(page);
    await page.getByTestId('viewer-cad-organisation-rows').click();
    await page.getByTestId('viewer-pages-per-row').fill('3');
    await page.keyboard.press('Escape');
    await waitForDiagnostics(page, {
      cadViewOrganisation: 'rows',
      pageColumnsEnabled: true,
      pagesPerColumn: 3,
    });
    await expectCentreAnchorClose(page, initialAnchor, { ratioX: false });

    await app.close();
  });

  test.skip('preserves the viewport anchor when entering column view', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(0.8));
    await expect.poll(async () => page.locator('[data-render-quality]').count()).toBeGreaterThanOrEqual(3);

    const targetIndex = 2;
    await page.evaluate((index) => {
      const viewport = document.querySelector('[data-testid="document-viewport"]');
      const target = document.querySelector(`[data-page-index="${index}"]`);
      if (!(viewport instanceof HTMLElement) || !(target instanceof HTMLElement)) {
        return;
      }
      viewport.scrollTo({ left: 0, top: Math.max(0, target.offsetTop - 120), behavior: 'auto' });
    }, targetIndex);
    await page.getByTestId(`page-${targetIndex + 1}`).click({ position: { x: 20, y: 20 } });
    await waitForDiagnostics(page, { currentPage: targetIndex });

    const before = await measurePagePosition(page, targetIndex);
    await enableColumns(page, 2);
    await waitForDiagnostics(page, {
      currentPage: targetIndex,
      pageColumnsEnabled: true,
      pagesPerColumn: 2,
    });

    await expect.poll(async () => {
      const after = await measurePagePosition(page, targetIndex);
      return Math.abs(after.top - before.top);
    }).toBeLessThan(1);
    await expect.poll(async () => {
      const after = await measurePagePosition(page, targetIndex);
      return Math.abs(after.centreY - before.centreY);
    }).toBeLessThan(1);

    await app.close();
  });

  test.skip('preserves the viewport anchor when leaving column view and changing column count', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(0.8));
    await enableColumns(page, 2);
    await waitForDiagnostics(page, {
      pageColumnsEnabled: true,
      pagesPerColumn: 2,
    });

    const targetIndex = 2;
    await scrollPageNearViewportTop(page, targetIndex, { left: true });
    await page.getByTestId(`page-${targetIndex + 1}`).click({ position: { x: 20, y: 20 } });
    await waitForDiagnostics(page, { currentPage: targetIndex });

    const beforeColumnCount = await measurePagePosition(page, targetIndex);
    await setPagesPerColumn(page, 3);
    await waitForDiagnostics(page, {
      currentPage: targetIndex,
      pageColumnsEnabled: true,
      pagesPerColumn: 3,
    });
    const afterColumnCount = await measurePagePosition(page, targetIndex);
    expect(Math.abs(afterColumnCount.top - beforeColumnCount.top)).toBeLessThan(10);
    expect(Math.abs(afterColumnCount.centreY - beforeColumnCount.centreY)).toBeLessThan(10);

    const beforeLeavingColumns = await measurePagePosition(page, targetIndex);
    await disableColumns(page);
    await waitForDiagnostics(page, {
      currentPage: targetIndex,
      pageColumnsEnabled: false,
    });
    const afterLeavingColumns = await measurePagePosition(page, targetIndex);
    expect(Math.abs(afterLeavingColumns.top - beforeLeavingColumns.top)).toBeLessThan(10);
    expect(Math.abs(afterLeavingColumns.centreY - beforeLeavingColumns.centreY)).toBeLessThan(10);

    await app.close();
  });

  test.skip('preserves the centre page point when zooming a scrolled column view', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(0.8));
    await enableColumns(page, 2);
    await waitForDiagnostics(page, {
      pageColumnsEnabled: true,
      pagesPerColumn: 2,
    });

    const targetIndex = 2;
    await scrollPageNearViewportTop(page, targetIndex, { left: true });
    await page.getByTestId(`page-${targetIndex + 1}`).click({ position: { x: 20, y: 20 } });
    await waitForDiagnostics(page, { currentPage: targetIndex });

    const before = await measureViewportCentrePageAnchor(page);
    expect(before.pageIndex).toBe(targetIndex);
    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(1.05));
    await waitForDiagnostics(page, {
      currentPage: targetIndex,
      zoomPreset: 'manual',
    });
    const after = await measureViewportCentrePageAnchor(page);
    expect(after.pageIndex).toBe(targetIndex);
    expect(Math.abs(after.ratioX - before.ratioX)).toBeLessThan(0.002);
    expect(Math.abs(after.ratioY - before.ratioY)).toBeLessThan(0.015);

    await app.close();
  });

  test('keeps column pages visually populated during zoom and pan motion', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await enableColumns(page, 2);
    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(0.8));
    await waitForDiagnostics(page, {
      pageColumnsEnabled: true,
      pagesPerColumn: 2,
      zoomPreset: 'manual',
    });

    await expect.poll(async () => page.locator('[data-render-quality]').count()).toBeGreaterThanOrEqual(4);
    await expect(page.locator('[data-render-placeholder="page"]')).toHaveCount(0);

    await page.evaluate(() => window.__butterPaperTestHooks?.resetPerfSnapshot());
    const viewport = page.getByTestId('document-viewport');
    for (const zoom of [0.55, 1.0, 0.45, 0.9]) {
      await page.evaluate((nextZoom) => window.__butterPaperTestHooks?.setZoom(nextZoom), zoom);
      await viewport.evaluate((element) => {
        element.scrollBy({ left: 160, top: 140, behavior: 'auto' });
      });
      await page.waitForTimeout(80);
      await expect(page.locator('[data-render-placeholder="page"]')).toHaveCount(0);
    }

    const motionPerf = await page.evaluate(() => window.__butterPaperTestHooks?.getPerfSnapshot());
    expect(motionPerf?.placeholderShows.page ?? 0).toBe(0);
    await page.waitForTimeout(350);
    await expect.poll(async () => countBrokenVisiblePageImages(page)).toBe(0);
    await expect(page.locator('[data-render-state="error"]')).toHaveCount(0);

    await page.evaluate(() => {
      window.__butterPaperTestHooks?.resetPerfSnapshot();
      window.__butterPaperTestHooks?.setZoom(0.05);
    });
    await expect.poll(async () => page.locator('[data-overview-tile="true"]').count()).toBeGreaterThan(0);
    const overviewPerf = await page.evaluate(() => window.__butterPaperTestHooks?.getPerfSnapshot());
    expect(overviewPerf?.renderPage.requests ?? 0).toBe(0);
    await expect.poll(async () => countBrokenVisiblePageImages(page)).toBe(0);

    await app.close();
  });

  test('does not change the active page while panning continuous or column views', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await waitForDiagnostics(page, {
      pageCount: 6,
      currentPage: 0,
      scrollMode: 'continuous',
      pageColumnsEnabled: false,
    });

    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(1));
    await waitForDiagnostics(page, { zoomPreset: 'manual' });
    await page.getByTestId('tool-pan').click();
    const viewport = page.getByTestId('document-viewport');
    const continuousViewportBox = await viewport.boundingBox();
    expect(continuousViewportBox).not.toBeNull();

    await drag(
      page,
      continuousViewportBox.x + continuousViewportBox.width * 0.5,
      continuousViewportBox.y + continuousViewportBox.height * 0.75,
      continuousViewportBox.x + continuousViewportBox.width * 0.5,
      continuousViewportBox.y + continuousViewportBox.height * 0.15,
    );
    await page.waitForTimeout(120);
    await expect.poll(async () => (await getDiagnostics(page))?.currentPage).toBe(0);

    await enableColumns(page, 2);
    await page.evaluate(() => window.__butterPaperTestHooks?.setZoom(0.8));
    await waitForDiagnostics(page, {
      currentPage: 0,
      pageColumnsEnabled: true,
      pagesPerColumn: 2,
      zoomPreset: 'manual',
    });
    const columnViewportBox = await viewport.boundingBox();
    expect(columnViewportBox).not.toBeNull();

    await drag(
      page,
      columnViewportBox.x + columnViewportBox.width * 0.7,
      columnViewportBox.y + columnViewportBox.height * 0.7,
      columnViewportBox.x + columnViewportBox.width * 0.25,
      columnViewportBox.y + columnViewportBox.height * 0.2,
    );
    await page.waitForTimeout(120);
    await expect.poll(async () => (await getDiagnostics(page))?.currentPage).toBe(0);

    await app.close();
  });
});

async function waitForDiagnostics(page, expected) {
  await expect.poll(async () => {
    const diagnostics = await getDiagnostics(page);
    return {
      pageCount: diagnostics?.pageCount ?? 0,
      currentPage: diagnostics?.currentPage ?? 0,
      scrollMode: diagnostics?.scrollMode ?? 'continuous',
      zoomPreset: diagnostics?.zoomPreset ?? 'manual',
      pageColumnsEnabled: diagnostics?.pageColumnsEnabled ?? false,
      scrollWheelMode: diagnostics?.scrollWheelMode ?? 'scroll',
      cadViewOrganisation: diagnostics?.cadViewOrganisation ?? 'columns',
      pagesPerColumn: diagnostics?.pagesPerColumn ?? 10,
    };
  }).toMatchObject(expected);
}

async function countBrokenVisiblePageImages(page) {
  return await page.locator('[data-page-index] img').evaluateAll((images) => {
    return images.filter((image) => {
      const element = image;
      const bounds = element.getBoundingClientRect();
      return bounds.width > 0
        && bounds.height > 0
        && element.offsetParent !== null
        && element.naturalWidth === 0;
    }).length;
  });
}

async function expectViewButtonWidthsToMatch(page) {
  const widths = await Promise.all([
    getControlWidth(page, 'viewer-scroll-continuous'),
    getControlWidth(page, 'viewer-scroll-single-page'),
    getControlWidth(page, 'viewer-cad-view'),
  ]);
  expect(new Set(widths).size).toBe(1);
}

async function getControlWidth(page, testId) {
  const box = await page.getByTestId(testId).boundingBox();
  expect(box).not.toBeNull();
  return Math.round(box.width);
}

async function enableColumns(page, pagesPerColumn) {
  await activateCadView(page);
  await openCadViewSettings(page);
  await page.getByTestId('viewer-cad-organisation-columns').click();
  await page.getByTestId('viewer-pages-per-column').fill(String(pagesPerColumn));
  await page.keyboard.press('Escape');
}

async function disableColumns(page) {
  await page.getByTestId('viewer-scroll-continuous').click();
}

async function setPagesPerColumn(page, pagesPerColumn) {
  await openCadViewSettings(page);
  await page.getByTestId('viewer-pages-per-column').fill(String(pagesPerColumn));
  await page.keyboard.press('Escape');
}

async function activateCadView(page) {
  const diagnostics = await getDiagnostics(page);
  if (diagnostics?.pageColumnsEnabled && diagnostics?.scrollMode === 'continuous') {
    return;
  }

  await page.getByTestId('viewer-cad-view').click();
  await waitForDiagnostics(page, { scrollMode: 'continuous', pageColumnsEnabled: true });
}

async function openCadViewSettings(page) {
  const settings = page.getByTestId('viewer-cad-settings');
  const openSettings = page.locator('[data-testid="viewer-cad-settings"][data-open]');
  const columnsButton = page.getByTestId('viewer-cad-organisation-columns');
  const rowsButton = page.getByTestId('viewer-cad-organisation-rows');
  if (await openSettings.isVisible().catch(() => false)) {
    return;
  }

  await expect(settings).toHaveCount(0);
  await page.getByTestId('viewer-cad-view-settings').click();
  await expect(openSettings).toBeVisible();
  await expect(columnsButton).toBeVisible();
  await expect(rowsButton).toBeVisible();
}

async function getViewportScroll(page) {
  return await page.getByTestId('document-viewport').evaluate((element) => ({
    left: element.scrollLeft,
    top: element.scrollTop,
  }));
}

async function measurePagePosition(page, pageIndex) {
  return await page.evaluate((index) => {
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    const target = document.querySelector(`[data-page-index="${index}"]`);
    if (!(viewport instanceof HTMLElement) || !(target instanceof HTMLElement)) {
      throw new Error(`Page ${index + 1} is not mounted`);
    }
    const viewportRect = viewport.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    return {
      left: targetRect.left - viewportRect.left,
      top: targetRect.top - viewportRect.top,
      centreX: targetRect.left + targetRect.width / 2 - viewportRect.left,
      centreY: targetRect.top + targetRect.height / 2 - viewportRect.top,
    };
  }, pageIndex);
}

async function scrollPageNearViewportTop(page, pageIndex, options = {}) {
  await page.evaluate(({ pageIndex: index, alignLeft }) => {
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    const target = document.querySelector(`[data-page-index="${index}"]`);
    if (!(viewport instanceof HTMLElement) || !(target instanceof HTMLElement)) {
      return;
    }

    viewport.scrollTo({
      left: alignLeft ? Math.max(0, target.offsetLeft - 120) : viewport.scrollLeft,
      top: Math.max(0, target.offsetTop - 120),
      behavior: 'auto',
    });
  }, { pageIndex, alignLeft: Boolean(options.left) });
}

async function scrollPageToViewportCentre(page, pageIndex) {
  await page.evaluate((index) => {
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    const target = document.querySelector(`[data-page-index="${index}"]`);
    if (!(viewport instanceof HTMLElement) || !(target instanceof HTMLElement)) {
      return;
    }

    viewport.scrollTo({
      left: Math.max(0, target.offsetLeft + target.offsetWidth / 2 - viewport.clientWidth / 2),
      top: Math.max(0, target.offsetTop + target.offsetHeight / 2 - viewport.clientHeight / 2),
      behavior: 'auto',
    });
  }, pageIndex);
}

async function scrollViewportToRatio(page, ratio) {
  await page.getByTestId('document-viewport').evaluate((element, nextRatio) => {
    const maxScrollTop = Math.max(0, element.scrollHeight - element.clientHeight);
    element.scrollTo({
      left: 0,
      top: maxScrollTop * nextRatio,
      behavior: 'auto',
    });
  }, ratio);
}

async function measureViewportCentrePageAnchor(page) {
  return await page.evaluate(() => {
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    if (!(viewport instanceof HTMLElement)) {
      throw new Error('Document viewport is not mounted');
    }

    const viewportRect = viewport.getBoundingClientRect();
    const x = viewportRect.left + viewportRect.width / 2;
    const y = viewportRect.top + viewportRect.height / 2;
    const target = document.elementFromPoint(x, y)?.closest('[data-page-index]');
    if (!(target instanceof HTMLElement)) {
      throw new Error('No page under viewport centre');
    }

    const targetRect = target.getBoundingClientRect();
    return {
      pageIndex: Number(target.getAttribute('data-page-index')),
      ratioX: (x - targetRect.left) / targetRect.width,
      ratioY: (y - targetRect.top) / targetRect.height,
    };
  });
}

async function expectCentreAnchorClose(page, expected, options = {}) {
  const tolerance = options.tolerance ?? 0.035;
  const actual = await measureViewportCentrePageAnchor(page);
  expect(actual.pageIndex).toBe(expected.pageIndex);
  if (options.ratioX !== false) {
    expect(Math.abs(actual.ratioX - expected.ratioX)).toBeLessThan(tolerance);
  }
  if (options.ratioY !== false) {
    expect(Math.abs(actual.ratioY - expected.ratioY)).toBeLessThan(tolerance);
  }
}

async function drag(page, startX, startY, endX, endY) {
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(endX, endY, { steps: 12 });
  await page.mouse.up();
}

async function middlePanViewport(page) {
  const box = await page.getByTestId('document-viewport').boundingBox();
  expect(box).not.toBeNull();
  const startX = box.x + box.width * 0.5;
  const startY = box.y + box.height * 0.5;
  await page.mouse.move(startX, startY);
  await page.mouse.down({ button: 'middle' });
  await page.mouse.move(startX + 24, startY + 18, { steps: 4 });
  await page.mouse.up({ button: 'middle' });
}

async function middleDoubleClickViewport(page) {
  const box = await page.getByTestId('document-viewport').boundingBox();
  expect(box).not.toBeNull();
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + box.height * 0.5, {
    button: 'middle',
  });
}
