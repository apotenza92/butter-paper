import { link, lstat, mkdtemp, open, readFile, rm, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { constants } from 'node:fs';
import { basename, dirname, isAbsolute, join, resolve } from 'node:path';
import {
  createDocument,
  size,
  type Markup,
  type PageModel,
  type PageScale,
} from '@butter-paper/core';
import { openPdfDocument, openPdfGeometryDocument, type PdfGeometryDocument, type PdfPageGeometryIndex, type PdfPageRotation, type PdfSaveMode, type PdfSaveResult } from '@butter-paper/pdf';
import type { DocumentOpenStageTimings, LoadedDocumentPayload } from '../shared/protocol';
import {
  assertPdfPublicationDirectory,
  capturePdfPublicationTarget,
  type PdfPublicationDirectoryIdentity,
} from './pdfPublication';

interface CachedGeometryDocument {
  readonly signature: string;
  readonly document: Promise<PdfGeometryDocument>;
  readonly pages: Map<number, Promise<PdfPageGeometryIndex>>;
  lastUsedAt: number;
}

interface StableFileIdentity {
  readonly dev: bigint;
  readonly ino: bigint;
  readonly size: bigint;
  readonly mtimeMs: bigint;
  readonly ctimeMs: bigint;
}

const maxGeometryDocumentCacheEntries = 4;
const geometryDocumentCache = new Map<string, CachedGeometryDocument>();
export async function loadDocumentPayload(
  filePath: string,
): Promise<Omit<LoadedDocumentPayload, 'documentAccess'>> {
  const loadStartedAt = performance.now();
  const sourceBytes = new Uint8Array(await readFile(filePath));
  const handle = await openPdfDocument(filePath, { sourceBytes });

  try {
    const metadataStartedAt = performance.now();
    const metadata = await handle.getMetadata();
    const mainMetadataMs = performance.now() - metadataStartedAt;

    const pageModelStartedAt = performance.now();
    const pages: PageModel[] = [];
    for (let pageIndex = 0; pageIndex < metadata.pageCount; pageIndex += 1) {
      const pageInfo = await handle.getPageInfo(pageIndex);
      pages.push({
        id: `${filePath}#page-${pageIndex + 1}`,
        index: pageIndex,
        viewBox: pageInfo.viewBox,
        userUnit: pageInfo.userUnit,
        size: size(pageInfo.width, pageInfo.height),
        rotation: pageInfo.rotation,
      });
    }
    const mainPageModelMs = performance.now() - pageModelStartedAt;

    const annotationStartedAt = performance.now();
    const annotationsByPage = await handle.annotations.readAllPageAnnotations();
    const markups: Markup[] = annotationsByPage.flat();
    const mainAnnotationReadMs = performance.now() - annotationStartedAt;

    const openStageTimings: DocumentOpenStageTimings = {
      mainPayloadMs: roundDuration(performance.now() - loadStartedAt),
      mainMetadataMs: roundDuration(mainMetadataMs),
      mainPageModelMs: roundDuration(mainPageModelMs),
      mainAnnotationReadMs: roundDuration(mainAnnotationReadMs),
    };

    return {
      filePath,
      fileName: basename(filePath),
      document: createDocument({
        id: filePath,
        path: filePath,
        metadata: {
          title: metadata.title,
          author: metadata.author,
          subject: metadata.subject,
          creator: metadata.creator,
          producer: metadata.producer,
        },
        pages,
        markups,
      }),
      openStageTimings,
    };
  } finally {
    await handle.close();
  }
}

function roundDuration(value: number): number {
  return Math.round(value * 1000) / 1000;
}

export async function loadPageGeometryIndex(filePath: string, pageIndex: number): Promise<PdfPageGeometryIndex> {
  const cacheEntry = await getGeometryDocumentCacheEntry(filePath);
  const cachedPage = cacheEntry.pages.get(pageIndex);
  if (cachedPage) {
    return cachedPage;
  }

  const pagePromise = cacheEntry.document.then((document) => document.getPageGeometryIndex(pageIndex)).catch((error) => {
    cacheEntry.pages.delete(pageIndex);
    throw error;
  });
  cacheEntry.pages.set(pageIndex, pagePromise);
  return pagePromise;
}

export async function saveDocumentPayload(
  sourcePath: string,
  markups: readonly Markup[],
  mode: PdfSaveMode,
  targetPath?: string,
  pageScales?: readonly PageScale[],
  pageRotations?: readonly PdfPageRotation[],
  expectedPublicationDirectory?: PdfPublicationDirectoryIdentity,
): Promise<PdfSaveResult> {
  if (!isAbsolute(sourcePath) || (targetPath !== undefined && !isAbsolute(targetPath))) {
    throw new TypeError('PDF save paths must be absolute main-process paths.');
  }
  if (mode === 'save') {
    throw new PdfOutputPublicationError(
      'In-place full-rewrite saving is disabled because it cannot conditionally replace the exact validated source. Use Save As to create a new PDF while preserving the original.',
    );
  }
  const source = resolve(sourcePath);
  const requestedDestination = resolve(targetPath ?? '');
  if (!targetPath) throw new TypeError('Save As requires an absolute destination path.');
  if (requestedDestination === source) {
    throw new PdfOutputPublicationError('Save As will not replace the source PDF. Choose a new destination so the original remains preserved.');
  }
  const capturedPublicationTarget = await capturePdfPublicationTarget(requestedDestination);
  const destination = capturedPublicationTarget.targetPath;
  const publicationDirectory = expectedPublicationDirectory ?? capturedPublicationTarget.directoryIdentity;
  if (publicationDirectory.canonicalPath !== dirname(destination)) {
    throw new PdfOutputPublicationError('The selected Save As destination changed before saving.');
  }
  await assertPdfPublicationDirectory(publicationDirectory);

  const sourceBytes = new Uint8Array(await readRegularFileWithoutFollowingSymlinks(source));
  const sourceSha256 = createHash('sha256').update(sourceBytes).digest('hex');

  const confirmedBytes = new Uint8Array(await readRegularFileWithoutFollowingSymlinks(source));
  const confirmedSha256 = createHash('sha256').update(confirmedBytes).digest('hex');
  if (confirmedSha256 !== sourceSha256) {
    throw new PdfDocumentSourceError();
  }

  const temporaryDirectory = await mkdtemp(join(dirname(destination), '.butter-paper-save-'));
  const validatedSourceSnapshot = join(temporaryDirectory, 'validated-source.pdf');
  const temporaryOutput = join(temporaryDirectory, 'validated-output.pdf');
  try {
    await writeExclusiveSyncedFile(validatedSourceSnapshot, confirmedBytes);
    const snapshotBytes = new Uint8Array(
      await readRegularFileWithoutFollowingSymlinks(validatedSourceSnapshot),
    );
    if (createHash('sha256').update(snapshotBytes).digest('hex') !== sourceSha256) {
      throw new PdfDocumentSourceError();
    }

    const handle = await openPdfDocument(validatedSourceSnapshot);
    try {
      await handle.writer.save(handle, markups, 'saveAs', temporaryOutput, pageScales, pageRotations);
    } finally {
      await handle.close();
    }

    await syncFile(temporaryOutput);
    const outputBytes = new Uint8Array(await readFile(temporaryOutput));
    const outputSha256 = createHash('sha256').update(outputBytes).digest('hex');
    const confirmedOutputBytes = new Uint8Array(
      await readRegularFileWithoutFollowingSymlinks(temporaryOutput),
    );
    if (createHash('sha256').update(confirmedOutputBytes).digest('hex') !== outputSha256) {
      throw new PdfDocumentSourceError();
    }

    const sourceBytesBeforePublish = new Uint8Array(await readRegularFileWithoutFollowingSymlinks(source));
    if (createHash('sha256').update(sourceBytesBeforePublish).digest('hex') !== sourceSha256) {
      throw new PdfDocumentSourceError();
    }

    try {
      await assertPdfPublicationDirectory(publicationDirectory);
      await link(temporaryOutput, destination);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'EEXIST') {
        throw new PdfOutputPublicationError('Save As will not replace an existing destination.');
      }
      throw error;
    }
    try {
      await assertPdfPublicationDirectory(publicationDirectory);
    } catch {
      await removeLinkedTargetIfOurs(destination, temporaryOutput).catch(() => undefined);
      throw new PdfOutputPublicationError('The selected Save As destination changed during publication.');
    }
    await rm(temporaryOutput);
    await syncDirectory(dirname(destination));
    return { path: requestedDestination, bytesWritten: confirmedOutputBytes.byteLength };
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true }).catch(() => undefined);
  }
}

