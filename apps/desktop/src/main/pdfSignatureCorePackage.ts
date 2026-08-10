import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, readFile, readdir, realpath } from 'node:fs/promises';
import { isAbsolute, join, posix, relative, resolve, sep } from 'node:path';

export const PDF_SIGNATURE_CORE_PROTOCOL_VERSION = 1;
export const PDF_SIGNATURE_CORE_JAVA_VERSION = '21.0.12';
export const PDF_SIGNATURE_CORE_MAX_INPUT_BYTES = 512 * 1024 * 1024;
const FORBIDDEN_SOURCE_INVENTORY_PATH = 'notices/dependencies/inventory.json';
const DSS_SBOM_PATH = 'sbom/pdf-signature-core.cdx.json';
const COMPLETE_SOURCE_DESCRIPTOR_PATH = 'complete-source-artifact.json';
const COMPLETE_SOURCE_DISTRIBUTION_ID = 'butter-paper-pdf-signature-core-complete-source-v1';
const COMPLETE_SOURCE_ROOT = `${COMPLETE_SOURCE_DISTRIBUTION_ID}-0.1.0`;
const COMPLETE_SOURCE_PRODUCT = 'Butter Paper';
const COMPLETE_SOURCE_CORE_NAME = 'pdf-signature-core';
const COMPLETE_SOURCE_CORE_VERSION = '0.1.0';
const BUTTER_SOURCE_FILE_COUNT = 22;
const REQUIRED_RUNTIME_PACKAGE_PATHS = Object.freeze([
  COMPLETE_SOURCE_DESCRIPTOR_PATH,
  DSS_SBOM_PATH,
  'notices/LGPL-RELINKING.md',
  'notices/MIT.txt',
  'notices/THIRD-PARTY-NOTICES.md',
  'notices/MICROSOFT-OPENJDK-LICENSE.txt',
]);
const REVIEWED_DSS_SOURCE: ReviewedDssSourceIdentity = {
  bytes: 137227450,
  dependencyInventorySha256: '8c943772bc55dbb45ab3baad0236196c4f1ec293d100fe3f5cf84b2ffb0c0a45',
  resolvedCommit: '26a2e3338d8d4fe6c6281c2b53d13546fa64c9bf',
  sha256: '5f2421d6bf1c6073aa1e3c1ed4b44d2f058c6d751a4d89dbf326082860b224a4',
};

export type PdfSignatureCorePlatform = 'darwin' | 'win32' | 'linux';
export type PdfSignatureCoreArchitecture = 'arm64' | 'x64';

export interface PdfSignatureCoreComponent {
  path: string;
  sha256: string;
  size: number;
  executable: boolean;
}

export interface PdfSignatureCoreSigningMutableComponent {
  path: string;
  reason: 'platform-signing';
}

export interface PdfSignatureCorePackageManifest {
  schemaVersion: 2;
  protocolVersion: 1;
  engineVersion: string;
  javaVersion: string;
  platform: PdfSignatureCorePlatform;
  arch: PdfSignatureCoreArchitecture;
  launcher: string;
  buildState: 'unsigned-build';
  immutableComponents: PdfSignatureCoreComponent[];
  signingMutableComponents: PdfSignatureCoreSigningMutableComponent[];
  signingMutablePathRules: ['**/_CodeSignature/**', '**/CodeResources'];
  postSignInventory: 'post-sign-inventory.json';
  postSignInventoryRequiredForRelease: true;
  postSignTrust: 'requires-enclosing-signed-or-tuf-verified-package';
  /** Verified post-sign components, derived from post-sign-inventory.json. */
  components: PdfSignatureCoreComponent[];
}

export interface PdfSignatureCorePostSignInventory {
  schemaVersion: 1;
  manifestSha256: string;
  platform: PdfSignatureCorePlatform;
  arch: PdfSignatureCoreArchitecture;
  evidenceState: 'post-nested-signing-unsealed';
  releaseSealed: false;
  components: PdfSignatureCoreComponent[];
  trustRequirement: 'must-be-covered-by-enclosing-signed-or-tuf-verified-package';
}

export interface VerifiedPdfSignatureCorePackage {
  packageRoot: string;
  launcherPath: string;
  manifest: PdfSignatureCorePackageManifest;
  postSignInventory: PdfSignatureCorePostSignInventory;
  sourceDescriptor: PdfSignatureCoreSourceDescriptor;
}

