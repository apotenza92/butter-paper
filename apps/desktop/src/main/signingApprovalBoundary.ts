import type {
  CertificationPermission,
  SigningApprovalRequest,
  SigningApprovalResult,
  SigningDigestAlgorithm,
  SigningFieldRequest,
  SigningIdentitySelectionResult,
  SigningOperation,
} from '../shared/signingProtocol';

const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const MAX_HANDLE_LENGTH = 256;
const MAX_FIELD_NAME_LENGTH = 512;
const MAX_METADATA_LENGTH = 1024;

const REQUIRED_REQUEST_KEYS = [
  'appearance',
  'certificateSha256',
  'digestAlgorithm',
  'documentHandle',
  'field',
  'identityHandle',
  'operation',
  'profile',
  'protocolVersion',
  'targetHandle',
] as const;
const OPTIONAL_REQUEST_KEYS = ['contact', 'location', 'reason'] as const;

export class SigningApprovalBoundaryError extends Error {
  constructor(readonly code: 'INVALID_REQUEST', message: string) {
    super(message);
    this.name = 'SigningApprovalBoundaryError';
  }
}

/**
 * Validate the closed renderer approval contract before it reaches any
 * privileged code. This deliberately has no password, PFX, path, or byte
 * fields to accept, and rejects unknown keys instead of silently ignoring
 * them.
 */
export function assertSigningApprovalRequest(request: unknown): asserts request is SigningApprovalRequest {
  if (!isRecord(request)) throw invalidRequest();
  const operation = request.operation;
  if (operation !== 'sign' && operation !== 'certify') throw invalidRequest();

  const requiredKeys = operation === 'certify'
    ? [...REQUIRED_REQUEST_KEYS, 'certificationPermission']
    : REQUIRED_REQUEST_KEYS;
  assertExactKeys(request, requiredKeys, OPTIONAL_REQUEST_KEYS);
  if (request.protocolVersion !== 1 || request.profile !== 'PAdES-B-B') throw invalidRequest();
  assertBoundedHandle(request.documentHandle);
  assertBoundedHandle(request.targetHandle);
  assertBoundedHandle(request.identityHandle);
  if (typeof request.certificateSha256 !== 'string' || !SHA256_PATTERN.test(request.certificateSha256)) {
    throw invalidRequest();
  }
  if (!isDigestAlgorithm(request.digestAlgorithm)) throw invalidRequest();
  assertSigningField(request.field);
  assertSigningAppearance(request.appearance);
  for (const key of ['reason', 'location', 'contact'] as const) {
    if (request[key] !== undefined) assertBoundedText(request[key], MAX_METADATA_LENGTH);
  }
  if (operation === 'certify' && !isCertificationPermission(request.certificationPermission)) {
    throw invalidRequest();
  }
}

/**
 * Current release behaviour is intentionally fail-closed. Keeping this in a
 * pure function makes the disabled capability observable and testable without
 * launching Electron or a native signing process.
 */
export function signingCapabilityDisabledResult(operation: SigningOperation): SigningApprovalResult {
  return {
    outcome: 'failed',
    operation,
    errorCode: 'CAPABILITY_DISABLED',
  };
}

/** Identity inspection is also unavailable while certificate mutation is disabled. */
export function signingIdentitySelectionDisabledResult(): SigningIdentitySelectionResult {
  return {
    outcome: 'failed',
    errorCode: 'CAPABILITY_DISABLED',
  };
}

function assertSigningField(value: unknown): asserts value is SigningFieldRequest {
  if (!isRecord(value) || (value.mode !== 'existing' && value.mode !== 'new')) throw invalidRequest();
  if (value.mode === 'existing') {
    assertExactKeys(value, ['mode', 'name']);
    assertBoundedText(value.name, MAX_FIELD_NAME_LENGTH);
    return;
  }
  assertExactKeys(value, ['lock', 'mode', 'name', 'pageIndex', 'pageRotation', 'rect']);
  assertBoundedText(value.name, MAX_FIELD_NAME_LENGTH);
  if (!Number.isSafeInteger(value.pageIndex) || value.pageIndex < 0 || value.pageIndex > 100_000) {
    throw invalidRequest();
  }
  if (value.pageRotation !== 0 && value.pageRotation !== 90 && value.pageRotation !== 180 && value.pageRotation !== 270) {
    throw invalidRequest();
  }
  if (!isRecord(value.rect)) throw invalidRequest();
  assertExactKeys(value.rect, ['height', 'width', 'x', 'y']);
  for (const coordinate of ['x', 'y', 'width', 'height'] as const) {
    if (!Number.isFinite(value.rect[coordinate]) || Math.abs(value.rect[coordinate]) > 1_000_000) {
      throw invalidRequest();
    }
  }
  if (value.rect.width <= 0 || value.rect.height <= 0) throw invalidRequest();
  if (value.lock !== null) {
    if (!isRecord(value.lock)) throw invalidRequest();
    assertExactKeys(value.lock, ['action', 'fieldNames']);
    if (value.lock.action !== 'all' && value.lock.action !== 'include' && value.lock.action !== 'exclude') {
      throw invalidRequest();
    }
    if (!Array.isArray(value.lock.fieldNames) || value.lock.fieldNames.length > 1024) throw invalidRequest();
    for (const fieldName of value.lock.fieldNames) assertBoundedText(fieldName, MAX_FIELD_NAME_LENGTH);
  }
}

function assertSigningAppearance(value: unknown): void {
  if (!isRecord(value) || (value.mode !== 'visible' && value.mode !== 'invisible')) throw invalidRequest();
  if (value.mode === 'invisible') {
    assertExactKeys(value, ['mode']);
    return;
  }
  assertExactKeys(value, ['assetHandle', 'mode']);
  assertBoundedHandle(value.assetHandle);
}

function assertExactKeys(
  value: Record<string, unknown>,
  requiredKeys: readonly string[],
  optionalKeys: readonly string[] = [],
): void {
  const expected = new Set([...requiredKeys, ...optionalKeys]);
  if (Object.keys(value).some((key) => !expected.has(key)) || requiredKeys.some((key) => !(key in value))) {
    throw invalidRequest();
  }
}

function assertBoundedHandle(value: unknown): asserts value is string {
  assertBoundedText(value, MAX_HANDLE_LENGTH);
}

function assertBoundedText(value: unknown, maximum: number): asserts value is string {
  if (typeof value !== 'string'
    || value.length === 0
    || value.length > maximum
    || /[\u0000-\u001f\u007f]/u.test(value)) {
    throw invalidRequest();
  }
}

function isDigestAlgorithm(value: unknown): value is SigningDigestAlgorithm {
  return value === 'SHA-256' || value === 'SHA-384' || value === 'SHA-512';
}

function isCertificationPermission(value: unknown): value is CertificationPermission {
  return value === 'no-changes'
    || value === 'form-filling-and-signatures'
    || value === 'form-filling-signatures-and-annotations';
}

function isRecord(value: unknown): value is Record<string, any> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function invalidRequest(): SigningApprovalBoundaryError {
  return new SigningApprovalBoundaryError('INVALID_REQUEST', 'The signing approval request is invalid.');
}
