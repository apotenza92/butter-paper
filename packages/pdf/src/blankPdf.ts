import { beginMarkedContent, endMarkedContent, PDFDocument, rgb, type PDFPage } from 'pdf-lib';

export const MIN_BLANK_PDF_DIMENSION_MM = 10;
export const MAX_BLANK_PDF_DIMENSION_MM = 5_000;
export const MIN_BLANK_PDF_PATTERN_SPACING_MM = 1;
export const MAX_BLANK_PDF_PATTERN_SPACING_MM = 500;
export const MAX_BLANK_PDF_PATTERN_ELEMENTS = 50_000;

export interface BlankPdfPattern {
  readonly type: 'grid' | 'dots' | 'lined' | 'isometric' | 'triangle';
  readonly spacingMm: number;
  readonly color: string;
}

export interface CreateBlankPdfParams {
  readonly widthMm: number;
  readonly heightMm: number;
  readonly pattern?: BlankPdfPattern;
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
  if (params.pattern) assertBlankPdfPattern(params, params.pattern);

  const document = await PDFDocument.create({ updateMetadata: false });
  document.setTitle('Untitled');
  document.setCreator('Butter Paper');
  document.setProducer('Butter Paper');
  if (params.pattern) document.setSubject(pageGridSubject(params, params.pattern));
  const page = document.addPage([
    millimetresToPdfPoints(params.widthMm),
    millimetresToPdfPoints(params.heightMm),
  ]);
  if (params.pattern) drawBlankPdfPattern(page, params, params.pattern);
  return document.save();
}

function pageGridSubject(params: CreateBlankPdfParams, pattern: BlankPdfPattern): string {
  const type = pattern.type === 'dots' || pattern.type === 'grid'
    ? 'rectangular'
    : pattern.type === 'lined' ? 'ruled' : pattern.type;
  return `butter-paper:page-grid:${JSON.stringify({
    version: 1,
    type,
    origin: { x: 0, y: 0 },
    spacing: millimetresToPdfPoints(pattern.spacingMm),
    width: millimetresToPdfPoints(params.widthMm),
    height: millimetresToPdfPoints(params.heightMm),
    rotationDegrees: 0,
    source: 'generated',
  })}`;
}

function assertBlankPdfPattern(params: CreateBlankPdfParams, pattern: BlankPdfPattern): void {
  if (!['grid', 'dots', 'lined', 'isometric', 'triangle'].includes(pattern.type)) {
    throw new RangeError('pattern.type must be grid, dots, lined, isometric, or triangle.');
  }
  if (
    !Number.isFinite(pattern.spacingMm)
    || pattern.spacingMm < MIN_BLANK_PDF_PATTERN_SPACING_MM
    || pattern.spacingMm > MAX_BLANK_PDF_PATTERN_SPACING_MM
  ) {
    throw new RangeError(
      `pattern.spacingMm must be between ${MIN_BLANK_PDF_PATTERN_SPACING_MM} and ${MAX_BLANK_PDF_PATTERN_SPACING_MM} millimetres.`,
    );
  }
  if (!/^#[0-9a-f]{6}$/i.test(pattern.color)) {
    throw new RangeError('pattern.color must be a six-digit hexadecimal colour.');
  }

  const columns = interiorIntervalCount(params.widthMm, pattern.spacingMm);
  const rows = interiorIntervalCount(params.heightMm, pattern.spacingMm);
  const elementCount = patternElementCount(params, pattern, columns, rows);
  if (elementCount > MAX_BLANK_PDF_PATTERN_ELEMENTS) {
    throw new RangeError(`The selected pattern would exceed ${MAX_BLANK_PDF_PATTERN_ELEMENTS} elements. Increase the spacing.`);
  }
}

function drawBlankPdfPattern(
  page: PDFPage,
  params: CreateBlankPdfParams,
  pattern: BlankPdfPattern,
): void {
  page.pushOperators(beginMarkedContent('Artifact'));
  drawBlankPdfPatternArtwork(page, params, pattern);
  page.pushOperators(endMarkedContent());
}

function drawBlankPdfPatternArtwork(
  page: PDFPage,
  params: CreateBlankPdfParams,
  pattern: BlankPdfPattern,
): void {
  const pageWidth = millimetresToPdfPoints(params.widthMm);
  const pageHeight = millimetresToPdfPoints(params.heightMm);
  const spacing = millimetresToPdfPoints(pattern.spacingMm);
  const color = hexadecimalColor(pattern.color);
  const columns = interiorIntervalCount(params.widthMm, pattern.spacingMm);
  const rows = interiorIntervalCount(params.heightMm, pattern.spacingMm);

  if (pattern.type === 'grid') {
    for (let column = 1; column <= columns; column += 1) {
      const x = column * spacing;
      page.drawLine({ start: { x, y: 0 }, end: { x, y: pageHeight }, thickness: 0.25, color });
    }
    for (let row = 1; row <= rows; row += 1) {
      const y = row * spacing;
      page.drawLine({ start: { x: 0, y }, end: { x: pageWidth, y }, thickness: 0.25, color });
    }
    return;
  }

  if (pattern.type === 'dots') {
    for (let column = 1; column <= columns; column += 1) {
      for (let row = 1; row <= rows; row += 1) {
        page.drawCircle({
          x: column * spacing,
          y: row * spacing,
          size: 0.75,
          color,
        });
      }
    }
    return;
  }

  if (pattern.type === 'lined') {
    for (let row = 1; row <= rows; row += 1) {
      const y = row * spacing;
      page.drawLine({ start: { x: 0, y }, end: { x: pageWidth, y }, thickness: 0.25, color });
    }
    return;
  }

  const angles = pattern.type === 'isometric'
    ? [Math.PI / 6, Math.PI / 2, 5 * Math.PI / 6]
    : [0, Math.PI / 3, 2 * Math.PI / 3];
  for (const angle of angles) {
    drawLineFamily(page, pageWidth, pageHeight, spacing, angle, color);
  }
}