export interface PdfSignatureCoreSourceDescriptor {
  schemaVersion: 1;
  distributionId: 'butter-paper-pdf-signature-core-complete-source-v1';
  product: 'Butter Paper';
  core: { name: 'pdf-signature-core'; version: '0.1.0' };
  delivery: {
    kind: 'distribution-wide-sibling';
    canonicalFileName: string;
    distributionRelativePath: string;
    authenticationRequirement: 'same-release-signed-manifest-or-tuf-target';
    retentionRequirement: 'immutable-and-co-retained-for-corresponding-binary-lifetime';
    requiredForRedistribution: true;
    packageLocalSourcePresent: false;
  };
  artifact: { bytes: number; format: 'tar.gz'; rootDirectory: string; sha256: string };
  internalManifest: { path: 'SOURCE-MANIFEST.json'; sha256: string };
  runtimeNotices: Array<{ bytes: number; path: string; sha256: string }>;
  sourceIdentity: {
    butter: { algorithm: string; fileCount: number; sha256: string };
    dss: { bytes: number; resolvedCommit: string; sha256: string; version: '6.4' };
    policySha256: string;
    sbomSha256: string;
    dependencyInventorySha256: string;
  };
  legalApproval: false;
  releaseSealed: false;
}

export interface ResolvePdfSignatureCorePackageOptions {
  isPackaged: boolean;
  resourcesPath: string;
  appPath: string;
  platform?: NodeJS.Platform;
  arch?: string;
  developmentRoot?: string;
  /** Test seam only; production callers must use the pinned package verifier. */
  packageVerifier?: typeof verifyPdfSignatureCorePackage;
}

export interface ReviewedDssSourceIdentity {
  bytes: number;
  /** Omitted only by the deterministic tiny-fixture verifier. */
  dependencyInventorySha256?: string;
  resolvedCommit: string;
  sha256: string;
}

export function pdfSignatureCoreTarget(
  platform: NodeJS.Platform,
  arch: string,
): `${PdfSignatureCorePlatform}-${PdfSignatureCoreArchitecture}` {
  if (!isSupportedPlatform(platform)) {
    throw new Error(`PDF signature core does not support platform ${platform}.`);
  }
  if (arch !== 'arm64' && arch !== 'x64') {
    throw new Error(`PDF signature core does not support architecture ${arch}.`);
  }
  return `${platform}-${arch}`;
}

export async function resolvePdfSignatureCorePackage(
  options: ResolvePdfSignatureCorePackageOptions,
): Promise<VerifiedPdfSignatureCorePackage> {
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const target = pdfSignatureCoreTarget(platform, arch);
  const packageRoot = options.isPackaged
    ? join(options.resourcesPath, 'pdf-signature-core', target)
    : join(
        options.developmentRoot
          ? resolve(options.developmentRoot)
          : resolve(options.appPath, '../../native/pdf-signature-core/build/package'),
        target,
      );

  return (options.packageVerifier ?? verifyPdfSignatureCorePackage)(packageRoot, { platform, arch });
}

export async function verifyPdfSignatureCorePackage(
  packageRoot: string,
  expected: { platform: NodeJS.Platform; arch: string },
): Promise<VerifiedPdfSignatureCorePackage> {
  return verifyPdfSignatureCorePackageWithIdentity(packageRoot, expected, REVIEWED_DSS_SOURCE);
}

/** Test-only verifier for tiny deterministic source fixtures. */
export async function verifyPdfSignatureCorePackageFixtureForTesting(
  packageRoot: string,
  expected: { platform: NodeJS.Platform; arch: string },
  reviewedSource: ReviewedDssSourceIdentity,
): Promise<VerifiedPdfSignatureCorePackage> {
  if (process.env.NODE_ENV !== 'test') {
    throw packageError('fixture verifier is unavailable outside the deterministic test environment');
  }
  return verifyPdfSignatureCorePackageWithIdentity(
    packageRoot,
    expected,
    reviewedSource,
    process.platform === 'win32',
  );
}

