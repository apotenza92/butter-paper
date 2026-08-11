import { describe, expect, it } from 'vitest';
import { signatureAppearanceToPendingImageAsset } from './signaturePlacement';

describe('signature placement', () => {
  it('maps a visual signature to the existing image-placement contract', () => {
    expect(signatureAppearanceToPendingImageAsset({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: 640,
      height: 240,
      source: 'drawn',
    })).toEqual({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: 640,
      height: 240,
      fileName: 'signature.png',
      aspectRatioLocked: true,
      selectAfterPlacement: true,
    });
  });

  it('uses a filename that matches an imported JPEG', () => {
    expect(signatureAppearanceToPendingImageAsset({
      dataUrl: 'data:image/jpeg;base64,/9j/4AAQSkZJRg==',
      mimeType: 'image/jpeg',
      width: 480,
      height: 160,
      source: 'image',
    }).fileName).toBe('signature.jpg');
  });
});
