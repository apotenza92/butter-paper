import electron from 'electron';
import { ipcChannels } from '../shared/ipc';
import { resolveCadViewEnabled } from '../shared/featureFlags';
import type {
  BlankPdfCreateRequest,
  ButterPaperBridge,
  ApplicationMenuCommand,
  ApplicationMenuState,
  PageGeometryRequest,
  PdfDocumentAccessRequest,
  PhoneSignatureMode,
  SaveDocumentRequest,
  ThemeSnapshot,
  UpdateFrequency,
  UpdateStatus,
} from '../shared/protocol';

const { contextBridge, ipcRenderer, webUtils } = electron;

const isTestMode = process.env.BP_TEST_MODE === '1';
const defaultSamplePdfPath = resolveDefaultSamplePdfPath();
let openPdfPathsListener: ((filePaths: string[]) => void) | null = null;
let closeRequestedListener: (() => void) | null = null;
let closeTabRequestedListener: (() => void) | null = null;
let menuCommandListener: ((command: ApplicationMenuCommand) => void) | null = null;
let menuBarVisibilityListener: ((visible: boolean) => void) | null = null;
let windowFullScreenListener: ((fullScreen: boolean) => void) | null = null;
const pendingOpenPdfPaths: string[][] = [];
const pendingCloseRequests: true[] = [];
const pendingCloseTabRequests: true[] = [];
const pendingMenuCommands: ApplicationMenuCommand[] = [];
const pendingMenuBarVisibilityChanges: boolean[] = [];
const pendingWindowFullScreenChanges: boolean[] = [];

ipcRenderer.on(ipcChannels.applicationOpenPdfPaths, (_event, filePaths: string[]) => {
  if (openPdfPathsListener) {
    openPdfPathsListener(filePaths);
  } else {
    pendingOpenPdfPaths.push(filePaths);
  }
});

ipcRenderer.on(ipcChannels.applicationCloseRequested, () => {
  if (closeRequestedListener) closeRequestedListener();
  else pendingCloseRequests.push(true);
});

ipcRenderer.on(ipcChannels.applicationCloseTabRequested, () => {
  if (closeTabRequestedListener) closeTabRequestedListener();
  else pendingCloseTabRequests.push(true);
});

ipcRenderer.on(ipcChannels.applicationMenuCommand, (_event, command: ApplicationMenuCommand) => {
  if (menuCommandListener) menuCommandListener(command);
  else pendingMenuCommands.push(command);
});

ipcRenderer.on(ipcChannels.applicationMenuBarVisibilityChanged, (_event, visible: boolean) => {
  if (menuBarVisibilityListener) menuBarVisibilityListener(visible);
  else pendingMenuBarVisibilityChanges.push(visible);
});

ipcRenderer.on(ipcChannels.applicationWindowFullScreenChanged, (_event, fullScreen: boolean) => {
  if (windowFullScreenListener) windowFullScreenListener(fullScreen);
  else pendingWindowFullScreenChanges.push(fullScreen);
});

const bridge: ButterPaperBridge = {
  environment: {
    testMode: isTestMode,
    defaultSamplePdfPath,
    cadViewEnabled: resolveCadViewEnabled(process.env.BP_CAD_VIEW_ENABLED),
    cadRenderExperiment: process.env.BP_CAD_RENDER_EXPERIMENT ?? null,
    renderCoordinatorV2: process.env.BP_RENDER_COORDINATOR_V2 === '1',
  },
  application: {
    getMetadata: async () => ipcRenderer.invoke(ipcChannels.applicationGetMetadata),
    setAsDefaultPdfApp: async () => ipcRenderer.invoke(ipcChannels.applicationSetDefaultPdfApp),
    takePendingPdfPaths: async () => ipcRenderer.invoke(ipcChannels.applicationTakePendingPdfPaths),
    authorizeDroppedPdf: async (file: File) => {
      const filePath = webUtils.getPathForFile(file);
      return ipcRenderer.invoke(ipcChannels.applicationAuthorizeDroppedPdf, filePath);
    },
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
      for (const _request of pendingCloseRequests.splice(0)) listener();
      return () => {
        if (closeRequestedListener === listener) {
          closeRequestedListener = null;
        }
      };
    },
    onCloseTabRequested: (listener: () => void) => {
      closeTabRequestedListener = listener;
      for (const _request of pendingCloseTabRequests.splice(0)) listener();
      return () => {
        if (closeTabRequestedListener === listener) {
          closeTabRequestedListener = null;
        }
      };
    },
    onMenuCommand: (listener: (command: ApplicationMenuCommand) => void) => {
      menuCommandListener = listener;
      for (const command of pendingMenuCommands.splice(0)) listener(command);
      return () => {
        if (menuCommandListener === listener) {
          menuCommandListener = null;
        }
      };
    },
    onMenuBarVisibilityChanged: (listener: (visible: boolean) => void) => {
      menuBarVisibilityListener = listener;
      for (const visible of pendingMenuBarVisibilityChanges.splice(0)) listener(visible);
      return () => {
        if (menuBarVisibilityListener === listener) {
          menuBarVisibilityListener = null;
        }
      };
    },
    getWindowFullScreen: async () => ipcRenderer.invoke(ipcChannels.applicationGetWindowFullScreen),
    onWindowFullScreenChanged: (listener: (fullScreen: boolean) => void) => {
      windowFullScreenListener = listener;
      for (const fullScreen of pendingWindowFullScreenChanges.splice(0)) listener(fullScreen);
      return () => {
        if (windowFullScreenListener === listener) {
          windowFullScreenListener = null;
        }
      };
    },
    toggleWindowFullScreen: async () => {
      await ipcRenderer.invoke(ipcChannels.applicationToggleWindowFullScreen);
    },
    reloadWindow: async (force: boolean) => {
      await ipcRenderer.invoke(ipcChannels.applicationReloadWindow, force);
    },
    setMenuBarVisibility: async (visible: boolean) => {
      await ipcRenderer.invoke(ipcChannels.applicationMenuBarVisibilityChanged, visible);
    },
    setMenuState: async (state: ApplicationMenuState) => {
      await ipcRenderer.invoke(ipcChannels.applicationSetMenuState, state);
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
  signaturePhone: {
    start: async (mode: PhoneSignatureMode) => ipcRenderer.invoke(ipcChannels.signaturePhoneStart, mode),
    poll: async (sessionId: string) => ipcRenderer.invoke(ipcChannels.signaturePhonePoll, sessionId),
    stop: async (sessionId: string) => {
      await ipcRenderer.invoke(ipcChannels.signaturePhoneStop, sessionId);
    },
  },
  signatureRecent: {
    list: async () => ipcRenderer.invoke(ipcChannels.signatureRecentList),
    remember: async (asset) => ipcRenderer.invoke(ipcChannels.signatureRecentRemember, asset),
    remove: async (id: string) => ipcRenderer.invoke(ipcChannels.signatureRecentRemove, id),
    clear: async () => ipcRenderer.invoke(ipcChannels.signatureRecentClear),
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
