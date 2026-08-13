import type { ApplicationMenuCommand, UpdateFrequency } from './protocol';

export interface ApplicationMenuCommandDefinition {
  readonly command: ApplicationMenuCommand;
  readonly label: string;
  readonly accelerator?: string;
}

export const APPLICATION_MENU_BAR_VISIBILITY_LABEL = 'Show Menu Bar in App Windows';

export const APPLICATION_MENU_COMMANDS = {
  newPdf: {
    command: 'new-pdf',
    label: 'New from Template...',
    accelerator: 'CommandOrControl+N',
  },
  openPdf: {
    command: 'open-pdf',
    label: 'Open...',
    accelerator: 'CommandOrControl+O',
  },
  save: {
    command: 'save',
    label: 'Save',
    accelerator: 'CommandOrControl+S',
  },
  saveAs: {
    command: 'save-as',
    label: 'Save As...',
    accelerator: 'CommandOrControl+Shift+S',
  },
  saveDocumentAsTemplate: {
    command: 'save-document-as-template',
    label: 'Save Document as Template...',
  },
  undo: { command: 'undo', label: 'Undo', accelerator: 'CommandOrControl+Z' },
  redo: { command: 'redo', label: 'Redo', accelerator: 'CommandOrControl+Shift+Z' },
  cut: { command: 'cut', label: 'Cut', accelerator: 'CommandOrControl+X' },
  copy: { command: 'copy', label: 'Copy', accelerator: 'CommandOrControl+C' },
  paste: { command: 'paste', label: 'Paste', accelerator: 'CommandOrControl+V' },
  selectAll: { command: 'select-all', label: 'Select All', accelerator: 'CommandOrControl+A' },
  setDefaultPdfApp: {
    command: 'set-default-pdf-app',
    label: 'Set as Default PDF App...',
  },
  checkForUpdates: {
    command: 'check-for-updates',
    label: 'Check for Updates...',
  },
  openReleasePage: {
    command: 'open-release-page',
    label: 'View Releases...',
  },
} as const satisfies Record<string, ApplicationMenuCommandDefinition>;

export const APPLICATION_MENU_UPDATE_FREQUENCIES: readonly {
  readonly value: UpdateFrequency;
  readonly label: string;
}[] = [
  { value: 'never', label: 'Never' },
  { value: 'startup', label: 'At startup' },
  { value: 'hourly', label: 'Hourly' },
  { value: 'sixHours', label: 'Every 6 hours' },
  { value: 'twelveHours', label: 'Every 12 hours' },
  { value: 'daily', label: 'Daily' },
  { value: 'weekly', label: 'Weekly' },
  { value: 'monthly', label: 'Monthly' },
];

export function updateCheckMenuLabel(phase: string, downloadPercent: number | null): string {
  if (phase === 'checking') {
    return 'Checking for Updates...';
  }
  if (phase === 'available' || phase === 'downloading') {
    return downloadPercent == null
      ? 'Downloading Update...'
      : `Downloading Update (${Math.round(downloadPercent)}%)`;
  }
  if (phase === 'downloaded') {
    return 'Update Ready...';
  }
  return APPLICATION_MENU_COMMANDS.checkForUpdates.label;
}
