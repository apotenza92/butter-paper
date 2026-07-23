import { Grip, Plus } from 'lucide-react';
import { useRef, type KeyboardEvent } from 'react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import {
  CONTROL_ICON_SIZE,
  CONTROL_ICON_SIZE_CLASS,
  CONTROL_ICON_STROKE_WIDTH,
  SHELL_CONTROL_GAP,
  SHELL_SURFACE_PANEL,
  TAB_BAR_HEIGHT,
} from './shellSpacing';

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
  const tabRefs = useRef(new Map<string, HTMLButtonElement>());

  function focusTabAt(index: number): void {
    const tab = tabs[index];
    if (!tab) {
      return;
    }
    onSelectTab(tab.id);
    tabRefs.current.get(tab.id)?.focus();
  }

  function handleTabKeyDown(event: KeyboardEvent<HTMLButtonElement>, index: number): void {
    let nextIndex: number | null = null;
    if (event.key === 'ArrowLeft') {
      nextIndex = (index - 1 + tabs.length) % tabs.length;
    } else if (event.key === 'ArrowRight') {
      nextIndex = (index + 1) % tabs.length;
    } else if (event.key === 'Home') {
      nextIndex = 0;
    } else if (event.key === 'End') {
      nextIndex = tabs.length - 1;
    }

    if (nextIndex !== null) {
      event.preventDefault();
      focusTabAt(nextIndex);
    }
  }

  function handleCloseTab(tabId: string, index: number): void {
    const fallbackTab = tabs[index + 1] ?? tabs[index - 1];
    onCloseTab(tabId);
    if (tabId === activeTabId && fallbackTab) {
      window.requestAnimationFrame(() => tabRefs.current.get(fallbackTab.id)?.focus());
    }
  }

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
      <div
        aria-label="Open documents"
        className={['flex min-w-0 flex-1 items-center overflow-x-auto', SHELL_CONTROL_GAP].join(' ')}
        role="tablist"
      >
        {tabs.map((tab, index) => (
          <div key={tab.id} className="flex shrink-0 items-center rounded-2xl bg-muted p-[2px]">
            <Button
              ref={(node) => {
                if (node) tabRefs.current.set(tab.id, node);
                else tabRefs.current.delete(tab.id);
              }}
              type="button"
              variant={tab.id === activeTabId ? 'secondary' : 'ghost'}
              size="sm"
              className="max-w-[250px] rounded-xl px-2 text-[12px]"
              id={`document-tab-trigger-${index}`}
              data-testid={`document-tab-${index}`}
              role="tab"
              aria-controls="document-tab-panel"
              aria-selected={tab.id === activeTabId}
              tabIndex={tab.id === activeTabId ? 0 : -1}
              onClick={() => onSelectTab(tab.id)}
              onKeyDown={(event) => handleTabKeyDown(event, index)}
            >
              <span className="truncate">{tab.documentName}</span>
              {tab.dirty ? <span className="text-amber-600" aria-label="Unsaved changes">*</span> : null}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              className="rounded-xl"
              aria-label={`Close ${tab.documentName}`}
              data-testid={`document-tab-close-${index}`}
              onClick={() => handleCloseTab(tab.id, index)}
            >
              <span aria-hidden="true">×</span>
            </Button>
          </div>
        ))}
      </div>
      <Tooltip>
        <TooltipTrigger
          render={(
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-8 shrink-0 rounded-2xl"
              aria-label="New Butter Canvas"
              data-testid="document-tab-new-canvas"
              onClick={onNewCanvas}
            >
              <ButterCanvasIcon />
            </Button>
          )}
        />
        <TooltipContent data-testid="document-tab-new-canvas-tooltip">New Butter Canvas</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger
          render={(
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-8 shrink-0 rounded-2xl"
              aria-label="Open PDF"
              data-testid="document-tab-open"
              onClick={onOpenTab}
            >
              <Plus
                aria-hidden="true"
                size={CONTROL_ICON_SIZE}
                strokeWidth={CONTROL_ICON_STROKE_WIDTH}
                absoluteStrokeWidth
                className={CONTROL_ICON_SIZE_CLASS}
              />
            </Button>
          )}
        />
        <TooltipContent data-testid="document-tab-open-tooltip">Open PDF</TooltipContent>
      </Tooltip>
    </div>
  );
}

function ButterCanvasIcon() {
  return (
    <Grip
      aria-hidden="true"
      data-testid="icon-butter-canvas"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={CONTROL_ICON_SIZE_CLASS}
    />
  );
}
