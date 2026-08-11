import type { Markup } from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';
import { getMarkupToolDefinition, getToolDefinition } from '../pdf-tools/toolRegistry';
import {
  DEFAULT_RIGHT_SIDEBAR_WIDTH,
  MAX_RIGHT_SIDEBAR_WIDTH,
  MIN_RIGHT_SIDEBAR_WIDTH,
  useViewerStore,
} from '../state/viewerStore';
import { SidebarResizeHandle } from './SidebarResizeHandle';
import { ToolPropertiesPanel } from './ToolPropertiesPanel';
import {
  PRIMARY_BAND_HEIGHT,
  SHELL_BORDER_SUBTLE,
  SHELL_HEADER_INSET_X,
  SHELL_SURFACE_PANEL,
  SHELL_TEXT_PRIMARY,
} from './shellSpacing';

interface RightSidebarProps {
  activeTool: ToolMode;
  mutationDisabled?: boolean;
  width: number;
  onWidthChange: (width: number) => void;
}

export function RightSidebar({ activeTool, mutationDisabled = false, width, onWidthChange }: RightSidebarProps) {
  const selectedMarkupIds = useViewerStore((state) => state.selectedMarkupIds);
  const document = useViewerStore((state) => state.document?.document ?? null);
  const selectedMarkup = activeTool === 'select'
    ? findFocusedSelectedMarkup(document?.markups ?? [], selectedMarkupIds)
    : null;
  const heading = selectedMarkup
    ? getMarkupToolDefinition(selectedMarkup)?.label ?? 'Markup'
    : getToolDefinition(activeTool).label;
  return (
    <aside
      className={['relative flex h-full flex-none flex-col border-l', SHELL_SURFACE_PANEL, SHELL_BORDER_SUBTLE].join(' ')}
      data-testid="right-sidebar"
      style={{ width: `${width}px` }}
    >
      <div
        className={[
          'flex items-center justify-center border-b text-center text-[12px] font-semibold',
          PRIMARY_BAND_HEIGHT,
          SHELL_HEADER_INSET_X,
          SHELL_BORDER_SUBTLE,
          SHELL_TEXT_PRIMARY,
        ].join(' ')}
        data-testid="right-sidebar-header"
      >
        <span className="w-full truncate text-center" data-testid="right-sidebar-heading">
          {heading}
        </span>
      </div>
      <ToolPropertiesPanel activeTool={activeTool} selectedMarkup={selectedMarkup} mutationDisabled={mutationDisabled} />
      <SidebarResizeHandle
        side="right"
        width={width}
        minWidth={MIN_RIGHT_SIDEBAR_WIDTH}
        maxWidth={MAX_RIGHT_SIDEBAR_WIDTH}
        defaultWidth={DEFAULT_RIGHT_SIDEBAR_WIDTH}
        label="Tool sidebar"
        testId="right-sidebar-resize-handle"
        onWidthChange={onWidthChange}
      />
    </aside>
  );
}

export function findFocusedSelectedMarkup(markups: readonly Markup[], selectedMarkupIds: readonly string[]): Markup | null {
  const focusedMarkupId = selectedMarkupIds[0];
  return focusedMarkupId
    ? markups.find((markup) => markup.id === focusedMarkupId) ?? null
    : null;
}
