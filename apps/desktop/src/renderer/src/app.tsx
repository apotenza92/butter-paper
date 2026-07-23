import { useEffect, useRef, useState } from 'react';
import type { ChangeEvent, DragEvent } from 'react';
import {
  BUTTER_CANVAS_FILE_EXTENSION,
  createButterCanvasDocument,
  type ButterCanvasAsset,
  type ButterCanvasDocument,
  type ButterCanvasTraceSettings,
  type ButterCanvasTraceZone,
  type DocumentModel,
  type Markup,
  type PdfPoint,
  type Rect,
} from '@butter-paper/core';
import { openPdfDocumentFromBytes } from '@butter-paper/pdf/browser';
import { AppMenuBar } from './components/AppMenuBar';
import { ButterCanvasToolbar } from './components/ButterCanvasToolbar';
import { ButterCanvasTracePanel } from './components/ButterCanvasTracePanel';
import { ButterCanvasViewport } from './components/ButterCanvasViewport';
import { DocumentTabBar } from './components/DocumentTabBar';
import { DocumentViewport } from './components/DocumentViewport';
import { LeftRail } from './components/LeftRail';
import { LeftSidebar } from './components/LeftSidebar';
import { PageScaleDialog } from './components/PageScaleDialog';
import { RightRail } from './components/RightRail';
import { RightSidebar } from './components/RightSidebar';
import { UpdateDialog } from './components/UpdateDialog';
import { ViewerToolbar } from './components/ViewerToolbar';
import { useUpdater } from './hooks/useUpdater';
import { LocalPdfSession, type DiagnosticsSnapshot } from './services/documentSession';
import { getPerfSnapshot, initialisePerfTracking, recordComponentRender, recordWindowBounds, resetPerfTracking } from './services/perfTracker';
import { getRenderCoordinatorDiagnostics } from './services/renderCoordinator';
import { useViewerStore, type CadViewOrganisation, type LoadedDocumentState, type SnapSettings } from './state/viewerStore';
import { subscribeToThemeMode } from './theme';
import type { ApplicationMetadata, LoadedDocumentPayload, ScrollMode, ScrollWheelMode, ThemeMode, ToolMode, ViewerDiagnostics, ZoomPreset } from '../../shared/protocol';
import { PDF_TOOL_REGISTRY } from './pdf-tools/toolRegistry';
import { traceImageToMarkups } from './utils/butterCanvasTrace';
import { clampViewerZoom } from './utils/renderZoom';

const INACTIVE_TRIM_DELAY_MS = 5000;
const SPACE_DOUBLE_TAP_MS = 300;
const CANVAS_HISTORY_LIMIT = 50;
const DEFAULT_APPLICATION_METADATA: ApplicationMetadata = {
  channel: 'stable',
  productName: 'Butter Paper',
};
const TOOL_SHORTCUTS = PDF_TOOL_REGISTRY
  .filter((tool) => tool.shortcut)
  .map((tool) => ({ tool: tool.id, shortcut: parseToolShortcut(tool.shortcut ?? '') }));

function clampZoom(zoom: number): number {
  return clampViewerZoom(zoom);
}

function saveDefaultName(fileName: string): string {
  const stripped = fileName.replace(/\.pdf$/i, '');
  return `${stripped || 'butter-paper'}-annotated.pdf`;
}

function saveDefaultCanvasName(fileName: string): string {
  const stripped = fileName.replace(/\.bpc$/i, '');
  return `${stripped || 'untitled-canvas'}${BUTTER_CANVAS_FILE_EXTENSION}`;
}

function normalizeDocumentPath(filePath: string): string {
  const normalized = filePath.trim().replace(/\\+/g, '/');
  return navigator.platform.toLowerCase().includes('mac') || navigator.platform.toLowerCase().includes('win')
    ? normalized.toLowerCase()
    : normalized;
}

function extractPdfPathsFromDataTransfer(dataTransfer: DataTransfer): string[] {
  return Array.from(dataTransfer.files)
    .map((file) => (file as File & { path?: string }).path ?? '')
    .filter((path) => /\.pdf$/i.test(path));
}

function extractCanvasPathsFromDataTransfer(dataTransfer: DataTransfer): string[] {
  return Array.from(dataTransfer.files)
    .map((file) => (file as File & { path?: string }).path ?? '')
    .filter((path) => /\.bpc$/i.test(path));
}

function assetTraceZoneRect(asset: ButterCanvasAsset, zone: ButterCanvasTraceZone | null): Rect {
  const normalizedZone = zone ?? { x: 0, y: 0, width: 1, height: 1 };
  return {
    x: asset.rect.x + normalizedZone.x * asset.rect.width,
    y: asset.rect.y + normalizedZone.y * asset.rect.height,
    width: normalizedZone.width * asset.rect.width,
    height: normalizedZone.height * asset.rect.height,
  };
}

function markupBounds(markup: Markup): Rect | null {
  if ('rect' in markup) {
    return markup.rect;
  }
  if ('start' in markup && 'end' in markup) {
    return rectFromPoints([markup.start, markup.end]);
  }
  if ('points' in markup) {
    return rectFromPoints(markup.points);
  }
  if ('paths' in markup) {
    return rectFromPoints(markup.paths.flat());
  }
  if ('controlPath' in markup) {
    return rectFromPoints(markup.controlPath);
  }
  return null;
}

function rectFromPoints(points: readonly PdfPoint[]): Rect {
  if (points.length === 0) {
    return { x: 0, y: 0, width: 0, height: 0 };
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY,
  };
}

function rectsIntersect(first: Rect, second: Rect): boolean {
  return (
    first.x <= second.x + second.width
    && first.x + first.width >= second.x
    && first.y <= second.y + second.height
    && first.y + first.height >= second.y
  );
}

function isGeneratedTraceMarkupInZone(markup: Markup, idPrefix: string, zoneRect: Rect): boolean {
  if (!markup.id.startsWith(`${idPrefix}-`)) {
    return false;
  }
  const bounds = markupBounds(markup);
  return bounds ? rectsIntersect(bounds, zoneRect) : false;
}

function parseToolShortcut(shortcut: string): { key: string; shift: boolean; alt: boolean } {
  const parts = shortcut.split('+').map((part) => part.trim()).filter(Boolean);
  const key = parts.at(-1) ?? shortcut;
  const modifiers = parts.slice(0, -1).map((part) => part.toLowerCase());
  return {
    key: normalizeShortcutKey(key),
    shift: modifiers.includes('shift'),
    alt: modifiers.includes('alt') || modifiers.includes('option'),
  };
}

function normalizeShortcutKey(key: string): string {
  if (key === ' ' || key.toLowerCase() === 'spacebar') {
    return 'space';
  }
  return key.toLowerCase();
}

function isEditableShortcutTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target.isContentEditable) {
    return true;
  }
  const tagName = target.tagName.toLowerCase();
  return tagName === 'input' || tagName === 'textarea' || tagName === 'select';
}

function isInteractiveShortcutTarget(target: EventTarget | null): boolean {
  if (isEditableShortcutTarget(target)) {
    return true;
  }
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  return Boolean(target.closest([
    'button',
    'a[href]',
    '[role="button"]',
    '[role="combobox"]',
    '[role="checkbox"]',
    '[role="dialog"]',
    '[role="grid"]',
    '[role="listbox"]',
    '[role="menu"]',
    '[role="menuitem"]',
    '[role="menuitemcheckbox"]',
    '[role="menuitemradio"]',
    '[role="option"]',
    '[role="radio"]',
    '[role="slider"]',
    '[role="switch"]',
    '[role="tab"]',
    '[role="textbox"]',
    '[role="tree"]',
  ].join(',')));
}

function toolForKeyboardEvent(event: KeyboardEvent): ToolMode | null {
  if (event.metaKey || event.ctrlKey || isInteractiveShortcutTarget(event.target)) {
    return null;
  }

  const key = normalizeShortcutKey(event.key);
  const shift = event.shiftKey;
  const alt = event.altKey;
  return TOOL_SHORTCUTS.find((entry) => entry.shortcut.key === key && entry.shortcut.shift === shift && entry.shortcut.alt === alt)?.tool ?? null;
}

function emptySessionDiagnostics(): DiagnosticsSnapshot {
  return {
    pageRendererMode: 'raster',
    cadRenderExperiment: window.butterPaper?.environment.cadRenderExperiment ?? null,
    renderCacheEntries: 0,
    renderCacheBytes: 0,
    thumbnailCacheEntries: 0,
    thumbnailCacheBytes: 0,
    pageRenderReady: false,
    thumbnailRenderReady: false,
    firstVisiblePageIndex: null,
    firstVisiblePageReady: false,
    firstVisiblePageReadyAtMs: null,
    firstVisiblePageReadyRequestClass: null,
    firstVisiblePageWarmupStatus: 'idle',
    lastPageRenderError: null,
    lastThumbnailRenderError: null,
    sessionBackendKind: null,
    surfaceTransportKind: null,
    openStageTimings: null,
    inactive: false,
    viewportInMotion: false,
    thumbnailListInMotion: false,
    queuedPageRenders: 0,
    queuedThumbnailRenders: 0,
    inflightPageRenders: 0,
    inflightThumbnailRenders: 0,
    overviewThumbnailConcurrencyLimit: 0,
    overviewThumbnailConcurrencyCeiling: 0,
    overviewThumbnailLastThroughputPerSecond: null,
    overviewThumbnailBestThroughputPerSecond: null,
    snapIndexBuilds: 0,
    snapIndexPrimitiveCount: 0,
    totalSnapIndexBuildMs: 0,
    lastSnapIndexBuildMs: null,
    lastSnapIndexPageIndex: null,
    deepZoomRenderCount: 0,
    lastDeepZoomRenderMs: null,
  };
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener('load', () => {
      if (typeof reader.result === 'string') {
        resolve(reader.result);
        return;
      }
      reject(new Error('Unable to read image file.'));
    });
    reader.addEventListener('error', () => reject(new Error('Unable to read image file.')));
    reader.readAsDataURL(file);
  });
}

