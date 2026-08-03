import { describe, expect, it } from 'vitest';
import { resolveRailTooltipAnchor } from './RailScrollArea';

describe('rail tooltip anchoring', () => {
  it('anchors each tooltip to its actual button column', () => {
    const rootRect = { left: 900, top: 120 };
    const firstColumn = resolveRailTooltipAnchor(
      { left: 908, top: 144, width: 32, height: 32 },
      rootRect,
    );
    const secondColumn = resolveRailTooltipAnchor(
      { left: 948, top: 144, width: 32, height: 32 },
      rootRect,
    );

    expect(firstColumn).toEqual({ left: 8, top: 40, width: 32 });
    expect(secondColumn).toEqual({ left: 48, top: 40, width: 32 });
  });
});
