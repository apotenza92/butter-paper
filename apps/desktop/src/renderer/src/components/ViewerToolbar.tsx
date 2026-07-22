import {
  Check,
  ChevronDown,
  Expand,
  Grid2x2,
  Magnet,
  MoveHorizontal,
  MoveVertical,
  RectangleVertical,
  RotateCcw,
  Search,
  Shapes,
  Square,
  VectorSquare,
  ZoomIn,
  ZoomOut,
  type LucideIcon,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState, type ComponentType, type Dispatch, type MouseEvent as ReactMouseEvent, type ReactNode, type RefObject, type SetStateAction } from 'react';
import {
  CONTROL_ACTIVE,
  CONTROL_DEFAULT,
  CONTROL_DISABLED,
  CONTROL_ICON_SIZE,
  CONTROL_ICON_SIZE_CLASS,
  CONTROL_ICON_STROKE_WIDTH,
  MENU_DROPDOWN,
  MENU_ITEM_DEFAULT,
  PRIMARY_BAND_HEIGHT,
  SHELL_CONTROL_GAP,
  SHELL_DIVIDER,
  SHELL_SURFACE_PANEL,
  VIEWER_TOOLBAR_BUTTON_SIZE,
  VIEWER_TOOLBAR_INSET_X,
} from './shellSpacing';
import { Tooltip, useTooltipDelay } from './Tooltip';
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
}

interface ToolbarIconProps {
  'aria-hidden'?: boolean | 'false' | 'true';
  className?: string;
  size?: string | number;
  strokeWidth?: string | number;
}

type ToolbarIconComponent = ComponentType<ToolbarIconProps>;

function scaledIconSize(size: string | number, scale: number): string | number {
  return typeof size === 'number' ? size * scale : size;
}

function FitWidthIcon({ size = 24, strokeWidth = 2, className, 'aria-hidden': ariaHidden }: ToolbarIconProps) {
  return (
    <span
      aria-hidden={ariaHidden}
      className={['relative inline-flex items-center justify-center', className].filter(Boolean).join(' ')}
      style={{ width: size, height: size }}
    >
      <Square
        size={size}
        strokeWidth={strokeWidth}
        className="absolute inset-0 h-full w-full"
      />
      <MoveHorizontal
        size={scaledIconSize(size, 0.58)}
        strokeWidth={strokeWidth}
        className="absolute"
      />
    </span>
  );
}

function FitPageIcon({ size = 24, strokeWidth = 2, className, 'aria-hidden': ariaHidden }: ToolbarIconProps) {
  return (
    <span
      aria-hidden={ariaHidden}
      className={['relative inline-flex items-center justify-center', className].filter(Boolean).join(' ')}
      style={{ width: size, height: size }}
    >
      <Square
        size={size}
        strokeWidth={strokeWidth}
        className="absolute inset-0 h-full w-full"
      />
      <Expand
        size={scaledIconSize(size, 0.48)}
        strokeWidth={strokeWidth}
        className="absolute"
      />
    </span>
  );
}

