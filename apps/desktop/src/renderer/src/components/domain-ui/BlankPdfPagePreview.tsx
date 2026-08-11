import { useId, type CSSProperties, type ReactNode } from 'react';
import {
  BLANK_PDF_PAPER_PRESETS,
  BLANK_PDF_PATTERN_COLORS,
  DEFAULT_BLANK_PDF_SETTINGS,
  resolveBlankPdfDimensions,
  type BlankPdfPatternType,
  type BlankPdfSettings,
} from '../blankPdfSettings';

interface BlankPdfPagePreviewProps {
  settings: BlankPdfSettings;
}

export function BlankPdfPagePreview({ settings }: BlankPdfPagePreviewProps) {
  const patternId = `blank-pdf-preview-${useId().replaceAll(':', '')}`;
  const dimensions = previewDimensions(settings);
  const spacingMm = previewSpacing(settings);
  const color = previewColor(settings);
  const paperLabel = settings.preset === 'custom'
    ? 'Custom'
    : BLANK_PDF_PAPER_PRESETS[settings.preset].label;
  const orientation = dimensions.widthMm >= dimensions.heightMm ? 'landscape' : 'portrait';
  const aspectRatio = dimensions.widthMm / dimensions.heightMm;
  const pageStyle = {
    aspectRatio: `${dimensions.widthMm} / ${dimensions.heightMm}`,
    width: 'var(--blank-pdf-preview-width)',
    height: 'var(--blank-pdf-preview-height)',
    '--blank-pdf-preview-width': `min(100cqw, ${aspectRatio * 100}cqh)`,
    '--blank-pdf-preview-height': `min(100cqh, ${100 / aspectRatio}cqw)`,
  } as CSSProperties;

  return (
    <div
      className="flex h-48 w-full items-center justify-center rounded-lg bg-muted/50 p-3"
      data-domain-ui-exception="blank-pdf-page-preview"
      data-testid="blank-pdf-page-preview-frame"
      style={{ containerType: 'size' }}
    >
      <div
        role="img"
        aria-label={`${paperLabel} ${orientation} preview, ${dimensions.widthMm} by ${dimensions.heightMm} millimetres, ${settings.patternType} paper`}
        className="relative overflow-hidden border border-border bg-white shadow-sm"
        data-testid="blank-pdf-page-preview"
        data-paper-label={paperLabel}
        data-pattern={settings.patternType}
        data-spacing-mm={spacingMm}
        data-pattern-color={color}
        data-width-mm={dimensions.widthMm}
        data-height-mm={dimensions.heightMm}
        style={pageStyle}
      >
        <svg
          className="absolute inset-0 size-full"
          viewBox={`0 0 ${dimensions.widthMm} ${dimensions.heightMm}`}
          preserveAspectRatio="xMidYMid meet"
          aria-hidden="true"
          data-testid="blank-pdf-page-preview-artwork"
        >
          <defs>
            {settings.patternType !== 'blank' ? (
              <PatternDefinition
                id={patternId}
                type={settings.patternType}
                spacing={spacingMm}
                color={color}
              />
            ) : null}
          </defs>
          {settings.patternType !== 'blank' ? (
            <rect
              width={dimensions.widthMm}
              height={dimensions.heightMm}
              fill={`url(#${patternId})`}
              data-testid="blank-pdf-page-preview-pattern"
            />
          ) : null}
        </svg>
        <div
          className="absolute inset-0 flex items-center justify-center"
          aria-hidden="true"
        >
          <span
            className="rounded-sm bg-white/90 px-1.5 py-0.5 text-xs font-medium leading-none text-black shadow-sm"
            data-testid="blank-pdf-page-preview-paper-label"
          >
            {paperLabel}
          </span>
        </div>
        <span
          className="absolute bottom-1 left-1/2 -translate-x-1/2 whitespace-nowrap rounded-sm bg-white/90 px-1 py-0.5 text-xs leading-none text-black/70"
          data-testid="blank-pdf-page-preview-width-label"
          aria-hidden="true"
        >
          {dimensions.widthMm} mm wide
        </span>
        <span
          className="absolute left-1 top-1/2 -translate-y-1/2 rotate-180 whitespace-nowrap rounded-sm bg-white/90 px-1 py-0.5 text-xs leading-none text-black/70"
          data-testid="blank-pdf-page-preview-height-label"
          aria-hidden="true"
          style={{ writingMode: 'vertical-rl' }}
        >
          {dimensions.heightMm} mm high
        </span>
      </div>
    </div>
  );
}

function PatternDefinition({
  id,
  type,
  spacing,
  color,
}: {
  id: string;
  type: Exclude<BlankPdfPatternType, 'blank'>;
  spacing: number;
  color: string;
}): ReactNode {
  if (type === 'dots') {
    return (
      <pattern id={id} width={spacing} height={spacing} patternUnits="userSpaceOnUse">
        <circle cx={spacing / 2} cy={spacing / 2} r={Math.max(0.6, spacing / 16)} fill={color} />
      </pattern>
    );
  }
  if (type === 'grid') {
    return (
      <pattern id={id} width={spacing} height={spacing} patternUnits="userSpaceOnUse">
        <path d={`M ${spacing} 0 L 0 0 0 ${spacing}`} fill="none" stroke={color} strokeWidth="0.75" vectorEffect="non-scaling-stroke" />
      </pattern>
    );
  }
  if (type === 'lined') {
    return (
      <pattern id={id} width={spacing} height={spacing} patternUnits="userSpaceOnUse">
        <path d={`M 0 ${spacing} H ${spacing}`} fill="none" stroke={color} strokeWidth="0.75" vectorEffect="non-scaling-stroke" />
      </pattern>
    );
  }

  const triangleHeight = spacing * Math.sqrt(3);
  return (
    <pattern
      id={id}
      width={spacing * 2}
      height={triangleHeight}
      patternUnits="userSpaceOnUse"
      patternTransform={type === 'isometric' ? 'rotate(30)' : undefined}
    >
      <path
        d={`M 0 0 H ${spacing * 2} M 0 0 L ${spacing} ${triangleHeight} ${spacing * 2} 0`}
        fill="none"
        stroke={color}
        strokeWidth="0.75"
        vectorEffect="non-scaling-stroke"
      />
    </pattern>
  );
}

function previewDimensions(settings: BlankPdfSettings): { widthMm: number; heightMm: number } {
  try {
    return resolveBlankPdfDimensions({ ...settings, patternType: 'blank' });
  } catch {
    return resolveBlankPdfDimensions({ ...DEFAULT_BLANK_PDF_SETTINGS, patternType: 'blank' });
  }
}

function previewSpacing(settings: BlankPdfSettings): number {
  const spacing = settings.patternSpacingPreset === 'custom'
    ? Number(settings.customPatternSpacing)
    : Number(settings.patternSpacingPreset);
  return Number.isFinite(spacing) && spacing > 0 ? spacing : 10;
}

function previewColor(settings: BlankPdfSettings): string {
  if (settings.patternColorPreset !== 'custom') {
    return BLANK_PDF_PATTERN_COLORS[settings.patternColorPreset];
  }
  return /^#[0-9a-f]{6}$/i.test(settings.customPatternColor)
    ? settings.customPatternColor.toLowerCase()
    : DEFAULT_BLANK_PDF_SETTINGS.customPatternColor;
}
