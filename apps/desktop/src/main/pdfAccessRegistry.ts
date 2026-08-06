import { randomBytes } from 'node:crypto';
import { constants, type BigIntStats } from 'node:fs';
import { lstat, open, realpath, stat } from 'node:fs/promises';
import { basename, dirname, extname, isAbsolute, join, resolve } from 'node:path';
import {
  assertPdfPublicationDirectory,
  capturePdfPublicationTarget,
  type PdfPublicationDirectoryIdentity,
} from './pdfPublication';

const DOCUMENT_HANDLE_PATTERN = /^pdfdoc_[A-Za-z0-9_-]{32,128}$/;
const TARGET_HANDLE_PATTERN = /^pdftarget_[A-Za-z0-9_-]{32,128}$/;
const DEFAULT_MAX_DOCUMENTS = 128;
const DEFAULT_MAX_SOURCE_GRANTS = 128;
const DEFAULT_MAX_TARGET_GRANTS = 64;
const DEFAULT_MAX_SOURCE_BYTES = 512 * 1024 * 1024;

interface StableFileIdentity {
  readonly dev: bigint;
  readonly ino: bigint;
  readonly size: bigint;
  readonly mtimeMs: bigint;
  readonly ctimeMs: bigint;
}

interface SourceGrant {
  readonly ownerWebContentsId: number;
  readonly requestedPath: string;
  readonly canonicalPath: string;
  readonly identity: StableFileIdentity;
  readonly cleanupOnRelease: boolean;
}

interface DocumentEntry extends SourceGrant {
  readonly handle: string;
}

interface TargetEntry {
  readonly ownerWebContentsId: number;
  readonly handle: string;
  readonly targetPath: string;
  readonly canonicalParentPath: string;
  readonly directoryIdentity: PdfPublicationDirectoryIdentity;
}

export interface ResolvedPdfSaveTarget {
  readonly targetPath: string;
  readonly directoryIdentity: PdfPublicationDirectoryIdentity;
}

export interface PdfDocumentAccessDescriptor {
  readonly handle: string;
}

export interface PdfSaveTargetDescriptor {
  readonly targetHandle: string;
  readonly displayPath: string;
}

export interface ResolvedPdfDocumentAccess {
  readonly sourcePath: string;
  readonly cleanupOnRelease: boolean;
}

export type PdfAccessRegistryErrorCode =
  | 'INVALID_REQUEST'
  | 'UNAUTHORIZED_SOURCE'
  | 'UNSAFE_SOURCE'
  | 'STALE_DOCUMENT'
  | 'NOT_FOUND'
  | 'LIMIT_EXCEEDED'
  | 'UNSAFE_TARGET';

export class PdfAccessRegistryError extends Error {
  constructor(readonly code: PdfAccessRegistryErrorCode, message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'PdfAccessRegistryError';
  }
}

export interface PdfAccessRegistryOptions {
  readonly maxDocuments?: number;
  readonly maxSourceGrants?: number;
  readonly maxTargetGrants?: number;
  readonly maxSourceBytes?: number;
  readonly createDocumentHandle?: () => string;
  readonly createTargetHandle?: () => string;
  /** Test-only pause used to prove teardown during an awaited source operation. */
  readonly beforeSourceCommitForTesting?: () => Promise<void>;
}

/**
 * Main-process registry for renderer PDF authority. Display paths remain useful
 * UI metadata, but only an owner-scoped random handle can reach a loaded source
 * or an approved Save As destination after the initial main-owned path grant.
 */
export class PdfAccessRegistry {
  private readonly sourceGrants = new Map<string, SourceGrant>();
  private readonly documents = new Map<string, DocumentEntry>();
  private readonly targets = new Map<string, TargetEntry>();
  private readonly maxDocuments: number;
  private readonly maxSourceGrants: number;
  private readonly maxTargetGrants: number;
  private readonly createDocumentHandle: () => string;
  private readonly createTargetHandle: () => string;
  private readonly maxSourceBytes: number;
  private readonly activeOwners = new Set<number>();
  private readonly beforeSourceCommitForTesting?: () => Promise<void>;
  private operationQueue: Promise<void> = Promise.resolve();

