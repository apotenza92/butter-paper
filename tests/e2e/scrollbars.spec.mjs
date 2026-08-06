import { expect, test } from '@playwright/test';
import { getDiagnostics, launchButterPaper, openFixturePdf, resolveDesktopEntryPoint } from './helpers/electron.mjs';

test.describe('custom scrollbars', () => {
  test('renders custom rails for sidebars and keeps deterministic no-overflow chrome', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await waitForShellDiagnostics(page, {
      leftSidebarOpen: true,
      rightSidebarOpen: false,
      pageCount: 6,
      themeMode: 'light',
    });
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      thumbnailRenderReady: true,
    });

    await expect(page.getByTestId('page-thumbnail-scrollbar-track')).toBeVisible();
    await expect(page.getByTestId('page-thumbnail-scrollbar-track')).toHaveAttribute('data-overflow', 'true');
    await expect(page.getByTestId('page-thumbnail-scrollbar-thumb')).toBeVisible();

    await page.getByTestId('tool-rectangle').click();
    await waitForShellDiagnostics(page, {
      rightSidebarOpen: false,
    });

    await app.close();
  });

  test('supports scrollbar dragging and forgiving left sidebar resize hit zones', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      thumbnailRenderReady: true,
    });

    const beforeThumbScrollTop = await getScrollPosition(page, 'page-thumbnail-list');
    const thumbnailThumbBox = await page.getByTestId('page-thumbnail-scrollbar-thumb').boundingBox();
    expect(thumbnailThumbBox).not.toBeNull();
    await drag(page, thumbnailThumbBox.x + thumbnailThumbBox.width / 2, thumbnailThumbBox.y + 4, thumbnailThumbBox.x + thumbnailThumbBox.width / 2, thumbnailThumbBox.y + 48);
    const afterThumbScrollTop = await getScrollPosition(page, 'page-thumbnail-list');
    expect(afterThumbScrollTop.top).toBeGreaterThan(beforeThumbScrollTop.top);

    const leftSidebarBefore = await page.getByTestId('left-sidebar').boundingBox();
    expect(leftSidebarBefore).not.toBeNull();
    const leftResizeBox = await page.getByTestId('left-sidebar-resize-handle').boundingBox();
    expect(leftResizeBox).not.toBeNull();
    await drag(page, leftResizeBox.x + leftResizeBox.width - 1, leftResizeBox.y + leftResizeBox.height / 2, leftResizeBox.x + leftResizeBox.width + 32, leftResizeBox.y + leftResizeBox.height / 2);
    const leftSidebarAfter = await page.getByTestId('left-sidebar').boundingBox();
    expect(leftSidebarAfter).not.toBeNull();
    expect(leftSidebarAfter.width).toBeGreaterThan(leftSidebarBefore.width);

    await app.close();
  });

  test('supports horizontal viewport overflow and pan with custom scrollbars', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'multi-page');
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      thumbnailRenderReady: true,
    });

    await page.getByTestId('viewer-zoom-in').click();
    await page.getByTestId('viewer-zoom-in').click();
    await page.getByTestId('viewer-zoom-in').click();

    await expect.poll(async () => {
      return await page.getByTestId('document-viewport-scrollbar-track-x').getAttribute('data-overflow');
    }).toBe('true');

    await page.getByTestId('tool-pan').click();
    const viewportBox = await page.getByTestId('document-viewport').boundingBox();
    expect(viewportBox).not.toBeNull();

    const beforePan = await getScrollPosition(page, 'document-viewport');
    await drag(
      page,
      viewportBox.x + viewportBox.width * 0.65,
      viewportBox.y + viewportBox.height * 0.65,
      viewportBox.x + viewportBox.width * 0.35,
      viewportBox.y + viewportBox.height * 0.35,
    );
    const afterPan = await getScrollPosition(page, 'document-viewport');

    expect(afterPan.left).toBeGreaterThan(beforePan.left);
    expect(afterPan.top).toBeGreaterThan(beforePan.top);

    await app.close();
  });

  test('anchors wheel zoom to the cursor while a full page fits', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'single-page');
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      thumbnailRenderReady: true,
    });

    await page.getByTestId('viewer-fit-page').click();
    await page.getByTestId('viewer-scroll-single-page').click();
    await waitForShellDiagnostics(page, {
      zoomPreset: 'fit-page',
      scrollMode: 'single-page',
      pageCount: 1,
    });

    const beforeDiagnostics = await getDiagnostics(page);
    const beforePageBox = await page.getByTestId('page-1').boundingBox();
    expect(beforePageBox).not.toBeNull();

    const cursor = {
      x: beforePageBox.x + beforePageBox.width * 0.76,
      y: beforePageBox.y + beforePageBox.height * 0.38,
    };
    const beforeAnchor = pageAnchorRatio(beforePageBox, cursor);

    await page.mouse.move(cursor.x, cursor.y);
    await page.mouse.wheel(0, -260);

    await expect.poll(async () => {
      const diagnostics = await getDiagnostics(page);
      return {
        zoom: diagnostics?.zoom ?? 0,
        zoomPreset: diagnostics?.zoomPreset ?? 'fit-page',
      };
    }).toMatchObject({
      zoomPreset: 'manual',
    });

    const afterDiagnostics = await getDiagnostics(page);
    expect(afterDiagnostics?.zoom ?? 0).toBeGreaterThan(beforeDiagnostics?.zoom ?? 0);

    const afterPageBox = await page.getByTestId('page-1').boundingBox();
    expect(afterPageBox).not.toBeNull();
    const afterAnchor = pageAnchorRatio(afterPageBox, cursor);

    expect(Math.abs(afterAnchor.x - beforeAnchor.x)).toBeLessThan(0.04);
    expect(Math.abs(afterAnchor.y - beforeAnchor.y)).toBeLessThan(0.04);

    await app.close();
  });

  test('does not expand loose canvas from zooming out alone', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'single-page');
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      thumbnailRenderReady: true,
    });

    const viewportBox = await page.getByTestId('document-viewport').boundingBox();
    expect(viewportBox).not.toBeNull();
    const cursor = {
      x: viewportBox.x + viewportBox.width * 0.45,
      y: viewportBox.y + viewportBox.height * 0.34,
    };

    await page.getByTestId('viewer-fit-page').click();
    await page.getByTestId('viewer-scroll-single-page').click();
    await waitForShellDiagnostics(page, {
      zoomPreset: 'fit-page',
      scrollMode: 'single-page',
    });
    await page.mouse.move(cursor.x, cursor.y);
    await page.mouse.wheel(0, 1200);
    await expect.poll(async () => {
      return await page.getByTestId('document-viewport').evaluate((element) => element.scrollWidth - element.clientWidth);
    }).toBeLessThanOrEqual(4);

    await app.close();
  });

  test('collapses loose canvas when returning to fit presets', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'single-page');
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      thumbnailRenderReady: true,
    });

    await page.getByTestId('viewer-fit-page').click();
    await waitForShellDiagnostics(page, {
      zoomPreset: 'fit-page',
    });
    await page.getByTestId('viewer-zoom-out').click();
    await waitForShellDiagnostics(page, {
      zoomPreset: 'manual',
    });
    await page.getByTestId('tool-pan').click();
    const viewportBox = await page.getByTestId('document-viewport').boundingBox();
    expect(viewportBox).not.toBeNull();
    await drag(
      page,
      viewportBox.x + viewportBox.width * 0.45,
      viewportBox.y + viewportBox.height * 0.45,
      viewportBox.x + viewportBox.width * 0.75,
      viewportBox.y + viewportBox.height * 0.45,
    );
    await expect.poll(async () => {
      return await page.getByTestId('document-viewport-scrollbar-track-x').getAttribute('data-overflow');
    }).toBe('true');

    await page.getByTestId('viewer-fit-page').click();
    await waitForShellDiagnostics(page, {
      zoomPreset: 'fit-page',
    });
    await expect.poll(async () => {
      return await page.getByTestId('document-viewport').evaluate((element) => element.scrollWidth - element.clientWidth);
    }).toBe(0);

    await page.getByTestId('viewer-fit-width').click();
    await waitForShellDiagnostics(page, {
      zoomPreset: 'fit-width',
    });
    await expect.poll(async () => {
      return await page.getByTestId('document-viewport').evaluate((element) => element.scrollWidth - element.clientWidth);
    }).toBe(0);

    await app.close();
  });

  test('allows panning beyond the page edge into the surrounding canvas', async () => {
    const entryPoint = resolveDesktopEntryPoint();
    test.skip(!entryPoint, 'Desktop app entrypoint not available yet');

    const app = await launchButterPaper({ theme: 'light' });
    if (!app) {
      test.skip(true, 'Desktop app could not be launched');
    }

    const { page } = await openFixturePdf(app, 'single-page');
    await waitForRenderDiagnostics(page, {
      pageRenderReady: true,
      thumbnailRenderReady: true,
    });

    await page.getByTestId('viewer-fit-page').click();
    await waitForShellDiagnostics(page, {
      zoomPreset: 'fit-page',
    });
    await page.getByTestId('viewer-zoom-out').click();

    await page.getByTestId('tool-pan').click();
    const viewportBox = await page.getByTestId('document-viewport').boundingBox();
    const beforePageBox = await page.getByTestId('page-1').boundingBox();
    expect(viewportBox).not.toBeNull();
    expect(beforePageBox).not.toBeNull();

    await drag(
      page,
      viewportBox.x + viewportBox.width * 0.45,
      viewportBox.y + viewportBox.height * 0.45,
      viewportBox.x + viewportBox.width * 0.75,
      viewportBox.y + viewportBox.height * 0.75,
    );

    const afterPageBox = await page.getByTestId('page-1').boundingBox();
    expect(afterPageBox).not.toBeNull();
    expect(afterPageBox.x).toBeGreaterThan(beforePageBox.x + 40);
    expect(afterPageBox.y).toBeGreaterThan(beforePageBox.y + 40);

    await app.close();
  });
});

