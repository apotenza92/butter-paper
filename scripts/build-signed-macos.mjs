import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import process from 'node:process';
import { buildBlockMap } from 'app-builder-lib/out/targets/blockmap/blockmap.js';
import {
  BUTTER_PAPER_SIGNING_CERTIFICATE_SHA256,
  normalizeFingerprint,
  resolveConfiguredReleaseContract,
  resolveReleaseContract,
} from './verify-macos-package.mjs';
import { refreshUpdateMetadataArtifact } from './update-metadata-contract.mjs';

const repoRoot = resolve(import.meta.dirname, '..');

function readOption(name, fallback = null) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : fallback;
}

const arch = readOption('--arch', process.env.BP_RELEASE_ARCH?.trim() || process.arch);
const channel = readOption('--channel', process.env.BP_RELEASE_CHANNEL?.trim() || 'stable');
const baseReleaseContract = resolveReleaseContract(channel, arch);
const releaseDir = resolve(readOption(
  '--output-dir',
  process.env.BP_RELEASE_OUTPUT_DIR?.trim()
    || join(repoRoot, 'apps/desktop/release', baseReleaseContract.channel, baseReleaseContract.arch),
));
const releaseContract = resolveConfiguredReleaseContract(channel, arch, releaseDir);
const skipBuild = process.argv.includes('--skip-build');
const expectedCertificateSha256 = normalizeFingerprint(
  process.env.APPLE_SIGNING_CERTIFICATE_SHA256
    || BUTTER_PAPER_SIGNING_CERTIFICATE_SHA256,
);

const requiredEnvironment = [
  'APPLE_SIGNING_CERTIFICATE_P12_BASE64',
  'APPLE_SIGNING_CERTIFICATE_PASSWORD',
  'APPLE_NOTARYTOOL_KEY_ID',
  'APPLE_NOTARYTOOL_ISSUER_ID',
  'APPLE_NOTARYTOOL_KEY_P8_BASE64',
  'APPLE_SIGNING_IDENTITY',
  'APPLE_TEAM_ID',
];

const credentials = Object.fromEntries(requiredEnvironment.map((name) => {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(`Required release environment variable is missing: ${name}`);
  }
  return [name, value];
}));

for (const secretName of [
  'APPLE_SIGNING_CERTIFICATE_P12_BASE64',
  'APPLE_SIGNING_CERTIFICATE_PASSWORD',
  'APPLE_NOTARYTOOL_KEY_P8_BASE64',
]) {
  delete process.env[secretName];
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    encoding: 'utf8',
    env: options.env ?? process.env,
    input: options.input,
    stdio: options.capture ? ['pipe', 'pipe', 'pipe'] : ['inherit', 'inherit', 'inherit'],
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0 && !options.allowFailure) {
    const diagnostic = options.capture
      ? `\n${`${result.stdout ?? ''}${result.stderr ?? ''}`.trim()}`
      : '';
    throw new Error(`${command} ${args.join(' ')} failed with exit code ${result.status}${diagnostic}`);
  }
  return result;
}

function decodeBase64(value, label) {
  let encoded = value.trim();
  if (encoded.startsWith("'") && encoded.endsWith("'")) {
    encoded = encoded.slice(1, -1);
  }
  const commaIndex = encoded.indexOf(';base64,');
  if (commaIndex >= 0) {
    encoded = encoded.slice(commaIndex + ';base64,'.length);
  }
  const decoded = Buffer.from(encoded.replace(/\s+/g, ''), 'base64');
  if (decoded.length === 0) {
    throw new Error(`${label} did not decode to any data`);
  }
  return decoded;
}

function resolveDeveloperDirectory() {
  if (process.env.DEVELOPER_DIR?.trim()) {
    return process.env.DEVELOPER_DIR.trim();
  }
  const candidates = [
    '/Applications/Xcode.app/Contents/Developer',
    '/Applications/Xcode-beta.app/Contents/Developer',
  ];
  const candidate = candidates.find(existsSync);
  if (!candidate) {
    throw new Error('A complete Xcode installation is required for notarization');
  }
  return candidate;
}

function parseKeychainList(output) {
  return output
    .split('\n')
    .map((line) => line.trim().replace(/^"(.*)"$/, '$1'))
    .filter(Boolean);
}

function writeChecksum(filePath) {
  const result = run('shasum', ['-a', '256', filePath], { capture: true });
  const hash = result.stdout.trim().split(/\s+/)[0];
  if (!/^[a-f0-9]{64}$/.test(hash)) {
    throw new Error(`Unable to calculate SHA-256 for ${filePath}`);
  }
  writeFileSync(`${filePath}.sha256`, `${hash}  ${basename(filePath)}\n`, { mode: 0o644 });
}

function removeCurrentVariantArtifacts() {
  mkdirSync(releaseDir, { recursive: true });
  const artifactStem = `${releaseContract.artifactPrefix}-${releaseContract.arch}`;
  for (const suffix of [
    '.dmg',
    '.dmg.blockmap',
    '.dmg.sha256',
    '.zip',
    '.zip.blockmap',
    '.zip.sha256',
  ]) {
    rmSync(join(releaseDir, `${artifactStem}${suffix}`), { force: true });
  }
  rmSync(
    join(releaseDir, `notarization-${releaseContract.channel}-${releaseContract.arch}.json`),
    { force: true },
  );
}

