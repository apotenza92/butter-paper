import { copyFileSync, existsSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';

const require = createRequire(import.meta.url);

ensureElectron();
ensureEsbuild();
ensureWindowsArmCanvasIcu();

function ensureElectron() {
  const electronRoot = packageRoot('electron');
  if (!electronRoot) {
    return;
  }

  const executableName = process.platform === 'win32' ? 'electron.exe' : 'electron';
  const executablePath =
    process.platform === 'darwin'
      ? join(electronRoot, 'dist', 'Electron.app', 'Contents', 'MacOS', 'Electron')
      : join(electronRoot, 'dist', executableName);

  if (existsSync(executablePath)) {
    return;
  }

  runPackageInstaller(electronRoot, 'install.js');
}

function ensureEsbuild() {
  const esbuildRoot = packageRoot('esbuild');
  if (!esbuildRoot) {
    return;
  }

  try {
    const esbuild = require('esbuild');
    esbuild.transformSync('let nativeDependencyCheck = 1;', { loader: 'js' });
    return;
  } catch {
    runPackageInstaller(esbuildRoot, 'install.js');
  }
}

function ensureWindowsArmCanvasIcu() {
  if (process.platform !== 'win32' || process.arch !== 'arm64') {
    return;
  }

  const electronRoot = packageRoot('electron');
  const canvasRoot = packageRoot('@napi-rs/canvas-win32-arm64-msvc');
  if (!electronRoot || !canvasRoot) {
    return;
  }

  const source = join(electronRoot, 'dist', 'icudtl.dat');
  const destination = join(canvasRoot, 'icudtl.dat');
  if (!existsSync(destination)) {
    if (!existsSync(source)) {
      // Electron's installer considers the payload complete when its executable and
      // version marker exist. Requiring a fresh marker check repairs rare partial
      // extractions that omit ICU data while leaving electron.exe behind.
      rmSync(join(electronRoot, 'dist', 'version'), { force: true });
      runPackageInstaller(electronRoot, 'install.js');
    }
    if (!existsSync(source)) {
      throw new Error(`Expected Electron ICU data at ${source} after reinstalling Electron`);
    }
    copyFileSync(source, destination);
  }
}

function packageRoot(packageName) {
  try {
    return dirname(require.resolve(`${packageName}/package.json`));
  } catch {
    return null;
  }
}

function runPackageInstaller(packageRootPath, installerName) {
  const installerPath = resolve(packageRootPath, installerName);
  if (!existsSync(installerPath)) {
    throw new Error(`Expected native dependency installer at ${installerPath}`);
  }

  execFileSync(process.execPath, [installerPath], {
    cwd: packageRootPath,
    stdio: 'inherit',
    env: process.env,
  });
}
