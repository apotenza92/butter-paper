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
import type { PendingImageAsset, SnapTarget } from '../state/viewerStore';
import {
  addCloudNodeDraftPoint,
  addMeasurementPathDraftPoint,
  beginCloudNodeDraftPoint,
  createArcDraft,
  createCloudNodeDraft,
  createMeasurementPathDraft,
  createMoveDraft,
  createTransformDraft,
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
  type AnnotationDraft,
  type MeasurementPathDraft,
} from '../pdf-tools/annotationLifecycle';
import { createArcMarkupFromThreePoints } from '../pdf-tools/builtins/arcTool';
import { createAreaMeasurementMarkup, createLengthMeasurementMarkup, createPolylengthMeasurementMarkup } from '../pdf-tools/builtins/measurementTool';
import { dimensionCaptionRect } from '../pdf-tools/builtins/dimensionTool';
import { annotationFontCssFamily, getAnnotationContentStyle, getAnnotationTextContentStyle } from '../pdf-tools/annotationStyles';
import { DEFAULT_IMAGE_DATA_URL } from '../pdf-tools/builtins/imageTool';
import { getChromeStyle, getMoveCursor, getResizeCursor, getRotateCursor, type ChromeBoundsKind, type InteractionState } from '../pdf-tools/interactionChrome';
import { expandViewportRect, projectChromeHandlePoint, unrotateViewportPointAroundBounds } from '../pdf-tools/selectionHitZones';
import { layoutTextBoxLines, measureAnnotationText, splitAnnotationTextLines, type TextMeasurementContext } from '../pdf-tools/textLayout';
import { getMarkupToolDefinition, getToolDefinition } from '../pdf-tools/toolRegistry';
import { findNearestSnapPoint, getAnnotationSnapCandidates, type SnapCandidate, type SnapResult } from '../pdf-tools/snapping';
import type { RenderPrimitive, SelectionChromeDescriptor, ToolHandleDescriptor, ToolHit } from '../pdf-tools/types';

type PageTransform = ReturnType<typeof createPageTransform>;
const IMPORTED_MARKUP_MIN_RENDER_ZOOM = 0.35;
const TEXT_BOX_GHOST_CURSOR = createTextBoxGhostCursor();

interface TextEditState {
  readonly markupId: string;
  readonly text: string;
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
}

