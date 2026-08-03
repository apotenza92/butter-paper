import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

interface UnsavedChangesDialogProps {
  mode: 'tab' | 'application';
  documentName?: string;
  dirtyDocumentCount?: number;
  busy?: boolean;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}

export function UnsavedChangesDialog({
  mode,
  documentName,
  dirtyDocumentCount = 1,
  busy = false,
  onSave,
  onDiscard,
  onCancel,
}: UnsavedChangesDialogProps) {
  const applicationMode = mode === 'application';
  return (
    <Dialog open onOpenChange={(open) => { if (!open && !busy) onCancel(); }}>
      <DialogContent data-testid="unsaved-changes-dialog" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{applicationMode ? 'Save changes before quitting?' : `Save changes to ${documentName ?? 'this PDF'}?`}</DialogTitle>
          <DialogDescription>
            {applicationMode
              ? `${dirtyDocumentCount} modified ${dirtyDocumentCount === 1 ? 'PDF has' : 'PDFs have'} unsaved changes.`
              : 'Your changes will be lost if you close this tab without saving.'}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="outline" disabled={busy} onClick={onCancel}>Cancel</Button>
          <Button type="button" variant="destructive" disabled={busy} onClick={onDiscard} data-testid="unsaved-discard">
            {applicationMode ? 'Discard All' : 'Discard'}
          </Button>
          <Button type="button" disabled={busy} onClick={onSave} data-testid="unsaved-save">
            {busy ? 'Saving…' : applicationMode ? 'Save All' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
