import {
  ChevronDown,
  Expand,
  Grid2x2,
  MoveHorizontal,
  MoveVertical,
  RectangleVertical,
  RotateCcw,
  Search,
  ZoomIn,
  ZoomOut,
  type LucideIcon,
} from 'lucide-react';
import { useEffect, useRef, useState, type ComponentType, type KeyboardEvent as ReactKeyboardEvent, type ReactElement } from 'react';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Field, FieldLabel } from '@/components/ui/field';
import { Popover, PopoverContent, PopoverDescription, PopoverHeader, PopoverTitle, PopoverTrigger } from '@/components/ui/popover';
import { Tooltip as ShadcnTooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { Toggle } from '@/components/ui/toggle';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import {
  CONTROL_ICON_SIZE,
  CONTROL_ICON_SIZE_CLASS,
  CONTROL_ICON_STROKE_WIDTH,
  PRIMARY_BAND_HEIGHT,
  SHELL_SURFACE_PANEL,
  VIEWER_TOOLBAR_INSET_X,
} from './shellSpacing';
import type { CadViewOrganisation, ScrollWheelMode } from '../state/viewerStore';
import { SplitButtonSegment } from './domain-ui/SplitButtonSegment';

interface ViewerToolbarProps {
  disabled?: boolean;
  zoom: number;
  zoomPreset: 'manual' | 'fit-width' | 'fit-page';
  scrollMode: 'continuous' | 'single-page';
  continuousScrollWheelMode: ScrollWheelMode;
  singlePageScrollWheelMode: ScrollWheelMode;
  pageColumnsEnabled: boolean;
  cadViewOrganisation: CadViewOrganisation;
  pagesPerColumn: number;
  onZoomOut: () => void;
  onZoomReset: () => void;
  onZoomIn: () => void;
  onZoomChange: (zoom: number) => void;
  onFitWidth: () => void;
  onFitPage: () => void;
  onScrollModeChange: (mode: 'continuous' | 'single-page') => void;
  onContinuousScrollWheelModeChange: (mode: ScrollWheelMode) => void;
  onSinglePageScrollWheelModeChange: (mode: ScrollWheelMode) => void;
  onPageColumnsEnabledChange: (enabled: boolean) => void;
  onCadViewOrganisationChange: (organisation: CadViewOrganisation) => void;
  onPagesPerColumnChange: (count: number) => void;
}

interface ToolbarIconProps {
  'aria-hidden'?: boolean | 'false' | 'true';
  className?: string;
  size?: string | number;
  strokeWidth?: string | number;
}

type ToolbarIconComponent = ComponentType<ToolbarIconProps>;

export const TOOLBAR_ACTION_BUTTON_VARIANT = 'ghost' as const;

interface GestureHintState {
  id: string;
  text: string;
  visible: boolean;
}

export function resolveGestureHintPresentation(
  gestureHint: GestureHintState | null,
  id: string,
): { hint: string | undefined; suppressTooltip: boolean } {
  const isCurrentHint = gestureHint?.id === id;
  return {
    hint: isCurrentHint && gestureHint.visible ? gestureHint.text : undefined,
    suppressTooltip: Boolean(isCurrentHint && !gestureHint.visible),
  };
}

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

function ToolbarTriggerTooltip({
  disabled,
  hint,
  hintTestId,
  label,
  suppressTooltip,
  testId,
  trigger,
}: {
  disabled?: boolean;
  hint?: string;
  hintTestId?: string;
  label: string;
  suppressTooltip?: boolean;
  testId?: string;
  trigger: ReactElement;
}) {
  if (disabled || suppressTooltip) {
    return trigger;
  }

  return (
    <ShadcnTooltip open={hint ? true : undefined}>
      <TooltipTrigger render={trigger} />
      <TooltipContent data-testid={hint ? hintTestId : testId}>{hint ?? label}</TooltipContent>
    </ShadcnTooltip>
  );
}

function ToolbarIconButton({
  disabled,
  icon: Icon,
  label,
  onClick,
  testId,
}: {
  disabled?: boolean;
  icon: ToolbarIconComponent;
  label: string;
  onClick?: () => void;
  testId: string;
}) {
  const button = (
    <Button
      type="button"
      variant={TOOLBAR_ACTION_BUTTON_VARIANT}
      size="icon"
      disabled={disabled}
      onClick={onClick}
      data-testid={testId}
      aria-label={label}
    >
      <Icon data-icon="inline-start" aria-hidden="true" />
    </Button>
  );

  if (disabled) {
    return button;
  }

  return (
    <ShadcnTooltip>
      <TooltipTrigger render={button} />
      <TooltipContent data-testid={`${testId}-tooltip`}>{label}</TooltipContent>
    </ShadcnTooltip>
  );
}

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
            <Button
              type="button"
              variant={TOOLBAR_ACTION_BUTTON_VARIANT}
              size="default"
              className="min-w-[68px] tabular-nums"
              disabled={disabled}
              data-testid="viewer-zoom-menu"
              aria-label={`Zoom ${zoomLabel}`}
            >
              <span>{zoomLabel}</span>
              <ChevronDown data-icon="inline-end" aria-hidden="true" />
            </Button>
          )} />
        )} />
        <TooltipContent>{`Zoom ${zoomLabel}`}</TooltipContent>
      </ShadcnTooltip>
      <DropdownMenuContent align="start" className="min-w-[112px]">
        <DropdownMenuRadioGroup
          value={selectedZoom === undefined ? '' : String(selectedZoom)}
          onValueChange={(value) => onZoomChange(Number(value))}
        >
          {zoomPresetOptions.map((option) => (
            <DropdownMenuRadioItem
              key={option}
              value={String(option)}
              className="tabular-nums"
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

const wheelModeOptions: ReadonlyArray<{ mode: ScrollWheelMode; label: string; icon: LucideIcon }> = [
  { mode: 'zoom', label: 'Mousewheel Zoom', icon: Search },
  { mode: 'scroll', label: 'Mousewheel Scroll', icon: MoveVertical },
];
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
            data-testid={`${testIdPrefix}-wheel-${option.mode}`}
          >
            <Icon aria-hidden="true" />
            <span className="whitespace-nowrap">{option.label}</span>
          </DropdownMenuRadioItem>
        );
      })}
    </DropdownMenuRadioGroup>
  );
}

