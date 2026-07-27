import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  closeSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readFileSync,
  readSync,
  readdirSync,
  readlinkSync,
  realpathSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { createRequire } from 'node:module';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';
import { gunzipSync } from 'node:zlib';
import { validateUpdateMetadataFile } from './update-metadata-contract.mjs';

export const BUTTER_PAPER_APPLE_TEAM_ID = '27JL2VERNC';
export const BUTTER_PAPER_SIGNING_IDENTITY = `Developer ID Application: Alexander Potenza (${BUTTER_PAPER_APPLE_TEAM_ID})`;
export const BUTTER_PAPER_SIGNING_CERTIFICATE_SHA256 = 'C20E3A100252224861FF8474DEBB21E5A120210E7CD61905EFDA0B6464E18594';
const smokeEnvironmentNames = [
  'CI',
  'GITHUB_ACTIONS',
  'HOME',
  'LANG',
  'LC_ALL',
  'LOGNAME',
  'PATH',
  'PWD',
  'RUNNER_TEMP',
  'SHELL',
  'TEMP',
  'TMP',
  'TMPDIR',
  'USER',
  'XDG_CACHE_HOME',
];

export function createSmokeEnvironment(source, overrides = {}) {
  const environment = {};
  for (const name of smokeEnvironmentNames) {
    if (typeof source[name] === 'string' && source[name] !== '') {
      environment[name] = source[name];
    }
  }
  return { ...environment, ...overrides };
}
const allowedEntitlements = new Set([
  'com.apple.security.cs.allow-jit',
  'com.apple.security.cs.allow-unsigned-executable-memory',
]);
const machOMagic = new Set([
  'feedface',
  'feedfacf',
  'cefaedfe',
  'cffaedfe',
  'cafebabe',
  'bebafeca',
  'cafebabf',
  'bfbafeca',
]);
const signedBundleExtensions = ['.app', '.framework', '.xpc', '.appex', '.bundle'];

const repoRoot = resolve(import.meta.dirname, '..');
const require = createRequire(import.meta.url);
const { extractFile: extractAsarFile } = require('@electron/asar');
const YAML = require('yaml');

export function resolveReleaseContract(channel, arch) {
  if (!['stable', 'beta'].includes(channel)) {
    fail(`Expected release channel stable or beta, received: ${channel}`);
  }
  if (!['arm64', 'x64'].includes(arch)) {
    fail(`Expected release architecture arm64 or x64, received: ${arch}`);
  }
  const beta = channel === 'beta';
  const productName = beta ? 'Butter Paper Beta' : 'Butter Paper';
  return {
    appName: `${productName}.app`,
    arch,
    artifactPrefix: beta ? 'Butter-Paper-Beta-macOS' : 'Butter-Paper-macOS',
    bundleId: beta ? 'com.butterpaper.desktop.beta' : 'com.butterpaper.desktop',
    channel,
    executableName: productName,
    packageName: beta ? 'butter-paper-beta' : 'butter-paper',
    productName,
  };
}

export function resolveConfiguredReleaseContract(channel, arch, outputDirectory) {
  resolveReleaseContract(channel, arch);
  const configPath = join(repoRoot, 'apps/desktop/electron-builder.config.cjs');
  const previousEnvironment = Object.fromEntries([
    'BP_RELEASE_ARCH',
    'BP_RELEASE_CHANNEL',
    'BP_RELEASE_OUTPUT_DIR',
    'BP_RELEASE_PLATFORM',
  ].map((name) => [name, process.env[name]]));
  try {
    process.env.BP_RELEASE_ARCH = arch;
    process.env.BP_RELEASE_CHANNEL = channel;
    process.env.BP_RELEASE_OUTPUT_DIR = outputDirectory;
    process.env.BP_RELEASE_PLATFORM = 'darwin';
    delete require.cache[require.resolve(configPath)];
    const config = require(configPath);
    const artifactSuffix = '-${arch}.${ext}';
    if (
      typeof config.appId !== 'string'
      || typeof config.productName !== 'string'
      || typeof config.extraMetadata?.name !== 'string'
      || config.extraMetadata?.butterPaperChannel !== channel
      || typeof config.mac?.icon !== 'string'
      || typeof config.mac?.artifactName !== 'string'
      || !config.mac.artifactName.endsWith(artifactSuffix)
      || config.publish?.[0]?.provider !== 'generic'
      || config.publish?.[0]?.channel !== 'latest'
      || typeof config.publish?.[0]?.url !== 'string'
    ) {
      fail('Electron Builder macOS release identity or artifact template is invalid');
    }
    return {
      appName: `${config.productName}.app`,
      arch,
      artifactPrefix: config.mac.artifactName.slice(0, -artifactSuffix.length),
      bundleId: config.appId,
      channel,
      executableName: config.productName,
      iconName: 'Icon',
      iconSourcePath: resolve(dirname(configPath), config.mac.icon),
      legacyIconSourcePath: resolve(dirname(configPath), config.dmg.icon),
      packageName: config.extraMetadata.name,
      productName: config.productName,
      updateFeedUrl: config.publish[0].url,
    };
  } finally {
    delete require.cache[require.resolve(configPath)];
    for (const [name, value] of Object.entries(previousEnvironment)) {
      if (value === undefined) {
        delete process.env[name];
      } else {
        process.env[name] = value;
      }
    }
  }
}

