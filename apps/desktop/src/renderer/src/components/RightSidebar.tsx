import type { Markup } from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';
import { getMarkupToolDefinition, getToolDefinition } from '../pdf-tools/toolRegistry';
import { DEFAULT_RIGHT_SIDEBAR_WIDTH, useViewerStore } from '../state/viewerStore';
import { ToolPropertiesPanel } from './ToolPropertiesPanel';
import { PRIMARY_BAND_HEIGHT, SHELL_BAND_BORDER_BOTTOM, SHELL_HEADER_INSET_X, SHELL_PANEL_BORDER_LEFT, SHELL_SURFACE_PANEL, SHELL_TEXT_PRIMARY } from './shellSpacing';

interface RightSidebarProps {
  activeTool: ToolMode;
  mutationDisabled?: boolean;
}

export function RightSidebar({ activeTool, mutationDisabled = false }: RightSidebarProps) {
  const selectedMarkupIds = useViewerStore((state) => state.selectedMarkupIds);
  const document = useViewerStore((state) => state.document?.document ?? null);
  const selectedMarkup = activeTool === 'select' ? findFocusedSelectedMarkup(document?.markups ?? [], selectedMarkupIds) : null;
  const heading = selectedMarkup ? (getMarkupToolDefinition(selectedMarkup)?.label ?? 'Markup') : getToolDefinition(activeTool).label;
  return (
    <aside className={['relative flex h-full flex-none flex-col', SHELL_SURFACE_PANEL, SHELL_PANEL_BORDER_LEFT].join(' ')} data-testid="right-sidebar" style={{ width: `${DEFAULT_RIGHT_SIDEBAR_WIDTH}px` }}>
      <div className={['flex items-center justify-center text-center text-[12px] font-semibold', PRIMARY_BAND_HEIGHT, SHELL_HEADER_INSET_X, SHELL_BAND_BORDER_BOTTOM, SHELL_TEXT_PRIMARY].join(' ')} data-testid="right-sidebar-header">
        <span className="w-full truncate text-center" data-testid="right-sidebar-heading">
          {heading}
        </span>
      </div>
      <ToolPropertiesPanel activeTool={activeTool} selectedMarkup={selectedMarkup} mutationDisabled={mutationDisabled} />
    </aside>
  );
}

export function findFocusedSelectedMarkup(markups: readonly Markup[], selectedMarkupIds: readonly string[]): Markup | null {
  const focusedMarkupId = selectedMarkupIds[0];
  return focusedMarkupId ? (markups.find((markup) => markup.id === focusedMarkupId) ?? null) : null;
}
