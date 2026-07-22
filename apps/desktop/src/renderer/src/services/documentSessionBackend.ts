import type { Markup, PageScale } from '@butter-paper/core';
import {
  openPdfDocumentFromBytes,
  type BrowserPdfDocumentHandle,
} from '@butter-paper/pdf/browser';
import type {
  PdfBlobRenderRequest,
  PdfPageInfo,
  PdfRenderedBitmap,
  PdfRenderedBlob,
} from '@butter-paper/pdf';
import type {
  DesktopRenderCapabilities,
  DesktopRenderBackendSelection,
  LoadedDocumentPayload,
  RenderCoreRenderMode,
  RenderCoreRenderRequestClass,
  RenderCoreOpenDocumentResponse,
  RenderCorePageInfo,
  RenderCorePdfRect,
  RenderCoreReadSurfaceResponse,
  RenderCoreRenderPageResponse,
  RenderCoreResult,
} from '../../../shared/protocol';

export interface PdfSessionRenderRequest extends PdfBlobRenderRequest {
  readonly renderMode?: RenderCoreRenderMode;
  readonly requestClass?: RenderCoreRenderRequestClass;
  readonly cropPdfRect?: RenderCorePdfRect;
}

export interface PdfSessionDocumentHandle {
  getPageInfo(pageIndex: number): Promise<PdfPageInfo>;
  renderPageToBlob(request: PdfSessionRenderRequest): Promise<PdfRenderedBlob>;
  renderPageToBitmap(request: PdfSessionRenderRequest): Promise<PdfRenderedBitmap>;
  close(): Promise<void>;
}

export interface PdfSessionBackendOpenResult {
  payload: LoadedDocumentPayload;
  handle: PdfSessionDocumentHandle;
  backendInfo: {
    sessionBackendKind: 'pdfjs' | 'pdfium-render-core';
    surfaceTransportKind: 'pdfjs-blob-url' | 'pdfium-png-bridge';
  };
  openStageTimings: {
    mainPayloadMs: number;
    rendererFileReadMs: number;
    rendererBrowserOpenMs: number;
  };
}

export interface PdfSessionSaveArgs {
  sourcePath: string;
  targetPath?: string;
  markups: readonly Markup[];
  pageScales?: readonly PageScale[];
}

export interface PdfSessionBackend {
  readonly kind: string;
  open(filePath: string): Promise<PdfSessionBackendOpenResult>;
  save(args: PdfSessionSaveArgs): Promise<{ path: string }>;
}

export function createPdfJsSessionBackend(): PdfSessionBackend {
  return {
    kind: 'pdfjs',
    async open(filePath) {
      const mainPayloadStartedAt = performance.now();
      const payload = await window.butterPaper.pdf.loadDocument(filePath);
      const mainPayloadMs = performance.now() - mainPayloadStartedAt;

      const fileReadStartedAt = performance.now();
      const bytes = await window.butterPaper.files.readFile(payload.filePath);
      const rendererFileReadMs = performance.now() - fileReadStartedAt;

      const browserOpenStartedAt = performance.now();
      const handle = await openPdfDocumentFromBytes(bytes);
      const rendererBrowserOpenMs = performance.now() - browserOpenStartedAt;

      return {
        payload,
        handle: handle as BrowserPdfDocumentHandle,
        backendInfo: {
          sessionBackendKind: 'pdfjs',
          surfaceTransportKind: 'pdfjs-blob-url',
        },
        openStageTimings: {
          mainPayloadMs,
          rendererFileReadMs,
          rendererBrowserOpenMs,
        },
      };
    },
    async save({ sourcePath, targetPath, markups, pageScales }) {
      return await window.butterPaper.pdf.saveDocument({
        sourcePath,
        targetPath,
        markups,
        pageScales,
        mode: targetPath ? 'saveAs' : 'save',
      });
    },
  };
}

