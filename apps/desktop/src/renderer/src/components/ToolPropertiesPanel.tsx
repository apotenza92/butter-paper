import { useState } from 'react';
import { pdfPoint, resolveMarkupAppearance, type Markup, type PdfPoint, type Rect } from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';
import { BooleanPropertyField, ColorPropertyField, EndpointPropertyFields, FontSizePropertyField, LineStylePropertyField, NumericPropertyField, PropertyAccordion, PropertySection, SelectPropertyField, TogglePropertyField, TypographyPropertyFields } from './domain-ui/PropertyControls';
import { CustomScrollArea } from './CustomScrollArea';
import { Button } from '@/components/ui/button';
import { ConfirmationPopover } from './ConfirmationPopover';
import { getToolDefinition } from '../pdf-tools/toolRegistry';
import type { ToolPropertyDefinition } from '../pdf-tools/types';
import type { ToolPropertyValue } from '../pdf-tools/toolPropertyDefaults';
import { useViewerStore } from '../state/viewerStore';

interface PrototypeValues {
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
  const propertyKind = activeTool === 'select' ? (selectedMarkup?.kind ?? null) : activeTool;
  const initialValues = selectedMarkup && activeTool === 'select' ? prototypeValuesForMarkup(selectedMarkup) : DEFAULT_VALUES;
  const updateDocument = useViewerStore((state) => state.updateDocument);
  const values = initialValues;
  const setValue = <K extends keyof PrototypeValues>(key: K, value: PrototypeValues[K]) => {
    if (!selectedMarkup || activeTool !== 'select') return;
    if (selectedMarkup.locked && key !== 'locked') return;
    updateDocument((document) => ({
      ...document,
      markups: document.markups.map((markup) => markup.id === selectedMarkup.id ? updateSelectedMarkupProperty(markup, key, value) : markup),
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
      <div className="min-h-full" data-testid="properties-local-state-prototype">
        <div>
          {TEXT_TOOLS.has(propertyKind as ToolMode) ? <TypographyPropertyFields value={values.typography} color={values.strokeColor} onChange={(value) => setValue('typography', value)} onColorChange={(value) => setValue('strokeColor', value)} /> : null}

          <PropertySection title="Details">
            <BooleanPropertyField label="Locked" value={values.locked} onChange={(value) => setValue('locked', value)} />
          </PropertySection>

          <PropertySection title="Appearance">
            {!TEXT_TOOLS.has(propertyKind as ToolMode) ? <ColorPropertyField label={propertyKind === 'image' || propertyKind === 'snapshot' ? 'Border Color' : 'Color'} value={values.strokeColor} onChange={(value) => setValue('strokeColor', value)} /> : null}
            <NumericPropertyField label="Opacity" value={values.opacity} min={0} max={100} unit="%" slider onChange={(value) => setValue('opacity', value)} />
            {propertyKind !== 'image' && propertyKind !== 'snapshot' ? <NumericPropertyField label="Line Width" value={values.lineWidth} min={0} max={20} step={0.25} unit="pt" slider onChange={(value) => setValue('lineWidth', value)} /> : null}
            {propertyKind !== 'image' && propertyKind !== 'snapshot' && propertyKind !== 'highlight' ? <LineStylePropertyField label="Line Style" value={values.lineStyle} onChange={(value) => setValue('lineStyle', value)} /> : null}
            {CLOSED_TOOLS.has(propertyKind as ToolMode) ? (
              <>
                <ColorPropertyField label="Fill Color" value={values.fillColor} allowTransparent onChange={(value) => setValue('fillColor', value)} />
                <NumericPropertyField label="Fill Opacity" value={values.fillOpacity} min={0} max={100} unit="%" slider onChange={(value) => setValue('fillOpacity', value)} />
                <SelectPropertyField label="Hatch Pattern" value={values.hatchPattern} options={['none', 'diagonal', 'crosshatch', 'grid'].map((value) => ({ value, label: value }))} onChange={(value) => setValue('hatchPattern', value)} />
                {values.hatchPattern !== 'none' ? <NumericPropertyField label="Hatch Scale" value={values.hatchScale} min={50} max={200} unit="%" slider onChange={(value) => setValue('hatchScale', value)} /> : null}
                {values.hatchPattern !== 'none' ? <ColorPropertyField label="Hatch Color" value={values.hatchColor} onChange={(value) => setValue('hatchColor', value)} /> : null}
              </>
            ) : null}
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
      <div className="min-h-full" data-testid="tool-default-properties">
        <div>
          {definition.properties.properties.length > 0 ? (
            groupToolProperties(definition.properties.properties).map((group) => (
              <PropertySection key={group.title} title={group.title}>
                {group.properties.map((property) => (
                  <ToolDefaultPropertyField key={property.key} property={property} value={(values[property.key] ?? property.default) as ToolPropertyValue} onChange={(value) => setToolPropertyValue(activeTool, property.key, value)} />
                ))}
              </PropertySection>
            ))
          ) : (
            <p className="px-3 py-5 text-xs text-muted-foreground">This tool has no configurable properties.</p>
          )}
        </div>
        <PropertyAccordion title="Reset" defaultOpen={false}>
          <ConfirmationPopover
            open={resetConfirmationOpen}
            onOpenChange={setResetConfirmationOpen}
            trigger={
              <Button type="button" variant="outline" size="sm" className="w-full">
                Reset properties
              </Button>
            }
            side="left"
            title={`Reset ${definition.label} properties?`}
            description={`This restores the ${definition.label} tool's default property values. Other tools will not change.`}
            actionLabel="Reset"
            onAction={() => {
              resetToolPropertyValues(activeTool);
              setResetConfirmationOpen(false);
            }}
          />
        </PropertyAccordion>
      </div>
    </CustomScrollArea>
  );
}

export function groupToolProperties(properties: readonly ToolPropertyDefinition[]): Array<{
  title: string;
  properties: ToolPropertyDefinition[];
}> {
  const groups = new Map<string, ToolPropertyDefinition[]>();
  for (const property of properties) {
    const title = toolPropertyCategory(property.key);
    groups.set(title, [...(groups.get(title) ?? []), property]);
  }
  return ['Text', 'Appearance', 'Shape', 'Other'].flatMap((title) => {
    const groupedProperties = groups.get(title);
    return groupedProperties ? [{ title, properties: groupedProperties }] : [];
  });
}

export function toolPropertyCategory(key: string): 'Appearance' | 'Text' | 'Shape' | 'Other' {
  if (['strokeColor', 'strokeWidthPt', 'fillColor', 'opacity'].includes(key)) return 'Appearance';
  if (['textColor', 'fontFamily', 'fontSizePt'].includes(key)) return 'Text';
  if (['cloudIntensity', 'smoothCurves'].includes(key)) return 'Shape';
  return 'Other';
}

function ToolDefaultPropertyField({ property, value, onChange }: { property: ToolPropertyDefinition; value: ToolPropertyValue; onChange: (value: ToolPropertyValue) => void }) {
  const label = property.key === 'textColor' ? 'Color' : property.label;
  if (property.kind === 'color') {
    return <ColorPropertyField label={label} value={typeof value === 'string' ? value : 'transparent'} allowTransparent={property.default === null} onChange={(next) => onChange(next === 'transparent' ? null : next)} />;
  }
  if (property.kind === 'number') {
    const isOpacity = property.key === 'opacity';
    const numberValue = typeof value === 'number' ? value : property.default;
    if (property.key === 'fontSizePt') {
      return <FontSizePropertyField label={label} value={numberValue} onChange={onChange} />;
    }
    return <NumericPropertyField label={label} value={isOpacity ? numberValue * 100 : numberValue} min={isOpacity ? (property.min ?? 0) * 100 : property.min} max={isOpacity ? (property.max ?? 1) * 100 : property.max} step={isOpacity ? (property.step ?? 0.05) * 100 : property.step} unit={isOpacity ? '%' : property.key.endsWith('Pt') ? 'pt' : undefined} slider={property.min !== undefined && property.max !== undefined} onChange={(next) => onChange(isOpacity ? next / 100 : next)} />;
  }
  if (property.kind === 'select') {
    return <SelectPropertyField label={label} value={typeof value === 'string' ? value : property.default} options={property.options} onChange={onChange} />;
  }
  return <BooleanPropertyField label={label} value={typeof value === 'boolean' ? value : property.default} onChange={onChange} />;
}

export function prototypeValuesForMarkup(markup: Markup): PrototypeValues {
  const appearance = resolveMarkupAppearance(markup);
  const bounds = markupBounds(markup);
  const metadata = markup.source?.annotationMetadata?.[0];
  const text = appearance.text;

  return {
    ...DEFAULT_VALUES,
    locked: markup.locked ?? metadataHasLockedFlag(metadata?.flags),
    strokeColor: appearance.stroke?.color ?? appearance.text?.color ?? DEFAULT_VALUES.strokeColor,
    fillColor: appearance.fill?.color ?? 'transparent',
    opacity: appearance.opacity * 100,
    lineWidth: appearance.stroke?.widthPt ?? DEFAULT_VALUES.lineWidth,
    typography: text
      ? {
          ...DEFAULT_VALUES.typography,
          font: text.fontId,
          size: text.fontSizePt,
          alignment: text.align,
          lineSpacing: text.lineHeightPt / text.fontSizePt,
          margin: text.insetPt,
        }
      : DEFAULT_VALUES.typography,
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
  return [{ x: rect.x, y: rect.y } as PdfPoint, { x: rect.x + rect.width, y: rect.y + rect.height } as PdfPoint];
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

export function updateSelectedMarkupProperty<K extends keyof PrototypeValues>(markup: Markup, key: K, value: PrototypeValues[K]): Markup {
  if (key === 'locked' && typeof value === 'boolean') {
    return { ...markup, locked: value } as Markup;
  }
  const appearance = resolveMarkupAppearance(markup);
  if (key === 'strokeColor' && typeof value === 'string') {
    return TEXT_TOOLS.has(markup.kind as ToolMode) && appearance.text
      ? { ...markup, appearance: { ...markup.appearance, text: { ...markup.appearance?.text, color: value } } } as Markup
      : { ...markup, appearance: { ...markup.appearance, stroke: { ...markup.appearance?.stroke, color: value } } } as Markup;
  }
  if (key === 'fillColor' && typeof value === 'string') {
    return { ...markup, appearance: { ...markup.appearance, fill: { ...markup.appearance?.fill, color: value === 'transparent' ? null : value } } } as Markup;
  }
  if (key === 'opacity' && typeof value === 'number') {
    return { ...markup, appearance: { ...markup.appearance, opacity: Math.min(1, Math.max(0, value / 100)) } } as Markup;
  }
  if (key === 'lineWidth' && typeof value === 'number') {
    return { ...markup, appearance: { ...markup.appearance, stroke: { ...markup.appearance?.stroke, widthPt: Math.max(0, value) } } } as Markup;
  }
  if (key === 'typography' && typeof value === 'object' && value && appearance.text) {
    return {
      ...markup,
      appearance: {
        ...markup.appearance,
        text: {
          ...markup.appearance?.text,
          fontId: value.font,
          fontSizePt: value.size,
          lineHeightPt: value.size * value.lineSpacing,
          align: value.alignment as 'left' | 'center' | 'right',
          insetPt: value.margin,
        },
      },
    } as Markup;
  }
  if (key === 'rotation' && typeof value === 'number' && 'rotation' in markup) {
    return { ...markup, rotation: ((value % 360) + 360) % 360 } as Markup;
  }
  if ((key === 'x' || key === 'y' || key === 'width' || key === 'height') && typeof value === 'number') {
    const bounds = markupBounds(markup);
    const target = {
      ...bounds,
      [key]: key === 'width' || key === 'height' ? Math.max(0, value) : value,
    };
    return transformMarkupToBounds(markup, target);
  }
  return markup;
}

function metadataHasLockedFlag(flags: number | undefined): boolean {
  return typeof flags === 'number' && (flags & 128) !== 0;
}

function transformMarkupToBounds(markup: Markup, target: Rect): Markup {
  const source = markupBounds(markup);
  const mapPoint = (point: PdfPoint): PdfPoint => pdfPoint(
    target.x + (source.width === 0 ? 0 : ((point.x - source.x) / source.width) * target.width),
    target.y + (source.height === 0 ? 0 : ((point.y - source.y) / source.height) * target.height),
  );
  const mapRect = (value: Rect): Rect => {
    const topLeft = mapPoint(pdfPoint(value.x, value.y));
    const bottomRight = mapPoint(pdfPoint(value.x + value.width, value.y + value.height));
    return { x: topLeft.x, y: topLeft.y, width: bottomRight.x - topLeft.x, height: bottomRight.y - topLeft.y };
  };

  if ('rect' in markup) return { ...markup, rect: mapRect(markup.rect) } as Markup;
  if ('start' in markup && 'end' in markup) return { ...markup, start: mapPoint(markup.start), end: mapPoint(markup.end) } as Markup;
  if ('points' in markup) return { ...markup, points: markup.points.map(mapPoint) } as Markup;
  if ('paths' in markup) return { ...markup, paths: markup.paths.map((path) => path.map(mapPoint)) } as Markup;
  if (markup.kind === 'cloud') return { ...markup, controlPath: markup.controlPath.map(mapPoint) };
  if (markup.kind === 'cloud-plus') return { ...markup, cloud: { ...markup.cloud, controlPath: markup.cloud.controlPath.map(mapPoint) }, leader: { points: markup.leader.points.map(mapPoint) }, textBox: mapRect(markup.textBox) };
  if (markup.kind === 'callout') return { ...markup, leader: { points: markup.leader.points.map(mapPoint) }, textBox: mapRect(markup.textBox) };
  return markup;
}
