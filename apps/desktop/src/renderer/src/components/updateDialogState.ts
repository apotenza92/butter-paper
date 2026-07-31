import type { UpdateStatus } from '../../../shared/protocol';

export interface ManualUpdateCheckState {
  open: boolean;
  pending: boolean;
  errorMessage: string | null;
}

export type UpdateDialogMode =
  | 'hidden'
  | 'checking'
  | 'downloading'
  | 'up-to-date'
  | 'error'
  | 'ready';

export function resolveUpdateDialogMode(
  status: UpdateStatus | null,
  manualCheck: ManualUpdateCheckState,
): UpdateDialogMode {
  if (status?.phase === 'downloaded') {
    return 'ready';
  }
  if (!manualCheck.open) {
    return 'hidden';
  }
  if (manualCheck.errorMessage) {
    return 'error';
  }
  if (manualCheck.pending || status == null || status.phase === 'checking') {
    return 'checking';
  }
  if (status.phase === 'error' || status.phase === 'disabled') {
    return 'error';
  }
  if (status.phase === 'available' || status.phase === 'downloading') {
    return 'downloading';
  }
  if (status.phase === 'idle') {
    return 'up-to-date';
  }
  return 'checking';
}
