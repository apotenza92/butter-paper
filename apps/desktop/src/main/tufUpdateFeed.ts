import {
  constants,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
} from 'node:fs';
import { createServer, type Server } from 'node:http';
import { join, posix } from 'node:path';
import { BaseFetcher, Updater, type Fetcher } from 'tuf-js';
import { DownloadHTTPError } from 'tuf-js/dist/error.js';

export interface TufUpdaterLike {
  refresh(): Promise<void>;
  getTargetInfo(targetName: string): Promise<unknown>;
  downloadTarget(targetInfo: unknown, destinationPath: string): Promise<string>;
}

export interface TufUpdaterConstructor {
  new(options: {
    metadataBaseUrl: string;
    targetBaseUrl: string;
    metadataDir: string;
    targetDir: string;
    config: { userAgent: string };
    fetcher?: Fetcher;
  }): TufUpdaterLike;
}

export interface TufVerifiedUpdateFeed {
  close(): Promise<void>;
  feedUrl: string;
  refresh(): Promise<string>;
  targetPath: string;
  trustInitialized: boolean;
  trustedRootPath: string;
}

export interface CreateTufVerifiedUpdateFeedOptions {
  embeddedRootPath: string;
  repositoryUrl: string;
  targetName: string;
  trustDirectory: string;
  allowLoopbackHttp?: boolean;
  UpdaterClass?: TufUpdaterConstructor;
}

class NoRedirectTufFetcher extends BaseFetcher {
  constructor(private readonly repositoryUrl: string) {
    super();
  }

  async fetch(url: string): Promise<ReadableStream<Uint8Array<ArrayBuffer>>> {
    const requested = new URL(url);
    const repository = new URL(`${this.repositoryUrl}/`);
    if (requested.origin !== repository.origin
      || (!requested.pathname.startsWith(`${repository.pathname}metadata/`)
        && !requested.pathname.startsWith(`${repository.pathname}targets/`))
      || requested.username !== ''
      || requested.password !== ''
      || requested.search !== ''
      || requested.hash !== '') {
      throw new Error(`TUF attempted an unexpected repository URL: ${url}`);
    }

    const response = await fetch(requested, {
      headers: { 'User-Agent': 'Butter Paper desktop updater' },
      redirect: 'error',
      signal: AbortSignal.timeout(30_000),
    });
    if (!response.ok || response.body == null) {
      if (response.status === 403 || response.status === 404) {
        throw new DownloadHTTPError('TUF metadata was not found.', response.status);
      }
      throw new Error(`TUF download failed with HTTP ${response.status}: ${requested}`);
    }
    if (response.redirected || response.url !== requested.toString()) {
      throw new Error(`TUF metadata redirects are not allowed: ${requested}`);
    }
    return response.body;
  }
}

function isLoopbackHost(hostname: string): boolean {
  return hostname === '127.0.0.1' || hostname === '::1' || hostname === 'localhost';
}

export function validateTufRepositoryUrl(
  value: string,
  options: { allowLoopbackHttp?: boolean } = {},
): string {
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error('Butter Paper TUF repository URL is invalid.');
  }

  const loopbackTestUrl = options.allowLoopbackHttp === true
    && parsed.protocol === 'http:'
    && isLoopbackHost(parsed.hostname)
    && parsed.port !== '';
  if (parsed.protocol !== 'https:' && !loopbackTestUrl) {
    throw new Error('Butter Paper TUF repositories must use HTTPS; loopback HTTP is test-only.');
  }
  if (parsed.username !== '' || parsed.password !== '' || parsed.search !== '' || parsed.hash !== '') {
    throw new Error('Butter Paper TUF repository URLs must not contain credentials, queries, or fragments.');
  }

  return parsed.toString().replace(/\/$/, '');
}

export function validateTufTargetName(value: string): string {
  if (value === ''
    || value !== posix.basename(value)
    || value.includes('\\')
    || value.includes('\0')) {
    throw new Error(`Unsafe Butter Paper TUF update target name: ${value}`);
  }
  return value;
}

export function initializeTrustedRoot(options: {
  embeddedRootPath: string;
  metadataDirectory: string;
}): { initialized: boolean; trustedRootPath: string } {
  const trustedRootPath = join(options.metadataDirectory, 'root.json');
  mkdirSync(options.metadataDirectory, { recursive: true, mode: 0o700 });

  if (existsSync(trustedRootPath)) {
    if (!statSync(trustedRootPath).isFile()) {
      throw new Error('The persisted Butter Paper TUF root is not a regular file.');
    }
    return { initialized: false, trustedRootPath };
  }
  if (!existsSync(options.embeddedRootPath) || !statSync(options.embeddedRootPath).isFile()) {
    throw new Error('Butter Paper has no embedded TUF trust root.');
  }

  copyFileSync(options.embeddedRootPath, trustedRootPath, constants.COPYFILE_EXCL);
  return { initialized: true, trustedRootPath };
}

