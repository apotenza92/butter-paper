import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const desktopDir = resolve(import.meta.dirname, '../apps/desktop');
const inspectScript = `
  const config = require('./electron-builder.config.cjs');
  process.stdout.write(JSON.stringify({
    appId: config.appId,
    productName: config.productName,
    packageName: config.extraMetadata.name,
    desktopName: config.extraMetadata.desktopName,
    channel: config.extraMetadata.butterPaperChannel,
    output: config.directories.output,
    feed: config.publish?.[0] ?? null,
    tufRepository: config.extraMetadata.butterPaperTufRepositoryUrl,
    updateFeedUrl: config.extraMetadata.butterPaperUpdateFeedUrl,
    updateTargetName: config.extraMetadata.butterPaperUpdateTargetName,
    macArtifact: config.mac.artifactName,
    macFileAssociations: config.mac.fileAssociations,
    windowsArtifact: config.win.artifactName,
    windowsFileAssociations: config.win.fileAssociations ?? null,
    compression: config.compression,
    electronLanguages: config.electronLanguages,
    releaseNotes: config.releaseInfo?.releaseNotes,
    nsisInclude: config.nsis.include,
    nsisOneClick: config.nsis.oneClick,
    nsisUseZip: config.nsis.useZip,
    nsisAllowElevation: config.nsis.allowElevation,
    nsisAllowToChangeInstallationDirectory: config.nsis.allowToChangeInstallationDirectory,
    linuxArtifact: config.linux.artifactName,
    linuxFileAssociations: config.linux.fileAssociations,
    linuxSyncDesktopName: config.linux.syncDesktopName,
    afterPack: config.afterPack,
    macMinimumSystemVersion: config.mac.minimumSystemVersion,
    macExtendInfo: config.mac.extendInfo,
    extraResources: config.extraResources,
    files: config.files,
  }));
`;

function loadConfig(environment: Record<string, string>) {
  const result = spawnSync(process.execPath, ['-e', inspectScript], {
    cwd: desktopDir,
    encoding: 'utf8',
    env: {
      ...process.env,
      CSC_KEYCHAIN: '',
      ...environment,
    },
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout);
  }
  return JSON.parse(result.stdout) as Record<string, unknown>;
}

