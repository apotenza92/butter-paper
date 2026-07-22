import type { TextMetricsProvider } from '@butter-paper/core';

export interface TextMeasurementContext {
  readonly fontFamily?: string;
  readonly fontSizePt?: number;
  readonly bold?: boolean;
  readonly italic?: boolean;
}

export interface TextBoxLayoutOptions extends TextMeasurementContext {
  readonly boxWidthPt: number;
  readonly insetPt?: number;
  readonly measureText?: (text: string, context: TextMeasurementContext) => number;
}

export interface TextBoxLine {
  readonly text: string;
}

const DEFAULT_FONT_SIZE_PT = 12;
const DEFAULT_TEXT_INSET_PT = 5;

let canvasContext: CanvasRenderingContext2D | null = null;

export function layoutTextBoxLines(text: string, options: TextBoxLayoutOptions): readonly TextBoxLine[] {
  const inset = options.insetPt ?? DEFAULT_TEXT_INSET_PT;
  const maxWidth = Math.max(1, options.boxWidthPt - inset * 2);
  const paragraphs = splitAnnotationTextLines(text);
  const lines: TextBoxLine[] = [];

  for (const paragraph of paragraphs) {
    lines.push(...wrapParagraph(paragraph, maxWidth, options));
  }

  return lines.length > 0 ? lines : [{ text: '' }];
}

export function splitAnnotationTextLines(text: string): readonly string[] {
  return text.split(/\r\n|\r|\n/);
}

export function measureAnnotationText(text: string, context: TextMeasurementContext): number {
  return browserTextMetricsProvider.measureText(text, {
    fontId: context.fontFamily ?? 'Helvetica, Arial, sans-serif',
    fontSizePt: context.fontSizePt ?? DEFAULT_FONT_SIZE_PT,
    bold: context.bold,
    italic: context.italic,
  });
}

export const browserTextMetricsProvider: TextMetricsProvider = {
  measureText(text, context) {
    return measureTextWithCanvas(text, {
      fontFamily: context.fontId === 'ArialUnicode' ? 'Arial Unicode MS, Arial, sans-serif' : context.fontId,
      fontSizePt: context.fontSizePt,
      bold: context.bold,
      italic: context.italic,
    });
  },
};

function wrapParagraph(paragraph: string, maxWidth: number, options: TextBoxLayoutOptions): readonly TextBoxLine[] {
  if (paragraph.length === 0) {
    return [{ text: '' }];
  }

  const tokens = paragraph.match(/ +|[^ ]+/g) ?? [];
  const lines: TextBoxLine[] = [];
  let line = '';

  for (const token of tokens) {
    const candidate = `${line}${token}`;
    if (measure(candidate, options) <= maxWidth) {
      line = candidate;
      continue;
    }

    if (/^ +$/.test(token)) {
      if (line) {
        const breakResult = breakLineWithOverflowingSpaces(line, token, maxWidth, options);
        lines.push({ text: breakResult.previous });
        line = breakResult.nextPrefix;
      } else {
        line = token;
      }
      continue;
    }

    if (line) {
      const breakResult = breakLineBeforeWord(line, maxWidth, options);
      lines.push({ text: breakResult.previous });
      line = breakResult.nextPrefix;
    }

    const nextCandidate = `${line}${token}`;
    if (measure(nextCandidate, options) <= maxWidth) {
      line = nextCandidate;
    } else {
      line = appendWordWithCharacterBreaks(token, line, maxWidth, lines, options);
    }
  }

  lines.push({ text: line });
  return lines;
}

function breakLineWithOverflowingSpaces(line: string, spaces: string, maxWidth: number, options: TextBoxLayoutOptions): { previous: string; nextPrefix: string } {
  if (spaces.length <= 1) {
    return { previous: line, nextPrefix: '' };
  }

  const distributableSpaces = spaces.length - 1;
  const singleSpaceWidth = measure(' ', options);
  const fittingSpaceCount = Math.floor((maxWidth - measure(line, options)) / singleSpaceWidth);
  const previousSpaceCount = Math.max(0, Math.min(distributableSpaces, fittingSpaceCount - 1));
  const nextSpaceCount = distributableSpaces - previousSpaceCount;
  return {
    previous: `${line}${' '.repeat(previousSpaceCount)}`,
    nextPrefix: ' '.repeat(nextSpaceCount),
  };
}

function breakLineBeforeWord(line: string, maxWidth: number, options: TextBoxLayoutOptions): { previous: string; nextPrefix: string } {
  const trailingSpaces = line.match(/ +$/)?.[0] ?? '';
  if (trailingSpaces.length <= 1) {
    return { previous: line.trimEnd(), nextPrefix: '' };
  }

  const base = line.slice(0, -trailingSpaces.length);
  const distributableSpaces = trailingSpaces.length - 1;
  const singleSpaceWidth = measure(' ', options);
  const fittingSpaceCount = Math.floor((maxWidth - measure(base, options)) / singleSpaceWidth);
  const previousSpaceCount = Math.max(0, Math.min(distributableSpaces, fittingSpaceCount - 1));
  const nextSpaceCount = distributableSpaces - previousSpaceCount;
  return {
    previous: `${base}${' '.repeat(previousSpaceCount)}`,
    nextPrefix: ' '.repeat(nextSpaceCount),
  };
}

function appendWordWithCharacterBreaks(
  word: string,
  initialLine: string,
  maxWidth: number,
  lines: TextBoxLine[],
  options: TextBoxLayoutOptions,
): string {
  let line = initialLine;

  for (const char of word) {
    const candidate = `${line}${char}`;
    if (line && measure(candidate, options) > maxWidth) {
      lines.push({ text: line });
      line = char;
    } else {
      line = candidate;
    }
  }

  return line;
}

function measure(text: string, options: TextBoxLayoutOptions): number {
  if (options.measureText) {
    return options.measureText(text, options);
  }

  return measureTextWithCanvas(text, options);
}

function measureTextWithCanvas(text: string, options: TextMeasurementContext): number {
  if (typeof document === 'undefined') {
    return text.length * (options.fontSizePt ?? DEFAULT_FONT_SIZE_PT) * 0.5;
  }

  const canvas = canvasContext ? null : document.createElement('canvas');
  canvasContext = canvasContext ?? canvas?.getContext('2d') ?? null;
  if (!canvasContext) {
    return text.length * (options.fontSizePt ?? DEFAULT_FONT_SIZE_PT) * 0.5;
  }

  const fontSize = options.fontSizePt ?? DEFAULT_FONT_SIZE_PT;
  const fontStyle = options.italic ? 'italic ' : '';
  const fontWeight = options.bold ? 'bold ' : '';
  canvasContext.font = `${fontStyle}${fontWeight}${fontSize}px ${options.fontFamily ?? 'Helvetica, Arial, sans-serif'}`;
  return canvasContext.measureText(text).width;
}