function patternElementCount(
  params: CreateBlankPdfParams,
  pattern: BlankPdfPattern,
  columns: number,
  rows: number,
): number {
  if (pattern.type === 'grid') return columns + rows;
  if (pattern.type === 'dots') return columns * rows;
  if (pattern.type === 'lined') return rows;

  const angles = pattern.type === 'isometric'
    ? [Math.PI / 6, Math.PI / 2, 5 * Math.PI / 6]
    : [0, Math.PI / 3, 2 * Math.PI / 3];
  return angles.reduce(
    (count, angle) => count + lineFamilyIndexes(params.widthMm, params.heightMm, pattern.spacingMm, angle).length,
    0,
  );
}

function drawLineFamily(
  page: PDFPage,
  width: number,
  height: number,
  spacing: number,
  angle: number,
  color: ReturnType<typeof hexadecimalColor>,
): void {
  for (const index of lineFamilyIndexes(width, height, spacing, angle)) {
    const segment = lineSegmentForOffset(width, height, angle, index * spacing);
    if (segment) {
      page.drawLine({ start: segment[0], end: segment[1], thickness: 0.25, color });
    }
  }
}

function lineFamilyIndexes(width: number, height: number, spacing: number, angle: number): number[] {
  const normal = { x: -Math.sin(angle), y: Math.cos(angle) };
  const projections = [
    0,
    normal.x * width,
    normal.y * height,
    normal.x * width + normal.y * height,
  ];
  const first = Math.ceil(Math.min(...projections) / spacing);
  const last = Math.floor(Math.max(...projections) / spacing);
  return Array.from({ length: Math.max(0, last - first + 1) }, (_, offset) => first + offset);
}

function lineSegmentForOffset(
  width: number,
  height: number,
  angle: number,
  offset: number,
): [{ x: number; y: number }, { x: number; y: number }] | null {
  const direction = { x: Math.cos(angle), y: Math.sin(angle) };
  const normal = { x: -direction.y, y: direction.x };
  const point = { x: normal.x * offset, y: normal.y * offset };
  const candidates: Array<{ x: number; y: number; distance: number }> = [];
  const epsilon = 1e-7;

  const append = (x: number, y: number) => {
    if (x < -epsilon || x > width + epsilon || y < -epsilon || y > height + epsilon) return;
    const clamped = { x: Math.min(width, Math.max(0, x)), y: Math.min(height, Math.max(0, y)) };
    if (candidates.some((candidate) => Math.hypot(candidate.x - clamped.x, candidate.y - clamped.y) < epsilon)) return;
    candidates.push({ ...clamped, distance: clamped.x * direction.x + clamped.y * direction.y });
  };

  if (Math.abs(direction.x) > epsilon) {
    for (const x of [0, width]) {
      const distance = (x - point.x) / direction.x;
      append(x, point.y + distance * direction.y);
    }
  }
  if (Math.abs(direction.y) > epsilon) {
    for (const y of [0, height]) {
      const distance = (y - point.y) / direction.y;
      append(point.x + distance * direction.x, y);
    }
  }

  if (candidates.length < 2) return null;
  candidates.sort((a, b) => a.distance - b.distance);
  const first = candidates[0];
  const last = candidates[candidates.length - 1];
  return [{ x: first.x, y: first.y }, { x: last.x, y: last.y }];
}

function interiorIntervalCount(lengthMm: number, spacingMm: number): number {
  return Math.max(0, Math.ceil(lengthMm / spacingMm) - 1);
}

function hexadecimalColor(value: string) {
  return rgb(
    Number.parseInt(value.slice(1, 3), 16) / 255,
    Number.parseInt(value.slice(3, 5), 16) / 255,
    Number.parseInt(value.slice(5, 7), 16) / 255,
  );
}

function assertDimension(value: number, name: string): void {
  if (!Number.isFinite(value) || value < MIN_BLANK_PDF_DIMENSION_MM || value > MAX_BLANK_PDF_DIMENSION_MM) {
    throw new RangeError(`${name} must be between ${MIN_BLANK_PDF_DIMENSION_MM} and ${MAX_BLANK_PDF_DIMENSION_MM} millimetres.`);
  }
}
