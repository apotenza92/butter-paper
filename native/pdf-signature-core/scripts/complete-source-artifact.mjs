#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { constants as fsConstants, createReadStream, createWriteStream } from 'node:fs';
import {
  chmod,
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  open,
  readFile,
  readdir,
  realpath,
  rm,
  stat,
  unlink,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { once } from 'node:events';
import { basename, dirname, join, posix, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createGzip, createGunzip, constants as zlibConstants } from 'node:zlib';
import { validateGeneratedSbomForCveScanning } from './canonicalize-generated-source-inputs.mjs';

export const COMPLETE_SOURCE_SCHEMA_VERSION = 1;
export const COMPLETE_SOURCE_ARTIFACT_VERSION = '0.1.0';
export const COMPLETE_SOURCE_DISTRIBUTION_ID = 'butter-paper-pdf-signature-core-complete-source-v1';
export const COMPLETE_SOURCE_PRODUCT = 'Butter Paper';
export const COMPLETE_SOURCE_CORE = Object.freeze({ name: 'pdf-signature-core', version: '0.1.0' });
export const COMPLETE_SOURCE_MANIFEST_NAME = 'SOURCE-MANIFEST.json';
export const COMPLETE_SOURCE_DESCRIPTOR_NAME = 'complete-source-artifact.json';
export const COMPLETE_SOURCE_TARGETS = Object.freeze([
  'darwin-arm64',
  'darwin-x64',
  'linux-arm64',
  'linux-x64',
  'win32-arm64',
  'win32-x64',
]);

const MAX_COMPRESSED_BYTES = 256 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES = 512 * 1024 * 1024;
const MAX_ENTRY_BYTES = 192 * 1024 * 1024;
const MAX_MANIFEST_BYTES = 4 * 1024 * 1024;
const MAX_ENTRIES = 20_000;
const REQUIRED_BUTTER_SOURCES = Object.freeze([
  'AuthoritativeSignedPolicies.java',
  'BoundedLineReader.java',
  'EngineVersions.java',
  'ExactTrustPolicy.java',
  'FramedProtocolServer.java',
  'InspectionService.java',
  'LicenseEvidenceVerifier.java',
  'Main.java',
  'PackageManifestWriter.java',
  'Pkcs12IdentityService.java',
  'Pkcs12PasswordPrompt.java',
  'Protocol.java',
  'ProtocolServer.java',
  'SafePdfMutation.java',
  'SecretScrubber.java',
  'SignatureFieldService.java',
  'SignatureFieldSpec.java',
  'SignedMutationPostcheck.java',
  'SignedMutationPostvalidationService.java',
  'SigningService.java',
  'UnsignedCopyService.java',
  'ValidationService.java',
].map((name) => `src/main/java/com/butterpaper/signaturecore/${name}`));
const REQUIRED_SOURCE_PATHS = Object.freeze([
  '.mvn/jvm.config',
  '.mvn/maven.config',
  'generated/dependency-inventory.json',
  'generated/pdf-signature-core-sources.jar',
  'generated/pdf-signature-core.cdx.json',
  'mvnw',
  'notices/LGPL-RELINKING.md',
  'notices/MIT.txt',
  'notices/THIRD-PARTY-NOTICES.md',
  'pom.xml',
  'scripts/canonicalize-generated-source-inputs.mjs',
  'scripts/fetch-license-evidence.sh',
  'scripts/rebuild-from-package-source.sh',
  'src/license/license-evidence.json',
  'src/license/runtime-cve-scan-input.json',
  'source/upstream/dss-6.4-source.tar.gz',
  ...REQUIRED_BUTTER_SOURCES,
].sort(comparePaths));

export async function createCompleteSourceArtifact({
  baseDirectory,
  descriptorPath,
  licenseEvidenceDirectory,
  outputDirectory,
}) {
  const base = await canonicalDirectory(baseDirectory, 'source base directory');
  const evidence = await canonicalDirectory(licenseEvidenceDirectory, 'licence evidence directory');
  const output = resolve(outputDirectory);
  await mkdir(output, { recursive: true, mode: 0o700 });
  if ((await readdir(output)).length !== 0) throw new Error('Complete-source output directory must be empty');
  const staging = await mkdtemp(join(tmpdir(), 'bp-complete-source-'));
  try {
    await chmod(staging, 0o700);
    const rootDirectory = `${COMPLETE_SOURCE_DISTRIBUTION_ID}-${COMPLETE_SOURCE_ARTIFACT_VERSION}`;
    const stagingRoot = join(staging, rootDirectory);
    await mkdir(stagingRoot, { recursive: false, mode: 0o700 });
    const mappings = await sourceMappings(base, evidence);
    const files = [];
    for (const mapping of mappings.sort((left, right) => comparePaths(left.path, right.path))) {
      const source = await canonicalRegularFile(mapping.source, `complete-source input ${mapping.path}`);
      const destination = safeChild(stagingRoot, mapping.path);
      await mkdir(dirname(destination), { recursive: true, mode: 0o700 });
      await copyFile(source, destination);
      const mode = mapping.executable ? 0o755 : 0o644;
      await chmod(destination, mode);
      const bytes = await readFile(destination);
      files.push({
        bytes: bytes.byteLength,
        executable: mapping.executable,
        path: mapping.path,
        sha256: sha256(bytes),
      });
    }

    const identities = await sourceIdentities(stagingRoot, files);
    const manifest = {
      schemaVersion: COMPLETE_SOURCE_SCHEMA_VERSION,
      distributionId: COMPLETE_SOURCE_DISTRIBUTION_ID,
      product: COMPLETE_SOURCE_PRODUCT,
      core: COMPLETE_SOURCE_CORE,
      compatibleRuntimeTargets: [...COMPLETE_SOURCE_TARGETS],
      sourceIdentity: identities,
      legalApproval: false,
      releaseSealed: false,
      files,
    };
    const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
    const manifestSha256 = sha256(manifestBytes);
    await writeFile(join(stagingRoot, COMPLETE_SOURCE_MANIFEST_NAME), manifestBytes, { flag: 'wx', mode: 0o644 });
    const archiveExecutability = new Map(files.map((file) => [file.path, file.executable]));
    archiveExecutability.set(COMPLETE_SOURCE_MANIFEST_NAME, false);

    const temporaryArchive = join(output, '.complete-source.tar.gz.part');
    await assertAbsent(temporaryArchive, 'temporary complete-source archive');
    await writeCanonicalUstarGzip(stagingRoot, rootDirectory, temporaryArchive, archiveExecutability);
    const archiveInfo = await stat(temporaryArchive);
    if (!archiveInfo.isFile() || archiveInfo.size <= 0 || archiveInfo.size > MAX_COMPRESSED_BYTES) {
      throw new Error('Complete-source archive size is invalid');
    }
    const archiveSha256 = await sha256File(temporaryArchive);
    const artifactName = `${COMPLETE_SOURCE_DISTRIBUTION_ID}-${COMPLETE_SOURCE_ARTIFACT_VERSION}-${archiveSha256.slice(0, 16)}.tar.gz`;
    const artifactPath = join(output, artifactName);
    // Hardlinks are unavailable on Windows/Parallels shared paths; an exclusive copy still
    // prevents replacement and leaves the delivered artifact as a single-link regular file.
    await copyFile(temporaryArchive, artifactPath, fsConstants.COPYFILE_EXCL);
    await unlink(temporaryArchive);

    const descriptor = {
      schemaVersion: COMPLETE_SOURCE_SCHEMA_VERSION,
      distributionId: COMPLETE_SOURCE_DISTRIBUTION_ID,
      product: COMPLETE_SOURCE_PRODUCT,
      core: COMPLETE_SOURCE_CORE,
      delivery: {
        kind: 'distribution-wide-sibling',
        canonicalFileName: artifactName,
        distributionRelativePath: `pdf-signature-core/source/${artifactName}`,
        authenticationRequirement: 'same-release-signed-manifest-or-tuf-target',
        retentionRequirement: 'immutable-and-co-retained-for-corresponding-binary-lifetime',
        requiredForRedistribution: true,
        packageLocalSourcePresent: false,
      },
      artifact: {
        bytes: archiveInfo.size,
        format: 'tar.gz',
        rootDirectory,
        sha256: archiveSha256,
      },
      internalManifest: {
        path: COMPLETE_SOURCE_MANIFEST_NAME,
        sha256: manifestSha256,
      },
      runtimeNotices: files
        .filter((file) => file.path.startsWith('notices/dependencies/'))
        .map((file) => ({ bytes: file.bytes, path: file.path, sha256: file.sha256 })),
      sourceIdentity: identities,
      legalApproval: false,
      releaseSealed: false,
    };
    const parsedDescriptor = parseCompleteSourceDescriptor(descriptor);
    await writeFile(resolve(descriptorPath), `${JSON.stringify(parsedDescriptor, null, 2)}\n`, { flag: 'wx' });
    await inspectCompleteSourceArchive(artifactPath, parsedDescriptor);
    return { artifactPath, descriptor: parsedDescriptor, descriptorPath: resolve(descriptorPath), manifest };
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

export async function verifyAndExtractCompleteSourceArtifact({ artifactPath, descriptor, extractionRoot }) {
  const parsed = typeof descriptor === 'string'
    ? parseCompleteSourceDescriptor(JSON.parse(await readFile(descriptor, 'utf8')))
    : parseCompleteSourceDescriptor(descriptor);
  const inspected = await inspectCompleteSourceArchive(artifactPath, parsed);
  const destination = resolve(extractionRoot);
  await assertAbsent(destination, 'complete-source extraction root');
  await mkdir(destination, { recursive: false, mode: 0o700 });
  await chmod(destination, 0o700);
  try {
    await runTar(['-xzpf', resolve(artifactPath), '-C', destination]);
    const extractedRoot = join(destination, parsed.artifact.rootDirectory);
    await verifyExtractedTree(extractedRoot, inspected.manifest, inspected.internalManifestBytes);
    return { ...inspected, extractedRoot };
  } catch (error) {
    await rm(destination, { recursive: true, force: true });
    throw error;
  }
}

export async function inspectCompleteSourceArchive(artifactPath, descriptor) {
  const parsed = parseCompleteSourceDescriptor(descriptor);
  const archive = await canonicalRegularFile(artifactPath, 'complete-source artifact');
  const archiveInfo = await stat(archive);
  if (archiveInfo.size !== parsed.artifact.bytes || archiveInfo.size > MAX_COMPRESSED_BYTES) {
    throw new Error('Complete-source artifact length does not match its descriptor');
  }
  if (basename(archive) !== parsed.delivery.canonicalFileName) {
    throw new Error('Complete-source artifact filename is not the canonical descriptor filename');
  }
  if (await sha256File(archive) !== parsed.artifact.sha256) {
    throw new Error('Complete-source artifact SHA-256 does not match its descriptor');
  }
  const archiveEntries = await inspectUstarGzip(archive, parsed.artifact.rootDirectory);
  const manifestEntry = archiveEntries.files.get(`${parsed.artifact.rootDirectory}/${COMPLETE_SOURCE_MANIFEST_NAME}`);
  if (!manifestEntry || !manifestEntry.bytes || manifestEntry.bytes.byteLength > MAX_MANIFEST_BYTES) {
    throw new Error('Complete-source artifact internal manifest is missing or oversized');
  }
  if (sha256(manifestEntry.bytes) !== parsed.internalManifest.sha256) {
    throw new Error('Complete-source artifact internal manifest does not match its descriptor');
  }
  const manifest = parseCompleteSourceManifest(JSON.parse(manifestEntry.bytes.toString('utf8')));
  if (JSON.stringify(manifest.sourceIdentity) !== JSON.stringify(parsed.sourceIdentity)) {
    throw new Error('Complete-source manifest source identity does not match its runtime descriptor');
  }
  const expectedFiles = new Map(manifest.files.map((file) => [
    `${parsed.artifact.rootDirectory}/${file.path}`,
    file,
  ]));
  expectedFiles.set(`${parsed.artifact.rootDirectory}/${COMPLETE_SOURCE_MANIFEST_NAME}`, {
    bytes: manifestEntry.bytes.byteLength,
    executable: false,
    path: COMPLETE_SOURCE_MANIFEST_NAME,
    sha256: parsed.internalManifest.sha256,
  });
  if (archiveEntries.files.size !== expectedFiles.size) {
    throw new Error('Complete-source artifact has an extra or missing file');
  }
  for (const [path, expected] of expectedFiles) {
    const actual = archiveEntries.files.get(path);
    if (!actual
      || actual.size !== expected.bytes
      || actual.sha256 !== expected.sha256
      || actual.executable !== expected.executable
      || actual.mode !== (expected.executable ? 0o755 : 0o644)) {
      throw new Error(`Complete-source artifact file differs from its manifest: ${path}`);
    }
  }
  const expectedDirectories = new Set([parsed.artifact.rootDirectory]);
  for (const path of expectedFiles.keys()) {
    let parent = posix.dirname(path);
    while (parent !== '.') {
      expectedDirectories.add(parent);
      parent = posix.dirname(parent);
    }
  }
  for (const path of archiveEntries.directories) {
    if (!expectedDirectories.has(path)) throw new Error(`Complete-source artifact has an extra directory: ${path}`);
  }
  verifySourceEvidenceInventory(archiveEntries, parsed, manifest);
  return { archivePath: archive, descriptor: parsed, manifest, internalManifestBytes: manifestEntry.bytes };
}

function verifySourceEvidenceInventory(archiveEntries, descriptor, manifest) {
  const root = descriptor.artifact.rootDirectory;
  const capturedJson = (path, label) => {
    const entry = archiveEntries.files.get(`${root}/${path}`);
    if (!entry?.bytes || entry.bytes.byteLength > MAX_MANIFEST_BYTES) {
      throw new Error(`Complete-source ${label} is missing or oversized`);
    }
    try {
      return JSON.parse(entry.bytes.toString('utf8'));
    } catch (error) {
      throw new Error(`Complete-source ${label} is not valid JSON`, { cause: error });
    }
  };
  const manifestFiles = new Map(manifest.files.map((file) => [file.path, file]));
  for (const [path, hash] of [
    ['src/license/license-evidence.json', descriptor.sourceIdentity.policySha256],
    ['generated/pdf-signature-core.cdx.json', descriptor.sourceIdentity.sbomSha256],
    ['generated/dependency-inventory.json', descriptor.sourceIdentity.dependencyInventorySha256],
  ]) {
    if (manifestFiles.get(path)?.sha256 !== hash) {
      throw new Error(`Complete-source descriptor does not bind ${path}`);
    }
  }
  const policy = capturedJson('src/license/license-evidence.json', 'source policy');
  const inventory = capturedJson('generated/dependency-inventory.json', 'dependency inventory');
  const sbom = capturedJson('generated/pdf-signature-core.cdx.json', 'SBOM');
  const runtimeScanInput = capturedJson('src/license/runtime-cve-scan-input.json', 'runtime CVE scan input');
  validateGeneratedSbomForCveScanning(sbom);
  validateRuntimeCveScanInput(runtimeScanInput);
  if (inventory?.legalApproval !== false || !Array.isArray(inventory.components)
    || !Array.isArray(inventory.correspondingSources) || inventory.correspondingSources.length !== 1) {
    throw new Error('Complete-source dependency inventory is invalid or overstates legal approval');
  }
  validateSbomInventoryBinding(sbom, inventory);
  const source = inventory.correspondingSources[0];
  if (source?.evidenceFile !== 'source/upstream/dss-6.4-source.tar.gz'
    || source.evidenceSha256 !== descriptor.sourceIdentity.dss.sha256
    || source.bytes !== descriptor.sourceIdentity.dss.bytes
    || source.resolvedCommit !== descriptor.sourceIdentity.dss.resolvedCommit
    || manifestFiles.get(source.evidenceFile)?.sha256 !== source.evidenceSha256) {
    throw new Error('Complete-source dependency inventory does not bind the present DSS source');
  }
  const policySource = policy?.correspondingSources?.[0];
  if (policySource?.packagePath !== source.evidenceFile
    || policySource.sha256 !== source.evidenceSha256
    || policySource.bytes !== source.bytes
    || policySource.resolvedCommit !== source.resolvedCommit) {
    throw new Error('Complete-source policy and dependency inventory source identities differ');
  }
  const referencedNotices = new Map();
  for (const component of inventory.components) {
    if (!component || typeof component !== 'object'
      || typeof component.evidenceFile !== 'string' || typeof component.evidenceSha256 !== 'string'
      || !Array.isArray(component.retainedJarNotices)) {
      throw new Error('Complete-source dependency inventory contains a malformed component');
    }
    referencedNotices.set(`notices/dependencies/${component.evidenceFile}`, component.evidenceSha256);
    for (const notice of component.retainedJarNotices) {
      if (!notice || typeof notice.path !== 'string' || typeof notice.sha256 !== 'string') {
        throw new Error('Complete-source dependency inventory contains a malformed retained notice');
      }
      referencedNotices.set(`notices/dependencies/jar-notices/${notice.path}`, notice.sha256);
    }
  }
  const descriptorNotices = new Map(descriptor.runtimeNotices.map((notice) => [notice.path, notice]));
  if (descriptorNotices.size !== referencedNotices.size) {
    throw new Error('Complete-source runtime notice descriptor does not match the dependency inventory');
  }
  for (const [path, hash] of referencedNotices) {
    const file = manifestFiles.get(path);
    const runtimeNotice = descriptorNotices.get(path);
    if (!file || file.sha256 !== hash || !runtimeNotice
      || runtimeNotice.sha256 !== hash || runtimeNotice.bytes !== file.bytes) {
      throw new Error(`Complete-source dependency evidence differs from its inventory: ${path}`);
    }
  }
}

const RUNTIME_CVE_SCAN_TARGETS = Object.freeze({
  'darwin-arm64': Object.freeze({
    archiveUrl: 'https://aka.ms/download-jdk/microsoft-jdk-21.0.12-macos-aarch64.tar.gz',
    archiveSha256: '56f338a3c6af071649047e78f2d31a2ca97c57be6c1b2c213823acb016463ee1',
  }),
  'darwin-x64': Object.freeze({
    archiveUrl: 'https://aka.ms/download-jdk/microsoft-jdk-21.0.12-macos-x64.tar.gz',
    archiveSha256: '9b06efd99de047f2aba42ea0804854e51b69ed633e995730aef049adac62cb98',
  }),
  'linux-arm64': Object.freeze({
    archiveUrl: 'https://aka.ms/download-jdk/microsoft-jdk-21.0.12-linux-aarch64.tar.gz',
    archiveSha256: 'c61cadbc8ad4f950131dc260f0cdfd8d4d1f200fb16f8ac6a2611f17a77ab301',
  }),
  'linux-x64': Object.freeze({
    archiveUrl: 'https://aka.ms/download-jdk/microsoft-jdk-21.0.12-linux-x64.tar.gz',
    archiveSha256: 'f2a84ad31ebeaf3a26252dd86a4a8e1b74aefb6bfc8e55fd20190110d1353c0f',
  }),
  'win32-arm64': Object.freeze({
    archiveUrl: 'https://aka.ms/download-jdk/microsoft-jdk-21.0.12-windows-aarch64.zip',
    archiveSha256: '2118bb60b19002a0bcc420267518352f10d2be25ce1c79c51701b87b209bbc2a',
  }),
  'win32-x64': Object.freeze({
    archiveUrl: 'https://aka.ms/download-jdk/microsoft-jdk-21.0.12-windows-x64.zip',
    archiveSha256: 'bf27a5d6298c736af8daf5b8c883098e83291446e5766118d8a5ea6a2617195d',
  }),
});

export function validateRuntimeCveScanInput(value) {
  exactKeys(value, [
    'applicationSbom', 'component', 'inputType', 'legalApproval', 'scanStatus', 'schemaVersion', 'targets',
  ], 'runtime CVE scan input');
  if (value.schemaVersion !== 1
    || value.inputType !== 'butter-paper/runtime-cve-scan-input'
    || value.scanStatus !== 'not-run'
    || value.legalApproval !== false) {
    throw new Error('Runtime CVE scan input identity or claim is invalid');
  }
  exactKeys(value.component, ['javaFeature', 'product', 'runtimeImage', 'vendor', 'version'], 'runtime CVE component');
  if (value.component.vendor !== 'Microsoft'
    || value.component.product !== 'Microsoft Build of OpenJDK'
    || value.component.version !== '21.0.12+8-LTS'
    || value.component.javaFeature !== 21
    || value.component.runtimeImage !== 'jlink') {
    throw new Error('Runtime CVE component identity is invalid');
  }
  exactKeys(value.applicationSbom, [
    'completeSourcePath', 'format', 'runtimePackagePath', 'specVersion',
  ], 'runtime CVE application SBOM');
  if (value.applicationSbom.format !== 'CycloneDX JSON'
    || value.applicationSbom.specVersion !== '1.6'
    || value.applicationSbom.runtimePackagePath !== 'sbom/pdf-signature-core.cdx.json'
    || value.applicationSbom.completeSourcePath !== 'generated/pdf-signature-core.cdx.json') {
    throw new Error('Runtime CVE application SBOM reference is invalid');
  }
  if (!Array.isArray(value.targets)
    || value.targets.length !== COMPLETE_SOURCE_TARGETS.length
    || value.targets.map((target) => target?.target).join('\n') !== COMPLETE_SOURCE_TARGETS.join('\n')) {
    throw new Error('Runtime CVE scan input target set is not the exact six-target matrix');
  }
  for (const target of value.targets) {
    exactKeys(target, ['archiveSha256', 'archiveUrl', 'target'], `runtime CVE target ${target?.target ?? 'unknown'}`);
    const expected = RUNTIME_CVE_SCAN_TARGETS[target.target];
    if (!expected || target.archiveUrl !== expected.archiveUrl || target.archiveSha256 !== expected.archiveSha256) {
      throw new Error(`Runtime CVE scan input archive identity is invalid: ${target.target}`);
    }
  }
  return value;
}

function validateSbomInventoryBinding(sbom, inventory) {
  validateGeneratedSbomForCveScanning(sbom);
  if (!Array.isArray(inventory?.components) || inventory.components.length !== sbom.components.length) {
    throw new Error('Complete-source SBOM and dependency inventory component counts differ');
  }
  const sbomByCoordinate = new Map();
  for (const component of sbom.components) {
    const coordinate = `${component.group}:${component.name}:${component.version}`;
    const sha256Hash = component.hashes.find((hash) => hash.alg === 'SHA-256').content;
    if (sbomByCoordinate.has(coordinate)) throw new Error(`Complete-source SBOM repeats component ${coordinate}`);
    sbomByCoordinate.set(coordinate, sha256Hash);
  }
  const inventoryCoordinates = new Set();
  for (const component of inventory.components) {
    if (typeof component?.coordinate !== 'string'
      || !isHash(component?.jarSha256)
      || inventoryCoordinates.has(component.coordinate)
      || sbomByCoordinate.get(component.coordinate) !== component.jarSha256) {
      throw new Error(`Complete-source SBOM does not bind dependency inventory component ${component?.coordinate ?? 'unknown'}`);
    }
    inventoryCoordinates.add(component.coordinate);
  }
  if (inventoryCoordinates.size !== sbomByCoordinate.size
    || [...sbomByCoordinate.keys()].some((coordinate) => !inventoryCoordinates.has(coordinate))) {
    throw new Error('Complete-source SBOM and dependency inventory component sets differ');
  }
}

export function parseCompleteSourceDescriptor(value) {
  exactKeys(value, [
    'artifact', 'core', 'delivery', 'distributionId', 'internalManifest', 'legalApproval',
    'product', 'releaseSealed', 'runtimeNotices', 'schemaVersion', 'sourceIdentity',
  ], 'complete-source descriptor');
  if (value.schemaVersion !== 1
    || value.distributionId !== COMPLETE_SOURCE_DISTRIBUTION_ID
    || value.product !== COMPLETE_SOURCE_PRODUCT
    || value.legalApproval !== false
    || value.releaseSealed !== false) {
    throw new Error('Complete-source descriptor identity or claim is invalid');
  }
  exactCore(value.core, 'complete-source descriptor');
  exactKeys(value.delivery, [
    'authenticationRequirement', 'canonicalFileName', 'distributionRelativePath', 'kind',
    'packageLocalSourcePresent', 'requiredForRedistribution', 'retentionRequirement',
  ], 'complete-source delivery');
  if (value.delivery.kind !== 'distribution-wide-sibling'
    || value.delivery.authenticationRequirement !== 'same-release-signed-manifest-or-tuf-target'
    || value.delivery.retentionRequirement !== 'immutable-and-co-retained-for-corresponding-binary-lifetime'
    || value.delivery.requiredForRedistribution !== true
    || value.delivery.packageLocalSourcePresent !== false
    || typeof value.delivery.canonicalFileName !== 'string'
    || !/^butter-paper-pdf-signature-core-complete-source-v1-0\.1\.0-[a-f0-9]{16}\.tar\.gz$/.test(value.delivery.canonicalFileName)
    || value.delivery.distributionRelativePath !== `pdf-signature-core/source/${value.delivery.canonicalFileName}`) {
    throw new Error('Complete-source sibling delivery contract is invalid');
  }
  exactKeys(value.artifact, ['bytes', 'format', 'rootDirectory', 'sha256'], 'complete-source artifact');
  if (!Number.isSafeInteger(value.artifact.bytes) || value.artifact.bytes <= 0 || value.artifact.bytes > MAX_COMPRESSED_BYTES
    || value.artifact.format !== 'tar.gz'
    || value.artifact.rootDirectory !== `${COMPLETE_SOURCE_DISTRIBUTION_ID}-${COMPLETE_SOURCE_ARTIFACT_VERSION}`
    || !isHash(value.artifact.sha256)) {
    throw new Error('Complete-source artifact descriptor is invalid');
  }
  exactKeys(value.internalManifest, ['path', 'sha256'], 'complete-source internal manifest');
  if (value.internalManifest.path !== COMPLETE_SOURCE_MANIFEST_NAME || !isHash(value.internalManifest.sha256)) {
    throw new Error('Complete-source internal manifest descriptor is invalid');
  }
  return {
    schemaVersion: 1,
    distributionId: COMPLETE_SOURCE_DISTRIBUTION_ID,
    product: COMPLETE_SOURCE_PRODUCT,
    core: { ...COMPLETE_SOURCE_CORE },
    delivery: { ...value.delivery },
    artifact: { ...value.artifact },
    internalManifest: { ...value.internalManifest },
    runtimeNotices: parseRuntimeNotices(value.runtimeNotices),
    sourceIdentity: parseSourceIdentity(value.sourceIdentity),
    legalApproval: false,
    releaseSealed: false,
  };
}

function parseRuntimeNotices(value) {
  if (!Array.isArray(value) || value.length === 0 || value.length > MAX_ENTRIES) {
    throw new Error('Complete-source runtime notice inventory is invalid');
  }
  const notices = value.map((notice, index) => {
    exactKeys(notice, ['bytes', 'path', 'sha256'], `runtime notice ${index}`);
    const path = safeArchivePath(notice.path, false);
    if (!path.startsWith('notices/dependencies/')
      || !Number.isSafeInteger(notice.bytes) || notice.bytes < 0 || notice.bytes > MAX_ENTRY_BYTES
      || !isHash(notice.sha256)) {
      throw new Error(`Complete-source runtime notice is invalid: ${path}`);
    }
    return { bytes: notice.bytes, path, sha256: notice.sha256 };
  });
  if (notices.some((notice, index) => index > 0 && comparePaths(notices[index - 1].path, notice.path) >= 0)) {
    throw new Error('Complete-source runtime notices must be uniquely sorted');
  }
  return notices;
}

export function parseCompleteSourceManifest(value) {
  exactKeys(value, [
    'compatibleRuntimeTargets', 'core', 'distributionId', 'files', 'legalApproval', 'product',
    'releaseSealed', 'schemaVersion', 'sourceIdentity',
  ], 'complete-source manifest');
  if (value.schemaVersion !== 1
    || value.distributionId !== COMPLETE_SOURCE_DISTRIBUTION_ID
    || value.product !== COMPLETE_SOURCE_PRODUCT
    || value.legalApproval !== false
    || value.releaseSealed !== false) {
    throw new Error('Complete-source manifest identity or claim is invalid');
  }
  exactCore(value.core, 'complete-source manifest');
  if (!Array.isArray(value.compatibleRuntimeTargets)
    || value.compatibleRuntimeTargets.join('\n') !== COMPLETE_SOURCE_TARGETS.join('\n')) {
    throw new Error('Complete-source manifest runtime targets are invalid');
  }
  if (!Array.isArray(value.files) || value.files.length === 0 || value.files.length > MAX_ENTRIES) {
    throw new Error('Complete-source manifest file list is invalid');
  }
  const folded = new Set();
  const files = value.files.map((file, index) => {
    exactKeys(file, ['bytes', 'executable', 'path', 'sha256'], `complete-source file ${index}`);
    const path = safeArchivePath(file.path, false);
    const caseFolded = path.toLowerCase();
    if (folded.has(caseFolded)) throw new Error('Complete-source manifest has a case-fold path collision');
    folded.add(caseFolded);
    if (!Number.isSafeInteger(file.bytes) || file.bytes < 0 || file.bytes > MAX_ENTRY_BYTES
      || typeof file.executable !== 'boolean' || !isHash(file.sha256)) {
      throw new Error(`Complete-source manifest file is invalid: ${path}`);
    }
    return { path, bytes: file.bytes, executable: file.executable, sha256: file.sha256 };
  });
  if (files.some((file, index) => index > 0 && comparePaths(files[index - 1].path, file.path) >= 0)) {
    throw new Error('Complete-source manifest files must be uniquely sorted');
  }
  for (const required of REQUIRED_SOURCE_PATHS) {
    if (!files.some((file) => file.path === required)) {
      throw new Error(`Complete-source manifest is missing required file: ${required}`);
    }
  }
  const butterSources = files.filter((file) => file.path.startsWith('src/main/java/')).map((file) => file.path);
  if (butterSources.join('\n') !== REQUIRED_BUTTER_SOURCES.join('\n')) {
    throw new Error('Complete-source manifest Butter source tree is not the exact reviewed tree');
  }
  return {
    schemaVersion: 1,
    distributionId: COMPLETE_SOURCE_DISTRIBUTION_ID,
    product: COMPLETE_SOURCE_PRODUCT,
    core: { ...COMPLETE_SOURCE_CORE },
    compatibleRuntimeTargets: [...COMPLETE_SOURCE_TARGETS],
    sourceIdentity: parseSourceIdentity(value.sourceIdentity),
    legalApproval: false,
    releaseSealed: false,
    files,
  };
}

async function sourceMappings(base, evidence) {
  const mappings = [
    ['.mvn/jvm.config', join(base, '.mvn/jvm.config')],
    ['.mvn/maven.config', join(base, '.mvn/maven.config')],
    ['generated/dependency-inventory.json', join(evidence, 'inventory.json')],
    ['generated/pdf-signature-core-sources.jar', join(base, 'target/pdf-signature-core-sources.jar')],
    ['generated/pdf-signature-core.cdx.json', join(base, 'target/pdf-signature-core.cdx.json')],
    ['mvnw', join(base, 'mvnw'), true],
    ['pom.xml', join(base, 'pom.xml')],
    ['scripts/canonicalize-generated-source-inputs.mjs', join(base, 'scripts/canonicalize-generated-source-inputs.mjs'), true],
    ['scripts/fetch-license-evidence.sh', join(base, 'scripts/fetch-license-evidence.sh'), true],
    ['scripts/rebuild-from-package-source.sh', join(base, 'scripts/rebuild-from-package-source.sh'), true],
    ['src/license/license-evidence.json', join(base, 'src/license/license-evidence.json')],
    ['src/license/runtime-cve-scan-input.json', join(base, 'src/license/runtime-cve-scan-input.json')],
    ['source/upstream/dss-6.4-source.tar.gz', join(evidence, 'sources/dss-6.4-source.tar.gz')],
  ];
  await addTreeMappings(mappings, join(base, 'src/main/java'), 'src/main/java');
  await addTreeMappings(mappings, join(base, 'notices'), 'notices');
  await addTreeMappings(mappings, join(evidence, 'licenses'), 'notices/dependencies/licenses');
  await addTreeMappings(mappings, join(evidence, 'jar-notices'), 'notices/dependencies/jar-notices');
  return mappings.map(([path, source, executable = false]) => ({ path, source, executable }));
}

async function addTreeMappings(mappings, sourceRoot, destinationRoot) {
  async function visit(directory, relativePath = '') {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries.sort((left, right) => comparePaths(left.name, right.name))) {
      const child = join(directory, entry.name);
      const childRelative = relativePath ? `${relativePath}/${entry.name}` : entry.name;
      if (entry.isSymbolicLink()) throw new Error(`Complete-source input contains a symlink: ${child}`);
      if (entry.isDirectory()) await visit(child, childRelative);
      else if (entry.isFile()) mappings.push([`${destinationRoot}/${childRelative}`, child, false]);
      else throw new Error(`Complete-source input contains a non-regular entry: ${child}`);
    }
  }
  await visit(sourceRoot);
}

async function sourceIdentities(root, files) {
  const policy = JSON.parse(await readFile(join(root, 'src/license/license-evidence.json'), 'utf8'));
  const inventory = JSON.parse(await readFile(join(root, 'generated/dependency-inventory.json'), 'utf8'));
  const sbom = JSON.parse(await readFile(join(root, 'generated/pdf-signature-core.cdx.json'), 'utf8'));
  const runtimeScanInput = JSON.parse(await readFile(join(root, 'src/license/runtime-cve-scan-input.json'), 'utf8'));
  const dss = policy?.correspondingSources?.[0];
  if (!dss || dss.version !== '6.4' || !Number.isSafeInteger(dss.bytes) || !isHash(dss.sha256)
    || typeof dss.resolvedCommit !== 'string' || !/^[a-f0-9]{40}$/.test(dss.resolvedCommit)) {
    throw new Error('Complete-source DSS policy identity is invalid');
  }
  if (inventory?.legalApproval !== false) throw new Error('Complete-source dependency inventory must keep legalApproval false');
  validateGeneratedSbomForCveScanning(sbom);
  validateRuntimeCveScanInput(runtimeScanInput);
  validateSbomInventoryBinding(sbom, inventory);
  const sourceFiles = files.filter((file) => file.path.startsWith('src/main/java/'));
  const tree = createHash('sha256');
  for (const file of sourceFiles) {
    const relativePath = file.path.slice('src/main/java/'.length);
    const bytes = await readFile(join(root, file.path));
    tree.update(`${Buffer.byteLength(relativePath)}:`);
    tree.update(relativePath);
    tree.update(`\0${bytes.byteLength}:`);
    tree.update(bytes);
    tree.update('\0');
  }
  return {
    butter: {
      algorithm: 'sha256-path-length-path-nul-byte-length-bytes-nul-v1',
      fileCount: sourceFiles.length,
      sha256: tree.digest('hex'),
    },
    dss: {
      bytes: dss.bytes,
      resolvedCommit: dss.resolvedCommit,
      sha256: dss.sha256,
      version: '6.4',
    },
    policySha256: fileHash(files, 'src/license/license-evidence.json'),
    sbomSha256: fileHash(files, 'generated/pdf-signature-core.cdx.json'),
    dependencyInventorySha256: fileHash(files, 'generated/dependency-inventory.json'),
  };
}

function parseSourceIdentity(value) {
  exactKeys(value, ['butter', 'dependencyInventorySha256', 'dss', 'policySha256', 'sbomSha256'], 'source identity');
  exactKeys(value.butter, ['algorithm', 'fileCount', 'sha256'], 'Butter source identity');
  if (value.butter.algorithm !== 'sha256-path-length-path-nul-byte-length-bytes-nul-v1'
    || !Number.isSafeInteger(value.butter.fileCount) || value.butter.fileCount !== REQUIRED_BUTTER_SOURCES.length
    || !isHash(value.butter.sha256)) {
    throw new Error('Butter source identity is invalid');
  }
  exactKeys(value.dss, ['bytes', 'resolvedCommit', 'sha256', 'version'], 'DSS source identity');
  if (value.dss.version !== '6.4' || !Number.isSafeInteger(value.dss.bytes) || value.dss.bytes <= 0
    || typeof value.dss.resolvedCommit !== 'string' || !/^[a-f0-9]{40}$/.test(value.dss.resolvedCommit)
    || !isHash(value.dss.sha256)) {
    throw new Error('DSS source identity is invalid');
  }
  for (const field of ['dependencyInventorySha256', 'policySha256', 'sbomSha256']) {
    if (!isHash(value[field])) throw new Error(`Source identity ${field} is invalid`);
  }
  return JSON.parse(JSON.stringify(value));
}

async function inspectUstarGzip(path, expectedRoot) {
  const gunzip = createGunzip();
  const input = createReadStream(path);
  input.pipe(gunzip);
  let buffered = Buffer.alloc(0);
  let uncompressedBytes = 0;
  let zeroBlocks = 0;
  let current = null;
  const files = new Map();
  const directories = new Set();
  const folded = new Map();

  for await (const chunk of gunzip) {
    uncompressedBytes += chunk.byteLength;
    if (uncompressedBytes > MAX_UNCOMPRESSED_BYTES) throw new Error('Complete-source archive exceeds the uncompressed size limit');
    buffered = Buffer.concat([buffered, chunk]);
    while (true) {
      if (current) {
        if (current.remaining > 0) {
          if (buffered.length === 0) break;
          const length = Math.min(buffered.length, current.remaining);
          const data = buffered.subarray(0, length);
          current.hash.update(data);
          if (current.capture) current.chunks.push(Buffer.from(data));
          buffered = buffered.subarray(length);
          current.remaining -= length;
          if (current.remaining > 0) continue;
        }
        if (buffered.length < current.padding) break;
        if (current.padding > 0 && !buffered.subarray(0, current.padding).every((byte) => byte === 0)) {
          throw new Error('Complete-source archive has non-zero TAR padding');
        }
        buffered = buffered.subarray(current.padding);
        files.set(current.path, {
          bytes: current.capture ? Buffer.concat(current.chunks) : undefined,
          executable: (current.mode & 0o111) !== 0,
          mode: current.mode,
          sha256: current.hash.digest('hex'),
          size: current.size,
        });
        current = null;
        continue;
      }
      if (buffered.length < 512) break;
      const header = buffered.subarray(0, 512);
      buffered = buffered.subarray(512);
      if (header.every((byte) => byte === 0)) {
        zeroBlocks += 1;
        continue;
      }
      if (zeroBlocks > 0) throw new Error('Complete-source archive has data after its TAR terminator');
      validateTarChecksum(header);
      const path = tarPath(header);
      const type = String.fromCharCode(header[156] || 0x30);
      const directory = type === '5';
      if (type !== '0' && type !== '\0' && !directory) {
        throw new Error(`Complete-source archive contains a forbidden TAR entry type ${JSON.stringify(type)}`);
      }
      const normalized = safeArchivePath(path, directory);
      if (normalized !== expectedRoot && !normalized.startsWith(`${expectedRoot}/`)) {
        throw new Error('Complete-source archive entry is outside its canonical root');
      }
      const foldedPath = normalized.toLowerCase();
      if (folded.has(foldedPath)) throw new Error('Complete-source archive has a duplicate or case-fold path collision');
      folded.set(foldedPath, normalized);
      if (files.size + directories.size >= MAX_ENTRIES) throw new Error('Complete-source archive has too many entries');
      const size = parseTarOctal(header, 124, 12, 'size');
      const mode = parseTarOctal(header, 100, 8, 'mode');
      if (directory) {
        if (size !== 0 || mode !== 0o755) throw new Error('Complete-source archive directory has invalid data or mode');
        directories.add(normalized);
        continue;
      }
      if (size > MAX_ENTRY_BYTES) throw new Error('Complete-source archive entry exceeds the per-file size limit');
      if (mode !== 0o644 && mode !== 0o755) {
        throw new Error('Complete-source archive file mode is not exactly 0644 or 0755');
      }
      const capture = normalized === `${expectedRoot}/${COMPLETE_SOURCE_MANIFEST_NAME}`
        || normalized === `${expectedRoot}/generated/dependency-inventory.json`
        || normalized === `${expectedRoot}/generated/pdf-signature-core.cdx.json`
        || normalized === `${expectedRoot}/src/license/license-evidence.json`
        || normalized === `${expectedRoot}/src/license/runtime-cve-scan-input.json`;
      if (capture && size > MAX_MANIFEST_BYTES) throw new Error('Complete-source JSON evidence exceeds the size limit');
      current = {
        path: normalized,
        size,
        mode,
        remaining: size,
        padding: (512 - (size % 512)) % 512,
        capture,
        chunks: [],
        hash: createHash('sha256'),
      };
    }
  }
  if (current || zeroBlocks < 2 || buffered.some((byte) => byte !== 0)) {
    throw new Error('Complete-source archive is truncated or lacks a canonical TAR terminator');
  }
  return { files, directories };
}

async function verifyExtractedTree(root, manifest, internalManifestBytes) {
  const canonical = await canonicalDirectory(root, 'extracted complete-source root');
  const rootInfo = await lstat(canonical);
  if ((rootInfo.mode & 0o777) !== 0o700) await chmod(canonical, 0o700);
  const expected = new Map(manifest.files.map((file) => [file.path, file]));
  expected.set(COMPLETE_SOURCE_MANIFEST_NAME, {
    bytes: internalManifestBytes.byteLength,
    executable: false,
    sha256: sha256(internalManifestBytes),
  });
  const actual = [];
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const child = join(directory, entry.name);
      const childRelative = relative(canonical, child).split(sep).join('/');
      const info = await lstat(child);
      if (entry.isSymbolicLink()) throw new Error(`Extracted complete-source tree contains a symlink: ${childRelative}`);
      if (entry.isDirectory()) await visit(child);
      else if (entry.isFile()) {
        if (info.nlink !== 1) throw new Error(`Extracted complete-source tree contains a hardlink: ${childRelative}`);
        actual.push(childRelative);
      }
      else throw new Error(`Extracted complete-source tree contains a non-regular entry: ${childRelative}`);
    }
  }
  await visit(canonical);
  if (actual.sort(comparePaths).join('\n') !== [...expected.keys()].sort(comparePaths).join('\n')) {
    throw new Error('Extracted complete-source tree differs from its exact manifest');
  }
  for (const [path, expectedFile] of expected) {
    const child = safeChild(canonical, path);
    const info = await lstat(child);
    if (info.size !== expectedFile.bytes || await sha256File(child) !== expectedFile.sha256
      || !isExtractedModeAcceptable(info.mode, expectedFile.executable)) {
      throw new Error(`Extracted complete-source file differs from its manifest: ${path}`);
    }
  }
}

