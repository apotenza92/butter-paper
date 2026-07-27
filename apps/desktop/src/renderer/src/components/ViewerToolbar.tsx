import {
  ChevronDown,
  Expand,
  Grid2x2,
  Magnet,
  MoveHorizontal,
  MoveVertical,
  RectangleVertical,
  RotateCcw,
  Ruler,
  Search,
  Shapes,
  VectorSquare,
  ZoomIn,
  ZoomOut,
  type LucideIcon,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState, type ComponentProps, type ComponentType, type KeyboardEvent as ReactKeyboardEvent, type ReactElement, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverDescription, PopoverHeader, PopoverTitle, PopoverTrigger } from '@/components/ui/popover';
import { Tooltip as ShadcnTooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import {
  CONTROL_ICON_SIZE,
  CONTROL_ICON_SIZE_CLASS,
  CONTROL_ICON_STROKE_WIDTH,
  PRIMARY_BAND_HEIGHT,
  SHELL_SURFACE_PANEL,
  VIEWER_TOOLBAR_INSET_X,
} from './shellSpacing';
import { Tooltip } from './Tooltip';
import type { CadViewOrganisation, ScrollWheelMode, SnapSettings, SnapTarget } from '../state/viewerStore';

interface ViewerToolbarProps {
  disabled?: boolean;
  zoom: number;
  zoomPreset: 'manual' | 'fit-width' | 'fit-page';
  scrollMode: 'continuous' | 'single-page';
  continuousScrollWheelMode: ScrollWheelMode;
  singlePageScrollWheelMode: ScrollWheelMode;
  cadScrollWheelMode: ScrollWheelMode;
  pageColumnsEnabled: boolean;
  cadViewOrganisation: CadViewOrganisation;
  pagesPerColumn: number;
  snapSettings: SnapSettings;
  onZoomOut: () => void;
  onZoomReset: () => void;
  onZoomIn: () => void;
  onZoomChange: (zoom: number) => void;
  onFitWidth: () => void;
  onFitPage: () => void;
  onScrollModeChange: (mode: 'continuous' | 'single-page') => void;
  onContinuousScrollWheelModeChange: (mode: ScrollWheelMode) => void;
  onSinglePageScrollWheelModeChange: (mode: ScrollWheelMode) => void;
  onCadScrollWheelModeChange: (mode: ScrollWheelMode) => void;
  onPageColumnsEnabledChange: (enabled: boolean) => void;
  onCadViewOrganisationChange: (organisation: CadViewOrganisation) => void;
  onPagesPerColumnChange: (count: number) => void;
  onSnapSettingsChange: (settings: Partial<SnapSettings>) => void;
  onSetPageScale: () => void;
}

interface ToolbarIconProps {
  'aria-hidden'?: boolean | 'false' | 'true';
  className?: string;
  size?: string | number;
  strokeWidth?: string | number;
}

type ToolbarIconComponent = ComponentType<ToolbarIconProps>;

function FitWidthIcon({ size = 24, strokeWidth = 2, className, 'aria-hidden': ariaHidden }: ToolbarIconProps) {
  return (
    <MoveHorizontal
      aria-hidden={ariaHidden}
      data-testid="icon-fit-width"
      size={size}
      strokeWidth={strokeWidth}
      className={className}
    />
  );
}

function FitPageIcon({ size = 24, strokeWidth = 2, className, 'aria-hidden': ariaHidden }: ToolbarIconProps) {
  return (
    <Expand
      aria-hidden={ariaHidden}
      data-testid="icon-fit-page"
      size={size}
      strokeWidth={strokeWidth}
      className={className}
    />
  );
}

function ContinuousIcon({ size = 24, strokeWidth = 2, className, 'aria-hidden': ariaHidden }: ToolbarIconProps) {
  return (
    <span
      aria-hidden={ariaHidden}
      data-testid="icon-continuous-view"
      className={['relative inline-flex items-center justify-center', className].filter(Boolean).join(' ')}
      style={{ width: size, height: size }}
    >
      <RectangleVertical
        size={size}
        strokeWidth={strokeWidth}
        className="absolute inset-0 size-full"
      />
      <svg
        aria-hidden="true"
        className="absolute inset-0 size-full"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth={strokeWidth}
      >
        <path d="M8 12h8" />
      </svg>
    </span>
  );
}

type ToolbarButtonProps = Omit<ComponentProps<typeof Button>, 'size' | 'variant'> & {
  active?: boolean;
  testId?: string;
  ariaPressed?: boolean;
};

function ToolbarButton({
  active,
  ariaPressed,
  className,
  disabled,
  ref,
  testId,
  title,
  ...props
}: ToolbarButtonProps) {
  return (
    <Button
      {...props}
      ref={ref}
      type="button"
      variant={active ? 'secondary' : 'ghost'}
      size="sm"
      disabled={disabled}
      data-testid={testId}
      aria-label={title}
      aria-pressed={ariaPressed}
      className={['relative h-8 rounded-2xl px-2 text-[12px]', className].filter(Boolean).join(' ')}
    />
  );
}

function ToolbarTriggerTooltip({
  disabled,
  label,
  testId,
  trigger,
}: {
  disabled?: boolean;
  label: string;
  testId?: string;
  trigger: ReactElement;
}) {
  if (disabled) {
    return trigger;
  }

  return (
    <ShadcnTooltip>
      <TooltipTrigger render={trigger} />
      <TooltipContent data-testid={testId}>{label}</TooltipContent>
    </ShadcnTooltip>
  );
}

function ToolbarIconButton({
  active,
  disabled,
  icon: Icon,
  label,
  onClick,
  onDoubleClick,
  suppressTooltip,
  testId,
}: {
  active?: boolean;
  disabled?: boolean;
  icon: ToolbarIconComponent;
  label: string;
  onClick?: () => void;
  onDoubleClick?: () => void;
  suppressTooltip?: boolean;
  testId: string;
}) {
  const button = (
    <ToolbarButton
      active={active}
      ariaPressed={active}
      className="w-8 px-0"
      disabled={disabled}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      testId={testId}
      title={label}
    >
      <Icon
        size={CONTROL_ICON_SIZE}
        strokeWidth={CONTROL_ICON_STROKE_WIDTH}
        className={CONTROL_ICON_SIZE_CLASS}
        aria-hidden="true"
      />
    </ToolbarButton>
  );

  if (disabled || suppressTooltip) {
    return button;
  }

  return (
    <ShadcnTooltip>
      <TooltipTrigger render={button} />
      <TooltipContent data-testid={`${testId}-tooltip`}>{label}</TooltipContent>
    </ShadcnTooltip>
  );
}

const snapTargetOptions: ReadonlyArray<{ target: SnapTarget; label: string; hint: string }> = [
  { target: 'endpoint', label: 'Ends + Corners', hint: 'Endpoint' },
  { target: 'midpoint', label: 'Midpoints', hint: 'Midpoint' },
  { target: 'center', label: 'Centers', hint: 'Center' },
  { target: 'intersection', label: 'Intersections', hint: 'Intersection' },
  { target: 'nearest', label: 'Nearest', hint: 'Anywhere on edge' },
];

const zoomPresetOptions: ReadonlyArray<number> = [
  0.0625,
  0.1,
  0.25,
  0.5,
  0.75,
  1,
  1.25,
  1.5,
  2,
  4,
  8,
  16,
  32,
  64,
];
const VIEW_DROPDOWN_OPEN_DELAY_MS = 220;

function formatZoomPercent(zoom: number): string {
  if (zoom < 0.1) {
    return `${Number((zoom * 100).toFixed(2))}%`;
  }

  return `${Math.round(zoom * 100)}%`;
}

function ZoomDropdown({
  disabled,
  zoom,
  onZoomChange,
}: {
  disabled?: boolean;
  zoom: number;
  onZoomChange: (zoom: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const zoomLabel = formatZoomPercent(zoom);
  const selectedZoom = zoomPresetOptions.find((option) => Math.abs(zoom - option) < 0.001);

  useEffect(() => {
    if (disabled) {
      setOpen(false);
    }
  }, [disabled, setOpen]);

  return (
    <DropdownMenu open={open} onOpenChange={setOpen} disabled={disabled}>
      <ShadcnTooltip disabled={disabled || open}>
        <TooltipTrigger render={(
          <DropdownMenuTrigger render={(
            <ToolbarButton
              active={open && !disabled}
              className="min-w-[68px] gap-1.5 px-2 tabular-nums"
              disabled={disabled}
              testId="viewer-zoom-menu"
              title={`Zoom ${zoomLabel}`}
              ariaPressed={open}
            >
              <span>{zoomLabel}</span>
              <ChevronDown size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={CONTROL_ICON_SIZE_CLASS} aria-hidden="true" />
            </ToolbarButton>
          )} />
        )} />
        <TooltipContent>{`Zoom ${zoomLabel}`}</TooltipContent>
      </ShadcnTooltip>
      <DropdownMenuContent align="start" className="min-w-[112px] text-xs">
        <DropdownMenuRadioGroup
          value={selectedZoom === undefined ? '' : String(selectedZoom)}
          onValueChange={(value) => onZoomChange(Number(value))}
        >
          {zoomPresetOptions.map((option) => (
            <DropdownMenuRadioItem
              key={option}
              value={String(option)}
              className="text-xs tabular-nums"
              data-testid={`viewer-zoom-preset-${Math.round(option * 100)}`}
            >
              {formatZoomPercent(option)}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function SnapDropdown({
  disabled,
  snapSettings,
  onSnapSettingsChange,
}: {
  disabled?: boolean;
  snapSettings: SnapSettings;
  onSnapSettingsChange: (settings: Partial<SnapSettings>) => void;
}) {
  const [open, setOpen] = useState(false);
  const targets = snapSettings.snapTargets;
  const selectedTargets = new Set(targets);

  function toggleTarget(target: SnapTarget): void {
    const next = selectedTargets.has(target)
      ? targets.filter((candidate) => candidate !== target)
      : [...targets, target];
    onSnapSettingsChange({ snapTargets: next });
  }

  useEffect(() => {
    if (disabled) {
      setOpen(false);
    }
  }, [disabled, setOpen]);

  return (
    <div data-testid="viewer-snap-controls">
      <DropdownMenu open={open} onOpenChange={setOpen} disabled={disabled}>
        <ShadcnTooltip disabled={disabled || open}>
          <TooltipTrigger render={(
            <DropdownMenuTrigger render={(
              <ToolbarButton
                active={!disabled && (open || snapSettings.snapToContent || snapSettings.snapToMarkup)}
                className="gap-1.5 px-2"
                disabled={disabled}
                testId="viewer-snap-target-menu"
                title="Snap"
                ariaPressed={open}
              >
                <Magnet size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={CONTROL_ICON_SIZE_CLASS} aria-hidden="true" />
                <ChevronDown size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={CONTROL_ICON_SIZE_CLASS} aria-hidden="true" />
              </ToolbarButton>
            )} />
          )} />
          <TooltipContent>Snap</TooltipContent>
        </ShadcnTooltip>
        <DropdownMenuContent align="start" className="min-w-[240px] text-xs">
          <DropdownMenuCheckboxItem
            checked={snapSettings.snapToContent}
            data-testid="viewer-snap-content"
            onCheckedChange={(checked) => onSnapSettingsChange({ snapToContent: checked })}
          >
            <VectorSquare
              size={CONTROL_ICON_SIZE}
              strokeWidth={CONTROL_ICON_STROKE_WIDTH}
              className={CONTROL_ICON_SIZE_CLASS}
              aria-hidden="true"
            />
            <span>Content</span>
          </DropdownMenuCheckboxItem>
          <DropdownMenuCheckboxItem
            checked={snapSettings.snapToMarkup}
            data-testid="viewer-snap-markup"
            onCheckedChange={(checked) => onSnapSettingsChange({ snapToMarkup: checked })}
          >
            <Shapes
              size={CONTROL_ICON_SIZE}
              strokeWidth={CONTROL_ICON_STROKE_WIDTH}
              className={CONTROL_ICON_SIZE_CLASS}
              aria-hidden="true"
            />
            <span>Markup</span>
          </DropdownMenuCheckboxItem>
          <DropdownMenuSeparator />
          {snapTargetOptions.map((option) => {
            const selected = selectedTargets.has(option.target);
            return (
              <DropdownMenuCheckboxItem
                key={option.target}
                checked={selected}
                className="text-xs"
                data-testid={`viewer-snap-target-${option.target}`}
                onCheckedChange={() => toggleTarget(option.target)}
              >
                <span className="flex min-w-0 flex-1 items-center justify-between gap-3">
                  <span>{option.label}</span>
                  <span className="text-[11px] text-muted-foreground">{option.hint}</span>
                </span>
              </DropdownMenuCheckboxItem>
            );
          })}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

const wheelModeOptions: ReadonlyArray<{ mode: ScrollWheelMode; label: string; icon: LucideIcon }> = [
  { mode: 'zoom', label: 'Mousewheel Zoom', icon: Search },
  { mode: 'scroll', label: 'Mousewheel Scroll', icon: MoveVertical },
];
const ACTIVE_VIEW_CHEVRON_CLASS = CONTROL_ICON_SIZE_CLASS;
const INACTIVE_VIEW_CHEVRON_CLASS = `${CONTROL_ICON_SIZE_CLASS} opacity-35`;

function nextSingleSelectValue<Value extends string>(
  event: ReactKeyboardEvent,
  values: readonly Value[],
  currentValue: Value,
  orientation: 'horizontal' | 'vertical',
): Value | null {
  if (event.altKey || event.ctrlKey || event.metaKey || values.length < 2) {
    return null;
  }

  const currentIndex = Math.max(0, values.indexOf(currentValue));
  if (event.key === 'Home') {
    return values[0] ?? null;
  }
  if (event.key === 'End') {
    return values.at(-1) ?? null;
  }

  const previousKey = orientation === 'horizontal' ? 'ArrowLeft' : 'ArrowUp';
  const nextKey = orientation === 'horizontal' ? 'ArrowRight' : 'ArrowDown';
  if (event.key === previousKey) {
    return values[(currentIndex - 1 + values.length) % values.length] ?? null;
  }
  if (event.key === nextKey) {
    return values[(currentIndex + 1) % values.length] ?? null;
  }
  return null;
}

function WheelBehaviourMenuItems({
  mode,
  testIdPrefix,
  onModeChange,
}: {
  mode: ScrollWheelMode;
  testIdPrefix: string;
  onModeChange: (mode: ScrollWheelMode) => void;
}) {
  return (
    <DropdownMenuRadioGroup value={mode} onValueChange={(value) => onModeChange(value as ScrollWheelMode)}>
      {wheelModeOptions.map((option) => {
        const Icon = option.icon;
        return (
          <DropdownMenuRadioItem
            key={option.mode}
            value={option.mode}
            className="text-xs"
            data-testid={`${testIdPrefix}-wheel-${option.mode}`}
          >
            <Icon
              size={CONTROL_ICON_SIZE}
              strokeWidth={CONTROL_ICON_STROKE_WIDTH}
              className={CONTROL_ICON_SIZE_CLASS}
              aria-hidden="true"
            />
            <span className="whitespace-nowrap">{option.label}</span>
          </DropdownMenuRadioItem>
        );
      })}
    </DropdownMenuRadioGroup>
  );
}

function ViewWheelDropdown({
  disabled,
  active,
  icon: Icon,
  label,
  mode,
  testId,
  testIdPrefix,
  suppressTooltip,
  onActivate,
  onDoubleClick,
  onModeChange,
}: {
  disabled?: boolean;
  active: boolean;
  icon: ToolbarIconComponent;
  label: string;
  mode: ScrollWheelMode;
  testId: string;
  testIdPrefix: string;
  suppressTooltip?: boolean;
  onActivate: () => void;
  onDoubleClick?: () => void;
  onModeChange: (mode: ScrollWheelMode) => void;
}) {
  const [open, setOpen] = useState(false);
  const dropdownOpenTimerRef = useRef<number | null>(null);

  const cancelPendingDropdownOpen = useCallback(() => {
    if (dropdownOpenTimerRef.current === null) {
      return;
    }

    window.clearTimeout(dropdownOpenTimerRef.current);
    dropdownOpenTimerRef.current = null;
  }, []);

  useEffect(() => {
    if (disabled || !active) {
      cancelPendingDropdownOpen();
      setOpen(false);
    }
  }, [active, cancelPendingDropdownOpen, disabled, setOpen]);

  useEffect(() => {
    return () => cancelPendingDropdownOpen();
  }, [cancelPendingDropdownOpen]);

  function handleOpenChange(nextOpen: boolean, eventDetails: { event: Event }): void {
    if (!nextOpen) {
      cancelPendingDropdownOpen();
      setOpen(false);
      return;
    }

    if (!active) {
      onActivate();
      setOpen(false);
      return;
    }

    cancelPendingDropdownOpen();
    const isKeyboardOpen = eventDetails.event instanceof KeyboardEvent;
    dropdownOpenTimerRef.current = window.setTimeout(() => {
      dropdownOpenTimerRef.current = null;
      setOpen(true);
    }, isKeyboardOpen || !onDoubleClick ? 0 : VIEW_DROPDOWN_OPEN_DELAY_MS);
  }

  const trigger = (
    <DropdownMenuTrigger
      render={(
        <ToolbarButton
          active={!disabled && active}
          ariaPressed={active}
          className="w-14 gap-1 px-2"
          disabled={disabled}
          onDoubleClick={() => {
            cancelPendingDropdownOpen();
            setOpen(false);
            onDoubleClick?.();
          }}
          testId={testId}
          title={label}
        >
          <Icon size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={CONTROL_ICON_SIZE_CLASS} aria-hidden="true" />
          <ChevronDown size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={active ? ACTIVE_VIEW_CHEVRON_CLASS : INACTIVE_VIEW_CHEVRON_CLASS} aria-hidden="true" />
        </ToolbarButton>
      )}
    />
  );

  return (
    <DropdownMenu open={open} onOpenChange={handleOpenChange} disabled={disabled}>
      <ToolbarTriggerTooltip disabled={disabled || suppressTooltip || open} label={label} testId={`${testId}-tooltip`} trigger={trigger} />
      <DropdownMenuContent align="start" className="min-w-[190px] text-xs">
          <DropdownMenuLabel className="text-[11px]">Mousewheel Behaviour</DropdownMenuLabel>
          <WheelBehaviourMenuItems
            mode={mode}
            testIdPrefix={testIdPrefix}
            onModeChange={onModeChange}
          />
          <div className="px-2 py-1 text-[11px] leading-4 text-muted-foreground">Ctrl + mousewheel does the opposite.</div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function CadViewButton({
  disabled,
  scrollMode,
  pageColumnsEnabled,
  cadViewOrganisation,
  cadScrollWheelMode,
  pagesPerColumn,
  onScrollModeChange,
  onPageColumnsEnabledChange,
  onCadViewOrganisationChange,
  onCadScrollWheelModeChange,
  onPagesPerColumnChange,
}: {
  disabled?: boolean;
  scrollMode: 'continuous' | 'single-page';
  pageColumnsEnabled: boolean;
  cadViewOrganisation: CadViewOrganisation;
  cadScrollWheelMode: ScrollWheelMode;
  pagesPerColumn: number;
  onScrollModeChange: (mode: 'continuous' | 'single-page') => void;
  onPageColumnsEnabledChange: (enabled: boolean) => void;
  onCadViewOrganisationChange: (organisation: CadViewOrganisation) => void;
  onCadScrollWheelModeChange: (mode: ScrollWheelMode) => void;
  onPagesPerColumnChange: (count: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const cadViewActive = scrollMode === 'continuous' && pageColumnsEnabled;
  const countLabel = cadViewOrganisation === 'columns' ? 'Pages/column' : 'Pages/row';

  function handlePageCountChange(value: string): void {
    const nextValue = Number.parseInt(value, 10);
    if (Number.isFinite(nextValue)) {
      onPagesPerColumnChange(nextValue);
    }
  }

  useEffect(() => {
    if (disabled) {
      setOpen(false);
    }
  }, [disabled, setOpen]);

  const trigger = (
    <PopoverTrigger
      render={(
        <ToolbarButton
          active={!disabled && cadViewActive}
          ariaPressed={cadViewActive}
          className="w-14 gap-1 px-2"
          disabled={disabled}
          testId="viewer-cad-view"
          title="CAD View"
        >
          <Grid2x2 size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={CONTROL_ICON_SIZE_CLASS} aria-hidden="true" />
          <ChevronDown size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={cadViewActive ? ACTIVE_VIEW_CHEVRON_CLASS : INACTIVE_VIEW_CHEVRON_CLASS} aria-hidden="true" />
        </ToolbarButton>
      )}
    />
  );

  return (
    <Popover
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen && !cadViewActive) {
          onScrollModeChange('continuous');
          onPageColumnsEnabledChange(true);
          setOpen(false);
          return;
        }
        setOpen(nextOpen);
      }}
    >
      <ToolbarTriggerTooltip disabled={disabled || open} label="CAD View" testId="viewer-cad-view-tooltip" trigger={trigger} />
      <PopoverContent
        align="start"
        className="w-[230px] gap-3 p-3"
        data-testid="viewer-cad-settings"
        finalFocus={() => globalThis.document.querySelector<HTMLElement>('[data-testid="viewer-cad-view"]')}
      >
          <PopoverHeader>
            <PopoverTitle className="text-sm">CAD View</PopoverTitle>
            <PopoverDescription className="text-xs">Organise drawing sheets and choose mousewheel behaviour.</PopoverDescription>
          </PopoverHeader>
          <ToggleGroup
            aria-label="Organise by"
            className="grid w-full grid-cols-2"
            value={[cadViewOrganisation]}
            onKeyDown={(event) => {
              const nextValue = nextSingleSelectValue(event, ['columns', 'rows'], cadViewOrganisation, 'horizontal');
              if (nextValue) {
                onCadViewOrganisationChange(nextValue);
              }
            }}
            onValueChange={(values) => {
              const nextValue = values[0] as CadViewOrganisation | undefined;
              if (nextValue) {
                onCadViewOrganisationChange(nextValue);
              }
            }}
          >
            {([
              ['columns', 'Columns'],
              ['rows', 'Rows'],
            ] as const).map(([organisation, label]) => {
              return (
                <ToggleGroupItem
                  key={organisation}
                  size="sm"
                  value={organisation}
                  className="w-full text-xs"
                  data-testid={`viewer-cad-organisation-${organisation}`}
                >
                  {label}
                </ToggleGroupItem>
              );
            })}
          </ToggleGroup>
          <label className="flex items-center justify-between gap-3 text-xs">
            <span className="text-muted-foreground">{countLabel}</span>
            <Input
              type="number"
              min={1}
              max={100}
              step={1}
              value={pagesPerColumn}
              className="h-7 w-16 text-right text-xs"
              data-testid={cadViewOrganisation === 'columns' ? 'viewer-pages-per-column' : 'viewer-pages-per-row'}
              onChange={(event) => handlePageCountChange(event.currentTarget.value)}
            />
          </label>
          <ToggleGroup
            aria-label="Mousewheel behaviour"
            className="grid w-full gap-1"
            orientation="vertical"
            value={[cadScrollWheelMode]}
            onKeyDown={(event) => {
              const nextValue = nextSingleSelectValue(event, ['zoom', 'scroll'], cadScrollWheelMode, 'vertical');
              if (nextValue) {
                onCadScrollWheelModeChange(nextValue);
              }
            }}
            onValueChange={(values) => {
              const nextValue = values[0] as ScrollWheelMode | undefined;
              if (nextValue) {
                onCadScrollWheelModeChange(nextValue);
              }
            }}
          >
            {wheelModeOptions.map((option) => {
              const Icon = option.icon;
              return (
                <ToggleGroupItem
                  key={option.mode}
                  size="sm"
                  value={option.mode}
                  className="w-full justify-start text-xs"
                  data-testid={`viewer-cad-wheel-${option.mode}`}
                >
                  <Icon aria-hidden="true" />
                  {option.label}
                </ToggleGroupItem>
              );
            })}
          </ToggleGroup>
          <div className="text-[11px] leading-4 text-muted-foreground">Ctrl + mousewheel does the opposite.</div>
      </PopoverContent>
    </Popover>
  );
}

export function ViewerToolbar({
  disabled = false,
  zoom,
  zoomPreset,
  scrollMode,
  continuousScrollWheelMode,
  singlePageScrollWheelMode,
  cadScrollWheelMode,
  pageColumnsEnabled,
  cadViewOrganisation,
  pagesPerColumn,
  snapSettings,
  onZoomOut,
  onZoomReset,
  onZoomIn,
  onZoomChange,
  onFitWidth,
  onFitPage,
  onScrollModeChange,
  onContinuousScrollWheelModeChange,
  onSinglePageScrollWheelModeChange,
  onCadScrollWheelModeChange,
  onPageColumnsEnabledChange,
  onCadViewOrganisationChange,
  onPagesPerColumnChange,
  onSnapSettingsChange,
  onSetPageScale,
}: ViewerToolbarProps) {
  const [gestureHint, setGestureHint] = useState<{ id: string; text: string; nonce: number } | null>(null);
  const hintTimerRef = useRef<number | null>(null);

  function showGestureHint(id: string, text: string): void {
    if (hintTimerRef.current !== null) {
      window.clearTimeout(hintTimerRef.current);
    }
    setGestureHint({ id, text, nonce: Date.now() });
    hintTimerRef.current = window.setTimeout(() => {
      hintTimerRef.current = null;
      setGestureHint(null);
    }, 2000);
  }

  useEffect(() => {
    return () => {
      if (hintTimerRef.current !== null) {
        window.clearTimeout(hintTimerRef.current);
      }
    };
  }, []);

  function hintFor(id: string): ReactNode {
    if (gestureHint?.id !== id) {
      return null;
    }

    return (
      <Tooltip
        key={gestureHint.nonce}
        className="bp-toolbar-gesture-tooltip"
        testId={`viewer-toolbar-hint-${id}`}
      >
        {gestureHint.text}
      </Tooltip>
    );
  }

  return (
    <div
      className={[
        'bp-border-bottom-inset bp-native-scroll-hidden flex min-w-0 items-center overflow-x-auto [justify-content:safe_center]',
        PRIMARY_BAND_HEIGHT,
        VIEWER_TOOLBAR_INSET_X,
        'gap-2',
        SHELL_SURFACE_PANEL,
      ].join(' ')}
      data-testid="viewer-toolbar"
    >
      <ButtonGroup aria-label="Zoom controls" className="gap-1 rounded-2xl bg-muted/50 p-1 [&>[data-slot]]:rounded-xl!">
        <ToolbarIconButton
          disabled={disabled}
          icon={ZoomOut}
          label="Zoom Out"
          onClick={onZoomOut}
          testId="viewer-zoom-out"
        />
        <ToolbarIconButton
          disabled={disabled}
          icon={ZoomIn}
          label="Zoom In"
          onClick={onZoomIn}
          testId="viewer-zoom-in"
        />
        <ZoomDropdown
          disabled={disabled}
          zoom={zoom}
          onZoomChange={onZoomChange}
        />
        <ToolbarIconButton
          disabled={disabled}
          icon={RotateCcw}
          label="Reset Zoom to 100%"
          onClick={onZoomReset}
          testId="viewer-zoom-reset"
        />
      </ButtonGroup>

      <ButtonGroup aria-label="Fit controls" className="gap-1 rounded-2xl bg-muted/50 p-1 [&>[data-slot]]:rounded-xl!">
        <span className="relative inline-flex justify-center">
          <ToolbarIconButton
            active={zoomPreset === 'fit-width'}
            disabled={disabled || pageColumnsEnabled}
            icon={FitWidthIcon}
            label="Fit Width"
            suppressTooltip={gestureHint?.id === 'fit-width'}
            onClick={() => {
              onFitWidth();
              showGestureHint('fit-width', 'Double click to view Continuous');
            }}
            onDoubleClick={() => {
              onScrollModeChange('continuous');
              onPageColumnsEnabledChange(false);
            }}
            testId="viewer-fit-width"
          />
          {hintFor('fit-width')}
        </span>
        <span className="relative inline-flex justify-center">
          <ToolbarIconButton
            active={zoomPreset === 'fit-page'}
            disabled={disabled || pageColumnsEnabled}
            icon={FitPageIcon}
            label="Fit Page"
            suppressTooltip={gestureHint?.id === 'fit-page'}
            onClick={() => {
              onFitPage();
              showGestureHint('fit-page', 'Double click to view Single Page');
            }}
            onDoubleClick={() => {
              onScrollModeChange('single-page');
              onPageColumnsEnabledChange(false);
            }}
            testId="viewer-fit-page"
          />
          {hintFor('fit-page')}
        </span>
      </ButtonGroup>

      <ButtonGroup aria-label="Page view controls" className="gap-1 rounded-2xl bg-muted/50 p-1 [&>[data-slot]]:rounded-xl!">
        <span className="relative inline-flex justify-center">
          <ViewWheelDropdown
            active={scrollMode === 'continuous' && !pageColumnsEnabled}
            disabled={disabled}
            icon={ContinuousIcon}
            label="Continuous View"
            mode={continuousScrollWheelMode}
            suppressTooltip={gestureHint?.id === 'continuous'}
            onActivate={() => {
              onScrollModeChange('continuous');
              onPageColumnsEnabledChange(false);
              showGestureHint('continuous', 'Double click to Fit Width');
            }}
            onDoubleClick={onFitWidth}
            testId="viewer-scroll-continuous"
            testIdPrefix="viewer-continuous"
            onModeChange={onContinuousScrollWheelModeChange}
          />
          {hintFor('continuous')}
        </span>
        <span className="relative inline-flex justify-center">
          <ViewWheelDropdown
            active={scrollMode === 'single-page'}
            disabled={disabled}
            icon={RectangleVertical}
            label="Single Page View"
            mode={singlePageScrollWheelMode}
            suppressTooltip={gestureHint?.id === 'single-page'}
            onActivate={() => {
              onScrollModeChange('single-page');
              onPageColumnsEnabledChange(false);
              showGestureHint('single-page', 'Double click to Fit Page');
            }}
            onDoubleClick={onFitPage}
            testId="viewer-scroll-single-page"
            testIdPrefix="viewer-single-page"
            onModeChange={onSinglePageScrollWheelModeChange}
          />
          {hintFor('single-page')}
        </span>
        <CadViewButton
          disabled={disabled}
          scrollMode={scrollMode}
          pageColumnsEnabled={pageColumnsEnabled}
          cadViewOrganisation={cadViewOrganisation}
          cadScrollWheelMode={cadScrollWheelMode}
          pagesPerColumn={pagesPerColumn}
          onScrollModeChange={onScrollModeChange}
          onPageColumnsEnabledChange={onPageColumnsEnabledChange}
          onCadViewOrganisationChange={onCadViewOrganisationChange}
          onCadScrollWheelModeChange={onCadScrollWheelModeChange}
          onPagesPerColumnChange={onPagesPerColumnChange}
        />
      </ButtonGroup>

      <ButtonGroup aria-label="Snapping controls" className="gap-1 rounded-2xl bg-muted/50 p-1 [&>[data-slot]]:rounded-xl!">
        <SnapDropdown
          disabled={disabled}
          snapSettings={snapSettings}
          onSnapSettingsChange={onSnapSettingsChange}
        />
      </ButtonGroup>

      <ButtonGroup aria-label="Document controls" className="gap-1 rounded-2xl bg-muted/50 p-1 [&>[data-slot]]:rounded-xl!">
        <ToolbarIconButton
          disabled={disabled}
          icon={Ruler}
          label="Set Page Scale"
          onClick={onSetPageScale}
          testId="viewer-set-page-scale"
        />
      </ButtonGroup>
    </div>
  );
}