async function readFileAsBytes(file: File): Promise<Uint8Array> {
  return new Uint8Array(await file.arrayBuffer());
}

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener('load', () => {
      if (typeof reader.result === 'string') {
        resolve(reader.result);
        return;
      }
      reject(new Error('Unable to read rendered PDF page.'));
    });
    reader.addEventListener('error', () => reject(new Error('Unable to read rendered PDF page.')));
    reader.readAsDataURL(blob);
  });
}

function readImageDimensions(dataUrl: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.addEventListener('load', () => resolve({
      width: image.naturalWidth || image.width,
      height: image.naturalHeight || image.height,
    }));
    image.addEventListener('error', () => reject(new Error('Unable to decode image.')));
    image.src = dataUrl;
  });
}

async function readImageData(dataUrl: string): Promise<ImageData> {
  const image = await new Promise<HTMLImageElement>((resolve, reject) => {
    const element = new Image();
    element.addEventListener('load', () => resolve(element));
    element.addEventListener('error', () => reject(new Error('Unable to decode image for tracing.')));
    element.src = dataUrl;
  });
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, image.naturalWidth || image.width);
  canvas.height = Math.max(1, image.naturalHeight || image.height);
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) {
    throw new Error('Canvas tracing is not available.');
  }
  context.drawImage(image, 0, 0, canvas.width, canvas.height);
  return context.getImageData(0, 0, canvas.width, canvas.height);
}

function parseCanvasPdfPageSelection(selection: string | null, pageCount: number): number[] {
  const normalized = (selection ?? 'all').trim().toLowerCase();
  if (!normalized || normalized === 'all' || normalized === '*') {
    return Array.from({ length: pageCount }, (_value, index) => index);
  }

  const pages = new Set<number>();
  for (const part of normalized.split(',').map((item) => item.trim()).filter(Boolean)) {
    const range = part.match(/^(\d+)\s*-\s*(\d+)$/);
    if (range) {
      const start = Math.max(1, Number(range[1]));
      const end = Math.min(pageCount, Number(range[2]));
      for (let page = Math.min(start, end); page <= Math.max(start, end); page += 1) {
        if (page >= 1 && page <= pageCount) {
          pages.add(page - 1);
        }
      }
      continue;
    }
    const page = Number(part);
    if (Number.isInteger(page) && page >= 1 && page <= pageCount) {
      pages.add(page - 1);
    }
  }

  return [...pages].sort((a, b) => a - b);
}

interface AppProps { initialThemeMode: ThemeMode; }

interface ViewerTabSnapshot {
  zoom: number;
  activeTool: ToolMode;
  leftSidebarOpen: boolean;
  rightSidebarOpen: boolean;
  leftSidebarWidth: number;
  rightSidebarWidth: number;
  scrollMode: ScrollMode;
  continuousScrollWheelMode: ScrollWheelMode;
  singlePageScrollWheelMode: ScrollWheelMode;
  cadScrollWheelMode: ScrollWheelMode;
  pageColumnsEnabled: boolean;
  cadViewOrganisation: CadViewOrganisation;
  pagesPerColumn: number;
  zoomPreset: ZoomPreset;
  currentPage: number;
  visiblePageIndices: number[];
  selectedMarkupIds: string[];
  snapSettings: SnapSettings;
}

interface DocumentTab {
  kind: 'pdf';
  id: string;
  filePath: string;
  normalizedPath: string;
  session: LocalPdfSession;
  document: LoadedDocumentState;
  viewSnapshot: ViewerTabSnapshot;
}

interface CanvasDocumentState {
  filePath: string | null;
  fileName: string;
  document: ButterCanvasDocument;
  dirty: boolean;
}

interface CanvasTraceSession {
  readonly assetId: string;
  readonly imageData: ImageData;
  readonly settings: ButterCanvasTraceSettings;
  readonly previewMarkups: readonly Markup[];
}

interface CanvasTab {
  kind: 'canvas';
  id: string;
  filePath: string | null;
  normalizedPath: string | null;
  document: CanvasDocumentState;
  selectedAssetId: string | null;
  selectedMarkupId: string | null;
  undoStack: readonly ButterCanvasDocument[];
  redoStack: readonly ButterCanvasDocument[];
  viewSnapshot: ViewerTabSnapshot;
}

type WorkspaceTab = DocumentTab | CanvasTab;

function isPdfTab(tab: WorkspaceTab): tab is DocumentTab {
  return tab.kind === 'pdf';
}

interface PageScaleCalibrationPick {
  readonly points: readonly PdfPoint[];
  readonly pageIndex: number | null;
}

let nextTabId = 1;

function initialViewSnapshot(): ViewerTabSnapshot {
  return {
    zoom: 1,
    activeTool: 'select',
    leftSidebarOpen: true,
    rightSidebarOpen: false,
    leftSidebarWidth: 220,
    rightSidebarWidth: 280,
    scrollMode: 'continuous',
    continuousScrollWheelMode: 'scroll',
    singlePageScrollWheelMode: 'zoom',
    cadScrollWheelMode: 'zoom',
    pageColumnsEnabled: false,
    cadViewOrganisation: 'columns',
    pagesPerColumn: 10,
    zoomPreset: 'fit-width',
    currentPage: 0,
    visiblePageIndices: [],
    selectedMarkupIds: [],
    snapSettings: {
      snapToContent: true,
      snapToMarkup: true,
      sensitivityPx: 8,
      snapTargets: ['endpoint', 'midpoint', 'center', 'intersection'],
    },
  };
}

function captureViewSnapshot(): ViewerTabSnapshot {
  const state = useViewerStore.getState();
  return {
    zoom: state.zoom,
    activeTool: state.activeTool,
    leftSidebarOpen: state.leftSidebarOpen,
    rightSidebarOpen: state.rightSidebarOpen,
    leftSidebarWidth: state.leftSidebarWidth,
    rightSidebarWidth: state.rightSidebarWidth,
    scrollMode: state.scrollMode,
    continuousScrollWheelMode: state.continuousScrollWheelMode,
    singlePageScrollWheelMode: state.singlePageScrollWheelMode,
    cadScrollWheelMode: state.cadScrollWheelMode,
    pageColumnsEnabled: state.pageColumnsEnabled,
    cadViewOrganisation: state.cadViewOrganisation,
    pagesPerColumn: state.pagesPerColumn,
    zoomPreset: state.zoomPreset,
    currentPage: state.currentPage,
    visiblePageIndices: [...state.visiblePageIndices],
    selectedMarkupIds: [...state.selectedMarkupIds],
    snapSettings: state.snapSettings,
  };
}

function loadedDocumentState(payload: LoadedDocumentPayload, dirty = false): LoadedDocumentState {
  return { ...payload, dirty };
}

function restoreableTool(tool: ToolMode): ToolMode {
  return tool === 'image' ? 'select' : tool;
}

function scheduleFirstVisibleWarmup(tab: DocumentTab, delayMs: number): void {
  window.setTimeout(() => {
    void tab.session.warmFirstVisiblePage(tab.document.document.pages[0]?.index ?? 0);
  }, delayMs);
}

