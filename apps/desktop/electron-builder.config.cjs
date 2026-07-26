const releaseChannel = process.env.BP_RELEASE_CHANNEL || 'stable';
const releasePlatform = process.env.BP_RELEASE_PLATFORM || process.platform;
const releaseArch = process.env.BP_RELEASE_ARCH || process.arch;
const updateFeedBaseUrl = (process.env.BP_UPDATE_FEED_BASE_URL
  || 'https://raw.githubusercontent.com/apotenza92/butter-paper/updates').replace(/\/$/, '');

if (!['stable', 'beta'].includes(releaseChannel)) {
  throw new Error(`BP_RELEASE_CHANNEL must be stable or beta, received: ${releaseChannel}`);
}
if (!['darwin', 'win32', 'linux'].includes(releasePlatform)) {
  throw new Error(`BP_RELEASE_PLATFORM must be darwin, win32, or linux, received: ${releasePlatform}`);
}
if (!['arm64', 'x64'].includes(releaseArch)) {
  throw new Error(`BP_RELEASE_ARCH must be arm64 or x64, received: ${releaseArch}`);
}

const isBeta = releaseChannel === 'beta';
const productName = isBeta ? 'Butter Paper Beta' : 'Butter Paper';
const packageName = isBeta ? 'butter-paper-beta' : 'butter-paper';
const artifactPrefix = isBeta ? 'Butter-Paper-Beta' : 'Butter-Paper';
const updateFeedUrl = `${updateFeedBaseUrl}/${releaseChannel}/${releasePlatform}/${releaseArch}`;

const hasAppleNotarizationCredentials = Boolean(
  process.env.APPLE_API_KEY
  && process.env.APPLE_API_KEY_ID
  && process.env.APPLE_API_ISSUER,
);
const hasMacSigningCredentials = Boolean(process.env.CSC_KEYCHAIN);
const canvasTargetByPlatform = {
  darwin: { arm64: 'darwin-arm64', x64: 'darwin-x64' },
  win32: { arm64: 'win32-arm64-msvc', x64: 'win32-x64-msvc' },
  linux: { arm64: 'linux-arm64-gnu', x64: 'linux-x64-gnu' },
};
const targetCanvasSuffix = canvasTargetByPlatform[releasePlatform][releaseArch];
const canvasPlatformSuffixes = [
  'android-arm64',
  'darwin-arm64',
  'darwin-x64',
  'linux-arm-gnueabihf',
  'linux-arm64-gnu',
  'linux-arm64-musl',
  'linux-riscv64-gnu',
  'linux-x64-gnu',
  'linux-x64-musl',
  'win32-arm64-msvc',
  'win32-x64-msvc',
];

module.exports = {
  appId: isBeta ? 'com.butterpaper.desktop.beta' : 'com.butterpaper.desktop',
  productName,
  asar: true,
  compression: 'maximum',
  extraMetadata: {
    name: packageName,
    productName,
    butterPaperChannel: releaseChannel,
  },
  directories: {
    output: process.env.BP_RELEASE_OUTPUT_DIR || 'release',
    buildResources: 'assets',
  },
  files: [
    '.vite/**/*',
    'package.json',
    ...canvasPlatformSuffixes
      .filter((suffix) => suffix !== targetCanvasSuffix)
      .map((suffix) => `!node_modules/@napi-rs/canvas-${suffix}/**/*`),
  ],
  asarUnpack: [
    '**/*.node',
  ],
  mac: {
    category: 'public.app-category.productivity',
    icon: 'assets/icon.icns',
    hardenedRuntime: true,
    gatekeeperAssess: false,
    entitlements: 'build/entitlements.mac.plist',
    entitlementsInherit: 'build/entitlements.mac.plist',
    notarize: hasAppleNotarizationCredentials,
    target: [
      { target: 'dmg', arch: ['arm64', 'x64'] },
      { target: 'zip', arch: ['arm64', 'x64'] },
    ],
    artifactName: `${artifactPrefix}-macOS-\${arch}.\${ext}`,
  },
  dmg: {
    sign: hasMacSigningCredentials,
  },
  win: {
    icon: 'assets/icon.ico',
    target: [
      { target: 'nsis', arch: ['arm64', 'x64'] },
    ],
    artifactName: `${artifactPrefix}-Windows-\${arch}-Setup.\${ext}`,
  },
  nsis: {
    oneClick: false,
    perMachine: false,
    allowElevation: true,
    allowToChangeInstallationDirectory: true,
    deleteAppDataOnUninstall: false,
    installerIcon: 'assets/icon.ico',
    uninstallerIcon: 'assets/icon.ico',
  },
  linux: {
    icon: 'assets/linux',
    category: 'Office',
    maintainer: 'Alex Potenza <apotenza92@users.noreply.github.com>',
    executableName: packageName,
    synopsis: 'Cross-platform PDF review and markup',
    description: 'Cross-platform PDF review and markup for architecture, engineering, and construction.',
    target: [
      { target: 'AppImage', arch: ['arm64', 'x64'] },
      { target: 'deb', arch: ['arm64', 'x64'] },
      { target: 'rpm', arch: ['arm64', 'x64'] },
    ],
    artifactName: `${artifactPrefix}-Linux-${releaseArch}.\${ext}`,
  },
  ...(releasePlatform === 'darwin' ? {
    publish: [{
      provider: 'generic',
      url: updateFeedUrl,
      channel: 'latest',
    }],
  } : {}),
};