async function verifyPdfSignatureCorePackageWithIdentity(
  packageRoot: string,
  expected: { platform: NodeJS.Platform; arch: string },
  reviewedSource: ReviewedDssSourceIdentity,
  fixtureCannotObservePosixModes = false,
): Promise<VerifiedPdfSignatureCorePackage> {
  const expectedTarget = pdfSignatureCoreTarget(expected.platform, expected.arch);
  const expectedPlatform = expectedTarget.slice(0, expectedTarget.lastIndexOf('-')) as PdfSignatureCorePlatform;
  const expectedArch = expectedTarget.slice(expectedTarget.lastIndexOf('-') + 1) as PdfSignatureCoreArchitecture;
  const root = resolve(packageRoot);
  const rootStat = await lstat(root).catch((error: unknown) => {
    throw packageError(`is missing at ${root}`, error);
  });
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) {
    throw packageError('root must be a real directory, not a symlink');
  }
  const canonicalRoot = await realpath(root);
  const manifestPath = join(canonicalRoot, 'manifest.json');
  await assertRegularFile(manifestPath, canonicalRoot, 'manifest.json');
  const manifestBytes = await readFile(manifestPath);
  if (manifestBytes.byteLength > 1024 * 1024) {
    throw packageError('manifest exceeds the 1 MiB limit');
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(manifestBytes.toString('utf8'));
  } catch (error) {
    throw packageError('manifest is not valid JSON', error);
  }
  const parsedManifest = parseManifest(parsed);
  let manifest = parsedManifest;
  if (manifest.platform !== expectedPlatform || manifest.arch !== expectedArch) {
    throw packageError(
      `manifest target ${manifest.platform}-${manifest.arch} does not match ${expectedTarget}`,
    );
  }

  const declaredPaths = new Set<string>();
  for (const component of [...manifest.immutableComponents, ...manifest.signingMutableComponents]) {
    if (declaredPaths.has(component.path)) {
      throw packageError(`manifest contains duplicate component ${component.path}`);
    }
    declaredPaths.add(component.path);
  }
  if (!declaredPaths.has(manifest.launcher)) {
    throw packageError('manifest launcher must be listed exactly once as a component');
  }

  const actualFiles = await listPackageFiles(canonicalRoot);
  const postSignInventoryPath = join(canonicalRoot, manifest.postSignInventory);
  await assertRegularFile(postSignInventoryPath, canonicalRoot, manifest.postSignInventory);
  const postSignInventoryBytes = await readFile(postSignInventoryPath);
  if (postSignInventoryBytes.byteLength > 4 * 1024 * 1024) {
    throw packageError('post-sign inventory exceeds the 4 MiB limit');
  }
  let parsedPostSignInventory: unknown;
  try {
    parsedPostSignInventory = JSON.parse(postSignInventoryBytes.toString('utf8'));
  } catch (error) {
    throw packageError('post-sign inventory is not valid JSON', error);
  }
  const postSignInventory = parsePostSignInventory(parsedPostSignInventory);
  const manifestSha256 = createHash('sha256').update(manifestBytes).digest('hex');
  if (postSignInventory.manifestSha256 !== manifestSha256) {
    throw packageError('post-sign inventory does not bind the current manifest SHA-256');
  }
  if (postSignInventory.platform !== expectedPlatform || postSignInventory.arch !== expectedArch) {
    throw packageError('post-sign inventory target does not match the package target');
  }

  const postSignPaths = new Set<string>();
  for (const component of postSignInventory.components) {
    if (postSignPaths.has(component.path)) {
      throw packageError(`post-sign inventory contains duplicate component ${component.path}`);
    }
    postSignPaths.add(component.path);
  }
  const expectedFiles = [...postSignPaths, 'manifest.json', manifest.postSignInventory].sort(comparePortablePaths);
  if (actualFiles.join('\n') !== expectedFiles.join('\n')) {
    const unexpected = actualFiles.filter((file) => !expectedFiles.includes(file));
    const missing = expectedFiles.filter((file) => !actualFiles.includes(file));
    throw packageError(
      `post-sign inventory mismatch (missing: ${missing.join(', ') || 'none'}; unexpected: ${unexpected.join(', ') || 'none'})`,
    );
  }

  for (const component of postSignInventory.components) {
    const componentPath = join(canonicalRoot, ...component.path.split('/'));
    const componentStat = await assertRegularFile(componentPath, canonicalRoot, component.path);
    if (componentStat.size !== component.size) {
      throw packageError(`${component.path} size does not match its manifest entry`);
    }
    const digest = await sha256File(componentPath);
    if (digest !== component.sha256) {
      throw packageError(`${component.path} checksum does not match its manifest entry`);
    }
    if (expectedPlatform !== 'win32'
      && !fixtureCannotObservePosixModes
      && ((componentStat.mode & 0o111) !== 0) !== component.executable) {
      throw packageError(`${component.path} executable bit does not match its post-sign inventory entry`);
    }
    if (componentStat.nlink !== 1) {
      throw packageError(`${component.path} must not be a hardlink`);
    }
  }

  const postSignByPath = new Map(postSignInventory.components.map((component) => [component.path, component]));
  const sourceDescriptor = await verifySeparateSourceDescriptor(
    canonicalRoot,
    postSignByPath,
    reviewedSource,
  );
  for (const immutable of manifest.immutableComponents) {
    const postSign = postSignByPath.get(immutable.path);
    if (!postSign
      || postSign.sha256 !== immutable.sha256
      || postSign.size !== immutable.size
      || postSign.executable !== immutable.executable) {
      throw packageError(`immutable component ${immutable.path} changed after the unsigned-build manifest`);
    }
  }
  for (const mutable of manifest.signingMutableComponents) {
    if (!postSignByPath.has(mutable.path)) {
      throw packageError(`signing-mutable component ${mutable.path} is missing from the post-sign inventory`);
    }
    const mutablePath = join(canonicalRoot, ...mutable.path.split('/'));
    if (!await isNativeSigningMutableFile(mutablePath) && !matchesSigningMetadataRule(mutable.path)) {
      throw packageError(`signing-mutable component ${mutable.path} is not native signing material`);
    }
  }
  for (const component of postSignInventory.components) {
    if (!declaredPaths.has(component.path) && !matchesSigningMetadataRule(component.path)) {
      throw packageError(`post-sign inventory contains impermissible additional path ${component.path}`);
    }
  }

  manifest = { ...manifest, components: postSignInventory.components };

  const launcherPath = join(canonicalRoot, ...manifest.launcher.split('/'));
  const launcherStat = await lstat(launcherPath);
  const launcherComponent = postSignByPath.get(manifest.launcher)!;
  if (expectedPlatform !== 'win32') {
    if (launcherComponent.executable !== true
      || (!fixtureCannotObservePosixModes && (launcherStat.mode & 0o111) === 0)) {
      throw packageError('POSIX launcher is not declared and stored as executable');
    }
  }
  await assertNativeLauncherArchitecture(launcherPath, expectedPlatform, expectedArch);

  return { packageRoot: canonicalRoot, launcherPath, manifest, postSignInventory, sourceDescriptor };
}