export function isExtractedModeAcceptable(mode, executable, platform = process.platform) {
  return platform === 'win32' || (mode & 0o777) === (executable ? 0o755 : 0o644);
}

function tarPath(header) {
  const name = tarText(header, 0, 100);
  const prefix = tarText(header, 345, 155);
  return prefix ? `${prefix}/${name}` : name;
}

function tarText(header, offset, length) {
  const field = header.subarray(offset, offset + length);
  const nul = field.indexOf(0);
  const bytes = nul === -1 ? field : field.subarray(0, nul);
  if (bytes.some((byte) => byte < 0x20 || byte > 0x7e)) throw new Error('Complete-source archive has a non-portable TAR path');
  return bytes.toString('ascii');
}

function parseTarOctal(header, offset, length, label) {
  const field = header.subarray(offset, offset + length);
  if ((field[0] & 0x80) !== 0) throw new Error(`Complete-source archive uses a non-portable TAR ${label}`);
  const text = field.toString('ascii').replace(/\0.*$/, '').trim();
  if (!/^[0-7]+$/.test(text)) throw new Error(`Complete-source archive has an invalid TAR ${label}`);
  const value = Number.parseInt(text, 8);
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`Complete-source archive TAR ${label} is out of range`);
  return value;
}

function validateTarChecksum(header) {
  const expected = parseTarOctal(header, 148, 8, 'checksum');
  let actual = 0;
  for (let index = 0; index < header.length; index += 1) {
    actual += index >= 148 && index < 156 ? 0x20 : header[index];
  }
  if (actual !== expected) throw new Error('Complete-source archive TAR header checksum is invalid');
}

