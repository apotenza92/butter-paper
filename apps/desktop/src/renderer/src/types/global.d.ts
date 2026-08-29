import type { DocumentModel, PdfPoint } from '@butter-paper/core';
import type { HandleBehavior } from '../pdf-tools/types';
import type { BlankPdfCreateRequest, ButterPaperBridge, PerfSnapshot, ViewerDiagnostics, WindowState } from '../../../shared/protocol';
import type { CadViewOrganisation, SnapSettings } from '../state/viewerStore';

declare global {
  type ButterPaperTestMarkupMutation =
    | { readonly kind: 'replace-text'; readonly markupId: string; readonly text: string }
    | { readonly kind: 'translate'; readonly markupId: string; readonly delta: PdfPoint }
    | {
        readonly kind: 'set-properties';
        readonly markupId: string;
        readonly values: Partial<Record<'x' | 'y' | 'width' | 'height' | 'opacity', number>>;
      }
    | {
        readonly kind: 'tool-transform';
        readonly markupId: string;
        readonly handleId: string;
        readonly handleBehavior: HandleBehavior;
        readonly startPoint: PdfPoint;
        readonly currentPoint: PdfPoint;
      };

  interface Window {
    butterPaper: ButterPaperBridge;
    __butterPaperDiagnostics?: () => ViewerDiagnostics;
    __butterPaperTestHooks?: {
      openDocumentPath: (filePath: string) => Promise<void>;
      openDocumentPaths: (filePaths: string[]) => Promise<void>;
      createBlankPdf: (request: BlankPdfCreateRequest) => Promise<void>;
      getActiveDocument: () => DocumentModel | null;
      replaceDocumentMarkups: (
        markups: DocumentModel['markups'],
        pageScales: NonNullable<DocumentModel['pageScales']>,
        selectedMarkupIds: readonly string[],
        recordHistory?: boolean,
      ) => void;
      queryMarkupSpatialIndex: (pageIndex: number, point: PdfPoint, tolerance: number) => {
        pageIndex: number;
        totalMarkupCount: number;
        indexedMarkupCount: number;
        queriedCellCount: number;
        candidateMarkupIds: readonly string[];
        generation: ReadonlyArray<{ id: string; bounds: { x: number; y: number; width: number; height: number } }>;
        hitMarkupId: string | null;
      };
      selectMarkupAtPoint: (pageIndex: number, point: PdfPoint, tolerance: number) => string | null;
      applyMarkupMutation: (mutation: ButterPaperTestMarkupMutation) => void;
      undoDocument: () => boolean;
      redoDocument: () => boolean;
      getDocumentHistory: () => { past: number; future: number; currentRevision: number; savedRevision: number };
      openFixturePdf: (fixtureName: string) => Promise<void>;
      switchToTab: (indexOrPath: number | string) => Promise<void>;
      closeTab: (indexOrPath: number | string) => Promise<void>;
      saveCurrentDocument: () => Promise<void>;
      saveCurrentDocumentAs: (filePath: string) => Promise<void>;
      getDiagnostics: () => ViewerDiagnostics;
      getPerfSnapshot: () => PerfSnapshot;
      resetPerfSnapshot: () => void;
      setSnapSettings: (settings: Partial<SnapSettings>) => void;
      setPageColumnsEnabled: (enabled: boolean) => void;
      setCadViewOrganisation: (organisation: CadViewOrganisation) => void;
      setPagesPerColumn: (count: number) => void;
      setZoom: (zoom: number) => void;
      getWindowState: () => Promise<WindowState | null>;
      setWindowBounds: (bounds: Partial<WindowState['bounds']>) => Promise<WindowState | null>;
    };
  }
}

export {};
