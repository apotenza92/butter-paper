import { useState } from 'react';
import { resolveMarkupAppearance, type Markup, type PdfPoint, type Rect } from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';
import { BooleanPropertyField, ColorPropertyField, CommentsPropertyField, EndpointPropertyFields, LineStylePropertyField, NumericPropertyField, PropertySection, SelectPropertyField, SimpleTextPropertyField, TogglePropertyField, TypographyPropertyFields } from './domain-ui/PropertyControls';
import { CustomScrollArea } from './CustomScrollArea';
import { Button } from '@/components/ui/button';
import { ConfirmationPopover } from './ConfirmationPopover';
import { getToolDefinition } from '../pdf-tools/toolRegistry';
import type { ToolPropertyDefinition } from '../pdf-tools/types';
import type { ToolPropertyValue } from '../pdf-tools/toolPropertyDefaults';
import { useViewerStore } from '../state/viewerStore';

interface PrototypeValues {
  subject: string;
  comments: string;
  locked: boolean;
  strokeColor: string;
  fillColor: string;
  opacity: number;
  fillOpacity: number;
  lineWidth: number;
  lineStyle: string;
  hatchPattern: string;
  hatchColor: string;
  hatchScale: number;
  startEndpoint: string;
  endEndpoint: string;
  endpointScale: number;
  endpointFilled: string;
  typography: {
    font: string;
    size: number;
    alignment: string;
    verticalAlignment: string;
    styles: string[];
    lineSpacing: number;
    margin: number;
    autoSize: boolean;
  };
  units: string;
  precision: string;
  scale: number;
  caption: boolean;
  countSymbol: string;
  countScale: number;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
}

const DEFAULT_VALUES: PrototypeValues = {
  subject: '',
  comments: '',
  locked: false,
  strokeColor: '#000000',
  fillColor: 'transparent',
  opacity: 100,
  fillOpacity: 100,
  lineWidth: 1,
  lineStyle: 'solid',
  hatchPattern: 'none',
  hatchColor: '#000000',
  hatchScale: 100,
  startEndpoint: 'none',
  endEndpoint: 'none',
  endpointScale: 100,
  endpointFilled: '#000000',
  typography: {
    font: 'Helvetica',
    size: 12,
    alignment: 'left',
    verticalAlignment: 'top',
    styles: [],
    lineSpacing: 1,
    margin: 3,
    autoSize: false,
  },
  units: 'page-scale',
  precision: '0.01',
  scale: 1,
  caption: true,
  countSymbol: 'circle',
  countScale: 100,
  x: 0,
  y: 0,
  width: 100,
  height: 60,
  rotation: 0,
};

const CLOSED_TOOLS = new Set<ToolMode>(['text-box', 'rectangle', 'ellipse', 'area', 'polygon', 'cloud', 'cloud-plus', 'callout']);
const LINE_TOOLS = new Set<ToolMode>(['arc', 'line', 'arrow', 'dimension', 'length', 'polylength', 'polyline', 'cloud-plus', 'callout']);
const TEXT_TOOLS = new Set<ToolMode>(['text-box', 'dimension', 'length', 'polylength', 'area', 'cloud-plus', 'callout']);
const MEASUREMENT_TOOLS = new Set<ToolMode>(['dimension', 'length', 'polylength', 'area']);
const VISUAL_TOOLS = new Set<ToolMode>(['text-box', 'rectangle', 'ellipse', 'arc', 'line', 'arrow', 'dimension', 'length', 'polylength', 'area', 'polyline', 'polygon', 'pen', 'highlight', 'cloud', 'cloud-plus', 'callout', 'image', 'snapshot']);

interface ToolPropertiesPanelProps {
  activeTool: ToolMode;
  selectedMarkup?: Markup | null;
  mutationDisabled?: boolean;
}