function safeArchivePath(value, directory) {
  if (typeof value !== 'string' || value.length === 0 || value.length > 255
    || value.includes('\\') || value.includes('\0') || posix.isAbsolute(value)) {
    throw new Error('Complete-source archive path is not a portable relative path');
  }
  const path = directory && value.endsWith('/') ? value.slice(0, -1) : value;
  const segments = path.split('/');
  if (segments.some((segment) => segment === '' || segment === '.' || segment === '..')
    || posix.normalize(path) !== path) {
    throw new Error('Complete-source archive path is empty, dotted, traversing, or non-normalized');
  }
  return path;
}

async function runTar(args) {
  const normalizedArgs = normalizeTarArgumentsForHost(args);
  await new Promise((resolvePromise, rejectPromise) => {
    const child = spawn('tar', normalizedArgs, {
      shell: false,
      windowsHide: true,
      stdio: ['ignore', 'ignore', 'pipe'],
      env: {
        ...process.env,
        COPYFILE_DISABLE: '1',
        COPY_EXTENDED_ATTRIBUTES_DISABLE: '1',
      },
    });
    let stderr = '';
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => { stderr = `${stderr}${chunk}`.slice(-4096); });
    child.once('error', rejectPromise);
    child.once('close', (code) => {
      if (code === 0) resolvePromise();
      else rejectPromise(new Error(`tar failed${stderr.trim() ? `: ${stderr.trim()}` : ` with exit code ${code}`}`));
    });
  });
}

