import { useMemo } from 'react';
import type { PdfRect } from '../../../shared/protocol';
import type {
  LocalPdfSession,
  PageRenderSurface,
  RenderRequestClass,
  ReusablePageImage,
} from './documentSession';
import type { PageModel } from '@butter-paper/core';

export type RenderCoordinatorRole = 'target-page' | 'main-page' | 'overview-page' | 'sidebar-thumbnail';
export type RenderCoordinatorQuality = 'stale-preview' | 'preview' | 'full' | 'detail';
export type RenderCoordinatorSourceKind = 'page-surface' | 'page-url' | 'thumbnail-url' | 'overview-url';
export type RenderCoordinatorState =
  | 'empty'
  | 'showing-stale'
  | 'showing-preview'
  | 'showing-full'
  | 'showing-detail'
  | 'upgrading'
  | 'errored';
export type RenderCoordinatorTier = 1 | 2 | 3 | 4 | 5 | 6;
export type RenderCoordinatorUrgency = 'visible' | 'prefetch';

export interface RenderCoordinatorIntent {
  readonly role: RenderCoordinatorRole;
  readonly pageIndex: number;
  readonly isVisible: boolean;
  readonly hasDisplayedSource: boolean;
  readonly displayedQuality: RenderCoordinatorQuality | null;
  readonly viewportInMotion: boolean;
  readonly renderUrgency: RenderCoordinatorUrgency;
}

export interface ThumbnailRenderBounds {
  readonly maxWidth: number;
  readonly maxHeight: number;
  readonly pixelRatio?: number;
  readonly minScale?: number;
  readonly pageWidth?: number;
  readonly pageHeight?: number;
}

export interface CoordinatedRenderOptions {
  readonly priority?: number;
  readonly urgency?: RenderCoordinatorUrgency;
  readonly requestClass?: RenderRequestClass;
  readonly abortStartedRender?: boolean;
  readonly signal?: AbortSignal;
  readonly cropPdfRect?: PdfRect;
  readonly rotation?: PageModel['rotation'];
}

export interface RenderCoordinatorDiagnostics {
  enabled: boolean;
  sourceSelections: number;
  staleRetentions: number;
  blankAvoidanceSelections: number;
  directRenderFallbacks: number;
  requestsByRole: Record<RenderCoordinatorRole, number>;
  requestsByTier: Record<RenderCoordinatorTier, number>;
}

const EMPTY_REQUEST_COUNTS_BY_ROLE: Record<RenderCoordinatorRole, number> = {
  'target-page': 0,
  'main-page': 0,
  'overview-page': 0,
  'sidebar-thumbnail': 0,
};

const EMPTY_REQUEST_COUNTS_BY_TIER: Record<RenderCoordinatorTier, number> = {
  1: 0,
  2: 0,
  3: 0,
  4: 0,
  5: 0,
  6: 0,
};

const globalRenderCoordinatorDiagnostics: RenderCoordinatorDiagnostics = {
  enabled: false,
  sourceSelections: 0,
  staleRetentions: 0,
  blankAvoidanceSelections: 0,
  directRenderFallbacks: 0,
  requestsByRole: { ...EMPTY_REQUEST_COUNTS_BY_ROLE },
  requestsByTier: { ...EMPTY_REQUEST_COUNTS_BY_TIER },
};

export class RenderCoordinator {
  constructor(private readonly session: LocalPdfSession) {}

  diagnostics(): RenderCoordinatorDiagnostics {
    return getRenderCoordinatorDiagnostics();
  }

