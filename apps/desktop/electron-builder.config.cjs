const path = require('node:path');
const { existsSync, readFileSync } = require('node:fs');
const { extractReleaseNotes } = require('../../scripts/release-notes.cjs');
const desktopPackage = require('./package.json');

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
const iconAssetsDirectory = isBeta ? 'assets/beta' : 'assets';
const desktopFileName = `${packageName}.desktop`;
const macIcon = isBeta
  ? 'assets/beta/macos/Butter Paper Beta.icon'
  : 'assets/macos/Butter Paper.icon';
const updateFeedUrl = `${updateFeedBaseUrl}/${releaseChannel}/${releasePlatform}/${releaseArch}`;
const releaseNotes = extractReleaseNotes(
  readFileSync(path.resolve(__dirname, '../../CHANGELOG.md'), 'utf8'),
  desktopPackage.version,
);
const tufRootPath = path.join(__dirname, 'build', 'update-trust', 'root.json');
if (process.env.BP_REQUIRE_TUF_ROOT === '1' && !existsSync(tufRootPath)) {
  throw new Error(`A reviewed Butter Paper TUF trust root is required at ${tufRootPath}.`);
}
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
const pdfSignatureCoreTarget = `${releasePlatform}-${releaseArch}`;
const pdfSignatureCorePackagePath = path.resolve(
  __dirname,
  '../../native/pdf-signature-core/build/package',
  pdfSignatureCoreTarget,
);
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
const rendererBundledPackages = [
  '@babel',
  '@base-ui',
  '@floating-ui',
  '@fontsource',
  'class-variance-authority',
  'clsx',
  'lucide-react',
  'react',
  'react-dom',
  'reselect',
  'scheduler',
  'tailwind-merge',
  'tw-animate-css',
  'use-sync-external-store',
  'zustand',
];

module.exports = {
  appId: isBeta ? 'com.butterpaper.desktop.beta' : 'com.butterpaper.desktop',
  productName,
  asar: true,
  compression: 'maximum',
  electronLanguages: ['en-US'],
  releaseInfo: {
    releaseNotes,
  },
  extraMetadata: {
    name: packageName,
    productName,
    desktopName: desktopFileName,
    butterPaperChannel: releaseChannel,
    butterPaperTufRepositoryUrl: `${updateFeedUrl}/tuf`,
    butterPaperUpdateFeedUrl: updateFeedUrl,
    butterPaperUpdateTargetName: metadataFileName(releasePlatform, releaseArch),
  },
  afterPack: 'build/after-pack.cjs',
  directories: {
    output: process.env.BP_RELEASE_OUTPUT_DIR || 'release',
    buildResources: 'assets',
  },
  files: [
    '.vite/**/*',
    'package.json',
    '!**/*.map',
    ...rendererBundledPackages.map((packageName) => `!node_modules/${packageName}/**/*`),
    ...canvasPlatformSuffixes
      .filter((suffix) => suffix !== targetCanvasSuffix)
      .map((suffix) => `!node_modules/@napi-rs/canvas-${suffix}/**/*`),
  ],
  asarUnpack: [
    '**/*.node',
  ],
  extraResources: [
    {
      // The main-process resolver uses resources/pdf-signature-core/<target>.
      // Keep this required so a release build cannot silently ship without the
      // verified sidecar package; the package build lane must run first.
      from: pdfSignatureCorePackagePath,
      to: `pdf-signature-core/${pdfSignatureCoreTarget}`,
    },
    ...(existsSync(tufRootPath) ? [{
      from: tufRootPath,
      to: 'update-trust/root.json',
    }] : []),
  ],
  mac: {
    category: 'public.app-category.productivity',
    icon: macIcon,
    fileAssociations: [{
      ext: 'pdf',
      name: 'PDF document',
      role: 'Viewer',
      rank: 'Alternate',
    }],
    hardenedRuntime: true,
    minimumSystemVersion: '12.0',
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
    icon: `${iconAssetsDirectory}/icon.icns`,
    sign: hasMacSigningCredentials,
  },
  win: {
    icon: `${iconAssetsDirectory}/icon.ico`,
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
    include: 'build/installer.nsh',
    deleteAppDataOnUninstall: false,
    installerIcon: `${iconAssetsDirectory}/icon.ico`,
    uninstallerIcon: `${iconAssetsDirectory}/icon.ico`,
  },
  linux: {
    icon: `${iconAssetsDirectory}/linux`,
    category: 'Office',
    syncDesktopName: true,
    maintainer: 'Alex Potenza <apotenza92@users.noreply.github.com>',
    executableName: packageName,
    fileAssociations: [{
      ext: 'pdf',
      name: 'PDF document',
      mimeType: 'application/pdf',
    }],
    synopsis: 'Cross-platform PDF review and markup',
    description: 'Cross-platform PDF review and markup for architecture, engineering, and construction.',
    target: [
      { target: 'AppImage', arch: ['arm64', 'x64'] },
      { target: 'deb', arch: ['arm64', 'x64'] },
      { target: 'rpm', arch: ['arm64', 'x64'] },
    ],
    artifactName: `${artifactPrefix}-Linux-${releaseArch}.\${ext}`,
  },
  publish: [{
    provider: 'generic',
    url: updateFeedUrl,
    channel: 'latest',
  }],
};

function metadataFileName(platform, arch) {
  if (platform === 'darwin') return 'latest-mac.yml';
  if (platform === 'win32') return 'latest.yml';
  return arch === 'arm64' ? 'latest-linux-arm64.yml' : 'latest-linux.yml';
}
