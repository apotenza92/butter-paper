import {
  forwardRef,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type HTMLAttributes,
  type MutableRefObject,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from 'react';
import {
  getScrollbarAxisMetrics,
  mapThumbDragDeltaToScrollOffset,
  mapTrackClickToScrollOffset,
  type ScrollbarAxisMetrics,
} from '../utils/scrollbarMath';
import { CUSTOM_SCROLLBAR_MIN_THUMB_SIZE, CUSTOM_SCROLLBAR_SIZE } from './scrollbarSizing';

const DEFAULT_SCROLLBAR_SIZE = CUSTOM_SCROLLBAR_SIZE;
const DEFAULT_MIN_THUMB_SIZE = CUSTOM_SCROLLBAR_MIN_THUMB_SIZE;

interface DragState {
  axis: 'x' | 'y';
  pointerId: number;
  startPointerOffset: number;
  startScrollOffset: number;
}

export interface CustomScrollAreaProps {
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
  viewportClassName?: string;
  viewportStyle?: CSSProperties;
  viewportProps?: Omit<HTMLAttributes<HTMLDivElement>, 'children' | 'className' | 'style'>;
  viewportTestId?: string;
  contentClassName?: string;
  orientation?: 'vertical' | 'both';
  scrollbarSize?: number;
  minThumbSize?: number;
  verticalTrackTestId?: string;
  verticalThumbTestId?: string;
  horizontalTrackTestId?: string;
  horizontalThumbTestId?: string;
  cornerTestId?: string;
}

function joinClassNames(...values: Array<string | undefined | false | null>): string {
  return values.filter(Boolean).join(' ');
}

export const CustomScrollArea = forwardRef<HTMLDivElement, CustomScrollAreaProps>(function CustomScrollArea(
  {
    children,
    className,
    style,
    viewportClassName,
    viewportStyle,
    viewportProps,
    viewportTestId,
    contentClassName,
    orientation = 'vertical',
    scrollbarSize = DEFAULT_SCROLLBAR_SIZE,
    minThumbSize = DEFAULT_MIN_THUMB_SIZE,
    verticalTrackTestId,
    verticalThumbTestId,
    horizontalTrackTestId,
    horizontalThumbTestId,
    cornerTestId,
  },
  forwardedRef,
) {
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const verticalTrackRef = useRef<HTMLDivElement | null>(null);
  const verticalThumbRef = useRef<HTMLDivElement | null>(null);
  const horizontalTrackRef = useRef<HTMLDivElement | null>(null);
  const horizontalThumbRef = useRef<HTMLDivElement | null>(null);
  const dragStateRef = useRef<DragState | null>(null);
  const frameRef = useRef<number | null>(null);
  const verticalMetricsRef = useRef<ScrollbarAxisMetrics>({
    hasOverflow: false,
    maxScrollOffset: 0,
    trackSize: 0,
    thumbSize: 0,
    thumbOffset: 0,
    maxThumbOffset: 0,
  });
  const horizontalMetricsRef = useRef<ScrollbarAxisMetrics>({
    hasOverflow: false,
    maxScrollOffset: 0,
    trackSize: 0,
    thumbSize: 0,
    thumbOffset: 0,
    maxThumbOffset: 0,
  });
  const [overflowState, setOverflowState] = useState({ x: false, y: false });

  const setViewportRef = useCallback(
    (node: HTMLDivElement | null) => {
      viewportRef.current = node;
      if (typeof forwardedRef === 'function') {
        forwardedRef(node);
        return;
      }

      if (forwardedRef) {
        (forwardedRef as MutableRefObject<HTMLDivElement | null>).current = node;
      }
    },
    [forwardedRef],
  );

  const applyMetrics = useCallback(() => {
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }

    const verticalMetrics = getScrollbarAxisMetrics({
      clientSize: viewport.clientHeight,
      scrollSize: viewport.scrollHeight,
      scrollOffset: viewport.scrollTop,
      trackSize: verticalTrackRef.current?.clientHeight ?? 0,
      minThumbSize,
    });
    const horizontalMetrics = orientation === 'both'
      ? getScrollbarAxisMetrics({
          clientSize: viewport.clientWidth,
          scrollSize: viewport.scrollWidth,
          scrollOffset: viewport.scrollLeft,
          trackSize: horizontalTrackRef.current?.clientWidth ?? 0,
          minThumbSize,
        })
      : {
          hasOverflow: false,
          maxScrollOffset: 0,
          trackSize: 0,
          thumbSize: 0,
          thumbOffset: 0,
          maxThumbOffset: 0,
        };

    verticalMetricsRef.current = verticalMetrics;
    horizontalMetricsRef.current = horizontalMetrics;

    if (verticalThumbRef.current) {
      verticalThumbRef.current.style.height = `${verticalMetrics.thumbSize}px`;
      verticalThumbRef.current.style.transform = `translateY(${verticalMetrics.thumbOffset}px)`;
      verticalThumbRef.current.style.opacity = verticalMetrics.hasOverflow ? '1' : '0';
      verticalThumbRef.current.style.pointerEvents = verticalMetrics.hasOverflow ? 'auto' : 'none';
    }

    if (horizontalThumbRef.current) {
      horizontalThumbRef.current.style.width = `${horizontalMetrics.thumbSize}px`;
      horizontalThumbRef.current.style.transform = `translateX(${horizontalMetrics.thumbOffset}px)`;
      horizontalThumbRef.current.style.opacity = horizontalMetrics.hasOverflow ? '1' : '0';
      horizontalThumbRef.current.style.pointerEvents = horizontalMetrics.hasOverflow ? 'auto' : 'none';
    }

    setOverflowState((current) => {
      if (current.x === horizontalMetrics.hasOverflow && current.y === verticalMetrics.hasOverflow) {
        return current;
      }

      return {
        x: horizontalMetrics.hasOverflow,
        y: verticalMetrics.hasOverflow,
      };
    });
  }, [minThumbSize, orientation]);

  const scheduleApplyMetrics = useCallback(() => {
    if (frameRef.current !== null) {
      return;
    }

    frameRef.current = window.requestAnimationFrame(() => {
      frameRef.current = null;
      applyMetrics();
    });
  }, [applyMetrics]);

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    const content = contentRef.current;
    if (!viewport) {
      return;
    }

    const handleScroll = () => {
      scheduleApplyMetrics();
    };

    const observer = new ResizeObserver(() => {
      scheduleApplyMetrics();
    });

    viewport.addEventListener('scroll', handleScroll, { passive: true });
    observer.observe(viewport);
    if (content) {
      observer.observe(content);
    }
    scheduleApplyMetrics();

    return () => {
      viewport.removeEventListener('scroll', handleScroll);
      observer.disconnect();
      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
    };
  }, [scheduleApplyMetrics]);

  useEffect(() => {
    scheduleApplyMetrics();
  }, [children, orientation, scheduleApplyMetrics, scrollbarSize]);

  useEffect(() => {
    scheduleApplyMetrics();
  }, [overflowState.x, overflowState.y, scheduleApplyMetrics]);

  function updateViewportScroll(axis: 'x' | 'y', nextScrollOffset: number): void {
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }

    if (axis === 'y') {
      viewport.scrollTop = nextScrollOffset;
    } else {
      viewport.scrollLeft = nextScrollOffset;
    }

    scheduleApplyMetrics();
  }

  function startThumbDrag(axis: 'x' | 'y', event: ReactPointerEvent<HTMLDivElement>): void {
    if (event.button !== 0) {
      return;
    }

    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }

    dragStateRef.current = {
      axis,
      pointerId: event.pointerId,
      startPointerOffset: axis === 'y' ? event.clientY : event.clientX,
      startScrollOffset: axis === 'y' ? viewport.scrollTop : viewport.scrollLeft,
    };

    event.currentTarget.setPointerCapture(event.pointerId);
    event.preventDefault();
    event.stopPropagation();
  }

  function handleThumbPointerMove(axis: 'x' | 'y', event: ReactPointerEvent<HTMLDivElement>): void {
    const dragState = dragStateRef.current;
    if (!dragState || dragState.axis !== axis || dragState.pointerId !== event.pointerId) {
      return;
    }

    const metrics = axis === 'y' ? verticalMetricsRef.current : horizontalMetricsRef.current;
    const pointerOffset = axis === 'y' ? event.clientY : event.clientX;
    const delta = pointerOffset - dragState.startPointerOffset;
    const nextScrollOffset = mapThumbDragDeltaToScrollOffset(delta, dragState.startScrollOffset, metrics);
    updateViewportScroll(axis, nextScrollOffset);
  }

  function finishThumbDrag(axis: 'x' | 'y', event: ReactPointerEvent<HTMLDivElement>): void {
    const dragState = dragStateRef.current;
    if (!dragState || dragState.axis !== axis || dragState.pointerId !== event.pointerId) {
      return;
    }

    dragStateRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  }

  function handleTrackPointerDown(axis: 'x' | 'y', event: ReactPointerEvent<HTMLDivElement>): void {
    if (event.button !== 0 || event.target !== event.currentTarget) {
      return;
    }

    const metrics = axis === 'y' ? verticalMetricsRef.current : horizontalMetricsRef.current;
    if (!metrics.hasOverflow) {
      return;
    }

    const rect = event.currentTarget.getBoundingClientRect();
    const clickOffset = axis === 'y' ? event.clientY - rect.top : event.clientX - rect.left;
    const nextScrollOffset = mapTrackClickToScrollOffset(clickOffset, metrics);
    updateViewportScroll(axis, nextScrollOffset);
    event.preventDefault();
  }

  const showVerticalTrack = overflowState.y;
  const showHorizontalTrack = orientation === 'both' && overflowState.x;
  const showCorner = showVerticalTrack && showHorizontalTrack;
  const rootStyle: CSSProperties =
    orientation === 'both'
      ? {
          ...style,
          gridTemplateColumns: `minmax(0, 1fr) ${showVerticalTrack ? scrollbarSize : 0}px`,
          gridTemplateRows: `minmax(0, 1fr) ${showHorizontalTrack ? scrollbarSize : 0}px`,
        }
      : {
          ...style,
          gridTemplateColumns: `minmax(0, 1fr) ${showVerticalTrack ? scrollbarSize : 0}px`,
          gridTemplateRows: 'minmax(0, 1fr)',
        };

  return (
    <div className={joinClassNames('bp-custom-scroll-area grid min-h-0 min-w-0', className)} style={rootStyle}>
      <div
        ref={setViewportRef}
        className={joinClassNames('bp-custom-scroll-area-viewport bp-native-scroll-hidden min-h-0 min-w-0 overflow-auto', viewportClassName)}
        style={{ gridColumn: '1 / 2', gridRow: '1 / 2', ...viewportStyle }}
        data-testid={viewportTestId}
        {...viewportProps}
      >
        <div ref={contentRef} className={joinClassNames('bp-custom-scroll-area-content min-h-full min-w-full', contentClassName)}>
          {children}
        </div>
      </div>

      {showVerticalTrack ? (
        <div
          ref={verticalTrackRef}
          className="bp-custom-scrollbar-track bp-custom-scrollbar-track-y relative"
          data-overflow={overflowState.y}
          data-testid={verticalTrackTestId}
          style={{ gridColumn: '2 / 3', gridRow: '1 / 2' }}
          onPointerDown={(event) => handleTrackPointerDown('y', event)}
        >
          <div
            ref={verticalThumbRef}
            className="bp-custom-scrollbar-thumb bp-custom-scrollbar-thumb-y absolute"
            data-testid={verticalThumbTestId}
            onPointerDown={(event) => startThumbDrag('y', event)}
            onPointerMove={(event) => handleThumbPointerMove('y', event)}
            onPointerUp={(event) => finishThumbDrag('y', event)}
            onPointerCancel={(event) => finishThumbDrag('y', event)}
          />
        </div>
      ) : null}

      {showHorizontalTrack ? (
        <>
          <div
            ref={horizontalTrackRef}
            className="bp-custom-scrollbar-track bp-custom-scrollbar-track-x relative"
            data-overflow={overflowState.x}
            data-testid={horizontalTrackTestId}
            style={{ gridColumn: '1 / 2', gridRow: '2 / 3' }}
            onPointerDown={(event) => handleTrackPointerDown('x', event)}
          >
            <div
              ref={horizontalThumbRef}
              className="bp-custom-scrollbar-thumb bp-custom-scrollbar-thumb-x absolute"
              data-testid={horizontalThumbTestId}
              onPointerDown={(event) => startThumbDrag('x', event)}
              onPointerMove={(event) => handleThumbPointerMove('x', event)}
              onPointerUp={(event) => finishThumbDrag('x', event)}
              onPointerCancel={(event) => finishThumbDrag('x', event)}
            />
          </div>
          {showCorner ? (
            <div
              className="bp-custom-scrollbar-corner"
              data-testid={cornerTestId}
              style={{ gridColumn: '2 / 3', gridRow: '2 / 3' }}
            />
          ) : null}
        </>
      ) : null}
    </div>
  );
});
