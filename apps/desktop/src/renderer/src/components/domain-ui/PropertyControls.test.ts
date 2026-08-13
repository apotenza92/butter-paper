// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ColorPropertyField, FontSizePropertyField, NumericPropertyField, PropertyAccordion, PropertySection, TypographyPropertyFields } from './PropertyControls';

describe('property controls', () => {
  let host: HTMLDivElement;
  let root: Root;
  let originalGetAnimations: PropertyDescriptor | undefined;

  beforeEach(() => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    );
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    originalGetAnimations = Object.getOwnPropertyDescriptor(Element.prototype, 'getAnimations');
    Object.defineProperty(Element.prototype, 'getAnimations', {
      configurable: true,
      value: vi.fn(() => []),
    });
    window.localStorage.clear();
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    document.querySelectorAll('[data-slot="popover-content"]').forEach((element) => element.remove());
    vi.unstubAllGlobals();
    if (originalGetAnimations) Object.defineProperty(Element.prototype, 'getAnimations', originalGetAnimations);
    else Reflect.deleteProperty(Element.prototype, 'getAnimations');
  });

  it('uses a compact typed numeric field without step buttons', () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();

    act(() =>
      root.render(
        createElement(NumericPropertyField, {
          label: 'Thickness',
          value: 10,
          step: 2,
          unit: 'pt',
          onChange,
          onCommit,
        }),
      ),
    );

    const group = host.querySelector<HTMLElement>('[data-slot="input-group"]');
    expect(group).toBeTruthy();
    expect(group?.firstElementChild?.getAttribute('data-slot')).toBe('input-group-control');

    expect(group?.querySelectorAll('button')).toHaveLength(0);
    expect(group?.textContent).toContain('pt');

    const input = group?.querySelector<HTMLInputElement>('input');
    act(() => {
      input?.focus();
      input?.click();
    });
    expect(input?.selectionStart).toBe(0);
    expect(input?.selectionEnd).toBe(input?.value.length);

    act(() => {
      if (!input) throw new Error('Numeric input was not rendered.');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, '10.123456');
      input.dispatchEvent(new Event('input', { bubbles: true }));
    });
    expect(onChange).toHaveBeenLastCalledWith(10.123456);

    act(() => input?.dispatchEvent(new FocusEvent('focusout', { bubbles: true })));
    expect(onCommit).toHaveBeenLastCalledWith(10.123456);
    expect(input?.value).toBe('10');
  });

  it('pairs ranges with one compact value field and no competing step buttons', () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();
    act(() =>
      root.render(
        createElement(NumericPropertyField, {
          label: 'Opacity',
          value: 75,
          min: 0,
          max: 100,
          unit: '%',
          slider: true,
          onChange,
          onCommit,
        }),
      ),
    );

    expect(host.querySelector('[data-slot="slider"]')).toBeTruthy();
    expect(host.querySelectorAll('[data-slot="slider-thumb"]')).toHaveLength(1);
    expect(host.querySelector('[aria-label="Increase Opacity"]')).toBeNull();
    expect(host.querySelector('[aria-label="Decrease Opacity"]')).toBeNull();
    expect(host.querySelector('[data-slot="input-group"]')?.textContent).toContain('%');
    expect(host.querySelector('[data-slot="field"]')?.className).toContain('col-span-2');

    const wheelArea = host.querySelector<HTMLElement>('[data-slider-wheel-area]');
    expect(wheelArea?.className).toContain('min-h-8');
    act(() => wheelArea?.dispatchEvent(new WheelEvent('wheel', { deltaY: -1, bubbles: true, cancelable: true })));
    expect(onChange).toHaveBeenLastCalledWith(76);
    expect(onCommit).toHaveBeenLastCalledWith(76);

    act(() => wheelArea?.dispatchEvent(new WheelEvent('wheel', { deltaX: -1, bubbles: true, cancelable: true })));
    expect(onChange).toHaveBeenLastCalledWith(74);
    expect(onCommit).toHaveBeenLastCalledWith(74);
  });

  it('removes floating-point residue from displayed opacity percentages', () => {
    act(() =>
      root.render(
        createElement(NumericPropertyField, {
          label: 'Opacity',
          value: 55.00000000000001,
          min: 0,
          max: 100,
          unit: '%',
          slider: true,
          onChange: () => undefined,
        }),
      ),
    );

    expect(host.querySelector<HTMLInputElement>('input[inputmode="decimal"]')?.value).toBe('55');
  });

  it('shows the current hex colour in a full-width property trigger', () => {
    act(() =>
      root.render(
        createElement(ColorPropertyField, {
          label: 'Color',
          value: '#0000ff',
          onChange: () => undefined,
        }),
      ),
    );

    const trigger = host.querySelector<HTMLButtonElement>('button');
    expect(trigger?.className).toContain('w-full');
    expect(trigger?.className).toContain('justify-start');
    expect(trigger?.textContent).toBe('#0000FF');
    expect(trigger?.getAttribute('aria-label')).toBe('Color: #0000FF');
    expect(host.querySelector('[aria-label="Color presets"]')).toBeNull();
  });

  it('offers common font sizes and accepts a custom typed size', () => {
    const onChange = vi.fn();
    act(() =>
      root.render(
        createElement(FontSizePropertyField, {
          label: 'Font Size',
          value: 12,
          onChange,
        }),
      ),
    );

    const input = host.querySelector<HTMLInputElement>('[aria-label="Font size"]');
    expect(input?.value).toBe('12 pt');
    expect(host.querySelector('[data-slot="slider"]')).toBeNull();
    expect(host.querySelector('[aria-label="Increase Font Size"]')).toBeNull();

    act(() => input?.click());
    expect(input?.selectionStart).toBe(0);
    expect(input?.selectionEnd).toBe(input?.value.length);

    act(() => {
      if (!input) throw new Error('Font size input was not rendered.');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, '13.5');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    });
    expect(onChange).toHaveBeenLastCalledWith(13.5);
  });

  it('renders one flat full-width tool accordion that starts open', async () => {
    act(() =>
      root.render(
        createElement(PropertyAccordion, {
          title: 'Text Box',
          children: createElement('span', null, 'Text controls'),
        }),
      ),
    );

    const section = host.querySelector<HTMLElement>('[data-slot="accordion"]');
    const item = host.querySelector<HTMLElement>('[data-slot="accordion-item"]');
    const trigger = host.querySelector<HTMLButtonElement>('[data-slot="accordion-trigger"]');
    expect(section?.className).toContain('w-full');
    expect(item?.className).toContain('border-b');
    expect(trigger?.className).toContain('rounded-none');
    expect(trigger?.className).toContain('border-0');
    expect(trigger?.className).toContain('hover:no-underline');
    expect(trigger?.textContent).toContain('Text Box');
    expect(trigger?.getAttribute('aria-expanded')).toBe('true');

    act(() => trigger?.click());
    await act(async () => {
      await Promise.resolve();
    });
    expect(trigger?.getAttribute('aria-expanded')).toBe('false');
  });

  it('renders each property group as a full-width collapsible row', async () => {
    act(() =>
      root.render(
        createElement(PropertySection, {
          title: 'Appearance',
          children: createElement('span', null, 'Appearance controls'),
        }),
      ),
    );

    const trigger = host.querySelector<HTMLButtonElement>('[data-slot="accordion-trigger"]');
    expect(trigger?.textContent?.trim()).toBe('Appearance');
    expect(trigger?.className).toContain('text-sm');
    expect(trigger?.className).toContain('font-medium');
    expect(trigger?.getAttribute('aria-expanded')).toBe('true');
    expect(host.querySelector('[data-slot="accordion-item"]')?.className).toContain('border-b');
    expect(host.querySelector('[data-slot="field-legend"]')?.className).toContain('sr-only');
    expect(host.querySelector('[data-slot="field-group"]')?.className).toContain('grid-cols-2');
    expect(host.querySelector('[data-slot="separator"]')).toBeNull();

    act(() => trigger?.click());
    await act(async () => {
      await Promise.resolve();
    });
    expect(trigger?.getAttribute('aria-expanded')).toBe('false');
  });

  it('adds a custom preset from the embedded picker with explicit add and cancel actions', async () => {
    const onChange = vi.fn();
    act(() =>
      root.render(
        createElement(ColorPropertyField, {
          label: 'Color',
          value: '#3b82f6',
          onChange,
        }),
      ),
    );

    act(() => host.querySelector<HTMLButtonElement>('button')?.click());
    await act(async () => {
      await Promise.resolve();
    });
    const defaultPresets = document.querySelector<HTMLElement>('[aria-label="Color default presets"]');
    expect(defaultPresets?.className).toContain('grid-cols-6');
    expect(defaultPresets?.querySelectorAll('button')).toHaveLength(12);
    expect(defaultPresets?.querySelector('button')?.className).toContain('size-8');
    expect(defaultPresets?.querySelector('[aria-label="Use #000000"] span')?.className).toContain('ring-border');
    expect(defaultPresets?.querySelector('[aria-label="Use #ffffff"] span')?.className).toContain('ring-border');
    expect(document.querySelector('[data-slot="separator"]')).toBeTruthy();
    expect(document.body.textContent).toContain('Custom colors');

    act(() => document.querySelector<HTMLButtonElement>('[aria-label="Use #0000ff"]')?.focus());
    await act(async () => {
      await Promise.resolve();
    });
    const tooltip = [...document.querySelectorAll('[data-slot="tooltip-content"]')].find((candidate) => candidate.textContent?.includes('#0000FF'));
    expect(tooltip?.textContent).toContain('HEX#0000FF');
    expect(tooltip?.textContent).toContain('RGB0, 0, 255');
    expect(tooltip?.textContent).toContain('HSL240°, 100%, 50%');

    act(() => document.querySelector<HTMLButtonElement>('[aria-label="Use #000000"]')?.focus());
    await act(async () => {
      await Promise.resolve();
    });
    const blackTooltip = [...document.querySelectorAll('[data-slot="tooltip-content"]')].find((candidate) => candidate.textContent?.includes('#000000'));
    expect(blackTooltip?.querySelector('[aria-hidden="true"]')?.className).toContain('ring-background/50');

    act(() => [...document.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent?.trim() === 'Add')?.click());
    expect(document.querySelector('.react-colorful')).toBeTruthy();
    expect([...document.querySelectorAll<HTMLButtonElement>('button')].some((button) => button.textContent?.trim() === 'Cancel')).toBe(true);
    expect(document.querySelector('[aria-label="Color format"]')?.querySelectorAll('[data-slot="toggle-group-item"]')).toHaveLength(3);

    act(() => [...document.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent?.trim() === 'RGB')?.click());
    expect(document.querySelector('[aria-label="Custom preset R"]')).toBeTruthy();
    expect(document.querySelector('[aria-label="Custom preset G"]')).toBeTruthy();
    expect(document.querySelector('[aria-label="Custom preset B"]')).toBeTruthy();

    act(() => [...document.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent?.trim() === 'Hex')?.click());

    act(() => {
      const hexInput = document.querySelector<HTMLInputElement>('[aria-label="Custom preset hex color"]');
      if (!hexInput) throw new Error('Custom preset hex input was not rendered.');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(hexInput, '#123456');
      hexInput.dispatchEvent(new Event('input', { bubbles: true }));
    });

    expect(onChange).not.toHaveBeenCalledWith('#123456');
    expect(window.localStorage.getItem('butter-paper.color-presets.v1')).toBeNull();
    act(() => [...document.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent?.trim() === 'Add')?.click());

    expect(onChange).toHaveBeenLastCalledWith('#123456');
    expect(window.localStorage.getItem('butter-paper.color-presets.v1')).toContain('#123456');
    expect(document.querySelector('[aria-label="Use #123456"]')).toBeTruthy();
  });

  it('provides a context menu on saved presets', async () => {
    window.localStorage.setItem('butter-paper.color-presets.v1', '["#123456"]');
    act(() =>
      root.render(
        createElement(ColorPropertyField, {
          label: 'Color',
          value: '#0000ff',
          onChange: () => undefined,
        }),
      ),
    );

    act(() => host.querySelector<HTMLButtonElement>('button')?.click());
    await act(async () => {
      await Promise.resolve();
    });

    expect(document.querySelectorAll('[data-color-context-menu-trigger]')).toHaveLength(1);
    expect(document.querySelector('[aria-label="Use #123456"][data-color-context-menu-trigger][data-slot="tooltip-trigger"]')).toBeTruthy();
    expect(document.querySelector('[aria-label="Use #0000ff"][data-color-context-menu-trigger]')).toBeNull();
  });

  it('uses finite Toggle Groups for emphasis and mutually exclusive script styles', () => {
    act(() =>
      root.render(
        createElement(TypographyPropertyFields, {
          value: {
            font: 'Helvetica',
            size: 12,
            alignment: 'left',
            verticalAlignment: 'top',
            styles: ['bold'],
            lineSpacing: 1,
            margin: 3,
            autoSize: false,
          },
          onChange: () => undefined,
        }),
      ),
    );

    const emphasis = host.querySelector('[aria-label="Font emphasis"]');
    const script = host.querySelector('[aria-label="Font script"]');
    expect(emphasis?.querySelectorAll('[data-slot="toggle-group-item"]')).toHaveLength(4);
    expect(script?.querySelectorAll('[data-slot="toggle-group-item"]')).toHaveLength(3);
    expect(host.querySelector<HTMLInputElement>('[role="combobox"]')?.value).toBe('Helvetica');
  });

  it('places text color and font size in one compact row', () => {
    act(() =>
      root.render(
        createElement(TypographyPropertyFields, {
          value: {
            font: 'Helvetica',
            size: 12,
            alignment: 'left',
            verticalAlignment: 'top',
            styles: [],
            lineSpacing: 1,
            margin: 3,
            autoSize: false,
          },
          color: '#ff0000',
          onChange: () => undefined,
          onColorChange: () => undefined,
        }),
      ),
    );

    const textSection = host.querySelector('[data-slot="field-group"]');
    expect(textSection?.className).toContain('grid-cols-2');
    expect(textSection?.querySelector('[aria-label="Color: #FF0000"]')).toBeTruthy();
    expect(textSection?.querySelector('[aria-label="Font size"]')).toBeTruthy();
    expect(textSection?.querySelector('[aria-label="Font size"]')?.closest('[data-slot="field"]')?.previousElementSibling?.querySelector('[aria-label="Color: #FF0000"]')).toBeTruthy();
  });

  it('exposes mixed, disabled, and invalid state on the controlled input', () => {
    act(() =>
      root.render(
        createElement(NumericPropertyField, {
          label: 'Width',
          value: 10,
          mixed: true,
          disabled: true,
          validation: 'Width is invalid',
          onChange: () => undefined,
        }),
      ),
    );

    const input = host.querySelector<HTMLInputElement>('input[inputmode="decimal"]');
    expect(input?.placeholder).toBe('Mixed');
    expect(input?.disabled).toBe(true);
    expect(input?.getAttribute('aria-invalid')).toBe('true');
    expect(host.textContent).toContain('Width is invalid');
  });
});