async function removeLinkedTargetIfOurs(targetPath: string, sourcePath: string): Promise<void> {
  const [target, source] = await Promise.all([
    lstat(targetPath, { bigint: true }),
    lstat(sourcePath, { bigint: true }),
  ]);
  if (target.dev === source.dev && target.ino === source.ino) await rm(targetPath);
}

export class PdfOutputPublicationError extends Error {
  readonly code = 'PDF_OUTPUT_PUBLICATION_DENIED' as const;

  constructor(message: string) {
    super(message);
    this.name = 'PdfOutputPublicationError';
  }
}

export class PdfDocumentSourceError extends Error {
  readonly code = 'PDF_SOURCE_INVALID' as const;

  constructor() {
    super('PDF saving requires a regular, non-symlink source file owned by the main process.');
    this.name = 'PdfDocumentSourceError';
  }
}

async function syncFile(filePath: string): Promise<void> {
  const file = await open(filePath, 'r+');
  try {
    await file.sync();
  } finally {
    await file.close();
  }
}

async function writeExclusiveSyncedFile(filePath: string, bytes: Uint8Array): Promise<void> {
  const file = await open(filePath, 'wx', 0o600);
  try {
    let offset = 0;
    while (offset < bytes.byteLength) {
      const { bytesWritten } = await file.write(bytes, offset, bytes.byteLength - offset);
      if (bytesWritten < 1) throw new Error('Butter Paper could not write the complete validated source snapshot.');
      offset += bytesWritten;
    }
    await file.sync();
  } finally {
    await file.close();
  }
}