function fail(message) {
  throw new Error(message);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    encoding: 'utf8',
    env: options.env ?? process.env,
    input: options.input,
    maxBuffer: 64 * 1024 * 1024,
  });
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0 && !options.allowFailure) {
    fail(`${command} ${args.join(' ')} failed (${result.status}):\n${output.trim()}`);
  }
  return { ...result, output };
}

export function normalizeFingerprint(value) {
  const normalized = String(value ?? '').replace(/[^a-fA-F0-9]/g, '').toUpperCase();
  if (!/^[A-F0-9]{64}$/.test(normalized)) {
    fail(`Expected a SHA-256 certificate fingerprint, received: ${value}`);
  }
  return normalized;
}

export function resolvePriorSigningFingerprints(currentValue, priorValue) {
  const current = normalizeFingerprint(currentValue);
  const prior = normalizeFingerprint(priorValue || current);
  return [...new Set([current, prior])];
}

export function parseCodesignMetadata(output) {
  const values = new Map();
  const authorities = [];
  for (const line of String(output).split(/\r?\n/)) {
    if (line.startsWith('CodeDirectory ')) {
      values.set('CodeDirectory', line);
      continue;
    }
    const separatorIndex = line.indexOf('=');
    if (separatorIndex < 1) {
      continue;
    }
    const key = line.slice(0, separatorIndex).trim();
    const value = line.slice(separatorIndex + 1).trim();
    if (key === 'Authority') {
      authorities.push(value);
    } else if (!values.has(key)) {
      values.set(key, value);
    }
  }
  return {
    authorities,
    cdHash: values.get('CDHash') ?? null,
    flags: values.get('CodeDirectory') ?? '',
    identifier: values.get('Identifier') ?? null,
    teamIdentifier: values.get('TeamIdentifier') ?? null,
    timestamp: values.get('Timestamp') ?? null,
    ticket: values.get('Notarization Ticket') ?? null,
  };
}

export function validateSignatureMetadata(metadata, expectations, label) {
  if (metadata.authorities[0] !== expectations.identity) {
    fail(`${label} was signed by ${metadata.authorities[0] ?? 'no authority'}, expected ${expectations.identity}`);
  }
  if (metadata.teamIdentifier !== expectations.teamId) {
    fail(`${label} team ${metadata.teamIdentifier ?? 'missing'} does not match ${expectations.teamId}`);
  }
  if (!metadata.flags.includes('runtime')) {
    fail(`${label} does not have the hardened-runtime signature flag`);
  }
  if (!metadata.timestamp) {
    fail(`${label} does not have a secure signing timestamp`);
  }
  if (!metadata.cdHash) {
    fail(`${label} does not expose a CDHash`);
  }
}

export function validateExactArchitecture(output, expectedArch, label) {
  const expectedLipoArch = expectedArch === 'x64' ? 'x86_64' : 'arm64';
  const architectures = String(output).trim().split(/\s+/).filter(Boolean);
  if (architectures.length !== 1 || architectures[0] !== expectedLipoArch) {
    fail(`${label} architectures ${architectures.join(', ') || 'missing'} do not exactly match ${expectedLipoArch}`);
  }
  return expectedLipoArch;
}

export function validateEntitlements(entitlements, label) {
  for (const key of Object.keys(entitlements)) {
    if (key === 'com.apple.security.get-task-allow') {
      fail(`${label} includes the forbidden com.apple.security.get-task-allow entitlement`);
    }
    if (!allowedEntitlements.has(key)) {
      fail(`${label} includes unexpected entitlement ${key}`);
    }
    if (entitlements[key] !== true) {
      fail(`${label} entitlement ${key} must be true when present`);
    }
  }
}

export function validateZipEntries(entries, contract = resolveReleaseContract('stable', 'arm64')) {
  const { appName, executableName } = contract;
  const names = String(entries).split(/\r?\n/).filter(Boolean);
  if (names.length === 0) {
    fail('ZIP contains no entries');
  }
  const seen = new Set();
  for (const name of names) {
    if (seen.has(name)) {
      fail(`ZIP contains duplicate entry ${name}`);
    }
    seen.add(name);
    if (name.includes('\\') || name.includes('\0') || name.startsWith('/')) {
      fail(`ZIP contains unsafe entry ${name}`);
    }
    const segments = name.split('/').filter(Boolean);
    if (segments.some((segment) => segment === '.' || segment === '..')) {
      fail(`ZIP contains unsafe traversal entry ${name}`);
    }
    if (segments[0] !== appName) {
      fail(`ZIP contains unexpected top-level entry ${name}`);
    }
  }
  const executable = `${appName}/Contents/MacOS/${executableName}`;
  if (!seen.has(executable)) {
    fail(`ZIP is missing ${executable}`);
  }
  return names;
}