function ViewWheelControl({
  disabled,
  active,
  icon: Icon,
  preserveIconGeometry = false,
  label,
  mode,
  testId,
  testIdPrefix,
  gestureHint,
  suppressTooltip,
  onActivate,
  onDoubleClick,
  onHideGestureHint,
  onRestartTooltip,
  onShowGestureHint,
  onModeChange,
}: {
  disabled?: boolean;
  active: boolean;
  icon: ToolbarIconComponent;
  preserveIconGeometry?: boolean;
  label: string;
  mode: ScrollWheelMode;
  testId: string;
  testIdPrefix: string;
  gestureHint?: string;
  suppressTooltip?: boolean;
  onActivate: () => void;
  onDoubleClick: () => void;
  onHideGestureHint: () => void;
  onRestartTooltip: () => void;
  onShowGestureHint: () => void;
  onModeChange: (mode: ScrollWheelMode) => void;
}) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (disabled) {
      setOpen(false);
    }
  }, [disabled]);

  return (
    <ButtonGroup aria-label={`${label} controls`}>
      <ToolbarTriggerTooltip
        disabled={disabled}
        hint={gestureHint}
        hintTestId={`viewer-toolbar-hint-${testIdPrefix.replace(/^viewer-/, '')}`}
        label={label}
        suppressTooltip={suppressTooltip}
        testId={`${testId}-tooltip`}
        trigger={(
          <Toggle
            variant="outline"
            size="default"
            pressed={active}
            disabled={disabled}
            data-testid={testId}
            aria-label={label}
            onClick={onShowGestureHint}
            onBlur={onHideGestureHint}
            onFocus={onRestartTooltip}
            onPointerEnter={onRestartTooltip}
            onPointerLeave={onHideGestureHint}
            onDoubleClick={() => {
              onHideGestureHint();
              onDoubleClick();
            }}
            onPressedChange={(pressed) => {
              if (pressed) {
                onActivate();
              }
            }}
          >
            {preserveIconGeometry ? (
              <Icon size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={CONTROL_ICON_SIZE_CLASS} aria-hidden="true" />
            ) : (
              <Icon aria-hidden="true" />
            )}
          </Toggle>
        )}
      />
      <DropdownMenu open={open} onOpenChange={setOpen} disabled={disabled}>
        <ShadcnTooltip disabled={disabled || open}>
          <DropdownMenuTrigger
            render={(
              <TooltipTrigger
                render={(
                  <SplitButtonSegment
                    type="button"
                    size="icon"
                    selected={active}
                    disabled={disabled}
                    data-testid={`${testId}-settings`}
                    aria-label={`${label} settings`}
                  >
                    <ChevronDown data-icon="inline-start" aria-hidden="true" />
                  </SplitButtonSegment>
                )}
              />
            )}
          />
          <TooltipContent>{label} settings</TooltipContent>
        </ShadcnTooltip>
        <DropdownMenuContent align="start" className="min-w-[190px]">
          <DropdownMenuGroup>
            <DropdownMenuLabel>Mousewheel Behaviour</DropdownMenuLabel>
            <WheelBehaviourMenuItems
              mode={mode}
              testIdPrefix={testIdPrefix}
              onModeChange={onModeChange}
            />
          </DropdownMenuGroup>
          <div className="px-2 py-1 text-muted-foreground">Ctrl + mousewheel does the opposite.</div>
        </DropdownMenuContent>
      </DropdownMenu>
    </ButtonGroup>
  );
}

