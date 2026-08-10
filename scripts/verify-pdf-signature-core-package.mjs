#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createReadStream } from 'node:fs';
import { mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import {
  normalizeTarArgumentsForHost,
  verifyAndExtractCompleteSourceArtifact,
} from '../native/pdf-signature-core/scripts/complete-source-artifact.mjs';
import {
  PDF_SIGNATURE_CORE_PROTOCOL_VERSION,
  verifyPdfSignatureCorePackage,
} from '../apps/desktop/src/main/pdfSignatureCorePackage.ts';

const HANDSHAKE_TIMEOUT_MS = 15_000;
const MAX_PROTOCOL_BYTES = 1024 * 1024;
const CURRENT_OPERATIONS = [
  'handshake',
  'version',
  'inspect',
  'validate',
  'createUnsignedCopy',
  'inspectUnsignedStructure',
  'cancel',
];
const CURRENT_CAPABILITIES = {
  certificateSign: false,
  certify: false,
  createUnsignedCopy: true,
  inspect: true,
  ltv: false,
  onlineValidation: false,
  pkcs11: false,
  signatureRead: true,
  signatureValidation: true,
  signedIncrementalEdit: false,
};
const OUTER_AUTHENTICATION_DECLARATIONS = new Set([
  'none',
  'macos-developer-id-notarized',
  'windows-authenticode',
  'tuf',
]);
const OUTER_AUTHENTICATION_NOT_VERIFIED = 'declared-not-verified';

export function parseArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (![
      '--package-root', '--search-root', '--platform', '--arch', '--evidence', '--outer-artifact',
      '--outer-authentication', '--source-artifact', '--verification-mode',
    ].includes(argument)) {
      throw new Error(`Unknown PDF signature package verifier argument: ${argument}`);
    }
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`${argument} requires a value`);
    options[argument.slice(2)] = value;
    index += 1;
  }
  if (Boolean(options['package-root']) === Boolean(options['search-root'])) {
    throw new Error('Exactly one of --package-root or --search-root is required');
  }
  for (const required of ['platform', 'arch']) {
    if (!options[required]) throw new Error(`--${required} is required`);
  }
  options['verification-mode'] ??= 'runtime';
  if (!['runtime', 'proof'].includes(options['verification-mode'])) {
    throw new Error('--verification-mode must be runtime or proof');
  }
  if (options['verification-mode'] === 'proof' && !options['source-artifact']) {
    throw new Error('--source-artifact is required in proof verification mode');
  }
  if (options['outer-authentication']
    && !OUTER_AUTHENTICATION_DECLARATIONS.has(options['outer-authentication'])) {
    throw new Error('--outer-authentication is invalid');
  }
  if (options['outer-authentication'] && !options['outer-artifact']) {
    throw new Error('--outer-authentication requires --outer-artifact');
  }
  return options;
}

export async function findPackagedSignatureCore(searchRoot, platform, arch) {
  const target = `${platform}-${arch}`;
  const matches = [];
  const pending = [resolve(searchRoot)];
  while (pending.length > 0) {
    const directory = pending.pop();
    const entries = await readdir(directory, { withFileTypes: true }).catch(() => []);
    for (const entry of entries) {
      if (entry.isSymbolicLink()) continue;
      const candidate = resolve(directory, entry.name);
      if (!entry.isDirectory()) continue;
      if (entry.name === target && directory.endsWith(`${process.platform === 'win32' ? '\\' : '/'}pdf-signature-core`)) {
        const manifest = await readFile(resolve(candidate, 'manifest.json')).catch(() => undefined);
        if (manifest) matches.push(candidate);
        continue;
      }
      pending.push(candidate);
    }
  }
  if (matches.length !== 1) {
    throw new Error(`Expected exactly one packaged PDF signature core under ${resolve(searchRoot)}; found ${matches.length}.`);
  }
  return matches[0];
}

