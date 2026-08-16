import electron from 'electron';
import { isAbsolute } from 'node:path';
import { ipcChannels } from '../shared/ipc';
import { resolvePdfPathsFromCommandLine } from './openPdfPaths';
import { enqueuePendingPdfPaths, hasPendingPdfPaths, takePendingPdfPaths } from './pendingPdfPaths';
import { desktopPdfAccessRegistry } from './pdfAccessRegistry';
import { recordTestStartupMilestone } from './startupDiagnostics';
import { bootstrapDesktop } from './window';

const { app, BrowserWindow } = electron;
const isTestMode = process.env.BP_TEST_MODE === '1';
let pendingPdfFlushScheduled = false;
let activePdfOpenDispatches = 0;
let pdfSessionPreparationPromise: Promise<unknown> | null = null;

const testUserDataDir = process.env.BP_TEST_USER_DATA_DIR?.trim();
if (testUserDataDir) {
  if (!isTestMode || !isAbsolute(testUserDataDir)) {
    throw new Error('BP_TEST_USER_DATA_DIR requires BP_TEST_MODE=1 and an absolute path.');
  }
  app.setPath('userData', testUserDataDir);
  app.setPath('sessionData', testUserDataDir);
}

recordTestStartupMilestone('main-module-loaded');
app.on('ready', () => recordTestStartupMilestone('app-ready'));
app.on('browser-window-created', (_event, window) => {
  recordTestStartupMilestone('browser-window-created');
  window.webContents.once('did-start-loading', () => recordTestStartupMilestone('renderer-load-started'));
  window.webContents.once('dom-ready', () => recordTestStartupMilestone('renderer-dom-ready'));
  window.webContents.once('did-finish-load', () => recordTestStartupMilestone('renderer-load-finished'));
  window.once('ready-to-show', () => recordTestStartupMilestone('window-ready-to-show'));
});

process.on('unhandledRejection', (error) => {
  console.error('Unhandled rejection in main process:', error);
});

process.on('uncaughtException', (error) => {
  console.error('Uncaught exception in main process:', error);
});

const singleInstance = isTestMode ? true : app.requestSingleInstanceLock();
const initialPdfPaths = resolvePdfPathsFromCommandLine(process.argv.slice(1), process.cwd());
enqueuePendingPdfPaths(initialPdfPaths);
if (initialPdfPaths.length > 0) {
  preparePdfSessionForOpen();
}

app.on('open-file', (event, filePath) => {
  event.preventDefault();
  void dispatchPdfPaths([filePath]);
});

if (!singleInstance) {
  app.quit();
} else {
  recordTestStartupMilestone('bootstrap-started');
  void bootstrapDesktop()
    .then(() => {
      recordTestStartupMilestone('bootstrap-completed');
      void flushPendingPdfPaths();
    })
    .catch((error) => {
      recordTestStartupMilestone('bootstrap-failed', error);
      console.error('Unable to start Butter Paper:', error);
      app.exit(1);
    });
}

app.on('second-instance', (_event, commandLine, workingDirectory) => {
  const focused = BrowserWindow.getAllWindows()[0];
  if (focused) {
    if (focused.isMinimized()) {
      focused.restore();
    }
    focused.focus();
  }
  void dispatchPdfPaths(resolvePdfPathsFromCommandLine(commandLine, workingDirectory));
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

async function dispatchPdfPaths(filePaths: readonly string[]): Promise<void> {
  const pdfPaths = resolvePdfPathsFromCommandLine(filePaths, process.cwd());
  if (pdfPaths.length === 0) {
    return;
  }
  preparePdfSessionForOpen();

  const window = BrowserWindow.getAllWindows()[0];
  if (!window || window.isDestroyed()) {
    queuePendingPdfPaths(pdfPaths);
    return;
  }
  if (window.webContents.isLoadingMainFrame()) {
    queuePendingPdfPaths(pdfPaths);
    if (!pendingPdfFlushScheduled) {
      pendingPdfFlushScheduled = true;
      window.webContents.once('did-finish-load', () => {
        pendingPdfFlushScheduled = false;
        void flushPendingPdfPaths();
      });
    }
    return;
  }

  if (window.isMinimized()) {
    window.restore();
  }
  window.focus();
  activePdfOpenDispatches += 1;
  window.webContents.send(ipcChannels.applicationPdfOpenPendingChanged, true);
  try {
    const authorizedPaths = await Promise.all(pdfPaths.map((filePath) => (
      desktopPdfAccessRegistry.authorizeSource(window.webContents.id, filePath)
    )));
    if (!window.isDestroyed() && !window.webContents.isDestroyed()) {
      window.webContents.send(ipcChannels.applicationOpenPdfPaths, authorizedPaths);
    }
  } catch {
    console.error('Unable to authorize PDFs supplied by the operating system.');
  } finally {
    activePdfOpenDispatches = Math.max(0, activePdfOpenDispatches - 1);
    if (!window.isDestroyed() && !window.webContents.isDestroyed()) {
      window.webContents.send(ipcChannels.applicationPdfOpenPendingChanged, activePdfOpenDispatches > 0);
    }
  }
}

async function flushPendingPdfPaths(): Promise<void> {
  if (!hasPendingPdfPaths()) {
    return;
  }
  const filePaths = takePendingPdfPaths();
  await dispatchPdfPaths(filePaths);
}

function queuePendingPdfPaths(filePaths: readonly string[]): void {
  enqueuePendingPdfPaths(filePaths);
}

function preparePdfSessionForOpen(): void {
  if (pdfSessionPreparationPromise) {
    return;
  }
  recordTestStartupMilestone('pdf-session-preparation-started');
  pdfSessionPreparationPromise = import('./pdfSession')
    .then(() => {
      recordTestStartupMilestone('pdf-session-preparation-completed');
    })
    .catch((error) => {
      recordTestStartupMilestone('pdf-session-preparation-failed', error);
      console.warn('Unable to prepare PDF services before opening a document.', error);
    });
}