async function verifySeparateSourceDescriptor(
  packageRoot: string,
  postSignByPath: Map<string, PdfSignatureCoreComponent>,
  reviewedSource: ReviewedDssSourceIdentity,
): Promise<PdfSignatureCoreSourceDescriptor> {
  for (const requiredPath of REQUIRED_RUNTIME_PACKAGE_PATHS) {
    if (!postSignByPath.has(requiredPath)) {
      throw packageError(`required runtime package component is missing: ${requiredPath}`);
    }
  }
  if ([...postSignByPath.keys()].some((path) => path === 'source' || path.startsWith('source/'))) {
    throw packageError('runtime package must not embed the separate complete-source tree');
  }
  if (postSignByPath.has(FORBIDDEN_SOURCE_INVENTORY_PATH)) {
    throw packageError('runtime package must not copy source-artifact inventory with package-local source claims');
  }
  const descriptorComponent = postSignByPath.get(COMPLETE_SOURCE_DESCRIPTOR_PATH);
  if (!descriptorComponent || descriptorComponent.size > 64 * 1024) {
    throw packageError('runtime package complete-source descriptor is missing or oversized');
  }
  const value = await readBoundedPackageJson(
    packageRoot,
    COMPLETE_SOURCE_DESCRIPTOR_PATH,
    'complete-source descriptor',
  );
  if (!isRecord(value)) throw packageError('complete-source descriptor must be an object');
  assertExactKeys(value, [
    'artifact', 'core', 'delivery', 'distributionId', 'internalManifest', 'legalApproval',
    'product', 'releaseSealed', 'runtimeNotices', 'schemaVersion', 'sourceIdentity',
  ], 'complete-source descriptor');
  if (value.schemaVersion !== 1
    || value.distributionId !== COMPLETE_SOURCE_DISTRIBUTION_ID
    || value.product !== COMPLETE_SOURCE_PRODUCT
    || value.legalApproval !== false
    || value.releaseSealed !== false) {
    throw packageError('complete-source descriptor identity or claims are invalid');
  }
  if (!isRecord(value.core)) throw packageError('complete-source descriptor core must be an object');
  assertExactKeys(value.core, ['name', 'version'], 'complete-source descriptor core');
  if (value.core.name !== COMPLETE_SOURCE_CORE_NAME || value.core.version !== COMPLETE_SOURCE_CORE_VERSION) {
    throw packageError('complete-source descriptor core identity is invalid');
  }
  if (!isRecord(value.delivery)) throw packageError('complete-source descriptor delivery must be an object');
  assertExactKeys(value.delivery, [
    'authenticationRequirement', 'canonicalFileName', 'distributionRelativePath', 'kind',
    'packageLocalSourcePresent', 'requiredForRedistribution', 'retentionRequirement',
  ], 'complete-source descriptor delivery');
  if (value.delivery.kind !== 'distribution-wide-sibling'
    || value.delivery.authenticationRequirement !== 'same-release-signed-manifest-or-tuf-target'
    || value.delivery.retentionRequirement !== 'immutable-and-co-retained-for-corresponding-binary-lifetime'
    || value.delivery.requiredForRedistribution !== true
    || value.delivery.packageLocalSourcePresent !== false
    || typeof value.delivery.canonicalFileName !== 'string'
    || !/^butter-paper-pdf-signature-core-complete-source-v1-0\.1\.0-[a-f0-9]{16}\.tar\.gz$/.test(
      value.delivery.canonicalFileName,
    )
    || value.delivery.distributionRelativePath !== `pdf-signature-core/source/${value.delivery.canonicalFileName}`) {
    throw packageError('complete-source descriptor sibling delivery is invalid');
  }
  if (!isRecord(value.artifact)) throw packageError('complete-source descriptor artifact must be an object');
  assertExactKeys(value.artifact, ['bytes', 'format', 'rootDirectory', 'sha256'], 'complete-source descriptor artifact');
  if (!Number.isSafeInteger(value.artifact.bytes) || (value.artifact.bytes as number) <= 0
    || (value.artifact.bytes as number) > 256 * 1024 * 1024
    || value.artifact.format !== 'tar.gz'
    || value.artifact.rootDirectory !== COMPLETE_SOURCE_ROOT
    || !isSha256(value.artifact.sha256)) {
    throw packageError('complete-source descriptor artifact identity is invalid');
  }
  if (!isRecord(value.internalManifest)) throw packageError('complete-source descriptor internal manifest must be an object');
  assertExactKeys(value.internalManifest, ['path', 'sha256'], 'complete-source descriptor internal manifest');
  if (value.internalManifest.path !== 'SOURCE-MANIFEST.json' || !isSha256(value.internalManifest.sha256)) {
    throw packageError('complete-source descriptor internal manifest identity is invalid');
  }
  if (!isRecord(value.sourceIdentity)) throw packageError('complete-source descriptor source identity must be an object');
  assertExactKeys(value.sourceIdentity, [
    'butter', 'dependencyInventorySha256', 'dss', 'policySha256', 'sbomSha256',
  ], 'complete-source descriptor source identity');
  if (!isRecord(value.sourceIdentity.butter)) throw packageError('Butter source identity must be an object');
  assertExactKeys(value.sourceIdentity.butter, ['algorithm', 'fileCount', 'sha256'], 'Butter source identity');
  if (value.sourceIdentity.butter.algorithm !== 'sha256-path-length-path-nul-byte-length-bytes-nul-v1'
    || value.sourceIdentity.butter.fileCount !== BUTTER_SOURCE_FILE_COUNT
    || !isSha256(value.sourceIdentity.butter.sha256)) {
    throw packageError('Butter source identity is invalid');
  }
  if (!isRecord(value.sourceIdentity.dss)) throw packageError('DSS source identity must be an object');
  assertExactKeys(value.sourceIdentity.dss, ['bytes', 'resolvedCommit', 'sha256', 'version'], 'DSS source identity');
  if (value.sourceIdentity.dss.version !== '6.4'
    || value.sourceIdentity.dss.bytes !== reviewedSource.bytes
    || value.sourceIdentity.dss.resolvedCommit !== reviewedSource.resolvedCommit
    || value.sourceIdentity.dss.sha256 !== reviewedSource.sha256) {
    throw packageError('complete-source descriptor DSS identity differs from the reviewed source');
  }
  for (const field of ['policySha256', 'sbomSha256', 'dependencyInventorySha256'] as const) {
    if (!isSha256(value.sourceIdentity[field])) throw packageError(`complete-source descriptor ${field} is invalid`);
  }
  if (reviewedSource.dependencyInventorySha256
    && value.sourceIdentity.dependencyInventorySha256 !== reviewedSource.dependencyInventorySha256) {
    throw packageError('complete-source descriptor dependency inventory differs from the reviewed identity');
  }
  const sbomComponent = postSignByPath.get(DSS_SBOM_PATH);
  if (!sbomComponent || sbomComponent.sha256 !== value.sourceIdentity.sbomSha256) {
    throw packageError('runtime SBOM does not match the complete-source descriptor');
  }
  if (!Array.isArray(value.runtimeNotices) || value.runtimeNotices.length === 0 || value.runtimeNotices.length > 20_000) {
    throw packageError('complete-source descriptor runtime notice inventory is invalid');
  }
  const runtimeNotices = value.runtimeNotices.map((entry, index) => {
    if (!isRecord(entry)) throw packageError(`runtime notice ${index} must be an object`);
    assertExactKeys(entry, ['bytes', 'path', 'sha256'], `runtime notice ${index}`);
    const path = parseSafeRelativePath(entry.path, `runtime notice ${index} path`);
    if (!path.startsWith('notices/dependencies/')
      || !Number.isSafeInteger(entry.bytes) || (entry.bytes as number) < 0 || (entry.bytes as number) > 1024 * 1024 * 1024
      || !isSha256(entry.sha256)) {
      throw packageError(`runtime notice ${path} is invalid`);
    }
    return { path, bytes: entry.bytes as number, sha256: entry.sha256 };
  });
  if (runtimeNotices.some((entry, index) => index > 0
    && comparePortablePaths(runtimeNotices[index - 1]!.path, entry.path) >= 0)) {
    throw packageError('runtime notice inventory must be uniquely sorted');
  }
  const actualNoticePaths = [...postSignByPath.keys()]
    .filter((path) => path.startsWith('notices/dependencies/'))
    .sort(comparePortablePaths);
  if (actualNoticePaths.join('\n') !== runtimeNotices.map((entry) => entry.path).join('\n')) {
    throw packageError('runtime dependency notice files do not match the complete-source descriptor');
  }
  for (const expected of runtimeNotices) {
    const actual = postSignByPath.get(expected.path)!;
    if (actual.size !== expected.bytes || actual.sha256 !== expected.sha256) {
      throw packageError(`runtime dependency notice differs from the complete-source descriptor: ${expected.path}`);
    }
  }
  return value as unknown as PdfSignatureCoreSourceDescriptor;
}

