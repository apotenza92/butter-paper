import { describe, expect, it } from 'vitest';
import { PDFDocument } from 'pdf-lib';
import {
  createBlankPdf,
  millimetresToPdfPoints,
} from './blankPdf.js';

describe('blank PDF creation', () => {
  it.each([
    ['A0 portrait', 841, 1189],
    ['A1 portrait', 594, 841],
    ['A2 portrait', 420, 594],
    ['A3 portrait', 297, 420],
    ['A4 portrait', 210, 297],
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

  it.each([
    { widthMm: 9.99, heightMm: 297 },
    { widthMm: 210, heightMm: 5_001 },
    { widthMm: Number.NaN, heightMm: 297 },
    { widthMm: 210, heightMm: Number.POSITIVE_INFINITY },
  ])('rejects invalid dimensions %#', async (params) => {
    await expect(createBlankPdf(params)).rejects.toThrow(/must be between 10 and 5000 millimetres/);
  });
});
