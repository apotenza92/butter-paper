import { env, exports as workerExports } from 'cloudflare:workers';
import { reset, runDurableObjectAlarm, runInDurableObject } from 'cloudflare:test';
import { afterEach, describe, expect, it } from 'vitest';
import {
  SignatureSession,
} from '../src/index';
// @ts-expect-error The static browser module is imported directly for an end-to-end transfer test.
import { createEncryptedEnvelope } from '../public/phone-protocol.js';
import {
  ENVELOPE_FIXED_OVERHEAD_BYTES,
  MAX_ACK_ATTEMPTS,
  MAX_ENVELOPE_BYTES,
  MAX_RETRIEVAL_POLLS,
  MAX_UPLOAD_ATTEMPTS,
  isRelayEnabled,
} from '../src/protocol';

const API_ORIGIN = 'https://signature-relay.test';
let sessionSequence = 0;

interface StoredSession {
  version: 1;
  desktopTokenHash: string;
  phoneTokenHash: string;
  expiresAt: number;
  uploadAttempts: number;
  retrievalPolls: number;
  ackAttempts: number;
}

function bytesToArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const buffer = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(buffer).set(bytes);
  return buffer;
}

function bytesToBase64Url(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/u, '');
}

function fixedBytes(length: number, value: number): Uint8Array {
  return new Uint8Array(length).fill(value);
}

function nextSessionId(): string {
  sessionSequence += 1;
  const bytes = fixedBytes(16, 0);
  new DataView(bytes.buffer).setUint32(12, sessionSequence);
  return bytesToBase64Url(bytes);
}

function token(value: number): string {
  return bytesToBase64Url(fixedBytes(32, value));
}

function messageId(value: number): string {
  return bytesToBase64Url(fixedBytes(16, value));
}

function envelope(messageValue: number, bodyValue = 0x5a, length = ENVELOPE_FIXED_OVERHEAD_BYTES): Uint8Array {
  const bytes = fixedBytes(length, bodyValue);
  bytes.set([0x42, 0x50, 0x53, 0x31], 0);
  bytes.set(fixedBytes(16, messageValue), 4);
  bytes.set(fixedBytes(12, 0x33), 20);
  return bytes;
}

async function tokenHash(rawToken: string): Promise<string> {
  const base64 = rawToken.replace(/-/g, '+').replace(/_/g, '/').padEnd(44, '=');
  const tokenBytes = Uint8Array.from(atob(base64), (character) => character.charCodeAt(0));
  return bytesToBase64Url(new Uint8Array(await crypto.subtle.digest('SHA-256', bytesToArrayBuffer(tokenBytes))));
}

async function call(path: string, init?: RequestInit): Promise<Response> {
  return workerExports.default.fetch(new Request(`${API_ORIGIN}${path}`, init));
}

