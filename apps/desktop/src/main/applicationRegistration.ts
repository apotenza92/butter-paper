import { existsSync, realpathSync } from 'node:fs';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { dirname, resolve } from 'node:path';

const require = createRequire(import.meta.url);
const { execFile } = require('node:child_process') as typeof import('node:child_process');

interface ApplicationRegistrationOptions {
  readonly platform: NodeJS.Platform;
  readonly isPackaged: boolean;
  readonly executablePath: string;
  readonly bundleIdentifier: string;
}

const LAUNCH_SERVICES_REGISTER = '/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister';
const STABLE_BUNDLE_IDENTIFIER = 'com.butterpaper.desktop';
const BETA_BUNDLE_IDENTIFIER = 'com.butterpaper.desktop.beta';
const BUTTER_PAPER_BUNDLE_IDENTIFIERS = new Set([
  STABLE_BUNDLE_IDENTIFIER,
  BETA_BUNDLE_IDENTIFIER,
]);

export async function synchronizeMacosApplicationRegistration(
  options: ApplicationRegistrationOptions,
): Promise<void> {
  if (options.platform !== 'darwin' || !options.isPackaged) {
    return;
  }

  if (!existsSync(LAUNCH_SERVICES_REGISTER)) {
    return;
  }

  const applicationBundlePath = resolve(dirname(options.executablePath), '..', '..');
  if (!applicationBundlePath.endsWith('.app')) {
    throw new Error('Butter Paper must be run from its installed application bundle.');
  }

  if (!BUTTER_PAPER_BUNDLE_IDENTIFIERS.has(options.bundleIdentifier)) {
    throw new Error(`Unknown Butter Paper bundle identifier: ${options.bundleIdentifier}`);
  }

  const currentApplicationPath = realpathSync(applicationBundlePath);
  const dump = await execFileAsync(LAUNCH_SERVICES_REGISTER, ['-dump']);
  const registrations = parseLaunchServicesRegistrations(dump.stdout);
  const preferredPathByBundleIdentifier = new Map<string, string>([
    [options.bundleIdentifier, currentApplicationPath],
  ]);
  for (const bundleIdentifier of BUTTER_PAPER_BUNDLE_IDENTIFIERS) {
    if (bundleIdentifier === options.bundleIdentifier) {
      continue;
    }

    const installedCandidate = registrations
      .filter((registration) => registration.bundleIdentifier === bundleIdentifier)
      .map((registration) => registration.path)
      .filter((path) => existsSync(path) && isInstalledApplicationPath(path))
      .sort((left, right) => applicationPathPriority(left) - applicationPathPriority(right))[0];
    if (installedCandidate) {
      preferredPathByBundleIdentifier.set(bundleIdentifier, installedCandidate);
    }
  }

  for (const registration of registrations) {
    const preferredPath = preferredPathByBundleIdentifier.get(registration.bundleIdentifier);
    if (preferredPath && samePath(registration.path, preferredPath)) {
      continue;
    }

    await execFileAsync(LAUNCH_SERVICES_REGISTER, ['-u', registration.path]).catch(() => undefined);
  }

  await execFileAsync(LAUNCH_SERVICES_REGISTER, ['-f', currentApplicationPath]);
}

interface LaunchServicesRegistration {
  readonly bundleIdentifier: string;
  readonly path: string;
}

function parseLaunchServicesRegistrations(dump: string): LaunchServicesRegistration[] {
  const registrations: LaunchServicesRegistration[] = [];
  for (const block of dump.split(/\n-{8,}\n/)) {
    const bundleIdentifier = block.match(/^identifier:\s+([^\s(]+)/m)?.[1];
    const applicationPath = block.match(/^path:\s+(.+?)\s+\(0x[0-9a-f]+\)$/m)?.[1];
    if (bundleIdentifier && applicationPath && BUTTER_PAPER_BUNDLE_IDENTIFIERS.has(bundleIdentifier)) {
      registrations.push({ bundleIdentifier, path: applicationPath });
    }
  }
  return registrations;
}

function samePath(left: string, right: string): boolean {
  return left === right || (existsSync(left) && realpathSync(left) === right);
}

function isInstalledApplicationPath(applicationPath: string): boolean {
  // Repository release copies, test bundles, and trashed apps must not count
  // as installed choices in Finder's Open With list.
  const applicationRoots = ['/Applications', resolve(homedir(), 'Applications')];
  return applicationRoots.some((root) => applicationPath === root || applicationPath.startsWith(`${root}/`));
}

function applicationPathPriority(applicationPath: string): number {
  return applicationPath.startsWith('/Applications/') ? 0 : 1;
}

function execFileAsync(file: string, args: readonly string[]): Promise<{ stdout: string; stderr: string }> {
  return new Promise((resolvePromise, reject) => {
    execFile(file, args, { encoding: 'utf8', timeout: 30_000, maxBuffer: 64 * 1024 * 1024 }, (error, stdout, stderr) => {
      if (error) {
        reject(error);
      } else {
        resolvePromise({ stdout, stderr });
      }
    });
  });
}
