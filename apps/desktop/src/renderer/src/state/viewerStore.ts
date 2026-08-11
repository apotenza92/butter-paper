import { create } from 'zustand';
import type { DocumentModel } from '@butter-paper/core';
import type {
  LoadedDocumentPayload,
  ScrollMode,
  ToolMode,
  ZoomPreset,
} from '../../../shared/protocol';
import {
  builtInToolPropertyValues,
  createInitialToolPropertyValues,
  type ToolPropertyValue,
  type ToolPropertyValuesByTool,
} from '../pdf-tools/toolPropertyDefaults';

export interface PendingImageAsset {
  readonly dataUrl: string;
  readonly mimeType: 'image/png' | 'image/jpeg';
  readonly width: number;
  readonly height: number;
  readonly fileName: string;
  readonly aspectRatioLocked?: boolean;
  readonly selectAfterPlacement?: boolean;
}

export interface PostPlacementState {
  readonly markupId: string;
  readonly tool: ToolMode;
}

export interface LoadedDocumentState extends LoadedDocumentPayload {
  dirty: boolean;
}

export interface DocumentHistoryEntry {
  readonly document: DocumentModel;
  readonly revision: number;
}

export interface DocumentHistorySnapshot {
  readonly past: readonly DocumentHistoryEntry[];
  readonly future: readonly DocumentHistoryEntry[];
  readonly currentRevision: number;
  readonly savedRevision: number;
  readonly nextRevision: number;
}

export type LeftSidebarPanel = 'pages';

export type SnapTarget = 'endpoint' | 'midpoint' | 'center' | 'intersection' | 'nearest';
export type SnapGuideType = 'alignment' | 'equal-size' | 'equal-spacing';
export type ScrollWheelMode = 'zoom' | 'scroll';
export type CadViewOrganisation = 'columns' | 'rows';

export const DEFAULT_SNAP_TARGETS: readonly SnapTarget[] = ['endpoint', 'midpoint', 'center', 'intersection'];
export const DEFAULT_SNAP_GUIDE_TYPES: readonly SnapGuideType[] = ['alignment', 'equal-size', 'equal-spacing'];

export interface SnapSettings {
  readonly snapToContent: boolean;
  readonly snapToMarkup: boolean;
  readonly sensitivityPx: number;
  readonly snapTargets: readonly SnapTarget[];
  readonly snapGuidesEnabled: boolean;
  readonly snapGuideTypes: readonly SnapGuideType[];
}