type EditableTextMarkup = Extract<Markup, { kind: 'text-box' | 'callout' | 'cloud-plus' | 'dimension' }>;

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
  activeTool: ToolMode;
  selectedMarkupIds: readonly string[];
  pendingImageAsset: PendingImageAsset | null;
  setSelectedMarkupIds: (markupIds: string[]) => void;
  consumePendingImageAsset: () => PendingImageAsset | null;
  createSnapshotDataUrl?: (rect: Rect) => string | null;
  updateDocument: (updater: (document: DocumentModel) => DocumentModel) => void;
  onToolError?: (message: string) => void;
  calibrationPickActive?: boolean;
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
  activeTool,
  selectedMarkupIds,
  pendingImageAsset,
  setSelectedMarkupIds,
  consumePendingImageAsset,
  createSnapshotDataUrl,
  updateDocument,
  onToolError,
  calibrationPickActive = false,
  onCalibrationPoint,
}: AnnotationLayerProps) {
  const svgRef = useRef<SVGSVGElement | null>(null);
  const [draft, setDraft] = useState<AnnotationDraft | null>(null);
  const activeToolRef = useRef(activeTool);
  const clickPlacementToolRef = useRef<ToolMode | null>(null);
  const [hoveredMarkupId, setHoveredMarkupId] = useState<string | null>(null);
  const [hoverCursor, setHoverCursor] = useState<string | null>(null);
  const [textEdit, setTextEdit] = useState<TextEditState | null>(null);
  const [pendingTextBox, setPendingTextBox] = useState<TextBoxMarkup | null>(null);
  const [snapResult, setSnapResult] = useState<SnapResult | null>(null);

  const selectedMarkupIdSet = useMemo(() => new Set(selectedMarkupIds), [selectedMarkupIds]);
  const visibleMarkups = useMemo(() => (
    markups.filter((markup) => shouldRenderMarkupAtZoom(markup, transform))
  ), [markups, transform.zoom]);
  const annotationSnapCandidates = useMemo(() => (
    getAnnotationSnapCandidates(visibleMarkups, page)
  ), [visibleMarkups, page]);

  useEffect(() => {
    const previousTool = activeToolRef.current;
    if (previousTool === activeTool) {
      return;
    }

    activeToolRef.current = activeTool;
    if (!shouldCancelDraftForToolChange(previousTool, activeTool, draft)) {
      return;
    }

    setDraft(null);
    clickPlacementToolRef.current = null;
    setSnapResult(null);
    setHoveredMarkupId(null);
    setHoverCursor(null);
  }, [activeTool, draft]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isEditableKeyboardTarget(event.target)) {
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
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [draft, page.index, pageScale]);

  function toPdfPoint(event: ReactPointerEventLike): PdfPoint {
    const bounds = svgRef.current?.getBoundingClientRect();
    if (!bounds) {
      return pdfPoint(0, 0);
    }

    return transform.viewportToPdf({
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top,
    } as never);
  }

  function snapPdfPoint(point: PdfPoint, options: SnapPointOptions): PdfPoint {
    if (!options.enabled) {
      setSnapResult(null);
      return point;
    }

    const excludedIds = options.excludeMarkupIds ? new Set(options.excludeMarkupIds) : undefined;
    if ((!snapToMarkup || annotationSnapCandidates.length === 0) && (!snapToContent || pdfContentSnapCandidates.length === 0)) {
      setSnapResult(null);
      return point;
    }

    const markupResult = snapToMarkup
      ? findNearestSnapPoint(point, annotationSnapCandidates, transform, {
        tolerancePx: snapTolerancePx,
        excludeOwnerIds: excludedIds,
        snapTargets,
      })
      : null;
    const contentResult = snapToContent
      ? findNearestSnapPoint(point, pdfContentSnapCandidates, transform, {
        tolerancePx: snapTolerancePx,
        snapTargets,
      })
      : null;
    const result = nearestSnapResult(markupResult, contentResult);
    setSnapResult(result);
    return result?.point ?? point;
  }

  function handlePointerDown(event: React.PointerEvent<SVGSVGElement>): void {
    if (event.button !== 0) {
      return;
    }

    if (activeTool === 'pan') {
      return;
    }

    const rawPoint = toPdfPoint(event);
    const point = snapPdfPoint(rawPoint, { enabled: shouldSnapCreationTool(activeTool) });
    if (activeTool === 'text-box') {
      beginTextBoxPlacement(point);
      event.stopPropagation();
      event.preventDefault();
      return;
    }

    if (activeTool === 'arc') {
      handleArcPointerDown(point);
      setSelectedMarkupIds([]);
      return;
    }

    if (activeTool === 'image') {
      if (placePendingImage(point)) {
        return;
      }
    }

    if (activeTool === 'cloud' && draft?.kind === 'cloud-node') {
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

    if (draft && isClickPlacementDraft(draft)) {
      if (commitClickPlacementDraft(point)) {
        event.stopPropagation();
        event.preventDefault();
      }
      return;
    }

    if (activeTool === 'select' && beginHandleTransform(event, point)) {
      return;
    }

    if (activeTool === 'select' && event.detail >= 2 && beginTextEditFromHit(event, point)) {
      return;
    }

    if (activeTool === 'select' && beginMarkupMoveFromHit(event, point)) {
      return;
    }

    const activeDefinition = getToolDefinition(activeTool);
    if (activeDefinition.category === 'markup' && activeDefinition.interaction?.createDraft) {
      setDraft(activeDefinition.interaction.createDraft({
        pointerId: event.pointerId,
        startPoint: point,
        currentPoint: point,
      }) as AnnotationDraft);
      if (activeDefinition.interaction.placement === 'click') {
        clickPlacementToolRef.current = activeTool;
      } else {
        clickPlacementToolRef.current = null;
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
    onCalibrationPoint?.(page.index, toPdfPoint(event));
  }

  function handleArcPointerDown(point: PdfPoint): void {
    if (draft?.kind === 'arc' && draft.phase === 'end') {
      if (shouldCommitLine(draft.start, point)) {
        setDraft(setArcDraftEnd(draft, point));
      }
      return;
    }

    if (draft?.kind === 'arc' && draft.phase === 'mid') {
      const markup = draft.end
        ? createArcMarkupFromThreePoints(createMarkupId('arc'), page.index, draft.start, draft.end, point)
        : null;
      if (markup) {
        updateDocument((document) => createMarkup(document, markup));
        setSelectedMarkupIds([markup.id]);
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
        page,
        hasExceededDragThreshold: hasExceededDragThreshold(startPoint, currentPoint),
        createMarkupId,
      });
      if (markup) {
        updateDocument((document) => createMarkup(document, markup));
        setSelectedMarkupIds([markup.id]);
        if (markup.kind === 'dimension') {
          setTextEdit({ markupId: markup.id, text: markup.text });
        }
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
          updateDocument((document) => createMarkup(document, markup));
          setSelectedMarkupIds([markup.id]);
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
    updateDocument((document) => createMarkup(document, markup));
    setSelectedMarkupIds([markup.id]);
    setDraft(null);
    setSnapResult(null);
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
      source: { source: 'butter' },
    });
    updateDocument((document) => createMarkup(document, markup));
    setSelectedMarkupIds([markup.id]);
    return true;
  }

  function beginTextBoxPlacement(point: PdfPoint): void {
    const fontSizePt = 12;
    const lineHeightPt = fontSizePt * 1.15;
    const pending = createTextBoxMarkup({
      id: createMarkupId('text'),
      pageIndex: page.index,
      rect: initialTextBoxRectAtPointer(point, transform, { fontSizePt, lineHeightPt }),
      text: '',
      color: '#ff0000',
      fontFamily: 'Helvetica',
      fontSizePt,
      lineHeightPt,
      source: { source: 'butter' },
    });
    setDraft(null);
    clickPlacementToolRef.current = null;
    setSnapResult(null);
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

    const markup = {
      ...pending,
      text,
      rect: autosizeTextBoxRectDownward(pending.rect, text, {
        fontFamily: annotationFontFamily(pending),
        fontSizePt: pending.fontSizePt,
        lineHeightPt: pending.lineHeightPt,
      }),
    };
    updateDocument((document) => createMarkup(document, markup));
    setSelectedMarkupIds([]);
  }

  function isClickPlacementDraft(candidate: AnnotationDraft): candidate is Extract<AnnotationDraft, { kind: 'line' | 'rectangle' | 'text-box' }> {
    return Boolean(clickPlacementToolRef.current)
      && (candidate.kind === 'line' || candidate.kind === 'rectangle' || candidate.kind === 'text-box');
  }

  function updateClickPlacementDraft(candidate: Extract<AnnotationDraft, { kind: 'line' | 'rectangle' | 'text-box' }>, point: PdfPoint): AnnotationDraft {
    if (candidate.kind === 'line') {
      return updateLineDraft(candidate, point);
    }
    if (candidate.kind === 'text-box') {
      return { ...candidate, current: point };
    }
    return { ...candidate, current: point };
  }

  function commitClickPlacementDraft(point: PdfPoint): boolean {
    if (!draft || !isClickPlacementDraft(draft)) {
      return false;
    }

    const tool = clickPlacementToolRef.current ?? activeTool;
    const definition = getToolDefinition(tool);
    const finalDraft = updateClickPlacementDraft(draft, point);
    const markup = definition.interaction?.commitDraft?.(finalDraft as never, {
      page,
      hasExceededDragThreshold: true,
      createMarkupId,
    });
    setDraft(null);
    clickPlacementToolRef.current = null;
    setSnapResult(null);
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
      updateDocument((document) => createMarkup(document, snapshotMarkup));
      setSelectedMarkupIds([snapshotMarkup.id]);
      return true;
    }

    updateDocument((document) => createMarkup(document, markup));
    setSelectedMarkupIds([markup.id]);
    if (markup.kind === 'text-box' || markup.kind === 'callout' || markup.kind === 'cloud-plus' || markup.kind === 'dimension') {
      setTextEdit({ markupId: markup.id, text: markup.text });
    }
    return true;
  }

  function handlePointerMove(event: React.PointerEvent<SVGSVGElement>): void {
    const rawPoint = toPdfPoint(event);
    const point = draft?.kind === 'transform'
      ? snapPdfPoint(rawPoint, { enabled: true, excludeMarkupIds: [draft.markupId] })
      : snapPdfPoint(rawPoint, { enabled: shouldSnapCreationTool(activeTool) });

    if (!draft) {
      const hit = activeTool === 'select' ? hitTestSelectMode(point) : null;
      setHoveredMarkupId(hit?.markupId ?? null);
      setHoverCursor(hit?.cursor ?? cursorForHit(hit));
      return;
    }

    if (isClickPlacementDraft(draft)) {
      setDraft(updateClickPlacementDraft(draft, point));
      return;
    }

    if (draft.kind === 'measurement-path') {
      setDraft(updateMeasurementPathDraft(draft, point));
      return;
    }

    if (draft.kind === 'arc') {
      setDraft(updateArcDraft(draft, point));
      return;
    }

    const activeDefinition = getToolDefinition(activeTool);
    if (activeDefinition.interaction?.updateDraft) {
      setDraft(activeDefinition.interaction.updateDraft(draft as never, point) as AnnotationDraft);
      return;
    }

    if (draft.kind === 'move') {
      if (draft.pointerId !== event.pointerId) {
        return;
      }

      const delta = moveDelta(draft, point);
      if (delta.x !== 0 || delta.y !== 0) {
        updateDocument((document) => moveDraftMarkups(document, draft, delta));
        setDraft({ ...draft, lastPoint: point });
      }
    }

    if (draft.kind === 'transform') {
      if (draft.pointerId !== event.pointerId) {
        return;
      }

      const nextDraft = updateTransformDraft(draft, point);
      const definition = getMarkupToolDefinition(draft.originalMarkup);
      const transformed = definition?.interaction?.transformMarkup?.(draft.originalMarkup as never, {
        handleId: draft.handleId,
        handleBehavior: draft.handleBehavior,
        startPoint: draft.startPoint,
        currentPoint: point,
      });

      if (transformed && transformed !== draft.originalMarkup) {
        updateDocument((document) => ({
          ...document,
          markups: document.markups.map((markup) => markup.id === draft.markupId ? transformed : markup),
        }));
      }
      setDraft(nextDraft);
    }
  }

  function handlePointerUp(event: React.PointerEvent<SVGSVGElement>): void {
    if (!draft) {
      return;
    }

    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }

    const rawPoint = toPdfPoint(event);
    const point = draft.kind === 'transform'
      ? snapPdfPoint(rawPoint, { enabled: true, excludeMarkupIds: [draft.markupId] })
      : snapPdfPoint(rawPoint, { enabled: shouldSnapCreationTool(activeTool) });
    if (isClickPlacementDraft(draft)) {
      return;
    }

    const activeDefinition = getToolDefinition(activeTool);
    if (draft.kind === 'arc') {
      return;
    }

    if (activeTool === 'dimension') {
      return;
    }

    if (activeTool === 'cloud' && draft.kind === 'cloud-node') {
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
      if (activeTool === 'cloud' && !exceededDragThreshold) {
        setDraft(createCloudNodeDraft(point));
        setSelectedMarkupIds([]);
        setSnapResult(null);
        return;
      }

      const markup = activeDefinition.interaction.commitDraft(draft as never, {
        page,
        hasExceededDragThreshold: exceededDragThreshold,
        createMarkupId,
      });
      if (markup) {
        updateDocument((document) => createMarkup(document, markup));
        setSelectedMarkupIds([markup.id]);
        if (markup.kind === 'dimension') {
          setTextEdit({ markupId: markup.id, text: markup.text });
        }
      }
    }

    setDraft(null);
    clickPlacementToolRef.current = null;
    setSnapResult(null);
  }

  function handlePointerCancel(event: React.PointerEvent<SVGSVGElement>): void {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setDraft(null);
    clickPlacementToolRef.current = null;
    setSnapResult(null);
  }

  function handleDoubleClick(event: React.MouseEvent<SVGSVGElement>): void {
    if (draft?.kind === 'measurement-path') {
      const point = snapPdfPoint(toPdfPoint(event), { enabled: shouldSnapCreationTool(activeTool) });
      const finalDraft = addMeasurementPathDraftPoint(draft, point);
      if (commitMeasurementPathDraft(finalDraft)) {
        event.stopPropagation();
        event.preventDefault();
      }
      return;
    }

    if (activeTool === 'cloud' && draft?.kind === 'cloud-node') {
      const activeDefinition = getToolDefinition(activeTool);
      const point = toPdfPoint(event);
      const finalDraft = addCloudNodeDraftPoint(draft, point);
      const markup = activeDefinition.interaction?.commitDraft?.(finalDraft as never, {
        page,
        hasExceededDragThreshold: true,
        createMarkupId,
      });
      if (markup) {
        updateDocument((document) => createMarkup(document, markup));
        setSelectedMarkupIds([markup.id]);
      }
      setDraft(null);
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

    if (beginTextEditFromHit(event, point)) {
      event.stopPropagation();
      event.preventDefault();
    }
  }

  function beginTextEditFromHit(event: React.PointerEvent<SVGElement> | React.MouseEvent<SVGSVGElement>, point: PdfPoint): boolean {
    const hit = hitTestToolMarkups(visibleMarkups, point, { page, tolerance: pdfToleranceForScale(transform.zoom), transform });
    const markup = hit ? visibleMarkups.find((candidate) => candidate.id === hit.markupId) : null;
    if (markup?.kind !== 'text-box' && markup?.kind !== 'callout' && markup?.kind !== 'cloud-plus' && markup?.kind !== 'dimension') {
      return false;
    }

    event.stopPropagation();
    setDraft(null);
    setSelectedMarkupIds([markup.id]);
    setHoveredMarkupId(markup.id);
    setTextEdit({ markupId: markup.id, text: markup.text });
    return true;
  }

  function beginMarkupMove(event: React.PointerEvent<SVGElement>, markup: Markup): void {
    if (event.button !== 0) {
      return;
    }

    if (activeTool !== 'select') {
      return;
    }

    const point = toPdfPoint(event);
    if (beginHandleTransform(event, point)) {
      return;
    }

    const definition = getMarkupToolDefinition(markup);
    if (!shouldRenderMarkupAtZoom(markup, transform)) {
      return;
    }

    const hit = definition?.geometry?.hitTest(markup as never, point, {
      page,
      tolerance: pdfToleranceForScale(transform.zoom),
      transform,
    });
    if (!hit) {
      return;
    }

    event.stopPropagation();
    const nextSelection = selectedMarkupIds.includes(markup.id) ? [...selectedMarkupIds] : [markup.id];
    setSelectedMarkupIds(nextSelection);
    setHoveredMarkupId(markup.id);
    setDraft(createMoveDraft(event.pointerId, nextSelection, point, {
      componentId: hit.componentId,
      bodyDrag: hit.bodyDrag,
    }));
    svgRef.current?.setPointerCapture(event.pointerId);
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
    const nextSelection = selectedMarkupIds.includes(markup.id) ? [...selectedMarkupIds] : [markup.id];
    setSelectedMarkupIds(nextSelection);
    setHoveredMarkupId(markup.id);
    setDraft(createMoveDraft(event.pointerId, nextSelection, point, {
      componentId: hit.componentId,
      bodyDrag: hit.bodyDrag,
    }));
    event.currentTarget.setPointerCapture(event.pointerId);
    return true;
  }

  function beginHandleTransform(event: React.PointerEvent<SVGElement>, point: PdfPoint): boolean {
    const hit = hitTestInteractiveHandles(point);
    if (!hit?.handleId || !hit.handleBehavior) {
      return false;
    }

    const markup = markups.find((candidate) => candidate.id === hit.markupId);
    if (!markup) {
      return false;
    }

    event.stopPropagation();
    setSelectedMarkupIds([markup.id]);
    setHoveredMarkupId(markup.id);
    if (hit.handleBehavior === 'rotateSelf' && event.detail >= 2) {
      resetMarkupRotation(markup.id);
      return true;
    }

    setDraft(createTransformDraft(event.pointerId, markup, hit.handleId, hit.handleBehavior, point));
    svgRef.current?.setPointerCapture(event.pointerId);
    return true;
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
      const definition = markup ? getMarkupToolDefinition(markup) : null;
      const interactionState = markupId === selectedMarkupIds[0] ? 'focused' : 'selected';
      const chrome = markup && definition?.selection?.getSelectionChrome(markup as never, { page, phase: interactionState });
      const handles = chrome?.handles?.filter((handle) => {
        return isHandleVisibleForBounds(handle, chrome);
      }) ?? [];
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
    const definition = markup ? getMarkupToolDefinition(markup) : null;
    const chrome = markup && definition?.selection?.getSelectionChrome(markup as never, { page, phase: 'hovered' });
    const handles = chrome?.handles?.filter((handle) => {
      return isHandleVisibleForBounds(handle, chrome);
    }) ?? [];
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
        candidate.id === markupId && (candidate.kind === 'rectangle' || candidate.kind === 'ellipse')
          ? { ...candidate, rotation: 0 }
          : candidate.id === markupId && candidate.kind === 'text-box'
            ? { ...candidate, rotation: 0 }
          : candidate
      )),
    }));
  }

  function commitTextEdit(markupId: string, text: string): void {
    updateDocument((document) => updateMarkupText(document, markupId, text));
    setTextEdit(null);
    const markup = visibleMarkups.find((candidate) => candidate.id === markupId);
    if (markup?.kind === 'text-box') {
      setSelectedMarkupIds([]);
    }
  }

  const toolCursor = calibrationPickActive
    ? 'crosshair'
    : activeTool === 'text-box' && pendingTextBox
      ? 'default'
      : activeTool === 'text-box' && !textEdit
        ? TEXT_BOX_GHOST_CURSOR
      : hoverCursor ?? getToolDefinition(activeTool).cursor;
  const activeDefinition = getToolDefinition(activeTool);
  const draftPrimitives = draft && activeDefinition.render?.getDraftPrimitives
    ? activeDefinition.render.getDraftPrimitives(draft as never, { page, pageScale, phase: 'draft' })
    : [];
  const editingMarkup = textEdit
      ? visibleMarkups.find((markup): markup is EditableTextMarkup => (
      markup.id === textEdit.markupId
      && (markup.kind === 'text-box' || markup.kind === 'callout' || markup.kind === 'cloud-plus' || markup.kind === 'dimension')
    ))
    : null;

  return (
    <svg
      ref={svgRef}
      className="absolute inset-0 z-20 h-full w-full overflow-visible"
      data-testid={`annotation-layer-${page.index + 1}`}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerCancel}
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
      onPointerLeave={() => {
        if (!draft) {
          setHoveredMarkupId(null);
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
          const interactionState = getInteractionState(markup.id, selectedMarkupIdSet, selectedMarkupIds[0], hoveredMarkupId);
          return (
            <PrimitiveAnnotation
              key={markup.id}
              markup={markup}
              definition={definition}
              transform={transform}
              page={page}
              pageScale={pageScale}
              interactionState={interactionState}
              editingText={textEdit?.markupId === markup.id ? textEdit.text : null}
              onPointerDown={(event) => beginMarkupMove(event, markup)}
            />
          );
        }

        if (markup.kind === 'callout') {
          return <ReadOnlyCalloutAnnotation key={markup.id} markup={markup} transform={transform} />;
        }

        return null;
      })}

      {pendingTextBox ? (
        <PrimitiveAnnotation
          markup={pendingTextBox}
          definition={getMarkupToolDefinition(pendingTextBox)!}
          transform={transform}
          page={page}
          pageScale={pageScale}
          interactionState="focused"
          editingText={pendingTextBox.text}
          onPointerDown={() => undefined}
        />
      ) : null}

      {draftPrimitives.map((primitive, index) => (
        <RenderPrimitiveElement key={`draft-${index}`} primitive={primitive} transform={transform} />
      ))}
      {snapResult ? <SnapIndicator result={snapResult} transform={transform} /> : null}
      {pendingTextBox ? (
        <TextBoxEditor
          markup={pendingTextBox}
          text={pendingTextBox.text}
          transform={transform}
          selectOnFocus={false}
          onChange={updatePendingTextBox}
          onCommit={finishPendingTextBox}
          onOutsidePointerDown={(text) => finishPendingTextBox(text)}
          onCancel={() => finishPendingTextBox(pendingTextBox.text)}
        />
      ) : null}
      {editingMarkup && textEdit ? (
        <TextBoxEditor
          markup={editingMarkup}
          text={textEdit.text}
          transform={transform}
          onChange={(text) => setTextEdit({ markupId: editingMarkup.id, text })}
          onCommit={(text) => commitTextEdit(editingMarkup.id, text)}
          onCancel={() => setTextEdit(null)}
        />
      ) : null}
    </svg>
  );
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
  interactionState,
  editingText,
  onPointerDown,
}: {
  markup: Markup;
  definition: NonNullable<ReturnType<typeof getMarkupToolDefinition>>;
  transform: PageTransform;
  page: PageModel;
  pageScale?: PageScale;
  interactionState: InteractionState;
  editingText: string | null;
  onPointerDown: (event: React.PointerEvent<SVGElement>) => void;
}) {
  if (!shouldRenderMarkupAtZoom(markup, transform)) {
    return null;
  }

  const isInteractive = interactionState !== 'idle';
  const primitives = (definition.render?.getContentPrimitives(markup as never, { page, pageScale, phase: interactionState }) ?? [])
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
      <g onPointerDown={onPointerDown}>
        {primitives.map((primitive, index) => (
          <RenderPrimitiveElement key={index} primitive={primitive} transform={transform} />
        ))}
      </g>
      {chrome ? <SelectionChrome chrome={chrome} transform={transform} state={interactionState} /> : null}
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
  const editRect = editableTextRect(markup);
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
    if (markup.kind !== 'text-box') {
      return;
    }

    const finishOnOutsidePointerDown = (event: PointerEvent) => {
      const editor = editorRef.current;
      if (!editor || editor.contains(event.target as Node)) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      if (onOutsidePointerDown) {
        onOutsidePointerDown(editor.value, event);
      } else {
        onCommit(editor.value);
      }
    };

    document.addEventListener('pointerdown', finishOnOutsidePointerDown, true);
    return () => document.removeEventListener('pointerdown', finishOnOutsidePointerDown, true);
  }, [markup.kind, onCommit, onOutsidePointerDown]);

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
  const rect = editableTextRect(markup);
  const style = getAnnotationTextContentStyle(markup, markup.kind === 'dimension' ? 13 / 12 : 14.3146 / 12);
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

