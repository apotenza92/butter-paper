import { PagesRailIcon } from './RailIcons';
import { RailScrollArea } from './RailScrollArea';
import { Toggle } from '@/components/ui/toggle';
import { cn } from '@/lib/utils';
import {
  RAIL_INSET,
  RAIL_BUTTON_SIZE,
  RAIL_WIDTH,
  SHELL_SURFACE_APP,
} from './shellSpacing';

interface LeftRailProps {
  activePanel: 'pages' | null;
  disabled?: boolean;
  onToggle: (panel: 'pages') => void;
}

export function LeftRail({ activePanel, disabled = false, onToggle }: LeftRailProps) {
  return (
    <aside
      className={cn(
        'flex h-full min-h-0 flex-none flex-col items-center border-r border-border',
        RAIL_WIDTH,
        RAIL_INSET,
        SHELL_SURFACE_APP,
      )}
      data-testid="left-rail"
    >
      <RailScrollArea
        overflowIndicatorTestId="left-rail-overflow-indicator"
        overflowSide="right"
      >
        <Toggle
          type="button"
          pressed={activePanel === 'pages'}
          data-testid="left-rail-pages"
          data-rail-tooltip="Page Thumbnails"
          aria-label="Page Thumbnails"
          aria-expanded={activePanel === 'pages'}
          aria-controls="left-sidebar-panel"
          disabled={disabled}
          className={cn('relative shrink-0 p-0', RAIL_BUTTON_SIZE)}
          onPressedChange={() => onToggle('pages')}
        >
          <PagesRailIcon />
        </Toggle>
      </RailScrollArea>
    </aside>
  );
}
