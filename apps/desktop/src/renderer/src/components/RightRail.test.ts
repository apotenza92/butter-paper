import { describe, expect, it } from 'vitest';
import {
  getRightRailWidth,
  resolveRightRailColumnCount,
  RIGHT_RAIL_DEFAULT_COLUMNS,
  RIGHT_RAIL_MAX_COLUMNS,
} from './RightRail';

describe('resizable right rail sizing', () => {
  it('uses compact widths that fit stock Nova toggles and 8px gaps', () => {
    expect(getRightRailWidth(1)).toBe(48);
    expect(getRightRailWidth(2)).toBe(88);
    expect(getRightRailWidth(3)).toBe(128);
  });

  it('snaps arbitrary drag widths to the nearest whole column', () => {
    expect(resolveRightRailColumnCount(67)).toBe(1);
    expect(resolveRightRailColumnCount(68)).toBe(2);
    expect(resolveRightRailColumnCount(107)).toBe(2);
    expect(resolveRightRailColumnCount(108)).toBe(3);
  });

  it('clamps to useful group bounds and recovers invalid widths', () => {
    expect(resolveRightRailColumnCount(-1_000)).toBe(1);
    expect(resolveRightRailColumnCount(100_000)).toBe(RIGHT_RAIL_MAX_COLUMNS);
    expect(resolveRightRailColumnCount(Number.NaN)).toBe(RIGHT_RAIL_DEFAULT_COLUMNS);
    expect(getRightRailWidth(RIGHT_RAIL_MAX_COLUMNS + 1)).toBe(getRightRailWidth(RIGHT_RAIL_MAX_COLUMNS));
  });
});
