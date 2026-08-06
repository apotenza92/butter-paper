import { useEffect, useRef, useState } from 'react';
import type { ChangeEvent, DragEvent } from 'react';
import {
  rotateDocumentPage,
  type DocumentModel,
  type PageRotationDirection,
  type PdfPoint,
} from '@butter-paper/core';
import { AppMenuBar } from './components/AppMenuBar';
import { FINISH_CLOUD_POLYGON_EVENT } from './components/AnnotationLayer';
import { CanvasContextMenu } from './components/CanvasContextMenu';
import {
  formatBlankPdfSettings,
  loadBlankPdfSettings,
  resolveBlankPdfDimensions,
  saveBlankPdfSettings,
  type BlankPdfSettings,
} from './components/blankPdfSettings';
import { applyTabOrder, DocumentTabBar, resolveActiveTabId } from './components/DocumentTabBar';
import { DocumentViewport } from './components/DocumentViewport';
import { LeftRail } from './components/LeftRail';
import { LeftSidebar } from './components/LeftSidebar';
import { NewBlankPdfDialog } from './components/NewBlankPdfDialog';
import { PageScaleDialog } from './components/PageScaleDialog';
import { RightRail, shouldDispatchToolSelection } from './components/RightRail';
import { RightSidebar } from './components/RightSidebar';
import { UnsavedChangesDialog } from './components/UnsavedChangesDialog';
import { UpdateDialog } from './components/UpdateDialog';
import { ViewerToolbar } from './components/ViewerToolbar';
import { useUpdater } from './hooks/useUpdater';
import { LocalPdfSession, type DiagnosticsSnapshot } from './services/documentSession';
import { getPerfSnapshot, initialisePerfTracking, recordComponentRender, recordWindowBounds, resetPerfTracking } from './services/perfTracker';
import { getRenderCoordinatorDiagnostics } from './services/renderCoordinator';
import {
  DEFAULT_LEFT_SIDEBAR_WIDTH,
  DEFAULT_RIGHT_SIDEBAR_WIDTH,
  useViewerStore,
  type CadViewOrganisation,
  type LeftSidebarPanel,
  type LoadedDocumentState,
  type SnapSettings,
} from './state/viewerStore';
import { subscribeToThemeMode } from './theme';
import type { ApplicationMetadata, BlankPdfCreateRequest, LoadedDocumentPayload, PdfSaveTargetDescriptor, ScrollMode, ScrollWheelMode, ThemeMode, ToolMode, ViewerDiagnostics, ZoomPreset } from '../../shared/protocol';
import { PDF_TOOL_REGISTRY } from './pdf-tools/toolRegistry';
import { clampViewerZoom } from './utils/renderZoom';
import { isEditableShortcutTarget, isToolShortcutBlockedTarget, normalizeShortcutKey, parseToolShortcut, resolveToolShortcut } from './utils/toolShortcuts';
import { saveDocumentsInOrder } from './utils/unsavedDocuments';