  selectBestReusableSource({
    pageIndex,
    minimumDisplayWidth,
    role,
    hasDisplayedSource,
    rotation,
  }: {
    pageIndex: number;
    minimumDisplayWidth: number;
    role: RenderCoordinatorRole;
    hasDisplayedSource: boolean;
    rotation?: PageModel['rotation'];
  }): ReusablePageImage | null {
    if (hasDisplayedSource) {
      globalRenderCoordinatorDiagnostics.staleRetentions += 1;
      return null;
    }

    globalRenderCoordinatorDiagnostics.sourceSelections += 1;
    const source = role === 'sidebar-thumbnail'
      ? this.session.getBestReusableThumbnailImage(pageIndex, minimumDisplayWidth, rotation)
        ?? this.session.getBestReusablePageImage(pageIndex, minimumDisplayWidth, rotation)
      : this.session.getBestReusablePageImage(pageIndex, minimumDisplayWidth, rotation)
        ?? this.session.getBestReusableThumbnailImage(pageIndex, minimumDisplayWidth, rotation);

    if (source) {
      globalRenderCoordinatorDiagnostics.blankAvoidanceSelections += 1;
    }

    return source;
  }

  getReusablePreviewUrl(
    pageIndex: number,
    minimumRenderedWidth?: number,
    rotation?: PageModel['rotation'],
  ): string | null {
    return this.session.getReusablePagePreviewInfo(pageIndex, minimumRenderedWidth, rotation)?.objectUrl ?? null;
  }

  consumePrimedPagePreview(pageIndex: number): string | null {
    return this.session.consumePrimedPagePreview(pageIndex);
  }

  async renderPageUrl(
    role: RenderCoordinatorRole,
    pageIndex: number,
    zoom: number,
    pixelRatio: number,
    options: CoordinatedRenderOptions = {},
  ): Promise<string> {
    this.recordIntent(role, options);
    return this.session.renderPage(pageIndex, zoom, pixelRatio, options);
  }

  async renderPageSurface(
    role: RenderCoordinatorRole,
    pageIndex: number,
    zoom: number,
    pixelRatio: number,
    options: CoordinatedRenderOptions = {},
  ): Promise<PageRenderSurface> {
    this.recordIntent(role, options);
    return this.session.renderPageBitmap(pageIndex, zoom, pixelRatio, options);
  }

  async renderThumbnailUrl(
    role: RenderCoordinatorRole,
    pageIndex: number,
    bounds: ThumbnailRenderBounds,
    pixelRatio: number,
    options: CoordinatedRenderOptions = {},
  ): Promise<string> {
    this.recordIntent(role, options);
    return this.session.renderThumbnail(pageIndex, bounds, pixelRatio, options);
  }

  updatePageUrlPriority(
    role: RenderCoordinatorRole,
    pageIndex: number,
    zoom: number,
    pixelRatio: number,
    options: Pick<CoordinatedRenderOptions, 'priority' | 'urgency' | 'requestClass' | 'rotation'>,
  ): void {
    this.recordIntent(role, options);
    this.session.updatePageRenderPriority(pageIndex, zoom, pixelRatio, options);
  }

  updatePageSurfacePriority(
    role: RenderCoordinatorRole,
    pageIndex: number,
    zoom: number,
    pixelRatio: number,
    options: Pick<CoordinatedRenderOptions, 'priority' | 'urgency' | 'requestClass' | 'rotation'>,
  ): void {
    this.recordIntent(role, options);
    this.session.updatePageBitmapRenderPriority(pageIndex, zoom, pixelRatio, options);
  }

  updateThumbnailPriority(
    role: RenderCoordinatorRole,
    pageIndex: number,
    bounds: ThumbnailRenderBounds,
    pixelRatio: number,
    options: Pick<CoordinatedRenderOptions, 'priority' | 'urgency' | 'requestClass' | 'rotation'>,
  ): void {
    this.recordIntent(role, options);
    this.session.updateThumbnailRenderPriority(pageIndex, bounds, pixelRatio, options);
  }

  private recordIntent(role: RenderCoordinatorRole, options: Pick<CoordinatedRenderOptions, 'urgency' | 'requestClass'>): void {
    const tier = resolveRenderCoordinatorTier({
      role,
      isVisible: options.urgency !== 'prefetch',
      hasDisplayedSource: isUpgradeRequestClass(options.requestClass),
      displayedQuality: isUpgradeRequestClass(options.requestClass) ? 'preview' : null,
      viewportInMotion: false,
      renderUrgency: options.urgency ?? 'visible',
      pageIndex: 0,
    });
    globalRenderCoordinatorDiagnostics.requestsByRole[role] += 1;
    globalRenderCoordinatorDiagnostics.requestsByTier[tier] += 1;
    if (!globalRenderCoordinatorDiagnostics.enabled) {
      globalRenderCoordinatorDiagnostics.directRenderFallbacks += 1;
    }
  }
}

