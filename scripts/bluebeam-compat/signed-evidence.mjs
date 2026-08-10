import { createHash } from 'node:crypto';
import { lstat, readFile, realpath, stat } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve } from 'node:path';
import { validateOracleResult } from './signed-oracle.mjs';

const MANIFEST_VERSION = 1;
const HEX_DIGITS = /^[0-9a-f]+$/i;
const SECRET_VALUE = /-----BEGIN [^-]*PRIVATE KEY-----|(?:password|passphrase|private[_ -]?key|pfx(?:bytes|path)?|pkcs12|token|secret)\s*[:=]/i;
const ABSOLUTE_PATH = /(?:^|[\s=(])(?:file:\/\/|(?:\\\\)+|\/(?:Users|home|private\/tmp|var\/folders|tmp)\/|[A-Za-z]:[\\/])/i;

export async function inspectSignedPdf(pdfPath) {
  return inspectSignedPdfBytes(await readFile(pdfPath));
}

export function inspectSignedPdfBytes(input) {
  const bytes = Buffer.from(input);
  const text = bytes.toString('latin1');
  const signatures = [];
  const byteRangePattern = /\/ByteRange\s*\[([^\]]*)\]/g;
  let match;
  while ((match = byteRangePattern.exec(text))) {
    const values = parsePdfIntegers(match[1]);
    const rangeCheck = validateByteRange(values, bytes.length);
    const contents = findContents(text, match.index);
    const cmsCheck = inspectCms(contents);
    const ranges = rangeCheck.valid ? signedRanges(bytes, values) : null;
    signatures.push({
      index: signatures.length,
      byteRange: values,
      byteRangeValid: rangeCheck.valid,
      byteRangeError: rangeCheck.error,
      signedByteRangeSha256: ranges ? sha256(ranges) : null,
      cms: cmsCheck,
      revision: revisionAt(text, match.index),
      structuralOnly: true,
    });
  }
  const eofOffsets = [...text.matchAll(/%%EOF/g)].map((item) => item.index);
  const previousOffsets = [...text.matchAll(/\/Prev\s+(\d+)/g)].map((item) => Number(item[1]));
  const dss = inspectDssVri(text);
  return {
    schema: 'butter-paper/signed-pdf-structural-inspection',
    version: 1,
    bytes: bytes.length,
    pdfHeader: text.startsWith('%PDF-'),
    signatures,
    signatureCount: signatures.length,
    signatureWidgetCount: countMatches(text, /\/Subtype\s*\/Widget[\s\S]{0,500}?\/FT\s*\/Sig/g),
    revisionAncestry: {
      eofCount: eofOffsets.length,
      eofOffsets,
      previousOffsets,
      incremental: eofOffsets.length > 1 || previousOffsets.length > 0,
      complete: eofOffsets.length === previousOffsets.length + 1,
    },
    dssVri: dss,
    claimsCryptographicValidity: false,
    inspectionLimit: 'This report describes bytes and PDF objects only; it does not validate CMS signatures, certificates, trust, revocation, or signing policy.',
  };
}

export function assertSignedByteRangesUnchanged(before, after) {
  const errors = [];
  if (before?.signatureCount !== after?.signatureCount) errors.push('signature count changed');
  const count = Math.min(before?.signatures?.length ?? 0, after?.signatures?.length ?? 0);
  for (let index = 0; index < count; index += 1) {
    const left = before.signatures[index];
    const right = after.signatures[index];
    if (!left.byteRangeValid || !right.byteRangeValid) errors.push(`signature ${index} has an invalid ByteRange`);
    if (left.signedByteRangeSha256 !== right.signedByteRangeSha256) errors.push(`signature ${index} signed bytes changed`);
  }
  if (errors.length) throw evidenceError('SIGNED_BYTE_RANGE_CHANGED', errors);
  return { passed: true, signatures: count };
}