export function ToolPropertiesPanel({ activeTool, selectedMarkup = null, mutationDisabled = false }: ToolPropertiesPanelProps) {
  const propertyKind = activeTool === 'select' ? selectedMarkup?.kind ?? null : activeTool;
  const contextKey = selectedMarkup && activeTool === 'select' ? selectedMarkup.id : activeTool;
  const initialValues = selectedMarkup && activeTool === 'select'
    ? prototypeValuesForMarkup(selectedMarkup)
    : DEFAULT_VALUES;
  const [byContext, setByContext] = useState<Record<string, PrototypeValues>>({});
  const values = byContext[contextKey] ?? initialValues;
  const setValue = <K extends keyof PrototypeValues>(key: K, value: PrototypeValues[K]) => {
    setByContext((current) => ({
      ...current,
      [contextKey]: {
        ...(current[contextKey] ?? initialValues),
        [key]: value,
      },
    }));
  };

  if (mutationDisabled) {
    return (
      <div className="p-3 text-xs text-muted-foreground" data-testid="tool-properties-panel">
        Properties are unavailable while editing is disabled.
      </div>
    );
  }

  if (activeTool !== 'select') {
    return <ToolDefaultsPanel activeTool={activeTool} />;
  }

  if (activeTool === 'select' && !selectedMarkup) {
    return (
      <div className="p-3 text-xs text-muted-foreground" data-testid="tool-properties-panel">
        Select a markup to view its properties.
      </div>
    );
  }

  if (!propertyKind || (propertyKind !== 'imported-annotation' && !VISUAL_TOOLS.has(propertyKind))) {
    return (
      <div className="p-3 text-xs text-muted-foreground" data-testid="tool-properties-panel">
        Choose an annotation tool to preview its properties.
      </div>
    );
  }

  return (
    <CustomScrollArea className="min-h-0 flex-1" viewportClassName="overflow-y-auto overflow-x-hidden" viewportTestId="tool-properties-panel" verticalTrackTestId="tool-properties-scrollbar-track" verticalThumbTestId="tool-properties-scrollbar-thumb">
      <div className="flex min-h-full flex-col gap-5 px-3 py-3" data-testid="properties-local-state-prototype">
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          {selectedMarkup && activeTool === 'select'
            ? 'Selected markup preview. Changes are not written to the markup or PDF.'
            : 'Local preview only. Changes are not written to the markup, tool defaults, or PDF.'}
        </p>

        <PropertySection title="General">
          <SimpleTextPropertyField label="Subject" value={values.subject} onChange={(value) => setValue('subject', value)} />
          <CommentsPropertyField label="Comments" value={values.comments} onChange={(value) => setValue('comments', value)} />
          <BooleanPropertyField label="Locked" value={values.locked} onChange={(value) => setValue('locked', value)} />
        </PropertySection>

        <PropertySection title="Appearance">
          <ColorPropertyField label={propertyKind === 'image' || propertyKind === 'snapshot' ? 'Border Color' : 'Color'} value={values.strokeColor} onChange={(value) => setValue('strokeColor', value)} />
          {CLOSED_TOOLS.has(propertyKind as ToolMode) ? <ColorPropertyField label="Fill Color" value={values.fillColor} allowTransparent onChange={(value) => setValue('fillColor', value)} /> : null}
          <NumericPropertyField label="Opacity" value={values.opacity} min={0} max={100} unit="%" slider onChange={(value) => setValue('opacity', value)} />
          {CLOSED_TOOLS.has(propertyKind as ToolMode) ? <NumericPropertyField label="Fill Opacity" value={values.fillOpacity} min={0} max={100} unit="%" slider onChange={(value) => setValue('fillOpacity', value)} /> : null}
          {propertyKind !== 'image' && propertyKind !== 'snapshot' ? <NumericPropertyField label="Line Width" value={values.lineWidth} min={0} max={20} step={0.25} unit="pt" slider onChange={(value) => setValue('lineWidth', value)} /> : null}
          {propertyKind !== 'image' && propertyKind !== 'snapshot' && propertyKind !== 'highlight' ? <LineStylePropertyField label="Line Style" value={values.lineStyle} onChange={(value) => setValue('lineStyle', value)} /> : null}
          {CLOSED_TOOLS.has(propertyKind as ToolMode) ? <SelectPropertyField label="Hatch Pattern" value={values.hatchPattern} options={['none', 'diagonal', 'crosshatch', 'grid'].map((value) => ({ value, label: value }))} onChange={(value) => setValue('hatchPattern', value)} /> : null}
          {CLOSED_TOOLS.has(propertyKind as ToolMode) && values.hatchPattern !== 'none' ? <NumericPropertyField label="Hatch Scale" value={values.hatchScale} min={50} max={200} unit="%" slider onChange={(value) => setValue('hatchScale', value)} /> : null}
          {CLOSED_TOOLS.has(propertyKind as ToolMode) && values.hatchPattern !== 'none' ? <ColorPropertyField label="Hatch Color" value={values.hatchColor} onChange={(value) => setValue('hatchColor', value)} /> : null}
        </PropertySection>

        {LINE_TOOLS.has(propertyKind as ToolMode) ? (
          <PropertySection title="Endpoints">
            <EndpointPropertyFields
              start={values.startEndpoint}
              end={values.endEndpoint}
              scale={values.endpointScale}
              filled={values.endpointFilled}
              onChange={(key, value) => {
                const property = {
                  start: 'startEndpoint',
                  end: 'endEndpoint',
                  scale: 'endpointScale',
                  filled: 'endpointFilled',
                }[key] as keyof PrototypeValues;
                setValue(property, value as never);
              }}
            />
          </PropertySection>
        ) : null}

        {TEXT_TOOLS.has(propertyKind as ToolMode) ? (
          <PropertySection title="Typography">
            <TypographyPropertyFields value={values.typography} onChange={(value) => setValue('typography', value)} />
          </PropertySection>
        ) : null}

        {MEASUREMENT_TOOLS.has(propertyKind as ToolMode) ? (
          <PropertySection title="Measurement">
            <SelectPropertyField
              label="Units"
              value={values.units}
              options={['page-scale', 'mm', 'cm', 'm', 'in', 'ft'].map((value) => ({
                value,
                label: value === 'page-scale' ? 'Page scale' : value,
              }))}
              onChange={(value) => setValue('units', value)}
            />
            <SelectPropertyField
              label="Precision"
              value={values.precision}
              options={['1', '0.1', '0.01', '0.001'].map((value) => ({
                value,
                label: value,
              }))}
              onChange={(value) => setValue('precision', value)}
            />
            <NumericPropertyField label="Scale" value={values.scale} min={0.001} step={0.001} unit="×" onChange={(value) => setValue('scale', value)} />
            <BooleanPropertyField label="Show Caption" value={values.caption} onChange={(value) => setValue('caption', value)} />
            {values.caption ? (
              <TogglePropertyField
                label="Caption Position"
                value="inline"
                options={['inline', 'top', 'bottom'].map((value) => ({
                  value,
                  label: value,
                }))}
                onChange={() => undefined}
              />
            ) : null}
          </PropertySection>
        ) : null}

        <PropertySection title="Layout">
          <NumericPropertyField label="X" value={values.x} unit="pt" onChange={(value) => setValue('x', value)} />
          <NumericPropertyField label="Y" value={values.y} unit="pt" onChange={(value) => setValue('y', value)} />
          <NumericPropertyField label="Width" value={values.width} min={0} unit="pt" onChange={(value) => setValue('width', value)} />
          <NumericPropertyField label="Height" value={values.height} min={0} unit="pt" onChange={(value) => setValue('height', value)} />
          <NumericPropertyField label="Rotation" value={values.rotation} min={0} max={359} unit="°" slider onChange={(value) => setValue('rotation', value)} />
        </PropertySection>
      </div>
    </CustomScrollArea>
  );
}