export function createRenderCoreSessionBackend(): PdfSessionBackend {
  return {
    kind: 'pdfium-render-core',
    async open(filePath) {
      const mainPayloadStartedAt = performance.now();
      const payload = await window.butterPaper.pdf.loadDocument(filePath);
      const mainPayloadMs = performance.now() - mainPayloadStartedAt;

      const renderCoreOpenStartedAt = performance.now();
      const opened = unwrapRenderCoreResult<RenderCoreOpenDocumentResponse>(
        await window.butterPaper.renderCore.openDocument({
          filePath: payload.filePath,
          password: null,
        }),
      );
      const rendererBrowserOpenMs = performance.now() - renderCoreOpenStartedAt;

      const pageInfoCache = createPageInfoCacheFromPayload(payload);

      const handle: PdfSessionDocumentHandle = {
        async getPageInfo(pageIndex) {
          const cached = pageInfoCache.get(pageIndex);
          if (cached) {
            return cached;
          }

          const pageInfo = unwrapRenderCoreResult<RenderCorePageInfo>(
            await window.butterPaper.renderCore.getPageInfo({
              documentId: opened.documentId,
              pageIndex,
            }),
          );

          const normalized: PdfPageInfo = {
            index: pageIndex,
            width: pageInfo.width,
            height: pageInfo.height,
            rotation: normalizeRotation(pageInfo.rotation),
          };
          pageInfoCache.set(pageIndex, normalized);
          return normalized;
        },
        async renderPageToBlob(request) {
          const pageInfo = await handle.getPageInfo(request.pageIndex);
          const renderWidth = request.cropPdfRect?.width ?? pageInfo.width;
          const renderHeight = request.cropPdfRect?.height ?? pageInfo.height;
          const rendered = unwrapRenderCoreResult<RenderCoreRenderPageResponse>(
            await window.butterPaper.renderCore.renderPage({
              documentId: opened.documentId,
              pageIndex: request.pageIndex,
              target: {
                width: renderWidth,
                height: renderHeight,
                scale: request.scale,
              },
              rotation: request.rotation,
              cropPdfRect: request.cropPdfRect,
              renderMode: request.renderMode,
              requestClass: request.requestClass,
            }),
          );

          try {
            const surface = unwrapRenderCoreResult<RenderCoreReadSurfaceResponse>(
              await window.butterPaper.renderCore.readSurface({
                surfaceId: rendered.surfaceId,
              }),
            );
            const pngBytes = new Uint8Array(surface.bytes);
            return {
              pageIndex: request.pageIndex,
              width: rendered.pixelWidth,
              height: rendered.pixelHeight,
              blob: new Blob([
                pngBytes.buffer.slice(pngBytes.byteOffset, pngBytes.byteOffset + pngBytes.byteLength),
              ], { type: 'image/png' }),
            };
          } finally {
            await window.butterPaper.renderCore.releaseSurface({
              surfaceId: rendered.surfaceId,
            });
          }
        },
        async renderPageToBitmap(request) {
          const rendered = await handle.renderPageToBlob(request);
          return {
            pageIndex: rendered.pageIndex,
            width: rendered.width,
            height: rendered.height,
            bitmap: await createImageBitmap(rendered.blob),
          };
        },
        async close() {
          unwrapRenderCoreResult(
            await window.butterPaper.renderCore.closeDocument({
              documentId: opened.documentId,
            }),
          );
        },
      };

      return {
        payload,
        handle,
        backendInfo: {
          sessionBackendKind: 'pdfium-render-core',
          surfaceTransportKind: 'pdfium-png-bridge',
        },
        openStageTimings: {
          mainPayloadMs,
          rendererFileReadMs: 0,
          rendererBrowserOpenMs,
        },
      };
    },
    async save({ sourcePath, targetPath, markups, pageScales }) {
      return await window.butterPaper.pdf.saveDocument({
        sourcePath,
        targetPath,
        markups,
        pageScales,
        mode: targetPath ? 'saveAs' : 'save',
      });
    },
  };
}

export function createDesktopSessionBackend(): PdfSessionBackend {
  const pdfJsBackend = createPdfJsSessionBackend();
  const renderCoreBackend = createRenderCoreSessionBackend();

  return {
    kind: 'desktop-auto',
    async open(filePath) {
      const renderCore = window.butterPaper.renderCore;
      if (!renderCore) {
        return await pdfJsBackend.open(filePath);
      }

      const [selection, capabilities] = await Promise.all([
        renderCore.getBackendSelection().catch(() => null as DesktopRenderBackendSelection | null),
        renderCore.getCapabilities().catch(() => null as DesktopRenderCapabilities | null),
      ]);

      if (
        selection?.activeBackend === 'pdfium'
        && capabilities?.backend === 'pdfium'
        && capabilities.available
        && capabilities.canOpenDocument
        && capabilities.canGetPageInfo
        && capabilities.canRenderPage
        && capabilities.canReadSurface
        && capabilities.canReleaseSurface
        && capabilities.canCloseDocument
      ) {
        return await renderCoreBackend.open(filePath);
      }

      return await pdfJsBackend.open(filePath);
    },
    async save(args) {
      return await pdfJsBackend.save(args);
    },
  };
}

function unwrapRenderCoreResult<T>(result: RenderCoreResult<T>): T {
  if (result.ok) {
    return result.value;
  }

  throw new Error(result.error.message);
}

function createPageInfoCacheFromPayload(payload: LoadedDocumentPayload): Map<number, PdfPageInfo> {
  const cache = new Map<number, PdfPageInfo>();

  for (const page of payload.document.pages) {
    if (
      !Number.isInteger(page.index) ||
      page.index < 0 ||
      typeof page.size?.width !== 'number' ||
      !Number.isFinite(page.size.width) ||
      page.size.width <= 0 ||
      typeof page.size?.height !== 'number' ||
      !Number.isFinite(page.size.height) ||
      page.size.height <= 0
    ) {
      continue;
    }

    cache.set(page.index, {
      index: page.index,
      width: page.size.width,
      height: page.size.height,
      rotation: normalizeRotation(page.rotation),
    });
  }

  return cache;
}

function normalizeRotation(rotation: number): 0 | 90 | 180 | 270 {
  const normalized = ((rotation % 360) + 360) % 360;
  if (normalized === 90 || normalized === 180 || normalized === 270) {
    return normalized;
  }

  return 0;
}
