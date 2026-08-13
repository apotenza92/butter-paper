import type { ArcMarkup, AreaMarkup, ArrowMarkup, CalloutMarkup, CloudMarkup, CloudPlusMarkup, DimensionMarkup, EllipseMarkup, HighlightMarkup, ImageMarkup, ImportedAnnotationMarkup, LengthMarkup, LineMarkup, Markup, MarkupBase, PenMarkup, PolygonMarkup, PolylengthMarkup, PolylineMarkup, RectangleMarkup, RedactMarkup, SnapshotMarkup, TextBoxMarkup } from './document.js';
import { resolveMarkupAppearance } from './appearance.js';
import { normalizeRect, rect, translatePoint, translateRect } from './points.js';
import type { PointLike } from './points.js';

export function createRectangleMarkup(
  params: MarkupBase & Omit<RectangleMarkup, keyof MarkupBase | 'kind'>,
): RectangleMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'rectangle',
    rect: normalizeRect(params.rect),
  });
}

export function createRedactMarkup(
  params: MarkupBase & Omit<RedactMarkup, keyof MarkupBase | 'kind'>,
): RedactMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'redact',
    rect: normalizeRect(params.rect),
    redactionColor: params.redactionColor ?? '#000000',
  });
}

export function createEllipseMarkup(
  params: MarkupBase & Omit<EllipseMarkup, keyof MarkupBase | 'kind'>,
): EllipseMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'ellipse',
    rect: normalizeRect(params.rect),
  });
}

export function createArcMarkup(
  params: MarkupBase & Omit<ArcMarkup, keyof MarkupBase | 'kind'>,
): ArcMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'arc',
    rect: normalizeRect(params.rect),
  });
}

export function createTextBoxMarkup(
  params: MarkupBase & Omit<TextBoxMarkup, keyof MarkupBase | 'kind'>,
): TextBoxMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'text-box',
    rect: normalizeRect(params.rect),
  });
}

export function createLineMarkup(
  params: MarkupBase & Omit<LineMarkup, keyof MarkupBase | 'kind'>,
): LineMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'line',
  });
}

export function createArrowMarkup(
  params: MarkupBase & Omit<ArrowMarkup, keyof MarkupBase | 'kind'>,
): ArrowMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'arrow',
  });
}

export function createDimensionMarkup(
  params: MarkupBase & Omit<DimensionMarkup, keyof MarkupBase | 'kind'>,
): DimensionMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'dimension',
  });
}

export function createLengthMarkup(
  params: MarkupBase & Omit<LengthMarkup, keyof MarkupBase | 'kind'>,
): LengthMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'length',
  });
}

export function createPolylengthMarkup(
  params: MarkupBase & Omit<PolylengthMarkup, keyof MarkupBase | 'kind'>,
): PolylengthMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'polylength',
    points: params.points.map((point) => ({ ...point })),
  });
}

export function createAreaMarkup(
  params: MarkupBase & Omit<AreaMarkup, keyof MarkupBase | 'kind'>,
): AreaMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'area',
    points: params.points.map((point) => ({ ...point })),
  });
}

export function createPolylineMarkup(
  params: MarkupBase & Omit<PolylineMarkup, keyof MarkupBase | 'kind'>,
): PolylineMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'polyline',
    points: params.points.map((point) => ({ ...point })),
  });
}

export function createPolygonMarkup(
  params: MarkupBase & Omit<PolygonMarkup, keyof MarkupBase | 'kind'>,
): PolygonMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'polygon',
    points: params.points.map((point) => ({ ...point })),
  });
}

export function createPenMarkup(
  params: MarkupBase & Omit<PenMarkup, keyof MarkupBase | 'kind'>,
): PenMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'pen',
    paths: params.paths.map((path) => path.map((point) => ({ ...point }))),
  });
}

export function createHighlightMarkup(
  params: MarkupBase & Omit<HighlightMarkup, keyof MarkupBase | 'kind'>,
): HighlightMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'highlight',
    paths: params.paths.map((path) => path.map((point) => ({ ...point }))),
  });
}

export function createCloudMarkup(
  params: MarkupBase & Omit<CloudMarkup, keyof MarkupBase | 'kind'>,
): CloudMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'cloud',
    controlPath: params.controlPath.map((point) => ({ ...point })),
  });
}

export function createCloudPlusMarkup(
  params: MarkupBase & Omit<CloudPlusMarkup, keyof MarkupBase | 'kind'>,
): CloudPlusMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'cloud-plus',
    cloud: {
      ...params.cloud,
      controlPath: params.cloud.controlPath.map((point) => ({ ...point })),
    },
    leader: {
      points: params.leader.points.map((point) => ({ ...point })),
    },
    textBox: normalizeRect(params.textBox),
  });
}

export function createCalloutMarkup(
  params: MarkupBase & Omit<CalloutMarkup, keyof MarkupBase | 'kind'>,
): CalloutMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'callout',
  });
}

export function createImageMarkup(
  params: MarkupBase & Omit<ImageMarkup, keyof MarkupBase | 'kind'>,
): ImageMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'image',
    rect: normalizeRect(params.rect),
  });
}

