import { useEffect, useState } from 'react';
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
import { FieldLegend, FieldSet } from '@/components/ui/field';
import { Separator } from '@/components/ui/separator';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { cn } from '@/lib/utils';
import type { SnapSettings, SnapTarget } from '../state/viewerStore';
import { RAIL_BUTTON_SIZE } from './shellSpacing';
import { SNAP_MARKER_COLOR } from './snapMarkerVisuals';

const SNAP_TARGET_OPTIONS: ReadonlyArray<{ target: SnapTarget; label: string }> = [
  { target: 'endpoint', label: 'Ends' },
  { target: 'midpoint', label: 'Midpoints' },
  { target: 'center', label: 'Centers' },
  { target: 'intersection', label: 'Intersections' },
  { target: 'nearest', label: 'Nearest' },
];

type SnapSource = 'content' | 'markup';

const SNAP_SOURCE_OPTIONS: ReadonlyArray<{ source: SnapSource; label: string }> = [
  { source: 'content', label: 'Content' },
  { source: 'markup', label: 'Markup' },
];

function SnapTargetGlyph({ target }: { target: SnapTarget }) {
  const className = 'size-3.5 shrink-0';
  if (target === 'midpoint') return <TriangleIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
  if (target === 'center') return <CircleIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
  if (target === 'intersection') return <XIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
  if (target === 'nearest') return <DiamondIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
  return <SquareIcon aria-hidden="true" className={className} color={SNAP_MARKER_COLOR} data-snap-target-glyph={target} />;
}

export function snapTargetsAvailable(settings: Pick<SnapSettings, 'snapToContent' | 'snapToMarkup'>): boolean {
  return settings.snapToContent || settings.snapToMarkup;
}

export function enabledSnapSources(settings: Pick<SnapSettings, 'snapToContent' | 'snapToMarkup'>): SnapSource[] {
  return SNAP_SOURCE_OPTIONS
    .filter(({ source }) => source === 'content' ? settings.snapToContent : settings.snapToMarkup)
    .map(({ source }) => source);
}

export function snapSourceSettingsForValues(values: readonly string[]): Pick<SnapSettings, 'snapToContent' | 'snapToMarkup'> {
  return {
    snapToContent: values.includes('content'),
    snapToMarkup: values.includes('markup'),
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
  const targets = snapSettings.snapTargets;
  const selectedTargets = new Set(targets);
  const selectedSources = enabledSnapSources(snapSettings);
  const targetsEnabled = snapTargetsAvailable(snapSettings);

  useEffect(() => {
    if (disabled) {
      setOpen(false);
    }
  }, [disabled]);

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
          className="w-80"
          data-testid="viewer-snap-popover"
        >
          <FieldSet className="gap-2">
            <FieldLegend variant="label" className="w-full text-center">Snap to</FieldLegend>
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
            <FieldLegend variant="label" className="w-full text-center">Snap points</FieldLegend>
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
        </PopoverContent>
      </Popover>
    </div>
  );
}