export function validateBlockmap(blockmap, artifactSize, label) {
  if (!blockmap || blockmap.version !== '2' || !Array.isArray(blockmap.files) || blockmap.files.length === 0) {
    fail(`${label} is not a non-empty blockmap v2 document`);
  }
  let representedBytes = 0;
  for (const file of blockmap.files) {
    if (!file || typeof file.name !== 'string' || !Array.isArray(file.sizes) || !Array.isArray(file.checksums)) {
      fail(`${label} contains an invalid file entry`);
    }
    if (file.sizes.length === 0 || file.sizes.length !== file.checksums.length) {
      fail(`${label} block sizes and checksums do not align`);
    }
    for (let index = 0; index < file.sizes.length; index += 1) {
      const size = file.sizes[index];
      const checksum = file.checksums[index];
      if (!Number.isInteger(size) || size <= 0 || typeof checksum !== 'string' || checksum.length === 0) {
        fail(`${label} contains an invalid block at index ${index}`);
      }
      representedBytes += size;
    }
  }
  if (representedBytes !== artifactSize) {
    fail(`${label} represents ${representedBytes} bytes, expected ${artifactSize}`);
  }
}

export function validateChecksumText(text, artifactName, actualHash) {
  const match = String(text).trim().match(/^([a-fA-F0-9]{64})  ([^/\\]+)$/);
  if (!match || match[2] !== artifactName) {
    fail(`Checksum file for ${artifactName} has invalid content`);
  }
  if (match[1].toLowerCase() !== actualHash.toLowerCase()) {
    fail(`Checksum for ${artifactName} does not match the artifact`);
  }
}

function hashFile(filePath) {
  return createHash('sha256').update(readFileSync(filePath)).digest('hex');
}

function isMachO(filePath) {
  if (!lstatSync(filePath).isFile()) {
    return false;
  }
  const descriptor = openSync(filePath, 'r');
  try {
    const header = Buffer.alloc(4);
    return readSync(descriptor, header, 0, 4, 0) === 4 && machOMagic.has(header.toString('hex'));
  } finally {
    closeSync(descriptor);
  }
}

function collectTree(rootPath) {
  const files = [];
  const bundles = [];
  function visit(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const filePath = join(directory, entry.name);
      if (entry.isSymbolicLink()) {
        continue;
      }
      if (entry.isDirectory()) {
        if (signedBundleExtensions.some((extension) => entry.name.endsWith(extension))) {
          bundles.push(filePath);
        }
        visit(filePath);
      } else if (entry.isFile()) {
        files.push(filePath);
      }
    }
  }
  visit(rootPath);
  return { files, bundles };
}

export function createAppManifest(appPath) {
  const manifest = [];
  function visit(directory) {
    const entries = readdirSync(directory, { withFileTypes: true })
      .sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const filePath = join(directory, entry.name);
      const entryPath = relative(appPath, filePath).split(sep).join('/');
      if (entry.isSymbolicLink()) {
        const target = readlinkSync(filePath);
        if (isAbsolute(target)) {
          fail(`App contains absolute symlink ${entryPath} -> ${target}`);
        }
        const resolvedTarget = resolve(dirname(filePath), target);
        const relativeTarget = relative(appPath, resolvedTarget);
        if (relativeTarget === '..' || relativeTarget.startsWith(`..${sep}`)) {
          fail(`App contains escaping symlink ${entryPath} -> ${target}`);
        }
        manifest.push({ path: entryPath, type: 'symlink', target });
      } else if (entry.isDirectory()) {
        manifest.push({ path: entryPath, type: 'directory' });
        visit(filePath);
      } else if (entry.isFile()) {
        manifest.push({ path: entryPath, type: 'file', sha256: hashFile(filePath), size: statSync(filePath).size });
      } else {
        fail(`App contains unsupported filesystem entry ${entryPath}`);
      }
    }
  }
  visit(appPath);
  return manifest;
}

function parseEntitlements(targetPath) {
  const result = run('codesign', ['-d', '--xml', '--entitlements', '-', targetPath]);
  const xmlStart = result.stdout.indexOf('<?xml');
  if (xmlStart < 0) {
    return {};
  }
  const xml = result.stdout.slice(xmlStart);
  const converted = run('plutil', ['-convert', 'json', '-o', '-', '--', '-'], { input: xml });
  return JSON.parse(converted.stdout);
}