async function readBoundedPackageJson(
  packageRoot: string,
  relativePath: string,
  label: string,
): Promise<unknown> {
  const bytes = await readFile(join(packageRoot, ...relativePath.split('/')));
  if (bytes.byteLength > 4 * 1024 * 1024) throw packageError(`${label} exceeds the 4 MiB limit`);
  try {
    return JSON.parse(bytes.toString('utf8'));
  } catch (error) {
    throw packageError(`${label} is not valid JSON`, error);
  }
}

export async function assertPdfSignatureCoreInputFile(inputPath: string): Promise<string> {
  if (!isAbsolute(inputPath)) {
    throw new Error('PDF signature core input must be an absolute main-process path.');
  }
  const inputStat = await lstat(inputPath).catch((error: unknown) => {
    throw new Error('PDF signature core input is unavailable.', { cause: error });
  });
  if (!inputStat.isFile() || inputStat.isSymbolicLink()) {
    throw new Error('PDF signature core input must be a regular file, not a symlink.');
  }
  if (inputStat.size > PDF_SIGNATURE_CORE_MAX_INPUT_BYTES) {
    throw new Error('PDF signature core input exceeds the 512 MiB safety limit.');
  }
  return realpath(inputPath);
}

function parseManifest(value: unknown): PdfSignatureCorePackageManifest {
  if (!isRecord(value)) throw packageError('manifest must be an object');
  assertExactKeys(value, [
    'arch',
    'buildState',
    'engineVersion',
    'immutableComponents',
    'javaVersion',
    'launcher',
    'platform',
    'postSignInventory',
    'postSignInventoryRequiredForRelease',
    'postSignTrust',
    'protocolVersion',
    'schemaVersion',
    'signingMutableComponents',
    'signingMutablePathRules',
  ], 'manifest');
  if (value.schemaVersion !== 2) throw packageError('manifest schemaVersion must be 2');
  if (value.protocolVersion !== PDF_SIGNATURE_CORE_PROTOCOL_VERSION) {
    throw packageError(`manifest protocolVersion must be ${PDF_SIGNATURE_CORE_PROTOCOL_VERSION}`);
  }
  if (typeof value.engineVersion !== 'string' || value.engineVersion.length === 0 || value.engineVersion.length > 128) {
    throw packageError('manifest engineVersion is invalid');
  }
  if (value.javaVersion !== PDF_SIGNATURE_CORE_JAVA_VERSION) {
    throw packageError(`manifest javaVersion must be ${PDF_SIGNATURE_CORE_JAVA_VERSION}`);
  }
  if (!isSupportedPlatform(value.platform)) throw packageError('manifest platform is invalid');
  if (value.arch !== 'arm64' && value.arch !== 'x64') throw packageError('manifest arch is invalid');
  const launcher = parseSafeRelativePath(value.launcher, 'launcher');
  if (value.buildState !== 'unsigned-build') throw packageError('manifest buildState must remain unsigned-build');
  if (!Array.isArray(value.immutableComponents) || value.immutableComponents.length === 0 || value.immutableComponents.length > 20_000) {
    throw packageError('manifest immutableComponents must contain between 1 and 20000 entries');
  }
  if (!Array.isArray(value.signingMutableComponents)
    || value.signingMutableComponents.length === 0
    || value.signingMutableComponents.length > 20_000) {
    throw packageError('manifest signingMutableComponents must contain between 1 and 20000 entries');
  }
  const immutableComponents = value.immutableComponents.map((entry, index) => parseComponent(entry, index, 'immutable component'));
  const signingMutableComponents = value.signingMutableComponents.map(parseSigningMutableComponent);
  assertSortedComponents(immutableComponents, 'manifest immutableComponents');
  assertSortedComponents(signingMutableComponents, 'manifest signingMutableComponents');
  const declaredComponents = [...immutableComponents, ...signingMutableComponents];
  if (!declaredComponents.some((component) => /(?:^|\/)runtime\//.test(component.path))) {
    throw packageError('manifest must inventory the bundled runtime');
  }
  if (!immutableComponents.some((component) => component.path.startsWith('notices/'))) {
    throw packageError('manifest must inventory licence notices');
  }
  if (!immutableComponents.some((component) => component.path.startsWith('sbom/'))) {
    throw packageError('manifest must inventory the SBOM');
  }
  if (!Array.isArray(value.signingMutablePathRules)
    || value.signingMutablePathRules.length !== 2
    || value.signingMutablePathRules[0] !== '**/_CodeSignature/**'
    || value.signingMutablePathRules[1] !== '**/CodeResources') {
    throw packageError('manifest signingMutablePathRules are not the exact reviewed rules');
  }
  if (value.postSignInventory !== 'post-sign-inventory.json'
    || value.postSignInventoryRequiredForRelease !== true
    || value.postSignTrust !== 'requires-enclosing-signed-or-tuf-verified-package') {
    throw packageError('manifest post-sign evidence policy is invalid');
  }
  return {
    schemaVersion: 2,
    protocolVersion: 1,
    engineVersion: value.engineVersion,
    javaVersion: value.javaVersion,
    platform: value.platform,
    arch: value.arch,
    launcher,
    buildState: 'unsigned-build',
    immutableComponents,
    signingMutableComponents,
    signingMutablePathRules: ['**/_CodeSignature/**', '**/CodeResources'],
    postSignInventory: 'post-sign-inventory.json',
    postSignInventoryRequiredForRelease: true,
    postSignTrust: 'requires-enclosing-signed-or-tuf-verified-package',
    components: [],
  };
}

function parsePostSignInventory(value: unknown): PdfSignatureCorePostSignInventory {
  if (!isRecord(value)) throw packageError('post-sign inventory must be an object');
  assertExactKeys(value, [
    'arch',
    'components',
    'evidenceState',
    'manifestSha256',
    'platform',
    'releaseSealed',
    'schemaVersion',
    'trustRequirement',
  ], 'post-sign inventory');
  if (value.schemaVersion !== 1) throw packageError('post-sign inventory schemaVersion must be 1');
  if (typeof value.manifestSha256 !== 'string' || !/^[a-f0-9]{64}$/.test(value.manifestSha256)) {
    throw packageError('post-sign inventory manifestSha256 is invalid');
  }
  if (!isSupportedPlatform(value.platform)) throw packageError('post-sign inventory platform is invalid');
  if (value.arch !== 'arm64' && value.arch !== 'x64') throw packageError('post-sign inventory arch is invalid');
  if (value.evidenceState !== 'post-nested-signing-unsealed' || value.releaseSealed !== false) {
    throw packageError('post-sign inventory must remain post-nested-signing-unsealed with releaseSealed false');
  }
  if (value.trustRequirement !== 'must-be-covered-by-enclosing-signed-or-tuf-verified-package') {
    throw packageError('post-sign inventory trust requirement is invalid');
  }
  if (!Array.isArray(value.components) || value.components.length === 0 || value.components.length > 20_000) {
    throw packageError('post-sign inventory components must contain between 1 and 20000 entries');
  }
  const components = value.components.map((entry, index) => parseComponent(entry, index, 'post-sign component'));
  assertSortedComponents(components, 'post-sign inventory components');
  if (components.some((component) => component.path === 'manifest.json' || component.path === 'post-sign-inventory.json')) {
    throw packageError('post-sign inventory must not inventory itself or manifest.json');
  }
  return {
    schemaVersion: 1,
    manifestSha256: value.manifestSha256,
    platform: value.platform,
    arch: value.arch,
    evidenceState: 'post-nested-signing-unsealed',
    releaseSealed: false,
    components,
    trustRequirement: 'must-be-covered-by-enclosing-signed-or-tuf-verified-package',
  };
}

function parseComponent(value: unknown, index: number, label: string): PdfSignatureCoreComponent {
  if (!isRecord(value)) throw packageError(`${label} ${index} must be an object`);
  assertExactKeys(value, ['executable', 'path', 'sha256', 'size'], `${label} ${index}`);
  const path = parseSafeRelativePath(value.path, `${label} ${index} path`);
  if (typeof value.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(value.sha256)) {
    throw packageError(`component ${path} has an invalid SHA-256`);
  }
  if (!Number.isSafeInteger(value.size) || (value.size as number) < 0 || (value.size as number) > 1024 * 1024 * 1024) {
    throw packageError(`component ${path} has an invalid size`);
  }
  if (typeof value.executable !== 'boolean') {
    throw packageError(`component ${path} has an invalid executable flag`);
  }
  return {
    path,
    sha256: value.sha256,
    size: value.size as number,
    executable: value.executable,
  };
}

function parseSigningMutableComponent(value: unknown, index: number): PdfSignatureCoreSigningMutableComponent {
  if (!isRecord(value)) throw packageError(`signing-mutable component ${index} must be an object`);
  assertExactKeys(value, ['path', 'reason'], `signing-mutable component ${index}`);
  const path = parseSafeRelativePath(value.path, `signing-mutable component ${index} path`);
  if (value.reason !== 'platform-signing') {
    throw packageError(`signing-mutable component ${path} reason must be platform-signing`);
  }
  return { path, reason: 'platform-signing' };
}

function parseSafeRelativePath(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0 || value.length > 1024) {
    throw packageError(`${label} is invalid`);
  }
  if (value.includes('\\') || value.includes('\0') || posix.isAbsolute(value)) {
    throw packageError(`${label} must be a portable relative path`);
  }
  const normalized = posix.normalize(value);
  if (normalized !== value || normalized === '..' || normalized.startsWith('../')) {
    throw packageError(`${label} escapes the package root`);
  }
  return value;
}

