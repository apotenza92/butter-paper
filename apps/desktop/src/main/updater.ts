import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { appendFileSync, mkdirSync, renameSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, isAbsolute, join } from 'node:path';
import type {
  UpdateDisabledReason,
  UpdateFrequency,
  UpdateStatus,
} from '../shared/protocol';
import {
  createDefaultUpdateSettings,
  getSchedulerWakeInterval,
  isAutomaticCheckDue,
  isUpdateFrequency,
  parseUpdateSettings,
  resolveLoopbackFeedOverride,
  resolveLoopbackTufRepositoryOverride,
  resolveUpdateChannel,
  updaterNetworkDisabled,
  type UpdateSettings,
} from './updatePolicy';
import {
  createTufVerifiedUpdateFeed,
  type CreateTufVerifiedUpdateFeedOptions,
  type TufVerifiedUpdateFeed,
} from './tufUpdateFeed';

export const UPDATE_SETTINGS_FILE_NAME = 'update-settings.json';
let settingsWriteSequence = 0;

interface UpdateInfoLike {
  version?: string;
  butterPaperChannel?: string;
  releaseNotes?: string | Array<{ version?: string; note?: string }> | null;
}

interface DownloadProgressLike {
  percent?: number;
}

type UpdaterEventListener = (...args: any[]) => void;

export interface ElectronUpdaterLike {
  channel: string | null;
  allowPrerelease: boolean;
  allowDowngrade: boolean;
  autoDownload: boolean;
  autoInstallOnAppQuit: boolean;
  disableWebInstaller?: boolean;
  checkForUpdates(): Promise<unknown>;
  downloadUpdate(): Promise<unknown>;
  quitAndInstall(isSilent?: boolean, isForceRunAfter?: boolean): void;
  setFeedURL?(options: { provider: 'generic'; url: string }): void;
  on(eventName: string, listener: UpdaterEventListener): unknown;
  removeListener(eventName: string, listener: UpdaterEventListener): unknown;
}

export interface UpdaterLogger {
  error(message: string, error?: unknown): void;
}

export interface UpdaterScheduler {
  setInterval(callback: () => void, delayMs: number): unknown;
  clearInterval(handle: unknown): void;
}

export interface DesktopUpdaterServiceOptions {
  updater: ElectronUpdaterLike;
  isPackaged: boolean;
  userDataPath: string;
  currentVersion: string;
  resourcesPath?: string;
  platform?: NodeJS.Platform;
  buildMetadata: unknown;
  environment?: NodeJS.ProcessEnv;
  now?: () => Date;
  scheduler?: UpdaterScheduler;
  logger?: UpdaterLogger;
  createVerifiedFeed?: (
    options: CreateTufVerifiedUpdateFeedOptions,
  ) => Promise<TufVerifiedUpdateFeed>;
}

export interface LoadedUpdateSettings {
  settings: UpdateSettings;
  recovered: boolean;
}

const defaultScheduler: UpdaterScheduler = {
  setInterval(callback, delayMs) {
    const handle = setInterval(callback, delayMs);
    handle.unref();
    return handle;
  },
  clearInterval(handle) {
    clearInterval(handle as ReturnType<typeof setInterval>);
  },
};

const defaultLogger: UpdaterLogger = {
  error(message, error) {
    console.error(message, error);
  },
};

export function loadElectronAutoUpdater(): ElectronUpdaterLike {
  const require = createRequire(import.meta.url);
  const updaterModule = require('electron-updater') as { autoUpdater?: ElectronUpdaterLike };
  if (updaterModule.autoUpdater == null) {
    throw new Error('electron-updater did not export autoUpdater.');
  }
  return updaterModule.autoUpdater;
}

export async function loadUpdateSettings(settingsPath: string): Promise<LoadedUpdateSettings> {
  try {
    const parsed = JSON.parse(await readFile(settingsPath, 'utf8')) as unknown;
    const settings = parseUpdateSettings(parsed);
    if (settings == null) {
      return { settings: createDefaultUpdateSettings(), recovered: true };
    }
    return { settings, recovered: false };
  } catch (error) {
    if (isNodeError(error) && error.code === 'ENOENT') {
      return { settings: createDefaultUpdateSettings(), recovered: false };
    }
    return { settings: createDefaultUpdateSettings(), recovered: true };
  }
}