export function App({ initialThemeMode }: AppProps) {
  recordComponentRender('App');
  const updater = useUpdater();
  const [tabs, setTabs] = useState<WorkspaceTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [applicationMetadata, setApplicationMetadata] = useState<ApplicationMetadata>(DEFAULT_APPLICATION_METADATA);
  const [themeMode, setThemeMode] = useState<ThemeMode>(initialThemeMode);
  const [pageScaleDialogOpen, setPageScaleDialogOpen] = useState(false);
  const [pageScaleDialogInitialMode, setPageScaleDialogInitialMode] = useState<'preset' | 'custom' | 'calibrate'>('preset');
  const [pageScaleCalibrationPoints, setPageScaleCalibrationPoints] = useState<{ pageIndex: number; start: PdfPoint; end: PdfPoint } | null>(null);
  const [pageScaleCalibrationPick, setPageScaleCalibrationPick] = useState<PageScaleCalibrationPick | null>(null);
  const setDocument = useViewerStore((state) => state.setDocument);
  const updateDocument = useViewerStore((state) => state.updateDocument);
  const setZoom = useViewerStore((state) => state.setZoom);
  const setZoomPreset = useViewerStore((state) => state.setZoomPreset);
  const setActiveTool = useViewerStore((state) => state.setActiveTool);
  const setPendingImageAsset = useViewerStore((state) => state.setPendingImageAsset);
  const setScrollMode = useViewerStore((state) => state.setScrollMode);
  const setContinuousScrollWheelMode = useViewerStore((state) => state.setContinuousScrollWheelMode);
  const setSinglePageScrollWheelMode = useViewerStore((state) => state.setSinglePageScrollWheelMode);
  const setCadScrollWheelMode = useViewerStore((state) => state.setCadScrollWheelMode);
  const setPageColumnsEnabled = useViewerStore((state) => state.setPageColumnsEnabled);
  const setCadViewOrganisation = useViewerStore((state) => state.setCadViewOrganisation);
  const setPagesPerColumn = useViewerStore((state) => state.setPagesPerColumn);
  const openLeftSidebar = useViewerStore((state) => state.openLeftSidebar);
  const toggleLeftSidebar = useViewerStore((state) => state.toggleLeftSidebar);
  const collapseRightSidebar = useViewerStore((state) => state.collapseRightSidebar);
  const requestPageScroll = useViewerStore((state) => state.requestPageScroll);
  const setStatusMessage = useViewerStore((state) => state.setStatusMessage);
  const setErrorMessage = useViewerStore((state) => state.setErrorMessage);
  const setSelectedMarkupIds = useViewerStore((state) => state.setSelectedMarkupIds);
  const setCurrentPage = useViewerStore((state) => state.setCurrentPage);
  const activeTool = useViewerStore((state) => state.activeTool);
  const documentState = useViewerStore((state) => state.document);
  const leftSidebarOpen = useViewerStore((state) => state.leftSidebarOpen);
  const rightSidebarOpen = useViewerStore((state) => state.rightSidebarOpen);
  const scrollMode = useViewerStore((state) => state.scrollMode);
  const continuousScrollWheelMode = useViewerStore((state) => state.continuousScrollWheelMode);
  const singlePageScrollWheelMode = useViewerStore((state) => state.singlePageScrollWheelMode);
  const cadScrollWheelMode = useViewerStore((state) => state.cadScrollWheelMode);
  const pageColumnsEnabled = useViewerStore((state) => state.pageColumnsEnabled);
  const cadViewOrganisation = useViewerStore((state) => state.cadViewOrganisation);
  const pagesPerColumn = useViewerStore((state) => state.pagesPerColumn);
  const zoomPreset = useViewerStore((state) => state.zoomPreset);
  const currentPage = useViewerStore((state) => state.currentPage);
  const zoom = useViewerStore((state) => state.zoom);
  const snapSettings = useViewerStore((state) => state.snapSettings);
  const setSnapSettings = useViewerStore((state) => state.setSnapSettings);
  const leftSidebarWidth = useViewerStore((state) => state.leftSidebarWidth);
  const rightSidebarWidth = useViewerStore((state) => state.rightSidebarWidth);
  const setLeftSidebarWidth = useViewerStore((state) => state.setLeftSidebarWidth);
  const setRightSidebarWidth = useViewerStore((state) => state.setRightSidebarWidth);
  const isTestMode = window.butterPaper.environment.testMode;
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? null;
  const activePdfTab = activeTab?.kind === 'pdf' ? activeTab : null;
  const activeCanvasTab = activeTab?.kind === 'canvas' ? activeTab : null;
  const session = activePdfTab?.session ?? null;
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const canvasImageInputRef = useRef<HTMLInputElement | null>(null);
  const canvasPdfInputRef = useRef<HTMLInputElement | null>(null);
  const [selectedCanvasAssetId, setSelectedCanvasAssetId] = useState<string | null>(null);
  const [selectedCanvasMarkupId, setSelectedCanvasMarkupId] = useState<string | null>(null);
  const [canvasTraceSession, setCanvasTraceSession] = useState<CanvasTraceSession | null>(null);
  const [canvasFitRequest, setCanvasFitRequest] = useState(0);
  const defaultSampleOpenedRef = useRef(false);
  const heldPanRestoreToolRef = useRef<ToolMode | null>(null);
  const activeToolRef = useRef<ToolMode>(activeTool);
  const lastSpaceTapAtRef = useRef(0);

  useEffect(() => subscribeToThemeMode(setThemeMode), []);

  useEffect(() => {
    let cancelled = false;
    void window.butterPaper.application.getMetadata()
      .then((metadata) => {
        if (!cancelled) {
          setApplicationMetadata(metadata);
          document.title = metadata.productName;
        }
      })
      .catch((error) => console.error('Unable to load application metadata.', error));
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    activeToolRef.current = activeTool;
  }, [activeTool]);

  useEffect(() => {
    if (!activeCanvasTab || !activeCanvasTab.document.document.assets.some((asset) => asset.id === selectedCanvasAssetId)) {
      handleSelectedCanvasAssetChange(null);
    }
  }, [activeCanvasTab?.id, activeCanvasTab?.document.document.assets, selectedCanvasAssetId]);

  useEffect(() => {
    if (!activeCanvasTab || !activeCanvasTab.document.document.markups.some((markup) => markup.id === selectedCanvasMarkupId)) {
      handleSelectedCanvasMarkupChange(null);
    }
  }, [activeCanvasTab?.id, activeCanvasTab?.document.document.markups, selectedCanvasMarkupId]);

  useEffect(() => {
    if (!activeCanvasTab || !canvasTraceSession) {
      return;
    }
    if (!activeCanvasTab.document.document.assets.some((asset) => asset.id === canvasTraceSession.assetId)) {
      setCanvasTraceSession(null);
    }
  }, [activeCanvasTab?.id, activeCanvasTab?.document.document.assets, canvasTraceSession]);

  useEffect(() => {
    document.documentElement.dataset.testMode = isTestMode ? 'true' : 'false';
    if (isTestMode) { initialisePerfTracking(); resetPerfTracking(); }
    return () => { delete document.documentElement.dataset.testMode; };
  }, [isTestMode]);

  useEffect(() => {
    const defaultSamplePdfPath = window.butterPaper.environment.defaultSamplePdfPath;
    if (defaultSampleOpenedRef.current || isTestMode || tabs.length > 0 || !defaultSamplePdfPath) {
      return;
    }

    defaultSampleOpenedRef.current = true;
    void openDocumentPaths([defaultSamplePdfPath]);
  }, [isTestMode, tabs.length]);

  function activateTab(tabId: string | null, nextTabs: WorkspaceTab[] = tabs): void {
    const currentState = useViewerStore.getState();
    const nextTabsWithSnapshot = nextTabs.map((tab) => {
      if (tab.id !== activeTabId) {
        return tab;
      }
      if (tab.kind === 'canvas') {
        return {
          ...tab,
          selectedAssetId: selectedCanvasAssetId,
          selectedMarkupId: selectedCanvasMarkupId,
          viewSnapshot: captureViewSnapshot(),
        };
      }
      return {
        ...tab,
        document: currentState.document ?? tab.document,
        viewSnapshot: captureViewSnapshot(),
      };
    });
    const nextTab = nextTabsWithSnapshot.find((tab) => tab.id === tabId) ?? null;
    for (const tab of nextTabsWithSnapshot) {
      if (tab.kind !== 'pdf') {
        continue;
      }
      if (tab.id === tabId) tab.session.activate(); else tab.session.deactivate();
    }
    setTabs(nextTabsWithSnapshot);
    setActiveTabId(nextTab?.id ?? null);
    if (!nextTab) {
      setDocument(null);
      setSelectedCanvasAssetId(null);
      setSelectedCanvasMarkupId(null);
      setCanvasTraceSession(null);
      return;
    }
    if (nextTab.kind === 'canvas') {
      setDocument(null);
      setSelectedCanvasAssetId(nextTab.selectedAssetId);
      setSelectedCanvasMarkupId(nextTab.selectedMarkupId);
      useViewerStore.setState({
        ...nextTab.viewSnapshot,
        activeTool: restoreableTool(nextTab.viewSnapshot.activeTool),
        leftSidebarOpen: false,
        pendingImageAsset: null,
        pendingPageScroll: null,
      });
      setStatusMessage(`${nextTab.document.document.title} ready`);
      return;
    }
    setSelectedCanvasAssetId(null);
    setSelectedCanvasMarkupId(null);
    setCanvasTraceSession(null);
    useViewerStore.setState({
      document: nextTab.document,
      ...nextTab.viewSnapshot,
      activeTool: restoreableTool(nextTab.viewSnapshot.activeTool),
      pendingImageAsset: null,
      pendingPageScroll: null,
    });
  }

  async function openDocumentPaths(filePaths: string[]): Promise<void> {
    const candidates = [...new Set(filePaths.map((path) => path.trim()).filter((path) => /\.pdf$/i.test(path)))];
    if (candidates.length === 0) return;
    setErrorMessage(null); setStatusMessage(candidates.length === 1 ? 'Loading document' : `Loading ${candidates.length} documents`);
    const currentTabs = tabs;
    const created: DocumentTab[] = [];
    let duplicateToFocus: string | null = null;
    for (const filePath of candidates) {
      const normalizedPath = normalizeDocumentPath(filePath);
      const existing = [...currentTabs, ...created].find((tab) => tab.kind === 'pdf' && tab.normalizedPath === normalizedPath);
      if (existing) { duplicateToFocus ??= existing.id; continue; }
      const nextSession = new LocalPdfSession(filePath);
      try {
        const payload = await nextSession.open();
        created.push({
          kind: 'pdf',
          id: `tab-${nextTabId++}`,
          filePath: payload.filePath,
          normalizedPath: normalizeDocumentPath(payload.filePath),
          session: nextSession,
          document: loadedDocumentState(payload),
          viewSnapshot: initialViewSnapshot(),
        });
      } catch (error) {
        nextSession.dispose();
        setErrorMessage(error instanceof Error ? error.message : `Failed to open ${filePath}`);
      }
    }
    const nextTabs = [...currentTabs, ...created];
    setTabs(nextTabs);
    const targetId = created[0]?.id ?? duplicateToFocus ?? activeTabId ?? nextTabs[0]?.id ?? null;
    activateTab(targetId, nextTabs);
    created.forEach((tab, index) => {
      if (tab.id !== targetId) {
        scheduleFirstVisibleWarmup(tab, 20 + index * 20);
      }
    });
    setStatusMessage(created.length > 1 ? `Loaded ${created.length} documents` : created.length === 1 ? `Loaded ${created[0].document.fileName}` : 'Focused existing document');
  }

  async function loadDocumentFromPath(filePath: string) { await openDocumentPaths([filePath]); }

  function createCanvasTab(document: ButterCanvasDocument, filePath: string | null = null, dirty = true): CanvasTab {
    const fileName = filePath?.split(/[/\\]/).pop() ?? `${document.title}${BUTTER_CANVAS_FILE_EXTENSION}`;
    return {
      kind: 'canvas',
      id: `tab-${nextTabId++}`,
      filePath,
      normalizedPath: filePath ? normalizeDocumentPath(filePath) : null,
      document: {
        filePath,
        fileName,
        document,
        dirty,
      },
      selectedAssetId: null,
      selectedMarkupId: null,
      undoStack: [],
      redoStack: [],
      viewSnapshot: {
        ...initialViewSnapshot(),
        leftSidebarOpen: false,
        scrollMode: 'continuous',
        continuousScrollWheelMode: 'zoom',
        zoomPreset: 'manual',
      },
    };
  }

  function handleNewCanvas(): void {
    const document = createButterCanvasDocument({
      id: `canvas-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`,
      title: 'Untitled Canvas',
    });
    const nextTab = createCanvasTab(document);
    const nextTabs = [...tabs, nextTab];
    setTabs(nextTabs);
    activateTab(nextTab.id, nextTabs);
    setStatusMessage('Created Butter Canvas');
    setErrorMessage(null);
  }

  async function openCanvasPaths(filePaths: string[]): Promise<void> {
    const candidates = [...new Set(filePaths.map((path) => path.trim()).filter((path) => /\.bpc$/i.test(path)))];
    if (candidates.length === 0) return;
    setErrorMessage(null);
    setStatusMessage(candidates.length === 1 ? 'Loading Butter Canvas' : `Loading ${candidates.length} Butter Canvas files`);
    const created: CanvasTab[] = [];
    let duplicateToFocus: string | null = null;
    for (const filePath of candidates) {
      const normalizedPath = normalizeDocumentPath(filePath);
      const existing = [...tabs, ...created].find((tab) => tab.kind === 'canvas' && tab.normalizedPath === normalizedPath);
      if (existing) {
        duplicateToFocus ??= existing.id;
        continue;
      }
      try {
        const document = await window.butterPaper.canvas.readDocument(filePath);
        created.push(createCanvasTab(document, filePath, false));
      } catch (error) {
        setErrorMessage(error instanceof Error ? error.message : `Failed to open ${filePath}`);
      }
    }
    const nextTabs = [...tabs, ...created];
    setTabs(nextTabs);
    const targetId = created[0]?.id ?? duplicateToFocus ?? activeTabId ?? nextTabs[0]?.id ?? null;
    activateTab(targetId, nextTabs);
    setStatusMessage(created.length === 1 ? `Loaded ${created[0].document.fileName}` : created.length > 1 ? `Loaded ${created.length} canvases` : 'Focused existing canvas');
  }

  async function handleOpenCanvas(): Promise<void> {
    const paths = await window.butterPaper.dialogs.openCanvasDialog();
    if (paths?.length) {
      await openCanvasPaths(paths);
    }
  }

  function updateCanvasTabDocument(tabId: string, document: ButterCanvasDocument, dirty = true): WorkspaceTab[] {
    const nextTabs = tabs.map((tab) => {
      if (tab.kind !== 'canvas' || tab.id !== tabId) {
        return tab;
      }
      return {
        ...tab,
        undoStack: dirty ? [...tab.undoStack, tab.document.document].slice(-CANVAS_HISTORY_LIMIT) : tab.undoStack,
        redoStack: dirty ? [] : tab.redoStack,
        document: {
          ...tab.document,
          document,
          dirty: tab.document.dirty || dirty,
        },
      };
    });
    setTabs(nextTabs);
    return nextTabs;
  }

  function handleCanvasDocumentChange(document: ButterCanvasDocument): void {
    if (!activeCanvasTab) {
      return;
    }
    updateCanvasTabDocument(activeCanvasTab.id, document);
  }

  function handleCanvasUndo(): void {
    if (!activeCanvasTab || activeCanvasTab.undoStack.length === 0) {
      return;
    }
    const previousDocument = activeCanvasTab.undoStack.at(-1);
    if (!previousDocument) {
      return;
    }
    const nextUndoStack = activeCanvasTab.undoStack.slice(0, -1);
    const currentDocument = activeCanvasTab.document.document;
    setTabs((currentTabs) => currentTabs.map((tab) => tab.kind === 'canvas' && tab.id === activeCanvasTab.id
      ? {
          ...tab,
          undoStack: nextUndoStack,
          redoStack: [...tab.redoStack, currentDocument].slice(-CANVAS_HISTORY_LIMIT),
          document: {
            ...tab.document,
            document: previousDocument,
            dirty: true,
          },
        }
      : tab));
    setSelectedCanvasAssetId(null);
    setSelectedCanvasMarkupId(null);
    setCanvasTraceSession(null);
    setStatusMessage('Undid canvas change');
  }

  function handleCanvasRedo(): void {
    if (!activeCanvasTab || activeCanvasTab.redoStack.length === 0) {
      return;
    }
    const nextDocument = activeCanvasTab.redoStack.at(-1);
    if (!nextDocument) {
      return;
    }
    const nextRedoStack = activeCanvasTab.redoStack.slice(0, -1);
    const currentDocument = activeCanvasTab.document.document;
    setTabs((currentTabs) => currentTabs.map((tab) => tab.kind === 'canvas' && tab.id === activeCanvasTab.id
      ? {
          ...tab,
          undoStack: [...tab.undoStack, currentDocument].slice(-CANVAS_HISTORY_LIMIT),
          redoStack: nextRedoStack,
          document: {
            ...tab.document,
            document: nextDocument,
            dirty: true,
          },
        }
      : tab));
    setSelectedCanvasAssetId(null);
    setSelectedCanvasMarkupId(null);
    setCanvasTraceSession(null);
    setStatusMessage('Redid canvas change');
  }

  function handleSelectedCanvasAssetChange(assetId: string | null): void {
    setSelectedCanvasAssetId(assetId);
    setSelectedCanvasMarkupId(null);
    if (!activeCanvasTab) {
      return;
    }
    setTabs((currentTabs) => currentTabs.map((tab) => tab.kind === 'canvas' && tab.id === activeCanvasTab.id
      ? { ...tab, selectedAssetId: assetId, selectedMarkupId: null }
      : tab));
  }

  function handleSelectedCanvasMarkupChange(markupId: string | null): void {
    setSelectedCanvasMarkupId(markupId);
    setSelectedCanvasAssetId(null);
    if (!activeCanvasTab) {
      return;
    }
    setTabs((currentTabs) => currentTabs.map((tab) => tab.kind === 'canvas' && tab.id === activeCanvasTab.id
      ? { ...tab, selectedAssetId: null, selectedMarkupId: markupId }
      : tab));
  }

  function updateActiveCanvasWithUpdater(
    updater: (document: ButterCanvasDocument) => ButterCanvasDocument,
    options: { statusMessage?: string; selectedAssetId?: string | null } = {},
  ): void {
    if (!activeCanvasTab) {
      return;
    }
    const nextDocument = updater(activeCanvasTab.document.document);
    updateCanvasTabDocument(activeCanvasTab.id, nextDocument);
    if (options.selectedAssetId !== undefined) {
      handleSelectedCanvasAssetChange(options.selectedAssetId);
    }
    if (options.statusMessage) {
      setStatusMessage(options.statusMessage);
    }
  }

  function updateActiveCanvasAfterSave(filePath: string, document: ButterCanvasDocument): void {
    if (!activeCanvasTab) {
      return;
    }
    const fileName = filePath.split(/[/\\]/).pop() ?? saveDefaultCanvasName(document.title);
    const nextTabs = tabs.map((tab) => tab.kind === 'canvas' && tab.id === activeCanvasTab.id
      ? {
          ...tab,
          filePath,
          normalizedPath: normalizeDocumentPath(filePath),
          document: {
            filePath,
            fileName,
            document,
            dirty: false,
          },
        }
      : tab);
    setTabs(nextTabs);
  }

  function diagnostics(): ViewerDiagnostics {
    const activeDiagnostics = session?.diagnostics() ?? emptySessionDiagnostics();
    const activeIndex = activeTabId ? tabs.findIndex((tab) => tab.id === activeTabId) : -1;
    return {
      documentPath: activeCanvasTab?.filePath ?? documentState?.filePath ?? null,
      documentName: activeCanvasTab?.document.fileName ?? documentState?.fileName ?? null,
      themeMode,
      pageCount: documentState?.document.pages.length ?? 0,
      zoom,
      zoomPreset,
      scrollMode,
      scrollWheelMode: scrollMode === 'single-page'
        ? singlePageScrollWheelMode
        : (pageColumnsEnabled ? cadScrollWheelMode : continuousScrollWheelMode),
      continuousScrollWheelMode,
      singlePageScrollWheelMode,
      cadScrollWheelMode,
      pageColumnsEnabled,
      cadViewOrganisation,
      pagesPerColumn,
      activeTool,
      currentPage: useViewerStore.getState().currentPage,
      visiblePageIndices: useViewerStore.getState().visiblePageIndices,
      selectedMarkupIds: useViewerStore.getState().selectedMarkupIds,
      leftSidebarOpen, rightSidebarOpen, leftSidebarWidth, rightSidebarWidth,
      markupCount: activeCanvasTab?.document.document.markups.length ?? documentState?.document.markups.length ?? 0,
      renderCacheEntries: activeDiagnostics.renderCacheEntries,
      renderCacheBytes: activeDiagnostics.renderCacheBytes,
      pageRenderReady: activeDiagnostics.pageRenderReady,
      thumbnailRenderReady: activeDiagnostics.thumbnailRenderReady,
      lastPageRenderError: activeDiagnostics.lastPageRenderError,
      lastThumbnailRenderError: activeDiagnostics.lastThumbnailRenderError,
      thumbnailCacheEntries: activeDiagnostics.thumbnailCacheEntries,
      thumbnailCacheBytes: activeDiagnostics.thumbnailCacheBytes,
      sessionBackendKind: activeDiagnostics.sessionBackendKind,
      surfaceTransportKind: activeDiagnostics.surfaceTransportKind,
      openStageTimings: activeDiagnostics.openStageTimings,
      pageRendererMode: activeDiagnostics.pageRendererMode,
      cadRenderExperiment: activeDiagnostics.cadRenderExperiment,
      overviewThumbnailConcurrencyLimit: activeDiagnostics.overviewThumbnailConcurrencyLimit,
      overviewThumbnailConcurrencyCeiling: activeDiagnostics.overviewThumbnailConcurrencyCeiling,
      overviewThumbnailLastThroughputPerSecond: activeDiagnostics.overviewThumbnailLastThroughputPerSecond,
      overviewThumbnailBestThroughputPerSecond: activeDiagnostics.overviewThumbnailBestThroughputPerSecond,
      snapIndexBuilds: activeDiagnostics.snapIndexBuilds,
      snapIndexPrimitiveCount: activeDiagnostics.snapIndexPrimitiveCount,
      totalSnapIndexBuildMs: activeDiagnostics.totalSnapIndexBuildMs,
      lastSnapIndexBuildMs: activeDiagnostics.lastSnapIndexBuildMs,
      lastSnapIndexPageIndex: activeDiagnostics.lastSnapIndexPageIndex,
      deepZoomRenderCount: activeDiagnostics.deepZoomRenderCount,
      lastDeepZoomRenderMs: activeDiagnostics.lastDeepZoomRenderMs,
      renderCoordinator: getRenderCoordinatorDiagnostics(),
      activeTabId,
      activeTabIndex: activeIndex,
      tabs: tabs.map((tab) => ({
        id: tab.id,
        filePath: tab.filePath ?? '',
        fileName: tab.document.fileName,
        dirty: Boolean(tab.document.dirty),
        active: tab.id === activeTabId,
        diagnostics: tab.kind === 'pdf' ? tab.session.diagnostics() : emptySessionDiagnostics(),
      })),
    };
  }

  useEffect(() => { window.__butterPaperDiagnostics = diagnostics; return () => { delete window.__butterPaperDiagnostics; }; });

  useEffect(() => {
    if (!isTestMode) { delete window.__butterPaperTestHooks; return; }
    window.__butterPaperTestHooks = {
      openDocumentPath: loadDocumentFromPath,
      openDocumentPaths,
      createButterCanvas: async () => { handleNewCanvas(); },
      openCanvasPath: async (filePath: string) => { await openCanvasPaths([filePath]); },
      openCanvasPaths,
      importCanvasPdfPath: async (filePath: string, pageSelection = 'all') => {
        const bytes = await window.butterPaper.files.readFile(filePath);
        await importPdfSnapshotsToActiveCanvas(filePath.split(/[/\\]/).pop() ?? filePath, bytes, pageSelection);
      },
      getActiveDocument: () => documentState?.document ?? null,
      getActiveCanvasDocument: () => activeCanvasTab?.document.document ?? null,
      openFixturePdf: async (fixtureName: string) => { const filePath = await window.butterPaper.test?.resolveFixturePath(fixtureName); if (!filePath) throw new Error(`Fixture not found: ${fixtureName}`); await loadDocumentFromPath(filePath); },
      switchToTab: async (indexOrPath: number | string) => { const tab = typeof indexOrPath === 'number' ? tabs[indexOrPath] : tabs.find((candidate) => candidate.normalizedPath === normalizeDocumentPath(indexOrPath)); if (tab) activateTab(tab.id); },
      closeTab: async (indexOrPath: number | string) => { const tab = typeof indexOrPath === 'number' ? tabs[indexOrPath] : tabs.find((candidate) => candidate.normalizedPath === normalizeDocumentPath(indexOrPath)); if (tab) closeTab(tab.id); },
      saveCurrentDocument: async () => { await handleSave(); },
      saveCurrentDocumentAs: async (filePath?: string) => {
        if (filePath && activeCanvasTab) {
          await window.butterPaper.canvas.writeDocument(filePath, activeCanvasTab.document.document);
          updateActiveCanvasAfterSave(filePath, activeCanvasTab.document.document);
          setStatusMessage(`Saved canvas to ${filePath.split(/[/\\]/).pop() ?? filePath}`);
          return;
        }
        if (filePath && session && documentState) {
          const payload = await session.save(documentState.document.markups, filePath, documentState.document.pageScales);
          updateActiveTabAfterSave(payload);
          setStatusMessage(`Saved copy to ${filePath.split(/[/\\]/).pop() ?? filePath}`);
          return;
        }
        await handleSaveAs();
      },
      getDiagnostics: diagnostics,
      getPerfSnapshot: () => getPerfSnapshot(),
      resetPerfSnapshot: () => resetPerfTracking(),
      setSnapSettings,
      setPageColumnsEnabled,
      setCadViewOrganisation,
      setPagesPerColumn,
      setZoom: (nextZoom: number) => {
        setZoomPreset('manual');
        setZoom(clampZoom(nextZoom));
      },
      getWindowState: async () => { const nextState = await window.butterPaper.test?.getWindowState(); if (nextState) recordWindowBounds(nextState.bounds); return nextState ?? null; },
      setWindowBounds: async (bounds) => { const nextState = await window.butterPaper.test?.setWindowBounds(bounds); if (nextState) recordWindowBounds(nextState.bounds); return nextState ?? null; },
    };
    return () => { delete window.__butterPaperTestHooks; };
  });

  useEffect(() => {
    if (!documentState && !activeCanvasTab) setErrorMessage(null);
    setStatusMessage(documentState
      ? `${documentState.document.pages.length} pages ready`
      : activeCanvasTab
        ? `${activeCanvasTab.document.document.title} ready`
        : 'Open a PDF or create a Butter Canvas to begin.');
  }, [activeCanvasTab, documentState, setErrorMessage, setStatusMessage]);

  useEffect(() => {
    const trimTimers = tabs
      .filter((tab): tab is DocumentTab => isPdfTab(tab) && tab.id !== activeTabId)
      .map((tab) => window.setTimeout(() => tab.session.trimInactiveCaches(), INACTIVE_TRIM_DELAY_MS));
    return () => trimTimers.forEach((timer) => window.clearTimeout(timer));
  }, [activeTabId, tabs]);

  async function handleOpen() { const paths = await window.butterPaper.dialogs.openPdfDialog(); if (paths?.length) await openDocumentPaths(paths); }

  function updateActiveTabAfterSave(payload: LoadedDocumentPayload): void {
    const currentDocument = useViewerStore.getState().document?.document;
    const nextDocument = loadedDocumentState({
      ...payload,
      document: currentDocument
        ? {
            ...payload.document,
            pageScales: currentDocument.pageScales,
            scalePresets: currentDocument.scalePresets,
          }
        : payload.document,
    });
    const nextTabs = tabs.map((tab) => tab.kind === 'pdf' && tab.id === activeTabId
      ? { ...tab, filePath: payload.filePath, normalizedPath: normalizeDocumentPath(payload.filePath), document: nextDocument, viewSnapshot: captureViewSnapshot() }
      : tab);
    setTabs(nextTabs);
    useViewerStore.setState({ document: nextDocument });
  }

  async function handleSave() {
    if (activeCanvasTab) {
      if (!activeCanvasTab.filePath) {
        await handleSaveAs();
        return;
      }
      try {
        await window.butterPaper.canvas.writeDocument(activeCanvasTab.filePath, activeCanvasTab.document.document);
        updateActiveCanvasAfterSave(activeCanvasTab.filePath, activeCanvasTab.document.document);
        setStatusMessage(`Saved ${activeCanvasTab.document.fileName}`);
      } catch (error) {
        setErrorMessage(error instanceof Error ? error.message : 'Save failed.');
      }
      return;
    }
    if (!session || !documentState) return;
    try { const payload = await session.save(documentState.document.markups, undefined, documentState.document.pageScales); updateActiveTabAfterSave(payload); setStatusMessage(`Saved ${payload.fileName}`); } catch (error) { setErrorMessage(error instanceof Error ? error.message : 'Save failed.'); }
  }
  async function handleSaveAs() {
    if (activeCanvasTab) {
      const path = await window.butterPaper.dialogs.saveCanvasAsDialog(saveDefaultCanvasName(activeCanvasTab.document.fileName));
      if (!path) return;
      try {
        await window.butterPaper.canvas.writeDocument(path, activeCanvasTab.document.document);
        updateActiveCanvasAfterSave(path, activeCanvasTab.document.document);
        setStatusMessage(`Saved canvas to ${path.split(/[/\\]/).pop() ?? path}`);
      } catch (error) {
        setErrorMessage(error instanceof Error ? error.message : 'Save as failed.');
      }
      return;
    }
    if (!session || !documentState) return;
    const path = await window.butterPaper.dialogs.savePdfAsDialog(saveDefaultName(documentState.fileName)); if (!path) return; try { const payload = await session.save(documentState.document.markups, path, documentState.document.pageScales); updateActiveTabAfterSave(payload); setStatusMessage(`Saved copy to ${path.split(/[/\\]/).pop() ?? path}`); } catch (error) { setErrorMessage(error instanceof Error ? error.message : 'Save as failed.'); }
  }

  function closeTab(tabId: string): void {
    const closingIndex = tabs.findIndex((tab) => tab.id === tabId);
    if (closingIndex < 0) return;
    const closingTab = tabs[closingIndex];
    if (closingTab.kind === 'pdf') {
      closingTab.session.dispose();
    }
    const nextTabs = tabs.filter((tab) => tab.id !== tabId);
    setTabs(nextTabs);
    if (activeTabId === tabId) activateTab(nextTabs[Math.min(closingIndex, nextTabs.length - 1)]?.id ?? null, nextTabs);
  }

  function updateZoom(nextZoom: number) { setZoomPreset('manual'); setZoom(clampZoom(nextZoom)); }
  function handleToolChange(tool: ToolMode) {
    heldPanRestoreToolRef.current = null;
    if (tool === 'image') {
      setActiveTool(tool);
      imageInputRef.current?.click();
      return;
    }

    if (tool === activeTool && rightSidebarOpen) {
      collapseRightSidebar();
      return;
    }
    setActiveTool(tool);
  }

  function handlePageScaleApply(updater: (document: DocumentModel) => DocumentModel, message: string): void {
    updateDocument(updater);
    setStatusMessage(message);
    setErrorMessage(null);
    setPageScaleCalibrationPoints(null);
  }

  function openPageScaleDialog(mode: 'preset' | 'custom' | 'calibrate' = 'preset'): void {
    setPageScaleDialogInitialMode(mode);
    if (mode !== 'calibrate') {
      setPageScaleCalibrationPoints(null);
    }
    setPageScaleDialogOpen(true);
  }

  function startPageScaleCalibrationPick(): void {
    setPageScaleDialogOpen(false);
    setPageScaleCalibrationPick({ points: [], pageIndex: null });
    setStatusMessage('Click the first point of a known distance.');
    setErrorMessage(null);
  }

  function cancelPageScaleCalibrationPick(): void {
    setPageScaleCalibrationPick(null);
    setStatusMessage('Calibration cancelled.');
  }

  function handlePageScaleCalibrationPoint(pageIndex: number, point: PdfPoint): void {
    setCurrentPage(pageIndex);
    setPageScaleCalibrationPick((current) => {
      if (!current || current.points.length >= 2) {
        return current;
      }

      if (current.pageIndex !== null && current.pageIndex !== pageIndex) {
        setErrorMessage('Pick both calibration points on the same page.');
        return current;
      }

      const points = [...current.points, point];
      if (points.length === 1) {
        setStatusMessage('Click the second point of the known distance.');
        return { points, pageIndex };
      }

      setPageScaleCalibrationPoints({ pageIndex, start: points[0], end: points[1] });
      setPageScaleDialogInitialMode('calibrate');
      setPageScaleDialogOpen(true);
      setStatusMessage('Enter the known real-world length for the picked distance.');
      return null;
    });
  }

  function deleteSelectedMarkups(): boolean {
    const ids = useViewerStore.getState().selectedMarkupIds;
    if (ids.length === 0) {
      return false;
    }

    const selectedIds = new Set(ids);
    updateDocument((document) => ({
      ...document,
      markups: document.markups.filter((markup) => !selectedIds.has(markup.id)),
    }));
    setSelectedMarkupIds([]);
    setStatusMessage(ids.length === 1 ? 'Deleted markup' : `Deleted ${ids.length} markups`);
    return true;
  }

  async function handleImageFileChange(event: ChangeEvent<HTMLInputElement>): Promise<void> {
    const file = event.currentTarget.files?.[0] ?? null;
    event.currentTarget.value = '';
    if (!file) {
      setPendingImageAsset(null);
      return;
    }

    if (file.type !== 'image/png' && file.type !== 'image/jpeg') {
      setErrorMessage('Insert Image supports PNG and JPEG files.');
      setPendingImageAsset(null);
      return;
    }

    try {
      const dataUrl = await readFileAsDataUrl(file);
      const dimensions = await readImageDimensions(dataUrl);
      setPendingImageAsset({
        dataUrl,
        mimeType: file.type,
        width: dimensions.width,
        height: dimensions.height,
        fileName: file.name,
      });
      setStatusMessage(`Click the page to place ${file.name}`);
      setErrorMessage(null);
    } catch (error) {
      setPendingImageAsset(null);
      setErrorMessage(error instanceof Error ? error.message : 'Unable to read image.');
    }
  }

  async function handleCanvasImageFileChange(event: ChangeEvent<HTMLInputElement>): Promise<void> {
    const file = event.currentTarget.files?.[0] ?? null;
    event.currentTarget.value = '';
    if (!file || !activeCanvasTab) {
      return;
    }

    if (file.type !== 'image/png' && file.type !== 'image/jpeg') {
      setErrorMessage('Butter Canvas image assets support PNG and JPEG files.');
      return;
    }

    try {
      const dataUrl = await readFileAsDataUrl(file);
      const dimensions = await readImageDimensions(dataUrl);
      const document = activeCanvasTab.document.document;
      const assetWidth = Math.min(dimensions.width, 960);
      const assetHeight = dimensions.height * (assetWidth / Math.max(dimensions.width, 1));
      const asset: ButterCanvasAsset = {
        id: `asset-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`,
        kind: 'image',
        name: file.name,
        rect: {
          x: -assetWidth / 2,
          y: -assetHeight / 2,
          width: assetWidth,
          height: assetHeight,
        },
        dataUrl,
        mimeType: file.type,
        opacity: 1,
        visible: true,
        locked: false,
        source: {
          type: 'image-file',
          fileName: file.name,
        },
      };
      handleCanvasDocumentChange({
        ...document,
        updatedAt: new Date().toISOString(),
        assets: [...document.assets, asset],
      });
      handleSelectedCanvasAssetChange(asset.id);
      setStatusMessage(`Inserted ${file.name}`);
      setErrorMessage(null);
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to read image.');
    }
  }

  async function importPdfSnapshotsToActiveCanvas(
    fileName: string,
    bytes: Uint8Array,
    pageSelection: string | null = null,
  ): Promise<void> {
    let handle: Awaited<ReturnType<typeof openPdfDocumentFromBytes>> | null = null;
    try {
      setStatusMessage(`Rendering ${fileName} for Butter Canvas`);
      handle = await openPdfDocumentFromBytes(bytes);
      const metadata = await handle.getMetadata();
      const pageCount = metadata.pageCount;
      const selection = pageSelection ?? (pageCount > 1
        ? window.prompt(`Import pages from ${fileName}. Use "all", a page number, or ranges like 1,3-5.`, 'all')
        : 'all');
      if (selection === null) {
        setStatusMessage('PDF import cancelled');
        return;
      }
      const pageIndices = parseCanvasPdfPageSelection(selection, pageCount);
      if (pageIndices.length === 0) {
        setErrorMessage('No valid PDF pages selected for import.');
        return;
      }

      const renderedAssets: ButterCanvasAsset[] = [];
      const columns = Math.max(1, Math.ceil(Math.sqrt(pageIndices.length)));
      const spacing = 80;
      const maxAssetWidth = 760;
      const cellWidth = maxAssetWidth + spacing;
      const startX = -((columns - 1) * cellWidth) / 2;
      let maxRowHeight = 0;
      for (const [index, pageIndex] of pageIndices.entries()) {
        const rendered = await handle.renderPageToBlob({
          pageIndex,
          scale: 1.5,
          renderAnnotations: true,
        });
        const dataUrl = await blobToDataUrl(rendered.blob);
        const scale = Math.min(1, maxAssetWidth / Math.max(1, rendered.width));
        const width = Math.max(1, rendered.width * scale);
        const height = Math.max(1, rendered.height * scale);
        const row = Math.floor(index / columns);
        const column = index % columns;
        maxRowHeight = Math.max(maxRowHeight, height);
        renderedAssets.push({
          id: `asset-${Date.now().toString(36)}-${pageIndex}-${Math.random().toString(36).slice(2)}`,
          kind: 'pdf-page-snapshot',
          name: `${fileName} page ${pageIndex + 1}`,
          rect: {
            x: startX + column * cellWidth,
            y: row * (maxRowHeight + spacing),
            width,
            height,
          },
          dataUrl,
          mimeType: 'image/png',
          opacity: 1,
          visible: true,
          locked: false,
          source: {
            type: 'pdf-page',
            fileName,
            pageIndex,
            pageCount,
          },
        });
      }

      updateActiveCanvasWithUpdater((document) => ({
        ...document,
        updatedAt: new Date().toISOString(),
        assets: [...document.assets, ...renderedAssets],
      }), {
        selectedAssetId: renderedAssets.at(0)?.id ?? null,
        statusMessage: `Inserted ${renderedAssets.length} PDF page snapshot${renderedAssets.length === 1 ? '' : 's'}`,
      });
      setErrorMessage(null);
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to import PDF into Butter Canvas.');
    } finally {
      await handle?.close().catch(() => undefined);
    }
  }

  async function handleCanvasPdfFileChange(event: ChangeEvent<HTMLInputElement>): Promise<void> {
    const file = event.currentTarget.files?.[0] ?? null;
    event.currentTarget.value = '';
    if (!file || !activeCanvasTab) {
      return;
    }

    if (file.type !== 'application/pdf' && !/\.pdf$/i.test(file.name)) {
      setErrorMessage('Butter Canvas PDF import supports PDF files.');
      return;
    }

    await importPdfSnapshotsToActiveCanvas(file.name, await readFileAsBytes(file));
  }

  function buildTracePreview(
    asset: ButterCanvasAsset,
    imageData: ImageData,
    settings: ButterCanvasTraceSettings,
  ): readonly Markup[] {
    return traceImageToMarkups({
      image: {
        width: imageData.width,
        height: imageData.height,
        data: imageData.data,
      },
      assetRect: asset.rect,
      sensitivity: settings.sensitivity,
      zone: settings.zone,
      outputMode: settings.outputMode,
      idPrefix: `trace-${asset.id}`,
    });
  }

  async function handleTraceSelectedCanvasAsset(): Promise<void> {
    if (!activeCanvasTab) {
      return;
    }
    const document = activeCanvasTab.document.document;
    const asset = document.assets.find((candidate) => candidate.id === selectedCanvasAssetId)
      ?? document.assets.at(-1)
      ?? null;
    if (!asset) {
      setErrorMessage('Select an image or PDF snapshot to trace.');
      return;
    }

    try {
      setStatusMessage(`Preparing trace for ${asset.name}`);
      const imageData = await readImageData(asset.dataUrl);
      const settings = {
        ...document.traceDefaults,
        zone: document.traceDefaults.zone ?? { x: 0, y: 0, width: 1, height: 1 },
      };
      const previewMarkups = buildTracePreview(asset, imageData, settings);
      setCanvasTraceSession({
        assetId: asset.id,
        imageData,
        settings,
        previewMarkups,
      });
      handleSelectedCanvasAssetChange(asset.id);
      setStatusMessage(previewMarkups.length > 0 ? `Previewing ${previewMarkups.length} trace segment${previewMarkups.length === 1 ? '' : 's'}` : `No traceable lines found in ${asset.name}`);
      setErrorMessage(null);
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to trace selected asset.');
    }
  }

  function handleTraceSettingsChange(settings: ButterCanvasTraceSettings): void {
    if (!activeCanvasTab || !canvasTraceSession) {
      return;
    }
    const asset = activeCanvasTab.document.document.assets.find((candidate) => candidate.id === canvasTraceSession.assetId);
    if (!asset) {
      setCanvasTraceSession(null);
      return;
    }
    setCanvasTraceSession({
      ...canvasTraceSession,
      settings,
      previewMarkups: buildTracePreview(asset, canvasTraceSession.imageData, settings),
    });
  }

  function handleApplyCanvasTrace(): void {
    if (!activeCanvasTab || !canvasTraceSession) {
      return;
    }
    const asset = activeCanvasTab.document.document.assets.find((candidate) => candidate.id === canvasTraceSession.assetId);
    if (!asset) {
      setCanvasTraceSession(null);
      setErrorMessage('The selected trace asset is no longer available.');
      return;
    }
    const idPrefix = `trace-${asset.id}`;
    const previewMarkups = canvasTraceSession.previewMarkups;
    const currentDocument = activeCanvasTab.document.document;
    const clearZoneRect = assetTraceZoneRect(asset, canvasTraceSession.settings.zone);
    const retainedMarkups = canvasTraceSession.settings.clearExistingInZone
      ? currentDocument.markups.filter((markup) => !isGeneratedTraceMarkupInZone(markup, idPrefix, clearZoneRect))
      : currentDocument.markups;
    handleCanvasDocumentChange({
      ...currentDocument,
      updatedAt: new Date().toISOString(),
      traceDefaults: canvasTraceSession.settings,
      markups: [...retainedMarkups, ...previewMarkups],
    });
    setCanvasTraceSession(null);
    handleSelectedCanvasAssetChange(asset.id);
    setStatusMessage(previewMarkups.length > 0 ? `Applied ${previewMarkups.length} trace segment${previewMarkups.length === 1 ? '' : 's'}` : `No traceable lines found in ${asset.name}`);
    setErrorMessage(null);
  }

  function handleCancelCanvasTrace(): void {
    setCanvasTraceSession(null);
    setStatusMessage('Trace cancelled');
  }

  function handleSetCanvasScale(): void {
    if (!activeCanvasTab) {
      return;
    }
    const document = activeCanvasTab.document.document;
    handleCanvasDocumentChange({
      ...document,
      updatedAt: new Date().toISOString(),
      scale: document.scale
        ? null
        : {
            source: 'custom',
            name: '1:100',
            canvasUnits: 'px',
            realUnits: 'm',
            canvasUnitsPerRealUnit: 100,
            precision: { mode: 'decimal', value: 0.01 },
          },
    });
    setStatusMessage(document.scale ? 'Canvas scale cleared' : 'Canvas scale set to 1:100');
  }
  function handleSelectPage(pageIndex: number, source: 'thumbnail' | 'generic' = 'generic', previewUrl: string | null = null) { session?.primePagePreview(pageIndex, previewUrl); session?.setNavigationIntent(pageIndex, 2500, source); setCurrentPage(pageIndex); requestPageScroll(pageIndex); }
  function handleDragOver(event: DragEvent<HTMLDivElement>) { if (Array.from(event.dataTransfer.items).some((item) => item.kind === 'file')) event.preventDefault(); }
  function handleDrop(event: DragEvent<HTMLDivElement>) {
    const pdfPaths = extractPdfPathsFromDataTransfer(event.dataTransfer);
    const canvasPaths = extractCanvasPathsFromDataTransfer(event.dataTransfer);
    if (pdfPaths.length || canvasPaths.length) {
      event.preventDefault();
      if (pdfPaths.length) void openDocumentPaths(pdfPaths);
      if (canvasPaths.length) void openCanvasPaths(canvasPaths);
    }
  }

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const key = normalizeShortcutKey(event.key);
      const isSpacePanShortcut = key === 'space' && !isInteractiveShortcutTarget(event.target);
      if (event.defaultPrevented && !isSpacePanShortcut) {
        return;
      }

      const mod = navigator.platform.toLowerCase().includes('mac') ? event.metaKey : event.ctrlKey;
      if (mod) {
        if (key === 'o') {
          event.preventDefault();
          void handleOpen();
        } else if (key === 's' && event.shiftKey) {
          event.preventDefault();
          void handleSaveAs();
        } else if (key === 's') {
          event.preventDefault();
          void handleSave();
        }
        return;
      }

      if (isSpacePanShortcut) {
        event.preventDefault();
        event.stopPropagation();
        if (event.repeat) {
          return;
        }

        const now = performance.now();
        const isDoubleTap = lastSpaceTapAtRef.current > 0 && now - lastSpaceTapAtRef.current <= SPACE_DOUBLE_TAP_MS;
        if (isDoubleTap) {
          lastSpaceTapAtRef.current = 0;
          heldPanRestoreToolRef.current = null;
          setActiveTool(activeToolRef.current === 'pan' ? 'select' : 'pan');
          return;
        }

        if (activeTool !== 'pan' && heldPanRestoreToolRef.current === null) {
          heldPanRestoreToolRef.current = activeTool;
          setActiveTool('pan');
        }
        return;
      }

      if (event.repeat) {
        return;
      }

      if ((key === 'delete' || key === 'backspace') && !isInteractiveShortcutTarget(event.target)) {
        if (deleteSelectedMarkups()) {
          event.preventDefault();
        }
        return;
      }

      if (key === 'escape' && !isInteractiveShortcutTarget(event.target)) {
        event.preventDefault();
        handleToolChange('select');
        return;
      }

      const shortcutTool = toolForKeyboardEvent(event);
      if (!shortcutTool) {
        return;
      }

      event.preventDefault();
      handleToolChange(shortcutTool);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (normalizeShortcutKey(event.key) !== 'space' || isInteractiveShortcutTarget(event.target)) {
        return;
      }

      event.preventDefault();
      event.stopPropagation();
      const restoreTool = heldPanRestoreToolRef.current;
      lastSpaceTapAtRef.current = performance.now();
      if (!restoreTool) {
        return;
      }

      heldPanRestoreToolRef.current = null;
      setActiveTool(restoreTool);
    };
    window.addEventListener('keydown', onKey, { capture: true });
    window.addEventListener('keyup', onKeyUp, { capture: true });
    return () => {
      window.removeEventListener('keydown', onKey, { capture: true });
      window.removeEventListener('keyup', onKeyUp, { capture: true });
    };
  });

  const pages = documentState?.document.pages ?? [];
  const canSave = Boolean((session && documentState) || activeCanvasTab);
  const hasDirtyDocuments = tabs.some((tab) => tab.document.dirty);
  const viewerControlsDisabled = !documentState && !activeCanvasTab;
  const leftRailDisabled = !documentState;
  const activeTraceAsset = activeCanvasTab && canvasTraceSession
    ? activeCanvasTab.document.document.assets.find((asset) => asset.id === canvasTraceSession.assetId) ?? null
    : null;

  useEffect(() => {
    void window.butterPaper.updates.setRestartBlocked(hasDirtyDocuments)
      .catch((error) => console.error('Unable to update the restart safety state.', error));
  }, [hasDirtyDocuments]);

  return (
    <div className="bp-surface-app bp-text-primary flex h-screen flex-col" data-testid="app-root" onDragOver={handleDragOver} onDrop={handleDrop}>
      <AppMenuBar
        canSave={canSave}
        productName={applicationMetadata.productName}
        updateStatus={updater.status}
        onOpen={() => void handleOpen()}
        onOpenCanvas={() => void handleOpenCanvas()}
        onSave={() => void handleSave()}
        onSaveAs={() => void handleSaveAs()}
        onSetPageScale={() => openPageScaleDialog()}
        onCheckForUpdates={() => void updater.actions.checkNow()}
        onOpenReleasePage={() => void updater.actions.openReleasePage()}
        onUpdateFrequencyChange={(frequency) => void updater.actions.setFrequency(frequency)}
      />
      <DocumentTabBar
        tabs={tabs.map((tab) => ({ id: tab.id, documentName: tab.document.fileName, dirty: Boolean(tab.document.dirty) }))}
        activeTabId={activeTabId}
        onSelectTab={activateTab}
        onCloseTab={closeTab}
        onOpenTab={() => void handleOpen()}
        onNewCanvas={handleNewCanvas}
      />
      <input
        ref={imageInputRef}
        type="file"
        accept="image/png,image/jpeg"
        className="hidden"
        tabIndex={-1}
        data-testid="insert-image-file-input"
        onChange={(event) => { void handleImageFileChange(event); }}
      />
      <input
        ref={canvasImageInputRef}
        type="file"
        accept="image/png,image/jpeg"
        className="hidden"
        tabIndex={-1}
        data-testid="butter-canvas-image-file-input"
        onChange={(event) => { void handleCanvasImageFileChange(event); }}
      />
      <input
        ref={canvasPdfInputRef}
        type="file"
        accept="application/pdf,.pdf"
        className="hidden"
        tabIndex={-1}
        data-testid="butter-canvas-pdf-file-input"
        onChange={(event) => { void handleCanvasPdfFileChange(event); }}
      />
      <main className="min-h-0 flex-1">
        <div className="flex h-full min-h-0" data-testid="workspace-shell">
          <LeftRail active={leftSidebarOpen && Boolean(documentState)} disabled={leftRailDisabled} onToggle={() => toggleLeftSidebar('pages')} />
          {leftSidebarOpen && documentState ? (
            <LeftSidebar
              session={session}
              pages={pages}
              width={leftSidebarWidth}
              onSelectPage={handleSelectPage}
              onWidthChange={setLeftSidebarWidth}
            />
          ) : null}
          <div
            className="flex min-w-0 flex-1 flex-col"
            id="document-tab-panel"
            role={activeTabId ? 'tabpanel' : undefined}
            aria-labelledby={activeTabId ? `document-tab-trigger-${tabs.findIndex((tab) => tab.id === activeTabId)}` : undefined}
          >
            {activeCanvasTab ? (
              <>
                <ButterCanvasToolbar
                  document={activeCanvasTab.document.document}
                  canUndo={activeCanvasTab.undoStack.length > 0}
                  canRedo={activeCanvasTab.redoStack.length > 0}
                  onDocumentChange={handleCanvasDocumentChange}
                  onInsertImage={() => canvasImageInputRef.current?.click()}
                  onInsertPdf={() => canvasPdfInputRef.current?.click()}
                  onTraceImage={() => { void handleTraceSelectedCanvasAsset(); }}
                  onSetScale={handleSetCanvasScale}
                  onZoomIn={() => handleCanvasDocumentChange({
                    ...activeCanvasTab.document.document,
                    camera: {
                      ...activeCanvasTab.document.document.camera,
                      zoom: Math.min(64, activeCanvasTab.document.document.camera.zoom * 1.1),
                    },
                  })}
                  onZoomOut={() => handleCanvasDocumentChange({
                    ...activeCanvasTab.document.document,
                    camera: {
                      ...activeCanvasTab.document.document.camera,
                      zoom: Math.max(0.05, activeCanvasTab.document.document.camera.zoom / 1.1),
                    },
                  })}
                  onFit={() => setCanvasFitRequest((request) => request + 1)}
                  onUndo={handleCanvasUndo}
                  onRedo={handleCanvasRedo}
                />
                <div className="min-h-0 flex-1">
                  <ButterCanvasViewport
                    document={activeCanvasTab.document.document}
                    activeTool={activeTool}
                    selectedAssetId={selectedCanvasAssetId}
                    selectedMarkupId={selectedCanvasMarkupId}
                    fitRequest={canvasFitRequest}
                    tracePreviewMarkups={canvasTraceSession?.previewMarkups}
                    tracePreviewZone={canvasTraceSession?.settings.zone ?? null}
                    onDocumentChange={handleCanvasDocumentChange}
                    onSelectedAssetChange={handleSelectedCanvasAssetChange}
                    onSelectedMarkupChange={handleSelectedCanvasMarkupChange}
                    onOpenDocument={() => canvasPdfInputRef.current?.click()}
                  />
                </div>
              </>
            ) : (
              <>
                <ViewerToolbar disabled={viewerControlsDisabled} zoom={zoom} zoomPreset={zoomPreset} scrollMode={scrollMode} continuousScrollWheelMode={continuousScrollWheelMode} singlePageScrollWheelMode={singlePageScrollWheelMode} cadScrollWheelMode={cadScrollWheelMode} pageColumnsEnabled={pageColumnsEnabled} cadViewOrganisation={cadViewOrganisation} pagesPerColumn={pagesPerColumn} snapSettings={snapSettings} onSnapSettingsChange={setSnapSettings} onFitPage={() => setZoomPreset('fit-page')} onFitWidth={() => setZoomPreset('fit-width')} onScrollModeChange={setScrollMode} onContinuousScrollWheelModeChange={setContinuousScrollWheelMode} onSinglePageScrollWheelModeChange={setSinglePageScrollWheelMode} onCadScrollWheelModeChange={setCadScrollWheelMode} onPageColumnsEnabledChange={setPageColumnsEnabled} onCadViewOrganisationChange={setCadViewOrganisation} onPagesPerColumnChange={setPagesPerColumn} onZoomIn={() => updateZoom(zoom * 1.1)} onZoomOut={() => updateZoom(zoom / 1.1)} onZoomReset={() => updateZoom(1)} onZoomChange={updateZoom} />
                <div className="min-h-0 flex-1">
                  <DocumentViewport session={session} onOpenDocument={() => void handleOpen()} calibrationPick={pageScaleCalibrationPick ? { active: true, pointCount: pageScaleCalibrationPick.points.length } : null} onCalibrationPoint={handlePageScaleCalibrationPoint} onCancelCalibrationPick={cancelPageScaleCalibrationPick} />
                </div>
              </>
            )}
          </div>
          {activeTraceAsset && canvasTraceSession ? (
            <ButterCanvasTracePanel
              asset={activeTraceAsset}
              settings={canvasTraceSession.settings}
              previewCount={canvasTraceSession.previewMarkups.length}
              width={rightSidebarWidth}
              onSettingsChange={handleTraceSettingsChange}
              onApply={handleApplyCanvasTrace}
              onCancel={handleCancelCanvasTrace}
              onWidthChange={setRightSidebarWidth}
            />
          ) : rightSidebarOpen ? <RightSidebar activeTool={activeTool} width={rightSidebarWidth} onWidthChange={setRightSidebarWidth} /> : null}
          <RightRail activeTool={activeTool} disabled={viewerControlsDisabled} onSelectTool={handleToolChange} />
        </div>
      </main>
      {pageScaleDialogOpen && documentState ? (
        <PageScaleDialog
          document={documentState.document}
          currentPage={pageScaleCalibrationPoints?.pageIndex ?? currentPage}
          initialMode={pageScaleDialogInitialMode}
          initialCalibrationPoints={pageScaleCalibrationPoints}
          onRequestCalibrationPick={startPageScaleCalibrationPick}
          onApply={handlePageScaleApply}
          onClose={() => setPageScaleDialogOpen(false)}
        />
      ) : null}
      <UpdateDialog
        hasDirtyDocuments={hasDirtyDocuments}
        productName={applicationMetadata.productName}
        status={updater.status}
        onInstall={() => void updater.actions.installDownloaded()}
        onOpenReleasePage={() => void updater.actions.openReleasePage()}
      />
    </div>
  );
}
