import { describe, expect, it } from 'vitest';
import {
  OFFLINE_SIGNATURE_TRUST_MAX_ANCHORS,
  offlineSignatureTrustConfigurationSha256,
} from './signatureTrustPolicy';

describe('offline exact-certificate trust policy', () => {
  it('binds the empty policy to a stable reviewed digest', () => {
    expect(offlineSignatureTrustConfigurationSha256([])).toBe(
      '65621a8373d3e6869d50a8572da7d20ae5c4d7c91a915eeda34493187f071f0e',
    );
  });

  it('sorts fingerprints canonically and rejects duplicates, malformed digests, and overflow', () => {
    const a = 'a'.repeat(64);
    const b = 'b'.repeat(64);
    expect(offlineSignatureTrustConfigurationSha256([b, a])).toBe(
      offlineSignatureTrustConfigurationSha256([a, b]),
    );
    expect(() => offlineSignatureTrustConfigurationSha256([a, a])).toThrow(/unique/i);
    expect(() => offlineSignatureTrustConfigurationSha256(['not-a-digest'])).toThrow(/invalid/i);
    expect(() => offlineSignatureTrustConfigurationSha256(
      Array.from({ length: OFFLINE_SIGNATURE_TRUST_MAX_ANCHORS + 1 }, (_, index) => index.toString(16).padStart(64, '0')),
    )).toThrow(/limit/i);
  });
});