export async function writeUpdateSettingsAtomic(
  settingsPath: string,
  settings: UpdateSettings,
): Promise<void> {
  await mkdir(dirname(settingsPath), { recursive: true });
  settingsWriteSequence += 1;
  const temporaryPath = `${settingsPath}.${process.pid}.${Date.now()}.${settingsWriteSequence}.tmp`;
  try {
    await writeFile(temporaryPath, `${JSON.stringify(settings, null, 2)}\n`, {
      encoding: 'utf8',
      mode: 0o600,
    });
    await rename(temporaryPath, settingsPath);
  } finally {
    await rm(temporaryPath, { force: true }).catch(() => undefined);
  }
}

export class DesktopUpdaterService {
  readonly settingsPath: string;

  private readonly updater: ElectronUpdaterLike;
  private readonly isPackaged: boolean;
  private readonly environment: NodeJS.ProcessEnv;
  private readonly now: () => Date;
  private readonly scheduler: UpdaterScheduler;
  private readonly logger: UpdaterLogger;
  private readonly platform: NodeJS.Platform;
  private readonly resourcesPath: string;
  private readonly userDataPath: string;
  private readonly buildMetadata: unknown;
  private readonly createVerifiedFeed;
  private readonly currentVersion: string;
  private readonly listeners = new Set<(status: UpdateStatus) => void>();
  private readonly updaterListeners = new Map<string, UpdaterEventListener>();
  private readonly channel;
  private settings = createDefaultUpdateSettings();
  private status: UpdateStatus;
  private schedulerHandle: unknown = null;
  private started = false;
  private checkInFlight = false;
  private restartBlocked = false;
  private persistenceQueue: Promise<void> = Promise.resolve();
  private verifiedFeed: TufVerifiedUpdateFeed | null = null;

  constructor(options: DesktopUpdaterServiceOptions) {
    this.updater = options.updater;
    this.isPackaged = options.isPackaged;
    this.environment = options.environment ?? process.env;
    this.now = options.now ?? (() => new Date());
    this.scheduler = options.scheduler ?? defaultScheduler;
    this.logger = options.logger ?? defaultLogger;
    this.platform = options.platform ?? process.platform;
    this.resourcesPath = options.resourcesPath ?? process.resourcesPath;
    this.userDataPath = options.userDataPath;
    this.buildMetadata = options.buildMetadata;
    this.createVerifiedFeed = options.createVerifiedFeed ?? createTufVerifiedUpdateFeed;
    this.currentVersion = options.currentVersion;
    this.channel = resolveUpdateChannel(options.buildMetadata);
    this.settingsPath = join(options.userDataPath, UPDATE_SETTINGS_FILE_NAME);

    const disabledReason = this.getInitialDisabledReason();
    this.status = {
      phase: disabledReason == null ? 'idle' : 'disabled',
      channel: this.channel,
      frequency: this.settings.frequency,
      enabled: disabledReason == null,
      automaticChecksEnabled: disabledReason == null,
      currentVersion: options.currentVersion,
      availableVersion: null,
      releaseNotes: null,
      downloadPercent: null,
      lastSuccessfulCheckAt: null,
      disabledReason,
      errorMessage: null,
    };
  }

  getStatus(): UpdateStatus {
    return { ...this.status };
  }

  subscribe(listener: (status: UpdateStatus) => void): () => void {
    this.listeners.add(listener);
    listener(this.getStatus());
    return () => {
      this.listeners.delete(listener);
    };
  }

  async start(): Promise<void> {
    if (this.started) {
      return;
    }
    this.started = true;

    const loaded = await loadUpdateSettings(this.settingsPath);
    this.settings = loaded.settings;
    this.patchStatus({
      frequency: this.settings.frequency,
      lastSuccessfulCheckAt: this.settings.lastSuccessfulCheckAt,
      automaticChecksEnabled: this.status.enabled && this.settings.frequency !== 'never',
    });

    if (loaded.recovered) {
      await this.queueSettingsWrite().catch(error => {
        this.logger.error('Failed to replace invalid update settings.', error);
      });
    }

    if (!this.status.enabled) {
      return;
    }

    try {
      await this.configureUpdater();
    } catch (error) {
      this.disableForConfiguration(error);
      return;
    }

    this.attachUpdaterListeners();
    this.reschedule();
    if (this.environment.BP_UPDATE_TEST_MODE === '1'
      && this.environment.BP_UPDATE_EXPECT_VERSION === this.currentVersion) {
      this.recordTestEvent('updated-runtime-launched');
      return;
    }

    if (this.settings.frequency === 'startup'
      || isAutomaticCheckDue(
        this.settings.frequency,
        this.settings.lastSuccessfulCheckAt,
        this.now(),
      )) {
      void this.runCheck();
    }
  }

