import { Ellipsis } from 'lucide-react';
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type MouseEvent, type ReactNode, type WheelEvent } from 'react';
import {
  CONTROL_ICON_STROKE_WIDTH,
  RAIL_BUTTON_GAP,
  RAIL_BUTTON_SIZE,
} from './shellSpacing';
import { Tooltip, useTooltipDelay } from './Tooltip';

const RAIL_OVERFLOW_FADE_HEIGHT = 56;
const RAIL_OVERFLOW_ICON_SIZE = 22;
const RAIL_OVERFLOW_TOOLTIP = 'Scroll to see other tools';
const RAIL_OVERFLOW_BOTTOM_FADE = 'linear-gradient(to top, var(--background) 0%, var(--background) 72%, transparent 100%)';
const RAIL_OVERFLOW_TOP_FADE = 'linear-gradient(to bottom, var(--background) 0%, var(--background) 72%, transparent 100%)';
const RAIL_TOOLTIP_ATTRIBUTE = 'data-rail-tooltip';
const RAIL_DOUBLE_CLICK_TOOLTIP_ATTRIBUTE = 'data-rail-double-click-tooltip';
const RAIL_DOUBLE_CLICK_TOOLTIP_DURATION_MS = 2_000;

interface RailTooltipState {
  label: string;
  left: number;
  top: number;
  width: number;
}

interface RailTooltipRect {
  height: number;
  left: number;
  top: number;
  width: number;
}

export function resolveRailTooltipAnchor(
  triggerRect: RailTooltipRect,
  rootRect: Pick<RailTooltipRect, 'left' | 'top'>,
): Omit<RailTooltipState, 'label'> {
  return {
    left: triggerRect.left - rootRect.left,
    top: triggerRect.top - rootRect.top + triggerRect.height / 2,
    width: triggerRect.width,
  };
}

interface RailScrollAreaProps {
  children: ReactNode;
  contentClassName?: string;
  fixedContent?: ReactNode;
  overflowIndicatorTestId: string;
  overflowSide: 'left' | 'right';
  viewportTestId?: string;
}

