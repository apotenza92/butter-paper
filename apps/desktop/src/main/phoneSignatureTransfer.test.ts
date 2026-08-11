import { createHash } from 'node:crypto';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  encryptPhoneSignaturePayloadForTest,
  PhoneSignatureTransferService,
  resolveSignatureRelayOrigin,
} from './phoneSignatureTransfer';

const NOW = Date.UTC(2026, 7, 11, 1, 2, 3);
const SESSION_ID_BYTES = Buffer.alloc(16, 0x11);
const DESKTOP_TOKEN_BYTES = Buffer.alloc(32, 0x22);
const PHONE_TOKEN_BYTES = Buffer.alloc(32, 0x33);
const ENCRYPTION_KEY = Buffer.alloc(32, 0x44);
const SESSION_ID = SESSION_ID_BYTES.toString('base64url');
const DESKTOP_TOKEN = DESKTOP_TOKEN_BYTES.toString('base64url');
const PHONE_TOKEN = PHONE_TOKEN_BYTES.toString('base64url');
const PNG_BYTES = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);
const PNG_DATA_URL = `data:image/png;base64,${PNG_BYTES.toString('base64')}`;

describe('PhoneSignatureTransferService', () => {
  let service: PhoneSignatureTransferService | null = null;

  afterEach(() => {
    service?.dispose();
    service = null;
  });

  it('creates a QR session without exposing its capabilities to the renderer', async () => {
    const request = vi.fn().mockResolvedValueOnce(jsonResponse({ expiresAt: NOW + 300_000 }, 201));
    service = createService(request);

    const session = await service.start(12, 'draw');

    expect(session).toEqual({
      id: SESSION_ID,
      qrDataUrl: expect.stringMatching(/^data:image\/svg\+xml;base64,/),
      expiresAt: NOW + 300_000,
      mode: 'draw',
    });
    const [url, init] = request.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`https://signatures.example.test/api/sessions/${SESSION_ID}`);
    expect(init.method).toBe('POST');
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body).toEqual({
      version: 1,
      desktopTokenHash: tokenHash(DESKTOP_TOKEN),
      phoneTokenHash: tokenHash(PHONE_TOKEN),
    });
    expect(String(init.body)).not.toContain(DESKTOP_TOKEN);
    expect(String(init.body)).not.toContain(PHONE_TOKEN);
    expect(String(init.body)).not.toContain(ENCRYPTION_KEY.toString('base64url'));
  });

  it('polls with the desktop capability and decrypts one authenticated drawing', async () => {
    const encrypted = encryptPhoneSignaturePayloadForTest(
      { id: SESSION_ID, mode: 'draw', expiresAt: NOW + 300_000 },
      ENCRYPTION_KEY,
      { mode: 'draw', mimeType: 'image/png', dataUrl: PNG_DATA_URL },
    );
    const request = vi.fn()
      .mockResolvedValueOnce(jsonResponse({ expiresAt: NOW + 300_000 }, 201))
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
      .mockResolvedValueOnce(new Response(responseBody(encrypted), {
        status: 200,
        headers: { 'content-length': String(encrypted.byteLength) },
      }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    service = createService(request);
    const session = await service.start(21, 'draw');

    await expect(service.poll(session.id, 21)).resolves.toEqual({ status: 'waiting' });
    await expect(service.poll(session.id, 21)).resolves.toEqual({
      status: 'received',
      image: { dataUrl: PNG_DATA_URL, mimeType: 'image/png', mode: 'draw' },
    });
    const [, pollInit] = request.mock.calls[1] as [string, RequestInit];
    expect(new Headers(pollInit.headers).get('authorization')).toBe(`Bearer ${DESKTOP_TOKEN}`);
    const [ackUrl, ackInit] = request.mock.calls[3] as [string, RequestInit];
    expect(ackUrl).toBe(`https://signatures.example.test/api/sessions/${SESSION_ID}/ack`);
    expect(JSON.parse(String(ackInit.body))).toEqual({
      version: 1,
      messageId: Buffer.alloc(16, 6).toString('base64url'),
    });
    await expect(service.poll(session.id, 21)).rejects.toThrow('not available');
  });

  it('retries a lost retrieval without destroying the relay payload', async () => {
    const encrypted = encryptPhoneSignaturePayloadForTest(
      { id: SESSION_ID, mode: 'image', expiresAt: NOW + 300_000 },
      ENCRYPTION_KEY,
      { mode: 'image', mimeType: 'image/png', dataUrl: PNG_DATA_URL },
    );
    const request = vi.fn()
      .mockResolvedValueOnce(jsonResponse({ expiresAt: NOW + 300_000 }, 201))
      .mockRejectedValueOnce(new Error('connection dropped'))
      .mockResolvedValueOnce(new Response(responseBody(encrypted), { status: 200 }))
      .mockRejectedValueOnce(new Error('ack dropped'));
    service = createService(request);
    const session = await service.start(25, 'image');

    await expect(service.poll(session.id, 25)).resolves.toEqual({ status: 'waiting' });
    await expect(service.poll(session.id, 25)).resolves.toEqual({
      status: 'received',
      image: { dataUrl: PNG_DATA_URL, mimeType: 'image/png', mode: 'image' },
    });
  });

  it('retains the ACK capability and retries until the relay confirms deletion', async () => {
    vi.useFakeTimers();
    try {
      const encrypted = encryptPhoneSignaturePayloadForTest(
        { id: SESSION_ID, mode: 'draw', expiresAt: NOW + 300_000 },
        ENCRYPTION_KEY,
        { mode: 'draw', mimeType: 'image/png', dataUrl: PNG_DATA_URL },
      );
      const request = vi.fn()
        .mockResolvedValueOnce(jsonResponse({ expiresAt: NOW + 300_000 }, 201))
        .mockResolvedValueOnce(new Response(responseBody(encrypted), { status: 200 }))
        .mockResolvedValueOnce(new Response(null, { status: 503 }))
        .mockResolvedValueOnce(new Response(null, { status: 204 }));
      service = createService(request);
      const session = await service.start(26, 'draw');

      await expect(service.poll(session.id, 26)).resolves.toMatchObject({ status: 'received' });
      await expect(service.poll(session.id, 26)).rejects.toThrow('not available');
      await vi.advanceTimersByTimeAsync(2_000);
      expect(request.mock.calls.filter(([url]) => String(url).endsWith('/ack'))).toHaveLength(2);
      await expect(service.poll(session.id, 26)).rejects.toThrow('not available');
    } finally {
      vi.useRealTimers();
    }
  });

  it('rejects the wrong owner, a mode mismatch, and modified ciphertext', async () => {
    const modeMismatch = encryptPhoneSignaturePayloadForTest(
      { id: SESSION_ID, mode: 'image', expiresAt: NOW + 300_000 },
      ENCRYPTION_KEY,
      { mode: 'image', mimeType: 'image/png', dataUrl: PNG_DATA_URL },
    );
    const request = vi.fn()
      .mockResolvedValueOnce(jsonResponse({ expiresAt: NOW + 300_000 }, 201))
      .mockResolvedValueOnce(new Response(responseBody(modeMismatch), { status: 200 }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    service = createService(request);
    const session = await service.start(31, 'draw');
    await expect(service.poll(session.id, 99)).rejects.toThrow('not available');
    await expect(service.poll(session.id, 31)).rejects.toThrow('could not be authenticated');

    const tampered = encryptPhoneSignaturePayloadForTest(
      { id: SESSION_ID, mode: 'image', expiresAt: NOW + 300_000 },
      ENCRYPTION_KEY,
      { mode: 'image', mimeType: 'image/png', dataUrl: PNG_DATA_URL },
    );
    tampered[tampered.length - 1] ^= 1;
    request.mockResolvedValueOnce(jsonResponse({ expiresAt: NOW + 300_000 }, 201));
    request.mockResolvedValueOnce(new Response(responseBody(tampered), { status: 200 }));
    request.mockResolvedValueOnce(new Response(null, { status: 204 }));
    const second = await service.start(32, 'image');
    await expect(service.poll(second.id, 32)).rejects.toThrow('could not be authenticated');
  });

  it('expires remote sessions and cancels locally even when relay deletion fails', async () => {
    const request = vi.fn()
      .mockResolvedValueOnce(jsonResponse({ expiresAt: NOW + 300_000 }, 201))
      .mockResolvedValueOnce(new Response(null, { status: 410 }));
    service = createService(request);
    const expired = await service.start(41, 'image');
    await expect(service.poll(expired.id, 41)).resolves.toEqual({ status: 'expired' });

    request.mockResolvedValueOnce(jsonResponse({ expiresAt: NOW + 300_000 }, 201));
    request.mockRejectedValueOnce(new Error('offline'));
    const cancelled = await service.start(42, 'image');
    await expect(service.stop(cancelled.id, 42)).resolves.toBeUndefined();
    await expect(service.poll(cancelled.id, 42)).rejects.toThrow('not available');
  });

  it('rejects invalid relay expiry values', async () => {
    const request = vi.fn().mockResolvedValueOnce(jsonResponse({ expiresAt: NOW + 600_000 }, 201));
    service = createService(request);
    await expect(service.start(51, 'image')).rejects.toThrow('invalid expiry time');
  });

  it('matches the phone Web Crypto implementation with a fixed BPS1 vector', () => {
    const envelope = encryptPhoneSignaturePayloadForTest(
      {
        id: 'AAECAwQFBgcICQoLDA0ODw',
        mode: 'draw',
        expiresAt: 1_786_435_200_000,
      },
      Buffer.from(Uint8Array.from({ length: 32 }, (_, index) => index)),
      {
        mode: 'draw',
        mimeType: 'image/png',
        dataUrl: `data:image/png;base64,${Buffer.from([
          0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
        ]).toString('base64')}`,
      },
      Buffer.from(Uint8Array.from({ length: 16 }, (_, index) => 0x10 + index)),
      Buffer.from(Uint8Array.from({ length: 12 }, (_, index) => 0x20 + index)),
    );

    expect(envelope.toString('hex')).toBe(
      '42505331101112131415161718191a1b1c1d1e1f202122232425262728292a2bd33aa67064114a405d7148d4cb30dd22af9c14131adc13efbb6074ca6f',
    );
  });
});

describe('resolveSignatureRelayOrigin', () => {
  it('requires an HTTPS origin that can be pinned into the signed production bundle', () => {
    expect(() => resolveSignatureRelayOrigin(undefined)).toThrow('not configured in this build');
    expect(() => resolveSignatureRelayOrigin('http://signature.example')).toThrow('must use HTTPS');
    expect(resolveSignatureRelayOrigin('https://signature.example/')).toBe('https://signature.example');
  });

  it('allows explicit HTTPS or loopback origins only in relay test mode', () => {
    expect(() => resolveSignatureRelayOrigin(undefined, true)).toThrow('not configured');
    expect(() => resolveSignatureRelayOrigin('https://user@example.test', true)).toThrow('only an origin');
    expect(resolveSignatureRelayOrigin('https://signatures.example.test/', true)).toBe('https://signatures.example.test');
    expect(resolveSignatureRelayOrigin('http://127.0.0.1:8787', true)).toBe('http://127.0.0.1:8787');
    expect(() => resolveSignatureRelayOrigin('http://192.168.1.5:8787', true)).toThrow('must use HTTPS');
  });
});

function createService(request: ReturnType<typeof vi.fn>): PhoneSignatureTransferService {
  const values = [SESSION_ID_BYTES, DESKTOP_TOKEN_BYTES, PHONE_TOKEN_BYTES, ENCRYPTION_KEY];
  let randomIndex = 0;
  return new PhoneSignatureTransferService({
    relayOrigin: 'https://signatures.example.test',
    allowInsecureLoopback: true,
    fetch: request as typeof fetch,
    now: () => NOW,
    sanitizeImage: async (image) => image,
    randomBytes: (size) => {
      const value = values[randomIndex % values.length];
      randomIndex += 1;
      if (!value || value.byteLength !== size) throw new Error(`Unexpected random byte request: ${size}`);
      return Buffer.from(value);
    },
  });
}

function jsonResponse(value: unknown, status: number): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function tokenHash(token: string): string {
  return createHash('sha256').update(Buffer.from(token, 'base64url')).digest('base64url');
}

function responseBody(value: Buffer): ArrayBuffer {
  return Uint8Array.from(value).buffer;
}