  async stop(): Promise<void> {
    if (this.schedulerHandle != null) {
      this.scheduler.clearInterval(this.schedulerHandle);
      this.schedulerHandle = null;
    }

    for (const [eventName, listener] of this.updaterListeners) {
      this.updater.removeListener(eventName, listener);
    }
    this.updaterListeners.clear();
    this.started = false;
    await this.closeVerifiedFeed();
  }

  async setFrequency(frequency: UpdateFrequency): Promise<void> {
    if (!isUpdateFrequency(frequency)) {
      throw new TypeError(`Unsupported update frequency: ${String(frequency)}`);
    }

    this.settings = { ...this.settings, frequency };
    this.patchStatus({
      frequency,
      automaticChecksEnabled: this.status.enabled && frequency !== 'never',
    });
    await this.queueSettingsWrite();
    if (this.started) {
      this.reschedule();
    }
  }

  checkNow(): Promise<boolean> {
    return this.runCheck();
  }

  async installDownloaded(): Promise<boolean> {
    if (this.status.phase !== 'downloaded' || this.restartBlocked) {
      return false;
    }

    try {
      await this.closeVerifiedFeed();
      this.recordTestEvent('update-install-started');
      this.updater.quitAndInstall(this.platform !== 'darwin', true);
      return true;
    } catch (error) {
      this.handleUpdaterError(error);
      return false;
    }
  }

  setRestartBlocked(blocked: boolean): void {
    this.restartBlocked = blocked;
  }

  private getInitialDisabledReason(): UpdateDisabledReason | null {
    if (!this.isPackaged) {
      return 'development';
    }
    if (updaterNetworkDisabled(this.environment)) {
      return 'test-mode';
    }
    if (!['darwin', 'win32', 'linux'].includes(this.platform)) {
      return 'platform-policy';
    }
    if (this.platform === 'linux'
      && !this.environment.APPIMAGE
      && this.environment.BP_UPDATE_TEST_MODE !== '1') {
      return 'platform-policy';
    }
    if (this.channel == null) {
      return 'configuration';
    }
    return null;
  }

  private async configureUpdater(): Promise<void> {
    if (this.channel == null) {
      throw new Error('Packaged build metadata must declare butterPaperChannel.');
    }

    // Stable and beta use isolated generic-provider feed roots. Each root serves
    // electron-updater's standard latest* metadata files.
    this.updater.channel = 'latest';
    this.updater.allowPrerelease = this.channel === 'beta';
    // Setting channel can enable downgrades in electron-updater, so reset it afterwards.
    this.updater.allowDowngrade = false;
    // Keep downloads automatic from the user's perspective, but start them
    // ourselves only after the feed declares the same stable/beta channel as
    // the installed application.
    this.updater.autoDownload = false;
    this.updater.autoInstallOnAppQuit = false;
    if (this.platform === 'win32') {
      this.updater.disableWebInstaller = true;
    }

    const feedOverride = resolveLoopbackFeedOverride(this.environment);
    if (this.platform === 'darwin') {
      if (feedOverride != null) {
        this.setFeedUrl(feedOverride);
      }
      return;
    }
    if (feedOverride != null) {
      throw new Error('Windows and Linux updater tests must use TUF-authenticated metadata.');
    }

    const metadata = readPackagedUpdateMetadata(this.buildMetadata);
    const repositoryOverride = resolveLoopbackTufRepositoryOverride(this.environment);
    const repositoryUrl = repositoryOverride ?? metadata?.butterPaperTufRepositoryUrl;
    const targetName = metadata?.butterPaperUpdateTargetName;
    if (repositoryUrl == null || targetName == null) {
      throw new Error('Packaged Butter Paper TUF updater metadata is invalid.');
    }

    this.verifiedFeed = await this.createVerifiedFeed({
      embeddedRootPath: join(this.resourcesPath, 'update-trust', 'root.json'),
      expectedChannel: this.channel,
      repositoryUrl,
      targetName,
      trustDirectory: join(this.userDataPath, 'update-trust'),
      allowLoopbackHttp: repositoryOverride != null,
    });
    this.setFeedUrl(this.verifiedFeed.feedUrl);
  }

