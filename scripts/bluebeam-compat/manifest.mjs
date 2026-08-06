import { createHash } from 'node:crypto';
import { lstat, readFile, realpath, stat } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { dirname, isAbsolute, relative, resolve } from 'node:path';

const MANIFEST_VERSION = 1;

export async function sha256File(filePath) {
  return sha256(await readFile(filePath));
}

export async function gitWorktreeIdentity(repoRoot) {
  const status = git(repoRoot, ['status', '--porcelain=v1', '-z', '--untracked-files=all']);
  const diff = git(repoRoot, ['diff', '--binary', 'HEAD', '--', '.']);
  const entries = status.toString('utf8').split('\0').filter(Boolean);
  const untracked = entries
    .filter((entry) => entry.startsWith('?? '))
    .map((entry) => entry.slice(3))
    .sort();
  const hash = createHash('sha256').update(status).update(diff);
  for (const name of untracked) {
    const absolute = resolve(repoRoot, name);
    const info = await stat(absolute).catch(() => undefined);
    if (!info?.isFile()) continue;
    hash.update(name).update(await readFile(absolute));
  }
  return {
    commit: git(repoRoot, ['rev-parse', 'HEAD']).toString('utf8').trim(),
    dirty: entries.length > 0,
    dirtyHash: hash.digest('hex'),
  };
}

export async function createRunManifest({
  repoRoot,
  pdfPath,
  specimen,
  producer,
  environment,
  expectedTools = [],
  rois = [],
}) {
  requireFields(environment, ['os', 'appVersion', 'displayResolution', 'displayScale', 'locale', 'theme']);
  const absolutePdf = resolve(pdfPath);
  return {
    schema: 'butter-paper/bluebeam-compat-run',
    version: MANIFEST_VERSION,
    specimen,
    producer,
    pdf: {
      file: relative(resolve(repoRoot), absolutePdf),
      sha256: await sha256File(absolutePdf),
    },
    git: await gitWorktreeIdentity(repoRoot),
    environment: {
      ...environment,
      fonts: [...(environment.fonts ?? [])].sort(),
    },
    expectedTools: [...expectedTools].sort(),
    rois: [...rois].sort((a, b) => a.id.localeCompare(b.id)),
  };
}

export function compatibilityIdentity(manifest) {
  return {
    schema: manifest.schema,
    version: manifest.version,
    specimen: manifest.specimen,
    pdfSha256: manifest.pdf?.sha256,
    os: manifest.environment?.os,
    appVersion: manifest.environment?.appVersion,
    displayResolution: manifest.environment?.displayResolution,
    displayScale: manifest.environment?.displayScale,
    locale: manifest.environment?.locale,
    theme: manifest.environment?.theme,
    profile: manifest.environment?.profile ?? null,
    fonts: [...(manifest.environment?.fonts ?? [])].sort(),
    expectedTools: [...(manifest.expectedTools ?? [])].sort(),
    rois: (manifest.rois ?? []).map(({ id, page, x, y, width, height, mask, exclusionMask, maximumRegistrationOffset, thresholds }) => ({
      id, page, x, y, width, height,
      mask: mask ?? null,
      exclusionMask: exclusionMask ? { sha256: exclusionMask.sha256 ?? null, threshold: exclusionMask.threshold ?? 128, invert: exclusionMask.invert ?? false } : null,
      maximumRegistrationOffset: maximumRegistrationOffset ?? 8,
      thresholds: thresholds ?? {},
    })),
  };
}

export function interoperabilityIdentity(manifest) {
  const identity = compatibilityIdentity(manifest);
  const { os: _os, appVersion: _appVersion, profile: _profile, ...captureContract } = identity;
  return captureContract;
}

