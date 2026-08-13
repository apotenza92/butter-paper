import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createDocument } from '@butter-paper/core';
import type { LoadedDocumentPayload } from '../../../shared/protocol';
import {
  LocalPdfSession,
  isRenderBacklogIdle,
  isRenderTaskAllowedDuringActivationCriticalWindow,
  resolveInitialOverviewThumbnailConcurrencyLimit,
  resolveOverviewThumbnailConcurrencyCeiling,
  shouldProtectFreshImageUrlCacheEntry,
} from './documentSession';
import type { PdfSessionBackend, PdfSessionDocumentHandle } from './documentSessionBackend';
import { openPdfDocumentFromBytes } from '@butter-paper/pdf/browser';

vi.mock('@butter-paper/pdf/browser', () => ({
  openPdfDocumentFromBytes: vi.fn(),
}));

function createDeferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

function createBitmap(): ImageBitmap {
  return {
    close: vi.fn(),
  } as unknown as ImageBitmap;
}

function createPayload(filePath: string): LoadedDocumentPayload {
  return {
    filePath,
    fileName: 'fixture.pdf',
    documentAccess: { handle: `pdfdoc_${'a'.repeat(32)}` },
    document: createDocument({
      id: filePath,
      path: filePath,
      metadata: {},
      pages: [
        {
          id: `${filePath}#page-1`,
          index: 0,
          size: { width: 320, height: 480 },
          rotation: 0,
        },
      ],
      markups: [],
    }),
    openStageTimings: {
      mainPayloadMs: 1,
      mainMetadataMs: 1,
      mainPageModelMs: 1,
      mainAnnotationReadMs: 1,
    },
  };
}

function createBackendHandle(overrides: Partial<PdfSessionDocumentHandle> = {}): PdfSessionDocumentHandle {
  return {
    getPageInfo: vi.fn(async (pageIndex: number) => ({
      index: pageIndex,
      width: 320,
      height: 480,
      rotation: 0 as const,
    })),
    renderPageToBlob: vi.fn(async ({ pageIndex }: { pageIndex: number }) => ({
      pageIndex,
      width: 320,
      height: 480,
      blob: new Blob(['x'], { type: 'image/png' }),
    })),
    renderPageToBitmap: vi.fn(async ({ pageIndex }: { pageIndex: number }) => ({
      pageIndex,
      width: 320,
      height: 480,
      bitmap: createBitmap(),
    })),
    close: vi.fn(async () => undefined),
    ...overrides,
  };
}

