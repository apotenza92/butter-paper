import type { DocumentModel, PageScale, ScalePrecision, ScalePreset, ScaleUnit } from './document.js';
import type { PdfPoint } from './points.js';

export interface PageScaleRange {
  readonly startPageIndex: number;
  readonly endPageIndex: number;
}

export type PageScaleApplyTarget =
  | { readonly kind: 'current'; readonly pageIndex: number }
  | { readonly kind: 'all' }
  | { readonly kind: 'ranges'; readonly ranges: readonly PageScaleRange[] };

export const BUILT_IN_SCALE_PRESETS: readonly ScalePreset[] = [
  createMetricRatioScalePreset(1),
  createMetricRatioScalePreset(2),
  createMetricRatioScalePreset(5),
  createMetricRatioScalePreset(10),
  createMetricRatioScalePreset(20),
  createMetricRatioScalePreset(50),
  createMetricRatioScalePreset(100),
  createMetricRatioScalePreset(200),
  createMetricRatioScalePreset(500),
  createMetricRatioScalePreset(1000),
];

export const DEFAULT_SCALE_PRECISION: ScalePrecision = {
  mode: 'decimal',
  value: 0.001,
};

export function createPresetPageScale(pageIndex: number, preset: ScalePreset, precision: ScalePrecision = DEFAULT_SCALE_PRECISION): PageScale {
  return {
    pageIndex,
    source: preset.source,
    name: preset.name,
    pdfUnits: preset.pdfUnits,
    realUnits: preset.realUnits,
    scaleX: preset.scaleX,
    scaleY: preset.scaleY,
    precision,
  };
}

export function createCustomPageScale(params: {
  readonly pageIndex: number;
  readonly name: string;
  readonly pdfUnits: ScaleUnit;
  readonly realUnits: ScaleUnit;
  readonly scaleX?: number;
  readonly scaleY?: number;
  readonly pdfLength?: number;
  readonly realLength?: number;
  readonly yPdfLength?: number;
  readonly yRealLength?: number;
  readonly precision?: ScalePrecision;
}): PageScale {
  const scaleX = params.scaleX ?? ratioToRealUnitsPerPdfPoint(params.pdfLength ?? 1, params.pdfUnits, params.realLength ?? 1);
  const scaleY = params.scaleY ?? (
    params.yPdfLength && params.yRealLength
      ? ratioToRealUnitsPerPdfPoint(params.yPdfLength, params.pdfUnits, params.yRealLength)
      : scaleX
  );
  return {
    pageIndex: params.pageIndex,
    source: 'custom',
    name: params.name,
    pdfUnits: params.pdfUnits,
    realUnits: params.realUnits,
    scaleX,
    scaleY,
    precision: params.precision ?? DEFAULT_SCALE_PRECISION,
  };
}

export function calibratePageScale(params: {
  readonly pageIndex: number;
  readonly start: PdfPoint;
  readonly end: PdfPoint;
  readonly realLength: number;
  readonly realUnits: ScaleUnit;
  readonly pdfUnits?: ScaleUnit;
  readonly name?: string;
  readonly precision?: ScalePrecision;
}): PageScale {
  const pdfDistance = Math.hypot(params.end.x - params.start.x, params.end.y - params.start.y);
  if (pdfDistance <= 0 || params.realLength <= 0) {
    throw new Error('Calibration requires positive PDF and real-world distances.');
  }
  const scale = params.realLength / pdfDistance;
  return {
    pageIndex: params.pageIndex,
    source: 'calibrated',
    name: params.name ?? `Calibrated ${formatNumber(params.realLength)} ${params.realUnits}`,
    pdfUnits: params.pdfUnits ?? 'in',
    realUnits: params.realUnits,
    scaleX: scale,
    scaleY: scale,
    precision: params.precision ?? DEFAULT_SCALE_PRECISION,
  };
}

export function applyPageScale(document: DocumentModel, scale: PageScale, target: PageScaleApplyTarget): DocumentModel {
  const pageIndices = pageScaleTargetIndices(document, target);
  const replacements = new Map(pageIndices.map((pageIndex) => [pageIndex, { ...scale, pageIndex }]));
  const untouched = (document.pageScales ?? []).filter((candidate) => !replacements.has(candidate.pageIndex));
  return {
    ...document,
    pageScales: [...untouched, ...replacements.values()].sort((a, b) => a.pageIndex - b.pageIndex),
  };
}

