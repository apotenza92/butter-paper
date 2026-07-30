import { EventEmitter } from 'node:events';
import { mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { UpdateStatus } from '../shared/protocol';
import type { TufVerifiedUpdateFeed } from './tufUpdateFeed';
import {
  DesktopUpdaterService,
  UPDATE_SETTINGS_FILE_NAME,
  loadUpdateSettings,
  writeUpdateSettingsAtomic,
  type ElectronUpdaterLike,
  type UpdaterScheduler,
} from './updater';

class FakeUpdater extends EventEmitter implements ElectronUpdaterLike {
  private channelValue: string | null = null;
  allowPrerelease = false;
  allowDowngrade = false;
  autoDownload = false;
  autoInstallOnAppQuit = true;
  disableWebInstaller?: boolean;
  checkForUpdates = vi.fn<() => Promise<unknown>>(async () => undefined);
  downloadUpdate = vi.fn<() => Promise<unknown>>(async () => []);
  quitAndInstall = vi.fn();
  setFeedURL = vi.fn();

  get channel(): string | null {
    return this.channelValue;
  }

  set channel(value: string | null) {
    this.channelValue = value;
    // electron-updater's channel setter can enable downgrades.
    this.allowDowngrade = true;
  }
}

class FakeScheduler implements UpdaterScheduler {
  readonly scheduled: Array<{ callback: () => void; delayMs: number; handle: object }> = [];
  readonly cleared: unknown[] = [];

  setInterval(callback: () => void, delayMs: number): unknown {
    const handle = {};
    this.scheduled.push({ callback, delayMs, handle });
    return handle;
  }

  clearInterval(handle: unknown): void {
    this.cleared.push(handle);
  }
}

const silentLogger = { error: vi.fn() };
const fixedNow = new Date('2026-07-22T12:00:00.000Z');

async function createUserDataDirectory(): Promise<string> {
  return mkdtemp(join(tmpdir(), 'butter-paper-updater-'));
}

function createService(options: {
  updater?: FakeUpdater;
  scheduler?: FakeScheduler;
  userDataPath: string;
  isPackaged?: boolean;
  buildMetadata?: unknown;
  environment?: NodeJS.ProcessEnv;
  platform?: NodeJS.Platform;
  createVerifiedFeed?: () => Promise<TufVerifiedUpdateFeed>;
}): { service: DesktopUpdaterService; updater: FakeUpdater; scheduler: FakeScheduler } {
  const updater = options.updater ?? new FakeUpdater();
  const scheduler = options.scheduler ?? new FakeScheduler();
  return {
    updater,
    scheduler,
    service: new DesktopUpdaterService({
      updater,
      scheduler,
      userDataPath: options.userDataPath,
      isPackaged: options.isPackaged ?? true,
      currentVersion: '0.0.1',
      platform: options.platform ?? 'darwin',
      resourcesPath: join(options.userDataPath, 'resources'),
      buildMetadata: options.buildMetadata ?? { butterPaperChannel: 'stable' },
      environment: options.environment ?? {},
      now: () => fixedNow,
      logger: silentLogger,
      createVerifiedFeed: options.createVerifiedFeed,
    }),
  };
}

function createFakeVerifiedFeed(): TufVerifiedUpdateFeed {
  return {
    close: vi.fn(async () => undefined),
    feedUrl: 'http://127.0.0.1:4317',
    refresh: vi.fn(async () => '/tmp/latest.yml'),
    targetPath: '/tmp/latest.yml',
    trustInitialized: true,
    trustedRootPath: '/tmp/root.json',
  };
}

describe('update settings persistence', () => {
  it('writes a versioned settings file atomically without leaving temporary files', async () => {
    const userDataPath = await createUserDataDirectory();
    const settingsPath = join(userDataPath, UPDATE_SETTINGS_FILE_NAME);
    await writeUpdateSettingsAtomic(settingsPath, {
      schemaVersion: 1,
      frequency: 'weekly',
      lastSuccessfulCheckAt: null,
    });

    expect(JSON.parse(await readFile(settingsPath, 'utf8'))).toEqual({
      schemaVersion: 1,
      frequency: 'weekly',
      lastSuccessfulCheckAt: null,
    });
    expect((await readdir(userDataPath)).filter(name => name.endsWith('.tmp'))).toEqual([]);
  });

  it('recovers invalid state to daily and reports that it should be replaced', async () => {
    const userDataPath = await createUserDataDirectory();
    const settingsPath = join(userDataPath, UPDATE_SETTINGS_FILE_NAME);
    await writeFile(settingsPath, '{broken json', 'utf8');

    await expect(loadUpdateSettings(settingsPath)).resolves.toEqual({
      settings: {
        schemaVersion: 1,
        frequency: 'daily',
        lastSuccessfulCheckAt: null,
      },
      recovered: true,
    });
  });
});

describe('DesktopUpdaterService', () => {
  beforeEach(() => {
    silentLogger.error.mockClear();
  });

  it('configures stable automatic background downloads without install-on-quit or downgrades', async () => {
    const { service, updater, scheduler } = createService({
      userDataPath: await createUserDataDirectory(),
    });

    await service.start();

    expect(updater.channel).toBe('latest');
    expect(updater.allowPrerelease).toBe(false);
    expect(updater.allowDowngrade).toBe(false);
    expect(updater.autoDownload).toBe(false);
    expect(updater.autoInstallOnAppQuit).toBe(false);
    expect(updater.checkForUpdates).toHaveBeenCalledTimes(1);
    expect(scheduler.scheduled).toHaveLength(1);
    expect(scheduler.scheduled[0]?.delayMs).toBe(60 * 60 * 1_000);
  });

  it('does not check before the service has been started', async () => {
    const { service, updater } = createService({
      userDataPath: await createUserDataDirectory(),
    });

    await expect(service.checkNow()).resolves.toBe(false);
    expect(updater.checkForUpdates).not.toHaveBeenCalled();
  });

  it('configures beta from explicit packaged metadata', async () => {
    const { service, updater } = createService({
      userDataPath: await createUserDataDirectory(),
      buildMetadata: { butterPaperChannel: 'beta' },
    });

    await service.start();

    expect(updater.channel).toBe('latest');
    expect(service.getStatus().channel).toBe('beta');
    expect(updater.allowPrerelease).toBe(true);
    expect(updater.allowDowngrade).toBe(false);
  });

  it('disables network activity in development, ordinary tests, and builds without an explicit channel', async () => {
    const cases = [
      { isPackaged: false, buildMetadata: { butterPaperChannel: 'stable' }, environment: {} },
      { isPackaged: true, buildMetadata: { butterPaperChannel: 'stable' }, environment: { BP_TEST_MODE: '1' } },
      { isPackaged: true, buildMetadata: {}, environment: {} },
    ];

    for (const testCase of cases) {
      const { service, updater, scheduler } = createService({
        userDataPath: await createUserDataDirectory(),
        ...testCase,
      });
      await service.start();
      expect(service.getStatus().phase).toBe('disabled');
      expect(updater.checkForUpdates).not.toHaveBeenCalled();
      expect(scheduler.scheduled).toHaveLength(0);
    }
  });

  it('enables Windows and AppImage updates only through a TUF-verified local feed', async () => {
    for (const platform of ['win32', 'linux'] as const) {
      const feed = createFakeVerifiedFeed();
      const userDataPath = await createUserDataDirectory();
      const createVerifiedFeed = vi.fn(async () => feed);
      const { service, updater } = createService({
        userDataPath,
        platform,
        buildMetadata: {
          butterPaperChannel: 'stable',
          butterPaperTufRepositoryUrl: `https://updates.example/${platform}/tuf`,
          butterPaperUpdateTargetName: platform === 'win32' ? 'latest.yml' : 'latest-linux.yml',
        },
        environment: platform === 'linux' ? { APPIMAGE: '/tmp/Butter-Paper.AppImage' } : {},
        createVerifiedFeed,
      });

      await service.start();
      expect(service.getStatus()).toMatchObject({
        phase: 'checking',
        disabledReason: null,
        enabled: true,
      });
      expect(createVerifiedFeed).toHaveBeenCalledWith(expect.objectContaining({
        embeddedRootPath: join(userDataPath, 'resources', 'update-trust', 'root.json'),
        repositoryUrl: `https://updates.example/${platform}/tuf`,
        targetName: platform === 'win32' ? 'latest.yml' : 'latest-linux.yml',
        trustDirectory: join(userDataPath, 'update-trust'),
      }));
      expect(updater.setFeedURL).toHaveBeenCalledWith({
        provider: 'generic',
        url: feed.feedUrl,
      });
      expect(feed.refresh).toHaveBeenCalledTimes(1);
      if (platform === 'win32') {
        expect(updater.disableWebInstaller).toBe(true);
      }
      await service.stop();
    }
  });

  it('leaves DEB and RPM upgrades to the Linux package manager', async () => {
    const { service, updater } = createService({
      userDataPath: await createUserDataDirectory(),
      platform: 'linux',
      environment: {},
    });
    await service.start();
    expect(service.getStatus()).toMatchObject({
      phase: 'disabled',
      disabledReason: 'platform-policy',
    });
    expect(updater.checkForUpdates).not.toHaveBeenCalled();
  });

  it('allows only a gated loopback feed override and disables invalid overrides', async () => {
    const allowed = createService({
      userDataPath: await createUserDataDirectory(),
      environment: {
        BP_TEST_MODE: '1',
        BP_UPDATE_TEST_MODE: '1',
        BP_UPDATE_FEED_URL: 'http://127.0.0.1:4317/updates',
      },
    });
    await allowed.service.start();
    expect(allowed.updater.setFeedURL).toHaveBeenCalledWith({
      provider: 'generic',
      url: 'http://127.0.0.1:4317/updates',
    });

    const rejected = createService({
      userDataPath: await createUserDataDirectory(),
      environment: { BP_UPDATE_FEED_URL: 'https://updates.example.com/' },
    });
    await rejected.service.start();
    expect(rejected.service.getStatus()).toMatchObject({
      phase: 'disabled',
      disabledReason: 'configuration',
    });
    expect(rejected.updater.setFeedURL).not.toHaveBeenCalled();
    expect(rejected.updater.checkForUpdates).not.toHaveBeenCalled();
  });

  it('persists frequency changes and stops scheduling never/startup', async () => {
    const userDataPath = await createUserDataDirectory();
    const { service, scheduler } = createService({ userDataPath });
    await service.start();

    await service.setFrequency('weekly');
    expect(service.getStatus()).toMatchObject({
      frequency: 'weekly',
      automaticChecksEnabled: true,
    });
    expect((await loadUpdateSettings(service.settingsPath)).settings.frequency).toBe('weekly');
    expect(scheduler.scheduled.at(-1)?.delayMs).toBe(60 * 60 * 1_000);

    await service.setFrequency('never');
    expect(service.getStatus().automaticChecksEnabled).toBe(false);
    expect(scheduler.cleared.length).toBeGreaterThan(0);
    expect(scheduler.scheduled).toHaveLength(2);
  });

  it('does not overlap checks and retries only when the recurring schedule is due', async () => {
    const userDataPath = await createUserDataDirectory();
    await writeUpdateSettingsAtomic(join(userDataPath, UPDATE_SETTINGS_FILE_NAME), {
      schemaVersion: 1,
      frequency: 'daily',
      lastSuccessfulCheckAt: '2026-07-22T11:00:00.000Z',
    });
    const updater = new FakeUpdater();
    let resolveCheck: (() => void) | undefined;
    updater.checkForUpdates.mockImplementation(() => new Promise<void>(resolve => {
      resolveCheck = resolve;
    }));
    const { service, scheduler } = createService({ userDataPath, updater });
    await service.start();
    expect(updater.checkForUpdates).not.toHaveBeenCalled();

    await scheduler.scheduled[0]?.callback();
    expect(updater.checkForUpdates).not.toHaveBeenCalled();

    const first = service.checkNow();
    expect(updater.checkForUpdates).toHaveBeenCalledTimes(1);
    await expect(service.checkNow()).resolves.toBe(false);
    resolveCheck?.();
    await expect(first).resolves.toBe(true);
  });

  it('publishes download state, records successful checks, and installs only a downloaded update', async () => {
    const userDataPath = await createUserDataDirectory();
    const { service, updater } = createService({ userDataPath });
    const snapshots: UpdateStatus[] = [];
    service.subscribe(status => snapshots.push(status));
    await service.start();

    await expect(service.installDownloaded()).resolves.toBe(false);
    updater.emit('update-available', {
      version: '0.0.2',
      butterPaperChannel: 'stable',
      releaseNotes: '- Added release notes to the updater.',
    });
    await vi.waitFor(() => expect(updater.downloadUpdate).toHaveBeenCalledTimes(1));
    updater.emit('download-progress', { percent: 42.5 });
    updater.emit('update-downloaded', { version: '0.0.2' });

    expect(service.getStatus()).toMatchObject({
      phase: 'downloaded',
      availableVersion: '0.0.2',
      releaseNotes: '- Added release notes to the updater.',
      downloadPercent: 100,
      lastSuccessfulCheckAt: fixedNow.toISOString(),
    });
    expect(snapshots.some(status => status.phase === 'downloading' && status.downloadPercent === 42.5)).toBe(true);
    service.setRestartBlocked(true);
    await expect(service.installDownloaded()).resolves.toBe(false);
    expect(updater.quitAndInstall).not.toHaveBeenCalled();
    service.setRestartBlocked(false);
    await expect(service.installDownloaded()).resolves.toBe(true);
    expect(updater.quitAndInstall).toHaveBeenCalledWith(false, true);

    await vi.waitFor(async () => {
      const loaded = await loadUpdateSettings(service.settingsPath);
      expect(loaded.settings.lastSuccessfulCheckAt).toBe(fixedNow.toISOString());
    });
  });

  it('closes the authenticated loopback feed before handing off to a Windows installer', async () => {
    const feed = createFakeVerifiedFeed();
    const updater = new FakeUpdater();
    const { service } = createService({
      userDataPath: await createUserDataDirectory(),
      platform: 'win32',
      updater,
      buildMetadata: {
        butterPaperChannel: 'stable',
        butterPaperTufRepositoryUrl: 'https://updates.example/win32/tuf',
        butterPaperUpdateTargetName: 'latest.yml',
      },
      createVerifiedFeed: async () => feed,
    });
    await service.start();
    updater.emit('update-available', {
      version: '0.0.2',
      butterPaperChannel: 'stable',
    });
    await vi.waitFor(() => expect(updater.downloadUpdate).toHaveBeenCalledTimes(1));
    updater.emit('update-downloaded', { version: '0.0.2' });

    await expect(service.installDownloaded()).resolves.toBe(true);
    expect(feed.close).toHaveBeenCalledTimes(1);
    expect(vi.mocked(feed.close).mock.invocationCallOrder[0]).toBeLessThan(
      updater.quitAndInstall.mock.invocationCallOrder[0]!,
    );
    expect(updater.quitAndInstall).toHaveBeenCalledWith(true, true);
  });

  it('rejects missing or cross-channel metadata before downloading', async () => {
    for (const butterPaperChannel of [undefined, 'beta']) {
      const { service, updater } = createService({
        userDataPath: await createUserDataDirectory(),
      });
      await service.start();

      updater.emit('update-available', { version: '0.0.2', butterPaperChannel });

      await vi.waitFor(() => expect(service.getStatus().phase).toBe('error'));
      expect(service.getStatus().errorMessage).toMatch(/Rejected update metadata for channel/);
      expect(updater.downloadUpdate).not.toHaveBeenCalled();
      expect(updater.quitAndInstall).not.toHaveBeenCalled();
    }
  });

  it('removes updater and scheduler listeners when stopped', async () => {
    const { service, updater, scheduler } = createService({
      userDataPath: await createUserDataDirectory(),
    });
    await service.start();
    const statusBeforeStop = service.getStatus();
    await service.stop();

    expect(scheduler.cleared).toHaveLength(1);
    updater.emit('update-downloaded', { version: '9.9.9' });
    expect(service.getStatus()).toEqual(statusBeforeStop);
  });
});
