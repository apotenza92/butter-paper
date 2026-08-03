import { useState } from 'react';
import { PagesRailIcon } from './RailIcons';
import { RailScrollArea } from './RailScrollArea';
import { RailSettingsPopover } from './RailSettingsPopover';
import { Toggle } from '@/components/ui/toggle';
import { cn } from '@/lib/utils';
import {
  RAIL_INSET,
  RAIL_BUTTON_SIZE,
  RAIL_EXPANDED_WIDTH,
  RAIL_WIDTH,
  SHELL_SURFACE_APP,
} from './shellSpacing';
import { loadRailExpandOnHover, saveRailExpandOnHover, shouldExpandRail } from './railSettings';

interface LeftRailProps {
  active: boolean;
  disabled?: boolean;
  onToggle: () => void;
}

export function LeftRail({ active, disabled = false, onToggle }: LeftRailProps) {
  const [hovered, setHovered] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [expandOnHover, setExpandOnHover] = useState(() => loadRailExpandOnHover(window.localStorage, 'left'));
  const expanded = shouldExpandRail({
    enabled: expandOnHover,
    hovered,
    settingsOpen,
    singleColumn: true,
  });

  function handleExpandOnHoverChange(enabled: boolean): void {
    setExpandOnHover(enabled);
    saveRailExpandOnHover(window.localStorage, 'left', enabled);
  }

  return (
    <aside
      className={cn(
        'flex h-full min-h-0 flex-none flex-col items-center border-r border-border transition-[width] duration-150',
        RAIL_WIDTH,
        RAIL_INSET,
        SHELL_SURFACE_APP,
      )}
      data-testid="left-rail"
      data-expanded={expanded ? '' : undefined}
      style={expanded ? { width: `${RAIL_EXPANDED_WIDTH}px` } : undefined}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
    >
      <RailScrollArea
        overflowIndicatorTestId="left-rail-overflow-indicator"
        overflowSide="right"
        tooltipsDisabled={expanded}
      >
        <Toggle
          type="button"
          pressed={active}
          data-testid="left-rail-pages"
          data-rail-tooltip="Pages"
          aria-label="Pages"
          aria-expanded={active}
          disabled={disabled}
          className={cn(
            'relative shrink-0',
            expanded ? 'w-full justify-start px-2' : [RAIL_BUTTON_SIZE, 'p-0'],
          )}
          onPressedChange={onToggle}
        >
          <PagesRailIcon />
          {expanded ? <span className="truncate">Pages</span> : null}
        </Toggle>
      </RailScrollArea>
      <div className={cn('mt-2 shrink-0', expanded ? 'w-full' : RAIL_BUTTON_SIZE)}>
        <RailSettingsPopover
          side="left"
          expanded={expanded}
          open={settingsOpen}
          expandOnHover={expandOnHover}
          onOpenChange={setSettingsOpen}
          onExpandOnHoverChange={handleExpandOnHoverChange}
        />
      </div>
    </aside>
  );
}