function CadViewButton({
  disabled,
  scrollMode,
  pageColumnsEnabled,
  cadViewOrganisation,
  pagesPerColumn,
  onScrollModeChange,
  onPageColumnsEnabledChange,
  onCadViewOrganisationChange,
  onPagesPerColumnChange,
}: {
  disabled?: boolean;
  scrollMode: 'continuous' | 'single-page';
  pageColumnsEnabled: boolean;
  cadViewOrganisation: CadViewOrganisation;
  pagesPerColumn: number;
  onScrollModeChange: (mode: 'continuous' | 'single-page') => void;
  onPageColumnsEnabledChange: (enabled: boolean) => void;
  onCadViewOrganisationChange: (organisation: CadViewOrganisation) => void;
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

  return (
    <ButtonGroup aria-label="CAD View controls">
      <ToolbarTriggerTooltip
        disabled={disabled}
        label="CAD View"
        testId="viewer-cad-view-tooltip"
        trigger={(
          <Toggle
            variant="outline"
            size="default"
            pressed={cadViewActive}
            disabled={disabled}
            data-testid="viewer-cad-view"
            aria-label="CAD View"
            onPressedChange={(pressed) => {
              if (pressed) {
                onScrollModeChange('continuous');
                onPageColumnsEnabledChange(true);
              }
            }}
          >
            <Grid2x2 aria-hidden="true" />
          </Toggle>
        )}
      />
      <Popover open={open} onOpenChange={setOpen}>
        <ShadcnTooltip disabled={disabled || open}>
          <PopoverTrigger
            render={(
              <TooltipTrigger
                render={(
                  <SplitButtonSegment
                    type="button"
                    size="icon"
                    selected={cadViewActive}
                    disabled={disabled}
                    data-testid="viewer-cad-view-settings"
                    aria-label="CAD View settings"
                  >
                    <ChevronDown data-icon="inline-start" aria-hidden="true" />
                  </SplitButtonSegment>
                )}
              />
            )}
          />
          <TooltipContent>CAD View settings</TooltipContent>
        </ShadcnTooltip>
        <PopoverContent
          align="start"
          className="w-[230px]"
          data-testid="viewer-cad-settings"
          finalFocus={() => globalThis.document.querySelector<HTMLElement>('[data-testid="viewer-cad-view-settings"]')}
      >
          <PopoverHeader>
            <PopoverTitle>CAD View</PopoverTitle>
            <PopoverDescription>Organise drawing sheets. Mousewheel always zooms in CAD View.</PopoverDescription>
          </PopoverHeader>
          <ToggleGroup
            aria-label="Organise by"
            className="grid w-full grid-cols-2"
            spacing={0}
            variant="outline"
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
                  className="w-full"
                  data-testid={`viewer-cad-organisation-${organisation}`}
                >
                  {label}
                </ToggleGroupItem>
              );
            })}
          </ToggleGroup>
          <Field orientation="horizontal">
            <FieldLabel>{countLabel}</FieldLabel>
            <Input
              type="number"
              min={1}
              max={100}
              step={1}
              value={pagesPerColumn}
              className="w-16 text-right"
              data-testid={cadViewOrganisation === 'columns' ? 'viewer-pages-per-column' : 'viewer-pages-per-row'}
              onChange={(event) => handlePageCountChange(event.currentTarget.value)}
            />
          </Field>
        </PopoverContent>
      </Popover>
    </ButtonGroup>
  );
}

