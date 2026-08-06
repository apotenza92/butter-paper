import { describe, expect, it } from 'vitest';
import { resolveApplicationMetadata } from './applicationMetadata';

describe('application metadata', () => {
  it('preserves the reviewed stable and beta product identities', () => {
    expect(resolveApplicationMetadata({
      butterPaperChannel: 'stable',
      productName: 'Butter Paper',
    })).toMatchObject({
      channel: 'stable',
      productName: 'Butter Paper',
      development: false,
      windowTitle: 'Butter Paper',
    });
    expect(resolveApplicationMetadata({
      butterPaperChannel: 'beta',
      productName: 'Butter Paper Beta',
    })).toMatchObject({
      channel: 'beta',
      productName: 'Butter Paper Beta',
      development: false,
      windowTitle: 'Butter Paper Beta',
    });
  });

  it('fails closed to stable branding for incomplete or mismatched metadata', () => {
    for (const metadata of [
      null,
      {},
      { butterPaperChannel: 'beta' },
      { productName: 'Butter Paper Beta' },
      { butterPaperChannel: 'beta', productName: 'Butter Paper' },
      { butterPaperChannel: 'preview', productName: 'Butter Paper Preview' },
    ]) {
      expect(resolveApplicationMetadata(metadata)).toMatchObject({
        channel: 'stable',
        productName: 'Butter Paper',
        development: false,
      });
    }
  });

  it('exposes exact development provenance in the window title and bridge metadata', () => {
    expect(resolveApplicationMetadata({
      butterPaperChannel: 'stable',
      productName: 'Butter Paper',
      version: '0.0.18',
    }, {
      packaged: false,
      version: '0.0.18',
      devProvenance: {
        schemaVersion: 1,
        version: '0.0.18',
        commit: 'a'.repeat(40),
        branch: 'codex/recover-v0.0.18-nonsigning',
        dirty: true,
        checkoutId: 'b'.repeat(64),
        statusFingerprint: 'c'.repeat(64),
      },
    })).toMatchObject({
      development: true,
      version: '0.0.18',
      commit: 'a'.repeat(40),
      branch: 'codex/recover-v0.0.18-nonsigning',
      dirty: true,
      windowTitle: 'Butter Paper Dev · codex/recover-v0.0.18-nonsigning@aaaaaaaa dirty',
    });
  });
});
