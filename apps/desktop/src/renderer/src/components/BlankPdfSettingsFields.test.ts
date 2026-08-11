// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DEFAULT_BLANK_PDF_SETTINGS } from './blankPdfSettings';
import { BlankPdfSettingsFields } from './BlankPdfSettingsFields';

describe('blank PDF settings fields', () => {
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

  it('lists A5 first and continues through A0 before Custom', () => {
    act(() => root.render(createElement(BlankPdfSettingsFields, {
      settings: DEFAULT_BLANK_PDF_SETTINGS,
      error: null,
      testIdPrefix: 'blank-pdf',
      onSettingsChange: () => undefined,
    })));

    expect(Array.from(host.querySelectorAll('option'), (option) => option.value)).toEqual([
      'a5',
      'a4',
      'a3',
      'a2',
      'a1',
      'a0',
      'custom',
    ]);
  });

  it('keeps a trailing checkmark slot and shows it on the selected orientation', () => {
    const renderFields = (orientation: 'portrait' | 'landscape') => {
      act(() => root.render(createElement(BlankPdfSettingsFields, {
        settings: { ...DEFAULT_BLANK_PDF_SETTINGS, orientation },
        error: null,
        testIdPrefix: 'blank-pdf',
        onSettingsChange: () => undefined,
      })));
    };

    renderFields('landscape');
    expect(host.querySelector('[data-testid="blank-pdf-portrait"] svg')?.getAttribute('visibility')).toBe('hidden');
    expect(host.querySelector('[data-testid="blank-pdf-landscape"] svg')?.getAttribute('visibility')).toBe('visible');

    renderFields('portrait');
    expect(host.querySelector('[data-testid="blank-pdf-portrait"] svg')?.getAttribute('visibility')).toBe('visible');
    expect(host.querySelector('[data-testid="blank-pdf-landscape"] svg')?.getAttribute('visibility')).toBe('hidden');
  });

  it('shows spacing and alphabetized colour controls for patterned paper', () => {
    act(() => root.render(createElement(BlankPdfSettingsFields, {
      settings: {
        ...DEFAULT_BLANK_PDF_SETTINGS,
        patternType: 'grid',
      },
      error: null,
      testIdPrefix: 'blank-pdf',
      onSettingsChange: () => undefined,
    })));

    expect(host.querySelector('[data-testid="blank-pdf-background-grid"]')?.getAttribute('aria-pressed')).toBe('true');
    expect((host.querySelector('[data-testid="blank-pdf-pattern-spacing"]') as HTMLSelectElement).value).toBe('10');
    const colourSelect = host.querySelector('[data-testid="blank-pdf-pattern-colour-preset"]') as HTMLSelectElement;
    expect(colourSelect.value).toBe('grey');
    expect(Array.from(colourSelect.options, (option) => option.textContent)).toEqual([
      'Black',
      'Grey',
      'Light blue',
      'Custom',
    ]);
    expect(host.querySelector('[data-testid="blank-pdf-custom-pattern-spacing"]')).toBeNull();
    expect(host.querySelector('[data-testid="blank-pdf-custom-pattern-colour"]')).toBeNull();

    act(() => root.render(createElement(BlankPdfSettingsFields, {
      settings: {
        ...DEFAULT_BLANK_PDF_SETTINGS,
        patternType: 'dots',
        patternSpacingPreset: 'custom',
        patternColorPreset: 'custom',
      },
      error: null,
      testIdPrefix: 'blank-pdf',
      onSettingsChange: () => undefined,
    })));

    expect(host.querySelector('[data-testid="blank-pdf-custom-pattern-spacing"]')).toBeTruthy();
    expect((host.querySelector('[data-testid="blank-pdf-custom-pattern-colour"]') as HTMLInputElement).value).toBe('#808080');
  });
});