export function normalizeTarArgumentsForHost(
  args,
  { platform = process.platform, msystem = process.env.MSYSTEM } = {},
) {
  if (platform !== 'win32' || !msystem) return [...args];
  return args.map((argument) => {
    const match = /^([a-zA-Z]):[\\/](.*)$/.exec(argument);
    if (!match) return argument;
    return `/${match[1].toLowerCase()}/${match[2].replaceAll('\\', '/')}`;
  });
}

async function writeCanonicalUstarGzip(sourceRoot, archiveRoot, outputPath, archiveExecutability) {
  const entries = [];
  async function visit(directory, relativePath = '') {
    for (const entry of (await readdir(directory, { withFileTypes: true }))
      .sort((left, right) => comparePaths(left.name, right.name))) {
      const child = join(directory, entry.name);
      const childRelative = relativePath ? `${relativePath}/${entry.name}` : entry.name;
      const info = await lstat(child);
      if (entry.isSymbolicLink() || (!entry.isDirectory() && info.nlink !== 1)) {
        throw new Error(`Complete-source staging tree contains a link: ${childRelative}`);
      }
      if (entry.isDirectory()) await visit(child, childRelative);
      else if (entry.isFile()) {
        if (!archiveExecutability.has(childRelative)) {
          throw new Error(`Complete-source staging tree contains an undeclared file: ${childRelative}`);
        }
        entries.push({
          absolutePath: child,
          executable: archiveExecutability.get(childRelative),
          path: `${archiveRoot}/${childRelative}`,
          info,
        });
      }
      else throw new Error(`Complete-source staging tree contains a non-regular entry: ${childRelative}`);
    }
  }
  await visit(sourceRoot);
  if (entries.length !== archiveExecutability.size) {
    throw new Error('Complete-source staging tree is missing a declared file');
  }
  entries.sort((left, right) => comparePaths(left.path, right.path));
  const output = createWriteStream(outputPath, { flags: 'wx', mode: 0o600 });
  const gzip = createGzip({
    level: 9,
    mtime: 0,
    strategy: zlibConstants.Z_DEFAULT_STRATEGY,
  });
  gzip.pipe(output);
  const outputFinished = once(output, 'finish');
  const outputError = once(output, 'error').then(([error]) => { throw error; });
  const gzipError = once(gzip, 'error').then(([error]) => { throw error; });
  try {
    for (const entry of entries) {
      await writeStreamChunk(gzip, ustarHeader(entry.path, entry.info.size, entry.executable));
      for await (const chunk of createReadStream(entry.absolutePath)) await writeStreamChunk(gzip, chunk);
      const padding = (512 - (entry.info.size % 512)) % 512;
      if (padding > 0) await writeStreamChunk(gzip, Buffer.alloc(padding));
    }
    await writeStreamChunk(gzip, Buffer.alloc(1024));
    gzip.end();
    await Promise.race([outputFinished, outputError, gzipError]);
  } catch (error) {
    gzip.destroy();
    output.destroy();
    await rm(outputPath, { force: true });
    throw error;
  }
  const handle = await open(outputPath, 'r+');
  try {
    const header = Buffer.alloc(10);
    const { bytesRead } = await handle.read(header, 0, header.length, 0);
    if (bytesRead !== header.length || header[0] !== 0x1f || header[1] !== 0x8b || header[2] !== 0x08) {
      throw new Error('Canonical complete-source gzip header is invalid');
    }
    header.fill(0, 4, 8);
    header[9] = 0xff;
    await handle.write(header, 0, header.length, 0);
  } finally {
    await handle.close();
  }
}

