import type { ToolMode } from '../../../shared/protocol';
import { getToolDefinition } from '../pdf-tools/toolRegistry';
import {
  DEFAULT_RIGHT_SIDEBAR_WIDTH,
  MAX_RIGHT_SIDEBAR_WIDTH,
  MIN_RIGHT_SIDEBAR_WIDTH,
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
  const activeToolDefinition = getToolDefinition(activeTool);
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
          {activeToolDefinition.label}
        </span>
      </div>
      <ToolPropertiesPanel activeTool={activeTool} mutationDisabled={mutationDisabled} />
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
