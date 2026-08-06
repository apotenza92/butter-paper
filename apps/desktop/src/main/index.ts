import electron from 'electron';
import { appendFileSync } from 'node:fs';
import { isAbsolute } from 'node:path';
import { ipcChannels } from '../shared/ipc';
import { resolvePdfPathsFromCommandLine } from './openPdfPaths';
import { enqueuePendingPdfPaths, hasPendingPdfPaths, takePendingPdfPaths } from './pendingPdfPaths';
import { desktopPdfAccessRegistry } from './pdfAccessRegistry';
import { bootstrapDesktop } from './window';

const { app, BrowserWindow } = electron;
const isTestMode = process.env.BP_TEST_MODE === '1';
let pendingPdfFlushScheduled = false;

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
app.on('browser-window-created', () => recordTestStartupMilestone('browser-window-created'));

process.on('unhandledRejection', (error) => {
  console.error('Unhandled rejection in main process:', error);
});

process.on('uncaughtException', (error) => {
  console.error('Uncaught exception in main process:', error);
});

const singleInstance = isTestMode ? true : app.requestSingleInstanceLock();
const initialPdfPaths = resolvePdfPathsFromCommandLine(process.argv.slice(1), process.cwd());
enqueuePendingPdfPaths(initialPdfPaths);

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
  try {
    const authorizedPaths = await Promise.all(pdfPaths.map((filePath) => (
      desktopPdfAccessRegistry.authorizeSource(window.webContents.id, filePath)
    )));
    if (!window.isDestroyed() && !window.webContents.isDestroyed()) {
      window.webContents.send(ipcChannels.applicationOpenPdfPaths, authorizedPaths);
    }
  } catch {
    console.error('Unable to authorize PDFs supplied by the operating system.');
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

function recordTestStartupMilestone(name: string, error?: unknown): void {
  const logPath = process.env.BP_TEST_STARTUP_LOG_PATH?.trim();
  if (!isTestMode || !logPath || !isAbsolute(logPath)) {
    return;
  }
  const detail = error instanceof Error
    ? `${error.name}: ${error.message}\n${error.stack ?? ''}`
    : error == null ? '' : String(error);
  try {
    appendFileSync(logPath, `${new Date().toISOString()} ${name}${detail ? ` ${detail}` : ''}\n`);
  } catch {
    // Startup diagnostics must never affect application startup.
  }
}