function listen(server: Server): Promise<void> {
  return new Promise((resolve, reject) => {
    const onError = (error: Error) => {
      server.off('listening', onListening);
      reject(error);
    };
    const onListening = () => {
      server.off('error', onError);
      resolve();
    };
    server.once('error', onError);
    server.once('listening', onListening);
    server.listen(0, '127.0.0.1');
  });
}

function close(server: Server): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close(error => {
      if (error) {
        reject(error);
      } else {
        resolve();
      }
    });
    server.closeIdleConnections?.();
  });
}

export async function createTufVerifiedUpdateFeed(
  options: CreateTufVerifiedUpdateFeedOptions,
): Promise<TufVerifiedUpdateFeed> {
  const repositoryUrl = validateTufRepositoryUrl(options.repositoryUrl, {
    allowLoopbackHttp: options.allowLoopbackHttp,
  });
  const targetName = validateTufTargetName(options.targetName);
  const metadataDirectory = join(options.trustDirectory, 'metadata');
  const targetDirectory = join(options.trustDirectory, 'targets');
  mkdirSync(targetDirectory, { recursive: true, mode: 0o700 });
  const trust = initializeTrustedRoot({
    embeddedRootPath: options.embeddedRootPath,
    metadataDirectory,
  });

  const targetPath = join(targetDirectory, targetName);
  let targetBytes: Buffer | null = null;
  let refreshPromise: Promise<string> | null = null;
  const refresh = (): Promise<string> => {
    if (refreshPromise != null) {
      return refreshPromise;
    }

    refreshPromise = (async () => {
      const UpdaterClass = options.UpdaterClass ?? Updater as unknown as TufUpdaterConstructor;
      const updater = new UpdaterClass({
        metadataBaseUrl: `${repositoryUrl}/metadata`,
        targetBaseUrl: `${repositoryUrl}/targets`,
        metadataDir: metadataDirectory,
        targetDir: targetDirectory,
        config: { userAgent: 'Butter Paper desktop updater' },
        fetcher: new NoRedirectTufFetcher(repositoryUrl),
      });
      await updater.refresh();
      const targetInfo = await updater.getTargetInfo(targetName);
      if (targetInfo == null) {
        throw new Error(`The signed Butter Paper update repository has no ${targetName} target.`);
      }

      const temporaryTargetPath = `${targetPath}.${process.pid}.${Date.now()}.tmp`;
      try {
        await updater.downloadTarget(targetInfo, temporaryTargetPath);
        renameSync(temporaryTargetPath, targetPath);
      } finally {
        rmSync(temporaryTargetPath, { force: true });
      }
      targetBytes = readFileSync(targetPath);
      return targetPath;
    })().finally(() => {
      refreshPromise = null;
    });
    return refreshPromise;
  };
  await refresh();

  const requestPath = `/${encodeURIComponent(targetName)}`;
  const server = createServer((request, response) => {
    let pathname: string;
    try {
      pathname = new URL(request.url ?? '/', 'http://127.0.0.1').pathname;
    } catch {
      response.writeHead(400, { 'Cache-Control': 'no-store' }).end();
      return;
    }
    if ((request.method !== 'GET' && request.method !== 'HEAD') || pathname !== requestPath) {
      response.writeHead(404, { 'Cache-Control': 'no-store' }).end();
      return;
    }
    if (targetBytes == null) {
      response.writeHead(503, { 'Cache-Control': 'no-store' }).end();
      return;
    }

    response.writeHead(200, {
      'Cache-Control': 'no-store',
      'Content-Length': targetBytes.length,
      'Content-Type': 'application/yaml',
    });
    response.end(request.method === 'HEAD' ? undefined : targetBytes);
  });
  await listen(server);
  const address = server.address();
  if (address == null || typeof address === 'string') {
    await close(server);
    throw new Error('Butter Paper could not start its verified local update feed.');
  }

  let closePromise: Promise<void> | null = null;
  const closeFeed = (): Promise<void> => {
    if (closePromise == null) {
      closePromise = server.listening ? close(server) : Promise.resolve();
    }
    return closePromise;
  };

  return {
    close: closeFeed,
    feedUrl: `http://127.0.0.1:${address.port}`,
    refresh,
    targetPath,
    trustInitialized: trust.initialized,
    trustedRootPath: trust.trustedRootPath,
  };
}