function ToolDefaultsPanel({ activeTool }: { activeTool: ToolMode }) {
  const definition = getToolDefinition(activeTool);
  const [resetConfirmationOpen, setResetConfirmationOpen] = useState(false);
  const values = useViewerStore((state) => state.toolPropertyValues[activeTool] ?? definition.defaults);
  const setToolPropertyValue = useViewerStore((state) => state.setToolPropertyValue);
  const resetToolPropertyValues = useViewerStore((state) => state.resetToolPropertyValues);

  return (
    <CustomScrollArea className="min-h-0 flex-1" viewportClassName="overflow-y-auto overflow-x-hidden" viewportTestId="tool-properties-panel" verticalTrackTestId="tool-properties-scrollbar-track" verticalThumbTestId="tool-properties-scrollbar-thumb">
      <div className="flex min-h-full flex-col gap-5 px-3 py-3" data-testid="tool-default-properties">
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          These properties apply to the next {definition.label.toLowerCase()} markup.
        </p>
        {definition.properties.properties.length > 0 ? (
          <PropertySection title="Defaults">
            {definition.properties.properties.map((property) => (
              <ToolDefaultPropertyField
                key={property.key}
                property={property}
                value={(values[property.key] ?? property.default) as ToolPropertyValue}
                onChange={(value) => setToolPropertyValue(activeTool, property.key, value)}
              />
            ))}
          </PropertySection>
        ) : (
          <p className="text-xs text-muted-foreground">This tool has no configurable properties.</p>
        )}
        <ConfirmationPopover
          open={resetConfirmationOpen}
          onOpenChange={setResetConfirmationOpen}
          trigger={<Button type="button" variant="outline" size="sm" className="mt-auto w-full">Reset properties</Button>}
          side="left"
          title={`Reset ${definition.label} properties?`}
          description={`This restores the ${definition.label} tool's default property values. Other tools will not change.`}
          actionLabel="Reset"
          onAction={() => {
            resetToolPropertyValues(activeTool);
            setResetConfirmationOpen(false);
          }}
        />
      </div>
    </CustomScrollArea>
  );
}

