import type { Markup, ScalePrecision, ScaleUnit } from './document.js';
import type { Rect } from './points.js';

export const BUTTER_CANVAS_DOCUMENT_KIND = 'butter-canvas';
export const BUTTER_CANVAS_SCHEMA_VERSION = 1;
export const BUTTER_CANVAS_FILE_EXTENSION = '.bpc';

export type ButterCanvasGridPattern = 'lines' | 'dots';
export type ButterCanvasAssetKind = 'image' | 'pdf-page-snapshot';
export type ButterCanvasScaleSource = 'custom' | 'calibrated';
export type ButterCanvasTraceOutputMode = 'line' | 'polyline' | 'pen';

export interface ButterCanvasCamera {
  readonly x: number;
  readonly y: number;
  readonly zoom: number;
}

export interface ButterCanvasGridSettings {
  readonly visible: boolean;
  readonly snap: boolean;
  readonly size: number;
  readonly pattern: ButterCanvasGridPattern;
}

export interface ButterCanvasSnapSettings {
  readonly enabled: boolean;
  readonly grid: boolean;
  readonly endpoints: boolean;
  readonly midpoints: boolean;
  readonly centers: boolean;
  readonly intersections: boolean;
  readonly angles: boolean;
  readonly angleIncrementDeg: number;
  readonly sensitivityPx: number;
}

export interface ButterCanvasScale {
  readonly source: ButterCanvasScaleSource;
  readonly name: string;
  readonly canvasUnits: 'px';
  readonly realUnits: ScaleUnit;
  readonly canvasUnitsPerRealUnit: number;
  readonly precision: ScalePrecision;
}

