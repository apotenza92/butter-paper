import { useEffect, useMemo, useRef, useState } from 'react';
import type {
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
  WheelEvent as ReactWheelEvent,
} from 'react';
import {
  createEllipseMarkup,
  createLineMarkup,
  createPenMarkup,
  createRectangleMarkup,
  pdfPoint,
  type ButterCanvasAsset,
  type ButterCanvasDocument,
  type ButterCanvasTraceZone,
  type Markup,
  type PdfPoint,
  type Rect,
} from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';

interface ButterCanvasViewportProps {
  document: ButterCanvasDocument;
  activeTool: ToolMode;
  selectedAssetId: string | null;
  selectedMarkupId: string | null;
  fitRequest: number;
  tracePreviewMarkups?: readonly Markup[];
  tracePreviewZone?: ButterCanvasTraceZone | null;
  onDocumentChange: (document: ButterCanvasDocument) => void;
  onSelectedAssetChange: (assetId: string | null) => void;
  onSelectedMarkupChange: (markupId: string | null) => void;
  onOpenDocument: () => void;
}

type DraftMarkup =
  | { kind: 'line'; start: PdfPoint; end: PdfPoint }
  | { kind: 'rectangle' | 'ellipse'; start: PdfPoint; end: PdfPoint }
  | { kind: 'pen'; points: PdfPoint[] };

interface PanState {
  readonly pointerId: number;
  readonly startClientX: number;
  readonly startClientY: number;
  readonly startCameraX: number;
  readonly startCameraY: number;
}

interface DrawState {
  readonly pointerId: number;
  readonly draft: DraftMarkup;
}

interface AssetDragState {
  readonly pointerId: number;
  readonly assetId: string;
  readonly mode: 'move' | 'resize';
  readonly startClientX: number;
  readonly startClientY: number;
  readonly startRect: ButterCanvasAsset['rect'];
}

interface MarkupDragState {
  readonly pointerId: number;
  readonly markupId: string;
  readonly startClientX: number;
  readonly startClientY: number;
  readonly startMarkup: Markup;
}

const CANVAS_BACKGROUND = 'var(--bp-surface-app)';
const GRID_COLOUR = 'color-mix(in srgb, var(--bp-text-primary) 12%, transparent)';
const MAJOR_GRID_COLOUR = 'color-mix(in srgb, var(--bp-text-primary) 20%, transparent)';

function nextId(prefix: string): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function clampZoom(zoom: number): number {
  return Math.min(64, Math.max(0.05, zoom));
}

function worldToScreen(point: PdfPoint, document: ButterCanvasDocument): { x: number; y: number } {
  return {
    x: point.x * document.camera.zoom + document.camera.x,
    y: point.y * document.camera.zoom + document.camera.y,
  };
}

function screenToWorld(clientX: number, clientY: number, rect: DOMRect, document: ButterCanvasDocument): PdfPoint {
  return pdfPoint(
    (clientX - rect.left - document.camera.x) / document.camera.zoom,
    (clientY - rect.top - document.camera.y) / document.camera.zoom,
  );
}

function snapPoint(point: PdfPoint, document: ButterCanvasDocument): PdfPoint {
  if (!document.snap.enabled) {
    return point;
  }
  let nextPoint = point;
  if (document.grid.snap && document.grid.size > 0) {
    const size = document.grid.size;
    nextPoint = pdfPoint(
      Math.round(point.x / size) * size,
      Math.round(point.y / size) * size,
    );
  }
  const markupSnap = nearestMarkupSnapPoint(nextPoint, document);
  return markupSnap ?? nextPoint;
}

