import type {
  ButterCanvasAsset,
  ButterCanvasTraceOutputMode,
  ButterCanvasTraceSettings,
  ButterCanvasTraceZone,
} from '@butter-paper/core';
import { X } from 'lucide-react';
import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select';
import { Slider } from '@/components/ui/slider';
import {
  DEFAULT_RIGHT_SIDEBAR_WIDTH,
  MAX_RIGHT_SIDEBAR_WIDTH,
  MIN_RIGHT_SIDEBAR_WIDTH,
} from '../state/viewerStore';
import { SidebarResizeHandle } from './SidebarResizeHandle';
import {
  PRIMARY_BAND_HEIGHT,
  CONTROL_ICON_SIZE,
  CONTROL_ICON_STROKE_WIDTH,
  SHELL_BORDER_SUBTLE,
  SHELL_HEADER_INSET_X,
  SHELL_SURFACE_PANEL,
  SHELL_TEXT_MUTED,
  SHELL_TEXT_PRIMARY,
} from './shellSpacing';

interface ButterCanvasTracePanelProps {
  asset: ButterCanvasAsset;
  settings: ButterCanvasTraceSettings;
  previewCount: number;
  width: number;
  onSettingsChange: (settings: ButterCanvasTraceSettings) => void;
  onApply: () => void;
  onCancel: () => void;
  onWidthChange: (width: number) => void;
}

const OUTPUT_OPTIONS: readonly { value: ButterCanvasTraceOutputMode; label: string }[] = [
  { value: 'polyline', label: 'Polyline' },
  { value: 'line', label: 'Line' },
  { value: 'pen', label: 'Pen' },
];