export function validateRunManifest(manifest, { requireImages = false, strictHashes = true } = {}) {
  const errors = [];
  if (manifest?.schema !== 'butter-paper/bluebeam-compat-run') errors.push('schema must be butter-paper/bluebeam-compat-run');
  if (manifest?.version !== MANIFEST_VERSION) errors.push(`version must be ${MANIFEST_VERSION}`);
  if (!manifest?.specimen) errors.push('specimen is required');
  if (strictHashes && !/^[a-f0-9]{64}$/i.test(String(manifest?.pdf?.sha256 ?? ''))) errors.push('pdf.sha256 must be a SHA-256 digest');
  try { requireFields(manifest?.environment, ['os', 'appVersion', 'displayResolution', 'displayScale', 'locale', 'theme']); } catch (error) { errors.push(error.message); }
  const ids = new Set();
  for (const [index, roi] of (manifest?.rois ?? []).entries()) {
    const label = `rois[${index}]`;
    if (!roi?.id || typeof roi.id !== 'string') errors.push(`${label}.id is required`);
    else if (ids.has(roi.id)) errors.push(`${label}.id duplicates ${roi.id}`);
    else ids.add(roi.id);
    if (!Number.isInteger(roi?.page) || roi.page < 1) errors.push(`${label}.page must be a positive integer`);
    for (const field of ['x', 'y']) if (!Number.isInteger(roi?.[field]) || roi[field] < 0) errors.push(`${label}.${field} must be a non-negative integer`);
    for (const field of ['width', 'height']) if (!Number.isInteger(roi?.[field]) || roi[field] < 1) errors.push(`${label}.${field} must be a positive integer`);
    if (requireImages && (!roi?.image || typeof roi.image !== 'string')) errors.push(`${label}.image is required`);
    if (roi?.imageSha256 !== undefined && !/^[a-f0-9]{64}$/i.test(String(roi.imageSha256))) errors.push(`${label}.imageSha256 must be a SHA-256 digest`);
    if (roi?.exclusionMask?.sha256 !== undefined && !/^[a-f0-9]{64}$/i.test(String(roi.exclusionMask.sha256))) errors.push(`${label}.exclusionMask.sha256 must be a SHA-256 digest`);
    if (roi?.maximumRegistrationOffset !== undefined && (!Number.isInteger(roi.maximumRegistrationOffset) || roi.maximumRegistrationOffset < 0)) errors.push(`${label}.maximumRegistrationOffset must be a non-negative integer`);
    for (const [name, value] of Object.entries(roi?.thresholds ?? {})) if (typeof value !== 'number' || Number.isNaN(value)) errors.push(`${label}.thresholds.${name} must be numeric`);
  }
  if (errors.length) {
    const error = new Error(`Invalid compatibility manifest:\n${errors.map((item) => `- ${item}`).join('\n')}`);
    error.code = 'BLUEBEAM_MANIFEST_INVALID';
    error.errors = errors;
    throw error;
  }
  return manifest;
}

export async function verifyManifestArtifacts(manifest, manifestPath, { requireHashes = manifest?.provenance?.requireArtifactHashes === true } = {}) {
  validateRunManifest(manifest, { requireImages: true, strictHashes: true });
  const artifacts = [];
  for (const roi of manifest.rois ?? []) {
    artifacts.push(await verifyArtifact({
      manifestPath,
      relativePath: roi.image,
      expectedSha256: roi.imageSha256,
      requireHash: requireHashes,
      label: `ROI ${roi.id} image`,
    }));
    if (roi.exclusionMask?.image) artifacts.push(await verifyArtifact({
      manifestPath,
      relativePath: roi.exclusionMask.image,
      expectedSha256: roi.exclusionMask.sha256,
      requireHash: requireHashes,
      label: `ROI ${roi.id} exclusion mask`,
    }));
  }
  return { passed: true, requireHashes, artifacts };
}

