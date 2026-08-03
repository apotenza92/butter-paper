import { Ellipsis } from 'lucide-react';
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode, type WheelEvent } from 'react';
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
  overflowIndicatorTestId: string;
  overflowSide: 'left' | 'right';
  viewportTestId?: string;
}

export function RailScrollArea({
  children,
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
  const activeTooltipTriggerRef = useRef<HTMLElement | null>(null);
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

  const updateTooltip = useCallback((target: EventTarget | null, clearWhenMissing = true, immediate = false) => {
    if (!(target instanceof HTMLElement)) {
      if (clearWhenMissing) {
        hideTooltip();
      }
      return;
    }

    const trigger = target.closest<HTMLElement>(`[${RAIL_TOOLTIP_ATTRIBUTE}]`);
    if (!trigger || !contentRef.current?.contains(trigger) || !rootRef.current) {
      if (clearWhenMissing) {
        hideTooltip();
      }
      return;
    }

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
  }, [canShowTooltip, hideTooltip, showTooltip, showTooltipAfterDelay]);

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

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }

    const observer = new ResizeObserver(updateOverflow);
    observer.observe(viewport);
    if (contentRef.current) {
      observer.observe(contentRef.current);
    }

    viewport.addEventListener('scroll', updateOverflow, { passive: true });
    updateOverflow();

    return () => {
      observer.disconnect();
      viewport.removeEventListener('scroll', updateOverflow);
    };
  }, [updateOverflow]);

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
    updateOverflow();
  }, [updateOverflow]);

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
    <div ref={rootRef} className="relative min-h-0 w-full flex-1 overflow-visible">
      <div
        ref={viewportRef}
        className="bp-native-scroll-hidden h-full overflow-y-auto overflow-x-hidden"
        data-testid={viewportTestId}
      >
        <div
          ref={contentRef}
          className={['flex flex-col items-center', RAIL_BUTTON_GAP].join(' ')}
          onBlurCapture={(event) => {
            if (!contentRef.current?.contains(event.relatedTarget as Node | null)) {
              hideTooltip();
            }
          }}
          onFocusCapture={(event) => updateTooltip(event.target, true, true)}
          onPointerLeave={hideTooltip}
          onPointerMove={(event) => updateTooltip(event.target)}
          onPointerOver={(event) => updateTooltip(event.target)}
        >
          {children}
        </div>
      </div>
      {tooltip && tooltipVisible ? (
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