function extractAndValidateCertificate(targetPath, certificateDir, prefix, expectations, validateChain) {
  const certificatePrefix = join(certificateDir, prefix);
  run('codesign', ['-d', `--extract-certificates=${certificatePrefix}`, targetPath]);
  const leafPath = `${certificatePrefix}0`;
  if (!existsSync(leafPath)) {
    fail(`codesign did not extract a leaf certificate for ${targetPath}`);
  }
  const fingerprint = normalizeFingerprint(hashFile(leafPath));
  const allowedFingerprints = expectations.allowedFingerprints ?? [expectations.fingerprint];
  if (!allowedFingerprints.includes(fingerprint)) {
    fail(`${targetPath} leaf certificate ${fingerprint} is not one of the explicitly trusted fingerprints`);
  }
  if (validateChain) {
    const intermediatePath = `${certificatePrefix}1`;
    const rootPath = `${certificatePrefix}2`;
    if (!existsSync(intermediatePath) || !existsSync(rootPath)) {
      fail(`${targetPath} does not embed a complete leaf/intermediate/root certificate chain`);
    }
    run('security', [
      'verify-cert',
      '-N',
      '-L',
      '-p', 'codeSign',
      '-c', leafPath,
      '-c', intermediatePath,
      '-r', rootPath,
    ]);
  }
}

export function verifyTrustedMacApp(appPath, {
  arch,
  bundleId,
  certificateDirectory,
  fingerprint = BUTTER_PAPER_SIGNING_CERTIFICATE_SHA256,
  fingerprints,
  identity = BUTTER_PAPER_SIGNING_IDENTITY,
  teamId = BUTTER_PAPER_APPLE_TEAM_ID,
}) {
  const resolvedAppPath = realpathSync(appPath);
  const infoPlistPath = join(resolvedAppPath, 'Contents', 'Info.plist');
  const actualBundleId = readPlistValue(infoPlistPath, 'CFBundleIdentifier');
  if (actualBundleId !== bundleId) {
    fail(`Expected bundle ID ${bundleId}, received ${actualBundleId}`);
  }
  run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', resolvedAppPath]);
  const allowedFingerprints = fingerprints == null
    ? [normalizeFingerprint(fingerprint)]
    : [...new Set(fingerprints.map(normalizeFingerprint))];
  if (allowedFingerprints.length === 0) {
    fail('At least one trusted SHA-256 certificate fingerprint is required');
  }
  const expectations = {
    fingerprint: allowedFingerprints[0],
    allowedFingerprints,
    identity,
    teamId,
  };
  const metadata = parseCodesignMetadata(run('codesign', ['-dvvv', resolvedAppPath]).output);
  validateSignatureMetadata(metadata, expectations, resolvedAppPath);
  extractAndValidateCertificate(
    resolvedAppPath,
    certificateDirectory,
    'trusted-app-certificate-',
    expectations,
    true,
  );
  const executableName = readPlistValue(infoPlistPath, 'CFBundleExecutable');
  const executablePath = join(resolvedAppPath, 'Contents', 'MacOS', executableName);
  validateExactArchitecture(run('lipo', ['-archs', executablePath]).stdout, arch, executablePath);
  run('xcrun', ['stapler', 'validate', resolvedAppPath]);
  run('spctl', ['--assess', '--type', 'execute', '--verbose=4', resolvedAppPath]);
  return { executablePath, metadata };
}

function validateCodeObject(targetPath, context, options = {}) {
  const label = `${context.containerLabel}:${relative(context.appPath, targetPath) || '.'}`;
  run('codesign', ['--verify', '--strict', '--verbose=2', targetPath]);
  const metadata = parseCodesignMetadata(run('codesign', ['-dvvv', targetPath]).output);
  validateSignatureMetadata(metadata, context.expectations, label);
  validateEntitlements(parseEntitlements(targetPath), label);
  extractAndValidateCertificate(
    targetPath,
    context.certificateDir,
    `${context.containerLabel.toLowerCase()}-certificate-${context.certificateIndex++}-`,
    context.expectations,
    options.validateChain === true,
  );
  if (options.machO) {
    validateExactArchitecture(run('lipo', ['-archs', targetPath]).stdout, context.expectedArch, label);
  }
  return metadata;
}

function readPlistValue(plistPath, key) {
  return run('plutil', ['-extract', key, 'raw', '-o', '-', plistPath]).stdout.trim();
}

export function validateIconStackSystemBackground(stack, expectedBackground, label) {
  const explicitSystemBackgrounds = (stack.Layers ?? []).filter((layer) => (
    typeof layer.Name === 'string'
    && layer.Name.startsWith('Icon_Assets/system-')
  ));
  if (
    explicitSystemBackgrounds.length > 0
    && !explicitSystemBackgrounds.some((layer) => layer.Name === expectedBackground)
  ) {
    fail(`${label} is missing ${expectedBackground}`);
  }
}