const INACTIVE_TRIM_DELAY_MS = 5000;
const SPACE_DOUBLE_TAP_MS = 300;
const DEFAULT_APPLICATION_METADATA: ApplicationMetadata = {
  channel: 'stable',
  productName: 'Butter Paper',
  version: '0.0.0',
  commit: null,
  branch: null,
  dirty: false,
  development: false,
  checkoutId: null,
  statusFingerprint: null,
  windowTitle: 'Butter Paper',
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
  return resolveToolShortcut(TOOL_SHORTCUTS, {
    key: event.key,
    shiftKey: event.shiftKey,
    altKey: event.altKey,
    metaKey: event.metaKey,
    ctrlKey: event.ctrlKey,
    blockedByFocus: isToolShortcutBlockedTarget(event.target),
  });
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

interface AppProps { initialThemeMode: ThemeMode; }

interface ViewerTabSnapshot {
  zoom: number;
  activeTool: ToolMode;
  leftSidebarOpen: boolean;
  leftSidebarPanel: LeftSidebarPanel;
  rightSidebarOpen: boolean;
  leftSidebarWidth: number;
  rightSidebarWidth: number;
  scrollMode: ScrollMode;
  continuousScrollWheelMode: ScrollWheelMode;
  singlePageScrollWheelMode: ScrollWheelMode;
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
  id: string;
  filePath: string;
  normalizedPath: string;
  session: LocalPdfSession;
  document: LoadedDocumentState;
  viewSnapshot: ViewerTabSnapshot;
  temporarySourcePath: string | null;
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
    leftSidebarPanel: 'pages',
    rightSidebarOpen: false,
    leftSidebarWidth: DEFAULT_LEFT_SIDEBAR_WIDTH,
    rightSidebarWidth: DEFAULT_RIGHT_SIDEBAR_WIDTH,
    scrollMode: 'continuous',
    continuousScrollWheelMode: 'scroll',
    singlePageScrollWheelMode: 'zoom',
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
    leftSidebarPanel: state.leftSidebarPanel,
    rightSidebarOpen: state.rightSidebarOpen,
    leftSidebarWidth: state.leftSidebarWidth,
    rightSidebarWidth: state.rightSidebarWidth,
    scrollMode: state.scrollMode,
    continuousScrollWheelMode: state.continuousScrollWheelMode,
    singlePageScrollWheelMode: state.singlePageScrollWheelMode,
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
  const [tabs, setTabs] = useState<DocumentTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [applicationMetadata, setApplicationMetadata] = useState<ApplicationMetadata>(DEFAULT_APPLICATION_METADATA);
  const [themeMode, setThemeMode] = useState<ThemeMode>(initialThemeMode);
  const [pageScaleDialogOpen, setPageScaleDialogOpen] = useState(false);
  const [pageScaleDialogInitialMode, setPageScaleDialogInitialMode] = useState<'preset' | 'custom' | 'calibrate'>('preset');
  const [pageScaleCalibrationPoints, setPageScaleCalibrationPoints] = useState<{ pageIndex: number; start: PdfPoint; end: PdfPoint } | null>(null);
  const [pageScaleCalibrationPick, setPageScaleCalibrationPick] = useState<PageScaleCalibrationPick | null>(null);
  const [blankPdfSettings, setBlankPdfSettings] = useState(() => loadBlankPdfSettings(window.localStorage));
  const [newBlankPdfDialogOpen, setNewBlankPdfDialogOpen] = useState(false);
  const [pendingCloseTabId, setPendingCloseTabId] = useState<string | null>(null);
  const [applicationCloseRequested, setApplicationCloseRequested] = useState(false);
  const [closeActionBusy, setCloseActionBusy] = useState(false);
  const setDocument = useViewerStore((state) => state.setDocument);
  const updateDocument = useViewerStore((state) => state.updateDocument);
  const setZoom = useViewerStore((state) => state.setZoom);
  const setZoomPreset = useViewerStore((state) => state.setZoomPreset);
  const setActiveTool = useViewerStore((state) => state.setActiveTool);
  const resetToSelectionTool = useViewerStore((state) => state.resetToSelectionTool);
  const setPendingImageAsset = useViewerStore((state) => state.setPendingImageAsset);
  const setScrollMode = useViewerStore((state) => state.setScrollMode);
  const setContinuousScrollWheelMode = useViewerStore((state) => state.setContinuousScrollWheelMode);
  const setSinglePageScrollWheelMode = useViewerStore((state) => state.setSinglePageScrollWheelMode);
  const setPageColumnsEnabled = useViewerStore((state) => state.setPageColumnsEnabled);
  const setCadViewOrganisation = useViewerStore((state) => state.setCadViewOrganisation);
  const setPagesPerColumn = useViewerStore((state) => state.setPagesPerColumn);
  const openLeftSidebar = useViewerStore((state) => state.openLeftSidebar);
  const toggleLeftSidebar = useViewerStore((state) => state.toggleLeftSidebar);
  const toggleRightSidebar = useViewerStore((state) => state.toggleRightSidebar);
  const requestPageScroll = useViewerStore((state) => state.requestPageScroll);
  const setStatusMessage = useViewerStore((state) => state.setStatusMessage);
  const setErrorMessage = useViewerStore((state) => state.setErrorMessage);
  const setSelectedMarkupIds = useViewerStore((state) => state.setSelectedMarkupIds);
  const setCurrentPage = useViewerStore((state) => state.setCurrentPage);
  const activeTool = useViewerStore((state) => state.activeTool);
  const documentState = useViewerStore((state) => state.document);
  const documentMutationDisabled = false;
  const leftSidebarOpen = useViewerStore((state) => state.leftSidebarOpen);
  const leftSidebarPanel = useViewerStore((state) => state.leftSidebarPanel);
  const rightSidebarOpen = useViewerStore((state) => state.rightSidebarOpen);
  const scrollMode = useViewerStore((state) => state.scrollMode);
  const continuousScrollWheelMode = useViewerStore((state) => state.continuousScrollWheelMode);
  const singlePageScrollWheelMode = useViewerStore((state) => state.singlePageScrollWheelMode);
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
  const session = activeTab?.session ?? null;
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const defaultSampleOpenedRef = useRef(false);
  const heldPanRestoreToolRef = useRef<ToolMode | null>(null);
  const activeToolRef = useRef<ToolMode>(activeTool);
  const lastSpaceTapAtRef = useRef(0);
  const openDocumentPathsRef = useRef<(filePaths: string[]) => Promise<void>>(async () => {});

  useEffect(() => subscribeToThemeMode(setThemeMode), []);

  useEffect(() => {
    const openPaths = (filePaths: string[]) => {
      void openDocumentPathsRef.current(filePaths).catch((error) => {
        setErrorMessage(error instanceof Error ? error.message : 'Unable to open the selected PDF.');
      });
    };
    const unsubscribe = window.butterPaper.application.onOpenPdfPaths(openPaths);
    void window.butterPaper.application.takePendingPdfPaths().then(openPaths).catch((error) => {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to read pending PDF paths.');
    });
    return unsubscribe;
  }, [setErrorMessage]);

  useEffect(() => {
    let cancelled = false;
    void window.butterPaper.application.getMetadata()
      .then((metadata) => {
        if (!cancelled) {
          setApplicationMetadata(metadata);
          document.title = metadata.windowTitle;
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

  function activateTab(tabId: string | null, nextTabs: DocumentTab[] = tabs): void {
    const currentState = useViewerStore.getState();
    const nextTabsWithSnapshot = nextTabs.map((tab) => {
      if (tab.id !== activeTabId) {
        return tab;
      }
      return {
        ...tab,
        document: currentState.document ?? tab.document,
        viewSnapshot: captureViewSnapshot(),
      };
    });
    const nextTab = nextTabsWithSnapshot.find((tab) => tab.id === tabId) ?? null;
    for (const tab of nextTabsWithSnapshot) {
      if (tab.id === tabId) tab.session.activate(); else tab.session.deactivate();
    }
    setTabs(nextTabsWithSnapshot);
    setActiveTabId(nextTab?.id ?? null);
    if (!nextTab) {
      setDocument(null);
      return;
    }
    useViewerStore.setState({
      document: nextTab.document,
      ...nextTab.viewSnapshot,
      activeTool: restoreableTool(nextTab.viewSnapshot.activeTool),
      postPlacement: null,
      pendingImageAsset: null,
      pendingPageScroll: null,
    });
  }

  useEffect(() => {
    const recoveredTabId = resolveActiveTabId(tabs, activeTabId);
    if (recoveredTabId !== activeTabId || (recoveredTabId !== null && !documentState)) {
      activateTab(recoveredTabId, tabs);
    }
  }, [activeTabId, documentState, tabs]);

  async function openDocumentPaths(filePaths: string[]): Promise<void> {
    const candidates = [...new Set(filePaths.map((path) => path.trim()).filter((path) => /\.pdf$/i.test(path)))];
    if (candidates.length === 0) return;
    setErrorMessage(null); setStatusMessage(candidates.length === 1 ? 'Loading document' : `Loading ${candidates.length} documents`);
    const currentTabs = tabs;
    const created: DocumentTab[] = [];
    let firstOpenError: Error | null = null;
    let duplicateToFocus: string | null = null;
    for (const filePath of candidates) {
      const normalizedPath = normalizeDocumentPath(filePath);
      const existing = [...currentTabs, ...created].find((tab) => tab.normalizedPath === normalizedPath);
      if (existing) { duplicateToFocus ??= existing.id; continue; }
      const nextSession = new LocalPdfSession(filePath);
      try {
        const payload = await nextSession.open();
        created.push({
          id: `tab-${nextTabId++}`,
          filePath: payload.filePath,
          normalizedPath: normalizeDocumentPath(payload.filePath),
          session: nextSession,
          document: loadedDocumentState(payload),
          viewSnapshot: initialViewSnapshot(),
          temporarySourcePath: null,
        });
      } catch (error) {
        nextSession.dispose();
        firstOpenError ??= error instanceof Error ? error : new Error(`Failed to open ${filePath}`);
        setErrorMessage(firstOpenError.message);
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
    if (created.length === 0 && firstOpenError) {
      throw firstOpenError;
    }
  }
  openDocumentPathsRef.current = openDocumentPaths;

  async function loadDocumentFromPath(filePath: string) { await openDocumentPaths([filePath]); }

  async function handleCreateBlankPdf(request: BlankPdfCreateRequest): Promise<void> {
    await window.butterPaper.application.setCloseBlocked(true);
    let temporaryDocument: Awaited<ReturnType<typeof window.butterPaper.pdf.createBlankDocument>> | null = null;
    let nextSession: LocalPdfSession | null = null;
    try {
      temporaryDocument = await window.butterPaper.pdf.createBlankDocument(request);
      nextSession = new LocalPdfSession(temporaryDocument.filePath);
      const payload = await nextSession.open();
      const nextTab: DocumentTab = {
        id: `tab-${nextTabId++}`,
        filePath: payload.filePath,
        normalizedPath: normalizeDocumentPath(payload.filePath),
        session: nextSession,
        document: loadedDocumentState({ ...payload, fileName: temporaryDocument.fileName }, true),
        viewSnapshot: initialViewSnapshot(),
        temporarySourcePath: temporaryDocument.temporarySourcePath,
      };
      const nextTabs = [...tabs, nextTab];
      setTabs(nextTabs);
      activateTab(nextTab.id, nextTabs);
      setStatusMessage(`Created ${temporaryDocument.fileName}`);
      setErrorMessage(null);
    } catch (error) {
      nextSession?.dispose();
      await window.butterPaper.application.setCloseBlocked(
        tabs.some((tab) => documentStateForTab(tab).dirty),
      ).catch(() => undefined);
      throw error;
    }
  }

  async function handleCreateDefaultBlankPdf(): Promise<void> {
    try {
      await handleCreateBlankPdf(resolveBlankPdfDimensions(blankPdfSettings));
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to create a blank PDF.');
    }
  }

  function handleBlankPdfSettingsChange(settings: BlankPdfSettings): void {
    saveBlankPdfSettings(window.localStorage, settings);
    setBlankPdfSettings(settings);
    setErrorMessage(null);
  }

  function diagnostics(): ViewerDiagnostics {
    const activeDiagnostics = session?.diagnostics() ?? emptySessionDiagnostics();
    const activeIndex = activeTabId ? tabs.findIndex((tab) => tab.id === activeTabId) : -1;
    return {
      documentPath: documentState?.filePath ?? null,
      documentName: documentState?.fileName ?? null,
      themeMode,
      pageCount: documentState?.document.pages.length ?? 0,
      zoom,
      zoomPreset,
      scrollMode,
      scrollWheelMode: scrollMode === 'single-page'
        ? singlePageScrollWheelMode
        : (pageColumnsEnabled ? 'zoom' : continuousScrollWheelMode),
      continuousScrollWheelMode,
      singlePageScrollWheelMode,
      pageColumnsEnabled,
      cadViewOrganisation,
      pagesPerColumn,
      activeTool,
      currentPage: useViewerStore.getState().currentPage,
      visiblePageIndices: useViewerStore.getState().visiblePageIndices,
      selectedMarkupIds: useViewerStore.getState().selectedMarkupIds,
      leftSidebarOpen, rightSidebarOpen, leftSidebarWidth, rightSidebarWidth,
      markupCount: documentState?.document.markups.length ?? 0,
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
        filePath: tab.filePath,
        fileName: tab.document.fileName,
        dirty: Boolean(tab.document.dirty),
        active: tab.id === activeTabId,
        diagnostics: tab.session.diagnostics(),
      })),
    };
  }

  useEffect(() => { window.__butterPaperDiagnostics = diagnostics; return () => { delete window.__butterPaperDiagnostics; }; });

  useEffect(() => {
    if (!isTestMode) { delete window.__butterPaperTestHooks; return; }
    window.__butterPaperTestHooks = {
      openDocumentPath: async (filePath: string) => {
        const authorizedPath = await window.butterPaper.test?.authorizePdfSource(filePath);
        if (!authorizedPath) throw new Error('Test PDF source authorization is unavailable.');
        await loadDocumentFromPath(authorizedPath);
      },
      openDocumentPaths: async (filePaths: string[]) => {
        const authorizedPaths = await Promise.all(filePaths.map(async (filePath) => {
          const authorizedPath = await window.butterPaper.test?.authorizePdfSource(filePath);
          if (!authorizedPath) throw new Error('Test PDF source authorization is unavailable.');
          return authorizedPath;
        }));
        await openDocumentPaths(authorizedPaths);
      },
      createBlankPdf: handleCreateBlankPdf,
      getActiveDocument: () => documentState?.document ?? null,
      openFixturePdf: async (fixtureName: string) => { const filePath = await window.butterPaper.test?.resolveFixturePath(fixtureName.endsWith('.pdf') ? fixtureName : `${fixtureName}.pdf`); if (!filePath) throw new Error(`Fixture not found: ${fixtureName}`); await loadDocumentFromPath(filePath); },
      switchToTab: async (indexOrPath: number | string) => { const tab = typeof indexOrPath === 'number' ? tabs[indexOrPath] : tabs.find((candidate) => candidate.normalizedPath === normalizeDocumentPath(indexOrPath)); if (tab) activateTab(tab.id); },
      closeTab: async (indexOrPath: number | string) => { const tab = typeof indexOrPath === 'number' ? tabs[indexOrPath] : tabs.find((candidate) => candidate.normalizedPath === normalizeDocumentPath(indexOrPath)); if (tab) requestCloseTab(tab.id); },
      saveCurrentDocument: async () => { await handleSave(); },
      saveCurrentDocumentAs: async (filePath: string) => {
        const target = await window.butterPaper.test?.authorizePdfSaveTarget(filePath);
        if (!target) throw new Error('Test PDF target authorization is unavailable.');
        await saveTab(activeTab, target);
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
    if (!documentState) setErrorMessage(null);
    setStatusMessage(documentState
      ? `${documentState.document.pages.length} pages ready`
      : 'Open a PDF or create a blank PDF to begin.');
  }, [documentState, setErrorMessage, setStatusMessage]);

  useEffect(() => {
    const trimTimers = tabs
      .filter((tab) => tab.id !== activeTabId)
      .map((tab) => window.setTimeout(() => tab.session.trimInactiveCaches(), INACTIVE_TRIM_DELAY_MS));
    return () => trimTimers.forEach((timer) => window.clearTimeout(timer));
  }, [activeTabId, tabs]);

  async function handleOpen() { const paths = await window.butterPaper.dialogs.openPdfDialog(); if (paths?.length) await openDocumentPaths(paths); }

  async function handleSetAsDefaultPdfApp(): Promise<void> {
    try {
      const result = await window.butterPaper.application.setAsDefaultPdfApp();
      setErrorMessage(null);
      setStatusMessage(result.message);
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to change the default PDF app.');
    }
  }

  function documentStateForTab(tab: DocumentTab): LoadedDocumentState {
    return tab.id === activeTabId ? useViewerStore.getState().document ?? tab.document : tab.document;
  }

  async function saveTab(tab: DocumentTab | null, explicitTarget?: PdfSaveTargetDescriptor): Promise<boolean> {
    if (!tab) return false;
    const sourceDocument = documentStateForTab(tab);
    const target = explicitTarget ?? await window.butterPaper.dialogs.savePdfAsDialog(
      tab.temporarySourcePath ? sourceDocument.fileName : saveDefaultName(sourceDocument.fileName),
    ) ?? undefined;
    if (!target) return false;

    const payload = await tab.session.save(
      sourceDocument.document.markups,
      target,
      sourceDocument.document.pageScales,
      sourceDocument.document.pages,
    );
    const nextDocument = loadedDocumentState({
      ...payload,
      document: {
        ...payload.document,
        pageScales: sourceDocument.document.pageScales,
        scalePresets: sourceDocument.document.scalePresets,
      },
    });
    setTabs((currentTabs) => currentTabs.map((candidate) => candidate.id === tab.id
      ? {
          ...candidate,
          filePath: payload.filePath,
          normalizedPath: normalizeDocumentPath(payload.filePath),
          document: nextDocument,
          viewSnapshot: candidate.id === activeTabId ? captureViewSnapshot() : candidate.viewSnapshot,
          temporarySourcePath: null,
        }
      : candidate));
    if (tab.id === activeTabId) {
      useViewerStore.setState({ document: nextDocument });
    }
    setStatusMessage(`Saved new PDF ${payload.fileName}; the original was preserved.`);
    setErrorMessage(null);
    return true;
  }

  async function handleSave(): Promise<void> {
    if (!activeTab) return;
    try {
      await saveTab(activeTab);
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Save failed.');
    }
  }

  async function handleSaveAs(): Promise<void> {
    if (!activeTab) return;
    const sourceDocument = documentStateForTab(activeTab);
    const defaultName = activeTab.temporarySourcePath ? sourceDocument.fileName : saveDefaultName(sourceDocument.fileName);
    const target = await window.butterPaper.dialogs.savePdfAsDialog(defaultName);
    if (!target) return;
    try {
      await saveTab(activeTab, target);
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Save as failed.');
    }
  }

  async function disposeAndRemoveTab(tabId: string, releaseTemporarySource = true): Promise<void> {
    const closingIndex = tabs.findIndex((tab) => tab.id === tabId);
    if (closingIndex < 0) return;
    const closingTab = tabs[closingIndex];
    closingTab.session.dispose();
    const nextTabs = tabs.filter((tab) => tab.id !== tabId);
    setTabs(nextTabs);
    if (activeTabId === tabId) activateTab(nextTabs[Math.min(closingIndex, nextTabs.length - 1)]?.id ?? null, nextTabs);
  }

  function requestCloseTab(tabId: string): void {
    const tab = tabs.find((candidate) => candidate.id === tabId);
    if (!tab) return;
    if (documentStateForTab(tab).dirty) {
      setPendingCloseTabId(tabId);
      return;
    }
    void disposeAndRemoveTab(tabId);
  }

  async function savePendingCloseTab(): Promise<void> {
    const tab = tabs.find((candidate) => candidate.id === pendingCloseTabId);
    if (!tab) {
      setPendingCloseTabId(null);
      return;
    }
    setCloseActionBusy(true);
    try {
      if (await saveTab(tab)) {
        setPendingCloseTabId(null);
        await disposeAndRemoveTab(tab.id, false);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Save failed.');
    } finally {
      setCloseActionBusy(false);
    }
  }

  async function saveAllAndCloseApplication(): Promise<void> {
    setCloseActionBusy(true);
    try {
      const dirtyTabs = tabs.filter((tab) => documentStateForTab(tab).dirty);
      if (!await saveDocumentsInOrder(dirtyTabs, saveTab)) {
        setApplicationCloseRequested(false);
        await window.butterPaper.application.cancelClose();
        return;
      }
      setApplicationCloseRequested(false);
      await window.butterPaper.application.confirmClose();
    } catch (error) {
      setApplicationCloseRequested(false);
      await window.butterPaper.application.cancelClose().catch(() => undefined);
      setErrorMessage(error instanceof Error ? error.message : 'Unable to save all PDFs.');
    } finally {
      setCloseActionBusy(false);
    }
  }

  async function discardAllAndCloseApplication(): Promise<void> {
    setCloseActionBusy(true);
    try {
      await Promise.all(tabs.map(async (tab) => {
        tab.session.dispose();
      }));
      setApplicationCloseRequested(false);
      await window.butterPaper.application.confirmClose();
    } finally {
      setCloseActionBusy(false);
    }
  }

  function updateZoom(nextZoom: number) { setZoomPreset('manual'); setZoom(clampZoom(nextZoom)); }
  function handleToolChange(tool: ToolMode, clickCount = 1) {
    if (!shouldDispatchToolSelection(clickCount)) {
      return;
    }
    heldPanRestoreToolRef.current = null;
    if (tool === 'image') {
      setActiveTool(tool);
      imageInputRef.current?.click();
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

  function openPageScaleDialogForPage(pageIndex: number): void {
    setCurrentPage(pageIndex);
    openPageScaleDialog();
  }

  function handleRotatePage(pageIndex: number, direction: PageRotationDirection): void {
    updateDocument((document) => rotateDocumentPage(document, pageIndex, direction));
    setCurrentPage(pageIndex);
    setStatusMessage(`Rotated page ${pageIndex + 1} ${direction}`);
    setErrorMessage(null);
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

  function handleSelectPage(pageIndex: number, source: 'thumbnail' | 'generic' = 'generic', previewUrl: string | null = null) { session?.primePagePreview(pageIndex, previewUrl); session?.setNavigationIntent(pageIndex, 2500, source); setCurrentPage(pageIndex); requestPageScroll(pageIndex); }
  function handleDragOver(event: DragEvent<HTMLDivElement>) { if (Array.from(event.dataTransfer.items).some((item) => item.kind === 'file')) event.preventDefault(); }
  function handleDrop(event: DragEvent<HTMLDivElement>) {
    const pdfPaths = extractPdfPathsFromDataTransfer(event.dataTransfer);
    if (pdfPaths.length) {
      event.preventDefault();
      void openDocumentPaths(pdfPaths);
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
        if (key === 'n') {
          event.preventDefault();
          setNewBlankPdfDialogOpen(true);
        } else if (key === 'o') {
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

      if (key === 'escape' && !isToolShortcutBlockedTarget(event.target)) {
        event.preventDefault();
        const finishCloudPolygon = new Event(FINISH_CLOUD_POLYGON_EVENT, { cancelable: true });
        window.dispatchEvent(finishCloudPolygon);
        heldPanRestoreToolRef.current = null;
        resetToSelectionTool();
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
  const canSave = Boolean(session && documentState);
  const hasDirtyDocuments = tabs.some((tab) => (
    tab.id === activeTabId && documentState ? documentState.dirty : tab.document.dirty
  ));
  const dirtyDocumentCount = tabs.filter((tab) => (
    tab.id === activeTabId && documentState ? documentState.dirty : tab.document.dirty
  )).length;
  const viewerControlsDisabled = !documentState;
  const leftRailDisabled = !documentState;

  useEffect(() => {
    if (!activeTabId || !documentState) return;
    setTabs((currentTabs) => currentTabs.map((tab) => tab.id === activeTabId && tab.document !== documentState
      ? { ...tab, document: documentState }
      : tab));
  }, [activeTabId, documentState]);

  useEffect(() => {
    void Promise.all([
      window.butterPaper.updates.setRestartBlocked(hasDirtyDocuments),
      window.butterPaper.application.setCloseBlocked(hasDirtyDocuments),
    ]).catch((error) => console.error('Unable to update document safety state.', error));
  }, [hasDirtyDocuments]);

  useEffect(() => window.butterPaper.application.onCloseRequested(() => {
    if (tabs.some((tab) => documentStateForTab(tab).dirty)) {
      setApplicationCloseRequested(true);
      return;
    }
    void window.butterPaper.application.confirmClose();
  }), [activeTabId, documentState, tabs]);

  return (
    <div className="flex h-screen flex-col bg-background text-foreground" data-testid="app-root" onDragOver={handleDragOver} onDrop={handleDrop}>
      <AppMenuBar
        canSave={canSave}
        productName={applicationMetadata.productName}
        updateStatus={updater.status}
        onNewPdf={() => setNewBlankPdfDialogOpen(true)}
        onOpen={() => void handleOpen()}
        onSave={() => void handleSave()}
        onSaveAs={() => void handleSaveAs()}
        onSetAsDefaultPdfApp={() => void handleSetAsDefaultPdfApp()}
        onCheckForUpdates={() => void updater.actions.checkNow()}
        onOpenReleasePage={() => void updater.actions.openReleasePage()}
        onUpdateFrequencyChange={(frequency) => void updater.actions.setFrequency(frequency)}
        onQuit={() => void window.butterPaper.application.requestQuit().catch((error) => {
          console.error('Unable to quit Butter Paper.', error);
        })}
      />
      <DocumentTabBar
        tabs={tabs.map((tab) => ({ id: tab.id, documentName: tab.document.fileName, dirty: Boolean(tab.document.dirty) }))}
        activeTabId={activeTabId}
        onSelectTab={activateTab}
        onCloseTab={requestCloseTab}
        onReorderTabs={(orderedTabIds) => setTabs((currentTabs) => applyTabOrder(currentTabs, orderedTabIds))}
        onOpenTab={() => void handleOpen()}
        onNewPdf={() => void handleCreateDefaultBlankPdf()}
        onBlankPdfSettingsChange={handleBlankPdfSettingsChange}
        blankPdfSettings={blankPdfSettings}
        blankPdfDefaultLabel={formatBlankPdfSettings(blankPdfSettings)}
      />
      <input
        ref={imageInputRef}
        type="file"
        accept="image/png,image/jpeg"
        className="hidden"
        tabIndex={-1}
        data-testid="insert-image-file-input"
        disabled={documentMutationDisabled}
        onChange={(event) => { void handleImageFileChange(event); }}
      />
      <main className="min-h-0 flex-1">
        <div className="flex h-full min-h-0" data-testid="workspace-shell">
          <LeftRail
            activePanel={leftSidebarOpen && documentState ? leftSidebarPanel : null}
            disabled={leftRailDisabled}
            onToggle={toggleLeftSidebar}
          />
          {leftSidebarOpen && documentState ? (
            <LeftSidebar
              session={session}
              pages={pages}
              panel={leftSidebarPanel}
              width={leftSidebarWidth}
              mutationDisabled={documentMutationDisabled}
              onSelectPage={handleSelectPage}
              onSetPageScale={openPageScaleDialogForPage}
              onRotatePage={handleRotatePage}
              onWidthChange={setLeftSidebarWidth}
            />
          ) : null}
          <div
            className="flex min-w-0 flex-1 flex-col"
            id="document-tab-panel"
            role={activeTabId ? 'tabpanel' : undefined}
            aria-labelledby={activeTabId ? `document-tab-trigger-${tabs.findIndex((tab) => tab.id === activeTabId)}` : undefined}
          >
            <>
              <ViewerToolbar disabled={viewerControlsDisabled} zoom={zoom} zoomPreset={zoomPreset} scrollMode={scrollMode} continuousScrollWheelMode={continuousScrollWheelMode} singlePageScrollWheelMode={singlePageScrollWheelMode} pageColumnsEnabled={pageColumnsEnabled} cadViewOrganisation={cadViewOrganisation} pagesPerColumn={pagesPerColumn} onFitPage={() => setZoomPreset('fit-page')} onFitWidth={() => setZoomPreset('fit-width')} onScrollModeChange={setScrollMode} onContinuousScrollWheelModeChange={setContinuousScrollWheelMode} onSinglePageScrollWheelModeChange={setSinglePageScrollWheelMode} onPageColumnsEnabledChange={setPageColumnsEnabled} onCadViewOrganisationChange={setCadViewOrganisation} onPagesPerColumnChange={setPagesPerColumn} onZoomIn={() => updateZoom(zoom * 1.1)} onZoomOut={() => updateZoom(zoom / 1.1)} onZoomReset={() => updateZoom(1)} onZoomChange={updateZoom} />
              <div className="min-h-0 flex-1">
                <CanvasContextMenu
                  disabled={viewerControlsDisabled}
                  mutationDisabled={documentMutationDisabled}
                  pageIndex={currentPage}
                  onSelectTool={() => handleToolChange('select')}
                  onPanTool={() => handleToolChange('pan')}
                  onZoomIn={() => updateZoom(zoom * 1.1)}
                  onZoomOut={() => updateZoom(zoom / 1.1)}
                  onFitWidth={() => setZoomPreset('fit-width')}
                  onFitPage={() => setZoomPreset('fit-page')}
                  onSetPageScale={openPageScaleDialogForPage}
                  onRotatePage={handleRotatePage}
                >
                  <DocumentViewport session={session} onOpenDocument={() => void handleOpen()} calibrationPick={!documentMutationDisabled && pageScaleCalibrationPick ? { active: true, pointCount: pageScaleCalibrationPick.points.length } : null} onCalibrationPoint={handlePageScaleCalibrationPoint} onCancelCalibrationPick={cancelPageScaleCalibrationPick} />
                </CanvasContextMenu>
              </div>
            </>
          </div>
          {rightSidebarOpen ? <RightSidebar activeTool={activeTool} mutationDisabled={documentMutationDisabled} width={rightSidebarWidth} onWidthChange={setRightSidebarWidth} /> : null}
          <RightRail
            activeTool={activeTool}
            disabled={viewerControlsDisabled}
            mutationDisabled={documentMutationDisabled}
            propertiesOpen={rightSidebarOpen}
            snapSettings={snapSettings}
            onSelectTool={handleToolChange}
            onSnapSettingsChange={setSnapSettings}
            onToggleProperties={() => toggleRightSidebar('tools')}
          />
        </div>
      </main>
      <NewBlankPdfDialog
        open={newBlankPdfDialogOpen}
        settings={blankPdfSettings}
        onCreate={handleCreateBlankPdf}
        onOpenChange={setNewBlankPdfDialogOpen}
        onSettingsChange={handleBlankPdfSettingsChange}
      />
      {pageScaleDialogOpen && documentState && !documentMutationDisabled ? (
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
      {pendingCloseTabId ? (
        <UnsavedChangesDialog
          mode="tab"
          documentName={tabs.find((tab) => tab.id === pendingCloseTabId)?.document.fileName}
          busy={closeActionBusy}
          onSave={() => { void savePendingCloseTab(); }}
          onDiscard={() => {
            const tabId = pendingCloseTabId;
            setPendingCloseTabId(null);
            void disposeAndRemoveTab(tabId);
          }}
          onCancel={() => setPendingCloseTabId(null)}
        />
      ) : null}
      {applicationCloseRequested ? (
        <UnsavedChangesDialog
          mode="application"
          dirtyDocumentCount={dirtyDocumentCount}
          busy={closeActionBusy}
          onSave={() => { void saveAllAndCloseApplication(); }}
          onDiscard={() => { void discardAllAndCloseApplication(); }}
          onCancel={() => {
            setApplicationCloseRequested(false);
            void window.butterPaper.application.cancelClose();
          }}
        />
      ) : null}
      <UpdateDialog
        hasDirtyDocuments={hasDirtyDocuments}
        manualCheck={updater.manualCheck}
        productName={applicationMetadata.productName}
        status={updater.status}
        onCheckAgain={() => void updater.actions.checkNow()}
        onDismissManualCheck={updater.actions.dismissManualCheck}
        onInstall={() => void updater.actions.installDownloaded()}
        onOpenReleasePage={() => void updater.actions.openReleasePage()}
      />
    </div>
  );
}
