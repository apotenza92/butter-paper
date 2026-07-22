import { PagesRailIcon } from './RailIcons';
import { RailScrollArea } from './RailScrollArea';
import {
  CONTROL_ACTIVE,
  CONTROL_DEFAULT,
  CONTROL_DISABLED,
  RAIL_BUTTON_SIZE,
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
        <button
          type="button"
          data-testid="left-rail-pages"
          data-rail-tooltip="Pages"
          aria-label="Pages"
          aria-expanded={active}
          disabled={disabled}
          className={[
            'group relative inline-flex shrink-0 items-center justify-center rounded-[6px] border transition',
            RAIL_BUTTON_SIZE,
            disabled ? CONTROL_DISABLED : active ? CONTROL_ACTIVE : CONTROL_DEFAULT,
          ].join(' ')}
          onClick={onToggle}
        >
          <PagesRailIcon />
        </button>
      </RailScrollArea>
    </aside>
  );
}
