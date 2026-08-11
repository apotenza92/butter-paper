import type { SignatureAppearanceAsset } from '@butter-paper/core';
import type { PendingImageAsset } from '../state/viewerStore';

export function signatureAppearanceToPendingImageAsset(
  asset: SignatureAppearanceAsset,
): PendingImageAsset {
  return {
    dataUrl: asset.dataUrl,
    mimeType: asset.mimeType,
    width: asset.width,
    height: asset.height,
    fileName: asset.mimeType === 'image/jpeg' ? 'signature.jpg' : 'signature.png',
    aspectRatioLocked: true,
    selectAfterPlacement: true,
  };
}
