import { expect, test } from '@playwright/test';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { createCanvas, loadImage } from '@napi-rs/canvas';
import { closeButterPaperDiscardingUnsaved, firstWindow, getDiagnostics, launchButterPaper, openFixturePdf, resolveDesktopEntryPoint, saveCurrentDocumentAs } from './helpers/electron.mjs';

const LAUNCH_BUDGET_MS = 45_000;
const OPEN_BUDGET_MS = 45_000;
const PAGE_JUMP_BUDGET_MS = 12_000;
const MAX_RENDER_CACHE_ENTRIES = 8;
const MAX_RENDER_CACHE_BYTES = 120 * 1024 * 1024;
const MAX_THUMBNAIL_CACHE_ENTRIES = 24;
const MAX_THUMBNAIL_CACHE_BYTES = 24 * 1024 * 1024;

test.describe('Butter Paper electron workflows', () => {
  test('launches the sidebar shell chrome', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const launchStartedAt = performance.now();
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const page = await firstWindow(app);
    const launchElapsedMs = performance.now() - launchStartedAt;
    await expect(page).toHaveTitle(/Butter Paper|Butter/i);
    expect(launchElapsedMs).toBeLessThan(LAUNCH_BUDGET_MS);
    await waitForShellDiagnostics(page, { themeMode: 'light' });

    await expect(page.getByTestId('app-menu-bar')).toBeVisible();
    await expect(page.getByTestId('menu-trigger-butter-paper')).toBeVisible();
    await expect(page.getByTestId('menu-trigger-file')).toBeVisible();
    await expect(page.getByTestId('menu-trigger-edit')).toHaveCount(0);
    await expect(page.getByTestId('menu-trigger-view')).toHaveCount(0);
    await expect(page.getByTestId('menu-trigger-document')).toHaveCount(0);
    await expect(page.getByTestId('document-tab-bar')).toBeVisible();
    await expect(page.getByTestId('viewer-toolbar')).toBeVisible();
    await expect(page.getByTestId('page-thumbnail-set-scale-1')).toHaveCount(0);
    await expect(page.getByTestId('left-rail')).toBeVisible();
    await expect(page.getByTestId('right-rail')).toBeVisible();
    await expectShellSizing(page);
    await expect(page.getByTestId('left-rail-pages')).toHaveAttribute('aria-label', 'Page Thumbnails');
    await expect(page.getByTestId('tool-select')).toHaveAttribute('aria-label', 'Select');
    await expect(page.getByTestId('tool-pan')).toHaveAttribute('aria-label', 'Pan');
    await expect(page.getByTestId('tool-rectangle')).toHaveAttribute('aria-label', 'Rectangle');
    await page.getByTestId('menu-trigger-file').click();
    await expect(page.getByTestId('menu-file-open')).toBeVisible();
    await expect(page.getByTestId('menu-file-save')).toBeDisabled();
    await expect(page.getByTestId('menu-file-save-as')).toBeDisabled();
    await page.locator('body').click({ position: { x: 6, y: 6 } });

    const openStartedAt = performance.now();
    const fixture = await openFixturePdf(app, 'multi-page');
    const { page: fixturePage } = fixture;

    await waitForShellDiagnostics(fixturePage, {
      leftSidebarOpen: true,
      rightSidebarOpen: false,
      scrollMode: 'continuous',
      zoomPreset: 'fit-width',
      pageCount: 6,
    });
    await waitForRenderDiagnostics(fixturePage, {
      pageRenderReady: true,
      lastPageRenderError: null,
      lastThumbnailRenderError: null,
    });
    await expect(fixturePage.getByTestId('page-thumbnail-set-scale-1')).toBeEnabled();
    const openElapsedMs = performance.now() - openStartedAt;
    expect(openElapsedMs).toBeLessThan(OPEN_BUDGET_MS);

    await expect(fixturePage.getByTestId('left-sidebar')).toBeVisible();
    await expect(fixturePage.getByTestId('page-thumbnail-list')).toBeVisible();
    await expect(fixturePage.getByTestId('page-thumbnail-item-1')).toBeVisible();
    await expect(fixturePage.getByTestId('page-thumbnail-item-2')).toBeVisible();
    await expect(fixturePage.locator('[data-testid="page-1"] img, [data-testid="page-1"] canvas').first()).toBeVisible();
    await waitForRenderedThumbnail(fixturePage, 1);
    await waitForRenderedThumbnail(fixturePage, 2);
    await expectCacheBudgets(fixturePage);
    await expectShellSizing(fixturePage);

    const pageJumpStartedAt = performance.now();
    await fixturePage.getByTestId('page-thumbnail-item-3').click();
    await waitForShellDiagnostics(fixturePage, {
      currentPage: 2,
    });
    const pageJumpElapsedMs = performance.now() - pageJumpStartedAt;
    expect(pageJumpElapsedMs).toBeLessThan(PAGE_JUMP_BUDGET_MS);
    await expectCacheBudgets(fixturePage);

    await fixturePage.getByTestId('tool-rectangle').click();
    await waitForShellDiagnostics(fixturePage, { activeTool: 'rectangle', rightSidebarOpen: false });
    await expect(fixturePage.getByTestId('right-sidebar')).toHaveCount(0);
    await expectSidebarHeaders(fixturePage, { rightSidebarVisible: false });

    await app.close();
  });

  test('creates, moves, saves, and reopens markups with the new shell', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const launchStartedAt = performance.now();
    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }
    const launchElapsedMs = performance.now() - launchStartedAt;
    expect(launchElapsedMs).toBeLessThan(LAUNCH_BUDGET_MS);

    const openStartedAt = performance.now();
    const { page } = await openFixturePdf(app, 'zoom-target');
    await waitForShellDiagnostics(page, {
      pageCount: 1,
      leftSidebarOpen: true,
      rightSidebarOpen: false,
      scrollMode: 'continuous',
      zoomPreset: 'fit-width',
    });
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      lastPageRenderError: null,
      lastThumbnailRenderError: null,
    });
    const openElapsedMs = performance.now() - openStartedAt;
    expect(openElapsedMs).toBeLessThan(OPEN_BUDGET_MS);
    await expectCacheBudgets(page);
    await waitForShellDiagnostics(page, { themeMode: 'light' });

    await expect(page.getByTestId('menu-trigger-file')).toBeVisible();
    await expect(page.getByTestId('document-tab-bar')).toBeVisible();
    await expect(page.getByTestId('viewer-toolbar')).toBeVisible();
    await expect(page.getByTestId('left-rail')).toBeVisible();
    await expect(page.getByTestId('right-rail')).toBeVisible();
    await expectShellSizing(page);
    await expectSidebarHeaders(page, { rightSidebarVisible: false });
    await expect(page.getByTestId('left-rail-pages')).toHaveAttribute('aria-label', 'Page Thumbnails');
    await expect(page.getByTestId('tool-select')).toHaveAttribute('aria-label', 'Select');
    await expect(page.getByTestId('tool-pan')).toHaveAttribute('aria-label', 'Pan');
    await expect(page.getByTestId('tool-rectangle')).toHaveAttribute('aria-label', 'Rectangle');
    await expect(page.getByTestId('left-sidebar')).toBeVisible();
    await expect(page.getByTestId('page-thumbnail-list')).toBeVisible();
    await expect(page.locator('[data-testid="page-1"] img, [data-testid="page-1"] canvas').first()).toBeVisible();
    await waitForRenderedThumbnail(page, 1);
    await expect(page.locator('[data-testid="document-viewport"]')).not.toContainText('Unable to render page');
    await expect(page.locator('[data-testid="left-sidebar"]')).not.toContainText('Preview unavailable');

    const initialDiagnostics = await getDiagnostics(page);
    const initialMarkupCount = initialDiagnostics?.markupCount ?? 0;
    const pageCanvas = page.locator('[data-testid="annotation-layer-1"]');

    await page.getByTestId('tool-rectangle').click();
    await waitForShellDiagnostics(page, { activeTool: 'rectangle', rightSidebarOpen: false });
    const rectangleDraftBox = await pageCanvas.boundingBox();
    expect(rectangleDraftBox).not.toBeNull();
    const rectangleCanvasBox = rectangleDraftBox;
    const rectangleViewportBox = await page.locator('[data-testid="document-viewport"]').boundingBox();
    expect(rectangleViewportBox).not.toBeNull();
    const rectangleStartY = Math.max(rectangleCanvasBox.y + 90, rectangleViewportBox.y + 90);
    await page.mouse.click(rectangleCanvasBox.x + 80, rectangleStartY);
    await page.mouse.click(rectangleCanvasBox.x + 220, rectangleStartY + 80);
    await waitForMarkupDiagnostics(page, { markupCount: initialMarkupCount + 1 });
    await expect(page.locator('[data-testid="thumbnail-annotation-layer-1"] [data-testid^="thumbnail-markup-"] rect').first()).toBeVisible();

    await page.mouse.click(rectangleCanvasBox.x + 300, rectangleStartY + 180);
    await waitForMarkupDiagnostics(page, { markupCount: initialMarkupCount + 1 });
    await expect(pageCanvas.locator('[data-interaction-state="focused"]')).toHaveCount(0);

    await page.getByTestId('tool-select').click();
    const rectangle = page.locator('[data-testid^="markup-rect-"]').first();
    const rectangleContent = rectangle.locator(':scope > g').first().locator('rect').first();
    const rectangleBox = await rectangleContent.boundingBox();
    expect(rectangleBox).not.toBeNull();
    await page.mouse.move(rectangleBox.x + 2, rectangleBox.y + rectangleBox.height * 0.35);
    await expect(rectangle.locator('[data-interaction-state="hovered"]')).toBeVisible();
    const eastHandle = rectangle.locator('[data-handle-id="rectangle.resize.e"]');
    await expect(eastHandle).toHaveAttribute('fill', '#fef08a');
    const eastHandleBox = await eastHandle.boundingBox();
    expect(eastHandleBox).not.toBeNull();
    await page.mouse.move(eastHandleBox.x + eastHandleBox.width / 2, eastHandleBox.y + eastHandleBox.height / 2);
    await expect(eastHandle).toHaveAttribute('data-handle-state', 'hot');
    await expect(eastHandle).toHaveAttribute('fill', '#facc15');
    await drag(
      page,
      eastHandleBox.x + eastHandleBox.width / 2,
      eastHandleBox.y + eastHandleBox.height / 2,
      eastHandleBox.x + eastHandleBox.width / 2 + 32,
      eastHandleBox.y + eastHandleBox.height / 2,
    );
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toHaveCount(0);
    await expect(rectangle.locator('[data-interaction-state="hovered"]')).toBeVisible();
    const resizedRectangleBox = await rectangleContent.boundingBox();
    expect(resizedRectangleBox).not.toBeNull();
    expect(resizedRectangleBox.width).toBeGreaterThan(rectangleBox.width);

    const resizedEastHandleBox = await eastHandle.boundingBox();
    expect(resizedEastHandleBox).not.toBeNull();
    await page.mouse.click(
      resizedEastHandleBox.x + resizedEastHandleBox.width / 2,
      resizedEastHandleBox.y + resizedEastHandleBox.height / 2,
    );
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toBeVisible();

    const selectionTogglePoint = {
      x: resizedRectangleBox.x + 2,
      y: resizedRectangleBox.y + resizedRectangleBox.height * 0.35,
    };
    await page.mouse.move(selectionTogglePoint.x, selectionTogglePoint.y);
    await page.keyboard.down('Shift');
    await expect(page.getByTestId('selection-cursor-hint')).toHaveCount(0);
    await page.mouse.click(selectionTogglePoint.x, selectionTogglePoint.y);
    await page.keyboard.up('Shift');
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toHaveCount(0);

    await page.keyboard.down('Shift');
    await page.mouse.click(selectionTogglePoint.x, selectionTogglePoint.y);
    await page.keyboard.up('Shift');
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toBeVisible();

    const emptySelectionPoint = {
      x: rectangleCanvasBox.x + 300,
      y: rectangleStartY + 180,
    };
    await page.mouse.click(emptySelectionPoint.x, emptySelectionPoint.y);
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toHaveCount(0);
    await page.keyboard.press('Escape');

    const windowStart = { x: resizedRectangleBox.x - 24, y: resizedRectangleBox.y - 24 };
    const windowEnd = {
      x: resizedRectangleBox.x + resizedRectangleBox.width + 24,
      y: resizedRectangleBox.y + resizedRectangleBox.height + 24,
    };
    await page.mouse.click(windowStart.x, windowStart.y);
    await page.mouse.move(windowEnd.x, windowEnd.y, { steps: 5 });
    const windowMarquee = page.getByTestId('selection-marquee');
    await expect(windowMarquee).toHaveAttribute('data-selection-kind', 'window');
    await expect(windowMarquee).toHaveAttribute('data-selection-shape', 'box');
    await expect(windowMarquee).toHaveAttribute('data-selection-operation', 'replace');
    await expect(windowMarquee).toHaveAttribute('stroke', '#2563eb');
    await expect(windowMarquee).not.toHaveAttribute('stroke-dasharray');
    await expect(page.getByTestId('selection-cursor-hint')).toHaveCount(0);
    await expect(page.getByTestId('selection-marquee-operation')).toHaveCount(0);
    await page.mouse.click(windowEnd.x, windowEnd.y);
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toBeVisible();

    await page.mouse.click(emptySelectionPoint.x, emptySelectionPoint.y);
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toHaveCount(0);
    await page.keyboard.press('Escape');
    const crossingStart = {
      x: resizedRectangleBox.x + resizedRectangleBox.width + 24,
      y: resizedRectangleBox.y - 12,
    };
    const crossingEnd = {
      x: resizedRectangleBox.x + resizedRectangleBox.width - 2,
      y: resizedRectangleBox.y + resizedRectangleBox.height + 12,
    };
    await page.mouse.move(crossingStart.x, crossingStart.y);
    await page.mouse.down();
    await page.mouse.move(crossingStart.x, crossingEnd.y, { steps: 3 });
    await page.mouse.move(crossingEnd.x, crossingEnd.y, { steps: 3 });
    await page.mouse.move(crossingEnd.x, crossingStart.y, { steps: 3 });
    const crossingMarquee = page.getByTestId('selection-marquee');
    await expect(crossingMarquee).toHaveAttribute('data-selection-kind', 'crossing');
    await expect(crossingMarquee).toHaveAttribute('data-selection-shape', 'lasso');
    await expect(crossingMarquee).toHaveAttribute('data-selection-operation', 'replace');
    await expect(crossingMarquee).toHaveAttribute('stroke', '#22c55e');
    await expect(crossingMarquee).toHaveAttribute('stroke-dasharray', '7 5');
    await page.mouse.up();
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toBeVisible();

    await page.keyboard.down('Alt');
    await page.mouse.click(windowStart.x, windowStart.y);
    await page.keyboard.up('Alt');
    await page.mouse.move(windowEnd.x, windowEnd.y, { steps: 5 });
    await expect(page.getByTestId('selection-marquee')).toHaveAttribute('data-selection-operation', 'remove');
    await page.mouse.click(windowEnd.x, windowEnd.y);
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toHaveCount(0);

    await page.keyboard.down('Shift');
    await page.mouse.click(windowStart.x, windowStart.y);
    await page.keyboard.up('Shift');
    await page.mouse.move(windowEnd.x, windowEnd.y, { steps: 5 });
    await expect(page.getByTestId('selection-marquee')).toHaveAttribute('data-selection-operation', 'add');
    await page.mouse.click(windowEnd.x, windowEnd.y);
    await expect(rectangle.locator('[data-interaction-state="focused"]')).toBeVisible();

    const rectBounds = resizedRectangleBox;
    await drag(
      page,
      rectBounds.x + 2,
      rectBounds.y + rectBounds.height / 2,
      rectBounds.x + 42,
      rectBounds.y + rectBounds.height / 2 + 24,
    );
    await expectCacheBudgets(page);

    const tempDir = await mkdtemp(join(tmpdir(), 'butter-paper-e2e-'));
    const savePath = join(tempDir, 'annotated.pdf');
    await saveCurrentDocumentAs(page, savePath);

    await page.evaluate(async ({ savePath: nextPath }) => {
      await window.__butterPaperTestHooks?.openDocumentPath(nextPath);
    }, { savePath });

    await waitForShellDiagnostics(page, {
      pageCount: 1,
      leftSidebarOpen: true,
      rightSidebarOpen: false,
      scrollMode: 'continuous',
      zoomPreset: 'fit-width',
    });
    await expect.poll(async () => {
      const diagnostics = await getDiagnostics(page);
      return diagnostics?.markupCount ?? 0;
    }).toBeGreaterThanOrEqual(initialMarkupCount + 1);
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      lastPageRenderError: null,
      lastThumbnailRenderError: null,
    });
    await expectCacheBudgets(page);
    await expect(page.locator('[data-testid^="markup-rect-"]')).toBeVisible();

    await app.close();
  });

  test('shows created markups in page thumbnails', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'zoom-target');
    await waitForShellDiagnostics(page, {
      pageCount: 1,
      leftSidebarOpen: true,
      rightSidebarOpen: false,
      scrollMode: 'continuous',
      zoomPreset: 'fit-width',
    });
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      lastPageRenderError: null,
      lastThumbnailRenderError: null,
    });
    await waitForRenderedThumbnail(page, 1);

    const initialDiagnostics = await getDiagnostics(page);
    const initialMarkupCount = initialDiagnostics?.markupCount ?? 0;
    const pageCanvasBox = await page.locator('[data-testid="annotation-layer-1"]').boundingBox();
    expect(pageCanvasBox).not.toBeNull();

    await page.getByTestId('tool-rectangle').click();
    const documentViewportBox = await page.locator('[data-testid="document-viewport"]').boundingBox();
    expect(documentViewportBox).not.toBeNull();
    const startY = Math.max(pageCanvasBox.y + 90, documentViewportBox.y + 90);
    await page.mouse.click(pageCanvasBox.x + 80, startY);
    await page.mouse.click(pageCanvasBox.x + 220, startY + 80);
    await waitForMarkupDiagnostics(page, { markupCount: initialMarkupCount + 1 });

    const thumbnailMarkupRect = page.locator('[data-testid="thumbnail-annotation-layer-1"] [data-testid^="thumbnail-markup-"] rect').first();
    await expect(thumbnailMarkupRect).toBeVisible();

    await closeButterPaperDiscardingUnsaved(app, page);
  });

  test('places text boxes from a caret-first provisional editor', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'zoom-target');
    await waitForShellDiagnostics(page, { pageCount: 1, activeTool: 'select' });
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      lastPageRenderError: null,
      lastThumbnailRenderError: null,
    });

    const initialDiagnostics = await getDiagnostics(page);
    const initialMarkupCount = initialDiagnostics?.markupCount ?? 0;
    const annotationLayer = page.getByTestId('annotation-layer-1');
    const layerBox = await annotationLayer.boundingBox();
    expect(layerBox).not.toBeNull();
    const viewportBox = await page.getByTestId('document-viewport').boundingBox();
    expect(viewportBox).not.toBeNull();
    const insertionPoint = {
      x: layerBox.x + 110,
      y: Math.max(layerBox.y + 120, viewportBox.y + 120),
    };

    await page.getByTestId('tool-text-box').click();
    await page.mouse.move(insertionPoint.x, insertionPoint.y);
    await expect.poll(async () => annotationLayer.evaluate((element) => getComputedStyle(element).cursor)).toContain('url(');
    await expect(page.getByTestId('text-placement-preview')).toHaveCount(0);
    await expect(annotationLayer).not.toContainText('Text Box');

    await page.mouse.click(insertionPoint.x, insertionPoint.y);
    const editor = page.locator('[data-testid^="text-box-editor-"] textarea');
    await expect(editor).toBeFocused();
    await expect(editor).toHaveValue('');
    await expect(page.getByTestId('text-placement-preview')).toHaveCount(0);
    await expect(annotationLayer).toHaveCSS('cursor', 'default');
    await expect(annotationLayer.locator('[data-interaction-state="focused"] rect').first()).toBeVisible();
    await expect(editor).toHaveCSS('overflow', 'hidden');
    await expect(editor).toHaveCSS('color', 'rgba(0, 0, 0, 0)');
    await expect(page.getByTestId('text-box-editing-caret')).toHaveCount(1);
    await waitForMarkupDiagnostics(page, { markupCount: initialMarkupCount });

    const emptyEditorBox = await editor.boundingBox();
    await editor.fill('First');
    const firstLineBox = await editor.boundingBox();
    expect(firstLineBox?.width ?? 0).toBeGreaterThan(emptyEditorBox?.width ?? 0);
    await editor.press('Enter');
    await editor.type('Second');
    await expect(editor).toHaveValue('First\nSecond');
    await expect(annotationLayer).toContainText('FirstSecond');
    const secondLineBox = await editor.boundingBox();
    expect(secondLineBox?.height ?? 0).toBeGreaterThan(firstLineBox?.height ?? 0);
    expect(secondLineBox?.y).toBeCloseTo(firstLineBox?.y ?? 0, 1);
    const editingTextBox = await annotationLayer.locator('g[data-testid^="markup-text-"] text').last().boundingBox();

    await editor.press('Escape');
    await expect(editor).toHaveCount(0);
    await waitForMarkupDiagnostics(page, { markupCount: initialMarkupCount + 1 });
    await waitForShellDiagnostics(page, { activeTool: 'text-box' });
    await expect(annotationLayer.locator('[data-interaction-state="focused"]')).toHaveCount(0);
    const finalizedTextBox = await annotationLayer.locator('g[data-testid^="markup-text-"] text').last().boundingBox();
    expect(finalizedTextBox?.x).toBeCloseTo(editingTextBox?.x ?? 0, 1);
    expect(finalizedTextBox?.y).toBeCloseTo(editingTextBox?.y ?? 0, 1);
    expect(finalizedTextBox?.width).toBeCloseTo(editingTextBox?.width ?? 0, 1);
    expect(finalizedTextBox?.height).toBeCloseTo(editingTextBox?.height ?? 0, 1);

    await page.mouse.click(insertionPoint.x + 80, insertionPoint.y + 80);
    const blankEditor = page.locator('[data-testid^="text-box-editor-"] textarea');
    await expect(blankEditor).toBeFocused();
    await blankEditor.press('Escape');
    await waitForMarkupDiagnostics(page, { markupCount: initialMarkupCount + 1 });

    await page.mouse.click(insertionPoint.x + 120, insertionPoint.y + 120);
    const clickAwayEditor = page.locator('[data-testid^="text-box-editor-"] textarea');
    await clickAwayEditor.fill('Click away');
    await page.mouse.click(insertionPoint.x + 220, insertionPoint.y + 160);
    await expect(clickAwayEditor).toHaveCount(0);
    await waitForMarkupDiagnostics(page, { markupCount: initialMarkupCount + 2 });
    await expect(page.locator('[data-testid^="text-box-editor-"] textarea')).toHaveCount(0);
    await expect.poll(async () => annotationLayer.evaluate((element) => getComputedStyle(element).cursor)).toContain('url(');
    await expect(page.getByTestId('text-placement-preview')).toHaveCount(0);
    await expect(annotationLayer.locator('[data-testid^="markup-text-"] [data-interaction-state="focused"]')).toHaveCount(0);

    await page.mouse.move(10, 10);
    const liveText = annotationLayer.locator('g[data-testid^="markup-text-"] text').filter({ hasText: 'FirstSecond' }).first();
    const liveBounds = await liveText.boundingBox();
    expect(liveBounds).not.toBeNull();
    const comparisonClip = paddedScreenshotClip(liveBounds, 4);
    const livePixels = await page.screenshot({ clip: comparisonClip });

    const tempDir = await mkdtemp(join(tmpdir(), 'butter-paper-text-parity-'));
    const savePath = join(tempDir, 'text-parity.pdf');
    await saveCurrentDocumentAs(page, savePath);
    await page.evaluate(async ({ path }) => {
      await window.__butterPaperTestHooks?.openDocumentPath(path);
    }, { path: savePath });
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      lastPageRenderError: null,
      lastThumbnailRenderError: null,
    });
    const reopenedText = page.getByTestId('annotation-layer-1').locator('g[data-testid^="markup-text-"] text').filter({ hasText: 'FirstSecond' }).first();
    await expect(reopenedText).toBeVisible();
    const reopenedBounds = await reopenedText.boundingBox();
    expect(reopenedBounds?.x).toBeCloseTo(liveBounds.x, 1);
    expect(reopenedBounds?.y).toBeCloseTo(liveBounds.y, 1);
    expect(reopenedBounds?.width).toBeCloseTo(liveBounds.width, 1);
    expect(reopenedBounds?.height).toBeCloseTo(liveBounds.height, 1);
    const reopenedPixels = await page.screenshot({ clip: comparisonClip });
    expect(await visibleInkDifferenceRatio(livePixels, reopenedPixels)).toBeLessThan(0.2);

    await app.close();
  });

  test('sizes landscape page thumbnails to the rendered page aspect ratio', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const portraitApp = await launchButterPaper({ theme: 'light' });
    if (!portraitApp) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page: portraitPage } = await openFixturePdf(portraitApp, 'single-page');
    await waitForRenderDiagnostics(portraitPage, {
      pageRenderReady: true,
      lastPageRenderError: null,
      lastThumbnailRenderError: null,
    });
    await waitForRenderedThumbnail(portraitPage, 1);
    const portraitPreviewBox = await portraitPage.getByTestId('page-thumbnail-preview-1').boundingBox();
    expect(portraitPreviewBox).not.toBeNull();
    await portraitApp.close();

    const landscapeApp = await launchButterPaper({ theme: 'light' });
    if (!landscapeApp) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page: landscapePage } = await openFixturePdf(landscapeApp, 'engineering-large');
    await waitForRenderDiagnostics(landscapePage, {
      thumbnailRenderReady: true,
      lastPageRenderError: null,
      lastThumbnailRenderError: null,
    });
    await waitForRenderedThumbnail(landscapePage, 1);
    await expect.poll(async () => {
      const nextBox = await landscapePage.getByTestId('page-thumbnail-preview-1').boundingBox();
      return nextBox?.height ?? 0;
    }).toBeLessThan(170);
    const landscapePreviewBox = await landscapePage.getByTestId('page-thumbnail-preview-1').boundingBox();
    expect(landscapePreviewBox).not.toBeNull();

    expect(landscapePreviewBox.height).toBeLessThan(portraitPreviewBox.height - 40);
    expect(landscapePreviewBox.height).toBeLessThan(170);

    await landscapeApp.close();
  });

  test('renders the shell in dark mode', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'dark' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await waitForShellDiagnostics(page, {
      pageCount: 6,
      leftSidebarOpen: true,
      rightSidebarOpen: false,
      scrollMode: 'continuous',
      zoomPreset: 'fit-width',
      themeMode: 'dark',
    });
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      lastPageRenderError: null,
      lastThumbnailRenderError: null,
    });
    await waitForRenderedThumbnail(page, 1);
    await expectShellSizing(page);
    await expectSidebarHeaders(page, { rightSidebarVisible: false });
    await page.getByTestId('tool-rectangle').click();
    await waitForShellDiagnostics(page, { activeTool: 'rectangle', rightSidebarOpen: false });
    await expectSidebarHeaders(page, { rightSidebarVisible: false });

    await app.close();
  });
});

