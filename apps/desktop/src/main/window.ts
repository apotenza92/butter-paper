import electron from 'electron';
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import type { BrowserWindow as BrowserWindowInstance, Event as ElectronEvent, IpcMainInvokeEvent, MenuItemConstructorOptions, RenderProcessGoneDetails } from 'electron';
import { ipcChannels } from '../shared/ipc';
import type {
  ApplicationMetadata,
  ApplicationMenuCommand,
  ApplicationMenuState,
  BlankPdfCreateRequest,
  PageGeometryRequest,
  PdfDocumentAccessRequest,
  SaveDocumentRequest,
  ThemeMode,
  ThemeSnapshot,
  UpdateFrequency,
  UpdateStatus,
} from '../shared/protocol';
import {
  APPLICATION_MENU_BAR_VISIBILITY_LABEL,
  APPLICATION_MENU_COMMANDS,
  APPLICATION_MENU_UPDATE_FREQUENCIES,
  updateCheckMenuLabel,
} from '../shared/applicationMenu';
import { resolveApplicationMetadata } from './applicationMetadata';
import { ancestorFileCandidates } from './applicationPaths';
import { BlankPdfTemporaryStore } from './blankPdfTemporaryStore';
import { configureCameraPermissions } from './cameraPermissions';
import { setAsDefaultPdfApp } from './defaultPdfApp';
import { takePendingPdfPaths } from './pendingPdfPaths';
import { desktopPdfAccessRegistry } from './pdfAccessRegistry';
import { PhoneSignatureTransferService } from './phoneSignatureTransfer';
import { RECENT_SIGNATURES_FILE_NAME, RecentSignatureStore } from './recentSignatureStore';
import { sanitizePhoneSignatureImage, sanitizeSignatureAppearanceAsset } from './signatureImageSanitizer';
import { loadDocumentPayload, loadPageGeometryIndex, saveDocumentPayload } from './pdfSession';
import { createDesktopProcessMetricsSnapshot } from './processMetrics';
import { synchronizeMacosApplicationRegistration } from './applicationRegistration';
import { getFocusedWindowState, isTestModeEnabled, resolveFixturePath, setFocusedWindowBounds } from './testMode';
import { DesktopUpdaterService, loadElectronAutoUpdater } from './updater';
import { resolveReleasePageUrl } from './releasePage';
import { resolveApplicationShortcutAction } from './windowShortcuts';
import { MAIN_WINDOW_GEOMETRY } from './windowGeometry';
import { registerWindowFullScreenNotifications } from './windowFullScreen';
import { getWindowTitleBarOptions } from './windowTitleBar';
import {
  loadWindowBounds,
  resolveRestoredWindowBounds,
  WINDOW_STATE_FILE_NAME,
  writeWindowBoundsAtomic,
} from './windowState';

const { app, BrowserWindow, Menu, ipcMain, dialog, nativeTheme, safeStorage, screen, shell } = electron;
const moduleDir = dirname(fileURLToPath(import.meta.url));
const preloadPath = join(moduleDir, 'preload.cjs');
const defaultSamplePdfPath = join(moduleDir, '../../../..', 'tests/fixtures/generated/zoom-target.pdf');
let mainWindow: BrowserWindowInstance | null = null;
let themeListenerRegistered = false;
let updaterService: DesktopUpdaterService | null = null;
let unsubscribeUpdaterStatus: (() => void) | null = null;
let blankPdfTemporaryStore: BlankPdfTemporaryStore | null = null;
let applicationQuitRequested = false;
let applicationMenuBarVisible = true;
let applicationMenuState: ApplicationMenuState = {
  canSave: false,
  canUndo: false,
  canRedo: false,
  canCut: false,
  canCopy: false,
  canPaste: false,
  canSelectAll: false,
  updateStatus: null,
  menuBarVisible: applicationMenuBarVisible,
};
const closeBlockedWebContents = new Set<number>();
const closeConfirmedWebContents = new Set<number>();
const trustedRendererUrls = new Map<number, string>();
const phoneSignatureRelayTestMode = !app.isPackaged && process.env.BP_SIGNATURE_RELAY_TEST_MODE === '1';
const phoneSignatureTransferService = new PhoneSignatureTransferService({
  allowInsecureLoopback: phoneSignatureRelayTestMode,
  relayOrigin: phoneSignatureRelayTestMode ? process.env.BP_SIGNATURE_RELAY_URL : undefined,
  sanitizeImage: sanitizePhoneSignatureImage,
});
let recentSignatureStore: RecentSignatureStore | null = null;

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