function ustarHeader(path, size, executable) {
  const { name, prefix } = splitUstarPath(path);
  const header = Buffer.alloc(512);
  writeTarText(header, 0, 100, name);
  writeTarOctal(header, 100, 8, executable ? 0o755 : 0o644);
  writeTarOctal(header, 108, 8, 0);
  writeTarOctal(header, 116, 8, 0);
  writeTarOctal(header, 124, 12, size);
  writeTarOctal(header, 136, 12, 0);
  header.fill(0x20, 148, 156);
  header[156] = 0x30;
  writeTarText(header, 257, 6, 'ustar');
  writeTarText(header, 263, 2, '00');
  writeTarText(header, 345, 155, prefix);
  let checksum = 0;
  for (const byte of header) checksum += byte;
  const checksumText = checksum.toString(8).padStart(6, '0');
  header.write(checksumText, 148, 6, 'ascii');
  header[154] = 0;
  header[155] = 0x20;
  return header;
}

function splitUstarPath(path) {
  const bytes = Buffer.byteLength(path);
  if (bytes <= 100) return { name: path, prefix: '' };
  for (let index = path.length - 1; index > 0; index -= 1) {
    if (path[index] !== '/') continue;
    const prefix = path.slice(0, index);
    const name = path.slice(index + 1);
    if (Buffer.byteLength(prefix) <= 155 && Buffer.byteLength(name) <= 100) return { name, prefix };
  }
  throw new Error(`Complete-source path does not fit portable USTAR fields: ${path}`);
}