export function assertComparableManifests(baseline, candidate, { mode = 'same-environment' } = {}) {
  if (!['same-environment', 'interoperability'].includes(mode)) {
    throw new Error(`Unknown comparison mode: ${mode}`);
  }
  const identity = mode === 'interoperability' ? interoperabilityIdentity : compatibilityIdentity;
  const left = identity(baseline);
  const right = identity(candidate);
  const differences = collectDifferences(left, right);
  if (differences.length) {
    const error = new Error(`${mode} compatibility manifests do not match:\n${differences.map((item) => `- ${item}`).join('\n')}`);
    error.code = 'BLUEBEAM_MANIFEST_MISMATCH';
    error.mode = mode;
    error.differences = differences;
    throw error;
  }
}

function collectDifferences(left, right, prefix = '') {
  if (Object.is(left, right)) return [];
  if (Array.isArray(left) || Array.isArray(right)) {
    return JSON.stringify(left) === JSON.stringify(right) ? [] : [`${prefix || 'manifest'} differs`];
  }
  if (left && right && typeof left === 'object' && typeof right === 'object') {
    const keys = [...new Set([...Object.keys(left), ...Object.keys(right)])].sort();
    return keys.flatMap((key) => collectDifferences(left[key], right[key], prefix ? `${prefix}.${key}` : key));
  }
  return [`${prefix}: ${JSON.stringify(left)} != ${JSON.stringify(right)}`];
}

function requireFields(value, fields) {
  for (const field of fields) {
    if (value?.[field] === undefined || value[field] === '') throw new Error(`Missing required environment field: ${field}`);
  }
}

async function verifyArtifact({ manifestPath, relativePath, expectedSha256, requireHash, label }) {
  if (isAbsolute(relativePath)) throw provenanceError(`${label} path must be relative to its manifest`);
  const manifestDirectory = dirname(resolve(manifestPath));
  const absolutePath = resolve(manifestDirectory, relativePath);
  const escaped = relative(manifestDirectory, absolutePath);
  if (escaped === '..' || escaped.startsWith('../')) throw provenanceError(`${label} path escapes its manifest directory`);
  let candidate = manifestDirectory;
  for (const segment of relativePath.split(/[\\/]/).filter((item) => item !== '' && item !== '.')) {
    candidate = resolve(candidate, segment);
    const componentInfo = await lstat(candidate).catch(() => undefined);
    if (componentInfo?.isSymbolicLink()) {
      throw provenanceError(`${label} must not traverse a symlink`);
    }
  }
  const lexicalInfo = await lstat(absolutePath).catch(() => undefined);
  if (!lexicalInfo || lexicalInfo.isSymbolicLink()) {
    throw provenanceError(`${label} must be a real contained file, not a symlink`);
  }
  const canonicalDirectory = await realpath(manifestDirectory);
  const canonicalPath = await realpath(absolutePath).catch(() => undefined);
  if (!canonicalPath) throw provenanceError(`${label} does not exist: ${relativePath}`);
  const canonicalEscape = relative(canonicalDirectory, canonicalPath);
  if (canonicalEscape === '..' || canonicalEscape.startsWith('../') || isAbsolute(canonicalEscape)) {
    throw provenanceError(`${label} resolves outside its manifest directory`);
  }
  const info = await stat(canonicalPath).catch(() => undefined);
  if (!info?.isFile()) throw provenanceError(`${label} does not exist: ${relativePath}`);
  if (requireHash && !expectedSha256) throw provenanceError(`${label} is missing its SHA-256 provenance`);
  const actualSha256 = await sha256File(canonicalPath);
  if (expectedSha256 && actualSha256 !== expectedSha256.toLowerCase()) throw provenanceError(`${label} SHA-256 does not match its manifest`);
  return { file: relativePath, bytes: info.size, sha256: actualSha256, hashDeclared: Boolean(expectedSha256) };
}

function provenanceError(message) {
  const error = new Error(message);
  error.code = 'BLUEBEAM_PROVENANCE_INVALID';
  return error;
}

function git(repoRoot, args) {
  return execFileSync('git', args, { cwd: repoRoot, encoding: 'buffer', maxBuffer: 128 * 1024 * 1024 });
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}