describe('LocalPdfSession', () => {
  const mockedOpenPdfDocumentFromBytes = vi.mocked(openPdfDocumentFromBytes);

  beforeEach(() => {
    vi.clearAllMocks();

    vi.stubGlobal('navigator', {
      hardwareConcurrency: 12,
    });

    const createObjectURL = vi.fn(() => 'blob:rendered');
    const revokeObjectURL = vi.fn();
    vi.stubGlobal('URL', {
      createObjectURL,
      revokeObjectURL,
    });

    vi.stubGlobal('window', {
      devicePixelRatio: 1,
      butterPaper: {
        environment: {
          testMode: false,
          defaultSamplePdfPath: null,
          cadRenderExperiment: null,
        },
        pdf: {
          loadDocument: vi.fn(async (filePath: string) => createPayload(filePath)),
          readDocumentBytes: vi.fn(async () => new Uint8Array()),
          releaseDocument: vi.fn(async () => undefined),
        },
        files: {
          readFile: vi.fn(async () => new Uint8Array([1, 2, 3])),
        },
      },
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('ignores stale page render completions after dispose', async () => {
    const renderDeferred = createDeferred<{ pageIndex: number; width: number; height: number; bitmap: ImageBitmap }>();
    const browserHandle = {
      getPageInfo: vi.fn(),
      renderPageToBitmap: vi.fn(() => renderDeferred.promise),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    const renderPromise = session.renderPageBitmap(0, 1);
    session.dispose();

    renderDeferred.resolve({
      pageIndex: 0,
      width: 10,
      height: 10,
      bitmap: createBitmap(),
    });

    await expect(renderPromise).rejects.toMatchObject({ name: 'AbortError' });
    expect(URL.createObjectURL).not.toHaveBeenCalled();
    expect(session.diagnostics()).toMatchObject({
      renderCacheEntries: 0,
      renderCacheBytes: 0,
      pageRenderReady: false,
      lastPageRenderError: null,
    });
  });

  it('closes and rejects a stale open that finishes after disposal', async () => {
    const browserHandle = {
      close: vi.fn(async () => undefined),
    };
    const openDeferred = createDeferred<typeof browserHandle>();
    mockedOpenPdfDocumentFromBytes.mockReturnValue(openDeferred.promise as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    const openPromise = session.open();
    await vi.waitFor(() => {
      expect(mockedOpenPdfDocumentFromBytes).toHaveBeenCalledTimes(1);
    });
    session.dispose();
    openDeferred.resolve(browserHandle);

    await expect(openPromise).rejects.toMatchObject({ name: 'AbortError' });
    expect(browserHandle.close).toHaveBeenCalledTimes(1);
    expect(session.diagnostics()).toMatchObject({
      renderCacheEntries: 0,
      thumbnailCacheEntries: 0,
      pageRenderReady: false,
      thumbnailRenderReady: false,
      openStageTimings: null,
    });
  });

  it('supports an injected backend for open and save while preserving open-stage timings', async () => {
    const firstHandle = createBackendHandle();
    const secondHandle = createBackendHandle();
    const backend: PdfSessionBackend = {
      kind: 'test-backend',
      open: vi
        .fn()
        .mockResolvedValueOnce({
          payload: createPayload('/tmp/injected.pdf'),
          handle: firstHandle,
          backendInfo: {
            sessionBackendKind: 'pdfjs' as const,
            surfaceTransportKind: 'pdfjs-blob-url' as const,
          },
          openStageTimings: {
            mainPayloadMs: 12.34,
            rendererFileReadMs: 23.45,
            rendererBrowserOpenMs: 34.56,
          },
        })
        .mockResolvedValueOnce({
          payload: createPayload('/tmp/saved.pdf'),
          handle: secondHandle,
          backendInfo: {
            sessionBackendKind: 'pdfjs' as const,
            surfaceTransportKind: 'pdfjs-blob-url' as const,
          },
          openStageTimings: {
            mainPayloadMs: 45.67,
            rendererFileReadMs: 56.78,
            rendererBrowserOpenMs: 67.89,
          },
        }),
      save: vi.fn(async () => ({ path: '/tmp/saved.pdf' })),
    };

    const session = new LocalPdfSession('/tmp/injected.pdf', backend);
    const opened = await session.open();

    expect(opened.filePath).toBe('/tmp/injected.pdf');
    expect(opened.openStageTimings).toMatchObject({
      mainPayloadMs: 12.34,
      rendererFileReadMs: 23.45,
      rendererBrowserOpenMs: 34.56,
    });
    expect(session.diagnostics()).toMatchObject({
      sessionBackendKind: 'pdfjs',
      surfaceTransportKind: 'pdfjs-blob-url',
      openStageTimings: {
        mainPayloadMs: 12.34,
        rendererFileReadMs: 23.45,
        rendererBrowserOpenMs: 34.56,
      },
    });

    const target = { targetHandle: `pdftarget_${'b'.repeat(32)}`, displayPath: '/tmp/saved.pdf' };
    const saved = await session.save([], target);

    expect(backend.save).toHaveBeenCalledWith({
      documentHandle: `pdfdoc_${'a'.repeat(32)}`,
      target,
      markups: [],
      pageScales: undefined,
      pageRotations: undefined,
    });
    expect(backend.open).toHaveBeenCalledTimes(2);
    expect(firstHandle.close).toHaveBeenCalledTimes(1);
    expect(saved.filePath).toBe('/tmp/saved.pdf');
    expect(saved.openStageTimings).toMatchObject({
      mainPayloadMs: 45.67,
      rendererFileReadMs: 56.78,
      rendererBrowserOpenMs: 67.89,
    });
    expect(mockedOpenPdfDocumentFromBytes).not.toHaveBeenCalled();
  });

  it('saves to the opened PDF when no Save As target is supplied', async () => {
    const firstHandle = createBackendHandle();
    const secondHandle = createBackendHandle();
    const backend: PdfSessionBackend = {
      kind: 'test-backend',
      open: vi
        .fn()
        .mockResolvedValueOnce({
          payload: createPayload('/tmp/opened.pdf'),
          handle: firstHandle,
          backendInfo: {
            sessionBackendKind: 'pdfjs' as const,
            surfaceTransportKind: 'pdfjs-blob-url' as const,
          },
          openStageTimings: { mainPayloadMs: 1, rendererFileReadMs: 2, rendererBrowserOpenMs: 3 },
        })
        .mockResolvedValueOnce({
          payload: createPayload('/tmp/opened.pdf'),
          handle: secondHandle,
          backendInfo: {
            sessionBackendKind: 'pdfjs' as const,
            surfaceTransportKind: 'pdfjs-blob-url' as const,
          },
          openStageTimings: { mainPayloadMs: 4, rendererFileReadMs: 5, rendererBrowserOpenMs: 6 },
        }),
      save: vi.fn(async () => ({ path: '/tmp/opened.pdf' })),
    };
    const session = new LocalPdfSession('/tmp/opened.pdf', backend);
    await session.open();

    await session.save([]);

    expect(backend.save).toHaveBeenCalledWith({
      documentHandle: `pdfdoc_${'a'.repeat(32)}`,
      target: undefined,
      markups: [],
      pageScales: undefined,
      pageRotations: undefined,
    });
  });

  it('treats render requests during a save reopen as transient instead of session errors', async () => {
    const firstHandle = createBackendHandle();
    const secondHandle = createBackendHandle();
    const reopenDeferred = createDeferred<Awaited<ReturnType<PdfSessionBackend['open']>>>();
    const backend: PdfSessionBackend = {
      kind: 'test-backend',
      open: vi
        .fn()
        .mockResolvedValueOnce({
          payload: createPayload('/tmp/injected.pdf'),
          handle: firstHandle,
          backendInfo: {
            sessionBackendKind: 'pdfjs' as const,
            surfaceTransportKind: 'pdfjs-blob-url' as const,
          },
          openStageTimings: {
            mainPayloadMs: 1,
            rendererFileReadMs: 2,
            rendererBrowserOpenMs: 3,
          },
        })
        .mockReturnValueOnce(reopenDeferred.promise),
      save: vi.fn(async () => ({ path: '/tmp/saved.pdf' })),
    };

    const session = new LocalPdfSession('/tmp/injected.pdf', backend);
    await session.open();

    const savePromise = session.save([], {
      targetHandle: `pdftarget_${'b'.repeat(32)}`,
      displayPath: '/tmp/saved.pdf',
    });
    await vi.waitFor(() => expect(backend.open).toHaveBeenCalledTimes(2));
    const versionBeforeTransientRender = session.version;

    await expect(session.renderPageBitmap(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-hq',
    })).rejects.toMatchObject({ name: 'RenderUnavailableError' });
    expect(session.version).toBe(versionBeforeTransientRender);
    expect(session.diagnostics()).toMatchObject({
      pageRenderReady: false,
      lastPageRenderError: null,
    });

    reopenDeferred.resolve({
      payload: createPayload('/tmp/saved.pdf'),
      handle: secondHandle,
      backendInfo: {
        sessionBackendKind: 'pdfjs',
        surfaceTransportKind: 'pdfjs-blob-url',
      },
      openStageTimings: {
        mainPayloadMs: 4,
        rendererFileReadMs: 5,
        rendererBrowserOpenMs: 6,
      },
    });

    await expect(savePromise).resolves.toMatchObject({ filePath: '/tmp/saved.pdf' });
    await expect(session.renderPageBitmap(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-hq',
    })).resolves.toMatchObject({ pageIndex: 0 });
  });

  it('renders pages from an injected backend handle using only the abstract handle contract', async () => {
    let objectUrlCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL: vi.fn(),
    });

    const handle = createBackendHandle();
    const backend: PdfSessionBackend = {
      kind: 'test-backend',
      open: vi.fn(async () => ({
        payload: createPayload('/tmp/injected.pdf'),
        handle,
        backendInfo: {
          sessionBackendKind: 'pdfjs' as const,
          surfaceTransportKind: 'pdfjs-blob-url' as const,
        },
        openStageTimings: {
          mainPayloadMs: 1,
          rendererFileReadMs: 2,
          rendererBrowserOpenMs: 3,
        },
      })),
      save: vi.fn(async () => ({ path: '/tmp/injected.pdf' })),
    };

    const session = new LocalPdfSession('/tmp/injected.pdf', backend);
    await session.open();

    const objectUrl = await session.renderPage(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-preview',
    });

    expect(objectUrl).toBe('blob:rendered-1');
    expect(handle.renderPageToBlob).toHaveBeenCalledTimes(1);
    expect(mockedOpenPdfDocumentFromBytes).not.toHaveBeenCalled();
  });

  it('protects freshly rendered visible page URLs during immediate cache enforcement', async () => {
    vi.useFakeTimers();
    let objectUrlCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL: vi.fn(),
    });

    const handle = createBackendHandle();
    const backend: PdfSessionBackend = {
      kind: 'test-backend',
      open: vi.fn(async () => ({
        payload: createPayload('/tmp/cache-protection.pdf'),
        handle,
        backendInfo: {
          sessionBackendKind: 'pdfjs' as const,
          surfaceTransportKind: 'pdfjs-blob-url' as const,
        },
        openStageTimings: {
          mainPayloadMs: 1,
          rendererFileReadMs: 2,
          rendererBrowserOpenMs: 3,
        },
      })),
      save: vi.fn(async () => ({ path: '/tmp/cache-protection.pdf' })),
    };

    const session = new LocalPdfSession('/tmp/cache-protection.pdf', backend);
    await session.open();

    for (let index = 0; index < 10; index += 1) {
      await session.renderPage(0, 1 + index * 0.01, 1, {
        urgency: 'visible',
        requestClass: 'visible-page-preview',
      });
    }

    expect(URL.revokeObjectURL).not.toHaveBeenCalled();
    expect(session.diagnostics().renderCacheEntries).toBe(10);

    vi.advanceTimersByTime(12_100);
    await session.renderPage(0, 2, 1, {
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    });

    expect(URL.revokeObjectURL).toHaveBeenCalled();
    expect(session.diagnostics().renderCacheEntries).toBeLessThanOrEqual(8);
  });

  it('keeps held page URLs protected and discards broken URLs from caches', async () => {
    let objectUrlCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL: vi.fn(),
    });

    const handle = createBackendHandle();
    const backend: PdfSessionBackend = {
      kind: 'test-backend',
      open: vi.fn(async () => ({
        payload: createPayload('/tmp/discard-url.pdf'),
        handle,
        backendInfo: {
          sessionBackendKind: 'pdfjs' as const,
          surfaceTransportKind: 'pdfjs-blob-url' as const,
        },
        openStageTimings: {
          mainPayloadMs: 1,
          rendererFileReadMs: 2,
          rendererBrowserOpenMs: 3,
        },
      })),
      save: vi.fn(async () => ({ path: '/tmp/discard-url.pdf' })),
    };

    const session = new LocalPdfSession('/tmp/discard-url.pdf', backend);
    await session.open();

    const objectUrl = await session.renderPage(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-preview',
    });
    session.retainPageImageUrl(objectUrl);
    session.discardPageImageUrl(objectUrl);

    expect(URL.revokeObjectURL).toHaveBeenCalledWith(objectUrl);
    expect(session.getReusablePagePreview(0)).toBeNull();
  });

  it('protects fresh CAD overview thumbnails as well as visible page images', () => {
    expect(shouldProtectFreshImageUrlCacheEntry('overview-thumbnail')).toBe(true);
    expect(shouldProtectFreshImageUrlCacheEntry('target-page-preview')).toBe(true);
    expect(shouldProtectFreshImageUrlCacheEntry('target-page-crop')).toBe(true);
    expect(shouldProtectFreshImageUrlCacheEntry('visible-page-hq-upgrade')).toBe(true);
    expect(shouldProtectFreshImageUrlCacheEntry('nearby-prefetch')).toBe(false);
    expect(shouldProtectFreshImageUrlCacheEntry(undefined)).toBe(false);
  });

  it('reuses a cached thumbnail raster as a page preview candidate', async () => {
    let objectUrlCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL: vi.fn(),
    });

    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(async ({ scale }: { scale: number }) => ({
        pageIndex: 0,
        width: Math.round(320 * scale),
        height: Math.round(480 * scale),
        blob: new Blob(['x'], { type: 'image/png' }),
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    await session.renderThumbnail(0, {
      maxWidth: 160,
      maxHeight: 220,
      pageWidth: 320,
      pageHeight: 480,
    }, 1.25);

    expect(session.hasReusablePagePreview(0)).toBe(true);
    expect(session.getReusablePagePreview(0)).toBe('blob:rendered-1');
    expect(session.getReusablePagePreview(0, 120)).toBe('blob:rendered-1');
    expect(session.getReusablePagePreview(0, 300)).toBe('blob:rendered-1');
    expect(session.getReusablePagePreviewInfoAtLeast(0, 120)).toMatchObject({ objectUrl: 'blob:rendered-1', source: 'thumbnail' });
    expect(session.getReusablePagePreviewInfoAtLeast(0, 300)).toBeNull();
    expect(session.hasReusablePagePreview(1)).toBe(false);
    expect(session.getReusablePagePreview(1)).toBeNull();
  });

  it('allows ultra-low overview thumbnails below the normal thumbnail scale floor', async () => {
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(async ({ scale, requestClass }: { scale: number; requestClass?: string }) => ({
        pageIndex: 0,
        width: Math.round(320 * scale),
        height: Math.round(480 * scale),
        blob: new Blob([requestClass ?? 'x'], { type: 'image/png' }),
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    await session.renderThumbnail(0, {
      maxWidth: 8,
      maxHeight: 12,
      minScale: 0.025,
      pageWidth: 320,
      pageHeight: 480,
    }, 1, {
      urgency: 'visible',
      requestClass: 'overview-thumbnail',
    });

    expect(browserHandle.renderPageToBlob).toHaveBeenCalledWith(expect.objectContaining({
      scale: 0.025,
      requestClass: 'overview-thumbnail',
    }));
  });

  it('reuses a cached page-url raster as a page preview candidate when no thumbnail preview exists', async () => {
    let objectUrlCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL: vi.fn(),
    });

    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(async ({ scale }: { scale: number }) => ({
        pageIndex: 0,
        width: Math.round(320 * scale),
        height: Math.round(480 * scale),
        blob: new Blob(['x'], { type: 'image/png' }),
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    await session.renderPage(0, 0.6, 1, {
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    });

    expect(session.getReusablePagePreviewInfo(0)).toMatchObject({
      objectUrl: 'blob:rendered-1',
      source: 'page-url',
    });
    expect(session.getReusablePagePreviewInfoAtLeast(0, 150)).toMatchObject({
      objectUrl: 'blob:rendered-1',
      source: 'page-url',
    });
  });

  it('does not revoke page image object URLs while a view retains them', async () => {
    vi.useFakeTimers();
    let objectUrlCounter = 0;
    const revokeObjectURL = vi.fn();
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL,
    });

    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(async ({ scale }: { scale: number }) => ({
        pageIndex: 0,
        width: Math.round(320 * scale),
        height: Math.round(480 * scale),
        blob: new Blob(['x'], { type: 'image/png' }),
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    const heldUrl = await session.renderPage(0, 0.5, 1, {
      urgency: 'visible',
      requestClass: 'target-page-hq',
    });
    session.retainPageImageUrl(heldUrl);

    for (let index = 1; index <= 8; index += 1) {
      await session.renderPage(0, 0.5 + index * 0.01, 1, {
        urgency: 'visible',
        requestClass: 'target-page-hq',
      });
    }

    expect(revokeObjectURL).not.toHaveBeenCalledWith(heldUrl);

    session.releasePageImageUrl(heldUrl);
    vi.advanceTimersByTime(2_100);
    await session.renderPage(0, 0.99, 1, {
      urgency: 'visible',
      requestClass: 'target-page-hq',
    });

    expect(revokeObjectURL).toHaveBeenCalledWith(heldUrl);
  });

  it('selects the best reusable page image before falling back to thumbnails', async () => {
    let objectUrlCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL: vi.fn(),
    });

    const bitmap = createBitmap();
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(async ({ scale }: { scale: number }) => ({
        pageIndex: 0,
        width: Math.round(320 * scale),
        height: Math.round(480 * scale),
        blob: new Blob(['x'], { type: 'image/png' }),
      })),
      renderPageToBitmap: vi.fn(async ({ scale }: { scale: number }) => ({
        pageIndex: 0,
        width: Math.round(320 * scale),
        height: Math.round(480 * scale),
        bitmap,
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    await session.renderThumbnail(0, {
      maxWidth: 220,
      maxHeight: 330,
      pageWidth: 320,
      pageHeight: 480,
    }, 1);
    await session.renderPage(0, 0.6, 1, {
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    });
    const bitmapSurface = await session.renderPageBitmap(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-preview',
    });
    session.releasePageSurface(bitmapSurface);

    const selected = session.getBestReusablePageImage(0, 200);
    expect(selected).toMatchObject({
      kind: 'surface',
      source: 'page-bitmap',
      renderedWidth: 320,
      sourceRequestClass: 'target-page-preview',
    });
    if (selected?.kind === 'surface') {
      session.releasePageSurface(selected.surface);
    }

    expect(session.getBestReusablePageImage(1, 200)).toBeNull();
  });

  it('keeps crop bitmap cache entries separate from full-page reusable images', async () => {
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBitmap: vi.fn(async ({
        pageIndex,
        scale,
        cropPdfRect,
      }: {
        pageIndex: number;
        scale: number;
        cropPdfRect?: { width: number; height: number };
      }) => ({
        pageIndex,
        width: Math.round((cropPdfRect?.width ?? 320) * scale),
        height: Math.round((cropPdfRect?.height ?? 480) * scale),
        bitmap: createBitmap(),
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    const full = await session.renderPageBitmap(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-hq',
    });
    session.releasePageSurface(full);

    const cropRect = { x: 10, y: 20, width: 100, height: 120 };
    const crop = await session.renderPageBitmap(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-crop',
      cropPdfRect: cropRect,
    });
    expect(crop).toMatchObject({
      renderedWidth: 100,
      renderedHeight: 120,
      cropPdfRect: cropRect,
    });
    session.releasePageSurface(crop);

    const sameCrop = await session.renderPageBitmap(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-crop',
      cropPdfRect: cropRect,
    });
    session.releasePageSurface(sameCrop);

    expect(browserHandle.renderPageToBitmap).toHaveBeenCalledTimes(2);

    const selected = session.getBestReusablePageImage(0, 80);
    expect(selected).toMatchObject({
      kind: 'surface',
      renderedWidth: 320,
      sourceRequestClass: 'target-page-hq',
    });
    if (selected?.kind === 'surface') {
      session.releasePageSurface(selected.surface);
    }
  });

  it('batches thumbnail queue starts so visible thumbnails can render out of page order by priority', async () => {
    const startedPageIndices: number[] = [];
    const renderDeferreds: Array<{
      pageIndex: number;
      deferred: ReturnType<typeof createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>>;
    }> = [];
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(({ pageIndex }: { pageIndex: number }) => {
        const deferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
        startedPageIndices.push(pageIndex);
        renderDeferreds.push({ pageIndex, deferred });
        return deferred.promise;
      }),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    const thumbnailOptions = {
      maxWidth: 160,
      maxHeight: 220,
      pageWidth: 320,
      pageHeight: 480,
    };

    const lowPriority = session.renderThumbnail(0, thumbnailOptions, 1, {
      priority: 1000,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });
    const highPriority = session.renderThumbnail(1, thumbnailOptions, 1, {
      priority: 2000,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });
    const middlePriority = session.renderThumbnail(2, thumbnailOptions, 1, {
      priority: 1500,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });

    await Promise.resolve();
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(3);
    });
    expect(startedPageIndices).toEqual([1, 2, 0]);

    for (const { pageIndex, deferred } of renderDeferreds.splice(0)) {
      deferred.resolve({
        pageIndex,
        width: 160,
        height: 220,
        blob: new Blob([String(pageIndex)], { type: 'image/png' }),
      });
    }
    await Promise.all([lowPriority, highPriority, middlePriority]);
  });

  it('lets tiny visible overview thumbnails burst into the idle render slots', async () => {
    vi.stubGlobal('navigator', {
      hardwareConcurrency: 12,
    });
    const startedPageIndices: number[] = [];
    const renderDeferreds: Array<{
      pageIndex: number;
      deferred: ReturnType<typeof createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>>;
    }> = [];
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(({ pageIndex }: { pageIndex: number }) => {
        const deferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
        startedPageIndices.push(pageIndex);
        renderDeferreds.push({ pageIndex, deferred });
        return deferred.promise;
      }),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    const thumbnailOptions = {
      maxWidth: 32,
      maxHeight: 42,
      pageWidth: 320,
      pageHeight: 480,
      minScale: 0.025,
    };

    const overviewPromises = Array.from({ length: 7 }, (_, pageIndex) => session.renderThumbnail(pageIndex, thumbnailOptions, 1, {
      priority: 6000 - pageIndex,
      urgency: 'visible',
      requestClass: 'overview-thumbnail',
    }));

    await Promise.resolve();
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(6);
    });
    expect(startedPageIndices).toEqual([0, 1, 2, 3, 4, 5]);

    for (const { pageIndex, deferred } of renderDeferreds.splice(0)) {
      deferred.resolve({
        pageIndex,
        width: 32,
        height: 42,
        blob: new Blob([String(pageIndex)], { type: 'image/png' }),
      });
    }

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(7);
    });
    const finalRender = renderDeferreds.shift();
    finalRender?.deferred.resolve({
      pageIndex: finalRender.pageIndex,
      width: 32,
      height: 42,
      blob: new Blob([String(finalRender.pageIndex)], { type: 'image/png' }),
    });
    await Promise.all(overviewPromises);
  });

  it('caps overview burst expansion while the viewport is in motion', async () => {
    vi.stubGlobal('navigator', {
      hardwareConcurrency: 12,
    });
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(() => createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>().promise),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    session.setViewportInMotion(true);
    const thumbnailOptions = {
      maxWidth: 32,
      maxHeight: 42,
      pageWidth: 320,
      pageHeight: 480,
      minScale: 0.025,
    };

    for (let pageIndex = 0; pageIndex < 7; pageIndex += 1) {
      void session.renderThumbnail(pageIndex, thumbnailOptions, 1, {
        priority: 6000 - pageIndex,
        urgency: 'visible',
        requestClass: 'overview-thumbnail',
      }).catch(() => undefined);
    }

    await Promise.resolve();
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(3);
    });
  });

  it('still starts visible sidebar thumbnails while the document viewport is in motion', async () => {
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(async ({ pageIndex }: { pageIndex: number }) => ({
        pageIndex,
        width: 160,
        height: 220,
        blob: new Blob([String(pageIndex)], { type: 'image/png' }),
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    session.setViewportInMotion(true);

    await session.renderThumbnail(12, {
      maxWidth: 160,
      maxHeight: 220,
      pageWidth: 320,
      pageHeight: 480,
    }, 1, {
      priority: 2000,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });

    expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(1);
  });

  it('reserves a render lane for visible sidebar thumbnails even when page previews are queued', async () => {
    const startedRequestClasses: Array<string | undefined> = [];
    const renderDeferreds: Array<ReturnType<typeof createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>>> = [];
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(({ pageIndex, requestClass }: { pageIndex: number; requestClass?: string }) => {
        const deferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
        startedRequestClasses.push(requestClass);
        renderDeferreds.push(deferred);
        return deferred.promise;
      }),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    void session.renderPage(0, 0.5, 1, {
      priority: 1000,
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    }).catch(() => undefined);
    void session.renderPage(1, 0.5, 1, {
      priority: 999,
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    }).catch(() => undefined);
    void session.renderPage(2, 0.5, 1, {
      priority: 998,
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    }).catch(() => undefined);

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    });

    const visibleThumbnail = session.renderThumbnail(12, {
      maxWidth: 160,
      maxHeight: 220,
      pageWidth: 320,
      pageHeight: 480,
    }, 1, {
      priority: 2000,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });

    await vi.waitFor(() => {
      expect(startedRequestClasses).toContain('visible-thumbnail');
    });

    const thumbnailDeferred = renderDeferreds[startedRequestClasses.indexOf('visible-thumbnail')];
    thumbnailDeferred?.resolve({
      pageIndex: 12,
      width: 160,
      height: 220,
      blob: new Blob(['12'], { type: 'image/png' }),
    });
    await visibleThumbnail;
  });

  it('reserves a render lane for visible CAD overview thumbnails even when page previews are queued', async () => {
    const startedRequestClasses: Array<string | undefined> = [];
    const renderDeferreds: Array<ReturnType<typeof createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>>> = [];
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(({ requestClass }: { pageIndex: number; requestClass?: string }) => {
        const deferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
        startedRequestClasses.push(requestClass);
        renderDeferreds.push(deferred);
        return deferred.promise;
      }),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    void session.renderPage(0, 0.5, 1, {
      priority: 1000,
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    }).catch(() => undefined);
    void session.renderPage(1, 0.5, 1, {
      priority: 999,
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    }).catch(() => undefined);
    void session.renderPage(2, 0.5, 1, {
      priority: 998,
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    }).catch(() => undefined);

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    });

    const visibleOverviewThumbnail = session.renderThumbnail(12, {
      maxWidth: 240,
      maxHeight: 320,
      pageWidth: 320,
      pageHeight: 480,
      minScale: 0.025,
    }, 1, {
      priority: 7000,
      urgency: 'visible',
      requestClass: 'overview-thumbnail',
    });

    await vi.waitFor(() => {
      expect(startedRequestClasses).toContain('overview-thumbnail');
    });

    const thumbnailDeferred = renderDeferreds[startedRequestClasses.indexOf('overview-thumbnail')];
    thumbnailDeferred?.resolve({
      pageIndex: 12,
      width: 240,
      height: 320,
      blob: new Blob(['12'], { type: 'image/png' }),
    });
    await visibleOverviewThumbnail;
  });

  it('promotes a queued thumbnail when a visible request attaches to pending prefetch work', async () => {
    const startedPageIndices: number[] = [];
    const startedRequestClasses: Array<string | undefined> = [];
    const renderDeferreds: Array<{
      pageIndex: number;
      deferred: ReturnType<typeof createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>>;
    }> = [];
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(({ pageIndex, requestClass }: { pageIndex: number; requestClass?: string }) => {
        const deferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
        startedPageIndices.push(pageIndex);
        startedRequestClasses.push(requestClass);
        renderDeferreds.push({ pageIndex, deferred });
        return deferred.promise;
      }),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    const thumbnailOptions = {
      maxWidth: 160,
      maxHeight: 220,
      pageWidth: 320,
      pageHeight: 480,
    };

    const promoted = session.renderThumbnail(0, thumbnailOptions, 1, {
      priority: 1,
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    });
    const attachedVisible = session.renderThumbnail(0, thumbnailOptions, 1, {
      priority: 2500,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });
    const otherVisible = session.renderThumbnail(1, thumbnailOptions, 1, {
      priority: 2000,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });

    await Promise.resolve();
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    });
    expect(startedPageIndices).toEqual([0, 1]);
    expect(startedRequestClasses).toEqual(['visible-thumbnail', 'visible-thumbnail']);

    for (const { pageIndex, deferred } of renderDeferreds.splice(0)) {
      deferred.resolve({
        pageIndex,
        width: 160,
        height: 220,
        blob: new Blob([String(pageIndex)], { type: 'image/png' }),
      });
    }
    await Promise.all([promoted, attachedVisible, otherVisible]);
  });

  it('preempts active prefetch thumbnails when a visible thumbnail is waiting for a slot', async () => {
    const started: Array<{
      pageIndex: number;
      signal?: AbortSignal;
      deferred: ReturnType<typeof createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>>;
    }> = [];
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(({ pageIndex, signal }: { pageIndex: number; signal?: AbortSignal }) => {
        const deferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
        signal?.addEventListener('abort', () => {
          const error = new Error('aborted');
          error.name = 'AbortError';
          deferred.reject(error);
        }, { once: true });
        started.push({ pageIndex, signal, deferred });
        return deferred.promise;
      }),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    const thumbnailOptions = {
      maxWidth: 160,
      maxHeight: 220,
      pageWidth: 320,
      pageHeight: 480,
    };

    const readyThumbnail = session.renderThumbnail(9, thumbnailOptions, 1, {
      priority: 1,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });
    await Promise.resolve();
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(1);
    });
    started[0]?.deferred.resolve({
      pageIndex: 9,
      width: 160,
      height: 220,
      blob: new Blob(['9'], { type: 'image/png' }),
    });
    await readyThumbnail;
    started.splice(0, started.length);
    browserHandle.renderPageToBlob.mockClear();

    const firstPrefetch = session.renderThumbnail(0, thumbnailOptions, 1, {
      priority: 1,
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    }).catch(() => 'aborted');
    const existingVisible = session.renderThumbnail(1, thumbnailOptions, 1, {
      priority: 2,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });

    await Promise.resolve();
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    });
    expect(started.map((entry) => entry.pageIndex)).toEqual([1, 0]);
    session.setThumbnailListInMotion(true);

    const visible = session.renderThumbnail(2, thumbnailOptions, 1, {
      priority: 3000,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(3);
    });
    expect(started.map((entry) => entry.pageIndex)).toEqual([1, 0, 2]);

    const visibleRender = started.find((entry) => entry.pageIndex === 2);
    visibleRender?.deferred.resolve({
      pageIndex: 2,
      width: 160,
      height: 220,
      blob: new Blob(['2'], { type: 'image/png' }),
    });
    await visible;

    const prefetchRender = started.find((entry) => entry.pageIndex === 0);
    prefetchRender?.deferred.resolve({
      pageIndex: 0,
      width: 160,
      height: 220,
      blob: new Blob(['0'], { type: 'image/png' }),
    });
    await firstPrefetch;

    const existingVisibleRender = started.find((entry) => entry.pageIndex === 1);
    existingVisibleRender?.deferred.resolve({
      pageIndex: 1,
      width: 160,
      height: 220,
      blob: new Blob(['1'], { type: 'image/png' }),
    });
    await existingVisible;
  });

  it('preempts active prefetch thumbnails for visible sidebar rows after motion settles', async () => {
    const started: Array<{
      pageIndex: number;
      requestClass?: string;
      signal?: AbortSignal;
      deferred: ReturnType<typeof createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>>;
    }> = [];
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(({
        pageIndex,
        requestClass,
        signal,
      }: {
        pageIndex: number;
        requestClass?: string;
        signal?: AbortSignal;
      }) => {
        const deferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
        signal?.addEventListener('abort', () => {
          const error = new Error('aborted');
          error.name = 'AbortError';
          deferred.reject(error);
        }, { once: true });
        started.push({ pageIndex, requestClass, signal, deferred });
        return deferred.promise;
      }),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    const thumbnailOptions = {
      maxWidth: 160,
      maxHeight: 220,
      pageWidth: 320,
      pageHeight: 480,
    };

    const readyThumbnail = session.renderThumbnail(9, thumbnailOptions, 1, {
      priority: 1,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(1);
    });
    started[0]?.deferred.resolve({
      pageIndex: 9,
      width: 160,
      height: 220,
      blob: new Blob(['9'], { type: 'image/png' }),
    });
    await readyThumbnail;
    started.splice(0, started.length);
    browserHandle.renderPageToBlob.mockClear();

    const firstPrefetch = session.renderThumbnail(0, thumbnailOptions, 1, {
      priority: 3000,
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    }).catch(() => 'aborted');
    const activeVisible = session.renderThumbnail(1, thumbnailOptions, 1, {
      priority: 1,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });
    void session.renderThumbnail(3, thumbnailOptions, 1, {
      priority: 2999,
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    }).catch(() => undefined);

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    });

    const visible = session.renderThumbnail(2, thumbnailOptions, 1, {
      priority: 2000,
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    });

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(3);
    });
    expect(started.map((entry) => entry.requestClass)).toEqual(expect.arrayContaining([
      'nearby-prefetch',
      'visible-thumbnail',
      'visible-thumbnail',
    ]));

    const visibleRender = started.find((entry) => entry.pageIndex === 2);
    visibleRender?.deferred.resolve({
      pageIndex: 2,
      width: 160,
      height: 220,
      blob: new Blob(['2'], { type: 'image/png' }),
    });
    await visible;

    const firstPrefetchRender = started.find((entry) => entry.pageIndex === 0);
    firstPrefetchRender?.deferred.resolve({
      pageIndex: 0,
      width: 160,
      height: 220,
      blob: new Blob(['0'], { type: 'image/png' }),
    });
    await firstPrefetch;

    const activeVisibleRender = started.find((entry) => entry.pageIndex === 1);
    activeVisibleRender?.deferred.resolve({
      pageIndex: 1,
      width: 160,
      height: 220,
      blob: new Blob(['1'], { type: 'image/png' }),
    });
    await activeVisible;
  });

  it('selects only adequate cached page images for cache-only promotion', async () => {
    let objectUrlCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL: vi.fn(),
    });

    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(async ({ scale }: { scale: number }) => ({
        pageIndex: 0,
        width: Math.round(320 * scale),
        height: Math.round(480 * scale),
        blob: new Blob(['x'], { type: 'image/png' }),
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    await session.renderThumbnail(0, {
      maxWidth: 220,
      maxHeight: 330,
      pageWidth: 320,
      pageHeight: 480,
    }, 1);
    await session.renderPage(0, 0.5, 1, {
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    });
    await session.renderPage(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'visible-page-hq-upgrade',
    });

    expect(session.getBestReusablePageImageAtLeast(0, 300)).toMatchObject({
      kind: 'object-url',
      source: 'page-url',
      renderedWidth: 320,
      sourceRequestClass: 'visible-page-hq-upgrade',
    });
    expect(session.getBestReusablePageImageAtLeast(0, 500)).toBeNull();
  });

  it('uses the widest lower-resolution page raster instead of blanking or falling back to thumbnails', async () => {
    let objectUrlCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL: vi.fn(),
    });

    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(async ({ scale }: { scale: number }) => ({
        pageIndex: 0,
        width: Math.round(320 * scale),
        height: Math.round(480 * scale),
        blob: new Blob(['x'], { type: 'image/png' }),
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    await session.renderThumbnail(0, {
      maxWidth: 220,
      maxHeight: 330,
      pageWidth: 320,
      pageHeight: 480,
    }, 1);
    await session.renderPage(0, 0.3, 1, {
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    });
    await session.renderPage(0, 0.5, 1, {
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    });

    expect(session.getBestReusablePageImage(0, 300)).toMatchObject({
      kind: 'object-url',
      source: 'page-url',
      renderedWidth: 160,
      objectUrl: 'blob:rendered-3',
    });
  });

  it('keeps preview reuse disabled until interaction enables it', async () => {
    const browserHandle = {
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    expect(session.isPreviewReuseEnabled()).toBe(false);
    session.enablePreviewReuse();
    expect(session.isPreviewReuseEnabled()).toBe(true);
    session.dispose();
    expect(session.isPreviewReuseEnabled()).toBe(false);
  });

  it('warms the first visible page for inactive tabs and exposes readiness diagnostics', async () => {
    let objectUrlCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => `blob:rendered-${++objectUrlCounter}`),
      revokeObjectURL: vi.fn(),
    });

    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(async ({ pageIndex, requestClass }: { pageIndex: number; requestClass?: string }) => ({
        pageIndex,
        width: 160,
        height: 240,
        blob: new Blob([requestClass ?? 'x'], { type: 'image/png' }),
      })),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    session.deactivate();

    await session.warmFirstVisiblePage();

    expect(browserHandle.renderPageToBlob).toHaveBeenCalledWith(expect.objectContaining({
      pageIndex: 0,
      requestClass: 'warming',
    }));
    expect(session.diagnostics()).toMatchObject({
      firstVisiblePageIndex: 0,
      firstVisiblePageReady: true,
      firstVisiblePageReadyRequestClass: 'warming',
      firstVisiblePageWarmupStatus: 'ready',
      pageRenderReady: true,
    });
    expect(session.diagnostics().firstVisiblePageReadyAtMs).not.toBeNull();
  });

  it('tracks thumbnail navigation intent separately from generic navigation intent', async () => {
    const browserHandle = {
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    session.setNavigationIntent(3, 2500, 'thumbnail');
    expect(session.isNavigationIntentPage(3)).toBe(true);
    expect(session.isThumbnailNavigationIntentPage(3)).toBe(true);

    session.setNavigationIntent(3, 2500, 'generic');
    expect(session.isNavigationIntentPage(3)).toBe(true);
    expect(session.isThumbnailNavigationIntentPage(3)).toBe(false);
  });

  it('consumes a primed page preview only once', async () => {
    const browserHandle = {
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    session.primePagePreview(3, 'blob:thumbnail-preview');

    expect(session.consumePrimedPagePreview(3)).toBe('blob:thumbnail-preview');
    expect(session.consumePrimedPagePreview(3)).toBeNull();
  });

  it('trims retained bitmap page cache entries more aggressively than URL page cache entries', async () => {
    const bitmaps = [createBitmap(), createBitmap(), createBitmap()];
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBitmap: vi
        .fn()
        .mockResolvedValueOnce({ pageIndex: 0, width: 320, height: 480, bitmap: bitmaps[0] })
        .mockResolvedValueOnce({ pageIndex: 0, width: 512, height: 768, bitmap: bitmaps[1] })
        .mockResolvedValueOnce({ pageIndex: 0, width: 640, height: 960, bitmap: bitmaps[2] }),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    const first = await session.renderPageBitmap(0, 0.5, 1);
    session.releasePageSurface(first);
    const second = await session.renderPageBitmap(0, 0.8, 1);
    session.releasePageSurface(second);
    const third = await session.renderPageBitmap(0, 1, 1);
    session.releasePageSurface(third);

    expect(browserHandle.renderPageToBitmap).toHaveBeenCalledTimes(3);
    expect(bitmaps[0].close).toHaveBeenCalledTimes(1);
    expect(bitmaps[1].close).not.toHaveBeenCalled();
    expect(bitmaps[2].close).not.toHaveBeenCalled();
    expect(session.diagnostics()).toMatchObject({
      renderCacheEntries: 2,
    });
  });

  it('does not close a held bitmap surface when cache pressure evicts its entry', async () => {
    const bitmaps = [createBitmap(), createBitmap(), createBitmap()];
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBitmap: vi
        .fn()
        .mockResolvedValueOnce({ pageIndex: 0, width: 320, height: 480, bitmap: bitmaps[0] })
        .mockResolvedValueOnce({ pageIndex: 0, width: 512, height: 768, bitmap: bitmaps[1] })
        .mockResolvedValueOnce({ pageIndex: 0, width: 640, height: 960, bitmap: bitmaps[2] }),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    const held = await session.renderPageBitmap(0, 0.5, 1);
    const second = await session.renderPageBitmap(0, 0.8, 1);
    session.releasePageSurface(second);
    const third = await session.renderPageBitmap(0, 1, 1);
    session.releasePageSurface(third);

    expect(bitmaps[0].close).not.toHaveBeenCalled();
    expect(session.diagnostics()).toMatchObject({
      renderCacheEntries: 3,
    });

    session.releasePageSurface(held);
    expect(bitmaps[0].close).toHaveBeenCalledTimes(1);
  });

  it('suppresses nearby prefetch during viewport motion while still allowing initial target-page work', async () => {
    const renderDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(() => renderDeferred.promise),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    session.setViewportInMotion(true);
    const prefetchPromise = session.renderPage(0, 0.8, 1, {
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    });

    await Promise.resolve();
    expect(browserHandle.renderPageToBlob).not.toHaveBeenCalled();

    const targetPromise = session.renderPage(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-hq',
    });

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(1);
    });

    renderDeferred.resolve({
      pageIndex: 0,
      width: 320,
      height: 480,
      blob: new Blob(['x'], { type: 'image/png' }),
    });

    await targetPromise;
    session.setViewportInMotion(false);
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    });
    await prefetchPromise;
  });

  it('starts visible overview thumbnail work during viewport motion while holding nearby prefetch', async () => {
    const thumbnailDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(() => thumbnailDeferred.promise),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    session.setViewportInMotion(true);
    const prefetchPromise = session.renderPage(0, 0.8, 1, {
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    });
    await Promise.resolve();
    expect(browserHandle.renderPageToBlob).not.toHaveBeenCalled();

    const overviewPromise = session.renderThumbnail(0, {
      maxWidth: 32,
      maxHeight: 42,
      pageWidth: 320,
      pageHeight: 480,
      minScale: 0.025,
    }, 1, {
      urgency: 'visible',
      requestClass: 'overview-thumbnail',
    });

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(1);
    });

    thumbnailDeferred.resolve({
      pageIndex: 0,
      width: 32,
      height: 42,
      blob: new Blob(['overview'], { type: 'image/png' }),
    });
    await overviewPromise;

    session.setViewportInMotion(false);
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    });
    await prefetchPromise;
  });

  it('does not demote a queued promoted page task back to nearby prefetch', async () => {
    const firstDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const secondDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const thirdDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi
        .fn()
        .mockImplementationOnce(() => firstDeferred.promise)
        .mockImplementationOnce(() => secondDeferred.promise)
        .mockImplementationOnce(() => thirdDeferred.promise),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    const activeA = session.renderPage(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-preview',
    });
    const activeB = session.renderPage(1, 1, 1, {
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    });
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    });

    session.setViewportInMotion(true);
    const queued = session.renderPage(2, 0.8, 1, {
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    });
    await Promise.resolve();
    expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);

    session.updatePageRenderPriority(2, 0.8, 1, {
      urgency: 'visible',
      requestClass: 'target-page-preview',
      priority: 2000,
    });
    session.updatePageRenderPriority(2, 0.8, 1, {
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
      priority: 10,
    });

    firstDeferred.resolve({
      pageIndex: 0,
      width: 320,
      height: 480,
      blob: new Blob(['a'], { type: 'image/png' }),
    });
    secondDeferred.resolve({
      pageIndex: 1,
      width: 320,
      height: 480,
      blob: new Blob(['b'], { type: 'image/png' }),
    });

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(3);
    });

    thirdDeferred.resolve({
      pageIndex: 2,
      width: 256,
      height: 384,
      blob: new Blob(['c'], { type: 'image/png' }),
    });

    await Promise.all([activeA, activeB, queued]);
  });

  it('promotes a queued nearby-prefetch page task instead of recreating it', async () => {
    const firstDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const secondDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const thirdDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi
        .fn()
        .mockImplementationOnce(() => firstDeferred.promise)
        .mockImplementationOnce(() => secondDeferred.promise)
        .mockImplementationOnce(() => thirdDeferred.promise),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();

    const activeA = session.renderPage(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-preview',
    });
    const activeB = session.renderPage(1, 1, 1, {
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    });
    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    });

    const prefetchPromise = session.renderPage(2, 0.8, 1, {
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    });
    await Promise.resolve();
    expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);

    const promotedPromise = session.renderPage(2, 0.8, 1, {
      urgency: 'visible',
      requestClass: 'target-page-preview',
      priority: 2000,
    });

    await expect(prefetchPromise).rejects.toMatchObject({ name: 'AbortError' });

    firstDeferred.resolve({
      pageIndex: 0,
      width: 320,
      height: 480,
      blob: new Blob(['a'], { type: 'image/png' }),
    });
    secondDeferred.resolve({
      pageIndex: 1,
      width: 320,
      height: 480,
      blob: new Blob(['b'], { type: 'image/png' }),
    });

    await vi.waitFor(() => {
      expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(3);
    });

    thirdDeferred.resolve({
      pageIndex: 2,
      width: 256,
      height: 384,
      blob: new Blob(['c'], { type: 'image/png' }),
    });

    await Promise.all([activeA, activeB, promotedPromise]);
    expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(3);
  });

  it('holds warming thumbnail work during tab activation while allowing target page preview', async () => {
    vi.useFakeTimers();
    const firstDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const secondDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi
        .fn()
        .mockImplementationOnce(() => firstDeferred.promise)
        .mockImplementationOnce(() => secondDeferred.promise),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    session.activate();

    const thumbnailPromise = session.renderThumbnail(0, {
      maxWidth: 160,
      maxHeight: 220,
      pageWidth: 320,
      pageHeight: 480,
    }, 1, {
      urgency: 'prefetch',
      requestClass: 'warming',
    });
    await Promise.resolve();
    expect(browserHandle.renderPageToBlob).not.toHaveBeenCalled();

    const pagePromise = session.renderPage(0, 0.5, 1, {
      urgency: 'visible',
      requestClass: 'target-page-preview',
    });
    await Promise.resolve();
    expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(1);

    firstDeferred.resolve({
      pageIndex: 0,
      width: 160,
      height: 240,
      blob: new Blob(['page'], { type: 'image/png' }),
    });
    await pagePromise;

    await vi.advanceTimersByTimeAsync(451);
    expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(2);
    secondDeferred.resolve({
      pageIndex: 0,
      width: 160,
      height: 220,
      blob: new Blob(['thumbnail'], { type: 'image/png' }),
    });
    await thumbnailPromise;
  });

  it('drops queued warming work when a tab deactivates', async () => {
    vi.useFakeTimers();
    const firstDeferred = createDeferred<{ pageIndex: number; width: number; height: number; blob: Blob }>();
    const browserHandle = {
      getPageInfo: vi.fn(async () => ({ width: 320, height: 480 })),
      renderPageToBlob: vi.fn(() => firstDeferred.promise),
      close: vi.fn(async () => undefined),
    };
    mockedOpenPdfDocumentFromBytes.mockResolvedValue(browserHandle as never);

    const session = new LocalPdfSession('/tmp/fixture.pdf');
    await session.open();
    session.activate();

    const activeRender = session.renderPage(0, 1, 1, {
      urgency: 'visible',
      requestClass: 'target-page-preview',
    });
    await Promise.resolve();
    expect(browserHandle.renderPageToBlob).toHaveBeenCalledTimes(1);

    const queuedThumbnail = session.renderThumbnail(0, {
      maxWidth: 160,
      maxHeight: 220,
      pageWidth: 320,
      pageHeight: 480,
    }, 1, {
      urgency: 'prefetch',
      requestClass: 'warming',
    });
    await Promise.resolve();

    session.deactivate();
    await expect(queuedThumbnail).rejects.toMatchObject({ name: 'AbortError' });
    expect(session.diagnostics()).toMatchObject({
      inactive: true,
      queuedPageRenders: 0,
      queuedThumbnailRenders: 0,
    });

    firstDeferred.resolve({
      pageIndex: 0,
      width: 320,
      height: 480,
      blob: new Blob(['page'], { type: 'image/png' }),
    });
    await activeRender;
  });

});

