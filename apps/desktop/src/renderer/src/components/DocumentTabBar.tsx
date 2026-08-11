import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  PointerSensor,
  useSensor,
  useSensors,
} from '@dnd-kit/core';
import { horizontalListSortingStrategy, SortableContext } from '@dnd-kit/sortable';
import { FilePlus, Plus } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { ButtonGroup } from '@/components/ui/button-group';
import { Separator } from '@/components/ui/separator';
import { Tabs, TabsList } from '@/components/ui/tabs';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import type { BlankPdfSettings } from './blankPdfSettings';
import { BlankPdfSettingsPopover } from './BlankPdfSettingsPopover';
import { ClosableDocumentTab } from './domain-ui/ClosableDocumentTab';
import { SplitButtonSegment } from './domain-ui/SplitButtonSegment';

export const DOCUMENT_TAB_TOOLTIP_SIDE = 'bottom' as const;

export interface DocumentTabItem {
  id: string;
  documentName: string;
  dirty: boolean;
}

export interface DocumentTabCloseConfirmationState {
  busy: boolean;
  onCancel: () => void;
  onDiscard: () => void;
  onSave: () => void;
  tabId: string | null;
}

interface DocumentTabBarProps {
  tabs: DocumentTabItem[];
  activeTabId: string | null;
  onSelectTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onReorderTabs: (orderedTabIds: string[]) => void;
  onOpenTab: () => void;
  onNewPdf: () => void;
  onBlankPdfSettingsChange: (settings: BlankPdfSettings) => void;
  blankPdfSettings: BlankPdfSettings;
  blankPdfDefaultLabel: string;
  closeConfirmation: DocumentTabCloseConfirmationState;
}

