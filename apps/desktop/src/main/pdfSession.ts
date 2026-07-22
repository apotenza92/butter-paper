import { stat } from 'node:fs/promises';
import { basename } from 'node:path';
import { createDocument, size, type Markup, type PageModel, type PageScale } from '@butter-paper/core';
import { openPdfDocument, openPdfGeometryDocument, type PdfGeometryDocument, type PdfPageGeometryIndex, type PdfSaveMode, type PdfSaveResult } from '@butter-paper/pdf';
import type { DocumentOpenStageTimings, LoadedDocumentPayload } from '../shared/protocol';

interface CachedGeometryDocument {
  readonly signature: string;
  readonly document: Promise<PdfGeometryDocument>;
  readonly pages: Map<number, Promise<PdfPageGeometryIndex>>;
  lastUsedAt: number;
}

const maxGeometryDocumentCacheEntries = 4;
const geometryDocumentCache = new Map<string, CachedGeometryDocument>();

export async function loadDocumentPayload(filePath: string): Promise<LoadedDocumentPayload> {
  const loadStartedAt = performance.now();
  const handle = await openPdfDocument(filePath);

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
): Promise<PdfSaveResult> {
  const handle = await openPdfDocument(sourcePath);

  try {
    return await handle.writer.save(handle, markups, mode, targetPath, pageScales);
  } finally {
    await handle.close();
  }
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
