import { Fragment, useState, type MouseEvent, type ReactNode } from 'react';
import type { ToolMode } from '../../../shared/protocol';
import { Toggle } from '@/components/ui/toggle';
import { Separator } from '@/components/ui/separator';
import { SlidersHorizontal } from 'lucide-react';
import { cn } from '@/lib/utils';
import { getToolDefinition, PDF_TOOL_RAIL_GROUPS } from '../pdf-tools/toolRegistry';
import type { SnapSettings } from '../state/viewerStore';
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
import { SnapSettingsMenu } from './SnapSettingsMenu';
import {
  RAIL_BUTTON_SIZE,
  RAIL_COLUMN_PITCH_PX,
  RAIL_HEADING_TEXT_CLASS,
  RAIL_SINGLE_COLUMN_WIDTH_PX,
  SHELL_SURFACE_APP,
} from './shellSpacing';

interface RightRailProps {
  activeTool: ToolMode;
  disabled?: boolean;
  mutationDisabled?: boolean;
  propertiesOpen: boolean;
  snapSettings: SnapSettings;
  onSelectTool: (tool: ToolMode, clickCount: number) => void;
  onSnapSettingsChange: (settings: Partial<SnapSettings>) => void;
  onToggleProperties: () => void;
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
  ...Object.values(PDF_TOOL_RAIL_GROUPS).map((group) => group.length),
);
export const RIGHT_RAIL_COLUMN_PITCH = RAIL_COLUMN_PITCH_PX;
const RIGHT_RAIL_WIDTH_OFFSET = RAIL_SINGLE_COLUMN_WIDTH_PX - RIGHT_RAIL_COLUMN_PITCH;

export function getTopControlColumnCount(columnCount: number): number {
  return columnCount > RIGHT_RAIL_MIN_COLUMNS ? 2 : 1;
}

export function shouldDispatchToolSelection(clickCount: number): boolean {
  return clickCount <= 1;
}

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

export function shouldShowRightRailHeadings(columnCount: number): boolean {
  return columnCount > RIGHT_RAIL_MIN_COLUMNS;
}

export function getToolPropertiesDoubleClickTooltip(propertiesOpen: boolean): string {
  return propertiesOpen ? 'Double click to hide properties' : 'Double click to show properties';
}

function RailToolButton({
  active,
  disabled,
  doubleClickTooltip,
  label,
  shortcut,
  shortcutLabel,
  icon,
  testId,
  onClick,
  onDoubleClick,
}: {
  active: boolean;
  disabled?: boolean;
  doubleClickTooltip: string;
  label: string;
  shortcut?: string;
  shortcutLabel?: string;
  icon: ReactNode;
  testId: string;
  onClick: (clickCount: number) => void;
  onDoubleClick: () => void;
}) {
  const accessibleLabel = shortcut ? `${label} (${shortcutLabel ?? shortcut})` : label;
  return (
    <Toggle
      type="button"
      pressed={active}
      data-testid={testId}
      data-rail-tooltip={accessibleLabel}
      data-rail-double-click-tooltip={doubleClickTooltip}
      aria-label={label}
      disabled={disabled}
      className={cn('relative shrink-0 p-0', RAIL_BUTTON_SIZE)}
      onClick={(event: MouseEvent<HTMLButtonElement>) => {
        if (shouldDispatchToolSelection(event.detail)) onClick(event.detail);
      }}
      onDoubleClick={disabled ? undefined : onDoubleClick}
    >
      {icon}
    </Toggle>
  );
}

const TOP_RAIL_TOOL_IDS = ['select', 'pan'] as const;

