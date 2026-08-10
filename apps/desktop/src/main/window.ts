import electron from 'electron';
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { BrowserWindow as BrowserWindowInstance, Event as ElectronEvent, RenderProcessGoneDetails } from 'electron';
import { ipcChannels } from '../shared/ipc';
import type {
  ApplicationMetadata,
  BlankPdfCreateRequest,
  LoadedDocumentSignatureProtection,
  PageGeometryRequest,
  PdfDocumentAccessRequest,
  SaveDocumentRequest,
  ThemeMode,
  ThemeSnapshot,
  UpdateFrequency,
} from '../shared/protocol';
import { resolveApplicationMetadata } from './applicationMetadata';
import { ancestorFileCandidates } from './applicationPaths';
import { BlankPdfTemporaryStore } from './blankPdfTemporaryStore';
import { setAsDefaultPdfApp } from './defaultPdfApp';
import { takePendingPdfPaths } from './pendingPdfPaths';
import { desktopPdfAccessRegistry } from './pdfAccessRegistry';
import {
  loadDocumentPayload,
  loadPageGeometryIndex,
  PdfSignedSourcePolicyError,
  saveDocumentPayload,
} from './pdfSession';
import {
  analyzePdfSignatureDocument,
  createDefaultPdfSignatureDocumentAnalysisDependencies,
} from './pdfSignatureDocumentAnalysis';
import { PdfSignatureCoreSigningClient } from './pdfSignatureCoreSigning';
import {
  PdfSigningIdentityRegistry,
  PdfSigningIdentityRegistryError,
  type PdfSigningIdentityOwnerLease,
} from './pdfSigningIdentityRegistry';
import { PdfSigningQuarantine } from './pdfSigningQuarantine';
import { executePdfSigningApproval } from './pdfSigningApprovalOrchestrator';
import { createPdfSignedMutation } from './pdfSignedMutationWorkflow';
import { SignatureDocumentRegistry } from './signatureDocumentRegistry';
import {
  assertSigningApprovalRequest,
  signingCapabilityDisabledResult,
  signingIdentitySelectionDisabledResult,
} from './signingApprovalBoundary';
import { offlineSignatureTrustConfigurationSha256 } from './signatureTrustPolicy';
import { createDesktopProcessMetricsSnapshot } from './processMetrics';
import { getFocusedWindowState, isTestModeEnabled, resolveFixturePath, setFocusedWindowBounds } from './testMode';
import { DesktopUpdaterService, loadElectronAutoUpdater } from './updater';
import { resolveReleasePageUrl } from './releasePage';
import { MAIN_WINDOW_GEOMETRY } from './windowGeometry';
import {
  loadWindowBounds,
  resolveRestoredWindowBounds,
  WINDOW_STATE_FILE_NAME,
  writeWindowBoundsAtomic,
} from './windowState';

const { app, BrowserWindow, ipcMain, dialog, nativeTheme, screen, shell } = electron;
const moduleDir = dirname(fileURLToPath(import.meta.url));
const preloadPath = join(moduleDir, 'preload.cjs');
const defaultSamplePdfPath = join(moduleDir, '../../../..', 'tests/fixtures/generated/zoom-target.pdf');
let mainWindow: BrowserWindowInstance | null = null;
let themeListenerRegistered = false;
let updaterService: DesktopUpdaterService | null = null;
let unsubscribeUpdaterStatus: (() => void) | null = null;
let blankPdfTemporaryStore: BlankPdfTemporaryStore | null = null;
let signingQuarantine: PdfSigningQuarantine | null = null;
const signatureDocumentRegistry = new SignatureDocumentRegistry();
const signatureDocumentHandleByPdfHandle = new Map<string, string>();
const signatureProtectionByPdfHandle = new Map<string, LoadedDocumentSignatureProtection>();
const signatureOwnerWebContentsIdByPdfHandle = new Map<string, number>();
const signingIdentityOwnerLeaseByWebContentsId = new Map<number, PdfSigningIdentityOwnerLease>();
// The initial release keeps certificate mutation disabled until the commercial,
// platform, legal, and security gates are independently accepted.
const PDF_CERTIFICATE_SIGNING_ENABLED = false as const;
const signingIdentityRegistry = new PdfSigningIdentityRegistry(
  {
    pickPkcs12File: async (ownerWindowId) => {
      const ownerWindow = BrowserWindow.fromId(ownerWindowId);
      const result = ownerWindow
        ? await dialog.showOpenDialog(ownerWindow, {
            title: 'Choose signing identity',
            properties: ['openFile'],
            filters: [{ name: 'PKCS#12 identity', extensions: ['p12', 'pfx'] }],
          })
        : await dialog.showOpenDialog({
            title: 'Choose signing identity',
            properties: ['openFile'],
            filters: [{ name: 'PKCS#12 identity', extensions: ['p12', 'pfx'] }],
          });
      return result.canceled || !result.filePaths[0]
        ? { canceled: true as const }
        : { canceled: false as const, filePath: result.filePaths[0] };
    },
  },
  {
    inspectPkcs12: async (pkcs12Frame, options) => (
      new PdfSignatureCoreSigningClient(createPdfSignatureCoreOptions()).inspectPkcs12(pkcs12Frame, options)
    ),
  },
);
let applicationQuitRequested = false;
const closeBlockedWebContents = new Set<number>();
const closeConfirmedWebContents = new Set<number>();

declare const MAIN_WINDOW_VITE_DEV_SERVER_URL: string | undefined;
declare const MAIN_WINDOW_VITE_NAME: string | undefined;

