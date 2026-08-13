import { Fragment, useState, type MouseEvent, type ReactNode } from 'react';
import type { SignatureAppearanceAsset } from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';
import { Button } from '@/components/ui/button';
import { Toggle } from '@/components/ui/toggle';
import { Separator } from '@/components/ui/separator';
import { ScanLine, SlidersHorizontal } from 'lucide-react';
import { cn } from '@/lib/utils';
import { getToolDefinition, PDF_TOOL_RAIL_GROUPS } from '../pdf-tools/toolRegistry';
import type { SnapSettings } from '../state/viewerStore';
import { ToolRailIcon } from './RailIcons';
import { RailScrollArea } from './RailScrollArea';
import { SidebarResizeHandle } from './SidebarResizeHandle';
import { SnapSettingsMenu } from './SnapSettingsMenu';
import { SignatureMenu } from './SignatureMenu';
import {
  RAIL_BUTTON_SIZE,
  RAIL_COLUMN_PITCH_PX,
  RAIL_HEADING_TEXT_CLASS,
  RAIL_SINGLE_COLUMN_WIDTH_PX,
  PRIMARY_BAND_HEIGHT,
  SHELL_BAND_BORDER_BOTTOM,
  SHELL_HORIZONTAL_SEPARATOR,
  SHELL_PANEL_BORDER_LEFT,
  SHELL_SURFACE_APP,
} from './shellSpacing';

interface RightRailProps {
  activeTool: ToolMode;
  disabled?: boolean;
  mutationDisabled?: boolean;
  propertiesOpen: boolean;
  snapSettings: SnapSettings;
  signatureContextId?: string | null;
  onSelectTool: (tool: ToolMode, clickCount: number) => void;
  onSetPageScale?: () => void;
  onUseSignature?: (asset: SignatureAppearanceAsset) => void;
  onSnapSettingsChange: (settings: Partial<SnapSettings>) => void;
  onToggleProperties: () => void;
}

export const RIGHT_RAIL_MIN_COLUMNS = 1;
export const RIGHT_RAIL_DEFAULT_COLUMNS = 2;
export const RIGHT_RAIL_MAX_COLUMNS = 8;
export const RIGHT_RAIL_COLUMN_PITCH = RAIL_COLUMN_PITCH_PX;
const RIGHT_RAIL_WIDTH_OFFSET = RAIL_SINGLE_COLUMN_WIDTH_PX - RIGHT_RAIL_COLUMN_PITCH;

export function getTopControlColumnCount(columnCount: number): number {
  return Math.min(2, clampRightRailColumnCount(columnCount));
}

export function getToolGroupColumnCount(
  group: keyof typeof PDF_TOOL_RAIL_GROUPS,
  columnCount: number,
): number {
  const extraControlCount = group === 'markup' || group === 'measure' ? 1 : 0;
  return Math.min(
    PDF_TOOL_RAIL_GROUPS[group].length + extraControlCount,
    clampRightRailColumnCount(columnCount),
  );
}

