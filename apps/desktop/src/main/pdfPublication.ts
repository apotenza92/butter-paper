import { lstat, realpath, stat } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, resolve } from 'node:path';

export interface PdfPublicationDirectoryIdentity {
  readonly canonicalPath: string;
  readonly dev: bigint;
  readonly ino: bigint;
}

export interface PdfPublicationTarget {
  readonly targetPath: string;
  readonly directoryIdentity: PdfPublicationDirectoryIdentity;
}

/**
 * Captures the user-approved destination directory. Node has no portable
 * openat-style publication API, so callers retain this identity and revalidate
 * it immediately before and after their no-replace hard link.
 */
export async function capturePdfPublicationTarget(requestedPath: string): Promise<PdfPublicationTarget> {
  if (typeof requestedPath !== 'string' || !isAbsolute(requestedPath)) {
    throw new TypeError('PDF publication requires an absolute destination path.');
  }
  const normalizedPath = resolve(requestedPath);
  const requestedParent = dirname(normalizedPath);
  const parentInfo = await lstat(requestedParent);
  if (!parentInfo.isDirectory() || parentInfo.isSymbolicLink()) {
    throw new Error('The selected PDF destination directory is unsafe.');
  }
  const canonicalPath = await realpath(requestedParent);
  const canonicalInfo = await stat(canonicalPath, { bigint: true });
  if (!canonicalInfo.isDirectory()) {
    throw new Error('The selected PDF destination directory is unsafe.');
  }
  return {
    targetPath: join(canonicalPath, basename(normalizedPath)),
    directoryIdentity: {
      canonicalPath,
      dev: canonicalInfo.dev,
      ino: canonicalInfo.ino,
    },
  };
}

export async function assertPdfPublicationDirectory(
  identity: PdfPublicationDirectoryIdentity,
): Promise<void> {
  const pathInfo = await lstat(identity.canonicalPath);
  if (!pathInfo.isDirectory() || pathInfo.isSymbolicLink()) {
    throw new Error('The selected PDF destination directory changed before publication.');
  }
  const canonicalPath = await realpath(identity.canonicalPath);
  const current = await stat(canonicalPath, { bigint: true });
  if (canonicalPath !== identity.canonicalPath
    || !current.isDirectory()
    || current.dev !== identity.dev
    || current.ino !== identity.ino) {
    throw new Error('The selected PDF destination directory changed before publication.');
  }
}