async function listPackageFiles(root: string): Promise<string[]> {
  const files: string[] = [];
  async function visit(directory: string): Promise<void> {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const absolutePath = join(directory, entry.name);
      const relativePath = relative(root, absolutePath).split(sep).join('/');
      if (entry.isSymbolicLink()) throw packageError(`package contains symlink ${relativePath}`);
      if (entry.isDirectory()) {
        await visit(absolutePath);
      } else if (entry.isFile()) {
        files.push(relativePath);
      } else {
        throw packageError(`package contains non-regular entry ${relativePath}`);
      }
    }
  }
  await visit(root);
  return files.sort();
}

async function assertRegularFile(path: string, root: string, label: string) {
  const pathStat = await lstat(path).catch((error: unknown) => {
    throw packageError(`${label} is missing`, error);
  });
  if (!pathStat.isFile() || pathStat.isSymbolicLink()) {
    throw packageError(`${label} must be a regular file, not a symlink`);
  }
  const canonicalPath = await realpath(path);
  const pathRelativeToRoot = relative(root, canonicalPath);
  if (pathRelativeToRoot === '..' || pathRelativeToRoot.startsWith(`..${sep}`) || isAbsolute(pathRelativeToRoot)) {
    throw packageError(`${label} resolves outside the package root`);
  }
  return pathStat;
}