  constructor(options: PdfAccessRegistryOptions = {}) {
    this.maxDocuments = positiveInteger(options.maxDocuments, DEFAULT_MAX_DOCUMENTS);
    this.maxSourceGrants = positiveInteger(options.maxSourceGrants, DEFAULT_MAX_SOURCE_GRANTS);
    this.maxTargetGrants = positiveInteger(options.maxTargetGrants, DEFAULT_MAX_TARGET_GRANTS);
    this.maxSourceBytes = positiveInteger(options.maxSourceBytes, DEFAULT_MAX_SOURCE_BYTES);
    this.createDocumentHandle = options.createDocumentHandle
      ?? (() => `pdfdoc_${randomBytes(32).toString('base64url')}`);
    this.createTargetHandle = options.createTargetHandle
      ?? (() => `pdftarget_${randomBytes(32).toString('base64url')}`);
    this.beforeSourceCommitForTesting = options.beforeSourceCommitForTesting;
  }

  registerOwner(ownerWebContentsId: number): void {
    assertOwner(ownerWebContentsId);
    this.activeOwners.add(ownerWebContentsId);
  }

  isOwnerActive(ownerWebContentsId: number): boolean {
    return this.activeOwners.has(ownerWebContentsId);
  }

  async authorizeSource(
    ownerWebContentsId: number,
    requestedPath: string,
    options: { readonly cleanupOnRelease?: boolean } = {},
  ): Promise<string> {
    return this.exclusive(async () => {
      this.assertOwnerActive(ownerWebContentsId);
      if (this.sourceGrants.size >= this.maxSourceGrants) {
        throw new PdfAccessRegistryError('LIMIT_EXCEEDED', 'Too many pending PDF open grants.');
      }
      const normalizedRequestedPath = normalizeAbsolutePath(requestedPath);
      const { canonicalPath, identity } = await snapshotRegularPdf(normalizedRequestedPath, this.maxSourceBytes);
      await this.beforeSourceCommitForTesting?.();
      this.assertOwnerActive(ownerWebContentsId);
      const key = sourceGrantKey(ownerWebContentsId, normalizedRequestedPath);
      this.sourceGrants.set(key, {
        ownerWebContentsId,
        requestedPath: normalizedRequestedPath,
        canonicalPath,
        identity,
        cleanupOnRelease: options.cleanupOnRelease === true,
      });
      return normalizedRequestedPath;
    });
  }

  async openAuthorizedSource(
    ownerWebContentsId: number,
    requestedPath: string,
  ): Promise<{ readonly descriptor: PdfDocumentAccessDescriptor; readonly sourcePath: string }> {
    return this.exclusive(async () => {
      this.assertOwnerActive(ownerWebContentsId);
      const normalizedRequestedPath = normalizeAbsolutePath(requestedPath);
      const key = sourceGrantKey(ownerWebContentsId, normalizedRequestedPath);
      const grant = this.sourceGrants.get(key);
      if (!grant) {
        throw new PdfAccessRegistryError('UNAUTHORIZED_SOURCE', 'The PDF was not selected by a trusted application source.');
      }
      if (this.documents.size >= this.maxDocuments) {
        throw new PdfAccessRegistryError('LIMIT_EXCEEDED', 'Too many PDF documents are open.');
      }
      const snapshot = await snapshotRegularPdf(normalizedRequestedPath, this.maxSourceBytes).catch((error) => {
        throw staleDocument(error);
      });
      if (snapshot.canonicalPath !== grant.canonicalPath || !sameIdentity(snapshot.identity, grant.identity)) {
        throw staleDocument();
      }
      this.assertOwnerActive(ownerWebContentsId);
      const handle = this.createDocumentHandle();
      if (!DOCUMENT_HANDLE_PATTERN.test(handle) || this.documents.has(handle)) {
        throw new PdfAccessRegistryError('INVALID_REQUEST', 'A unique PDF document handle could not be created.');
      }
      this.sourceGrants.delete(key);
      this.documents.set(handle, { ...grant, handle });
      return { descriptor: { handle }, sourcePath: grant.canonicalPath };
    });
  }