export function getPageScale(document: Pick<DocumentModel, 'pageScales'>, pageIndex: number): PageScale | undefined {
  return document.pageScales?.find((scale) => scale.pageIndex === pageIndex);
}

export function requirePageScale(document: Pick<DocumentModel, 'pageScales'>, pageIndex: number): PageScale {
  const scale = getPageScale(document, pageIndex);
  if (!scale) {
    throw new Error('Set page scale before placing measurement markups.');
  }
  return scale;
}

export function convertPdfDistanceToReal(scale: PageScale, pdfDistance: number, axis: 'x' | 'y' | 'uniform' = 'uniform'): number {
  const factor = axis === 'x' ? scale.scaleX : axis === 'y' ? scale.scaleY : (scale.scaleX + scale.scaleY) * 0.5;
  return pdfDistance * factor;
}

export function measureScaledLength(start: PdfPoint, end: PdfPoint, scale: PageScale): number {
  return Math.hypot((end.x - start.x) * scale.scaleX, (end.y - start.y) * scale.scaleY);
}

export function measureScaledPolyline(points: readonly PdfPoint[], scale: PageScale): number {
  let total = 0;
  for (let index = 1; index < points.length; index += 1) {
    total += measureScaledLength(points[index - 1], points[index], scale);
  }
  return total;
}

export function measureScaledPolygonArea(points: readonly PdfPoint[], scale: PageScale): number {
  if (points.length < 3) {
    return 0;
  }

  let twiceArea = 0;
  for (let index = 0; index < points.length; index += 1) {
    const current = points[index];
    const next = points[(index + 1) % points.length];
    twiceArea += current.x * next.y - next.x * current.y;
  }
  return Math.abs(twiceArea) * 0.5 * scale.scaleX * scale.scaleY;
}

export function convertScaledValueUnit(value: number, fromUnit: ScaleUnit, toUnit: ScaleUnit): number {
  return value * pointsPerUnit(fromUnit) / pointsPerUnit(toUnit);
}

export function convertScaledAreaUnit(value: number, fromUnit: ScaleUnit, toUnit: ScaleUnit): number {
  const factor = pointsPerUnit(fromUnit) / pointsPerUnit(toUnit);
  return value * factor * factor;
}

export function formatScaledLengthLabel(value: number, scale: PageScale, displayUnit: ScaleUnit = scale.realUnits): string {
  return `${formatScaledValue(convertScaledValueUnit(value, scale.realUnits, displayUnit), scale.precision)} ${displayUnit}`;
}

export function formatScaledAreaLabel(value: number, scale: PageScale, displayUnit: ScaleUnit = scale.realUnits): string {
  return `${formatScaledValue(convertScaledAreaUnit(value, scale.realUnits, displayUnit), scale.precision)} ${displayUnit}^2`;
}

export function scalePresetLabel(pdfLength: number, pdfUnits: ScaleUnit, realLength: number, realUnits: ScaleUnit): string {
  return `${formatNumber(pdfLength)} ${pdfUnits} = ${formatNumber(realLength)} ${realUnits}`;
}

export function formatPageScaleRatio(scale: PageScale): string {
  const realUnitsPerPaperUnit = scale.scaleX * pointsPerUnit(scale.realUnits);
  if (!Number.isFinite(realUnitsPerPaperUnit) || realUnitsPerPaperUnit <= 0) {
    return scale.name;
  }

  return `1:${formatNumber(realUnitsPerPaperUnit)}`;
}

export function parsePageScaleRanges(input: string, pageCount: number): readonly PageScaleRange[] {
  const ranges: PageScaleRange[] = [];
  for (const part of input.split(',').map((value) => value.trim()).filter(Boolean)) {
    const match = part.match(/^(\d+)(?:\s*-\s*(\d+))?$/);
    if (!match) {
      throw new Error('Enter page ranges like 1-3, 5, 9.');
    }

    const startPage = Number.parseInt(match[1], 10);
    const endPage = match[2] ? Number.parseInt(match[2], 10) : startPage;
    if (startPage < 1 || endPage < 1 || startPage > pageCount || endPage > pageCount) {
      throw new Error(`Page range must be between 1 and ${pageCount}.`);
    }

    ranges.push({
      startPageIndex: Math.min(startPage, endPage) - 1,
      endPageIndex: Math.max(startPage, endPage) - 1,
    });
  }

  if (ranges.length === 0) {
    throw new Error('Enter at least one page range.');
  }

  return ranges;
}