function sendApplicationMenuCommand(command: ApplicationMenuCommand): void {
  const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
  if (!window || window.isDestroyed() || window.webContents.isDestroyed()) {
    return;
  }
  window.webContents.send(ipcChannels.applicationMenuCommand, command);
}

function assertApplicationWindowSender(event: IpcMainInvokeEvent): void {
  const window = BrowserWindow.fromWebContents(event.sender);
  const senderFrame = event.senderFrame;
  const trustedRendererUrl = trustedRendererUrls.get(event.sender.id);
  if (!window || window.isDestroyed() || event.sender.isDestroyed()
    || !senderFrame || senderFrame !== event.sender.mainFrame || !trustedRendererUrl
    || !matchesTrustedRendererUrl(senderFrame.url, trustedRendererUrl)) {
    throw new Error('Application IPC sender is not an active Butter Paper window.');
  }
}

function requireRecentSignatureStore(): RecentSignatureStore {
  if (!recentSignatureStore) throw new Error('Recent signature storage is not initialized.');
  return recentSignatureStore;
}

function matchesTrustedRendererUrl(actualValue: string, trustedValue: string): boolean {
  try {
    const actual = new URL(actualValue);
    const trusted = new URL(trustedValue);
    if (trusted.protocol === 'file:') {
      actual.hash = '';
      trusted.hash = '';
      return actual.href === trusted.href;
    }
    return actual.origin === trusted.origin;
  } catch {
    return false;
  }
}

function assertPhoneSignatureSessionId(value: unknown): asserts value is string {
  if (typeof value !== 'string'
    || !/^[A-Za-z0-9_-]{22}$/.test(value)) {
    throw new TypeError('Phone signature session ID is invalid.');
  }
}

function assertPhoneSignatureMode(value: unknown): asserts value is 'draw' | 'image' {
  if (value !== 'draw' && value !== 'image') {
    throw new TypeError('Phone signature mode is invalid.');
  }
}

const APPLICATION_MENU_ITEM_IDS = {
  save: 'butter-paper-save',
  saveAs: 'butter-paper-save-as',
  undo: 'butter-paper-undo',
  redo: 'butter-paper-redo',
  cut: 'butter-paper-cut',
  copy: 'butter-paper-copy',
  paste: 'butter-paper-paste',
  selectAll: 'butter-paper-select-all',
  checkForUpdates: 'butter-paper-check-for-updates',
  updateFrequency: 'butter-paper-update-frequency',
  menuBarVisibility: 'butter-paper-menu-bar-visibility',
} as const;

function findApplicationMenuItem(id: string) {
  for (const menuItem of Menu.getApplicationMenu()?.items ?? []) {
    if (menuItem.id === id) return menuItem;
    for (const submenuItem of menuItem.submenu?.items ?? []) {
      if (submenuItem.id === id) return submenuItem;
      const nestedMatch = submenuItem.submenu?.items.find((item) => item.id === id);
      if (nestedMatch) return nestedMatch;
    }
  }
  return undefined;
}

function isUpdateCheckInProgress(status: UpdateStatus | null): boolean {
  return status?.phase === 'checking' || status?.phase === 'available' || status?.phase === 'downloading';
}