  async resolveDocument(ownerWebContentsId: number, handle: string): Promise<ResolvedPdfDocumentAccess> {
    return this.exclusive(async () => {
      this.assertOwnerActive(ownerWebContentsId);
      assertHandle(handle, DOCUMENT_HANDLE_PATTERN, 'PDF document');
      const entry = this.documents.get(handle);
      if (!entry || entry.ownerWebContentsId !== ownerWebContentsId) {
        throw new PdfAccessRegistryError('NOT_FOUND', 'The PDF document handle is unavailable.');
      }
      const snapshot = await snapshotRegularPdf(entry.canonicalPath, this.maxSourceBytes).catch((error) => {
        this.documents.delete(handle);
        throw staleDocument(error);
      });
      if (snapshot.canonicalPath !== entry.canonicalPath || !sameIdentity(snapshot.identity, entry.identity)) {
        this.documents.delete(handle);
        throw staleDocument();
      }
      this.assertOwnerActive(ownerWebContentsId);
      return { sourcePath: entry.canonicalPath, cleanupOnRelease: entry.cleanupOnRelease };
    });
  }

  async readDocumentBytes(ownerWebContentsId: number, handle: string): Promise<Uint8Array> {
    return this.exclusive(async () => {
      this.assertOwnerActive(ownerWebContentsId);
      assertHandle(handle, DOCUMENT_HANDLE_PATTERN, 'PDF document');
      const entry = this.documents.get(handle);
      if (!entry || entry.ownerWebContentsId !== ownerWebContentsId) {
        throw new PdfAccessRegistryError('NOT_FOUND', 'The PDF document handle is unavailable.');
      }
      let file: Awaited<ReturnType<typeof open>> | null = null;
      try {
        file = await open(entry.canonicalPath, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
        const before = identityFromStats(await file.stat({ bigint: true }));
        if (!sameIdentity(before, entry.identity) || before.size > BigInt(this.maxSourceBytes)) throw staleDocument();
        const bytes = await file.readFile();
        const after = identityFromStats(await file.stat({ bigint: true }));
        if (!sameIdentity(before, after) || !sameIdentity(after, entry.identity)) throw staleDocument();
        this.assertOwnerActive(ownerWebContentsId);
        return new Uint8Array(bytes);
      } catch (error) {
        this.documents.delete(handle);
        if (error instanceof PdfAccessRegistryError && error.code === 'STALE_DOCUMENT') throw error;
        throw staleDocument(error);
      } finally {
        await file?.close().catch(() => undefined);
      }
    });
  }

  releaseDocument(ownerWebContentsId: number, handle: string): string | null {
    this.assertOwnerActive(ownerWebContentsId);
    assertHandle(handle, DOCUMENT_HANDLE_PATTERN, 'PDF document');
    const entry = this.documents.get(handle);
    if (!entry || entry.ownerWebContentsId !== ownerWebContentsId) {
      throw new PdfAccessRegistryError('NOT_FOUND', 'The PDF document handle is unavailable.');
    }
    this.documents.delete(handle);
    return entry.cleanupOnRelease ? entry.canonicalPath : null;
  }

  async authorizeSaveTarget(ownerWebContentsId: number, requestedPath: string): Promise<PdfSaveTargetDescriptor> {
    return this.exclusive(async () => {
      this.assertOwnerActive(ownerWebContentsId);
      if (this.targets.size >= this.maxTargetGrants) {
        throw new PdfAccessRegistryError('LIMIT_EXCEEDED', 'Too many pending PDF save grants.');
      }
      const normalizedPath = normalizeAbsolutePath(requestedPath);
      if (extname(normalizedPath).toLowerCase() !== '.pdf' || basename(normalizedPath).length <= 4) {
        throw new PdfAccessRegistryError('UNSAFE_TARGET', 'The selected destination must be a PDF file.');
      }
      const publicationTarget = await capturePdfPublicationTarget(normalizedPath).catch((error) => {
        throw new PdfAccessRegistryError('UNSAFE_TARGET', 'The selected PDF destination is unavailable.', { cause: error });
      });
      const targetInfo = await lstat(normalizedPath).catch((error: unknown) => {
        if (isFileSystemError(error, 'ENOENT')) return null;
        throw error;
      });
      if (targetInfo?.isSymbolicLink() || targetInfo?.isDirectory()) {
        throw new PdfAccessRegistryError('UNSAFE_TARGET', 'The selected PDF destination is unsafe.');
      }
      this.assertOwnerActive(ownerWebContentsId);
      const targetPath = publicationTarget.targetPath;
      const handle = this.createTargetHandle();
      if (!TARGET_HANDLE_PATTERN.test(handle) || this.targets.has(handle)) {
        throw new PdfAccessRegistryError('INVALID_REQUEST', 'A unique PDF save handle could not be created.');
      }
      this.targets.set(handle, {
        ownerWebContentsId,
        handle,
        targetPath,
        canonicalParentPath: publicationTarget.directoryIdentity.canonicalPath,
        directoryIdentity: publicationTarget.directoryIdentity,
      });
      return { targetHandle: handle, displayPath: targetPath };
    });
  }

  async takeSaveTarget(ownerWebContentsId: number, handle: string): Promise<ResolvedPdfSaveTarget> {
    return this.exclusive(async () => {
      this.assertOwnerActive(ownerWebContentsId);
      assertHandle(handle, TARGET_HANDLE_PATTERN, 'PDF save target');
      const entry = this.targets.get(handle);
      if (!entry || entry.ownerWebContentsId !== ownerWebContentsId) {
        throw new PdfAccessRegistryError('NOT_FOUND', 'The PDF save target handle is unavailable.');
      }
      this.targets.delete(handle);
      const canonicalParentPath = await realpath(dirname(entry.targetPath)).catch((error) => {
        throw new PdfAccessRegistryError('UNSAFE_TARGET', 'The selected PDF destination is unavailable.', { cause: error });
      });
      if (canonicalParentPath !== entry.canonicalParentPath) {
        throw new PdfAccessRegistryError('UNSAFE_TARGET', 'The selected PDF destination changed before saving.');
      }
      await assertPdfPublicationDirectory(entry.directoryIdentity).catch((error) => {
        throw new PdfAccessRegistryError('UNSAFE_TARGET', 'The selected PDF destination changed before saving.', { cause: error });
      });
      const targetInfo = await lstat(entry.targetPath).catch((error: unknown) => {
        if (isFileSystemError(error, 'ENOENT')) return null;
        throw error;
      });
      if (targetInfo?.isSymbolicLink() || targetInfo?.isDirectory()) {
        throw new PdfAccessRegistryError('UNSAFE_TARGET', 'The selected PDF destination is unsafe.');
      }
      this.assertOwnerActive(ownerWebContentsId);
      return { targetPath: entry.targetPath, directoryIdentity: entry.directoryIdentity };
    });
  }

  clearOwner(ownerWebContentsId: number): readonly string[] {
    assertOwner(ownerWebContentsId);
    // Tombstone synchronously before deleting capabilities. Any async registry
    // operation that resumes after renderer teardown must fail its final owner
    // check instead of repopulating state for a dead webContents ID.
    this.activeOwners.delete(ownerWebContentsId);
    const cleanupPaths = new Set<string>();
    for (const [key, grant] of this.sourceGrants) {
      if (grant.ownerWebContentsId === ownerWebContentsId) {
        this.sourceGrants.delete(key);
        if (grant.cleanupOnRelease) cleanupPaths.add(grant.canonicalPath);
      }
    }
    for (const [handle, entry] of this.documents) {
      if (entry.ownerWebContentsId === ownerWebContentsId) {
        this.documents.delete(handle);
        if (entry.cleanupOnRelease) cleanupPaths.add(entry.canonicalPath);
      }
    }
    for (const [handle, entry] of this.targets) {
      if (entry.ownerWebContentsId === ownerWebContentsId) this.targets.delete(handle);
    }
    return [...cleanupPaths];
  }

  private assertOwnerActive(ownerWebContentsId: number): void {
    assertOwner(ownerWebContentsId);
    if (!this.activeOwners.has(ownerWebContentsId)) {
      throw new PdfAccessRegistryError('NOT_FOUND', 'The renderer owner is unavailable.');
    }
  }

  private async exclusive<T>(operation: () => Promise<T>): Promise<T> {
    const previous = this.operationQueue;
    let release!: () => void;
    this.operationQueue = new Promise<void>((resolveQueue) => { release = resolveQueue; });
    await previous;
    try {
      return await operation();
    } finally {
      release();
    }
  }
}

export const desktopPdfAccessRegistry = new PdfAccessRegistry();

async function snapshotRegularPdf(filePath: string, maxSourceBytes: number): Promise<{ canonicalPath: string; identity: StableFileIdentity }> {
  if (extname(filePath).toLowerCase() !== '.pdf') {
    throw new PdfAccessRegistryError('UNSAFE_SOURCE', 'The selected source is not a PDF file.');
  }
  const inputInfo = await lstat(filePath).catch((error) => {
    throw new PdfAccessRegistryError('UNSAFE_SOURCE', 'The selected PDF source is unavailable.', { cause: error });
  });
  if (!inputInfo.isFile() || inputInfo.isSymbolicLink()) {
    throw new PdfAccessRegistryError('UNSAFE_SOURCE', 'The selected PDF source is unsafe.');
  }
  const canonicalPath = await realpath(filePath);
  const info = await stat(canonicalPath, { bigint: true });
  if (!info.isFile()) throw new PdfAccessRegistryError('UNSAFE_SOURCE', 'The selected PDF source is unsafe.');
  if (info.size > BigInt(maxSourceBytes)) {
    throw new PdfAccessRegistryError('LIMIT_EXCEEDED', 'The selected PDF exceeds the supported size limit.');
  }
  return {
    canonicalPath,
    identity: {
      dev: info.dev,
      ino: info.ino,
      size: info.size,
      mtimeMs: info.mtimeMs,
      ctimeMs: info.ctimeMs,
    },
  };
}

function normalizeAbsolutePath(filePath: string): string {
  if (typeof filePath !== 'string' || filePath.length === 0 || !isAbsolute(filePath)) {
    throw new PdfAccessRegistryError('INVALID_REQUEST', 'An absolute PDF path is required.');
  }
  return resolve(filePath);
}

function sourceGrantKey(ownerWebContentsId: number, requestedPath: string): string {
  return `${ownerWebContentsId}\0${requestedPath}`;
}

function sameIdentity(left: StableFileIdentity, right: StableFileIdentity): boolean {
  return left.dev === right.dev
    && left.ino === right.ino
    && left.size === right.size
    && left.mtimeMs === right.mtimeMs
    && left.ctimeMs === right.ctimeMs;
}

function identityFromStats(info: BigIntStats): StableFileIdentity {
  return {
    dev: info.dev,
    ino: info.ino,
    size: info.size,
    mtimeMs: info.mtimeMs,
    ctimeMs: info.ctimeMs,
  };
}

function staleDocument(cause?: unknown): PdfAccessRegistryError {
  return new PdfAccessRegistryError('STALE_DOCUMENT', 'The PDF changed after access was granted.', { cause });
}

function assertOwner(ownerWebContentsId: number): void {
  if (!Number.isSafeInteger(ownerWebContentsId) || ownerWebContentsId < 1) {
    throw new PdfAccessRegistryError('INVALID_REQUEST', 'A valid renderer owner is required.');
  }
}

function assertHandle(handle: string, pattern: RegExp, label: string): void {
  if (typeof handle !== 'string' || !pattern.test(handle)) {
    throw new PdfAccessRegistryError('INVALID_REQUEST', `A valid ${label} handle is required.`);
  }
}

function positiveInteger(value: number | undefined, fallback: number): number {
  return Number.isSafeInteger(value) && (value ?? 0) > 0 ? value as number : fallback;
}

function isFileSystemError(error: unknown, code: string): error is NodeJS.ErrnoException {
  return error instanceof Error && 'code' in error && error.code === code;
}