describe('render backlog gating', () => {
  it('requires ready page and thumbnail paths with no queued or inflight work', () => {
    const idleDiagnostics = {
      pageRenderReady: true,
      thumbnailRenderReady: true,
      queuedPageRenders: 0,
      queuedThumbnailRenders: 0,
      inflightPageRenders: 0,
      inflightThumbnailRenders: 0,
    };

    expect(isRenderBacklogIdle(idleDiagnostics)).toBe(true);
    expect(isRenderBacklogIdle({ ...idleDiagnostics, thumbnailRenderReady: false })).toBe(false);
    expect(isRenderBacklogIdle({ ...idleDiagnostics, queuedPageRenders: 1 })).toBe(false);
    expect(isRenderBacklogIdle({ ...idleDiagnostics, inflightThumbnailRenders: 1 })).toBe(false);
  });
});

describe('adaptive overview thumbnail concurrency', () => {
  it('derives initial and ceiling limits from runtime hardware concurrency', () => {
    expect(resolveInitialOverviewThumbnailConcurrencyLimit(12, 3, 2)).toBe(6);
    expect(resolveOverviewThumbnailConcurrencyCeiling(12, 3, 2)).toBe(12);
    expect(resolveInitialOverviewThumbnailConcurrencyLimit(4, 3, 2)).toBe(2);
    expect(resolveOverviewThumbnailConcurrencyCeiling(0, 3, 2)).toBe(3);
  });
});

