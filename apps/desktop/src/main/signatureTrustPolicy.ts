import { createHash } from 'node:crypto';

export const OFFLINE_SIGNATURE_TRUST_POLICY_ID = 'butter-paper-local-explicit-certificates' as const;
export const OFFLINE_SIGNATURE_TRUST_POLICY_VERSION = 1 as const;
export const OFFLINE_SIGNATURE_TRUST_MAX_ANCHORS = 16 as const;
export const OFFLINE_SIGNATURE_TRUST_MAX_CERTIFICATE_DER_BYTES = 32 * 1024;
export const OFFLINE_SIGNATURE_TRUST_MAX_TOTAL_DER_BYTES = 512 * 1024;

const SHA256_PATTERN = /^[a-f0-9]{64}$/;

/**
 * Hashes only the effective exact-certificate decisions. Opaque UI IDs,
 * labels, import order, timestamps, and disabled certificates do not affect
 * the policy snapshot bound to a validation report.
 */
export function offlineSignatureTrustConfigurationSha256(
  enabledExactCertificateFingerprints: readonly string[],
): string {
  if (enabledExactCertificateFingerprints.length > OFFLINE_SIGNATURE_TRUST_MAX_ANCHORS
    || !enabledExactCertificateFingerprints.every((fingerprint) => SHA256_PATTERN.test(fingerprint))) {
    throw new TypeError('Offline signature trust fingerprints are invalid or exceed the protocol limit.');
  }
  const sorted = [...enabledExactCertificateFingerprints].sort();
  if (sorted.some((fingerprint, index) => index > 0 && sorted[index - 1] === fingerprint)) {
    throw new TypeError('Offline signature trust fingerprints must be unique.');
  }
  return createHash('sha256').update(JSON.stringify({
    policyId: OFFLINE_SIGNATURE_TRUST_POLICY_ID,
    policyVersion: OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
    enabledExactCertificateFingerprints: sorted,
  }), 'utf8').digest('hex');
}
