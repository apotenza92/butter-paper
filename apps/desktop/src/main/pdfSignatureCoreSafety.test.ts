import { createHash } from 'node:crypto';
import {
  PdfSignatureCoreSafetyError,
  assertPdfSignatureCoreValidationBoundary,
  createPdfSignatureCoreValidationPayload,
} from './pdfSignatureCoreSafety';
import {
  OFFLINE_SIGNATURE_TRUST_POLICY_ID,
  OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
  offlineSignatureTrustConfigurationSha256,
} from './signatureTrustPolicy';

const EMPTY_TRUST = {
  policyId: OFFLINE_SIGNATURE_TRUST_POLICY_ID,
  policyVersion: OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
  configurationSha256: offlineSignatureTrustConfigurationSha256([]),
  configuredExactCertificateFingerprints: [],
} as const;
const OBSERVED_CLOCK = {
  validationTime: '2026-08-05T00:00:00Z',
  validationTimeProvenance: 'observed-system-utc' as const,
};
const CLOCK_BOUNDS = {
  validationStartedAt: '2026-08-04T23:59:59.000Z',
  validationCompletedAt: '2026-08-05T00:00:01.000Z',
};

describe('PDF signature validation main-process boundary', () => {
  it('makes offline validation the unambiguous default', () => {
    expect(createPdfSignatureCoreValidationPayload('/main-owned/document.pdf')).toEqual({
      inputPath: '/main-owned/document.pdf',
      onlineValidation: false,
    });
    expect(createPdfSignatureCoreValidationPayload('/main-owned/document.pdf', {
      onlineValidationAuthorized: false,
    }).onlineValidation).toBe(false);
  });

  it('requires an explicit per-operation authorization before requesting network use', () => {
    expect(createPdfSignatureCoreValidationPayload('/main-owned/document.pdf', {
      onlineValidationAuthorized: true,
    }).onlineValidation).toBe(true);
    expect(() => createPdfSignatureCoreValidationPayload('')).toThrow(TypeError);
  });

  it('serializes only a canonical historical fixed reference clock over stdin', () => {
    expect(createPdfSignatureCoreValidationPayload('/main-owned/document.pdf', {
      fixedReferenceValidationTime: '2026-08-05T00:00:00Z',
    })).toMatchObject({
      validationClock: { mode: 'fixed-reference', instant: '2026-08-05T00:00:00Z' },
    });
    for (const invalid of [
      '2026-08-05T10:00:00+10:00',
      '2026-08-05T00:00:00.000Z',
      '1899-12-31T23:59:59Z',
      new Date(Date.now() + 60_000).toISOString(),
    ]) {
      expect(() => createPdfSignatureCoreValidationPayload('/main-owned/document.pdf', {
        fixedReferenceValidationTime: invalid,
      })).toThrow(/canonical historical UTC/);
    }
  });

  it('accepts only a report bound to the exact current input bytes and request mode', () => {
    const digest = 'a'.repeat(64);
    const result = {
      inputSha256: digest,
      validationMode: 'offline' as const,
      trust: EMPTY_TRUST,
      inventory: {},
      ...OBSERVED_CLOCK,
    };
    expect(assertPdfSignatureCoreValidationBoundary(result, {
      inputSha256: digest,
      ...CLOCK_BOUNDS,
    })).toBe(result);
  });

  it('serializes a bounded exact-certificate policy and binds the result to it', () => {
    const certificateDer = Uint8Array.from([1, 2, 3, 4]);
    const fingerprint = createHash('sha256').update(certificateDer).digest('hex');
    const trustPolicy = {
      policyId: OFFLINE_SIGNATURE_TRUST_POLICY_ID,
      policyVersion: OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
      configurationSha256: offlineSignatureTrustConfigurationSha256([fingerprint]),
      exactCertificateAnchors: [{ sha256Fingerprint: fingerprint, certificateDer }],
    } as const;
    const payload = createPdfSignatureCoreValidationPayload('/main-owned/document.pdf', { trustPolicy });
    expect(payload.trustPolicy).toEqual({
      policyId: OFFLINE_SIGNATURE_TRUST_POLICY_ID,
      policyVersion: OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
      configurationSha256: trustPolicy.configurationSha256,
      exactCertificateAnchors: [{
        sha256Fingerprint: fingerprint,
        derBase64: Buffer.from(certificateDer).toString('base64'),
      }],
    });
    const result = {
      inputSha256: 'a'.repeat(64),
      validationMode: 'offline' as const,
      ...OBSERVED_CLOCK,
      trust: {
        policyId: trustPolicy.policyId,
        policyVersion: trustPolicy.policyVersion,
        configurationSha256: trustPolicy.configurationSha256,
        configuredExactCertificateFingerprints: [fingerprint],
      },
    };
    expect(assertPdfSignatureCoreValidationBoundary(result, {
      inputSha256: result.inputSha256,
      ...CLOCK_BOUNDS,
      trustPolicy,
    })).toBe(result);
    expect(() => createPdfSignatureCoreValidationPayload('/main-owned/document.pdf', {
      trustPolicy: { ...trustPolicy, configurationSha256: 'f'.repeat(64) },
    })).toThrow(/stale|malformed/i);
  });

  it('rejects malformed and stale result hashes without returning partial evidence', () => {
    expect(() => assertPdfSignatureCoreValidationBoundary(
      { inputSha256: 'not-a-digest', validationMode: 'offline', trust: EMPTY_TRUST, ...OBSERVED_CLOCK },
      { inputSha256: 'a'.repeat(64), ...CLOCK_BOUNDS },
    )).toThrowError(expect.objectContaining<PdfSignatureCoreSafetyError>({
      code: 'INVALID_RESULT', name: 'PdfSignatureCoreSafetyError', message: expect.any(String),
    }));
    expect(() => assertPdfSignatureCoreValidationBoundary(
      { inputSha256: 'b'.repeat(64), validationMode: 'offline', trust: EMPTY_TRUST, ...OBSERVED_CLOCK },
      { inputSha256: 'a'.repeat(64), ...CLOCK_BOUNDS },
    )).toThrowError(expect.objectContaining<PdfSignatureCoreSafetyError>({
      code: 'STALE_RESULT', name: 'PdfSignatureCoreSafetyError', message: expect.any(String),
    }));
  });

  it('rejects any sidecar network-mode disagreement', () => {
    const digest = 'a'.repeat(64);
    expect(() => assertPdfSignatureCoreValidationBoundary(
      { inputSha256: digest, validationMode: 'online', trust: EMPTY_TRUST, ...OBSERVED_CLOCK },
      { inputSha256: digest, ...CLOCK_BOUNDS },
    )).toThrowError(expect.objectContaining<PdfSignatureCoreSafetyError>({
      code: 'NETWORK_MODE_MISMATCH', name: 'PdfSignatureCoreSafetyError', message: expect.any(String),
    }));
    expect(() => assertPdfSignatureCoreValidationBoundary(
      { inputSha256: digest, validationMode: 'offline', trust: EMPTY_TRUST, ...OBSERVED_CLOCK },
      { inputSha256: digest, onlineValidationAuthorized: true, ...CLOCK_BOUNDS },
    )).toThrowError(expect.objectContaining<PdfSignatureCoreSafetyError>({
      code: 'NETWORK_MODE_MISMATCH', name: 'PdfSignatureCoreSafetyError', message: expect.any(String),
    }));
  });

  it('rejects a report produced under a different trust snapshot', () => {
    const digest = 'a'.repeat(64);
    expect(() => assertPdfSignatureCoreValidationBoundary({
      inputSha256: digest,
      validationMode: 'offline',
      ...OBSERVED_CLOCK,
      trust: { ...EMPTY_TRUST, configurationSha256: 'f'.repeat(64) },
    }, { inputSha256: digest, ...CLOCK_BOUNDS })).toThrowError(expect.objectContaining<PdfSignatureCoreSafetyError>({
      code: 'TRUST_POLICY_MISMATCH', name: 'PdfSignatureCoreSafetyError', message: expect.any(String),
    }));
  });

  it('binds ordinary reports to operation time bounds and fixed reports to the exact reference', () => {
    const digest = 'a'.repeat(64);
    const base = { inputSha256: digest, validationMode: 'offline' as const, trust: EMPTY_TRUST };
    const fixed = {
      ...base,
      validationTime: '2026-08-05T00:00:00Z',
      validationTimeProvenance: 'caller-supplied-fixed-reference' as const,
    };
    expect(assertPdfSignatureCoreValidationBoundary(fixed, {
      inputSha256: digest,
      fixedReferenceValidationTime: fixed.validationTime,
      ...CLOCK_BOUNDS,
    })).toBe(fixed);
    for (const [result, fixedReferenceValidationTime] of [
      [{ ...base, ...OBSERVED_CLOCK, validationTime: '2026-08-04T23:59:58Z' }, undefined],
      [{ ...base, ...OBSERVED_CLOCK, validationTimeProvenance: 'caller-supplied-fixed-reference' as const }, undefined],
      [{ ...fixed, validationTime: '2026-08-04T23:59:59Z' }, '2026-08-05T00:00:00Z'],
      [{ ...fixed, validationTimeProvenance: 'observed-system-utc' as const }, '2026-08-05T00:00:00Z'],
    ] as const) {
      expect(() => assertPdfSignatureCoreValidationBoundary(result, {
        inputSha256: digest,
        ...(fixedReferenceValidationTime === undefined ? {} : { fixedReferenceValidationTime }),
        ...CLOCK_BOUNDS,
      })).toThrowError(expect.objectContaining<PdfSignatureCoreSafetyError>({
        code: 'VALIDATION_CLOCK_MISMATCH',
        name: 'PdfSignatureCoreSafetyError',
        message: expect.any(String),
      }));
    }
  });
});
