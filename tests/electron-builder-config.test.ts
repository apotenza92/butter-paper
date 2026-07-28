import { spawnSync } from 'node:child_process';
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
    macArtifact: config.mac.artifactName,
    macFileAssociations: config.mac.fileAssociations,
    windowsArtifact: config.win.artifactName,
    windowsFileAssociations: config.win.fileAssociations ?? null,
    compression: config.compression,
    electronLanguages: config.electronLanguages,
    releaseNotes: config.releaseInfo?.releaseNotes,
    nsisInclude: config.nsis.include,
    linuxArtifact: config.linux.artifactName,
    linuxFileAssociations: config.linux.fileAssociations,
    linuxSyncDesktopName: config.linux.syncDesktopName,
    afterPack: config.afterPack,
    files: config.files,
    arm64UnpackedDir: process.env.BP_NSIS_ARM64_UNPACKED_DIR ?? null,
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
      compression: 'maximum',
      electronLanguages: ['en-US'],
      releaseNotes: expect.stringContaining('Added native PDF file registration'),
      nsisInclude: 'build/installer.nsh',
    });
    expect(config.files).toContain('!**/*.map');
    expect(config.files).toContain('!node_modules/@base-ui/**/*');
    expect(config.files).toContain('!node_modules/react/**/*');
    expect(config.files).toContain('!node_modules/lucide-react/**/*');
    expect(config.files).toContain('!node_modules/@napi-rs/canvas-darwin-x64/**/*');
    expect(config.files).not.toContain('!node_modules/@napi-rs/canvas-darwin-arm64/**/*');
  });

  it('keeps the beta Windows identity separate without claiming unsupported updater feeds', () => {
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
      feed: null,
      windowsArtifact: 'Butter-Paper-Beta-Windows-${arch}-Setup.${ext}',
      compression: 'maximum',
      nsisInclude: 'build/installer.nsh',
    });
    expect(config.files).toContain('!node_modules/@napi-rs/canvas-win32-arm64-msvc/**/*');
    expect(config.files).not.toContain('!node_modules/@napi-rs/canvas-win32-x64-msvc/**/*');
  });

  it('uses the standard Windows ARM64 NSIS payload with the executable restore include', () => {
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
    expect(config.arm64UnpackedDir).toMatch(/release[/\\]win-arm64-unpacked$/);
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
});