function synchronizeApplicationMenu(state: ApplicationMenuState): void {
  applicationMenuState = state;
  const saveItem = findApplicationMenuItem(APPLICATION_MENU_ITEM_IDS.save);
  const saveAsItem = findApplicationMenuItem(APPLICATION_MENU_ITEM_IDS.saveAs);
  if (saveItem) saveItem.enabled = state.canSave;
  if (saveAsItem) saveAsItem.enabled = state.canSave;
  for (const [key, enabled] of [
    ['undo', state.canUndo],
    ['redo', state.canRedo],
    ['cut', state.canCut],
    ['copy', state.canCopy],
    ['paste', state.canPaste],
    ['selectAll', state.canSelectAll],
  ] as const) {
    const item = findApplicationMenuItem(APPLICATION_MENU_ITEM_IDS[key]);
    if (item) item.enabled = enabled;
  }

  const status = state.updateStatus;
  const checkForUpdatesItem = findApplicationMenuItem(APPLICATION_MENU_ITEM_IDS.checkForUpdates);
  if (checkForUpdatesItem) {
    checkForUpdatesItem.label = updateCheckMenuLabel(status?.phase ?? 'idle', status?.downloadPercent ?? null);
    checkForUpdatesItem.enabled = Boolean(status?.enabled) && !isUpdateCheckInProgress(status);
  }

  const updateFrequencyItem = findApplicationMenuItem(APPLICATION_MENU_ITEM_IDS.updateFrequency);
  if (updateFrequencyItem) updateFrequencyItem.enabled = status != null;
  for (const frequency of APPLICATION_MENU_UPDATE_FREQUENCIES) {
    const frequencyItem = findApplicationMenuItem(`butter-paper-update-frequency-${frequency.value}`);
    if (frequencyItem) frequencyItem.checked = status?.frequency === frequency.value;
  }

  applicationMenuBarVisible = state.menuBarVisible;
  const menuBarVisibilityItem = findApplicationMenuItem(APPLICATION_MENU_ITEM_IDS.menuBarVisibility);
  if (menuBarVisibilityItem) menuBarVisibilityItem.checked = state.menuBarVisible;
}

function assertApplicationMenuState(value: unknown): asserts value is ApplicationMenuState {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new TypeError('Application menu state must be an object.');
  }
  const state = value as Record<string, unknown>;
  const booleanKeys = ['canSave', 'canUndo', 'canRedo', 'canCut', 'canCopy', 'canPaste', 'canSelectAll', 'menuBarVisible'];
  if (Object.keys(state).some((key) => ![...booleanKeys, 'updateStatus'].includes(key))) {
    throw new TypeError('Application menu state contains unknown fields.');
  }
  if (booleanKeys.some((key) => typeof state[key] !== 'boolean')) {
    throw new TypeError('Application menu state has invalid boolean fields.');
  }
  if (state.updateStatus === null) return;
  if (!state.updateStatus || typeof state.updateStatus !== 'object' || Array.isArray(state.updateStatus)) {
    throw new TypeError('Application menu update status must be an object or null.');
  }
  const status = state.updateStatus as Record<string, unknown>;
  const statusKeys = [
    'phase', 'channel', 'frequency', 'enabled', 'automaticChecksEnabled', 'currentVersion',
    'availableVersion', 'releaseNotes', 'downloadPercent', 'lastSuccessfulCheckAt',
    'disabledReason', 'errorMessage',
  ];
  if (Object.keys(status).some((key) => !statusKeys.includes(key))) {
    throw new TypeError('Application menu update status contains unknown fields.');
  }
  const phases = ['disabled', 'idle', 'checking', 'available', 'downloading', 'downloaded', 'error'];
  const frequencies = APPLICATION_MENU_UPDATE_FREQUENCIES.map(({ value: frequency }) => frequency);
  const channels = ['stable', 'beta'];
  const disabledReasons = ['development', 'test-mode', 'configuration', 'platform-policy', null];
  if (!phases.includes(String(status.phase))
    || (status.channel !== null && !channels.includes(String(status.channel)))
    || !frequencies.includes(String(status.frequency) as UpdateFrequency)
    || typeof status.enabled !== 'boolean'
    || typeof status.automaticChecksEnabled !== 'boolean'
    || typeof status.currentVersion !== 'string'
    || (status.availableVersion !== null && typeof status.availableVersion !== 'string')
    || (status.releaseNotes !== null && typeof status.releaseNotes !== 'string')
    || (status.downloadPercent !== null
      && (typeof status.downloadPercent !== 'number' || !Number.isFinite(status.downloadPercent)
        || status.downloadPercent < 0 || status.downloadPercent > 100))
    || (status.lastSuccessfulCheckAt !== null && typeof status.lastSuccessfulCheckAt !== 'string')
    || !disabledReasons.includes(status.disabledReason as string | null)
    || (status.errorMessage !== null && typeof status.errorMessage !== 'string')) {
    throw new TypeError('Application menu update status is invalid.');
  }
}