export function validateSignedManifest(manifest, { manifestPath, contract, verifyFiles = false } = {}) {
  const errors = [];
  if (manifest?.schema !== 'butter-paper/signed-evidence-manifest') errors.push('schema must be butter-paper/signed-evidence-manifest');
  if (manifest?.version !== MANIFEST_VERSION) errors.push('version must be 1');
  if (!/^[A-H]$/.test(String(manifest?.flowId ?? ''))) errors.push('flowId must reference A-H');
  if (!manifest?.source?.file) errors.push('source.file is required');
  if (!/^[a-f0-9]{64}$/i.test(String(manifest?.source?.sha256 ?? ''))) errors.push('source.sha256 must be a SHA-256 digest');
  if (manifest?.output?.file && manifest.output.file === manifest?.source?.file) errors.push('output.file must be Save As/new and differ from source.file');
  for (const [label, artifact] of [['source', manifest?.source], ['output', manifest?.output]]) {
    if (!artifact) continue;
    if (artifact.file !== undefined) {
      if (isAbsolute(String(artifact.file))) errors.push(`${label}.file must be relative`);
      if (containsTraversal(String(artifact.file))) errors.push(`${label}.file must not contain path traversal`);
    }
    if (artifact.sha256 !== undefined && !/^[a-f0-9]{64}$/i.test(String(artifact.sha256))) errors.push(`${label}.sha256 must be a SHA-256 digest`);
  }
  if (manifest?.structuralInspection?.claimsCryptographicValidity !== false) errors.push('structural inspection cannot claim cryptographic validity');
  if (!['passed', 'failed', 'not-run', 'unavailable'].includes(manifest?.commercialEvidence?.status)) errors.push('commercialEvidence.status must be passed, failed, not-run, or unavailable');
  if (manifest?.commercialEvidence?.status === 'passed' && (!Array.isArray(manifest.commercialEvidence.evidenceFiles) || manifest.commercialEvidence.evidenceFiles.length === 0)) errors.push('commercial evidence cannot be passed without evidence files');
  if (!manifest?.oracle) errors.push('oracle result is required');
  else {
    try { validateOracleResult(manifest.oracle); } catch (error) { errors.push(...(error.errors ?? [error.message])); }
  }
  errors.push(...scanSecretAndPathLeakage(manifest).map((finding) => `${finding.type} at ${finding.path}: ${finding.message}`));
  if (errors.length) throw evidenceError('SIGNED_MANIFEST_INVALID', errors);
  if (verifyFiles && manifestPath) return verifyManifestFiles(manifest, manifestPath);
  if (contract?.flows && !contract.flows.some((flow) => flow.id === manifest.flowId)) throw evidenceError('SIGNED_MANIFEST_INVALID', [`flowId ${manifest.flowId} is not in the signed interoperability contract`]);
  return { passed: true, manifest };
}

export async function verifyManifestFiles(manifest, manifestPath) {
  const root = dirname(resolve(manifestPath));
  const artifacts = [];
  for (const [label, artifact, required] of [['source', manifest.source, true], ['output', manifest.output, false]]) {
    if (!artifact?.file && !required) continue;
    const safePath = await containedRealFile(root, artifact.file, label);
    const bytes = await readFile(safePath);
    const actual = sha256(bytes);
    if (actual !== artifact.sha256.toLowerCase()) throw evidenceError('SIGNED_MANIFEST_ARTIFACT_MISMATCH', [`${label}.sha256 does not match ${artifact.file}`]);
    artifacts.push({ label, file: artifact.file, bytes: bytes.length, sha256: actual });
  }
  return { passed: true, artifacts };
}

export function scanSecretAndPathLeakage(value, path = '$', findings = []) {
  if (typeof value === 'string') {
    if (SECRET_VALUE.test(value)) findings.push({ type: 'secret', path, message: 'secret-bearing value is not permitted in signed evidence' });
    if (ABSOLUTE_PATH.test(value) || containsTraversal(value)) findings.push({ type: 'path', path, message: 'absolute or traversing path is not permitted in signed evidence' });
    return findings;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => scanSecretAndPathLeakage(item, `${path}[${index}]`, findings));
    return findings;
  }
  if (value && typeof value === 'object') {
    for (const [key, item] of Object.entries(value)) scanSecretAndPathLeakage(item, `${path}.${key}`, findings);
  }
  return findings;
}

export function createSignedEvidenceManifest({ flowId, source, output, structuralInspection, oracle, commercialEvidence = { status: 'not-run', reason: 'No approved commercial application run was available.' }, postvalidation }) {
  const manifest = {
    schema: 'butter-paper/signed-evidence-manifest',
    version: MANIFEST_VERSION,
    flowId,
    source: artifactDescriptor(source),
    output: output ? artifactDescriptor(output) : null,
    structuralInspection: structuralInspection ?? null,
    oracle,
    commercialEvidence,
    postvalidation: postvalidation ?? null,
  };
  validateSignedManifest(manifest);
  return manifest;
}

function artifactDescriptor(artifact) {
  if (!artifact?.file || isAbsolute(artifact.file) || containsTraversal(artifact.file)) throw evidenceError('SIGNED_MANIFEST_INVALID', ['artifact paths must be relative and contained']);
  return { file: artifact.file, bytes: artifact.bytes, sha256: artifact.sha256 };
}