export function ViewerToolbar({
  disabled = false,
  zoom,
  zoomPreset,
  scrollMode,
  continuousScrollWheelMode,
  singlePageScrollWheelMode,
  pageColumnsEnabled,
  cadViewOrganisation,
  pagesPerColumn,
  onZoomOut,
  onZoomReset,
  onZoomIn,
  onZoomChange,
  onFitWidth,
  onFitPage,
  onScrollModeChange,
  onContinuousScrollWheelModeChange,
  onSinglePageScrollWheelModeChange,
  onPageColumnsEnabledChange,
  onCadViewOrganisationChange,
  onPagesPerColumnChange,
}: ViewerToolbarProps) {
  const [gestureHint, setGestureHint] = useState<GestureHintState | null>(null);
  const gestureHintTimerRef = useRef<number | null>(null);

  function showGestureHint(id: string, text: string): void {
    if (gestureHintTimerRef.current !== null) {
      window.clearTimeout(gestureHintTimerRef.current);
    }
    setGestureHint({ id, text, visible: true });
    gestureHintTimerRef.current = window.setTimeout(() => {
      gestureHintTimerRef.current = null;
      setGestureHint((current) => current?.id === id ? { ...current, visible: false } : current);
    }, 2_000);
  }

  function hideGestureHint(id: string): void {
    setGestureHint((current) => current?.id === id ? { ...current, visible: false } : current);
  }

  function restartTooltip(id: string): void {
    setGestureHint((current) => current?.id === id && !current.visible ? null : current);
  }

  useEffect(() => () => {
    if (gestureHintTimerRef.current !== null) {
      window.clearTimeout(gestureHintTimerRef.current);
    }
  }, []);

  const fitWidthGestureHint = resolveGestureHintPresentation(gestureHint, 'fit-width');
  const fitPageGestureHint = resolveGestureHintPresentation(gestureHint, 'fit-page');
  const continuousGestureHint = resolveGestureHintPresentation(gestureHint, 'continuous');
  const singlePageGestureHint = resolveGestureHintPresentation(gestureHint, 'single-page');

  return (
    <div
      className={[
        'bp-native-scroll-hidden flex min-w-0 items-center overflow-x-auto border-b border-border [justify-content:safe_center]',
        PRIMARY_BAND_HEIGHT,
        VIEWER_TOOLBAR_INSET_X,
        'gap-2',
        SHELL_SURFACE_PANEL,
      ].join(' ')}
      data-testid="viewer-toolbar"
    >
      <ButtonGroup aria-label="Zoom controls">
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

      <ToggleGroup
        aria-label="Fit controls"
        variant="outline"
        spacing={0}
        value={zoomPreset === 'manual' ? [] : [zoomPreset]}
        onValueChange={(values) => {
          const nextValue = values.at(-1);
          if (nextValue === 'fit-width') {
            onFitWidth();
          } else if (nextValue === 'fit-page') {
            onFitPage();
          }
        }}
      >
        <ToolbarTriggerTooltip
          disabled={disabled || pageColumnsEnabled}
          hint={fitWidthGestureHint.hint}
          hintTestId="viewer-toolbar-hint-fit-width"
          label="Fit Width"
          suppressTooltip={fitWidthGestureHint.suppressTooltip}
          testId="viewer-fit-width-tooltip"
          trigger={(
            <ToggleGroupItem
              value="fit-width"
              disabled={disabled || pageColumnsEnabled}
              data-testid="viewer-fit-width"
              aria-label="Fit Width"
              onClick={() => showGestureHint('fit-width', 'Double click to view Continuous')}
              onBlur={() => hideGestureHint('fit-width')}
              onFocus={() => restartTooltip('fit-width')}
              onPointerEnter={() => restartTooltip('fit-width')}
              onPointerLeave={() => hideGestureHint('fit-width')}
              onDoubleClick={() => {
                hideGestureHint('fit-width');
                onScrollModeChange('continuous');
                onPageColumnsEnabledChange(false);
              }}
            >
              <FitWidthIcon size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={CONTROL_ICON_SIZE_CLASS} aria-hidden="true" />
            </ToggleGroupItem>
          )}
        />
        <ToolbarTriggerTooltip
          disabled={disabled || pageColumnsEnabled}
          hint={fitPageGestureHint.hint}
          hintTestId="viewer-toolbar-hint-fit-page"
          label="Fit Page"
          suppressTooltip={fitPageGestureHint.suppressTooltip}
          testId="viewer-fit-page-tooltip"
          trigger={(
            <ToggleGroupItem
              value="fit-page"
              disabled={disabled || pageColumnsEnabled}
              data-testid="viewer-fit-page"
              aria-label="Fit Page"
              onClick={() => showGestureHint('fit-page', 'Double click to view Single Page')}
              onBlur={() => hideGestureHint('fit-page')}
              onFocus={() => restartTooltip('fit-page')}
              onPointerEnter={() => restartTooltip('fit-page')}
              onPointerLeave={() => hideGestureHint('fit-page')}
              onDoubleClick={() => {
                hideGestureHint('fit-page');
                onScrollModeChange('single-page');
                onPageColumnsEnabledChange(false);
              }}
            >
              <FitPageIcon size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} className={CONTROL_ICON_SIZE_CLASS} aria-hidden="true" />
            </ToggleGroupItem>
          )}
        />
      </ToggleGroup>

      <ButtonGroup aria-label="Page view controls">
        <ViewWheelControl
          active={scrollMode === 'continuous' && !pageColumnsEnabled}
          disabled={disabled}
          icon={ContinuousIcon}
          preserveIconGeometry
          label="Continuous View"
          mode={continuousScrollWheelMode}
          onActivate={() => {
            onScrollModeChange('continuous');
            onPageColumnsEnabledChange(false);
          }}
          testId="viewer-scroll-continuous"
          testIdPrefix="viewer-continuous"
          gestureHint={continuousGestureHint.hint}
          suppressTooltip={continuousGestureHint.suppressTooltip}
          onShowGestureHint={() => showGestureHint('continuous', 'Double click to Fit Width')}
          onHideGestureHint={() => hideGestureHint('continuous')}
          onRestartTooltip={() => restartTooltip('continuous')}
          onDoubleClick={onFitWidth}
          onModeChange={onContinuousScrollWheelModeChange}
        />
        <ViewWheelControl
          active={scrollMode === 'single-page'}
          disabled={disabled}
          icon={RectangleVertical}
          label="Single Page View"
          mode={singlePageScrollWheelMode}
          onActivate={() => {
            onScrollModeChange('single-page');
            onPageColumnsEnabledChange(false);
          }}
          testId="viewer-scroll-single-page"
          testIdPrefix="viewer-single-page"
          gestureHint={singlePageGestureHint.hint}
          suppressTooltip={singlePageGestureHint.suppressTooltip}
          onShowGestureHint={() => showGestureHint('single-page', 'Double click to Fit Page')}
          onHideGestureHint={() => hideGestureHint('single-page')}
          onRestartTooltip={() => restartTooltip('single-page')}
          onDoubleClick={onFitPage}
          onModeChange={onSinglePageScrollWheelModeChange}
        />
        <CadViewButton
          disabled={disabled}
          scrollMode={scrollMode}
          pageColumnsEnabled={pageColumnsEnabled}
          cadViewOrganisation={cadViewOrganisation}
          pagesPerColumn={pagesPerColumn}
          onScrollModeChange={onScrollModeChange}
          onPageColumnsEnabledChange={onPageColumnsEnabledChange}
          onCadViewOrganisationChange={onCadViewOrganisationChange}
          onPagesPerColumnChange={onPagesPerColumnChange}
        />
      </ButtonGroup>
    </div>
  );
}
