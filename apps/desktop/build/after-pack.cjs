const { execFileSync } = require('node:child_process');
const { mkdirSync } = require('node:fs');
const path = require('node:path');
const { Arch } = require('builder-util');

module.exports = async function afterPack(context) {
  if (context.electronPlatformName !== 'darwin') {
    return;
  }

  const architecture = Arch[context.arch];
  const clangArchitecture = architecture === 'x64' ? 'x86_64' : architecture;
  if (!['arm64', 'x86_64'].includes(clangArchitecture)) {
    throw new Error(`Unsupported macOS helper architecture: ${architecture}`);
  }

  const appName = `${context.packager.appInfo.productFilename}.app`;
  const resourcesDirectory = path.join(context.appOutDir, appName, 'Contents', 'Resources');
  const helperDirectory = path.join(resourcesDirectory, 'bin');
  const helperPath = path.join(helperDirectory, 'set-default-pdf-app');
  const sourcePath = path.join(__dirname, 'macos', 'set-default-pdf-app.m');

  mkdirSync(helperDirectory, { recursive: true });
  execFileSync('/usr/bin/xcrun', [
    'clang',
    '-fobjc-arc',
    '-fblocks',
    '-framework',
    'AppKit',
    '-framework',
    'UniformTypeIdentifiers',
    '-mmacosx-version-min=12.0',
    '-arch',
    clangArchitecture,
    sourcePath,
    '-o',
    helperPath,
  ], { stdio: 'inherit' });
};
