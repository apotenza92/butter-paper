import { readFile } from 'node:fs/promises';
import { pdfPoint, rect, type PdfPoint } from '@butter-paper/core';
import { decodePDFRawStream, PDFArray, PDFDocument, PDFName, PDFRawStream, type PDFPage } from 'pdf-lib';
import type { PdfContentPrimitive, PdfPageGeometryIndex, PdfPageGridDefinition } from './types.js';

type Matrix = readonly [number, number, number, number, number, number];

interface GraphicsState {
  readonly ctm: Matrix;
}

interface PathState {
  points: PdfPoint[];
  start: PdfPoint | null;
  current: PdfPoint | null;
}

type Token =
  | { readonly kind: 'number'; readonly value: number }
  | { readonly kind: 'name'; readonly value: string }
  | { readonly kind: 'operator'; readonly value: string };

export interface PdfGeometryDocument {
  readonly pageCount: number;
  getPageGeometryIndex(pageIndex: number): PdfPageGeometryIndex;
}

const identityMatrix: Matrix = [1, 0, 0, 1, 0, 0];
const pathPaintOperators = new Set(['S', 's', 'B', 'B*', 'b', 'b*']);
const pathResetOperators = new Set(['n', 'f', 'F', 'f*']);

export async function openPdfGeometryDocument(filePath: string): Promise<PdfGeometryDocument> {
  const pdfDoc = await PDFDocument.load(await readFile(filePath));
  return {
    pageCount: pdfDoc.getPageCount(),
    getPageGeometryIndex(pageIndex: number): PdfPageGeometryIndex {
      return extractPdfPageGeometryIndexFromDocument(pdfDoc, pageIndex);
    },
  };
}

export async function extractPdfPageGeometryIndex(filePath: string, pageIndex: number): Promise<PdfPageGeometryIndex> {
  const startedAt = performanceNow();
  const pdfDoc = await PDFDocument.load(await readFile(filePath));
  return extractPdfPageGeometryIndexFromDocument(pdfDoc, pageIndex, startedAt);
}

export function extractPdfPageGeometryIndexFromDocument(
  pdfDoc: PDFDocument,
  pageIndex: number,
  startedAt = performanceNow(),
): PdfPageGeometryIndex {
  if (pageIndex < 0 || pageIndex >= pdfDoc.getPageCount()) {
    throw new RangeError(`Page ${pageIndex + 1} is outside this document.`);
  }

  const page = pdfDoc.getPage(pageIndex);
  const primitives = extractPageContentPrimitives(pdfDoc, page);
  return {
    pageIndex,
    primitives,
    pageGrid: readPageGridDefinition(pdfDoc),
    buildMs: roundDuration(performanceNow() - startedAt),
  };
}

function readPageGridDefinition(pdfDoc: PDFDocument): PdfPageGridDefinition | undefined {
  const prefix = 'butter-paper:page-grid:';
  const subject = pdfDoc.getSubject();
  if (!subject?.startsWith(prefix)) return undefined;
  try {
    const value = JSON.parse(subject.slice(prefix.length)) as Partial<PdfPageGridDefinition> & { version?: number };
    if (value.version !== 1
      || !['rectangular', 'ruled', 'isometric', 'triangle'].includes(value.type ?? '')
      || !value.origin || !Number.isFinite(value.origin.x) || !Number.isFinite(value.origin.y)
      || !Number.isFinite(value.spacing) || value.spacing! <= 0
      || !Number.isFinite(value.width) || value.width! <= 0
      || !Number.isFinite(value.height) || value.height! <= 0
      || !Number.isFinite(value.rotationDegrees)
      || !['generated', 'detected', 'manual'].includes(value.source ?? '')) return undefined;
    return value as PdfPageGridDefinition;
  } catch {
    return undefined;
  }
}

export function extractPageContentPrimitives(pdfDoc: PDFDocument, page: PDFPage): readonly PdfContentPrimitive[] {
  const primitives: PdfContentPrimitive[] = [];
  const streams = getPageContentStreams(pdfDoc, page);
  for (const stream of streams) {
    primitives.push(...parsePdfContentStream(Buffer.from(decodePDFRawStream(stream).decode()).toString('latin1')));
  }
  return primitives;
}

