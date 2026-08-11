import { useCallback, useEffect, useMemo, useState } from 'react';
import type { UpdateFrequency, UpdateStatus } from '../../../shared/protocol';
import type { ManualUpdateCheckState } from '../components/updateDialogState';

export interface UpdaterActions {
  checkNow(): Promise<void>;
  dismissManualCheck(): void;
  installDownloaded(): Promise<void>;
  openReleasePage(): Promise<void>;
  setFrequency(frequency: UpdateFrequency): Promise<void>;
}

const closedManualCheck: ManualUpdateCheckState = {
  open: false,
  pending: false,
  errorMessage: null,
};

export function useUpdater(): {
  status: UpdateStatus | null;
  manualCheck: ManualUpdateCheckState;
  actions: UpdaterActions;
} {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [manualCheck, setManualCheck] = useState<ManualUpdateCheckState>(closedManualCheck);

  useEffect(() => {
    let active = true;
    const unsubscribe = window.butterPaper.updates.onStatusChanged((nextStatus) => {
      if (active) {
        setStatus(nextStatus);
      }
    });

    void window.butterPaper.updates.getStatus()
      .then((nextStatus) => {
        if (active) {
          setStatus(nextStatus);
        }
      })
      .catch((error) => console.error('Unable to load updater status.', error));

    return () => {
      active = false;
      unsubscribe();
    };
  }, []);

  const checkNow = useCallback(async () => {
    setManualCheck({ open: true, pending: true, errorMessage: null });
    try {
      setStatus(await window.butterPaper.updates.checkNow());
      setManualCheck(current => current.open
        ? { ...current, pending: false }
        : current);
    } catch (error) {
      console.error('Unable to check for updates.', error);
      setManualCheck(current => current.open
        ? {
            ...current,
            pending: false,
            errorMessage: error instanceof Error
              ? error.message
              : 'Unable to contact the update service.',
          }
        : current);
    }
  }, []);

  const dismissManualCheck = useCallback(() => {
    setManualCheck(closedManualCheck);
  }, []);

  const installDownloaded = useCallback(async () => {
    try {
      await window.butterPaper.updates.installDownloaded();
    } catch (error) {
      console.error('Unable to install the downloaded update.', error);
    }
  }, []);

  const openReleasePage = useCallback(async () => {
    try {
      await window.butterPaper.updates.openReleasePage();
    } catch (error) {
      console.error('Unable to open the release page.', error);
    }
  }, []);

  const setFrequency = useCallback(async (frequency: UpdateFrequency) => {
    try {
      setStatus(await window.butterPaper.updates.setFrequency(frequency));
    } catch (error) {
      console.error('Unable to save the update frequency.', error);
    }
  }, []);

  const actions = useMemo(() => ({
    checkNow,
    dismissManualCheck,
    installDownloaded,
    openReleasePage,
    setFrequency,
  }), [checkNow, dismissManualCheck, installDownloaded, openReleasePage, setFrequency]);

  return {
    status,
    manualCheck,
    actions,
  };
}
