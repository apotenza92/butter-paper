import electron from 'electron';
import { parseButterCanvasDocument, serializeButterCanvasDocument } from '@butter-paper/core';
import { existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { BrowserWindow as BrowserWindowInstance, Event as ElectronEvent, RenderProcessGoneDetails } from 'electron';
import { ipcChannels } from '../shared/ipc';
import type {
  ApplicationMetadata,
  DesktopRenderBackend,
  DesktopRenderBackendConfig,
  DesktopRenderBackendSelection,
  DesktopRenderBackendSelectionSource,
  DesktopRenderCapabilities,
  RenderCoreCliCommand,
  RenderCoreCliCommandStats,
  RenderCoreCloseDocumentRequest,
  RenderCoreDiagnostics,
  RenderCoreError,
  RenderCoreErrorCode,
  RenderCoreGetPageInfoRequest,
  RenderCoreNativeRenderPageDiagnostics,
  RenderCoreNativeStageStats,
  RenderCoreOpenDocumentRequest,
  RenderCoreOpenDocumentResponse,
  RenderCorePageInfo,
  RenderCoreReadSurfaceRequest,
  RenderCoreReadSurfaceResponse,
  RenderCoreReleaseSurfaceRequest,
  RenderCoreRenderMode,
  RenderCoreRenderPageRequest,
  RenderCoreRenderPageResponse,
  RenderCoreRenderRequestClass,
  RenderCoreResult,
  RenderCoreWorkerPoolStats,
  ThemeMode,
  ThemeSnapshot,
  UpdateFrequency,
} from '../shared/protocol';
import { resolveApplicationMetadata } from './applicationMetadata';
import { setAsDefaultPdfApp } from './defaultPdfApp';
import { takePendingPdfPaths } from './pendingPdfPaths';
import { loadDocumentPayload, loadPageGeometryIndex, saveDocumentPayload } from './pdfSession';
import { getFocusedWindowState, isTestModeEnabled, resolveFixturePath, setFocusedWindowBounds } from './testMode';
import { readBinaryFile, readTextFile, writeBinaryFile, writeTextFile } from './fileSystem';
import { DesktopUpdaterService, loadElectronAutoUpdater } from './updater';
import { resolveReleasePageUrl } from './releasePage';

const { app, BrowserWindow, ipcMain, dialog, nativeTheme, shell } = electron;
const require = createRequire(import.meta.url);
const moduleDir = dirname(fileURLToPath(import.meta.url));
const preloadPath = join(moduleDir, 'preload.cjs');
const repoRoot = process.cwd();
const defaultSamplePdfPath = join(moduleDir, '../../../..', 'tests/fixtures/generated/zoom-target.pdf');
const pdfiumRenderCoreDir = join(repoRoot, 'native/pdfium-render-core');
const pdfiumRenderCoreExecutableName = process.platform === 'win32'
  ? 'butter-paper-pdfium-render-core.exe'
  : 'butter-paper-pdfium-render-core';
const pdfiumRenderCoreReleasePath = join(pdfiumRenderCoreDir, 'target/release', pdfiumRenderCoreExecutableName);
const pdfiumRenderCoreDebugPath = join(pdfiumRenderCoreDir, 'target/debug', pdfiumRenderCoreExecutableName);
let mainWindow: BrowserWindowInstance | null = null;
let themeListenerRegistered = false;
let pdfiumCliPathPromise: Promise<string> | null = null;
let updaterService: DesktopUpdaterService | null = null;
let unsubscribeUpdaterStatus: (() => void) | null = null;
const documentRegistry = new Map<string, {
  ownerWebContentsId: number;
  filePath: string;
  pageCount: number;
}>();
const surfaceRegistry = new Map<string, {
  ownerWebContentsId: number;
  documentId: string;
  bytes: Uint8Array;
  pixelWidth: number;
  pixelHeight: number;
}>();

const renderCoreCliCommands: readonly RenderCoreCliCommand[] = ['document-info', 'page-info', 'render-page', 'other'];
const renderCoreCliStats = createRenderCoreCliStats();
const renderPageNativeDiagnostics = createEmptyRenderPageNativeDiagnostics();
let renderPageWorkerPoolStats: RenderCoreWorkerPoolStats | null = null;

declare const MAIN_WINDOW_VITE_DEV_SERVER_URL: string | undefined;
declare const MAIN_WINDOW_VITE_NAME: string | undefined;

interface PdfiumDocumentInfoPayload {
  pageCount: number;
}

interface PdfiumPageInfoPayload {
  width: number;
  height: number;
  rotation: number;
}

interface PdfiumRenderPagePayload {
  width: number;
  height: number;
  byteLength: number;
  timings?: PdfiumRenderPageTimings;
}

interface PdfiumCliErrorPayload {
  error?: string;
}

interface PdfiumRenderWorkerRequest {
  file: string;
  pageIndex: number;
  width: number;
  height: number;
  rotation?: number;
  renderMode?: RenderCoreRenderMode;
  requestClass?: RenderCoreRenderRequestClass;
  cropPdfRect?: RenderCoreRenderPageRequest['cropPdfRect'];
}

interface PdfiumRenderWorkerMetadata extends PdfiumRenderPagePayload {
  id?: string;
  error?: string;
}

interface PdfiumRenderPageTimings {
  resolvePdfPathMs?: number;
  loadDocumentMs?: number;
  getPageMs?: number;
  buildRenderConfigMs?: number;
  pdfiumRenderMs?: number;
  bitmapToImageMs?: number;
  pngEncodeMs?: number;
  nativeTotalMs?: number;
}

interface RenderPageWorkerJob {
  id: string;
  request: PdfiumRenderWorkerRequest;
  enqueuedAt: number;
  startedAt: number;
  resolve(value: { metadata: PdfiumRenderWorkerMetadata; bytes: Uint8Array }): void;
  reject(error: Error): void;
}

interface RenderPageWorkerLane {
  index: number;
  child: import('node:child_process').ChildProcessWithoutNullStreams;
  buffer: Buffer;
  metadata: PdfiumRenderWorkerMetadata | null;
  currentJob: RenderPageWorkerJob | null;
  closed: boolean;
}

let renderPageWorkerPoolPromise: Promise<RenderPageWorkerPool> | null = null;

if (
  !isTestModeEnabled()
  && process.env.BP_OPEN_SAMPLE_PDF !== '0'
  && !process.env.BP_DEFAULT_SAMPLE_PDF?.trim()
  && existsSync(defaultSamplePdfPath)
) {
  process.env.BP_DEFAULT_SAMPLE_PDF = defaultSamplePdfPath;
}

function revealWindow(window: BrowserWindowInstance): void {
  if (window.isDestroyed()) {
    return;
  }

  if (window.isMinimized()) {
    window.restore();
  }

  window.show();
  if (process.platform === 'darwin') {
    app.focus({ steal: true });
  }
  window.focus();
}

function clearRenderCoreRegistries(): void {
  documentRegistry.clear();
  surfaceRegistry.clear();
}

function shutdownRenderPageWorkerPool(): void {
  if (!renderPageWorkerPoolPromise) {
    return;
  }

  void renderPageWorkerPoolPromise.then((pool) => {
    pool.close();
  }).catch(() => {
    // Ignore shutdown failures; the pool may have failed during startup.
  });
  renderPageWorkerPoolPromise = null;
}

function createEmptyRenderCoreCliCommandStats(): RenderCoreCliCommandStats {
  return {
    count: 0,
    failures: 0,
    totalMs: 0,
    maxMs: 0,
    lastMs: null,
  };
}

function createRenderCoreCliStats(): Record<RenderCoreCliCommand, RenderCoreCliCommandStats> {
  return {
    'document-info': createEmptyRenderCoreCliCommandStats(),
    'page-info': createEmptyRenderCoreCliCommandStats(),
    'render-page': createEmptyRenderCoreCliCommandStats(),
    other: createEmptyRenderCoreCliCommandStats(),
  };
}

function createEmptyRenderCoreNativeStageStats(): RenderCoreNativeStageStats {
  return {
    count: 0,
    totalMs: 0,
    maxMs: 0,
    lastMs: null,
  };
}

function createEmptyRenderPageNativeDiagnostics(): RenderCoreNativeRenderPageDiagnostics {
  return {
    count: 0,
    failures: 0,
    totalMs: 0,
    maxMs: 0,
    lastMs: null,
    stages: createEmptyRenderPageNativeStages(),
    byRenderMode: {
      full: createEmptyRenderPageNativeModeDiagnostics(),
      preview: createEmptyRenderPageNativeModeDiagnostics(),
    },
    byRequestClass: {},
  };
}

function createEmptyRenderPageNativeModeDiagnostics(): RenderCoreNativeRenderPageDiagnostics['byRenderMode'][RenderCoreRenderMode] {
  return {
    count: 0,
    failures: 0,
    totalMs: 0,
    maxMs: 0,
    lastMs: null,
    stages: createEmptyRenderPageNativeStages(),
  };
}

function createEmptyRenderPageNativeStages(): RenderCoreNativeRenderPageDiagnostics['stages'] {
  return {
    resolvePdfPathMs: createEmptyRenderCoreNativeStageStats(),
    loadDocumentMs: createEmptyRenderCoreNativeStageStats(),
    getPageMs: createEmptyRenderCoreNativeStageStats(),
    buildRenderConfigMs: createEmptyRenderCoreNativeStageStats(),
    pdfiumRenderMs: createEmptyRenderCoreNativeStageStats(),
    bitmapToImageMs: createEmptyRenderCoreNativeStageStats(),
    pngEncodeMs: createEmptyRenderCoreNativeStageStats(),
    nativeTotalMs: createEmptyRenderCoreNativeStageStats(),
  };
}

function cloneRenderCoreCliStats(): Record<RenderCoreCliCommand, RenderCoreCliCommandStats> {
  return {
    'document-info': { ...renderCoreCliStats['document-info'] },
    'page-info': { ...renderCoreCliStats['page-info'] },
    'render-page': { ...renderCoreCliStats['render-page'] },
    other: { ...renderCoreCliStats.other },
  };
}

function cloneRenderPageNativeDiagnostics(): RenderCoreNativeRenderPageDiagnostics {
  return {
    count: renderPageNativeDiagnostics.count,
    failures: renderPageNativeDiagnostics.failures,
    totalMs: renderPageNativeDiagnostics.totalMs,
    maxMs: renderPageNativeDiagnostics.maxMs,
    lastMs: renderPageNativeDiagnostics.lastMs,
    stages: {
      resolvePdfPathMs: { ...renderPageNativeDiagnostics.stages.resolvePdfPathMs },
      loadDocumentMs: { ...renderPageNativeDiagnostics.stages.loadDocumentMs },
      getPageMs: { ...renderPageNativeDiagnostics.stages.getPageMs },
      buildRenderConfigMs: { ...renderPageNativeDiagnostics.stages.buildRenderConfigMs },
      pdfiumRenderMs: { ...renderPageNativeDiagnostics.stages.pdfiumRenderMs },
      bitmapToImageMs: { ...renderPageNativeDiagnostics.stages.bitmapToImageMs },
      pngEncodeMs: { ...renderPageNativeDiagnostics.stages.pngEncodeMs },
      nativeTotalMs: { ...renderPageNativeDiagnostics.stages.nativeTotalMs },
    },
    byRenderMode: {
      full: cloneRenderPageNativeModeDiagnostics(renderPageNativeDiagnostics.byRenderMode.full),
      preview: cloneRenderPageNativeModeDiagnostics(renderPageNativeDiagnostics.byRenderMode.preview),
    },
    byRequestClass: Object.fromEntries(
      Object.entries(renderPageNativeDiagnostics.byRequestClass).map(([requestClass, diagnostics]) => [
        requestClass,
        cloneRenderPageNativeModeDiagnostics(diagnostics),
      ]),
    ),
  };
}

function cloneRenderPageNativeModeDiagnostics(
  diagnostics: RenderCoreNativeRenderPageDiagnostics['byRenderMode'][RenderCoreRenderMode],
): RenderCoreNativeRenderPageDiagnostics['byRenderMode'][RenderCoreRenderMode] {
  return {
    count: diagnostics.count,
    failures: diagnostics.failures,
    totalMs: diagnostics.totalMs,
    maxMs: diagnostics.maxMs,
    lastMs: diagnostics.lastMs,
    stages: {
      resolvePdfPathMs: { ...diagnostics.stages.resolvePdfPathMs },
      loadDocumentMs: { ...diagnostics.stages.loadDocumentMs },
      getPageMs: { ...diagnostics.stages.getPageMs },
      buildRenderConfigMs: { ...diagnostics.stages.buildRenderConfigMs },
      pdfiumRenderMs: { ...diagnostics.stages.pdfiumRenderMs },
      bitmapToImageMs: { ...diagnostics.stages.bitmapToImageMs },
      pngEncodeMs: { ...diagnostics.stages.pngEncodeMs },
      nativeTotalMs: { ...diagnostics.stages.nativeTotalMs },
    },
  };
}

function normalizeRenderCoreCliCommand(command: string | undefined): RenderCoreCliCommand {
  return renderCoreCliCommands.includes(command as RenderCoreCliCommand)
    ? command as RenderCoreCliCommand
    : 'other';
}

function recordRenderPageNativeMetadata(
  metadata: PdfiumRenderWorkerMetadata | undefined,
  failed = false,
  renderMode: RenderCoreRenderMode = 'full',
  requestClass?: RenderCoreRenderRequestClass,
): void {
  const timings = metadata?.timings;
  if (!timings) {
    if (failed) {
      renderPageNativeDiagnostics.failures += 1;
      renderPageNativeDiagnostics.byRenderMode[renderMode].failures += 1;
      if (requestClass) {
        ensureRenderPageNativeRequestClassDiagnostics(requestClass).failures += 1;
      }
    }
    return;
  }

  const nativeTotalMs = normalizeDurationMs(timings.nativeTotalMs);
  recordRenderPageNativeTiming(renderPageNativeDiagnostics, timings, nativeTotalMs, failed);
  recordRenderPageNativeTiming(renderPageNativeDiagnostics.byRenderMode[renderMode], timings, nativeTotalMs, failed);
  if (requestClass) {
    recordRenderPageNativeTiming(
      ensureRenderPageNativeRequestClassDiagnostics(requestClass),
      timings,
      nativeTotalMs,
      failed,
    );
  }
}

function ensureRenderPageNativeRequestClassDiagnostics(
  requestClass: RenderCoreRenderRequestClass,
): RenderCoreNativeRenderPageDiagnostics['byRenderMode'][RenderCoreRenderMode] {
  renderPageNativeDiagnostics.byRequestClass[requestClass] ??= createEmptyRenderPageNativeModeDiagnostics();
  return renderPageNativeDiagnostics.byRequestClass[requestClass];
}

function recordRenderPageNativeTiming(
  diagnostics: RenderCoreNativeRenderPageDiagnostics | RenderCoreNativeRenderPageDiagnostics['byRenderMode'][RenderCoreRenderMode],
  timings: PdfiumRenderPageTimings,
  nativeTotalMs: number,
  failed: boolean,
): void {
  diagnostics.count += 1;
  diagnostics.totalMs = Math.round((diagnostics.totalMs + nativeTotalMs) * 1000) / 1000;
  diagnostics.maxMs = Math.max(diagnostics.maxMs, nativeTotalMs);
  diagnostics.lastMs = nativeTotalMs;
  if (failed) {
    diagnostics.failures += 1;
  }

  recordRenderPageNativeStage(diagnostics.stages.resolvePdfPathMs, timings.resolvePdfPathMs);
  recordRenderPageNativeStage(diagnostics.stages.loadDocumentMs, timings.loadDocumentMs);
  recordRenderPageNativeStage(diagnostics.stages.getPageMs, timings.getPageMs);
  recordRenderPageNativeStage(diagnostics.stages.buildRenderConfigMs, timings.buildRenderConfigMs);
  recordRenderPageNativeStage(diagnostics.stages.pdfiumRenderMs, timings.pdfiumRenderMs);
  recordRenderPageNativeStage(diagnostics.stages.bitmapToImageMs, timings.bitmapToImageMs);
  recordRenderPageNativeStage(diagnostics.stages.pngEncodeMs, timings.pngEncodeMs);
  recordRenderPageNativeStage(diagnostics.stages.nativeTotalMs, timings.nativeTotalMs);
}

function recordRenderPageNativeStage(stats: RenderCoreNativeStageStats, durationMs: number | undefined): void {
  const roundedDurationMs = normalizeDurationMs(durationMs);
  stats.count += 1;
  stats.totalMs = Math.round((stats.totalMs + roundedDurationMs) * 1000) / 1000;
  stats.maxMs = Math.max(stats.maxMs, roundedDurationMs);
  stats.lastMs = roundedDurationMs;
}

function normalizeDurationMs(durationMs: number | undefined): number {
  return Number.isFinite(durationMs) && typeof durationMs === 'number'
    ? Math.max(0, Math.round(durationMs * 1000) / 1000)
    : 0;
}

function recordRenderCoreCliInvocation(command: RenderCoreCliCommand, durationMs: number, failed: boolean): void {
  const stats = renderCoreCliStats[command];
  const roundedDurationMs = Math.round(durationMs * 1000) / 1000;
  stats.count += 1;
  stats.totalMs = Math.round((stats.totalMs + roundedDurationMs) * 1000) / 1000;
  stats.maxMs = Math.max(stats.maxMs, roundedDurationMs);
  stats.lastMs = roundedDurationMs;
  if (failed) {
    stats.failures += 1;
  }
}

function getRenderCoreDiagnostics(): RenderCoreDiagnostics {
  const surfacesByDocumentMap = new Map<string, { documentId: string; count: number; bytes: number }>();
  let activeSurfaceBytes = 0;

  for (const surface of surfaceRegistry.values()) {
    const byteLength = surface.bytes.byteLength;
    activeSurfaceBytes += byteLength;
    const current = surfacesByDocumentMap.get(surface.documentId) ?? { documentId: surface.documentId, count: 0, bytes: 0 };
    current.count += 1;
    current.bytes += byteLength;
    surfacesByDocumentMap.set(surface.documentId, current);
  }

  return {
    backend: resolveDesktopRenderBackend(),
    activeDocuments: documentRegistry.size,
    activeSurfaces: surfaceRegistry.size,
    activeSurfaceBytes,
    surfacesByDocument: [...surfacesByDocumentMap.values()],
    cli: cloneRenderCoreCliStats(),
    renderPageWorkerPool: renderPageWorkerPoolStats ? { ...renderPageWorkerPoolStats } : null,
    renderPageNative: cloneRenderPageNativeDiagnostics(),
  };
}

function clearRenderCoreRegistriesForOwner(ownerWebContentsId: number): void {
  for (const [documentId, document] of documentRegistry.entries()) {
    if (document.ownerWebContentsId === ownerWebContentsId) {
      documentRegistry.delete(documentId);
    }
  }

  for (const [surfaceId, surface] of surfaceRegistry.entries()) {
    if (surface.ownerWebContentsId === ownerWebContentsId) {
      surfaceRegistry.delete(surfaceId);
    }
  }
}

function createRuntimeId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

function resolveThemeMode(): ThemeMode {
  if (isTestModeEnabled()) {
    const forcedTheme = process.env.BP_TEST_THEME?.trim().toLowerCase();
    if (forcedTheme === 'light' || forcedTheme === 'dark') {
      return forcedTheme;
    }
  }

  return nativeTheme.shouldUseDarkColors ? 'dark' : 'light';
}

function getThemeSnapshot(): ThemeSnapshot {
  return { mode: resolveThemeMode() };
}

function getWindowBackgroundColor(mode: ThemeMode): string {
  return mode === 'dark' ? '#111315' : '#f5f5f5';
}

function notifyThemeChanged(snapshot: ThemeSnapshot): void {
  for (const window of BrowserWindow.getAllWindows()) {
    window.setBackgroundColor(getWindowBackgroundColor(snapshot.mode));
    window.webContents.send(ipcChannels.themeChanged, snapshot);
  }
}

function registerThemeListener(): void {
  if (themeListenerRegistered) {
    return;
  }

  nativeTheme.on('updated', () => {
    if (isTestModeEnabled() && process.env.BP_TEST_THEME) {
      return;
    }

    notifyThemeChanged(getThemeSnapshot());
  });

  themeListenerRegistered = true;
}

function parseDesktopRenderBackendOverride(): {
  requestedBackend: DesktopRenderBackend;
  selectionSource: DesktopRenderBackendSelectionSource;
  envOverride: string | null;
} {
  const envOverride = process.env.BP_DESKTOP_RENDER_BACKEND?.trim().toLowerCase() ?? null;
  if (envOverride === 'pdfium' || envOverride === 'pdfjs') {
    return {
      requestedBackend: envOverride,
      selectionSource: 'env',
      envOverride,
    };
  }

  return {
    requestedBackend: 'pdfjs',
    selectionSource: 'default',
    envOverride,
  };
}

function resolveDesktopRenderBackend(): DesktopRenderBackend {
  return parseDesktopRenderBackendOverride().requestedBackend;
}

function getDesktopRenderBackendConfig(): DesktopRenderBackendConfig {
  const parsed = parseDesktopRenderBackendOverride();
  return {
    requestedBackend: parsed.requestedBackend,
    selectionSource: parsed.selectionSource,
    envOverride: parsed.envOverride,
  };
}

function isPdfiumBackendSelected(): boolean {
  return resolveDesktopRenderBackend() === 'pdfium';
}

function renderCoreError(code: RenderCoreErrorCode, message: string): RenderCoreError {
  return {
    code,
    message,
    backend: resolveDesktopRenderBackend(),
  };
}

function errorResult<T>(code: RenderCoreErrorCode, message: string): RenderCoreResult<T> {
  return {
    ok: false,
    error: renderCoreError(code, message),
  };
}

function successResult<T>(value: T): RenderCoreResult<T> {
  return {
    ok: true,
    value,
  };
}

function notImplementedRenderCoreResult<T>(): RenderCoreResult<T> {
  const backend = resolveDesktopRenderBackend();
  return errorResult(
    backend === 'pdfium' ? 'backend-unavailable' : 'not-implemented',
    backend === 'pdfium'
      ? 'The PDFium desktop render backend is configured but not available.'
      : 'Desktop render-core protocol is only available when BP_DESKTOP_RENDER_BACKEND=pdfium.',
  );
}

async function fileExists(path: string): Promise<boolean> {
  try {
    const { access } = require('node:fs/promises') as typeof import('node:fs/promises');
    const { constants } = require('node:fs') as typeof import('node:fs');
    await access(path, constants.F_OK | constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

async function runCargoBuildRelease(): Promise<void> {
  const { spawn } = require('node:child_process') as typeof import('node:child_process');

  return await new Promise((resolve, reject) => {
    const child = spawn('cargo', ['build', '--release'], {
      cwd: pdfiumRenderCoreDir,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: process.env,
    });

    let stdout = '';
    let stderr = '';

    child.stdout.on('data', (chunk: Buffer | string) => {
      stdout += chunk.toString();
    });

    child.stderr.on('data', (chunk: Buffer | string) => {
      stderr += chunk.toString();
    });

    child.on('error', (error) => {
      reject(new Error(`Failed to run cargo build --release: ${error.message}`));
    });

    child.on('close', (code, signal) => {
      if (code === 0) {
        resolve(undefined);
        return;
      }

      const details = [stdout.trim(), stderr.trim()].filter(Boolean).join('\n');
      reject(
        new Error(
          signal
            ? `cargo build --release terminated by signal ${signal}${details ? `\n${details}` : ''}`
            : `cargo build --release failed with exit code ${code ?? 'unknown'}${details ? `\n${details}` : ''}`,
        ),
      );
    });
  });
}

async function resolvePdfiumCliPath(): Promise<string> {
  if (await fileExists(pdfiumRenderCoreReleasePath)) {
    return pdfiumRenderCoreReleasePath;
  }

  if (await fileExists(pdfiumRenderCoreDebugPath)) {
    return pdfiumRenderCoreDebugPath;
  }

  await runCargoBuildRelease();

  if (await fileExists(pdfiumRenderCoreReleasePath)) {
    return pdfiumRenderCoreReleasePath;
  }

  throw new Error(`Unable to locate PDFium render-core binary at ${pdfiumRenderCoreReleasePath}`);
}

async function ensurePdfiumCliPath(): Promise<string> {
  if (pdfiumCliPathPromise === null) {
    pdfiumCliPathPromise = resolvePdfiumCliPath().catch((error: unknown) => {
      pdfiumCliPathPromise = null;
      throw error;
    });
  }

  return pdfiumCliPathPromise;
}

async function getDesktopRenderBackendSelection(): Promise<DesktopRenderBackendSelection> {
  const config = getDesktopRenderBackendConfig();

  if (config.requestedBackend !== 'pdfium') {
    return {
      configuredBackend: config.requestedBackend,
      activeBackend: null,
      selectionSource: config.selectionSource,
    };
  }

  try {
    await ensurePdfiumCliPath();
    return {
      configuredBackend: config.requestedBackend,
      activeBackend: 'pdfium',
      selectionSource: config.selectionSource,
    };
  } catch {
    return {
      configuredBackend: config.requestedBackend,
      activeBackend: null,
      selectionSource: config.selectionSource,
    };
  }
}

async function getDesktopRenderCapabilities(): Promise<DesktopRenderCapabilities> {
  const backend = resolveDesktopRenderBackend();

  if (backend !== 'pdfium') {
    return {
      backend,
      available: false,
      canOpenDocument: false,
      canGetPageInfo: false,
      canRenderPage: false,
      canReadSurface: false,
      canReleaseSurface: false,
      canCloseDocument: false,
      notes: ['Desktop render-core is disabled unless BP_DESKTOP_RENDER_BACKEND=pdfium.'],
    };
  }

  try {
    const cliPath = await ensurePdfiumCliPath();
    return {
      backend,
      available: true,
      canOpenDocument: true,
      canGetPageInfo: true,
      canRenderPage: true,
      canReadSurface: true,
      canReleaseSurface: true,
      canCloseDocument: true,
      notes: [`PDFium render-core CLI resolved at ${cliPath}.`],
    };
  } catch (error) {
    return {
      backend,
      available: false,
      canOpenDocument: false,
      canGetPageInfo: false,
      canRenderPage: false,
      canReadSurface: false,
      canReleaseSurface: false,
      canCloseDocument: false,
      notes: [error instanceof Error ? error.message : 'Failed to resolve PDFium render-core CLI.'],
    };
  }
}

async function ensurePdfiumBackendReady(): Promise<RenderCoreError | null> {
  if (!isPdfiumBackendSelected()) {
    return renderCoreError(
      'not-implemented',
      'Desktop render-core protocol is only available when BP_DESKTOP_RENDER_BACKEND=pdfium.',
    );
  }

  try {
    await ensurePdfiumCliPath();
    return null;
  } catch (error) {
    return renderCoreError(
      'backend-unavailable',
      error instanceof Error ? error.message : 'Failed to resolve PDFium render-core backend.',
    );
  }
}

function parseCliJson(stdout: string): unknown {
  const trimmed = stdout.trim();
  if (trimmed.length === 0) {
    throw new Error('PDFium render-core CLI returned empty stdout.');
  }

  return JSON.parse(trimmed) as unknown;
}

function parseCliBinaryResponse(stdout: Buffer): { metadata: unknown; bytes: Uint8Array } {
  const newlineIndex = stdout.indexOf(0x0a);
  if (newlineIndex < 0) {
    throw new Error('PDFium render-core CLI returned binary response without a metadata separator.');
  }

  const metadataLine = stdout.subarray(0, newlineIndex).toString('utf8').trim();
  if (metadataLine.length === 0) {
    throw new Error('PDFium render-core CLI returned empty binary response metadata.');
  }

  return {
    metadata: JSON.parse(metadataLine) as unknown,
    bytes: new Uint8Array(stdout.subarray(newlineIndex + 1)),
  };
}

async function runPdfiumCli<T>(args: string[]): Promise<T> {
  const cliPath = await ensurePdfiumCliPath();
  const { spawn } = require('node:child_process') as typeof import('node:child_process');
  const command = normalizeRenderCoreCliCommand(args[0]);
  const startedAt = Date.now();

  return await new Promise<T>((resolve, reject) => {
    let recorded = false;
    const recordOnce = (failed: boolean) => {
      if (recorded) {
        return;
      }

      recorded = true;
      recordRenderCoreCliInvocation(command, Date.now() - startedAt, failed);
    };

    const child = spawn(cliPath, args, {
      cwd: pdfiumRenderCoreDir,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: process.env,
    });

    let stdout = '';
    let stderr = '';

    child.stdout.on('data', (chunk: Buffer | string) => {
      stdout += chunk.toString();
    });

    child.stderr.on('data', (chunk: Buffer | string) => {
      stderr += chunk.toString();
    });

    child.on('error', (error) => {
      recordOnce(true);
      reject(new Error(`Failed to execute PDFium render-core CLI: ${error.message}`));
    });

    child.on('close', (code, signal) => {
      let payload: unknown;

      try {
        payload = parseCliJson(stdout);
      } catch (parseError) {
        recordOnce(true);
        const details = [stdout.trim(), stderr.trim()].filter(Boolean).join('\n');
        reject(
          new Error(
            parseError instanceof Error
              ? `${parseError.message}${details ? `\n${details}` : ''}`
              : `Failed to parse PDFium render-core CLI response.${details ? `\n${details}` : ''}`,
          ),
        );
        return;
      }

      const errorPayload = payload as PdfiumCliErrorPayload;
      if (code !== 0 || signal !== null || typeof errorPayload.error === 'string') {
        recordOnce(true);
        const messageParts = [errorPayload.error, stderr.trim()].filter(Boolean);
        const message = messageParts.join('\n') || `PDFium render-core CLI failed with exit code ${code ?? 'unknown'}.`;
        reject(new Error(signal ? `${message}\nTerminated by signal ${signal}.` : message));
        return;
      }

      recordOnce(false);
      resolve(payload as T);
    });
  });
}

async function runPdfiumCliBinary(args: string[]): Promise<{ metadata: unknown; bytes: Uint8Array }> {
  const cliPath = await ensurePdfiumCliPath();
  const { spawn } = require('node:child_process') as typeof import('node:child_process');
  const command = normalizeRenderCoreCliCommand(args[0]);
  const startedAt = Date.now();

  return await new Promise((resolve, reject) => {
    let recorded = false;
    const recordOnce = (failed: boolean) => {
      if (recorded) {
        return;
      }

      recorded = true;
      recordRenderCoreCliInvocation(command, Date.now() - startedAt, failed);
    };

    const child = spawn(cliPath, args, {
      cwd: pdfiumRenderCoreDir,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: process.env,
    });

    const stdoutChunks: Buffer[] = [];
    let stderr = '';

    child.stdout.on('data', (chunk: Buffer | string) => {
      stdoutChunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
    });

    child.stderr.on('data', (chunk: Buffer | string) => {
      stderr += chunk.toString();
    });

    child.on('error', (error) => {
      recordOnce(true);
      reject(new Error(`Failed to execute PDFium render-core CLI: ${error.message}`));
    });

    child.on('close', (code, signal) => {
      const stdout = Buffer.concat(stdoutChunks);

      if (code !== 0 || signal !== null) {
        recordOnce(true);
        let payload: unknown = null;
        try {
          payload = parseCliJson(stdout.toString('utf8'));
        } catch {
          // Keep the original process failure details when the error payload is absent or malformed.
        }

        const errorPayload = payload as PdfiumCliErrorPayload | null;
        const messageParts = [errorPayload?.error, stderr.trim()].filter(Boolean);
        const message = messageParts.join('\n') || `PDFium render-core CLI failed with exit code ${code ?? 'unknown'}.`;
        reject(new Error(signal ? `${message}\nTerminated by signal ${signal}.` : message));
        return;
      }

      try {
        const parsed = parseCliBinaryResponse(stdout);
        recordOnce(false);
        resolve(parsed);
      } catch (error) {
        recordOnce(true);
        const details = [stdout.subarray(0, 200).toString('utf8').trim(), stderr.trim()].filter(Boolean).join('\n');
        reject(
          new Error(
            error instanceof Error
              ? `${error.message}${details ? `\n${details}` : ''}`
              : `Failed to parse PDFium render-core CLI binary response.${details ? `\n${details}` : ''}`,
          ),
        );
      }
    });
  });
}

class RenderPageWorkerPool {
  private readonly lanes: RenderPageWorkerLane[];
  private readonly queue: RenderPageWorkerJob[] = [];
  private readonly spawn: typeof import('node:child_process').spawn;
  private nextRequestId = 0;
  private closing = false;

  constructor(
    private readonly cliPath: string,
    private readonly cwd: string,
    private readonly size: number,
  ) {
    const childProcess = require('node:child_process') as typeof import('node:child_process');
    this.spawn = childProcess.spawn;
    this.lanes = Array.from({ length: size }, (_value, index) => this.createLane(index));
    renderPageWorkerPoolStats = this.snapshotStats();
  }

  render(request: PdfiumRenderWorkerRequest): Promise<{ metadata: PdfiumRenderWorkerMetadata; bytes: Uint8Array }> {
    if (this.closing) {
      return Promise.reject(new Error('PDFium render-page worker pool is closing.'));
    }

    return new Promise((resolve, reject) => {
      this.queue.push({
        id: `render-${++this.nextRequestId}`,
        request,
        enqueuedAt: Date.now(),
        startedAt: 0,
        resolve,
        reject,
      });
      this.updateStats();
      this.dispatch();
    });
  }

  close(): void {
    this.closing = true;
    while (this.queue.length > 0) {
      const job = this.queue.shift();
      job?.reject(new Error('PDFium render-page worker pool closed before the request started.'));
    }

    for (const lane of this.lanes) {
      lane.currentJob?.reject(new Error('PDFium render-page worker pool closed before the request completed.'));
      lane.currentJob = null;
      lane.closed = true;
      lane.child.kill();
    }
    this.updateStats();
  }

  private createLane(index: number): RenderPageWorkerLane {
    const child = this.spawn(this.cliPath, ['render-worker'], {
      cwd: this.cwd,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: process.env,
    });
    const lane: RenderPageWorkerLane = {
      index,
      child,
      buffer: Buffer.alloc(0),
      metadata: null,
      currentJob: null,
      closed: false,
    };

    child.stdout.on('data', (chunk: Buffer | string) => {
      lane.buffer = Buffer.concat([lane.buffer, Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)]);
      this.consumeLaneOutput(lane);
    });

    child.stderr.on('data', (chunk: Buffer | string) => {
      const text = chunk.toString().trim();
      if (text.length > 0) {
        console.warn(`PDFium render-page worker ${index} stderr: ${text}`);
      }
    });

    child.on('error', (error) => {
      this.failLane(lane, new Error(`PDFium render-page worker ${index} failed: ${error.message}`));
    });

    child.on('exit', (code, signal) => {
      lane.closed = true;
      if (!this.closing) {
        const suffix = signal ? `signal ${signal}` : `code ${code ?? 'unknown'}`;
        this.failLane(lane, new Error(`PDFium render-page worker ${index} exited with ${suffix}.`));
      }
    });

    return lane;
  }

  private dispatch(): void {
    if (this.closing) {
      return;
    }

    for (let index = 0; index < this.lanes.length; index += 1) {
      let lane = this.lanes[index];
      if (lane.closed) {
        lane = this.createLane(index);
        this.lanes[index] = lane;
      }

      if (lane.currentJob || this.queue.length === 0) {
        continue;
      }

      const job = this.queue.shift();
      if (!job) {
        continue;
      }

      lane.currentJob = job;
      lane.metadata = null;
      lane.buffer = Buffer.alloc(0);
      job.startedAt = Date.now();
      this.recordAssignment(job.startedAt - job.enqueuedAt);

      const requestLine = JSON.stringify({
        id: job.id,
        file: job.request.file,
        pageIndex: job.request.pageIndex,
        width: job.request.width,
        height: job.request.height,
        rotation: job.request.rotation,
        renderMode: job.request.renderMode,
        cropPdfRect: job.request.cropPdfRect,
      });

      lane.child.stdin.write(`${requestLine}\n`, (error) => {
        if (error) {
          this.failLane(lane, new Error(`Failed to write PDFium render-page worker request: ${error.message}`));
        }
      });
      this.updateStats();
    }
  }

  private consumeLaneOutput(lane: RenderPageWorkerLane): void {
    while (lane.currentJob) {
      if (!lane.metadata) {
        const newlineIndex = lane.buffer.indexOf(0x0a);
        if (newlineIndex < 0) {
          return;
        }

        const metadataLine = lane.buffer.subarray(0, newlineIndex).toString('utf8').trim();
        lane.buffer = lane.buffer.subarray(newlineIndex + 1);
        let metadata: PdfiumRenderWorkerMetadata;
        try {
          metadata = JSON.parse(metadataLine) as PdfiumRenderWorkerMetadata;
        } catch (error) {
          this.failLane(lane, new Error(error instanceof Error ? error.message : 'Failed to parse worker metadata.'));
          return;
        }

        if (metadata.error) {
          this.finishLane(lane, true, new Error(metadata.error));
          continue;
        }

        lane.metadata = metadata;
      }

      const byteLength = lane.metadata.byteLength;
      if (!Number.isInteger(byteLength) || byteLength < 0) {
        this.failLane(lane, new Error('PDFium render-page worker returned invalid byteLength metadata.'));
        return;
      }

      if (lane.buffer.byteLength < byteLength) {
        return;
      }

      const bytes = new Uint8Array(lane.buffer.subarray(0, byteLength));
      lane.buffer = lane.buffer.subarray(byteLength);
      const metadata = lane.metadata;
      this.finishLane(lane, false, null, { metadata, bytes });
    }
  }

  private finishLane(
    lane: RenderPageWorkerLane,
    failed: boolean,
    error: Error | null,
    value?: { metadata: PdfiumRenderWorkerMetadata; bytes: Uint8Array },
  ): void {
    const job = lane.currentJob;
    if (!job) {
      return;
    }

    recordRenderCoreCliInvocation('render-page', Date.now() - job.startedAt, failed);
    recordRenderPageNativeMetadata(
      value?.metadata,
      failed,
      job.request.renderMode ?? 'full',
      job.request.requestClass,
    );
    lane.currentJob = null;
    lane.metadata = null;

    if (failed) {
      job.reject(error ?? new Error('PDFium render-page worker failed.'));
    } else if (value) {
      job.resolve(value);
    }

    this.updateStats();
    this.dispatch();
  }

  private failLane(lane: RenderPageWorkerLane, error: Error): void {
    const job = lane.currentJob;
    if (job) {
      recordRenderCoreCliInvocation('render-page', Date.now() - job.startedAt, true);
      job.reject(error);
    }

    lane.currentJob = null;
    lane.metadata = null;
    lane.buffer = Buffer.alloc(0);
    lane.closed = true;
    lane.child.kill();
    this.updateStats();
    this.dispatch();
  }

  private recordAssignment(queueWaitMs: number): void {
    const roundedQueueWaitMs = Math.max(0, Math.round(queueWaitMs * 1000) / 1000);
    const stats = this.ensureStats();
    stats.assignments += 1;
    stats.totalQueueWaitMs = Math.round((stats.totalQueueWaitMs + roundedQueueWaitMs) * 1000) / 1000;
    stats.maxQueueWaitMs = Math.max(stats.maxQueueWaitMs, roundedQueueWaitMs);
    stats.lastQueueWaitMs = roundedQueueWaitMs;
  }

  private updateStats(): void {
    const stats = this.ensureStats();
    stats.queued = this.queue.length;
    stats.active = this.lanes.reduce((count, lane) => count + (lane.currentJob ? 1 : 0), 0);
  }

  private ensureStats(): RenderCoreWorkerPoolStats {
    renderPageWorkerPoolStats ??= this.snapshotStats();
    return renderPageWorkerPoolStats;
  }

  private snapshotStats(): RenderCoreWorkerPoolStats {
    return {
      queued: this.queue.length,
      active: this.lanes.reduce((count, lane) => count + (lane.currentJob ? 1 : 0), 0),
      assignments: 0,
      totalQueueWaitMs: 0,
      maxQueueWaitMs: 0,
      lastQueueWaitMs: null,
    };
  }
}

async function getRenderPageWorkerPool(): Promise<RenderPageWorkerPool> {
  if (!renderPageWorkerPoolPromise) {
    renderPageWorkerPoolPromise = ensurePdfiumCliPath().then((cliPath) => (
      new RenderPageWorkerPool(cliPath, pdfiumRenderCoreDir, getRenderPageWorkerPoolSize())
    ));
  }

  return renderPageWorkerPoolPromise;
}

function getRenderPageWorkerPoolSize(): number {
  const parsed = Number.parseInt(process.env.BP_PDFIUM_RENDER_WORKERS ?? '', 10);
  if (Number.isInteger(parsed) && parsed > 0) {
    return Math.min(8, parsed);
  }

  return 3;
}

async function runPdfiumRenderWorker(request: PdfiumRenderWorkerRequest): Promise<{
  metadata: PdfiumRenderWorkerMetadata;
  bytes: Uint8Array;
}> {
  const pool = await getRenderPageWorkerPool();
  return await pool.render(request);
}

function validateDocumentRequest(request: RenderCoreOpenDocumentRequest): RenderCoreError | null {
  if (typeof request.filePath !== 'string' || request.filePath.trim().length === 0) {
    return renderCoreError('invalid-request', 'renderCore.openDocument requires a non-empty filePath.');
  }

  if (request.password != null && request.password.length > 0) {
    return renderCoreError(
      'invalid-request',
      'Password-protected documents are not supported by the PDFium CLI proof path yet.',
    );
  }

  return null;
}

function validatePageIndex(pageIndex: number): boolean {
  return Number.isInteger(pageIndex) && pageIndex >= 0;
}

function validateRenderMode(renderMode: RenderCoreRenderMode | undefined): boolean {
  return renderMode === undefined || renderMode === 'full' || renderMode === 'preview';
}

function validateRenderRequestClass(requestClass: RenderCoreRenderRequestClass | undefined): boolean {
  return requestClass === undefined
    || requestClass === 'target-page-hq'
    || requestClass === 'target-page-crop'
    || requestClass === 'target-page-preview'
    || requestClass === 'visible-page-preview'
    || requestClass === 'visible-page-hq-upgrade'
    || requestClass === 'overview-thumbnail'
    || requestClass === 'visible-thumbnail'
    || requestClass === 'nearby-prefetch'
    || requestClass === 'warming';
}

function validateCropPdfRect(cropPdfRect: RenderCoreRenderPageRequest['cropPdfRect']): boolean {
  if (cropPdfRect === undefined) {
    return true;
  }

  return typeof cropPdfRect.x === 'number'
    && Number.isFinite(cropPdfRect.x)
    && typeof cropPdfRect.y === 'number'
    && Number.isFinite(cropPdfRect.y)
    && typeof cropPdfRect.width === 'number'
    && Number.isFinite(cropPdfRect.width)
    && cropPdfRect.width > 0
    && typeof cropPdfRect.height === 'number'
    && Number.isFinite(cropPdfRect.height)
    && cropPdfRect.height > 0;
}

function validateRotation(rotation: number | undefined): boolean {
  return rotation === undefined || rotation === 0 || rotation === 90 || rotation === 180 || rotation === 270;
}

function getRegisteredDocument(
  documentId: string,
  ownerWebContentsId: number,
): RenderCoreResult<{ ownerWebContentsId: number; filePath: string; pageCount: number }> {
  const document = documentRegistry.get(documentId);
  if (!document || document.ownerWebContentsId !== ownerWebContentsId) {
    return errorResult('not-found', `Unknown render-core documentId: ${documentId}`);
  }

  return successResult(document);
}

function getRegisteredSurface(
  surfaceId: string,
  ownerWebContentsId: number,
): RenderCoreResult<{
  ownerWebContentsId: number;
  documentId: string;
  bytes: Uint8Array;
  pixelWidth: number;
  pixelHeight: number;
}> {
  const surface = surfaceRegistry.get(surfaceId);
  if (!surface || surface.ownerWebContentsId !== ownerWebContentsId) {
    return errorResult('not-found', `Unknown render-core surfaceId: ${surfaceId}`);
  }

  return successResult(surface);
}

async function handleRenderCoreOpenDocument(
  ownerWebContentsId: number,
  request: RenderCoreOpenDocumentRequest,
): Promise<RenderCoreResult<RenderCoreOpenDocumentResponse>> {
  const validationError = validateDocumentRequest(request);
  if (validationError) {
    return { ok: false, error: validationError };
  }

  const backendError = await ensurePdfiumBackendReady();
  if (backendError) {
    return { ok: false, error: backendError };
  }

  try {
    const payload = await runPdfiumCli<PdfiumDocumentInfoPayload>(['document-info', '--file', request.filePath]);
    if (!Number.isInteger(payload.pageCount) || payload.pageCount < 0) {
      return errorResult('process-failed', 'PDFium render-core CLI returned an invalid document-info payload.');
    }

    const documentId = createRuntimeId('doc');
    documentRegistry.set(documentId, {
      ownerWebContentsId,
      filePath: request.filePath,
      pageCount: payload.pageCount,
    });

    return successResult({
      documentId,
      pageCount: payload.pageCount,
      backend: 'pdfium',
    });
  } catch (error) {
    return errorResult('process-failed', error instanceof Error ? error.message : 'Failed to open document via PDFium render-core CLI.');
  }
}

async function handleRenderCoreGetPageInfo(
  ownerWebContentsId: number,
  request: RenderCoreGetPageInfoRequest,
): Promise<RenderCoreResult<RenderCorePageInfo>> {
  const backendError = await ensurePdfiumBackendReady();
  if (backendError) {
    return { ok: false, error: backendError };
  }

  if (typeof request.documentId !== 'string' || request.documentId.length === 0) {
    return errorResult('invalid-request', 'renderCore.getPageInfo requires a non-empty documentId.');
  }

  if (!validatePageIndex(request.pageIndex)) {
    return errorResult('invalid-request', 'renderCore.getPageInfo requires pageIndex >= 0.');
  }

  const registeredDocument = getRegisteredDocument(request.documentId, ownerWebContentsId);
  if (!registeredDocument.ok) {
    return registeredDocument;
  }

  try {
    const payload = await runPdfiumCli<PdfiumPageInfoPayload>([
      'page-info',
      '--file',
      registeredDocument.value.filePath,
      '--page-index',
      String(request.pageIndex),
    ]);

    if (
      typeof payload.width !== 'number' ||
      typeof payload.height !== 'number' ||
      typeof payload.rotation !== 'number'
    ) {
      return errorResult('process-failed', 'PDFium render-core CLI returned an invalid page-info payload.');
    }

    return successResult({
      documentId: request.documentId,
      pageIndex: request.pageIndex,
      width: payload.width,
      height: payload.height,
      rotation: payload.rotation,
    });
  } catch (error) {
    return errorResult('process-failed', error instanceof Error ? error.message : 'Failed to get page info via PDFium render-core CLI.');
  }
}

async function handleRenderCoreRenderPage(
  ownerWebContentsId: number,
  request: RenderCoreRenderPageRequest,
): Promise<RenderCoreResult<RenderCoreRenderPageResponse>> {
  const backendError = await ensurePdfiumBackendReady();
  if (backendError) {
    return { ok: false, error: backendError };
  }

  if (typeof request.documentId !== 'string' || request.documentId.length === 0) {
    return errorResult('invalid-request', 'renderCore.renderPage requires a non-empty documentId.');
  }

  if (!validatePageIndex(request.pageIndex)) {
    return errorResult('invalid-request', 'renderCore.renderPage requires pageIndex >= 0.');
  }

  if (
    typeof request.target?.width !== 'number' ||
    !Number.isFinite(request.target.width) ||
    request.target.width <= 0 ||
    typeof request.target?.height !== 'number' ||
    !Number.isFinite(request.target.height) ||
    request.target.height <= 0 ||
    typeof request.target?.scale !== 'number' ||
    !Number.isFinite(request.target.scale) ||
    request.target.scale <= 0
  ) {
    return errorResult('invalid-request', 'renderCore.renderPage requires a target with positive width, height, and scale values.');
  }

  if (!validateRotation(request.rotation)) {
    return errorResult('invalid-request', 'renderCore.renderPage rotation must be one of 0, 90, 180, or 270.');
  }

  if (!validateRenderMode(request.renderMode)) {
    return errorResult('invalid-request', 'renderCore.renderPage renderMode must be full or preview.');
  }

  if (!validateRenderRequestClass(request.requestClass)) {
    return errorResult('invalid-request', 'renderCore.renderPage requestClass is not recognized.');
  }

  if (!validateCropPdfRect(request.cropPdfRect)) {
    return errorResult('invalid-request', 'renderCore.renderPage cropPdfRect must contain finite positive PDF-space bounds.');
  }

  const registeredDocument = getRegisteredDocument(request.documentId, ownerWebContentsId);
  if (!registeredDocument.ok) {
    return registeredDocument;
  }

  const pixelWidth = Math.max(1, Math.round(request.target.width * request.target.scale));
  const pixelHeight = Math.max(1, Math.round(request.target.height * request.target.scale));
  try {
    const { metadata, bytes } = await runPdfiumRenderWorker({
      file: registeredDocument.value.filePath,
      pageIndex: request.pageIndex,
      width: pixelWidth,
      height: pixelHeight,
      rotation: request.rotation,
      renderMode: request.renderMode ?? 'full',
      requestClass: request.requestClass,
      cropPdfRect: request.cropPdfRect,
    });
    const payload = metadata as PdfiumRenderPagePayload;
    if (
      !Number.isInteger(payload.width) ||
      !Number.isInteger(payload.height) ||
      !Number.isInteger(payload.byteLength) ||
      payload.byteLength < 0
    ) {
      return errorResult('process-failed', 'PDFium render-core CLI returned an invalid render-page payload.');
    }

    if (bytes.byteLength !== payload.byteLength) {
      return errorResult(
        'decode-failed',
        `PDFium render-core CLI returned ${bytes.byteLength} PNG bytes, expected ${payload.byteLength}.`,
      );
    }

    const surfaceId = createRuntimeId('surface');
    surfaceRegistry.set(surfaceId, {
      ownerWebContentsId,
      documentId: request.documentId,
      bytes,
      pixelWidth: payload.width,
      pixelHeight: payload.height,
    });

    return successResult({
      surfaceId,
      pixelWidth: payload.width,
      pixelHeight: payload.height,
      pixelFormat: 'rgba8',
      surfaceByteFormat: 'png',
      byteLength: bytes.byteLength,
    });
  } catch (error) {
    return errorResult('process-failed', error instanceof Error ? error.message : 'Failed to render page via PDFium render-core CLI.');
  }
}

async function handleRenderCoreReadSurface(
  ownerWebContentsId: number,
  request: RenderCoreReadSurfaceRequest,
): Promise<RenderCoreResult<RenderCoreReadSurfaceResponse>> {
  const backendError = await ensurePdfiumBackendReady();
  if (backendError) {
    return { ok: false, error: backendError };
  }

  if (typeof request.surfaceId !== 'string' || request.surfaceId.length === 0) {
    return errorResult('invalid-request', 'renderCore.readSurface requires a non-empty surfaceId.');
  }

  const surface = getRegisteredSurface(request.surfaceId, ownerWebContentsId);
  if (!surface.ok) {
    return surface;
  }

  return successResult({
    surfaceId: request.surfaceId,
    byteFormat: 'png',
    bytes: surface.value.bytes,
    byteLength: surface.value.bytes.byteLength,
  });
}

async function handleRenderCoreReleaseSurface(
  ownerWebContentsId: number,
  request: RenderCoreReleaseSurfaceRequest,
): Promise<RenderCoreResult<null>> {
  const backendError = await ensurePdfiumBackendReady();
  if (backendError) {
    return { ok: false, error: backendError };
  }

  if (typeof request.surfaceId !== 'string' || request.surfaceId.length === 0) {
    return errorResult('invalid-request', 'renderCore.releaseSurface requires a non-empty surfaceId.');
  }

  const surface = getRegisteredSurface(request.surfaceId, ownerWebContentsId);
  if (!surface.ok) {
    return surface;
  }

  surfaceRegistry.delete(request.surfaceId);
  return successResult(null);
}

async function handleRenderCoreCloseDocument(
  ownerWebContentsId: number,
  request: RenderCoreCloseDocumentRequest,
): Promise<RenderCoreResult<null>> {
  const backendError = await ensurePdfiumBackendReady();
  if (backendError) {
    return { ok: false, error: backendError };
  }

  if (typeof request.documentId !== 'string' || request.documentId.length === 0) {
    return errorResult('invalid-request', 'renderCore.closeDocument requires a non-empty documentId.');
  }

  const document = getRegisteredDocument(request.documentId, ownerWebContentsId);
  if (!document.ok) {
    return document;
  }

  documentRegistry.delete(request.documentId);
  for (const [surfaceId, surface] of surfaceRegistry.entries()) {
    if (surface.documentId === request.documentId) {
      surfaceRegistry.delete(surfaceId);
    }
  }

  return successResult(null);
}

interface RendererFailurePageOptions {
  readonly errorCode: number;
  readonly errorDescription: string;
  readonly rendererDevServerUrl?: string;
  readonly rendererHtmlPath: string;
  readonly validatedUrl: string;
}

async function loadRendererFailurePage(
  window: BrowserWindowInstance,
  options: RendererFailurePageOptions,
): Promise<void> {
  if (window.isDestroyed()) {
    return;
  }

  const canLoadFileFallback = options.rendererDevServerUrl && existsSync(options.rendererHtmlPath);
  if (canLoadFileFallback) {
    try {
      await window.loadFile(options.rendererHtmlPath);
      return;
    } catch (error) {
      console.error('Renderer fallback file failed to load', error);
    }
  }

  const detailLines = [
    ['Error code', String(options.errorCode)],
    ['Description', options.errorDescription || 'Unknown load failure'],
    ['URL', options.validatedUrl || options.rendererDevServerUrl || options.rendererHtmlPath],
  ];
  const details = detailLines
    .map(([label, value]) => `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value)}</dd>`)
    .join('');
  const html = `<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Butter Paper failed to load</title>
  <style>
    html, body { height: 100%; margin: 0; }
    body {
      align-items: center;
      background: #f4f4f5;
      color: #18181b;
      display: flex;
      font: 13px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      justify-content: center;
    }
    main {
      background: white;
      border: 1px solid #d4d4d8;
      border-radius: 8px;
      box-shadow: 0 12px 32px rgba(24, 24, 27, 0.12);
      max-width: 560px;
      padding: 24px;
    }
    h1 { font-size: 18px; margin: 0 0 8px; }
    p { color: #52525b; line-height: 1.45; margin: 0 0 16px; }
    dl { display: grid; gap: 8px 16px; grid-template-columns: max-content 1fr; margin: 0; }
    dt { color: #71717a; font-weight: 600; }
    dd { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; margin: 0; overflow-wrap: anywhere; }
  </style>
</head>
<body>
  <main>
    <h1>Butter Paper failed to load its renderer.</h1>
    <p>The app window is not blank. The renderer entry point could not be loaded. If this is a development launch, start the desktop app through the dev command so the Vite renderer server is running.</p>
    <dl>${details}</dl>
  </main>
</body>
</html>`;

  await window.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(html)}`);
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => {
    switch (character) {
      case '&':
        return '&amp;';
      case '<':
        return '&lt;';
      case '>':
        return '&gt;';
      case '"':
        return '&quot;';
      default:
        return '&#39;';
    }
  });
}

export function createMainWindow(): BrowserWindowInstance {
  const testMode = isTestModeEnabled();
  const applicationMetadata = getApplicationMetadata();
  const rendererDevServerUrl =
    process.env.BP_DISABLE_RENDERER_DEV_SERVER === '1'
      ? undefined
      : process.env.BP_RENDERER_DEV_SERVER_URL
        ?? (typeof MAIN_WINDOW_VITE_DEV_SERVER_URL === 'string' ? MAIN_WINDOW_VITE_DEV_SERVER_URL : undefined);
  const rendererName =
    typeof MAIN_WINDOW_VITE_NAME === 'string' && MAIN_WINDOW_VITE_NAME.length > 0 ? MAIN_WINDOW_VITE_NAME : 'main_window';
  const rendererHtmlPath = join(moduleDir, '../renderer', rendererName, 'index.html');
  const themeSnapshot = getThemeSnapshot();
  let window: BrowserWindowInstance;
  try {
    window = new BrowserWindow({
      width: 1280,
      height: 960,
      minWidth: 1100,
      minHeight: 720,
      backgroundColor: getWindowBackgroundColor(themeSnapshot.mode),
      title: applicationMetadata.productName,
      show: testMode,
      webPreferences: {
        preload: preloadPath,
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
      },
    });
  } catch (error) {
    console.error('Failed to construct main window', error);
    throw error;
  }

  if (!testMode) {
    window.once('ready-to-show', () => {
      revealWindow(window);
    });
    setTimeout(() => {
      revealWindow(window);
    }, 2_500).unref();
  }

  window.webContents.on('did-fail-load', (
    _event: ElectronEvent,
    errorCode: number,
    errorDescription: string,
    validatedUrl: string,
    isMainFrame: boolean,
  ) => {
    console.error('Renderer failed to load', { errorCode, errorDescription, validatedUrl, isMainFrame });
    if (isMainFrame) {
      void loadRendererFailurePage(window, {
        errorCode,
        errorDescription,
        rendererDevServerUrl,
        rendererHtmlPath,
        validatedUrl,
      });
    }
  });

  window.webContents.on('render-process-gone', (_event: ElectronEvent, details: RenderProcessGoneDetails) => {
    clearRenderCoreRegistriesForOwner(window.webContents.id);
    console.error('Renderer process gone', details);
  });

  if (rendererDevServerUrl) {
    void window.loadURL(rendererDevServerUrl);
  } else {
    void window.loadFile(rendererHtmlPath);
  }

  const webContentsId = window.webContents.id;
  window.on('closed', () => {
    clearRenderCoreRegistriesForOwner(webContentsId);
    if (mainWindow === window) {
      mainWindow = null;
    }
  });

  mainWindow = window;
  return window;
}

export function registerIpcHandlers(): void {
  ipcMain.handle(ipcChannels.applicationGetMetadata, async () => {
    return getApplicationMetadata();
  });

  ipcMain.handle(ipcChannels.applicationSetDefaultPdfApp, async () => {
    const metadata = getApplicationMetadata();
    return setAsDefaultPdfApp({
      platform: process.platform,
      isPackaged: app.isPackaged,
      productName: metadata.productName,
      packageName: metadata.channel === 'beta' ? 'butter-paper-beta' : 'butter-paper',
      executablePath: app.getPath('exe'),
      resourcesPath: process.resourcesPath,
      openExternal: async (url) => {
        await shell.openExternal(url);
      },
    });
  });

  ipcMain.handle(ipcChannels.applicationTakePendingPdfPaths, async () => {
    return takePendingPdfPaths();
  });

  ipcMain.handle(ipcChannels.updatesGetStatus, async () => {
    return requireUpdaterService().getStatus();
  });

  ipcMain.handle(ipcChannels.updatesSetFrequency, async (_event, frequency: UpdateFrequency) => {
    const service = requireUpdaterService();
    await service.setFrequency(frequency);
    return service.getStatus();
  });

  ipcMain.handle(ipcChannels.updatesCheckNow, async () => {
    const service = requireUpdaterService();
    await service.checkNow();
    return service.getStatus();
  });

  ipcMain.handle(ipcChannels.updatesInstallDownloaded, async () => {
    if (!await requireUpdaterService().installDownloaded()) {
      throw new Error('No downloaded Butter Paper update is ready to install.');
    }
  });

  ipcMain.handle(ipcChannels.updatesSetRestartBlocked, async (_event, blocked: boolean) => {
    if (typeof blocked !== 'boolean') {
      throw new TypeError('Update restart-blocked state must be a boolean.');
    }
    requireUpdaterService().setRestartBlocked(blocked);
  });

  ipcMain.handle(ipcChannels.updatesOpenReleasePage, async () => {
    const availableVersion = requireUpdaterService().getStatus().availableVersion;
    await shell.openExternal(resolveReleasePageUrl(availableVersion));
  });

  ipcMain.handle(ipcChannels.dialogOpenPdf, async () => {
    const window = BrowserWindow.getFocusedWindow();
    const result = window
      ? await dialog.showOpenDialog(window, {
          title: 'Open PDFs',
          properties: ['openFile', 'multiSelections'],
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
        })
      : await dialog.showOpenDialog({
          title: 'Open PDFs',
          properties: ['openFile', 'multiSelections'],
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
        });

    if (result.canceled || result.filePaths.length === 0) {
      return null;
    }

    return result.filePaths;
  });

  ipcMain.handle(ipcChannels.dialogSavePdfAs, async (_event, defaultPath?: string) => {
    const window = BrowserWindow.getFocusedWindow();
    const result = window
      ? await dialog.showSaveDialog(window, {
          title: 'Save PDF As',
          defaultPath: defaultPath ?? 'butter-paper-annotated.pdf',
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
        })
      : await dialog.showSaveDialog({
          title: 'Save PDF As',
          defaultPath: defaultPath ?? 'butter-paper-annotated.pdf',
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
        });

    if (result.canceled || !result.filePath) {
      return null;
    }

    return result.filePath;
  });

  ipcMain.handle(ipcChannels.dialogOpenCanvas, async () => {
    const window = BrowserWindow.getFocusedWindow();
    const result = window
      ? await dialog.showOpenDialog(window, {
          title: 'Open Butter Canvas',
          properties: ['openFile', 'multiSelections'],
          filters: [{ name: 'Butter Canvas', extensions: ['bpc'] }],
        })
      : await dialog.showOpenDialog({
          title: 'Open Butter Canvas',
          properties: ['openFile', 'multiSelections'],
          filters: [{ name: 'Butter Canvas', extensions: ['bpc'] }],
        });

    if (result.canceled || result.filePaths.length === 0) {
      return null;
    }

    return result.filePaths;
  });

  ipcMain.handle(ipcChannels.dialogSaveCanvasAs, async (_event, defaultPath?: string) => {
    const window = BrowserWindow.getFocusedWindow();
    const result = window
      ? await dialog.showSaveDialog(window, {
          title: 'Save Butter Canvas As',
          defaultPath: defaultPath ?? 'untitled-canvas.bpc',
          filters: [{ name: 'Butter Canvas', extensions: ['bpc'] }],
        })
      : await dialog.showSaveDialog({
          title: 'Save Butter Canvas As',
          defaultPath: defaultPath ?? 'untitled-canvas.bpc',
          filters: [{ name: 'Butter Canvas', extensions: ['bpc'] }],
        });

    if (result.canceled || !result.filePath) {
      return null;
    }

    return result.filePath;
  });

  ipcMain.handle(ipcChannels.fileRead, async (_event, filePath: string) => {
    return readBinaryFile(filePath);
  });

  ipcMain.handle(ipcChannels.fileWrite, async (_event, filePath: string, bytes: Uint8Array) => {
    await writeBinaryFile(filePath, bytes);
    return true;
  });

  ipcMain.handle(ipcChannels.canvasReadDocument, async (_event, filePath: string) => {
    return parseButterCanvasDocument(await readTextFile(filePath));
  });

  ipcMain.handle(ipcChannels.canvasWriteDocument, async (_event, filePath: string, document) => {
    await writeTextFile(filePath, serializeButterCanvasDocument(document));
    return true;
  });

  ipcMain.handle(ipcChannels.pdfLoadDocument, async (_event, filePath: string) => {
    return loadDocumentPayload(filePath);
  });

  ipcMain.handle(ipcChannels.pdfGetPageGeometry, async (_event, request) => {
    return loadPageGeometryIndex(request.filePath, request.pageIndex);
  });

  ipcMain.handle(ipcChannels.pdfSaveDocument, async (_event, request) => {
    return saveDocumentPayload(request.sourcePath, request.markups, request.mode, request.targetPath, request.pageScales);
  });

  ipcMain.handle(ipcChannels.renderCoreGetBackendConfig, async () => {
    return getDesktopRenderBackendConfig();
  });

  ipcMain.handle(ipcChannels.renderCoreGetBackendSelection, async () => {
    return getDesktopRenderBackendSelection();
  });

  ipcMain.handle(ipcChannels.renderCoreGetCapabilities, async () => {
    return getDesktopRenderCapabilities();
  });

  ipcMain.handle(ipcChannels.renderCoreGetDiagnostics, async () => {
    return getRenderCoreDiagnostics();
  });

  ipcMain.handle(ipcChannels.renderCoreOpenDocument, async (event, request: RenderCoreOpenDocumentRequest) => {
    return handleRenderCoreOpenDocument(event.sender.id, request);
  });

  ipcMain.handle(ipcChannels.renderCoreGetPageInfo, async (event, request: RenderCoreGetPageInfoRequest) => {
    return handleRenderCoreGetPageInfo(event.sender.id, request);
  });

  ipcMain.handle(ipcChannels.renderCoreRenderPage, async (event, request: RenderCoreRenderPageRequest) => {
    return handleRenderCoreRenderPage(event.sender.id, request);
  });

  ipcMain.handle(ipcChannels.renderCoreReadSurface, async (event, request: RenderCoreReadSurfaceRequest) => {
    return handleRenderCoreReadSurface(event.sender.id, request);
  });

  ipcMain.handle(ipcChannels.renderCoreReleaseSurface, async (event, request: RenderCoreReleaseSurfaceRequest) => {
    return handleRenderCoreReleaseSurface(event.sender.id, request);
  });

  ipcMain.handle(ipcChannels.renderCoreCloseDocument, async (event, request: RenderCoreCloseDocumentRequest) => {
    return handleRenderCoreCloseDocument(event.sender.id, request);
  });

  ipcMain.handle(ipcChannels.themeGetSnapshot, async () => {
    return getThemeSnapshot();
  });

  ipcMain.handle(ipcChannels.testResolveFixture, async (_event, name: string) => {
    if (!isTestModeEnabled()) {
      return null;
    }

    return resolveFixturePath(name);
  });

  ipcMain.handle(ipcChannels.testGetWindowState, async () => {
    if (!isTestModeEnabled()) {
      return null;
    }

    return getFocusedWindowState();
  });

  ipcMain.handle(ipcChannels.testSetWindowBounds, async (_event, bounds) => {
    if (!isTestModeEnabled()) {
      return null;
    }

    return setFocusedWindowBounds(bounds ?? {});
  });
}

export async function bootstrapDesktop(): Promise<void> {
  await app.whenReady();
  updaterService = createUpdaterService();
  registerThemeListener();
  registerIpcHandlers();
  createMainWindow();
  unsubscribeUpdaterStatus = updaterService.subscribe((status) => {
    for (const window of BrowserWindow.getAllWindows()) {
      if (!window.isDestroyed()) {
        window.webContents.send(ipcChannels.updatesStatusChanged, status);
      }
    }
  });
  await updaterService.start();

  app.on('before-quit', () => {
    unsubscribeUpdaterStatus?.();
    unsubscribeUpdaterStatus = null;
    void updaterService?.stop();
    shutdownRenderPageWorkerPool();
    clearRenderCoreRegistries();
  });

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow();
    }
  });
}

function createUpdaterService(): DesktopUpdaterService {
  return new DesktopUpdaterService({
    updater: loadElectronAutoUpdater(),
    isPackaged: app.isPackaged,
    userDataPath: app.getPath('userData'),
    currentVersion: app.getVersion(),
    resourcesPath: process.resourcesPath,
    platform: process.platform,
    buildMetadata: readDesktopPackageMetadata(),
  });
}

function requireUpdaterService(): DesktopUpdaterService {
  if (updaterService == null) {
    throw new Error('Butter Paper updater has not been initialised.');
  }
  return updaterService;
}

function readDesktopPackageMetadata(): unknown {
  try {
    return JSON.parse(readFileSync(join(app.getAppPath(), 'package.json'), 'utf8')) as unknown;
  } catch (error) {
    console.warn('Unable to read packaged update channel metadata.', error);
    return null;
  }
}

function getApplicationMetadata(): ApplicationMetadata {
  return resolveApplicationMetadata(readDesktopPackageMetadata());
}