describe('activation critical render gating', () => {
  it('blocks only non-essential warming and prefetch while preserving target and visible thumbnail work', () => {
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('page', {
      urgency: 'visible',
      requestClass: 'target-page-preview',
    })).toBe(true);
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('page', {
      urgency: 'prefetch',
      requestClass: 'nearby-prefetch',
    })).toBe(false);
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('thumbnail', {
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    })).toBe(true);
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('thumbnail', {
      urgency: 'prefetch',
      requestClass: 'warming',
    })).toBe(false);
  });

  it('can hold non-target full page upgrades during activation-critical isolation', () => {
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('page', {
      urgency: 'visible',
      requestClass: 'target-page-hq',
    }, true)).toBe(true);
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('page', {
      urgency: 'visible',
      requestClass: 'visible-page-preview',
    }, true)).toBe(true);
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('page', {
      urgency: 'visible',
      requestClass: 'visible-page-hq-upgrade',
    }, true)).toBe(false);
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('thumbnail', {
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    }, true)).toBe(true);
  });

  it('keeps activation gating broad enough for full visible upgrades by default', () => {
    vi.stubGlobal('window', {
      butterPaper: {
        environment: {},
      },
    });

    expect(isRenderTaskAllowedDuringActivationCriticalWindow('page', {
      urgency: 'visible',
      requestClass: 'target-page-crop',
    })).toBe(true);
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('page', {
      urgency: 'visible',
      requestClass: 'visible-page-hq-upgrade',
    })).toBe(true);
    expect(isRenderTaskAllowedDuringActivationCriticalWindow('thumbnail', {
      urgency: 'visible',
      requestClass: 'visible-thumbnail',
    })).toBe(true);
  });
});