function installApplicationMenu(productName: string): void {
  if (process.platform !== 'darwin') {
    return;
  }

  const applicationMenu: MenuItemConstructorOptions = {
    label: productName,
    submenu: [
      { role: 'about' },
      { type: 'separator' },
      { label: APPLICATION_MENU_COMMANDS.setDefaultPdfApp.label, click: () => sendApplicationMenuCommand(APPLICATION_MENU_COMMANDS.setDefaultPdfApp.command) },
      { id: APPLICATION_MENU_ITEM_IDS.checkForUpdates, label: APPLICATION_MENU_COMMANDS.checkForUpdates.label, enabled: false, click: () => sendApplicationMenuCommand(APPLICATION_MENU_COMMANDS.checkForUpdates.command) },
      {
        id: APPLICATION_MENU_ITEM_IDS.updateFrequency,
        label: 'Check Automatically',
        enabled: false,
        submenu: APPLICATION_MENU_UPDATE_FREQUENCIES.map((frequency) => ({
          id: `butter-paper-update-frequency-${frequency.value}`,
          label: frequency.label,
          type: 'checkbox' as const,
          click: () => {
            void requireUpdaterService().setFrequency(frequency.value).then(() => {
              const status = requireUpdaterService().getStatus();
              synchronizeApplicationMenu({
                ...applicationMenuState,
                updateStatus: status,
              });
            }).catch((error) => {
              console.error('Unable to save the update frequency.', error);
            });
          },
        })),
      },
      { label: APPLICATION_MENU_COMMANDS.openReleasePage.label, click: () => sendApplicationMenuCommand(APPLICATION_MENU_COMMANDS.openReleasePage.command) },
      { type: 'separator' },
      { role: 'services' },
      { type: 'separator' },
      { role: 'hide' },
      { role: 'hideOthers' },
      { role: 'unhide' },
      { type: 'separator' },
      { role: 'quit' },
    ],
  };
  const fileMenu: MenuItemConstructorOptions = {
    label: 'File',
    submenu: [
      { label: APPLICATION_MENU_COMMANDS.newPdf.label, accelerator: APPLICATION_MENU_COMMANDS.newPdf.accelerator, click: () => sendApplicationMenuCommand(APPLICATION_MENU_COMMANDS.newPdf.command) },
      { label: APPLICATION_MENU_COMMANDS.openPdf.label, accelerator: APPLICATION_MENU_COMMANDS.openPdf.accelerator, click: () => sendApplicationMenuCommand(APPLICATION_MENU_COMMANDS.openPdf.command) },
      { type: 'separator' },
      { id: APPLICATION_MENU_ITEM_IDS.save, label: APPLICATION_MENU_COMMANDS.save.label, accelerator: APPLICATION_MENU_COMMANDS.save.accelerator, enabled: false, click: () => sendApplicationMenuCommand(APPLICATION_MENU_COMMANDS.save.command) },
      { id: APPLICATION_MENU_ITEM_IDS.saveAs, label: APPLICATION_MENU_COMMANDS.saveAs.label, accelerator: APPLICATION_MENU_COMMANDS.saveAs.accelerator, enabled: false, click: () => sendApplicationMenuCommand(APPLICATION_MENU_COMMANDS.saveAs.command) },
    ],
  };
  const editMenu: MenuItemConstructorOptions = {
    label: 'Edit',
    submenu: [
      { id: APPLICATION_MENU_ITEM_IDS.undo, label: APPLICATION_MENU_COMMANDS.undo.label, accelerator: APPLICATION_MENU_COMMANDS.undo.accelerator, enabled: false, click: () => sendApplicationMenuCommand('undo') },
      { id: APPLICATION_MENU_ITEM_IDS.redo, label: APPLICATION_MENU_COMMANDS.redo.label, accelerator: APPLICATION_MENU_COMMANDS.redo.accelerator, enabled: false, click: () => sendApplicationMenuCommand('redo') },
      { type: 'separator' },
      { id: APPLICATION_MENU_ITEM_IDS.cut, label: APPLICATION_MENU_COMMANDS.cut.label, accelerator: APPLICATION_MENU_COMMANDS.cut.accelerator, enabled: false, click: () => sendApplicationMenuCommand('cut') },
      { id: APPLICATION_MENU_ITEM_IDS.copy, label: APPLICATION_MENU_COMMANDS.copy.label, accelerator: APPLICATION_MENU_COMMANDS.copy.accelerator, enabled: false, click: () => sendApplicationMenuCommand('copy') },
      { id: APPLICATION_MENU_ITEM_IDS.paste, label: APPLICATION_MENU_COMMANDS.paste.label, accelerator: APPLICATION_MENU_COMMANDS.paste.accelerator, enabled: false, click: () => sendApplicationMenuCommand('paste') },
      { id: APPLICATION_MENU_ITEM_IDS.selectAll, label: APPLICATION_MENU_COMMANDS.selectAll.label, accelerator: APPLICATION_MENU_COMMANDS.selectAll.accelerator, enabled: false, click: () => sendApplicationMenuCommand('select-all') },
    ],
  };
  const viewMenu: MenuItemConstructorOptions = {
    label: 'View',
    submenu: [
      {
        id: APPLICATION_MENU_ITEM_IDS.menuBarVisibility,
        label: APPLICATION_MENU_BAR_VISIBILITY_LABEL,
        type: 'checkbox',
        checked: applicationMenuBarVisible,
        click: (menuItem) => {
          applicationMenuBarVisible = menuItem.checked;
          const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
          if (!window || window.isDestroyed() || window.webContents.isDestroyed()) {
            return;
          }
          window.webContents.send(ipcChannels.applicationMenuBarVisibilityChanged, applicationMenuBarVisible);
        },
      },
      { type: 'separator' },
      { role: 'reload' },
      { role: 'forceReload' },
      { role: 'togglefullscreen' },
    ],
  };
  const windowMenu: MenuItemConstructorOptions = {
    role: 'windowMenu',
    submenu: [
      { label: 'Close Window', accelerator: 'CommandOrControl+Shift+W', click: () => BrowserWindow.getFocusedWindow()?.close() },
      { type: 'separator' },
      { role: 'minimize' },
      { role: 'zoom' },
      { type: 'separator' },
      { role: 'front' },
    ],
  };

  Menu.setApplicationMenu(Menu.buildFromTemplate([applicationMenu, fileMenu, editMenu, viewMenu, windowMenu]));
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
      ...getWindowTitleBarOptions(),
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
    const trustedRendererUrl = rendererDevServerUrl ?? pathToFileURL(rendererHtmlPath).href;
    trustedRendererUrls.set(window.webContents.id, trustedRendererUrl);
    configureCameraPermissions(window, trustedRendererUrl);
    desktopPdfAccessRegistry.registerOwner(window.webContents.id);
    closeBlockedWebContents.add(window.webContents.id);
    registerWindowFullScreenNotifications(window, ipcChannels.applicationWindowFullScreenChanged);
  } catch (error) {
    console.error('Failed to construct main window', error);
    throw error;
  }

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
    phoneSignatureTransferService.stopOwner(window.webContents.id);
    console.error('Renderer process gone', details);
  });

  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  window.webContents.on('will-navigate', (event) => {
    event.preventDefault();
  });
  window.webContents.on('before-input-event', (event, input) => {
    const action = resolveApplicationShortcutAction(input, process.platform);
    if (!action) {
      return;
    }

    // Native macOS menu accelerators own window close and application quit.
    // Returning here lets Electron dispatch the visible native menu command;
    // other platforms use this deterministic before-input path.
    if (process.platform === 'darwin' && (action === 'close-window' || action === 'quit-app')) {
      return;
    }

    event.preventDefault();
    if (action === 'close-tab') {
      window.webContents.send(ipcChannels.applicationCloseTabRequested);
    } else if (action === 'close-window') {
      window.close();
    } else if (action === 'quit-app') {
      app.quit();
    } else if (action === 'zoom-reset') {
      window.webContents.setZoomLevel(0);
    } else {
      const zoomDirection = action === 'zoom-in' ? 1 : -1;
      window.webContents.setZoomLevel(window.webContents.getZoomLevel() + zoomDirection);
    }
  });

  if (rendererDevServerUrl) {
    void window.loadURL(rendererDevServerUrl);
  } else {
    void window.loadFile(rendererHtmlPath);
  }

  const webContentsId = window.webContents.id;
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
    if (!closeConfirmedWebContents.has(webContentsId)) {
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
    phoneSignatureTransferService.stopOwner(webContentsId);
    trustedRendererUrls.delete(webContentsId);
    if (windowStateSaveTimer != null) {
      clearTimeout(windowStateSaveTimer);
      windowStateSaveTimer = null;
    }
    if (mainWindow === window) {
      mainWindow = null;
    }
  });

  mainWindow = window;
  return window;
}

