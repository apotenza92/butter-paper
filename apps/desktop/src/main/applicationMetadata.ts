import type { ApplicationMetadata } from '../shared/protocol';

const STABLE_METADATA: ApplicationMetadata = {
  channel: 'stable',
  productName: 'Butter Paper',
};

const BETA_METADATA: ApplicationMetadata = {
  channel: 'beta',
  productName: 'Butter Paper Beta',
};

export function resolveApplicationMetadata(value: unknown): ApplicationMetadata {
  if (value == null || typeof value !== 'object') {
    return STABLE_METADATA;
  }

  const metadata = value as Record<string, unknown>;
  if (metadata.butterPaperChannel === 'beta' && metadata.productName === BETA_METADATA.productName) {
    return BETA_METADATA;
  }
  if (metadata.butterPaperChannel === 'stable' && metadata.productName === STABLE_METADATA.productName) {
    return STABLE_METADATA;
  }
  return STABLE_METADATA;
}