  private attachUpdaterListeners(): void {
    this.addUpdaterListener('checking-for-update', () => {
      this.patchStatus({ phase: 'checking', errorMessage: null });
    });
    this.addUpdaterListener('update-available', (info: UpdateInfoLike) => {
      this.recordTestEvent('update-available', { version: info?.version ?? null });
      void this.handleUpdateAvailable(info);
    });
    this.addUpdaterListener('update-not-available', () => {
      this.recordTestEvent('update-not-available');
      this.recordSuccessfulCheck();
      this.patchStatus({
        phase: 'idle',
        availableVersion: null,
        releaseNotes: null,
        downloadPercent: null,
        errorMessage: null,
      });
    });
    this.addUpdaterListener('download-progress', (progress: DownloadProgressLike) => {
      this.patchStatus({
        phase: 'downloading',
        downloadPercent: normalisePercent(progress?.percent),
      });
    });
    this.addUpdaterListener('update-downloaded', (info: UpdateInfoLike) => {
      this.patchStatus({
        phase: 'downloaded',
        availableVersion: normaliseVersion(info?.version) ?? this.status.availableVersion,
        releaseNotes: normaliseReleaseNotes(info?.releaseNotes) ?? this.status.releaseNotes,
        downloadPercent: 100,
        errorMessage: null,
      });
      this.recordTestEvent('update-downloaded', { version: info?.version ?? null });
      if (this.environment.BP_UPDATE_TEST_MODE === '1'
        && this.environment.BP_UPDATE_INSTALL === '1') {
        setTimeout(() => {
          void this.installDownloaded();
        }, 100).unref();
      }
    });
    this.addUpdaterListener('error', (error: unknown) => {
      this.handleUpdaterError(error);
    });
  }

  private async handleUpdateAvailable(info: UpdateInfoLike): Promise<void> {
    if (this.platform === 'darwin'
      && info?.butterPaperChannel != null
      && info.butterPaperChannel !== this.channel) {
      this.handleUpdaterError(new Error(
        `Rejected update metadata for channel ${String(info?.butterPaperChannel ?? '<missing>')}; expected ${this.channel}.`,
      ));
      return;
    }

    this.recordSuccessfulCheck();
    this.patchStatus({
      phase: 'available',
      availableVersion: normaliseVersion(info?.version),
      releaseNotes: normaliseReleaseNotes(info?.releaseNotes),
      downloadPercent: null,
      errorMessage: null,
    });
    try {
      await this.updater.downloadUpdate();
    } catch (error) {
      this.handleUpdaterError(error);
    }
  }

  private addUpdaterListener(eventName: string, listener: UpdaterEventListener): void {
    this.updaterListeners.set(eventName, listener);
    this.updater.on(eventName, listener);
  }

  private async runCheck(): Promise<boolean> {
    if (!this.started
      || !this.status.enabled
      || this.checkInFlight
      || updateWorkPending(this.status.phase)) {
      return false;
    }

    this.checkInFlight = true;
    this.patchStatus({ phase: 'checking', errorMessage: null });
    try {
      if (this.verifiedFeed != null) {
        await this.verifiedFeed.refresh();
      }
      await this.updater.checkForUpdates();
      return true;
    } catch (error) {
      this.handleUpdaterError(error);
      return false;
    } finally {
      this.checkInFlight = false;
    }
  }

  private async checkIfDue(): Promise<void> {
    if (isAutomaticCheckDue(
      this.settings.frequency,
      this.settings.lastSuccessfulCheckAt,
      this.now(),
    )) {
      await this.runCheck();
    }
  }

  private reschedule(): void {
    if (this.schedulerHandle != null) {
      this.scheduler.clearInterval(this.schedulerHandle);
      this.schedulerHandle = null;
    }

    if (!this.status.enabled) {
      return;
    }

    const wakeInterval = getSchedulerWakeInterval(this.settings.frequency);
    if (wakeInterval == null) {
      return;
    }

    this.schedulerHandle = this.scheduler.setInterval(() => {
      void this.checkIfDue();
    }, wakeInterval);
  }

  private recordSuccessfulCheck(): void {
    const timestamp = this.now().toISOString();
    this.settings = { ...this.settings, lastSuccessfulCheckAt: timestamp };
    this.patchStatus({ lastSuccessfulCheckAt: timestamp });
    void this.queueSettingsWrite().catch(error => {
      this.logger.error('Failed to persist the successful update check.', error);
    });
  }