async function waitForShellDiagnostics(page, expected) {
  await expect.poll(async () => {
    const diagnostics = await getDiagnostics(page);
    return {
      pageCount: diagnostics?.pageCount ?? 0,
      markupCount: diagnostics?.markupCount ?? 0,
      currentPage: diagnostics?.currentPage ?? 0,
      leftSidebarOpen: diagnostics?.leftSidebarOpen ?? false,
      rightSidebarOpen: diagnostics?.rightSidebarOpen ?? false,
      activeTool: diagnostics?.activeTool ?? 'select',
      scrollMode: diagnostics?.scrollMode ?? 'continuous',
      zoomPreset: diagnostics?.zoomPreset ?? 'manual',
      themeMode: diagnostics?.themeMode ?? 'light',
    };
  }).toMatchObject(expected);
}

async function waitForMarkupDiagnostics(page, expected) {
  await expect.poll(async () => {
    const diagnostics = await getDiagnostics(page);
    return {
      pageCount: diagnostics?.pageCount ?? 0,
      markupCount: diagnostics?.markupCount ?? 0,
    };
  }).toMatchObject(expected);
}

async function waitForRenderDiagnostics(page, expected) {
  await expect.poll(async () => {
    const diagnostics = await getDiagnostics(page);
    return {
      pageRenderReady: diagnostics?.pageRenderReady ?? false,
      thumbnailRenderReady: diagnostics?.thumbnailRenderReady ?? false,
      lastPageRenderError: diagnostics?.lastPageRenderError ?? null,
      lastThumbnailRenderError: diagnostics?.lastThumbnailRenderError ?? null,
    };
  }).toMatchObject(expected);
}