export function useRenderCoordinator(session: LocalPdfSession): RenderCoordinator;
export function useRenderCoordinator(session: LocalPdfSession | null): RenderCoordinator | null;
export function useRenderCoordinator(session: LocalPdfSession | null): RenderCoordinator | null {
  return useMemo(() => (session ? new RenderCoordinator(session) : null), [session]);
}

export function isRenderCoordinatorV2Enabled(): boolean {
  return Boolean(window.butterPaper?.environment.renderCoordinatorV2);
}

export function getRenderCoordinatorDiagnostics(): RenderCoordinatorDiagnostics {
  globalRenderCoordinatorDiagnostics.enabled = isRenderCoordinatorV2Enabled();
  return {
    ...globalRenderCoordinatorDiagnostics,
    requestsByRole: { ...globalRenderCoordinatorDiagnostics.requestsByRole },
    requestsByTier: { ...globalRenderCoordinatorDiagnostics.requestsByTier },
  };
}

export function resolveCoordinatorStateAfterSource({
  currentState,
  hasDisplayedSource,
  nextQuality,
}: {
  currentState: RenderCoordinatorState;
  hasDisplayedSource: boolean;
  nextQuality: RenderCoordinatorQuality | null;
}): RenderCoordinatorState {
  if (!nextQuality) {
    return hasDisplayedSource && currentState !== 'empty' && currentState !== 'errored'
      ? currentState
      : 'empty';
  }

  if (nextQuality === 'stale-preview') {
    return 'showing-stale';
  }

  if (nextQuality === 'preview') {
    return 'showing-preview';
  }

  if (nextQuality === 'full') {
    return 'showing-full';
  }

  return 'showing-detail';
}

export function resolveStateAfterRenderAbort(currentState: RenderCoordinatorState): RenderCoordinatorState {
  return currentState === 'empty' || currentState === 'errored'
    ? currentState
    : currentState;
}

export function resolveRenderCoordinatorTier(intent: RenderCoordinatorIntent): RenderCoordinatorTier {
  if (!intent.isVisible) {
    return intent.renderUrgency === 'prefetch' ? 5 : 6;
  }

  if (!intent.hasDisplayedSource) {
    return intent.role === 'target-page' || intent.role === 'main-page' ? 1 : 2;
  }

  return intent.role === 'target-page' || intent.role === 'main-page' ? 3 : 4;
}

export function renderRequestClassForRole({
  role,
  quality,
  urgency,
}: {
  role: RenderCoordinatorRole;
  quality: 'preview' | 'full' | 'detail';
  urgency: RenderCoordinatorUrgency;
}): RenderRequestClass {
  if (urgency === 'prefetch') {
    return 'nearby-prefetch';
  }

  if (role === 'sidebar-thumbnail') {
    return 'visible-thumbnail';
  }

  if (role === 'overview-page') {
    return 'overview-thumbnail';
  }

  if (role === 'target-page') {
    return quality === 'preview' ? 'target-page-preview' : 'target-page-hq';
  }

  return quality === 'preview' ? 'visible-page-preview' : 'visible-page-hq-upgrade';
}

export function sourceKindForReusableImage(source: ReusablePageImage): RenderCoordinatorSourceKind {
  if (source.kind === 'surface') {
    return 'page-surface';
  }

  if (source.source === 'thumbnail') {
    return source.sourceRequestClass === 'overview-thumbnail' ? 'overview-url' : 'thumbnail-url';
  }

  return 'page-url';
}

export function shouldRetainDisplayedSourceDuringTransition(hasDisplayedSource: boolean): boolean {
  return hasDisplayedSource;
}

function isUpgradeRequestClass(requestClass: RenderRequestClass | undefined): boolean {
  return requestClass === 'target-page-hq'
    || requestClass === 'target-page-crop'
    || requestClass === 'visible-page-hq-upgrade';
}
