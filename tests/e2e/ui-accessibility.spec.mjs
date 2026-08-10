import { expect, test } from '@playwright/test';
import {
  firstWindow,
  getDiagnostics,
  launchButterPaper,
  openFixturePdf,
  resolveDesktopEntryPoint,
} from './helpers/electron.mjs';

test.describe('shadcn Base UI shell accessibility', () => {
  test('supports keyboard menus, accessible tooltips, valid tabs, and protected custom icons', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) test.skip(true, 'Desktop app could not be launched');

    const page = await firstWindow(app);
    const menuBarVisual = await page.getByTestId('app-menu-bar').evaluate((element) => {
      const style = getComputedStyle(element);
      return {
        borderBottomWidth: style.borderBottomWidth,
        borderLeftWidth: style.borderLeftWidth,
        borderRightWidth: style.borderRightWidth,
        borderTopWidth: style.borderTopWidth,
        radius: style.borderTopLeftRadius,
      };
    });
    expect(menuBarVisual).toEqual({
      borderBottomWidth: '1px',
      borderLeftWidth: '0px',
      borderRightWidth: '0px',
      borderTopWidth: '0px',
      radius: '0px',
    });
    const fileTrigger = page.getByTestId('menu-trigger-file');
    await fileTrigger.focus();
    await page.keyboard.press('Enter');
    const newPdfItem = page.getByTestId('menu-file-new-pdf');
    const openItem = page.getByTestId('menu-file-open');
    await expect(newPdfItem).toBeVisible();
    await expect(newPdfItem).toHaveRole('menuitem');
    await expect(newPdfItem).toBeFocused();
    const activeToolBeforeMenuShortcut = (await getDiagnostics(page))?.activeTool;
    await page.keyboard.press('r');
    await expect.poll(async () => (await getDiagnostics(page))?.activeTool).toBe(activeToolBeforeMenuShortcut);
    await page.keyboard.press('ArrowDown');
    await expect(openItem).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(openItem).toHaveCount(0);
    await expect(fileTrigger).toBeFocused();

    const appTrigger = page.getByTestId('menu-trigger-butter-paper');
    await appTrigger.click();
    const checkForUpdates = page.getByTestId('menu-check-for-updates');
    await expect(checkForUpdates).toBeVisible();
    await expect(checkForUpdates).toHaveAttribute('aria-disabled', 'true');
    await expect(page.getByTestId('menu-open-release-page')).toBeVisible();
    const quitItem = page.getByTestId('menu-quit');
    await expect(quitItem).toBeVisible();
    await expect(quitItem).toHaveAccessibleName('Quit Butter Paper');
    await expect(quitItem.locator('svg')).toHaveCount(1);
    await expect(page.getByRole('menuitem', { name: /^Version / })).toHaveCount(0);
    const updateFrequency = page.getByTestId('menu-update-frequency');
    await updateFrequency.hover();
    const weeklyFrequency = page.getByTestId('menu-update-frequency-weekly');
    await expect(weeklyFrequency).toBeVisible();
    const frequencyOptions = page.locator('[role="menuitemradio"][data-testid^="menu-update-frequency-"]');
    await expect.poll(async () => frequencyOptions.evaluateAll((elements) => ({
      optionCount: new Set(elements.map((element) => element.getAttribute('data-testid'))).size,
      checkedCount: new Set(elements
        .filter((element) => element.getAttribute('aria-checked') === 'true')
        .map((element) => element.getAttribute('data-testid'))).size,
    }))).toEqual({ optionCount: 8, checkedCount: 1 });
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');

    const { page: documentPage } = await openFixturePdf(app, 'zoom-target');
    const railToggleVisuals = await documentPage.evaluate(() => {
      const pages = document.querySelector('[data-testid="left-rail-pages"]');
      const select = document.querySelector('[data-testid="tool-select"]');
      const pan = document.querySelector('[data-testid="tool-pan"]');
      if (!pages || !select || !pan) return null;
      const read = (element) => {
        const style = getComputedStyle(element);
        return {
          background: style.backgroundColor,
          borderWidth: style.borderTopWidth,
        };
      };
      return { pages: read(pages), select: read(select), pan: read(pan) };
    });
    expect(railToggleVisuals).not.toBeNull();
    expect(railToggleVisuals.pages.borderWidth).toBe('0px');
    expect(railToggleVisuals.select.borderWidth).toBe('0px');
    expect(railToggleVisuals.pan.borderWidth).toBe('0px');
    expect(railToggleVisuals.pages.background).not.toBe(railToggleVisuals.pan.background);
    expect(railToggleVisuals.select.background).not.toBe(railToggleVisuals.pan.background);
    const viewerToolbar = documentPage.getByTestId('viewer-toolbar');
    await expect(viewerToolbar.locator('[data-slot="separator"]')).toHaveCount(0);
    for (const groupName of ['Zoom controls', 'Fit controls', 'Page view controls']) {
      await expect(viewerToolbar.getByRole('group', { name: groupName, exact: true })).toBeVisible();
    }
    await openFixturePdf(app, 'single-page');
    const tablist = documentPage.getByRole('tablist', { name: 'Open documents' });
    await expect(tablist).toBeVisible();
    const zoomTab = documentPage.getByRole('tab', { name: /^zoom-target$/i });
    const singlePageTab = documentPage.getByRole('tab', { name: /^single-page$/i });
    await expect(tablist.getByRole('tab')).toHaveCount(2);
    const adjacentTabGap = await documentPage.evaluate(() => {
      const tabs = Array.from(document.querySelectorAll('[role="tab"]'));
      const first = tabs[0]?.getBoundingClientRect();
      const second = tabs[1]?.getBoundingClientRect();
      return first && second ? second.left - first.right : null;
    });
    expect(adjacentTabGap).toBeCloseTo(8, 0);
    await expect(zoomTab).not.toContainText(/\.pdf/i);
    await expect(singlePageTab).not.toContainText(/\.pdf/i);
    await expect(tablist.locator('[data-domain-ui-exception="closable-document-tab"]')).toHaveCount(2);
    await expect(tablist).toHaveAttribute('data-variant', 'default');
    await expect(singlePageTab).toHaveAttribute('aria-selected', 'true');
    await expect(singlePageTab).toHaveAttribute('data-active', '');
    await expect(zoomTab).not.toHaveAttribute('data-active', '');
    await expect(zoomTab).toHaveAttribute('tabindex', '-1');
    await expect(singlePageTab).toHaveAttribute('tabindex', '0');
    await expect(singlePageTab).toHaveAttribute('aria-controls', 'document-tab-panel');
    await expect(singlePageTab).toHaveAttribute('aria-keyshortcuts', 'Alt+Shift+ArrowLeft Alt+Shift+ArrowRight');
    await expect(documentPage.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'document-tab-trigger-1');
    await expect(singlePageTab.locator('button')).toHaveCount(0);
    const zoomCloseButton = documentPage.getByRole('button', { name: 'Close zoom-target.pdf' });
    const singlePageCloseButton = documentPage.getByRole('button', { name: 'Close single-page.pdf' });
    await expect(zoomCloseButton).toHaveCSS('opacity', '0');
    await expect(singlePageCloseButton).toHaveCSS('opacity', '0');
    await expect(singlePageCloseButton).toHaveAttribute('data-domain-ui-control', 'tab-close');
    const closeControlGeometry = await singlePageCloseButton.evaluate((element) => {
      const control = element.getBoundingClientRect();
      const surfaceElement = element.querySelector('[data-tab-close-surface]');
      const surface = surfaceElement?.getBoundingClientRect();
      const glyph = element.querySelector('svg')?.getBoundingClientRect();
      return {
        controlHeight: control.height,
        controlWidth: control.width,
        glyphHeight: glyph?.height,
        glyphWidth: glyph?.width,
        horizontalOffset: glyph ? Math.abs(control.x + control.width / 2 - (glyph.x + glyph.width / 2)) : null,
        surfaceHeight: surface?.height,
        surfaceRadius: surfaceElement ? Number.parseFloat(getComputedStyle(surfaceElement).borderTopLeftRadius) : null,
        surfaceWidth: surface?.width,
        verticalOffset: glyph ? Math.abs(control.y + control.height / 2 - (glyph.y + glyph.height / 2)) : null,
      };
    });
    expect(closeControlGeometry.controlHeight).toBe(24);
    expect(closeControlGeometry.controlWidth).toBe(24);
    expect(closeControlGeometry.surfaceHeight).toBe(20);
    expect(closeControlGeometry.surfaceWidth).toBe(20);
    expect(closeControlGeometry.glyphHeight).toBe(14);
    expect(closeControlGeometry.glyphWidth).toBe(14);
    expect(closeControlGeometry.horizontalOffset).toBeLessThanOrEqual(0.5);
    expect(closeControlGeometry.verticalOffset).toBeLessThanOrEqual(0.5);
    expect(closeControlGeometry.surfaceRadius).toBeGreaterThan(0);
    expect(closeControlGeometry.surfaceRadius).toBeLessThan(closeControlGeometry.surfaceWidth / 2);
    const tabVisualState = await documentPage.evaluate(() => {
      const activeTab = document.querySelector('[role="tab"][aria-selected="true"]');
      const inactiveTab = document.querySelector('[role="tab"][aria-selected="false"]');
      const activeLabel = activeTab?.querySelector('.bp-document-tab-label');
      const inactiveLabel = inactiveTab?.querySelector('.bp-document-tab-label');
      const actions = document.querySelector('[data-testid="document-tab-actions"]');
      if (!activeTab || !inactiveTab || !activeLabel || !inactiveLabel || !actions) return null;
      const tablistStyle = getComputedStyle(activeTab.closest('[role="tablist"]'));
      const activeStyle = getComputedStyle(activeTab);
      const inactiveStyle = getComputedStyle(inactiveTab);
      return {
        activeBackground: activeStyle.backgroundColor,
        activeUnderlineOpacity: getComputedStyle(activeTab, '::after').opacity,
        activeMask: getComputedStyle(activeLabel).maskImage,
        actionsBackground: getComputedStyle(actions).backgroundColor,
        inactiveBackground: inactiveStyle.backgroundColor,
        inactiveMask: getComputedStyle(inactiveLabel).maskImage,
        paddingRight: activeStyle.paddingRight,
        tablistBackground: tablistStyle.backgroundColor,
        tablistRadius: tablistStyle.borderTopLeftRadius,
      };
    });
    expect(tabVisualState).not.toBeNull();
    expect(tabVisualState.activeBackground).not.toBe(tabVisualState.inactiveBackground);
    expect(tabVisualState.activeUnderlineOpacity).toBe('0');
    expect(tabVisualState.tablistBackground).not.toBe('rgba(0, 0, 0, 0)');
    expect(tabVisualState.actionsBackground).toBe(tabVisualState.tablistBackground);
    expect(tabVisualState.inactiveBackground).toBe(tabVisualState.tablistBackground);
    expect(Number.parseFloat(tabVisualState.tablistRadius)).toBe(0);
    expect(tabVisualState.activeMask).toBe('none');
    expect(tabVisualState.inactiveMask).toBe('none');
    expect(tabVisualState.paddingRight).toBe('6px');
    const tabVerticalAlignment = await documentPage.evaluate(() => {
      const tabBar = document.querySelector('[data-testid="document-tab-bar"]')?.getBoundingClientRect();
      const tabSurface = document.querySelector('[data-testid="document-tab-surface"]')?.getBoundingClientRect();
      const tabList = document.querySelector('[role="tablist"]')?.getBoundingClientRect();
      const openButton = document.querySelector('[data-testid="document-tab-open"]')?.getBoundingClientRect();
      const newPdfButton = document.querySelector('[data-testid="document-tab-new-pdf"]')?.getBoundingClientRect();
      const settingsButton = document.querySelector('[data-testid="document-tab-new-pdf-settings"]')?.getBoundingClientRect();
      const tabBarElement = document.querySelector('[data-testid="document-tab-bar"]');
      if (!tabBar || !tabSurface || !tabList || !openButton || !newPdfButton || !settingsButton || !tabBarElement) return null;
      return {
        barCenter: tabBar.top + tabBar.height / 2,
        borderBottomWidth: getComputedStyle(tabBarElement).borderBottomWidth,
        bottomGap: tabBar.bottom - tabList.bottom,
        newPdfButtonCenter: newPdfButton.top + newPdfButton.height / 2,
        openButtonCenter: openButton.top + openButton.height / 2,
        openButtonHeight: openButton.height,
        settingsButtonCenter: settingsButton.top + settingsButton.height / 2,
        surfaceLeftGap: tabSurface.left - tabBar.left,
        surfaceRightGap: tabBar.right - tabSurface.right,
        tabCenter: tabList.top + tabList.height / 2,
        tabHeight: tabList.height,
        topGap: tabList.top - tabBar.top,
      };
    });
    expect(tabVerticalAlignment).not.toBeNull();
    expect(tabVerticalAlignment.borderBottomWidth).toBe('1px');
    expect(Math.abs(tabVerticalAlignment.tabHeight - tabVerticalAlignment.openButtonHeight)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(tabVerticalAlignment.topGap - tabVerticalAlignment.bottomGap)).toBeLessThanOrEqual(1);
    expect(Math.abs(tabVerticalAlignment.tabCenter - tabVerticalAlignment.barCenter)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(tabVerticalAlignment.openButtonCenter - tabVerticalAlignment.barCenter)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(tabVerticalAlignment.newPdfButtonCenter - tabVerticalAlignment.barCenter)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(tabVerticalAlignment.settingsButtonCenter - tabVerticalAlignment.barCenter)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(tabVerticalAlignment.surfaceLeftGap - 8)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(tabVerticalAlignment.surfaceRightGap - 8)).toBeLessThanOrEqual(0.5);
    await zoomTab.hover();
    await expect(zoomCloseButton).toHaveCSS('opacity', '1');
    expect(await zoomTab.locator('.bp-document-tab-label').evaluate((element) => getComputedStyle(element).maskImage)).not.toBe('none');
    await viewerToolbar.hover();
    await expect(zoomCloseButton).toHaveCSS('opacity', '0');
    await zoomCloseButton.focus();
    await expect(zoomCloseButton).toHaveCSS('opacity', '1');
    await singlePageTab.focus();
    await page.keyboard.press('Alt+Shift+ArrowLeft');
    await expect(documentPage.getByTestId('document-tab-0')).toContainText('single-page');
    await expect(documentPage.getByTestId('document-tab-1')).toContainText('zoom-target');
    await expect(documentPage.getByTestId('document-tab-reorder-status')).toHaveText('Moved single-page.pdf to position 1 of 2.');
    await expect(documentPage.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'document-tab-trigger-0');
    await expect(singlePageTab).toBeFocused();
    await page.keyboard.press('Alt+Shift+ArrowRight');
    await expect(documentPage.getByTestId('document-tab-0')).toContainText('zoom-target');
    await expect(documentPage.getByTestId('document-tab-1')).toContainText('single-page');

    const dragSource = await singlePageTab.boundingBox();
    const dragTarget = await zoomTab.boundingBox();
    expect(dragSource).not.toBeNull();
    expect(dragTarget).not.toBeNull();
    await documentPage.mouse.move(dragSource.x + 16, dragSource.y + dragSource.height / 2);
    await documentPage.mouse.down();
    await documentPage.mouse.move(dragTarget.x + 16, dragTarget.y + dragTarget.height / 2, { steps: 10 });
    const draggingTab = documentPage.locator('[data-domain-ui-exception="closable-document-tab"][data-dragging] [role="tab"]');
    await expect(draggingTab).toHaveCount(1);
    expect(await draggingTab.evaluate((element) => getComputedStyle(element, '::after').opacity)).toBe('0');
    await documentPage.mouse.up();
    await expect(documentPage.getByTestId('document-tab-0')).toContainText('single-page');
    await expect(documentPage.getByTestId('document-tab-1')).toContainText('zoom-target');
    await singlePageTab.focus();
    await page.keyboard.press('Alt+Shift+ArrowRight');
    await expect(documentPage.getByTestId('document-tab-0')).toContainText('zoom-target');
    await expect(documentPage.getByTestId('document-tab-1')).toContainText('single-page');

    await singlePageTab.focus();
    await page.keyboard.press('Home');
    await expect(zoomTab).toBeFocused();
    await expect(zoomTab).toHaveAttribute('aria-selected', 'true');
    await expect(zoomTab).toHaveAttribute('data-active', '');
    await expect(singlePageTab).not.toHaveAttribute('data-active', '');
    await expect(zoomTab).toHaveAttribute('tabindex', '0');
    await expect(singlePageTab).toHaveAttribute('tabindex', '-1');
    await page.keyboard.press('End');
    await expect(singlePageTab).toBeFocused();
    await page.keyboard.press('ArrowLeft');
    await expect(zoomTab).toBeFocused();
    await zoomCloseButton.click();
    await expect(singlePageTab).toBeFocused();
    await expect(documentPage.getByRole('button', { name: 'Close zoom-target.pdf' })).toHaveCount(0);

    const openButton = documentPage.getByTestId('document-tab-open');
    const newPdfButton = documentPage.getByTestId('document-tab-new-pdf');
    const settingsButton = documentPage.getByTestId('document-tab-new-pdf-settings');
    const actionGroup = documentPage.getByRole('group', { name: 'Document actions' });
    await expect(actionGroup).toBeVisible();
    const tabActionPlacement = await documentPage.evaluate(() => {
      const tablist = document.querySelector('[role="tablist"][aria-label="Open documents"]')?.getBoundingClientRect();
      const separator = document.querySelector('[data-testid="document-tab-actions-separator"]')?.getBoundingClientRect();
      const open = document.querySelector('[data-testid="document-tab-open"]')?.getBoundingClientRect();
      const newPdf = document.querySelector('[data-testid="document-tab-new-pdf"]')?.getBoundingClientRect();
      if (!tablist || !separator || !open || !newPdf) return null;
      return {
        blankAfterOpen: newPdf.left - open.right,
        separatorAfterTabs: separator.left - tablist.right,
        openAfterSeparator: open.left - separator.right,
      };
    });
    expect(tabActionPlacement).not.toBeNull();
    expect(tabActionPlacement.separatorAfterTabs).toBeCloseTo(8, 0);
    expect(tabActionPlacement.openAfterSeparator).toBeCloseTo(8, 0);
    expect(tabActionPlacement.blankAfterOpen).toBeCloseTo(8, 0);
    expect(Math.abs(tabActionPlacement.separatorAfterTabs - tabActionPlacement.openAfterSeparator)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(tabActionPlacement.openAfterSeparator - tabActionPlacement.blankAfterOpen)).toBeLessThanOrEqual(0.5);
    const splitButtonGeometry = await documentPage.evaluate(() => {
      const main = document.querySelector('[data-testid="document-tab-new-pdf"]');
      const settings = document.querySelector('[data-testid="document-tab-new-pdf-settings"]');
      if (!main || !settings) return null;
      const mainBounds = main.getBoundingClientRect();
      const settingsBounds = settings.getBoundingClientRect();
      const mainStyle = getComputedStyle(main);
      const settingsStyle = getComputedStyle(settings);
      return {
        gap: settingsBounds.left - mainBounds.right,
        heightDifference: Math.abs(mainBounds.height - settingsBounds.height),
        mainRightRadius: Number.parseFloat(mainStyle.borderTopRightRadius),
        settingsLeftRadius: Number.parseFloat(settingsStyle.borderTopLeftRadius),
      };
    });
    expect(splitButtonGeometry).not.toBeNull();
    expect(Math.abs(splitButtonGeometry.gap)).toBeLessThanOrEqual(0.5);
    expect(splitButtonGeometry.heightDifference).toBeLessThanOrEqual(0.5);
    expect(splitButtonGeometry.mainRightRadius).toBe(0);
    expect(splitButtonGeometry.settingsLeftRadius).toBe(0);
    await documentPage.getByRole('button', { name: 'Close single-page.pdf' }).focus();
    await page.keyboard.press('Tab');
    await expect(openButton).toBeFocused();
    await expect(openButton).toHaveAccessibleName('Open PDF');
    await page.keyboard.press('Tab');
    await expect(newPdfButton).toBeFocused();
    await expect(newPdfButton).toHaveAccessibleName('New blank PDF using A3 · Landscape');
    await expect(documentPage.getByTestId('document-tab-new-pdf-tooltip')).toContainText('New blank PDF');
    await page.keyboard.press('Tab');
    await expect(settingsButton).toBeFocused();
    await expect(settingsButton).toHaveAccessibleName('Blank PDF settings');
    await page.keyboard.press('Enter');
    await expect(documentPage.getByTestId('new-blank-pdf-settings')).toBeVisible();
    await expect(documentPage.getByTestId('new-blank-pdf-paper-size')).toHaveValue('a3');
    await expect(documentPage.getByTestId('new-blank-pdf-landscape')).toHaveAttribute('aria-pressed', 'true');
    await page.keyboard.press('Escape');
    await expect(settingsButton).toBeFocused();

    const thumbnailPreview = documentPage.getByTestId('page-thumbnail-preview-1');
    const thumbnailLabel = documentPage.getByTestId('page-thumbnail-item-1').getByText('Page 1', { exact: true });
    const previewBounds = await thumbnailPreview.boundingBox();
    const labelBounds = await thumbnailLabel.boundingBox();
    expect(previewBounds).not.toBeNull();
    expect(labelBounds).not.toBeNull();
    expect(labelBounds.y + labelBounds.height).toBeLessThanOrEqual(previewBounds.y);

    for (const testId of ['icon-fit-width', 'icon-fit-page', 'icon-continuous-view']) {
      const icon = documentPage.getByTestId(testId);
      const primaryGlyph = await icon.evaluate((element) => element instanceof SVGElement) ? icon : icon.locator('svg').first();
      const primaryGlyphBounds = await primaryGlyph.boundingBox();
      expect(primaryGlyphBounds, `${testId} primary glyph should render`).not.toBeNull();
      expect(primaryGlyphBounds.width, `${testId} primary glyph width`).toBeGreaterThanOrEqual(15.5);
      expect(primaryGlyphBounds.width, `${testId} primary glyph width`).toBeLessThanOrEqual(16.5);
      expect(primaryGlyphBounds.height, `${testId} primary glyph height`).toBeGreaterThanOrEqual(15.5);
      expect(primaryGlyphBounds.height, `${testId} primary glyph height`).toBeLessThanOrEqual(16.5);
    }

    for (const testId of ['icon-fit-width', 'icon-fit-page']) {
      await expect(documentPage.getByTestId(testId).locator('svg')).toHaveCount(0);
    }

    for (const [testId, overlaySize] of [['tool-cloud-plus', 7], ['tool-callout', 6]]) {
      const glyphs = documentPage.getByTestId(testId).locator('svg');
      const primaryBounds = await glyphs.first().boundingBox();
      const overlayBounds = await glyphs.last().boundingBox();
      expect(primaryBounds, `${testId} primary glyph should render`).not.toBeNull();
      expect(primaryBounds.width).toBeGreaterThanOrEqual(15.5);
      expect(primaryBounds.width).toBeLessThanOrEqual(16.5);
      expect(primaryBounds.height).toBeGreaterThanOrEqual(15.5);
      expect(primaryBounds.height).toBeLessThanOrEqual(16.5);
      expect(overlayBounds, `${testId} overlay glyph should render`).not.toBeNull();
      expect(overlayBounds.width).toBeGreaterThanOrEqual(overlaySize - 0.5);
      expect(overlayBounds.width).toBeLessThanOrEqual(overlaySize + 0.5);
      expect(overlayBounds.height).toBeGreaterThanOrEqual(overlaySize - 0.5);
      expect(overlayBounds.height).toBeLessThanOrEqual(overlaySize + 0.5);
    }

    const continuousTrigger = documentPage.getByTestId('viewer-scroll-continuous');
    const continuousSettingsTrigger = documentPage.getByTestId('viewer-scroll-continuous-settings');
    await expect(continuousTrigger).toHaveAttribute('aria-pressed', 'true');
    await expect(continuousSettingsTrigger).not.toHaveAttribute('aria-pressed', /.*/);
    await continuousSettingsTrigger.click();
    await expect(continuousSettingsTrigger).toHaveAttribute('aria-expanded', 'true');
    await expect(continuousTrigger).toHaveAttribute('aria-pressed', 'true');
    await page.keyboard.press('Escape');
    await expect(continuousSettingsTrigger).toBeFocused();

    await documentPage.evaluate(() => window.__butterPaperTestHooks?.setZoom(1.25));
    await expect.poll(async () => (await getDiagnostics(documentPage))?.zoomPreset).toBe('manual');
    await continuousTrigger.click();
    await expect(documentPage.getByTestId('viewer-toolbar-hint-continuous')).toHaveText('Double click to Fit Width');
    await documentPage.waitForTimeout(2_100);
    await expect(documentPage.getByTestId('viewer-toolbar-hint-continuous')).toHaveCount(0);
    await expect(documentPage.getByTestId('viewer-scroll-continuous-tooltip')).toHaveCount(0);
    await continuousTrigger.dblclick();
    await expect(documentPage.getByTestId('viewer-toolbar-hint-continuous')).toHaveCount(0);
    await expect(documentPage.getByTestId('viewer-scroll-continuous-tooltip')).toHaveCount(0);
    await expect.poll(async () => (await getDiagnostics(documentPage))?.zoomPreset).toBe('fit-width');

    const singlePageTrigger = documentPage.getByTestId('viewer-scroll-single-page');
    await singlePageTrigger.click();
    await expect.poll(async () => (await getDiagnostics(documentPage))?.scrollMode).toBe('single-page');
    await expect(documentPage.getByTestId('viewer-toolbar-hint-single-page')).toHaveText('Double click to Fit Page');
    await documentPage.evaluate(() => window.__butterPaperTestHooks?.setZoom(1.25));
    await singlePageTrigger.dblclick();
    await expect.poll(async () => (await getDiagnostics(documentPage))?.zoomPreset).toBe('fit-page');

    const fitWidthTrigger = documentPage.getByTestId('viewer-fit-width');
    await fitWidthTrigger.click();
    await expect(documentPage.getByTestId('viewer-toolbar-hint-fit-width')).toHaveText('Double click to view Continuous');
    await fitWidthTrigger.dblclick();
    await expect.poll(async () => (await getDiagnostics(documentPage))?.scrollMode).toBe('continuous');

    const fitPageTrigger = documentPage.getByTestId('viewer-fit-page');
    await fitPageTrigger.click();
    await expect(documentPage.getByTestId('viewer-toolbar-hint-fit-page')).toHaveText('Double click to view Single Page');
    await fitPageTrigger.dblclick();
    await expect.poll(async () => (await getDiagnostics(documentPage))?.scrollMode).toBe('single-page');

    const cadTrigger = documentPage.getByTestId('viewer-cad-view');
    const cadSettingsTrigger = documentPage.getByTestId('viewer-cad-view-settings');
    await expect(cadTrigger).toHaveAttribute('aria-pressed', 'false');
    await cadSettingsTrigger.click();
    await expect(cadSettingsTrigger).toHaveAttribute('aria-expanded', 'true');
    await expect(cadTrigger).toHaveAttribute('aria-pressed', 'false');
    await page.keyboard.press('Escape');
    await expect(cadSettingsTrigger).toBeFocused();
    await cadTrigger.click();
    await expect(cadTrigger).toHaveAttribute('aria-pressed', 'true');
    await cadSettingsTrigger.click();
    const columnsToggle = documentPage.getByTestId('viewer-cad-organisation-columns');
    const rowsToggle = documentPage.getByTestId('viewer-cad-organisation-rows');
    await expect(columnsToggle).toBeVisible();
    await columnsToggle.focus();
    await page.keyboard.press('ArrowRight');
    await expect(rowsToggle).toBeFocused();
    await expect(rowsToggle).toHaveAttribute('aria-pressed', 'true');
    await expect(documentPage.getByText('Mousewheel always zooms in CAD View.')).toBeVisible();
    await expect(documentPage.getByTestId('viewer-cad-wheel-zoom')).toHaveCount(0);
    await expect(documentPage.getByTestId('viewer-cad-wheel-scroll')).toHaveCount(0);
    const activeToolBeforePopoverShortcut = (await getDiagnostics(documentPage))?.activeTool;
    await page.keyboard.press('r');
    await expect.poll(async () => (await getDiagnostics(documentPage))?.activeTool).toBe(activeToolBeforePopoverShortcut);
    await page.keyboard.press('Escape');
    await expect(cadSettingsTrigger).toBeFocused();

    await app.close();
  });

  test('contains the toolbar and portal menus at 200 percent zoom', async () => {
    test.skip(!resolveDesktopEntryPoint(), 'Desktop app entrypoint not available yet');
    const app = await launchButterPaper({ theme: 'dark' });
    if (!app) test.skip(true, 'Desktop app could not be launched');

    const { page } = await openFixturePdf(app, 'zoom-target');
    await expect.poll(async () => (await getDiagnostics(page))?.pageCount).toBeGreaterThan(0);
    const splitSurfaceState = async () => page.evaluate(() => {
      const background = (testId) => {
        const element = document.querySelector(`[data-testid="${testId}"]`);
        return element ? getComputedStyle(element).backgroundColor : null;
      };
      return {
        addTab: background('document-tab-open'),
        blankPdf: background('document-tab-new-pdf'),
        blankPdfSettings: background('document-tab-new-pdf-settings'),
        continuous: background('viewer-scroll-continuous'),
        continuousSettings: background('viewer-scroll-continuous-settings'),
        singlePage: background('viewer-scroll-single-page'),
        singlePageSettings: background('viewer-scroll-single-page-settings'),
        rootChildren: document.querySelector('#root')?.childElementCount ?? 0,
      };
    });
    const initialSplitSurfaces = await splitSurfaceState();
    expect(initialSplitSurfaces.blankPdf).toBe(initialSplitSurfaces.addTab);
    expect(initialSplitSurfaces.blankPdfSettings).toBe(initialSplitSurfaces.addTab);
    expect(initialSplitSurfaces.continuousSettings).toBe(initialSplitSurfaces.continuous);
    expect(initialSplitSurfaces.singlePageSettings).toBe(initialSplitSurfaces.singlePage);

    const blankPdfSettings = page.getByTestId('document-tab-new-pdf-settings');
    await blankPdfSettings.click();
    await expect(page.getByTestId('new-blank-pdf-settings')).toBeVisible();
    expect((await splitSurfaceState()).blankPdfSettings).toBe(initialSplitSurfaces.addTab);
    await page.keyboard.press('Escape');
    await expect(blankPdfSettings).toHaveAttribute('aria-expanded', 'false');

    const continuousSettings = page.getByTestId('viewer-scroll-continuous-settings');
    await continuousSettings.click();
    await expect(page.locator('[data-slot="dropdown-menu-content"]')).toContainText('Mousewheel Behaviour');
    expect((await splitSurfaceState()).continuousSettings).toBe(initialSplitSurfaces.continuous);
    await page.keyboard.press('Escape');
    await expect(continuousSettings).toHaveAttribute('aria-expanded', 'false');
    expect((await splitSurfaceState()).rootChildren).toBeGreaterThan(0);

    const browserWindow = await app.browserWindow(page);
    await browserWindow.evaluate((window) => window.webContents.setZoomFactor(2));

    await expect.poll(async () => page.evaluate(() => document.body.scrollWidth <= window.innerWidth)).toBe(true);
    const toolbar = page.getByTestId('viewer-toolbar');
    await expect.poll(async () => toolbar.evaluate((element) => element.scrollWidth > element.clientWidth)).toBe(true);

    const snapTrigger = page.getByTestId('viewer-snap-target-menu');
    await snapTrigger.evaluate((element) => element.scrollIntoView({ block: 'nearest', inline: 'nearest' }));
    await expect(snapTrigger).toHaveAccessibleName('Snap settings');
    await snapTrigger.click();
    const snapItem = page.getByTestId('viewer-snap-content');
    await expect(snapItem).toBeVisible();
    await expect(snapItem).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByTestId('viewer-snap-content-check')).toBeVisible();
    const nearestItem = page.getByTestId('viewer-snap-target-nearest');
    await expect(nearestItem).toHaveAttribute('aria-pressed', 'false');
    await expect(page.getByTestId('viewer-snap-target-nearest-check')).not.toBeVisible();
    const activeBackground = await snapItem.evaluate((element) => getComputedStyle(element).backgroundColor);
    const inactiveBackground = await nearestItem.evaluate((element) => getComputedStyle(element).backgroundColor);
    expect(activeBackground).not.toBe(inactiveBackground);
    const popup = page.getByTestId('viewer-snap-popover');
    const popupBounds = await popup.boundingBox();
    const snapTriggerBounds = await snapTrigger.boundingBox();
    const viewport = await page.evaluate(() => ({ width: window.innerWidth, height: window.innerHeight }));
    expect(popupBounds).not.toBeNull();
    expect(snapTriggerBounds).not.toBeNull();
    await expect(popup).toHaveAttribute('data-side', 'left');
    expect(popupBounds.x + popupBounds.width).toBeLessThanOrEqual(snapTriggerBounds.x + 1);
    expect(popupBounds.x).toBeGreaterThanOrEqual(0);
    expect(popupBounds.y).toBeGreaterThanOrEqual(0);
    expect(popupBounds.x + popupBounds.width).toBeLessThanOrEqual(viewport.width + 1);
    expect(popupBounds.y + popupBounds.height).toBeLessThanOrEqual(viewport.height + 1);

    await browserWindow.evaluate((window) => window.webContents.setZoomFactor(1));
    await app.close();
  });

});
