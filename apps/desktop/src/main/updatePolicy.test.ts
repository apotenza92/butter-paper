import {
  DEFAULT_UPDATE_FREQUENCY,
  UPDATE_SCHEDULER_MAX_WAKE_MS,
  createDefaultUpdateSettings,
  getSchedulerWakeInterval,
  isAutomaticCheckDue,
  isUpdateFrequency,
  parseUpdateSettings,
  resolveLoopbackFeedOverride,
  resolveUpdateChannel,
  updaterNetworkDisabled,
} from './updatePolicy';

describe('update policy', () => {
  it('accepts only supported frequencies and defaults to daily', () => {
    expect(DEFAULT_UPDATE_FREQUENCY).toBe('daily');
    expect(createDefaultUpdateSettings()).toEqual({
      schemaVersion: 1,
      frequency: 'daily',
      lastSuccessfulCheckAt: null,
    });
    expect(isUpdateFrequency('monthly')).toBe(true);
    expect(isUpdateFrequency('sometimes')).toBe(false);
  });

  it('rejects invalid, obsolete, or non-canonical persisted settings', () => {
    expect(parseUpdateSettings({
      schemaVersion: 1,
      frequency: 'weekly',
      lastSuccessfulCheckAt: '2026-07-22T00:00:00.000Z',
    })).toEqual({
      schemaVersion: 1,
      frequency: 'weekly',
      lastSuccessfulCheckAt: '2026-07-22T00:00:00.000Z',
    });
    expect(parseUpdateSettings({ schemaVersion: 0, frequency: 'daily', lastSuccessfulCheckAt: null })).toBeNull();
    expect(parseUpdateSettings({ schemaVersion: 1, frequency: 'sometimes', lastSuccessfulCheckAt: null })).toBeNull();
    expect(parseUpdateSettings({ schemaVersion: 1, frequency: 'daily', lastSuccessfulCheckAt: 'not-a-date' })).toBeNull();
    expect(parseUpdateSettings({ schemaVersion: 1, frequency: 'daily', lastSuccessfulCheckAt: '2026-07-22' })).toBeNull();
  });

  it('requires an explicit stable or beta build channel', () => {
    expect(resolveUpdateChannel({ butterPaperChannel: 'stable' })).toBe('stable');
    expect(resolveUpdateChannel({ butterPaperChannel: 'beta' })).toBe('beta');
    expect(resolveUpdateChannel('stable')).toBe('stable');
    expect(resolveUpdateChannel({ name: 'butter-paper-beta' })).toBeNull();
    expect(resolveUpdateChannel({ butterPaperChannel: 'preview' })).toBeNull();
  });

  it('calculates interval checks and treats missing, invalid, and future timestamps as due', () => {
    const now = new Date('2026-07-22T12:00:00.000Z');
    expect(isAutomaticCheckDue('never', null, now)).toBe(false);
    expect(isAutomaticCheckDue('startup', '2026-07-22T11:59:59.000Z', now)).toBe(true);
    expect(isAutomaticCheckDue('daily', null, now)).toBe(true);
    expect(isAutomaticCheckDue('daily', 'invalid', now)).toBe(true);
    expect(isAutomaticCheckDue('daily', '2026-07-23T12:00:00.000Z', now)).toBe(true);
    expect(isAutomaticCheckDue('daily', '2026-07-21T12:00:00.001Z', now)).toBe(false);
    expect(isAutomaticCheckDue('daily', '2026-07-21T12:00:00.000Z', now)).toBe(true);
    expect(isAutomaticCheckDue('monthly', '2026-06-23T12:00:00.000Z', now)).toBe(false);
    expect(isAutomaticCheckDue('monthly', '2026-06-22T12:00:00.000Z', now)).toBe(true);
  });

  it('wakes no more than hourly for recurring frequencies', () => {
    expect(getSchedulerWakeInterval('never')).toBeNull();
    expect(getSchedulerWakeInterval('startup')).toBeNull();
    expect(getSchedulerWakeInterval('hourly')).toBe(UPDATE_SCHEDULER_MAX_WAKE_MS);
    expect(getSchedulerWakeInterval('monthly')).toBe(UPDATE_SCHEDULER_MAX_WAKE_MS);
  });

  it('disables ordinary test traffic while allowing the dedicated update test mode', () => {
    expect(updaterNetworkDisabled({ BP_TEST_MODE: '1' })).toBe(true);
    expect(updaterNetworkDisabled({ BP_DISABLE_UPDATE_CHECKS: '1', BP_UPDATE_TEST_MODE: '1' })).toBe(true);
    expect(updaterNetworkDisabled({ BP_TEST_MODE: '1', BP_UPDATE_TEST_MODE: '1' })).toBe(false);
    expect(updaterNetworkDisabled({})).toBe(false);
  });

  it('permits only an explicitly gated IPv4 loopback feed override', () => {
    expect(resolveLoopbackFeedOverride({})).toBeNull();
    expect(resolveLoopbackFeedOverride({
      BP_UPDATE_TEST_MODE: '1',
      BP_UPDATE_FEED_URL: 'http://127.0.0.1:4317/updates',
    })).toBe('http://127.0.0.1:4317/updates');

    expect(() => resolveLoopbackFeedOverride({
      BP_UPDATE_FEED_URL: 'http://127.0.0.1:4317/updates',
    })).toThrow(/only allowed/);
    expect(() => resolveLoopbackFeedOverride({
      BP_UPDATE_TEST_MODE: '1',
      BP_UPDATE_FEED_URL: 'https://127.0.0.1:4317/updates',
    })).toThrow(/must use http/);
    expect(() => resolveLoopbackFeedOverride({
      BP_UPDATE_TEST_MODE: '1',
      BP_UPDATE_FEED_URL: 'http://localhost:4317/updates',
    })).toThrow(/must use http/);
    expect(() => resolveLoopbackFeedOverride({
      BP_UPDATE_TEST_MODE: '1',
      BP_UPDATE_FEED_URL: 'http://127.0.0.1/updates',
    })).toThrow(/explicit port/);
  });
});
