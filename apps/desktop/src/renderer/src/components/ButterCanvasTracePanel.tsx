import type {
  ButterCanvasAsset,
  ButterCanvasTraceOutputMode,
  ButterCanvasTraceSettings,
  ButterCanvasTraceZone,
} from '@butter-paper/core';
import { X } from 'lucide-react';
import type { ReactNode } from 'react';
import {
  DEFAULT_RIGHT_SIDEBAR_WIDTH,
  MAX_RIGHT_SIDEBAR_WIDTH,
  MIN_RIGHT_SIDEBAR_WIDTH,
} from '../state/viewerStore';
import { SidebarResizeHandle } from './SidebarResizeHandle';
import {
  PRIMARY_BAND_HEIGHT,
  CONTROL_ACTIVE,
  CONTROL_DEFAULT,
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
        <button type="button" aria-label="Cancel trace" className={['rounded-[6px] border p-1.5', CONTROL_DEFAULT].join(' ')} onClick={onCancel}>
          <X size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} aria-hidden="true" />
        </button>
      </header>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-3 py-3">
        <div>
          <div className={['truncate text-[12px] font-medium', SHELL_TEXT_PRIMARY].join(' ')} title={asset.name}>{asset.name}</div>
          <div className={['mt-0.5 text-[11px]', SHELL_TEXT_MUTED].join(' ')}>{previewCount} preview segment{previewCount === 1 ? '' : 's'}</div>
        </div>

        <Field label="Sensitivity">
          <input
            type="range"
            min={0}
            max={100}
            value={settings.sensitivity}
            data-testid="butter-canvas-trace-sensitivity"
            onChange={(event) => onSettingsChange({ ...settings, sensitivity: Number(event.target.value) })}
          />
          <span className="w-9 text-right text-[11px] tabular-nums">{settings.sensitivity}</span>
        </Field>

        <label className="flex items-center gap-2 text-[12px]">
          <input
            type="checkbox"
            checked={settings.clearExistingInZone}
            data-testid="butter-canvas-trace-clear"
            onChange={(event) => onSettingsChange({ ...settings, clearExistingInZone: event.target.checked })}
          />
          Clear generated lines in zone
        </label>

        <Field label="Output">
          <select
            value={settings.outputMode}
            className="min-w-0 flex-1 rounded-[6px] border bg-transparent px-2 py-1.5 text-[12px]"
            data-testid="butter-canvas-trace-output"
            onChange={(event) => onSettingsChange({ ...settings, outputMode: event.target.value as ButterCanvasTraceOutputMode })}
          >
            {OUTPUT_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
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
        <button type="button" className={['h-8 flex-1 rounded-[6px] border px-3 text-[12px] font-medium', CONTROL_DEFAULT].join(' ')} onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className={['h-8 flex-1 rounded-[6px] border px-3 text-[12px] font-medium', CONTROL_ACTIVE].join(' ')} data-testid="butter-canvas-trace-apply" onClick={onApply}>
          Apply
        </button>
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
    <label className="flex items-center gap-2 text-[12px]">
      <span className="w-20 shrink-0">{label}</span>
      {children}
    </label>
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
    <label className="flex items-center gap-2 text-[12px]">
      <span className="w-6 shrink-0">{label}</span>
      <input
        type="number"
        min={0}
        max={100}
        step={1}
        value={Math.round(value)}
        className="min-w-0 flex-1 rounded-[6px] border bg-transparent px-2 py-1.5 text-[12px]"
        data-testid={testId}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      <span className={['text-[11px]', SHELL_TEXT_MUTED].join(' ')}>%</span>
    </label>
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
