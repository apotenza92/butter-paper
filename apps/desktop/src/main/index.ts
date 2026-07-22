import electron from 'electron';
import { bootstrapDesktop } from './window';

const { app, BrowserWindow } = electron;
const isTestMode = process.env.BP_TEST_MODE === '1';

process.on('unhandledRejection', (error) => {
  console.error('Unhandled rejection in main process:', error);
});

process.on('uncaughtException', (error) => {
  console.error('Uncaught exception in main process:', error);
});

const singleInstance = isTestMode ? true : app.requestSingleInstanceLock();

if (!singleInstance) {
  app.quit();
} else {
  void bootstrapDesktop();
}

app.on('second-instance', () => {
  const focused = BrowserWindow.getAllWindows()[0];
  if (focused) {
    if (focused.isMinimized()) {
      focused.restore();
    }
    focused.focus();
  }
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
