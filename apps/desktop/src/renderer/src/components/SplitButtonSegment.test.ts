import { describe, expect, it } from 'vitest';
import { resolveSplitButtonSegmentSurface } from './domain-ui/SplitButtonSegment';

describe('resolveSplitButtonSegmentSurface', () => {
  it('keeps an unselected popup segment on the surrounding surface', () => {
    const surface = resolveSplitButtonSegmentSurface(false);

    expect(surface).toContain('bg-transparent!');
    expect(surface).toContain('aria-expanded:bg-transparent!');
    expect(surface).toContain('dark:bg-transparent!');
    expect(surface).toContain('dark:aria-expanded:bg-transparent!');
  });

  it('shares the selected surface with its adjacent view toggle', () => {
    const surface = resolveSplitButtonSegmentSurface(true);

    expect(surface).toContain('bg-muted!');
    expect(surface).toContain('aria-expanded:bg-muted!');
    expect(surface).toContain('dark:bg-muted!');
    expect(surface).toContain('dark:aria-expanded:bg-muted!');
  });
});