function getPageContentStreams(pdfDoc: PDFDocument, page: PDFPage): readonly PDFRawStream[] {
  const node = page.node as unknown as {
    Contents?: () => unknown;
    get?: (key: PDFName) => unknown;
  };
  const rawContents = node.Contents?.() ?? node.get?.(PDFName.of('Contents'));
  if (!rawContents) {
    return [];
  }

  const context = pdfDoc.context as unknown as { lookup(value: unknown): unknown };
  const resolved = context.lookup(rawContents);
  if (resolved instanceof PDFRawStream) {
    return [resolved];
  }

  if (!(resolved instanceof PDFArray)) {
    return [];
  }

  return resolved.asArray()
    .map((entry: unknown) => context.lookup(entry))
    .filter((entry): entry is PDFRawStream => entry instanceof PDFRawStream);
}

function parsePdfContentStream(content: string): readonly PdfContentPrimitive[] {
  const primitives: PdfContentPrimitive[] = [];
  const operandStack: number[] = [];
  const graphicsStack: GraphicsState[] = [];
  let graphicsState: GraphicsState = { ctm: identityMatrix };
  let path: PathState = createEmptyPath();
  const markedContentArtifactStack: boolean[] = [];
  const nameOperands: string[] = [];

  for (const token of tokenizePdfContent(content)) {
    if (token.kind === 'number') {
      operandStack.push(token.value);
      continue;
    }
    if (token.kind === 'name') {
      nameOperands.push(token.value);
      continue;
    }

    const op = token.value;
    const insideArtifact = markedContentArtifactStack.at(-1) ?? false;
    if (op === 'BMC' || op === 'BDC') {
      markedContentArtifactStack.push(insideArtifact || nameOperands.includes('Artifact'));
    } else if (op === 'EMC') {
      markedContentArtifactStack.pop();
    } else if (op === 'q') {
      graphicsStack.push(graphicsState);
    } else if (op === 'Q') {
      graphicsState = graphicsStack.pop() ?? { ctm: identityMatrix };
    } else if (op === 'cm' && operandStack.length >= 6) {
      const args = takeOperands(operandStack, 6);
      graphicsState = { ctm: multiplyMatrix(graphicsState.ctm, args as Matrix) };
    } else if (op === 'm' && operandStack.length >= 2) {
      const [x, y] = takeOperands(operandStack, 2);
      const point = transformPoint(graphicsState.ctm, x, y);
      path = { points: [point], start: point, current: point };
    } else if (op === 'l' && operandStack.length >= 2) {
      const [x, y] = takeOperands(operandStack, 2);
      const point = transformPoint(graphicsState.ctm, x, y);
      if (path.current && !insideArtifact) {
        primitives.push({ kind: 'line', start: path.current, end: point });
      }
      path.points.push(point);
      path.current = point;
    } else if ((op === 'c' || op === 'v' || op === 'y') && operandStack.length >= 2) {
      const [x, y] = takeOperands(operandStack, 2);
      const point = transformPoint(graphicsState.ctm, x, y);
      if (path.current && !insideArtifact) {
        primitives.push({ kind: 'line', start: path.current, end: point });
      }
      path.points.push(point);
      path.current = point;
    } else if (op === 'h') {
      if (!insideArtifact && path.current && path.start && !samePoint(path.current, path.start)) {
        primitives.push({ kind: 'line', start: path.current, end: path.start });
      }
      if (!insideArtifact && path.points.length >= 3) {
        primitives.push(rectPrimitiveFromClosedPath(path.points) ?? { kind: 'polyline', points: [...path.points], closed: true });
      }
      path.current = path.start;
    } else if (op === 're' && operandStack.length >= 4) {
      const [x, y, width, height] = takeOperands(operandStack, 4);
      const corners = [
        transformPoint(graphicsState.ctm, x, y),
        transformPoint(graphicsState.ctm, x + width, y),
        transformPoint(graphicsState.ctm, x + width, y + height),
        transformPoint(graphicsState.ctm, x, y + height),
      ];
      if (!insideArtifact) primitives.push(rectPrimitiveFromCorners(corners));
      path = { points: corners, start: corners[0], current: corners[3] };
    } else if (pathPaintOperators.has(op) || pathResetOperators.has(op)) {
      path = createEmptyPath();
    }

    operandStack.length = 0;
    nameOperands.length = 0;
  }

  return primitives;
}