export function registerIpcHandlers(): void {
  ipcMain.handle(ipcChannels.applicationGetMetadata, async () => {
    return getApplicationMetadata();
  });

  ipcMain.handle(ipcChannels.applicationGetWindowFullScreen, async (event) => {
    assertApplicationWindowSender(event);
    return BrowserWindow.fromWebContents(event.sender)?.isFullScreen() ?? false;
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

  ipcMain.handle(ipcChannels.applicationAuthorizeDroppedPdf, async (event, filePath: unknown) => {
    assertApplicationWindowSender(event);
    if (typeof filePath !== 'string' || !/\.pdf$/i.test(filePath)) {
      throw new TypeError('Dropped document must be a PDF file.');
    }
    return await desktopPdfAccessRegistry.authorizeSource(event.sender.id, filePath);
  });

  ipcMain.handle(ipcChannels.applicationSetCloseBlocked, async (event, blocked: boolean) => {
    assertApplicationWindowSender(event);
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

  ipcMain.handle(ipcChannels.applicationRequestQuit, async (event) => {
    assertApplicationWindowSender(event);
    app.quit();
  });

  ipcMain.handle(ipcChannels.applicationMenuBarVisibilityChanged, async (event, visible: boolean) => {
    assertApplicationWindowSender(event);
    if (typeof visible !== 'boolean') {
      throw new TypeError('Application menu-bar visibility must be a boolean.');
    }
    synchronizeApplicationMenu({ ...applicationMenuState, menuBarVisible: visible });
  });

  ipcMain.handle(ipcChannels.applicationToggleWindowFullScreen, async (event) => {
    assertApplicationWindowSender(event);
    const window = BrowserWindow.fromWebContents(event.sender);
    if (window && !window.isDestroyed()) {
      window.setFullScreen(!window.isFullScreen());
    }
  });

  ipcMain.handle(ipcChannels.applicationReloadWindow, async (event, force: boolean) => {
    assertApplicationWindowSender(event);
    if (typeof force !== 'boolean') {
      throw new TypeError('Application reload force flag must be a boolean.');
    }
    if (force) event.sender.reloadIgnoringCache();
    else event.sender.reload();
  });

  ipcMain.handle(ipcChannels.applicationSetMenuState, async (event, state: unknown) => {
    assertApplicationWindowSender(event);
    assertApplicationMenuState(state);
    synchronizeApplicationMenu(state);
  });

  ipcMain.handle(ipcChannels.applicationConfirmClose, async (event) => {
    assertApplicationWindowSender(event);
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

  ipcMain.handle(ipcChannels.applicationCancelClose, async (event) => {
    assertApplicationWindowSender(event);
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

  ipcMain.handle(ipcChannels.signaturePhoneStart, async (event, mode: unknown) => {
    assertApplicationWindowSender(event);
    assertPhoneSignatureMode(mode);
    return await phoneSignatureTransferService.start(event.sender.id, mode);
  });

  ipcMain.handle(ipcChannels.signaturePhonePoll, async (event, sessionId: unknown) => {
    assertApplicationWindowSender(event);
    assertPhoneSignatureSessionId(sessionId);
    return await phoneSignatureTransferService.poll(sessionId, event.sender.id);
  });

  ipcMain.handle(ipcChannels.signaturePhoneStop, async (event, sessionId: unknown) => {
    assertApplicationWindowSender(event);
    assertPhoneSignatureSessionId(sessionId);
    await phoneSignatureTransferService.stop(sessionId, event.sender.id);
  });

  ipcMain.handle(ipcChannels.signatureRecentList, async (event) => {
    assertApplicationWindowSender(event);
    return await requireRecentSignatureStore().list();
  });

  ipcMain.handle(ipcChannels.signatureRecentRemember, async (event, asset: unknown) => {
    assertApplicationWindowSender(event);
    return await requireRecentSignatureStore().remember(asset);
  });

  ipcMain.handle(ipcChannels.signatureRecentRemove, async (event, id: unknown) => {
    assertApplicationWindowSender(event);
    return await requireRecentSignatureStore().remove(id);
  });

  ipcMain.handle(ipcChannels.signatureRecentClear, async (event) => {
    assertApplicationWindowSender(event);
    return await requireRecentSignatureStore().clear();
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
    const temporarySourcePath = desktopPdfAccessRegistry.releaseDocument(event.sender.id, request.documentHandle);
    if (temporarySourcePath) await requireBlankPdfTemporaryStore().release(temporarySourcePath);
  });

  ipcMain.handle(ipcChannels.pdfLoadDocument, async (event, filePath: string) => {
    if (filePath === process.env.BP_DEFAULT_SAMPLE_PDF && existsSync(filePath)) {
      await desktopPdfAccessRegistry.authorizeSource(event.sender.id, filePath);
    }
    const openedAccess = await desktopPdfAccessRegistry.openAuthorizedSource(event.sender.id, filePath);
    let keepAccess = false;
    try {
      const payload = await loadDocumentPayload(openedAccess.sourcePath);
      await desktopPdfAccessRegistry.resolveDocument(event.sender.id, openedAccess.descriptor.handle);
      keepAccess = true;
      return { ...payload, documentAccess: openedAccess.descriptor };
    } finally {
      if (!keepAccess) {
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
  recentSignatureStore = new RecentSignatureStore({
    statePath: join(app.getPath('userData'), RECENT_SIGNATURES_FILE_NAME),
    secureStorage: safeStorage,
    sanitizeAsset: sanitizeSignatureAppearanceAsset,
  });
  const metadata = getApplicationMetadata();
  void synchronizeMacosApplicationRegistration({
    platform: process.platform,
    isPackaged: app.isPackaged,
    executablePath: app.getPath('exe'),
    bundleIdentifier: metadata.channel === 'beta' ? 'com.butterpaper.desktop.beta' : 'com.butterpaper.desktop',
  }).catch((error) => {
    console.warn('Unable to synchronize Butter Paper PDF application registration.', error);
  });
  blankPdfTemporaryStore = new BlankPdfTemporaryStore(app.getPath('temp'), `butter-paper-${metadata.channel}-blank-`);
  await blankPdfTemporaryStore.cleanupStaleSessions().catch((error) => {
    console.warn('Unable to remove stale blank PDF temporary files.', error);
  });
  updaterService = createUpdaterService();
  registerThemeListener();
  registerIpcHandlers();
  installApplicationMenu(metadata.productName);
  createMainWindow();
  unsubscribeUpdaterStatus = updaterService.subscribe((status) => {
    synchronizeApplicationMenu({ ...applicationMenuState, updateStatus: status });
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
