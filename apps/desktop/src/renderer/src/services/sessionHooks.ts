import { useSyncExternalStore } from 'react';
import type { LocalPdfSession } from './documentSession';

export function useSessionVersion(session: LocalPdfSession | null): number {
  return useSyncExternalStore(
    (onStoreChange) => {
      if (!session) {
        return () => undefined;
      }

      return session.subscribe(onStoreChange);
    },
    () => (session ? session.version : 0),
    () => 0,
  );
}
