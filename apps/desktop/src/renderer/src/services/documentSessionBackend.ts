import type { Markup, PageModel, PageScale } from '@butter-paper/core';
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
  LoadedDocumentPayload,
  PdfSaveTargetDescriptor,
  PdfRect,
  PdfRenderRequestClass,
} from '../../../shared/protocol';

export interface PdfSessionRenderRequest extends PdfBlobRenderRequest {
  readonly requestClass?: PdfRenderRequestClass;
  readonly cropPdfRect?: PdfRect;
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
    sessionBackendKind: 'pdfjs';
    surfaceTransportKind: 'pdfjs-blob-url';
  };
  openStageTimings: {
    mainPayloadMs: number;
    rendererFileReadMs: number;
    rendererBrowserOpenMs: number;
  };
}

export interface PdfSessionSaveArgs {
  documentHandle: string;
  target?: PdfSaveTargetDescriptor;
  markups: readonly Markup[];
  pageScales?: readonly PageScale[];
  pageRotations?: ReadonlyArray<{
    readonly pageIndex: number;
    readonly rotation: PageModel['rotation'];
  }>;
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

      try {
        const fileReadStartedAt = performance.now();
        const bytes = await window.butterPaper.pdf.readDocumentBytes({
          documentHandle: payload.documentAccess.handle,
        });
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
      } catch (error) {
        await window.butterPaper.pdf.releaseDocument({
          documentHandle: payload.documentAccess.handle,
        }).catch(() => undefined);
        throw error;
      }
    },
    async save({ documentHandle, target, markups, pageScales, pageRotations }) {
      const request = {
        documentHandle,
        markups,
        pageScales,
        pageRotations,
      };
      return target
        ? await window.butterPaper.pdf.saveDocument({
            ...request,
            targetHandle: target.targetHandle,
            mode: 'saveAs',
          })
        : await window.butterPaper.pdf.saveDocument({
            ...request,
            mode: 'save',
          });
    },
  };
}

export function createDesktopSessionBackend(): PdfSessionBackend {
  return createPdfJsSessionBackend();
}