export function validateIconGroupCanvas(iconGroup, label) {
  if (!iconGroup?.Layers?.length) {
    return false;
  }
  for (const layer of iconGroup.Layers) {
    if (layer.LayerPosition !== undefined && layer.LayerPosition !== '0,0') {
      fail(`${label} artwork is offset from the icon origin`);
    }
    if (
      layer.LayerSize !== undefined
      && layer.LayerSize !== '0,0'
      && layer.LayerSize !== '1024,1024'
    ) {
      fail(`${label} artwork does not fill the native icon canvas`);
    }
  }
  return true;
}

function validateIconAssetCatalog(assetCatalogPath, iconName, label) {
  const catalog = JSON.parse(run('xcrun', ['assetutil', '--info', assetCatalogPath]).stdout);
  const findRendition = (assetType, appearance) => catalog.find((entry) => (
    entry.AssetType === assetType
    && entry.Name === iconName
    && (appearance === undefined || entry.Appearance === appearance)
  ));
  const lightStack = findRendition('IconImageStack', 'NSAppearanceNameAqua');
  const darkStack = findRendition('IconImageStack', 'NSAppearanceNameDarkAqua');
  if (!lightStack) {
    fail(`${label} Icon Composer catalog is missing its light appearance`);
  }
  if (!darkStack) {
    fail(`${label} Icon Composer catalog is missing its dark appearance`);
  }
  const expectedSystemBackgrounds = [
    ['light', 'NSAppearanceNameAqua', lightStack, 'Icon_Assets/system-light', 'Icon_Assets/01-artwork'],
    ['dark', 'NSAppearanceNameDarkAqua', darkStack, 'Icon_Assets/system-dark', 'Icon_Assets/01-artwork-dark'],
  ];
  for (const [
    appearance,
    appearanceName,
    stack,
    expectedBackground,
    expectedArtwork,
  ] of expectedSystemBackgrounds) {
    validateIconStackSystemBackground(
      stack,
      expectedBackground,
      `${label} Icon Composer ${appearance} appearance`,
    );
    const iconGroup = findRendition('IconGroup', appearanceName);
    const exposesArtworkLayers = validateIconGroupCanvas(
      iconGroup,
      `${label} Icon Composer ${appearance}`,
    );
    if (
      exposesArtworkLayers
      && !iconGroup.Layers.some((layer) => layer.Name === expectedArtwork)
    ) {
      fail(`${label} Icon Composer ${appearance} appearance is missing ${expectedArtwork}`);
    }
  }
  if (!findRendition('MultiSized Image')) {
    fail(`${label} Icon Composer catalog is missing its legacy fallback`);
  }
}

