import { createHash } from 'node:crypto';
import { isCanonicalSignatureValidationTime } from '@butter-paper/core';
import {
  OFFLINE_SIGNATURE_TRUST_MAX_ANCHORS,
  OFFLINE_SIGNATURE_TRUST_MAX_CERTIFICATE_DER_BYTES,
  OFFLINE_SIGNATURE_TRUST_MAX_TOTAL_DER_BYTES,
  OFFLINE_SIGNATURE_TRUST_POLICY_ID,
  OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
  offlineSignatureTrustConfigurationSha256,
} from './signatureTrustPolicy';

const SHA256_PATTERN = /^[a-f0-9]{64}$/;

export interface PdfSignatureCoreTrustPolicyAnchorInput {
  readonly sha256Fingerprint: string;
  readonly certificateDer: Uint8Array;
}

export interface PdfSignatureCoreTrustPolicyInput {
  readonly policyId: typeof OFFLINE_SIGNATURE_TRUST_POLICY_ID;
  readonly policyVersion: typeof OFFLINE_SIGNATURE_TRUST_POLICY_VERSION;
  readonly configurationSha256: string;
  readonly exactCertificateAnchors: readonly PdfSignatureCoreTrustPolicyAnchorInput[];
}

export interface PdfSignatureCoreTrustPolicyPayload {
  readonly policyId: typeof OFFLINE_SIGNATURE_TRUST_POLICY_ID;
  readonly policyVersion: typeof OFFLINE_SIGNATURE_TRUST_POLICY_VERSION;
  readonly configurationSha256: string;
  readonly exactCertificateAnchors: readonly {
    readonly sha256Fingerprint: string;
    readonly derBase64: string;
  }[];
}

export interface PdfSignatureCoreValidationPayload {
  readonly inputPath: string;
  /** Always explicit in the privileged request; omission must never enable I/O. */
  readonly onlineValidation: boolean;
  /** Main-process-only deterministic reference seam. Never expose through renderer IPC. */
  readonly validationClock?: {
    readonly mode: 'fixed-reference';
    readonly instant: string;
  };
  readonly trustPolicy?: PdfSignatureCoreTrustPolicyPayload;
}

export interface PdfSignatureCoreValidationBoundaryResult {
  readonly inputSha256: string;
  readonly validationMode: 'offline' | 'online';
  readonly validationTime: string;
  readonly validationTimeProvenance: 'observed-system-utc' | 'caller-supplied-fixed-reference';
  readonly trust: {
    readonly policyId: string;
    readonly policyVersion: number;
    readonly configurationSha256: string;
    readonly configuredExactCertificateFingerprints: readonly string[];
  };
}

/**
 * Creates the privileged validate payload. Online evidence refresh is disabled
 * by default and can only be enabled by an explicit, per-operation decision in
 * the main process. This object is sent over stdin, never process arguments.
 */
export function createPdfSignatureCoreValidationPayload(
  inputPath: string,
  options: {
    onlineValidationAuthorized?: boolean;
    /** Privileged deterministic/reference use only; must be historical canonical UTC. */
    fixedReferenceValidationTime?: string;
    trustPolicy?: PdfSignatureCoreTrustPolicyInput;
  } = {},
): PdfSignatureCoreValidationPayload {
  if (typeof inputPath !== 'string' || inputPath.length === 0) {
    throw new TypeError('A non-empty main-process PDF path is required.');
  }
  const fixedReference = options.fixedReferenceValidationTime;
  if (fixedReference !== undefined
    && (!isCanonicalSignatureValidationTime(fixedReference)
      || Date.parse(fixedReference) > Date.now())) {
    throw new TypeError('Fixed reference validation time must be canonical historical UTC.');
  }
  const payload: PdfSignatureCoreValidationPayload = {
    inputPath,
    onlineValidation: options.onlineValidationAuthorized === true,
    ...(fixedReference === undefined ? {} : {
      validationClock: { mode: 'fixed-reference' as const, instant: fixedReference },
    }),
  };
  if (options.trustPolicy) {
    return { ...payload, trustPolicy: serializeTrustPolicy(options.trustPolicy) };
  }
  return payload;
}