async function containedRealFile(root, relativePath, label) {
  if (!relativePath || isAbsolute(relativePath) || containsTraversal(relativePath)) throw evidenceError('SIGNED_MANIFEST_PATH_INVALID', [`${label}.file is not a safe relative path`]);
  let current = root;
  for (const segment of relativePath.split(/[\\/]/).filter(Boolean)) {
    current = resolve(current, segment);
    const info = await lstat(current).catch(() => undefined);
    if (info?.isSymbolicLink()) throw evidenceError('SIGNED_MANIFEST_PATH_INVALID', [`${label}.file traverses a symlink`]);
  }
  const canonicalRoot = await realpath(root);
  const canonical = await realpath(resolve(root, relativePath)).catch(() => undefined);
  if (!canonical) throw evidenceError('SIGNED_MANIFEST_PATH_INVALID', [`${label}.file does not exist`]);
  const escaped = relative(canonicalRoot, canonical);
  if (escaped === '..' || escaped.startsWith('../') || isAbsolute(escaped)) throw evidenceError('SIGNED_MANIFEST_PATH_INVALID', [`${label}.file escapes evidence root`]);
  const info = await stat(canonical);
  if (!info.isFile()) throw evidenceError('SIGNED_MANIFEST_PATH_INVALID', [`${label}.file is not a regular file`]);
  return canonical;
}

function findContents(text, byteRangeIndex) {
  const candidates = [
    text.lastIndexOf('/Contents', byteRangeIndex),
    text.indexOf('/Contents', byteRangeIndex),
  ].filter((index) => index >= 0).sort((left, right) => Math.abs(left - byteRangeIndex) - Math.abs(right - byteRangeIndex));
  for (const index of candidates) {
    const contentsPattern = /\/Contents\s*<([0-9a-f\s]*)>/iy;
    contentsPattern.lastIndex = index;
    const match = contentsPattern.exec(text);
    if (match) return Buffer.from(match[1].replace(/\s/g, ''), 'hex');
  }
  return null;
}

function inspectCms(contents) {
  if (!contents) return { present: false, der: false, lengthValid: false, structuralStatus: 'missing' };
  const der = contents[0] === 0x30;
  const length = der ? derLength(contents) : null;
  return { present: true, der, lengthValid: Number.isInteger(length) && length <= contents.length, byteLength: contents.length, structuralStatus: der && Number.isInteger(length) && length <= contents.length ? 'present-structural-only' : 'malformed' };
}

function derLength(bytes) {
  if (bytes.length < 2) return null;
  if (bytes[1] < 0x80) return bytes[1] + 2;
  const count = bytes[1] & 0x7f;
  if (count === 0 || count > 4 || bytes.length < count + 2) return null;
  let value = 0;
  for (let index = 0; index < count; index += 1) value = value * 256 + bytes[index + 2];
  return value + count + 2;
}

function validateByteRange(values, length) {
  if (values.length !== 4) return { valid: false, error: 'ByteRange must contain four integers' };
  if (!values.every((value) => Number.isSafeInteger(value) && value >= 0)) return { valid: false, error: 'ByteRange values must be non-negative safe integers' };
  if (values[0] !== 0) return { valid: false, error: 'ByteRange must start at zero' };
  if (values[1] > values[2] || values[2] + values[3] > length || values[2] + values[3] !== length) return { valid: false, error: 'ByteRange exceeds file bounds, has an inverted gap, or does not cover the file tail' };
  if (values[1] + values[0] > length || values[2] > length) return { valid: false, error: 'ByteRange segment exceeds file bounds' };
  return { valid: true, error: null };
}

function signedRanges(bytes, values) {
  return Buffer.concat([bytes.subarray(values[0], values[0] + values[1]), bytes.subarray(values[2], values[2] + values[3])]);
}

function parsePdfIntegers(value) {
  return value.trim().split(/\s+/).filter(Boolean).map((item) => Number(item)).filter((item) => Number.isFinite(item));
}

function revisionAt(text, offset) {
  const before = text.slice(0, offset);
  return { index: (before.match(/%%EOF/g) ?? []).length, eofCountBefore: (before.match(/%%EOF/g) ?? []).length, previousOffset: [...before.matchAll(/\/Prev\s+(\d+)/g)].at(-1)?.[1] ? Number([...before.matchAll(/\/Prev\s+(\d+)/g)].at(-1)[1]) : null };
}

function inspectDssVri(text) {
  const dssPresent = /\/DSS\b/.test(text);
  const vriPresent = /\/VRI\b/.test(text);
  const vriBodies = [...text.matchAll(/\/VRI\s*<<(.*?)>>/gs)].map((item) => item[1]);
  const vriCount = vriBodies.reduce((total, body) => total + (body.match(/\/[0-9A-Fa-f]{40,64}\s*(?=<<)/g) ?? []).length, 0);
  return { dssPresent, vriPresent, vriCount, status: dssPresent && vriPresent ? 'present-structural-only' : 'absent' };
}

function countMatches(text, pattern) { return [...text.matchAll(pattern)].length; }
function containsTraversal(value) { return /(?:^|[\\/])\.\.(?:[\\/]|$)/.test(value); }
function sha256(value) { return createHash('sha256').update(value).digest('hex'); }
function evidenceError(code, errors) { const error = new Error(`${code}:\n${errors.map((item) => `- ${item}`).join('\n')}`); error.code = code; error.errors = errors; return error; }