function ContinuousIcon({ size = 24, strokeWidth = 2, className, 'aria-hidden': ariaHidden }: ToolbarIconProps) {
  return (
    <span
      aria-hidden={ariaHidden}
      className={['relative inline-flex items-center justify-center', className].filter(Boolean).join(' ')}
      style={{ width: size, height: size }}
    >
      <RectangleVertical
        size={size}
        strokeWidth={strokeWidth}
        className="absolute inset-0 h-full w-full"
      />
      <svg
        aria-hidden="true"
        className="absolute inset-0 h-full w-full"
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

function ToolbarButton({
  active,
  disabled,
  children,
  onClick,
  onDoubleClick,
  suppressTooltip,
  testId,
  title,
  ariaPressed,
  className,
}: {
  active?: boolean;
  disabled?: boolean;
  children: ReactNode;
  onClick?: (event: ReactMouseEvent<HTMLButtonElement>) => void;
  onDoubleClick?: () => void;
  suppressTooltip?: boolean;
  testId?: string;
  title?: string;
  ariaPressed?: boolean;
  className?: string;
}) {
  const buttonRef = useRef<HTMLButtonElement | null>(null);
  const canShowTooltip = useCallback(() => {
    const button = buttonRef.current;
    if (!button) {
      return false;
    }

    const activeElement = button.ownerDocument.activeElement;
    return button.matches(':hover') || (activeElement instanceof Node && button.contains(activeElement));
  }, []);
  const tooltip = useTooltipDelay({ canShow: canShowTooltip, disabled, suppressed: suppressTooltip });
  const tooltipVisible = tooltip.visible;
  const showTooltip = Boolean(title && tooltipVisible && !disabled && !suppressTooltip);

  return (
    <button
      ref={buttonRef}
      type="button"
      disabled={disabled}
      data-testid={testId}
      aria-label={title}
      aria-pressed={ariaPressed}
      className={[
        'relative inline-flex items-center justify-center rounded-[6px] border px-2 text-[12px] font-medium transition',
        VIEWER_TOOLBAR_BUTTON_SIZE,
        disabled ? CONTROL_DISABLED : active ? CONTROL_ACTIVE : CONTROL_DEFAULT,
        className,
      ].join(' ')}
      onBlur={tooltip.hideTooltip}
      onClick={(event) => {
        tooltip.hideTooltip();
        onClick?.(event);
      }}
      onDoubleClick={() => {
        tooltip.hideTooltip();
        onDoubleClick?.();
      }}
      onFocus={tooltip.showTooltip}
      onPointerEnter={tooltip.showTooltipAfterDelay}
      onPointerLeave={tooltip.hideTooltip}
    >
      {children}
      {showTooltip ? (
        <Tooltip testId={testId ? `${testId}-tooltip` : undefined}>
          {title}
        </Tooltip>
      ) : null}
    </button>
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
  return (
    <ToolbarButton
      active={active}
      ariaPressed={active}
      className="w-8 px-0"
      disabled={disabled}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      suppressTooltip={suppressTooltip}
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
}

function useToolbarDropdown(): {
  open: boolean;
  setOpen: Dispatch<SetStateAction<boolean>>;
  rootRef: RefObject<HTMLDivElement | null>;
} {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) {
      return undefined;
    }

    function handlePointerDown(event: PointerEvent): void {
      if (rootRef.current?.contains(event.target as Node)) {
        return;
      }
      setOpen(false);
    }

    window.addEventListener('pointerdown', handlePointerDown);
    return () => window.removeEventListener('pointerdown', handlePointerDown);
  }, [open]);

  return { open, setOpen, rootRef };
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
  const { open, setOpen, rootRef } = useToolbarDropdown();
  const zoomLabel = formatZoomPercent(zoom);

  useEffect(() => {
    if (disabled) {
      setOpen(false);
    }
  }, [disabled, setOpen]);

  return (
    <div ref={rootRef} className="relative">
      <ToolbarButton
        active={open && !disabled}
        className="min-w-[68px] gap-1.5 px-2 tabular-nums"
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        testId="viewer-zoom-menu"
        title={`Zoom ${zoomLabel}`}
        ariaPressed={open}
      >
        <span>{zoomLabel}</span>
        <ChevronDown
          size={CONTROL_ICON_SIZE}
          strokeWidth={CONTROL_ICON_STROKE_WIDTH}
          className={CONTROL_ICON_SIZE_CLASS}
          aria-hidden="true"
        />
      </ToolbarButton>
      {open ? (
        <div className={['absolute left-0 top-[calc(100%+4px)] z-50 max-h-[calc(100vh-72px)] min-w-[112px] overflow-auto rounded-[6px] border p-1', MENU_DROPDOWN].join(' ')}>
          {zoomPresetOptions.map((option) => {
            const selected = Math.abs(zoom - option) < 0.001;
            const label = formatZoomPercent(option);
            return (
              <button
                key={option}
                type="button"
                className={['flex h-8 w-full items-center gap-2 rounded-[4px] px-2 text-left text-[12px] transition', MENU_ITEM_DEFAULT].join(' ')}
                data-testid={`viewer-zoom-preset-${Math.round(option * 100)}`}
                onClick={() => {
                  onZoomChange(option);
                  setOpen(false);
                }}
              >
                <span className="flex h-4 w-4 items-center justify-center">
                  {selected ? <Check size={14} strokeWidth={2} aria-hidden="true" /> : null}
                </span>
                <span className="whitespace-nowrap tabular-nums">{label}</span>
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
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
  const { open, setOpen, rootRef } = useToolbarDropdown();
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
    <div ref={rootRef} className="relative" data-testid="viewer-snap-controls">
      <ToolbarButton
        active={!disabled && (open || snapSettings.snapToContent || snapSettings.snapToMarkup)}
        className="gap-1.5 px-2"
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        testId="viewer-snap-target-menu"
        title="Snap"
        ariaPressed={open}
      >
        <Magnet
          size={CONTROL_ICON_SIZE}
          strokeWidth={CONTROL_ICON_STROKE_WIDTH}
          className={CONTROL_ICON_SIZE_CLASS}
          aria-hidden="true"
        />
        <ChevronDown
          size={CONTROL_ICON_SIZE}
          strokeWidth={CONTROL_ICON_STROKE_WIDTH}
          className={CONTROL_ICON_SIZE_CLASS}
          aria-hidden="true"
        />
      </ToolbarButton>
      {open ? (
        <div className={['absolute left-0 top-[calc(100%+4px)] z-50 min-w-[240px] rounded-[6px] border p-1', MENU_DROPDOWN].join(' ')}>
          <button
            type="button"
            className={['flex h-8 w-full items-center gap-2 rounded-[4px] px-2 text-left text-[12px] transition', MENU_ITEM_DEFAULT].join(' ')}
            data-testid="viewer-snap-content"
            onClick={() => onSnapSettingsChange({ snapToContent: !snapSettings.snapToContent })}
          >
            <span className="flex h-4 w-4 items-center justify-center">
              {snapSettings.snapToContent ? <Check size={14} strokeWidth={2} aria-hidden="true" /> : null}
            </span>
            <VectorSquare
              size={CONTROL_ICON_SIZE}
              strokeWidth={CONTROL_ICON_STROKE_WIDTH}
              className={CONTROL_ICON_SIZE_CLASS}
              aria-hidden="true"
            />
            <span>Content</span>
          </button>
          <button
            type="button"
            className={['flex h-8 w-full items-center gap-2 rounded-[4px] px-2 text-left text-[12px] transition', MENU_ITEM_DEFAULT].join(' ')}
            data-testid="viewer-snap-markup"
            onClick={() => onSnapSettingsChange({ snapToMarkup: !snapSettings.snapToMarkup })}
          >
            <span className="flex h-4 w-4 items-center justify-center">
              {snapSettings.snapToMarkup ? <Check size={14} strokeWidth={2} aria-hidden="true" /> : null}
            </span>
            <Shapes
              size={CONTROL_ICON_SIZE}
              strokeWidth={CONTROL_ICON_STROKE_WIDTH}
              className={CONTROL_ICON_SIZE_CLASS}
              aria-hidden="true"
            />
            <span>Markup</span>
          </button>
          <div className={['my-1 h-px', SHELL_DIVIDER].join(' ')} />
          {snapTargetOptions.map((option) => {
            const selected = selectedTargets.has(option.target);
            return (
              <button
                key={option.target}
                type="button"
                className={['flex h-8 w-full items-center gap-2 rounded-[4px] px-2 text-left text-[12px] transition', MENU_ITEM_DEFAULT].join(' ')}
                data-testid={`viewer-snap-target-${option.target}`}
                onClick={() => toggleTarget(option.target)}
              >
                <span className="flex h-4 w-4 items-center justify-center">
                  {selected ? <Check size={14} strokeWidth={2} aria-hidden="true" /> : null}
                </span>
                <span className="flex min-w-0 flex-1 items-center justify-between gap-3">
                  <span>{option.label}</span>
                  <span className="bp-text-muted text-[11px]">{option.hint}</span>
                </span>
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

const wheelModeOptions: ReadonlyArray<{ mode: ScrollWheelMode; label: string; icon: LucideIcon }> = [
  { mode: 'zoom', label: 'Mousewheel Zoom', icon: Search },
  { mode: 'scroll', label: 'Mousewheel Scroll', icon: MoveVertical },
];
const ACTIVE_VIEW_CHEVRON_CLASS = CONTROL_ICON_SIZE_CLASS;
const INACTIVE_VIEW_CHEVRON_CLASS = `${CONTROL_ICON_SIZE_CLASS} opacity-35`;

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
    <>
      {wheelModeOptions.map((option) => {
        const selected = mode === option.mode;
        const Icon = option.icon;
        return (
          <button
            key={option.mode}
            type="button"
            className={['flex h-8 w-full items-center gap-2 rounded-[4px] px-2 text-left text-[12px] transition', MENU_ITEM_DEFAULT].join(' ')}
            data-testid={`${testIdPrefix}-wheel-${option.mode}`}
            onClick={() => onModeChange(option.mode)}
          >
            <span className="flex h-4 w-4 items-center justify-center">
              {selected ? <Check size={14} strokeWidth={2} aria-hidden="true" /> : null}
            </span>
            <Icon
              size={CONTROL_ICON_SIZE}
              strokeWidth={CONTROL_ICON_STROKE_WIDTH}
              className={CONTROL_ICON_SIZE_CLASS}
              aria-hidden="true"
            />
            <span className="whitespace-nowrap">{option.label}</span>
          </button>
        );
      })}
    </>
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
  const { open, setOpen, rootRef } = useToolbarDropdown();
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

  return (
    <div ref={rootRef} className="relative">
      <ToolbarButton
        active={!disabled && active}
        ariaPressed={active}
        className="gap-1 px-2"
        disabled={disabled}
        onClick={(event) => {
          if (!active) {
            onActivate();
            setOpen(false);
            return;
          }

          if (event.detail > 1) {
            return;
          }

          cancelPendingDropdownOpen();
          dropdownOpenTimerRef.current = window.setTimeout(() => {
            dropdownOpenTimerRef.current = null;
            setOpen((current) => !current);
          }, onDoubleClick ? VIEW_DROPDOWN_OPEN_DELAY_MS : 0);
        }}
        onDoubleClick={() => {
          cancelPendingDropdownOpen();
          setOpen(false);
          onDoubleClick?.();
        }}
        suppressTooltip={suppressTooltip}
        testId={testId}
        title={label}
      >
        <Icon
          size={CONTROL_ICON_SIZE}
          strokeWidth={CONTROL_ICON_STROKE_WIDTH}
          className={CONTROL_ICON_SIZE_CLASS}
          aria-hidden="true"
        />
        <ChevronDown
          size={CONTROL_ICON_SIZE}
          strokeWidth={CONTROL_ICON_STROKE_WIDTH}
          className={active ? ACTIVE_VIEW_CHEVRON_CLASS : INACTIVE_VIEW_CHEVRON_CLASS}
          aria-hidden="true"
        />
      </ToolbarButton>
      {open ? (
        <div className={['absolute left-0 top-[calc(100%+4px)] z-50 min-w-[190px] rounded-[6px] border p-1', MENU_DROPDOWN].join(' ')}>
          <div className="bp-text-muted px-2 pb-1 pt-1 text-[11px] font-medium">Mousewheel Behaviour</div>
          <WheelBehaviourMenuItems
            mode={mode}
            testIdPrefix={testIdPrefix}
            onModeChange={onModeChange}
          />
          <div className="bp-text-muted px-2 pb-1 pt-1 text-[11px] leading-4">Ctrl + mousewheel does the opposite.</div>
        </div>
      ) : null}
    </div>
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
  const { open, setOpen, rootRef } = useToolbarDropdown();
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
    <div ref={rootRef} className="relative">
      <ToolbarButton
        active={!disabled && cadViewActive}
        ariaPressed={cadViewActive}
        className="gap-1 px-2"
        disabled={disabled}
        onClick={() => {
          if (!cadViewActive) {
            onScrollModeChange('continuous');
            onPageColumnsEnabledChange(true);
            setOpen(false);
            return;
          }

          setOpen((current) => !current);
        }}
        testId="viewer-cad-view"
        title="CAD View"
      >
        <Grid2x2
          size={CONTROL_ICON_SIZE}
          strokeWidth={CONTROL_ICON_STROKE_WIDTH}
          className={CONTROL_ICON_SIZE_CLASS}
          aria-hidden="true"
        />
        <ChevronDown
          size={CONTROL_ICON_SIZE}
          strokeWidth={CONTROL_ICON_STROKE_WIDTH}
          className={cadViewActive ? ACTIVE_VIEW_CHEVRON_CLASS : INACTIVE_VIEW_CHEVRON_CLASS}
          aria-hidden="true"
        />
      </ToolbarButton>
      {open ? (
        <div className={['absolute left-0 top-[calc(100%+4px)] z-50 min-w-[210px] rounded-[6px] border p-2', MENU_DROPDOWN].join(' ')}>
          <div className="bp-text-muted px-2 pb-1 text-[11px] font-medium">Organise by</div>
          <div className="grid grid-cols-2 gap-1">
            {([
              ['columns', 'Columns'],
              ['rows', 'Rows'],
            ] as const).map(([organisation, label]) => {
              const selected = cadViewOrganisation === organisation;
              return (
                <button
                  key={organisation}
                  type="button"
                  className={[
                    'flex h-8 items-center justify-center rounded-[4px] border px-2 text-[12px] transition',
                    selected ? CONTROL_ACTIVE : `${MENU_ITEM_DEFAULT} border-transparent`,
                  ].join(' ')}
                  data-testid={`viewer-cad-organisation-${organisation}`}
                  onClick={() => onCadViewOrganisationChange(organisation)}
                >
                  {label}
                </button>
              );
            })}
          </div>
          <label className="mt-2 flex items-center justify-between gap-3 px-2 text-[12px]">
            <span className="bp-text-muted">{countLabel}</span>
            <input
              type="number"
              min={1}
              max={100}
              step={1}
              value={pagesPerColumn}
              className="h-6 w-14 rounded-[4px] border border-neutral-300 bg-transparent px-1 text-right text-[12px] disabled:opacity-50"
              data-testid={cadViewOrganisation === 'columns' ? 'viewer-pages-per-column' : 'viewer-pages-per-row'}
              onChange={(event) => handlePageCountChange(event.currentTarget.value)}
            />
          </label>
          <div className="bp-text-muted mt-2 px-2 pb-1 pt-1 text-[11px] font-medium">Mousewheel Behaviour</div>
          <WheelBehaviourMenuItems
            mode={cadScrollWheelMode}
            testIdPrefix="viewer-cad"
            onModeChange={onCadScrollWheelModeChange}
          />
          <div className="bp-text-muted px-2 pb-1 pt-1 text-[11px] leading-4">Ctrl + mousewheel does the opposite.</div>
        </div>
      ) : null}
    </div>
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
        'bp-border-bottom-inset flex items-center justify-center',
        PRIMARY_BAND_HEIGHT,
        VIEWER_TOOLBAR_INSET_X,
        SHELL_CONTROL_GAP,
        SHELL_SURFACE_PANEL,
      ].join(' ')}
      data-testid="viewer-toolbar"
    >
      <div className={['flex items-center', SHELL_CONTROL_GAP].join(' ')}>
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
      </div>

      <div className={['h-5 w-px', SHELL_DIVIDER].join(' ')} />

      <div className={['flex items-center', SHELL_CONTROL_GAP].join(' ')}>
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
      </div>

      <div className={['h-5 w-px', SHELL_DIVIDER].join(' ')} />

      <div className={['flex items-center', SHELL_CONTROL_GAP].join(' ')}>
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
      </div>

      <div className={['h-5 w-px', SHELL_DIVIDER].join(' ')} />

      <SnapDropdown
        disabled={disabled}
        snapSettings={snapSettings}
        onSnapSettingsChange={onSnapSettingsChange}
      />
    </div>
  );
}