async function sha256File(path: string): Promise<string> {
  const hash = createHash('sha256');
  await new Promise<void>((resolvePromise, reject) => {
    const stream = createReadStream(path);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('error', reject);
    stream.on('end', resolvePromise);
  });
  return hash.digest('hex');
}

async function isNativeSigningMutableFile(path: string): Promise<boolean> {
  const handle = await import('node:fs/promises').then(({ open }) => open(path, 'r'));
  try {
    const header = Buffer.alloc(4);
    const { bytesRead } = await handle.read(header, 0, header.byteLength, 0);
    if (bytesRead < 4) return false;
    const magic = header.readUInt32BE(0);
    return magic === 0xfeedface
      || magic === 0xfeedfacf
      || magic === 0xcefaedfe
      || magic === 0xcffaedfe
      || magic === 0xcafebabe
      || magic === 0xbebafeca
      || magic === 0x7f454c46
      || (header[0] === 0x4d && header[1] === 0x5a);
  } finally {
    await handle.close();
  }
}

function matchesSigningMetadataRule(path: string): boolean {
  return path.includes('/_CodeSignature/') || path.endsWith('/CodeResources');
}

async function assertNativeLauncherArchitecture(
  launcherPath: string,
  platform: PdfSignatureCorePlatform,
  arch: PdfSignatureCoreArchitecture,
): Promise<void> {
  const handle = await import('node:fs/promises').then(({ open }) => open(launcherPath, 'r'));
  try {
    const header = Buffer.alloc(4096);
    const { bytesRead } = await handle.read(header, 0, header.byteLength, 0);
    const detected = detectNativeArchitecture(header.subarray(0, bytesRead), platform);
    if (detected !== arch) {
      throw packageError(`launcher architecture ${detected ?? 'unknown'} does not match ${arch}`);
    }
  } finally {
    await handle.close();
  }
}