export function validateHandshakeEnvelope(envelope, verifiedPackage, requestId) {
  if (!envelope || typeof envelope !== 'object' || Array.isArray(envelope)
    || envelope.protocolVersion !== PDF_SIGNATURE_CORE_PROTOCOL_VERSION
    || envelope.requestId !== requestId
    || envelope.operation !== 'handshake'
    || envelope.engineVersion !== verifiedPackage.manifest.engineVersion
    || envelope.event !== 'result'
    || !envelope.result
    || typeof envelope.result !== 'object'
    || Array.isArray(envelope.result)) {
    throw new Error('Packaged PDF signature core returned a mismatched handshake envelope.');
  }
  const { result } = envelope;
  if (!result.versions || typeof result.versions !== 'object'
    || result.versions.protocol !== PDF_SIGNATURE_CORE_PROTOCOL_VERSION
    || result.versions.engine !== verifiedPackage.manifest.engineVersion
    || result.versions.javaFeature !== 21
    || typeof result.versions.java !== 'string'
    || !result.versions.java.startsWith(verifiedPackage.manifest.javaVersion)
    || result.versions.dss !== '6.4'
    || result.versions.pdfBox !== '3.0.6'
    || result.versions.jackson !== '2.21.5'
    || !matchesExactStringArray(result.operations, CURRENT_OPERATIONS)
    || !matchesCurrentCapabilities(result.capabilities)) {
    throw new Error('Packaged PDF signature core handshake does not match the current capability contract.');
  }
  return result;
}

function matchesCurrentCapabilities(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const expectedKeys = Object.keys(CURRENT_CAPABILITIES).sort();
  const actualKeys = Object.keys(value).sort();
  return actualKeys.length === expectedKeys.length
    && actualKeys.every((key, index) => key === expectedKeys[index])
    && expectedKeys.every((key) => value[key] === CURRENT_CAPABILITIES[key]);
}

function matchesExactStringArray(value, expected) {
  return Array.isArray(value)
    && value.length === expected.length
    && value.every((entry, index) => entry === expected[index]);
}

export async function buildEvidence(
  verifiedPackage,
  handshake,
  measurements = {},
  outerArtifact = null,
  completeSourceProof = null,
) {
  const outerArtifactEvidence = validateOuterArtifactDeclaration(outerArtifact);
  const manifestBytes = await readFile(resolve(verifiedPackage.packageRoot, 'manifest.json'));
  const postSignInventoryBytes = await readFile(resolve(
    verifiedPackage.packageRoot,
    verifiedPackage.manifest.postSignInventory,
  ));
  const installedBytes = verifiedPackage.manifest.components.reduce(
    (total, component) => total + component.size,
    manifestBytes.byteLength + postSignInventoryBytes.byteLength,
  );
  const launcher = verifiedPackage.manifest.components.find(
    (component) => component.path === verifiedPackage.manifest.launcher,
  );
  return {
    schemaVersion: 1,
    target: `${verifiedPackage.manifest.platform}-${verifiedPackage.manifest.arch}`,
    package: {
      engineVersion: verifiedPackage.manifest.engineVersion,
      javaVersion: verifiedPackage.manifest.javaVersion,
      launcher: verifiedPackage.manifest.launcher,
      launcherSha256: launcher.sha256,
      manifestSha256: createHash('sha256').update(manifestBytes).digest('hex'),
      postSignInventorySha256: createHash('sha256').update(postSignInventoryBytes).digest('hex'),
      componentCount: verifiedPackage.manifest.components.length,
      installedBytes,
      compressedArchiveBytes: measurements.compressedArchiveBytes ?? null,
      buildState: verifiedPackage.manifest.buildState,
      evidenceState: verifiedPackage.postSignInventory.evidenceState,
      releaseSealed: verifiedPackage.postSignInventory.releaseSealed,
    },
    performance: {
      coldHandshakeMs: measurements.coldHandshakeMs ?? null,
      firstInspectMs: measurements.firstInspectMs ?? null,
    },
    outerArtifact: outerArtifactEvidence,
    completeSourceArtifact: completeSourceProof,
    handshake: {
      versions: handshake.versions,
      operations: handshake.operations,
      profiles: handshake.profiles,
      providers: handshake.providers,
      capabilities: handshake.capabilities,
      limits: handshake.limits,
    },
    claims: {
      packageInventoryVerified: true,
      nativeArchitectureVerified: true,
      launcherExecutedOnMatchingHost: true,
      codeSigningVerified: false,
      notarizationVerified: false,
      authenticodeVerified: false,
      outerTufVerified: false,
      runtimeExcludesCompleteSource: true,
      completeSourceArtifactVerified: completeSourceProof !== null,
      completeSourceRelinkVerified: completeSourceProof?.relinkVerified === true,
    },
  };
}

