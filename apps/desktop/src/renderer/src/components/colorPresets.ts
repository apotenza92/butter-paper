export const DEFAULT_COLOR_PRESETS = ['#000000', '#808080', '#ff0000', '#ffa500', '#ffff00', '#00ff00', '#008080', '#00ffff', '#0000ff', '#800080', '#ff00ff', '#ffffff'] as const;

const LEGACY_DEFAULT_COLOR_PRESETS = ['#6b7280', '#ef4444', '#f97316', '#facc15', '#22c55e', '#14b8a6', '#06b6d4', '#3b82f6', '#8b5cf6', '#ec4899'] as const;

export const COLOR_PRESETS_STORAGE_KEY = 'butter-paper.color-presets.v1';

interface ColorPresetStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

export function normalizeHexColor(value: string): string | null {
  const candidate = value.trim().toLowerCase();
  if (/^#[0-9a-f]{6}$/.test(candidate)) return candidate;
  if (/^#[0-9a-f]{3}$/.test(candidate)) {
    return `#${candidate
      .slice(1)
      .split('')
      .map((digit) => `${digit}${digit}`)
      .join('')}`;
  }
  return null;
}

export interface RgbColor {
  r: number;
  g: number;
  b: number;
}

export interface HslColor {
  h: number;
  s: number;
  l: number;
}

export function hexToRgb(value: string): RgbColor | null {
  const hex = normalizeHexColor(value);
  if (!hex) return null;
  return {
    r: Number.parseInt(hex.slice(1, 3), 16),
    g: Number.parseInt(hex.slice(3, 5), 16),
    b: Number.parseInt(hex.slice(5, 7), 16),
  };
}

export function rgbToHex({ r, g, b }: RgbColor): string {
  return `#${[r, g, b].map((channel) => Math.round(clamp(channel, 0, 255)).toString(16).padStart(2, '0')).join('')}`;
}

export function hexToHsl(value: string): HslColor | null {
  const rgb = hexToRgb(value);
  if (!rgb) return null;
  const r = rgb.r / 255;
  const g = rgb.g / 255;
  const b = rgb.b / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  const lightness = (max + min) / 2;
  let hue = 0;
  if (delta !== 0) {
    if (max === r) hue = ((g - b) / delta) % 6;
    else if (max === g) hue = (b - r) / delta + 2;
    else hue = (r - g) / delta + 4;
    hue *= 60;
    if (hue < 0) hue += 360;
  }
  const saturation = delta === 0 ? 0 : delta / (1 - Math.abs(2 * lightness - 1));
  return { h: Math.round(hue), s: Math.round(saturation * 100), l: Math.round(lightness * 100) };
}

export function hslToHex({ h, s, l }: HslColor): string {
  const hue = ((h % 360) + 360) % 360;
  const saturation = clamp(s, 0, 100) / 100;
  const lightness = clamp(l, 0, 100) / 100;
  const chroma = (1 - Math.abs(2 * lightness - 1)) * saturation;
  const segment = hue / 60;
  const x = chroma * (1 - Math.abs((segment % 2) - 1));
  const [r1, g1, b1] = segment < 1 ? [chroma, x, 0] : segment < 2 ? [x, chroma, 0] : segment < 3 ? [0, chroma, x] : segment < 4 ? [0, x, chroma] : segment < 5 ? [x, 0, chroma] : [chroma, 0, x];
  const match = lightness - chroma / 2;
  return rgbToHex({ r: (r1 + match) * 255, g: (g1 + match) * 255, b: (b1 + match) * 255 });
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, Number.isFinite(value) ? value : min));
}

export function loadColorPresets(storage: Pick<ColorPresetStorage, 'getItem'>): string[] {
  try {
    const stored = storage.getItem(COLOR_PRESETS_STORAGE_KEY);
    if (!stored) return [...DEFAULT_COLOR_PRESETS];
    const parsed = JSON.parse(stored) as unknown;
    if (!Array.isArray(parsed)) return [...DEFAULT_COLOR_PRESETS];
    return withDefaultColorPresets(parsed);
  } catch {
    return [...DEFAULT_COLOR_PRESETS];
  }
}

export function saveColorPresets(storage: Pick<ColorPresetStorage, 'setItem'>, colors: readonly string[]): string[] {
  const normalized = withDefaultColorPresets(colors);
  storage.setItem(COLOR_PRESETS_STORAGE_KEY, JSON.stringify(normalized));
  return normalized;
}

function withDefaultColorPresets(values: readonly unknown[]): string[] {
  const defaults = new Set<string>(DEFAULT_COLOR_PRESETS);
  const replacedDefaults = new Set<string>(LEGACY_DEFAULT_COLOR_PRESETS);
  const custom = [...new Set(values.flatMap((value) => (typeof value === 'string' ? normalizeHexColor(value) ?? [] : [])))].filter((color) => !defaults.has(color) && !replacedDefaults.has(color));
  return [...DEFAULT_COLOR_PRESETS, ...custom].slice(0, 32);
}
