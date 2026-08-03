import { useState, type ReactNode } from 'react';
import type { ToolMode } from '../../../shared/protocol';
import { Toggle } from '@/components/ui/toggle';
import { Separator } from '@/components/ui/separator';
import { cn } from '@/lib/utils';
import { getToolDefinition, PDF_TOOL_RAIL_GROUPS } from '../pdf-tools/toolRegistry';
import {
  PanRailIcon,
  ArcRailIcon,
  RectangleRailIcon,
  EllipseRailIcon,
  LineRailIcon,
  LengthRailIcon,
  PolylengthRailIcon,
  AreaRailIcon,
  ArrowRailIcon,
  CalloutRailIcon,
  CloudPlusRailIcon,
  CloudRailIcon,
  DimensionRailIcon,
  HighlightRailIcon,
  ImageRailIcon,
  PolylineRailIcon,
  PolygonRailIcon,
  PenRailIcon,
  SelectRailIcon,
  SnapshotRailIcon,
  TextBoxRailIcon,
} from './RailIcons';
import { RailScrollArea } from './RailScrollArea';
import { SidebarResizeHandle } from './SidebarResizeHandle';
import {
  RAIL_BUTTON_SIZE,
  RAIL_INSET,
  SHELL_SURFACE_APP,
} from './shellSpacing';

interface RightRailProps {
  activeTool: ToolMode;
  disabled?: boolean;
  onSelectTool: (tool: ToolMode) => void;
}

const TOOL_ICONS: Record<ToolMode, ReactNode> = {
  select: <SelectRailIcon />,
  pan: <PanRailIcon />,
  'text-box': <TextBoxRailIcon />,
  rectangle: <RectangleRailIcon />,
  ellipse: <EllipseRailIcon />,
  arc: <ArcRailIcon />,
  line: <LineRailIcon />,
  arrow: <ArrowRailIcon />,
  dimension: <DimensionRailIcon />,
  length: <LengthRailIcon />,
  polylength: <PolylengthRailIcon />,
  area: <AreaRailIcon />,
  polyline: <PolylineRailIcon />,
  polygon: <PolygonRailIcon />,
  pen: <PenRailIcon />,
  highlight: <HighlightRailIcon />,
  cloud: <CloudRailIcon />,
  'cloud-plus': <CloudPlusRailIcon />,
  callout: <CalloutRailIcon />,
  image: <ImageRailIcon />,
  snapshot: <SnapshotRailIcon />,
};

export const RIGHT_RAIL_MIN_COLUMNS = 1;
export const RIGHT_RAIL_DEFAULT_COLUMNS = 2;
export const RIGHT_RAIL_MAX_COLUMNS = Math.max(
  PDF_TOOL_RAIL_GROUPS.normal.length,
  PDF_TOOL_RAIL_GROUPS.cad.length,
);
export const RIGHT_RAIL_COLUMN_PITCH = 40;
const RIGHT_RAIL_WIDTH_OFFSET = 8;

function clampRightRailColumnCount(columnCount: number): number {
  return Math.min(
    RIGHT_RAIL_MAX_COLUMNS,
    Math.max(RIGHT_RAIL_MIN_COLUMNS, Math.round(columnCount)),
  );
}

export function getRightRailWidth(columnCount: number): number {
  return RIGHT_RAIL_WIDTH_OFFSET + RIGHT_RAIL_COLUMN_PITCH * clampRightRailColumnCount(columnCount);
}

export function resolveRightRailColumnCount(width: number): number {
  if (!Number.isFinite(width)) {
    return RIGHT_RAIL_DEFAULT_COLUMNS;
  }

  return clampRightRailColumnCount((width - RIGHT_RAIL_WIDTH_OFFSET) / RIGHT_RAIL_COLUMN_PITCH);
}

function RailToolButton({
  active,
  disabled,
  label,
  shortcut,
  icon,
  testId,
  onClick,
}: {
  active: boolean;
  disabled?: boolean;
  label: string;
  shortcut?: string;
  icon: ReactNode;
  testId: string;
  onClick: () => void;
}) {
  const accessibleLabel = shortcut ? `${label} (${shortcut})` : label;
  return (
    <Toggle
      type="button"
      pressed={active}
      data-testid={testId}
      data-rail-tooltip={accessibleLabel}
      aria-label={label}
      disabled={disabled}
      className={cn('relative shrink-0 p-0', RAIL_BUTTON_SIZE)}
      onPressedChange={onClick}
    >
      {icon}
    </Toggle>
  );
}

export function RightRail({ activeTool, disabled = false, onSelectTool }: RightRailProps) {
  const [columnCount, setColumnCount] = useState(RIGHT_RAIL_DEFAULT_COLUMNS);
  const width = getRightRailWidth(columnCount);

  return (
    <aside
      className={cn(
        'relative flex h-full min-h-0 flex-none flex-col items-center border-l border-border',
        RAIL_INSET,
        SHELL_SURFACE_APP,
      )}
      data-testid="right-rail"
      data-column-count={columnCount}
      style={{ width: `${width}px` }}
    >
      <RailScrollArea
        overflowIndicatorTestId="right-rail-overflow-indicator"
        overflowSide="left"
        viewportTestId="right-rail-viewport"
      >
        <RightRailGroup
          group="normal"
          heading="Review"
          columnCount={columnCount}
          activeTool={activeTool}
          disabled={disabled}
          onSelectTool={onSelectTool}
        />
        <Separator
          aria-label="Normal and CAD tools"
          data-testid="right-rail-group-divider"
        />
        <RightRailGroup
          group="cad"
          heading="Draw"
          columnCount={columnCount}
          activeTool={activeTool}
          disabled={disabled}
          onSelectTool={onSelectTool}
        />
      </RailScrollArea>
      <SidebarResizeHandle
        side="right"
        width={width}
        minWidth={getRightRailWidth(RIGHT_RAIL_MIN_COLUMNS)}
        maxWidth={getRightRailWidth(RIGHT_RAIL_MAX_COLUMNS)}
        defaultWidth={getRightRailWidth(RIGHT_RAIL_DEFAULT_COLUMNS)}
        label="Annotation tool rail"
        testId="right-rail-resize-handle"
        step={RIGHT_RAIL_COLUMN_PITCH}
        onWidthChange={(nextWidth) => setColumnCount(resolveRightRailColumnCount(nextWidth))}
      />
    </aside>
  );
}

function RightRailGroup({
  group,
  heading,
  columnCount,
  activeTool,
  disabled,
  onSelectTool,
}: RightRailProps & {
  group: keyof typeof PDF_TOOL_RAIL_GROUPS;
  heading: string;
  columnCount: number;
}) {
  return (
    <section className="flex w-full flex-col items-center gap-2" data-testid={`right-rail-${group}`}>
      <h2 className="w-full text-center text-[10px] font-medium leading-none text-muted-foreground">
        {heading}
      </h2>
      <div
        className="grid justify-center gap-2"
        style={{ gridTemplateColumns: `repeat(${columnCount}, 32px)` }}
      >
        {PDF_TOOL_RAIL_GROUPS[group].map((toolId) => {
          const tool = getToolDefinition(toolId);
          return (
            <RailToolButton
              key={tool.id}
              active={activeTool === tool.id}
              label={tool.label}
              shortcut={tool.shortcut}
              disabled={disabled}
              icon={TOOL_ICONS[tool.id]}
              testId={tool.testId}
              onClick={() => onSelectTool(tool.id)}
            />
          );
        })}
      </div>
    </section>
  );
}