function verifyApp(appPath, context) {
  const resolvedAppPath = realpathSync(appPath);
  const executablePath = join(resolvedAppPath, 'Contents', 'MacOS', context.contract.executableName);
  const infoPlistPath = join(resolvedAppPath, 'Contents', 'Info.plist');
  if (!existsSync(executablePath) || !existsSync(infoPlistPath)) {
    fail(`${context.containerLabel} does not contain a complete Butter Paper app`);
  }
  const plistExpectations = {
    CFBundleIdentifier: context.contract.bundleId,
    CFBundleShortVersionString: context.version,
    CFBundleVersion: context.version,
    CFBundleExecutable: context.contract.executableName,
  };
  for (const [key, expected] of Object.entries(plistExpectations)) {
    const actual = readPlistValue(infoPlistPath, key);
    if (actual !== expected) {
      fail(`${context.containerLabel} ${key} is ${actual}, expected ${expected}`);
    }
  }
  const packagedIconName = readPlistValue(infoPlistPath, 'CFBundleIconName');
  if (packagedIconName !== context.contract.iconName) {
    fail(`${context.containerLabel} CFBundleIconName is ${packagedIconName}, expected ${context.contract.iconName}`);
  }
  const packagedFallbackIconFile = readPlistValue(infoPlistPath, 'CFBundleIconFile');
  if (packagedFallbackIconFile !== 'icon.icns') {
    fail(`${context.containerLabel} CFBundleIconFile is ${packagedFallbackIconFile}, expected icon.icns`);
  }
  const packagedIconPath = join(resolvedAppPath, 'Contents', 'Resources', 'Assets.car');
  if (!existsSync(packagedIconPath) || !statSync(packagedIconPath).isFile()) {
    fail(`${context.containerLabel} packaged Icon Composer catalog is missing: ${packagedIconPath}`);
  }
  validateIconAssetCatalog(packagedIconPath, context.contract.iconName, context.containerLabel);
  const packagedFallbackIconPath = join(
    resolvedAppPath,
    'Contents',
    'Resources',
    packagedFallbackIconFile,
  );
  if (!existsSync(packagedFallbackIconPath) || !statSync(packagedFallbackIconPath).isFile()) {
    fail(`${context.containerLabel} packaged icon fallback is missing: ${packagedFallbackIconPath}`);
  }
  const iconSourceManifest = join(context.contract.iconSourcePath, 'icon.json');
  if (!existsSync(iconSourceManifest) || !statSync(iconSourceManifest).isFile()) {
    fail(`${context.containerLabel} maintained Icon Composer source is missing: ${iconSourceManifest}`);
  }
  validateEmbeddedUpdateContract(resolvedAppPath, context);

  const nativeCanvasPath = join(
    resolvedAppPath,
    'Contents',
    'Resources',
    'app.asar.unpacked',
    'node_modules',
    `@napi-rs/canvas-darwin-${context.expectedArch}`,
    `skia.darwin-${context.expectedArch}.node`,
  );
  const oppositeArch = context.expectedArch === 'arm64' ? 'x64' : 'arm64';
  const oppositeCanvasDirectory = join(
    resolvedAppPath,
    'Contents',
    'Resources',
    'app.asar.unpacked',
    'node_modules',
    `@napi-rs/canvas-darwin-${oppositeArch}`,
  );
  if (!existsSync(nativeCanvasPath)) {
    fail(`${context.containerLabel} is missing native canvas module ${nativeCanvasPath}`);
  }
  if (existsSync(oppositeCanvasDirectory)) {
    fail(`${context.containerLabel} contains the opposite-architecture native canvas package`);
  }

  run('codesign', ['--verify', '--deep', '--strict', '--verbose=4', resolvedAppPath]);
  const tree = collectTree(resolvedAppPath);
  const machOFiles = tree.files.filter(isMachO).sort();
  const bundlePaths = [resolvedAppPath, ...tree.bundles].sort();
  if (machOFiles.length === 0) {
    fail(`${context.containerLabel} contains no Mach-O files`);
  }

  const codeContext = {
    ...context,
    appPath: resolvedAppPath,
    certificateIndex: 0,
  };
  let appMetadata;
  for (const bundlePath of bundlePaths) {
    const metadata = validateCodeObject(bundlePath, codeContext, { validateChain: bundlePath === resolvedAppPath });
    if (bundlePath === resolvedAppPath) {
      appMetadata = metadata;
    }
  }
  for (const filePath of machOFiles) {
    validateCodeObject(filePath, codeContext, { machO: true });
  }
  if (!machOFiles.includes(realpathSync(nativeCanvasPath))) {
    fail(`${context.containerLabel} native canvas module was not included in the Mach-O signature walk`);
  }
  if (appMetadata.ticket !== 'stapled') {
    fail(`${context.containerLabel} app does not report a stapled notarization ticket`);
  }
  run('xcrun', ['stapler', 'validate', resolvedAppPath]);
  run('spctl', ['--assess', '--type', 'execute', '--verbose=4', resolvedAppPath]);

  return {
    appPath: resolvedAppPath,
    cdHash: appMetadata.cdHash,
    executablePath,
    manifest: createAppManifest(resolvedAppPath),
    machOCount: machOFiles.length,
    signedBundleCount: bundlePaths.length,
  };
}

function validateEmbeddedUpdateContract(appPath, context) {
  const resourcesPath = join(appPath, 'Contents', 'Resources');
  const asarPath = join(resourcesPath, 'app.asar');
  const updateConfigPath = join(resourcesPath, 'app-update.yml');
  if (!existsSync(asarPath) || !existsSync(updateConfigPath)) {
    fail(`${context.containerLabel} is missing packaged updater configuration`);
  }

  let packageMetadata;
  try {
    packageMetadata = JSON.parse(extractAsarFile(asarPath, 'package.json').toString('utf8'));
  } catch (error) {
    fail(`${context.containerLabel} has invalid packaged package.json: ${error.message}`);
  }
  const metadataExpectations = {
    name: context.contract.packageName,
    productName: context.contract.productName,
    butterPaperChannel: context.contract.channel,
    version: context.version,
  };
  for (const [key, expected] of Object.entries(metadataExpectations)) {
    if (packageMetadata?.[key] !== expected) {
      fail(`${context.containerLabel} packaged ${key} is ${String(packageMetadata?.[key])}, expected ${expected}`);
    }
  }

  let updateConfig;
  try {
    updateConfig = YAML.parse(readFileSync(updateConfigPath, 'utf8'));
  } catch (error) {
    fail(`${context.containerLabel} has invalid app-update.yml: ${error.message}`);
  }
  const expectedFeedUrl = context.contract.updateFeedUrl
    || `https://raw.githubusercontent.com/apotenza92/butter-paper/updates/${context.contract.channel}/darwin/${context.expectedArch}`;
  if (
    updateConfig?.provider !== 'generic'
    || updateConfig?.channel !== 'latest'
    || updateConfig?.url !== expectedFeedUrl
  ) {
    fail(`${context.containerLabel} has an unexpected packaged update feed`);
  }
}

