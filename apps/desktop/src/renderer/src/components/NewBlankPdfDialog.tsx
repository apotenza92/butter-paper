import { useEffect, useState } from 'react';
import type { BlankPdfCreateRequest } from '../../../shared/protocol';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Spinner } from '@/components/ui/spinner';
import { resolveBlankPdfDimensions, type BlankPdfSettings } from './blankPdfSettings';
import { BlankPdfSettingsFields } from './BlankPdfSettingsFields';

interface NewBlankPdfDialogProps {
  open: boolean;
  settings: BlankPdfSettings;
  onCreate: (request: BlankPdfCreateRequest) => Promise<void>;
  onOpenChange: (open: boolean) => void;
  onSettingsChange: (settings: BlankPdfSettings) => void;
}

export function NewBlankPdfDialog({
  open,
  settings,
  onCreate,
  onOpenChange,
  onSettingsChange,
}: NewBlankPdfDialogProps) {
  const [draft, setDraft] = useState(settings);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    if (open) {
      setDraft(settings);
      setError(null);
    }
  }, [open, settings]);

  async function handleCreate(): Promise<void> {
    try {
      const request = resolveBlankPdfDimensions(draft);
      setCreating(true);
      setError(null);
      await onCreate(request);
      onSettingsChange(draft);
      onOpenChange(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to create a blank PDF.');
    } finally {
      setCreating(false);
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!creating) onOpenChange(nextOpen);
      }}
    >
      <DialogContent
        data-testid="new-blank-pdf-dialog"
        finalFocus={() => document.querySelector<HTMLElement>('[data-testid="menu-trigger-file"]')}
        showCloseButton={false}
      >
        <DialogHeader>
          <DialogTitle>New Blank PDF</DialogTitle>
          <DialogDescription>Choose a paper size, orientation, and background for the new PDF.</DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-4"
          aria-busy={creating || undefined}
          onSubmit={(event) => {
            event.preventDefault();
            void handleCreate();
          }}
        >
          <BlankPdfSettingsFields
            settings={draft}
            error={error}
            testIdPrefix="new-blank-pdf-dialog"
            onSettingsChange={(nextSettings) => {
              setDraft(nextSettings);
              setError(null);
            }}
          />
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" disabled={creating} />}>
              Cancel
            </DialogClose>
            <Button type="submit" disabled={creating} data-testid="new-blank-pdf-dialog-create">
              {creating ? <Spinner data-icon="inline-start" aria-hidden="true" /> : null}
              {creating ? 'Creating…' : 'Create'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
