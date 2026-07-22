import { resolveMarkupAppearance, type Markup } from '@butter-paper/core';
import type { TextContentStyle } from './types';

export interface AnnotationContentStyle {
  readonly stroke: string;
  readonly fill: string;
  readonly strokeWidth: number;
  readonly opacity: number;
  readonly blendMode?: 'multiply';
}

export function getAnnotationContentStyle(markup: Markup): AnnotationContentStyle {
  const appearance = resolveMarkupAppearance(markup);

  return {
    stroke: appearance.stroke?.color ?? 'none',
    fill: appearance.fill?.color ?? 'none',
    strokeWidth: appearance.stroke?.widthPt ?? 0,
    opacity: appearance.opacity,
    blendMode: appearance.blendMode === 'multiply' ? 'multiply' : undefined,
  };
}

export function getAnnotationTextContentStyle(
  markup: Markup,
  firstBaselineRatio = 14.3146 / 12,
): TextContentStyle {
  const appearance = resolveMarkupAppearance(markup);
  const text = appearance.text;
  if (!text) {
    return { opacity: appearance.opacity };
  }
  return {
    textColor: text.color,
    fontFamily: annotationFontCssFamily(text.fontId),
    fontSizePt: text.fontSizePt,
    lineHeightPt: text.lineHeightPt,
    textAlign: text.align,
    textInsetPt: text.insetPt,
    firstBaselineOffsetPt: text.fontSizePt * firstBaselineRatio,
    opacity: appearance.opacity,
  };
}

export function annotationFontCssFamily(fontId: string): string {
  return fontId === 'ArialUnicode'
    ? 'Arial Unicode MS, Arial, sans-serif'
    : 'Helvetica, Arial, sans-serif';
}
