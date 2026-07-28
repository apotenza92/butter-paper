import electron from 'electron';
import { isAbsolute } from 'node:path';
import { ipcChannels } from '../shared/ipc';
import { resolvePdfPathsFromCommandLine } from './openPdfPaths';
import { enqueuePendingPdfPaths, hasPendingPdfPaths, takePendingPdfPaths } from './pendingPdfPaths';
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
  dispatchPdfPaths([filePath]);
});

if (!singleInstance) {
  app.quit();
} else {
  void bootstrapDesktop().then(() => {
    flushPendingPdfPaths();
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
  dispatchPdfPaths(resolvePdfPathsFromCommandLine(commandLine, workingDirectory));
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

function dispatchPdfPaths(filePaths: readonly string[]): void {
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
        flushPendingPdfPaths();
      });
    }
    return;
  }

  if (window.isMinimized()) {
    window.restore();
  }
  window.focus();
  window.webContents.send(ipcChannels.applicationOpenPdfPaths, pdfPaths);
}

function flushPendingPdfPaths(): void {
  if (!hasPendingPdfPaths()) {
    return;
  }
  const filePaths = takePendingPdfPaths();
  dispatchPdfPaths(filePaths);
}

function queuePendingPdfPaths(filePaths: readonly string[]): void {
  enqueuePendingPdfPaths(filePaths);
}
