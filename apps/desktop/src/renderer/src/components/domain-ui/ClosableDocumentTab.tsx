import { useSortable } from '@dnd-kit/sortable';
import { X } from 'lucide-react';
import type { CSSProperties, KeyboardEvent } from 'react';
import { TabsTrigger } from '@/components/ui/tabs';
import { ConfirmationPopover } from '../ConfirmationPopover';

export interface DocumentTabCloseConfirmation {
  busy: boolean;
  onDiscard: () => void;
  onOpenChange: (open: boolean) => void;
  onSave: () => void;
  open: boolean;
}

interface ClosableDocumentTabProps {
  active: boolean;
  dirty: boolean;
  documentName: string;
  index: number;
  tabId: string;
  closeConfirmation?: DocumentTabCloseConfirmation;
  onClose: () => void;
  onMove: (tabId: string, direction: -1 | 1) => void;
}

/**
 * Domain UI exception: shadcn Tabs does not include a closable document-tab
 * pattern. The tab remains an official TabsTrigger; this wrapper owns only the
 * overlaid close action, dirty state, and sortable behaviour. The reviewed
 * product treatment keeps inactive tabs on the shell background and uses the
 * semantic muted colour for the selected document.
 */
export function ClosableDocumentTab({
  active,
  dirty,
  documentName,
  index,
  tabId,
  closeConfirmation,
  onClose,
  onMove,
}: ClosableDocumentTabProps) {
  const documentLabel = formatDocumentTabLabel(documentName);
  const { isDragging, listeners, setNodeRef, transform, transition } = useSortable({ id: tabId });
  const sortableStyle: CSSProperties = {
    transform: transform ? `translate3d(${transform.x}px, 0, 0)` : undefined,
    transition,
  };
  const closeButton = (
    <button
      type="button"
      className="group/tab-close pointer-events-none absolute right-1 top-1/2 inline-flex size-6 -translate-y-1/2 items-center justify-center rounded-md border-0 bg-transparent p-0 text-muted-foreground opacity-0 outline-none transition-[color,opacity] group-hover/document-tab:pointer-events-auto group-hover/document-tab:opacity-100 hover:text-foreground focus-visible:pointer-events-auto focus-visible:text-foreground focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background"
      aria-label={`Close ${documentName}`}
      tabIndex={active ? 0 : -1}
      data-domain-ui-control="tab-close"
      data-testid={`document-tab-close-${index}`}
      onClick={closeConfirmation ? undefined : onClose}
    >
      <span
        className="inline-flex size-5 items-center justify-center rounded-md transition-colors group-hover/tab-close:bg-foreground/10 group-focus-visible/tab-close:bg-foreground/10"
        data-tab-close-surface
        aria-hidden="true"
      >
        <X className="size-3.5" strokeWidth={1.75} absoluteStrokeWidth />
      </span>
    </button>
  );

  function handleKeyDown(event: KeyboardEvent<HTMLButtonElement>): void {
    if (!event.altKey || !event.shiftKey) return;
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    event.stopPropagation();
    onMove(tabId, event.key === 'ArrowLeft' ? -1 : 1);
  }

  return (
    <div
      ref={setNodeRef}
      role="presentation"
      className="group/document-tab relative flex shrink-0 items-center"
      style={sortableStyle}
      data-dragging={isDragging ? '' : undefined}
      data-domain-ui-exception="closable-document-tab"
    >
      <TabsTrigger
        value={tabId}
        className="h-8! bg-background! data-active:bg-muted! group-data-[dragging]/document-tab:after:opacity-0!"
        id={`document-tab-trigger-${index}`}
        data-document-tab-id={tabId}
        data-testid={`document-tab-${index}`}
        aria-controls="document-tab-panel"
        aria-description="Drag to reorder. Press Alt+Shift+Left or Alt+Shift+Right to move this tab."
        aria-keyshortcuts="Alt+Shift+ArrowLeft Alt+Shift+ArrowRight"
        onKeyDown={handleKeyDown}
        {...listeners}
      >
        {dirty ? (
          <span
            className="inline-flex h-full shrink-0 items-center text-muted-foreground leading-none"
            data-document-tab-dirty
            aria-label="Unsaved changes"
          >
            *
          </span>
        ) : null}
        <span className="bp-document-tab-label truncate">{documentLabel}</span>
      </TabsTrigger>
      {closeConfirmation ? (
        <ConfirmationPopover
          open={closeConfirmation.open}
          onOpenChange={closeConfirmation.onOpenChange}
          trigger={closeButton}
          side="bottom"
          align="end"
          title={`Save changes to ${documentName}?`}
          description="Your changes will be lost if you close this tab without saving."
          busy={closeConfirmation.busy}
          actionLabel={closeConfirmation.busy ? 'Saving…' : 'Save'}
          onAction={closeConfirmation.onSave}
          secondaryActionLabel="Discard"
          secondaryActionVariant="destructive"
          onSecondaryAction={closeConfirmation.onDiscard}
        />
      ) : closeButton}
    </div>
  );
}

export function formatDocumentTabLabel(documentName: string): string {
  const label = documentName.replace(/\.pdf$/i, '');
  return label || documentName;
}
