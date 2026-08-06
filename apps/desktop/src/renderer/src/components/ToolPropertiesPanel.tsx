import { useState } from 'react';
import type { ToolMode } from '../../../shared/protocol';
import { BooleanPropertyField, ColorPropertyField, CommentsPropertyField, EndpointPropertyFields, LineStylePropertyField, NumericPropertyField, PropertySection, SelectPropertyField, SimpleTextPropertyField, TogglePropertyField, TypographyPropertyFields } from './domain-ui/PropertyControls';
import { CustomScrollArea } from './CustomScrollArea';

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

export function ToolPropertiesPanel({ activeTool, mutationDisabled = false }: { activeTool: ToolMode; mutationDisabled?: boolean }) {
  const [byTool, setByTool] = useState<Partial<Record<ToolMode, PrototypeValues>>>({});
  const values = byTool[activeTool] ?? DEFAULT_VALUES;
  const setValue = <K extends keyof PrototypeValues>(key: K, value: PrototypeValues[K]) => {
    setByTool((current) => ({
      ...current,
      [activeTool]: {
        ...(current[activeTool] ?? DEFAULT_VALUES),
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

  if (!VISUAL_TOOLS.has(activeTool)) {
    return (
      <div className="p-3 text-xs text-muted-foreground" data-testid="tool-properties-panel">
        Choose an annotation tool to preview its properties.
      </div>
    );
  }

  return (
    <CustomScrollArea className="min-h-0 flex-1" viewportClassName="overflow-y-auto overflow-x-hidden" viewportTestId="tool-properties-panel" verticalTrackTestId="tool-properties-scrollbar-track" verticalThumbTestId="tool-properties-scrollbar-thumb">
      <div className="flex min-h-full flex-col gap-5 px-3 py-3" data-testid="properties-local-state-prototype">
        <p className="text-[11px] leading-relaxed text-muted-foreground">Local preview only. Changes are not written to the markup, tool defaults, or PDF.</p>

        <PropertySection title="General">
          <SimpleTextPropertyField label="Subject" value={values.subject} onChange={(value) => setValue('subject', value)} />
          <CommentsPropertyField label="Comments" value={values.comments} onChange={(value) => setValue('comments', value)} />
          <BooleanPropertyField label="Locked" value={values.locked} onChange={(value) => setValue('locked', value)} />
        </PropertySection>

        <PropertySection title="Appearance">
          <ColorPropertyField label={activeTool === 'image' || activeTool === 'snapshot' ? 'Border Color' : 'Color'} value={values.strokeColor} onChange={(value) => setValue('strokeColor', value)} />
          {CLOSED_TOOLS.has(activeTool) ? <ColorPropertyField label="Fill Color" value={values.fillColor} allowTransparent onChange={(value) => setValue('fillColor', value)} /> : null}
          <NumericPropertyField label="Opacity" value={values.opacity} min={0} max={100} unit="%" slider onChange={(value) => setValue('opacity', value)} />
          {CLOSED_TOOLS.has(activeTool) ? <NumericPropertyField label="Fill Opacity" value={values.fillOpacity} min={0} max={100} unit="%" slider onChange={(value) => setValue('fillOpacity', value)} /> : null}
          {activeTool !== 'image' && activeTool !== 'snapshot' ? <NumericPropertyField label="Line Width" value={values.lineWidth} min={0} max={20} step={0.25} unit="pt" slider onChange={(value) => setValue('lineWidth', value)} /> : null}
          {activeTool !== 'image' && activeTool !== 'snapshot' && activeTool !== 'highlight' ? <LineStylePropertyField label="Line Style" value={values.lineStyle} onChange={(value) => setValue('lineStyle', value)} /> : null}
          {CLOSED_TOOLS.has(activeTool) ? <SelectPropertyField label="Hatch Pattern" value={values.hatchPattern} options={['none', 'diagonal', 'crosshatch', 'grid'].map((value) => ({ value, label: value }))} onChange={(value) => setValue('hatchPattern', value)} /> : null}
          {CLOSED_TOOLS.has(activeTool) && values.hatchPattern !== 'none' ? <NumericPropertyField label="Hatch Scale" value={values.hatchScale} min={50} max={200} unit="%" slider onChange={(value) => setValue('hatchScale', value)} /> : null}
          {CLOSED_TOOLS.has(activeTool) && values.hatchPattern !== 'none' ? <ColorPropertyField label="Hatch Color" value={values.hatchColor} onChange={(value) => setValue('hatchColor', value)} /> : null}
        </PropertySection>

        {LINE_TOOLS.has(activeTool) ? (
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

        {TEXT_TOOLS.has(activeTool) ? (
          <PropertySection title="Typography">
            <TypographyPropertyFields value={values.typography} onChange={(value) => setValue('typography', value)} />
          </PropertySection>
        ) : null}

        {MEASUREMENT_TOOLS.has(activeTool) ? (
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
