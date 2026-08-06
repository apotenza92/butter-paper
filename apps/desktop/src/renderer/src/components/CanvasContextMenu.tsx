import type { PageRotationDirection } from '@butter-paper/core';
import type { ReactNode } from 'react';
import {
  Expand,
  Hand,
  MousePointer2,
  MoveHorizontal,
  RotateCcw,
  RotateCw,
  ScanLine,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';

interface CanvasContextMenuProps {
  children: ReactNode;
  disabled?: boolean;
  mutationDisabled?: boolean;
  pageIndex: number;
  onSelectTool: () => void;
  onPanTool: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFitWidth: () => void;
  onFitPage: () => void;
  onSetPageScale: (pageIndex: number) => void;
  onRotatePage: (pageIndex: number, direction: PageRotationDirection) => void;
}

export function CanvasContextMenu({
  children,
  disabled,
  mutationDisabled = false,
  pageIndex,
  onSelectTool,
  onPanTool,
  onZoomIn,
  onZoomOut,
  onFitWidth,
  onFitPage,
  onSetPageScale,
  onRotatePage,
}: CanvasContextMenuProps) {
  if (disabled) {
    return children;
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger className="block h-full min-h-0" data-testid="canvas-context-menu-trigger">
        {children}
      </ContextMenuTrigger>
      <ContextMenuContent className="w-max min-w-48 whitespace-nowrap" data-testid="canvas-context-menu">
        <ContextMenuGroup>
          <ContextMenuItem data-testid="canvas-select-tool" onClick={onSelectTool}>
            <MousePointer2 aria-hidden="true" />
            Select
          </ContextMenuItem>
          <ContextMenuItem data-testid="canvas-pan-tool" onClick={onPanTool}>
            <Hand aria-hidden="true" />
            Pan
          </ContextMenuItem>
        </ContextMenuGroup>
        <ContextMenuSeparator />
        <ContextMenuGroup>
          <ContextMenuItem data-testid="canvas-zoom-in" onClick={onZoomIn}>
            <ZoomIn aria-hidden="true" />
            Zoom in
          </ContextMenuItem>
          <ContextMenuItem onClick={onZoomOut}>
            <ZoomOut aria-hidden="true" />
            Zoom out
          </ContextMenuItem>
          <ContextMenuItem onClick={onFitWidth}>
            <MoveHorizontal aria-hidden="true" />
            Fit width
          </ContextMenuItem>
          <ContextMenuItem onClick={onFitPage}>
            <Expand aria-hidden="true" />
            Fit page
          </ContextMenuItem>
        </ContextMenuGroup>
        <ContextMenuSeparator />
        <ContextMenuGroup>
          <ContextMenuLabel>Page {pageIndex + 1}</ContextMenuLabel>
          <ContextMenuItem
            data-testid="canvas-set-page-scale"
            disabled={mutationDisabled}
            onClick={mutationDisabled ? undefined : () => onSetPageScale(pageIndex)}
          >
            <ScanLine aria-hidden="true" />
            Set page scale…
          </ContextMenuItem>
          <ContextMenuItem
            data-testid="canvas-rotate-left"
            disabled={mutationDisabled}
            onClick={mutationDisabled ? undefined : () => onRotatePage(pageIndex, 'left')}
          >
            <RotateCcw aria-hidden="true" />
            Rotate left
          </ContextMenuItem>
          <ContextMenuItem
            data-testid="canvas-rotate-right"
            disabled={mutationDisabled}
            onClick={mutationDisabled ? undefined : () => onRotatePage(pageIndex, 'right')}
          >
            <RotateCw aria-hidden="true" />
            Rotate right
          </ContextMenuItem>
        </ContextMenuGroup>
      </ContextMenuContent>
    </ContextMenu>
  );
}
