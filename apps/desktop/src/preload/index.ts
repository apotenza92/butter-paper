import electron from 'electron';
import { ipcChannels } from '../shared/ipc';
import type {
  BlankPdfCreateRequest,
  ButterPaperBridge,
  PageGeometryRequest,
  PdfDocumentAccessRequest,
  SaveDocumentRequest,
  ThemeSnapshot,
  UpdateFrequency,
  UpdateStatus,
} from '../shared/protocol';
import type { SigningApprovalRequest } from '../shared/signingProtocol';

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
    requestQuit: async () => {
      await ipcRenderer.invoke(ipcChannels.applicationRequestQuit);
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
  pdf: {
    createBlankDocument: async (request: BlankPdfCreateRequest) => ipcRenderer.invoke(ipcChannels.pdfCreateBlankDocument, request),
    loadDocument: async (filePath: string) => ipcRenderer.invoke(ipcChannels.pdfLoadDocument, filePath),
    readDocumentBytes: async (request: PdfDocumentAccessRequest) => {
      const bytes = await ipcRenderer.invoke(ipcChannels.fileRead, request);
      return new Uint8Array(bytes);
    },
    releaseDocument: async (request: PdfDocumentAccessRequest) => {
      await ipcRenderer.invoke(ipcChannels.pdfReleaseDocument, request);
    },
    getPageGeometry: async (request: PageGeometryRequest) => ipcRenderer.invoke(ipcChannels.pdfGetPageGeometry, request),
    saveDocument: async (request: SaveDocumentRequest) => ipcRenderer.invoke(ipcChannels.pdfSaveDocument, request),
  },
  signing: {
    chooseIdentity: async () => ipcRenderer.invoke(ipcChannels.signingChooseIdentity),
    approve: async (request: SigningApprovalRequest) => ipcRenderer.invoke(ipcChannels.signingApprove, request),
  },
  test: isTestMode
    ? {
        resolveFixturePath: async (name: string) => ipcRenderer.invoke(ipcChannels.testResolveFixture, name),
        authorizePdfSource: async (filePath: string) => ipcRenderer.invoke(ipcChannels.testAuthorizePdfSource, filePath),
        authorizePdfSaveTarget: async (filePath: string) => ipcRenderer.invoke(ipcChannels.testAuthorizePdfSaveTarget, filePath),
        getWindowState: async () => ipcRenderer.invoke(ipcChannels.testGetWindowState),
        setWindowBounds: async (bounds) => ipcRenderer.invoke(ipcChannels.testSetWindowBounds, bounds),
        getProcessMetrics: async () => ipcRenderer.invoke(ipcChannels.testGetProcessMetrics),
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
