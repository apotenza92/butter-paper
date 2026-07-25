import { useCallback, useEffect, useRef, useState, type CSSProperties, type ReactElement, type ReactNode } from 'react';
import {
  Tooltip as ShadcnTooltip,
  TooltipContent as ShadcnTooltipContent,
  TooltipTrigger as ShadcnTooltipTrigger,
} from './ui/tooltip';

type TooltipSide = 'top' | 'right' | 'bottom' | 'left';
export const TOOLTIP_SHOW_DELAY_MS = 180;

interface TooltipProps {
  children: ReactNode;
  className?: string;
  side?: TooltipSide;
  style?: CSSProperties;
  testId?: string;
  trigger?: ReactElement;
  visible?: boolean;
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
  trigger,
  visible,
}: TooltipProps) {
  const usesCompatibilityAnchor = !trigger;
  const controlledOpen = visible ?? (usesCompatibilityAnchor ? true : undefined);

  return (
    <ShadcnTooltip open={controlledOpen}>
      <ShadcnTooltipTrigger
        delay={TOOLTIP_SHOW_DELAY_MS}
        render={trigger ?? (
          <span
            aria-hidden="true"
            className={style ? 'pointer-events-none absolute inset-x-0 h-px' : 'pointer-events-none absolute inset-0'}
            style={style}
          />
        )}
      />
      <ShadcnTooltipContent
        className={className}
        data-testid={testId}
        side={side}
        sideOffset={8}
      >
        {children}
      </ShadcnTooltipContent>
    </ShadcnTooltip>
  );
}
