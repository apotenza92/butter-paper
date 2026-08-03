export type RailSide = 'left' | 'right';

const RAIL_EXPAND_ON_HOVER_STORAGE_KEYS: Record<RailSide, string> = {
  left: 'butter-paper:left-rail-expand-on-hover',
  right: 'butter-paper:right-rail-expand-on-hover',
};

type RailSettingsStorage = Pick<Storage, 'getItem' | 'setItem'>;

export function loadRailExpandOnHover(storage: RailSettingsStorage, side: RailSide): boolean {
  try {
    return storage.getItem(RAIL_EXPAND_ON_HOVER_STORAGE_KEYS[side]) !== 'false';
  } catch {
    return true;
  }
}

export function saveRailExpandOnHover(storage: RailSettingsStorage, side: RailSide, enabled: boolean): void {
  try {
    storage.setItem(RAIL_EXPAND_ON_HOVER_STORAGE_KEYS[side], String(enabled));
  } catch {
    // Storage can be unavailable in a restricted renderer; the live setting still applies.
  }
}

export function shouldExpandRail({
  enabled,
  hovered,
  settingsOpen,
  singleColumn,
}: {
  enabled: boolean;
  hovered: boolean;
  settingsOpen: boolean;
  singleColumn: boolean;
}): boolean {
  return singleColumn && enabled && (hovered || settingsOpen);
}