async function waitForRenderedThumbnail(page, pageNumber) {
  await expect(
    page.locator(`[data-testid="page-thumbnail-item-${pageNumber}"] img, [data-testid="page-thumbnail-item-${pageNumber}"] canvas`).first()
  ).toBeVisible();
}

async function expectCacheBudgets(page) {
  await expect.poll(async () => {
    const diagnostics = await getDiagnostics(page);
    return Boolean(
      (diagnostics?.renderCacheEntries ?? 0) <= MAX_RENDER_CACHE_ENTRIES &&
      (diagnostics?.renderCacheBytes ?? 0) <= MAX_RENDER_CACHE_BYTES &&
      (diagnostics?.thumbnailCacheEntries ?? 0) <= MAX_THUMBNAIL_CACHE_ENTRIES &&
      (diagnostics?.thumbnailCacheBytes ?? 0) <= MAX_THUMBNAIL_CACHE_BYTES
    );
  }).toBe(true);
}

async function expectShellSizing(page) {
  const appMenuBarBox = await page.getByTestId('app-menu-bar').boundingBox();
  const documentTabBarBox = await page.getByTestId('document-tab-bar').boundingBox();
  const leftRailBox = await page.getByTestId('left-rail').boundingBox();
  const rightRailBox = await page.getByTestId('right-rail').boundingBox();
  const leftRailButtonBox = await page.getByTestId('left-rail-pages').boundingBox();
  const rightRailButtonBox = await page.getByTestId('tool-select').boundingBox();
  const rightRailTrailingButtonBox = await page.getByTestId('tool-pan').boundingBox();
  const viewerToolbarBox = await page.getByTestId('viewer-toolbar').boundingBox();
  const viewerToolbarButtonBox = await page.getByTestId('viewer-zoom-out').boundingBox();

  expect(appMenuBarBox?.height).toBeGreaterThanOrEqual(31);
  expect(appMenuBarBox?.height).toBeLessThanOrEqual(33);
  expect(documentTabBarBox?.height).toBeGreaterThanOrEqual(48);
  expect(documentTabBarBox?.height).toBeLessThanOrEqual(50);

  expect(leftRailBox).not.toBeNull();
  expect(rightRailBox).not.toBeNull();
  expect(leftRailButtonBox).not.toBeNull();
  expect(rightRailButtonBox).not.toBeNull();
  expect(rightRailTrailingButtonBox).not.toBeNull();
  await expect(page.getByTestId('right-rail')).toHaveAttribute('data-column-count', '2');
  expect(leftRailButtonBox?.width).toBeGreaterThanOrEqual(31);
  expect(leftRailButtonBox?.width).toBeLessThanOrEqual(33);
  expect(leftRailButtonBox?.height).toBeGreaterThanOrEqual(31);
  expect(leftRailButtonBox?.height).toBeLessThanOrEqual(33);
  expect(rightRailButtonBox?.width).toBeGreaterThanOrEqual(31);
  expect(rightRailButtonBox?.width).toBeLessThanOrEqual(33);
  expect(rightRailButtonBox?.height).toBeGreaterThanOrEqual(31);
  expect(rightRailButtonBox?.height).toBeLessThanOrEqual(33);
  expect(Math.abs(leftRailButtonBox.width - leftRailButtonBox.height)).toBeLessThanOrEqual(0.5);
  expect(Math.abs(rightRailButtonBox.width - rightRailButtonBox.height)).toBeLessThanOrEqual(0.5);
  expect(leftRailButtonBox.x).toBeGreaterThanOrEqual(leftRailBox.x);
  expect(leftRailButtonBox.x + leftRailButtonBox.width).toBeLessThanOrEqual(leftRailBox.x + leftRailBox.width);
  expect(rightRailButtonBox.x).toBeGreaterThanOrEqual(rightRailBox.x);
  expect(rightRailTrailingButtonBox.x + rightRailTrailingButtonBox.width).toBeLessThanOrEqual(rightRailBox.x + rightRailBox.width);

  expect(viewerToolbarBox?.height).toBeGreaterThanOrEqual(47);
  expect(viewerToolbarBox?.height).toBeLessThanOrEqual(49);
  expect(viewerToolbarButtonBox?.height).toBeGreaterThanOrEqual(31);
  expect(viewerToolbarButtonBox?.height).toBeLessThanOrEqual(33);
}