function validateImportedCertificateFingerprint(keychainPath, identity, expectedFingerprint) {
  const result = run(
    'security',
    ['find-certificate', '-a', '-c', identity, '-Z', keychainPath],
    { capture: true },
  );
  const fingerprints = [...result.stdout.matchAll(/SHA-256 hash:\s*([A-Fa-f0-9]+)/g)]
    .map((match) => normalizeFingerprint(match[1]));
  if (!fingerprints.includes(expectedFingerprint)) {
    throw new Error(
      `Imported Developer ID certificate fingerprints ${fingerprints.join(', ') || 'missing'} `
      + `do not include ${expectedFingerprint}`,
    );
  }
}

function parseJsonResult(result, label) {
  const candidates = [result.stdout, result.stderr].filter((value) => value?.trim());
  for (const candidate of candidates) {
    try {
      return JSON.parse(candidate);
    } catch {
      continue;
    }
  }
  throw new Error(`${label} did not return valid JSON`);
}

function fetchAndReviewNotaryLog(submissionId, apiKeyPath, builderEnvironment) {
  const logResult = run('xcrun', [
    'notarytool',
    'log', submissionId,
    '--key', apiKeyPath,
    '--key-id', credentials.APPLE_NOTARYTOOL_KEY_ID,
    '--issuer', credentials.APPLE_NOTARYTOOL_ISSUER_ID,
  ], { capture: true, env: builderEnvironment, allowFailure: true });
  if (logResult.status !== 0) {
    throw new Error(
      `Unable to retrieve notarization log ${submissionId}:\n`
      + `${logResult.stdout ?? ''}${logResult.stderr ?? ''}`.trim(),
    );
  }
  const log = parseJsonResult(logResult, `Notarization log ${submissionId}`);
  const issues = Array.isArray(log.issues) ? log.issues : [];
  for (const issue of issues) {
    const severity = String(issue.severity ?? 'unknown').toLowerCase();
    const path = issue.path ? ` (${issue.path})` : '';
    console.warn(`Notarization ${severity}${path}: ${issue.message ?? 'No message'}`);
  }
  if (issues.some((issue) => String(issue.severity).toLowerCase() === 'error')) {
    throw new Error(`Notarization log ${submissionId} contains error issues`);
  }
  writeFileSync(
    join(releaseDir, `notarization-${releaseContract.channel}-${releaseContract.arch}.json`),
    `${JSON.stringify(log, null, 2)}\n`,
    { mode: 0o644 },
  );
  return log;
}

const signingDir = mkdtempSync(join(tmpdir(), 'butter-paper-signing-'));
chmodSync(signingDir, 0o700);
const keychainPath = join(signingDir, 'signing.keychain-db');
const originalP12Path = join(signingDir, 'original.p12');
const passwordPath = join(signingDir, 'p12-password');
const combinedPemPath = join(signingDir, 'combined.pem');
const importP12Path = join(signingDir, 'import.p12');
const apiKeyPath = join(signingDir, 'AuthKey.p8');
const temporaryImportPassword = 'butter-paper-disposable-import';
let originalKeychains = [];
let keychainCreated = false;
let cleanupStarted = false;

function cleanup() {
  if (cleanupStarted) {
    return;
  }
  cleanupStarted = true;
  if (originalKeychains.length > 0) {
    spawnSync('security', ['list-keychains', '-d', 'user', '-s', ...originalKeychains], {
      stdio: 'ignore',
    });
  }
  if (keychainCreated) {
    spawnSync('security', ['delete-keychain', keychainPath], { stdio: 'ignore' });
  }
  rmSync(signingDir, { recursive: true, force: true });
}

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => {
    cleanup();
    process.exit(signal === 'SIGINT' ? 130 : 143);
  });
}

