import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

const desktopDir = resolve(import.meta.dirname, '../apps/desktop');
const inspectScript = `
  const config = require('./electron-builder.config.cjs');
  process.stdout.write(JSON.stringify({
    appId: config.appId,
    productName: config.productName,
    packageName: config.extraMetadata.name,
    channel: config.extraMetadata.butterPaperChannel,
    output: config.directories.output,
    feed: config.publish?.[0] ?? null,
    macArtifact: config.mac.artifactName,
    windowsArtifact: config.win.artifactName,
    linuxArtifact: config.linux.artifactName,
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
      channel: 'stable',
      output: 'release/stable/darwin/arm64',
      feed: {
        provider: 'generic',
        url: 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates/stable/darwin/arm64',
        channel: 'latest',
      },
      macArtifact: 'Butter-Paper-macOS-${arch}.${ext}',
      windowsArtifact: 'Butter-Paper-Windows-${arch}-Setup.${ext}',
      linuxArtifact: 'Butter-Paper-Linux-${arch}.${ext}',
    });
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
      channel: 'beta',
      feed: null,
      windowsArtifact: 'Butter-Paper-Beta-Windows-${arch}-Setup.${ext}',
    });
    expect(config.files).toContain('!node_modules/@napi-rs/canvas-win32-arm64-msvc/**/*');
    expect(config.files).not.toContain('!node_modules/@napi-rs/canvas-win32-x64-msvc/**/*');
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
