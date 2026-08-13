export type CompatibleAnnotationFontId = 'Arimo' | 'Helvetica' | 'Noto Sans' | 'Roboto Mono' | 'Tinos';

export interface AnnotationFontOption {
  readonly id: CompatibleAnnotationFontId;
  readonly label: string;
  readonly cssFamily: string;
  readonly aliases: readonly string[];
}

/**
 * The short font list shown for new annotation text. These are the familiar
 * Windows and macOS family names used by Revu. The CSS stacks prefer an
 * installed system font and use Butter Paper's bundled compatible font only
 * when the requested family is unavailable.
 */
export const ANNOTATION_FONT_OPTIONS: readonly AnnotationFontOption[] = [
  {
    id: 'Arimo',
    label: 'Arial',
    cssFamily: 'Arial, Arimo, sans-serif',
    aliases: ['Arial', 'ArialMT', 'Arial Unicode MS', 'ArialUnicode', 'Calibri'],
  },
  {
    id: 'Roboto Mono',
    label: 'Courier New',
    cssFamily: '"Courier New", "Roboto Mono", monospace',
    aliases: ['Courier New', 'CourierNewPSMT', 'Cousine'],
  },
  {
    id: 'Helvetica',
    label: 'Helvetica',
    cssFamily: 'Helvetica, Arial, Arimo, sans-serif',
    aliases: ['Helvetica Neue', 'HelveticaNeue'],
  },
  {
    id: 'Tinos',
    label: 'Times New Roman',
    cssFamily: '"Times New Roman", Tinos, serif',
    aliases: ['Times New Roman', 'TimesNewRomanPSMT', 'Times Roman', 'Times-Roman', 'Georgia', 'Cambria'],
  },
] as const;

/** Internal Unicode fallback. Keep it out of the default picker, like Revu's
 * current-document fonts, but retain it when an imported annotation uses it. */
const NOTO_SANS_OPTION: AnnotationFontOption = {
  id: 'Noto Sans',
  label: 'Noto Sans',
  cssFamily: '"Noto Sans", sans-serif',
  aliases: ['NotoSans'],
};

const ALL_ANNOTATION_FONT_OPTIONS = [...ANNOTATION_FONT_OPTIONS, NOTO_SANS_OPTION] as const;

export function compatibleAnnotationFontId(fontId: string | undefined): CompatibleAnnotationFontId {
  const normalized = normalizeFontName(fontId ?? '');
  for (const option of ALL_ANNOTATION_FONT_OPTIONS) {
    if (normalizeFontName(option.id) === normalized || option.aliases.some((alias) => normalizeFontName(alias) === normalized)) {
      return option.id;
    }
  }
  return 'Noto Sans';
}

export function annotationFontOption(fontId: string | undefined): AnnotationFontOption {
  const compatibleId = compatibleAnnotationFontId(fontId);
  return ALL_ANNOTATION_FONT_OPTIONS.find((option) => option.id === compatibleId)!;
}

export function annotationFontCssFamily(fontId: string | undefined): string {
  return annotationFontOption(fontId).cssFamily;
}

export function annotationFontDisplayName(fontId: string | undefined): string {
  return annotationFontOption(fontId).label;
}

function normalizeFontName(fontName: string): string {
  let normalized = fontName
    .replace(/^[A-Z]{6}\+/, '')
    .replace(/[-_,\s]+/g, '')
    .toLowerCase();
  let previous = '';
  while (normalized !== previous) {
    previous = normalized;
    normalized = normalized.replace(/(?:bolditalic|boldoblique|italic|oblique|bold|regular|ps|mt)$/i, '');
  }
  return normalized;
}