  private queueSettingsWrite(): Promise<void> {
    const snapshot = { ...this.settings };
    const write = this.persistenceQueue
      .catch(() => undefined)
      .then(() => writeUpdateSettingsAtomic(this.settingsPath, snapshot));
    this.persistenceQueue = write;
    return write;
  }

  private disableForConfiguration(error: unknown): void {
    this.recordTestEvent('error', {
      message: errorMessage(error),
      phase: 'configuration',
    });
    this.patchStatus({
      phase: 'disabled',
      enabled: false,
      automaticChecksEnabled: false,
      disabledReason: 'configuration',
      errorMessage: errorMessage(error),
    });
  }

  private setFeedUrl(url: string): void {
    if (this.updater.setFeedURL == null) {
      throw new Error('The updater does not support an explicit generic feed.');
    }
    this.updater.setFeedURL({ provider: 'generic', url });
  }

  private async closeVerifiedFeed(): Promise<void> {
    const feed = this.verifiedFeed;
    this.verifiedFeed = null;
    await feed?.close();
  }

  private handleUpdaterError(error: unknown): void {
    this.logger.error('Butter Paper updater error.', error);
    this.recordTestEvent('error', { message: errorMessage(error) });
    this.patchStatus({
      phase: 'error',
      downloadPercent: null,
      errorMessage: errorMessage(error),
    });
  }

  private recordTestEvent(name: string, details: Record<string, unknown> = {}): void {
    if (this.environment.BP_UPDATE_TEST_MODE !== '1') {
      return;
    }
    const eventPath = this.environment.BP_UPDATE_EVENT_PATH?.trim();
    if (!eventPath || !isAbsolute(eventPath)) {
      return;
    }
    const event = {
      name,
      channel: this.channel,
      currentVersion: this.currentVersion,
      executablePath: process.execPath,
      pid: process.pid,
      ...details,
    };
    mkdirSync(dirname(eventPath), { recursive: true, mode: 0o700 });
    appendFileSync(`${eventPath}.jsonl`, `${JSON.stringify(event)}\n`, { mode: 0o600 });
    const temporaryPath = `${eventPath}.${process.pid}.tmp`;
    writeFileSync(temporaryPath, `${JSON.stringify(event)}\n`, { mode: 0o600 });
    renameSync(temporaryPath, eventPath);
  }

  private patchStatus(patch: Partial<UpdateStatus>): void {
    this.status = { ...this.status, ...patch };
    const snapshot = this.getStatus();
    for (const listener of this.listeners) {
      listener(snapshot);
    }
  }
}

function updateWorkPending(phase: UpdateStatus['phase']): boolean {
  return phase === 'checking'
    || phase === 'available'
    || phase === 'downloading'
    || phase === 'downloaded';
}

function normaliseVersion(value: unknown): string | null {
  return typeof value === 'string' && value.trim() !== '' ? value : null;
}

function normaliseReleaseNotes(value: UpdateInfoLike['releaseNotes']): string | null {
  if (typeof value === 'string') {
    const notes = value.trim();
    return notes === '' ? null : notes;
  }
  if (!Array.isArray(value)) {
    return null;
  }
  const notes = value
    .map(entry => {
      const note = typeof entry?.note === 'string' ? entry.note.trim() : '';
      const version = typeof entry?.version === 'string' ? entry.version.trim() : '';
      return note === '' ? '' : `${version === '' ? '' : `${version}\n`}${note}`;
    })
    .filter(Boolean)
    .join('\n\n');
  return notes === '' ? null : notes;
}

function normalisePercent(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return null;
  }
  return Math.min(100, Math.max(0, value));
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error;
}

function readPackagedUpdateMetadata(value: unknown): {
  butterPaperTufRepositoryUrl: string;
  butterPaperUpdateTargetName: string;
} | null {
  if (!isRecord(value)) {
    return null;
  }
  const repositoryUrl = value.butterPaperTufRepositoryUrl;
  const targetName = value.butterPaperUpdateTargetName;
  if (typeof repositoryUrl !== 'string'
    || repositoryUrl.trim() === ''
    || typeof targetName !== 'string'
    || targetName.trim() === '') {
    return null;
  }
  return {
    butterPaperTufRepositoryUrl: repositoryUrl,
    butterPaperUpdateTargetName: targetName,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
