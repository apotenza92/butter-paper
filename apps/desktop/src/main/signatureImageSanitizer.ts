import electron from 'electron';
import { randomUUID } from 'node:crypto';
import {
  createSignatureAppearanceAsset,
  processSignaturePixels,
  type SignatureAppearanceAsset,
} from '@butter-paper/core';
import type { PhoneSignatureImage } from '../shared/protocol';

const { BrowserWindow, session } = electron;
const MAX_SANITIZED_SIGNATURE_BYTES = 10 * 1024 * 1024;
const MAX_SANITIZED_SIGNATURE_DIMENSION = 2048;
const ISOLATED_PAGE = `data:text/html;charset=utf-8,${encodeURIComponent(`<!doctype html>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:">
<title>Signature image sanitizer</title>`)}`;

export async function sanitizePhoneSignatureImage(image: PhoneSignatureImage): Promise<PhoneSignatureImage> {
  const result = await sanitizeSignatureImage(image);
  return {
    dataUrl: result.dataUrl,
    mimeType: 'image/png',
    mode: image.mode,
  };
}

export async function sanitizeSignatureAppearanceAsset(
  asset: SignatureAppearanceAsset,
): Promise<SignatureAppearanceAsset> {
  const result = await sanitizeSignatureImage(asset);
  return createSignatureAppearanceAsset({
    dataUrl: result.dataUrl,
    mimeType: 'image/png',
    width: result.width,
    height: result.height,
    source: asset.source,
  });
}

async function sanitizeSignatureImage(
  image: Pick<SignatureAppearanceAsset, 'dataUrl' | 'mimeType'>,
): Promise<{ dataUrl: string; width: number; height: number }> {
  const partition = `phone-signature-sanitizer-${randomUUID()}`;
  const isolatedSession = session.fromPartition(partition, { cache: false });
  isolatedSession.setPermissionCheckHandler(() => false);
  isolatedSession.setPermissionRequestHandler((_webContents, _permission, callback) => callback(false));
  isolatedSession.webRequest.onBeforeRequest(
    { urls: ['http://*/*', 'https://*/*', 'file://*/*', 'ws://*/*', 'wss://*/*'] },
    (_details, callback) => callback({ cancel: true }),
  );

  const window = new BrowserWindow({
    show: false,
    width: 1,
    height: 1,
    webPreferences: {
      partition,
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      webSecurity: true,
      allowRunningInsecureContent: false,
      offscreen: true,
    },
  });
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));

  try {
    await window.loadURL(ISOLATED_PAGE);
    window.webContents.on('will-navigate', (event) => event.preventDefault());
    const processorSource = processSignaturePixels.toString();
    const result = await window.webContents.executeJavaScript(`(async () => {
      const input = ${JSON.stringify(image)};
      const processSignaturePixels = ${processorSource};
      if (input.mimeType !== 'image/png' && input.mimeType !== 'image/jpeg') {
        throw new Error('The phone signature media type is invalid.');
      }
      const expectedPrefix = 'data:' + input.mimeType + ';base64,';
      if (typeof input.dataUrl !== 'string' || !input.dataUrl.startsWith(expectedPrefix)) {
        throw new Error('The phone signature image encoding is invalid.');
      }
      const decoded = new Image();
      await new Promise((resolve, reject) => {
        decoded.addEventListener('load', resolve, { once: true });
        decoded.addEventListener('error', () => reject(new Error('Unable to decode the phone signature.')), { once: true });
        decoded.src = input.dataUrl;
      });
      const width = decoded.naturalWidth;
      const height = decoded.naturalHeight;
      const aspectRatio = Math.max(width / height, height / width);
      if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0
        || width > 4096 || height > 4096 || width * height > 16 * 1024 * 1024 || aspectRatio > 25) {
        throw new Error('The phone signature image dimensions are unsafe.');
      }
      const scale = Math.min(1, ${MAX_SANITIZED_SIGNATURE_DIMENSION} / Math.max(width, height));
      const canvas = document.createElement('canvas');
      canvas.width = Math.max(1, Math.round(width * scale));
      canvas.height = Math.max(1, Math.round(height * scale));
      const context = canvas.getContext('2d');
      if (!context) throw new Error('Unable to sanitize the phone signature.');
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.imageSmoothingEnabled = true;
      context.imageSmoothingQuality = 'high';
      context.drawImage(decoded, 0, 0, canvas.width, canvas.height);
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
      const processed = processSignaturePixels({ data: pixels.data, width: canvas.width, height: canvas.height });
      const outputCanvas = document.createElement('canvas');
      outputCanvas.width = processed.width;
      outputCanvas.height = processed.height;
      const outputContext = outputCanvas.getContext('2d');
      if (!outputContext) throw new Error('Unable to sanitize the phone signature.');
      const outputPixels = outputContext.createImageData(processed.width, processed.height);
      outputPixels.data.set(processed.data);
      outputContext.putImageData(outputPixels, 0, 0);
      return { dataUrl: outputCanvas.toDataURL('image/png'), width: processed.width, height: processed.height };
    })()`, true) as { dataUrl?: unknown; width?: unknown; height?: unknown };
    if (typeof result.dataUrl !== 'string' || !result.dataUrl.startsWith('data:image/png;base64,')
      || !Number.isInteger(result.width) || !Number.isInteger(result.height)) {
      throw new Error('The isolated phone signature sanitizer returned an invalid result.');
    }
    const encodedImage = result.dataUrl.slice('data:image/png;base64,'.length);
    if (Buffer.byteLength(encodedImage, 'base64') > MAX_SANITIZED_SIGNATURE_BYTES) {
      throw new Error('The sanitized signature image is larger than 10 MB.');
    }
    return { dataUrl: result.dataUrl, width: result.width, height: result.height } as {
      dataUrl: string;
      width: number;
      height: number;
    };
  } finally {
    if (!window.isDestroyed()) window.destroy();
    await isolatedSession.clearStorageData().catch(() => undefined);
  }
}
