import { useEffect, useMemo, useRef, useState } from 'react';
import {
  createMarkup,
  createImageMarkup,
  createPageTransform,
  createSnapshotMarkup,
  createTextBoxMarkup,
  pdfPoint,
  rect,
  requirePageScale,
  translateMarkup,
  updateMarkupText,
  type DocumentModel,
  type Markup,
  type PageModel,
  type PageScale,
  type PdfPoint,
  type Rect,
  type TextBoxMarkup,
  type TextBoxRichTextRun,
} from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';
import type { PendingImageAsset, PostPlacementState, SnapGuideType, SnapTarget } from '../state/viewerStore';
import {
  addCloudNodeDraftPoint,
  addMeasurementPathDraftPoint,
  addVertexPathDraftPoint,
  beginCloudNodeDraftPoint,
  createArcDraft,
  createCloudNodeDraft,
  createMeasurementPathDraft,
  createMoveDraft,
  createTransformDraft,
  createVertexPathDraft,
  hasExceededDragThreshold,
  measurementPathPreviewPoints,
  moveDelta,
  rectangleDraftToRect,
  setArcDraftEnd,
  shouldCommitLine,
  updateLineDraft,
  updateMeasurementPathDraft,
  updateArcDraft,
  updateTransformDraft,
  updateVertexPathDraft,
  type AnnotationDraft,
  type MeasurementPathDraft,
  type TransformDraft,
  type VertexPathDraft,
} from '../pdf-tools/annotationLifecycle';
import { constrainArcBulgePoint, createArcMarkupFromThreePoints, snapArcBulgePoint } from '../pdf-tools/builtins/arcTool';
import { updateCloudPlusTextBox } from '../pdf-tools/builtins/cloudPlusTool';
import { constrainEllipseDraftPoint } from '../pdf-tools/builtins/ellipseTool';
import { createAreaMeasurementMarkup, createLengthMeasurementMarkup, createPolylengthMeasurementMarkup } from '../pdf-tools/builtins/measurementTool';
import { dimensionCaptionRect } from '../pdf-tools/builtins/dimensionTool';
import { annotationFontCssFamily, getAnnotationContentStyle, getAnnotationTextContentStyle, getVerticallyCenteredAnnotationTextContentStyle } from '../pdf-tools/annotationStyles';
import { DEFAULT_IMAGE_DATA_URL } from '../pdf-tools/builtins/imageTool';
import { getChromeHandleStyle, getChromeStyle, getMoveCursor, getResizeCursor, getRotateCursor, type ChromeBoundsKind, type InteractionState } from '../pdf-tools/interactionChrome';
import { expandViewportRect, projectChromeHandlePoint, unrotateViewportPointAroundBounds } from '../pdf-tools/selectionHitZones';
import { layoutTextBoxLines, measureAnnotationText, splitAnnotationTextLines, type TextMeasurementContext } from '../pdf-tools/textLayout';
import { getMarkupToolDefinition, getToolDefinition } from '../pdf-tools/toolRegistry';
import {
  constrainPointOrthogonally,
  findEqualSizeSnap,
  findEqualSpacingSnap,
  findNearestSnapPoint,
  findObjectSnapTrackingPoint,
  getAnnotationGuideRects,
  getAnnotationSnapCandidates,
  isPointOnOrthogonalConstraint,
  toggleAcquiredTrackingPoint,
  trackingPointKey,
  type AcquiredTrackingPoint,
  type ObjectSnapTrackingResult,
  type OrthogonalConstraint,
  type RelationshipSnapGuide,
  type SnapCandidate,
  type SnapResult,
} from '../pdf-tools/snapping';
import { SNAP_MARKER_COLOR, SNAP_MARKER_RADIUS_PX, SNAP_MARKER_STROKE_WIDTH_PX } from './snapMarkerVisuals';
import { ToolRailIcon } from './RailIcons';
import {
  createArmedBoxSelectionMarquee,
  createSelectionMarquee,
  isGeometrySelectedByMarquee,
  resolvedSelectionMarqueeKind,
  selectionAfterMarquee,
  selectionMarqueeBounds,
  selectionMarqueeOperationFromModifiers,
  updateSelectionMarquee,
  type SelectionMarqueeState,
  type ViewportPoint,
} from '../pdf-tools/selectionMarquee';
import type { RenderPrimitive, SelectionChromeDescriptor, ToolHandleDescriptor, ToolHit, ToolInteractionContext } from '../pdf-tools/types';
import { applyToolPropertyValues, type ToolPropertyValues, type ToolPropertyValuesByTool } from '../pdf-tools/toolPropertyDefaults';

type PageTransform = ReturnType<typeof createPageTransform>;
const IMPORTED_MARKUP_MIN_RENDER_ZOOM = 0.35;
const TEXT_BOX_GHOST_CURSOR = createTextBoxGhostCursor();
const TOOL_CURSOR_ICON_OFFSET_PX = 20;
const ROTATION_HANDLE_OFFSET_PX = 12;
const ARC_MIN_BULGE_PX = 8;
export const FINISH_CLOUD_POLYGON_EVENT = 'butter-paper:finish-cloud-polygon';

function toolCursorIconTransform(point: ViewportPoint): string {
  return `translate3d(${point.x + TOOL_CURSOR_ICON_OFFSET_PX}px, ${point.y + TOOL_CURSOR_ICON_OFFSET_PX}px, 0)`;
}

interface TextEditState {
  readonly markupId: string;
  readonly text: string;
}

interface HotHandleState {
  readonly markupId: string;
  readonly handleId: string;
}

interface TextBoxAutosizeOptions extends TextMeasurementContext {
  readonly insetPt?: number;
  readonly lineHeightPt?: number;
  readonly measureText?: (text: string, context: TextMeasurementContext) => number;
}

interface TextBoxCaretGeometryOptions extends TextMeasurementContext {
  readonly lineHeightPt?: number;
  readonly measureText?: (text: string, context: TextMeasurementContext) => number;
}

interface SnapPointOptions {
  readonly enabled: boolean;
  readonly excludeMarkupIds?: readonly string[];
  readonly orthogonalAnchor?: PdfPoint | null;
  readonly acquireTracking?: boolean;
  readonly anchorPoints?: readonly PdfPoint[];
}

type EditableTextMarkup = Extract<Markup, { kind: 'text-box' | 'callout' | 'cloud-plus' | 'dimension' }>;

function isCloudTool(tool: ToolMode): tool is Extract<ToolMode, 'cloud' | 'cloud-plus'> {
  return tool === 'cloud' || tool === 'cloud-plus';
}

export function isPostPlacementSelectionActive(
  postPlacement: PostPlacementState | null,
  activeTool: ToolMode,
  selectedMarkupIds: readonly string[],
  draftActive: boolean,
): boolean {
  return Boolean(
    postPlacement
    && postPlacement.tool === activeTool
    && selectedMarkupIds.includes(postPlacement.markupId)
    && !draftActive,
  );
}

export function shouldSelectMarkupAfterHandleTransform(
  draft: Pick<TransformDraft, 'startPoint' | 'dragStarted'>,
  currentPoint: PdfPoint,
  transform: PageTransform,
): boolean {
  if (draft.dragStarted) {
    return false;
  }

  return !hasExceededDragThreshold(
    transform.pdfToViewport(draft.startPoint),
    transform.pdfToViewport(currentPoint),
  );
}

export function selectionAfterMarkupClick(
  selectedMarkupIds: readonly string[],
  markupId: string,
  toggle: boolean,
): string[] {
  if (!toggle) {
    return selectedMarkupIds.includes(markupId) ? [...selectedMarkupIds] : [markupId];
  }

  return selectedMarkupIds.includes(markupId)
    ? selectedMarkupIds.filter((selectedMarkupId) => selectedMarkupId !== markupId)
    : [...selectedMarkupIds, markupId];
}

export function shouldConsumeSelectionClickAway(
  selectedMarkupIds: readonly string[],
  operation: SelectionMarqueeState['operation'],
): boolean {
  return selectedMarkupIds.length > 0 && operation === 'replace';
}

export function isDirectManipulationTool(activeTool: ToolMode): boolean {
  return activeTool === 'select';
}

export function resolveAnnotationCursor({
  activeTool,
  calibrationPickActive = false,
  selectionMarqueeActive = false,
  pendingTextBoxActive = false,
  textEditActive = false,
  postPlacementSelectionActive = false,
  pendingImagePreviewActive = false,
  transformDragActive = false,
  transformSnapActive = false,
  hoverCursor = null,
}: {
  readonly activeTool: ToolMode;
  readonly calibrationPickActive?: boolean;
  readonly selectionMarqueeActive?: boolean;
  readonly pendingTextBoxActive?: boolean;
  readonly textEditActive?: boolean;
  readonly postPlacementSelectionActive?: boolean;
  readonly pendingImagePreviewActive?: boolean;
  readonly transformDragActive?: boolean;
  readonly transformSnapActive?: boolean;
  readonly hoverCursor?: string | null;
}): string {
  if (calibrationPickActive || selectionMarqueeActive) {
    return 'crosshair';
  }
  if (activeTool === 'text-box' && pendingTextBoxActive) {
    return 'default';
  }
  if (activeTool === 'text-box' && !textEditActive) {
    return TEXT_BOX_GHOST_CURSOR;
  }
  if (activeTool === 'image' && pendingImagePreviewActive) {
    return 'none';
  }
  if (transformDragActive) {
    return transformSnapActive ? 'none' : 'crosshair';
  }
  if (postPlacementSelectionActive) {
    return 'default';
  }
  if (!isDirectManipulationTool(activeTool)) {
    return getToolDefinition(activeTool).cursor;
  }
  return hoverCursor ?? getToolDefinition(activeTool).cursor;
}

interface AnnotationLayerProps {
  page: PageModel;
  pageScale?: PageScale;
  markups: readonly Markup[];
  transform: PageTransform;
  pdfContentSnapCandidates?: readonly SnapCandidate[];
  snapToContent?: boolean;
  snapToMarkup?: boolean;
  snapTolerancePx?: number;
  snapTargets?: readonly SnapTarget[];
  snapGuidesEnabled?: boolean;
  snapGuideTypes?: readonly SnapGuideType[];
  dimensionIncrementMm?: number | null;
  activeTool: ToolMode;
  toolPropertyValues?: ToolPropertyValuesByTool;
  selectedMarkupIds: readonly string[];
  postPlacement: PostPlacementState | null;
  pendingImageAsset: PendingImageAsset | null;
  setSelectedMarkupIds: (markupIds: string[]) => void;
  setPostPlacement: (postPlacement: PostPlacementState | null) => void;
  consumePendingImageAsset: () => PendingImageAsset | null;
  onMarkupPlaced?: () => void;
  onImagePlaced?: () => void;
  onToggleProperties?: (wasSelectedBeforeDoubleClick: boolean) => void;
  createSnapshotDataUrl?: (rect: Rect) => string | null;
  updateDocument: (updater: (document: DocumentModel) => DocumentModel) => void;
  onToolError?: (message: string) => void;
  calibrationPickActive?: boolean;
  calibrationStartPoint?: PdfPoint | null;
  onCalibrationPoint?: (pageIndex: number, point: PdfPoint) => void;
}

interface ReadOnlyAnnotationLayerProps {
  page: PageModel;
  pageScale?: PageScale;
  markups: readonly Markup[];
  transform: PageTransform;
  testId?: string;
}

export function ReadOnlyAnnotationLayer({
  page,
  pageScale,
  markups,
  transform,
  testId,
}: ReadOnlyAnnotationLayerProps) {
  return (
    <svg
      className="pointer-events-none absolute inset-0 h-full w-full overflow-visible"
      data-testid={testId ?? `read-only-annotation-layer-${page.index + 1}`}
      aria-hidden="true"
    >
      {markups.map((markup) => (
        <ReadOnlyMarkup key={markup.id} markup={markup} transform={transform} page={page} pageScale={pageScale} />
      ))}
    </svg>
  );
}

