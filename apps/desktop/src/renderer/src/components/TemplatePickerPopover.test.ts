// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { TemplatePickerPopover } from './TemplatePickerPopover';
import { loadTemplateLibrary } from './templateLibrary';

describe('TemplatePickerPopover', () => {
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

  it('selects a template, previews it, and creates from that template', async () => {
    const create = vi.fn(async () => undefined);
    const use = vi.fn();
    const storage = { getItem: () => null, setItem: () => undefined };
    act(() => root.render(createElement(TooltipProvider, null,
      createElement(TemplatePickerPopover, {
        library: loadTemplateLibrary(storage),
        onCreate: create,
        onManage: vi.fn(),
        onUseTemplate: use,
      }))));

    act(() => host.querySelector<HTMLButtonElement>('[data-testid="document-tab-template-picker"]')?.click());
    const grid = document.querySelector<HTMLButtonElement>('[data-testid="template-picker-item-built-in-grid"]');
    expect(grid).toBeTruthy();
    act(() => grid?.click());
    expect(document.querySelector('[data-testid="template-preview-card"]')?.textContent).toContain('Square Grid');
    await act(async () => document.querySelector<HTMLButtonElement>('[data-testid="template-picker-create"]')?.click());

    expect(create).toHaveBeenCalledWith(expect.objectContaining({ id: 'built-in-grid' }));
    expect(use).toHaveBeenCalledWith('built-in-grid');
  });
});
