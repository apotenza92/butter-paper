import { useState } from 'react';
import type { UpdateStatus } from '../../../shared/protocol';
import { Button } from './ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';

interface UpdateDialogProps {
  hasDirtyDocuments: boolean;
  productName: string;
  status: UpdateStatus | null;
  onInstall: () => void;
  onOpenReleasePage: () => void;
}

export function UpdateDialog({ hasDirtyDocuments, productName, status, onInstall, onOpenReleasePage }: UpdateDialogProps) {
  const downloaded = status?.phase === 'downloaded';
  const version = status?.availableVersion;
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null);
  const open = downloaded && dismissedVersion !== (version ?? status.currentVersion);

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && downloaded) {
          setDismissedVersion(version ?? status.currentVersion);
        }
      }}
    >
      <DialogContent showCloseButton>
        <DialogHeader>
          <DialogTitle>{productName} {version ? `${version} ` : ''}is ready</DialogTitle>
          <DialogDescription>
            {hasDirtyDocuments
              ? 'Save or close every modified document before restarting to install the update.'
              : `The signed update has downloaded. Restart ${productName} to install it.`}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onOpenReleasePage}>Release notes</Button>
          <Button disabled={hasDirtyDocuments} onClick={onInstall}>Restart and update</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
