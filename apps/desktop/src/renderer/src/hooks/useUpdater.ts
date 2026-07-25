import { useCallback, useEffect, useState } from 'react';
import type { UpdateFrequency, UpdateStatus } from '../../../shared/protocol';

export interface UpdaterActions {
  checkNow(): Promise<void>;
  installDownloaded(): Promise<void>;
  openReleasePage(): Promise<void>;
  setFrequency(frequency: UpdateFrequency): Promise<void>;
}

export function useUpdater(): { status: UpdateStatus | null; actions: UpdaterActions } {
  const [status, setStatus] = useState<UpdateStatus | null>(null);

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
    try {
      setStatus(await window.butterPaper.updates.checkNow());
    } catch (error) {
      console.error('Unable to check for updates.', error);
    }
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

  return {
    status,
    actions: { checkNow, installDownloaded, openReleasePage, setFrequency },
  };
}
