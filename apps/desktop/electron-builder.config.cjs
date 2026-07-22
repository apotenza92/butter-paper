const hasAppleNotarizationCredentials = Boolean(
  process.env.APPLE_ID
  && process.env.APPLE_APP_SPECIFIC_PASSWORD
  && process.env.APPLE_TEAM_ID,
);

module.exports = {
  appId: 'com.butterpaper.desktop',
  productName: 'Butter Paper',
  asar: true,
  compression: 'maximum',
  extraMetadata: {
    name: 'butter-paper',
  },
  directories: {
    output: 'release',
    buildResources: 'assets',
  },
  files: [
    '.vite/**/*',
    'package.json',
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
    artifactName: 'Butter-Paper-macOS-${arch}.${ext}',
  },
  dmg: {
    sign: false,
  },
  win: {
    icon: 'assets/icon.ico',
    target: [
      { target: 'nsis', arch: ['arm64', 'x64'] },
    ],
    artifactName: 'Butter-Paper-Windows-${arch}-Setup.${ext}',
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
    executableName: 'butter-paper',
    synopsis: 'Cross-platform PDF review and markup',
    description: 'Cross-platform PDF review and markup for architecture, engineering, and construction.',
    target: [
      { target: 'AppImage', arch: ['arm64', 'x64'] },
      { target: 'deb', arch: ['arm64', 'x64'] },
      { target: 'rpm', arch: ['arm64', 'x64'] },
    ],
    artifactName: 'Butter-Paper-Linux-${arch}.${ext}',
  },
  publish: null,
};
