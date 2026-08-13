// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DEFAULT_BLANK_PDF_SETTINGS } from '../blankPdfSettings';
import { BlankPdfPagePreview } from './BlankPdfPagePreview';

describe('blank PDF page preview', () => {
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

  it('shows the default paper label, dimensions, and landscape aspect ratio', () => {
    act(() => root.render(createElement(BlankPdfPagePreview, {
      settings: DEFAULT_BLANK_PDF_SETTINGS,
    })));

    const frame = host.querySelector<HTMLElement>('[data-testid="blank-pdf-page-preview-frame"]');
    const preview = host.querySelector<HTMLElement>('[data-testid="blank-pdf-page-preview"]');
    const artwork = host.querySelector<SVGSVGElement>('[data-testid="blank-pdf-page-preview-artwork"]');
    const paperLabel = host.querySelector<HTMLElement>('[data-testid="blank-pdf-page-preview-paper-label"]');
    const widthLabel = host.querySelector<HTMLElement>('[data-testid="blank-pdf-page-preview-width-label"]');
    const heightLabel = host.querySelector<HTMLElement>('[data-testid="blank-pdf-page-preview-height-label"]');
    expect(preview?.dataset.paperLabel).toBe('A3');
    expect(preview?.dataset.widthMm).toBe('420');
    expect(preview?.dataset.heightMm).toBe('297');
    expect(preview?.dataset.pattern).toBe('blank');
    expect(frame?.className).toContain('h-48');
    expect(frame?.className).not.toContain('aspect-square');
    expect(frame?.style.containerType).toBe('size');
    expect(preview?.style.width).toBe('var(--blank-pdf-preview-width)');
    expect(preview?.style.height).toBe('var(--blank-pdf-preview-height)');
    expect(preview?.style.getPropertyValue('--blank-pdf-preview-width')).toBe(`min(100cqw, ${(420 / 297) * 100}cqh)`);
    expect(preview?.style.getPropertyValue('--blank-pdf-preview-height')).toBe(`min(100cqh, ${100 / (420 / 297)}cqw)`);
    expect(preview?.style.aspectRatio).toBe('420 / 297');
    expect(artwork?.getAttribute('viewBox')).toBe('0 0 420 297');
    expect(artwork?.getAttribute('preserveAspectRatio')).toBe('xMidYMid meet');
    expect(paperLabel?.textContent).toBe('A3');
    expect(widthLabel?.textContent).toBe('420 mm wide');
    expect(heightLabel?.textContent).toBe('297 mm high');
    expect(heightLabel?.className).toContain('left-1');
    expect(heightLabel?.className).not.toContain('-rotate-90');
    expect(heightLabel?.style.writingMode).toBe('vertical-rl');
    for (const label of [paperLabel, widthLabel, heightLabel]) {
      expect(label?.className).toContain('text-xs');
    }
  });

  it('renders the selected pattern, spacing, colour, and portrait dimensions', () => {
    act(() => root.render(createElement(BlankPdfPagePreview, {
      settings: {
        ...DEFAULT_BLANK_PDF_SETTINGS,
        preset: 'a5',
        orientation: 'portrait',
        patternType: 'triangle',
        patternSpacingPreset: '25',
        patternColorPreset: 'blue',
      },
    })));

    const preview = host.querySelector<HTMLElement>('[data-testid="blank-pdf-page-preview"]');
    expect(preview?.dataset.paperLabel).toBe('A5');
    expect(preview?.dataset.widthMm).toBe('148');
    expect(preview?.dataset.heightMm).toBe('210');
    expect(preview?.dataset.pattern).toBe('triangle');
    expect(preview?.dataset.spacingMm).toBe('25');
    expect(preview?.dataset.patternColor).toBe('#4e95cc');
    expect(preview?.style.width).toBe('var(--blank-pdf-preview-width)');
    expect(preview?.style.height).toBe('var(--blank-pdf-preview-height)');
    expect(preview?.style.getPropertyValue('--blank-pdf-preview-width')).toBe(`min(100cqw, ${(148 / 210) * 100}cqh)`);
    expect(preview?.style.getPropertyValue('--blank-pdf-preview-height')).toBe(`min(100cqh, ${100 / (148 / 210)}cqw)`);
    expect(preview?.style.aspectRatio).toBe('148 / 210');
    expect(host.querySelector('[data-testid="blank-pdf-page-preview-paper-label"]')?.textContent).toBe('A5');
    expect(host.querySelector('[data-testid="blank-pdf-page-preview-width-label"]')?.textContent).toBe('148 mm wide');
    expect(host.querySelector('[data-testid="blank-pdf-page-preview-height-label"]')?.textContent).toBe('210 mm high');
    expect(host.querySelector('pattern')).toBeTruthy();
    expect(host.querySelector('[data-testid="blank-pdf-page-preview-pattern"]')?.getAttribute('fill')).toMatch(/^url\(#blank-pdf-preview-/);
  });

  it('updates the rendered spacing and colour without changing the fitted page', () => {
    const renderPreview = (patternSpacingPreset: '5' | '25', patternColorPreset: 'black' | 'blue') => {
      act(() => root.render(createElement(BlankPdfPagePreview, {
        settings: {
          ...DEFAULT_BLANK_PDF_SETTINGS,
          patternType: 'grid',
          patternSpacingPreset,
          patternColorPreset,
        },
      })));
    };

    renderPreview('5', 'black');
    expect(host.querySelector('pattern')?.getAttribute('width')).toBe('5');
    expect(host.querySelector('pattern path')?.getAttribute('stroke')).toBe('#000000');

    renderPreview('25', 'blue');
    expect(host.querySelector('pattern')?.getAttribute('width')).toBe('25');
    expect(host.querySelector('pattern path')?.getAttribute('stroke')).toBe('#4e95cc');
    expect(host.querySelector('[data-testid="blank-pdf-page-preview-artwork"]')?.getAttribute('preserveAspectRatio')).toBe('xMidYMid meet');
  });

  it('uses exaggerated artwork only in the live preview', () => {
    act(() => root.render(createElement(BlankPdfPagePreview, {
      settings: {
        ...DEFAULT_BLANK_PDF_SETTINGS,
        patternType: 'dots',
        patternSpacingPreset: '10',
      },
    })));

    expect(host.querySelector('pattern circle')?.getAttribute('r')).toBe('1.1');

    act(() => root.render(createElement(BlankPdfPagePreview, {
      settings: {
        ...DEFAULT_BLANK_PDF_SETTINGS,
        patternType: 'grid',
      },
    })));
    expect(host.querySelector('pattern path')?.getAttribute('stroke-width')).toBe('1.5');
  });
});
