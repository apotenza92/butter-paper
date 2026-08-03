import electron from 'electron';
import { ipcChannels } from '../shared/ipc';
import type {
  BlankPdfCreateRequest,
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
  UpdateFrequency,
  UpdateStatus,
} from '../shared/protocol';

const { contextBridge, ipcRenderer } = electron;

const isTestMode = process.env.BP_TEST_MODE === '1';
const defaultSamplePdfPath = resolveDefaultSamplePdfPath();
let openPdfPathsListener: ((filePaths: string[]) => void) | null = null;
let closeRequestedListener: (() => void) | null = null;
const pendingOpenPdfPaths: string[][] = [];

ipcRenderer.on(ipcChannels.applicationOpenPdfPaths, (_event, filePaths: string[]) => {
  if (openPdfPathsListener) {
    openPdfPathsListener(filePaths);
  } else {
    pendingOpenPdfPaths.push(filePaths);
  }
});

ipcRenderer.on(ipcChannels.applicationCloseRequested, () => {
  closeRequestedListener?.();
});

const bridge: ButterPaperBridge = {
  environment: {
    testMode: isTestMode,
    defaultSamplePdfPath,
    cadRenderExperiment: process.env.BP_CAD_RENDER_EXPERIMENT ?? null,
    renderCoordinatorV2: process.env.BP_RENDER_COORDINATOR_V2 === '1',
  },
  application: {
    getMetadata: async () => ipcRenderer.invoke(ipcChannels.applicationGetMetadata),
    setAsDefaultPdfApp: async () => ipcRenderer.invoke(ipcChannels.applicationSetDefaultPdfApp),
    takePendingPdfPaths: async () => ipcRenderer.invoke(ipcChannels.applicationTakePendingPdfPaths),
    onOpenPdfPaths: (listener: (filePaths: string[]) => void) => {
      openPdfPathsListener = listener;
      for (const filePaths of pendingOpenPdfPaths.splice(0)) {
        listener(filePaths);
      }
      return () => {
        if (openPdfPathsListener === listener) {
          openPdfPathsListener = null;
        }
      };
    },
    setCloseBlocked: async (blocked: boolean) => {
      await ipcRenderer.invoke(ipcChannels.applicationSetCloseBlocked, blocked);
    },
    onCloseRequested: (listener: () => void) => {
      closeRequestedListener = listener;
      return () => {
        if (closeRequestedListener === listener) {
          closeRequestedListener = null;
        }
      };
    },
    confirmClose: async () => {
      await ipcRenderer.invoke(ipcChannels.applicationConfirmClose);
    },
    cancelClose: async () => {
      await ipcRenderer.invoke(ipcChannels.applicationCancelClose);
    },
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
  updates: {
    getStatus: async () => ipcRenderer.invoke(ipcChannels.updatesGetStatus),
    setFrequency: async (frequency: UpdateFrequency) => ipcRenderer.invoke(ipcChannels.updatesSetFrequency, frequency),
    checkNow: async () => ipcRenderer.invoke(ipcChannels.updatesCheckNow),
    installDownloaded: async () => {
      await ipcRenderer.invoke(ipcChannels.updatesInstallDownloaded);
    },
    setRestartBlocked: async (blocked: boolean) => {
      await ipcRenderer.invoke(ipcChannels.updatesSetRestartBlocked, blocked);
    },
    openReleasePage: async () => {
      await ipcRenderer.invoke(ipcChannels.updatesOpenReleasePage);
    },
    onStatusChanged: (listener: (status: UpdateStatus) => void) => {
      const handleStatusChanged = (_event: electron.IpcRendererEvent, status: UpdateStatus) => {
        listener(status);
      };

      ipcRenderer.on(ipcChannels.updatesStatusChanged, handleStatusChanged);
      return () => {
        ipcRenderer.off(ipcChannels.updatesStatusChanged, handleStatusChanged);
      };
    },
  },
  dialogs: {
    openPdfDialog: async () => ipcRenderer.invoke(ipcChannels.dialogOpenPdf),
    savePdfAsDialog: async (defaultPath?: string) => ipcRenderer.invoke(ipcChannels.dialogSavePdfAs, defaultPath),
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
    createBlankDocument: async (request: BlankPdfCreateRequest) => ipcRenderer.invoke(ipcChannels.pdfCreateBlankDocument, request),
    releaseTemporaryDocument: async (temporarySourcePath: string) => {
      await ipcRenderer.invoke(ipcChannels.pdfReleaseTemporaryDocument, temporarySourcePath);
    },
    loadDocument: async (filePath: string) => ipcRenderer.invoke(ipcChannels.pdfLoadDocument, filePath),
    getPageGeometry: async (request: PageGeometryRequest) => ipcRenderer.invoke(ipcChannels.pdfGetPageGeometry, request),
    saveDocument: async (request: SaveDocumentRequest) => ipcRenderer.invoke(ipcChannels.pdfSaveDocument, request),
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