export function formatScaledValue(value: number, precision: ScalePrecision): string {
  if (precision.mode === 'fraction') {
    return formatFraction(value, precision.value);
  }
  const step = precision.value > 0 ? precision.value : 0.01;
  const rounded = Math.round(value / step) * step;
  const decimals = Math.max(0, decimalPlaces(step));
  return rounded.toFixed(decimals);
}

export function saveScalePreset(document: DocumentModel, preset: Omit<ScalePreset, 'builtIn'> & { readonly builtIn?: boolean }): DocumentModel {
  const saved = { ...preset, builtIn: preset.builtIn ?? false };
  const existing = document.scalePresets ?? [];
  return {
    ...document,
    scalePresets: [saved, ...existing.filter((candidate) => candidate.id !== saved.id)],
  };
}

export function deleteUserScalePreset(document: DocumentModel, presetId: string): DocumentModel {
  const preset = document.scalePresets?.find((candidate) => candidate.id === presetId);
  if (preset?.builtIn) {
    throw new Error('Built-in scale presets cannot be deleted.');
  }
  return {
    ...document,
    scalePresets: (document.scalePresets ?? []).filter((candidate) => candidate.id !== presetId),
  };
}

function pageScaleTargetIndices(document: DocumentModel, target: PageScaleApplyTarget): readonly number[] {
  if (target.kind === 'current') {
    return [target.pageIndex];
  }
  if (target.kind === 'all') {
    return document.pages.map((page) => page.index);
  }
  const indices = new Set<number>();
  for (const range of target.ranges) {
    const start = Math.max(0, Math.min(range.startPageIndex, range.endPageIndex));
    const end = Math.min(document.pages.length - 1, Math.max(range.startPageIndex, range.endPageIndex));
    for (let pageIndex = start; pageIndex <= end; pageIndex += 1) {
      indices.add(pageIndex);
    }
  }
  return [...indices].sort((a, b) => a - b);
}

function createBuiltInScalePreset(
  id: string,
  pdfLength: number,
  pdfUnits: ScaleUnit,
  realLength: number,
  realUnits: ScaleUnit,
  name = scalePresetLabel(pdfLength, pdfUnits, realLength, realUnits),
): ScalePreset {
  const scale = ratioToRealUnitsPerPdfPoint(pdfLength, pdfUnits, realLength);
  return {
    id,
    name,
    pdfUnits,
    realUnits,
    scaleX: scale,
    scaleY: scale,
    source: 'preset',
    builtIn: true,
  };
}

function createMetricRatioScalePreset(ratio: number): ScalePreset {
  return createBuiltInScalePreset(`one-to-${ratio}`, 1, 'cm', ratio / 100, 'm', `1:${ratio}`);
}

function ratioToRealUnitsPerPdfPoint(pdfLength: number, pdfUnits: ScaleUnit, realLength: number): number {
  if (pdfLength <= 0 || realLength <= 0) {
    throw new Error('Scale lengths must be positive.');
  }
  return realLength / (pdfLength * pointsPerUnit(pdfUnits));
}

function pointsPerUnit(unit: ScaleUnit): number {
  switch (unit) {
    case 'in':
      return 72;
    case 'ft':
      return 864;
    case 'mm':
      return 72 / 25.4;
    case 'cm':
      return 72 / 2.54;
    case 'm':
      return 72 / 0.0254;
    default:
      return 72;
  }
}

function formatFraction(value: number, denominator: number): string {
  const safeDenominator = Math.max(1, Math.round(denominator));
  const whole = Math.trunc(value);
  const numerator = Math.round(Math.abs(value - whole) * safeDenominator);
  if (numerator === 0) {
    return String(whole);
  }
  if (numerator === safeDenominator) {
    return String(whole + Math.sign(value || 1));
  }
  return whole === 0 ? `${numerator}/${safeDenominator}` : `${whole} ${numerator}/${safeDenominator}`;
}

function decimalPlaces(value: number): number {
  const text = String(value);
  return text.includes('.') ? text.split('.')[1].length : 0;
}

function formatNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(3).replace(/0+$/, '').replace(/\.$/, '');
}
