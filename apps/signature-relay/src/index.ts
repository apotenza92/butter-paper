import { DurableObject } from 'cloudflare:workers';
import {
  ENVELOPE_FIXED_OVERHEAD_BYTES,
  isRelayEnabled,
  MAX_ACK_ATTEMPTS,
  MAX_ENVELOPE_BYTES,
  MAX_RETRIEVAL_POLLS,
  MAX_UPLOAD_ATTEMPTS,
  SESSION_TTL_MS,
} from './protocol';

const SESSION_ID_BYTES = 16;
const TOKEN_BYTES = 32;
const TOKEN_HASH_BYTES = 32;
const MESSAGE_ID_BYTES = 16;
const MAX_CREATE_BODY_BYTES = 4 * 1024;
const MAX_ACK_BODY_BYTES = 1024;
const SESSION_KEY = 'session';
const PAYLOAD_KEY = 'payload';
const ENVELOPE_MAGIC = new Uint8Array([0x42, 0x50, 0x53, 0x31]);

const API_SECURITY_HEADERS = {
  'Cache-Control': 'no-store',
  'Content-Security-Policy': "default-src 'none'; frame-ancestors 'none'",
  'Cross-Origin-Resource-Policy': 'same-origin',
  'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
  'Referrer-Policy': 'no-referrer',
  'X-Content-Type-Options': 'nosniff',
  'X-Frame-Options': 'DENY',
} as const;

interface SessionRecord {
  version: 1;
  desktopTokenHash: string;
  phoneTokenHash: string;
  expiresAt: number;
  uploadAttempts: number;
  retrievalPolls: number;
  ackAttempts: number;
}

interface CreateSessionBody {
  version: 1;
  desktopTokenHash: string;
  phoneTokenHash: string;
}

interface AckBody {
  version: 1;
  messageId: string;
}

interface StoredPayload {
  messageId: string;
  envelopeHash: string;
  envelope: Uint8Array;
}

interface OperationResult {
  status: number;
  payload?: Uint8Array;
}

type BodyReadResult =
  | { ok: true; bytes: Uint8Array }
  | { ok: false; status: 400 | 413 };

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

function base64UrlToBytes(value: string): Uint8Array | null {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) return null;

  try {
    const padded = value.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(value.length / 4) * 4, '=');
    const binary = atob(padded);
    const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
    return bytesToBase64Url(bytes) === value ? bytes : null;
  } catch {
    return null;
  }
}

function isBase64UrlBytes(value: unknown, expectedBytes: number): value is string {
  return typeof value === 'string' && base64UrlToBytes(value)?.byteLength === expectedBytes;
}

function hasExactKeys(value: Record<string, unknown>, keys: string[]): boolean {
  return Object.keys(value).sort().join(',') === [...keys].sort().join(',');
}

function isCreateSessionBody(value: unknown): value is CreateSessionBody {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  return (
    hasExactKeys(record, ['version', 'desktopTokenHash', 'phoneTokenHash']) &&
    record.version === 1 &&
    isBase64UrlBytes(record.desktopTokenHash, TOKEN_HASH_BYTES) &&
    isBase64UrlBytes(record.phoneTokenHash, TOKEN_HASH_BYTES)
  );
}

function isAckBody(value: unknown): value is AckBody {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  return (
    hasExactKeys(record, ['version', 'messageId']) &&
    record.version === 1 &&
    isBase64UrlBytes(record.messageId, MESSAGE_ID_BYTES)
  );
}

function apiResponse(status: number, body?: BodyInit | null, headers?: HeadersInit): Response {
  const responseHeaders = new Headers(API_SECURITY_HEADERS);
  if (headers) {
    for (const [name, value] of new Headers(headers)) responseHeaders.set(name, value);
  }
  return new Response(body ?? null, { status, headers: responseHeaders });
}

function jsonResponse(status: number, value: unknown): Response {
  return apiResponse(status, JSON.stringify(value), { 'Content-Type': 'application/json; charset=utf-8' });
}

