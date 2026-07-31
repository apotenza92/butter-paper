import type { UpdateStatus } from '../../../shared/protocol';
import {
  resolveUpdateDialogMode,
  type ManualUpdateCheckState,
} from './updateDialogState';

const baseStatus: UpdateStatus = {
  phase: 'idle',
  channel: 'stable',
  frequency: 'daily',
  enabled: true,
  automaticChecksEnabled: true,
  currentVersion: '1.0.0',
  availableVersion: null,
  releaseNotes: null,
  downloadPercent: null,
  lastSuccessfulCheckAt: null,
  disabledReason: null,
  errorMessage: null,
};

const closedManualCheck: ManualUpdateCheckState = {
  open: false,
  pending: false,
  errorMessage: null,
};

describe('update dialog state', () => {
  it('keeps automatic checks quiet until an update is ready', () => {
    expect(resolveUpdateDialogMode({ ...baseStatus, phase: 'checking' }, closedManualCheck)).toBe('hidden');
    expect(resolveUpdateDialogMode({ ...baseStatus, phase: 'downloading' }, closedManualCheck)).toBe('hidden');
    expect(resolveUpdateDialogMode({ ...baseStatus, phase: 'downloaded' }, closedManualCheck)).toBe('ready');
  });

  it('shows every meaningful state for a manual check', () => {
    const manualCheck = { ...closedManualCheck, open: true };

    expect(resolveUpdateDialogMode(baseStatus, { ...manualCheck, pending: true })).toBe('checking');
    expect(resolveUpdateDialogMode({ ...baseStatus, phase: 'checking' }, manualCheck)).toBe('checking');
    expect(resolveUpdateDialogMode({ ...baseStatus, phase: 'available' }, manualCheck)).toBe('downloading');
    expect(resolveUpdateDialogMode({
      ...baseStatus,
      phase: 'downloading',
      downloadPercent: 42.5,
    }, manualCheck)).toBe('downloading');
    expect(resolveUpdateDialogMode(baseStatus, manualCheck)).toBe('up-to-date');
    expect(resolveUpdateDialogMode({
      ...baseStatus,
      phase: 'error',
      errorMessage: 'Network unavailable.',
    }, manualCheck)).toBe('error');
    expect(resolveUpdateDialogMode({
      ...baseStatus,
      phase: 'downloaded',
      availableVersion: '1.1.0',
    }, manualCheck)).toBe('ready');
  });

  it('shows renderer IPC failures instead of leaving the manual check unanswered', () => {
    expect(resolveUpdateDialogMode(baseStatus, {
      open: true,
      pending: false,
      errorMessage: 'Unable to contact the updater.',
    })).toBe('error');
  });

  it('shows checking immediately when retrying after an updater error', () => {
    expect(resolveUpdateDialogMode({
      ...baseStatus,
      phase: 'error',
      errorMessage: 'Network unavailable.',
    }, {
      open: true,
      pending: true,
      errorMessage: null,
    })).toBe('checking');
  });
});