function* tokenizePdfContent(input: string): Generator<Token> {
  for (let index = 0; index < input.length;) {
    const char = input[index];
    if (isWhitespace(char)) {
      index += 1;
      continue;
    }
    if (char === '%') {
      while (index < input.length && input[index] !== '\n' && input[index] !== '\r') index += 1;
      continue;
    }
    if (char === '(') {
      index = skipPdfString(input, index + 1);
      continue;
    }
    if (char === '<' && input[index + 1] !== '<') {
      index += 1;
      while (index < input.length && input[index] !== '>') index += 1;
      index += 1;
      continue;
    }
    if (isDelimiter(char)) {
      index += 1;
      continue;
    }

    const end = scanTokenEnd(input, index);
    const raw = input.slice(index, end);
    index = end;
    if (!raw) {
      continue;
    }
    if (raw[0] === '/') {
      yield { kind: 'name', value: raw.slice(1) };
      continue;
    }

    const value = Number(raw);
    yield Number.isFinite(value) && /^[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?$/.test(raw)
      ? { kind: 'number', value }
      : { kind: 'operator', value: raw };
  }
}

function skipPdfString(input: string, index: number): number {
  let depth = 1;
  while (index < input.length && depth > 0) {
    const char = input[index];
    if (char === '\\') {
      index += 2;
      continue;
    }
    if (char === '(') {
      depth += 1;
    } else if (char === ')') {
      depth -= 1;
    }
    index += 1;
  }
  return index;
}

function scanTokenEnd(input: string, index: number): number {
  while (index < input.length && !isWhitespace(input[index]) && !isDelimiter(input[index])) {
    index += 1;
  }
  return index;
}

function isWhitespace(char: string): boolean {
  return char === ' ' || char === '\n' || char === '\r' || char === '\t' || char === '\f' || char === '\0';
}

function isDelimiter(char: string): boolean {
  return char === '[' || char === ']' || char === '<' || char === '>' || char === '(' || char === ')' || char === '{' || char === '}';
}

function takeOperands(stack: number[], count: number): readonly number[] {
  return stack.slice(Math.max(0, stack.length - count));
}

function createEmptyPath(): PathState {
  return { points: [], start: null, current: null };
}

function transformPoint(matrix: Matrix, x: number, y: number): PdfPoint {
  const [a, b, c, d, e, f] = matrix;
  return pdfPoint(a * x + c * y + e, b * x + d * y + f);
}

function multiplyMatrix(left: Matrix, right: Matrix): Matrix {
  const [a1, b1, c1, d1, e1, f1] = left;
  const [a2, b2, c2, d2, e2, f2] = right;
  return [
    a1 * a2 + c1 * b2,
    b1 * a2 + d1 * b2,
    a1 * c2 + c1 * d2,
    b1 * c2 + d1 * d2,
    a1 * e2 + c1 * f2 + e1,
    b1 * e2 + d1 * f2 + f1,
  ];
}

function rectPrimitiveFromCorners(corners: readonly PdfPoint[]): PdfContentPrimitive {
  const xs = corners.map((point) => point.x);
  const ys = corners.map((point) => point.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  return {
    kind: 'rect',
    rect: rect(minX, minY, maxX - minX, maxY - minY),
  };
}

function rectPrimitiveFromClosedPath(points: readonly PdfPoint[]): PdfContentPrimitive | null {
  if (points.length !== 4) {
    return null;
  }

  const xs = [...new Set(points.map((point) => roundCoordinate(point.x)))];
  const ys = [...new Set(points.map((point) => roundCoordinate(point.y)))];
  if (xs.length !== 2 || ys.length !== 2) {
    return null;
  }

  const [minX, maxX] = xs.sort((left, right) => left - right);
  const [minY, maxY] = ys.sort((left, right) => left - right);
  return {
    kind: 'rect',
    rect: rect(minX, minY, maxX - minX, maxY - minY),
  };
}

function roundCoordinate(value: number): number {
  return Math.round(value * 10000) / 10000;
}

function samePoint(a: PdfPoint, b: PdfPoint): boolean {
  return Math.abs(a.x - b.x) < 0.0001 && Math.abs(a.y - b.y) < 0.0001;
}

function performanceNow(): number {
  return globalThis.performance?.now() ?? Date.now();
}

function roundDuration(value: number): number {
  return Math.round(value * 1000) / 1000;
}
