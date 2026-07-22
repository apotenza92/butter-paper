import electron from 'electron';
import { ipcChannels } from '../shared/ipc';
import type { ButterCanvasDocument } from '@butter-paper/core';
import type {
  ButterPaperBridge,
  RenderCoreCloseDocumentRequest,
  RenderCoreGetPageInfoRequest,
  RenderCoreOpenDocumentRequest,
  RenderCoreReadSurfaceRequest,
  RenderCoreReleaseSurfaceRequest,
  RenderCoreRenderPageRequest,
  PageGeometryRequest,
  SaveDocumentRequest,
  ThemeSnapshot,
} from '../shared/protocol';

const { contextBridge, ipcRenderer } = electron;

const isTestMode = process.env.BP_TEST_MODE === '1';
const defaultSamplePdfPath = resolveDefaultSamplePdfPath();

const bridge: ButterPaperBridge = {
  environment: {
    testMode: isTestMode,
    defaultSamplePdfPath,
    cadRenderExperiment: process.env.BP_CAD_RENDER_EXPERIMENT ?? null,
    renderCoordinatorV2: process.env.BP_RENDER_COORDINATOR_V2 === '1',
  },
  theme: {
    getSnapshot: async () => ipcRenderer.invoke(ipcChannels.themeGetSnapshot),
    subscribe: (listener: (snapshot: ThemeSnapshot) => void) => {
      const handleThemeChanged = (_event: electron.IpcRendererEvent, snapshot: ThemeSnapshot) => {
        listener(snapshot);
      };

      ipcRenderer.on(ipcChannels.themeChanged, handleThemeChanged);
      return () => {
        ipcRenderer.off(ipcChannels.themeChanged, handleThemeChanged);
      };
    },
  },
  dialogs: {
    openPdfDialog: async () => ipcRenderer.invoke(ipcChannels.dialogOpenPdf),
    savePdfAsDialog: async (defaultPath?: string) => ipcRenderer.invoke(ipcChannels.dialogSavePdfAs, defaultPath),
    openCanvasDialog: async () => ipcRenderer.invoke(ipcChannels.dialogOpenCanvas),
    saveCanvasAsDialog: async (defaultPath?: string) => ipcRenderer.invoke(ipcChannels.dialogSaveCanvasAs, defaultPath),
  },
  files: {
    readFile: async (filePath: string) => {
      const bytes = await ipcRenderer.invoke(ipcChannels.fileRead, filePath);
      return new Uint8Array(bytes);
    },
    writeFile: async (filePath: string, bytes: Uint8Array) => {
      await ipcRenderer.invoke(ipcChannels.fileWrite, filePath, bytes);
    },
  },
  pdf: {
    loadDocument: async (filePath: string) => ipcRenderer.invoke(ipcChannels.pdfLoadDocument, filePath),
    getPageGeometry: async (request: PageGeometryRequest) => ipcRenderer.invoke(ipcChannels.pdfGetPageGeometry, request),
    saveDocument: async (request: SaveDocumentRequest) => ipcRenderer.invoke(ipcChannels.pdfSaveDocument, request),
  },
  canvas: {
    readDocument: async (filePath: string) => ipcRenderer.invoke(ipcChannels.canvasReadDocument, filePath),
    writeDocument: async (filePath: string, document: ButterCanvasDocument) => {
      await ipcRenderer.invoke(ipcChannels.canvasWriteDocument, filePath, document);
    },
  },
  renderCore: {
    getBackendConfig: async () => ipcRenderer.invoke(ipcChannels.renderCoreGetBackendConfig),
    getBackendSelection: async () => ipcRenderer.invoke(ipcChannels.renderCoreGetBackendSelection),
    getCapabilities: async () => ipcRenderer.invoke(ipcChannels.renderCoreGetCapabilities),
    getDiagnostics: async () => ipcRenderer.invoke(ipcChannels.renderCoreGetDiagnostics),
    openDocument: async (request: RenderCoreOpenDocumentRequest) =>
      ipcRenderer.invoke(ipcChannels.renderCoreOpenDocument, request),
    getPageInfo: async (request: RenderCoreGetPageInfoRequest) =>
      ipcRenderer.invoke(ipcChannels.renderCoreGetPageInfo, request),
    renderPage: async (request: RenderCoreRenderPageRequest) =>
      ipcRenderer.invoke(ipcChannels.renderCoreRenderPage, request),
    readSurface: async (request: RenderCoreReadSurfaceRequest) => {
      const response = await ipcRenderer.invoke(ipcChannels.renderCoreReadSurface, request);
      if (response && typeof response === 'object' && response.ok === true) {
        return {
          ...response,
          value: {
            ...response.value,
            bytes: new Uint8Array(response.value.bytes),
          },
        };
      }

      return response;
    },
    releaseSurface: async (request: RenderCoreReleaseSurfaceRequest) =>
      ipcRenderer.invoke(ipcChannels.renderCoreReleaseSurface, request),
    closeDocument: async (request: RenderCoreCloseDocumentRequest) =>
      ipcRenderer.invoke(ipcChannels.renderCoreCloseDocument, request),
  },
  test: isTestMode
    ? {
        resolveFixturePath: async (name: string) => ipcRenderer.invoke(ipcChannels.testResolveFixture, name),
        getWindowState: async () => ipcRenderer.invoke(ipcChannels.testGetWindowState),
        setWindowBounds: async (bounds) => ipcRenderer.invoke(ipcChannels.testSetWindowBounds, bounds),
      }
    : null,
};

contextBridge.exposeInMainWorld('butterPaper', bridge);

function resolveDefaultSamplePdfPath(): string | null {
  if (isTestMode || process.env.BP_OPEN_SAMPLE_PDF === '0') {
    return null;
  }

  return process.env.BP_DEFAULT_SAMPLE_PDF?.trim() || null;
}
