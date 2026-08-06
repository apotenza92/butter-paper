// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ColorPropertyField,
  NumericPropertyField,
  TypographyPropertyFields,
} from './PropertyControls';

describe('property controls', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it('keeps numeric steppers and units inside official Input Group addons', () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();

    act(() => root.render(createElement(NumericPropertyField, {
      label: 'Thickness', value: 10, step: 2, unit: 'pt', onChange, onCommit,
    })));

    const group = host.querySelector<HTMLElement>('[data-slot="input-group"]');
    expect(group).toBeTruthy();
    expect(group?.firstElementChild?.getAttribute('data-slot')).toBe('input-group-control');

    const buttons = Array.from(group?.querySelectorAll('button') ?? []);
    expect(buttons).toHaveLength(2);
    expect(buttons.every((button) => button.parentElement?.getAttribute('data-slot') === 'input-group-addon')).toBe(true);
    expect(group?.textContent).toContain('pt');

    act(() => host.querySelector<HTMLButtonElement>('[aria-label="Increase Thickness"]')?.click());
    expect(onChange).toHaveBeenCalledWith(12);
    expect(onCommit).toHaveBeenCalledWith(12);
  });

  it('pairs ranges with one compact value field and no competing step buttons', () => {
    act(() => root.render(createElement(NumericPropertyField, {
      label: 'Opacity', value: 75, min: 0, max: 100, unit: '%', slider: true, onChange: () => undefined,
    })));

    expect(host.querySelector('[data-slot="slider"]')).toBeTruthy();
    expect(host.querySelectorAll('[data-slot="slider-thumb"]')).toHaveLength(1);
    expect(host.querySelector('[aria-label="Increase Opacity"]')).toBeNull();
    expect(host.querySelector('[aria-label="Decrease Opacity"]')).toBeNull();
    expect(host.querySelector('[data-slot="input-group"]')?.textContent).toContain('%');
  });

  it('collapses colour presets behind one full-width popover trigger', () => {
    act(() => root.render(createElement(ColorPropertyField, {
      label: 'Color', value: '#3b82f6', onChange: () => undefined,
    })));

    const trigger = host.querySelector<HTMLButtonElement>('button');
    expect(trigger?.className).toContain('w-full');
    expect(trigger?.textContent).toBe('#3B82F6');
    expect(host.querySelector('[aria-label="Color presets"]')).toBeNull();
  });

  it('uses finite Toggle Groups for emphasis and mutually exclusive script styles', () => {
    act(() => root.render(createElement(TypographyPropertyFields, {
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
    })));

    const emphasis = host.querySelector('[aria-label="Font emphasis"]');
    const script = host.querySelector('[aria-label="Font script"]');
    expect(emphasis?.querySelectorAll('[data-slot="toggle-group-item"]')).toHaveLength(4);
    expect(script?.querySelectorAll('[data-slot="toggle-group-item"]')).toHaveLength(3);
    expect(host.querySelector<HTMLInputElement>('[role="combobox"]')?.value).toBe('Helvetica');
  });

  it('exposes mixed, disabled, and invalid state on the controlled input', () => {
    act(() => root.render(createElement(NumericPropertyField, {
      label: 'Width',
      value: 10,
      mixed: true,
      disabled: true,
      validation: 'Width is invalid',
      onChange: () => undefined,
    })));

    const input = host.querySelector<HTMLInputElement>('input[type="number"]');
    expect(input?.placeholder).toBe('Mixed');
    expect(input?.disabled).toBe(true);
    expect(input?.getAttribute('aria-invalid')).toBe('true');
    expect(host.textContent).toContain('Width is invalid');
  });
});
