export type SignatureAppearanceSource = 'drawn' | 'typed' | 'image';

export interface SignatureAppearanceAsset {
  readonly dataUrl: string;
  readonly mimeType: 'image/png' | 'image/jpeg';
  readonly width: number;
  readonly height: number;
  readonly source: SignatureAppearanceSource;
}

export const SIGNATURE_APPEARANCE_NOTICE = 'Adds a visual signature to the PDF. It does not verify identity or detect later changes.';

const MAX_SIGNATURE_DATA_URL_LENGTH = 16 * 1024 * 1024;

export function createSignatureAppearanceAsset(
  asset: SignatureAppearanceAsset,
): SignatureAppearanceAsset {
  if (asset.mimeType !== 'image/png' && asset.mimeType !== 'image/jpeg') {
    throw new TypeError('Signature appearance must use PNG or JPEG image data.');
  }
  const prefix = `data:${asset.mimeType};base64,`;
  if (!asset.dataUrl.startsWith(prefix)) {
    throw new TypeError('Signature appearance must use a matching PNG or JPEG data URL.');
  }
  const payload = asset.dataUrl.slice(prefix.length);
  if (!payload || payload.length > MAX_SIGNATURE_DATA_URL_LENGTH
    || payload.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(payload)) {
    throw new TypeError('Signature appearance must contain bounded base64 image data.');
  }
  if ((asset.mimeType === 'image/png' && !payload.startsWith('iVBORw0KGgo'))
    || (asset.mimeType === 'image/jpeg' && !payload.startsWith('/9j/'))) {
    throw new TypeError('Signature appearance data does not match its image type.');
  }
  if (!Number.isFinite(asset.width) || asset.width <= 0
    || !Number.isFinite(asset.height) || asset.height <= 0) {
    throw new TypeError('Signature appearance dimensions must be positive finite numbers.');
  }
  if (!['drawn', 'typed', 'image'].includes(asset.source)) {
    throw new TypeError('Signature appearance source is invalid.');
  }

  return {
    ...asset,
    width: Math.max(1, Math.round(asset.width)),
    height: Math.max(1, Math.round(asset.height)),
  };
}
