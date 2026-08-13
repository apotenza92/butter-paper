import type { ArcMarkup, AreaMarkup, ArrowMarkup, CalloutMarkup, CloudMarkup, CloudPlusMarkup, DimensionMarkup, EllipseMarkup, HighlightMarkup, ImageMarkup, ImportedAnnotationMarkup, LengthMarkup, LineMarkup, PdfPoint, PenMarkup, PolygonMarkup, PolylengthMarkup, PolylineMarkup, Rect, RectangleMarkup, RedactMarkup, SnapshotMarkup, TextBoxMarkup } from '@butter-paper/core';

export interface PdfPageRotation {
  readonly pageIndex: number;
  readonly rotation: 0 | 90 | 180 | 270;
}

export interface PdfDocumentMetadata {
  readonly title?: string;
  readonly author?: string;
  readonly subject?: string;
  readonly creator?: string;
  readonly producer?: string;
  readonly pageCount: number;
}

export interface PdfPageInfo {
  readonly index: number;
  readonly width: number;
  readonly height: number;
  readonly rotation: 0 | 90 | 180 | 270;
  /** Effective visible page box from PDF.js `page.view`, in unrotated PDF coordinates. */
  readonly viewBox?: Rect;
  /** PDF /UserUnit multiplier. Real PDF.js pages provide this. */
  readonly userUnit?: number;
}

export interface PdfRenderRequest {
  readonly pageIndex: number;
  readonly scale: number;
  readonly rotation?: 0 | 90 | 180 | 270;
  readonly canvas?: PdfCanvasLike;
  readonly renderAnnotations?: boolean;
}

export interface PdfBlobRenderRequest {
  readonly pageIndex: number;
  readonly scale: number;
  readonly rotation?: 0 | 90 | 180 | 270;
  readonly signal?: AbortSignal;
  readonly renderAnnotations?: boolean;
}

export interface PdfRenderedPage {
  readonly pageIndex: number;
  readonly width: number;
  readonly height: number;
  readonly canvas: PdfCanvasLike;
}

export interface PdfRenderedBlob {
  readonly pageIndex: number;
  readonly width: number;
  readonly height: number;
  readonly blob: Blob;
}

export interface PdfRenderedBitmap {
  readonly pageIndex: number;
  readonly width: number;
  readonly height: number;
  readonly bitmap: ImageBitmap;
}

export interface PdfCanvasLike {
  width: number;
  height: number;
  getContext(type: '2d'): CanvasRenderingContext2D | null;
}

export interface PdfCanvasFactory {
  create(width: number, height: number): Promise<PdfCanvasLike> | PdfCanvasLike;
  destroy(canvas: PdfCanvasLike): Promise<void> | void;
}

export interface PdfCacheStats {
  readonly entries: number;
  readonly estimatedBytes: number;
  readonly maxEntries: number;
  readonly maxBytes: number;
}

export type PdfContentPrimitive =
  | {
    readonly kind: 'line';
    readonly start: PdfPoint;
    readonly end: PdfPoint;
  }
  | {
    readonly kind: 'rect';
    readonly rect: Rect;
  }
  | {
    readonly kind: 'polyline';
    readonly points: readonly PdfPoint[];
    readonly closed: boolean;
  };

export interface PdfPageGeometryIndex {
  readonly pageIndex: number;
  readonly primitives: readonly PdfContentPrimitive[];
  readonly pageGrid?: PdfPageGridDefinition;
  readonly buildMs: number;
}

export interface PdfPageGridDefinition {
  readonly type: 'rectangular' | 'ruled' | 'isometric' | 'triangle';
  readonly origin: PdfPoint;
  readonly spacing: number;
  readonly width: number;
  readonly height: number;
  readonly rotationDegrees: number;
  readonly source: 'generated' | 'detected' | 'manual';
}

export type ImportedPdfMarkup = RectangleMarkup | RedactMarkup | EllipseMarkup | ArcMarkup | LineMarkup | ArrowMarkup | DimensionMarkup | LengthMarkup | PolylengthMarkup | AreaMarkup | PolylineMarkup | PolygonMarkup | PenMarkup | HighlightMarkup | CloudMarkup | CloudPlusMarkup | TextBoxMarkup | CalloutMarkup | ImageMarkup | SnapshotMarkup | ImportedAnnotationMarkup;

export type PdfSaveMode = 'save' | 'saveAs';

export interface PdfSaveResult {
  readonly path: string;
  readonly bytesWritten: number;
}
