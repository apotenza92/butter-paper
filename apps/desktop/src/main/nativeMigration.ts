import electron from 'electron';
import { execFile, spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { access, mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import {
  MAC_DESIGNATED_REQUIREMENT,
  NATIVE_RELEASE,
  isNativeBundleVersion,
  linuxPackageDirectory,
  macBundlePath,
  macDestinationDirectory,
  macElectronBundleCandidates,
  nativePackageUrl,
  peMachine,
  requiredPeMachine,
  resolveMigrationEligibility,
  windowsHandoverScript,
  type NativePackage,
} from './nativeMigrationPlan';

const { app, BrowserWindow, dialog, net, shell } = electron;
const run = promisify(execFile);

class MigrationError extends Error {}

// Returns true when the native app has been installed and opened and this
// Electron process is exiting; false to continue starting Electron normally.
export async function migrateToNativeApp(pdfPaths: readonly string[]): Promise<boolean> {
  const eligibility = resolveMigrationEligibility({
    platform: process.platform,
    arch: process.arch,
    runningUnderArm64Translation: app.runningUnderARM64Translation,
    isPackaged: app.isPackaged,
    isTestMode: process.env.BP_TEST_MODE === '1',
    disabledByEnvironment: process.env.BP_DISABLE_NATIVE_MIGRATION === '1',
    macosVersion: process.platform === 'darwin' ? process.getSystemVersion() : undefined,
    executablePath: app.getPath('exe'),
    appImagePath: process.env.APPIMAGE,
  });
  if (!eligibility.eligible) {
    if (eligibility.reason === 'macos-too-old') {
      await dialog.showMessageBox({
        type: 'info',
        message: 'The new Butter Paper needs a newer version of macOS',
        detail: `Butter Paper ${NATIVE_RELEASE.version} requires macOS ${eligibility.minimumMacosMajorVersion} or later. You can keep using this version until you update macOS.`,
        buttons: ['OK'],
      });
    }
    return false;
  }

  const choice = await dialog.showMessageBox({
    type: 'info',
    message: 'Butter Paper is now a native app',
    detail: `This update replaces this version with Butter Paper ${NATIVE_RELEASE.version}, rebuilt as a faster native app. It downloads about ${Math.round(eligibility.nativePackage.bytes / 1_000_000)} MB, installs the new app, removes the old Butter Paper apps and opens the new one. Your PDFs are not changed.`,
    buttons: ['Update Now', 'Later'],
    defaultId: 0,
    cancelId: 1,
  });
  if (choice.response !== 0) {
    return false;
  }

  const progress = createProgressWindow();
  const workDirectory = await mkdtemp(join(app.getPath('temp'), 'butter-paper-native-'));
  try {
    const archive = join(workDirectory, eligibility.nativePackage.assetName);
    await downloadVerified(eligibility.nativePackage, archive, (fraction) => progress.update(`Downloading… ${Math.round(fraction * 100)}%`));
    progress.update('Installing…');
    if (process.platform === 'darwin') {
      await installOnMac(archive, workDirectory, pdfPaths);
    } else if (process.platform === 'win32') {
      await handOverOnWindows(eligibility.nativePackage, archive, workDirectory, pdfPaths);
    } else {
      await installOnLinux(eligibility.nativePackage, archive, workDirectory, pdfPaths);
    }
    progress.close();
    app.exit(0);
    return true;
  } catch (error) {
    progress.close();
    if (process.platform !== 'win32') {
      await rm(workDirectory, { recursive: true, force: true }).catch(() => undefined);
    }
    console.error('Native migration failed:', error);
    const retry = await dialog.showMessageBox({
      type: 'warning',
      message: 'Butter Paper could not install the new app',
      detail: `${error instanceof MigrationError ? error.message : 'An unexpected error occurred.'} You can keep using this version, and Butter Paper will try again next time it starts. You can also download the new app yourself.`,
      buttons: ['Continue', 'Open Download Page'],
      defaultId: 0,
      cancelId: 0,
    });
    if (retry.response === 1) {
      void shell.openExternal(NATIVE_RELEASE.pageUrl);
    }
    return false;
  }
}

function createProgressWindow() {
  const window = new BrowserWindow({
    width: 380,
    height: 120,
    resizable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    title: 'Butter Paper',
    autoHideMenuBar: true,
    webPreferences: { sandbox: true, contextIsolation: true, javascript: true },
  });
  window.setMenu(null);
  const page = '<!doctype html><meta charset="utf-8"><body style="margin:0;font:14px system-ui;display:grid;place-items:center;height:100vh;color:#222;background:#fafafa"><p id="s">Preparing…</p></body>';
  void window.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(page)}`);
  return {
    update(text: string) {
      if (!window.isDestroyed()) {
        void window.webContents.executeJavaScript(`document.getElementById('s').textContent=${JSON.stringify(text)}`).catch(() => undefined);
      }
    },
    close() {
      if (!window.isDestroyed()) {
        window.destroy();
      }
    },
  };
}

async function downloadVerified(nativePackage: NativePackage, destination: string, onProgress: (fraction: number) => void): Promise<void> {
  const response = await net.fetch(nativePackageUrl(nativePackage));
  if (!response.ok || response.body == null) {
    throw new MigrationError(`The download failed (HTTP ${response.status}).`);
  }
  const hash = createHash('sha256');
  const output = createWriteStream(destination, { flags: 'wx' });
  let received = 0;
  const reader = response.body.getReader();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      received += value.byteLength;
      if (received > nativePackage.bytes) {
        throw new MigrationError('The download was larger than expected.');
      }
      hash.update(value);
      if (!output.write(value)) {
        await new Promise((resolveDrain) => output.once('drain', resolveDrain));
      }
      onProgress(received / nativePackage.bytes);
    }
  } finally {
    await new Promise((resolveClose) => output.end(resolveClose));
  }
  if (received !== nativePackage.bytes || hash.digest('hex') !== nativePackage.sha256) {
    throw new MigrationError('The download did not match the published release.');
  }
}

async function exists(path: string): Promise<boolean> {
  return access(path).then(() => true, () => false);
}

async function plistValue(bundle: string, key: string): Promise<string | null> {
  try {
    const { stdout } = await run('/usr/bin/plutil', ['-extract', key, 'raw', '-o', '-', join(bundle, 'Contents/Info.plist')]);
    return stdout.trim();
  } catch {
    return null;
  }
}

async function installOnMac(archive: string, workDirectory: string, pdfPaths: readonly string[]): Promise<void> {
  const home = homedir();
  const runningBundle = macBundlePath(app.getPath('exe'));
  if (runningBundle == null) {
    throw new MigrationError('This copy of Butter Paper is not in an app bundle.');
  }
  const extracted = join(workDirectory, 'extracted');
  await mkdir(extracted);
  await run('/usr/bin/ditto', ['-x', '-k', archive, extracted]);
  const downloadedApp = join(extracted, 'Butter Paper.app');
  try {
    await run('/usr/bin/codesign', ['--verify', '--deep', '--strict', '-R', MAC_DESIGNATED_REQUIREMENT, downloadedApp]);
    await run('/usr/sbin/spctl', ['--assess', '--type', 'execute', downloadedApp]);
  } catch {
    throw new MigrationError('The new app’s signature could not be verified.');
  }
  if (!isNativeBundleVersion(await plistValue(downloadedApp, 'CFBundleShortVersionString'))) {
    throw new MigrationError('The downloaded app is not the expected version.');
  }

  const destinationDirectory = macDestinationDirectory(runningBundle, home);
  const destination = join(destinationDirectory, 'Butter Paper.app');
  const alreadyNative = await exists(destination)
    && await plistValue(destination, 'CFBundleIdentifier') === NATIVE_RELEASE.macBundleIdentifier
    && isNativeBundleVersion(await plistValue(destination, 'CFBundleShortVersionString'));
  if (!alreadyNative) {
    // Stage beside the destination so the final step is a same-volume rename.
    const staging = join(destinationDirectory, `.Butter Paper native ${randomBytes(4).toString('hex')}.app`);
    try {
      await run('/usr/bin/ditto', [downloadedApp, staging]);
    } catch {
      throw new MigrationError(`Butter Paper could not write to ${destinationDirectory}.`);
    }
    try {
      if (await exists(destination)) {
        await shell.trashItem(destination);
      }
      await rename(staging, destination);
    } catch {
      await rm(staging, { recursive: true, force: true }).catch(() => undefined);
      throw new MigrationError('The old app could not be replaced.');
    }
  }

  // Remove every remaining Electron Butter Paper app, including this one.
  for (const candidate of new Set([runningBundle, ...macElectronBundleCandidates(home)])) {
    if (candidate === destination || !(await exists(candidate))) {
      continue;
    }
    const identifier = await plistValue(candidate, 'CFBundleIdentifier');
    const version = await plistValue(candidate, 'CFBundleShortVersionString');
    if (identifier?.startsWith(NATIVE_RELEASE.macBundleIdentifier) && !isNativeBundleVersion(version)) {
      await shell.trashItem(candidate).catch((error) => console.warn(`Could not remove ${candidate}:`, error));
    }
  }
  await rm(workDirectory, { recursive: true, force: true }).catch(() => undefined);
  spawn('/usr/bin/open', ['-n', '-a', destination, ...pdfPaths], { detached: true, stdio: 'ignore' }).unref();
}

async function ensureWindowsRuntime(nativePackage: NativePackage, workDirectory: string): Promise<void> {
  const runtimePath = join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'vcruntime140.dll');
  const expected = requiredPeMachine(nativePackage.target);
  const installed = async () => peMachine(await readFile(runtimePath).catch(() => new Uint8Array())) === expected;
  if (await installed()) {
    return;
  }
  const architecture = nativePackage.target === 'windows-arm64' ? 'arm64' : 'x64';
  const response = await net.fetch(`https://aka.ms/vs/17/release/vc_redist.${architecture}.exe`);
  if (!response.ok) {
    throw new MigrationError('The Microsoft Visual C++ runtime could not be downloaded.');
  }
  const installer = join(workDirectory, `vc_redist.${architecture}.exe`);
  await writeFile(installer, new Uint8Array(await response.arrayBuffer()), { flag: 'wx' });
  const quoted = `'${installer.replaceAll("'", "''")}'`;
  const script = `$s = Get-AuthenticodeSignature -LiteralPath ${quoted}; if ($s.Status -ne 'Valid' -or $s.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { exit 2 }; $p = Start-Process -FilePath ${quoted} -ArgumentList '/install','/quiet','/norestart' -Verb RunAs -Wait -PassThru; exit $p.ExitCode`;
  const exitCode = await new Promise<number | null>((resolveExit) => {
    const child = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true, stdio: 'ignore' });
    child.once('error', () => resolveExit(null));
    child.once('exit', (code) => resolveExit(code));
  });
  if (exitCode === 2) {
    throw new MigrationError('The Microsoft Visual C++ runtime installer was not signed by Microsoft.');
  }
  if (!(await installed())) {
    throw new MigrationError('The new app needs the Microsoft Visual C++ runtime, which could not be installed.');
  }
}

