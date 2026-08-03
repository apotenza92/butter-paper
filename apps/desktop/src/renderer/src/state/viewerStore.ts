import { create } from 'zustand';
import type { DocumentModel } from '@butter-paper/core';
import type {
  LoadedDocumentPayload,
  ScrollMode,
  ToolMode,
  ZoomPreset,
} from '../../../shared/protocol';

export interface PendingImageAsset {
  readonly dataUrl: string;
  readonly mimeType: 'image/png' | 'image/jpeg';
  readonly width: number;
  readonly height: number;
  readonly fileName: string;
}

export interface LoadedDocumentState extends LoadedDocumentPayload {
  dirty: boolean;
}

export type SnapTarget = 'endpoint' | 'midpoint' | 'center' | 'intersection' | 'nearest';
export type ScrollWheelMode = 'zoom' | 'scroll';
export type CadViewOrganisation = 'columns' | 'rows';

export const DEFAULT_SNAP_TARGETS: readonly SnapTarget[] = ['endpoint', 'midpoint', 'center', 'intersection'];

export interface SnapSettings {
  readonly snapToContent: boolean;
  readonly snapToMarkup: boolean;
  readonly sensitivityPx: number;
  readonly snapTargets: readonly SnapTarget[];
}

interface ViewerState {
  document: LoadedDocumentState | null;
  zoom: number;
  activeTool: ToolMode;
  leftSidebarOpen: boolean;
  rightSidebarOpen: boolean;
  leftSidebarPanel: 'pages';
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
  snapSettings: SnapSettings;
  pendingImageAsset: PendingImageAsset | null;
  pendingPageScroll: { pageIndex: number; requestId: number } | null;
  pendingThumbnailScroll: { pageIndex: number; requestId: number } | null;
  statusMessage: string;
  errorMessage: string | null;
  setDocument: (document: LoadedDocumentPayload | null) => void;
  updateDocument: (updater: (document: DocumentModel) => DocumentModel, markDirty?: boolean) => void;
  setZoom: (zoom: number) => void;
  setActiveTool: (tool: ToolMode) => void;
  setLeftSidebarWidth: (width: number) => void;
  setRightSidebarWidth: (width: number) => void;
  toggleLeftSidebar: (panel?: 'pages') => void;
  toggleRightSidebar: (panel?: 'tools') => void;
  openLeftSidebar: (panel: 'pages') => void;
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
  setSnapSettings: (settings: Partial<SnapSettings>) => void;
  setPendingImageAsset: (asset: PendingImageAsset | null) => void;
  consumePendingImageAsset: () => PendingImageAsset | null;
  requestPageScroll: (pageIndex: number) => number;
  consumePageScroll: (requestId: number) => void;
  requestThumbnailScroll: (pageIndex: number) => number;
  consumeThumbnailScroll: (requestId: number) => void;
  setStatusMessage: (message: string) => void;
  setErrorMessage: (message: string | null) => void;
}

export const DEFAULT_LEFT_SIDEBAR_WIDTH = 220;
export const MIN_LEFT_SIDEBAR_WIDTH = 180;
export const MAX_LEFT_SIDEBAR_WIDTH = 360;
export const DEFAULT_RIGHT_SIDEBAR_WIDTH = 280;
export const MIN_RIGHT_SIDEBAR_WIDTH = 220;
export const MAX_RIGHT_SIDEBAR_WIDTH = 420;
export const DEFAULT_PAGES_PER_COLUMN = 10;
export const MIN_PAGES_PER_COLUMN = 1;
export const MAX_PAGES_PER_COLUMN = 100;

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
  snapSettings: {
    snapToContent: true,
    snapToMarkup: true,
    sensitivityPx: 8,
    snapTargets: DEFAULT_SNAP_TARGETS,
  } satisfies SnapSettings,
  pendingImageAsset: null as PendingImageAsset | null,
  pendingPageScroll: null as { pageIndex: number; requestId: number } | null,
  pendingThumbnailScroll: null as { pageIndex: number; requestId: number } | null,
  statusMessage: 'Open a PDF to begin.',
  errorMessage: null as string | null,
};

export const useViewerStore = create<ViewerState>((set, get) => ({
  ...initialState,
  setDocument: (document) =>
    set(() => ({
      document: document ? { ...document, dirty: false } : null,
      leftSidebarOpen: false,
      rightSidebarOpen: false,
      currentPage: 0,
      visiblePageIndices: [],
      selectedMarkupIds: [],
      pendingImageAsset: null,
      pendingPageScroll: null,
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

      return {
        document: {
          ...state.document,
          document: updater(state.document.document),
          dirty: state.document.dirty || markDirty,
        },
      };
    }),
  setZoom: (zoom) => set({ zoom }),
  setActiveTool: (activeTool) => set({ activeTool }),
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
  setSelectedMarkupIds: (selectedMarkupIds) => set({ selectedMarkupIds }),
  setSnapSettings: (settings) =>
    set((state) => ({
      snapSettings: {
        ...state.snapSettings,
        ...settings,
        sensitivityPx: clampSnapSensitivity(settings.sensitivityPx ?? state.snapSettings.sensitivityPx),
        snapTargets: normalizeSnapTargets(settings.snapTargets ?? state.snapSettings.snapTargets),
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
    set({ pendingPageScroll: { pageIndex, requestId } });
    return requestId;
  },
  consumePageScroll: (requestId) =>
    set((state) => {
      if (state.pendingPageScroll?.requestId !== requestId) {
        return state;
      }

      return { pendingPageScroll: null };
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

export function resetViewerStore(): void {
  nextPageScrollRequestId = 1;
  nextThumbnailScrollRequestId = 1;
  useViewerStore.setState(initialState);
}
