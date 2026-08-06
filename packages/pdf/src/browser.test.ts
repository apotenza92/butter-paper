import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

function createDeferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

describe('BrowserPdfDocumentHandle', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('aborts if the signal fires while waiting for the page before render start', async () => {
    const getPageDeferred = createDeferred<any>();
    const render = vi.fn();

    vi.doMock('pdfjs-dist/legacy/build/pdf.mjs', () => ({
      GlobalWorkerOptions: { workerSrc: '' },
      getDocument: () => ({
        promise: Promise.resolve({
          numPages: 1,
          getMetadata: vi.fn(async () => ({ info: {} })),
          getPage: vi.fn(() => getPageDeferred.promise),
          destroy: vi.fn(async () => undefined),
        }),
        destroy: vi.fn(async () => undefined),
      }),
    }));

    const { openPdfDocumentFromBytes } = await import('./browser.js');
    const handle = await openPdfDocumentFromBytes(new Uint8Array([1, 2, 3]));
    const abortController = new AbortController();
    const renderPromise = handle.renderPageToBlob({
      pageIndex: 0,
      scale: 1,
      signal: abortController.signal,
    });

    abortController.abort();
    getPageDeferred.resolve({
      rotate: 0,
      view: [0, 0, 100, 100],
      userUnit: 1,
      getViewport: () => ({ width: 100, height: 100 }),
      render,
      cleanup: vi.fn(async () => undefined),
    });

    await expect(renderPromise).rejects.toMatchObject({ name: 'AbortError' });
    expect(render).not.toHaveBeenCalled();
  });

  it('renders a page to ImageBitmap without blob conversion', async () => {
    const renderTask = {
      promise: Promise.resolve(),
      cancel: vi.fn(),
    };
    const cleanup = vi.fn(async () => undefined);
    const canvas = {
      width: 0,
      height: 0,
      getContext: vi.fn(() => ({ clearRect: vi.fn(), drawImage: vi.fn() })),
      toBlob: vi.fn(),
    };
    const bitmap = { close: vi.fn() } as unknown as ImageBitmap;

    vi.stubGlobal('document', {
      createElement: vi.fn(() => canvas),
    });
    vi.stubGlobal('createImageBitmap', vi.fn(async () => bitmap));

    vi.doMock('pdfjs-dist/legacy/build/pdf.mjs', () => ({
      GlobalWorkerOptions: { workerSrc: '' },
      getDocument: () => ({
        promise: Promise.resolve({
          numPages: 1,
          getMetadata: vi.fn(async () => ({ info: {} })),
          getPage: vi.fn(async () => ({
            rotate: 0,
            view: [0, 0, 100, 200],
            userUnit: 1,
            getViewport: ({ scale }: { scale: number }) => ({ width: 100 * scale, height: 200 * scale }),
            render: vi.fn(() => renderTask),
            cleanup,
          })),
          destroy: vi.fn(async () => undefined),
        }),
        destroy: vi.fn(async () => undefined),
      }),
    }));

    const { openPdfDocumentFromBytes } = await import('./browser.js');
    const handle = await openPdfDocumentFromBytes(new Uint8Array([1, 2, 3]));
    const rendered = await handle.renderPageToBitmap({ pageIndex: 0, scale: 2 });

    expect(rendered.width).toBe(200);
    expect(rendered.height).toBe(400);
    expect(rendered.bitmap).toBe(bitmap);
    expect(createImageBitmap).toHaveBeenCalledWith(canvas);
    expect(canvas.toBlob).not.toHaveBeenCalled();
    expect(cleanup).toHaveBeenCalledTimes(1);
  });

  it('retains the effective PDF.js view box and UserUnit in page information', async () => {
    vi.doMock('pdfjs-dist/legacy/build/pdf.mjs', () => ({
      GlobalWorkerOptions: { workerSrc: '' },
      getDocument: () => ({
        promise: Promise.resolve({
          numPages: 1,
          getMetadata: vi.fn(async () => ({ info: {} })),
          getPage: vi.fn(async () => ({
            rotate: 270,
            view: [36, 72, 576, 792],
            userUnit: 2,
            getViewport: vi.fn(() => ({ width: 1440, height: 1080 })),
            render: vi.fn(),
          })),
          destroy: vi.fn(async () => undefined),
        }),
        destroy: vi.fn(async () => undefined),
      }),
    }));

    const { openPdfDocumentFromBytes } = await import('./browser.js');
    const handle = await openPdfDocumentFromBytes(new Uint8Array([1, 2, 3]));

    await expect(handle.getPageInfo(0)).resolves.toEqual({
      index: 0,
      width: 1440,
      height: 1080,
      rotation: 270,
      viewBox: { x: 36, y: 72, width: 540, height: 720 },
      userUnit: 2,
    });
  });
});
