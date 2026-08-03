import { useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';
import { SIDEBAR_RESIZE_FORGIVING_ZONE } from './scrollbarSizing';

interface SidebarResizeHandleProps {
  side: 'left' | 'right';
  width: number;
  minWidth: number;
  maxWidth: number;
  defaultWidth: number;
  label: string;
  testId: string;
  step?: number;
  onWidthChange: (width: number) => void;
}

interface DragState {
  pointerId: number;
  startX: number;
  startWidth: number;
}

function clampWidth(width: number, minWidth: number, maxWidth: number): number {
  return Math.min(maxWidth, Math.max(minWidth, Math.round(width)));
}

export function SidebarResizeHandle({
  side,
  width,
  minWidth,
  maxWidth,
  defaultWidth,
  label,
  testId,
  step = 16,
  onWidthChange,
}: SidebarResizeHandleProps) {
  const dragStateRef = useRef<DragState | null>(null);
  const [dragging, setDragging] = useState(false);

  function applyWidth(nextWidth: number): void {
    onWidthChange(clampWidth(nextWidth, minWidth, maxWidth));
  }

  function handlePointerDown(event: PointerEvent<HTMLDivElement>): void {
    if (event.button !== 0) {
      return;
    }

    dragStateRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: width,
    };
    setDragging(true);
    event.currentTarget.setPointerCapture(event.pointerId);
    event.preventDefault();
  }

  function handlePointerMove(event: PointerEvent<HTMLDivElement>): void {
    const dragState = dragStateRef.current;
    if (!dragState || dragState.pointerId !== event.pointerId) {
      return;
    }

    const deltaX = event.clientX - dragState.startX;
    const nextWidth = side === 'left' ? dragState.startWidth + deltaX : dragState.startWidth - deltaX;
    applyWidth(nextWidth);
  }

  function finishDrag(event: PointerEvent<HTMLDivElement>): void {
    const dragState = dragStateRef.current;
    if (!dragState || dragState.pointerId !== event.pointerId) {
      return;
    }

    dragStateRef.current = null;
    setDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
    if (event.key === 'ArrowLeft') {
      event.preventDefault();
      applyWidth(width - step);
      return;
    }

    if (event.key === 'ArrowRight') {
      event.preventDefault();
      applyWidth(width + step);
      return;
    }

    if (event.key === 'Home') {
      event.preventDefault();
      applyWidth(minWidth);
      return;
    }

    if (event.key === 'End') {
      event.preventDefault();
      applyWidth(maxWidth);
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      applyWidth(defaultWidth);
    }
  }

  return (
    <div
      role="separator"
      tabIndex={0}
      aria-label={`${label} width`}
      aria-orientation="vertical"
      aria-valuemin={minWidth}
      aria-valuemax={maxWidth}
      aria-valuenow={width}
      data-testid={testId}
      className={[
        'absolute inset-y-0 z-20 cursor-col-resize touch-none outline-none',
        side === 'left' ? 'right-0' : 'left-0',
      ].join(' ')}
      style={
        side === 'left'
          ? { width: `${SIDEBAR_RESIZE_FORGIVING_ZONE}px`, transform: `translateX(${SIDEBAR_RESIZE_FORGIVING_ZONE}px)` }
          : { width: `${SIDEBAR_RESIZE_FORGIVING_ZONE}px` }
      }
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={finishDrag}
      onPointerCancel={finishDrag}
      onDoubleClick={() => applyWidth(defaultWidth)}
      onKeyDown={handleKeyDown}
    >
      <span
        aria-hidden="true"
        className={[
          'bp-resize-handle-line absolute inset-y-0 left-0 h-full w-px rounded-full transition',
          dragging ? 'opacity-100' : 'opacity-0 hover:opacity-100 focus-within:opacity-100',
        ].join(' ')}
      />
    </div>
  );
}