export function ButterCanvasTracePanel({
  asset,
  settings,
  previewCount,
  width,
  onSettingsChange,
  onApply,
  onCancel,
  onWidthChange,
}: ButterCanvasTracePanelProps) {
  const zone = settings.zone ?? { x: 0, y: 0, width: 1, height: 1 };
  const updateZone = (key: keyof ButterCanvasTraceZone, value: number) => {
    const nextZone = {
      ...zone,
      [key]: clampPercent(value) / 100,
    };
    onSettingsChange({
      ...settings,
      zone: normalizeZone(nextZone),
    });
  };

  return (
    <aside
      className={['relative flex h-full flex-none flex-col border-l', SHELL_SURFACE_PANEL, SHELL_BORDER_SUBTLE].join(' ')}
      data-testid="butter-canvas-trace-panel"
      style={{ width: `${width}px` }}
    >
      <header
        className={[
          'flex items-center justify-between border-b text-[12px] font-semibold',
          PRIMARY_BAND_HEIGHT,
          SHELL_HEADER_INSET_X,
          SHELL_BORDER_SUBTLE,
          SHELL_TEXT_PRIMARY,
        ].join(' ')}
        data-testid="butter-canvas-trace-panel-header"
      >
        <span>Trace Image</span>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label="Cancel trace"
          className="size-[30px] rounded-2xl p-1.5"
          onClick={onCancel}
        >
          <X size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} aria-hidden="true" />
        </Button>
      </header>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-3 py-3">
        <div>
          <div className={['truncate text-[12px] font-medium', SHELL_TEXT_PRIMARY].join(' ')} title={asset.name}>{asset.name}</div>
          <div className={['mt-0.5 text-[11px]', SHELL_TEXT_MUTED].join(' ')}>{previewCount} preview segment{previewCount === 1 ? '' : 's'}</div>
        </div>

        <Field label="Sensitivity">
          <Slider
            aria-label="Sensitivity"
            className="min-w-0 flex-1"
            min={0}
            max={100}
            value={[settings.sensitivity]}
            data-testid="butter-canvas-trace-sensitivity"
            onValueChange={(value) => onSettingsChange({
              ...settings,
              sensitivity: typeof value === 'number' ? value : (value[0] ?? settings.sensitivity),
            })}
          />
          <span className="w-9 text-right text-[11px] tabular-nums">{settings.sensitivity}</span>
        </Field>

        <Label className="flex items-center gap-2 text-[12px] font-normal">
          <Checkbox
            checked={settings.clearExistingInZone}
            data-testid="butter-canvas-trace-clear"
            onCheckedChange={(checked) => onSettingsChange({ ...settings, clearExistingInZone: checked })}
          />
          Clear generated lines in zone
        </Label>

        <Field label="Output">
          <NativeSelect
            size="sm"
            value={settings.outputMode}
            className="min-w-0 flex-1 [&_[data-slot=native-select]]:rounded-[6px] [&_[data-slot=native-select]]:text-[12px]"
            data-testid="butter-canvas-trace-output"
            onChange={(event) => onSettingsChange({ ...settings, outputMode: event.target.value as ButterCanvasTraceOutputMode })}
          >
            {OUTPUT_OPTIONS.map((option) => (
              <NativeSelectOption key={option.value} value={option.value}>{option.label}</NativeSelectOption>
            ))}
          </NativeSelect>
        </Field>

        <div className="space-y-2">
          <div className={['text-[11px] font-semibold uppercase tracking-normal', SHELL_TEXT_MUTED].join(' ')}>Trace Zone</div>
          <NumberField label="X" value={zone.x * 100} testId="butter-canvas-trace-zone-x" onChange={(value) => updateZone('x', value)} />
          <NumberField label="Y" value={zone.y * 100} testId="butter-canvas-trace-zone-y" onChange={(value) => updateZone('y', value)} />
          <NumberField label="W" value={zone.width * 100} testId="butter-canvas-trace-zone-width" onChange={(value) => updateZone('width', value)} />
          <NumberField label="H" value={zone.height * 100} testId="butter-canvas-trace-zone-height" onChange={(value) => updateZone('height', value)} />
        </div>
      </div>

      <footer className={['flex gap-2 border-t px-3 py-3', SHELL_BORDER_SUBTLE].join(' ')}>
        <Button type="button" variant="outline" className="h-8 flex-1 text-[12px]" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="button" className="h-8 flex-1 text-[12px]" data-testid="butter-canvas-trace-apply" onClick={onApply}>
          Apply
        </Button>
      </footer>

      <SidebarResizeHandle
        side="right"
        width={width}
        minWidth={MIN_RIGHT_SIDEBAR_WIDTH}
        maxWidth={MAX_RIGHT_SIDEBAR_WIDTH}
        defaultWidth={DEFAULT_RIGHT_SIDEBAR_WIDTH}
        label="Trace sidebar"
        testId="butter-canvas-trace-resize-handle"
        onWidthChange={onWidthChange}
      />
    </aside>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Label className="flex items-center gap-2 text-[12px] font-normal">
      <span className="w-20 shrink-0">{label}</span>
      {children}
    </Label>
  );
}

function NumberField({
  label,
  value,
  testId,
  onChange,
}: {
  label: string;
  value: number;
  testId: string;
  onChange: (value: number) => void;
}) {
  return (
    <Label className="flex items-center gap-2 text-[12px] font-normal">
      <span className="w-6 shrink-0">{label}</span>
      <Input
        type="number"
        min={0}
        max={100}
        step={1}
        value={Math.round(value)}
        className="h-8 min-w-0 flex-1 rounded-[6px] px-2 py-1.5 text-[12px]"
        data-testid={testId}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      <span className={['text-[11px]', SHELL_TEXT_MUTED].join(' ')}>%</span>
    </Label>
  );
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(100, value));
}

function normalizeZone(zone: ButterCanvasTraceZone): ButterCanvasTraceZone {
  const x = Math.max(0, Math.min(1, zone.x));
  const y = Math.max(0, Math.min(1, zone.y));
  return {
    x,
    y,
    width: Math.max(0.01, Math.min(1 - x, zone.width)),
    height: Math.max(0.01, Math.min(1 - y, zone.height)),
  };
}