function validateOuterArtifactDeclaration(value) {
  if (value === null) return null;
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Outer artifact evidence must be a structural identity with a non-verified authentication declaration.');
  }
  const keys = Object.keys(value).sort();
  if (keys.join('\n') !== ['authentication', 'bytes', 'name', 'sha256'].join('\n')
    || typeof value.name !== 'string'
    || value.name.length === 0
    || value.name.length > 255
    || value.name !== basename(value.name)
    || !Number.isSafeInteger(value.bytes)
    || value.bytes < 0
    || typeof value.sha256 !== 'string'
    || !/^[a-f0-9]{64}$/.test(value.sha256)
    || !value.authentication
    || typeof value.authentication !== 'object'
    || Array.isArray(value.authentication)) {
    throw new Error('Outer artifact structural identity is invalid.');
  }
  const authenticationKeys = Object.keys(value.authentication).sort();
  if (authenticationKeys.join('\n') !== ['declaredMode', 'verificationState'].join('\n')
    || !OUTER_AUTHENTICATION_DECLARATIONS.has(value.authentication.declaredMode)
    || value.authentication.verificationState !== OUTER_AUTHENTICATION_NOT_VERIFIED) {
    throw new Error('Outer artifact authentication must be an allowed, explicitly non-verified declaration.');
  }
  return {
    name: value.name,
    bytes: value.bytes,
    sha256: value.sha256,
    authentication: {
      declaredMode: value.authentication.declaredMode,
      verificationState: OUTER_AUTHENTICATION_NOT_VERIFIED,
    },
  };
}

export async function verifyCompleteSourceAndRelink(verifiedPackage, sourceArtifactPath) {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'bp-signature-core-source-proof-'));
  const extractionRoot = join(temporaryRoot, 'source-extraction');
  const relinkOutput = join(temporaryRoot, 'relink-output');
  try {
    const inspected = await verifyAndExtractCompleteSourceArtifact({
      artifactPath: resolve(sourceArtifactPath),
      descriptor: verifiedPackage.sourceDescriptor,
      extractionRoot,
    });
    await runRelinkProof(
      join(inspected.extractedRoot, 'scripts/rebuild-from-package-source.sh'),
      verifiedPackage.packageRoot,
      inspected.extractedRoot,
      relinkOutput,
    );
    return {
      name: verifiedPackage.sourceDescriptor.delivery.canonicalFileName,
      bytes: verifiedPackage.sourceDescriptor.artifact.bytes,
      sha256: verifiedPackage.sourceDescriptor.artifact.sha256,
      format: verifiedPackage.sourceDescriptor.artifact.format,
      rootDirectory: verifiedPackage.sourceDescriptor.artifact.rootDirectory,
      internalManifestSha256: verifiedPackage.sourceDescriptor.internalManifest.sha256,
      distributionId: verifiedPackage.sourceDescriptor.distributionId,
      product: verifiedPackage.sourceDescriptor.product,
      core: verifiedPackage.sourceDescriptor.core,
      sourceIdentity: verifiedPackage.sourceDescriptor.sourceIdentity,
      compatibleRuntimeTargets: inspected.manifest.compatibleRuntimeTargets,
      legalApproval: false,
      releaseSealed: false,
      freshExtractionVerified: true,
      relinkVerified: true,
    };
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

async function runRelinkProof(script, packageRoot, sourceRoot, outputRoot) {
  await new Promise((resolvePromise, rejectPromise) => {
    const child = spawn('bash', [script, packageRoot, sourceRoot, outputRoot], {
      cwd: packageRoot,
      shell: false,
      windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: relinkProofEnvironment(process.env),
    });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => { stdout = `${stdout}${chunk}`.slice(-16_384); });
    child.stderr.on('data', (chunk) => { stderr = `${stderr}${chunk}`.slice(-16_384); });
    child.once('error', rejectPromise);
    child.once('close', (code, signal) => {
      if (code !== 0 || signal) {
        rejectPromise(new Error(`Complete-source relink proof failed${stderr.trim() ? `: ${stderr.trim()}` : '.'}`));
      } else if (!stdout.includes('separate complete-source artifact rebuilt, reconciled, and executed successfully')) {
        rejectPromise(new Error('Complete-source relink proof did not report successful reconciliation and execution'));
      } else {
        resolvePromise();
      }
    });
  });
}