export function RightRail({ activeTool, disabled = false, mutationDisabled = false, propertiesOpen, snapSettings, onSelectTool, onSnapSettingsChange, onToggleProperties }: RightRailProps) {
  const [columnCount, setColumnCount] = useState(RIGHT_RAIL_DEFAULT_COLUMNS);
  const width = getRightRailWidth(columnCount);

  return (
    <aside
      className={cn(
        'relative flex h-full min-h-0 flex-none flex-col items-center border-l border-border',
        'px-2',
        SHELL_SURFACE_APP,
      )}
      data-testid="right-rail"
      data-column-count={columnCount}
      style={{ width: `${width}px` }}
    >
      <RailScrollArea
        contentClassName="py-2"
        overflowIndicatorTestId="right-rail-overflow-indicator"
        overflowSide="left"
        viewportTestId="right-rail-viewport"
      >
        <GeneralRailGroup
          columnCount={columnCount}
          activeTool={activeTool}
          disabled={disabled}
          mutationDisabled={mutationDisabled}
          propertiesOpen={propertiesOpen}
          snapSettings={snapSettings}
          onSelectTool={onSelectTool}
          onSnapSettingsChange={onSnapSettingsChange}
          onToggleProperties={onToggleProperties}
        />
        <Separator
          aria-label="General and Markup tools"
          data-testid="right-rail-group-divider-markup"
        />
        {RIGHT_RAIL_GROUP_DEFINITIONS.map(({ group, heading }, index) => (
          <Fragment key={group}>
            {index > 0 ? (
              <Separator
                aria-label={`${RIGHT_RAIL_GROUP_DEFINITIONS[index - 1].heading} and ${heading} tools`}
                data-testid={`right-rail-group-divider-${group}`}
              />
            ) : null}
            <RightRailGroup
              group={group}
              heading={heading}
              columnCount={columnCount}
              activeTool={activeTool}
              disabled={disabled}
              mutationDisabled={mutationDisabled}
              propertiesOpen={propertiesOpen}
              onSelectTool={onSelectTool}
              onToggleProperties={onToggleProperties}
            />
          </Fragment>
        ))}
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

function GeneralRailGroup({
  columnCount,
  activeTool,
  disabled,
  mutationDisabled = false,
  propertiesOpen,
  snapSettings,
  onSelectTool,
  onSnapSettingsChange,
  onToggleProperties,
}: RightRailProps & { columnCount: number }) {
  const editingControlsDisabled = disabled || mutationDisabled;
  return (
    <section
      aria-label="General tools"
      className="flex w-full flex-col items-center gap-2"
      data-testid="properties-trigger-slot"
    >
      {shouldShowRightRailHeadings(columnCount) ? (
        <h2
          className={cn('w-full text-center', RAIL_HEADING_TEXT_CLASS)}
          data-testid="right-rail-general-heading"
        >
          General
        </h2>
      ) : null}
      <div
        className="grid shrink-0 justify-center gap-2"
        data-testid="top-rail-control-grid"
        style={{ gridTemplateColumns: `repeat(${getTopControlColumnCount(columnCount)}, 32px)` }}
      >
        <Toggle
          type="button"
          pressed={propertiesOpen}
          className={cn('relative shrink-0 p-0', RAIL_BUTTON_SIZE)}
          aria-label={propertiesOpen ? 'Hide properties' : 'Show properties'}
          aria-expanded={propertiesOpen}
          data-rail-tooltip={propertiesOpen ? 'Hide properties' : 'Show properties'}
          data-testid="properties-sidebar-trigger"
          disabled={editingControlsDisabled}
          onClick={editingControlsDisabled ? undefined : onToggleProperties}
        >
          <SlidersHorizontal aria-hidden="true" />
        </Toggle>
        <SnapSettingsMenu
          disabled={editingControlsDisabled}
          snapSettings={snapSettings}
          onSnapSettingsChange={onSnapSettingsChange}
        />
        {TOP_RAIL_TOOL_IDS.map((toolId) => {
          const tool = getToolDefinition(toolId);
          return (
            <RailToolButton
              key={tool.id}
              active={activeTool === tool.id}
              disabled={disabled}
              doubleClickTooltip={getToolPropertiesDoubleClickTooltip(propertiesOpen)}
              label={tool.label}
              shortcut={tool.shortcut}
              shortcutLabel={tool.shortcutLabel}
              icon={TOOL_ICONS[tool.id]}
              testId={tool.testId}
              onClick={(clickCount) => onSelectTool(tool.id, clickCount)}
              onDoubleClick={mutationDisabled ? () => undefined : onToggleProperties}
            />
          );
        })}
      </div>
    </section>
  );
}

const RIGHT_RAIL_GROUP_DEFINITIONS = [
  { group: 'markup', heading: 'Markup' },
  { group: 'draw', heading: 'Draw' },
  { group: 'measure', heading: 'Measure' },
] as const satisfies readonly {
  group: keyof typeof PDF_TOOL_RAIL_GROUPS;
  heading: string;
}[];

function RightRailGroup({
  group,
  heading,
  columnCount,
  activeTool,
  disabled,
  mutationDisabled = false,
  propertiesOpen,
  onSelectTool,
  onToggleProperties,
}: Pick<RightRailProps, 'activeTool' | 'disabled' | 'mutationDisabled' | 'propertiesOpen' | 'onSelectTool' | 'onToggleProperties'> & {
  group: keyof typeof PDF_TOOL_RAIL_GROUPS;
  heading: string;
  columnCount: number;
}) {
  return (
    <section
      aria-label={`${heading} tools`}
      className="flex w-full flex-col items-center gap-2"
      data-testid={`right-rail-${group}`}
    >
      {shouldShowRightRailHeadings(columnCount) ? (
        <h2 className={cn('w-full text-center', RAIL_HEADING_TEXT_CLASS)}>
          {heading}
        </h2>
      ) : null}
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
              doubleClickTooltip={getToolPropertiesDoubleClickTooltip(propertiesOpen)}
              label={tool.label}
              shortcut={tool.shortcut}
              shortcutLabel={tool.shortcutLabel}
              disabled={disabled || mutationDisabled}
              icon={TOOL_ICONS[tool.id]}
              testId={tool.testId}
              onClick={(clickCount) => onSelectTool(tool.id, clickCount)}
              onDoubleClick={onToggleProperties}
            />
          );
        })}
      </div>
    </section>
  );
}