function writeTarText(header, offset, length, value) {
  const bytes = Buffer.from(value, 'ascii');
  if (bytes.byteLength > length) throw new Error(`Complete-source TAR text field is too long: ${value}`);
  bytes.copy(header, offset);
}

function writeTarOctal(header, offset, length, value) {
  const text = value.toString(8).padStart(length - 1, '0');
  if (text.length > length - 1) throw new Error('Complete-source TAR numeric field is too large');
  header.write(text, offset, length - 1, 'ascii');
  header[offset + length - 1] = 0;
}

async function writeStreamChunk(stream, chunk) {
  if (!stream.write(chunk)) await once(stream, 'drain');
}

async function canonicalDirectory(path, label) {
  const absolute = resolve(path);
  const info = await lstat(absolute).catch((error) => { throw new Error(`${label} is missing`, { cause: error }); });
  if (!info.isDirectory() || info.isSymbolicLink() || info.nlink < 1) throw new Error(`${label} must be a real directory`);
  return realpath(absolute);
}

async function canonicalRegularFile(path, label) {
  const absolute = resolve(path);
  const info = await lstat(absolute).catch((error) => { throw new Error(`${label} is missing`, { cause: error }); });
  if (!info.isFile() || info.isSymbolicLink() || info.nlink !== 1) throw new Error(`${label} must be a non-hardlinked regular file`);
  return realpath(absolute);
}

