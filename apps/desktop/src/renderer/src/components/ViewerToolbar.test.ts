import { describe, expect, it } from 'vitest';
import { resolveGestureHintPresentation, TOOLBAR_ACTION_BUTTON_VARIANT } from './ViewerToolbar';

describe('viewer toolbar control semantics', () => {
  it('uses the neutral Nova button treatment for toolbar actions', () => {
    expect(TOOLBAR_ACTION_BUTTON_VARIANT).toBe('ghost');
  });
});

describe('viewer toolbar gesture hints', () => {
  it('suppresses the ordinary tooltip after the gesture hint times out', () => {
    expect(resolveGestureHintPresentation({
      id: 'continuous',
      text: 'Double click to Fit Width',
      visible: false,
    }, 'continuous')).toEqual({
      hint: undefined,
      suppressTooltip: true,
    });
  });

  it('shows only the current visible gesture hint', () => {
    const hint = {
      id: 'continuous',
      text: 'Double click to Fit Width',
      visible: true,
    };

    expect(resolveGestureHintPresentation(hint, 'continuous')).toEqual({
      hint: 'Double click to Fit Width',
      suppressTooltip: false,
    });
    expect(resolveGestureHintPresentation(hint, 'single-page')).toEqual({
      hint: undefined,
      suppressTooltip: false,
    });
  });
});