function createMarkupFromDraft(draft: DraftMarkup): Markup {
  switch (draft.kind) {
    case 'line':
      return createLineMarkup({
        id: nextId('canvas-line'),
        pageIndex: 0,
        start: draft.start,
        end: draft.end,
      });
    case 'rectangle':
      return createRectangleMarkup({
        id: nextId('canvas-rectangle'),
        pageIndex: 0,
        rect: {
          x: draft.start.x,
          y: draft.start.y,
          width: draft.end.x - draft.start.x,
          height: draft.end.y - draft.start.y,
        },
      });
    case 'ellipse':
      return createEllipseMarkup({
        id: nextId('canvas-ellipse'),
        pageIndex: 0,
        rect: {
          x: draft.start.x,
          y: draft.start.y,
          width: draft.end.x - draft.start.x,
          height: draft.end.y - draft.start.y,
        },
      });
    case 'pen':
      return createPenMarkup({
        id: nextId('canvas-pen'),
        pageIndex: 0,
        paths: [draft.points],
      });
  }
}

function renderMarkupPath(markup: Markup): string | null {
  switch (markup.kind) {
    case 'line':
    case 'arrow':
    case 'length':
    case 'dimension':
      return `M ${markup.start.x} ${markup.start.y} L ${markup.end.x} ${markup.end.y}`;
    case 'polyline':
    case 'polylength':
      return markup.points.map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`).join(' ');
    case 'polygon':
    case 'area':
      return `${markup.points.map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`).join(' ')} Z`;
    case 'pen':
    case 'highlight':
      return markup.paths
        .map((path) => path.map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`).join(' '))
        .join(' ');
    case 'cloud':
      return markup.controlPath.map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`).join(' ');
    default:
      return null;
  }
}

function renderDraftPath(draft: DraftMarkup): string {
  if (draft.kind === 'pen') {
    return draft.points.map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`).join(' ');
  }
  return `M ${draft.start.x} ${draft.start.y} L ${draft.end.x} ${draft.end.y}`;
}

function shouldDrawWithTool(tool: ToolMode): boolean {
  return tool === 'line' || tool === 'arrow' || tool === 'length' || tool === 'dimension' || tool === 'rectangle' || tool === 'ellipse' || tool === 'pen' || tool === 'highlight';
}

function draftKindForTool(tool: ToolMode): DraftMarkup['kind'] {
  if (tool === 'rectangle') {
    return 'rectangle';
  }
  if (tool === 'ellipse') {
    return 'ellipse';
  }
  if (tool === 'pen' || tool === 'highlight') {
    return 'pen';
  }
  return 'line';
}

function nearestMarkupSnapPoint(point: PdfPoint, document: ButterCanvasDocument): PdfPoint | null {
  const candidates = canvasSnapCandidates(document);
  const tolerance = document.snap.sensitivityPx / Math.max(0.1, document.camera.zoom);
  let best: { point: PdfPoint; distance: number } | null = null;
  for (const candidate of candidates) {
    const distance = Math.hypot(candidate.x - point.x, candidate.y - point.y);
    if (distance > tolerance || (best && distance >= best.distance)) {
      continue;
    }
    best = { point: candidate, distance };
  }
  return best?.point ?? null;
}

function canvasSnapCandidates(document: ButterCanvasDocument): PdfPoint[] {
  const points: PdfPoint[] = [];
  for (const markup of document.markups) {
    if (document.snap.endpoints) {
      points.push(...markupEndpointCandidates(markup));
    }
    if (document.snap.midpoints) {
      points.push(...markupMidpointCandidates(markup));
    }
    if (document.snap.centers) {
      const bounds = markupBounds(markup);
      if (bounds) {
        points.push(pdfPoint(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2));
      }
    }
  }
  if (document.snap.centers) {
    for (const asset of document.assets) {
      points.push(pdfPoint(asset.rect.x + asset.rect.width / 2, asset.rect.y + asset.rect.height / 2));
    }
  }
  return points;
}

function markupEndpointCandidates(markup: Markup): PdfPoint[] {
  if ('start' in markup && 'end' in markup && markup.start && markup.end) {
    return [markup.start, markup.end];
  }
  if ('points' in markup) {
    const first = markup.points[0];
    const last = markup.points.at(-1);
    return first && last ? [first, last] : [];
  }
  if ('paths' in markup) {
    return markup.paths.flatMap((path) => {
      const first = path[0];
      const last = path.at(-1);
      return first && last ? [first, last] : [];
    });
  }
  if ('controlPath' in markup) {
    const first = markup.controlPath[0];
    const last = markup.controlPath.at(-1);
    return first && last ? [first, last] : [];
  }
  if ('rect' in markup) {
    const { x, y, width, height } = markup.rect;
    return [
      pdfPoint(x, y),
      pdfPoint(x + width, y),
      pdfPoint(x, y + height),
      pdfPoint(x + width, y + height),
    ];
  }
  return [];
}

function markupMidpointCandidates(markup: Markup): PdfPoint[] {
  if ('start' in markup && 'end' in markup && markup.start && markup.end) {
    return [midpoint(markup.start, markup.end)];
  }
  if ('points' in markup) {
    return adjacentMidpoints(markup.points);
  }
  if ('paths' in markup) {
    return markup.paths.flatMap(adjacentMidpoints);
  }
  if ('controlPath' in markup) {
    return adjacentMidpoints(markup.controlPath);
  }
  if ('rect' in markup) {
    const { x, y, width, height } = markup.rect;
    return [
      pdfPoint(x + width / 2, y),
      pdfPoint(x + width, y + height / 2),
      pdfPoint(x + width / 2, y + height),
      pdfPoint(x, y + height / 2),
    ];
  }
  return [];
}

function adjacentMidpoints(points: readonly PdfPoint[]): PdfPoint[] {
  const midpoints: PdfPoint[] = [];
  for (let index = 1; index < points.length; index += 1) {
    midpoints.push(midpoint(points[index - 1], points[index]));
  }
  return midpoints;
}

function midpoint(start: PdfPoint, end: PdfPoint): PdfPoint {
  return pdfPoint((start.x + end.x) / 2, (start.y + end.y) / 2);
}

export function ButterCanvasViewport({
  document,
  activeTool,
  selectedAssetId,
  selectedMarkupId,
  fitRequest,
  tracePreviewMarkups = [],
  tracePreviewZone = null,
  onDocumentChange,
  onSelectedAssetChange,
  onSelectedMarkupChange,
  onOpenDocument,
}: ButterCanvasViewportProps) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [panState, setPanState] = useState<PanState | null>(null);
  const [drawState, setDrawState] = useState<DrawState | null>(null);
  const [assetDragState, setAssetDragState] = useState<AssetDragState | null>(null);
  const [markupDragState, setMarkupDragState] = useState<MarkupDragState | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });

  useEffect(() => {
    const root = rootRef.current;
    if (!root) {
      return undefined;
    }
    const observer = new ResizeObserver(([entry]) => {
      setSize({
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      });
    });
    observer.observe(root);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (fitRequest <= 0 || size.width <= 0 || size.height <= 0) {
      return;
    }
    const bounds = fitTargetBounds(document, selectedAssetId, selectedMarkupId);
    if (!bounds) {
      updateCamera({ x: 0, y: 0, zoom: 1 });
      return;
    }
    const padding = 80;
    const zoom = clampZoom(Math.min(
      (size.width - padding * 2) / Math.max(1, bounds.width),
      (size.height - padding * 2) / Math.max(1, bounds.height),
    ));
    updateCamera({
      x: size.width / 2 - (bounds.x + bounds.width / 2) * zoom,
      y: size.height / 2 - (bounds.y + bounds.height / 2) * zoom,
      zoom,
    });
  }, [fitRequest]);

  const gridBackground = useMemo(() => {
    if (!document.grid.visible) {
      return undefined;
    }
    const grid = Math.max(4, document.grid.size * document.camera.zoom);
    const major = grid * 5;
    const x = document.camera.x % grid;
    const y = document.camera.y % grid;
    const majorX = document.camera.x % major;
    const majorY = document.camera.y % major;
    if (document.grid.pattern === 'dots') {
      return {
        backgroundImage: `radial-gradient(circle, ${GRID_COLOUR} 1px, transparent 1px)`,
        backgroundSize: `${grid}px ${grid}px`,
        backgroundPosition: `${x}px ${y}px`,
      };
    }
    return {
      backgroundImage: [
        `linear-gradient(to right, ${GRID_COLOUR} 1px, transparent 1px)`,
        `linear-gradient(to bottom, ${GRID_COLOUR} 1px, transparent 1px)`,
        `linear-gradient(to right, ${MAJOR_GRID_COLOUR} 1px, transparent 1px)`,
        `linear-gradient(to bottom, ${MAJOR_GRID_COLOUR} 1px, transparent 1px)`,
      ].join(', '),
      backgroundSize: `${grid}px ${grid}px, ${grid}px ${grid}px, ${major}px ${major}px, ${major}px ${major}px`,
      backgroundPosition: `${x}px ${y}px, ${x}px ${y}px, ${majorX}px ${majorY}px, ${majorX}px ${majorY}px`,
    };
  }, [document.camera.x, document.camera.y, document.camera.zoom, document.grid.pattern, document.grid.size, document.grid.visible]);

  function updateCamera(camera: ButterCanvasDocument['camera']): void {
    onDocumentChange({
      ...document,
      updatedAt: new Date().toISOString(),
      camera,
    });
  }

  function updateAsset(assetId: string, updater: (asset: ButterCanvasAsset) => ButterCanvasAsset): void {
    onDocumentChange({
      ...document,
      updatedAt: new Date().toISOString(),
      assets: document.assets.map((asset) => asset.id === assetId ? updater(asset) : asset),
    });
  }

  function updateMarkup(markupId: string, updater: (markup: Markup) => Markup): void {
    onDocumentChange({
      ...document,
      updatedAt: new Date().toISOString(),
      markups: document.markups.map((markup) => markup.id === markupId ? updater(markup) : markup),
    });
  }

  function handleAssetPointerDown(
    event: ReactPointerEvent<HTMLDivElement>,
    asset: ButterCanvasAsset,
    mode: AssetDragState['mode'],
  ): void {
    if (activeTool !== 'select' || asset.locked) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    onSelectedAssetChange(asset.id);
    setAssetDragState({
      pointerId: event.pointerId,
      assetId: asset.id,
      mode,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startRect: asset.rect,
    });
  }

  function handleWheel(event: ReactWheelEvent<HTMLDivElement>): void {
    event.preventDefault();
    const rect = event.currentTarget.getBoundingClientRect();
    const before = screenToWorld(event.clientX, event.clientY, rect, document);
    const zoomFactor = Math.exp(-event.deltaY * 0.001);
    const zoom = clampZoom(document.camera.zoom * zoomFactor);
    updateCamera({
      x: event.clientX - rect.left - before.x * zoom,
      y: event.clientY - rect.top - before.y * zoom,
      zoom,
    });
  }

  function handlePointerDown(event: ReactPointerEvent<HTMLDivElement>): void {
    if (!(event.target instanceof Element) || !rootRef.current) {
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    if (event.target === event.currentTarget) {
      onSelectedAssetChange(null);
      onSelectedMarkupChange(null);
    }
    if (activeTool === 'select' && rootRef.current) {
      const rect = rootRef.current.getBoundingClientRect();
      const point = screenToWorld(event.clientX, event.clientY, rect, document);
      const markup = hitTestMarkup(point, document.markups, 8 / document.camera.zoom);
      if (markup) {
        onSelectedAssetChange(null);
        onSelectedMarkupChange(markup.id);
        setMarkupDragState({
          pointerId: event.pointerId,
          markupId: markup.id,
          startClientX: event.clientX,
          startClientY: event.clientY,
          startMarkup: markup,
        });
        return;
      }
    }
    const shouldPan = event.button === 1 || activeTool === 'pan' || !shouldDrawWithTool(activeTool);
    if (shouldPan) {
      setPanState({
        pointerId: event.pointerId,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startCameraX: document.camera.x,
        startCameraY: document.camera.y,
      });
      return;
    }

    const rect = rootRef.current.getBoundingClientRect();
    const start = snapPoint(screenToWorld(event.clientX, event.clientY, rect, document), document);
    const kind = draftKindForTool(activeTool);
    setDrawState({
      pointerId: event.pointerId,
      draft: kind === 'pen'
        ? { kind, points: [start] }
        : { kind, start, end: start },
    });
  }

  function handlePointerMove(event: ReactPointerEvent<HTMLDivElement>): void {
    if (markupDragState?.pointerId === event.pointerId) {
      const delta = pdfPoint(
        (event.clientX - markupDragState.startClientX) / document.camera.zoom,
        (event.clientY - markupDragState.startClientY) / document.camera.zoom,
      );
      updateMarkup(markupDragState.markupId, () => translateMarkup(markupDragState.startMarkup, delta));
      return;
    }

    if (assetDragState?.pointerId === event.pointerId) {
      const deltaX = (event.clientX - assetDragState.startClientX) / document.camera.zoom;
      const deltaY = (event.clientY - assetDragState.startClientY) / document.camera.zoom;
      updateAsset(assetDragState.assetId, (asset) => {
        if (assetDragState.mode === 'resize') {
          return {
            ...asset,
            rect: {
              ...asset.rect,
              width: Math.max(24, snapPoint(pdfPoint(assetDragState.startRect.width + deltaX, 0), document).x),
              height: Math.max(24, snapPoint(pdfPoint(0, assetDragState.startRect.height + deltaY), document).y),
            },
          };
        }
        const next = snapPoint(pdfPoint(assetDragState.startRect.x + deltaX, assetDragState.startRect.y + deltaY), document);
        return {
          ...asset,
          rect: {
            ...asset.rect,
            x: next.x,
            y: next.y,
          },
        };
      });
      return;
    }

    if (panState?.pointerId === event.pointerId) {
      updateCamera({
        ...document.camera,
        x: panState.startCameraX + event.clientX - panState.startClientX,
        y: panState.startCameraY + event.clientY - panState.startClientY,
      });
      return;
    }

    if (drawState?.pointerId === event.pointerId && rootRef.current) {
      const rect = rootRef.current.getBoundingClientRect();
      const point = snapPoint(screenToWorld(event.clientX, event.clientY, rect, document), document);
      setDrawState({
        ...drawState,
        draft: drawState.draft.kind === 'pen'
          ? { ...drawState.draft, points: [...drawState.draft.points, point] }
          : { ...drawState.draft, end: point },
      });
    }
  }

  function handlePointerUp(event: ReactPointerEvent<HTMLDivElement>): void {
    if (markupDragState?.pointerId === event.pointerId) {
      setMarkupDragState(null);
    }

    if (assetDragState?.pointerId === event.pointerId) {
      setAssetDragState(null);
    }

    if (panState?.pointerId === event.pointerId) {
      setPanState(null);
    }

    if (drawState?.pointerId === event.pointerId) {
      const markup = createMarkupFromDraft(drawState.draft);
      onDocumentChange({
        ...document,
        updatedAt: new Date().toISOString(),
        markups: [...document.markups, markup],
      });
      setDrawState(null);
    }
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLDivElement>): void {
    if ((event.key === 'Backspace' || event.key === 'Delete') && selectedMarkupId) {
      event.preventDefault();
      onDocumentChange({
        ...document,
        updatedAt: new Date().toISOString(),
        markups: document.markups.filter((markup) => markup.id !== selectedMarkupId),
      });
      onSelectedMarkupChange(null);
      return;
    }

    if ((event.key === 'Backspace' || event.key === 'Delete') && selectedAssetId) {
      event.preventDefault();
      onDocumentChange({
        ...document,
        updatedAt: new Date().toISOString(),
        assets: document.assets.filter((asset) => asset.id !== selectedAssetId),
      });
      onSelectedAssetChange(null);
    }
  }

  const worldTransform = `translate(${document.camera.x} ${document.camera.y}) scale(${document.camera.zoom})`;
  const selectedAsset = selectedAssetId ? document.assets.find((asset) => asset.id === selectedAssetId) ?? null : null;
  const traceZoneRect = selectedAsset && tracePreviewZone ? assetTraceZoneRect(selectedAsset, tracePreviewZone) : null;

  return (
    <div
      ref={rootRef}
      className="relative h-full w-full overflow-hidden outline-none"
      data-testid="butter-canvas-viewport"
      style={{ backgroundColor: CANVAS_BACKGROUND, ...gridBackground }}
      tabIndex={0}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onWheel={handleWheel}
      onKeyDown={handleKeyDown}
    >
      {document.assets.map((asset) => (
        <CanvasAssetView
          key={asset.id}
          asset={asset}
          document={document}
          selected={asset.id === selectedAssetId}
          onPointerDown={handleAssetPointerDown}
        />
      ))}
      <svg
        className="pointer-events-none absolute inset-0 h-full w-full"
        width={size.width}
        height={size.height}
        viewBox={`0 0 ${Math.max(size.width, 1)} ${Math.max(size.height, 1)}`}
      >
        <g transform={worldTransform}>
          {document.markups.map((markup) => (
            <MarkupView key={markup.id} markup={markup} selected={markup.id === selectedMarkupId} />
          ))}
          {traceZoneRect ? <TraceZoneView rect={traceZoneRect} /> : null}
          {tracePreviewMarkups.map((markup) => (
            <MarkupView key={markup.id} markup={markup} preview />
          ))}
          {drawState ? <DraftView draft={drawState.draft} /> : null}
        </g>
      </svg>
      {document.assets.length === 0 && document.markups.length === 0 ? (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <button
            type="button"
            className="pointer-events-auto rounded-[6px] border bg-white/85 px-3 py-2 text-[12px] font-medium shadow-sm"
            onClick={onOpenDocument}
          >
            Insert PDF or Image
          </button>
        </div>
      ) : null}
    </div>
  );
}

function assetTraceZoneRect(asset: ButterCanvasAsset, zone: ButterCanvasTraceZone): Rect {
  return {
    x: asset.rect.x + asset.rect.width * zone.x,
    y: asset.rect.y + asset.rect.height * zone.y,
    width: asset.rect.width * zone.width,
    height: asset.rect.height * zone.height,
  };
}

function TraceZoneView({ rect }: { rect: Rect }) {
  return (
    <rect
      x={rect.x}
      y={rect.y}
      width={rect.width}
      height={rect.height}
      fill="rgba(37, 99, 235, 0.08)"
      stroke="#2563eb"
      strokeDasharray="8 6"
      strokeWidth={2}
      vectorEffect="non-scaling-stroke"
      data-testid="butter-canvas-trace-zone-preview"
    />
  );
}

function fitTargetBounds(
  document: ButterCanvasDocument,
  selectedAssetId: string | null,
  selectedMarkupId: string | null,
): Rect | null {
  const selectedAsset = selectedAssetId ? document.assets.find((asset) => asset.id === selectedAssetId) : null;
  if (selectedAsset) {
    return selectedAsset.rect;
  }
  const selectedMarkup = selectedMarkupId ? document.markups.find((markup) => markup.id === selectedMarkupId) : null;
  if (selectedMarkup) {
    return markupBounds(selectedMarkup);
  }
  return unionRects([
    ...document.assets.filter((asset) => asset.visible).map((asset) => asset.rect),
    ...document.markups.map(markupBounds).filter((rect): rect is Rect => rect !== null),
  ]);
}

function unionRects(rects: readonly Rect[]): Rect | null {
  if (rects.length === 0) {
    return null;
  }
  const minX = Math.min(...rects.map((rect) => rect.x));
  const minY = Math.min(...rects.map((rect) => rect.y));
  const maxX = Math.max(...rects.map((rect) => rect.x + rect.width));
  const maxY = Math.max(...rects.map((rect) => rect.y + rect.height));
  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY,
  };
}

function hitTestMarkup(point: PdfPoint, markups: readonly Markup[], tolerance: number): Markup | null {
  for (let index = markups.length - 1; index >= 0; index -= 1) {
    const markup = markups[index];
    const bounds = markupBounds(markup);
    if (!bounds) {
      continue;
    }
    if (
      point.x >= bounds.x - tolerance
      && point.x <= bounds.x + bounds.width + tolerance
      && point.y >= bounds.y - tolerance
      && point.y <= bounds.y + bounds.height + tolerance
    ) {
      return markup;
    }
  }
  return null;
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

function translateMarkup(markup: Markup, delta: PdfPoint): Markup {
  if ('rect' in markup) {
    return {
      ...markup,
      rect: {
        ...markup.rect,
        x: markup.rect.x + delta.x,
        y: markup.rect.y + delta.y,
      },
    } as Markup;
  }
  if ('start' in markup && 'end' in markup) {
    return {
      ...markup,
      start: translatePoint(markup.start, delta),
      end: translatePoint(markup.end, delta),
    } as Markup;
  }
  if ('points' in markup) {
    return {
      ...markup,
      points: markup.points.map((point) => translatePoint(point, delta)),
    } as Markup;
  }
  if ('paths' in markup) {
    return {
      ...markup,
      paths: markup.paths.map((path) => path.map((point) => translatePoint(point, delta))),
    } as Markup;
  }
  if ('controlPath' in markup) {
    return {
      ...markup,
      controlPath: markup.controlPath.map((point) => translatePoint(point, delta)),
    } as Markup;
  }
  return markup;
}

function translatePoint(point: PdfPoint, delta: PdfPoint): PdfPoint {
  return pdfPoint(point.x + delta.x, point.y + delta.y);
}

function CanvasAssetView({
  asset,
  document,
  selected,
  onPointerDown,
}: {
  asset: ButterCanvasAsset;
  document: ButterCanvasDocument;
  selected: boolean;
  onPointerDown: (
    event: ReactPointerEvent<HTMLDivElement>,
    asset: ButterCanvasAsset,
    mode: AssetDragState['mode'],
  ) => void;
}) {
  if (!asset.visible) {
    return null;
  }
  const topLeft = worldToScreen(pdfPoint(asset.rect.x, asset.rect.y), document);
  const width = asset.rect.width * document.camera.zoom;
  const height = asset.rect.height * document.camera.zoom;
  return (
    <div
      className="absolute select-none"
      data-testid={`butter-canvas-asset-${asset.id}`}
      style={{
        left: topLeft.x,
        top: topLeft.y,
        width,
        height,
        opacity: asset.opacity,
        transform: `rotate(${asset.rotation ?? 0}deg)`,
        transformOrigin: 'center',
        cursor: asset.locked ? 'default' : 'move',
      }}
      onPointerDown={(event) => onPointerDown(event, asset, 'move')}
    >
      <img
        alt={asset.name}
        className="h-full w-full"
        src={asset.dataUrl}
        draggable={false}
      />
      {selected ? (
        <>
          <div className="pointer-events-none absolute inset-0 border-2 border-blue-500" />
          {!asset.locked ? (
            <div
              aria-label="Resize asset"
              className="absolute bottom-[-5px] right-[-5px] h-3 w-3 cursor-se-resize rounded-[3px] border border-white bg-blue-500 shadow"
              data-testid={`butter-canvas-resize-${asset.id}`}
              role="button"
              tabIndex={-1}
              onPointerDown={(event) => onPointerDown(event, asset, 'resize')}
            />
          ) : null}
        </>
      ) : null}
    </div>
  );
}

function MarkupView({ markup, preview = false, selected = false }: { markup: Markup; preview?: boolean; selected?: boolean }) {
  const stroke = preview ? '#2563eb' : (markup.color ?? '#1f2937');
  const strokeDasharray = preview ? '6 5' : undefined;
  const opacity = preview ? 0.82 : undefined;
  const bounds = selected ? markupBounds(markup) : null;
  const selectionChrome = bounds ? <SelectionBounds rect={bounds} /> : null;
  if (markup.kind === 'rectangle' || markup.kind === 'text-box' || markup.kind === 'image' || markup.kind === 'snapshot' || markup.kind === 'imported-annotation') {
    return (
      <>
        <rect
          x={markup.rect.x}
          y={markup.rect.y}
          width={markup.rect.width}
          height={markup.rect.height}
          fill="none"
          stroke={stroke}
          strokeDasharray={strokeDasharray}
          strokeWidth={2}
          opacity={opacity}
          vectorEffect="non-scaling-stroke"
        />
        {selectionChrome}
      </>
    );
  }
  if (markup.kind === 'ellipse') {
    return (
      <>
        <ellipse
          cx={markup.rect.x + markup.rect.width / 2}
          cy={markup.rect.y + markup.rect.height / 2}
          rx={Math.abs(markup.rect.width / 2)}
          ry={Math.abs(markup.rect.height / 2)}
          fill="none"
          stroke={stroke}
          strokeDasharray={strokeDasharray}
          strokeWidth={2}
          opacity={opacity}
          vectorEffect="non-scaling-stroke"
        />
        {selectionChrome}
      </>
    );
  }
  const path = renderMarkupPath(markup);
  if (!path) {
    return null;
  }
  return (
    <>
      <path
        d={path}
        fill={markup.kind === 'polygon' || markup.kind === 'area' ? 'rgba(31, 41, 55, 0.08)' : 'none'}
        stroke={stroke}
        strokeDasharray={strokeDasharray}
        strokeWidth={markup.kind === 'highlight' ? 8 : 2}
        opacity={opacity}
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
      {selectionChrome}
    </>
  );
}

function SelectionBounds({ rect }: { rect: Rect }) {
  return (
    <rect
      x={rect.x}
      y={rect.y}
      width={rect.width}
      height={rect.height}
      fill="none"
      stroke="#2563eb"
      strokeDasharray="5 4"
      strokeWidth={2}
      vectorEffect="non-scaling-stroke"
      data-testid="butter-canvas-markup-selection"
    />
  );
}

function DraftView({ draft }: { draft: DraftMarkup }) {
  if (draft.kind === 'rectangle' || draft.kind === 'ellipse') {
    const x = Math.min(draft.start.x, draft.end.x);
    const y = Math.min(draft.start.y, draft.end.y);
    const width = Math.abs(draft.end.x - draft.start.x);
    const height = Math.abs(draft.end.y - draft.start.y);
    if (draft.kind === 'ellipse') {
      return (
        <ellipse
          cx={x + width / 2}
          cy={y + height / 2}
          rx={width / 2}
          ry={height / 2}
          fill="none"
          stroke="#2563eb"
          strokeDasharray="4 4"
          strokeWidth={2}
          vectorEffect="non-scaling-stroke"
        />
      );
    }
    return (
      <rect
        x={x}
        y={y}
        width={width}
        height={height}
        fill="none"
        stroke="#2563eb"
        strokeDasharray="4 4"
        strokeWidth={2}
        vectorEffect="non-scaling-stroke"
      />
    );
  }
  return (
    <path
      d={renderDraftPath(draft)}
      fill="none"
      stroke="#2563eb"
      strokeDasharray={draft.kind === 'line' ? '4 4' : undefined}
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      vectorEffect="non-scaling-stroke"
    />
  );
}
