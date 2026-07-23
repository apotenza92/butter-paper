import {
  FileImage,
  FileText,
  Grid2x2,
  Magnet,
  Redo2,
  Ruler,
  ScanLine,
  Undo2,
  ZoomIn,
  ZoomOut,
  type LucideIcon,
} from 'lucide-react';
import type { ReactElement, ReactNode } from 'react';
import type { ButterCanvasDocument } from '@butter-paper/core';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Toggle } from '@/components/ui/toggle';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import {
  CONTROL_ICON_SIZE,
  CONTROL_ICON_SIZE_CLASS,
  CONTROL_ICON_STROKE_WIDTH,
  PRIMARY_BAND_HEIGHT,
  SHELL_SURFACE_PANEL,
  VIEWER_TOOLBAR_INSET_X,
} from './shellSpacing';

interface ButterCanvasToolbarProps {
  document: ButterCanvasDocument;
  canUndo?: boolean;
  canRedo?: boolean;
  onDocumentChange: (document: ButterCanvasDocument) => void;
  onInsertImage: () => void;
  onInsertPdf: () => void;
  onTraceImage: () => void;
  onSetScale: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFit: () => void;
  onUndo: () => void;
  onRedo: () => void;
}

function ToolbarButton({
  active,
  disabled,
  label,
  children,
  onClick,
}: {
  active?: boolean;
  disabled?: boolean;
  label: string;
  children: ReactNode;
  onClick: () => void;
}) {
  const className = 'h-8 min-w-8 rounded-2xl px-2 text-[12px]';
  let control: ReactElement;

  if (active !== undefined) {
    control = (
      <Toggle
        type="button"
        variant="outline"
        pressed={active}
        aria-label={label}
        disabled={disabled}
        className={className}
        onPressedChange={onClick}
      >
        {children}
      </Toggle>
    );
  } else {
    control = (
      <Button
        type="button"
        variant="ghost"
        aria-label={label}
        disabled={disabled}
        className={className}
        onClick={onClick}
      >
        {children}
      </Button>
    );
  }

  if (disabled) {
    return control;
  }

  return (
    <Tooltip>
      <TooltipTrigger render={control} />
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

function ToolbarIcon({ icon: Icon }: { icon: LucideIcon }) {
  return (
    <Icon
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      className={CONTROL_ICON_SIZE_CLASS}
      aria-hidden="true"
    />
  );
}

export function ButterCanvasToolbar({
  document,
  canUndo = false,
  canRedo = false,
  onDocumentChange,
  onInsertImage,
  onInsertPdf,
  onTraceImage,
  onSetScale,
  onZoomIn,
  onZoomOut,
  onFit,
  onUndo,
  onRedo,
}: ButterCanvasToolbarProps) {
  const zoomLabel = `${Math.round(document.camera.zoom * 100)}%`;
  const toggleGridVisible = () => {
    onDocumentChange({
      ...document,
      updatedAt: new Date().toISOString(),
      grid: {
        ...document.grid,
        visible: !document.grid.visible,
      },
    });
  };
  const toggleGridSnap = () => {
    onDocumentChange({
      ...document,
      updatedAt: new Date().toISOString(),
      grid: {
        ...document.grid,
        snap: !document.grid.snap,
      },
      snap: {
        ...document.snap,
        grid: !document.grid.snap,
      },
    });
  };

  return (
    <div
      className={[
        'bp-border-bottom-inset flex shrink-0 items-center overflow-x-auto',
        PRIMARY_BAND_HEIGHT,
        VIEWER_TOOLBAR_INSET_X,
        'gap-2',
        SHELL_SURFACE_PANEL,
      ].join(' ')}
      data-testid="butter-canvas-toolbar"
    >
      <ButtonGroup aria-label="Insert controls" className="gap-1 rounded-2xl bg-muted/50 p-1 [&>[data-slot]]:rounded-xl!">
        <ToolbarButton label="Insert Image" onClick={onInsertImage}>
          <ToolbarIcon icon={FileImage} />
        </ToolbarButton>
        <ToolbarButton label="Insert PDF" onClick={onInsertPdf}>
          <ToolbarIcon icon={FileText} />
        </ToolbarButton>
        <ToolbarButton label="Trace Image" disabled={document.assets.length === 0} onClick={onTraceImage}>
          <ToolbarIcon icon={ScanLine} />
        </ToolbarButton>
      </ButtonGroup>
      <ButtonGroup aria-label="Canvas controls" className="gap-1 rounded-2xl bg-muted/50 p-1 [&>[data-slot]]:rounded-xl!">
        <ToolbarButton active={document.grid.visible} label="Show Grid" onClick={toggleGridVisible}>
          <ToolbarIcon icon={Grid2x2} />
        </ToolbarButton>
        <ToolbarButton active={document.grid.snap || document.snap.grid} label="Snap to Grid" onClick={toggleGridSnap}>
          <ToolbarIcon icon={Magnet} />
        </ToolbarButton>
        <ToolbarButton active={document.scale !== null} label="Canvas Scale" onClick={onSetScale}>
          <ToolbarIcon icon={Ruler} />
        </ToolbarButton>
      </ButtonGroup>
      <ButtonGroup aria-label="Zoom controls" className="gap-1 rounded-2xl bg-muted/50 p-1 [&>[data-slot]]:rounded-xl!">
        <ToolbarButton label="Zoom Out" onClick={onZoomOut}>
          <ToolbarIcon icon={ZoomOut} />
        </ToolbarButton>
        <ToolbarButton label={`Zoom ${zoomLabel}`} onClick={onFit}>
          <span className="min-w-[52px] tabular-nums">{zoomLabel}</span>
        </ToolbarButton>
        <ToolbarButton label="Zoom In" onClick={onZoomIn}>
          <ToolbarIcon icon={ZoomIn} />
        </ToolbarButton>
      </ButtonGroup>
      <ButtonGroup aria-label="History controls" className="gap-1 rounded-2xl bg-muted/50 p-1 [&>[data-slot]]:rounded-xl!">
        <ToolbarButton disabled={!canUndo} label="Undo" onClick={onUndo}>
          <ToolbarIcon icon={Undo2} />
        </ToolbarButton>
        <ToolbarButton disabled={!canRedo} label="Redo" onClick={onRedo}>
          <ToolbarIcon icon={Redo2} />
        </ToolbarButton>
      </ButtonGroup>
    </div>
  );
}
