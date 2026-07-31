import { useState } from 'react';
import type { UpdateStatus } from '../../../shared/protocol';
import { Alert, AlertDescription, AlertTitle } from './ui/alert';
import { Button } from './ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Progress, ProgressLabel, ProgressValue } from './ui/progress';
import {
  resolveUpdateDialogMode,
  type ManualUpdateCheckState,
} from './updateDialogState';

interface UpdateDialogProps {
  hasDirtyDocuments: boolean;
  manualCheck: ManualUpdateCheckState;
  productName: string;
  status: UpdateStatus | null;
  onCheckAgain: () => void;
  onDismissManualCheck: () => void;
  onInstall: () => void;
  onOpenReleasePage: () => void;
}

export function UpdateDialog({
  hasDirtyDocuments,
  manualCheck,
  productName,
  status,
  onCheckAgain,
  onDismissManualCheck,
  onInstall,
  onOpenReleasePage,
}: UpdateDialogProps) {
  const mode = resolveUpdateDialogMode(status, manualCheck);
  const downloaded = mode === 'ready';
  const version = status?.availableVersion;
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null);
  const readyVersion = version ?? status?.currentVersion ?? null;
  const open = manualCheck.open
    || (downloaded && readyVersion != null && dismissedVersion !== readyVersion);
  const downloadPercent = status?.downloadPercent ?? null;
  const channelLabel = status?.channel === 'beta' ? 'beta' : 'stable';

  const closeDialog = () => {
    if (downloaded && readyVersion != null) {
      setDismissedVersion(readyVersion);
    }
    if (manualCheck.open) {
      onDismissManualCheck();
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          closeDialog();
        }
      }}
    >
      <DialogContent data-testid="update-dialog" showCloseButton>
        <DialogHeader>
          <DialogTitle>
            {mode === 'checking' ? 'Checking for updates'
              : mode === 'downloading' ? `Downloading ${productName}${version ? ` ${version}` : ''}`
                : mode === 'up-to-date' ? 'You’re up to date'
                  : mode === 'error' ? 'Couldn’t check for updates'
                    : `${productName} ${version ? `${version} ` : ''}is ready`}
          </DialogTitle>
          <DialogDescription data-testid="update-dialog-status">
            {mode === 'checking'
              ? `Checking the ${channelLabel} channel for a newer version of ${productName}.`
              : mode === 'downloading'
                ? 'The authenticated update is downloading. You can close this window and the download will continue in the background.'
                : mode === 'up-to-date'
                  ? `${productName} ${status?.currentVersion ?? ''} is the newest ${channelLabel} version available.`
                  : mode === 'error'
                    ? 'The update check did not complete. Your current installation has not been changed.'
                    : hasDirtyDocuments
                      ? 'Save or close every modified document before restarting to install the update.'
                      : `The authenticated update has downloaded. Restart ${productName} to install it.`}
          </DialogDescription>
        </DialogHeader>
        {mode === 'checking' ? (
          <Progress data-testid="update-check-progress" value={null}>
            <ProgressLabel>Contacting update service</ProgressLabel>
            <ProgressValue>{() => 'Checking…'}</ProgressValue>
          </Progress>
        ) : null}
        {mode === 'downloading' ? (
          <Progress data-testid="update-download-progress" value={downloadPercent}>
            <ProgressLabel>Download progress</ProgressLabel>
            <ProgressValue>
              {(_formattedValue, value) => value == null ? 'Starting…' : `${Math.round(value)}%`}
            </ProgressValue>
          </Progress>
        ) : null}
        {mode === 'up-to-date' ? (
          <Alert>
            <AlertTitle>No update is needed</AlertTitle>
            <AlertDescription>
              Automatic checks will continue according to your selected schedule.
            </AlertDescription>
          </Alert>
        ) : null}
        {mode === 'error' ? (
          <Alert variant="destructive">
            <AlertTitle>Update check failed</AlertTitle>
            <AlertDescription>
              {manualCheck.errorMessage
                ?? status?.errorMessage
                ?? 'Update checking is unavailable for this installation.'}
            </AlertDescription>
          </Alert>
        ) : null}
        {downloaded && status?.releaseNotes ? (
          <div className="max-h-48 overflow-y-auto whitespace-pre-wrap rounded-md border p-3 text-sm">
            {status.releaseNotes}
          </div>
        ) : null}
        <DialogFooter>
          {mode === 'ready' ? (
            <>
              <Button variant="outline" onClick={onOpenReleasePage}>Release notes</Button>
              <Button disabled={hasDirtyDocuments} onClick={onInstall}>Restart and update</Button>
            </>
          ) : mode === 'error' ? (
            <>
              <Button variant="outline" onClick={onOpenReleasePage}>View releases</Button>
              <Button onClick={onCheckAgain}>Try again</Button>
            </>
          ) : mode === 'up-to-date' ? (
            <>
              <Button variant="outline" onClick={onOpenReleasePage}>View releases</Button>
              <Button onClick={closeDialog}>Done</Button>
            </>
          ) : (
            <Button variant="outline" onClick={closeDialog}>Close</Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
