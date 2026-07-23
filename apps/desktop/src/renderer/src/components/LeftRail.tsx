import { PagesRailIcon } from './RailIcons';
import { RailScrollArea } from './RailScrollArea';
import { Toggle } from '@/components/ui/toggle';
import {
  RAIL_INSET,
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
      className={['bp-border-right-inset flex h-full min-h-0 flex-col items-center', RAIL_WIDTH, RAIL_INSET, SHELL_SURFACE_APP].join(' ')}
      data-testid="left-rail"
    >
      <RailScrollArea
        overflowIndicatorTestId="left-rail-overflow-indicator"
        overflowSide="right"
      >
        <Toggle
          type="button"
          variant="outline"
          pressed={active}
          data-testid="left-rail-pages"
          data-rail-tooltip="Pages"
          aria-label="Pages"
          aria-expanded={active}
          disabled={disabled}
          className="group relative size-8 shrink-0 rounded-2xl p-0"
          onPressedChange={onToggle}
        >
          <PagesRailIcon />
        </Toggle>
      </RailScrollArea>
    </aside>
  );
}