async function createSession(sessionId: string, desktopToken: string, phoneToken: string): Promise<Response> {
  return call(`/api/sessions/${sessionId}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      version: 1,
      desktopTokenHash: await tokenHash(desktopToken),
      phoneTokenHash: await tokenHash(phoneToken),
    }),
  });
}

function upload(sessionId: string, phoneToken: string, body: BodyInit): Promise<Response> {
  return call(`/api/sessions/${sessionId}/payload`, {
    method: 'PUT',
    headers: { Authorization: `Bearer ${phoneToken}`, 'Content-Type': 'application/octet-stream' },
    body,
  });
}

function retrieve(sessionId: string, desktopToken: string): Promise<Response> {
  return call(`/api/sessions/${sessionId}/payload`, {
    method: 'GET',
    headers: { Authorization: `Bearer ${desktopToken}` },
  });
}

function acknowledge(sessionId: string, desktopToken: string, id: string): Promise<Response> {
  return call(`/api/sessions/${sessionId}/ack`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${desktopToken}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({ version: 1, messageId: id }),
  });
}

afterEach(async () => {
  await reset();
});

describe('signature relay API', () => {
  it('carries an encrypted phone signature through the relay for desktop retrieval', async () => {
    const sessionId = nextSessionId();
    const desktopToken = token(1);
    const phoneToken = token(2);
    const created = await createSession(sessionId, desktopToken, phoneToken);
    const { expiresAt } = (await created.json()) as { expiresAt: number };
    const phonePayload = await createEncryptedEnvelope({
      rawBytes: new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      mediaType: 'image/png',
      sessionId,
      expiresAt,
      mode: 'draw',
      keyBytes: fixedBytes(32, 0x44),
      messageId: fixedBytes(16, 0x55),
      iv: fixedBytes(12, 0x66),
    });

    expect((await upload(sessionId, phoneToken, bytesToArrayBuffer(phonePayload.envelope))).status).toBe(201);
    const desktopResponse = await retrieve(sessionId, desktopToken);
    expect(desktopResponse.status).toBe(200);
    expect(new Uint8Array(await desktopResponse.arrayBuffer())).toEqual(phonePayload.envelope);
    expect((await acknowledge(sessionId, desktopToken, bytesToBase64Url(phonePayload.messageId))).status).toBe(204);
    expect((await retrieve(sessionId, desktopToken)).status).toBe(410);
  });

  it('creates once and reports a five-minute logical expiry', async () => {
    const sessionId = nextSessionId();
    const desktopToken = token(1);
    const phoneToken = token(2);
    const beforeCreate = Date.now();
    const created = await createSession(sessionId, desktopToken, phoneToken);

    expect(created.status).toBe(201);
    const { expiresAt } = (await created.json()) as { expiresAt: number };
    expect(expiresAt).toBeGreaterThanOrEqual(beforeCreate + 5 * 60 * 1_000);
    expect(expiresAt).toBeLessThanOrEqual(Date.now() + 5 * 60 * 1_000);
    expect(created.headers.get('Cache-Control')).toBe('no-store');
    expect(created.headers.get('Access-Control-Allow-Origin')).toBeNull();
    expect((await createSession(sessionId, desktopToken, phoneToken)).status).toBe(409);
    expect((await retrieve(sessionId, desktopToken)).status).toBe(204);
  });

  it('keeps GET retryable until an authenticated matching acknowledgement', async () => {
    const sessionId = nextSessionId();
    const desktopToken = token(3);
    const phoneToken = token(4);
    const body = envelope(0x44);
    await createSession(sessionId, desktopToken, phoneToken);
    expect((await upload(sessionId, phoneToken, bytesToArrayBuffer(body))).status).toBe(201);

    const interrupted = await retrieve(sessionId, desktopToken);
    expect(interrupted.status).toBe(200);
    await interrupted.body?.cancel('Simulated interrupted desktop read.');

    const retried = await retrieve(sessionId, desktopToken);
    expect(retried.status).toBe(200);
    expect(new Uint8Array(await retried.arrayBuffer())).toEqual(body);
    expect((await acknowledge(sessionId, desktopToken, messageId(0x45))).status).toBe(409);
    expect((await retrieve(sessionId, desktopToken)).status).toBe(200);
    expect((await acknowledge(sessionId, phoneToken, messageId(0x44))).status).toBe(401);
    expect((await acknowledge(sessionId, desktopToken, messageId(0x44))).status).toBe(204);
    expect((await retrieve(sessionId, desktopToken)).status).toBe(410);
  });

  it('accepts an identical upload retry and rejects a different second envelope', async () => {
    const sessionId = nextSessionId();
    const desktopToken = token(5);
    const phoneToken = token(6);
    const original = envelope(0x51, 0x61);
    await createSession(sessionId, desktopToken, phoneToken);

    expect((await upload(sessionId, phoneToken, bytesToArrayBuffer(original))).status).toBe(201);
    expect((await upload(sessionId, phoneToken, bytesToArrayBuffer(original))).status).toBe(200);
    expect((await upload(sessionId, phoneToken, bytesToArrayBuffer(envelope(0x51, 0x62)))).status).toBe(409);
    expect((await upload(sessionId, phoneToken, bytesToArrayBuffer(envelope(0x52, 0x61)))).status).toBe(409);
  });

  it('streams request bodies, cancels oversize uploads, and rejects content coding', async () => {
    const sessionId = nextSessionId();
    const desktopToken = token(7);
    const phoneToken = token(8);
    await createSession(sessionId, desktopToken, phoneToken);

    let sent = 0;
    let cancelled = false;
    const stream = new ReadableStream<Uint8Array>({
      pull(controller) {
        if (sent === 0) {
          const first = envelope(0x70, 0x22, 64 * 1024);
          sent += first.byteLength;
          controller.enqueue(first);
          return;
        }
        if (sent <= MAX_ENVELOPE_BYTES) {
          const chunk = fixedBytes(64 * 1024, 0x22);
          sent += chunk.byteLength;
          controller.enqueue(chunk);
          return;
        }
        controller.close();
      },
      cancel() {
        cancelled = true;
      },
    });
    const oversized = await upload(sessionId, phoneToken, stream);
    expect(oversized.status).toBe(413);
    expect(cancelled).toBe(true);
    expect((await retrieve(sessionId, desktopToken)).status).toBe(204);

    const encoded = await call(`/api/sessions/${sessionId}/payload`, {
      method: 'PUT',
      headers: {
        Authorization: `Bearer ${phoneToken}`,
        'Content-Type': 'application/octet-stream',
        'Content-Encoding': 'gzip',
      },
      body: bytesToArrayBuffer(envelope(0x71)),
    });
    expect(encoded.status).toBe(415);
  });

  it('enforces role authentication and strict envelope and acknowledgement formats', async () => {
    const sessionId = nextSessionId();
    const desktopToken = token(9);
    const phoneToken = token(10);
    const unrelatedToken = token(11);
    await createSession(sessionId, desktopToken, phoneToken);

    expect((await upload(sessionId, desktopToken, bytesToArrayBuffer(envelope(0x20)))).status).toBe(401);
    expect((await upload(sessionId, unrelatedToken, bytesToArrayBuffer(envelope(0x20)))).status).toBe(401);
    expect((await retrieve(sessionId, phoneToken)).status).toBe(401);
    expect((await upload(sessionId, phoneToken, bytesToArrayBuffer(new Uint8Array([1, 2, 3])))).status).toBe(400);
    expect(
      (
        await call(`/api/sessions/${sessionId}/ack`, {
          method: 'POST',
          headers: { Authorization: `Bearer ${desktopToken}`, 'Content-Type': 'application/json' },
          body: JSON.stringify({ version: 1, messageId: 'invalid' }),
        })
      ).status,
    ).toBe(400);
  });

  it('fails closed when the relay kill switch is not explicitly enabled', async () => {
    expect(isRelayEnabled(undefined)).toBe(false);
    expect(isRelayEnabled(false)).toBe(false);
    expect(isRelayEnabled('false')).toBe(false);
    expect(isRelayEnabled('TRUE')).toBe(false);
    expect(isRelayEnabled('true')).toBe(true);
  });

  it('enforces persisted per-session upload, retrieval, and acknowledgement counters', async () => {
    const sessionId = nextSessionId();
    const desktopToken = token(12);
    const phoneToken = token(13);
    await createSession(sessionId, desktopToken, phoneToken);
    const stub = env.SIGNATURE_SESSIONS.get(env.SIGNATURE_SESSIONS.idFromName(sessionId));

    await runInDurableObject(stub, async (_instance: SignatureSession, state) => {
      const session = (await state.storage.get<StoredSession>('session'))!;
      await state.storage.put('session', { ...session, uploadAttempts: MAX_UPLOAD_ATTEMPTS });
    });
    expect((await upload(sessionId, phoneToken, bytesToArrayBuffer(envelope(0x31)))).status).toBe(429);

    await runInDurableObject(stub, async (_instance: SignatureSession, state) => {
      const session = (await state.storage.get<StoredSession>('session'))!;
      await state.storage.put('session', {
        ...session,
        uploadAttempts: 0,
        retrievalPolls: MAX_RETRIEVAL_POLLS,
      });
    });
    expect((await retrieve(sessionId, desktopToken)).status).toBe(429);

    await runInDurableObject(stub, async (_instance: SignatureSession, state) => {
      const session = (await state.storage.get<StoredSession>('session'))!;
      await state.storage.put('session', {
        ...session,
        retrievalPolls: 0,
        ackAttempts: MAX_ACK_ATTEMPTS,
      });
    });
    expect((await acknowledge(sessionId, desktopToken, messageId(0x31))).status).toBe(429);
  });

  it('applies logical expiry on access and uses the alarm for eventual cleanup', async () => {
    const expiredSessionId = nextSessionId();
    const desktopToken = token(14);
    const phoneToken = token(15);
    await createSession(expiredSessionId, desktopToken, phoneToken);
    const expiredStub = env.SIGNATURE_SESSIONS.get(env.SIGNATURE_SESSIONS.idFromName(expiredSessionId));
    await runInDurableObject(expiredStub, async (_instance: SignatureSession, state) => {
      const session = (await state.storage.get<StoredSession>('session'))!;
      await state.storage.put('session', { ...session, expiresAt: Date.now() - 1 });
    });
    expect((await retrieve(expiredSessionId, desktopToken)).status).toBe(410);

    const alarmSessionId = nextSessionId();
    await createSession(alarmSessionId, desktopToken, phoneToken);
    const alarmStub = env.SIGNATURE_SESSIONS.get(env.SIGNATURE_SESSIONS.idFromName(alarmSessionId));
    expect(await runDurableObjectAlarm(alarmStub)).toBe(true);
    expect((await retrieve(alarmSessionId, desktopToken)).status).toBe(410);
  });

  it('deletes only with the authenticated desktop token', async () => {
    const sessionId = nextSessionId();
    const desktopToken = token(16);
    const phoneToken = token(17);
    await createSession(sessionId, desktopToken, phoneToken);
    expect(
      (
        await call(`/api/sessions/${sessionId}`, {
          method: 'DELETE',
          headers: { Authorization: `Bearer ${phoneToken}` },
        })
      ).status,
    ).toBe(401);
    expect(
      (
        await call(`/api/sessions/${sessionId}`, {
          method: 'DELETE',
          headers: { Authorization: `Bearer ${desktopToken}` },
        })
      ).status,
    ).toBe(204);
    expect((await retrieve(sessionId, desktopToken)).status).toBe(410);
  });
});