function withApiHeaders(response: Response): Response {
  const headers = new Headers(response.headers);
  for (const [name, value] of Object.entries(API_SECURITY_HEADERS)) headers.set(name, value);
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

function contentTypeIs(request: Request, expected: string): boolean {
  return request.headers.get('Content-Type')?.split(';', 1)[0]?.trim().toLowerCase() === expected;
}

function hasIdentityContentEncoding(request: Request): boolean {
  const contentEncoding = request.headers.get('Content-Encoding');
  return contentEncoding === null || contentEncoding.trim().toLowerCase() === 'identity';
}

function getBearerToken(request: Request): string | null {
  const authorization = request.headers.get('Authorization');
  if (!authorization?.startsWith('Bearer ')) return null;
  const token = authorization.slice('Bearer '.length);
  return isBase64UrlBytes(token, TOKEN_BYTES) ? token : null;
}

async function readBodyWithLimit(request: Request, limit: number): Promise<BodyReadResult> {
  const contentLength = request.headers.get('Content-Length');
  if (contentLength !== null) {
    const parsedLength = Number(contentLength);
    if (!Number.isSafeInteger(parsedLength) || parsedLength < 0) return { ok: false, status: 400 };
    if (parsedLength > limit) return { ok: false, status: 413 };
  }

  if (!request.body) return { ok: true, bytes: new Uint8Array() };
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let byteLength = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    byteLength += value.byteLength;
    if (byteLength > limit) {
      await reader.cancel('Body exceeds the allowed size.');
      return { ok: false, status: 413 };
    }
    chunks.push(value.slice());
  }

  const body = new Uint8Array(byteLength);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return { ok: true, bytes: body };
}

function parseJson(bytes: Uint8Array): unknown {
  return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
}

async function hashBytes(bytes: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(await crypto.subtle.digest('SHA-256', bytesToArrayBuffer(bytes)));
}

async function hashToken(token: string): Promise<Uint8Array> {
  const tokenBytes = base64UrlToBytes(token);
  if (!tokenBytes) throw new TypeError('Token is not canonical base64url.');
  return hashBytes(tokenBytes);
}

function constantTimeBytesMatch(actual: Uint8Array, expected: Uint8Array): boolean {
  if (actual.byteLength !== expected.byteLength) return false;
  let difference = 0;
  for (let index = 0; index < actual.byteLength; index += 1) difference |= actual[index] ^ expected[index];
  return difference === 0;
}

function constantTimeHashMatches(actualHash: Uint8Array, expectedBase64Url: string): boolean {
  const expectedHash = base64UrlToBytes(expectedBase64Url);
  return expectedHash ? constantTimeBytesMatch(actualHash, expectedHash) : false;
}

function parseEnvelope(envelope: Uint8Array): { messageId: string } | null {
  if (envelope.byteLength < ENVELOPE_FIXED_OVERHEAD_BYTES || envelope.byteLength > MAX_ENVELOPE_BYTES) return null;
  if (!constantTimeBytesMatch(envelope.slice(0, ENVELOPE_MAGIC.byteLength), ENVELOPE_MAGIC)) return null;
  return { messageId: bytesToBase64Url(envelope.slice(4, 4 + MESSAGE_ID_BYTES)) };
}

async function deleteTransactionState(transaction: DurableObjectTransaction): Promise<void> {
  const keys = [...(await transaction.list()).keys()];
  if (keys.length > 0) await transaction.delete(keys);
  await transaction.deleteAlarm();
}

async function applyRateLimit(
  limiter: RateLimit | undefined,
  key: string,
  testBypass: boolean,
): Promise<'allowed' | 'limited' | 'failed'> {
  if (!limiter) return testBypass ? 'allowed' : 'failed';
  try {
    return (await limiter.limit({ key })).success ? 'allowed' : 'limited';
  } catch {
    return 'failed';
  }
}

function sourceRateLimitKey(request: Request): string {
  return request.headers.get('CF-Connecting-IP') ?? 'unknown-client';
}

function rateLimitTestBypass(env: Env): boolean {
  return (env as Env & { RATE_LIMIT_TEST_BYPASS?: string }).RATE_LIMIT_TEST_BYPASS === 'true';
}