function editableTextRect(markup: EditableTextMarkup) {
  if (markup.kind === 'dimension') {
    return dimensionCaptionRect(markup);
  }

  return markup.kind === 'callout' || markup.kind === 'cloud-plus' ? markup.textBox : markup.rect;
}

function SelectionChrome({
  chrome,
  transform,
  state,
}: {
  chrome: SelectionChromeDescriptor;
  transform: PageTransform;
  state: InteractionState;
}) {
  if (!chrome.bounds) {
    return null;
  }

  const box = transform.pdfRectToViewport(chrome.bounds.rect);
  const style = getChromeStyle(state, chrome.bounds.kind as ChromeBoundsKind);
  const chromeBox = expandViewportRect(box, style.boundsOutsetPx);
  const handleSize = style.handleSize;
  const handleOffset = handleSize * 0.5;
  const center = {
    x: box.x + box.width * 0.5,
    y: box.y + box.height * 0.5,
  };
  const chromeTransform = chrome.bounds.rotation
    ? `rotate(${chrome.bounds.rotation} ${center.x} ${center.y})`
    : undefined;
  const handles = (chrome.handles ?? []).filter((handle) => {
    return isHandleVisibleForBounds(handle, chrome);
  });
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
            const point = projectChromeHandlePoint(transform.pdfToViewport(handle.point), box, chromeBox);
            const radius = Math.max(4, handleSize * 0.55);
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
          const point = shouldProjectHandleToChromeBounds(handle, chrome)
            ? projectChromeHandlePoint(transform.pdfToViewport(handle.point), box, chromeBox)
            : transform.pdfToViewport(handle.point);
          if (handle.behavior === 'rotateSelf') {
            const radius = Math.max(4, handleSize * 0.55);
            return (
              <circle
                key={handle.id}
                cx={point.x}
                cy={point.y}
                r={radius}
                fill={style.handleFill}
                stroke={style.boundsStroke}
                strokeWidth={1.25}
              />
            );
          }

          return (
            <rect
              key={handle.id}
              x={point.x - handleOffset}
              y={point.y - handleOffset}
              width={handleSize}
              height={handleSize}
              fill={style.handleFill}
              stroke={style.handleStroke}
              strokeWidth={1}
            />
          );
        })}
      </g>
    </g>
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
  const radius = 5;
  const role = result.candidate.role;

  return (
    <g className="pointer-events-none" data-testid="snap-indicator">
      {role === 'midpoint' ? (
        <path
          d={`M ${point.x} ${point.y - radius - 1} L ${point.x + radius + 1} ${point.y + radius} L ${point.x - radius - 1} ${point.y + radius} Z`}
          fill="none"
          stroke="#2563eb"
          strokeWidth={1.5}
        />
      ) : role === 'center' ? (
        <circle cx={point.x} cy={point.y} r={radius} fill="none" stroke="#2563eb" strokeWidth={1.5} />
      ) : role === 'intersection' ? (
        <>
          <line x1={point.x - radius} y1={point.y - radius} x2={point.x + radius} y2={point.y + radius} stroke="#2563eb" strokeWidth={1.5} />
          <line x1={point.x + radius} y1={point.y - radius} x2={point.x - radius} y2={point.y + radius} stroke="#2563eb" strokeWidth={1.5} />
        </>
      ) : role === 'edge' || role === 'bounds' ? (
        <rect
          x={point.x - radius}
          y={point.y - radius}
          width={radius * 2}
          height={radius * 2}
          transform={`rotate(45 ${point.x} ${point.y})`}
          fill="none"
          stroke="#2563eb"
          strokeWidth={1.5}
        />
      ) : (
        <rect
          x={point.x - radius}
          y={point.y - radius}
          width={radius * 2}
          height={radius * 2}
          fill="none"
          stroke="#2563eb"
          strokeWidth={1.5}
        />
      )}
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

function getInteractionState(
  markupId: string,
  selectedMarkupIds: ReadonlySet<string>,
  primaryMarkupId: string | undefined,
  hoveredMarkupId: string | null,
): InteractionState {
  if (selectedMarkupIds.has(markupId)) {
    return primaryMarkupId === markupId ? 'focused' : 'selected';
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

function isMeasurementTool(tool: ToolMode): tool is Extract<ToolMode, 'length' | 'polylength' | 'area'> {
  return tool === 'length' || tool === 'polylength' || tool === 'area';
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
    const handlePoint = shouldProjectHandleToChromeBounds(handle, chrome)
      ? projectChromeHandlePoint(transform.pdfToViewport(handle.point), box, chromeBox)
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

function isHandleVisibleForBounds(handle: ToolHandleDescriptor, chrome: SelectionChromeDescriptor): boolean {
  if (handle.behavior === 'resizeSelf') {
    return chrome.bounds?.kind === 'group' || Boolean(chrome.bounds?.canResize);
  }
  if (handle.behavior === 'rotateSelf') {
    return Boolean(chrome.bounds?.canRotate);
  }
  return true;
}

function shouldProjectHandleToChromeBounds(handle: ToolHandleDescriptor, chrome: SelectionChromeDescriptor): boolean {
  return chrome.bounds?.kind !== 'group' && (handle.behavior === 'resizeSelf' || handle.behavior === 'rotateSelf');
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

function moveDraftMarkups(document: DocumentModel, draft: Extract<AnnotationDraft, { kind: 'move' }>, delta: PdfPoint): DocumentModel {
  return {
    ...document,
    markups: document.markups.map((markup) => {
      if (!draft.markupIds.includes(markup.id)) {
        return markup;
      }

      const definition = draft.componentId && draft.bodyDrag ? getMarkupToolDefinition(markup) : null;
      const dragged = definition?.interaction?.dragMarkup?.(markup as never, {
        componentId: draft.componentId ?? '',
        bodyDrag: draft.bodyDrag ?? 'moveSelf',
        delta,
      });
      return dragged ?? translateMarkup(markup, delta);
    }),
  };
}

function distance(a: { readonly x: number; readonly y: number }, b: { readonly x: number; readonly y: number }): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function imagePlacementRect(point: PdfPoint, page: PageModel, asset: PendingImageAsset) {
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