interface ViewerState {
  document: LoadedDocumentState | null;
  zoom: number;
  activeTool: ToolMode;
  leftSidebarOpen: boolean;
  rightSidebarOpen: boolean;
  leftSidebarPanel: LeftSidebarPanel;
  rightSidebarPanel: 'tools';
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
  documentHistory: DocumentHistorySnapshot;
  postPlacement: PostPlacementState | null;
  snapSettings: SnapSettings;
  toolPropertyValues: ToolPropertyValuesByTool;
  pendingImageAsset: PendingImageAsset | null;
  pendingPageScroll: { pageIndex: number; requestId: number } | null;
  pendingDocumentScroll: { edge: 'top' | 'bottom'; requestId: number } | null;
  pendingThumbnailScroll: { pageIndex: number; requestId: number } | null;
  statusMessage: string;
  errorMessage: string | null;
  setDocument: (document: LoadedDocumentPayload | null, history?: DocumentHistorySnapshot) => void;
  updateDocument: (updater: (document: DocumentModel) => DocumentModel, markDirty?: boolean) => void;
  replaceDocumentAfterSave: (document: LoadedDocumentPayload) => void;
  undoDocument: () => boolean;
  redoDocument: () => boolean;
  setZoom: (zoom: number) => void;
  setActiveTool: (tool: ToolMode) => void;
  resetToSelectionTool: () => void;
  setLeftSidebarWidth: (width: number) => void;
  setRightSidebarWidth: (width: number) => void;
  toggleLeftSidebar: (panel?: LeftSidebarPanel) => void;
  toggleRightSidebar: (panel?: 'tools') => void;
  openLeftSidebar: (panel: LeftSidebarPanel) => void;
  openRightSidebar: (panel: 'tools') => void;
  collapseLeftSidebar: () => void;
  collapseRightSidebar: () => void;
  setScrollMode: (mode: ScrollMode) => void;
  setContinuousScrollWheelMode: (mode: ScrollWheelMode) => void;
  setSinglePageScrollWheelMode: (mode: ScrollWheelMode) => void;
  setPageColumnsEnabled: (enabled: boolean) => void;
  setCadViewOrganisation: (organisation: CadViewOrganisation) => void;
  setPagesPerColumn: (count: number) => void;
  setZoomPreset: (preset: ZoomPreset) => void;
  setCurrentPage: (pageIndex: number) => void;
  setVisiblePageIndices: (pageIndices: number[]) => void;
  setSelectedMarkupIds: (markupIds: string[]) => void;
  setPostPlacement: (postPlacement: PostPlacementState | null) => void;
  setSnapSettings: (settings: Partial<SnapSettings>) => void;
  setToolPropertyValue: (tool: ToolMode, key: string, value: ToolPropertyValue) => void;
  resetToolPropertyValues: (tool: ToolMode) => void;
  setPendingImageAsset: (asset: PendingImageAsset | null) => void;
  consumePendingImageAsset: () => PendingImageAsset | null;
  requestPageScroll: (pageIndex: number) => number;
  consumePageScroll: (requestId: number) => void;
  requestDocumentScroll: (edge: 'top' | 'bottom') => number;
  consumeDocumentScroll: (requestId: number) => void;
  requestThumbnailScroll: (pageIndex: number) => number;
  consumeThumbnailScroll: (requestId: number) => void;
  setStatusMessage: (message: string) => void;
  setErrorMessage: (message: string | null) => void;
}

export const DEFAULT_LEFT_SIDEBAR_WIDTH = 300;
export const MIN_LEFT_SIDEBAR_WIDTH = 180;
export const MAX_LEFT_SIDEBAR_WIDTH = 360;
export const DEFAULT_RIGHT_SIDEBAR_WIDTH = 300;
export const MIN_RIGHT_SIDEBAR_WIDTH = 220;
export const MAX_RIGHT_SIDEBAR_WIDTH = 420;
export const DEFAULT_PAGES_PER_COLUMN = 10;
export const MIN_PAGES_PER_COLUMN = 1;
export const MAX_PAGES_PER_COLUMN = 100;
export const DOCUMENT_HISTORY_LIMIT = 100;

export function createDocumentHistory(initiallyDirty = false): DocumentHistorySnapshot {
  return {
    past: [],
    future: [],
    currentRevision: 0,
    savedRevision: initiallyDirty ? -1 : 0,
    nextRevision: 1,
  };
}

function restoreEditableDocument(current: DocumentModel, snapshot: DocumentModel): DocumentModel {
  return {
    ...current,
    metadata: snapshot.metadata,
    pages: snapshot.pages,
    markups: snapshot.markups,
    pageScales: snapshot.pageScales,
    scalePresets: snapshot.scalePresets,
  };
}

export function clampLeftSidebarWidth(width: number): number {
  return Math.min(MAX_LEFT_SIDEBAR_WIDTH, Math.max(MIN_LEFT_SIDEBAR_WIDTH, Math.round(width)));
}

export function clampRightSidebarWidth(width: number): number {
  return Math.min(MAX_RIGHT_SIDEBAR_WIDTH, Math.max(MIN_RIGHT_SIDEBAR_WIDTH, Math.round(width)));
}

export function clampPagesPerColumn(count: number): number {
  if (!Number.isFinite(count)) {
    return DEFAULT_PAGES_PER_COLUMN;
  }
  return Math.min(MAX_PAGES_PER_COLUMN, Math.max(MIN_PAGES_PER_COLUMN, Math.round(count)));
}

let nextPageScrollRequestId = 1;
let nextDocumentScrollRequestId = 1;
let nextThumbnailScrollRequestId = 1;

