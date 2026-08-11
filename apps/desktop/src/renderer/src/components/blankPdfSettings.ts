import type { BlankPdfCreateRequest } from '../../../shared/protocol';

export type BlankPdfPaperPreset = 'a5' | 'a4' | 'a3' | 'a2' | 'a1' | 'a0' | 'custom';
export type BlankPdfOrientation = 'portrait' | 'landscape';
export type BlankPdfPatternType = 'blank' | 'dots' | 'grid' | 'lined' | 'isometric' | 'triangle';
export type BlankPdfPatternSpacingPreset = '5' | '10' | '25' | 'custom';
export type BlankPdfPatternColorPreset = 'grey' | 'black' | 'blue' | 'custom';

export const BLANK_PDF_PATTERN_COLORS = {
  grey: '#d1d5db',
  black: '#000000',
  blue: '#4e95cc',
} as const;
export const DEFAULT_CUSTOM_BLANK_PDF_PATTERN_COLOR = '#808080';

export interface BlankPdfSettings {
  preset: BlankPdfPaperPreset;
  orientation: BlankPdfOrientation;
  customWidth: string;
  customHeight: string;
  patternType: BlankPdfPatternType;
  patternSpacingPreset: BlankPdfPatternSpacingPreset;
  customPatternSpacing: string;
  patternColorPreset: BlankPdfPatternColorPreset;
  customPatternColor: string;
}

interface BlankPdfSettingsStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

export const BLANK_PDF_SETTINGS_STORAGE_KEY = 'butter-paper.blank-pdf-settings.v1';

export const BLANK_PDF_PAPER_PRESETS = {
  a5: { label: 'A5', widthMm: 148, heightMm: 210 },
  a4: { label: 'A4', widthMm: 210, heightMm: 297 },
  a3: { label: 'A3', widthMm: 297, heightMm: 420 },
  a2: { label: 'A2', widthMm: 420, heightMm: 594 },
  a1: { label: 'A1', widthMm: 594, heightMm: 841 },
  a0: { label: 'A0', widthMm: 841, heightMm: 1189 },
} as const;

export const DEFAULT_BLANK_PDF_SETTINGS: BlankPdfSettings = {
  preset: 'a3',
  orientation: 'landscape',
  customWidth: '420',
  customHeight: '297',
  patternType: 'blank',
  patternSpacingPreset: '10',
  customPatternSpacing: '10',
  patternColorPreset: 'grey',
  customPatternColor: DEFAULT_CUSTOM_BLANK_PDF_PATTERN_COLOR,
};

const PAPER_PRESET_VALUES = new Set<BlankPdfPaperPreset>([
  ...Object.keys(BLANK_PDF_PAPER_PRESETS) as Array<keyof typeof BLANK_PDF_PAPER_PRESETS>,
  'custom',
]);
const PATTERN_TYPE_VALUES = new Set<BlankPdfPatternType>(['blank', 'dots', 'grid', 'lined', 'isometric', 'triangle']);
const PATTERN_SPACING_PRESET_VALUES = new Set<BlankPdfPatternSpacingPreset>(['5', '10', '25', 'custom']);
const PATTERN_COLOR_PRESET_VALUES = new Set<BlankPdfPatternColorPreset>(['grey', 'black', 'blue', 'custom']);

export function loadBlankPdfSettings(storage: BlankPdfSettingsStorage): BlankPdfSettings {
  const stored = storage.getItem(BLANK_PDF_SETTINGS_STORAGE_KEY);
  if (!stored) return { ...DEFAULT_BLANK_PDF_SETTINGS };

  try {
    const candidate = JSON.parse(stored) as Partial<BlankPdfSettings>;
    const settings = { ...DEFAULT_BLANK_PDF_SETTINGS, ...candidate } as BlankPdfSettings;
    if (
      !PAPER_PRESET_VALUES.has(settings.preset)
      || (settings.orientation !== 'portrait' && settings.orientation !== 'landscape')
      || typeof settings.customWidth !== 'string'
      || typeof settings.customHeight !== 'string'
      || !PATTERN_TYPE_VALUES.has(settings.patternType)
      || !PATTERN_SPACING_PRESET_VALUES.has(settings.patternSpacingPreset)
      || typeof settings.customPatternSpacing !== 'string'
      || !PATTERN_COLOR_PRESET_VALUES.has(settings.patternColorPreset)
      || typeof settings.customPatternColor !== 'string'
    ) {
      return { ...DEFAULT_BLANK_PDF_SETTINGS };
    }

    resolveBlankPdfDimensions(settings);
    return settings;
  } catch {
    return { ...DEFAULT_BLANK_PDF_SETTINGS };
  }
}

