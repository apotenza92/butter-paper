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
  active: boolean;
  disabled?: boolean;
  onToggle: () => void;
}

export function LeftRail({ active, disabled = false, onToggle }: LeftRailProps) {
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
          pressed={active}
          data-testid="left-rail-pages"
          data-rail-tooltip="Pages"
          aria-label="Pages"
          aria-expanded={active}
          disabled={disabled}
          className={cn('relative shrink-0 p-0', RAIL_BUTTON_SIZE)}
          onPressedChange={onToggle}
        >
          <PagesRailIcon />
        </Toggle>
      </RailScrollArea>
    </aside>
  );
}
