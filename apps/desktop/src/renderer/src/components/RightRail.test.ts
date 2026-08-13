import { describe, expect, it } from 'vitest';
import {
  getToolPropertiesDoubleClickTooltip,
  getToolGroupColumnCount,
  getTopControlColumnCount,
  getRightRailWidth,
  resolveRightRailColumnCount,
  shouldDispatchToolSelection,
  shouldShowRightRailHeadings,
  RIGHT_RAIL_DEFAULT_COLUMNS,
  RIGHT_RAIL_MAX_COLUMNS,
} from './RightRail';

describe('resizable right rail sizing', () => {
  it('starts new users with two tool columns', () => {
    expect(RIGHT_RAIL_DEFAULT_COLUMNS).toBe(2);
  });

  it('allows at most eight tool columns', () => {
    expect(RIGHT_RAIL_MAX_COLUMNS).toBe(8);
  });

  it('leaves even focus-safe widths around stock Nova toggles', () => {
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

  it('hides group headings only at the single-column width', () => {
    expect(shouldShowRightRailHeadings(1)).toBe(false);
    expect(shouldShowRightRailHeadings(2)).toBe(true);
    expect(shouldShowRightRailHeadings(RIGHT_RAIL_MAX_COLUMNS)).toBe(true);
  });

  it('keeps the two pinned controls together above wider tool grids', () => {
    expect(getTopControlColumnCount(1)).toBe(1);
    expect(getTopControlColumnCount(2)).toBe(2);
    expect(getTopControlColumnCount(3)).toBe(2);
    expect(getTopControlColumnCount(4)).toBe(2);
    expect(getTopControlColumnCount(RIGHT_RAIL_MAX_COLUMNS)).toBe(2);
  });

  it('centers each tool group instead of reserving empty expansion columns', () => {
    expect(getToolGroupColumnCount('markup', RIGHT_RAIL_MAX_COLUMNS)).toBe(8);
    expect(getToolGroupColumnCount('draw', RIGHT_RAIL_MAX_COLUMNS)).toBe(8);
    expect(getToolGroupColumnCount('measure', RIGHT_RAIL_MAX_COLUMNS)).toBe(4);
    expect(getToolGroupColumnCount('measure', 2)).toBe(2);
  });

  it('dispatches one selection side effect for pointer and keyboard activation', () => {
    expect(shouldDispatchToolSelection(0)).toBe(true);
    expect(shouldDispatchToolSelection(1)).toBe(true);
    expect(shouldDispatchToolSelection(2)).toBe(false);
  });

  it('describes the available properties gesture from the current panel state', () => {
    expect(getToolPropertiesDoubleClickTooltip(false)).toBe('Double click to show properties');
    expect(getToolPropertiesDoubleClickTooltip(true)).toBe('Double click to hide properties');
  });
});
