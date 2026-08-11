import { describe, expect, it } from 'vitest';
import {
  createSignatureAppearanceAsset,
  SIGNATURE_APPEARANCE_NOTICE,
} from './signatureAppearance.js';

describe('signature appearances', () => {
  it('creates a bounded visual signature asset', () => {
    expect(createSignatureAppearanceAsset({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: 320.4,
      height: 119.6,
      source: 'drawn',
    })).toEqual({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: 320,
      height: 120,
      source: 'drawn',
    });
    expect(SIGNATURE_APPEARANCE_NOTICE).toContain('does not verify identity');
  });

  it('rejects mismatched image data and invalid dimensions', () => {
    expect(() => createSignatureAppearanceAsset({
      dataUrl: 'data:image/jpeg;base64,AAAA',
      mimeType: 'image/png',
      width: 320,
      height: 120,
      source: 'image',
    })).toThrow(/matching PNG or JPEG data URL/);
    expect(() => createSignatureAppearanceAsset({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: 0,
      height: 120,
      source: 'typed',
    })).toThrow(/positive finite/);
  });

  it('keeps positive sub-pixel dimensions valid after normalization', () => {
    expect(createSignatureAppearanceAsset({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: 0.4,
      height: 0.4,
      source: 'typed',
    })).toEqual(expect.objectContaining({ width: 1, height: 1 }));
  });

  it('rejects empty base64 data and mismatched image magic bytes', () => {
    expect(() => createSignatureAppearanceAsset({
      dataUrl: 'data:image/png;base64,',
      mimeType: 'image/png',
      width: 320,
      height: 120,
      source: 'image',
    })).toThrow(/bounded base64 image data/);
    expect(() => createSignatureAppearanceAsset({
      dataUrl: 'data:image/png;base64,QUFBQQ==',
      mimeType: 'image/png',
      width: 320,
      height: 120,
      source: 'image',
    })).toThrow(/does not match its image type/);
  });

  it('rejects an unsupported MIME type at runtime', () => {
    expect(() => createSignatureAppearanceAsset({
      dataUrl: 'data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=',
      mimeType: 'image/svg+xml',
      width: 320,
      height: 120,
      source: 'image',
    } as unknown as Parameters<typeof createSignatureAppearanceAsset>[0])).toThrow(/PNG or JPEG/);
  });
});
