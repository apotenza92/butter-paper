import { describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import {
  resolveGestureHintPresentation,
  TOOLBAR_ACTION_BUTTON_VARIANT,
  ViewerToolbar,
} from './ViewerToolbar';

function renderToolbar(cadViewEnabled: boolean, disabled = false): string {
  const doNothing = () => undefined;
  return renderToStaticMarkup(createElement(ViewerToolbar, {
    cadViewEnabled,
    disabled,
    zoom: 1,
    zoomPreset: 'manual',
    scrollMode: 'continuous',
    continuousScrollWheelMode: 'scroll',
    singlePageScrollWheelMode: 'zoom',
    pageColumnsEnabled: false,
    cadViewOrganisation: 'columns',
    pagesPerColumn: 10,
    onZoomOut: doNothing,
    onZoomReset: doNothing,
    onZoomIn: doNothing,
    onZoomChange: doNothing,
    onFitWidth: doNothing,
    onFitPage: doNothing,
    onScrollModeChange: doNothing,
    onContinuousScrollWheelModeChange: doNothing,
    onSinglePageScrollWheelModeChange: doNothing,
    onPageColumnsEnabledChange: doNothing,
    onCadViewOrganisationChange: doNothing,
    onPagesPerColumnChange: doNothing,
  }));
}

describe('viewer toolbar control semantics', () => {
  it('uses the neutral Nova button treatment for toolbar actions', () => {
    expect(TOOLBAR_ACTION_BUTTON_VARIANT).toBe('ghost');
  });

  it('omits the parked CAD View controls unless the feature is explicitly enabled', () => {
    expect(renderToolbar(false)).not.toContain('data-testid="viewer-cad-view"');
    expect(renderToolbar(true)).toContain('data-testid="viewer-cad-view"');
  });

  it('does not render a separate zoom reset button', () => {
    expect(renderToolbar(false)).not.toContain('data-testid="viewer-zoom-reset"');
  });

  it('does not show a selected view mode when no document is open', () => {
    const toolbar = renderToolbar(true, true);

    expect(toolbar).not.toContain('data-state="on"');
    expect(toolbar).not.toContain('aria-pressed="true"');
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