export function relinkProofEnvironment(source) {
  const allowed = [
    'PATH', 'JAVA_HOME', 'TMPDIR', 'TMP', 'TEMP', 'LANG', 'LC_ALL',
    'SYSTEMROOT', 'WINDIR', 'COMSPEC', 'PATHEXT',
  ];
  const environment = { BP_SIGNATURE_CORE_NETWORK: 'disabled' };
  for (const key of allowed) {
    if (typeof source[key] === 'string' && source[key].length > 0) environment[key] = source[key];
  }
  return environment;
}

async function describeOuterArtifact(artifactPath, authentication = 'none') {
  if (!artifactPath) return null;
  const absolute = resolve(artifactPath);
  const info = await stat(absolute);
  if (!info.isFile()) throw new Error(`Outer artifact is not a regular file: ${absolute}`);
  const hash = createHash('sha256');
  await new Promise((resolvePromise, rejectPromise) => {
    const stream = createReadStream(absolute);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('error', rejectPromise);
    stream.on('end', resolvePromise);
  });
  return {
    name: basename(absolute),
    bytes: info.size,
    sha256: hash.digest('hex'),
    authentication: {
      declaredMode: authentication,
      verificationState: OUTER_AUTHENTICATION_NOT_VERIFIED,
    },
  };
}

export async function verifyPackageAndLaunch({ packageRoot, platform, arch }) {
  const verifiedPackage = await verifyPdfSignatureCorePackage(resolve(packageRoot), { platform, arch });
  if (platform !== process.platform || arch !== process.arch) {
    throw new Error(`Packaged launcher must run on a matching host; requested ${platform}-${arch}, host is ${process.platform}-${process.arch}.`);
  }
  const requestId = 'package-handshake';
  const request = JSON.stringify({
    protocolVersion: PDF_SIGNATURE_CORE_PROTOCOL_VERSION,
    requestId,
    operation: 'handshake',
    payload: {},
  });
  const handshakeRun = await runRequest(verifiedPackage.launcherPath, verifiedPackage.packageRoot, request);
  const handshake = validateHandshakeEnvelope(handshakeRun.envelope, verifiedPackage, requestId);
  const firstInspectMs = await measureFirstInspection(verifiedPackage);
  const compressedArchiveBytes = await measureCompressedArchive(verifiedPackage.packageRoot);
  return {
    verifiedPackage,
    handshake,
    measurements: {
      coldHandshakeMs: handshakeRun.elapsedMs,
      firstInspectMs,
      compressedArchiveBytes,
    },
  };
}

async function runRequest(launcherPath, cwd, request) {
  return new Promise((resolvePromise, rejectPromise) => {
    const startedAt = performance.now();
    const child = spawn(launcherPath, [], {
      cwd,
      shell: false,
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: sidecarEnvironment(process.env),
    });
    let stdout = Buffer.alloc(0);
    let stderr = Buffer.alloc(0);
    let settled = false;
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      settle(new Error('Packaged PDF signature core handshake timed out.'));
    }, HANDSHAKE_TIMEOUT_MS);
    const settle = (error, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error) rejectPromise(error);
      else resolvePromise(value);
    };
    child.once('error', (error) => settle(new Error('Packaged PDF signature core could not launch.', { cause: error })));
    child.stdout.on('data', (chunk) => {
      stdout = Buffer.concat([stdout, chunk]);
      if (stdout.byteLength > MAX_PROTOCOL_BYTES) {
        child.kill('SIGKILL');
        settle(new Error('Packaged PDF signature core handshake exceeded 1 MiB.'));
      }
    });
    child.stderr.on('data', (chunk) => {
      if (stderr.byteLength < 64 * 1024) stderr = Buffer.concat([stderr, chunk]).subarray(0, 64 * 1024);
    });
    child.once('close', (code, signal) => {
      if (settled) return;
      if (code !== 0 || signal) {
        const diagnostic = stderr.toString('utf8').replace(/[\r\n\u0000-\u001f\u007f]+/g, ' ').trim();
        settle(new Error(`Packaged PDF signature core handshake failed${diagnostic ? `: ${diagnostic}` : '.'}`));
        return;
      }
      let decoded;
      try {
        decoded = new TextDecoder('utf-8', { fatal: true }).decode(stdout);
      } catch (error) {
        settle(new Error('Packaged PDF signature core emitted malformed UTF-8.', { cause: error }));
        return;
      }
      const lines = decoded.split('\n').filter(Boolean);
      if (lines.length !== 1) {
        settle(new Error(`Packaged PDF signature core emitted ${lines.length} handshake lines instead of one.`));
        return;
      }
      try {
        settle(undefined, {
          envelope: JSON.parse(lines[0]),
          elapsedMs: Number((performance.now() - startedAt).toFixed(3)),
        });
      } catch (error) {
        settle(new Error('Packaged PDF signature core emitted malformed handshake JSON.', { cause: error }));
      }
    });
    child.stdin.end(`${request}\n`);
  });
}