async function syncDirectory(directoryPath: string): Promise<void> {
  if (process.platform === 'win32') return;
  const directory = await open(directoryPath, 'r');
  try {
    await directory.sync();
  } finally {
    await directory.close();
  }
}

async function readRegularFileWithoutFollowingSymlinks(filePath: string): Promise<Buffer> {
  const pathBefore = await statPathWithoutSymlinks(filePath);
  let file;
  try {
    file = await open(filePath, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ELOOP') {
      throw new PdfDocumentSourceError();
    }
    throw error;
  }
  try {
    const openedBefore = await file.stat({ bigint: true });
    if (!openedBefore.isFile() || pathBefore.dev !== openedBefore.dev || pathBefore.ino !== openedBefore.ino) {
      throw new PdfDocumentSourceError();
    }
    const bytes = await file.readFile();
    const openedAfter = await file.stat({ bigint: true });
    const pathAfter = await statPathWithoutSymlinks(filePath);
    if (!sameStableFile(openedBefore, openedAfter)
      || pathAfter.dev !== openedAfter.dev
      || pathAfter.ino !== openedAfter.ino) {
      throw new PdfDocumentSourceError();
    }
    return bytes;
  } finally {
    await file.close();
  }
}

async function statPathWithoutSymlinks(filePath: string) {
  const info = await lstat(filePath, { bigint: true });
  if (info.isSymbolicLink() || !info.isFile()) {
    throw new PdfDocumentSourceError();
  }
  return info;
}

function sameStableFile(
  before: StableFileIdentity,
  after: StableFileIdentity,
): boolean {
  return before.dev === after.dev
    && before.ino === after.ino
    && before.size === after.size
    && before.mtimeMs === after.mtimeMs
    && before.ctimeMs === after.ctimeMs;
}

async function getGeometryDocumentCacheEntry(filePath: string): Promise<CachedGeometryDocument> {
  const signature = await geometryFileSignature(filePath);
  const cached = geometryDocumentCache.get(filePath);
  if (cached?.signature === signature) {
    cached.lastUsedAt = performance.now();
    return cached;
  }

  const entry: CachedGeometryDocument = {
    signature,
    document: openPdfGeometryDocument(filePath).catch((error) => {
      geometryDocumentCache.delete(filePath);
      throw error;
    }),
    pages: new Map(),
    lastUsedAt: performance.now(),
  };
  geometryDocumentCache.set(filePath, entry);
  pruneGeometryDocumentCache();
  return entry;
}

async function geometryFileSignature(filePath: string): Promise<string> {
  const info = await stat(filePath);
  return `${info.size}:${info.mtimeMs}`;
}

function pruneGeometryDocumentCache(): void {
  if (geometryDocumentCache.size <= maxGeometryDocumentCacheEntries) {
    return;
  }

  const entries = [...geometryDocumentCache.entries()].sort(([, left], [, right]) => left.lastUsedAt - right.lastUsedAt);
  for (const [filePath] of entries.slice(0, geometryDocumentCache.size - maxGeometryDocumentCacheEntries)) {
    geometryDocumentCache.delete(filePath);
  }
}