export function AnnotationLayer({
  page,
  pageScale,
  markups,
  transform,
  pdfContentSnapCandidates = [],
  snapToContent = true,
  snapToMarkup = true,
  snapTolerancePx = 8,
  snapTargets,
  snapGuidesEnabled = true,
  snapGuideTypes = ['alignment', 'equal-size', 'equal-spacing'],
  dimensionIncrementMm = null,
  activeTool,
  toolPropertyValues = {},
  selectedMarkupIds,
  postPlacement,
  pendingImageAsset,
  setSelectedMarkupIds,
  setPostPlacement,
  consumePendingImageAsset,
  onMarkupPlaced,
  onImagePlaced,
  onToggleProperties,
  createSnapshotDataUrl,
  updateDocument,
  onToolError,
  calibrationPickActive = false,
  calibrationStartPoint = null,
  onCalibrationPoint,
}: AnnotationLayerProps) {
  const svgRef = useRef<SVGSVGElement | null>(null);
  const toolCursorIconRef = useRef<HTMLDivElement | null>(null);
  const toolCursorPointRef = useRef<ViewportPoint | null>(null);
  const toolCursorInsideRef = useRef(false);
  const trackingHoverKeyRef = useRef<string | null>(null);
  const [draft, setDraft] = useState<AnnotationDraft | null>(null);
  const interactionPreviewRef = useRef<readonly Markup[] | null>(null);
  const [interactionPreview, setInteractionPreviewState] = useState<readonly Markup[] | null>(null);
  const activeToolRef = useRef(activeTool);
  const clickPlacementToolRef = useRef<ToolMode | null>(null);
  const propertiesClickCandidateRef = useRef<{ markupId: string; wasSelected: boolean } | null>(null);
  const [hoveredMarkupId, setHoveredMarkupId] = useState<string | null>(null);
  const [hotHandle, setHotHandle] = useState<HotHandleState | null>(null);
  const [hoverCursor, setHoverCursor] = useState<string | null>(null);
  const [textEdit, setTextEdit] = useState<TextEditState | null>(null);
  const [pendingTextBox, setPendingTextBox] = useState<TextBoxMarkup | null>(null);
  const [snapResult, setSnapResult] = useState<SnapResult | null>(null);
  const [acquiredTrackingPoints, setAcquiredTrackingPoints] = useState<readonly AcquiredTrackingPoint[]>([]);
  const [objectSnapTrackingResult, setObjectSnapTrackingResult] = useState<ObjectSnapTrackingResult | null>(null);
  const [orthogonalConstraint, setOrthogonalConstraint] = useState<OrthogonalConstraint | null>(null);
  const [relationshipSnapGuides, setRelationshipSnapGuides] = useState<readonly RelationshipSnapGuide[]>([]);
  const [selectionMarquee, setSelectionMarquee] = useState<SelectionMarqueeState | null>(null);
  const [pendingImagePoint, setPendingImagePoint] = useState<PdfPoint | null>(null);
  const [calibrationHoverPoint, setCalibrationHoverPoint] = useState<PdfPoint | null>(null);

  const renderedMarkups = useMemo(
    () => applyMarkupPreview(markups, interactionPreview),
    [interactionPreview, markups],
  );
  const selectedMarkupIdSet = useMemo(() => new Set(selectedMarkupIds), [selectedMarkupIds]);
  const visibleMarkups = useMemo(() => (
    renderedMarkups.filter((markup) => shouldRenderMarkupAtZoom(markup, transform))
  ), [renderedMarkups, transform.zoom]);
  const toolInteractionContext = useMemo(
    () => interactionContextForPage(page, renderedMarkups),
    [page, renderedMarkups],
  );
  const annotationSnapCandidates = useMemo(() => (
    getAnnotationSnapCandidates(visibleMarkups, page)
  ), [visibleMarkups, page]);
  const postPlacementSelectionActive = isPostPlacementSelectionActive(
    postPlacement,
    activeTool,
    selectedMarkupIds,
    draft !== null,
  );

  useEffect(() => {
    if (calibrationPickActive) {
      setDraft(null);
      setSelectionMarquee(null);
      return;
    }
    setCalibrationHoverPoint(null);
    setSnapResult(null);
    setObjectSnapTrackingResult(null);
    setOrthogonalConstraint(null);
  }, [calibrationPickActive]);

  useEffect(() => {
    const previousTool = activeToolRef.current;
    if (previousTool === activeTool) {
      return;
    }

    activeToolRef.current = activeTool;
    const cursorIcon = toolCursorIconRef.current;
    const cursorPoint = toolCursorPointRef.current;
    if (cursorIcon && cursorPoint && toolCursorInsideRef.current) {
      cursorIcon.style.transform = toolCursorIconTransform(cursorPoint);
      cursorIcon.style.opacity = '1';
    }
    setPostPlacement(null);
    setHoveredMarkupId(null);
    setHotHandle(null);
    setHoverCursor(null);
    setSnapResult(null);
    setAcquiredTrackingPoints([]);
    setObjectSnapTrackingResult(null);
    setOrthogonalConstraint(null);
    setRelationshipSnapGuides([]);
    trackingHoverKeyRef.current = null;
    setSelectionMarquee(null);
    setPendingImagePoint(null);
    if (!shouldCancelDraftForToolChange(previousTool, activeTool, draft)) {
      return;
    }

    clearInteractionPreview();
    setDraft(null);
    clickPlacementToolRef.current = null;
  }, [activeTool, draft]);

  const previousDraftRef = useRef<AnnotationDraft | null>(draft);
  useEffect(() => {
    if (previousDraftRef.current && !draft) {
      setAcquiredTrackingPoints([]);
      setObjectSnapTrackingResult(null);
      setOrthogonalConstraint(null);
      setRelationshipSnapGuides([]);
      trackingHoverKeyRef.current = null;
    }
    previousDraftRef.current = draft;
  }, [draft]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (isEditableKeyboardTarget(event.target)) {
        return;
      }
      if (event.key === 'Escape' && selectionMarquee) {
        event.preventDefault();
        setSelectionMarquee(null);
        setSnapResult(null);
        return;
      }
      if (event.defaultPrevented) {
        return;
      }
      if ((event.key === 'Enter' || event.key === 'Escape') && draft?.kind === 'vertex-path' && isVertexPathTool(activeTool)) {
        event.preventDefault();
        event.stopPropagation();
        commitVertexPathDraft(draft);
        return;
      }
      if (event.key === 'Escape' && draft?.kind === 'measurement-path') {
        event.preventDefault();
        setDraft(null);
        setSnapResult(null);
        return;
      }
      if (event.key === 'Enter' && draft?.kind === 'measurement-path') {
        if (commitMeasurementPathDraft(draft)) {
          event.preventDefault();
        }
        return;
      }
      if ((event.key === 'Enter' || event.key === 'Escape') && draft?.kind === 'cloud-node' && isCloudTool(activeTool)) {
        event.preventDefault();
        event.stopPropagation();
        if (draft.points.length >= 3) {
          commitCloudNodeDraft(draft);
        } else {
          setDraft(null);
          setSnapResult(null);
        }
        return;
      }
      if (event.key === 'Escape' && !draft && acquiredTrackingPoints.length > 0) {
        event.preventDefault();
        setAcquiredTrackingPoints([]);
        setObjectSnapTrackingResult(null);
        setOrthogonalConstraint(null);
        trackingHoverKeyRef.current = null;
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [acquiredTrackingPoints.length, activeTool, draft, page.index, pageScale, selectionMarquee]);

  useEffect(() => {
    const finishCloudPolygon = (event: Event) => {
      if (draft?.kind !== 'cloud-node' || !isCloudTool(activeTool)) {
        return;
      }
      event.preventDefault();
      if (draft.points.length >= 3) {
        commitCloudNodeDraft(draft);
      } else {
        setDraft(null);
        setSnapResult(null);
      }
    };
    window.addEventListener(FINISH_CLOUD_POLYGON_EVENT, finishCloudPolygon);
    return () => window.removeEventListener(FINISH_CLOUD_POLYGON_EVENT, finishCloudPolygon);
  }, [activeTool, draft, page.index]);

  function toViewportPoint(event: ReactPointerEventLike): ViewportPoint {
    const bounds = svgRef.current?.getBoundingClientRect();
    if (!bounds) {
      return { x: 0, y: 0 };
    }

    return {
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top,
    };
  }

  function toPdfPoint(event: ReactPointerEventLike): PdfPoint {
    return transform.viewportToPdf(toViewportPoint(event) as never);
  }

  function snapPdfPoint(point: PdfPoint, options: SnapPointOptions): PdfPoint {
    if (!options.enabled) {
      setSnapResult(null);
      setObjectSnapTrackingResult(null);
      setOrthogonalConstraint(null);
      return point;
    }

    const constraint = options.orthogonalAnchor
      ? constrainPointOrthogonally(options.orthogonalAnchor, point)
      : null;
    const constrainedPoint = constraint?.point ?? point;
    const excludedIds = options.excludeMarkupIds ? new Set(options.excludeMarkupIds) : undefined;
    const anchorPoints = options.anchorPoints?.length ? options.anchorPoints : [constrainedPoint];
    let result: SnapResult | null = null;
    let resultAnchor = constrainedPoint;
    for (const anchorPoint of anchorPoints) {
      const markupResult = snapToMarkup
        ? findNearestSnapPoint(anchorPoint, annotationSnapCandidates, transform, {
          tolerancePx: snapTolerancePx,
          excludeOwnerIds: excludedIds,
          snapTargets,
        })
        : null;
      const contentResult = snapToContent
        ? findNearestSnapPoint(anchorPoint, pdfContentSnapCandidates, transform, {
          tolerancePx: snapTolerancePx,
          snapTargets,
        })
        : null;
      const nearestResult = nearestSnapResult(markupResult, contentResult);
      const eligibleResult = nearestResult && (!constraint || isPointOnOrthogonalConstraint(nearestResult.point, constraint))
        ? nearestResult
        : null;
      if (eligibleResult && (!result || eligibleResult.distancePx < result.distancePx)) {
        result = eligibleResult;
        resultAnchor = anchorPoint;
      }
    }

    if (!result && dimensionIncrementMm && dimensionIncrementMm > 0) {
      const incrementAnchor = dimensionIncrementAnchor(draft);
      if (incrementAnchor) {
        result = snapToDimensionIncrement(incrementAnchor, constrainedPoint, dimensionIncrementMm * 72 / 25.4, transform.zoom);
        resultAnchor = constrainedPoint;
      }
    }

    if (options.acquireTracking) {
      updateTrackingAcquisition(result);
    }

    setSnapResult(result);
    if (result) {
      const resolvedPoint = translatePoint(constrainedPoint, pdfPoint(
        result.point.x - resultAnchor.x,
        result.point.y - resultAnchor.y,
      ));
      setObjectSnapTrackingResult(null);
      setOrthogonalConstraint(constraint ? { ...constraint, point: resolvedPoint } : null);
      return resolvedPoint;
    }

    const eligibleTrackingPoints = acquiredTrackingPoints.filter((trackingPoint) => (
      trackingPoint.source === 'annotation' ? snapToMarkup : snapToContent
    ));
    const allowedGuideAxes = constraint
      ? [constraint.axis === 'horizontal' ? 'vertical' as const : 'horizontal' as const]
      : undefined;
    let trackingResult: ObjectSnapTrackingResult | null = null;
    let trackingAnchor = constrainedPoint;
    for (const anchorPoint of anchorPoints) {
      const candidate = findObjectSnapTrackingPoint(anchorPoint, eligibleTrackingPoints, transform, {
        tolerancePx: snapTolerancePx,
        allowedGuideAxes,
      });
      if (candidate && (!trackingResult || candidate.distancePx < trackingResult.distancePx)) {
        trackingResult = candidate;
        trackingAnchor = anchorPoint;
      }
    }
    const resolvedPoint = trackingResult
      ? translatePoint(constrainedPoint, pdfPoint(
        trackingResult.point.x - trackingAnchor.x,
        trackingResult.point.y - trackingAnchor.y,
      ))
      : constrainedPoint;
    setObjectSnapTrackingResult(trackingResult);
    setOrthogonalConstraint(constraint ? { ...constraint, point: resolvedPoint } : null);
    return resolvedPoint;
  }

  function updateTrackingAcquisition(result: SnapResult | null): void {
    const candidate = result?.candidate.kind === 'point' ? result.candidate : null;
    if (!candidate) {
      trackingHoverKeyRef.current = null;
      return;
    }

    const key = trackingPointKey(candidate);
    if (trackingHoverKeyRef.current === key) {
      return;
    }

    trackingHoverKeyRef.current = key;
    setAcquiredTrackingPoints((current) => toggleAcquiredTrackingPoint(current, candidate));
  }

  function setInteractionPreview(nextPreview: readonly Markup[] | null): void {
    interactionPreviewRef.current = nextPreview;
    setInteractionPreviewState(nextPreview);
  }

  function clearInteractionPreview(): void {
    setInteractionPreview(null);
  }

  function commitInteractionPreview(preview = interactionPreviewRef.current): void {
    if (!preview || preview.length === 0) {
      clearInteractionPreview();
      return;
    }

    const replacements = new Map(preview.map((markup) => [markup.id, markup]));
    updateDocument((document) => ({
      ...document,
      markups: document.markups.map((markup) => replacements.get(markup.id) ?? markup),
    }));
    clearInteractionPreview();
  }

  function previewMoveAtPoint(
    moveDraft: Extract<AnnotationDraft, { kind: 'move' }>,
    rawPoint: PdfPoint,
  ): Extract<AnnotationDraft, { kind: 'move' }> {
    const desiredDelta = moveDelta(moveDraft, rawPoint);
    const prospectiveAnchorPoints = moveDraft.snapAnchorPoints?.map((anchorPoint) => (
      translatePoint(anchorPoint, desiredDelta)
    )) ?? [];
    const snappedPoint = snapPdfPoint(rawPoint, {
      enabled: prospectiveAnchorPoints.length > 0,
      excludeMarkupIds: moveDraft.markupIds,
      acquireTracking: true,
      anchorPoints: prospectiveAnchorPoints,
    });
    let delta = moveDelta(moveDraft, snappedPoint);
    if (delta.x === 0 && delta.y === 0) {
      return moveDraft;
    }

    const previewBase = applyMarkupPreview(markups, interactionPreviewRef.current);
    let preview = moveDraftMarkups(previewBase, moveDraft, delta, page);
    let movingBounds = combinedGuideBounds(getAnnotationGuideRects(preview, page));
    const referenceGuideRects = getAnnotationGuideRects(visibleMarkups, page, { excludeMarkupIds: moveDraft.markupIds });
    const spacingSnap = movingBounds
      ? findEqualSpacingSnap(
        movingBounds,
        referenceGuideRects,
        transform,
        snapTolerancePx,
      )
      : null;
    if (spacingSnap) {
      delta = pdfPoint(delta.x + spacingSnap.adjustment.x, delta.y + spacingSnap.adjustment.y);
      preview = moveDraftMarkups(previewBase, moveDraft, delta, page);
      movingBounds = combinedGuideBounds(getAnnotationGuideRects(preview, page));
      setSnapResult(null);
      setObjectSnapTrackingResult(null);
    }
    const sizeMatch = movingBounds
      ? findEqualSizeSnap(movingBounds, referenceGuideRects, transform, 0.5)
      : null;
    setRelationshipSnapGuides([
      ...(spacingSnap?.guides ?? []),
      ...(sizeMatch?.guides ?? []),
    ]);
    setInteractionPreview(preview);
    const resolvedPoint = pdfPoint(moveDraft.lastPoint.x + delta.x, moveDraft.lastPoint.y + delta.y);
    return {
      ...moveDraft,
      lastPoint: resolvedPoint,
      snapAnchorPoints: moveDraft.snapAnchorPoints?.map((anchorPoint) => translatePoint(anchorPoint, delta)),
    };
  }

  function transformedMarkupAtPoint(
    transformDraft: TransformDraft,
    point: PdfPoint,
    shiftKey = false,
  ): Markup | null {
    const definition = getMarkupToolDefinition(transformDraft.originalMarkup);
    return definition?.interaction?.transformMarkup?.(transformDraft.originalMarkup as never, {
      handleId: transformDraft.handleId,
      handleBehavior: transformDraft.handleBehavior,
      startPoint: transformDraft.startPoint,
      currentPoint: point,
      shiftKey,
    }, toolInteractionContext) ?? null;
  }

  function equalSizeSnapForTransform(
    transformDraft: TransformDraft,
    point: PdfPoint,
    shiftKey: boolean,
  ): { readonly point: PdfPoint; readonly markup: Markup; readonly guides: readonly RelationshipSnapGuide[] } | null {
    if (transformDraft.handleBehavior !== 'resizeSelf') {
      return null;
    }
    const rotation = 'rotation' in transformDraft.originalMarkup
      ? transformDraft.originalMarkup.rotation
      : 0;
    if (typeof rotation === 'number' && rotation !== 0) {
      return null;
    }

    const transformed = transformedMarkupAtPoint(transformDraft, point, shiftKey);
    const movingBounds = transformed
      ? getAnnotationGuideRects([transformed], page)[0]?.rect
      : null;
    if (!transformed || !movingBounds) {
      return null;
    }
    const sizeSnap = findEqualSizeSnap(
      movingBounds,
      getAnnotationGuideRects(visibleMarkups, page, { excludeMarkupIds: [transformDraft.markupId] }),
      transform,
      snapTolerancePx,
    );
    if (!sizeSnap) {
      return null;
    }

    const handle = transformDraft.handleId.split('.').at(-1) ?? '';
    const width = (handle.includes('e') || handle.includes('w')) ? sizeSnap.width : undefined;
    const height = (handle.includes('n') || handle.includes('s')) ? sizeSnap.height : undefined;
    if (width === undefined && height === undefined) {
      return null;
    }
    const resolvedPoint = pdfPoint(
      point.x + (width === undefined ? 0 : (width - movingBounds.width) * (handle.includes('w') ? -1 : 1)),
      point.y + (height === undefined ? 0 : (height - movingBounds.height) * (handle.includes('s') ? -1 : 1)),
    );
    const resolvedMarkup = transformedMarkupAtPoint(transformDraft, resolvedPoint, shiftKey);
    const resolvedBounds = resolvedMarkup
      ? getAnnotationGuideRects([resolvedMarkup], page)[0]?.rect
      : null;
    if (!resolvedMarkup || !resolvedBounds) {
      return null;
    }

    return {
      point: resolvedPoint,
      markup: resolvedMarkup,
      guides: sizeSnap.guides
        .filter((guide) => guide.axis === 'horizontal' ? width !== undefined : height !== undefined)
        .map((guide) => ({ ...guide, moving: resolvedBounds })),
    };
  }

  function equalSizeSnapForPlacementDraft(
    placementDraft: Extract<AnnotationDraft, { kind: 'line' | 'rectangle' | 'text-box' }>,
    point: PdfPoint,
    shiftKey: boolean,
  ): { readonly point: PdfPoint; readonly guides: readonly RelationshipSnapGuide[] } | null {
    if (placementDraft.kind === 'line') {
      return null;
    }
    const candidate = updateClickPlacementDraft(placementDraft, point, shiftKey);
    const movingBounds = draftRect(candidate);
    const sizeSnap = findEqualSizeSnap(
      movingBounds,
      getAnnotationGuideRects(visibleMarkups, page),
      transform,
      snapTolerancePx,
    );
    if (!sizeSnap) {
      return null;
    }

    const tool = clickPlacementToolRef.current ?? activeTool;
    let width = sizeSnap.width;
    let height = sizeSnap.height;
    if (tool === 'ellipse' && shiftKey && (width !== undefined || height !== undefined)) {
      const diameter = width !== undefined && height !== undefined
        ? Math.abs(width - movingBounds.width) <= Math.abs(height - movingBounds.height) ? width : height
        : width ?? height;
      width = diameter;
      height = diameter;
    }
    const resolvedPoint = pdfPoint(
      width === undefined ? candidate.current.x : candidate.start.x + width * (Math.sign(candidate.current.x - candidate.start.x) || 1),
      height === undefined ? candidate.current.y : candidate.start.y + height * (Math.sign(candidate.current.y - candidate.start.y) || 1),
    );
    const resolvedBounds = draftRect({ ...candidate, current: resolvedPoint });
    return {
      point: resolvedPoint,
      guides: sizeSnap.guides
        .filter((guide) => guide.axis === 'horizontal' ? width !== undefined : height !== undefined)
        .map((guide) => ({ ...guide, moving: resolvedBounds })),
    };
  }

  function handlePointerDown(event: React.PointerEvent<SVGSVGElement>): void {
    if (event.button !== 0) {
      return;
    }

    if (calibrationPickActive) {
      const point = snapPdfPoint(toPdfPoint(event), {
        enabled: snapToContent || snapToMarkup,
        orthogonalAnchor: event.shiftKey ? calibrationStartPoint : null,
      });
      setCalibrationHoverPoint(point);
      onCalibrationPoint?.(page.index, point);
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    if (activeTool === 'pan') {
      return;
    }

    if (selectionMarquee?.shape === 'box' && selectionMarquee.pointerId === null) {
      const completedMarquee = updateSelectionMarquee(selectionMarquee, toViewportPoint(event));
      setSelectedMarkupIds(selectionAfterMarquee(
        selectedMarkupIds,
        completedMarquee.active ? markupIdsSelectedByMarquee(completedMarquee) : [],
        completedMarquee.operation,
      ));
      setSelectionMarquee(null);
      setSnapResult(null);
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    const rawPoint = toPdfPoint(event);
    if (isDirectManipulationTool(activeTool) && beginHandleTransform(event, rawPoint)) {
      return;
    }

    if (postPlacementSelectionActive) {
      setSelectedMarkupIds([]);
      setPostPlacement(null);
      setHoveredMarkupId(null);
      setHotHandle(null);
      setHoverCursor(null);
      setSnapResult(null);
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    let point = snapPdfPoint(rawPoint, {
      enabled: shouldSnapCreationTool(activeTool),
      orthogonalAnchor: event.shiftKey ? orthogonalAnchorForDraft(draft, activeTool) : null,
    });
    if (draft && isClickPlacementDraft(draft)) {
      const sizeSnap = equalSizeSnapForPlacementDraft(draft, point, event.shiftKey);
      point = sizeSnap?.point ?? point;
      setRelationshipSnapGuides(sizeSnap?.guides ?? []);
      if (sizeSnap) {
        setSnapResult(null);
        setObjectSnapTrackingResult(null);
      }
    }
    if (activeTool === 'text-box') {
      beginTextBoxPlacement(point);
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    if (activeTool === 'arc') {
      handleArcPointerDown(point, event.shiftKey);
      setSelectedMarkupIds([]);
      return;
    }

    if (activeTool === 'image') {
      if (placePendingImage(point)) {
        return;
      }
    }

    if (isCloudTool(activeTool) && draft?.kind === 'cloud-node') {
      if (draft.points.length >= 3 && isCloudPolygonClosePoint(draft.points[0], point, transform)) {
        commitCloudNodeDraft(draft);
        event.stopPropagation();
        event.preventDefault();
        return;
      }
      setDraft(beginCloudNodeDraftPoint(draft, point));
      event.currentTarget.setPointerCapture(event.pointerId);
      return;
    }

    if (activeTool === 'dimension') {
      handleDimensionPointerDown(event, point);
      return;
    }

    if (isMeasurementTool(activeTool)) {
      handleMeasurementPointerDown(event, point);
      return;
    }

    if (isVertexPathTool(activeTool)) {
      handleVertexPathPointerDown(event, point, activeTool);
      return;
    }

    if (draft && isClickPlacementDraft(draft)) {
      if (commitClickPlacementDraft(point, event.shiftKey)) {
        event.stopPropagation();
        event.preventDefault();
      }
      return;
    }

    if (activeTool === 'select' && event.detail >= 2 && beginPropertiesFromHit(event, point, false)) {
      return;
    }

    if (activeTool === 'select' && beginMarkupMoveFromHit(event, point)) {
      return;
    }

    if (activeTool === 'select') {
      const operation = selectionMarqueeOperationFromModifiers(event);
      if (shouldConsumeSelectionClickAway(selectedMarkupIds, operation)) {
        setSelectedMarkupIds([]);
        setHoveredMarkupId(null);
        setHotHandle(null);
        setHoverCursor(null);
        setSnapResult(null);
        event.stopPropagation();
        event.preventDefault();
        return;
      }
      setSelectionMarquee(createSelectionMarquee(
        event.pointerId,
        toViewportPoint(event),
        operation,
      ));
      setHoveredMarkupId(null);
      setHotHandle(null);
      setHoverCursor(null);
      setSnapResult(null);
      event.currentTarget.setPointerCapture(event.pointerId);
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    const activeDefinition = getToolDefinition(activeTool);
    if (activeDefinition.category === 'markup' && activeDefinition.interaction?.createDraft) {
      setDraft(activeDefinition.interaction.createDraft({
        pointerId: event.pointerId,
        startPoint: point,
        currentPoint: point,
      }) as AnnotationDraft);
      const placement = activeDefinition.interaction.placement;
      if (placement === 'click' || placement === 'click-or-drag') {
        clickPlacementToolRef.current = activeTool;
      } else {
        clickPlacementToolRef.current = null;
      }
      if (placement !== 'click') {
        event.currentTarget.setPointerCapture(event.pointerId);
      }
      setSelectedMarkupIds([]);
      return;
    }

    setSelectedMarkupIds([]);
  }

  function handleClick(event: React.MouseEvent<SVGSVGElement>): void {
    if (!calibrationPickActive) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
  }

  function completePlacedMarkup(
    markup: Markup,
    options: {
      readonly tool?: ToolMode;
      readonly select?: boolean;
      readonly armClickAway?: boolean;
      readonly beginTextEdit?: boolean;
    } = {},
  ): void {
    const select = options.select ?? true;
    const tool = options.tool ?? activeTool;
    const placedMarkup = applyToolPropertyValues(markup, toolPropertyValues[tool]);
    updateDocument((document) => createMarkup(document, placedMarkup));
    setSelectedMarkupIds(select ? [placedMarkup.id] : []);
    setPostPlacement(select && (options.armClickAway ?? true) ? { markupId: placedMarkup.id, tool } : null);
    if (
      options.beginTextEdit
      && (placedMarkup.kind === 'text-box' || placedMarkup.kind === 'callout' || placedMarkup.kind === 'cloud-plus' || placedMarkup.kind === 'dimension')
    ) {
      setTextEdit({ markupId: placedMarkup.id, text: placedMarkup.text });
    }
    if (tool !== 'image') {
      onMarkupPlaced?.();
    }
  }

  function handleArcPointerDown(point: PdfPoint, snapAngle: boolean): void {
    if (draft?.kind === 'arc' && draft.phase === 'end') {
      if (shouldCommitLine(draft.start, point)) {
        setDraft(setArcDraftEnd(draft, point));
      }
      return;
    }

    if (draft?.kind === 'arc' && draft.phase === 'mid') {
      const bulgePoint = draft.end
        ? snapAngle
          ? snapArcBulgePoint(draft.start, draft.end, point).point
          : constrainArcBulgePoint(draft.start, draft.end, point, ARC_MIN_BULGE_PX / transform.zoom)
        : point;
      const markup = draft.end
        ? createArcMarkupFromThreePoints(createMarkupId('arc'), page.index, draft.start, draft.end, bulgePoint)
        : null;
      if (markup) {
        completePlacedMarkup(markup, { tool: 'arc' });
      }
      setDraft(null);
      setSnapResult(null);
      return;
    }

    setDraft(createArcDraft(point));
  }

  function handleDimensionPointerDown(event: React.PointerEvent<SVGSVGElement>, point: PdfPoint): void {
    const activeDefinition = getToolDefinition('dimension');

    if (draft?.kind === 'line') {
      const finalDraft = activeDefinition.interaction?.updateDraft?.(draft as never, point) as AnnotationDraft | undefined;
      const dimensionDraft = finalDraft?.kind === 'line' ? finalDraft : draft;
      const startPoint = transform.pdfToViewport(dimensionDraft.start);
      const currentPoint = transform.pdfToViewport(dimensionDraft.current);
      const markup = activeDefinition.interaction?.commitDraft?.(dimensionDraft as never, {
        ...toolInteractionContext,
        hasExceededDragThreshold: hasExceededDragThreshold(startPoint, currentPoint),
        createMarkupId,
      });
      if (markup) {
        completePlacedMarkup(markup, { tool: 'dimension', beginTextEdit: markup.kind === 'dimension' });
      }
      setDraft(null);
      setSnapResult(null);
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    if (activeDefinition.category === 'markup' && activeDefinition.interaction?.createDraft) {
      setDraft(activeDefinition.interaction.createDraft({
        pointerId: event.pointerId,
        startPoint: point,
        currentPoint: point,
      }) as AnnotationDraft);
      setSelectedMarkupIds([]);
      clickPlacementToolRef.current = activeTool;
      event.stopPropagation();
      event.preventDefault();
    }
  }

  function handleMeasurementPointerDown(event: React.PointerEvent<SVGSVGElement>, point: PdfPoint): void {
    if (!ensureMeasurementScale()) {
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    if (activeTool === 'length') {
      if (draft?.kind === 'line') {
        const finalDraft = updateLineDraft(draft, point);
        if (shouldCommitLine(finalDraft.start, finalDraft.current)) {
          const markup = createLengthMeasurementMarkup(createMarkupId('length'), page.index, finalDraft.start, finalDraft.current);
          completePlacedMarkup(markup, { tool: 'length' });
        }
        setDraft(null);
        setSnapResult(null);
      } else {
        setDraft({ kind: 'line', start: point, current: point });
        setSelectedMarkupIds([]);
        clickPlacementToolRef.current = activeTool;
      }
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    if (activeTool === 'polylength' || activeTool === 'area') {
      if (draft?.kind === 'measurement-path' && draft.tool === activeTool) {
        setDraft(addMeasurementPathDraftPoint(draft, point));
      } else {
        setDraft(createMeasurementPathDraft(activeTool, point));
        setSelectedMarkupIds([]);
      }
      clickPlacementToolRef.current = null;
      event.stopPropagation();
      event.preventDefault();
    }
  }

  function ensureMeasurementScale(): boolean {
    try {
      requirePageScale({ pageScales: pageScale ? [pageScale] : [] }, page.index);
      return true;
    } catch (caught) {
      onToolError?.(caught instanceof Error ? caught.message : 'Set page scale before placing measurement markups.');
      setDraft(null);
      setSnapResult(null);
      return false;
    }
  }

  function commitMeasurementPathDraft(measurementDraft: MeasurementPathDraft): boolean {
    if (!ensureMeasurementScale()) {
      return true;
    }

    const points = measurementPathPreviewPoints(measurementDraft);
    const isArea = measurementDraft.tool === 'area';
    const minimumPointCount = isArea ? 3 : 2;
    if (points.length < minimumPointCount) {
      return false;
    }

    const markup = isArea
      ? createAreaMeasurementMarkup(createMarkupId('area'), page.index, points)
      : createPolylengthMeasurementMarkup(createMarkupId('polylength'), page.index, points);
    completePlacedMarkup(markup, { tool: measurementDraft.tool });
    setDraft(null);
    setSnapResult(null);
    return true;
  }

  function handleVertexPathPointerDown(
    event: React.PointerEvent<SVGSVGElement>,
    point: PdfPoint,
    tool: VertexPathDraft['tool'],
  ): void {
    if (draft?.kind === 'vertex-path' && draft.tool === tool) {
      const firstPoint = draft.points[0];
      if (
        tool === 'polygon'
        && draft.points.length >= 3
        && firstPoint
        && isCloudPolygonClosePoint(firstPoint, point, transform)
      ) {
        commitVertexPathDraft(draft);
      } else {
        setDraft(addVertexPathDraftPoint(draft, point));
      }
    } else {
      setDraft(createVertexPathDraft(tool, point));
      setSelectedMarkupIds([]);
    }
    clickPlacementToolRef.current = null;
    event.stopPropagation();
    event.preventDefault();
  }

  function commitVertexPathDraft(vertexPathDraft: VertexPathDraft): boolean {
    const minimumPointCount = vertexPathDraft.tool === 'polygon' ? 3 : 2;
    if (vertexPathDraft.points.length < minimumPointCount) {
      setDraft(null);
      setSnapResult(null);
      return false;
    }

    const definition = getToolDefinition(vertexPathDraft.tool);
    const markup = definition.interaction?.commitDraft?.(vertexPathDraft as never, {
      ...toolInteractionContext,
      hasExceededDragThreshold: true,
      createMarkupId,
    });
    setDraft(null);
    setSnapResult(null);
    if (!markup) {
      return false;
    }

    completePlacedMarkup(markup, { tool: vertexPathDraft.tool });
    return true;
  }

  function commitCloudNodeDraft(cloudDraft: Extract<AnnotationDraft, { kind: 'cloud-node' }>): boolean {
    const definition = getToolDefinition(activeTool);
    const markup = definition.interaction?.commitDraft?.(cloudDraft as never, {
      ...toolInteractionContext,
      hasExceededDragThreshold: true,
      createMarkupId,
    });
    setDraft(null);
    setSnapResult(null);
    if (!markup) {
      return false;
    }

    completePlacedMarkup(markup, { beginTextEdit: markup.kind === 'cloud-plus' });
    return true;
  }

  function placePendingImage(point: PdfPoint): boolean {
    if (!pendingImageAsset) {
      return false;
    }

    const asset = consumePendingImageAsset();
    if (!asset) {
      return false;
    }

    const placement = imagePlacementRect(point, page, asset);
    const markup = createImageMarkup({
      id: createMarkupId('image'),
      pageIndex: page.index,
      rect: placement,
      dataUrl: asset.dataUrl,
      mimeType: asset.mimeType,
      aspectRatioLocked: asset.aspectRatioLocked,
      source: { source: 'butter' },
    });
    completePlacedMarkup(markup, { tool: 'image' });
    setPendingImagePoint(null);
    if (asset.selectAfterPlacement) {
      onImagePlaced?.();
    }
    return true;
  }

  function beginTextBoxPlacement(point: PdfPoint): void {
    const textBoxValues = toolPropertyValues['text-box'];
    const fontSizePt = typeof textBoxValues?.fontSizePt === 'number' ? textBoxValues.fontSizePt : 12;
    const lineHeightPt = fontSizePt * 1.15;
    const pending = applyTextBoxToolPropertyValues(createTextBoxMarkup({
      id: createMarkupId('text'),
      pageIndex: page.index,
      rect: initialTextBoxRectAtPointer(point, transform, { fontSizePt, lineHeightPt }),
      text: '',
      color: typeof textBoxValues?.textColor === 'string' ? textBoxValues.textColor : '#ff0000',
      fontFamily: typeof textBoxValues?.fontFamily === 'string' ? textBoxValues.fontFamily : 'Helvetica',
      fontSizePt,
      lineHeightPt,
      source: { source: 'butter' },
    }), textBoxValues);
    setDraft(null);
    clickPlacementToolRef.current = null;
    setSnapResult(null);
    setRelationshipSnapGuides([]);
    setSelectedMarkupIds([]);
    setPendingTextBox(pending);
  }

  function updatePendingTextBox(text: string): void {
    setPendingTextBox((pending) => {
      if (!pending) {
        return null;
      }
      return {
        ...pending,
        text,
        rect: autosizeTextBoxRectDownward(pending.rect, text, {
          fontFamily: annotationFontFamily(pending),
          fontSizePt: pending.fontSizePt,
          lineHeightPt: pending.lineHeightPt,
        }),
      };
    });
  }

  function finishPendingTextBox(text: string): void {
    const pending = pendingTextBox;
    setPendingTextBox(null);
    if (!pending || text.length === 0) {
      setSelectedMarkupIds([]);
      return;
    }

    const markup = applyTextBoxToolPropertyValues({
      ...pending,
      text,
    }, toolPropertyValues['text-box']);
    completePlacedMarkup(markup, { tool: 'text-box', select: false, armClickAway: false });
  }

  function isClickPlacementDraft(candidate: AnnotationDraft): candidate is Extract<AnnotationDraft, { kind: 'line' | 'rectangle' | 'text-box' }> {
    return Boolean(clickPlacementToolRef.current)
      && (candidate.kind === 'line' || candidate.kind === 'rectangle' || candidate.kind === 'text-box');
  }

  function updateClickPlacementDraft(
    candidate: Extract<AnnotationDraft, { kind: 'line' | 'rectangle' | 'text-box' }>,
    point: PdfPoint,
    shiftKey = false,
  ): Extract<AnnotationDraft, { kind: 'line' | 'rectangle' | 'text-box' }> {
    if (candidate.kind === 'line') {
      return updateLineDraft(candidate, point);
    }
    if (candidate.kind === 'text-box') {
      return { ...candidate, current: point };
    }
    const tool = clickPlacementToolRef.current ?? activeTool;
    return {
      ...candidate,
      current: tool === 'ellipse' && shiftKey
        ? constrainEllipseDraftPoint(candidate.start, point)
        : point,
    };
  }

  function commitClickPlacementDraft(point: PdfPoint, shiftKey = false): boolean {
    if (!draft || !isClickPlacementDraft(draft)) {
      return false;
    }

    const tool = clickPlacementToolRef.current ?? activeTool;
    const definition = getToolDefinition(tool);
    const finalDraft = updateClickPlacementDraft(draft, point, shiftKey);
    const markup = definition.interaction?.commitDraft?.(finalDraft as never, {
      ...toolInteractionContext,
      hasExceededDragThreshold: true,
      createMarkupId,
    });
    setDraft(null);
    clickPlacementToolRef.current = null;
    setSnapResult(null);
    setRelationshipSnapGuides([]);
    if (!markup) {
      return true;
    }

    if (tool === 'snapshot' && finalDraft.kind === 'rectangle') {
      const snapshotRect = rectangleDraftToRect(finalDraft);
      const snapshotMarkup = createSnapshotMarkup({
        id: createMarkupId('snapshot'),
        pageIndex: page.index,
        rect: snapshotRect,
        dataUrl: createSnapshotDataUrl?.(snapshotRect) ?? fallbackSnapshotDataUrl(),
        mimeType: 'image/png',
        source: { source: 'butter' },
      });
      completePlacedMarkup(snapshotMarkup, { tool: 'snapshot' });
      return true;
    }

    completePlacedMarkup(markup, {
      tool,
      beginTextEdit: markup.kind === 'text-box' || markup.kind === 'callout' || markup.kind === 'cloud-plus' || markup.kind === 'dimension',
    });
    return true;
  }

  function handlePointerMove(event: React.PointerEvent<SVGSVGElement>): void {
    const viewportPoint = toViewportPoint(event);
    toolCursorPointRef.current = viewportPoint;
    toolCursorInsideRef.current = true;
    if (toolCursorIconRef.current) {
      toolCursorIconRef.current.style.transform = toolCursorIconTransform(viewportPoint);
      toolCursorIconRef.current.style.opacity = '1';
    }
    if (calibrationPickActive) {
      setCalibrationHoverPoint(snapPdfPoint(toPdfPoint(event), {
        enabled: snapToContent || snapToMarkup,
        orthogonalAnchor: event.shiftKey ? calibrationStartPoint : null,
      }));
      return;
    }
    if (selectionMarquee) {
      if (selectionMarquee.pointerId !== null && selectionMarquee.pointerId !== event.pointerId) {
        return;
      }
      setSelectionMarquee(updateSelectionMarquee(selectionMarquee, viewportPoint));
      setHoveredMarkupId(null);
      setHotHandle(null);
      setHoverCursor(null);
      setSnapResult(null);
      return;
    }

    const rawPoint = toPdfPoint(event);
    if (activeTool === 'image' && pendingImageAsset && !draft) {
      setPendingImagePoint(rawPoint);
    } else if (pendingImagePoint) {
      setPendingImagePoint(null);
    }
    if (!draft) {
      const hit = isDirectManipulationTool(activeTool) ? hitTestSelectMode(rawPoint) : null;
      setHoveredMarkupId(hit?.markupId ?? null);
      setHotHandle(hit?.handleId ? { markupId: hit.markupId, handleId: hit.handleId } : null);
      setHoverCursor(hit?.cursor ?? cursorForHit(hit));
      if (activeTool === 'select' || postPlacementSelectionActive || hit) {
        setSnapResult(null);
        return;
      }
    }

    if (draft?.kind === 'transform') {
      if (draft.pointerId !== event.pointerId) {
        return;
      }

      const dragStarted = !shouldSelectMarkupAfterHandleTransform(draft, rawPoint, transform);
      if (!dragStarted) {
        setDraft(updateTransformDraft(draft, rawPoint));
        setSnapResult(null);
        return;
      }

      const point = snapPdfPoint(rawPoint, {
        enabled: draft.handleBehavior !== 'rotateSelf',
        excludeMarkupIds: [draft.markupId],
        acquireTracking: true,
      });
      const sizeSnap = equalSizeSnapForTransform(draft, point, event.shiftKey);
      const resolvedPoint = sizeSnap?.point ?? point;
      const transformed = sizeSnap?.markup ?? transformedMarkupAtPoint(draft, resolvedPoint, event.shiftKey);
      setRelationshipSnapGuides(sizeSnap?.guides ?? []);
      if (sizeSnap) {
        setSnapResult(null);
        setObjectSnapTrackingResult(null);
      }
      setInteractionPreview(transformed ? [transformed] : null);
      setDraft(updateTransformDraft(draft, resolvedPoint, true));
      return;
    }

    if (draft?.kind === 'move') {
      if (draft.pointerId !== event.pointerId) {
        return;
      }

      setDraft(previewMoveAtPoint(draft, rawPoint));
      return;
    }

    let point = snapPdfPoint(rawPoint, {
      enabled: shouldSnapCreationTool(activeTool),
      orthogonalAnchor: event.shiftKey ? orthogonalAnchorForDraft(draft, activeTool) : null,
      acquireTracking: shouldSnapCreationTool(activeTool),
    });

    if (draft && isClickPlacementDraft(draft)) {
      const sizeSnap = equalSizeSnapForPlacementDraft(draft, point, event.shiftKey);
      point = sizeSnap?.point ?? point;
      setRelationshipSnapGuides(sizeSnap?.guides ?? []);
      if (sizeSnap) {
        setSnapResult(null);
        setObjectSnapTrackingResult(null);
      }
    }

    if (!draft) {
      return;
    }

    if (isClickPlacementDraft(draft)) {
      setDraft(updateClickPlacementDraft(draft, point, event.shiftKey));
      return;
    }

    if (draft.kind === 'measurement-path') {
      setDraft(updateMeasurementPathDraft(draft, point));
      return;
    }

    if (draft.kind === 'vertex-path') {
      setDraft(updateVertexPathDraft(draft, point));
      return;
    }

    if (draft.kind === 'arc') {
      const current = draft.phase === 'mid' && draft.end
        ? event.shiftKey
          ? snapArcBulgePoint(draft.start, draft.end, point).point
          : constrainArcBulgePoint(draft.start, draft.end, point, ARC_MIN_BULGE_PX / transform.zoom)
        : point;
      setDraft(updateArcDraft(draft, current));
      return;
    }

    const activeDefinition = getToolDefinition(activeTool);
    if (activeDefinition.interaction?.updateDraft) {
      setDraft(activeDefinition.interaction.updateDraft(draft as never, point) as AnnotationDraft);
      return;
    }

  }

  function handlePointerUp(event: React.PointerEvent<SVGSVGElement>): void {
    if (selectionMarquee) {
      if (selectionMarquee.pointerId !== event.pointerId) {
        return;
      }
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }

      const completedMarquee = updateSelectionMarquee(selectionMarquee, toViewportPoint(event));
      if (completedMarquee.active) {
        setSelectedMarkupIds(selectionAfterMarquee(
          selectedMarkupIds,
          markupIdsSelectedByMarquee(completedMarquee),
          completedMarquee.operation,
        ));
        setSelectionMarquee(null);
      } else {
        if (completedMarquee.operation === 'replace') {
          setSelectedMarkupIds([]);
        }
        setSelectionMarquee(createArmedBoxSelectionMarquee(completedMarquee.start, completedMarquee.operation));
      }
      setSnapResult(null);
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    if (!draft) {
      return;
    }

    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }

    const rawPoint = toPdfPoint(event);
    if (draft.kind === 'transform') {
      if (shouldSelectMarkupAfterHandleTransform(draft, rawPoint, transform)) {
        setSelectedMarkupIds(selectionAfterMarkupClick(
          selectedMarkupIds,
          draft.markupId,
          activeTool === 'select' && event.shiftKey,
        ));
        setSnapResult(null);
        clearInteractionPreview();
      } else {
        const point = snapPdfPoint(rawPoint, { enabled: true, excludeMarkupIds: [draft.markupId] });
        const sizeSnap = equalSizeSnapForTransform(draft, point, event.shiftKey);
        const transformed = sizeSnap?.markup ?? transformedMarkupAtPoint(draft, point, event.shiftKey);
        commitInteractionPreview(transformed ? [transformed] : null);
      }
      setDraft(null);
      setRelationshipSnapGuides([]);
      clickPlacementToolRef.current = null;
      return;
    }

    if (draft.kind === 'move') {
      previewMoveAtPoint(draft, rawPoint);
      commitInteractionPreview();
      setDraft(null);
      setRelationshipSnapGuides([]);
      clickPlacementToolRef.current = null;
      setSnapResult(null);
      return;
    }

    let point = snapPdfPoint(rawPoint, {
      enabled: shouldSnapCreationTool(activeTool),
      orthogonalAnchor: event.shiftKey ? orthogonalAnchorForDraft(draft, activeTool) : null,
    });
    if (isClickPlacementDraft(draft)) {
      const sizeSnap = equalSizeSnapForPlacementDraft(draft, point, event.shiftKey);
      point = sizeSnap?.point ?? point;
      const placementTool = clickPlacementToolRef.current ?? activeTool;
      if (getToolDefinition(placementTool).interaction?.placement === 'click-or-drag') {
        const finalDraft = updateClickPlacementDraft(draft, point, event.shiftKey);
        const startPoint = transform.pdfToViewport(finalDraft.start);
        const currentPoint = transform.pdfToViewport(finalDraft.current);
        if (hasExceededDragThreshold(startPoint, currentPoint)) {
          commitClickPlacementDraft(point, event.shiftKey);
        }
      }
      return;
    }

    if (draft.kind === 'vertex-path') {
      return;
    }

    const activeDefinition = getToolDefinition(activeTool);
    if (draft.kind === 'arc') {
      return;
    }

    if (activeTool === 'dimension') {
      return;
    }

    if (isCloudTool(activeTool) && draft.kind === 'cloud-node') {
      const startPoint = transform.pdfToViewport(draft.start);
      const currentPoint = transform.pdfToViewport(draft.current);
      if (!hasExceededDragThreshold(startPoint, currentPoint)) {
        setDraft(addCloudNodeDraftPoint(draft, point));
      } else {
        setDraft(draft);
      }
      return;
    }

    if (activeDefinition.interaction?.commitDraft && 'start' in draft && 'current' in draft) {
      const startPoint = transform.pdfToViewport(draft.start);
      const currentPoint = transform.pdfToViewport(draft.current);
      const exceededDragThreshold = hasExceededDragThreshold(startPoint, currentPoint);
      if (isCloudTool(activeTool) && !exceededDragThreshold) {
        setDraft(createCloudNodeDraft(point));
        setSelectedMarkupIds([]);
        setSnapResult(null);
        return;
      }

      const markup = activeDefinition.interaction.commitDraft(draft as never, {
        ...toolInteractionContext,
        hasExceededDragThreshold: exceededDragThreshold,
        createMarkupId,
      });
      if (markup) {
        completePlacedMarkup(markup, {
          beginTextEdit: markup.kind === 'dimension' || markup.kind === 'cloud-plus',
        });
      }
    }

    setDraft(null);
    clickPlacementToolRef.current = null;
    setSnapResult(null);
    setRelationshipSnapGuides([]);
  }

  function handlePointerCancel(event: React.PointerEvent<SVGSVGElement>): void {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setDraft(null);
    setSelectionMarquee(null);
    clickPlacementToolRef.current = null;
    setSnapResult(null);
    clearInteractionPreview();
  }

  function handleDoubleClick(event: React.MouseEvent<SVGSVGElement>): void {
    if (draft?.kind === 'measurement-path') {
      const point = snapPdfPoint(toPdfPoint(event), {
        enabled: shouldSnapCreationTool(activeTool),
        orthogonalAnchor: event.shiftKey ? orthogonalAnchorForDraft(draft, activeTool) : null,
      });
      const finalDraft = addMeasurementPathDraftPoint(draft, point);
      if (commitMeasurementPathDraft(finalDraft)) {
        event.stopPropagation();
        event.preventDefault();
      }
      return;
    }

    if (isCloudTool(activeTool) && draft?.kind === 'cloud-node') {
      const point = toPdfPoint(event);
      const finalDraft = addCloudNodeDraftPoint(draft, point);
      commitCloudNodeDraft(finalDraft);
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    if (activeTool !== 'select') {
      return;
    }

    const point = toPdfPoint(event);
    if (resetRotationFromHandle(point)) {
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    if (beginPropertiesFromHit(event, point, true)) {
      event.stopPropagation();
      event.preventDefault();
    }
  }

  function beginPropertiesFromHit(
    event: React.PointerEvent<SVGElement> | React.MouseEvent<SVGSVGElement>,
    point: PdfPoint,
    toggleProperties: boolean,
  ): boolean {
    const hit = hitTestToolMarkups(visibleMarkups, point, { page, tolerance: pdfToleranceForScale(transform.zoom), transform });
    const markup = hit ? visibleMarkups.find((candidate) => candidate.id === hit.markupId) : null;
    if (!markup) {
      return false;
    }

    event.stopPropagation();
    event.preventDefault();
    setDraft(null);
    setSelectedMarkupIds([markup.id]);
    setHoveredMarkupId(markup.id);
    const existingCandidate = propertiesClickCandidateRef.current;
    const candidate = existingCandidate?.markupId === markup.id
      ? existingCandidate
      : {
        markupId: markup.id,
        wasSelected: selectedMarkupIds.length === 1 && selectedMarkupIds[0] === markup.id,
      };
    propertiesClickCandidateRef.current = candidate;
    if (toggleProperties) {
      onToggleProperties?.(candidate.wasSelected);
      propertiesClickCandidateRef.current = null;
    }
    if (!markup.locked && (markup.kind === 'text-box' || markup.kind === 'callout' || markup.kind === 'cloud-plus' || markup.kind === 'dimension')) {
      setTextEdit({ markupId: markup.id, text: markup.text });
    }
    return true;
  }

  function beginMarkupMoveFromHit(event: React.PointerEvent<SVGElement>, point: PdfPoint): boolean {
    const hit = hitTestToolMarkups(visibleMarkups, point, { page, tolerance: pdfToleranceForScale(transform.zoom), transform });
    if (!hit?.bodyDrag) {
      return false;
    }

    const markup = visibleMarkups.find((candidate) => candidate.id === hit.markupId);
    if (!markup) {
      return false;
    }

    event.stopPropagation();
    propertiesClickCandidateRef.current = {
      markupId: markup.id,
      wasSelected: selectedMarkupIds.length === 1 && selectedMarkupIds[0] === markup.id,
    };
    if (event.shiftKey) {
      setSelectedMarkupIds(selectionAfterMarkupClick(selectedMarkupIds, markup.id, true));
      setHoveredMarkupId(markup.id);
      setHotHandle(null);
      return true;
    }

    if (markup.locked) {
      setSelectedMarkupIds([markup.id]);
      setHoveredMarkupId(markup.id);
      setHotHandle(null);
      return true;
    }

    const nextSelection = selectionAfterMarkupClick(selectedMarkupIds, markup.id, false);
    const movableSelection = nextSelection.filter((markupId) => !visibleMarkups.find((candidate) => candidate.id === markupId)?.locked);
    setSelectedMarkupIds(nextSelection);
    setHoveredMarkupId(markup.id);
    setDraft(createMoveDraft(event.pointerId, movableSelection, point, {
      componentId: hit.componentId,
      bodyDrag: hit.bodyDrag,
      snapAnchorPoints: hit.bodyDrag === 'adjustOnly'
        ? undefined
        : movingSnapAnchorPoints(visibleMarkups, movableSelection, page),
    }));
    event.currentTarget.setPointerCapture(event.pointerId);
    return true;
  }

  function beginHandleTransform(event: React.PointerEvent<SVGElement>, point: PdfPoint): boolean {
    const hit = hitTestInteractiveHandles(point);
    if (!hit?.handleId || !hit.handleBehavior) {
      return false;
    }

    const markup = visibleMarkups.find((candidate) => candidate.id === hit.markupId);
    if (!markup || markup.locked) {
      return false;
    }

    event.stopPropagation();
    setHoveredMarkupId(markup.id);
    setHotHandle({ markupId: markup.id, handleId: hit.handleId });
    if (hit.handleBehavior === 'rotateSelf' && event.detail >= 2) {
      resetMarkupRotation(markup.id);
      return true;
    }

    setDraft(createTransformDraft(event.pointerId, markup, hit.handleId, hit.handleBehavior, point));
    svgRef.current?.setPointerCapture(event.pointerId);
    return true;
  }

  function markupIdsSelectedByMarquee(marquee: SelectionMarqueeState): string[] {
    return visibleMarkups.flatMap((markup) => {
      const definition = getMarkupToolDefinition(markup);
      const geometry = definition?.geometry?.getGeometry(markup as never, { page });
      return geometry && isGeometrySelectedByMarquee(geometry, marquee, transform) ? [markup.id] : [];
    });
  }

  function hitTestSelectMode(point: PdfPoint): ToolHit | null {
    const handleHit = hitTestInteractiveHandles(point);
    if (handleHit) {
      return handleHit;
    }

    return hitTestToolMarkups(visibleMarkups, point, { page, tolerance: pdfToleranceForScale(transform.zoom), transform });
  }

  function hitTestInteractiveHandles(point: PdfPoint): ToolHit | null {
    return hitTestSelectedHandles(point) ?? hitTestHoveredHandles(point);
  }

  function hitTestSelectedHandles(point: PdfPoint): ToolHit | null {
    for (let index = selectedMarkupIds.length - 1; index >= 0; index -= 1) {
      const markupId = selectedMarkupIds[index];
      const markup = visibleMarkups.find((candidate) => candidate.id === markupId);
      if (markup?.locked) {
        continue;
      }
      const definition = markup ? getMarkupToolDefinition(markup) : null;
      const interactionState = markupId === selectedMarkupIds[0] ? 'focused' : 'selected';
      const chrome = markup && definition?.selection?.getSelectionChrome(markup as never, { page, phase: interactionState });
      const handles = chrome?.handles ?? [];
      const handle = chrome ? hitTestChromeHandles(handles, point, chrome, transform, interactionState) : null;
      if (markup && handle) {
        const rotation = chrome?.bounds?.rotation ?? 0;
        return {
          markupId: markup.id,
          componentId: handle.componentId,
          region: 'handle',
          handleId: handle.id,
          handleBehavior: handle.behavior,
          cursor: cursorForHandle(handle.id, handle.behavior, rotation) ?? handle.cursor,
        };
      }
    }

    return null;
  }

  function hitTestHoveredHandles(point: PdfPoint): ToolHit | null {
    if (!hoveredMarkupId || selectedMarkupIdSet.has(hoveredMarkupId)) {
      return null;
    }

    const markup = visibleMarkups.find((candidate) => candidate.id === hoveredMarkupId);
    if (markup?.locked) {
      return null;
    }
    const definition = markup ? getMarkupToolDefinition(markup) : null;
    const chrome = markup && definition?.selection?.getSelectionChrome(markup as never, { page, phase: 'hovered' });
    const handles = chrome?.handles ?? [];
    const handle = chrome ? hitTestChromeHandles(handles, point, chrome, transform, 'hovered') : null;

    if (!markup || !handle) {
      return null;
    }

    const rotation = chrome?.bounds?.rotation ?? 0;
    return {
      markupId: markup.id,
      componentId: handle.componentId,
      region: 'handle',
      handleId: handle.id,
      handleBehavior: handle.behavior,
      cursor: cursorForHandle(handle.id, handle.behavior, rotation) ?? handle.cursor,
    };
  }

  function resetRotationFromHandle(point: PdfPoint): boolean {
    const hit = hitTestSelectedHandles(point);
    if (hit?.handleBehavior !== 'rotateSelf') {
      return false;
    }

    resetMarkupRotation(hit.markupId);
    return true;
  }

  function resetMarkupRotation(markupId: string): void {
    updateDocument((document) => ({
      ...document,
      markups: document.markups.map((candidate) => (
        candidate.id === markupId
          ? markupWithResetRotation(candidate)
          : candidate
      )),
    }));
  }

  function commitTextEdit(markupId: string, text: string): void {
    updateDocument((document) => updateMarkupTextAndCenterOnLeader(document, markupId, text));
    setTextEdit(null);
    setSelectedMarkupIds([]);
    setPostPlacement(null);
    setHotHandle(null);
  }

  const toolCursor = resolveAnnotationCursor({
    activeTool,
    calibrationPickActive,
    selectionMarqueeActive: selectionMarquee !== null,
    pendingTextBoxActive: pendingTextBox !== null,
    textEditActive: textEdit !== null,
    postPlacementSelectionActive,
    pendingImagePreviewActive: activeTool === 'image' && pendingImageAsset !== null && pendingImagePoint !== null,
    transformDragActive: draft?.kind === 'transform' && draft.dragStarted,
    transformSnapActive: snapResult !== null || objectSnapTrackingResult !== null || relationshipSnapGuides.length > 0,
    hoverCursor,
  });
  const snapEnabled = snapToContent || snapToMarkup;
  const hideChromeDuringManipulation = snapEnabled && (
    (draft?.kind === 'transform' && draft.dragStarted)
    || (draft?.kind === 'move' && interactionPreview !== null)
  );
  const activeDefinition = getToolDefinition(activeTool);
  const draftPrimitives = applyToolValuesToDraftPrimitives(draft && activeDefinition.render?.getDraftPrimitives
    ? activeDefinition.render.getDraftPrimitives(draft as never, { ...toolInteractionContext, pageScale, phase: 'draft' })
    : [], toolPropertyValues[activeTool]);
  const livePendingTextBox = pendingTextBox
    ? applyTextBoxToolPropertyValues(pendingTextBox, toolPropertyValues['text-box'])
    : null;
  const editingMarkup = textEdit
      ? visibleMarkups.find((markup): markup is EditableTextMarkup => (
      markup.id === textEdit.markupId
      && (markup.kind === 'text-box' || markup.kind === 'callout' || markup.kind === 'cloud-plus' || markup.kind === 'dimension')
    ))
    : null;
  const marqueeHoveredMarkupIdSet = new Set(
    selectionMarquee?.active ? markupIdsSelectedByMarquee(selectionMarquee) : [],
  );
  const snapGuideTypeSet = new Set(snapGuideTypes);
  const alignmentGuidesVisible = snapGuidesEnabled && snapGuideTypeSet.has('alignment');
  const visibleRelationshipSnapGuides = snapGuidesEnabled
    ? relationshipSnapGuides.filter((guide) => snapGuideTypeSet.has(guide.kind))
    : [];
  const snapSourceMarkupIdSet = new Set([
    ...(alignmentGuidesVisible && snapResult?.candidate.source === 'annotation' && snapResult.candidate.ownerId
      ? [snapResult.candidate.ownerId]
      : []),
    ...(alignmentGuidesVisible ? objectSnapTrackingResult?.guides.flatMap((guide) => (
      guide.source === 'annotation' && guide.ownerId ? [guide.ownerId] : []
    )) ?? [] : []),
    ...visibleRelationshipSnapGuides.flatMap((guide) => (
      guide.kind === 'equal-size'
        ? [guide.reference.ownerId]
        : [guide.before.ownerId, guide.after.ownerId]
    )),
  ]);
  const pendingImagePreviewBox = pendingImageAsset && pendingImagePoint
    ? transform.pdfRectToViewport(imagePlacementRect(pendingImagePoint, page, pendingImageAsset))
    : null;

  return (
    <>
      <svg
      ref={svgRef}
      className="absolute inset-0 z-20 h-full w-full overflow-visible"
      data-testid={`annotation-layer-${page.index + 1}`}
      data-annotation-canvas="true"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerCancel}
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
      onPointerLeave={() => {
        toolCursorInsideRef.current = false;
        if (toolCursorIconRef.current) {
          toolCursorIconRef.current.style.opacity = '0';
        }
        setObjectSnapTrackingResult(null);
        setOrthogonalConstraint(null);
        trackingHoverKeyRef.current = null;
        setCalibrationHoverPoint(null);
        if (!draft) {
          setPendingImagePoint(null);
          setHoveredMarkupId(null);
          setHotHandle(null);
          setHoverCursor(null);
          setSnapResult(null);
        }
      }}
      style={{ cursor: toolCursor }}
    >
      <rect width="100%" height="100%" fill="transparent" pointerEvents="all" />
      {visibleMarkups.map((markup) => {
        const definition = getMarkupToolDefinition(markup);
        if (definition?.render) {
          const interactionState = getInteractionState(
            markup.id,
            selectedMarkupIdSet,
            selectedMarkupIds[0],
            hoveredMarkupId,
            marqueeHoveredMarkupIdSet,
            snapSourceMarkupIdSet,
          );
          return (
            <PrimitiveAnnotation
              key={markup.id}
              markup={markup}
              definition={definition}
              transform={transform}
              page={page}
              pageScale={pageScale}
              interactionContext={toolInteractionContext}
              interactionState={interactionState}
              hotHandleId={hotHandle?.markupId === markup.id ? hotHandle.handleId : null}
              activeHandleId={draft?.kind === 'transform' && draft.markupId === markup.id && draft.dragStarted
                ? draft.handleId
                : null}
              hideHandles={markup.locked
                || (draft?.kind === 'transform' && draft.dragStarted && draft.markupId !== markup.id)
                || draft?.kind === 'move'}
              hideChrome={hideChromeDuringManipulation}
          editingText={!markup.locked && textEdit?.markupId === markup.id ? textEdit.text : null}
            />
          );
        }

        if (markup.kind === 'callout') {
          return <ReadOnlyCalloutAnnotation key={markup.id} markup={markup} transform={transform} />;
        }

        return null;
      })}

      {pendingImagePreviewBox && pendingImageAsset ? (
        <image
          href={pendingImageAsset.dataUrl}
          x={pendingImagePreviewBox.x}
          y={pendingImagePreviewBox.y}
          width={pendingImagePreviewBox.width}
          height={pendingImagePreviewBox.height}
          preserveAspectRatio="none"
          opacity={0.45}
          pointerEvents="none"
          aria-hidden="true"
          data-testid="pending-image-preview"
        />
      ) : null}

      {livePendingTextBox ? (
        <PrimitiveAnnotation
          markup={livePendingTextBox}
          definition={getMarkupToolDefinition(livePendingTextBox)!}
          transform={transform}
          page={page}
          pageScale={pageScale}
          interactionContext={toolInteractionContext}
          interactionState="focused"
          hotHandleId={null}
          activeHandleId={null}
          hideHandles={false}
          hideChrome={false}
          editingText={livePendingTextBox.text}
        />
      ) : null}

      {draftPrimitives.map((primitive, index) => (
        <RenderPrimitiveElement key={`draft-${index}`} primitive={primitive} transform={transform} />
      ))}
      {calibrationStartPoint && calibrationHoverPoint ? (
        <CalibrationLine start={calibrationStartPoint} end={calibrationHoverPoint} transform={transform} />
      ) : null}
      {alignmentGuidesVisible && (objectSnapTrackingResult || orthogonalConstraint) ? (
        <DraftingGuides
          trackingResult={objectSnapTrackingResult}
          orthogonalConstraint={orthogonalConstraint}
          transform={transform}
        />
      ) : null}
      {selectionMarquee?.active ? <SelectionMarquee marquee={selectionMarquee} /> : null}
      {alignmentGuidesVisible && snapResult ? <SnapIndicator result={snapResult} transform={transform} /> : null}
      {visibleRelationshipSnapGuides.length > 0 ? (
        <RelationshipGuides guides={visibleRelationshipSnapGuides} transform={transform} />
      ) : null}
      {draft?.kind === 'vertex-path' && draft.tool === 'polygon' && draft.points[0] ? (
        <PolygonStartMarker draft={draft} transform={transform} />
      ) : null}
      {livePendingTextBox ? (
        <TextBoxEditor
          markup={livePendingTextBox}
          text={livePendingTextBox.text}
          transform={transform}
          selectOnFocus={false}
          onChange={updatePendingTextBox}
          onCommit={finishPendingTextBox}
          onOutsidePointerDown={(text) => finishPendingTextBox(text)}
          onCancel={() => finishPendingTextBox(livePendingTextBox.text)}
        />
      ) : null}
      {editingMarkup && textEdit ? (
        <TextBoxEditor
          markup={editingMarkup}
          text={textEdit.text}
          transform={transform}
          onChange={(text) => setTextEdit({ markupId: editingMarkup.id, text })}
          onCommit={(text) => commitTextEdit(editingMarkup.id, text)}
          onOutsidePointerDown={(text) => commitTextEdit(editingMarkup.id, text)}
          onCancel={() => {
            setTextEdit(null);
            setSelectedMarkupIds([]);
            setPostPlacement(null);
            setHotHandle(null);
          }}
        />
      ) : null}
      </svg>
      {activeDefinition.category !== 'navigation'
        && toolCursor === 'crosshair'
        && !(draft?.kind === 'transform' && draft.dragStarted) ? (
        <div
          ref={toolCursorIconRef}
          className="pointer-events-none absolute left-0 top-0 z-30 size-4 text-white mix-blend-difference will-change-transform"
          style={{ opacity: 0, transform: 'translate3d(0, 0, 0)' }}
          aria-hidden="true"
          data-testid="tool-cursor-icon"
        >
          <ToolRailIcon tool={activeTool} />
        </div>
      ) : null}
    </>
  );
}

function dimensionIncrementAnchor(draft: AnnotationDraft | null): PdfPoint | null {
  if (!draft || draft.kind === 'ink' || draft.kind === 'move') return null;
  if (draft.kind === 'transform') return draft.startPoint;
  if (draft.kind === 'measurement-path' || draft.kind === 'vertex-path' || draft.kind === 'cloud-node') {
    return draft.points.at(-1) ?? draft.start;
  }
  return draft.start;
}

function snapToDimensionIncrement(anchor: PdfPoint, point: PdfPoint, increment: number, zoom: number): SnapResult | null {
  const dx = point.x - anchor.x;
  const dy = point.y - anchor.y;
  const distance = Math.hypot(dx, dy);
  if (distance <= Number.EPSILON) return null;
  const snappedDistance = Math.max(increment, Math.round(distance / increment) * increment);
  const scale = snappedDistance / distance;
  const snappedPoint = pdfPoint(anchor.x + dx * scale, anchor.y + dy * scale);
  return {
    point: snappedPoint,
    candidate: { kind: 'point', point: snappedPoint, source: 'dimension-increment', role: 'increment' },
    distancePx: Math.abs(snappedDistance - distance) * zoom,
  };
}

export function applyTextBoxToolPropertyValues(markup: TextBoxMarkup, values: ToolPropertyValues | undefined): TextBoxMarkup {
  const styledMarkup = applyToolPropertyValues(markup, values) as TextBoxMarkup;
  const textStyle = getAnnotationTextContentStyle(styledMarkup);
  const fontSizePt = textStyle.fontSizePt ?? styledMarkup.fontSizePt ?? 12;
  const lineHeightPt = textStyle.lineHeightPt ?? styledMarkup.lineHeightPt ?? fontSizePt * 1.15;
  return {
    ...styledMarkup,
    rect: autosizeTextBoxRectDownward(styledMarkup.rect, styledMarkup.text, {
      fontFamily: annotationFontFamily(styledMarkup),
      fontSizePt,
      lineHeightPt,
    }),
  };
}

export function markupWithResetRotation(markup: Markup): Markup {
  switch (markup.kind) {
    case 'rectangle':
    case 'ellipse':
    case 'text-box':
    case 'image':
    case 'snapshot':
      return { ...markup, rotation: 0 };
    default:
      return markup;
  }
}

function applyToolValuesToDraftPrimitives(
  primitives: readonly RenderPrimitive[],
  values: ToolPropertyValuesByTool[ToolMode] | undefined,
): readonly RenderPrimitive[] {
  if (!values) return primitives;
  return primitives.map((primitive): RenderPrimitive => {
    if (primitive.kind === 'image') {
      return {
        ...primitive,
        ...(typeof values.opacity === 'number' ? { opacity: values.opacity } : {}),
      };
    }
    return {
      ...primitive,
      style: {
        ...primitive.style,
        ...(typeof values.strokeColor === 'string' ? { stroke: values.strokeColor } : {}),
        ...(typeof values.fillColor === 'string' || values.fillColor === null
          ? { fill: values.fillColor ?? 'none' }
          : {}),
        ...(typeof values.strokeWidthPt === 'number' ? { strokeWidth: values.strokeWidthPt } : {}),
        ...(typeof values.opacity === 'number' ? { opacity: values.opacity } : {}),
        ...(typeof values.textColor === 'string' ? { textColor: values.textColor } : {}),
        ...(typeof values.fontFamily === 'string' ? { fontFamily: annotationFontCssFamily(values.fontFamily) } : {}),
        ...(typeof values.fontSizePt === 'number' ? { fontSizePt: values.fontSizePt } : {}),
      },
    };
  });
}

export function isCloudPolygonClosePoint(
  firstPoint: PdfPoint,
  currentPoint: PdfPoint,
  transform: Pick<PageTransform, 'pdfToViewport'>,
  thresholdPx = 10,
): boolean {
  const first = transform.pdfToViewport(firstPoint);
  const current = transform.pdfToViewport(currentPoint);
  return Math.hypot(current.x - first.x, current.y - first.y) <= thresholdPx;
}

function PolygonStartMarker({ draft, transform }: { draft: VertexPathDraft; transform: PageTransform }) {
  const firstPoint = transform.pdfToViewport(draft.points[0] ?? draft.start);
  const closeActive = draft.points.length >= 3
    && isCloudPolygonClosePoint(draft.points[0] ?? draft.start, draft.current, transform);
  return (
    <circle
      cx={firstPoint.x}
      cy={firstPoint.y}
      r={5}
      fill={closeActive ? '#ff0000' : '#ffffff'}
      stroke="#ff0000"
      strokeWidth={2}
      pointerEvents="none"
      aria-hidden="true"
      data-testid="polygon-start-marker"
      data-close-active={closeActive ? 'true' : 'false'}
    />
  );
}

function SelectionMarquee({ marquee }: { marquee: SelectionMarqueeState }) {
  const bounds = selectionMarqueeBounds(marquee.start, marquee.current);
  const kind = resolvedSelectionMarqueeKind(marquee);
  const crossing = kind === 'crossing';
  const commonProps = {
    'data-testid': 'selection-marquee',
    'data-selection-kind': kind,
    'data-selection-shape': marquee.shape,
    'data-selection-operation': marquee.operation,
    fill: crossing ? 'rgba(34, 197, 94, 0.14)' : 'rgba(37, 99, 235, 0.13)',
    stroke: crossing ? '#22c55e' : '#2563eb',
    strokeWidth: 1.5,
    strokeDasharray: crossing ? '7 5' : undefined,
    pointerEvents: 'none' as const,
    'aria-hidden': true,
  };
  const lassoPath = marquee.points.map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`).join(' ');
  const selectionShape = marquee.shape === 'lasso'
    ? <path {...commonProps} d={`${lassoPath} Z`} />
    : (
      <rect
        {...commonProps}
        x={bounds.left}
        y={bounds.top}
        width={bounds.right - bounds.left}
        height={bounds.bottom - bounds.top}
      />
    );
  return selectionShape;
}

function ReadOnlyMarkup({
  markup,
  transform,
  page,
  pageScale,
}: {
  markup: Markup;
  transform: PageTransform;
  page: PageModel;
  pageScale?: PageScale;
}) {
  if (!shouldRenderMarkupAtZoom(markup, transform)) {
    return null;
  }

  const definition = getMarkupToolDefinition(markup);
  if (definition?.render) {
    const primitives = definition.render.getContentPrimitives(markup as never, { page, pageScale, phase: 'idle' });
    return (
      <g data-testid={`thumbnail-markup-${markup.id}`}>
        {primitives.map((primitive, index) => (
          <RenderPrimitiveElement key={index} primitive={primitive} transform={transform} />
        ))}
      </g>
    );
  }

  if (markup.kind === 'callout') {
    return <ReadOnlyCalloutAnnotation markup={markup} transform={transform} />;
  }

  return null;
}

function PrimitiveAnnotation({
  markup,
  definition,
  transform,
  page,
  pageScale,
  interactionContext,
  interactionState,
  hotHandleId,
  activeHandleId,
  hideHandles,
  hideChrome,
  editingText,
}: {
  markup: Markup;
  definition: NonNullable<ReturnType<typeof getMarkupToolDefinition>>;
  transform: PageTransform;
  page: PageModel;
  pageScale?: PageScale;
  interactionContext?: ToolInteractionContext;
  interactionState: InteractionState;
  hotHandleId: string | null;
  activeHandleId: string | null;
  hideHandles: boolean;
  hideChrome: boolean;
  editingText: string | null;
}) {
  if (!shouldRenderMarkupAtZoom(markup, transform)) {
    return null;
  }

  const isInteractive = interactionState !== 'idle';
  const renderedMarkup = editingText !== null && markup.kind === 'cloud-plus'
    ? updateCloudPlusTextBox(
      { ...markup, text: editingText },
      centeredCompositeTextBoxRect(markup, editingText),
      interactionContext,
    )
    : editingText !== null && markup.kind === 'callout'
      ? { ...markup, text: editingText, textBox: centeredCompositeTextBoxRect(markup, editingText) }
      : markup;
  const primitives = (definition.render?.getContentPrimitives(renderedMarkup as never, { page, pageScale, phase: interactionState }) ?? [])
    .map((primitive) => (
      editingText !== null && (markup.kind === 'text-box' || markup.kind === 'callout' || markup.kind === 'cloud-plus' || markup.kind === 'dimension') && primitive.kind === 'textBox'
        ? {
          ...primitive,
          text: editingText,
          textLines: markup.kind === 'text-box' ? splitAnnotationTextLines(editingText) : undefined,
          richTextRuns: undefined,
        }
        : primitive
    ));
  const chrome = isInteractive && definition.selection
    ? definition.selection.getSelectionChrome(markup as never, { page, phase: interactionState as never })
    : null;

  return (
    <g data-testid={`markup-${markup.id}`}>
      <g>
        {primitives.map((primitive, index) => (
          <RenderPrimitiveElement key={index} primitive={primitive} transform={transform} />
        ))}
      </g>
      {chrome && !hideChrome ? (
        <SelectionChrome
          chrome={chrome}
          transform={transform}
          state={interactionState}
          hotHandleId={hotHandleId}
          activeHandleId={activeHandleId}
          hideHandles={hideHandles}
        />
      ) : null}
    </g>
  );
}

function RenderPrimitiveElement({ primitive, transform }: { primitive: RenderPrimitive; transform: PageTransform }) {
  const hideBelowZoom = primitive.kind === 'image' ? undefined : primitive.style.hideBelowZoom;
  if (hideBelowZoom !== undefined && transform.zoom < hideBelowZoom) {
    return null;
  }

  if (primitive.kind === 'rect') {
    const box = transform.pdfRectToViewport(primitive.rect);
    const center = {
      x: box.x + box.width * 0.5,
      y: box.y + box.height * 0.5,
    };
    return (
      <rect
        x={box.x}
        y={box.y}
        width={box.width}
        height={box.height}
        transform={primitive.rotation ? `rotate(${primitive.rotation} ${center.x} ${center.y})` : undefined}
        fill={primitive.style.fill ?? 'none'}
        stroke={primitive.style.stroke ?? 'none'}
        strokeWidth={scaleAnnotationStrokeWidth(primitive.style.strokeWidth, transform)}
        strokeDasharray={scaleAnnotationDashArray(primitive.style.dashArray, transform)}
        opacity={primitive.style.opacity ?? 1}
        pointerEvents={primitive.pointerEvents ?? 'none'}
      />
    );
  }

  if (primitive.kind === 'ellipse') {
    const box = transform.pdfRectToViewport(primitive.rect);
    const center = {
      x: box.x + box.width * 0.5,
      y: box.y + box.height * 0.5,
    };
    return (
      <ellipse
        cx={center.x}
        cy={center.y}
        rx={Math.abs(box.width) * 0.5}
        ry={Math.abs(box.height) * 0.5}
        transform={primitive.rotation ? `rotate(${primitive.rotation} ${center.x} ${center.y})` : undefined}
        fill={primitive.style.fill ?? 'none'}
        stroke={primitive.style.stroke ?? 'none'}
        strokeWidth={scaleAnnotationStrokeWidth(primitive.style.strokeWidth, transform)}
        strokeDasharray={scaleAnnotationDashArray(primitive.style.dashArray, transform)}
        opacity={primitive.style.opacity ?? 1}
        pointerEvents={primitive.pointerEvents ?? 'none'}
      />
    );
  }

  if (primitive.kind === 'polyline') {
    const points = primitive.points
      .map((point) => {
        const viewPoint = transform.pdfToViewport(point);
        return `${viewPoint.x},${viewPoint.y}`;
      })
      .join(' ');
    return (
      <polyline
        points={points}
        fill={primitive.style.fill ?? 'none'}
        stroke={primitive.style.stroke ?? 'none'}
        strokeWidth={scaleAnnotationStrokeWidth(primitive.style.strokeWidth, transform)}
        strokeLinecap={primitive.style.lineCap}
        strokeLinejoin={primitive.style.lineJoin}
        strokeDasharray={scaleAnnotationDashArray(primitive.style.dashArray, transform)}
        opacity={primitive.style.opacity ?? 1}
        pointerEvents={primitive.pointerEvents ?? 'none'}
        style={primitive.style.blendMode ? { mixBlendMode: primitive.style.blendMode } : undefined}
      />
    );
  }

  if (primitive.kind === 'polygon') {
    const points = primitive.points
      .map((point) => {
        const viewPoint = transform.pdfToViewport(point);
        return `${viewPoint.x},${viewPoint.y}`;
      })
      .join(' ');
    return (
      <polygon
        points={points}
        fill={primitive.style.fill ?? 'none'}
        stroke={primitive.style.stroke ?? 'none'}
        strokeWidth={scaleAnnotationStrokeWidth(primitive.style.strokeWidth, transform)}
        strokeDasharray={scaleAnnotationDashArray(primitive.style.dashArray, transform)}
        opacity={primitive.style.opacity ?? 1}
        pointerEvents={primitive.pointerEvents ?? 'none'}
      />
    );
  }

  if (primitive.kind === 'path') {
    return (
      <path
        d={transformPdfPathData(primitive.d, transform)}
        fill={primitive.style.fill ?? 'none'}
        stroke={primitive.style.stroke ?? 'none'}
        strokeWidth={scaleAnnotationStrokeWidth(primitive.style.strokeWidth, transform)}
        strokeLinecap={primitive.style.lineCap}
        strokeLinejoin={primitive.style.lineJoin}
        strokeDasharray={scaleAnnotationDashArray(primitive.style.dashArray, transform)}
        opacity={primitive.style.opacity ?? 1}
        pointerEvents={primitive.pointerEvents ?? 'none'}
      />
    );
  }

  if (primitive.kind === 'textBox') {
    const box = transform.pdfRectToViewport(primitive.rect);
    const fontSize = scaleAnnotationFontSize(primitive.style.fontSizePt, transform);
    const lineHeight = scaleAnnotationLineHeight(primitive.style.lineHeightPt, primitive.style.fontSizePt, transform);
    const inset = scaleAnnotationTextInset(primitive.style.textInsetPt, transform);
    const firstBaselineOffset = scaleAnnotationFirstBaselineOffset(
      primitive.style.firstBaselineOffsetPt,
      primitive.style.fontSizePt,
      transform,
    );
    const richLines = primitive.richTextRuns ? splitRichTextRunsIntoLines(primitive.richTextRuns) : null;
    const lines = richLines ?? (primitive.textLines
      ? primitive.textLines.map((text) => ({ text }))
      : layoutTextBoxLines(primitive.text, {
        boxWidthPt: primitive.rect.width,
        fontFamily: primitive.style.fontFamily,
        fontSizePt: primitive.style.fontSizePt,
        insetPt: primitive.style.textInsetPt,
      }));
    const lineX = (line: (typeof lines)[number]) => {
      const widthPt = 'runs' in line
        ? line.runs.reduce((width, run) => width + measureAnnotationText(run.text, {
          fontFamily: primitive.style.fontFamily,
          fontSizePt: run.fontSizePt ?? primitive.style.fontSizePt,
          bold: run.bold,
          italic: run.italic,
        }), 0)
        : measureAnnotationText(line.text, {
          fontFamily: primitive.style.fontFamily,
          fontSizePt: primitive.style.fontSizePt,
        });
      const width = widthPt * transform.zoom;
      if (primitive.style.textAlign === 'center') {
        return box.x + box.width * 0.5 - width * 0.5;
      }
      if (primitive.style.textAlign === 'right') {
        return box.x + box.width - inset - width;
      }
      return box.x + inset;
    };
    const center = {
      x: box.x + box.width * 0.5,
      y: box.y + box.height * 0.5,
    };
    return (
      <text
        x={lineX(lines[0])}
        y={box.y + firstBaselineOffset}
        transform={primitive.rotation ? `rotate(${primitive.rotation} ${center.x} ${center.y})` : undefined}
        fill={primitive.style.textColor ?? primitive.style.stroke ?? '#0f172a'}
        fontFamily={primitive.style.fontFamily}
        fontSize={fontSize}
        opacity={primitive.style.opacity ?? 1}
        pointerEvents={primitive.pointerEvents ?? 'none'}
        className="select-none"
        xmlSpace="preserve"
      >
        {lines.map((line, index) => (
          <tspan key={index} x={lineX(line)} dy={index === 0 ? 0 : lineHeight}>
            {'runs' in line ? line.runs.map((run, runIndex) => (
              <tspan
                key={runIndex}
                fill={run.color}
                fontSize={run.fontSizePt ? scaleAnnotationFontSize(run.fontSizePt, transform) : undefined}
                fontWeight={run.bold ? 'bold' : undefined}
                fontStyle={run.italic ? 'italic' : undefined}
              >
                {run.text}
              </tspan>
            )) : line.text}
          </tspan>
        ))}
      </text>
    );
  }

  if (primitive.kind === 'image') {
    const box = transform.pdfRectToViewport(primitive.rect);
    const center = {
      x: box.x + box.width * 0.5,
      y: box.y + box.height * 0.5,
    };
    return (
      <image
        href={primitive.assetId}
        x={box.x}
        y={box.y}
        width={box.width}
        height={box.height}
        transform={primitive.rotation ? `rotate(${primitive.rotation} ${center.x} ${center.y})` : undefined}
        preserveAspectRatio="none"
        opacity={primitive.opacity ?? 1}
        pointerEvents={primitive.pointerEvents ?? 'none'}
      />
    );
  }

  return null;
}

interface RichTextLine {
  readonly runs: readonly TextBoxRichTextRun[];
}

function splitRichTextRunsIntoLines(runs: readonly TextBoxRichTextRun[]): readonly RichTextLine[] {
  const lines: TextBoxRichTextRun[][] = [[]];
  for (const run of runs) {
    const parts = run.text.split(/\r\n|\r|\n/);
    parts.forEach((part, index) => {
      if (part.length > 0) {
        lines[lines.length - 1].push({ ...run, text: part });
      }
      if (index < parts.length - 1) {
        lines.push([]);
      }
    });
  }
  return lines.map((line) => ({ runs: line }));
}

function TextBoxEditor({
  markup,
  text,
  transform,
  onChange,
  onCommit,
  onCancel,
  onOutsidePointerDown,
  selectOnFocus = true,
}: {
  markup: EditableTextMarkup;
  text: string;
  transform: PageTransform;
  onChange: (text: string) => void;
  onCommit: (text: string) => void;
  onCancel: () => void;
  onOutsidePointerDown?: (text: string, event: PointerEvent) => void;
  selectOnFocus?: boolean;
}) {
  const editorRef = useRef<HTMLTextAreaElement | null>(null);
  const [selection, setSelection] = useState(() => ({ start: selectOnFocus ? 0 : text.length, end: text.length }));
  const editRect = editableTextRect(markup, text);
  const box = transform.pdfRectToViewport(editRect);
  const editorTextStyle = getAnnotationTextContentStyle(markup, markup.kind === 'dimension' ? 13 / 12 : 14.3146 / 12);
  const fontSizePt = editorTextStyle.fontSizePt ?? 12;
  const lineHeightPt = editorTextStyle.lineHeightPt ?? 13.8;
  const verticalInsetPt = textBoxEditorVerticalInsetPt(fontSizePt, lineHeightPt);
  const fontSize = fontSizePt * transform.zoom;
  const lineHeight = lineHeightPt * transform.zoom;
  const textInset = (editorTextStyle.textInsetPt ?? 0) * transform.zoom;
  const center = {
    x: box.x + box.width * 0.5,
    y: box.y + box.height * 0.5,
  };

  useEffect(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }

    editor.focus();
    if (selectOnFocus) {
      editor.select();
    } else {
      editor.setSelectionRange(editor.value.length, editor.value.length);
    }
    setSelection({ start: editor.selectionStart, end: editor.selectionEnd });
  }, [markup.id, selectOnFocus]);

  function syncSelection(editor: HTMLTextAreaElement): void {
    setSelection({ start: editor.selectionStart, end: editor.selectionEnd });
  }

  const caretGeometry = selection.start === selection.end
    ? editableTextCaretGeometry(markup, text, selection.end)
    : null;
  const caretX = caretGeometry ? box.x + (caretGeometry.x - editRect.x) * transform.zoom : 0;
  const caretY = caretGeometry ? box.y + (caretGeometry.y - editRect.y) * transform.zoom : 0;

  useEffect(() => {
    if (!onOutsidePointerDown) {
      return;
    }

    const finishOnOutsidePointerDown = (event: PointerEvent) => {
      const editor = editorRef.current;
      if (!editor || editor.contains(event.target as Node)) {
        return;
      }
      const target = event.target;
      if (!(target instanceof Element) || !target.closest('[data-annotation-canvas="true"]')) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      onOutsidePointerDown(editor.value, event);
    };

    document.addEventListener('pointerdown', finishOnOutsidePointerDown, true);
    return () => document.removeEventListener('pointerdown', finishOnOutsidePointerDown, true);
  }, [onOutsidePointerDown]);

  const rotation = markup.kind === 'text-box' && markup.rotation ? `rotate(${markup.rotation} ${center.x} ${center.y})` : undefined;

  return (
    <>
      <foreignObject
        x={box.x}
        y={box.y}
        width={Math.max(24, box.width)}
        height={Math.max(18, box.height)}
        transform={rotation}
        data-testid={`text-box-editor-${markup.id}`}
      >
        <textarea
          ref={editorRef}
          value={text}
          rows={Math.max(1, splitAnnotationTextLines(text).length)}
          wrap="off"
          spellCheck={false}
          aria-label="Text box content"
          onChange={(event) => {
            onChange(event.currentTarget.value);
            syncSelection(event.currentTarget);
          }}
          onSelect={(event) => syncSelection(event.currentTarget)}
          onBlur={(event) => onCommit(event.currentTarget.value)}
          onPointerDown={(event) => {
            if (event.button === 0) {
              event.stopPropagation();
            }
          }}
          onDoubleClick={(event) => event.stopPropagation()}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              event.stopPropagation();
              if (markup.kind === 'text-box') {
                onCommit(event.currentTarget.value);
              } else {
                onCancel();
              }
            }
            if (markup.kind !== 'text-box' && event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              onCommit(event.currentTarget.value);
            }
          }}
          style={{
            caretColor: 'transparent',
            color: 'transparent',
            fontFamily: annotationFontFamily(markup),
            fontSize,
            lineHeight: `${lineHeight}px`,
            paddingLeft: textInset,
            paddingTop: verticalInsetPt * transform.zoom,
            paddingBottom: verticalInsetPt * transform.zoom,
            overflow: 'hidden',
            border: 0,
            outline: 'none',
            boxShadow: 'none',
            appearance: 'none',
            transform: undefined,
          }}
          className="bp-native-scroll-hidden block h-full w-full resize-none border-0 bg-transparent pr-0 outline-none selection:bg-blue-200/70"
        />
      </foreignObject>
      {caretGeometry ? (
        <line
          x1={caretX}
          x2={caretX}
          y1={caretY}
          y2={caretY + caretGeometry.height * transform.zoom}
          transform={rotation}
          stroke={editorTextStyle.textColor ?? '#ff0000'}
          strokeWidth={Math.max(1, transform.zoom)}
          pointerEvents="none"
          className="bp-text-box-editing-caret"
          data-testid="text-box-editing-caret"
        />
      ) : null}
    </>
  );
}

export function textBoxCaretGeometry(
  markup: Pick<TextBoxMarkup, 'rect' | 'fontFamily' | 'fontSizePt' | 'lineHeightPt' | 'appearance'>,
  text: string,
  selectionOffset: number,
  options: TextBoxCaretGeometryOptions = {},
): { x: number; y: number; height: number } {
  const candidate = {
    id: 'text-caret',
    pageIndex: 0,
    kind: 'text-box',
    text,
    ...markup,
  } as TextBoxMarkup;
  return editableTextCaretGeometry(candidate, text, selectionOffset, options);
}

function editableTextCaretGeometry(
  markup: EditableTextMarkup,
  text: string,
  selectionOffset: number,
  options: TextBoxCaretGeometryOptions = {},
): { x: number; y: number; height: number } {
  const rect = editableTextRect(markup, text);
  const style = markup.kind === 'callout' || markup.kind === 'cloud-plus'
    ? getVerticallyCenteredAnnotationTextContentStyle(markup, rect, text)
    : getAnnotationTextContentStyle(markup, markup.kind === 'dimension' ? 13 / 12 : 14.3146 / 12);
  const fontFamily = options.fontFamily ?? style.fontFamily ?? annotationFontCssFamily('Helvetica');
  const fontSizePt = options.fontSizePt ?? style.fontSizePt ?? 12;
  const lineHeightPt = options.lineHeightPt ?? style.lineHeightPt ?? fontSizePt * 1.15;
  const measureText = options.measureText ?? measureAnnotationText;
  const beforeCaret = text.slice(0, Math.max(0, Math.min(selectionOffset, text.length)));
  const linesBeforeCaret = markup.kind === 'text-box'
    ? splitAnnotationTextLines(beforeCaret).map((line) => ({ text: line }))
    : layoutTextBoxLines(beforeCaret, {
      boxWidthPt: rect.width,
      fontFamily,
      fontSizePt,
      insetPt: style.textInsetPt,
      measureText,
    });
  const allLines = markup.kind === 'text-box'
    ? splitAnnotationTextLines(text).map((line) => ({ text: line }))
    : layoutTextBoxLines(text, {
      boxWidthPt: rect.width,
      fontFamily,
      fontSizePt,
      insetPt: style.textInsetPt,
      measureText,
    });
  const currentLinePrefix = linesBeforeCaret.at(-1)?.text ?? '';
  const lineIndex = Math.max(0, linesBeforeCaret.length - 1);
  const fullLine = allLines[lineIndex]?.text ?? currentLinePrefix;
  const prefixWidth = measureText(currentLinePrefix, { fontFamily, fontSizePt });
  const fullLineWidth = measureText(fullLine, { fontFamily, fontSizePt });
  const insetPt = style.textInsetPt ?? 0;
  const lineStartX = style.textAlign === 'center'
    ? rect.x + rect.width * 0.5 - fullLineWidth * 0.5
    : style.textAlign === 'right'
      ? rect.x + rect.width - insetPt - fullLineWidth
      : rect.x + insetPt;

  return {
    x: lineStartX + prefixWidth,
    y: rect.y + (style.firstBaselineOffsetPt ?? fontSizePt) - fontSizePt + lineIndex * lineHeightPt,
    height: lineHeightPt,
  };
}

export function autosizeTextBoxRect(anchor: PdfPoint, text: string, options: TextBoxAutosizeOptions = {}): Rect {
  const fontSizePt = options.fontSizePt ?? 12;
  const lineHeightPt = options.lineHeightPt ?? fontSizePt * 1.15;
  const insetPt = options.insetPt ?? 5;
  const measureText = options.measureText ?? measureAnnotationText;
  const lines = splitAnnotationTextLines(text);
  const widestLine = lines.reduce((width, line) => Math.max(width, measureText(line, options)), 0);
  const caretWidth = Math.max(1, fontSizePt / 12);
  const verticalInsetPt = textBoxEditorVerticalInsetPt(fontSizePt, lineHeightPt);
  return rect(
    anchor.x,
    anchor.y,
    Math.max(caretWidth, widestLine) + insetPt * 2,
    Math.max(1, lines.length) * lineHeightPt + verticalInsetPt * 2,
  );
}

export function autosizeTextBoxRectDownward(
  currentRect: Rect,
  text: string,
  options: TextBoxAutosizeOptions = {},
): Rect {
  const topLeft = pdfPoint(currentRect.x, currentRect.y + currentRect.height);
  const sized = autosizeTextBoxRect(topLeft, text, options);
  return rect(sized.x, topLeft.y - sized.height, sized.width, sized.height);
}

function createTextBoxGhostCursor(): string {
  const svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" width="32" height="24" viewBox="0 0 32 24">',
    '<rect x="1.5" y="2.5" width="29" height="19" rx="1" fill="#ffffff" fill-opacity="0.2" stroke="#2563eb" stroke-opacity="0.42" stroke-width="1" stroke-dasharray="3 2"/>',
    '<path d="M16 7v10" stroke="#ef4444" stroke-opacity="0.48" stroke-width="1.25"/>',
    '</svg>',
  ].join('');
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}") 16 12, default`;
}

function initialTextBoxRectAtPointer(
  point: PdfPoint,
  transform: PageTransform,
  options: TextBoxAutosizeOptions,
): Rect {
  const seedRect = autosizeTextBoxRect(point, '', options);
  const seedBox = transform.pdfRectToViewport(seedRect);
  const pointer = transform.pdfToViewport(point);
  return transform.viewportRectToPdf(rect(
    pointer.x - seedBox.width * 0.5,
    pointer.y - seedBox.height * 0.5,
    seedBox.width,
    seedBox.height,
  ));
}

function textBoxEditorVerticalInsetPt(fontSizePt: number, lineHeightPt: number): number {
  const minimumSingleLineHeightPt = fontSizePt * 1.5;
  return Math.max(0, (minimumSingleLineHeightPt - lineHeightPt) * 0.5);
}

function annotationFontFamily(markup: EditableTextMarkup | TextBoxMarkup): string {
  return getAnnotationTextContentStyle(markup).fontFamily ?? annotationFontCssFamily('Helvetica');
}

export function scaleAnnotationFontSize(fontSizePt: number | undefined, transform: Pick<PageTransform, 'zoom'>): number {
  return (fontSizePt ?? 12) * transform.zoom;
}

export function scaleAnnotationLineHeight(lineHeightPt: number | undefined, fontSizePt: number | undefined, transform: Pick<PageTransform, 'zoom'>): number {
  return (lineHeightPt ?? (fontSizePt ?? 12) * 1.15) * transform.zoom;
}

export function scaleAnnotationTextInset(textInsetPt: number | undefined, transform: Pick<PageTransform, 'zoom'>): number {
  return (textInsetPt ?? 5) * transform.zoom;
}

export function scaleAnnotationFirstBaselineOffset(firstBaselineOffsetPt: number | undefined, fontSizePt: number | undefined, transform: Pick<PageTransform, 'zoom'>): number {
  return (firstBaselineOffsetPt ?? (fontSizePt ?? 12)) * transform.zoom;
}

export function shouldRenderMarkupAtZoom(markup: Markup, transform: Pick<PageTransform, 'zoom'>): boolean {
  if (markup.source?.source !== 'imported') {
    return true;
  }

  return transform.zoom >= IMPORTED_MARKUP_MIN_RENDER_ZOOM;
}

export function scaleAnnotationStrokeWidth(strokeWidthPt: number | undefined, transform: Pick<PageTransform, 'zoom'>): number {
  const strokeWidth = strokeWidthPt ?? 1;
  if (strokeWidth <= 0) {
    return 0;
  }

  return Math.max(0.05, strokeWidth * transform.zoom);
}

export function scaleAnnotationDashArray(dashArray: string | undefined, transform: Pick<PageTransform, 'zoom'>): string | undefined {
  if (!dashArray) {
    return undefined;
  }

  return dashArray
    .trim()
    .split(/[\s,]+/)
    .map((value) => Number(value))
    .map((value) => (Number.isFinite(value) ? Number(Math.max(0.05, value * transform.zoom).toFixed(3)) : value))
    .join(' ');
}

function editableTextRect(markup: EditableTextMarkup, text = markup.text) {
  if (markup.kind === 'dimension') {
    return dimensionCaptionRect(markup);
  }

  return markup.kind === 'callout' || markup.kind === 'cloud-plus'
    ? centeredCompositeTextBoxRect(markup, text)
    : markup.rect;
}

export function centeredCompositeTextBoxRect(
  markup: Extract<Markup, { kind: 'callout' | 'cloud-plus' }>,
  text: string,
): Rect {
  const style = getAnnotationTextContentStyle(markup);
  const fontSizePt = style.fontSizePt ?? 12;
  const lineHeightPt = style.lineHeightPt ?? fontSizePt * 1.15;
  const lineCount = Math.max(1, splitAnnotationTextLines(text).length);
  const requiredHeight = lineCount * lineHeightPt + 12;
  const height = Math.max(markup.textBox.height, requiredHeight);
  const connection = markup.leader.points.at(-1);
  const existingCenterY = markup.textBox.y + markup.textBox.height * 0.5;
  const connectsToVerticalSide = connection
    ? Math.min(
      Math.abs(connection.x - markup.textBox.x),
      Math.abs(connection.x - (markup.textBox.x + markup.textBox.width)),
    ) <= Math.min(
      Math.abs(connection.y - markup.textBox.y),
      Math.abs(connection.y - (markup.textBox.y + markup.textBox.height)),
    )
    : false;
  const centerY = connection && connectsToVerticalSide ? connection.y : existingCenterY;
  return rect(markup.textBox.x, centerY - height * 0.5, markup.textBox.width, height);
}

export function updateMarkupTextAndCenterOnLeader(
  document: DocumentModel,
  markupId: string,
  text: string,
): DocumentModel {
  const markup = document.markups.find((candidate) => candidate.id === markupId);
  if (markup?.kind !== 'callout' && markup?.kind !== 'cloud-plus') {
    return updateMarkupText(document, markupId, text);
  }

  const markupPage = document.pages.find((page) => page.index === markup.pageIndex);
  const interactionContext = markupPage
    ? interactionContextForPage(markupPage, document.markups)
    : undefined;

  return {
    ...document,
    markups: document.markups.map((candidate) => candidate.id === markupId
      ? markup.kind === 'cloud-plus'
        ? updateCloudPlusTextBox(
          { ...markup, text },
          centeredCompositeTextBoxRect(markup, text),
          interactionContext,
        )
        : { ...markup, text, textBox: centeredCompositeTextBoxRect(markup, text) }
      : candidate),
  };
}

function SelectionChrome({
  chrome,
  transform,
  state,
  hotHandleId,
  activeHandleId,
  hideHandles,
}: {
  chrome: SelectionChromeDescriptor;
  transform: PageTransform;
  state: InteractionState;
  hotHandleId: string | null;
  activeHandleId: string | null;
  hideHandles: boolean;
}) {
  if (!chrome.bounds) {
    return null;
  }

  const box = transform.pdfRectToViewport(chrome.bounds.rect);
  const style = getChromeStyle(state, chrome.bounds.kind as ChromeBoundsKind);
  const chromeBox = expandViewportRect(box, style.boundsOutsetPx);
  const center = {
    x: box.x + box.width * 0.5,
    y: box.y + box.height * 0.5,
  };
  const chromeTransform = chrome.bounds.rotation
    ? `rotate(${chrome.bounds.rotation} ${center.x} ${center.y})`
    : undefined;
  const handles = (hideHandles ? [] : chrome.handles ?? []).filter((handle) => (
    (state !== 'hovered' || handle.behavior !== 'rotateSelf')
    && (activeHandleId === null || handle.id === activeHandleId)
  ));
  const controlPaths = chrome.controlPaths?.filter((path) => path.points.length > 1) ?? [];
  const lineBoundsPaths = controlPaths.filter((path) => !path.closed && path.points.length === 2);
  const shouldRenderBoundsBox = lineBoundsPaths.length === 0;

  return (
    <g className="pointer-events-none" data-interaction-state={state}>
      {lineBoundsPaths.length > 0 ? (
        <g>
          {lineBoundsPaths.map((path) => (
            <path
              key={`${path.id}.halo`}
              d={selectionLineBoundsPathData(path.points[0], path.points[1], transform, lineChromeHalfWidth(style))}
              fill="none"
              stroke={style.haloStroke}
              strokeWidth={style.haloStrokeWidth}
            />
          ))}
          {lineBoundsPaths.map((path) => (
            <path
              key={path.id}
              d={selectionLineBoundsPathData(path.points[0], path.points[1], transform, lineChromeHalfWidth(style))}
              fill="none"
              stroke={style.boundsStroke}
              strokeWidth={style.boundsStrokeWidth}
              strokeDasharray={style.strokeDasharray}
            />
          ))}
        </g>
      ) : null}
      <g transform={chromeTransform}>
        {shouldRenderBoundsBox ? (
          <>
            <rect
              x={chromeBox.x}
              y={chromeBox.y}
              width={chromeBox.width}
              height={chromeBox.height}
              fill="none"
              stroke={style.haloStroke}
              strokeWidth={style.haloStrokeWidth}
            />
            <rect
              x={chromeBox.x}
              y={chromeBox.y}
              width={chromeBox.width}
              height={chromeBox.height}
              fill="none"
              stroke={style.boundsStroke}
              strokeWidth={style.boundsStrokeWidth}
              strokeDasharray={style.strokeDasharray}
            />
          </>
        ) : null}
        {handles
          .filter((handle) => handle.behavior === 'rotateSelf')
          .map((handle) => {
            const point = projectChromeHandlePoint(
              transform.pdfToViewport(handle.point),
              box,
              chromeBox,
              ROTATION_HANDLE_OFFSET_PX,
            );
            const handleStyle = getChromeHandleStyle(style, state, handle.id === hotHandleId);
            const radius = Math.max(4, handleStyle.size * 0.55);
            const connectorStart = {
              x: Math.max(chromeBox.x, Math.min(chromeBox.x + chromeBox.width, point.x)),
              y: point.y < chromeBox.y ? chromeBox.y : chromeBox.y + chromeBox.height,
            };
            const connectorEnd = {
              x: point.x,
              y: point.y < chromeBox.y ? point.y + radius : point.y - radius,
            };
            return (
              <line
                key={`${handle.id}.connector`}
                x1={connectorStart.x}
                y1={connectorStart.y}
                x2={connectorEnd.x}
                y2={connectorEnd.y}
                stroke={style.boundsStroke}
                strokeWidth={style.boundsStrokeWidth}
                strokeDasharray={style.strokeDasharray}
              />
            );
          })}
        {handles.map((handle) => {
          const handleState = getChromeHandleStyle(style, state, handle.id === hotHandleId);
          const handleOffset = handleState.size * 0.5;
          const point = shouldProjectHandleToChromeBounds(handle, chrome)
            ? projectChromeHandlePoint(
              transform.pdfToViewport(handle.point),
              box,
              chromeBox,
              handle.behavior === 'rotateSelf' ? ROTATION_HANDLE_OFFSET_PX : undefined,
            )
            : transform.pdfToViewport(handle.point);
          if (handle.behavior === 'rotateSelf') {
            const radius = Math.max(4, handleState.size * 0.55);
            return (
              <circle
                key={handle.id}
                data-handle-id={handle.id}
                data-handle-state={handle.id === hotHandleId ? 'hot' : state}
                cx={point.x}
                cy={point.y}
                r={radius}
                fill={handleState.fill}
                stroke={handleState.stroke}
                strokeWidth={handleState.strokeWidth}
              />
            );
          }

          return (
            <rect
              key={handle.id}
              data-handle-id={handle.id}
              data-handle-state={handle.id === hotHandleId ? 'hot' : state}
              x={point.x - handleOffset}
              y={point.y - handleOffset}
              width={handleState.size}
              height={handleState.size}
              fill={handleState.fill}
              stroke={handleState.stroke}
              strokeWidth={handleState.strokeWidth}
            />
          );
        })}
      </g>
    </g>
  );
}

function CalibrationLine({
  start,
  end,
  transform,
}: {
  start: PdfPoint;
  end: PdfPoint;
  transform: PageTransform;
}) {
  const viewportStart = transform.pdfToViewport(start);
  const viewportEnd = transform.pdfToViewport(end);
  return (
    <line
      className="pointer-events-none"
      data-testid="page-scale-calibration-line"
      x1={viewportStart.x}
      y1={viewportStart.y}
      x2={viewportEnd.x}
      y2={viewportEnd.y}
      stroke="var(--primary)"
      strokeWidth={2}
      vectorEffect="non-scaling-stroke"
      aria-hidden="true"
    />
  );
}

function SnapIndicator({
  result,
  transform,
}: {
  result: SnapResult;
  transform: PageTransform;
}) {
  const point = transform.pdfToViewport(result.point);
  const radius = SNAP_MARKER_RADIUS_PX;
  const role = result.candidate.role;

  return (
    <g className="pointer-events-none" data-testid="snap-indicator">
      {role === 'midpoint' ? (
        <path
          d={`M ${point.x} ${point.y - radius - 1} L ${point.x + radius + 1} ${point.y + radius} L ${point.x - radius - 1} ${point.y + radius} Z`}
          fill="none"
          stroke={SNAP_MARKER_COLOR}
          strokeWidth={SNAP_MARKER_STROKE_WIDTH_PX}
        />
      ) : role === 'center' ? (
        <circle cx={point.x} cy={point.y} r={radius} fill="none" stroke={SNAP_MARKER_COLOR} strokeWidth={SNAP_MARKER_STROKE_WIDTH_PX} />
      ) : role === 'intersection' ? (
        <>
          <line x1={point.x - radius} y1={point.y - radius} x2={point.x + radius} y2={point.y + radius} stroke={SNAP_MARKER_COLOR} strokeWidth={SNAP_MARKER_STROKE_WIDTH_PX} />
          <line x1={point.x + radius} y1={point.y - radius} x2={point.x - radius} y2={point.y + radius} stroke={SNAP_MARKER_COLOR} strokeWidth={SNAP_MARKER_STROKE_WIDTH_PX} />
        </>
      ) : role === 'edge' || role === 'bounds' ? (
        <rect
          x={point.x - radius}
          y={point.y - radius}
          width={radius * 2}
          height={radius * 2}
          transform={`rotate(45 ${point.x} ${point.y})`}
          fill="none"
          stroke={SNAP_MARKER_COLOR}
          strokeWidth={SNAP_MARKER_STROKE_WIDTH_PX}
        />
      ) : (
        <rect
          x={point.x - radius}
          y={point.y - radius}
          width={radius * 2}
          height={radius * 2}
          fill="none"
          stroke={SNAP_MARKER_COLOR}
          strokeWidth={SNAP_MARKER_STROKE_WIDTH_PX}
        />
      )}
    </g>
  );
}

function DraftingGuides({
  trackingResult,
  orthogonalConstraint,
  transform,
}: {
  trackingResult: ObjectSnapTrackingResult | null;
  orthogonalConstraint: OrthogonalConstraint | null;
  transform: PageTransform;
}) {
  const resolvedPoint = trackingResult?.point ?? orthogonalConstraint?.point ?? null;
  const viewportResolvedPoint = resolvedPoint ? transform.pdfToViewport(resolvedPoint) : null;

  return (
    <g
      className="pointer-events-none"
      data-testid="drafting-guides"
      data-tracking-guide-count={trackingResult?.guides.length ?? 0}
      data-orthogonal-axis={orthogonalConstraint?.axis}
      aria-hidden="true"
    >
      {orthogonalConstraint ? (() => {
        const start = transform.pdfToViewport(orthogonalConstraint.anchor);
        const end = transform.pdfToViewport(orthogonalConstraint.point);
        return (
          <line
            data-testid="orthogonal-guide"
            x1={start.x}
            y1={start.y}
            x2={end.x}
            y2={end.y}
            stroke="#2563eb"
            strokeWidth={1.25}
            strokeDasharray="6 4"
            opacity={0.82}
          />
        );
      })() : null}
      {trackingResult?.guides.map((guide, index) => {
        const origin = transform.pdfToViewport(guide.origin);
        if (!viewportResolvedPoint) {
          return null;
        }
        return (
          <g
            key={`${trackingPointKey({ point: guide.origin })}-${guide.axis}-${index}`}
            data-testid="object-snap-tracking-source"
            data-tracking-axis={guide.axis}
            data-snap-source={guide.source}
            data-snap-owner-id={guide.ownerId}
          >
            <line
              data-testid="object-snap-tracking-guide"
              x1={origin.x}
              y1={origin.y}
              x2={viewportResolvedPoint.x}
              y2={viewportResolvedPoint.y}
              stroke={SNAP_MARKER_COLOR}
              strokeWidth={1.25}
              strokeDasharray="4 4"
              opacity={0.9}
            />
            <line x1={origin.x - 5} y1={origin.y} x2={origin.x + 5} y2={origin.y} stroke={SNAP_MARKER_COLOR} strokeWidth={2} />
            <line x1={origin.x} y1={origin.y - 5} x2={origin.x} y2={origin.y + 5} stroke={SNAP_MARKER_COLOR} strokeWidth={2} />
          </g>
        );
      })}
      {trackingResult && viewportResolvedPoint ? (
        <g data-testid="object-snap-tracking-point">
          <line
            x1={viewportResolvedPoint.x - SNAP_MARKER_RADIUS_PX}
            y1={viewportResolvedPoint.y - SNAP_MARKER_RADIUS_PX}
            x2={viewportResolvedPoint.x + SNAP_MARKER_RADIUS_PX}
            y2={viewportResolvedPoint.y + SNAP_MARKER_RADIUS_PX}
            stroke={SNAP_MARKER_COLOR}
            strokeWidth={SNAP_MARKER_STROKE_WIDTH_PX}
          />
          <line
            x1={viewportResolvedPoint.x + SNAP_MARKER_RADIUS_PX}
            y1={viewportResolvedPoint.y - SNAP_MARKER_RADIUS_PX}
            x2={viewportResolvedPoint.x - SNAP_MARKER_RADIUS_PX}
            y2={viewportResolvedPoint.y + SNAP_MARKER_RADIUS_PX}
            stroke={SNAP_MARKER_COLOR}
            strokeWidth={SNAP_MARKER_STROKE_WIDTH_PX}
          />
        </g>
      ) : null}
    </g>
  );
}

function RelationshipGuides({
  guides,
  transform,
}: {
  guides: readonly RelationshipSnapGuide[];
  transform: PageTransform;
}) {
  return (
    <g className="pointer-events-none" data-testid="relationship-snap-guides" aria-hidden="true">
      {guides.map((guide, index) => {
        if (guide.kind === 'equal-size') {
          const moving = guide.moving;
          const reference = guide.reference.rect;
          const movingPoints = guide.axis === 'horizontal'
            ? [pdfPoint(moving.x, moving.y), pdfPoint(moving.x + moving.width, moving.y)] as const
            : [pdfPoint(moving.x + moving.width, moving.y), pdfPoint(moving.x + moving.width, moving.y + moving.height)] as const;
          const referencePoints = guide.axis === 'horizontal'
            ? [pdfPoint(reference.x, reference.y), pdfPoint(reference.x + reference.width, reference.y)] as const
            : [pdfPoint(reference.x + reference.width, reference.y), pdfPoint(reference.x + reference.width, reference.y + reference.height)] as const;
          return (
            <g
              key={`equal-size-${guide.axis}-${guide.reference.ownerId}-${index}`}
              data-testid="equal-size-guide"
              data-guide-axis={guide.axis}
              data-reference-owner-id={guide.reference.ownerId}
            >
              <RelationshipMeasurement start={movingPoints[0]} end={movingPoints[1]} axis={guide.axis} transform={transform} offsetPx={8} />
              <RelationshipMeasurement start={referencePoints[0]} end={referencePoints[1]} axis={guide.axis} transform={transform} offsetPx={8} />
            </g>
          );
        }

        const moving = guide.moving;
        const before = guide.before.rect;
        const after = guide.after.rect;
        const cross = guide.axis === 'horizontal'
          ? moving.y + moving.height / 2
          : moving.x + moving.width / 2;
        const firstPoints = guide.axis === 'horizontal'
          ? guide.placement === 'before'
            ? [pdfPoint(moving.x + moving.width, cross), pdfPoint(before.x, cross)] as const
            : [pdfPoint(before.x + before.width, cross), pdfPoint(guide.placement === 'between' ? moving.x : after.x, cross)] as const
          : guide.placement === 'before'
            ? [pdfPoint(cross, moving.y + moving.height), pdfPoint(cross, before.y)] as const
            : [pdfPoint(cross, before.y + before.height), pdfPoint(cross, guide.placement === 'between' ? moving.y : after.y)] as const;
        const secondPoints = guide.axis === 'horizontal'
          ? guide.placement === 'after'
            ? [pdfPoint(after.x + after.width, cross), pdfPoint(moving.x, cross)] as const
            : [pdfPoint(guide.placement === 'between' ? moving.x + moving.width : before.x + before.width, cross), pdfPoint(after.x, cross)] as const
          : guide.placement === 'after'
            ? [pdfPoint(cross, after.y + after.height), pdfPoint(cross, moving.y)] as const
            : [pdfPoint(cross, guide.placement === 'between' ? moving.y + moving.height : before.y + before.height), pdfPoint(cross, after.y)] as const;
        return (
          <g
            key={`equal-spacing-${guide.axis}-${guide.before.ownerId}-${guide.after.ownerId}-${index}`}
            data-testid="equal-spacing-guide"
            data-guide-axis={guide.axis}
            data-guide-placement={guide.placement}
            data-before-owner-id={guide.before.ownerId}
            data-after-owner-id={guide.after.ownerId}
          >
            <RelationshipMeasurement start={firstPoints[0]} end={firstPoints[1]} axis={guide.axis} transform={transform} />
            <RelationshipMeasurement start={secondPoints[0]} end={secondPoints[1]} axis={guide.axis} transform={transform} />
          </g>
        );
      })}
    </g>
  );
}

function RelationshipMeasurement({
  start,
  end,
  axis,
  transform,
  offsetPx = 0,
}: {
  start: PdfPoint;
  end: PdfPoint;
  axis: 'horizontal' | 'vertical';
  transform: PageTransform;
  offsetPx?: number;
}) {
  const rawViewportStart = transform.pdfToViewport(start);
  const rawViewportEnd = transform.pdfToViewport(end);
  const viewportStart = {
    x: rawViewportStart.x + (axis === 'vertical' ? offsetPx : 0),
    y: rawViewportStart.y + (axis === 'horizontal' ? offsetPx : 0),
  };
  const viewportEnd = {
    x: rawViewportEnd.x + (axis === 'vertical' ? offsetPx : 0),
    y: rawViewportEnd.y + (axis === 'horizontal' ? offsetPx : 0),
  };
  const tick = 4;
  const middleX = (viewportStart.x + viewportEnd.x) / 2;
  const middleY = (viewportStart.y + viewportEnd.y) / 2;
  return (
    <g data-testid="relationship-measurement">
      <line x1={viewportStart.x} y1={viewportStart.y} x2={viewportEnd.x} y2={viewportEnd.y} stroke="#16a34a" strokeWidth={1.25} />
      <line
        x1={viewportStart.x + (axis === 'vertical' ? -tick : 0)}
        y1={viewportStart.y + (axis === 'horizontal' ? -tick : 0)}
        x2={viewportStart.x + (axis === 'vertical' ? tick : 0)}
        y2={viewportStart.y + (axis === 'horizontal' ? tick : 0)}
        stroke="#16a34a"
        strokeWidth={1.25}
      />
      <line
        x1={viewportEnd.x + (axis === 'vertical' ? -tick : 0)}
        y1={viewportEnd.y + (axis === 'horizontal' ? -tick : 0)}
        x2={viewportEnd.x + (axis === 'vertical' ? tick : 0)}
        y2={viewportEnd.y + (axis === 'horizontal' ? tick : 0)}
        stroke="#16a34a"
        strokeWidth={1.25}
      />
      <text
        x={middleX}
        y={middleY + 4}
        textAnchor="middle"
        fontSize={11}
        fontWeight={700}
        fill="#16a34a"
        stroke="white"
        strokeWidth={3}
        paintOrder="stroke"
      >
        =
      </text>
    </g>
  );
}

function selectionLineBoundsPathData(start: PdfPoint, end: PdfPoint, transform: PageTransform, halfWidth: number): string {
  const viewportStart = transform.pdfToViewport(start);
  const viewportEnd = transform.pdfToViewport(end);
  const dx = viewportEnd.x - viewportStart.x;
  const dy = viewportEnd.y - viewportStart.y;
  const length = Math.hypot(dx, dy);
  if (length <= 0) {
    return '';
  }

  const tx = dx / length;
  const ty = dy / length;
  const nx = -ty;
  const ny = tx;
  const startCap = {
    x: viewportStart.x - tx * halfWidth,
    y: viewportStart.y - ty * halfWidth,
  };
  const endCap = {
    x: viewportEnd.x + tx * halfWidth,
    y: viewportEnd.y + ty * halfWidth,
  };
  const corners = [
    { x: startCap.x + nx * halfWidth, y: startCap.y + ny * halfWidth },
    { x: endCap.x + nx * halfWidth, y: endCap.y + ny * halfWidth },
    { x: endCap.x - nx * halfWidth, y: endCap.y - ny * halfWidth },
    { x: startCap.x - nx * halfWidth, y: startCap.y - ny * halfWidth },
  ];

  return `M ${corners[0].x} ${corners[0].y} L ${corners[1].x} ${corners[1].y} L ${corners[2].x} ${corners[2].y} L ${corners[3].x} ${corners[3].y} Z`;
}

function lineChromeHalfWidth(style: ReturnType<typeof getChromeStyle>): number {
  return Math.max(5, style.boundsOutsetPx);
}

function ReadOnlyCalloutAnnotation({ markup, transform }: { markup: Extract<Markup, { kind: 'callout' }>; transform: PageTransform }) {
  const contentStyle = getAnnotationContentStyle(markup);
  const linePoints = markup.leader.points
    .map((point) => {
      const viewPoint = transform.pdfToViewport(point);
      return `${viewPoint.x},${viewPoint.y}`;
    })
    .join(' ');
  const textBox = transform.pdfRectToViewport(markup.textBox);
  const fontSize = 12 * transform.zoom;
  const textInsetX = 8 * transform.zoom;
  const textBaselineOffset = 18 * transform.zoom;

  return (
    <g data-testid={`markup-${markup.id}`} opacity={contentStyle.opacity}>
      <polyline points={linePoints} fill="none" stroke={contentStyle.stroke} strokeWidth={contentStyle.strokeWidth} />
      <rect
        x={textBox.x}
        y={textBox.y}
        width={textBox.width}
        height={textBox.height}
        rx={4 * transform.zoom}
        fill={contentStyle.fill}
        stroke={contentStyle.stroke}
        strokeWidth={contentStyle.strokeWidth}
      />
      <text x={textBox.x + textInsetX} y={textBox.y + textBaselineOffset} fill="#0f172a" fontSize={fontSize} className="select-none">
        {markup.text || 'Callout'}
      </text>
    </g>
  );
}

export function getInteractionState(
  markupId: string,
  selectedMarkupIds: ReadonlySet<string>,
  primaryMarkupId: string | undefined,
  hoveredMarkupId: string | null,
  marqueeHoveredMarkupIds?: ReadonlySet<string>,
  snapSourceMarkupIds?: ReadonlySet<string>,
): InteractionState {
  if (marqueeHoveredMarkupIds?.has(markupId)) {
    return 'hovered';
  }

  if (selectedMarkupIds.has(markupId)) {
    return primaryMarkupId === markupId ? 'focused' : 'selected';
  }

  if (snapSourceMarkupIds?.has(markupId)) {
    return 'hovered';
  }

  if (hoveredMarkupId === markupId) {
    return 'hovered';
  }

  return 'idle';
}

function pdfToleranceForScale(scale: number): number {
  return Math.max(2, 12 / Math.max(0.1, scale));
}

function nearestSnapResult(left: SnapResult | null, right: SnapResult | null): SnapResult | null {
  if (!left) {
    return right;
  }
  if (!right) {
    return left;
  }
  return left.distancePx <= right.distancePx ? left : right;
}

function shouldSnapCreationTool(tool: ToolMode): boolean {
  return tool !== 'select'
    && tool !== 'pan'
    && tool !== 'pen'
    && tool !== 'highlight'
    && tool !== 'snapshot';
}

export function orthogonalAnchorForDraft(draft: AnnotationDraft | null, activeTool: ToolMode): PdfPoint | null {
  if (!draft) {
    return null;
  }

  if (draft.kind === 'line') {
    return isCloudTool(activeTool) ? null : draft.start;
  }

  if (draft.kind === 'measurement-path' || draft.kind === 'cloud-node' || draft.kind === 'vertex-path') {
    return draft.points.at(-1) ?? draft.start;
  }

  if (draft.kind === 'arc' && draft.phase === 'end') {
    return draft.start;
  }

  return null;
}

function isMeasurementTool(tool: ToolMode): tool is Extract<ToolMode, 'length' | 'polylength' | 'area'> {
  return tool === 'length' || tool === 'polylength' || tool === 'area';
}

function isVertexPathTool(tool: ToolMode): tool is Extract<ToolMode, 'polyline' | 'polygon'> {
  return tool === 'polyline' || tool === 'polygon';
}

function isEditableKeyboardTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target.isContentEditable) {
    return true;
  }
  const tagName = target.tagName.toLowerCase();
  return tagName === 'input' || tagName === 'textarea' || tagName === 'select';
}

function transformPdfPathData(d: string, transform: PageTransform): string {
  const tokens = d.match(/[MLCZ]|-?\d+(?:\.\d+)?/g) ?? [];
  const out: string[] = [];
  for (let index = 0; index < tokens.length;) {
    const token = tokens[index++];
    if (token === 'M' || token === 'L') {
      const point = transform.pdfToViewport(pdfPoint(Number(tokens[index++]), Number(tokens[index++])));
      out.push(`${token} ${point.x} ${point.y}`);
      continue;
    }
    if (token === 'C') {
      const points = [0, 1, 2].map(() => transform.pdfToViewport(pdfPoint(Number(tokens[index++]), Number(tokens[index++]))));
      out.push(`C ${points.map((point) => `${point.x} ${point.y}`).join(' ')}`);
      continue;
    }
    if (token === 'Z') {
      out.push('Z');
    }
  }
  return out.join(' ');
}

function pdfHandleToleranceForScale(scale: number): number {
  return Math.max(4, 9 / Math.max(0.1, scale));
}

function viewportHandleToleranceForScale(scale: number): number {
  return pdfHandleToleranceForScale(scale) * scale;
}

function hitTestChromeHandles(
  handles: readonly ToolHandleDescriptor[],
  point: PdfPoint,
  chrome: SelectionChromeDescriptor,
  transform: PageTransform,
  state: InteractionState,
): ToolHandleDescriptor | null {
  if (!chrome.bounds) {
    return null;
  }

  const box = transform.pdfRectToViewport(chrome.bounds.rect);
  const style = getChromeStyle(state, chrome.bounds.kind as ChromeBoundsKind);
  const chromeBox = expandViewportRect(box, style.boundsOutsetPx);
  const viewportPoint = transform.pdfToViewport(point);
  const handleTestPoint = chrome.bounds.rotation
    ? unrotateViewportPointAroundBounds(viewportPoint, box, chrome.bounds.rotation)
    : viewportPoint;
  const tolerance = viewportHandleToleranceForScale(transform.zoom);

  for (let index = handles.length - 1; index >= 0; index -= 1) {
    const handle = handles[index];
    if (state === 'hovered' && handle.behavior === 'rotateSelf') {
      continue;
    }
    const handlePoint = shouldProjectHandleToChromeBounds(handle, chrome)
      ? projectChromeHandlePoint(
        transform.pdfToViewport(handle.point),
        box,
        chromeBox,
        handle.behavior === 'rotateSelf' ? ROTATION_HANDLE_OFFSET_PX : undefined,
      )
      : transform.pdfToViewport(handle.point);
    if (distance(handleTestPoint, handlePoint) <= tolerance) {
      return handle;
    }
  }

  return null;
}

function cursorForHit(hit: ToolHit | null): string | null {
  if (!hit) {
    return null;
  }

  if (hit.cursor) {
    return hit.cursor;
  }

  if (hit.bodyDrag === 'moveSelf' || hit.bodyDrag === 'moveGroup') {
    return getMoveCursor();
  }

  return null;
}

function cursorForHandle(handleId: string, behavior: string, rotation: number): string | null {
  if (behavior === 'rotateSelf') {
    return getRotateCursor();
  }

  if (behavior === 'resizeSelf') {
    const handle = resizeHandleKindFromId(handleId);
    return handle ? getResizeCursor(handle, rotation) : null;
  }

  return null;
}

function resizeHandleKindFromId(handleId: string) {
  const handle = handleId.split('.').at(-1);
  if (
    handle === 'nw'
    || handle === 'n'
    || handle === 'ne'
    || handle === 'e'
    || handle === 'se'
    || handle === 's'
    || handle === 'sw'
    || handle === 'w'
  ) {
    return handle;
  }

  return null;
}

function shouldProjectHandleToChromeBounds(handle: ToolHandleDescriptor, chrome: SelectionChromeDescriptor): boolean {
  return chrome.bounds?.kind !== 'group' && handle.behavior === 'rotateSelf';
}

function hitTestToolMarkups(
  markups: readonly Markup[],
  point: PdfPoint,
  context: { page: PageModel; tolerance: number; transform?: PageTransform },
) {
  for (let index = markups.length - 1; index >= 0; index -= 1) {
    const markup = markups[index];
    const definition = getMarkupToolDefinition(markup);
    const hit = definition?.geometry?.hitTest(markup as never, point, context);
    if (hit) {
      return hit;
    }
  }

  return null;
}

function moveDraftMarkups(
  markups: readonly Markup[],
  draft: Extract<AnnotationDraft, { kind: 'move' }>,
  delta: PdfPoint,
  page: PageModel,
): readonly Markup[] {
  const interactionContext = interactionContextForPage(page, markups);
  return markups.flatMap((markup) => {
    if (!draft.markupIds.includes(markup.id)) {
      return [];
    }

    const definition = draft.componentId && draft.bodyDrag ? getMarkupToolDefinition(markup) : null;
    const dragged = definition?.interaction?.dragMarkup?.(markup as never, {
      componentId: draft.componentId ?? '',
      bodyDrag: draft.bodyDrag ?? 'moveSelf',
      delta,
    }, interactionContext);
    return [dragged ?? translateMarkup(markup, delta)];
  });
}

function applyMarkupPreview(
  markups: readonly Markup[],
  preview: readonly Markup[] | null,
): readonly Markup[] {
  if (!preview || preview.length === 0) {
    return markups;
  }

  const replacements = new Map(preview.map((markup) => [markup.id, markup]));
  return markups.map((markup) => replacements.get(markup.id) ?? markup);
}

function movingSnapAnchorPoints(
  markups: readonly Markup[],
  markupIds: readonly string[],
  page: PageModel,
  maximum = 128,
): readonly PdfPoint[] {
  const movingMarkupIds = new Set(markupIds);
  const seen = new Set<string>();
  const points = getAnnotationSnapCandidates(
    markups.filter((markup) => movingMarkupIds.has(markup.id)),
    page,
  ).flatMap((candidate) => {
    if (candidate.kind !== 'point' || candidate.role === 'intersection') {
      return [];
    }
    const key = trackingPointKey(candidate);
    if (seen.has(key)) {
      return [];
    }
    seen.add(key);
    return [candidate.point];
  });

  if (points.length <= maximum) {
    return points;
  }

  const sampled: PdfPoint[] = [];
  for (let index = 0; index < maximum; index += 1) {
    sampled.push(points[Math.round(index * (points.length - 1) / (maximum - 1))]);
  }
  return sampled;
}

function translatePoint(point: PdfPoint, delta: PdfPoint): PdfPoint {
  return pdfPoint(point.x + delta.x, point.y + delta.y);
}

function combinedGuideBounds(guides: readonly { readonly rect: Rect }[]): Rect | null {
  if (guides.length === 0) {
    return null;
  }
  const left = Math.min(...guides.map((guide) => guide.rect.x));
  const bottom = Math.min(...guides.map((guide) => guide.rect.y));
  const right = Math.max(...guides.map((guide) => guide.rect.x + guide.rect.width));
  const top = Math.max(...guides.map((guide) => guide.rect.y + guide.rect.height));
  return rect(left, bottom, right - left, top - bottom);
}

function draftRect(draft: { readonly start: PdfPoint; readonly current: PdfPoint }): Rect {
  return rect(
    Math.min(draft.start.x, draft.current.x),
    Math.min(draft.start.y, draft.current.y),
    Math.abs(draft.current.x - draft.start.x),
    Math.abs(draft.current.y - draft.start.y),
  );
}

export function interactionContextForPage(page: PageModel, markups: readonly Markup[]) {
  return {
    page,
    pageBounds: rect(0, 0, page.size.width, page.size.height),
    markups: markups.filter((markup) => markup.pageIndex === page.index),
  } as const;
}

function distance(a: { readonly x: number; readonly y: number }, b: { readonly x: number; readonly y: number }): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

export function imagePlacementRect(point: PdfPoint, page: PageModel, asset: PendingImageAsset) {
  const aspectRatio = Math.max(0.01, asset.width / Math.max(1, asset.height));
  const maxWidth = page.size.width * 0.45;
  const maxHeight = page.size.height * 0.45;
  const naturalWidth = Math.max(24, asset.width);
  const naturalHeight = naturalWidth / aspectRatio;
  const scale = Math.min(1, maxWidth / naturalWidth, maxHeight / naturalHeight);
  const width = Math.max(24, naturalWidth * scale);
  const height = Math.max(24, width / aspectRatio);
  const x = Math.min(Math.max(0, point.x - width * 0.5), Math.max(0, page.size.width - width));
  const y = Math.min(Math.max(0, point.y - height * 0.5), Math.max(0, page.size.height - height));
  return rect(x, y, width, height);
}

function fallbackSnapshotDataUrl(): string {
  return DEFAULT_IMAGE_DATA_URL;
}

export function shouldCancelDraftForToolChange(
  previousTool: ToolMode,
  activeTool: ToolMode,
  draft: AnnotationDraft | null,
): boolean {
  return previousTool !== activeTool && draft !== null;
}

function createMarkupId(prefix: string): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return `${prefix}-${crypto.randomUUID()}`;
  }

  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

type ReactPointerEventLike = Pick<React.PointerEvent<SVGElement>, 'clientX' | 'clientY'>;