function ToolDefaultPropertyField({
  property,
  value,
  onChange,
}: {
  property: ToolPropertyDefinition;
  value: ToolPropertyValue;
  onChange: (value: ToolPropertyValue) => void;
}) {
  if (property.kind === 'color') {
    return (
      <ColorPropertyField
        label={property.label}
        value={typeof value === 'string' ? value : 'transparent'}
        allowTransparent={property.default === null}
        onChange={(next) => onChange(next === 'transparent' ? null : next)}
      />
    );
  }
  if (property.kind === 'number') {
    const isOpacity = property.key === 'opacity';
    const numberValue = typeof value === 'number' ? value : property.default;
    return (
      <NumericPropertyField
        label={property.label}
        value={isOpacity ? numberValue * 100 : numberValue}
        min={isOpacity ? (property.min ?? 0) * 100 : property.min}
        max={isOpacity ? (property.max ?? 1) * 100 : property.max}
        step={isOpacity ? (property.step ?? 0.05) * 100 : property.step}
        unit={isOpacity ? '%' : property.key.endsWith('Pt') ? 'pt' : undefined}
        slider={property.min !== undefined && property.max !== undefined}
        onChange={(next) => onChange(isOpacity ? next / 100 : next)}
      />
    );
  }
  if (property.kind === 'select') {
    return (
      <SelectPropertyField
        label={property.label}
        value={typeof value === 'string' ? value : property.default}
        options={property.options}
        onChange={onChange}
      />
    );
  }
  return (
    <BooleanPropertyField
      label={property.label}
      value={typeof value === 'boolean' ? value : property.default}
      onChange={onChange}
    />
  );
}

export function prototypeValuesForMarkup(markup: Markup): PrototypeValues {
  const appearance = resolveMarkupAppearance(markup);
  const bounds = markupBounds(markup);
  const metadata = markup.source?.annotationMetadata?.[0];
  const text = appearance.text;

  return {
    ...DEFAULT_VALUES,
    subject: markup.kind === 'imported-annotation' ? markup.subject ?? metadata?.subject ?? '' : metadata?.subject ?? '',
    comments: markup.kind === 'imported-annotation' ? markup.contents ?? metadata?.contents ?? '' : metadata?.contents ?? '',
    strokeColor: appearance.stroke?.color ?? appearance.text?.color ?? DEFAULT_VALUES.strokeColor,
    fillColor: appearance.fill?.color ?? 'transparent',
    opacity: appearance.opacity * 100,
    lineWidth: appearance.stroke?.widthPt ?? DEFAULT_VALUES.lineWidth,
    typography: text ? {
      ...DEFAULT_VALUES.typography,
      font: text.fontId,
      size: text.fontSizePt,
      alignment: text.align,
      lineSpacing: text.lineHeightPt / text.fontSizePt,
      margin: text.insetPt,
    } : DEFAULT_VALUES.typography,
    x: bounds.x,
    y: bounds.y,
    width: bounds.width,
    height: bounds.height,
    rotation: 'rotation' in markup && typeof markup.rotation === 'number' ? markup.rotation : 0,
  };
}

function markupBounds(markup: Markup): Rect {
  if ('rect' in markup) {
    return markup.rect;
  }
  if ('start' in markup && 'end' in markup) {
    return boundsForPoints([markup.start, markup.end]);
  }
  if ('points' in markup) {
    return boundsForPoints(markup.points);
  }
  if ('paths' in markup) {
    return boundsForPoints(markup.paths.flat());
  }
  if (markup.kind === 'cloud') {
    return boundsForPoints(markup.controlPath);
  }
  if (markup.kind === 'cloud-plus') {
    return boundsForPoints([...markup.cloud.controlPath, ...markup.leader.points, ...rectCorners(markup.textBox)]);
  }
  if (markup.kind === 'callout') {
    return boundsForPoints([...markup.leader.points, ...rectCorners(markup.textBox)]);
  }
  return { x: 0, y: 0, width: 0, height: 0 };
}

function rectCorners(rect: Rect): PdfPoint[] {
  return [
    { x: rect.x, y: rect.y } as PdfPoint,
    { x: rect.x + rect.width, y: rect.y + rect.height } as PdfPoint,
  ];
}

function boundsForPoints(points: readonly PdfPoint[]): Rect {
  if (points.length === 0) {
    return { x: 0, y: 0, width: 0, height: 0 };
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return {
    x,
    y,
    width: Math.max(...xs) - x,
    height: Math.max(...ys) - y,
  };
}
