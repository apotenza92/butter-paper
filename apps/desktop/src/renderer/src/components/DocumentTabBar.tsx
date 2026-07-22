import { Grip, Plus, Square } from 'lucide-react';
import { useCallback, useRef } from 'react';
import {
  CONTROL_ACTIVE,
  CONTROL_DEFAULT,
  CONTROL_ICON_SIZE,
  CONTROL_ICON_SIZE_CLASS,
  CONTROL_ICON_STROKE_WIDTH,
  RAIL_BUTTON_SIZE,
  SHELL_CONTROL_GAP,
  SHELL_SURFACE_PANEL,
  TAB_BAR_HEIGHT,
  VIEWER_TOOLBAR_BUTTON_SIZE,
} from './shellSpacing';
import { Tooltip, useTooltipDelay } from './Tooltip';

export interface DocumentTabItem {
  id: string;
  documentName: string;
  dirty: boolean;
}

interface DocumentTabBarProps {
  tabs: DocumentTabItem[];
  activeTabId: string | null;
  onSelectTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onOpenTab: () => void;
  onNewCanvas: () => void;
}

export function DocumentTabBar({ tabs, activeTabId, onSelectTab, onCloseTab, onOpenTab, onNewCanvas }: DocumentTabBarProps) {
  const openButtonRef = useRef<HTMLButtonElement | null>(null);
  const canvasButtonRef = useRef<HTMLButtonElement | null>(null);
  const canShowOpenTooltip = useCallback(() => {
    const button = openButtonRef.current;
    if (!button) {
      return false;
    }

    const activeElement = button.ownerDocument.activeElement;
    return button.matches(':hover') || (activeElement instanceof Node && button.contains(activeElement));
  }, []);
  const openTooltip = useTooltipDelay({ canShow: canShowOpenTooltip });
  const canShowCanvasTooltip = useCallback(() => {
    const button = canvasButtonRef.current;
    if (!button) {
      return false;
    }

    const activeElement = button.ownerDocument.activeElement;
    return button.matches(':hover') || (activeElement instanceof Node && button.contains(activeElement));
  }, []);
  const canvasTooltip = useTooltipDelay({ canShow: canShowCanvasTooltip });

  return (
    <div
      className={[
        'bp-border-bottom-inset flex items-center',
        TAB_BAR_HEIGHT,
        'px-2',
        SHELL_CONTROL_GAP,
        SHELL_SURFACE_PANEL,
      ].join(' ')}
      data-testid="document-tab-bar"
    >
      <div className={['flex min-w-0 flex-1 items-center overflow-x-auto', SHELL_CONTROL_GAP].join(' ')}>
        {tabs.map((tab, index) => (
          <button
            key={tab.id}
            type="button"
            className={[
              'inline-flex max-w-[280px] shrink-0 items-center justify-center rounded-[6px] border px-2 text-[12px] font-medium transition',
              VIEWER_TOOLBAR_BUTTON_SIZE,
              tab.id === activeTabId ? CONTROL_ACTIVE : CONTROL_DEFAULT,
            ].join(' ')}
            data-testid={`document-tab-${index}`}
            aria-selected={tab.id === activeTabId}
            onClick={() => onSelectTab(tab.id)}
          >
            <span className="truncate">{tab.documentName}</span>
            {tab.dirty ? <span className="ml-1 text-amber-600">*</span> : null}
            <span
              role="button"
              tabIndex={0}
              className="ml-2 rounded px-1 text-[11px] opacity-70 hover:opacity-100"
              aria-label={`Close ${tab.documentName}`}
              data-testid={`document-tab-close-${index}`}
              onClick={(event) => { event.stopPropagation(); onCloseTab(tab.id); }}
              onKeyDown={(event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); event.stopPropagation(); onCloseTab(tab.id); } }}
            >
              ×
            </span>
          </button>
        ))}
      </div>
      <button
        ref={canvasButtonRef}
        type="button"
        className={[
          'relative inline-flex shrink-0 items-center justify-center rounded-[6px] border transition',
          RAIL_BUTTON_SIZE,
          CONTROL_DEFAULT,
        ].join(' ')}
        aria-label="New Butter Canvas"
        data-testid="document-tab-new-canvas"
        onBlur={canvasTooltip.hideTooltip}
        onClick={() => {
          canvasTooltip.hideTooltip();
          onNewCanvas();
        }}
        onFocus={canvasTooltip.showTooltip}
        onPointerEnter={canvasTooltip.showTooltipAfterDelay}
        onPointerLeave={canvasTooltip.hideTooltip}
      >
        <ButterCanvasIcon />
        {canvasTooltip.visible ? (
          <Tooltip testId="document-tab-new-canvas-tooltip">
            New Butter Canvas
          </Tooltip>
        ) : null}
      </button>
      <button
        ref={openButtonRef}
        type="button"
        className={[
          'relative inline-flex shrink-0 items-center justify-center rounded-[6px] border transition',
          RAIL_BUTTON_SIZE,
          CONTROL_DEFAULT,
        ].join(' ')}
        aria-label="Open PDF"
        data-testid="document-tab-open"
        onBlur={openTooltip.hideTooltip}
        onClick={() => {
          openTooltip.hideTooltip();
          onOpenTab();
        }}
        onFocus={openTooltip.showTooltip}
        onPointerEnter={openTooltip.showTooltipAfterDelay}
        onPointerLeave={openTooltip.hideTooltip}
      >
        <Plus
          aria-hidden="true"
          size={CONTROL_ICON_SIZE}
          strokeWidth={CONTROL_ICON_STROKE_WIDTH}
          absoluteStrokeWidth
          className={CONTROL_ICON_SIZE_CLASS}
        />
        {openTooltip.visible ? (
          <Tooltip testId="document-tab-open-tooltip">
            Open PDF
          </Tooltip>
        ) : null}
      </button>
    </div>
  );
}

function ButterCanvasIcon() {
  return (
    <span className={['relative inline-flex items-center justify-center', CONTROL_ICON_SIZE_CLASS].join(' ')} aria-hidden="true">
      <Square
        size={CONTROL_ICON_SIZE}
        strokeWidth={CONTROL_ICON_STROKE_WIDTH}
        absoluteStrokeWidth
        className="absolute inset-0 h-[18px] w-[18px]"
      />
      <Grip
        size={10}
        strokeWidth={CONTROL_ICON_STROKE_WIDTH}
        absoluteStrokeWidth
        className="h-[10px] w-[10px]"
      />
    </span>
  );
}