function validateArtifactSelection(releaseDir, contract) {
  const artifactStem = `${contract.artifactPrefix}-${contract.arch}`;
  const expectedNames = [
    `${artifactStem}.dmg`,
    `${artifactStem}.zip`,
    `${artifactStem}.dmg.blockmap`,
    `${artifactStem}.zip.blockmap`,
  ];
  for (const name of expectedNames) {
    if (!existsSync(join(releaseDir, name))) {
      fail(`Required packaged artifact is missing: ${join(releaseDir, name)}`);
    }
  }
  const allowedNames = new Set([
    ...expectedNames,
    `${artifactStem}.dmg.sha256`,
    `${artifactStem}.zip.sha256`,
  ]);
  const selectedPrefix = artifactStem;
  const unexpected = readdirSync(releaseDir)
    .filter((name) => name.startsWith(selectedPrefix))
    .filter((name) => !allowedNames.has(name));
  if (unexpected.length > 0) {
    fail(`Unexpected ${contract.channel}/${contract.arch} artifact names: ${unexpected.join(', ')}`);
  }
  return Object.fromEntries(expectedNames.map((name) => [name, join(releaseDir, name)]));
}

function validateBlockmapFile(blockmapPath, artifactPath) {
  let parsed;
  try {
    parsed = JSON.parse(gunzipSync(readFileSync(blockmapPath)).toString('utf8'));
  } catch (error) {
    fail(`${blockmapPath} is not valid gzip-compressed JSON: ${error.message}`);
  }
  validateBlockmap(parsed, statSync(artifactPath).size, basename(blockmapPath));
}

function verifyOrReportChecksum(artifactPath, requireChecksums) {
  const checksumPath = `${artifactPath}.sha256`;
  const actualHash = hashFile(artifactPath);
  if (!existsSync(checksumPath)) {
    if (requireChecksums) {
      fail(`Required checksum is missing: ${checksumPath}`);
    }
  } else {
    validateChecksumText(readFileSync(checksumPath, 'utf8'), basename(artifactPath), actualHash);
  }
  console.log(`${actualHash}  ${basename(artifactPath)}`);
}

function compareVerifiedApps(dmgApp, zipApp) {
  if (dmgApp.cdHash !== zipApp.cdHash) {
    fail(`DMG and ZIP apps have different CDHashes: ${dmgApp.cdHash} and ${zipApp.cdHash}`);
  }
  if (dmgApp.manifest.length !== zipApp.manifest.length) {
    fail(`DMG and ZIP app manifests differ in length: ${dmgApp.manifest.length} and ${zipApp.manifest.length}`);
  }
  for (let index = 0; index < dmgApp.manifest.length; index += 1) {
    const left = JSON.stringify(dmgApp.manifest[index]);
    const right = JSON.stringify(zipApp.manifest[index]);
    if (left !== right) {
      fail(`DMG and ZIP app manifests first differ at index ${index}:\n${left}\n${right}`);
    }
  }
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", `'"'"'`)}'`;
}

function launchPackagedApp(executablePath, temporaryDirectory, channel) {
  const userDataPath = join(temporaryDirectory, 'user-data');
  const wrapperPath = join(temporaryDirectory, 'launch-butter-paper');
  mkdirSync(userDataPath, { recursive: true, mode: 0o700 });
  writeFileSync(
    wrapperPath,
    `#!/bin/sh\nexec ${shellQuote(executablePath)} ${shellQuote(`--user-data-dir=${userDataPath}`)} "$@"\n`,
    { mode: 0o700 },
  );
  chmodSync(wrapperPath, 0o700);
  run('pnpm', ['test:package:desktop'], {
    env: createSmokeEnvironment(process.env, {
      BP_ELECTRON_EXECUTABLE_PATH: wrapperPath,
      BP_RELEASE_CHANNEL: channel,
      BP_TEST_USER_DATA_DIR: userDataPath,
    }),
  });
  if (readdirSync(userDataPath).length === 0) {
    fail('Packaged launch did not initialize its isolated user-data directory');
  }
}

function readOption(name, fallback = null) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : fallback;
}