/**
 * Binds a sidecar result to the bytes the main process actually selected and
 * to the requested network mode. Callers must discard the complete result if
 * this check fails; a stale or unexpectedly-online report is never reusable.
 */
export function assertPdfSignatureCoreValidationBoundary<T extends PdfSignatureCoreValidationBoundaryResult>(
  result: T,
  expected: {
    inputSha256: string;
    onlineValidationAuthorized?: boolean;
    fixedReferenceValidationTime?: string;
    validationStartedAt: string;
    validationCompletedAt: string;
    trustPolicy?: PdfSignatureCoreTrustPolicyInput;
  },
): T {
  if (!SHA256_PATTERN.test(expected.inputSha256)) {
    throw new TypeError('Expected input SHA-256 must be a lowercase hexadecimal digest.');
  }
  if (!SHA256_PATTERN.test(result.inputSha256)) {
    throw new PdfSignatureCoreSafetyError('INVALID_RESULT', 'Signature validation returned an invalid input digest.');
  }
  if (result.inputSha256 !== expected.inputSha256) {
    throw new PdfSignatureCoreSafetyError('STALE_RESULT', 'Signature validation does not describe the current PDF bytes.');
  }
  const expectedMode = expected.onlineValidationAuthorized === true ? 'online' : 'offline';
  if (result.validationMode !== expectedMode) {
    throw new PdfSignatureCoreSafetyError(
      'NETWORK_MODE_MISMATCH',
      expectedMode === 'offline'
        ? 'Signature validation unexpectedly reported online network use.'
        : 'Signature validation did not perform the explicitly requested online refresh.',
    );
  }
  assertValidationClockBoundary(result, expected);
  const expectedTrust = expected.trustPolicy
    ? serializeTrustPolicy(expected.trustPolicy)
    : {
        policyId: OFFLINE_SIGNATURE_TRUST_POLICY_ID,
        policyVersion: OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
        configurationSha256: offlineSignatureTrustConfigurationSha256([]),
        exactCertificateAnchors: [],
      };
  const expectedFingerprints = expectedTrust.exactCertificateAnchors.map((anchor) => anchor.sha256Fingerprint);
  if (result.trust.policyId !== expectedTrust.policyId
    || result.trust.policyVersion !== expectedTrust.policyVersion
    || result.trust.configurationSha256 !== expectedTrust.configurationSha256
    || result.trust.configuredExactCertificateFingerprints.length !== expectedFingerprints.length
    || result.trust.configuredExactCertificateFingerprints.some((fingerprint, index) => (
      fingerprint !== expectedFingerprints[index]
    ))) {
    throw new PdfSignatureCoreSafetyError(
      'TRUST_POLICY_MISMATCH',
      'Signature validation did not use the requested offline trust-policy snapshot.',
    );
  }
  return result;
}

function assertValidationClockBoundary(
  result: PdfSignatureCoreValidationBoundaryResult,
  expected: {
    fixedReferenceValidationTime?: string;
    validationStartedAt: string;
    validationCompletedAt: string;
  },
): void {
  const startedAt = Date.parse(expected.validationStartedAt);
  const completedAt = Date.parse(expected.validationCompletedAt);
  if (!Number.isFinite(startedAt) || !Number.isFinite(completedAt) || startedAt > completedAt) {
    throw new TypeError('Validation clock bounds must be a chronological pair of UTC instants.');
  }
  if (!isCanonicalSignatureValidationTime(result.validationTime)) {
    throw new PdfSignatureCoreSafetyError(
      'VALIDATION_CLOCK_MISMATCH',
      'Signature validation returned a malformed validation clock.',
    );
  }
  if (expected.fixedReferenceValidationTime !== undefined) {
    if (result.validationTime !== expected.fixedReferenceValidationTime
      || result.validationTimeProvenance !== 'caller-supplied-fixed-reference') {
      throw new PdfSignatureCoreSafetyError(
        'VALIDATION_CLOCK_MISMATCH',
        'Signature validation did not use the requested fixed reference clock.',
      );
    }
    return;
  }
  const observed = Date.parse(result.validationTime);
  if (result.validationTimeProvenance !== 'observed-system-utc'
    || observed < startedAt || observed > completedAt) {
    throw new PdfSignatureCoreSafetyError(
      'VALIDATION_CLOCK_MISMATCH',
      'Signature validation system clock falls outside the privileged operation boundary.',
    );
  }
}