function rateLimitResponse(result: 'allowed' | 'limited' | 'failed'): Response | null {
  if (result === 'limited') return apiResponse(429, null, { 'Retry-After': '60' });
  if (result === 'failed') return apiResponse(503);
  return null;
}

export class SignatureSession extends DurableObject<Env> {
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (request.method === 'POST' && url.pathname === '/create') return this.create(request);
    if (request.method === 'PUT' && url.pathname === '/payload') return this.putPayload(request);
    if (request.method === 'GET' && url.pathname === '/payload') return this.getPayload(request);
    if (request.method === 'POST' && url.pathname === '/ack') return this.acknowledge(request);
    if (request.method === 'DELETE' && url.pathname === '/') return this.deleteSession(request);
    return new Response(null, { status: 404 });
  }

  async alarm(): Promise<void> {
    await this.ctx.storage.deleteAll();
  }

  private async create(request: Request): Promise<Response> {
    const body = (await request.json()) as CreateSessionBody;
    const expiresAt = Date.now() + SESSION_TTL_MS;
    const created = await this.ctx.storage.transaction(async (transaction) => {
      if (await transaction.get<SessionRecord>(SESSION_KEY)) return false;
      await transaction.put<SessionRecord>(SESSION_KEY, {
        ...body,
        expiresAt,
        uploadAttempts: 0,
        retrievalPolls: 0,
        ackAttempts: 0,
      });
      return true;
    });
    if (!created) return new Response(null, { status: 409 });

    try {
      await this.ctx.storage.setAlarm(expiresAt);
    } catch (error) {
      await this.ctx.storage.deleteAll();
      throw error;
    }
    return Response.json({ expiresAt }, { status: 201 });
  }

  private async putPayload(request: Request): Promise<Response> {
    const token = getBearerToken(request);
    if (!token) return new Response(null, { status: 401 });
    const envelope = new Uint8Array(await request.arrayBuffer());
    const envelopeData = parseEnvelope(envelope);
    if (!envelopeData) return new Response(null, { status: 400 });
    const tokenHash = await hashToken(token);
    const envelopeHash = bytesToBase64Url(await hashBytes(envelope));
    const now = Date.now();

    const result = await this.ctx.storage.transaction<OperationResult>(async (transaction) => {
      const session = await transaction.get<SessionRecord>(SESSION_KEY);
      if (!session) return { status: 410 };
      if (session.expiresAt <= now) {
        await deleteTransactionState(transaction);
        return { status: 410 };
      }
      if (!constantTimeHashMatches(tokenHash, session.phoneTokenHash)) return { status: 401 };
      if (session.uploadAttempts >= MAX_UPLOAD_ATTEMPTS) return { status: 429 };
      await transaction.put<SessionRecord>(SESSION_KEY, {
        ...session,
        uploadAttempts: session.uploadAttempts + 1,
      });

      const stored = await transaction.get<StoredPayload>(PAYLOAD_KEY);
      if (stored) {
        const identical =
          stored.messageId === envelopeData.messageId &&
          stored.envelopeHash === envelopeHash &&
          constantTimeBytesMatch(stored.envelope, envelope);
        return { status: identical ? 200 : 409 };
      }

      await transaction.put<StoredPayload>(PAYLOAD_KEY, {
        messageId: envelopeData.messageId,
        envelopeHash,
        envelope,
      });
      return { status: 201 };
    });

    if (result.status === 410) await this.ctx.storage.deleteAll();
    return new Response(null, {
      status: result.status,
      headers: result.status === 429 ? { 'Retry-After': '60' } : undefined,
    });
  }

  private async getPayload(request: Request): Promise<Response> {
    const token = getBearerToken(request);
    if (!token) return new Response(null, { status: 401 });
    const tokenHash = await hashToken(token);
    const now = Date.now();

    const result = await this.ctx.storage.transaction<OperationResult>(async (transaction) => {
      const session = await transaction.get<SessionRecord>(SESSION_KEY);
      if (!session) return { status: 410 };
      if (session.expiresAt <= now) {
        await deleteTransactionState(transaction);
        return { status: 410 };
      }
      if (!constantTimeHashMatches(tokenHash, session.desktopTokenHash)) return { status: 401 };
      if (session.retrievalPolls >= MAX_RETRIEVAL_POLLS) return { status: 429 };
      await transaction.put<SessionRecord>(SESSION_KEY, {
        ...session,
        retrievalPolls: session.retrievalPolls + 1,
      });

      const stored = await transaction.get<StoredPayload>(PAYLOAD_KEY);
      return stored ? { status: 200, payload: stored.envelope } : { status: 204 };
    });

    if (result.status === 410) await this.ctx.storage.deleteAll();
    return new Response(result.payload ? bytesToArrayBuffer(result.payload) : null, {
      status: result.status,
      headers: {
        ...(result.payload ? { 'Content-Type': 'application/octet-stream' } : {}),
        ...(result.status === 429 ? { 'Retry-After': '60' } : {}),
      },
    });
  }

  private async acknowledge(request: Request): Promise<Response> {
    const token = getBearerToken(request);
    if (!token) return new Response(null, { status: 401 });
    const body = (await request.json()) as AckBody;
    const tokenHash = await hashToken(token);
    const now = Date.now();

    const status = await this.ctx.storage.transaction<number>(async (transaction) => {
      const session = await transaction.get<SessionRecord>(SESSION_KEY);
      if (!session) return 410;
      if (session.expiresAt <= now) {
        await deleteTransactionState(transaction);
        return 410;
      }
      if (!constantTimeHashMatches(tokenHash, session.desktopTokenHash)) return 401;
      if (session.ackAttempts >= MAX_ACK_ATTEMPTS) return 429;
      await transaction.put<SessionRecord>(SESSION_KEY, {
        ...session,
        ackAttempts: session.ackAttempts + 1,
      });

      const stored = await transaction.get<StoredPayload>(PAYLOAD_KEY);
      if (!stored || stored.messageId !== body.messageId) return 409;
      await deleteTransactionState(transaction);
      return 204;
    });

    if (status === 204 || status === 410) await this.ctx.storage.deleteAll();
    return new Response(null, {
      status,
      headers: status === 429 ? { 'Retry-After': '60' } : undefined,
    });
  }

  private async deleteSession(request: Request): Promise<Response> {
    const token = getBearerToken(request);
    if (!token) return new Response(null, { status: 401 });
    const tokenHash = await hashToken(token);
    const now = Date.now();

    const status = await this.ctx.storage.transaction<number>(async (transaction) => {
      const session = await transaction.get<SessionRecord>(SESSION_KEY);
      if (!session) return 410;
      if (session.expiresAt <= now) {
        await deleteTransactionState(transaction);
        return 410;
      }
      if (!constantTimeHashMatches(tokenHash, session.desktopTokenHash)) return 401;
      await deleteTransactionState(transaction);
      return 204;
    });

    if (status === 204 || status === 410) await this.ctx.storage.deleteAll();
    return new Response(null, { status });
  }
}