const initialState = {
  document: null,
  zoom: 1,
  activeTool: 'select' as ToolMode,
  leftSidebarOpen: false,
  rightSidebarOpen: false,
  leftSidebarPanel: 'pages' as const,
  rightSidebarPanel: 'tools' as const,
  leftSidebarWidth: DEFAULT_LEFT_SIDEBAR_WIDTH,
  rightSidebarWidth: DEFAULT_RIGHT_SIDEBAR_WIDTH,
  scrollMode: 'continuous' as ScrollMode,
  continuousScrollWheelMode: 'scroll' as ScrollWheelMode,
  singlePageScrollWheelMode: 'zoom' as ScrollWheelMode,
  pageColumnsEnabled: false,
  cadViewOrganisation: 'columns' as CadViewOrganisation,
  pagesPerColumn: DEFAULT_PAGES_PER_COLUMN,
  zoomPreset: 'manual' as ZoomPreset,
  currentPage: 0,
  visiblePageIndices: [] as number[],
  selectedMarkupIds: [] as string[],
  documentHistory: createDocumentHistory(),
  postPlacement: null as PostPlacementState | null,
  snapSettings: {
    snapToContent: true,
    snapToMarkup: true,
    sensitivityPx: 8,
    snapTargets: DEFAULT_SNAP_TARGETS,
    snapGuidesEnabled: true,
    snapGuideTypes: DEFAULT_SNAP_GUIDE_TYPES,
  } satisfies SnapSettings,
  toolPropertyValues: createInitialToolPropertyValues(),
  pendingImageAsset: null as PendingImageAsset | null,
  pendingPageScroll: null as { pageIndex: number; requestId: number } | null,
  pendingDocumentScroll: null as { edge: 'top' | 'bottom'; requestId: number } | null,
  pendingThumbnailScroll: null as { pageIndex: number; requestId: number } | null,
  statusMessage: 'Open a PDF to begin.',
  errorMessage: null as string | null,
};

