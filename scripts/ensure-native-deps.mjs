import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';

const require = createRequire(import.meta.url);

await ensureElectron();
ensureEsbuild();
await ensureWindowsArmCanvasIcu();

async function ensureElectron() {
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

  await reinstallElectron(electronRoot);
  if (!existsSync(executablePath)) {
    throw new Error(`Expected Electron executable at ${executablePath} after reinstalling Electron`);
  }
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

async function ensureWindowsArmCanvasIcu() {
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
      // version marker exist. Removing the partial payload first repairs rare
      // extractions that omit ICU data while leaving electron.exe behind.
      await reinstallElectron(electronRoot);
    }
    if (!existsSync(source)) {
      throw new Error(`Expected Electron ICU data at ${source} after reinstalling Electron`);
    }
    copyFileSync(source, destination);
  }
}

async function reinstallElectron(electronRoot) {
  const electronRequire = createRequire(join(electronRoot, 'install.js'));
  const { downloadArtifact } = electronRequire('@electron/get');
  const electronPackage = JSON.parse(readFileSync(join(electronRoot, 'package.json'), 'utf8'));
  const platform = process.env.npm_config_platform || process.platform;
  const arch = process.env.npm_config_arch || process.arch;
  const archivePath = await downloadArtifact({
    version: electronPackage.version,
    artifactName: 'electron',
    checksums: JSON.parse(readFileSync(join(electronRoot, 'checksums.json'), 'utf8')),
    platform,
    arch,
  });
  const distDirectory = join(electronRoot, 'dist');
  rmSync(distDirectory, { recursive: true, force: true });
  rmSync(join(electronRoot, 'path.txt'), { force: true });
  mkdirSync(distDirectory, { recursive: true });
  extractElectronArchive(electronRoot, archivePath, distDirectory);

  const extractedTypes = join(distDirectory, 'electron.d.ts');
  if (existsSync(extractedTypes)) {
    const installedTypes = join(electronRoot, 'electron.d.ts');
    rmSync(installedTypes, { force: true });
    renameSync(extractedTypes, installedTypes);
  }
  writeFileSync(join(electronRoot, 'path.txt'), electronPlatformPath(platform));
}

function extractElectronArchive(electronRoot, archivePath, distDirectory) {
  const command = process.platform === 'darwin'
    ? '/usr/bin/ditto'
    : process.platform === 'win32'
      ? 'tar.exe'
      : 'unzip';
  const args = process.platform === 'darwin'
    ? ['-x', '-k', archivePath, distDirectory]
    : process.platform === 'win32'
      ? ['--force-local', '-xf', archivePath, '-C', distDirectory]
      : ['-q', archivePath, '-d', distDirectory];
  execFileSync(command, args, {
    cwd: electronRoot,
    stdio: 'inherit',
    env: process.env,
  });
}

function electronPlatformPath(platform) {
  if (platform === 'darwin' || platform === 'mas') {
    return 'Electron.app/Contents/MacOS/Electron';
  }
  if (platform === 'win32') {
    return 'electron.exe';
  }
  if (platform === 'linux' || platform === 'freebsd' || platform === 'openbsd') {
    return 'electron';
  }
  throw new Error(`Electron builds are not available on platform: ${platform}`);
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