if (
  !isTestModeEnabled()
  && process.env.BP_OPEN_SAMPLE_PDF !== '0'
  && !process.env.BP_DEFAULT_SAMPLE_PDF?.trim()
  && existsSync(defaultSamplePdfPath)
) {
  process.env.BP_DEFAULT_SAMPLE_PDF = defaultSamplePdfPath;
}

function revealWindow(window: BrowserWindowInstance): void {
  if (window.isDestroyed()) {
    return;
  }

  if (window.isMinimized()) {
    window.restore();
  }

  window.show();
  if (process.platform === 'darwin') {
    app.focus({ steal: true });
  }
  window.focus();
}

function resolveThemeMode(): ThemeMode {
  if (isTestModeEnabled()) {
    const forcedTheme = process.env.BP_TEST_THEME?.trim().toLowerCase();
    if (forcedTheme === 'light' || forcedTheme === 'dark') {
      return forcedTheme;
    }
  }

  return nativeTheme.shouldUseDarkColors ? 'dark' : 'light';
}

function getThemeSnapshot(): ThemeSnapshot {
  return { mode: resolveThemeMode() };
}

function getWindowBackgroundColor(mode: ThemeMode): string {
  return mode === 'dark' ? '#111315' : '#f5f5f5';
}

function notifyThemeChanged(snapshot: ThemeSnapshot): void {
  for (const window of BrowserWindow.getAllWindows()) {
    window.setBackgroundColor(getWindowBackgroundColor(snapshot.mode));
    window.webContents.send(ipcChannels.themeChanged, snapshot);
  }
}

function registerThemeListener(): void {
  if (themeListenerRegistered) {
    return;
  }

  nativeTheme.on('updated', () => {
    if (isTestModeEnabled() && process.env.BP_TEST_THEME) {
      return;
    }

    notifyThemeChanged(getThemeSnapshot());
  });

  themeListenerRegistered = true;
}

interface RendererFailurePageOptions {
  readonly errorCode: number;
  readonly errorDescription: string;
  readonly rendererDevServerUrl?: string;
  readonly rendererHtmlPath: string;
  readonly validatedUrl: string;
}