async function assertAbsent(path, label) {
  try {
    await lstat(path);
  } catch (error) {
    if (error?.code === 'ENOENT') return;
    throw error;
  }
  throw new Error(`${label} must not already exist`);
}

function safeChild(root, path) {
  const portable = safeArchivePath(path, false);
  const child = resolve(root, ...portable.split('/'));
  const relativePath = relative(root, child);
  if (relativePath === '..' || relativePath.startsWith(`..${sep}`) || posix.isAbsolute(relativePath)) {
    throw new Error('Complete-source path escapes its root');
  }
  return child;
}

function exactCore(value, label) {
  exactKeys(value, ['name', 'version'], `${label} core`);
  if (value.name !== COMPLETE_SOURCE_CORE.name || value.version !== COMPLETE_SOURCE_CORE.version) {
    throw new Error(`${label} core identity is invalid`);
  }
}

function exactKeys(value, keys, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)
    || Object.keys(value).sort(comparePaths).join('\n') !== [...keys].sort(comparePaths).join('\n')) {
    throw new Error(`${label} fields are not the exact schema`);
  }
}

function fileHash(files, path) {
  const file = files.find((candidate) => candidate.path === path);
  if (!file) throw new Error(`Complete-source file identity is missing: ${path}`);
  return file.sha256;
}