async function handOverOnWindows(nativePackage: NativePackage, archive: string, workDirectory: string, pdfPaths: readonly string[]): Promise<void> {
  await ensureWindowsRuntime(nativePackage, workDirectory);
  const packageDirectory = join(workDirectory, 'package');
  const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
  await run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `Expand-Archive -LiteralPath ${quote(archive)} -DestinationPath ${quote(packageDirectory)}`], { windowsHide: true });
  if (!(await exists(join(packageDirectory, 'install.ps1')))) {
    throw new MigrationError('The downloaded package is incomplete.');
  }
  const scriptPath = join(workDirectory, 'handover.ps1');
  const logPath = join(app.getPath('temp'), 'butter-paper-native-migration.log');
  await writeFile(scriptPath, windowsHandoverScript({ electronProcessId: process.pid, packageDirectory, target: nativePackage.target, pdfPaths, logPath }), { encoding: 'utf8' });
  spawn('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', scriptPath], {
    detached: true,
    stdio: 'ignore',
    windowsHide: true,
  }).unref();
}

async function installOnLinux(nativePackage: NativePackage, archive: string, workDirectory: string, pdfPaths: readonly string[]): Promise<void> {
  await run('tar', ['-xJf', archive, '-C', workDirectory]);
  const packageDirectory = join(workDirectory, linuxPackageDirectory(nativePackage.target));
  const dataHome = process.env.XDG_DATA_HOME?.startsWith('/') ? process.env.XDG_DATA_HOME : join(homedir(), '.local/share');
  const executable = join(dataHome, 'butter-paper', NATIVE_RELEASE.version, 'gpui-migration');
  if (!(await exists(executable))) {
    try {
      await run('/bin/sh', [join(packageDirectory, 'install-user.sh')]);
    } catch (error) {
      const detail = (error as { stderr?: string }).stderr?.trim();
      throw new MigrationError(`The new app could not be installed${detail ? `: ${detail}` : '.'}`);
    }
  }
  spawn(executable, [...pdfPaths], { detached: true, stdio: 'ignore' }).unref();
  const appImage = process.env.APPIMAGE;
  if (appImage) {
    await rm(appImage, { force: true }).catch((error) => console.warn('Could not remove the old AppImage:', error));
  }
  await rm(workDirectory, { recursive: true, force: true }).catch(() => undefined);
}
