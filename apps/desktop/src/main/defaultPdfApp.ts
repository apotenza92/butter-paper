import { existsSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import type { DefaultPdfAppResult } from '../shared/protocol';

const require = createRequire(import.meta.url);
const { execFile } = require('node:child_process') as typeof import('node:child_process');
const { homedir } = require('node:os') as typeof import('node:os');
const COMMAND_TIMEOUT_MS = 120_000;

function execFileAsync(
  file: string,
  args: readonly string[],
  options: Omit<import('node:child_process').ExecFileOptionsWithStringEncoding, 'encoding'>,
): Promise<{ stdout: string; stderr: string }> {
  return new Promise((resolvePromise, reject) => {
    execFile(file, args, { ...options, encoding: 'utf8' }, (error, stdout, stderr) => {
      if (error) {
        reject(error);
      } else {
        resolvePromise({ stdout, stderr });
      }
    });
  });
}

interface DefaultPdfAppOptions {
  platform: NodeJS.Platform;
  isPackaged: boolean;
  productName: string;
  packageName: string;
  executablePath: string;
  resourcesPath: string;
  openExternal(url: string): Promise<void>;
  environment?: NodeJS.ProcessEnv;
}

export async function setAsDefaultPdfApp(options: DefaultPdfAppOptions): Promise<DefaultPdfAppResult> {
  if (!options.isPackaged) {
    throw new Error('Setting the default PDF app is available in an installed Butter Paper build.');
  }

  switch (options.platform) {
    case 'darwin':
      return setMacosDefault(options);
    case 'win32':
      return openWindowsDefaultApps(options);
    case 'linux':
      return setLinuxDefault(options);
    default:
      throw new Error(`Setting the default PDF app is not supported on ${options.platform}.`);
  }
}

async function setMacosDefault(options: DefaultPdfAppOptions): Promise<DefaultPdfAppResult> {
  const helperPath = join(options.resourcesPath, 'bin', 'set-default-pdf-app');
  if (!existsSync(helperPath)) {
    throw new Error('The macOS default-app helper is missing from this Butter Paper installation.');
  }

  const applicationBundlePath = resolve(dirname(options.executablePath), '..', '..');
  if (!applicationBundlePath.endsWith('.app')) {
    throw new Error('Butter Paper must be run from its installed application bundle.');
  }

  await execFileAsync(helperPath, [applicationBundlePath], {
    timeout: COMMAND_TIMEOUT_MS,
    windowsHide: true,
  });
  return {
    outcome: 'changed',
    message: `${options.productName} is now the default app for PDF files.`,
  };
}

async function openWindowsDefaultApps(options: DefaultPdfAppOptions): Promise<DefaultPdfAppResult> {
  const registeredApp = encodeURIComponent(options.productName);
  await options.openExternal(`ms-settings:defaultapps?registeredAppUser=${registeredApp}`);
  return {
    outcome: 'requires-confirmation',
    message: `Choose ${options.productName} for .pdf files in Windows Default Apps.`,
  };
}

async function setLinuxDefault(options: DefaultPdfAppOptions): Promise<DefaultPdfAppResult> {
  const environment = options.environment ?? process.env;
  const desktopFileName = `${options.packageName}.desktop`;
  await ensureLinuxDesktopEntry({
    desktopFileName,
    environment,
    executablePath: environment.APPIMAGE || options.executablePath,
    packageName: options.packageName,
    productName: options.productName,
  });

  await execFileAsync('xdg-mime', ['default', desktopFileName, 'application/pdf'], {
    env: environment,
    timeout: COMMAND_TIMEOUT_MS,
    windowsHide: true,
  });
  const { stdout } = await execFileAsync('xdg-mime', ['query', 'default', 'application/pdf'], {
    env: environment,
    timeout: COMMAND_TIMEOUT_MS,
    windowsHide: true,
  });
  if (stdout.trim() !== desktopFileName) {
    throw new Error('The desktop environment did not retain Butter Paper as the default PDF app.');
  }

  return {
    outcome: 'changed',
    message: `${options.productName} is now the default app for PDF files.`,
  };
}

interface LinuxDesktopEntryOptions {
  desktopFileName: string;
  environment: NodeJS.ProcessEnv;
  executablePath: string;
  packageName: string;
  productName: string;
}

async function ensureLinuxDesktopEntry(options: LinuxDesktopEntryOptions): Promise<void> {
  const dataHome = options.environment.XDG_DATA_HOME || join(homedir(), '.local', 'share');
  const userApplicationsDirectory = join(dataHome, 'applications');
  const userDesktopEntryPath = join(userApplicationsDirectory, options.desktopFileName);
  const systemDesktopEntryPaths = [
    join('/usr/local/share/applications', options.desktopFileName),
    join('/usr/share/applications', options.desktopFileName),
  ];
  if (existsSync(userDesktopEntryPath) || systemDesktopEntryPaths.some(existsSync)) {
    return;
  }

  await mkdir(userApplicationsDirectory, { recursive: true });
  await writeFile(userDesktopEntryPath, [
    '[Desktop Entry]',
    'Type=Application',
    `Name=${escapeDesktopEntryValue(options.productName)}`,
    `Exec=${quoteDesktopExecArgument(options.executablePath)} %U`,
    `Icon=${escapeDesktopEntryValue(options.packageName)}`,
    'Terminal=false',
    'NoDisplay=true',
    'Categories=Office;',
    'MimeType=application/pdf;',
    '',
  ].join('\n'), { encoding: 'utf8', mode: 0o644 });

  try {
    await execFileAsync('update-desktop-database', [userApplicationsDirectory], {
      env: options.environment,
      timeout: COMMAND_TIMEOUT_MS,
      windowsHide: true,
    });
  } catch (error) {
    if (!isMissingCommandError(error)) {
      throw error;
    }
  }
}

export function quoteDesktopExecArgument(value: string): string {
  return `"${value.replaceAll('\\', '\\\\').replaceAll('"', '\\"').replaceAll('`', '\\`').replaceAll('$', '\\$')}"`;
}

function escapeDesktopEntryValue(value: string): string {
  return value.replaceAll('\\', '\\\\').replaceAll('\n', '\\n');
}

function isMissingCommandError(error: unknown): boolean {
  return Boolean(error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT');
}
