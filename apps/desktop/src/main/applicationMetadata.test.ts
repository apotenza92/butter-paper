import { describe, expect, it } from 'vitest';
import { resolveApplicationMetadata } from './applicationMetadata';

describe('application metadata', () => {
  it('preserves the reviewed stable and beta product identities', () => {
    expect(resolveApplicationMetadata({
      butterPaperChannel: 'stable',
      productName: 'Butter Paper',
    })).toEqual({
      channel: 'stable',
      productName: 'Butter Paper',
    });
    expect(resolveApplicationMetadata({
      butterPaperChannel: 'beta',
      productName: 'Butter Paper Beta',
    })).toEqual({
      channel: 'beta',
      productName: 'Butter Paper Beta',
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
      expect(resolveApplicationMetadata(metadata)).toEqual({
        channel: 'stable',
        productName: 'Butter Paper',
      });
    }
  });
});