export function main() {
  if (process.platform !== 'darwin') {
    fail('macOS package verification must run on macOS');
  }
  const expectedArch = readOption('--arch', process.arch);
  const channel = readOption('--channel', process.env.BP_RELEASE_CHANNEL?.trim() || 'stable');
  const skipLaunch = process.argv.includes('--skip-launch');
  const requireChecksums = process.argv.includes('--require-checksums');
  const baseContract = resolveReleaseContract(channel, expectedArch);
  if (!skipLaunch && process.arch !== expectedArch) {
    fail(`Native launch verification requires a ${expectedArch} runner, received ${process.arch}`);
  }

  const releaseDir = resolve(readOption(
    '--output-dir',
    readOption(
      '--release-dir',
      process.env.BP_RELEASE_OUTPUT_DIR?.trim()
        || join(repoRoot, 'apps/desktop/release', baseContract.channel, baseContract.arch),
    ),
  ));
  const contract = resolveConfiguredReleaseContract(channel, expectedArch, releaseDir);
  const expectedTeamId = process.env.APPLE_TEAM_ID?.trim() || BUTTER_PAPER_APPLE_TEAM_ID;
  const expectations = {
    teamId: expectedTeamId,
    identity: process.env.APPLE_SIGNING_IDENTITY?.trim()
      || (expectedTeamId === BUTTER_PAPER_APPLE_TEAM_ID
        ? BUTTER_PAPER_SIGNING_IDENTITY
        : `Developer ID Application: Alexander Potenza (${expectedTeamId})`),
    fingerprint: normalizeFingerprint(
      process.env.APPLE_SIGNING_CERTIFICATE_SHA256?.trim()
        || BUTTER_PAPER_SIGNING_CERTIFICATE_SHA256,
    ),
  };
  const desktopPackage = JSON.parse(readFileSync(join(repoRoot, 'apps/desktop/package.json'), 'utf8'));
  const artifactPaths = validateArtifactSelection(releaseDir, contract);
  const artifactStem = `${contract.artifactPrefix}-${contract.arch}`;
  const dmgPath = artifactPaths[`${artifactStem}.dmg`];
  const zipPath = artifactPaths[`${artifactStem}.zip`];
  const dmgBlockmapPath = artifactPaths[`${artifactStem}.dmg.blockmap`];
  const zipBlockmapPath = artifactPaths[`${artifactStem}.zip.blockmap`];

  run('hdiutil', ['verify', dmgPath]);
  run('unzip', ['-tq', zipPath]);
  validateZipEntries(run('unzip', ['-Z1', zipPath]).stdout, contract);
  validateBlockmapFile(dmgBlockmapPath, dmgPath);
  validateBlockmapFile(zipBlockmapPath, zipPath);
  verifyOrReportChecksum(dmgPath, requireChecksums);
  verifyOrReportChecksum(zipPath, requireChecksums);
  validateUpdateMetadataFile(join(releaseDir, 'latest-mac.yml'), {
    [basename(dmgPath)]: dmgPath,
    [basename(zipPath)]: zipPath,
  }, desktopPackage.version);

  const temporaryDirectory = mkdtempSync(join(tmpdir(), 'butter-paper-verify-'));
  chmodSync(temporaryDirectory, 0o700);
  const mountPath = join(temporaryDirectory, 'dmg');
  const zipExtractionPath = join(temporaryDirectory, 'zip');
  const certificatePath = join(temporaryDirectory, 'certificates');
  mkdirSync(mountPath, { mode: 0o700 });
  mkdirSync(zipExtractionPath, { mode: 0o700 });
  mkdirSync(certificatePath, { mode: 0o700 });
  let mounted = false;
  try {
    run('hdiutil', ['attach', '-readonly', '-nobrowse', '-mountpoint', mountPath, dmgPath]);
    mounted = true;
    run('ditto', ['-x', '-k', zipPath, zipExtractionPath]);
    const zipTopLevel = readdirSync(zipExtractionPath);
    if (zipTopLevel.length !== 1 || zipTopLevel[0] !== contract.appName) {
      fail(`ZIP extracted unexpected top-level entries: ${zipTopLevel.join(', ')}`);
    }

    const commonContext = {
      certificateDir: certificatePath,
      contract,
      expectedArch,
      expectations,
      version: desktopPackage.version,
    };
    const dmgApp = verifyApp(join(mountPath, contract.appName), {
      ...commonContext,
      containerLabel: 'DMG',
    });
    const zipApp = verifyApp(join(zipExtractionPath, contract.appName), {
      ...commonContext,
      containerLabel: 'ZIP',
    });
    compareVerifiedApps(dmgApp, zipApp);

    run('xcrun', ['stapler', 'validate', dmgPath]);
    run('spctl', [
      '--assess',
      '--type', 'open',
      '--context', 'context:primary-signature',
      '--verbose=4',
      dmgPath,
    ]);
    if (!skipLaunch) {
      launchPackagedApp(zipApp.executablePath, temporaryDirectory, contract.channel);
    }
    console.log(
      `macOS ${contract.channel}/${expectedArch} package verification passed `
      + `(${zipApp.machOCount} Mach-O files, ${zipApp.signedBundleCount} signed bundles).`,
    );
  } finally {
    if (mounted) {
      run('hdiutil', ['detach', mountPath], { allowFailure: true });
    }
    rmSync(temporaryDirectory, { recursive: true, force: true });
  }
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : null;
if (invokedPath === import.meta.url) {
  main();
}