async function sha256File(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function isHash(value) {
  return typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
}

function comparePaths(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

async function main() {
  const [operation, ...args] = process.argv.slice(2);
  const options = parseCli(args);
  if (operation === 'create') {
    const result = await createCompleteSourceArtifact({
      baseDirectory: requiredOption(options, 'base-directory'),
      descriptorPath: requiredOption(options, 'descriptor'),
      licenseEvidenceDirectory: requiredOption(options, 'license-evidence-directory'),
      outputDirectory: requiredOption(options, 'output-directory'),
    });
    process.stdout.write(`${JSON.stringify({ artifactPath: result.artifactPath, descriptorPath: result.descriptorPath })}\n`);
    return;
  }
  if (operation === 'verify') {
    const result = await verifyAndExtractCompleteSourceArtifact({
      artifactPath: requiredOption(options, 'artifact'),
      descriptor: requiredOption(options, 'descriptor'),
      extractionRoot: requiredOption(options, 'extraction-root'),
    });
    process.stdout.write(`${JSON.stringify({ extractedRoot: result.extractedRoot })}\n`);
    return;
  }
  throw new Error('usage: complete-source-artifact.mjs <create|verify> [options]');
}

function parseCli(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    const name = args[index];
    const value = args[index + 1];
    if (!name?.startsWith('--') || !value || value.startsWith('--')) throw new Error('Complete-source artifact option is invalid');
    const key = name.slice(2);
    if (Object.hasOwn(options, key)) throw new Error(`Duplicate complete-source artifact option: ${name}`);
    options[key] = value;
  }
  return options;
}

function requiredOption(options, name) {
  if (!options[name]) throw new Error(`--${name} is required`);
  return options[name];
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
