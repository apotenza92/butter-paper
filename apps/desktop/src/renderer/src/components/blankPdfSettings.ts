import type { BlankPdfCreateRequest } from '../../../shared/protocol';

export type BlankPdfPaperPreset = 'a0' | 'a1' | 'a2' | 'a3' | 'a4' | 'custom';
export type BlankPdfOrientation = 'portrait' | 'landscape';

export interface BlankPdfSettings {
  preset: BlankPdfPaperPreset;
  orientation: BlankPdfOrientation;
  customWidth: string;
  customHeight: string;
}

interface BlankPdfSettingsStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

export const BLANK_PDF_SETTINGS_STORAGE_KEY = 'butter-paper.blank-pdf-settings.v1';

export const BLANK_PDF_PAPER_PRESETS = {
  a0: { label: 'A0', widthMm: 841, heightMm: 1189 },
  a1: { label: 'A1', widthMm: 594, heightMm: 841 },
  a2: { label: 'A2', widthMm: 420, heightMm: 594 },
  a3: { label: 'A3', widthMm: 297, heightMm: 420 },
  a4: { label: 'A4', widthMm: 210, heightMm: 297 },
} as const;

export const DEFAULT_BLANK_PDF_SETTINGS: BlankPdfSettings = {
  preset: 'a3',
  orientation: 'landscape',
  customWidth: '420',
  customHeight: '297',
};

const PAPER_PRESET_VALUES = new Set<BlankPdfPaperPreset>([
  ...Object.keys(BLANK_PDF_PAPER_PRESETS) as Array<keyof typeof BLANK_PDF_PAPER_PRESETS>,
  'custom',
]);

export function loadBlankPdfSettings(storage: BlankPdfSettingsStorage): BlankPdfSettings {
  const stored = storage.getItem(BLANK_PDF_SETTINGS_STORAGE_KEY);
  if (!stored) return { ...DEFAULT_BLANK_PDF_SETTINGS };

  try {
    const candidate = JSON.parse(stored) as Partial<BlankPdfSettings>;
    if (
      typeof candidate.preset !== 'string'
      || !PAPER_PRESET_VALUES.has(candidate.preset as BlankPdfPaperPreset)
      || (candidate.orientation !== 'portrait' && candidate.orientation !== 'landscape')
      || typeof candidate.customWidth !== 'string'
      || typeof candidate.customHeight !== 'string'
    ) {
      return { ...DEFAULT_BLANK_PDF_SETTINGS };
    }

    const settings = candidate as BlankPdfSettings;
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
  if (preset === 'custom') {
    return {
      widthMm: parseDimension(customWidth, 'Width'),
      heightMm: parseDimension(customHeight, 'Height'),
    };
  }

  const paper = BLANK_PDF_PAPER_PRESETS[preset];
  return orientation === 'landscape'
    ? { widthMm: paper.heightMm, heightMm: paper.widthMm }
    : { widthMm: paper.widthMm, heightMm: paper.heightMm };
}

export function formatBlankPdfSettings(settings: BlankPdfSettings): string {
  if (settings.preset === 'custom') {
    return `${settings.customWidth} × ${settings.customHeight} mm`;
  }

  const preset = BLANK_PDF_PAPER_PRESETS[settings.preset];
  const orientation = settings.orientation === 'landscape' ? 'Landscape' : 'Portrait';
  return `${preset.label} · ${orientation}`;
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
  return `${paper.label} — ${dimensions.widthMm} × ${dimensions.heightMm} mm`;
}

function parseDimension(value: string, label: string): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 10 || parsed > 5_000) {
    throw new RangeError(`${label} must be between 10 and 5000 mm.`);
  }
  return parsed;
}