export const useViewerStore = create<ViewerState>((set, get) => ({
  ...initialState,
  setDocument: (document, history = createDocumentHistory()) =>
    set((state) => ({
      document: document ? { ...document, dirty: false } : null,
      documentHistory: history,
      activeTool: state.activeTool,
      leftSidebarOpen: false,
      rightSidebarOpen: false,
      currentPage: 0,
      visiblePageIndices: [],
      selectedMarkupIds: [],
      postPlacement: null,
      pendingImageAsset: null,
      pendingPageScroll: null,
      pendingDocumentScroll: null,
      pendingThumbnailScroll: null,
      zoomPreset: 'manual',
      scrollMode: 'continuous',
      continuousScrollWheelMode: 'scroll',
      singlePageScrollWheelMode: 'zoom',
      pageColumnsEnabled: false,
      cadViewOrganisation: 'columns',
      pagesPerColumn: DEFAULT_PAGES_PER_COLUMN,
    })),
  updateDocument: (updater, markDirty = true) =>
    set((state) => {
      if (!state.document) {
        return state;
      }
      const nextDocument = updater(state.document.document);
      if (nextDocument === state.document.document) {
        return state;
      }
      if (!markDirty) {
        return {
          document: { ...state.document, document: nextDocument },
        };
      }
      const nextRevision = state.documentHistory.nextRevision;
      return {
        document: {
          ...state.document,
          document: nextDocument,
          dirty: nextRevision !== state.documentHistory.savedRevision,
        },
        documentHistory: {
          past: [...state.documentHistory.past, {
            document: state.document.document,
            revision: state.documentHistory.currentRevision,
          }].slice(-DOCUMENT_HISTORY_LIMIT),
          future: [],
          currentRevision: nextRevision,
          savedRevision: state.documentHistory.savedRevision,
          nextRevision: nextRevision + 1,
        },
      };
    }),
  replaceDocumentAfterSave: (document) => set((state) => ({
    document: { ...document, dirty: false },
    documentHistory: {
      ...state.documentHistory,
      savedRevision: state.documentHistory.currentRevision,
    },
  })),
  undoDocument: () => {
    let changed = false;
    set((state) => {
      if (!state.document || state.documentHistory.past.length === 0) {
        return state;
      }
      const entry = state.documentHistory.past.at(-1);
      if (!entry) return state;
      changed = true;
      const restored = restoreEditableDocument(state.document.document, entry.document);
      return {
        document: {
          ...state.document,
          document: restored,
          dirty: entry.revision !== state.documentHistory.savedRevision,
        },
        documentHistory: {
          ...state.documentHistory,
          past: state.documentHistory.past.slice(0, -1),
          future: [{
            document: state.document.document,
            revision: state.documentHistory.currentRevision,
          }, ...state.documentHistory.future],
          currentRevision: entry.revision,
        },
        selectedMarkupIds: state.selectedMarkupIds.filter((id) => restored.markups.some((markup) => markup.id === id)),
        postPlacement: null,
      };
    });
    return changed;
  },
  redoDocument: () => {
    let changed = false;
    set((state) => {
      if (!state.document || state.documentHistory.future.length === 0) {
        return state;
      }
      const [entry, ...future] = state.documentHistory.future;
      changed = true;
      const restored = restoreEditableDocument(state.document.document, entry.document);
      return {
        document: {
          ...state.document,
          document: restored,
          dirty: entry.revision !== state.documentHistory.savedRevision,
        },
        documentHistory: {
          ...state.documentHistory,
          past: [...state.documentHistory.past, {
            document: state.document.document,
            revision: state.documentHistory.currentRevision,
          }].slice(-DOCUMENT_HISTORY_LIMIT),
          future,
          currentRevision: entry.revision,
        },
        selectedMarkupIds: state.selectedMarkupIds.filter((id) => restored.markups.some((markup) => markup.id === id)),
        postPlacement: null,
      };
    });
    return changed;
  },
  setZoom: (zoom) => set({ zoom }),
  setActiveTool: (activeTool) => set((state) => ({
    activeTool,
    postPlacement: null,
    pendingImageAsset: activeTool === 'image' ? state.pendingImageAsset : null,
  })),
  resetToSelectionTool: () => set({
    activeTool: 'select',
    selectedMarkupIds: [],
    postPlacement: null,
    pendingImageAsset: null,
  }),
  setLeftSidebarWidth: (leftSidebarWidth) => set({ leftSidebarWidth: clampLeftSidebarWidth(leftSidebarWidth) }),
  setRightSidebarWidth: (rightSidebarWidth) => set({ rightSidebarWidth: clampRightSidebarWidth(rightSidebarWidth) }),
  toggleLeftSidebar: (panel = 'pages') =>
    set((state) => ({
      leftSidebarOpen: state.leftSidebarPanel === panel ? !state.leftSidebarOpen : true,
      leftSidebarPanel: panel,
    })),
  toggleRightSidebar: (panel = 'tools') =>
    set((state) => ({
      rightSidebarOpen: state.rightSidebarPanel === panel ? !state.rightSidebarOpen : true,
      rightSidebarPanel: panel,
    })),
  openLeftSidebar: (panel) => set({ leftSidebarOpen: true, leftSidebarPanel: panel }),
  openRightSidebar: (panel) => set({ rightSidebarOpen: true, rightSidebarPanel: panel }),
  collapseLeftSidebar: () => set({ leftSidebarOpen: false }),
  collapseRightSidebar: () => set({ rightSidebarOpen: false }),
  setScrollMode: (scrollMode) => set({ scrollMode }),
  setContinuousScrollWheelMode: (continuousScrollWheelMode) => set({ continuousScrollWheelMode }),
  setSinglePageScrollWheelMode: (singlePageScrollWheelMode) => set({ singlePageScrollWheelMode }),
  setPageColumnsEnabled: (pageColumnsEnabled) =>
    set((state) => ({
      pageColumnsEnabled,
      zoomPreset: pageColumnsEnabled ? 'manual' : state.zoomPreset,
    })),
  setCadViewOrganisation: (cadViewOrganisation) => set({ cadViewOrganisation }),
  setPagesPerColumn: (pagesPerColumn) => set({ pagesPerColumn: clampPagesPerColumn(pagesPerColumn) }),
  setZoomPreset: (zoomPreset) => set({ zoomPreset }),
  setCurrentPage: (currentPage) => set({ currentPage }),
  setVisiblePageIndices: (visiblePageIndices) => set({ visiblePageIndices }),
  setSelectedMarkupIds: (selectedMarkupIds) => set((state) => ({
    selectedMarkupIds,
    postPlacement: state.postPlacement && selectedMarkupIds.includes(state.postPlacement.markupId)
      ? state.postPlacement
      : null,
  })),
  setPostPlacement: (postPlacement) => set({ postPlacement }),
  setSnapSettings: (settings) =>
    set((state) => ({
      snapSettings: {
        ...state.snapSettings,
        ...settings,
        sensitivityPx: clampSnapSensitivity(settings.sensitivityPx ?? state.snapSettings.sensitivityPx),
        snapTargets: normalizeSnapTargets(settings.snapTargets ?? state.snapSettings.snapTargets),
        snapGuideTypes: normalizeSnapGuideTypes(settings.snapGuideTypes ?? state.snapSettings.snapGuideTypes),
      },
    })),
  setToolPropertyValue: (tool, key, value) =>
    set((state) => ({
      toolPropertyValues: {
        ...state.toolPropertyValues,
        [tool]: {
          ...builtInToolPropertyValues(tool),
          ...state.toolPropertyValues[tool],
          [key]: value,
        },
      },
    })),
  resetToolPropertyValues: (tool) =>
    set((state) => ({
      toolPropertyValues: {
        ...state.toolPropertyValues,
        [tool]: builtInToolPropertyValues(tool),
      },
    })),
  setPendingImageAsset: (pendingImageAsset) => set({ pendingImageAsset }),
  consumePendingImageAsset: () => {
    const asset = get().pendingImageAsset;
    set({ pendingImageAsset: null });
    return asset;
  },
  requestPageScroll: (pageIndex) => {
    const requestId = nextPageScrollRequestId;
    nextPageScrollRequestId += 1;
    set({ pendingPageScroll: { pageIndex, requestId }, pendingDocumentScroll: null });
    return requestId;
  },
  consumePageScroll: (requestId) =>
    set((state) => {
      if (state.pendingPageScroll?.requestId !== requestId) {
        return state;
      }

      return { pendingPageScroll: null };
    }),
  requestDocumentScroll: (edge) => {
    const requestId = nextDocumentScrollRequestId;
    nextDocumentScrollRequestId += 1;
    set({ pendingDocumentScroll: { edge, requestId }, pendingPageScroll: null });
    return requestId;
  },
  consumeDocumentScroll: (requestId) =>
    set((state) => {
      if (state.pendingDocumentScroll?.requestId !== requestId) {
        return state;
      }

      return { pendingDocumentScroll: null };
    }),
  requestThumbnailScroll: (pageIndex) => {
    const requestId = nextThumbnailScrollRequestId;
    nextThumbnailScrollRequestId += 1;
    set({ pendingThumbnailScroll: { pageIndex, requestId } });
    return requestId;
  },
  consumeThumbnailScroll: (requestId) =>
    set((state) => {
      if (state.pendingThumbnailScroll?.requestId !== requestId) {
        return state;
      }

      return { pendingThumbnailScroll: null };
    }),
  setStatusMessage: (statusMessage) => set({ statusMessage }),
  setErrorMessage: (errorMessage) => set({ errorMessage }),
}));

function clampSnapSensitivity(value: number): number {
  return Math.min(24, Math.max(2, Math.round(value)));
}

function normalizeSnapTargets(targets: readonly SnapTarget[]): readonly SnapTarget[] {
  const allowed = new Set<SnapTarget>(['endpoint', 'midpoint', 'center', 'intersection', 'nearest']);
  const unique = targets.filter((target, index) => allowed.has(target) && targets.indexOf(target) === index);
  return unique.length > 0 ? unique : DEFAULT_SNAP_TARGETS;
}

function normalizeSnapGuideTypes(types: readonly SnapGuideType[]): readonly SnapGuideType[] {
  const allowed = new Set<SnapGuideType>(DEFAULT_SNAP_GUIDE_TYPES);
  return types.filter((type, index) => allowed.has(type) && types.indexOf(type) === index);
}

export function resetViewerStore(): void {
  nextPageScrollRequestId = 1;
  nextDocumentScrollRequestId = 1;
  nextThumbnailScrollRequestId = 1;
  useViewerStore.setState(initialState);
}
