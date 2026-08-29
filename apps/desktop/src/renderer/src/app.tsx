import { useEffect, useRef, useState } from 'react';
import type { ChangeEvent, DragEvent } from 'react';
import {
  rotateDocumentPage,
  translateMarkup,
  updateMarkupText,
  type DocumentModel,
  type Markup,
  type PageRotationDirection,
  type PdfPoint,
  type SignatureAppearanceAsset,
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
import { TemplateManagerDialog } from './components/TemplateManagerDialog';
import {
  loadTemplateLibrary,
  saveTemplateLibrary,
  templateCreateRequest,
  useTemplate,
  withImportedTemplates,
  type PdfTemplate,
} from './components/templateLibrary';
import { PageScaleDialog } from './components/PageScaleDialog';
import { RightRail, shouldDispatchToolSelection } from './components/RightRail';
import { RightSidebar } from './components/RightSidebar';
import { UnsavedChangesDialog } from './components/UnsavedChangesDialog';
import { UpdateDialog } from './components/UpdateDialog';
import { ViewerToolbar } from './components/ViewerToolbar';
import { formatWindowTitle, WindowTitleBar } from './components/WindowTitleBar';
import { updateSelectedMarkupProperty } from './components/ToolPropertiesPanel';
import { useUpdater } from './hooks/useUpdater';
import { LocalPdfSession, type DiagnosticsSnapshot } from './services/documentSession';
import { getPerfSnapshot, initialisePerfTracking, recordComponentRender, recordWindowBounds, resetPerfTracking } from './services/perfTracker';
import { getRenderCoordinatorDiagnostics } from './services/renderCoordinator';
import {
  DEFAULT_LEFT_SIDEBAR_WIDTH,
  DEFAULT_RIGHT_SIDEBAR_WIDTH,
  createDocumentHistory,
  useViewerStore,
  type CadViewOrganisation,
  type LeftSidebarPanel,
  type LoadedDocumentState,
  type DocumentHistorySnapshot,
  type SnapSettings,
} from './state/viewerStore';
import { subscribeToThemeMode } from './theme';
import type { ApplicationMenuCommand, ApplicationMenuState, ApplicationMetadata, BlankPdfCreateRequest, BlankPdfCreateResult, LoadedDocumentPayload, PdfOpenProgress, PdfSaveTargetDescriptor, ScrollMode, ScrollWheelMode, ThemeMode, ToolMode, ViewerDiagnostics, ZoomPreset } from '../../shared/protocol';
import { getMarkupToolDefinition, PDF_TOOL_REGISTRY } from './pdf-tools/toolRegistry';
import { buildMarkupSpatialIndex } from './pdf-tools/markupSpatialIndex';
import { clampViewerZoom } from './utils/renderZoom';
import { selectDroppedPdfFiles } from './utils/droppedPdfFiles';
import { resolveMacosFullScreenLayout } from './utils/macosFullScreenLayout';
import { canHideMenuBar, resolveMenuBarVisibility } from './utils/menuBarVisibility';
import { signatureAppearanceToPendingImageAsset } from './utils/signaturePlacement';
import {
  dismissToolShortcutPopup,
  isEditableShortcutTarget,
  isInteractiveShortcutTarget,
  isToolShortcutBlockedTarget,
  normalizeShortcutKey,
  parseToolShortcut,
  resolvePdfZoomShortcut,
  resolveToolShortcut,
  shouldResetToolOnEscape,
} from './utils/toolShortcuts';
import { saveDocumentsInOrder } from './utils/unsavedDocuments';
import { markupIdsOnPage, pasteCanvasMarkups, selectedCanvasMarkups } from './utils/canvasEdit';

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
const MENU_BAR_VISIBILITY_STORAGE_KEY = 'butterPaper.alwaysShowMenuBarInAppWindow';

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

function createCanvasMarkupId(kind: Markup['kind']): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return `${kind}-${crypto.randomUUID()}`;
  }
  return `${kind}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
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
  documentHistory: DocumentHistorySnapshot;
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

function initialViewSnapshot(initiallyDirty = false): ViewerTabSnapshot {
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
      snapToPageGrid: true,
      dimensionIncrementEnabled: false,
      dimensionIncrementMm: 5,
      constructionGridEnabled: false,
      constructionGridVisible: true,
      constructionGridSpacingMm: 10,
      sensitivityPx: 8,
      snapTargets: ['endpoint', 'midpoint', 'center', 'intersection'],
      snapGuidesEnabled: true,
      snapGuideTypes: ['alignment', 'equal-size', 'equal-spacing'],
    },
    documentHistory: createDocumentHistory(initiallyDirty),
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
    documentHistory: state.documentHistory,
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
  const [templateLibrary, setTemplateLibrary] = useState(() => loadTemplateLibrary(window.localStorage));
  const [templateManagerOpen, setTemplateManagerOpen] = useState(false);
  const [pendingCloseTabId, setPendingCloseTabId] = useState<string | null>(null);
  const [applicationCloseRequested, setApplicationCloseRequested] = useState(false);
  const menuBarCanBeHidden = canHideMenuBar(navigator.platform);
  const [menuBarVisible, setMenuBarVisible] = useState(() => resolveMenuBarVisibility(
    navigator.platform,
    window.localStorage.getItem(MENU_BAR_VISIBILITY_STORAGE_KEY),
  ));
  const [windowFullScreen, setWindowFullScreen] = useState(false);
  const [closeActionBusy, setCloseActionBusy] = useState(false);
  const [canvasClipboardCount, setCanvasClipboardCount] = useState(0);
  const [documentOpenCount, setDocumentOpenCount] = useState(0);
  const [systemDocumentOpenPending, setSystemDocumentOpenPending] = useState(false);
  const [documentOpenProgress, setDocumentOpenProgress] = useState<PdfOpenProgress | null>(null);

  useEffect(() => {
    const templatesBridge = window.butterPaper.templates;
    if (!templatesBridge) return;
    let active = true;
    void templatesBridge.list().then((records) => {
      if (active) setTemplateLibrary((current) => withImportedTemplates(current, records));
    }).catch((error) => {
      console.warn('Unable to load PDF templates.', error);
    });
    return () => { active = false; };
  }, []);
  const setDocument = useViewerStore((state) => state.setDocument);
  const updateDocument = useViewerStore((state) => state.updateDocument);
  const replaceDocumentAfterSave = useViewerStore((state) => state.replaceDocumentAfterSave);
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
  const undoDocument = useViewerStore((state) => state.undoDocument);
  const redoDocument = useViewerStore((state) => state.redoDocument);
  const selectedMarkupIds = useViewerStore((state) => state.selectedMarkupIds);
  const documentHistory = useViewerStore((state) => state.documentHistory);
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
  const rightSidebarWidth = useViewerStore((state) => state.rightSidebarWidth);
  const isTestMode = window.butterPaper.environment.testMode;
  const cadViewEnabled = window.butterPaper.environment.cadViewEnabled;
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? null;
  const windowTitle = formatWindowTitle(
    applicationMetadata.windowTitle,
    activeTab?.document.fileName,
    activeTab ? Math.max(0, tabs.length - 1) : 0,
  );
  const session = activeTab?.session ?? null;
  const windowChrome = resolveMacosFullScreenLayout({
    platform: navigator.platform,
    fullScreen: windowFullScreen,
    menuBarVisible,
  });
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const defaultSampleOpenedRef = useRef(false);
  const userOpenRequestedRef = useRef(false);
  const heldPanRestoreToolRef = useRef<ToolMode | null>(null);
  const activeToolRef = useRef<ToolMode>(activeTool);
  const lastSpaceTapAtRef = useRef(0);
  const openDocumentPathsRef = useRef<(filePaths: string[]) => Promise<void>>(async () => {});
  const canvasClipboardRef = useRef<Markup[]>([]);
  const pasteSequenceRef = useRef(0);

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

  useEffect(() => window.butterPaper.application.onPdfOpenPendingChanged(setSystemDocumentOpenPending), []);
  useEffect(() => window.butterPaper.application.onPdfOpenProgressChanged(setDocumentOpenProgress), []);

  useEffect(() => {
    let cancelled = false;
    void window.butterPaper.application.getMetadata()
      .then((metadata) => {
        if (!cancelled) {
          setApplicationMetadata(metadata);
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
    document.title = windowTitle;
  }, [windowTitle]);

  useEffect(() => {
    if (!cadViewEnabled && pageColumnsEnabled) {
      setPageColumnsEnabled(false);
    }
  }, [cadViewEnabled, pageColumnsEnabled, setPageColumnsEnabled]);

  useEffect(() => {
    document.documentElement.dataset.testMode = isTestMode ? 'true' : 'false';
    if (isTestMode) { initialisePerfTracking(); resetPerfTracking(); }
    return () => { delete document.documentElement.dataset.testMode; };
  }, [isTestMode]);

  useEffect(() => {
    const defaultSamplePdfPath = window.butterPaper.environment.defaultSamplePdfPath;
    if (defaultSampleOpenedRef.current || userOpenRequestedRef.current || isTestMode || tabs.length > 0 || !defaultSamplePdfPath) {
      return;
    }

    defaultSampleOpenedRef.current = true;
    void openDocumentPaths([defaultSamplePdfPath], { defaultSample: true });
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
      pageColumnsEnabled: cadViewEnabled && nextTab.viewSnapshot.pageColumnsEnabled,
      activeTool: restoreableTool(nextTab.viewSnapshot.activeTool),
      postPlacement: null,
      pendingImageAsset: null,
      pendingPageScroll: null,
      pendingDocumentScroll: null,
    });
  }

  useEffect(() => {
    const recoveredTabId = resolveActiveTabId(tabs, activeTabId);
    if (recoveredTabId !== activeTabId || (recoveredTabId !== null && !documentState)) {
      activateTab(recoveredTabId, tabs);
    }
  }, [activeTabId, documentState, tabs]);

  async function openDocumentPaths(filePaths: string[], options: { forceNewTabs?: boolean; defaultSample?: boolean } = {}): Promise<void> {
    if (!options.defaultSample) {
      userOpenRequestedRef.current = true;
    }
    const candidates = [...new Set(filePaths.map((path) => path.trim()).filter((path) => /\.pdf$/i.test(path)))];
    if (candidates.length === 0) return;
    setDocumentOpenCount((count) => count + 1);
    try {
      setErrorMessage(null); setStatusMessage(candidates.length === 1 ? 'Loading document' : `Loading ${candidates.length} documents`);
      const currentTabs = tabs;
      const created: DocumentTab[] = [];
      let firstOpenError: Error | null = null;
      let duplicateToFocus: string | null = null;
      for (const filePath of candidates) {
        const normalizedPath = normalizeDocumentPath(filePath);
        const existing = options.forceNewTabs
          ? undefined
          : [...currentTabs, ...created].find((tab) => tab.normalizedPath === normalizedPath);
        if (existing) { duplicateToFocus ??= existing.id; continue; }
        const nextSession = new LocalPdfSession(filePath);
        try {
          const payload = await nextSession.open();
          if (options.defaultSample && userOpenRequestedRef.current) {
            nextSession.dispose();
            continue;
          }
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
    } finally {
      setDocumentOpenCount((count) => Math.max(0, count - 1));
    }
  }
  openDocumentPathsRef.current = openDocumentPaths;

  async function loadDocumentFromPath(filePath: string) { await openDocumentPaths([filePath]); }

  async function handleCreateBlankPdf(request: BlankPdfCreateRequest): Promise<void> {
    return handleCreateTemporaryDocument(await window.butterPaper.pdf.createBlankDocument(request));
  }

  async function handleCreateTemporaryDocument(temporaryDocument: BlankPdfCreateResult): Promise<void> {
    await window.butterPaper.application.setCloseBlocked(true);
    let nextSession: LocalPdfSession | null = null;
    try {
      nextSession = new LocalPdfSession(temporaryDocument.filePath);
      const payload = await nextSession.open();
      const nextTab: DocumentTab = {
        id: `tab-${nextTabId++}`,
        filePath: payload.filePath,
        normalizedPath: normalizeDocumentPath(payload.filePath),
        session: nextSession,
        document: loadedDocumentState({ ...payload, fileName: temporaryDocument.fileName }, true),
        viewSnapshot: initialViewSnapshot(true),
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

  async function handleCreateFromTemplate(template: PdfTemplate): Promise<void> {
    try {
      if (template.kind === 'generated') {
        await handleCreateBlankPdf(templateCreateRequest(template));
      } else {
        await handleCreateTemporaryDocument(await window.butterPaper.templates.createDocument(template.id));
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to create a PDF from the template.');
      throw error;
    }
  }

  function handleTemplateLibraryChange(nextLibrary: typeof templateLibrary): void {
    const removedImported = templateLibrary.importedTemplates.filter((template) => (
      !nextLibrary.importedTemplates.some((candidate) => candidate.id === template.id)
    ));
    for (const template of removedImported) void window.butterPaper.templates.remove(template.id);
    saveTemplateLibrary(window.localStorage, nextLibrary);
    setTemplateLibrary(nextLibrary);
  }

  function handleUseTemplate(templateId: string): void {
    handleTemplateLibraryChange(useTemplate(templateLibrary, templateId));
  }

  async function handleImportPdfTemplate(): Promise<void> {
    const imported = await window.butterPaper.templates.importPdf();
    if (!imported) return;
    const next = withImportedTemplates(templateLibrary, [...templateLibrary.importedTemplates, imported]);
    handleTemplateLibraryChange({ ...next, lastTemplateId: imported.id });
    setStatusMessage(`Imported ${imported.name} as a template`);
  }

  async function handleSaveDocumentAsTemplate(): Promise<void> {
    if (!documentState) return;
    try {
      const imported = await window.butterPaper.templates.importDocument({
        documentHandle: documentState.documentAccess.handle,
        name: documentState.fileName,
      });
      const next = withImportedTemplates(templateLibrary, [...templateLibrary.importedTemplates, imported]);
      handleTemplateLibraryChange({ ...next, lastTemplateId: imported.id });
      setStatusMessage(`Saved ${imported.name} as a template`);
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to save the document as a template.');
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
      leftSidebarOpen, rightSidebarOpen, leftSidebarWidth: 300, rightSidebarWidth,
      markupCount: documentState?.document.markups.length ?? 0,
      renderCacheEntries: activeDiagnostics.renderCacheEntries,
      renderCacheBytes: activeDiagnostics.renderCacheBytes,
      pageUrlCacheBytes: activeDiagnostics.pageUrlCacheBytes,
      decodedRenderCacheBytes: activeDiagnostics.decodedRenderCacheBytes,
      pageUrlCacheByteLimit: activeDiagnostics.pageUrlCacheByteLimit,
      decodedRenderCacheByteLimit: activeDiagnostics.decodedRenderCacheByteLimit,
      pageRenderReady: activeDiagnostics.pageRenderReady,
      thumbnailRenderReady: activeDiagnostics.thumbnailRenderReady,
      lastPageRenderError: activeDiagnostics.lastPageRenderError,
      lastThumbnailRenderError: activeDiagnostics.lastThumbnailRenderError,
      thumbnailCacheEntries: activeDiagnostics.thumbnailCacheEntries,
      thumbnailCacheBytes: activeDiagnostics.thumbnailCacheBytes,
      thumbnailCacheByteLimit: activeDiagnostics.thumbnailCacheByteLimit,
      queuedPageRenders: activeDiagnostics.queuedPageRenders,
      inflightPageRenders: activeDiagnostics.inflightPageRenders,
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
      getActiveDocument: () => useViewerStore.getState().document?.document ?? null,
      replaceDocumentMarkups: (markups, pageScales, selectedMarkupIds, recordHistory = true) => {
        const state = useViewerStore.getState();
        const document = state.document?.document;
        if (!document) throw new Error('Test document is unavailable.');
        const pageIndices = new Set(document.pages.map(({ index }) => index));
        if (markups.some((markup) => !pageIndices.has(markup.pageIndex))) {
          throw new Error('Test markups contain an unknown page index.');
        }
        const markupIds = new Set(markups.map(({ id }) => id));
        if (markupIds.size !== markups.length
          || selectedMarkupIds.some((markupId) => !markupIds.has(markupId))) {
          throw new Error('Test markup replacement contains duplicate IDs or invalid selection.');
        }
        state.updateDocument((current) => ({ ...current, markups, pageScales }), recordHistory);
        state.setSelectedMarkupIds([...selectedMarkupIds]);
      },
      queryMarkupSpatialIndex: (pageIndex, point, tolerance) => {
        const document = useViewerStore.getState().document?.document;
        const page = document?.pages.find((candidate) => candidate.index === pageIndex);
        if (!document || !page) throw new Error('Test spatial-index page is unavailable.');
        const index = buildMarkupSpatialIndex(document.markups, page);
        const receipt = index.query(point, tolerance);
        const candidateIds = new Set(receipt.candidateMarkupIds);
        const hit = [...document.markups].reverse()
          .filter((markup) => markup.pageIndex === pageIndex && candidateIds.has(markup.id))
          .map((markup) => getMarkupToolDefinition(markup)?.geometry?.hitTest(
            markup,
            point,
            { page, tolerance },
          ) ?? null)
          .find((candidate) => candidate !== null) ?? null;
        return {
          ...receipt,
          pageIndex,
          generation: index.generation,
          hitMarkupId: hit?.markupId ?? null,
        };
      },
      selectMarkupAtPoint: (pageIndex, point, tolerance) => {
        const state = useViewerStore.getState();
        const document = state.document?.document;
        const page = document?.pages.find((candidate) => candidate.index === pageIndex);
        if (!document || !page) {
          state.setSelectedMarkupIds([]);
          return null;
        }
        const hit = [...document.markups]
          .reverse()
          .filter((markup) => markup.pageIndex === pageIndex)
          .map((markup) => getMarkupToolDefinition(markup)?.geometry?.hitTest(
            markup,
            point,
            { page, tolerance },
          ) ?? null)
          .find((candidate) => candidate !== null) ?? null;
        state.setSelectedMarkupIds(hit ? [hit.markupId] : []);
        return hit?.markupId ?? null;
      },
      applyMarkupMutation: (mutation) => {
        const state = useViewerStore.getState();
        const target = state.document?.document.markups.find((markup) => markup.id === mutation.markupId);
        if (!target) throw new Error(`Test markup is missing: ${mutation.markupId}`);
        state.setSelectedMarkupIds([mutation.markupId]);
        state.updateDocument((document) => {
          if (mutation.kind === 'replace-text') {
            return updateMarkupText(document, mutation.markupId, mutation.text);
          }
          return {
            ...document,
            markups: document.markups.map((markup) => {
              if (markup.id !== mutation.markupId) return markup;
              if (mutation.kind === 'translate') {
                return translateMarkup(markup, mutation.delta);
              }
              if (mutation.kind === 'set-properties') {
                return Object.entries(mutation.values).reduce(
                  (current, [key, value]) => updateSelectedMarkupProperty(
                    current,
                    key as 'x' | 'y' | 'width' | 'height' | 'opacity',
                    value,
                  ),
                  markup,
                );
              }
              const transform = getMarkupToolDefinition(markup)?.interaction?.transformMarkup;
              if (!transform) throw new Error(`Markup does not support a tool transform: ${markup.id}`);
              const page = document.pages.find((candidate) => candidate.index === markup.pageIndex);
              if (!page) throw new Error(`Markup page is missing: ${markup.pageIndex}`);
              return transform(markup, mutation, {
                page,
                pageScale: document.pageScales?.find((scale) => scale.pageIndex === markup.pageIndex),
                markups: document.markups,
              });
            }),
          };
        });
      },
      undoDocument: () => handleUndo(),
      redoDocument: () => handleRedo(),
      getDocumentHistory: () => {
        const history = useViewerStore.getState().documentHistory;
        return {
          past: history.past.length,
          future: history.future.length,
          currentRevision: history.currentRevision,
          savedRevision: history.savedRevision,
        };
      },
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

  async function handleOpen() {
    setDocumentOpenCount((count) => count + 1);
    try {
      const paths = await window.butterPaper.dialogs.openPdfDialog();
      if (paths?.length) await openDocumentPaths(paths);
    } finally {
      setDocumentOpenCount((count) => Math.max(0, count - 1));
    }
  }

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
    let target = explicitTarget;
    if (tab.temporarySourcePath && !target) {
      target = await window.butterPaper.dialogs.savePdfAsDialog(sourceDocument.fileName) ?? undefined;
      if (!target) return false;
    }

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
      replaceDocumentAfterSave(nextDocument);
    }
    setStatusMessage(target
      ? `Saved new PDF ${payload.fileName}; the original was preserved.`
      : `Saved ${payload.fileName}`);
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
      setPendingImageAsset(null);
      setActiveTool(tool);
      imageInputRef.current?.click();
      return;
    }
    setActiveTool(tool);
  }

  function handleUseSignature(asset: SignatureAppearanceAsset): void {
    setPendingImageAsset(signatureAppearanceToPendingImageAsset(asset));
    setActiveTool('image');
    setStatusMessage('Click the page to place the signature');
    setErrorMessage(null);
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
    const lockedIds = new Set(
      (useViewerStore.getState().document?.document.markups ?? [])
        .filter((markup) => selectedIds.has(markup.id) && markup.locked)
        .map((markup) => markup.id),
    );
    const deletableIds = new Set(ids.filter((id) => !lockedIds.has(id)));
    if (deletableIds.size === 0) {
      setStatusMessage(ids.length === 1 ? 'Markup is locked' : 'Selected markups are locked');
      return true;
    }
    updateDocument((document) => ({
      ...document,
      markups: document.markups.filter((markup) => !deletableIds.has(markup.id)),
    }));
    setSelectedMarkupIds(ids.filter((id) => lockedIds.has(id)));
    setStatusMessage(deletableIds.size === 1 ? 'Deleted markup' : `Deleted ${deletableIds.size} markups`);
    return true;
  }

  function copySelectedMarkups(): boolean {
    const state = useViewerStore.getState();
    const copied = selectedCanvasMarkups(state.document?.document.markups ?? [], state.selectedMarkupIds);
    if (copied.length === 0) return false;
    canvasClipboardRef.current = structuredClone(copied);
    setCanvasClipboardCount(copied.length);
    pasteSequenceRef.current = 0;
    setStatusMessage(copied.length === 1 ? 'Copied markup' : `Copied ${copied.length} markups`);
    return true;
  }

  function cutSelectedMarkups(): boolean {
    if (!copySelectedMarkups()) return false;
    return deleteSelectedMarkups();
  }

  function pasteMarkups(): boolean {
    const clipboard = canvasClipboardRef.current;
    if (clipboard.length === 0 || !useViewerStore.getState().document) return false;
    pasteSequenceRef.current += 1;
    const pasted = pasteCanvasMarkups(
      clipboard,
      useViewerStore.getState().currentPage,
      pasteSequenceRef.current,
      createCanvasMarkupId,
    );
    updateDocument((document) => ({ ...document, markups: [...document.markups, ...pasted] }));
    setSelectedMarkupIds(pasted.map((markup) => markup.id));
    setStatusMessage(pasted.length === 1 ? 'Pasted markup' : `Pasted ${pasted.length} markups`);
    return true;
  }

  function selectAllMarkupsOnCurrentPage(): boolean {
    const state = useViewerStore.getState();
    const ids = markupIdsOnPage(state.document?.document.markups ?? [], state.currentPage);
    setSelectedMarkupIds(ids);
    if (ids.length > 0) {
      setStatusMessage(ids.length === 1 ? 'Selected markup on page' : `Selected ${ids.length} markups on page`);
    }
    return ids.length > 0;
  }

  function handleUndo(): boolean {
    const changed = undoDocument();
    if (changed) setStatusMessage('Undid document change');
    return changed;
  }

  function handleRedo(): boolean {
    const changed = redoDocument();
    if (changed) setStatusMessage('Redid document change');
    return changed;
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
    if (event.dataTransfer.files.length === 0) {
      return;
    }
    event.preventDefault();
    const pdfFiles = selectDroppedPdfFiles(event.dataTransfer.files);
    if (pdfFiles.length === 0) {
      return;
    }
    userOpenRequestedRef.current = true;
    setDocumentOpenCount((count) => count + 1);
    void Promise.all(pdfFiles.map((file) => window.butterPaper.application.authorizeDroppedPdf(file)))
      .then((pdfPaths) => openDocumentPaths(pdfPaths, { forceNewTabs: true }))
      .catch((error) => {
        setErrorMessage(error instanceof Error ? error.message : 'Unable to open the dropped PDF.');
      })
      .finally(() => {
        setDocumentOpenCount((count) => Math.max(0, count - 1));
      });
  }

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const key = normalizeShortcutKey(event.key);
      const isSpacePanShortcut = key === 'space' && !isInteractiveShortcutTarget(event.target);
      if (event.defaultPrevented && !isSpacePanShortcut) {
        return;
      }

      const isMacPlatform = navigator.platform.toLowerCase().includes('mac');
      const mod = isMacPlatform ? event.metaKey : event.ctrlKey;
      const pdfZoomAction = resolvePdfZoomShortcut(event, isMacPlatform);
      if (pdfZoomAction && documentState && !isEditableShortcutTarget(event.target)) {
        event.preventDefault();
        updateZoom(pdfZoomAction === 'zoom-reset' ? 1 : pdfZoomAction === 'zoom-in' ? zoom * 1.1 : zoom / 1.1);
        return;
      }
      if (mod) {
        // On macOS the native application menu owns these accelerators and
        // forwards the typed command back through the preload bridge. Keeping
        // the renderer shortcut path for other platforms avoids double actions.
        if (isMacPlatform) {
          return;
        }
        if (isEditableShortcutTarget(event.target)) {
          return;
        }
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
        } else if (key === 'z' && event.shiftKey) {
          event.preventDefault();
          handleRedo();
        } else if (key === 'z') {
          event.preventDefault();
          handleUndo();
        } else if (key === 'x') {
          event.preventDefault();
          cutSelectedMarkups();
        } else if (key === 'c') {
          event.preventDefault();
          copySelectedMarkups();
        } else if (key === 'v') {
          event.preventDefault();
          pasteMarkups();
        } else if (key === 'a') {
          event.preventDefault();
          selectAllMarkupsOnCurrentPage();
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

      if (key === 'escape' && shouldResetToolOnEscape(event.target)) {
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
      dismissToolShortcutPopup(event.target);
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

  useEffect(() => window.butterPaper.application.onCloseTabRequested(() => {
    if (activeTabId) {
      requestCloseTab(activeTabId);
    }
  }), [activeTabId, documentState, tabs]);

  useEffect(() => window.butterPaper.application.onMenuBarVisibilityChanged((visible) => {
    setMenuBarVisible(menuBarCanBeHidden ? visible : true);
  }), [menuBarCanBeHidden]);

  useEffect(() => {
    let cancelled = false;
    const unsubscribe = window.butterPaper.application.onWindowFullScreenChanged(setWindowFullScreen);
    void window.butterPaper.application.getWindowFullScreen().then((fullScreen) => {
      if (!cancelled) setWindowFullScreen(fullScreen);
    }).catch((error) => {
      console.error('Unable to read window fullscreen state.', error);
    });
    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, []);

  useEffect(() => {
    window.localStorage.setItem(MENU_BAR_VISIBILITY_STORAGE_KEY, menuBarVisible ? '1' : '0');
    void window.butterPaper.application.setMenuBarVisibility(menuBarVisible).catch((error) => {
      console.error('Unable to update menu-bar visibility.', error);
    });
  }, [menuBarVisible]);

  useEffect(() => {
    const menuState: ApplicationMenuState = {
      canSave,
      canUndo: documentHistory.past.length > 0,
      canRedo: documentHistory.future.length > 0,
      canCut: selectedMarkupIds.length > 0,
      canCopy: selectedMarkupIds.length > 0,
      canPaste: canvasClipboardCount > 0 && Boolean(documentState),
      canSelectAll: Boolean(documentState?.document.markups.some((markup) => markup.pageIndex === currentPage)),
      updateStatus: updater.status,
      menuBarVisible,
    };
    void window.butterPaper.application.setMenuState(menuState).catch((error) => {
      console.error('Unable to update application menu state.', error);
    });
  }, [canSave, canvasClipboardCount, currentPage, documentHistory, documentState, menuBarVisible, selectedMarkupIds, updater.status]);

  useEffect(() => window.butterPaper.application.onMenuCommand((command: ApplicationMenuCommand) => {
    switch (command) {
      case 'new-pdf':
        setTemplateManagerOpen(true);
        break;
      case 'open-pdf':
        void handleOpen();
        break;
      case 'save':
        void handleSave();
        break;
      case 'save-as':
        void handleSaveAs();
        break;
      case 'save-document-as-template':
        void handleSaveDocumentAsTemplate();
        break;
      case 'undo':
        handleUndo();
        break;
      case 'redo':
        handleRedo();
        break;
      case 'cut':
        cutSelectedMarkups();
        break;
      case 'copy':
        copySelectedMarkups();
        break;
      case 'paste':
        pasteMarkups();
        break;
      case 'select-all':
        selectAllMarkupsOnCurrentPage();
        break;
      case 'set-default-pdf-app':
        void handleSetAsDefaultPdfApp();
        break;
      case 'check-for-updates':
        void updater.actions.checkNow();
        break;
      case 'open-release-page':
        void updater.actions.openReleasePage();
        break;
    }
  }), [handleOpen, handleSave, handleSaveAs, handleSetAsDefaultPdfApp, updater.actions]);

  return (
    <div
      className="flex h-screen flex-col bg-background text-foreground"
      data-testid="app-root"
      data-window-fullscreen={windowFullScreen}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
    >
      {windowChrome.showWindowTitleBar ? <WindowTitleBar title={windowTitle} /> : null}
      {windowChrome.showAppMenuBar ? (
        <AppMenuBar
          canSave={canSave}
          productName={applicationMetadata.productName}
          updateStatus={updater.status}
          menuBarVisible={menuBarVisible}
          showMenuBarVisibilityOption={menuBarCanBeHidden}
          canUndo={documentHistory.past.length > 0}
          canRedo={documentHistory.future.length > 0}
          canCut={selectedMarkupIds.length > 0}
          canCopy={selectedMarkupIds.length > 0}
          canPaste={canvasClipboardCount > 0 && Boolean(documentState)}
          canSelectAll={Boolean(documentState?.document.markups.some((markup) => markup.pageIndex === currentPage))}
          onNewPdf={() => setNewBlankPdfDialogOpen(true)}
          onOpen={() => void handleOpen()}
          onSave={() => void handleSave()}
          onSaveAs={() => void handleSaveAs()}
          onSaveDocumentAsTemplate={() => void handleSaveDocumentAsTemplate()}
          onUndo={() => handleUndo()}
          onRedo={() => handleRedo()}
          onCut={() => cutSelectedMarkups()}
          onCopy={() => copySelectedMarkups()}
          onPaste={() => pasteMarkups()}
          onSelectAll={() => selectAllMarkupsOnCurrentPage()}
          onMenuBarVisibilityChange={setMenuBarVisible}
          onReload={() => void window.butterPaper.application.reloadWindow(false)}
          onForceReload={() => void window.butterPaper.application.reloadWindow(true)}
          onToggleFullScreen={() => void window.butterPaper.application.toggleWindowFullScreen()}
          onSetAsDefaultPdfApp={() => void handleSetAsDefaultPdfApp()}
          onCheckForUpdates={() => void updater.actions.checkNow()}
          onOpenReleasePage={() => void updater.actions.openReleasePage()}
          onUpdateFrequencyChange={(frequency) => void updater.actions.setFrequency(frequency)}
          onQuit={() => void window.butterPaper.application.requestQuit().catch((error) => {
            console.error('Unable to quit Butter Paper.', error);
          })}
        />
      ) : null}
      <DocumentTabBar
        tabs={tabs.map((tab) => ({ id: tab.id, documentName: tab.document.fileName, dirty: Boolean(tab.document.dirty) }))}
        activeTabId={activeTabId}
        onSelectTab={activateTab}
        onCloseTab={requestCloseTab}
        closeConfirmation={{
          tabId: pendingCloseTabId,
          busy: closeActionBusy,
          onSave: () => { void savePendingCloseTab(); },
          onDiscard: () => {
            const tabId = pendingCloseTabId;
            if (!tabId) return;
            setPendingCloseTabId(null);
            void disposeAndRemoveTab(tabId);
          },
          onCancel: () => setPendingCloseTabId(null),
        }}
        onReorderTabs={(orderedTabIds) => setTabs((currentTabs) => applyTabOrder(currentTabs, orderedTabIds))}
        onOpenTab={() => void handleOpen()}
        onNewPdf={handleCreateFromTemplate}
        onManageTemplates={() => setTemplateManagerOpen(true)}
        onUseTemplate={handleUseTemplate}
        templateLibrary={templateLibrary}
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
              mutationDisabled={documentMutationDisabled}
              onSelectPage={handleSelectPage}
              onSetPageScale={openPageScaleDialogForPage}
              onRotatePage={handleRotatePage}
            />
          ) : null}
          <div
            className="flex min-w-0 flex-1 flex-col"
            id="document-tab-panel"
            role={activeTabId ? 'tabpanel' : undefined}
            aria-labelledby={activeTabId ? `document-tab-trigger-${tabs.findIndex((tab) => tab.id === activeTabId)}` : undefined}
          >
            <>
              <ViewerToolbar disabled={viewerControlsDisabled} cadViewEnabled={cadViewEnabled} zoom={zoom} zoomPreset={zoomPreset} scrollMode={scrollMode} continuousScrollWheelMode={continuousScrollWheelMode} singlePageScrollWheelMode={singlePageScrollWheelMode} pageColumnsEnabled={pageColumnsEnabled} cadViewOrganisation={cadViewOrganisation} pagesPerColumn={pagesPerColumn} onFitPage={() => setZoomPreset('fit-page')} onFitWidth={() => setZoomPreset('fit-width')} onScrollModeChange={setScrollMode} onContinuousScrollWheelModeChange={setContinuousScrollWheelMode} onSinglePageScrollWheelModeChange={setSinglePageScrollWheelMode} onPageColumnsEnabledChange={setPageColumnsEnabled} onCadViewOrganisationChange={setCadViewOrganisation} onPagesPerColumnChange={setPagesPerColumn} onZoomIn={() => updateZoom(zoom * 1.1)} onZoomOut={() => updateZoom(zoom / 1.1)} onZoomReset={() => updateZoom(1)} onZoomChange={updateZoom} />
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
                  <DocumentViewport session={session} opening={documentOpenCount > 0 || systemDocumentOpenPending} openingProgress={documentOpenProgress} onOpenDocument={() => void handleOpen()} calibrationPick={!documentMutationDisabled && pageScaleCalibrationPick ? { active: true, pointCount: pageScaleCalibrationPick.points.length, pageIndex: pageScaleCalibrationPick.pageIndex, startPoint: pageScaleCalibrationPick.points[0] ?? null } : null} onCalibrationPoint={handlePageScaleCalibrationPoint} onCancelCalibrationPick={cancelPageScaleCalibrationPick} />
                </CanvasContextMenu>
              </div>
            </>
          </div>
          {rightSidebarOpen ? <RightSidebar activeTool={activeTool} mutationDisabled={documentMutationDisabled} /> : null}
          <RightRail
            activeTool={activeTool}
            disabled={viewerControlsDisabled}
            mutationDisabled={documentMutationDisabled}
            propertiesOpen={rightSidebarOpen}
            snapSettings={snapSettings}
            signatureContextId={activeTabId}
            onSelectTool={handleToolChange}
            onSetPageScale={() => openPageScaleDialogForPage(currentPage)}
            onUseSignature={handleUseSignature}
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
      <TemplateManagerDialog
        open={templateManagerOpen}
        library={templateLibrary}
        onLibraryChange={handleTemplateLibraryChange}
        onOpenChange={setTemplateManagerOpen}
        onImportPdf={handleImportPdfTemplate}
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
