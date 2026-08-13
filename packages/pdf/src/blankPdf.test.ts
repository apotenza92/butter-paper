import { describe, expect, it } from 'vitest';
import { decodePDFRawStream, PDFArray, PDFDocument, PDFRawStream } from 'pdf-lib';
import {
  createBlankPdf,
  millimetresToPdfPoints,
} from './blankPdf.js';

describe('blank PDF creation', () => {
  it.each([
    ['A5 portrait', 148, 210],
    ['A0 portrait', 841, 1189],
    ['A1 portrait', 594, 841],
    ['A2 portrait', 420, 594],
    ['A3 portrait', 297, 420],
    ['A4 portrait', 210, 297],
    ['A5 landscape', 210, 148],
    ['A0 landscape', 1189, 841],
    ['A1 landscape', 841, 594],
    ['A2 landscape', 594, 420],
    ['A3 landscape', 420, 297],
    ['A4 landscape', 297, 210],
    ['custom', 320, 450],
  ])('creates one correctly sized %s page', async (_name, widthMm, heightMm) => {
    const bytes = await createBlankPdf({ widthMm, heightMm });
    const document = await PDFDocument.load(bytes, { updateMetadata: false });

    expect(document.getPageCount()).toBe(1);
    expect(document.getPage(0).getWidth()).toBeCloseTo(millimetresToPdfPoints(widthMm), 5);
    expect(document.getPage(0).getHeight()).toBeCloseTo(millimetresToPdfPoints(heightMm), 5);
    expect(document.getTitle()).toBe('Untitled');
    expect(document.getCreator()).toBe('Butter Paper');
    expect(document.getProducer()).toBe('Butter Paper');
  });

  it.each(['grid', 'dots', 'lined', 'isometric', 'triangle'] as const)(
    'draws a vector %s pattern into the page content',
    async (type) => {
      const bytes = await createBlankPdf({
        widthMm: 40,
        heightMm: 30,
        pattern: { type, spacingMm: 10, color: '#4e95cc' },
      });
      const document = await PDFDocument.load(bytes, { updateMetadata: false });
      const content = readPageContent(document);

      expect(content.length).toBeGreaterThan(0);
      expect(content).toMatch(/\/Artifact\s+BMC/);
      expect(content).toMatch(/\bEMC\b/);
      expect(document.getSubject()).toMatch(/^butter-paper:page-grid:/);
      if (type === 'dots') {
        expect(content).toMatch(/\bc\b/);
      } else {
        expect(content).toMatch(/\bm\b/);
        expect(content).toMatch(/\bl\b/);
      }
    },
  );

  it('uses distinct line orientations for isometric and triangle paper', async () => {
    const content = async (type: 'isometric' | 'triangle') => {
      const document = await PDFDocument.load(await createBlankPdf({
        widthMm: 40,
        heightMm: 30,
        pattern: { type, spacingMm: 10, color: '#d1d5db' },
      }), { updateMetadata: false });
      return readPageContent(document);
    };

    expect(await content('isometric')).not.toBe(await content('triangle'));
  });

  it.each([
    { widthMm: 9.99, heightMm: 297 },
    { widthMm: 210, heightMm: 5_001 },
    { widthMm: Number.NaN, heightMm: 297 },
    { widthMm: 210, heightMm: Number.POSITIVE_INFINITY },
  ])('rejects invalid dimensions %#', async (params) => {
    await expect(createBlankPdf(params)).rejects.toThrow(/must be between 10 and 5000 millimetres/);
  });

  it.each([
    { type: 'grid' as const, spacingMm: 0.5, color: '#d1d5db' },
    { type: 'dots' as const, spacingMm: 10, color: 'grey' },
  ])('rejects an invalid pattern %#', async (pattern) => {
    await expect(createBlankPdf({ widthMm: 210, heightMm: 297, pattern })).rejects.toThrow(/pattern\.(spacingMm|color)/);
  });

  it('rejects patterns that would create excessive PDF content', async () => {
    await expect(createBlankPdf({
      widthMm: 5_000,
      heightMm: 5_000,
      pattern: { type: 'dots', spacingMm: 10, color: '#d1d5db' },
    })).rejects.toThrow(/exceed 50000 elements/);
  });
});

function readPageContent(document: PDFDocument): string {
  const page = document.getPage(0);
  const rawContents = page.node.Contents();
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