async function waitForShellDiagnostics(page, expected) {
  await expect.poll(async () => {
    const diagnostics = await getDiagnostics(page);
    return {
      pageCount: diagnostics?.pageCount ?? 0,
      leftSidebarOpen: diagnostics?.leftSidebarOpen ?? false,
      rightSidebarOpen: diagnostics?.rightSidebarOpen ?? false,
      themeMode: diagnostics?.themeMode ?? 'light',
      scrollMode: diagnostics?.scrollMode ?? 'continuous',
      zoomPreset: diagnostics?.zoomPreset ?? 'manual',
    };
  }).toMatchObject(expected);
}

async function waitForRenderDiagnostics(page, expected) {
  await expect.poll(async () => {
    const diagnostics = await getDiagnostics(page);
    return {
      pageRenderReady: diagnostics?.pageRenderReady ?? false,
      thumbnailRenderReady: diagnostics?.thumbnailRenderReady ?? false,
    };
  }).toMatchObject(expected);
}

async function getScrollPosition(page, testId) {
  return await page.getByTestId(testId).evaluate((element) => ({
    left: element.scrollLeft,
    top: element.scrollTop,
  }));
}

function pageAnchorRatio(box, point) {
  return {
    x: (point.x - box.x) / box.width,
    y: (point.y - box.y) / box.height,
  };
}

async function drag(page, startX, startY, endX, endY) {
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(endX, endY, { steps: 12 });
  await page.mouse.up();
}
