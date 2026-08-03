import type { UpdateChannel, UpdateFrequency } from '../shared/protocol';

export const UPDATE_SETTINGS_SCHEMA_VERSION = 1 as const;
export const UPDATE_SCHEDULER_MAX_WAKE_MS = 60 * 60 * 1_000;

export interface UpdateSettings {
  schemaVersion: typeof UPDATE_SETTINGS_SCHEMA_VERSION;
  frequency: UpdateFrequency;
  lastSuccessfulCheckAt: string | null;
}

const UPDATE_FREQUENCIES = new Set<UpdateFrequency>([
  'never',
  'startup',
  'hourly',
  'sixHours',
  'twelveHours',
  'daily',
  'weekly',
  'monthly',
]);

const FREQUENCY_INTERVAL_MS: Readonly<Partial<Record<UpdateFrequency, number>>> = {
  hourly: 60 * 60 * 1_000,
  sixHours: 6 * 60 * 60 * 1_000,
  twelveHours: 12 * 60 * 60 * 1_000,
  daily: 24 * 60 * 60 * 1_000,
  weekly: 7 * 24 * 60 * 60 * 1_000,
  monthly: 30 * 24 * 60 * 60 * 1_000,
};

export function isUpdateFrequency(value: unknown): value is UpdateFrequency {
  return typeof value === 'string' && UPDATE_FREQUENCIES.has(value as UpdateFrequency);
}

export function getDefaultUpdateFrequency(channel: UpdateChannel | null): UpdateFrequency {
  return channel === 'stable' ? 'weekly' : 'daily';
}

export function createDefaultUpdateSettings(channel: UpdateChannel | null): UpdateSettings {
  return {
    schemaVersion: UPDATE_SETTINGS_SCHEMA_VERSION,
    frequency: getDefaultUpdateFrequency(channel),
    lastSuccessfulCheckAt: null,
  };
}

export function parseUpdateSettings(value: unknown): UpdateSettings | null {
  if (!isRecord(value)
    || value.schemaVersion !== UPDATE_SETTINGS_SCHEMA_VERSION
    || !isUpdateFrequency(value.frequency)
    || !isNullableIsoTimestamp(value.lastSuccessfulCheckAt)) {
    return null;
  }

  return {
    schemaVersion: UPDATE_SETTINGS_SCHEMA_VERSION,
    frequency: value.frequency,
    lastSuccessfulCheckAt: value.lastSuccessfulCheckAt,
  };
}

export function resolveUpdateChannel(buildMetadata: unknown): UpdateChannel | null {
  if (buildMetadata === 'stable' || buildMetadata === 'beta') {
    return buildMetadata;
  }

  if (!isRecord(buildMetadata)) {
    return null;
  }

  const channel = buildMetadata.butterPaperChannel;
  return channel === 'stable' || channel === 'beta' ? channel : null;
}

export function isAutomaticCheckDue(
  frequency: UpdateFrequency,
  lastSuccessfulCheckAt: string | null,
  now: Date,
): boolean {
  if (frequency === 'never') {
    return false;
  }

  if (frequency === 'startup') {
    return true;
  }

  const intervalMs = FREQUENCY_INTERVAL_MS[frequency];
  if (intervalMs == null || lastSuccessfulCheckAt == null) {
    return true;
  }

  const lastCheckMs = Date.parse(lastSuccessfulCheckAt);
  const nowMs = now.getTime();
  if (!Number.isFinite(lastCheckMs) || lastCheckMs > nowMs) {
    return true;
  }

  return nowMs - lastCheckMs >= intervalMs;
}

export function getSchedulerWakeInterval(frequency: UpdateFrequency): number | null {
  const intervalMs = FREQUENCY_INTERVAL_MS[frequency];
  return intervalMs == null ? null : Math.min(intervalMs, UPDATE_SCHEDULER_MAX_WAKE_MS);
}

export function updaterNetworkDisabled(environment: NodeJS.ProcessEnv): boolean {
  if (environment.BP_DISABLE_UPDATE_CHECKS === '1') {
    return true;
  }

  return environment.BP_TEST_MODE === '1' && environment.BP_UPDATE_TEST_MODE !== '1';
}

export function resolveLoopbackFeedOverride(environment: NodeJS.ProcessEnv): string | null {
  const rawUrl = environment.BP_UPDATE_FEED_URL?.trim();
  if (!rawUrl) {
    return null;
  }

  if (environment.BP_UPDATE_TEST_MODE !== '1') {
    throw new Error('BP_UPDATE_FEED_URL is only allowed when BP_UPDATE_TEST_MODE=1.');
  }

  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    throw new Error('BP_UPDATE_FEED_URL must be a valid loopback URL.');
  }

  if (url.protocol !== 'http:'
    || url.hostname !== '127.0.0.1'
    || url.port === ''
    || url.username !== ''
    || url.password !== '') {
    throw new Error('BP_UPDATE_FEED_URL must use http://127.0.0.1 with an explicit port.');
  }

  return url.toString();
}

export function resolveLoopbackTufRepositoryOverride(environment: NodeJS.ProcessEnv): string | null {
  const rawUrl = environment.BP_TUF_REPOSITORY_URL?.trim();
  if (!rawUrl) {
    return null;
  }

  if (environment.BP_UPDATE_TEST_MODE !== '1') {
    throw new Error('BP_TUF_REPOSITORY_URL is only allowed when BP_UPDATE_TEST_MODE=1.');
  }

  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    throw new Error('BP_TUF_REPOSITORY_URL must be a valid loopback URL.');
  }

  if (url.protocol !== 'http:'
    || url.hostname !== '127.0.0.1'
    || url.port === ''
    || url.username !== ''
    || url.password !== ''
    || url.search !== ''
    || url.hash !== '') {
    throw new Error('BP_TUF_REPOSITORY_URL must use http://127.0.0.1 with an explicit port.');
  }

  return url.toString().replace(/\/$/, '');
}

function isNullableIsoTimestamp(value: unknown): value is string | null {
  if (value === null) {
    return true;
  }

  return typeof value === 'string'
    && Number.isFinite(Date.parse(value))
    && new Date(value).toISOString() === value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