export function detectNativeArchitecture(
  bytes: Uint8Array,
  platform: PdfSignatureCorePlatform,
): PdfSignatureCoreArchitecture | null {
  const buffer = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (platform === 'linux') {
    if (buffer.length < 20 || !buffer.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))) return null;
    const littleEndian = buffer[5] === 1;
    const machine = littleEndian ? buffer.readUInt16LE(18) : buffer.readUInt16BE(18);
    return machine === 0xb7 ? 'arm64' : machine === 0x3e ? 'x64' : null;
  }
  if (platform === 'win32') {
    if (buffer.length < 64 || buffer.toString('ascii', 0, 2) !== 'MZ') return null;
    const peOffset = buffer.readUInt32LE(0x3c);
    if (peOffset + 6 > buffer.length || buffer.toString('ascii', peOffset, peOffset + 4) !== 'PE\0\0') return null;
    const machine = buffer.readUInt16LE(peOffset + 4);
    return machine === 0xaa64 ? 'arm64' : machine === 0x8664 ? 'x64' : null;
  }
  if (buffer.length < 8) return null;
  const magic = buffer.readUInt32BE(0);
  let cpuType: number;
  if (magic === 0xcffaedfe) cpuType = buffer.readUInt32LE(4);
  else if (magic === 0xfeedfacf) cpuType = buffer.readUInt32BE(4);
  else return null;
  return cpuType === 0x0100000c ? 'arm64' : cpuType === 0x01000007 ? 'x64' : null;
}

function isSupportedPlatform(value: unknown): value is PdfSignatureCorePlatform {
  return value === 'darwin' || value === 'win32' || value === 'linux';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isSha256(value: unknown): value is string {
  return typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
}

function assertExactKeys(value: Record<string, unknown>, expected: string[], label: string): void {
  const actual = Object.keys(value).sort(comparePortablePaths);
  const sortedExpected = [...expected].sort(comparePortablePaths);
  if (actual.join('\n') !== sortedExpected.join('\n')) {
    throw packageError(`${label} fields are not the exact schema contract`);
  }
}

function assertSortedComponents(
  components: ReadonlyArray<{ path: string }>,
  label: string,
): void {
  const sorted = [...components].sort((left, right) => comparePortablePaths(left.path, right.path));
  if (components.some((component, index) => component.path !== sorted[index]?.path)) {
    throw packageError(`${label} must be sorted by path`);
  }
}

function comparePortablePaths(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function packageError(message: string, cause?: unknown): Error {
  return new Error(`Invalid PDF signature core package: ${message}.`, cause === undefined ? undefined : { cause });
}