function serializeTrustPolicy(policy: PdfSignatureCoreTrustPolicyInput): PdfSignatureCoreTrustPolicyPayload {
  if (!policy || typeof policy !== 'object'
    || policy.policyId !== OFFLINE_SIGNATURE_TRUST_POLICY_ID
    || policy.policyVersion !== OFFLINE_SIGNATURE_TRUST_POLICY_VERSION
    || !SHA256_PATTERN.test(policy.configurationSha256)
    || !Array.isArray(policy.exactCertificateAnchors)
    || policy.exactCertificateAnchors.length > OFFLINE_SIGNATURE_TRUST_MAX_ANCHORS) {
    throw new TypeError('The offline signature trust-policy request is invalid.');
  }
  let totalDerBytes = 0;
  const anchors = policy.exactCertificateAnchors.map((anchor) => {
    if (!anchor || typeof anchor !== 'object'
      || !SHA256_PATTERN.test(anchor.sha256Fingerprint)
      || !(anchor.certificateDer instanceof Uint8Array)
      || anchor.certificateDer.byteLength < 1
      || anchor.certificateDer.byteLength > OFFLINE_SIGNATURE_TRUST_MAX_CERTIFICATE_DER_BYTES) {
      throw new TypeError('An exact-certificate trust anchor is invalid or exceeds the protocol limit.');
    }
    totalDerBytes += anchor.certificateDer.byteLength;
    const actualFingerprint = createHash('sha256').update(anchor.certificateDer).digest('hex');
    if (actualFingerprint !== anchor.sha256Fingerprint) {
      throw new TypeError('An exact-certificate trust anchor fingerprint does not match its public DER bytes.');
    }
    return {
      sha256Fingerprint: anchor.sha256Fingerprint,
      derBase64: Buffer.from(anchor.certificateDer).toString('base64'),
    };
  }).sort((left, right) => left.sha256Fingerprint.localeCompare(right.sha256Fingerprint));
  if (totalDerBytes > OFFLINE_SIGNATURE_TRUST_MAX_TOTAL_DER_BYTES
    || anchors.some((anchor, index) => index > 0 && anchors[index - 1].sha256Fingerprint === anchor.sha256Fingerprint)) {
    throw new TypeError('Exact-certificate trust anchors are duplicated or exceed the total protocol limit.');
  }
  const expectedConfigurationSha256 = offlineSignatureTrustConfigurationSha256(
    anchors.map((anchor) => anchor.sha256Fingerprint),
  );
  if (policy.configurationSha256 !== expectedConfigurationSha256) {
    throw new TypeError('The offline signature trust configuration digest is stale or malformed.');
  }
  return {
    policyId: OFFLINE_SIGNATURE_TRUST_POLICY_ID,
    policyVersion: OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
    configurationSha256: expectedConfigurationSha256,
    exactCertificateAnchors: anchors,
  };
}

export class PdfSignatureCoreSafetyError extends Error {
  constructor(
    readonly code:
      | 'INVALID_RESULT'
      | 'STALE_RESULT'
      | 'NETWORK_MODE_MISMATCH'
      | 'TRUST_POLICY_MISMATCH'
      | 'VALIDATION_CLOCK_MISMATCH',
    message: string,
  ) {
    super(message);
    this.name = 'PdfSignatureCoreSafetyError';
  }
}