function getControlGroupWidth(columnCount: number): number {
  return columnCount * RIGHT_RAIL_COLUMN_PITCH - (RIGHT_RAIL_COLUMN_PITCH - 32);
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

export function RightRail({ activeTool, disabled = false, mutationDisabled = false, propertiesOpen, snapSettings, signatureContextId, onSelectTool, onSetPageScale, onUseSignature, onSnapSettingsChange, onToggleProperties }: RightRailProps) {
  const [columnCount, setColumnCount] = useState(RIGHT_RAIL_DEFAULT_COLUMNS);
  const width = getRightRailWidth(columnCount);

  return (
    <aside
      className={cn(
        'relative flex h-full min-h-0 flex-none flex-col items-center',
        SHELL_PANEL_BORDER_LEFT,
        SHELL_SURFACE_APP,
      )}
      data-testid="right-rail"
      data-column-count={columnCount}
      style={{ width: `${width}px` }}
    >
      <RailScrollArea
        contentClassName="py-2"
        fixedContent={(
          <PinnedRailControls
            columnCount={columnCount}
            disabled={disabled}
            mutationDisabled={mutationDisabled}
            propertiesOpen={propertiesOpen}
            snapSettings={snapSettings}
            onSnapSettingsChange={onSnapSettingsChange}
            onToggleProperties={onToggleProperties}
          />
        )}
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
          onSelectTool={onSelectTool}
          onToggleProperties={onToggleProperties}
        />
        <Separator
          aria-label="General and Markup tools"
          className={SHELL_HORIZONTAL_SEPARATOR}
          data-testid="right-rail-group-divider-markup"
        />
        {RIGHT_RAIL_GROUP_DEFINITIONS.map(({ group, heading }, index) => (
          <Fragment key={group}>
            {index > 0 ? (
              <Separator
                aria-label={`${RIGHT_RAIL_GROUP_DEFINITIONS[index - 1].heading} and ${heading} tools`}
                className={SHELL_HORIZONTAL_SEPARATOR}
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
              signatureContextId={signatureContextId}
              onSelectTool={onSelectTool}
              onSetPageScale={onSetPageScale}
              onUseSignature={onUseSignature}
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
  onSelectTool,
  onToggleProperties,
}: Pick<RightRailProps, 'activeTool' | 'disabled' | 'mutationDisabled' | 'propertiesOpen' | 'onSelectTool' | 'onToggleProperties'> & { columnCount: number }) {
  return (
    <section
      aria-label="General tools"
      className="flex w-full flex-col items-center gap-2 px-2"
      data-testid="general-rail-group"
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
        className="flex shrink-0 flex-wrap justify-center gap-2"
        data-testid="general-rail-control-grid"
        style={{ width: `${getControlGroupWidth(getTopControlColumnCount(columnCount))}px` }}
      >
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
              icon={<ToolRailIcon tool={tool.id} />}
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

function PinnedRailControls({
  columnCount,
  disabled = false,
  mutationDisabled = false,
  propertiesOpen,
  snapSettings,
  onSnapSettingsChange,
  onToggleProperties,
}: Pick<RightRailProps, 'disabled' | 'mutationDisabled' | 'propertiesOpen' | 'snapSettings' | 'onSnapSettingsChange' | 'onToggleProperties'> & { columnCount: number }) {
  const editingControlsDisabled = disabled || mutationDisabled;
  return (
    <section
      aria-label="Properties and snap settings"
      className={cn(
        'flex w-full shrink-0 items-center justify-center px-2',
        PRIMARY_BAND_HEIGHT,
        SHELL_BAND_BORDER_BOTTOM,
      )}
      data-testid="properties-trigger-slot"
    >
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
  onSetPageScale,
  onUseSignature,
  onToggleProperties,
  signatureContextId,
}: Pick<RightRailProps, 'activeTool' | 'disabled' | 'mutationDisabled' | 'propertiesOpen' | 'signatureContextId' | 'onSelectTool' | 'onSetPageScale' | 'onUseSignature' | 'onToggleProperties'> & {
  group: keyof typeof PDF_TOOL_RAIL_GROUPS;
  heading: string;
  columnCount: number;
}) {
  return (
    <section
      aria-label={`${heading} tools`}
      className="flex w-full flex-col items-center gap-2 px-2"
      data-testid={`right-rail-${group}`}
    >
      {shouldShowRightRailHeadings(columnCount) ? (
        <h2 className={cn('w-full text-center', RAIL_HEADING_TEXT_CLASS)}>
          {heading}
        </h2>
      ) : null}
      <div
        className="flex flex-wrap justify-center gap-2"
        style={{ width: `${getControlGroupWidth(getToolGroupColumnCount(group, columnCount))}px` }}
      >
        {group === 'measure' ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className={cn('relative shrink-0 border-0 bg-transparent p-0', RAIL_BUTTON_SIZE)}
            aria-label="Set page scale"
            data-rail-tooltip="Set page scale"
            data-testid="measure-set-page-scale"
            disabled={disabled || mutationDisabled || !onSetPageScale}
            onClick={onSetPageScale}
          >
            <ScanLine data-icon="inline-start" aria-hidden="true" />
          </Button>
        ) : null}
        {PDF_TOOL_RAIL_GROUPS[group].map((toolId) => {
          const tool = getToolDefinition(toolId);
          return (
            <Fragment key={tool.id}>
              {group === 'markup' && toolId === 'image' ? (
                <SignatureMenu
                  contextId={signatureContextId}
                  disabled={disabled || mutationDisabled}
                  onUseSignature={onUseSignature ?? (() => undefined)}
                />
              ) : null}
              <RailToolButton
                active={activeTool === tool.id}
                doubleClickTooltip={getToolPropertiesDoubleClickTooltip(propertiesOpen)}
                label={tool.label}
                shortcut={tool.shortcut}
                shortcutLabel={tool.shortcutLabel}
                disabled={disabled || mutationDisabled}
                icon={<ToolRailIcon tool={tool.id} />}
                testId={tool.testId}
                onClick={(clickCount) => onSelectTool(tool.id, clickCount)}
                onDoubleClick={onToggleProperties}
              />
            </Fragment>
          );
        })}
      </div>
    </section>
  );
}