async function expectSidebarHeaders(page, { rightSidebarVisible }) {
  const leftSidebarHeaderBox = await page.getByTestId('left-sidebar-header').boundingBox();
  expect(leftSidebarHeaderBox?.height).toBeGreaterThanOrEqual(47);
  expect(leftSidebarHeaderBox?.height).toBeLessThanOrEqual(49);

  if (!rightSidebarVisible) {
    return;
  }

  const rightSidebarHeaderBox = await page.getByTestId('right-sidebar-header').boundingBox();
  expect(rightSidebarHeaderBox?.height).toBeGreaterThanOrEqual(47);
  expect(rightSidebarHeaderBox?.height).toBeLessThanOrEqual(49);
}

function paddedScreenshotClip(bounds, padding) {
  return {
    x: Math.max(0, Math.floor(bounds.x - padding)),
    y: Math.max(0, Math.floor(bounds.y - padding)),
    width: Math.max(1, Math.ceil(bounds.width + padding * 2)),
    height: Math.max(1, Math.ceil(bounds.height + padding * 2)),
  };
}

async function visibleInkDifferenceRatio(firstPng, secondPng) {
  const [firstImage, secondImage] = await Promise.all([loadImage(firstPng), loadImage(secondPng)]);
  expect(secondImage.width).toBe(firstImage.width);
  expect(secondImage.height).toBe(firstImage.height);
  const first = imagePixels(firstImage);
  const second = imagePixels(secondImage);
  const firstBackground = first.slice(0, 3);
  const secondBackground = second.slice(0, 3);
  let visiblePixels = 0;
  let differingPixels = 0;
  for (let index = 0; index < first.length; index += 4) {
    const firstInk = channelDistance(first, index, firstBackground) > 20;
    const secondInk = channelDistance(second, index, secondBackground) > 20;
    if (!firstInk && !secondInk) continue;
    visiblePixels += 1;
    if (firstInk !== secondInk || channelDistanceBetween(first, second, index) > 40) differingPixels += 1;
  }
  return visiblePixels === 0 ? 1 : differingPixels / visiblePixels;
}

function imagePixels(image) {
  const canvas = createCanvas(image.width, image.height);
  const context = canvas.getContext('2d');
  context.drawImage(image, 0, 0);
  return context.getImageData(0, 0, image.width, image.height).data;
}

function channelDistance(pixels, index, background) {
  return Math.max(
    Math.abs(pixels[index] - background[0]),
    Math.abs(pixels[index + 1] - background[1]),
    Math.abs(pixels[index + 2] - background[2]),
  );
}

function channelDistanceBetween(first, second, index) {
  return Math.max(
    Math.abs(first[index] - second[index]),
    Math.abs(first[index + 1] - second[index + 1]),
    Math.abs(first[index + 2] - second[index + 2]),
  );
}

async function drag(page, startX, startY, endX, endY) {
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(endX, endY, { steps: 12 });
  await page.mouse.up();
}
