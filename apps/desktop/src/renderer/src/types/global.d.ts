import type { ButterCanvasDocument, DocumentModel } from '@butter-paper/core';
import type { ButterPaperBridge, PerfSnapshot, ViewerDiagnostics, WindowState } from '../../../shared/protocol';
import type { CadViewOrganisation, SnapSettings } from '../state/viewerStore';

declare global {
  interface Window {
    butterPaper: ButterPaperBridge;
    __butterPaperDiagnostics?: () => ViewerDiagnostics;
    __butterPaperTestHooks?: {
      openDocumentPath: (filePath: string) => Promise<void>;
      openDocumentPaths: (filePaths: string[]) => Promise<void>;
      createButterCanvas: () => Promise<void>;
      openCanvasPath: (filePath: string) => Promise<void>;
      openCanvasPaths: (filePaths: string[]) => Promise<void>;
      importCanvasPdfPath: (filePath: string, pageSelection?: string) => Promise<void>;
      getActiveDocument: () => DocumentModel | null;
      getActiveCanvasDocument: () => ButterCanvasDocument | null;
      openFixturePdf: (fixtureName: string) => Promise<void>;
      switchToTab: (indexOrPath: number | string) => Promise<void>;
      closeTab: (indexOrPath: number | string) => Promise<void>;
      saveCurrentDocument: () => Promise<void>;
      saveCurrentDocumentAs: (filePath?: string) => Promise<void>;
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
