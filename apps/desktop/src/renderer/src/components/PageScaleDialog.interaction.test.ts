// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import type { DocumentModel } from '@butter-paper/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PageScaleDialog } from './PageScaleDialog';

const DOCUMENT: DocumentModel = {
  id: 'document-1',
  path: '',
  metadata: {},
  pages: [
    {
      id: 'page-1',
      index: 0,
      size: { width: 612, height: 792 },
      rotation: 0,
    },
  ],
  markups: [],
};

describe('PageScaleDialog calibration interactions', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  });

  it('shows calibration fields before requesting point picking', () => {
    const onRequestCalibrationPick = vi.fn();
    const onClose = vi.fn();

    act(() => {
      root.render(
        createElement(PageScaleDialog, {
          document: DOCUMENT,
          currentPage: 0,
          onRequestCalibrationPick,
          onApply: vi.fn(),
          onClose,
        }),
      );
    });

    const calibrate = document.body.querySelector<HTMLButtonElement>('[data-testid="page-scale-method-calibrate"]');
    act(() => calibrate?.click());

    const dialog = document.body.querySelector('[data-testid="page-scale-dialog"]');
    const pickPoints = document.body.querySelector<HTMLButtonElement>('[data-testid="page-scale-pick-calibration"]');
    expect(dialog).toBeTruthy();
    expect(document.body.querySelector('[data-testid="page-scale-calibrate-real-length"]')).toBeTruthy();
    expect(onRequestCalibrationPick).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();

    act(() => pickPoints?.click());

    expect(onRequestCalibrationPick).toHaveBeenCalledOnce();
  });
});