async function readJsonRequest(request: Request, limit: number): Promise<{ response?: Response; value?: unknown }> {
  if (!contentTypeIs(request, 'application/json')) return { response: apiResponse(415) };
  const body = await readBodyWithLimit(request, limit);
  if (!body.ok) return { response: apiResponse(body.status) };
  try {
    return { value: parseJson(body.bytes) };
  } catch {
    return { response: jsonResponse(400, { error: 'Invalid JSON.' }) };
  }
}

async function handleApiRequest(request: Request, env: Env): Promise<Response> {
  if (!isRelayEnabled(env.RELAY_ENABLED)) return apiResponse(503);
  if (!hasIdentityContentEncoding(request)) return apiResponse(415);

  const url = new URL(request.url);
  const match = /^\/api\/sessions\/([^/]+)(\/payload|\/ack)?$/u.exec(url.pathname);
  if (!match) return apiResponse(404);
  const sessionId = match[1];
  const subroute = match[2] ?? '';
  if (!isBase64UrlBytes(sessionId, SESSION_ID_BYTES)) return jsonResponse(400, { error: 'Invalid session ID.' });

  const stub = env.SIGNATURE_SESSIONS.get(env.SIGNATURE_SESSIONS.idFromName(sessionId));

  if (subroute === '' && request.method === 'POST') {
    const edgeLimit = rateLimitResponse(await applyRateLimit(
      env.CREATE_RATE_LIMITER,
      sourceRateLimitKey(request),
      rateLimitTestBypass(env),
    ));
    if (edgeLimit) return edgeLimit;
    const parsed = await readJsonRequest(request, MAX_CREATE_BODY_BYTES);
    if (parsed.response) return parsed.response;
    if (!isCreateSessionBody(parsed.value)) return jsonResponse(400, { error: 'Invalid session data.' });
    return withApiHeaders(
      await stub.fetch('https://session.internal/create', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(parsed.value),
      }),
    );
  }

  if (subroute === '/payload' && request.method === 'PUT') {
    if (!contentTypeIs(request, 'application/octet-stream')) return apiResponse(415);
    const token = getBearerToken(request);
    if (!token) return apiResponse(401);
    const edgeLimit = rateLimitResponse(await applyRateLimit(
      env.UPLOAD_RATE_LIMITER,
      sourceRateLimitKey(request),
      rateLimitTestBypass(env),
    ));
    if (edgeLimit) return edgeLimit;
    const body = await readBodyWithLimit(request, MAX_ENVELOPE_BYTES);
    if (!body.ok) return apiResponse(body.status);
    if (!parseEnvelope(body.bytes)) return jsonResponse(400, { error: 'Invalid encrypted envelope.' });
    return withApiHeaders(
      await stub.fetch('https://session.internal/payload', {
        method: 'PUT',
        headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/octet-stream' },
        body: bytesToArrayBuffer(body.bytes),
      }),
    );
  }

  if (subroute === '/payload' && request.method === 'GET') {
    const token = getBearerToken(request);
    if (!token) return apiResponse(401);
    const edgeLimit = rateLimitResponse(await applyRateLimit(
      env.RETRIEVAL_RATE_LIMITER,
      sourceRateLimitKey(request),
      rateLimitTestBypass(env),
    ));
    if (edgeLimit) return edgeLimit;
    return withApiHeaders(
      await stub.fetch('https://session.internal/payload', {
        method: 'GET',
        headers: { Authorization: `Bearer ${token}` },
      }),
    );
  }

  if (subroute === '/ack' && request.method === 'POST') {
    const token = getBearerToken(request);
    if (!token) return apiResponse(401);
    const edgeLimit = rateLimitResponse(await applyRateLimit(
      env.RETRIEVAL_RATE_LIMITER,
      sourceRateLimitKey(request),
      rateLimitTestBypass(env),
    ));
    if (edgeLimit) return edgeLimit;
    const parsed = await readJsonRequest(request, MAX_ACK_BODY_BYTES);
    if (parsed.response) return parsed.response;
    if (!isAckBody(parsed.value)) return jsonResponse(400, { error: 'Invalid acknowledgement.' });
    return withApiHeaders(
      await stub.fetch('https://session.internal/ack', {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
        body: JSON.stringify(parsed.value),
      }),
    );
  }

  if (subroute === '' && request.method === 'DELETE') {
    const token = getBearerToken(request);
    if (!token) return apiResponse(401);
    const edgeLimit = rateLimitResponse(await applyRateLimit(
      env.RETRIEVAL_RATE_LIMITER,
      sourceRateLimitKey(request),
      rateLimitTestBypass(env),
    ));
    if (edgeLimit) return edgeLimit;
    return withApiHeaders(
      await stub.fetch('https://session.internal/', {
        method: 'DELETE',
        headers: { Authorization: `Bearer ${token}` },
      }),
    );
  }

  const allow = subroute === '/payload' ? 'GET, PUT' : subroute === '/ack' ? 'POST' : 'POST, DELETE';
  return apiResponse(405, null, { Allow: allow });
}

export default {
  async fetch(request, env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname.startsWith('/api/')) return handleApiRequest(request, env);
    return env.ASSETS.fetch(request);
  },
} satisfies ExportedHandler<Env>;
