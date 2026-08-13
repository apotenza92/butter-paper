import { useEffect, useId, useState } from 'react';
import {
  CheckIcon,
  CircleIcon,
  DiamondIcon,
  Magnet,
  SquareIcon,
  TriangleIcon,
  XIcon,
} from 'lucide-react';
import { Toggle } from '@/components/ui/toggle';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Field, FieldLabel, FieldLegend, FieldSet } from '@/components/ui/field';
import { Separator } from '@/components/ui/separator';
import { Switch } from '@/components/ui/switch';
import { Input } from '@/components/ui/input';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { cn } from '@/lib/utils';
import type { SnapGuideType, SnapSettings, SnapTarget } from '../state/viewerStore';
import { RAIL_BUTTON_SIZE } from './shellSpacing';
import { SNAP_MARKER_COLOR } from './snapMarkerVisuals';

const SNAP_TARGET_OPTIONS: ReadonlyArray<{ target: SnapTarget; label: string }> = [
  { target: 'endpoint', label: 'Ends' },
  { target: 'midpoint', label: 'Midpoints' },
  { target: 'center', label: 'Centers' },
  { target: 'intersection', label: 'Intersections' },
  { target: 'nearest', label: 'Nearest' },
];

type SnapSource = 'content' | 'markup' | 'page-grid';

const SNAP_SOURCE_OPTIONS: ReadonlyArray<{ source: SnapSource; label: string }> = [
  { source: 'content', label: 'Content' },
  { source: 'markup', label: 'Markup' },
  { source: 'page-grid', label: 'Page grid' },
];

const SNAP_GUIDE_OPTIONS: ReadonlyArray<{ type: SnapGuideType; label: string }> = [
  { type: 'alignment', label: 'Alignment' },
  { type: 'equal-size', label: 'Equal size' },
  { type: 'equal-spacing', label: 'Equal spacing' },
];

function SnapTargetGlyph({ target }: { target: SnapTarget }) {
  const className = 'size-3.5 shrink-0';
  if (target === 'midpoint') return <TriangleIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
  if (target === 'center') return <CircleIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
  if (target === 'intersection') return <XIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
  if (target === 'nearest') return <DiamondIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
  return <SquareIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
}

type SnapSourceSettingValues = Pick<SnapSettings, 'snapToContent' | 'snapToMarkup'> & Partial<Pick<SnapSettings, 'snapToPageGrid'>>;

export function snapTargetsAvailable(settings: SnapSourceSettingValues): boolean {
  return settings.snapToContent || settings.snapToMarkup || Boolean(settings.snapToPageGrid);
}

export function enabledSnapSources(settings: SnapSourceSettingValues): SnapSource[] {
  return SNAP_SOURCE_OPTIONS
    .filter(({ source }) => source === 'content' ? settings.snapToContent : source === 'markup' ? settings.snapToMarkup : Boolean(settings.snapToPageGrid))
    .map(({ source }) => source);
}

export function snapSourceSettingsForValues(values: readonly string[]): Pick<SnapSettings, 'snapToContent' | 'snapToMarkup' | 'snapToPageGrid'> {
  return {
    snapToContent: values.includes('content'),
    snapToMarkup: values.includes('markup'),
    snapToPageGrid: values.includes('page-grid'),
  };
}

interface SnapSettingsMenuProps {
  disabled?: boolean;
  snapSettings: SnapSettings;
  onSnapSettingsChange: (settings: Partial<SnapSettings>) => void;
}

