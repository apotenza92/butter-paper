import { resolveMarkupAppearance, type Markup, type Rect } from '@butter-paper/core';
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

export function getVerticallyCenteredAnnotationTextContentStyle(
  markup: Extract<Markup, { kind: 'callout' | 'cloud-plus' }>,
  textBox: Rect = markup.textBox,
  text: string = markup.text,
): TextContentStyle {
  const style = getAnnotationTextContentStyle(markup);
  const fontSizePt = style.fontSizePt ?? 12;
  const lineHeightPt = style.lineHeightPt ?? fontSizePt * 1.15;
  const lineCount = Math.max(1, text.split(/\r\n|\r|\n/).length);
  const textBlockHeight = lineCount * lineHeightPt;
  const lineBoxTop = Math.max(0, (textBox.height - textBlockHeight) * 0.5);
  const baselineWithinLine = (lineHeightPt - fontSizePt) * 0.5 + fontSizePt * 0.8;
  return {
    ...style,
    firstBaselineOffsetPt: lineBoxTop + baselineWithinLine,
  };
}

export function annotationFontCssFamily(fontId: string): string {
  return fontId === 'ArialUnicode'
    ? 'Arial Unicode MS, Arial, sans-serif'
    : 'Helvetica, Arial, sans-serif';
}
