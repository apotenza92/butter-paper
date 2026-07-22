import type {
  PdfBlobRenderRequest,
  PdfDocumentMetadata,
  PdfPageInfo,
  PdfRenderedBitmap,
  PdfRenderedBlob,
} from './types.js';

interface PdfJsDocumentLike {
  numPages: number;
  getMetadata(): Promise<{ info?: Record<string, unknown> }>;
  getPage(pageNumber: number): Promise<PdfJsPageLike>;
  destroy(): Promise<void>;
}

interface PdfJsRenderTaskLike {
  promise: Promise<void>;
  cancel(extraDelay?: number): void;
}

interface PdfJsPageLike {
  rotate: number;
  getViewport(params: { scale: number; rotation?: number }): { width: number; height: number };
  render(params: {
    canvasContext: CanvasRenderingContext2D;
    viewport: { width: number; height: number };
    annotationMode?: number;
  }): PdfJsRenderTaskLike;
  cleanup?(): Promise<void>;
}

interface PdfLoadingTaskLike {
  promise: Promise<PdfJsDocumentLike>;
  destroy(): Promise<void> | void;
}

interface PdfJsModuleLike {
  getDocument: (params: unknown) => PdfLoadingTaskLike;
  GlobalWorkerOptions: { workerSrc: string };
}

let pdfjsModulePromise: Promise<PdfJsModuleLike> | undefined;
const pdfWorkerSrc = new URL('pdfjs-dist/build/pdf.worker.mjs', import.meta.url).toString();
const pdfWasmSrc = resolvePdfJsWasmSrc();

export class BrowserPdfDocumentHandle {
  private constructor(
    private readonly loadingTask: PdfLoadingTaskLike,
    private readonly document: PdfJsDocumentLike,
  ) {}

  static async open(bytes: Uint8Array): Promise<BrowserPdfDocumentHandle> {
    const pdfjs = await loadPdfJsBrowserModule();
    const loadingTask = pdfjs.getDocument({
      data: bytes.slice(),
      useSystemFonts: true,
      isOffscreenCanvasSupported: false,
      wasmUrl: pdfWasmSrc,
    }) as PdfLoadingTaskLike;
    const document = await loadingTask.promise;
    return new BrowserPdfDocumentHandle(loadingTask, document);
  }

  async getMetadata(): Promise<PdfDocumentMetadata> {
    const metadata = await this.document.getMetadata().catch(() => undefined);
    const info = metadata?.info ?? {};

    return {
      pageCount: this.document.numPages,
      title: stringOrUndefined(info.Title),
      author: stringOrUndefined(info.Author),
      subject: stringOrUndefined(info.Subject),
      creator: stringOrUndefined(info.Creator),
      producer: stringOrUndefined(info.Producer),
    };
  }

  async getPageInfo(pageIndex: number): Promise<PdfPageInfo> {
    const page = await this.getPage(pageIndex);
    const rotation = normalizeRotation(page.rotate);
    const viewport = page.getViewport({ scale: 1, rotation });

    return {
      index: pageIndex,
      width: viewport.width,
      height: viewport.height,
      rotation,
    };
  }

  async renderPageToBlob(request: PdfBlobRenderRequest): Promise<PdfRenderedBlob> {
    const rendered = await this.renderPageToCanvas(request);
    return {
      pageIndex: rendered.pageIndex,
      width: rendered.width,
      height: rendered.height,
      blob: await canvasToBlob(rendered.canvas),
    };
  }

  async renderPageToBitmap(request: PdfBlobRenderRequest): Promise<PdfRenderedBitmap> {
    const rendered = await this.renderPageToCanvas(request);
    return {
      pageIndex: rendered.pageIndex,
      width: rendered.width,
      height: rendered.height,
      bitmap: await canvasToImageBitmap(rendered.canvas),
    };
  }

  async close(): Promise<void> {
    await this.document.destroy();
    await this.loadingTask.destroy();
  }