export function SnapSettingsMenu({
  disabled = false,
  snapSettings,
  onSnapSettingsChange,
}: SnapSettingsMenuProps) {
  const [open, setOpen] = useState(false);
  const [viewportHeight, setViewportHeight] = useState(() => window.innerHeight);
  const guidesEnabledId = useId();
  const targets = snapSettings.snapTargets;
  const selectedTargets = new Set(targets);
  const selectedSources = enabledSnapSources(snapSettings);
  const targetsEnabled = snapTargetsAvailable(snapSettings);

  useEffect(() => {
    if (disabled) {
      setOpen(false);
    }
  }, [disabled]);

  useEffect(() => {
    const updateViewportHeight = () => setViewportHeight(window.innerHeight);
    window.addEventListener('resize', updateViewportHeight);
    window.visualViewport?.addEventListener('resize', updateViewportHeight);
    return () => {
      window.removeEventListener('resize', updateViewportHeight);
      window.visualViewport?.removeEventListener('resize', updateViewportHeight);
    };
  }, []);

  return (
    <div className="flex" data-testid="viewer-snap-controls">
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger render={(
          <Toggle
            type="button"
            pressed={open}
            className={cn('relative shrink-0 p-0', RAIL_BUTTON_SIZE)}
            disabled={disabled}
            data-testid="viewer-snap-target-menu"
            data-rail-tooltip="Snap settings"
            aria-label="Snap settings"
          >
            <Magnet aria-hidden="true" />
          </Toggle>
        )} />
        <PopoverContent
          side="left"
          align="start"
          sideOffset={8}
          className="max-h-(--available-height) w-80 overflow-y-auto overscroll-contain"
          data-testid="viewer-snap-popover"
          style={{ maxHeight: Math.max(0, viewportHeight - 16) }}
        >
          <FieldSet className="gap-2">
            <FieldLegend variant="label" className="w-full">Snap to</FieldLegend>
            <ToggleGroup
              multiple
              variant="outline"
              value={selectedSources}
              className="grid w-full grid-cols-2 gap-2"
              aria-label="Snap sources"
              onValueChange={(values) => onSnapSettingsChange(snapSourceSettingsForValues(values))}
            >
              {SNAP_SOURCE_OPTIONS.map(({ source, label }) => {
                const selected = selectedSources.includes(source);
                return (
                  <ToggleGroupItem
                    key={source}
                    value={source}
                    className="h-9 w-full justify-between px-2"
                    data-testid={`viewer-snap-${source}`}
                    aria-label={`Snap to ${label.toLowerCase()}`}
                  >
                    <span>{label}</span>
                    <CheckIcon
                      aria-hidden="true"
                      className={cn(!selected && 'invisible')}
                      data-icon="inline-end"
                      data-testid={`viewer-snap-${source}-check`}
                    />
                  </ToggleGroupItem>
                );
              })}
            </ToggleGroup>
          </FieldSet>
          <Separator />
          <FieldSet className="gap-2">
            <FieldLegend variant="label" className="w-full">Construction grid</FieldLegend>
            <Field orientation="horizontal">
              <FieldLabel htmlFor="snap-construction-grid">Snap to grid</FieldLabel>
              <Switch id="snap-construction-grid" checked={snapSettings.constructionGridEnabled} onCheckedChange={(checked) => onSnapSettingsChange({ constructionGridEnabled: checked })} />
            </Field>
            <Field orientation="horizontal">
              <FieldLabel htmlFor="show-construction-grid">Show grid</FieldLabel>
              <Switch id="show-construction-grid" checked={snapSettings.constructionGridVisible} disabled={!snapSettings.constructionGridEnabled} onCheckedChange={(checked) => onSnapSettingsChange({ constructionGridVisible: checked })} />
            </Field>
            <Field orientation="horizontal">
              <FieldLabel htmlFor="construction-grid-spacing">Spacing (mm)</FieldLabel>
              <Input id="construction-grid-spacing" type="number" min="1" max="500" className="w-24" disabled={!snapSettings.constructionGridEnabled} value={snapSettings.constructionGridSpacingMm} onChange={(event) => onSnapSettingsChange({ constructionGridSpacingMm: Math.min(500, Math.max(1, Number(event.currentTarget.value) || 10)) })} />
            </Field>
          </FieldSet>
          <Separator />
          <FieldSet className="gap-2">
            <FieldLegend variant="label" className="w-full">Dimension increments</FieldLegend>
            <Field orientation="horizontal">
              <FieldLabel htmlFor="snap-dimension-increments">Snap dimensions</FieldLabel>
              <Switch
                id="snap-dimension-increments"
                checked={snapSettings.dimensionIncrementEnabled}
                onCheckedChange={(checked) => onSnapSettingsChange({ dimensionIncrementEnabled: checked })}
              />
            </Field>
            <Field orientation="horizontal">
              <FieldLabel htmlFor="snap-dimension-increment-mm">Increment (mm)</FieldLabel>
              <Input
                id="snap-dimension-increment-mm"
                type="number"
                min="0.1"
                max="500"
                step="0.1"
                className="w-24"
                disabled={!snapSettings.dimensionIncrementEnabled}
                value={snapSettings.dimensionIncrementMm}
                onChange={(event) => onSnapSettingsChange({ dimensionIncrementMm: Math.min(500, Math.max(0.1, Number(event.currentTarget.value) || 5)) })}
              />
            </Field>
          </FieldSet>
          <Separator />
          <FieldSet className="gap-2">
            <FieldLegend variant="label" className="w-full">Snap points</FieldLegend>
            <ToggleGroup
              multiple
              variant="outline"
              value={targets}
              disabled={!targetsEnabled}
              className="grid w-full grid-cols-2 gap-2"
              aria-label="Snap points"
              onValueChange={(values) => onSnapSettingsChange({ snapTargets: values as SnapTarget[] })}
            >
              {SNAP_TARGET_OPTIONS.map((option) => {
                const selected = selectedTargets.has(option.target);
                return (
                  <ToggleGroupItem
                    key={option.target}
                    value={option.target}
                    className="h-9 w-full justify-between px-2"
                    data-testid={`viewer-snap-target-${option.target}`}
                    aria-label={`Snap to ${option.label.toLowerCase()}`}
                  >
                    <span className="flex min-w-0 items-center gap-1.5">
                      <SnapTargetGlyph target={option.target} />
                      <span className="truncate">{option.label}</span>
                    </span>
                    <CheckIcon
                      aria-hidden="true"
                      className={cn(!selected && 'invisible')}
                      data-icon="inline-end"
                      data-testid={`viewer-snap-target-${option.target}-check`}
                    />
                  </ToggleGroupItem>
                );
              })}
            </ToggleGroup>
          </FieldSet>
          <Separator />
          <FieldSet className="gap-2">
            <FieldLegend variant="label" className="w-full">Snap guides</FieldLegend>
            <Field orientation="horizontal">
              <FieldLabel htmlFor={guidesEnabledId}>Show snap guides</FieldLabel>
              <Switch
                id={guidesEnabledId}
                checked={snapSettings.snapGuidesEnabled}
                onCheckedChange={(checked) => onSnapSettingsChange({ snapGuidesEnabled: checked })}
                data-testid="viewer-snap-guides-enabled"
              />
            </Field>
            <ToggleGroup
              multiple
              variant="outline"
              value={[...snapSettings.snapGuideTypes]}
              disabled={!snapSettings.snapGuidesEnabled}
              className="grid w-full grid-cols-2 gap-2"
              aria-label="Snap guide types"
              onValueChange={(values) => onSnapSettingsChange({ snapGuideTypes: values as SnapGuideType[] })}
            >
              {SNAP_GUIDE_OPTIONS.map((option) => {
                const selected = snapSettings.snapGuideTypes.includes(option.type);
                return (
                  <ToggleGroupItem
                    key={option.type}
                    value={option.type}
                    className="h-9 w-full justify-between px-2"
                    data-testid={`viewer-snap-guide-${option.type}`}
                    aria-label={`Show ${option.label.toLowerCase()} guides`}
                  >
                    <span>{option.label}</span>
                    <CheckIcon
                      aria-hidden="true"
                      className={cn(!selected && 'invisible')}
                      data-icon="inline-end"
                      data-testid={`viewer-snap-guide-${option.type}-check`}
                    />
                  </ToggleGroupItem>
                );
              })}
            </ToggleGroup>
          </FieldSet>
        </PopoverContent>
      </Popover>
    </div>
  );
}