export function RailScrollArea({
  children,
  contentClassName,
  fixedContent,
  overflowIndicatorTestId,
  overflowSide,
  viewportTestId,
}: RailScrollAreaProps) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const [canScrollUp, setCanScrollUp] = useState(false);
  const [canScrollDown, setCanScrollDown] = useState(false);
  const [tooltip, setTooltip] = useState<RailTooltipState | null>(null);
  const [doubleClickTooltip, setDoubleClickTooltip] = useState<RailTooltipState | null>(null);
  const activeTooltipTriggerRef = useRef<HTMLElement | null>(null);
  const doubleClickTooltipTriggerRef = useRef<HTMLElement | null>(null);
  const suppressedTooltipTriggerRef = useRef<HTMLElement | null>(null);
  const doubleClickTooltipTimerRef = useRef<number | null>(null);
  const canShowTooltip = useCallback(() => {
    const trigger = activeTooltipTriggerRef.current;
    if (!trigger) {
      return false;
    }

    const activeElement = trigger.ownerDocument.activeElement;
    return trigger.matches(':hover') || (activeElement instanceof Node && trigger.contains(activeElement));
  }, []);
  const {
    visible: tooltipVisible,
    hideTooltip: hideDelayedTooltip,
    showTooltip,
    showTooltipAfterDelay,
  } = useTooltipDelay({ canShow: canShowTooltip });

  const hideTooltip = useCallback(() => {
    activeTooltipTriggerRef.current = null;
    hideDelayedTooltip();
    setTooltip(null);
  }, [hideDelayedTooltip]);

  const clearDoubleClickTooltipTimer = useCallback(() => {
    if (doubleClickTooltipTimerRef.current === null) {
      return;
    }
    window.clearTimeout(doubleClickTooltipTimerRef.current);
    doubleClickTooltipTimerRef.current = null;
  }, []);

  const hideDoubleClickTooltip = useCallback((suppressOrdinaryTooltip = false) => {
    const trigger = doubleClickTooltipTriggerRef.current;
    clearDoubleClickTooltipTimer();
    doubleClickTooltipTriggerRef.current = null;
    suppressedTooltipTriggerRef.current = suppressOrdinaryTooltip ? trigger : null;
    setDoubleClickTooltip(null);
  }, [clearDoubleClickTooltipTimer]);

  const hideAllTooltips = useCallback(() => {
    hideDoubleClickTooltip();
    hideTooltip();
  }, [hideDoubleClickTooltip, hideTooltip]);

  const updateTooltip = useCallback((target: EventTarget | null, clearWhenMissing = true, immediate = false) => {
    if (!(target instanceof HTMLElement)) {
      if (clearWhenMissing) {
        hideTooltip();
      }
      return;
    }

    const trigger = target.closest<HTMLElement>(`[${RAIL_TOOLTIP_ATTRIBUTE}]`);
    if (!trigger || !rootRef.current?.contains(trigger)) {
      if (clearWhenMissing) {
        hideTooltip();
      }
      return;
    }

    if (doubleClickTooltipTriggerRef.current === trigger) {
      return;
    }
    if (doubleClickTooltipTriggerRef.current) {
      hideDoubleClickTooltip();
    }
    if (suppressedTooltipTriggerRef.current === trigger) {
      return;
    }
    suppressedTooltipTriggerRef.current = null;

    const label = trigger.getAttribute(RAIL_TOOLTIP_ATTRIBUTE);
    if (!label) {
      if (clearWhenMissing) {
        hideTooltip();
      }
      return;
    }

    const isSameTrigger = activeTooltipTriggerRef.current === trigger;
    const triggerRect = trigger.getBoundingClientRect();
    const rootRect = rootRef.current.getBoundingClientRect();
    activeTooltipTriggerRef.current = trigger;
    setTooltip({
      label,
      ...resolveRailTooltipAnchor(triggerRect, rootRect),
    });
    if (!canShowTooltip()) {
      hideTooltip();
    } else if (immediate) {
      showTooltip();
    } else if (!isSameTrigger) {
      showTooltipAfterDelay();
    }
  }, [canShowTooltip, hideDoubleClickTooltip, hideTooltip, showTooltip, showTooltipAfterDelay]);

  const showDoubleClickTooltip = useCallback((event: MouseEvent<HTMLDivElement>) => {
    if (event.detail > 1 || !(event.target instanceof HTMLElement) || !rootRef.current) {
      return;
    }

    const trigger = event.target.closest<HTMLElement>(`[${RAIL_DOUBLE_CLICK_TOOLTIP_ATTRIBUTE}]`);
    const label = trigger?.getAttribute(RAIL_DOUBLE_CLICK_TOOLTIP_ATTRIBUTE);
    if (!trigger || !label || !rootRef.current.contains(trigger)) {
      return;
    }

    const triggerRect = trigger.getBoundingClientRect();
    const rootRect = rootRef.current.getBoundingClientRect();
    clearDoubleClickTooltipTimer();
    hideTooltip();
    suppressedTooltipTriggerRef.current = null;
    doubleClickTooltipTriggerRef.current = trigger;
    setDoubleClickTooltip({
      label,
      ...resolveRailTooltipAnchor(triggerRect, rootRect),
    });
    doubleClickTooltipTimerRef.current = window.setTimeout(() => {
      doubleClickTooltipTimerRef.current = null;
      doubleClickTooltipTriggerRef.current = null;
      suppressedTooltipTriggerRef.current = trigger;
      setDoubleClickTooltip(null);
    }, RAIL_DOUBLE_CLICK_TOOLTIP_DURATION_MS);
  }, [clearDoubleClickTooltipTimer, hideTooltip]);

  useEffect(() => clearDoubleClickTooltipTimer, [clearDoubleClickTooltipTimer]);

  const updateOverflow = useCallback(() => {
    const viewport = viewportRef.current;
    if (!viewport) {
      setCanScrollUp(false);
      setCanScrollDown(false);
      return;
    }

    setCanScrollUp(viewport.scrollTop > 1);
    setCanScrollDown(viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop > 1);
    hideTooltip();
  }, [hideTooltip]);

  const handleViewportChange = useCallback(() => {
    hideDoubleClickTooltip();
    updateOverflow();
  }, [hideDoubleClickTooltip, updateOverflow]);

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }

    const observer = new ResizeObserver(handleViewportChange);
    observer.observe(viewport);
    if (contentRef.current) {
      observer.observe(contentRef.current);
    }

    viewport.addEventListener('scroll', handleViewportChange, { passive: true });
    updateOverflow();

    return () => {
      observer.disconnect();
      viewport.removeEventListener('scroll', handleViewportChange);
    };
  }, [handleViewportChange, updateOverflow]);

  useEffect(() => {
    updateOverflow();
  }, [children, updateOverflow]);

  const handleIndicatorWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }

    event.preventDefault();
    viewport.scrollTop += event.deltaY;
    handleViewportChange();
  }, [handleViewportChange]);

  const tooltipSide = overflowSide === 'left' ? 'left' : 'right';

  const renderScrollHint = (position: 'top' | 'bottom') => (
    <div
      className={[
        'pointer-events-none absolute inset-x-0 flex justify-center',
        position === 'top' ? 'top-0 items-start pt-0.5' : 'bottom-0 items-end pb-0.5',
      ].join(' ')}
      style={{
        height: RAIL_OVERFLOW_FADE_HEIGHT,
        background: position === 'top' ? RAIL_OVERFLOW_TOP_FADE : RAIL_OVERFLOW_BOTTOM_FADE,
      }}
    >
      <Tooltip
        side={tooltipSide}
        trigger={(
          <div
            className={[
              'relative inline-flex shrink-0 cursor-default items-center justify-center text-muted-foreground opacity-95 transition hover:opacity-100',
              'pointer-events-auto',
              RAIL_BUTTON_SIZE,
            ].join(' ')}
            data-testid={position === 'bottom' ? overflowIndicatorTestId : `${overflowIndicatorTestId}-top`}
            aria-label={RAIL_OVERFLOW_TOOLTIP}
            onWheel={handleIndicatorWheel}
          >
            <Ellipsis
              size={RAIL_OVERFLOW_ICON_SIZE}
              strokeWidth={CONTROL_ICON_STROKE_WIDTH}
              className="h-[22px] w-[22px] shrink-0"
              aria-hidden="true"
            />
          </div>
        )}
      >
        {RAIL_OVERFLOW_TOOLTIP}
      </Tooltip>
    </div>
  );

  return (
    <div
      ref={rootRef}
      className="relative flex min-h-0 w-full flex-1 flex-col overflow-visible"
      onBlurCapture={(event) => {
        if (!rootRef.current?.contains(event.relatedTarget as Node | null)) {
          hideAllTooltips();
        }
      }}
      onClick={showDoubleClickTooltip}
      onDoubleClick={() => hideDoubleClickTooltip(true)}
      onFocusCapture={(event) => updateTooltip(event.target, true, true)}
      onPointerLeave={hideAllTooltips}
      onPointerMove={(event) => updateTooltip(event.target)}
      onPointerOver={(event) => updateTooltip(event.target)}
    >
      {fixedContent}
      <div
        ref={viewportRef}
        className="bp-native-scroll-hidden min-h-0 flex-1 overflow-y-auto overflow-x-hidden"
        data-testid={viewportTestId}
      >
        <div
          ref={contentRef}
          className={['flex flex-col items-center', RAIL_BUTTON_GAP, contentClassName].filter(Boolean).join(' ')}
        >
          {children}
        </div>
      </div>
      {doubleClickTooltip ? (
        <Tooltip
          side={tooltipSide}
          style={{
            left: doubleClickTooltip.left,
            top: doubleClickTooltip.top,
            width: doubleClickTooltip.width,
          }}
          testId="rail-double-click-tooltip"
        >
          {doubleClickTooltip.label}
        </Tooltip>
      ) : tooltip && tooltipVisible ? (
        <Tooltip
          side={tooltipSide}
          style={{ left: tooltip.left, top: tooltip.top, width: tooltip.width }}
        >
          {tooltip.label}
        </Tooltip>
      ) : null}
      {canScrollUp ? renderScrollHint('top') : null}
      {canScrollDown ? renderScrollHint('bottom') : null}
    </div>
  );
}
