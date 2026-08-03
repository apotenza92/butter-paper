import { PDFDocument } from 'pdf-lib';

export const MIN_BLANK_PDF_DIMENSION_MM = 10;
export const MAX_BLANK_PDF_DIMENSION_MM = 5_000;

export interface CreateBlankPdfParams {
  readonly widthMm: number;
  readonly heightMm: number;
}

export function millimetresToPdfPoints(millimetres: number): number {
  return millimetres * 72 / 25.4;
}

export function assertBlankPdfDimensions(params: CreateBlankPdfParams): void {
  assertDimension(params.widthMm, 'widthMm');
  assertDimension(params.heightMm, 'heightMm');
}

export async function createBlankPdf(params: CreateBlankPdfParams): Promise<Uint8Array> {
  assertBlankPdfDimensions(params);

  const document = await PDFDocument.create({ updateMetadata: false });
  document.setTitle('Untitled');
  document.setCreator('Butter Paper');
  document.setProducer('Butter Paper');
  document.addPage([
    millimetresToPdfPoints(params.widthMm),
    millimetresToPdfPoints(params.heightMm),
  ]);
  return document.save();
}

function assertDimension(value: number, name: string): void {
  if (!Number.isFinite(value) || value < MIN_BLANK_PDF_DIMENSION_MM || value > MAX_BLANK_PDF_DIMENSION_MM) {
    throw new RangeError(`${name} must be between ${MIN_BLANK_PDF_DIMENSION_MM} and ${MAX_BLANK_PDF_DIMENSION_MM} millimetres.`);
  }
}
