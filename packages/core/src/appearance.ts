import type { Markup, MarkupAppearance, MarkupBase, ResolvedMarkupAppearance } from './document.js';
import type { Rect } from './points.js';

export interface TextMetricsContext {
  readonly fontId: string;
  readonly fontSizePt: number;
  readonly bold?: boolean;
  readonly italic?: boolean;
}

export interface TextMetricsProvider {
  measureText(text: string, context: TextMetricsContext): number;
}

export interface MarkupPresentation<TPrimitive = unknown> {
  readonly primitives: readonly TPrimitive[];
  readonly visualBounds: Rect;
}

const RED = '#ff0000';
const HELVETICA = 'Helvetica';

const DEFAULT_TEXT = {
  color: RED,
  fontId: HELVETICA,
  fontSizePt: 12,
  lineHeightPt: 13.8,
  align: 'left' as const,
  insetPt: 0,
};

const DEFAULTS: Record<Markup['kind'], ResolvedMarkupAppearance> = {
  rectangle: shapeAppearance(1, null),
  ellipse: shapeAppearance(1, null),
  arc: shapeAppearance(1),
  line: shapeAppearance(1),
  arrow: shapeAppearance(0.5),
  dimension: textShapeAppearance(1, { ...DEFAULT_TEXT, insetPt: 0 }),
  length: textShapeAppearance(1, { ...DEFAULT_TEXT, insetPt: 0 }),
  polylength: textShapeAppearance(1, { ...DEFAULT_TEXT, insetPt: 0 }),
  area: textShapeAppearance(1, { ...DEFAULT_TEXT, insetPt: 0 }, 'rgba(255, 0, 0, 0.08)'),
  polyline: shapeAppearance(1),
  polygon: shapeAppearance(1, null),
  pen: shapeAppearance(1),
  highlight: {
    stroke: { color: '#ffff00', widthPt: 12 },
    opacity: 1,
    blendMode: 'multiply',
  },
  cloud: shapeAppearance(1),
  'cloud-plus': textShapeAppearance(1, { ...DEFAULT_TEXT, insetPt: 3 }),
  'text-box': {
    stroke: { color: RED, widthPt: 0 },
    fill: { color: null },
    text: { ...DEFAULT_TEXT, insetPt: 5 },
    opacity: 1,
    blendMode: 'normal',
  },
  callout: textShapeAppearance(1, { ...DEFAULT_TEXT, insetPt: 3 }),
  image: { opacity: 1, blendMode: 'normal' },
  snapshot: { opacity: 1, blendMode: 'normal' },
  'imported-annotation': { opacity: 1, blendMode: 'normal' },
};

export function defaultMarkupAppearance(kind: Markup['kind']): ResolvedMarkupAppearance {
  return cloneResolvedAppearance(DEFAULTS[kind]);
}

export function resolveMarkupAppearance(markup: Markup): ResolvedMarkupAppearance {
  const defaults = DEFAULTS[markup.kind];
  const legacy = legacyAppearance(markup, defaults);
  return mergeAppearance(defaults, legacy, markup.appearance);
}

export function resolveMarkupAppearanceForKind(
  kind: Markup['kind'],
  markup: MarkupBase & Partial<Markup>,
): ResolvedMarkupAppearance {
  return resolveMarkupAppearance({ ...markup, kind } as Markup);
}

function shapeAppearance(widthPt: number, fillColor?: string | null): ResolvedMarkupAppearance {
  return {
    stroke: { color: RED, widthPt },
    ...(fillColor !== undefined ? { fill: { color: fillColor } } : {}),
    opacity: 1,
    blendMode: 'normal',
  };
}

function textShapeAppearance(
  widthPt: number,
  text: NonNullable<ResolvedMarkupAppearance['text']>,
  fillColor?: string | null,
): ResolvedMarkupAppearance {
  return {
    stroke: { color: RED, widthPt },
    ...(fillColor !== undefined ? { fill: { color: fillColor } } : {}),
    text,
    opacity: 1,
    blendMode: 'normal',
  };
}

function legacyAppearance(markup: Markup, defaults: ResolvedMarkupAppearance): MarkupAppearance {
  const legacyColor = markup.color;
  let strokeWidthPt: number | undefined;
  if ('strokeWidth' in markup && typeof markup.strokeWidth === 'number') {
    strokeWidthPt = markup.strokeWidth;
  } else if (markup.kind === 'cloud-plus' && typeof markup.cloud.strokeWidth === 'number') {
    strokeWidthPt = markup.cloud.strokeWidth;
  } else if (markup.kind === 'text-box' && typeof markup.borderWidth === 'number') {
    strokeWidthPt = markup.borderWidth;
  }

  const text = defaults.text ? {
    color: legacyColor ?? defaults.text.color,
    ...(markup.kind === 'text-box' ? {
      fontId: markup.fontFamily ?? defaults.text.fontId,
      fontSizePt: markup.fontSizePt ?? defaults.text.fontSizePt,
      lineHeightPt: markup.lineHeightPt
        ?? (markup.fontSizePt !== undefined
          ? Number((markup.fontSizePt * (defaults.text.lineHeightPt / defaults.text.fontSizePt)).toFixed(6))
          : defaults.text.lineHeightPt),
      align: markup.textAlign ?? defaults.text.align,
    } : {}),
  } : undefined;

  return {
    stroke: defaults.stroke ? {
      color: markup.kind === 'text-box'
        ? markup.borderColor ?? legacyColor ?? defaults.stroke.color
        : legacyColor ?? defaults.stroke.color,
      widthPt: strokeWidthPt ?? defaults.stroke.widthPt,
    } : undefined,
    text,
    opacity: markup.opacity ?? defaults.opacity,
    blendMode: markup.kind === 'highlight' ? markup.blendMode ?? defaults.blendMode : defaults.blendMode,
  };
}

function mergeAppearance(
  defaults: ResolvedMarkupAppearance,
  legacy: MarkupAppearance,
  explicit: MarkupAppearance | undefined,
): ResolvedMarkupAppearance {
  return {
    stroke: defaults.stroke ? { ...defaults.stroke, ...legacy.stroke, ...explicit?.stroke } : undefined,
    fill: defaults.fill ? { ...defaults.fill, ...legacy.fill, ...explicit?.fill } : undefined,
    text: defaults.text ? { ...defaults.text, ...legacy.text, ...explicit?.text } : undefined,
    opacity: explicit?.opacity ?? legacy.opacity ?? defaults.opacity,
    blendMode: explicit?.blendMode ?? legacy.blendMode ?? defaults.blendMode,
  };
}

function cloneResolvedAppearance(appearance: ResolvedMarkupAppearance): ResolvedMarkupAppearance {
  return {
    stroke: appearance.stroke ? { ...appearance.stroke } : undefined,
    fill: appearance.fill ? { ...appearance.fill } : undefined,
    text: appearance.text ? { ...appearance.text } : undefined,
    opacity: appearance.opacity,
    blendMode: appearance.blendMode,
  };
}