try {
  writeFileSync(
    originalP12Path,
    decodeBase64(credentials.APPLE_SIGNING_CERTIFICATE_P12_BASE64, 'Signing certificate'),
    { mode: 0o600 },
  );
  writeFileSync(passwordPath, credentials.APPLE_SIGNING_CERTIFICATE_PASSWORD, { mode: 0o600 });

  const p8Value = credentials.APPLE_NOTARYTOOL_KEY_P8_BASE64;
  const p8Contents = p8Value.includes('BEGIN PRIVATE KEY')
    ? Buffer.from(p8Value)
    : decodeBase64(p8Value, 'App Store Connect private key');
  writeFileSync(apiKeyPath, p8Contents, { mode: 0o600 });

  run('openssl', [
    'pkcs12',
    '-legacy',
    '-in', originalP12Path,
    '-passin', `file:${passwordPath}`,
    '-nodes',
    '-out', combinedPemPath,
  ]);
  run('openssl', [
    'pkcs12',
    '-legacy',
    '-export',
    '-in', combinedPemPath,
    '-passout', `pass:${temporaryImportPassword}`,
    '-out', importP12Path,
    '-name', 'Butter Paper Developer ID',
  ]);

  originalKeychains = parseKeychainList(
    run('security', ['list-keychains', '-d', 'user'], { capture: true }).stdout,
  );
  run('security', ['create-keychain', '-p', '', keychainPath]);
  keychainCreated = true;
  run('security', ['set-keychain-settings', '-lut', '21600', keychainPath]);
  run('security', ['unlock-keychain', '-p', '', keychainPath]);
  run('security', [
    'import', importP12Path,
    '-k', keychainPath,
    '-P', temporaryImportPassword,
    '-T', '/usr/bin/codesign',
  ]);
  run('security', [
    'set-key-partition-list',
    '-S', 'apple-tool:,apple:,codesign:',
    '-s',
    '-k', '',
    keychainPath,
  ]);
  run('security', ['list-keychains', '-d', 'user', '-s', keychainPath, ...originalKeychains]);

  const identityOutput = run(
    'security',
    ['find-identity', '-v', '-p', 'codesigning', keychainPath],
    { capture: true },
  ).stdout;
  if (!identityOutput.includes(credentials.APPLE_SIGNING_IDENTITY)) {
    throw new Error(`Expected signing identity is unavailable: ${credentials.APPLE_SIGNING_IDENTITY}`);
  }
  validateImportedCertificateFingerprint(
    keychainPath,
    credentials.APPLE_SIGNING_IDENTITY,
    expectedCertificateSha256,
  );

  const developerDir = resolveDeveloperDirectory();
  const builderEnvironment = {
    ...process.env,
    DEVELOPER_DIR: developerDir,
    CSC_KEYCHAIN: keychainPath,
    CSC_NAME: credentials.APPLE_SIGNING_IDENTITY.replace(/^Developer ID Application:\s*/, ''),
    APPLE_API_KEY: apiKeyPath,
    APPLE_API_KEY_ID: credentials.APPLE_NOTARYTOOL_KEY_ID,
    APPLE_API_ISSUER: credentials.APPLE_NOTARYTOOL_ISSUER_ID,
    APPLE_SIGNING_IDENTITY: credentials.APPLE_SIGNING_IDENTITY,
    APPLE_SIGNING_CERTIFICATE_SHA256: expectedCertificateSha256,
    APPLE_TEAM_ID: credentials.APPLE_TEAM_ID,
    BP_RELEASE_ARCH: releaseContract.arch,
    BP_RELEASE_CHANNEL: releaseContract.channel,
    BP_RELEASE_OUTPUT_DIR: releaseDir,
  };

  if (!skipBuild) {
    run('pnpm', ['build:desktop'], { env: builderEnvironment });
  }
  removeCurrentVariantArtifacts();
  run('pnpm', [
    '--dir', 'apps/desktop',
    'exec', 'electron-builder',
    '--config', 'electron-builder.config.cjs',
    '--mac', 'dmg', 'zip',
    `--${arch}`,
    '--publish', 'never',
  ], { env: builderEnvironment });

  const artifactStem = `${releaseContract.artifactPrefix}-${releaseContract.arch}`;
  const dmgPath = join(releaseDir, `${artifactStem}.dmg`);
  const zipPath = join(releaseDir, `${artifactStem}.zip`);
  const notaryResult = run('xcrun', [
    'notarytool',
    'submit', dmgPath,
    '--key', apiKeyPath,
    '--key-id', credentials.APPLE_NOTARYTOOL_KEY_ID,
    '--issuer', credentials.APPLE_NOTARYTOOL_ISSUER_ID,
    '--wait',
    '--output-format', 'json',
  ], { capture: true, env: builderEnvironment, allowFailure: true });
  const notaryResponse = parseJsonResult(notaryResult, 'DMG notarization submission');
  if (notaryResponse.id) {
    fetchAndReviewNotaryLog(notaryResponse.id, apiKeyPath, builderEnvironment);
  }
  if (notaryResult.status !== 0) {
    throw new Error(`DMG notarization command failed: ${notaryResult.stdout || notaryResult.stderr}`);
  }
  if (notaryResponse.status !== 'Accepted') {
    throw new Error(`DMG notarization was not accepted: ${notaryResult.stdout}`);
  }
  console.log(`DMG notarization accepted: ${notaryResponse.id}`);
  run('xcrun', ['stapler', 'staple', dmgPath], { env: builderEnvironment });
  await buildBlockMap(dmgPath, 'gzip', `${dmgPath}.blockmap`);
  refreshUpdateMetadataArtifact(join(releaseDir, 'latest-mac.yml'), dmgPath);

  writeChecksum(dmgPath);
  writeChecksum(zipPath);
  run('node', [
    'scripts/verify-macos-package.mjs',
    '--arch', releaseContract.arch,
    '--channel', releaseContract.channel,
    '--output-dir', releaseDir,
    '--require-checksums',
    '--skip-launch',
  ], { env: builderEnvironment });
  console.log(`Signed and notarized macOS ${releaseContract.channel}/${releaseContract.arch} release build passed.`);
} finally {
  cleanup();
}