describe('Electron Builder release identity', () => {
  it('keeps the Node-based TUF runtime external to the browser-oriented Vite bundle', () => {
    const mainConfig = readFileSync(resolve(desktopDir, 'vite.main.config.ts'), 'utf8');
    expect(mainConfig).toContain("'tuf-js'");
    expect(mainConfig).toContain('/^tuf-js\\//');
  });

  it('bakes an isolated stable ARM64 macOS feed and product identity', () => {
    const config = loadConfig({
      BP_RELEASE_CHANNEL: 'stable',
      BP_RELEASE_PLATFORM: 'darwin',
      BP_RELEASE_ARCH: 'arm64',
      BP_RELEASE_OUTPUT_DIR: 'release/stable/darwin/arm64',
    });
    expect(config).toMatchObject({
      appId: 'com.butterpaper.desktop',
      productName: 'Butter Paper',
      packageName: 'butter-paper',
      desktopName: 'butter-paper.desktop',
      channel: 'stable',
      output: 'release/stable/darwin/arm64',
      feed: {
        provider: 'generic',
        url: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/stable/darwin/arm64',
        channel: 'latest',
      },
      tufRepository: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/stable/darwin/arm64/tuf',
      updateFeedUrl: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/stable/darwin/arm64',
      updateTargetName: 'latest-mac.yml',
      macArtifact: 'Butter-Paper-macOS-${arch}.${ext}',
      macFileAssociations: [{
        ext: 'pdf',
        name: 'PDF document',
        role: 'Viewer',
        rank: 'Alternate',
      }],
      windowsArtifact: 'Butter-Paper-Windows-${arch}-Setup.${ext}',
      windowsFileAssociations: null,
      linuxArtifact: 'Butter-Paper-Linux-arm64.${ext}',
      linuxFileAssociations: [{
        ext: 'pdf',
        name: 'PDF document',
        mimeType: 'application/pdf',
      }],
      linuxSyncDesktopName: true,
      afterPack: 'build/after-pack.cjs',
      compression: 'normal',
      electronLanguages: ['en-US'],
      releaseNotes: expect.stringContaining('Made blank and PDF launches faster by delaying non-essential updater'),
      nsisInclude: 'build/installer.nsh',
      nsisOneClick: false,
      macMinimumSystemVersion: '12.0',
      macExtendInfo: {
        NSCameraUsageDescription: 'Butter Paper uses the camera only when you choose to take a signature photo.',
        NSLocalNetworkUsageDescription: 'Butter Paper uses your local network only when you choose to transfer a signature from your phone.',
      },
    });
    expect(config.files).toContain('!**/*.map');
    expect(config.files).toContain('!node_modules/@base-ui/**/*');
    expect(config.files).toContain('!node_modules/react/**/*');
    expect(config.files).toContain('!node_modules/lucide-react/**/*');
    expect(config.files).toContain('!node_modules/@napi-rs/canvas-darwin-x64/**/*');
    expect(config.files).not.toContain('!node_modules/@napi-rs/canvas-darwin-arm64/**/*');
    expect(readFileSync(resolve(desktopDir, 'build/entitlements.mac.plist'), 'utf8'))
      .toContain('com.apple.security.device.camera');
  });

  it('keeps the beta Windows identity separate with an authenticated update feed', () => {
    const config = loadConfig({
      BP_RELEASE_CHANNEL: 'beta',
      BP_RELEASE_PLATFORM: 'win32',
      BP_RELEASE_ARCH: 'x64',
    });
    expect(config).toMatchObject({
      appId: 'com.butterpaper.desktop.beta',
      productName: 'Butter Paper Beta',
      packageName: 'butter-paper-beta',
      desktopName: 'butter-paper-beta.desktop',
      channel: 'beta',
      feed: {
        provider: 'generic',
        url: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/beta/win32/x64',
        channel: 'latest',
      },
      tufRepository: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/beta/win32/x64/tuf',
      updateFeedUrl: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/beta/win32/x64',
      updateTargetName: 'latest.yml',
      windowsArtifact: 'Butter-Paper-Beta-Windows-${arch}-Setup.${ext}',
      compression: 'maximum',
      nsisInclude: 'build/installer.nsh',
      nsisOneClick: false,
      nsisAllowElevation: true,
      nsisAllowToChangeInstallationDirectory: true,
    });
    expect(config.files).toContain('!node_modules/@napi-rs/canvas-win32-arm64-msvc/**/*');
    expect(config.files).not.toContain('!node_modules/@napi-rs/canvas-win32-x64-msvc/**/*');
  });

  it('uses the standard Windows ARM64 NSIS payload without duplicating runtime files', () => {
    const config = loadConfig({
      BP_RELEASE_CHANNEL: 'stable',
      BP_RELEASE_PLATFORM: 'win32',
      BP_RELEASE_ARCH: 'arm64',
    });
    expect(config).toMatchObject({
      compression: 'maximum',
      nsisInclude: 'build/installer.nsh',
      windowsArtifact: 'Butter-Paper-Windows-${arch}-Setup.${ext}',
    });
    const builderConfig = readFileSync(resolve(desktopDir, 'electron-builder.config.cjs'), 'utf8');
    const installerInclude = readFileSync(resolve(desktopDir, 'build/installer.nsh'), 'utf8');
    expect(config.nsisUseZip).toBeUndefined();
    expect(builderConfig).not.toContain('BP_NSIS_ARM64_UNPACKED_DIR');
    expect(installerInclude).not.toContain('customFiles_arm64');
    expect(installerInclude).not.toContain('BP_NSIS_ARM64_UNPACKED_DIR');
  });

  it('configures only AppImage runtime updates for Linux packages', () => {
    const config = loadConfig({
      BP_RELEASE_CHANNEL: 'stable',
      BP_RELEASE_PLATFORM: 'linux',
      BP_RELEASE_ARCH: 'arm64',
    });
    expect(config).toMatchObject({
      feed: {
        provider: 'generic',
        url: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/stable/linux/arm64',
        channel: 'latest',
      },
      tufRepository: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/stable/linux/arm64/tuf',
      updateTargetName: 'latest-linux-arm64.yml',
    });
  });

  it('rejects an undeclared release channel', () => {
    const result = spawnSync(process.execPath, ['-e', "require('./electron-builder.config.cjs')"], {
      cwd: desktopDir,
      encoding: 'utf8',
      env: { ...process.env, BP_RELEASE_CHANNEL: 'nightly' },
    });
    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain('BP_RELEASE_CHANNEL must be stable or beta');
  });

  it('fails release packaging when the reviewed TUF root is absent', () => {
    const builderConfig = readFileSync(resolve(desktopDir, 'electron-builder.config.cjs'), 'utf8');
    expect(builderConfig).toContain("process.env.BP_REQUIRE_TUF_ROOT === '1'");
    expect(builderConfig).toContain('A reviewed Butter Paper TUF trust root is required');
  });
});