export interface ButterCanvasTraceZone {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface ButterCanvasTraceSettings {
  readonly sensitivity: number;
  readonly clearExistingInZone: boolean;
  readonly zone: ButterCanvasTraceZone | null;
  readonly outputMode: ButterCanvasTraceOutputMode;
}

export interface ButterCanvasAssetSource {
  readonly type: 'image-file' | 'pdf-page';
  readonly fileName?: string;
  readonly filePath?: string;
  readonly pageIndex?: number;
  readonly pageCount?: number;
}

export interface ButterCanvasAsset {
  readonly id: string;
  readonly kind: ButterCanvasAssetKind;
  readonly name: string;
  readonly rect: Rect;
  readonly dataUrl: string;
  readonly mimeType: 'image/png' | 'image/jpeg';
  readonly rotation?: number;
  readonly opacity: number;
  readonly visible: boolean;
  readonly locked: boolean;
  readonly source?: ButterCanvasAssetSource;
}

export interface ButterCanvasDocument {
  readonly schemaVersion: typeof BUTTER_CANVAS_SCHEMA_VERSION;
  readonly kind: typeof BUTTER_CANVAS_DOCUMENT_KIND;
  readonly id: string;
  readonly title: string;
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly camera: ButterCanvasCamera;
  readonly grid: ButterCanvasGridSettings;
  readonly snap: ButterCanvasSnapSettings;
  readonly scale: ButterCanvasScale | null;
  readonly traceDefaults: ButterCanvasTraceSettings;
  readonly assets: readonly ButterCanvasAsset[];
  readonly markups: readonly Markup[];
}

export interface CreateButterCanvasDocumentParams {
  readonly id: string;
  readonly title?: string;
  readonly createdAt?: string;
  readonly updatedAt?: string;
  readonly camera?: Partial<ButterCanvasCamera>;
  readonly grid?: Partial<ButterCanvasGridSettings>;
  readonly snap?: Partial<ButterCanvasSnapSettings>;
  readonly scale?: ButterCanvasScale | null;
  readonly traceDefaults?: Partial<ButterCanvasTraceSettings>;
  readonly assets?: readonly ButterCanvasAsset[];
  readonly markups?: readonly Markup[];
}

export const DEFAULT_BUTTER_CANVAS_CAMERA = {
  x: 0,
  y: 0,
  zoom: 1,
} as const satisfies ButterCanvasCamera;

export const DEFAULT_BUTTER_CANVAS_GRID = {
  visible: true,
  snap: false,
  size: 50,
  pattern: 'lines',
} as const satisfies ButterCanvasGridSettings;

export const DEFAULT_BUTTER_CANVAS_SNAP = {
  enabled: true,
  grid: false,
  endpoints: true,
  midpoints: true,
  centers: true,
  intersections: true,
  angles: true,
  angleIncrementDeg: 45,
  sensitivityPx: 8,
} as const satisfies ButterCanvasSnapSettings;

export const DEFAULT_BUTTER_CANVAS_TRACE_SETTINGS = {
  sensitivity: 50,
  clearExistingInZone: true,
  zone: null,
  outputMode: 'polyline',
} as const satisfies ButterCanvasTraceSettings;

export function createButterCanvasDocument(params: CreateButterCanvasDocumentParams): ButterCanvasDocument {
  const now = new Date().toISOString();
  return {
    schemaVersion: BUTTER_CANVAS_SCHEMA_VERSION,
    kind: BUTTER_CANVAS_DOCUMENT_KIND,
    id: params.id,
    title: params.title ?? 'Untitled Canvas',
    createdAt: params.createdAt ?? now,
    updatedAt: params.updatedAt ?? params.createdAt ?? now,
    camera: {
      ...DEFAULT_BUTTER_CANVAS_CAMERA,
      ...params.camera,
    },
    grid: {
      ...DEFAULT_BUTTER_CANVAS_GRID,
      ...params.grid,
    },
    snap: {
      ...DEFAULT_BUTTER_CANVAS_SNAP,
      ...params.snap,
    },
    scale: params.scale ?? null,
    traceDefaults: {
      ...DEFAULT_BUTTER_CANVAS_TRACE_SETTINGS,
      ...params.traceDefaults,
    },
    assets: params.assets ?? [],
    markups: params.markups ?? [],
  };
}

export function serializeButterCanvasDocument(document: ButterCanvasDocument): string {
  assertButterCanvasDocument(document);
  return `${JSON.stringify(document, null, 2)}\n`;
}

export function parseButterCanvasDocument(contents: string): ButterCanvasDocument {
  let parsed: unknown;
  try {
    parsed = JSON.parse(contents);
  } catch (error) {
    throw new Error(`Invalid Butter Canvas document JSON: ${error instanceof Error ? error.message : 'Unable to parse.'}`);
  }

  assertButterCanvasDocument(parsed);
  return parsed;
}

export function assertButterCanvasDocument(value: unknown): asserts value is ButterCanvasDocument {
  if (!isRecord(value)) {
    throw new Error('Invalid Butter Canvas document: expected an object.');
  }
  if (value.kind !== BUTTER_CANVAS_DOCUMENT_KIND) {
    throw new Error('Invalid Butter Canvas document: kind must be "butter-canvas".');
  }
  if (value.schemaVersion !== BUTTER_CANVAS_SCHEMA_VERSION) {
    throw new Error(`Unsupported Butter Canvas schema version: ${String(value.schemaVersion)}.`);
  }
  requireString(value.id, 'id');
  requireString(value.title, 'title');
  requireString(value.createdAt, 'createdAt');
  requireString(value.updatedAt, 'updatedAt');
  assertCamera(value.camera);
  assertGrid(value.grid);
  assertSnap(value.snap);
  if (value.scale !== null) {
    assertScale(value.scale);
  }
  assertTraceSettings(value.traceDefaults);
  assertArray(value.assets, 'assets');
  value.assets.forEach((asset, index) => assertAsset(asset, `assets[${index}]`));
  assertArray(value.markups, 'markups');
  value.markups.forEach((markup, index) => assertMarkupShell(markup, `markups[${index}]`));
}

function assertCamera(value: unknown): asserts value is ButterCanvasCamera {
  assertRecord(value, 'camera');
  requireFiniteNumber(value.x, 'camera.x');
  requireFiniteNumber(value.y, 'camera.y');
  requirePositiveNumber(value.zoom, 'camera.zoom');
}

function assertGrid(value: unknown): asserts value is ButterCanvasGridSettings {
  assertRecord(value, 'grid');
  requireBoolean(value.visible, 'grid.visible');
  requireBoolean(value.snap, 'grid.snap');
  requirePositiveNumber(value.size, 'grid.size');
  if (value.pattern !== 'lines' && value.pattern !== 'dots') {
    throw new Error('Invalid Butter Canvas document: grid.pattern must be "lines" or "dots".');
  }
}

function assertSnap(value: unknown): asserts value is ButterCanvasSnapSettings {
  assertRecord(value, 'snap');
  requireBoolean(value.enabled, 'snap.enabled');
  requireBoolean(value.grid, 'snap.grid');
  requireBoolean(value.endpoints, 'snap.endpoints');
  requireBoolean(value.midpoints, 'snap.midpoints');
  requireBoolean(value.centers, 'snap.centers');
  requireBoolean(value.intersections, 'snap.intersections');
  requireBoolean(value.angles, 'snap.angles');
  requirePositiveNumber(value.angleIncrementDeg, 'snap.angleIncrementDeg');
  requirePositiveNumber(value.sensitivityPx, 'snap.sensitivityPx');
}

function assertScale(value: unknown): asserts value is ButterCanvasScale {
  assertRecord(value, 'scale');
  if (value.source !== 'custom' && value.source !== 'calibrated') {
    throw new Error('Invalid Butter Canvas document: scale.source must be "custom" or "calibrated".');
  }
  requireString(value.name, 'scale.name');
  if (value.canvasUnits !== 'px') {
    throw new Error('Invalid Butter Canvas document: scale.canvasUnits must be "px".');
  }
  if (!isScaleUnit(value.realUnits)) {
    throw new Error('Invalid Butter Canvas document: scale.realUnits is unsupported.');
  }
  requirePositiveNumber(value.canvasUnitsPerRealUnit, 'scale.canvasUnitsPerRealUnit');
  assertRecord(value.precision, 'scale.precision');
  if (value.precision.mode !== 'decimal' && value.precision.mode !== 'fraction') {
    throw new Error('Invalid Butter Canvas document: scale.precision.mode is unsupported.');
  }
  requirePositiveNumber(value.precision.value, 'scale.precision.value');
}

function assertTraceSettings(value: unknown): asserts value is ButterCanvasTraceSettings {
  assertRecord(value, 'traceDefaults');
  requireFiniteNumber(value.sensitivity, 'traceDefaults.sensitivity');
  if (value.sensitivity < 0 || value.sensitivity > 100) {
    throw new Error('Invalid Butter Canvas document: traceDefaults.sensitivity must be between 0 and 100.');
  }
  requireBoolean(value.clearExistingInZone, 'traceDefaults.clearExistingInZone');
  if (value.zone !== null) {
    assertTraceZone(value.zone);
  }
  if (value.outputMode !== 'line' && value.outputMode !== 'polyline' && value.outputMode !== 'pen') {
    throw new Error('Invalid Butter Canvas document: traceDefaults.outputMode is unsupported.');
  }
}

function assertTraceZone(value: unknown): asserts value is ButterCanvasTraceZone {
  assertRecord(value, 'traceDefaults.zone');
  requireFiniteNumber(value.x, 'traceDefaults.zone.x');
  requireFiniteNumber(value.y, 'traceDefaults.zone.y');
  requirePositiveNumber(value.width, 'traceDefaults.zone.width');
  requirePositiveNumber(value.height, 'traceDefaults.zone.height');
}

function assertAsset(value: unknown, path: string): asserts value is ButterCanvasAsset {
  assertRecord(value, path);
  requireString(value.id, `${path}.id`);
  if (value.kind !== 'image' && value.kind !== 'pdf-page-snapshot') {
    throw new Error(`Invalid Butter Canvas document: ${path}.kind is unsupported.`);
  }
  requireString(value.name, `${path}.name`);
  assertRect(value.rect, `${path}.rect`);
  requireString(value.dataUrl, `${path}.dataUrl`);
  if (value.mimeType !== 'image/png' && value.mimeType !== 'image/jpeg') {
    throw new Error(`Invalid Butter Canvas document: ${path}.mimeType is unsupported.`);
  }
  if (value.rotation !== undefined) {
    requireFiniteNumber(value.rotation, `${path}.rotation`);
  }
  requireFiniteNumber(value.opacity, `${path}.opacity`);
  if (value.opacity < 0 || value.opacity > 1) {
    throw new Error(`Invalid Butter Canvas document: ${path}.opacity must be between 0 and 1.`);
  }
  requireBoolean(value.visible, `${path}.visible`);
  requireBoolean(value.locked, `${path}.locked`);
  if (value.source !== undefined) {
    assertRecord(value.source, `${path}.source`);
    if (value.source.type !== 'image-file' && value.source.type !== 'pdf-page') {
      throw new Error(`Invalid Butter Canvas document: ${path}.source.type is unsupported.`);
    }
    if (value.source.fileName !== undefined) {
      requireString(value.source.fileName, `${path}.source.fileName`);
    }
    if (value.source.filePath !== undefined) {
      requireString(value.source.filePath, `${path}.source.filePath`);
    }
    if (value.source.pageIndex !== undefined) {
      requireInteger(value.source.pageIndex, `${path}.source.pageIndex`);
    }
    if (value.source.pageCount !== undefined) {
      requirePositiveInteger(value.source.pageCount, `${path}.source.pageCount`);
    }
  }
}

function assertMarkupShell(value: unknown, path: string): asserts value is Markup {
  assertRecord(value, path);
  requireString(value.id, `${path}.id`);
  requireInteger(value.pageIndex, `${path}.pageIndex`);
  requireString(value.kind, `${path}.kind`);
}

function assertRect(value: unknown, path: string): asserts value is Rect {
  assertRecord(value, path);
  requireFiniteNumber(value.x, `${path}.x`);
  requireFiniteNumber(value.y, `${path}.y`);
  requirePositiveNumber(value.width, `${path}.width`);
  requirePositiveNumber(value.height, `${path}.height`);
}

function isScaleUnit(value: unknown): value is ScaleUnit {
  return value === 'in' || value === 'ft' || value === 'mm' || value === 'cm' || value === 'm';
}

function assertRecord(value: unknown, path: string): asserts value is Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error(`Invalid Butter Canvas document: ${path} must be an object.`);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function assertArray(value: unknown, path: string): asserts value is unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`Invalid Butter Canvas document: ${path} must be an array.`);
  }
}

function requireString(value: unknown, path: string): asserts value is string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Invalid Butter Canvas document: ${path} must be a non-empty string.`);
  }
}

function requireBoolean(value: unknown, path: string): asserts value is boolean {
  if (typeof value !== 'boolean') {
    throw new Error(`Invalid Butter Canvas document: ${path} must be a boolean.`);
  }
}

function requireFiniteNumber(value: unknown, path: string): asserts value is number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new Error(`Invalid Butter Canvas document: ${path} must be a finite number.`);
  }
}

function requirePositiveNumber(value: unknown, path: string): asserts value is number {
  requireFiniteNumber(value, path);
  if (value <= 0) {
    throw new Error(`Invalid Butter Canvas document: ${path} must be greater than 0.`);
  }
}

function requireInteger(value: unknown, path: string): asserts value is number {
  requireFiniteNumber(value, path);
  if (!Number.isInteger(value)) {
    throw new Error(`Invalid Butter Canvas document: ${path} must be an integer.`);
  }
}

function requirePositiveInteger(value: unknown, path: string): asserts value is number {
  requireInteger(value, path);
  if (value <= 0) {
    throw new Error(`Invalid Butter Canvas document: ${path} must be greater than 0.`);
  }
}
