import { useCallback, useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';

type TooltipSide = 'top' | 'right' | 'bottom' | 'left';
export const TOOLTIP_SHOW_DELAY_MS = 180;

interface TooltipProps {
  children: ReactNode;
  className?: string;
  side?: TooltipSide;
  style?: CSSProperties;
  testId?: string;
  visible?: boolean;
  revealOnGroupHover?: boolean;
}

interface TooltipDelayOptions {
  canShow?: () => boolean;
  disabled?: boolean;
  delayMs?: number;
  suppressed?: boolean;
}

interface TooltipDelayControls {
  visible: boolean;
  hideTooltip: () => void;
  showTooltip: () => void;
  showTooltipAfterDelay: () => void;
}

const SIDE_CLASSES: Record<TooltipSide, string> = {
  top: 'bottom-full left-1/2 mb-2 -translate-x-1/2',
  right: 'top-1/2 left-full ml-2 -translate-y-1/2',
  bottom: 'top-full left-1/2 mt-2 -translate-x-1/2',
  left: 'top-1/2 right-full mr-2 -translate-y-1/2',
};

export function useTooltipDelay({
  canShow,
  disabled = false,
  delayMs = TOOLTIP_SHOW_DELAY_MS,
  suppressed = false,
}: TooltipDelayOptions = {}): TooltipDelayControls {
  const [visible, setVisible] = useState(false);
  const timerRef = useRef<number | null>(null);

  const clearTooltipTimer = useCallback((): void => {
    if (timerRef.current === null) {
      return;
    }
    window.clearTimeout(timerRef.current);
    timerRef.current = null;
  }, []);

  const hideTooltip = useCallback((): void => {
    clearTooltipTimer();
    setVisible(false);
  }, [clearTooltipTimer]);

  const showTooltip = useCallback((): void => {
    clearTooltipTimer();
    if (!disabled && !suppressed && (canShow?.() ?? true)) {
      setVisible(true);
    }
  }, [canShow, clearTooltipTimer, disabled, suppressed]);

  const showTooltipAfterDelay = useCallback((): void => {
    clearTooltipTimer();
    if (disabled || suppressed) {
      setVisible(false);
      return;
    }
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      if (canShow?.() ?? true) {
        setVisible(true);
      }
    }, delayMs);
  }, [canShow, clearTooltipTimer, delayMs, disabled, suppressed]);

  useEffect(() => {
    if (disabled || suppressed) {
      hideTooltip();
    }

    return clearTooltipTimer;
  }, [disabled, suppressed]);

  return {
    visible,
    hideTooltip,
    showTooltip,
    showTooltipAfterDelay,
  };
}

export function Tooltip({
  children,
  className,
  side = 'bottom',
  style,
  testId,
  visible = true,
  revealOnGroupHover = false,
}: TooltipProps) {
  return (
    <span
      className={[
        'bp-tooltip pointer-events-none absolute z-50 inline-flex items-center whitespace-nowrap border px-2 py-1 text-[11px] leading-none font-medium',
        SIDE_CLASSES[side],
        revealOnGroupHover ? 'bp-tooltip-group-hover opacity-0' : visible ? 'opacity-100' : 'opacity-0',
        className,
      ].join(' ')}
      data-testid={testId}
      style={style}
    >
      {children}
    </span>
  );
}