export function DocumentTabBar({
  tabs,
  activeTabId,
  onSelectTab,
  onCloseTab,
  onReorderTabs,
  onOpenTab,
  onNewPdf,
  onBlankPdfSettingsChange,
  blankPdfSettings,
  blankPdfDefaultLabel,
  closeConfirmation,
}: DocumentTabBarProps) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const tabListRef = useRef<HTMLDivElement | null>(null);
  const [reorderAnnouncement, setReorderAnnouncement] = useState('');
  const sensors = useSensors(useSensor(PointerSensor, {
    activationConstraint: { distance: 6 },
  }));

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    const handleWheel = (event: WheelEvent) => {
      const tabList = tabListRef.current;
      if (!tabList || tabList.scrollWidth <= tabList.clientWidth + 1) return;

      const delta = resolveHorizontalWheelDelta(event, tabList.clientWidth);
      if (delta === 0) return;

      event.preventDefault();
      tabList.scrollLeft += delta;
    };
    root.addEventListener('wheel', handleWheel, { passive: false });
    return () => root.removeEventListener('wheel', handleWheel);
  }, []);

  function handleCloseTab(tabId: string, index: number): void {
    const fallbackTab = tabs[index + 1] ?? tabs[index - 1];
    onCloseTab(tabId);
    if (tabId === activeTabId && fallbackTab) {
      window.requestAnimationFrame(() => {
        rootRef.current
          ?.querySelector<HTMLButtonElement>(`[data-document-tab-id="${fallbackTab.id}"]`)
          ?.focus();
      });
    }
  }

  function commitTabReorder(tabId: string, targetTabId: string, restoreFocus: boolean): void {
    const currentTabIds = tabs.map((tab) => tab.id);
    const nextTabIds = reorderTabIds(currentTabIds, tabId, targetTabId);
    if (nextTabIds === currentTabIds) return;

    onReorderTabs(nextTabIds);
    const nextIndex = nextTabIds.indexOf(tabId);
    const tab = tabs.find((candidate) => candidate.id === tabId);
    setReorderAnnouncement(`Moved ${tab?.documentName ?? 'document'} to position ${nextIndex + 1} of ${nextTabIds.length}.`);

    if (restoreFocus) {
      window.requestAnimationFrame(() => {
        rootRef.current
          ?.querySelector<HTMLButtonElement>(`[data-document-tab-id="${tabId}"]`)
          ?.focus();
      });
    }
  }

  function handleDragEnd(event: DragEndEvent): void {
    if (!event.over) return;
    commitTabReorder(String(event.active.id), String(event.over.id), false);
  }

  function handleKeyboardMove(tabId: string, direction: -1 | 1): void {
    const index = tabs.findIndex((tab) => tab.id === tabId);
    const target = tabs[index + direction];
    if (!target) return;
    commitTabReorder(tabId, target.id, true);
  }

  return (
    <div
      ref={rootRef}
      className="flex items-center border-b border-border bg-background p-2"
      data-testid="document-tab-bar"
    >
      <Tabs
        value={activeTabId}
        onValueChange={(value) => {
          if (typeof value === 'string') onSelectTab(value);
        }}
        className="min-w-0 flex-1 gap-0"
      >
        <div className="min-w-0" data-testid="document-tab-surface">
          <div
            ref={tabListRef}
            className="bp-native-scroll-hidden flex min-w-0 items-center gap-2 overflow-x-auto"
            data-testid="document-tab-list"
          >
            <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
              <SortableContext items={tabs.map((tab) => tab.id)} strategy={horizontalListSortingStrategy}>
                <TabsList
                  activateOnFocus
                  aria-label="Open documents"
                  className="shrink-0 justify-start gap-2 rounded-none bg-background! p-0! group-data-horizontal/tabs:h-8!"
                >
                  {tabs.map((tab, index) => (
                    <ClosableDocumentTab
                      key={tab.id}
                      active={tab.id === activeTabId}
                      dirty={tab.dirty}
                      documentName={tab.documentName}
                      index={index}
                      tabId={tab.id}
                      closeConfirmation={tab.dirty ? {
                        busy: closeConfirmation.busy,
                        open: closeConfirmation.tabId === tab.id,
                        onOpenChange: (nextOpen) => {
                          if (nextOpen) onCloseTab(tab.id);
                          else if (closeConfirmation.tabId === tab.id) closeConfirmation.onCancel();
                        },
                        onSave: closeConfirmation.onSave,
                        onDiscard: closeConfirmation.onDiscard,
                      } : undefined}
                      onClose={() => handleCloseTab(tab.id, index)}
                      onMove={handleKeyboardMove}
                    />
                  ))}
                </TabsList>
              </SortableContext>
            </DndContext>
            {tabs.length > 0 ? (
              <Separator
                orientation="vertical"
                data-testid="document-tab-actions-separator"
              />
            ) : null}
            <div
              className="flex h-8 shrink-0 items-center gap-2 bg-background"
              role="group"
              aria-label="Document actions"
              data-testid="document-tab-actions"
            >
              <Tooltip>
                <TooltipTrigger
                  render={(
                    <SplitButtonSegment
                      type="button"
                      size="icon"
                      aria-label="Open PDF"
                      data-testid="document-tab-open"
                      onClick={onOpenTab}
                    >
                      <Plus data-icon="inline-start" aria-hidden="true" />
                    </SplitButtonSegment>
                  )}
                />
                <TooltipContent side={DOCUMENT_TAB_TOOLTIP_SIDE} data-testid="document-tab-open-tooltip">
                  Open PDF
                </TooltipContent>
              </Tooltip>
              <ButtonGroup aria-label="New blank PDF controls">
                <Tooltip>
                  <TooltipTrigger
                    render={(
                      <SplitButtonSegment
                        type="button"
                        size="icon"
                        aria-label={`New blank PDF using ${blankPdfDefaultLabel}`}
                        data-testid="document-tab-new-pdf"
                        onClick={onNewPdf}
                      >
                        <FilePlus data-icon="inline-start" aria-hidden="true" />
                      </SplitButtonSegment>
                    )}
                  />
                  <TooltipContent side={DOCUMENT_TAB_TOOLTIP_SIDE} data-testid="document-tab-new-pdf-tooltip">
                    New blank PDF · {blankPdfDefaultLabel}
                  </TooltipContent>
                </Tooltip>
                <BlankPdfSettingsPopover
                  settings={blankPdfSettings}
                  tooltipSide={DOCUMENT_TAB_TOOLTIP_SIDE}
                  onSettingsChange={onBlankPdfSettingsChange}
                />
              </ButtonGroup>
            </div>
          </div>
        </div>
        <span className="sr-only" aria-live="polite" data-testid="document-tab-reorder-status">
          {reorderAnnouncement}
        </span>
      </Tabs>
    </div>
  );
}

export function reorderTabIds(tabIds: string[], tabId: string, targetTabId: string): string[] {
  const sourceIndex = tabIds.indexOf(tabId);
  const targetIndex = tabIds.indexOf(targetTabId);
  if (sourceIndex < 0 || targetIndex < 0 || sourceIndex === targetIndex) return tabIds;

  const reordered = [...tabIds];
  const [movedTabId] = reordered.splice(sourceIndex, 1);
  reordered.splice(targetIndex, 0, movedTabId);
  return reordered;
}

export function applyTabOrder<T extends { id: string }>(tabs: T[], orderedTabIds: string[]): T[] {
  if (tabs.length !== orderedTabIds.length || new Set(orderedTabIds).size !== orderedTabIds.length) return tabs;
  const tabsById = new Map(tabs.map((tab) => [tab.id, tab]));
  const reordered = orderedTabIds.map((tabId) => tabsById.get(tabId));
  return reordered.every((tab): tab is T => tab !== undefined) ? reordered : tabs;
}

export function resolveActiveTabId<T extends { id: string }>(tabs: T[], activeTabId: string | null): string | null {
  return activeTabId && tabs.some((tab) => tab.id === activeTabId)
    ? activeTabId
    : tabs[0]?.id ?? null;
}

export function resolveHorizontalWheelDelta(
  event: Pick<WheelEvent, 'deltaMode' | 'deltaX' | 'deltaY'>,
  viewportWidth: number,
): number {
  const dominantDelta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
  const multiplier = event.deltaMode === 1
    ? 16
    : event.deltaMode === 2
      ? Math.max(1, viewportWidth)
      : 1;
  return dominantDelta * multiplier;
}