  private async renderPageToCanvas(
    request: PdfBlobRenderRequest,
  ): Promise<{ pageIndex: number; width: number; height: number; canvas: HTMLCanvasElement }> {
    if (request.signal?.aborted) {
      throw createAbortError();
    }

    let renderTask: PdfJsRenderTaskLike | null = null;
    let aborted = false;
    const abortHandler = () => {
      aborted = true;
      renderTask?.cancel();
    };
    request.signal?.addEventListener('abort', abortHandler, { once: true });

    try {
      if (request.signal?.aborted || aborted) {
        throw createAbortError();
      }

      const page = await this.getPage(request.pageIndex);
      if (request.signal?.aborted || aborted) {
        throw createAbortError();
      }

      const rotation = request.rotation ?? normalizeRotation(page.rotate);
      const viewport = page.getViewport({ scale: request.scale, rotation });
      const canvas = document.createElement('canvas');
      canvas.width = Math.max(1, Math.ceil(viewport.width));
      canvas.height = Math.max(1, Math.ceil(viewport.height));

      const context = canvas.getContext('2d', { alpha: false });
      if (!context) {
        throw new Error('Canvas rendering is not available.');
      }

      renderTask = page.render({
        canvasContext: context,
        viewport,
        annotationMode: request.renderAnnotations ? 1 : 0,
      });

      if (request.signal?.aborted || aborted) {
        renderTask.cancel();
        throw createAbortError();
      }

      try {
        await renderTask.promise;
      } catch (error) {
        if (request.signal?.aborted || aborted || isRenderCancelledError(error)) {
          throw createAbortError();
        }
        throw error;
      }

      try {
        await page.cleanup?.();
      } catch {
        // Some pdf.js builds return a non-critical cleanup failure here.
      }

      return {
        pageIndex: request.pageIndex,
        width: canvas.width,
        height: canvas.height,
        canvas,
      };
    } finally {
      request.signal?.removeEventListener('abort', abortHandler);
    }
  }

  private async getPage(pageIndex: number): Promise<PdfJsPageLike> {
    if (pageIndex < 0 || pageIndex >= this.document.numPages) {
      throw new RangeError(`Page ${pageIndex + 1} is outside this document.`);
    }

    return await this.document.getPage(pageIndex + 1);
  }
}

export async function openPdfDocumentFromBytes(bytes: Uint8Array): Promise<BrowserPdfDocumentHandle> {
  return await BrowserPdfDocumentHandle.open(bytes);
}

async function loadPdfJsBrowserModule(): Promise<PdfJsModuleLike> {
  pdfjsModulePromise ??= (async () => {
    const pdfjs = await import('pdfjs-dist/legacy/build/pdf.mjs');
    pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerSrc;
    return pdfjs as unknown as PdfJsModuleLike;
  })();

  return await pdfjsModulePromise;
}

async function canvasToBlob(canvas: HTMLCanvasElement): Promise<Blob> {
  return await new Promise<Blob>((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (!blob) {
        reject(new Error('Failed to rasterise the current page.'));
        return;
      }

      resolve(blob);
    }, 'image/png');
  });
}

async function canvasToImageBitmap(canvas: HTMLCanvasElement): Promise<ImageBitmap> {
  if (typeof createImageBitmap !== 'function') {
    throw new Error('ImageBitmap rendering is not available.');
  }

  return await createImageBitmap(canvas);
}

function normalizeRotation(rotation: number): 0 | 90 | 180 | 270 {
  const normalized = ((rotation % 360) + 360) % 360;
  if (normalized === 90 || normalized === 180 || normalized === 270) {
    return normalized;
  }

  return 0;
}

function createAbortError(): Error {
  const error = new Error('Render request aborted.');
  error.name = 'AbortError';
  return error;
}

function isRenderCancelledError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }

  return error.name === 'RenderingCancelledException' || error.name === 'AbortException';
}

function resolvePdfJsWasmSrc(): string | undefined {
  const locationHref = globalThis.location?.href;
  if (!locationHref) {
    return undefined;
  }

  return new URL('assets/pdfjs-wasm/', locationHref).toString();
}

function stringOrUndefined(value: unknown): string | undefined {
  if (typeof value === 'string') {
    return value;
  }

  if (value && typeof value === 'object' && 'value' in value) {
    const nestedValue = (value as { value?: unknown }).value;
    return typeof nestedValue === 'string' ? nestedValue : undefined;
  }

  return undefined;
}
