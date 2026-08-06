import type { ApplicationMetadata } from '../shared/protocol';

interface ApplicationMetadataOptions {
  readonly packaged: boolean;
  readonly version: string;
  readonly devProvenance?: unknown;
}

const STABLE_PRODUCT = {
  channel: 'stable',
  productName: 'Butter Paper',
} as const;

const BETA_PRODUCT = {
  channel: 'beta',
  productName: 'Butter Paper Beta',
} as const;

export function resolveApplicationMetadata(
  value: unknown,
  options: ApplicationMetadataOptions = { packaged: true, version: '0.0.0' },
): ApplicationMetadata {
  const metadata = value != null && typeof value === 'object' ? value as Record<string, unknown> : {};
  const product = metadata.butterPaperChannel === 'beta' && metadata.productName === BETA_PRODUCT.productName
    ? BETA_PRODUCT
    : STABLE_PRODUCT;
  const packageVersion = typeof metadata.version === 'string' && metadata.version.length > 0
    ? metadata.version
    : options.version;

  if (!options.packaged) {
    const provenance = parseDevProvenance(options.devProvenance, packageVersion);
    const shortCommit = provenance.commit.slice(0, 8);
    return {
      ...product,
      version: provenance.version,
      commit: provenance.commit,
      branch: provenance.branch,
      dirty: provenance.dirty,
      development: true,
      checkoutId: provenance.checkoutId,
      statusFingerprint: provenance.statusFingerprint,
      windowTitle: `Butter Paper Dev · ${provenance.branch}@${shortCommit}${provenance.dirty ? ' dirty' : ''}`,
    };
  }

  return {
    ...product,
    version: packageVersion,
    commit: null,
    branch: null,
    dirty: false,
    development: false,
    checkoutId: null,
    statusFingerprint: null,
    windowTitle: product.productName,
  };
}

function parseDevProvenance(value: unknown, expectedVersion: string) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Development build provenance is missing. Run the repository dev preflight before launching.');
  }
  const provenance = value as Record<string, unknown>;
  if (provenance.schemaVersion !== 1
    || provenance.version !== expectedVersion
    || typeof provenance.commit !== 'string'
    || !/^[a-f0-9]{40}$/i.test(provenance.commit)
    || typeof provenance.branch !== 'string'
    || provenance.branch.length === 0
    || typeof provenance.dirty !== 'boolean'
    || typeof provenance.checkoutId !== 'string'
    || !/^[a-f0-9]{64}$/i.test(provenance.checkoutId)
    || typeof provenance.statusFingerprint !== 'string'
    || !/^[a-f0-9]{64}$/i.test(provenance.statusFingerprint)) {
    throw new Error('Development build provenance is invalid or does not match the desktop package version.');
  }
  return {
    version: provenance.version,
    commit: provenance.commit,
    branch: provenance.branch,
    dirty: provenance.dirty,
    checkoutId: provenance.checkoutId,
    statusFingerprint: provenance.statusFingerprint,
  } as const;
}