export function createSnapshotMarkup(
  params: MarkupBase & Omit<SnapshotMarkup, keyof MarkupBase | 'kind'>,
): SnapshotMarkup {
  return withCanonicalAppearance({
    ...params,
    kind: 'snapshot',
    rect: normalizeRect(params.rect),
  });
}

function withCanonicalAppearance<TMarkup extends Markup>(markup: TMarkup): TMarkup {
  return {
    ...markup,
    appearance: resolveMarkupAppearance(markup),
  };
}

export function createImportedAnnotationMarkup(
  params: MarkupBase & Omit<ImportedAnnotationMarkup, keyof MarkupBase | 'kind'>,
): ImportedAnnotationMarkup {
  return {
    ...params,
    kind: 'imported-annotation',
    rect: normalizeRect(params.rect),
  };
}

export function translateMarkup<MarkupType extends Markup>(
  markup: MarkupType,
  delta: PointLike,
): MarkupType {
  switch (markup.kind) {
    case 'rectangle':
    case 'redact':
    case 'ellipse':
      return {
        ...markup,
        rect: translateRect(markup.rect, delta),
      } as MarkupType;
    case 'arc':
      return {
        ...markup,
        rect: translateRect(markup.rect, delta),
        start: markup.start ? translatePoint(markup.start, delta) : undefined,
        end: markup.end ? translatePoint(markup.end, delta) : undefined,
        mid: markup.mid ? translatePoint(markup.mid, delta) : undefined,
      } as MarkupType;
    case 'text-box':
    case 'image':
    case 'snapshot':
      return {
        ...markup,
        rect: translateRect(markup.rect, delta),
      } as MarkupType;
    case 'line':
    case 'arrow':
    case 'dimension':
    case 'length':
      return {
        ...markup,
        start: translatePoint(markup.start, delta),
        end: translatePoint(markup.end, delta),
      } as MarkupType;
    case 'polylength':
    case 'area':
    case 'polyline':
    case 'polygon':
      return {
        ...markup,
        points: markup.points.map((point) => translatePoint(point, delta)),
      } as MarkupType;
    case 'pen':
    case 'highlight':
      return {
        ...markup,
        paths: markup.paths.map((path) => path.map((point) => translatePoint(point, delta))),
      } as MarkupType;
    case 'cloud':
      return {
        ...markup,
        controlPath: markup.controlPath.map((point) => translatePoint(point, delta)),
        appearancePath: markup.appearancePath ? translateAbsoluteSvgPath(markup.appearancePath, delta) : undefined,
      } as MarkupType;
    case 'cloud-plus':
      return {
        ...markup,
        cloud: {
          ...markup.cloud,
          controlPath: markup.cloud.controlPath.map((point) => translatePoint(point, delta)),
          appearancePath: markup.cloud.appearancePath ? translateAbsoluteSvgPath(markup.cloud.appearancePath, delta) : undefined,
        },
        leader: {
          points: markup.leader.points.map((point) => translatePoint(point, delta)),
        },
        textBox: translateRect(markup.textBox, delta),
      } as MarkupType;
    case 'callout':
      return {
        ...markup,
        leader: {
          points: markup.leader.points.map((point) => translatePoint(point, delta)),
        },
        textBox: translateRect(markup.textBox, delta),
      } as MarkupType;
    case 'imported-annotation':
      return {
        ...markup,
        rect: translateRect(markup.rect, delta),
      } as MarkupType;
  }

  return markup;
}

function translateAbsoluteSvgPath(path: string, delta: PointLike): string | undefined {
  const tokens = path.match(/[MLCZ]|-?\d*\.?\d+(?:[eE][+-]?\d+)?/g) ?? [];
  const commands: string[] = [];
  let index = 0;
  while (index < tokens.length) {
    const operator = tokens[index++];
    if (operator === 'M' || operator === 'L') {
      const x = Number(tokens[index++]);
      const y = Number(tokens[index++]);
      if (!Number.isFinite(x) || !Number.isFinite(y)) return undefined;
      commands.push(`${operator} ${formatPathNumber(x + delta.x)} ${formatPathNumber(y + delta.y)}`);
      continue;
    }
    if (operator === 'C') {
      const values = tokens.slice(index, index + 6).map(Number);
      index += 6;
      if (values.length !== 6 || values.some((value) => !Number.isFinite(value))) return undefined;
      commands.push(`C ${values.map((value, valueIndex) => formatPathNumber(value + (valueIndex % 2 === 0 ? delta.x : delta.y))).join(' ')}`);
      continue;
    }
    if (operator === 'Z') {
      commands.push('Z');
      continue;
    }
    return undefined;
  }
  return commands.join(' ');
}

function formatPathNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(4).replace(/0+$/, '').replace(/\.$/, '');
}

export function setCalloutText(markup: Markup, text: string): Markup {
  if (markup.kind === 'callout' || markup.kind === 'cloud-plus') {
    return {
      ...markup,
      text,
    };
  }

  if (markup.kind === 'text-box') {
    return {
      ...markup,
      text,
      richTextRuns: undefined,
      appearanceTextLines: undefined,
    };
  }

  if (markup.kind === 'dimension') {
    return {
      ...markup,
      text,
    };
  }

  return markup;
}
