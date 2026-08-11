import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { DimensionRailIcon, LengthRailIcon } from './RailIcons';

describe('measurement rail icons', () => {
  it('uses the precise dimension-line icon for Length and the ruler icon for Dimension', () => {
    const lengthIcon = renderToStaticMarkup(createElement(LengthRailIcon));
    const dimensionIcon = renderToStaticMarkup(createElement(DimensionRailIcon));

    expect(lengthIcon).toContain('lucide-ruler-dimension-line');
    expect(dimensionIcon).toContain('lucide-ruler');
    expect(dimensionIcon).not.toContain('lucide-ruler-dimension-line');
  });
});
