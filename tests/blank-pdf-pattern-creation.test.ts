import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { decodePDFRawStream, PDFArray, PDFDocument, PDFRawStream } from 'pdf-lib';
import { describe, expect, it } from 'vitest';
import { BlankPdfTemporaryStore } from '../apps/desktop/src/main/blankPdfTemporaryStore';
import {
  DEFAULT_BLANK_PDF_SETTINGS,
  resolveBlankPdfDimensions,
} from '../apps/desktop/src/renderer/src/components/blankPdfSettings';

describe('blank PDF pattern creation flow', () => {
  it.each(['grid', 'dots', 'lined', 'isometric', 'triangle'] as const)(
    'passes the selected %s settings through to the temporary PDF',
    async (patternType) => {
      const temporaryRoot = await mkdtemp(join(tmpdir(), 'butter-paper-pattern-flow-'));
      const store = new BlankPdfTemporaryStore(temporaryRoot);

      try {
        const request = resolveBlankPdfDimensions({
          ...DEFAULT_BLANK_PDF_SETTINGS,
          patternType,
          patternSpacingPreset: '10',
          patternColorPreset: 'black',
        });
        expect(request.pattern).toEqual({
          type: patternType,
          spacingMm: 10,
          color: '#000000',
        });

        const created = await store.create(request);
        const document = await PDFDocument.load(await readFile(created.filePath), { updateMetadata: false });
        expect(readPageContent(document)).not.toBe('');
      } finally {
        await store.cleanup();
        await rm(temporaryRoot, { recursive: true, force: true });
      }
    },
  );
});

function readPageContent(document: PDFDocument): string {
  const rawContents = document.getPage(0).node.Contents();
  const resolved = document.context.lookup(rawContents);
  const streams = resolved instanceof PDFRawStream
    ? [resolved]
    : resolved instanceof PDFArray
      ? resolved.asArray()
        .map((entry) => document.context.lookup(entry))
        .filter((entry): entry is PDFRawStream => entry instanceof PDFRawStream)
      : [];
  return streams
    .map((stream) => Buffer.from(decodePDFRawStream(stream).decode()).toString('latin1'))
    .join('\n');
}
