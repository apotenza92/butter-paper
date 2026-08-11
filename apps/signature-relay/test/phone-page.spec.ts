import { describe, expect, it, vi } from 'vitest';
// @ts-expect-error The static browser module is imported directly for a cross-runtime vector.
import { createEncryptedEnvelope } from '../public/phone-protocol.js';
// @ts-expect-error The static browser module supports dependency injection for deterministic tests.
import { planSanitizedDimensions, processSignaturePixels, readImageHeaderDimensions, sanitizeImageFile } from '../public/phone-image.js';
import appScript from '../public/app.js?raw';
import headers from '../public/_headers?raw';
import page from '../public/index.html?raw';
import protocolScript from '../public/phone-protocol.js?raw';
import imageScript from '../public/phone-image.js?raw';
import workerScript from '../src/index.ts?raw';
import wranglerConfiguration from '../wrangler.jsonc?raw';

function hex(bytes: Uint8Array): string {
  return [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

describe('phone page static contract', () => {
  it('uses separate same-origin modules with no inline executable content', () => {
    expect(page).toContain('<script type="module" src="/app.js"></script>');
    expect(page).toContain('<link rel="stylesheet" href="/styles.css" />');
    expect(page).not.toMatch(/<script(?![^>]*\bsrc=)/u);
    expect(page).not.toContain('<style');
    expect(appScript).toContain("from './phone-protocol.js'");
    expect(page).toContain('trusted signature service sends encrypted data');
  });

  it('parses all secrets and expiry only from the fragment, then removes the fragment', () => {
    expect(appScript).toContain('window.location.hash');
    expect(appScript).toContain("window.history.replaceState(null, '', window.location.pathname)");
    expect(appScript).toContain("const expectedNames = ['e', 'k', 'm', 't', 'v']");
    expect(appScript).not.toMatch(/searchParams\.get\(['"](?:e|k|t)['"]\)/u);
  });

  it('matches the desktop BPS1 AES-256-GCM contract with a fixed cross-runtime vector', async () => {
    const result = await createEncryptedEnvelope({
      rawBytes: new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      mediaType: 'image/png',
      sessionId: 'AAECAwQFBgcICQoLDA0ODw',
      expiresAt: 1_786_435_200_000,
      mode: 'draw',
      keyBytes: Uint8Array.from({ length: 32 }, (_, index) => index),
      messageId: Uint8Array.from({ length: 16 }, (_, index) => 0x10 + index),
      iv: Uint8Array.from({ length: 12 }, (_, index) => 0x20 + index),
    });

    expect(hex(result.envelope)).toBe(
      '42505331101112131415161718191a1b1c1d1e1f202122232425262728292a2bd33aa67064114a405d7148d4cb30dd22af9c14131adc13efbb6074ca6f',
    );
    expect(protocolScript).toContain("new TextEncoder().encode('ButterPaper.PhoneSignature.phone-to-desktop\\0')");
    expect(protocolScript).toContain('setBigUint64(0, BigInt(value), false)');
    expect(protocolScript).toContain("mediaType === 'image/png' ? 1");
    expect(protocolScript).toContain("mode === 'draw' ? 1");
    expect(protocolScript).toContain('writeUint32(plaintext.byteLength)');
  });

  it('supports pointer drawing, image capture, one MiB input, and retry of one envelope', () => {
    expect(page).toContain('capture="environment"');
    expect(page).toContain('accept="image/png,image/jpeg"');
    expect(appScript).toContain("canvas.addEventListener('pointerdown'");
    expect(appScript).toContain('if (!pendingUpload)');
    expect(appScript).toContain('body: pendingUpload.envelope');
    expect(protocolScript).toContain('export const MAX_IMAGE_BYTES = 1024 * 1024');
  });

  it('sanitizes and downscales a large camera image before encryption', async () => {
    const draws: Array<{ width: number; height: number; mediaType: string }> = [];
    const encodings: string[] = [];
    let closed = false;
    const source = new Blob([new Uint8Array(2 * 1024 * 1024)], { type: 'image/jpeg' });
    const output = await sanitizeImageFile(source, {
      async decode() {
        return { width: 4000, height: 3000, close: () => (closed = true) };
      },
      async inspect() {
        return { width: 4000, height: 3000 };
      },
      draw(_image: unknown, width: number, height: number, mediaType: string) {
        draws.push({ width, height, mediaType });
        return {};
      },
      async encode(_canvas: unknown, mediaType: string) {
        encodings.push(mediaType);
        return new Blob([new Uint8Array(900 * 1024)], { type: mediaType });
      },
    });

    expect(output.size).toBe(900 * 1024);
    expect(draws[0]).toEqual({ width: 2048, height: 1536, mediaType: 'image/jpeg' });
    expect(encodings).toEqual(['image/png']);
    expect(output.type).toBe('image/png');
    expect(closed).toBe(true);
    expect(imageScript).toContain("createImageBitmap(file, { imageOrientation: 'from-image' })");
    expect(imageScript).toContain('processSignaturePixels');
    expect(imageScript).toContain("adapter.encode(canvas, 'image/png')");
  });

  it('turns photographed ink into a cropped transparent signature', () => {
    const data = new Uint8ClampedArray(30 * 20 * 4);
    for (let offset = 0; offset < data.length; offset += 4) data.set([255, 255, 255, 255], offset);
    for (let y = 8; y < 12; y += 1) {
      for (let x = 11; x < 19; x += 1) data.set([0, 0, 0, 255], (y * 30 + x) * 4);
    }

    const processed = processSignaturePixels({ data, width: 30, height: 20 });

    expect(processed.width).toBeLessThan(30);
    expect([...processed.data].some((value, index) => index % 4 === 3 && value === 0)).toBe(true);
    expect([...processed.data].some((value, index) => index % 4 === 3 && value === 255)).toBe(true);
  });

  it('fails closed when sanitized output remains over one MiB', async () => {
    const source = new Blob([new Uint8Array(64)], { type: 'image/png' });
    await expect(
      sanitizeImageFile(source, {
        async decode() {
          return { width: 4096, height: 2048, close() {} };
        },
        async inspect() {
          return { width: 4096, height: 2048 };
        },
        draw() {
          return {};
        },
        async encode() {
          return new Blob([new Uint8Array(1024 * 1024 + 1)], { type: 'image/png' });
        },
      }),
    ).rejects.toThrow('The processed image is still larger than 1 MiB');
  });

  it('rejects unsafe header dimensions before browser decoding', async () => {
    expect(() => planSanitizedDimensions(4096, 4097)).toThrow('dimensions are too large');
    const png = new Uint8Array(24);
    png.set([0x89, 0x50, 0x4e, 0x47], 0);
    png.set([0x49, 0x48, 0x44, 0x52], 12);
    new DataView(png.buffer).setUint32(16, 4096, false);
    new DataView(png.buffer).setUint32(20, 4096, false);
    expect(readImageHeaderDimensions(png, 'image/png')).toEqual({ width: 4096, height: 4096 });
    const decode = vi.fn();
    await expect(sanitizeImageFile(new Blob([png], { type: 'image/png' }), {
      async inspect() { return { width: 4096, height: 4097 }; },
      decode,
    })).rejects.toThrow('dimensions are too large');
    expect(decode).not.toHaveBeenCalled();
    expect(imageScript.indexOf('adapter.inspect(file)')).toBeLessThan(imageScript.indexOf('adapter.decode(file)'));
  });

  it('preserves drawing and pointer alignment across viewport resize', () => {
    expect(appScript).toContain("window.addEventListener('resize', preserveDrawingOnResize)");
    expect(appScript).toContain("snapshot = document.createElement('canvas')");
    expect(appScript).toContain("snapshot.getContext('2d').drawImage(canvas, 0, 0)");
    expect(appScript).toContain('configureCanvas({ preserve: true })');
    expect(appScript).toContain('context.setTransform(scale, 0, 0, scale, 0, 0)');
    expect(appScript).toContain('event.clientX - bounds.left');
    expect(appScript).toContain('event.clientY - bounds.top');
  });

  it('declares restrictive headers, no CORS, edge limits, SQLite exports, and a disabled production switch', () => {
    expect(headers).toContain('Cache-Control: no-store');
    expect(headers).toContain("default-src 'none'");
    expect(headers).toContain("script-src 'self'");
    expect(headers).toContain("connect-src 'self'");
    expect(headers).toContain("frame-ancestors 'none'");
    expect(headers).not.toContain('Access-Control-Allow-Origin');
    expect(wranglerConfiguration).toContain('"storage": "sqlite"');
    expect(wranglerConfiguration).toContain('"RELAY_ENABLED": "false"');
    expect(wranglerConfiguration).toContain('"CREATE_RATE_LIMITER"');
    expect(wranglerConfiguration).toContain('"UPLOAD_RATE_LIMITER"');
    expect(wranglerConfiguration).toContain('"RETRIEVAL_RATE_LIMITER"');
    expect(wranglerConfiguration).not.toContain('RATE_LIMIT_TEST_BYPASS');
    expect(workerScript).toMatch(
      /subroute === '' && request\.method === 'DELETE'[\s\S]*env\.RETRIEVAL_RATE_LIMITER[\s\S]*sourceRateLimitKey\(request\)[\s\S]*if \(edgeLimit\) return edgeLimit/,
    );
  });

  it('provides accessible status and controls for both modes', () => {
    expect(page).toContain('role="status" aria-live="polite"');
    expect(page).toContain('aria-label="Signature drawing area"');
    expect(page).toContain('id="clear-button"');
    expect(page).toContain('id="send-drawing-button"');
    expect(page).toContain('id="send-image-button"');
  });
});