async function loadRendererFailurePage(
  window: BrowserWindowInstance,
  options: RendererFailurePageOptions,
): Promise<void> {
  if (window.isDestroyed()) {
    return;
  }

  const canLoadFileFallback = options.rendererDevServerUrl && existsSync(options.rendererHtmlPath);
  if (canLoadFileFallback) {
    try {
      await window.loadFile(options.rendererHtmlPath);
      return;
    } catch (error) {
      console.error('Renderer fallback file failed to load', error);
    }
  }

  const detailLines = [
    ['Error code', String(options.errorCode)],
    ['Description', options.errorDescription || 'Unknown load failure'],
    ['URL', options.validatedUrl || options.rendererDevServerUrl || options.rendererHtmlPath],
  ];
  const details = detailLines
    .map(([label, value]) => `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value)}</dd>`)
    .join('');
  const html = `<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Butter Paper failed to load</title>
  <style>
    html, body { height: 100%; margin: 0; }
    body {
      align-items: center;
      background: #f4f4f5;
      color: #18181b;
      display: flex;
      font: 13px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      justify-content: center;
    }
    main {
      background: white;
      border: 1px solid #d4d4d8;
      border-radius: 8px;
      box-shadow: 0 12px 32px rgba(24, 24, 27, 0.12);
      max-width: 560px;
      padding: 24px;
    }
    h1 { font-size: 18px; margin: 0 0 8px; }
    p { color: #52525b; line-height: 1.45; margin: 0 0 16px; }
    dl { display: grid; gap: 8px 16px; grid-template-columns: max-content 1fr; margin: 0; }
    dt { color: #71717a; font-weight: 600; }
    dd { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; margin: 0; overflow-wrap: anywhere; }
  </style>
</head>
<body>
  <main>
    <h1>Butter Paper failed to load its renderer.</h1>
    <p>The app window is not blank. The renderer entry point could not be loaded. If this is a development launch, start the desktop app through the dev command so the Vite renderer server is running.</p>
    <dl>${details}</dl>
  </main>
</body>
</html>`;

  await window.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(html)}`);
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => {
    switch (character) {
      case '&':
        return '&amp;';
      case '<':
        return '&lt;';
      case '>':
        return '&gt;';
      case '"':
        return '&quot;';
      default:
        return '&#39;';
    }
  });
}

export function createMainWindow(): BrowserWindowInstance {
  const testMode = isTestModeEnabled();
  const shouldPersistWindowState = !testMode || Boolean(process.env.BP_TEST_USER_DATA_DIR?.trim());
  const windowStatePath = shouldPersistWindowState
    ? join(app.getPath('userData'), WINDOW_STATE_FILE_NAME)
    : null;
  const restoredWindowBounds = resolveRestoredWindowBounds(
    windowStatePath == null ? null : loadWindowBounds(windowStatePath),
    screen.getAllDisplays().map((display) => display.workArea),
    { width: MAIN_WINDOW_GEOMETRY.minWidth, height: MAIN_WINDOW_GEOMETRY.minHeight },
  );
  const applicationMetadata = getApplicationMetadata();
  const rendererDevServerUrl =
    app.isPackaged || process.env.BP_DISABLE_RENDERER_DEV_SERVER === '1'
      ? undefined
      : process.env.BP_RENDERER_DEV_SERVER_URL
        ?? (typeof MAIN_WINDOW_VITE_DEV_SERVER_URL === 'string' ? MAIN_WINDOW_VITE_DEV_SERVER_URL : undefined);
  const rendererName =
    typeof MAIN_WINDOW_VITE_NAME === 'string' && MAIN_WINDOW_VITE_NAME.length > 0 ? MAIN_WINDOW_VITE_NAME : 'main_window';
  const rendererHtmlPath = join(moduleDir, '../renderer', rendererName, 'index.html');
  const themeSnapshot = getThemeSnapshot();
  let window: BrowserWindowInstance;
  try {
    window = new BrowserWindow({
      ...MAIN_WINDOW_GEOMETRY,
      ...restoredWindowBounds,
      backgroundColor: getWindowBackgroundColor(themeSnapshot.mode),
      title: applicationMetadata.windowTitle,
      show: testMode,
      webPreferences: {
        preload: preloadPath,
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
      },
    });
    desktopPdfAccessRegistry.registerOwner(window.webContents.id);
  } catch (error) {
    console.error('Failed to construct main window', error);
    throw error;
  }
  const webContentsId = window.webContents.id;
  const ownerLease = signingIdentityRegistry.registerOwner(window.id);
  signingIdentityOwnerLeaseByWebContentsId.set(webContentsId, ownerLease);
  let ownerCapabilitiesCleared = false;
  const clearOwnerCapabilities = () => {
    if (ownerCapabilitiesCleared) {
      return;
    }
    ownerCapabilitiesCleared = true;
    signingIdentityRegistry.revokeWindow(window.id, ownerLease.generation);
    if (signingIdentityOwnerLeaseByWebContentsId.get(webContentsId) === ownerLease) {
      signingIdentityOwnerLeaseByWebContentsId.delete(webContentsId);
    }
    for (const documentHandle of desktopPdfAccessRegistry.listDocumentHandles(webContentsId)) {
      forgetSignatureDocument(documentHandle);
    }
    for (const [documentHandle, ownerId] of signatureOwnerWebContentsIdByPdfHandle) {
      if (ownerId === webContentsId) forgetSignatureDocument(documentHandle);
    }
    const cleanupPaths = desktopPdfAccessRegistry.clearOwner(webContentsId);
    for (const cleanupPath of cleanupPaths) {
      void blankPdfTemporaryStore?.release(cleanupPath).catch(() => undefined);
    }
  };

  if (!testMode) {
    window.once('ready-to-show', () => {
      revealWindow(window);
    });
    setTimeout(() => {
      revealWindow(window);
    }, 2_500).unref();
  }

  window.webContents.on('did-fail-load', (
    _event: ElectronEvent,
    errorCode: number,
    errorDescription: string,
    validatedUrl: string,
    isMainFrame: boolean,
  ) => {
    console.error('Renderer failed to load', { errorCode, errorDescription, validatedUrl, isMainFrame });
    if (isMainFrame) {
      void loadRendererFailurePage(window, {
        errorCode,
        errorDescription,
        rendererDevServerUrl,
        rendererHtmlPath,
        validatedUrl,
      });
    }
  });

  window.webContents.on('render-process-gone', (_event: ElectronEvent, details: RenderProcessGoneDetails) => {
    console.error('Renderer process gone', details);
    clearOwnerCapabilities();
    if (!window.isDestroyed()) window.destroy();
  });
  window.webContents.once('destroyed', clearOwnerCapabilities);

  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  window.webContents.on('will-navigate', (event) => {
    event.preventDefault();
  });

  if (rendererDevServerUrl) {
    void window.loadURL(rendererDevServerUrl);
  } else {
    void window.loadFile(rendererHtmlPath);
  }

  let windowStateSaveTimer: ReturnType<typeof setTimeout> | null = null;
  const saveWindowState = () => {
    if (windowStateSaveTimer != null) {
      clearTimeout(windowStateSaveTimer);
      windowStateSaveTimer = null;
    }
    if (windowStatePath == null || window.isDestroyed()) {
      return;
    }
    try {
      writeWindowBoundsAtomic(windowStatePath, window.getNormalBounds());
    } catch (error) {
      console.warn('Unable to save window size and position.', error);
    }
  };
  const scheduleWindowStateSave = () => {
    if (windowStatePath == null) {
      return;
    }
    if (windowStateSaveTimer != null) {
      clearTimeout(windowStateSaveTimer);
    }
    windowStateSaveTimer = setTimeout(saveWindowState, 250);
    windowStateSaveTimer.unref();
  };
  window.on('resize', scheduleWindowStateSave);
  window.on('move', scheduleWindowStateSave);
  window.on('close', (event) => {
    saveWindowState();
    if (closeBlockedWebContents.has(webContentsId) && !closeConfirmedWebContents.has(webContentsId)) {
      event.preventDefault();
      if (!window.webContents.isDestroyed()) {
        window.webContents.send(ipcChannels.applicationCloseRequested);
      }
      return;
    }
    closeBlockedWebContents.delete(webContentsId);
    closeConfirmedWebContents.delete(webContentsId);
  });
  window.on('closed', () => {
    if (windowStateSaveTimer != null) {
      clearTimeout(windowStateSaveTimer);
      windowStateSaveTimer = null;
    }
    if (mainWindow === window) {
      mainWindow = null;
    }
    clearOwnerCapabilities();
  });

  mainWindow = window;
  return window;
}

export function registerIpcHandlers(): void {
  ipcMain.handle(ipcChannels.applicationGetMetadata, async () => {
    return getApplicationMetadata();
  });

  ipcMain.handle(ipcChannels.applicationSetDefaultPdfApp, async () => {
    const metadata = getApplicationMetadata();
    return setAsDefaultPdfApp({
      platform: process.platform,
      isPackaged: app.isPackaged,
      productName: metadata.productName,
      packageName: metadata.channel === 'beta' ? 'butter-paper-beta' : 'butter-paper',
      executablePath: app.getPath('exe'),
      resourcesPath: process.resourcesPath,
      openExternal: async (url) => {
        await shell.openExternal(url);
      },
    });
  });

  ipcMain.handle(ipcChannels.applicationTakePendingPdfPaths, async (event) => {
    const paths = takePendingPdfPaths();
    return await Promise.all(paths.map((filePath) => desktopPdfAccessRegistry.authorizeSource(event.sender.id, filePath)));
  });

  ipcMain.handle(ipcChannels.applicationSetCloseBlocked, async (event, blocked: boolean) => {
    if (typeof blocked !== 'boolean') {
      throw new TypeError('Application close-blocked state must be a boolean.');
    }
    if (blocked) {
      closeBlockedWebContents.add(event.sender.id);
      closeConfirmedWebContents.delete(event.sender.id);
    } else {
      closeBlockedWebContents.delete(event.sender.id);
    }
  });

  ipcMain.handle(ipcChannels.applicationRequestQuit, async () => {
    app.quit();
  });

  ipcMain.handle(ipcChannels.applicationConfirmClose, async (event) => {
    const window = BrowserWindow.fromWebContents(event.sender);
    if (!window || window.isDestroyed()) {
      return;
    }
    closeConfirmedWebContents.add(event.sender.id);
    if (applicationQuitRequested) {
      app.quit();
    } else {
      window.close();
    }
  });

  ipcMain.handle(ipcChannels.applicationCancelClose, async () => {
    applicationQuitRequested = false;
  });

  ipcMain.handle(ipcChannels.updatesGetStatus, async () => {
    return requireUpdaterService().getStatus();
  });

  ipcMain.handle(ipcChannels.updatesSetFrequency, async (_event, frequency: UpdateFrequency) => {
    const service = requireUpdaterService();
    await service.setFrequency(frequency);
    return service.getStatus();
  });

  ipcMain.handle(ipcChannels.updatesCheckNow, async () => {
    const service = requireUpdaterService();
    await service.checkNow();
    return service.getStatus();
  });

  ipcMain.handle(ipcChannels.updatesInstallDownloaded, async () => {
    if (!await requireUpdaterService().installDownloaded()) {
      throw new Error('No downloaded Butter Paper update is ready to install.');
    }
  });

  ipcMain.handle(ipcChannels.updatesSetRestartBlocked, async (_event, blocked: boolean) => {
    if (typeof blocked !== 'boolean') {
      throw new TypeError('Update restart-blocked state must be a boolean.');
    }
    requireUpdaterService().setRestartBlocked(blocked);
  });

  ipcMain.handle(ipcChannels.updatesOpenReleasePage, async () => {
    const availableVersion = requireUpdaterService().getStatus().availableVersion;
    await shell.openExternal(resolveReleasePageUrl(availableVersion));
  });

  ipcMain.handle(ipcChannels.dialogOpenPdf, async (event) => {
    const window = BrowserWindow.getFocusedWindow();
    const result = window
      ? await dialog.showOpenDialog(window, {
          title: 'Open PDFs',
          properties: ['openFile', 'multiSelections'],
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
        })
      : await dialog.showOpenDialog({
          title: 'Open PDFs',
          properties: ['openFile', 'multiSelections'],
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
        });

    if (result.canceled || result.filePaths.length === 0) {
      return null;
    }

    return await Promise.all(result.filePaths.map((filePath) => (
      desktopPdfAccessRegistry.authorizeSource(event.sender.id, filePath)
    )));
  });

  ipcMain.handle(ipcChannels.dialogSavePdfAs, async (event, defaultPath?: string) => {
    const window = BrowserWindow.getFocusedWindow();
    const result = window
      ? await dialog.showSaveDialog(window, {
          title: 'Save PDF As',
          defaultPath: defaultPath ?? 'butter-paper-annotated.pdf',
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
        })
      : await dialog.showSaveDialog({
          title: 'Save PDF As',
          defaultPath: defaultPath ?? 'butter-paper-annotated.pdf',
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
        });

    if (result.canceled || !result.filePath) {
      return null;
    }

    return await desktopPdfAccessRegistry.authorizeSaveTarget(event.sender.id, result.filePath);
  });

  ipcMain.handle(ipcChannels.fileRead, async (event, request: PdfDocumentAccessRequest) => {
    assertPdfDocumentAccessRequest(request, []);
    return await desktopPdfAccessRegistry.readDocumentBytes(event.sender.id, request.documentHandle);
  });

  ipcMain.handle(ipcChannels.pdfCreateBlankDocument, async (event, request: BlankPdfCreateRequest) => {
    const temporaryDocument = await requireBlankPdfTemporaryStore().create(request);
    try {
      await desktopPdfAccessRegistry.authorizeSource(event.sender.id, temporaryDocument.filePath, {
        cleanupOnRelease: true,
      });
      return temporaryDocument;
    } catch (error) {
      await requireBlankPdfTemporaryStore().release(temporaryDocument.temporarySourcePath).catch(() => undefined);
      throw error;
    }
  });

  ipcMain.handle(ipcChannels.pdfReleaseDocument, async (event, request: PdfDocumentAccessRequest) => {
    assertPdfDocumentAccessRequest(request, []);
    const temporarySourcePath = releasePdfDocumentAccess(event.sender.id, request.documentHandle);
    if (temporarySourcePath) await requireBlankPdfTemporaryStore().release(temporarySourcePath);
  });

  ipcMain.handle(ipcChannels.pdfLoadDocument, async (event, filePath: string) => {
    if (filePath === process.env.BP_DEFAULT_SAMPLE_PDF && existsSync(filePath)) {
      await desktopPdfAccessRegistry.authorizeSource(event.sender.id, filePath);
    }
    const openedAccess = await desktopPdfAccessRegistry.openAuthorizedSource(event.sender.id, filePath);
    let keepAccess = false;
    let signatureDocumentHandle: string | undefined;
    try {
      const payload = await loadDocumentPayload(openedAccess.sourcePath);
      const signatureAnalysis = await analyzePdfSignatureDocument(
        openedAccess.sourcePath,
        await readFile(openedAccess.sourcePath),
        createDefaultPdfSignatureDocumentAnalysisDependencies(
          createPdfSignatureCoreOptions(),
          signatureDocumentRegistry,
        ),
      );
      signatureDocumentHandle = signatureAnalysis.document?.handle;
      await desktopPdfAccessRegistry.resolveDocument(event.sender.id, openedAccess.descriptor.handle);
      keepAccess = true;
      if (signatureDocumentHandle) {
        signatureDocumentHandleByPdfHandle.set(openedAccess.descriptor.handle, signatureDocumentHandle);
      }
      signatureProtectionByPdfHandle.set(openedAccess.descriptor.handle, signatureAnalysis.protection);
      signatureOwnerWebContentsIdByPdfHandle.set(openedAccess.descriptor.handle, event.sender.id);
      return {
        ...payload,
        documentAccess: openedAccess.descriptor,
        signatureDocument: signatureAnalysis.document,
        signatureProtection: signatureAnalysis.protection,
        signatureValidation: signatureAnalysis.validation,
      };
    } finally {
      if (!keepAccess) {
        if (signatureDocumentHandle) signatureDocumentRegistry.release(signatureDocumentHandle);
        let temporarySourcePath: string | null = null;
        try {
          temporarySourcePath = desktopPdfAccessRegistry.releaseDocument(
            event.sender.id,
            openedAccess.descriptor.handle,
          );
        } catch {
          // A failed final identity check may already have revoked the handle.
        }
        if (temporarySourcePath) await requireBlankPdfTemporaryStore().release(temporarySourcePath).catch(() => undefined);
      }
    }
  });

  ipcMain.handle(ipcChannels.pdfGetPageGeometry, async (event, request: PageGeometryRequest) => {
    assertPdfDocumentAccessRequest(request, ['pageIndex']);
    if (!Number.isInteger(request.pageIndex) || request.pageIndex < 0) {
      throw new TypeError('PDF page geometry request is invalid.');
    }
    const source = await desktopPdfAccessRegistry.resolveDocument(event.sender.id, request.documentHandle);
    const geometry = await loadPageGeometryIndex(source.sourcePath, request.pageIndex);
    await desktopPdfAccessRegistry.resolveDocument(event.sender.id, request.documentHandle);
    return geometry;
  });

  ipcMain.handle(ipcChannels.pdfSaveDocument, async (event, request: SaveDocumentRequest) => {
    assertSaveDocumentRequest(request);
    const source = await desktopPdfAccessRegistry.resolveDocument(event.sender.id, request.documentHandle);
    const signatureProtection = signatureProtectionByPdfHandle.get(request.documentHandle);
    if (signatureProtection?.sourceReadOnly === true || signatureProtection?.status !== 'unsigned') {
      throw new PdfSignedSourcePolicyError();
    }
    const target = await desktopPdfAccessRegistry.takeSaveTarget(event.sender.id, request.targetHandle);
    const result = await saveDocumentPayload(
      source.sourcePath,
      request.markups,
      'saveAs',
      target.targetPath,
      request.pageScales,
      request.pageRotations,
      target.directoryIdentity,
    );
    await desktopPdfAccessRegistry.resolveDocument(event.sender.id, request.documentHandle);
    await desktopPdfAccessRegistry.authorizeSource(event.sender.id, result.path);
    return result;
  });

  ipcMain.handle(ipcChannels.signingChooseIdentity, async (event) => {
    if (!PDF_CERTIFICATE_SIGNING_ENABLED) {
      return signingIdentitySelectionDisabledResult();
    }
    try {
      const ownerWindow = BrowserWindow.fromWebContents(event.sender);
      if (!ownerWindow || ownerWindow.isDestroyed()) {
        return { outcome: 'failed' as const, errorCode: 'IDENTITY_UNAVAILABLE' as const };
      }
      const ownerLease = requireOwnerLease(event.sender.id, ownerWindow.id);
      const selection = await signingIdentityRegistry.choose(ownerWindow.id, {
        generation: ownerLease.generation,
        signal: ownerLease.signal,
      });
      assertOwnerLeaseActive(ownerLease);
      if (!selection) return { outcome: 'cancelled' as const };
      const certificate = selection.certificates.find((candidate) => candidate.suitableForSigning);
      if (!certificate) {
        signingIdentityRegistry.revoke(selection.handle, ownerWindow.id);
        return { outcome: 'failed' as const, errorCode: 'IDENTITY_UNAVAILABLE' as const };
      }
      return {
        outcome: 'selected' as const,
        identity: {
          identityHandle: selection.handle,
          certificateSha256: certificate.sha256Fingerprint,
          subject: certificate.subjectDisplayName,
          issuer: certificate.issuerDisplayName,
          serialNumber: certificate.serialNumber,
          validFrom: certificate.notBefore,
          validTo: certificate.notAfter,
          keyAlgorithm: certificate.publicKeyAlgorithm,
          privateKeyExported: false as const,
          passwordRemembered: false as const,
        },
      };
    } catch (error) {
      return {
        outcome: 'failed' as const,
        errorCode: error instanceof PdfSigningIdentityRegistryError && error.code === 'INSPECTION_FAILED'
          ? 'ENGINE_UNAVAILABLE' as const
          : 'IDENTITY_UNAVAILABLE' as const,
      };
    }
  });

  ipcMain.handle(ipcChannels.signingApprove, async (event, request: unknown) => {
    assertSigningApprovalRequest(request);
    const ownerWindow = BrowserWindow.fromWebContents(event.sender);
    if (!ownerWindow || ownerWindow.isDestroyed()) {
      return signingCapabilityDisabledResult(request.operation);
    }
    const ownerLease = requireOwnerLease(event.sender.id, ownerWindow.id);
    assertOwnerLeaseActive(ownerLease);
    const result = await executePdfSigningApproval(event.sender.id, ownerWindow.id, request, {
      signingEnabled: PDF_CERTIFICATE_SIGNING_ENABLED,
      signal: ownerLease.signal,
      currentTrustConfigurationSha256: offlineSignatureTrustConfigurationSha256([]),
      resolveDocument: async (ownerWebContentsId, documentHandle) => {
        assertOwnerLeaseActive(ownerLease);
        const source = await desktopPdfAccessRegistry.resolveDocument(ownerWebContentsId, documentHandle);
        assertOwnerLeaseActive(ownerLease);
        return source;
      },
      resolveProtection: (documentHandle) => {
        assertOwnerLeaseActive(ownerLease);
        return signatureProtectionByPdfHandle.get(documentHandle);
      },
      resolveSignatureDocumentHandle: (documentHandle) => {
        assertOwnerLeaseActive(ownerLease);
        return signatureDocumentHandleByPdfHandle.get(documentHandle);
      },
      resolveSignatureDocument: async (handle, currentTrustConfigurationSha256) => {
        assertOwnerLeaseActive(ownerLease);
        const document = await signatureDocumentRegistry.resolve(handle, currentTrustConfigurationSha256);
        assertOwnerLeaseActive(ownerLease);
        return document;
      },
      describeIdentity: (identityHandle, ownerWindowId) => {
        assertOwnerLeaseActive(ownerLease);
        return signingIdentityRegistry.describe(identityHandle, ownerWindowId, { generation: ownerLease.generation });
      },
      withPkcs12Frame: (identityHandle, ownerWindowId, consume) => (
        signingIdentityRegistry.withPkcs12Frame(identityHandle, ownerWindowId, consume, {
          generation: ownerLease.generation,
          signal: ownerLease.signal,
        })
      ),
      takeSaveTarget: async (ownerWebContentsId, targetHandle) => {
        assertOwnerLeaseActive(ownerLease);
        const target = await desktopPdfAccessRegistry.takeSaveTarget(ownerWebContentsId, targetHandle);
        assertOwnerLeaseActive(ownerLease);
        return target;
      },
      createSigningClient: () => {
        assertOwnerLeaseActive(ownerLease);
        return new PdfSignatureCoreSigningClient({
          ...createPdfSignatureCoreOptions(),
          allowExperimentalProofOperations: PDF_CERTIFICATE_SIGNING_ENABLED,
        });
      },
      createMutation: (options) => createPdfSignedMutation({
        ...options,
        quarantine: signingQuarantine ?? undefined,
        signal: ownerLease.signal,
      }),
      authorizePublishedDocument: async (ownerWebContentsId, targetPath) => {
        let authorized = false;
        let openedHandle: string | undefined;
        let signatureDocumentHandle: string | undefined;
        let signatureDocumentCommitted = false;
        try {
          assertOwnerLeaseActive(ownerLease);
          await desktopPdfAccessRegistry.authorizeSource(ownerWebContentsId, targetPath);
          authorized = true;
          assertOwnerLeaseActive(ownerLease);
          const opened = await desktopPdfAccessRegistry.openAuthorizedSource(ownerWebContentsId, targetPath);
          openedHandle = opened.descriptor.handle;
          assertOwnerLeaseActive(ownerLease);
          await desktopPdfAccessRegistry.resolveDocument(ownerWebContentsId, openedHandle);
          assertOwnerLeaseActive(ownerLease);
          const signatureAnalysis = await analyzePdfSignatureDocument(
            opened.sourcePath,
            await readFile(opened.sourcePath),
            createDefaultPdfSignatureDocumentAnalysisDependencies(
              createPdfSignatureCoreOptions(),
              signatureDocumentRegistry,
            ),
          );
          signatureDocumentHandle = signatureAnalysis.document?.handle;
          assertOwnerLeaseActive(ownerLease);
          await desktopPdfAccessRegistry.resolveDocument(ownerWebContentsId, openedHandle);
          assertOwnerLeaseActive(ownerLease);
          if (signatureAnalysis.document) {
            signatureDocumentHandleByPdfHandle.set(openedHandle, signatureAnalysis.document.handle);
          }
          signatureProtectionByPdfHandle.set(openedHandle, signatureAnalysis.protection);
          signatureOwnerWebContentsIdByPdfHandle.set(openedHandle, ownerWebContentsId);
          signatureDocumentCommitted = true;
          return openedHandle;
        } catch (error) {
          if (!signatureDocumentCommitted && signatureDocumentHandle) {
            signatureDocumentRegistry.release(signatureDocumentHandle);
          }
          if (openedHandle) {
            try {
              releasePdfDocumentAccess(ownerWebContentsId, openedHandle);
            } catch {
              // Owner teardown or stale-document revocation already removed it.
            }
          } else if (authorized) {
            await desktopPdfAccessRegistry.revokeSourceGrant(ownerWebContentsId, targetPath).catch(() => undefined);
          }
          throw error;
        }
      },
      releaseDocument: (ownerWebContentsId, documentHandle) => {
        if (!isOwnerLeaseActive(ownerLease)) return;
        releasePdfDocumentAccess(ownerWebContentsId, documentHandle);
      },
    });
    assertOwnerLeaseActive(ownerLease);
    return result;
  });

  ipcMain.handle(ipcChannels.themeGetSnapshot, async () => {
    return getThemeSnapshot();
  });

  ipcMain.handle(ipcChannels.testResolveFixture, async (event, name: string) => {
    if (!isTestModeEnabled()) {
      return null;
    }
    if (typeof name !== 'string' || name !== basename(name) || !/^[A-Za-z0-9._-]+\.pdf$/i.test(name)) {
      return null;
    }

    const fixturePath = resolveFixturePath(name);
    return fixturePath
      ? await desktopPdfAccessRegistry.authorizeSource(event.sender.id, fixturePath)
      : null;
  });

  ipcMain.handle(ipcChannels.testAuthorizePdfSource, async (event, filePath: string) => {
    assertTestPdfPath(filePath, 'source');
    return await desktopPdfAccessRegistry.authorizeSource(event.sender.id, filePath);
  });

  ipcMain.handle(ipcChannels.testAuthorizePdfSaveTarget, async (event, filePath: string) => {
    assertTestPdfPath(filePath, 'target');
    return await desktopPdfAccessRegistry.authorizeSaveTarget(event.sender.id, filePath);
  });

  ipcMain.handle(ipcChannels.testGetWindowState, async () => {
    if (!isTestModeEnabled()) {
      return null;
    }

    return getFocusedWindowState();
  });

  ipcMain.handle(ipcChannels.testSetWindowBounds, async (_event, bounds) => {
    if (!isTestModeEnabled()) {
      return null;
    }

    return setFocusedWindowBounds(bounds ?? {});
  });

  ipcMain.handle(ipcChannels.testGetProcessMetrics, async () => {
    if (!isTestModeEnabled()) {
      return null;
    }

    return createDesktopProcessMetricsSnapshot(app.getAppMetrics());
  });
}

export async function bootstrapDesktop(): Promise<void> {
  await app.whenReady();
  const metadata = getApplicationMetadata();
  blankPdfTemporaryStore = new BlankPdfTemporaryStore(app.getPath('temp'), `butter-paper-${metadata.channel}-blank-`);
  // Windows signing remains fail-closed until native ACL hooks are available;
  // the quarantine instance is therefore only constructed where its storage
  // boundary can currently be verified by this process.
  if (process.platform !== 'win32') {
    signingQuarantine = new PdfSigningQuarantine(join(app.getPath('userData'), 'pdf-signing-quarantine'));
  }
  await blankPdfTemporaryStore.cleanupStaleSessions().catch((error) => {
    console.warn('Unable to remove stale blank PDF temporary files.', error);
  });
  updaterService = createUpdaterService();
  registerThemeListener();
  registerIpcHandlers();
  createMainWindow();
  unsubscribeUpdaterStatus = updaterService.subscribe((status) => {
    for (const window of BrowserWindow.getAllWindows()) {
      if (!window.isDestroyed()) {
        window.webContents.send(ipcChannels.updatesStatusChanged, status);
      }
    }
  });
  await updaterService.start();

  app.on('before-quit', (event) => {
    const blockedWindows = BrowserWindow.getAllWindows().filter((window) => (
      !window.isDestroyed()
      && closeBlockedWebContents.has(window.webContents.id)
      && !closeConfirmedWebContents.has(window.webContents.id)
    ));
    if (blockedWindows.length > 0) {
      applicationQuitRequested = true;
      event.preventDefault();
      for (const window of blockedWindows) {
        window.webContents.send(ipcChannels.applicationCloseRequested);
      }
      return;
    }
    applicationQuitRequested = false;
    unsubscribeUpdaterStatus?.();
    unsubscribeUpdaterStatus = null;
    void updaterService?.stop();
  });

  app.on('will-quit', () => {
    blankPdfTemporaryStore?.cleanupSync();
    blankPdfTemporaryStore = null;
  });

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow();
    }
  });
}

function requireBlankPdfTemporaryStore(): BlankPdfTemporaryStore {
  blankPdfTemporaryStore ??= new BlankPdfTemporaryStore(app.getPath('temp'));
  return blankPdfTemporaryStore;
}

function requireOwnerLease(ownerWebContentsId: number, ownerWindowId: number): PdfSigningIdentityOwnerLease {
  const lease = signingIdentityOwnerLeaseByWebContentsId.get(ownerWebContentsId);
  if (!lease || lease.ownerWindowId !== ownerWindowId || !isOwnerLeaseActive(lease)) {
    throw new PdfSigningIdentityRegistryError('OWNER_UNAVAILABLE', 'The renderer owner is unavailable.');
  }
  return lease;
}

function assertOwnerLeaseActive(lease: PdfSigningIdentityOwnerLease): void {
  if (!isOwnerLeaseActive(lease)) {
    throw new PdfSigningIdentityRegistryError('OWNER_UNAVAILABLE', 'The renderer owner is unavailable.');
  }
}

function isOwnerLeaseActive(lease: PdfSigningIdentityOwnerLease): boolean {
  const ownerWindow = BrowserWindow.fromId(lease.ownerWindowId);
  return !lease.signal.aborted
    && ownerWindow != null
    && !ownerWindow.isDestroyed()
    && !ownerWindow.webContents.isDestroyed()
    && desktopPdfAccessRegistry.isOwnerActive(ownerWindow.webContents.id)
    && signingIdentityRegistry.isOwnerGenerationActive(lease.ownerWindowId, lease.generation);
}

function forgetSignatureDocument(documentHandle: string): void {
  const signatureDocumentHandle = signatureDocumentHandleByPdfHandle.get(documentHandle);
  signatureDocumentHandleByPdfHandle.delete(documentHandle);
  signatureProtectionByPdfHandle.delete(documentHandle);
  signatureOwnerWebContentsIdByPdfHandle.delete(documentHandle);
  if (signatureDocumentHandle) signatureDocumentRegistry.release(signatureDocumentHandle);
}

function releasePdfDocumentAccess(ownerWebContentsId: number, documentHandle: string): string | null {
  const temporarySourcePath = desktopPdfAccessRegistry.releaseDocument(ownerWebContentsId, documentHandle);
  forgetSignatureDocument(documentHandle);
  return temporarySourcePath;
}

function createPdfSignatureCoreOptions() {
  const developmentRoot = app.isPackaged ? undefined : resolvePdfSignatureCoreDevelopmentRoot();
  return {
    isPackaged: app.isPackaged,
    resourcesPath: process.resourcesPath,
    appPath: app.getAppPath(),
    ...(developmentRoot ? { developmentRoot } : {}),
  } as const;
}

function resolvePdfSignatureCoreDevelopmentRoot(): string | undefined {
  return ancestorFileCandidates(
    app.getAppPath(),
    'native/pdf-signature-core/build/package',
    6,
  ).find((candidate) => existsSync(join(candidate, `${process.platform}-${process.arch}`)));
}

function assertPdfDocumentAccessRequest(
  request: unknown,
  optionalKeys: readonly string[],
): asserts request is PdfDocumentAccessRequest & Record<string, unknown> {
  if (!request || typeof request !== 'object' || Array.isArray(request)) {
    throw new TypeError('PDF document access request is invalid.');
  }
  const value = request as Record<string, unknown>;
  const allowedKeys = new Set(['documentHandle', ...optionalKeys]);
  if (typeof value.documentHandle !== 'string'
    || value.documentHandle.length === 0
    || Object.keys(value).some((key) => !allowedKeys.has(key))) {
    throw new TypeError('PDF document access request is invalid.');
  }
}

function assertSaveDocumentRequest(request: unknown): asserts request is SaveDocumentRequest {
  if (!request || typeof request !== 'object' || Array.isArray(request)) {
    throw new TypeError('Save PDF request is invalid.');
  }
  const value = request as Record<string, unknown>;
  const allowedKeys = new Set([
    'documentHandle',
    'targetHandle',
    'markups',
    'pageScales',
    'pageRotations',
    'mode',
  ]);
  if (Object.keys(value).some((key) => !allowedKeys.has(key))
    || typeof value.documentHandle !== 'string'
    || value.documentHandle.length === 0
    || typeof value.targetHandle !== 'string'
    || value.targetHandle.length === 0
    || value.mode !== 'saveAs'
    || !Array.isArray(value.markups)
    || (value.pageRotations !== undefined && !Array.isArray(value.pageRotations))) {
    throw new TypeError('Save PDF request is invalid.');
  }
}

function assertTestPdfPath(filePath: string, kind: 'source' | 'target'): void {
  if (!isTestModeEnabled() || typeof filePath !== 'string' || !/\.pdf$/i.test(filePath)) {
    throw new TypeError('Test PDF path authorization is unavailable.');
  }
  const candidate = resolve(filePath);
  const canonicalCandidate = kind === 'source'
    ? realpathSync(candidate)
    : join(realpathSync(dirname(candidate)), basename(candidate));
  const temporaryRoot = realpathSync(resolve(app.getPath('temp')));
  const fixtureRoot = realpathSync(dirname(resolveFixturePath('__fixture_boundary__')));
  if (isPathInsideRoot(temporaryRoot, canonicalCandidate)
    || (kind === 'source' && isPathInsideRoot(fixtureRoot, canonicalCandidate))) {
    return;
  }
  throw new TypeError('Test PDF path is outside the approved test roots.');
}

function isPathInsideRoot(rootPath: string, candidatePath: string): boolean {
  const child = relative(rootPath, candidatePath);
  return child.length > 0 && child !== '..' && !child.startsWith(`..${sep}`);
}

function createUpdaterService(): DesktopUpdaterService {
  return new DesktopUpdaterService({
    updater: loadElectronAutoUpdater(),
    isPackaged: app.isPackaged,
    userDataPath: app.getPath('userData'),
    currentVersion: app.getVersion(),
    resourcesPath: process.resourcesPath,
    platform: process.platform,
    buildMetadata: readDesktopPackageMetadata(),
  });
}

function requireUpdaterService(): DesktopUpdaterService {
  if (updaterService == null) {
    throw new Error('Butter Paper updater has not been initialised.');
  }
  return updaterService;
}

function readDesktopPackageMetadata(): unknown {
  let lastError: unknown = null;
  for (const candidate of ancestorFileCandidates(app.getAppPath(), 'package.json')) {
    try {
      return JSON.parse(readFileSync(candidate, 'utf8')) as unknown;
    } catch (error) {
      lastError = error;
    }
  }
  console.warn('Unable to read packaged update channel metadata.', lastError);
  return null;
}

function getApplicationMetadata(): ApplicationMetadata {
  const packageMetadata = readDesktopPackageMetadata();
  const version = packageMetadata != null
    && typeof packageMetadata === 'object'
    && typeof (packageMetadata as Record<string, unknown>).version === 'string'
    ? (packageMetadata as Record<string, string>).version
    : app.getVersion();
  return resolveApplicationMetadata(packageMetadata, {
    packaged: app.isPackaged,
    version,
    devProvenance: app.isPackaged ? undefined : readDevProvenance(),
  });
}

function readDevProvenance(): unknown {
  for (const candidate of ancestorFileCandidates(
    app.getAppPath(),
    'test-results/desktop-dev-provenance.json',
  )) {
    try {
      return JSON.parse(readFileSync(candidate, 'utf8')) as unknown;
    } catch {
      // Try the next supported development app root.
    }
  }
  return null;
}
