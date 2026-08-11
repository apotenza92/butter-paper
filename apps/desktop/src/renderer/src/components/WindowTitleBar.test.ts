import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { formatWindowTitle, WindowTitleBar } from './WindowTitleBar';

describe('formatWindowTitle', () => {
  it('uses the active document name and preserves the application identity', () => {
    expect(formatWindowTitle('Butter Paper Beta', 'drawing.pdf')).toBe('drawing.pdf — Butter Paper Beta');
  });

  it('shows the number of other open tabs', () => {
    expect(formatWindowTitle('Butter Paper Beta', 'drawing.pdf', 12)).toBe('drawing.pdf (+12) — Butter Paper Beta');
  });

  it('uses the application title when no document is open', () => {
    expect(formatWindowTitle('Butter Paper')).toBe('Butter Paper');
  });
});

describe('WindowTitleBar', () => {
  it('renders the application title in the draggable title-bar surface', () => {
    const markup = renderToStaticMarkup(createElement(WindowTitleBar, { title: 'Butter Paper Beta' }));

    expect(markup).toContain('bp-window-titlebar');
    expect(markup).toContain('Butter Paper Beta');
    expect(markup).toContain('data-testid="window-title-bar-separator"');
    expect(markup).toContain('inset-x-0');
  });
});