export function saveBlankPdfSettings(storage: BlankPdfSettingsStorage, settings: BlankPdfSettings): void {
  resolveBlankPdfDimensions(settings);
  storage.setItem(BLANK_PDF_SETTINGS_STORAGE_KEY, JSON.stringify(settings));
}

export function resolveBlankPdfDimensions(settings: BlankPdfSettings): BlankPdfCreateRequest {
  const {
    preset,
    orientation,
    customWidth,
    customHeight,
  } = settings;
  const dimensions = preset === 'custom'
    ? {
      widthMm: parseDimension(customWidth, 'Width'),
      heightMm: parseDimension(customHeight, 'Height'),
    }
    : resolvePresetDimensions(preset, orientation);

  if (settings.patternType === 'blank') return dimensions;

  return {
    ...dimensions,
    pattern: {
      type: settings.patternType,
      spacingMm: settings.patternSpacingPreset === 'custom'
        ? parsePatternSpacing(settings.customPatternSpacing)
        : Number(settings.patternSpacingPreset),
      color: settings.patternColorPreset === 'custom'
        ? parsePatternColor(settings.customPatternColor)
        : BLANK_PDF_PATTERN_COLORS[settings.patternColorPreset],
    },
  };
}

function resolvePresetDimensions(
  preset: Exclude<BlankPdfPaperPreset, 'custom'>,
  orientation: BlankPdfOrientation,
): { widthMm: number; heightMm: number } {
  const paper = BLANK_PDF_PAPER_PRESETS[preset];
  return orientation === 'landscape'
    ? { widthMm: paper.heightMm, heightMm: paper.widthMm }
    : { widthMm: paper.widthMm, heightMm: paper.heightMm };
}

export function formatBlankPdfSettings(settings: BlankPdfSettings): string {
  const paper = settings.preset === 'custom'
    ? `${settings.customWidth} × ${settings.customHeight} mm`
    : `${BLANK_PDF_PAPER_PRESETS[settings.preset].label} · ${settings.orientation === 'landscape' ? 'Landscape' : 'Portrait'}`;
  if (settings.patternType === 'blank') return paper;
  const pattern = settings.patternType[0].toUpperCase() + settings.patternType.slice(1);
  return `${paper} · ${pattern}`;
}

export function formatBlankPdfPaperPresetOption(
  preset: Exclude<BlankPdfPaperPreset, 'custom'>,
  orientation: BlankPdfOrientation,
): string {
  const paper = BLANK_PDF_PAPER_PRESETS[preset];
  const dimensions = resolveBlankPdfDimensions({
    ...DEFAULT_BLANK_PDF_SETTINGS,
    preset,
    orientation,
  });
  return `${paper.label} — ${dimensions.widthMm} mm wide × ${dimensions.heightMm} mm high`;
}

function parseDimension(value: string, label: string): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 10 || parsed > 5_000) {
    throw new RangeError(`${label} must be between 10 and 5000 mm.`);
  }
  return parsed;
}

function parsePatternSpacing(value: string): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 1 || parsed > 500) {
    throw new RangeError('Pattern spacing must be between 1 and 500 mm.');
  }
  return parsed;
}

function parsePatternColor(value: string): string {
  if (!/^#[0-9a-f]{6}$/i.test(value)) {
    throw new RangeError('Pattern colour must be a six-digit hexadecimal value.');
  }
  return value.toLowerCase();
}