async function measureFirstInspection(verifiedPackage) {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'bp-signature-core-inspect-'));
  try {
    const inputPath = join(temporaryRoot, 'minimal.pdf');
    await writeFile(inputPath, [
      '%PDF-1.4',
      '1 0 obj',
      '<< /Type /Catalog >>',
      'endobj',
      'trailer',
      '<< /Root 1 0 R >>',
      '%%EOF',
      '',
    ].join('\n'), { flag: 'wx' });
    const requestId = 'package-first-inspect';
    const run = await runRequest(
      verifiedPackage.launcherPath,
      verifiedPackage.packageRoot,
      JSON.stringify({
        protocolVersion: PDF_SIGNATURE_CORE_PROTOCOL_VERSION,
        requestId,
        operation: 'inspect',
        payload: { inputPath },
      }),
    );
    if (!run.envelope || run.envelope.event !== 'result' || run.envelope.requestId !== requestId) {
      throw new Error('Packaged PDF signature core failed its first inspection operation.');
    }
    return run.elapsedMs;
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

async function measureCompressedArchive(packageRoot) {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'bp-signature-core-archive-'));
  const archivePath = join(temporaryRoot, 'package.tar.gz');
  try {
    await new Promise((resolvePromise, rejectPromise) => {
      const tarArguments = normalizeTarArgumentsForHost(['-czf', archivePath, '-C', packageRoot, '.']);
      const child = spawn('tar', tarArguments, {
        shell: false,
        windowsHide: true,
        stdio: ['ignore', 'ignore', 'pipe'],
      });
      let diagnostic = '';
      child.stderr.setEncoding('utf8');
      child.stderr.on('data', (chunk) => { diagnostic = `${diagnostic}${chunk}`.slice(-4096); });
      child.once('error', rejectPromise);
      child.once('close', (code) => {
        if (code === 0) resolvePromise();
        else rejectPromise(new Error(`Could not measure compressed sidecar size${diagnostic.trim() ? `: ${diagnostic.trim()}` : '.'}`));
      });
    });
    return (await stat(archivePath)).size;
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

function sidecarEnvironment(source) {
  const allowed = process.platform === 'win32'
    ? ['SYSTEMROOT', 'WINDIR', 'COMSPEC', 'PATHEXT', 'TEMP', 'TMP', 'LANG', 'LC_ALL']
    : ['TMPDIR', 'TMP', 'TEMP', 'LANG', 'LC_ALL'];
  const environment = { BP_SIGNATURE_CORE_NETWORK: 'disabled' };
  for (const key of allowed) if (source[key]) environment[key] = source[key];
  return environment;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const packageRoot = options['package-root'] ?? await findPackagedSignatureCore(
    options['search-root'],
    options.platform,
    options.arch,
  );
  const { verifiedPackage, handshake, measurements } = await verifyPackageAndLaunch({
    packageRoot,
    platform: options.platform,
    arch: options.arch,
  });
  const outerArtifact = await describeOuterArtifact(
    options['outer-artifact'],
    options['outer-authentication'] ?? 'none',
  );
  const completeSourceProof = options['source-artifact']
    ? await verifyCompleteSourceAndRelink(verifiedPackage, options['source-artifact'])
    : null;
  const evidence = await buildEvidence(
    verifiedPackage,
    handshake,
    measurements,
    outerArtifact,
    completeSourceProof,
  );
  const serialized = `${JSON.stringify(evidence, null, 2)}\n`;
  if (options.evidence) {
    const { mkdir, writeFile } = await import('node:fs/promises');
    const { dirname } = await import('node:path');
    const evidencePath = resolve(options.evidence);
    await mkdir(dirname(evidencePath), { recursive: true });
    await writeFile(evidencePath, serialized, { flag: 'wx' });
  }
  process.stdout.write(serialized);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
