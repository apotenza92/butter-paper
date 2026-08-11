import {
  createCipheriv,
  createDecipheriv,
  createHash,
  randomBytes,
} from 'node:crypto';
import QRCode from 'qrcode/lib/browser.js';
import type {
  PhoneSignatureImage,
  PhoneSignatureMode,
  PhoneSignaturePollResult,
  PhoneSignatureSession,
} from '../shared/protocol';

export const MAX_PHONE_SIGNATURE_BYTES = 1024 * 1024;
const SESSION_LIFETIME_MS = 5 * 60 * 1000;
const ENVELOPE_MAGIC = Buffer.from('BPS1', 'ascii');
const MESSAGE_ID_BYTES = 16;
const IV_BYTES = 12;
const AUTH_TAG_BYTES = 16;
const PLAINTEXT_HEADER_BYTES = 5;
const MAX_ENCRYPTED_BYTES = MAX_PHONE_SIGNATURE_BYTES
  + ENVELOPE_MAGIC.byteLength + MESSAGE_ID_BYTES + IV_BYTES + AUTH_TAG_BYTES + PLAINTEXT_HEADER_BYTES;
const AAD_DOMAIN = Buffer.from('ButterPaper.PhoneSignature.phone-to-desktop\0', 'utf8');

declare const BP_SIGNATURE_RELAY_PRODUCTION_ORIGIN: string | undefined;

interface TransferSession {
  readonly id: string;
  readonly ownerWebContentsId: number;
  readonly relayOrigin: string;
  readonly desktopToken: Buffer;
  encryptionKey: Buffer | null;
  readonly mode: PhoneSignatureMode;
  readonly expiresAt: number;
  ackMessageId: Buffer | null;
  ackAttempts: number;
  ackTimer: ReturnType<typeof setTimeout> | null;
}

interface PhoneSignatureTransferOptions {
  readonly relayOrigin?: string;
  readonly allowInsecureLoopback?: boolean;
  readonly fetch?: typeof fetch;
  readonly now?: () => number;
  readonly randomBytes?: (size: number) => Buffer;
  readonly sanitizeImage?: (image: PhoneSignatureImage) => Promise<PhoneSignatureImage>;
}

interface PhoneSignaturePayload {
  readonly mode: PhoneSignatureMode;
  readonly mimeType: PhoneSignatureImage['mimeType'];
  readonly dataUrl: string;
}

interface DecryptedPhonePayload {
  readonly image: PhoneSignatureImage;
  readonly messageId: Buffer;
}

export class PhoneSignatureTransferService {
  private readonly sessions = new Map<string, TransferSession>();
  private readonly options: PhoneSignatureTransferOptions;

  constructor(options: PhoneSignatureTransferOptions = {}) {
    this.options = options;
  }

  async start(ownerWebContentsId: number, mode: PhoneSignatureMode): Promise<PhoneSignatureSession> {
    this.stopOwner(ownerWebContentsId);
    const testMode = this.options.allowInsecureLoopback === true;
    const productionOrigin = typeof BP_SIGNATURE_RELAY_PRODUCTION_ORIGIN === 'string'
      ? BP_SIGNATURE_RELAY_PRODUCTION_ORIGIN
      : undefined;
    const relayOrigin = resolveSignatureRelayOrigin(
      testMode ? this.options.relayOrigin : productionOrigin,
      testMode,
    );
    const random = this.options.randomBytes ?? randomBytes;
    const idBytes = random(16);
    const desktopToken = random(32);
    const phoneToken = random(32);
    const encryptionKey = random(32);
    const id = idBytes.toString('base64url');
    const request = this.options.fetch ?? fetch;
    let response: Response;
    try {
      response = await request(`${relayOrigin}/api/sessions/${id}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          version: 1,
          desktopTokenHash: hashToken(desktopToken),
          phoneTokenHash: hashToken(phoneToken),
        }),
      });
    } catch {
      clearSecrets(desktopToken, phoneToken, encryptionKey);
      throw new Error('Unable to connect to the phone signature relay.');
    }
    if (response.status !== 201) {
      clearSecrets(desktopToken, phoneToken, encryptionKey);
      throw new Error(await readRelayError(response, 'Unable to create a private phone signature session.'));
    }
    let body: { expiresAt?: unknown };
    try {
      body = await response.json() as { expiresAt?: unknown };
    } catch {
      clearSecrets(desktopToken, phoneToken, encryptionKey);
      throw new Error('The phone signature relay returned an invalid response.');
    }
    const now = this.options.now?.() ?? Date.now();
    const expiresAt = typeof body.expiresAt === 'number' && Number.isSafeInteger(body.expiresAt)
      ? body.expiresAt
      : now + SESSION_LIFETIME_MS;
    if (expiresAt <= now || expiresAt > now + SESSION_LIFETIME_MS + 30_000) {
      clearSecrets(desktopToken, phoneToken, encryptionKey);
      throw new Error('The phone signature relay returned an invalid expiry time.');
    }

    const fragment = new URLSearchParams({
      v: '1',
      m: mode,
      e: String(expiresAt),
      t: phoneToken.toString('base64url'),
      k: encryptionKey.toString('base64url'),
    });
    const phoneUrl = `${relayOrigin}/s/${id}#${fragment.toString()}`;
    let qrSvg: string;
    try {
      qrSvg = await QRCode.toString(phoneUrl, {
        errorCorrectionLevel: 'M',
        margin: 1,
        width: 240,
      });
    } catch {
      await request(`${relayOrigin}/api/sessions/${id}`, {
        method: 'DELETE',
        headers: { Authorization: `Bearer ${desktopToken.toString('base64url')}` },
      }).catch(() => undefined);
      clearSecrets(desktopToken, phoneToken, encryptionKey);
      throw new Error('Unable to create the phone signature QR code.');
    }
    phoneToken.fill(0);
    const session: TransferSession = {
      id,
      ownerWebContentsId,
      relayOrigin,
      desktopToken,
      encryptionKey,
      mode,
      expiresAt,
      ackMessageId: null,
      ackAttempts: 0,
      ackTimer: null,
    };
    this.sessions.set(id, session);
    return {
      id,
      qrDataUrl: `data:image/svg+xml;base64,${Buffer.from(qrSvg).toString('base64')}`,
      expiresAt,
      mode,
    };
  }

  async poll(id: string, ownerWebContentsId: number): Promise<PhoneSignaturePollResult> {
    const session = this.requireOwnedSession(id, ownerWebContentsId);
    if (session.ackMessageId) throw new Error('This phone signature session is not available.');
    const now = this.options.now?.() ?? Date.now();
    if (now >= session.expiresAt) {
      this.destroySession(id);
      return { status: 'expired' };
    }
    const request = this.options.fetch ?? fetch;
    let response: Response;
    try {
      response = await request(`${session.relayOrigin}/api/sessions/${id}/payload`, {
        headers: { Authorization: `Bearer ${session.desktopToken.toString('base64url')}` },
      });
    } catch {
      return { status: 'waiting' };
    }
    if (response.status === 204) return { status: 'waiting' };
    if (response.status === 404 || response.status === 410) {
      this.destroySession(id);
      return { status: 'expired' };
    }
    if (response.status !== 200) {
      throw new Error(await readRelayError(response, 'Unable to receive the phone signature.'));
    }
    const contentLength = Number(response.headers.get('content-length') ?? 0);
    if (contentLength > MAX_ENCRYPTED_BYTES) {
      await this.stop(id, ownerWebContentsId);
      throw new Error('The encrypted phone signature is too large.');
    }
    const encrypted = Buffer.from(await response.arrayBuffer());
    if (encrypted.byteLength > MAX_ENCRYPTED_BYTES) {
      await this.stop(id, ownerWebContentsId);
      throw new Error('The encrypted phone signature is too large.');
    }
    let decrypted: DecryptedPhonePayload;
    try {
      decrypted = decryptPhoneSignaturePayload(encrypted, session);
    } catch (error) {
      await this.stop(id, ownerWebContentsId);
      throw error;
    }
    const sanitizeImage = this.options.sanitizeImage;
    if (!sanitizeImage) {
      await this.stop(id, ownerWebContentsId);
      decrypted.messageId.fill(0);
      throw new Error('The isolated phone signature sanitizer is not configured.');
    }
    let image: PhoneSignatureImage;
    try {
      image = await sanitizeImage(decrypted.image);
    } catch (error) {
      await this.stop(id, ownerWebContentsId);
      decrypted.messageId.fill(0);
      throw error;
    }
    session.encryptionKey?.fill(0);
    session.encryptionKey = null;
    session.ackMessageId = decrypted.messageId;
    const acknowledged = await this.tryAcknowledge(session);
    if (!acknowledged && this.sessions.has(id)) this.scheduleAcknowledgement(session);
    return { status: 'received', image };
  }

  async stop(id: string, ownerWebContentsId: number): Promise<void> {
    const session = this.sessions.get(id);
    if (!session) return;
    if (session.ownerWebContentsId !== ownerWebContentsId) {
      throw new Error('This phone signature session belongs to another window.');
    }
    const desktopBearer = session.desktopToken.toString('base64url');
    const relayOrigin = session.relayOrigin;
    this.destroySession(id);
    const request = this.options.fetch ?? fetch;
    try {
      await request(`${relayOrigin}/api/sessions/${id}`, {
        method: 'DELETE',
        headers: { Authorization: `Bearer ${desktopBearer}` },
      });
    } catch {
      // The relay session expires independently. Local cancellation must still complete.
    }
  }

  stopOwner(ownerWebContentsId: number): void {
    for (const session of [...this.sessions.values()]) {
      if (session.ownerWebContentsId === ownerWebContentsId) {
        void this.stop(session.id, ownerWebContentsId);
      }
    }
  }

  dispose(): void {
    for (const id of [...this.sessions.keys()]) this.destroySession(id);
  }

  private requireOwnedSession(id: string, ownerWebContentsId: number): TransferSession {
    const session = this.sessions.get(id);
    if (!session || session.ownerWebContentsId !== ownerWebContentsId) {
      throw new Error('This phone signature session is not available.');
    }
    return session;
  }

  private destroySession(id: string): void {
    const session = this.sessions.get(id);
    if (!session) return;
    this.sessions.delete(id);
    if (session.ackTimer) clearTimeout(session.ackTimer);
    session.ackTimer = null;
    clearSecrets(session.desktopToken);
    if (session.encryptionKey) clearSecrets(session.encryptionKey);
    session.encryptionKey = null;
    if (session.ackMessageId) clearSecrets(session.ackMessageId);
    session.ackMessageId = null;
  }

  private async tryAcknowledge(session: TransferSession): Promise<boolean> {
    const messageId = session.ackMessageId;
    if (!messageId) return true;
    session.ackAttempts += 1;
    const request = this.options.fetch ?? fetch;
    try {
      const response = await request(`${session.relayOrigin}/api/sessions/${session.id}/ack`, {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${session.desktopToken.toString('base64url')}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ version: 1, messageId: messageId.toString('base64url') }),
      });
      if (response.status === 204 || response.status === 410) {
        this.destroySession(session.id);
        return true;
      }
    } catch {
      // Retry below while retaining only the acknowledgement capability.
    }
    if (session.ackAttempts >= 3) this.destroySession(session.id);
    return false;
  }

  private scheduleAcknowledgement(session: TransferSession): void {
    if (session.ackTimer || session.ackAttempts >= 3) return;
    const delay = session.ackAttempts === 1 ? 2_000 : 8_000;
    session.ackTimer = setTimeout(() => {
      session.ackTimer = null;
      void this.tryAcknowledge(session).then((acknowledged) => {
        if (!acknowledged && this.sessions.has(session.id)) this.scheduleAcknowledgement(session);
      });
    }, delay);
  }
}

export function resolveSignatureRelayOrigin(value: string | undefined, testMode = false): string {
  if (!value?.trim()) {
    throw new Error(testMode
      ? 'The test phone signature relay is not configured.'
      : 'Phone transfer is not configured in this build.');
  }
  let url: URL;
  try {
    url = new URL(value.trim());
  } catch {
    throw new Error('The phone signature relay URL is invalid.');
  }
  const loopback = url.hostname === '127.0.0.1' || url.hostname === 'localhost' || url.hostname === '::1';
  if (url.protocol !== 'https:' && !(testMode && loopback && url.protocol === 'http:')) {
    throw new Error('The phone signature relay must use HTTPS.');
  }
  if (url.username || url.password || url.search || url.hash || (url.pathname !== '/' && url.pathname !== '')) {
    throw new Error('The phone signature relay URL must contain only an origin.');
  }
  return url.origin;
}

export function encryptPhoneSignaturePayloadForTest(
  session: { id: string; mode: PhoneSignatureMode; expiresAt: number },
  encryptionKey: Buffer,
  payload: PhoneSignaturePayload,
  messageId = Buffer.alloc(MESSAGE_ID_BYTES, 6),
  iv = Buffer.alloc(IV_BYTES, 7),
): Buffer {
  const imageBytes = decodeCanonicalDataUrl(payload.dataUrl, payload.mimeType);
  const plaintext = Buffer.alloc(PLAINTEXT_HEADER_BYTES + imageBytes.byteLength);
  plaintext[0] = payload.mimeType === 'image/png' ? 1 : 2;
  plaintext.writeUInt32BE(imageBytes.byteLength, 1);
  imageBytes.copy(plaintext, PLAINTEXT_HEADER_BYTES);
  const cipher = createCipheriv('aes-256-gcm', encryptionKey, iv);
  cipher.setAAD(payloadAad(session, messageId, plaintext.byteLength));
  const encrypted = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  return Buffer.concat([ENVELOPE_MAGIC, messageId, iv, encrypted, cipher.getAuthTag()]);
}

function decryptPhoneSignaturePayload(encrypted: Buffer, session: TransferSession): DecryptedPhonePayload {
  const minimumLength = ENVELOPE_MAGIC.byteLength + MESSAGE_ID_BYTES + IV_BYTES
    + AUTH_TAG_BYTES + PLAINTEXT_HEADER_BYTES;
  if (encrypted.byteLength < minimumLength
    || !encrypted.subarray(0, ENVELOPE_MAGIC.byteLength).equals(ENVELOPE_MAGIC)) {
    throw new Error('The encrypted phone signature has an invalid envelope.');
  }
  let offset = ENVELOPE_MAGIC.byteLength;
  const messageId = Buffer.from(encrypted.subarray(offset, offset + MESSAGE_ID_BYTES));
  offset += MESSAGE_ID_BYTES;
  const iv = encrypted.subarray(offset, offset + IV_BYTES);
  offset += IV_BYTES;
  const authTagOffset = encrypted.byteLength - AUTH_TAG_BYTES;
  const plaintextLength = authTagOffset - offset;
  if (!session.encryptionKey) throw new Error('The phone signature session key is not available.');
  const decipher = createDecipheriv('aes-256-gcm', session.encryptionKey, iv);
  decipher.setAAD(payloadAad(session, messageId, plaintextLength));
  decipher.setAuthTag(encrypted.subarray(authTagOffset));
  let plaintext: Buffer;
  try {
    plaintext = Buffer.concat([
      decipher.update(encrypted.subarray(offset, authTagOffset)),
      decipher.final(),
    ]);
  } catch {
    messageId.fill(0);
    throw new Error('The phone signature could not be authenticated.');
  }
  const mediaType = plaintext[0];
  const imageLength = plaintext.readUInt32BE(1);
  if ((mediaType !== 1 && mediaType !== 2)
    || imageLength === 0 || imageLength > MAX_PHONE_SIGNATURE_BYTES
    || plaintext.byteLength !== PLAINTEXT_HEADER_BYTES + imageLength) {
    messageId.fill(0);
    throw new Error('The phone signature payload is invalid.');
  }
  const mimeType = mediaType === 1 ? 'image/png' : 'image/jpeg';
  const imageBytes = plaintext.subarray(PLAINTEXT_HEADER_BYTES);
  if (!hasExpectedImageSignature(imageBytes, mimeType)) {
    messageId.fill(0);
    throw new Error('The phone signature is not a valid PNG or JPEG image.');
  }
  return {
    messageId,
    image: {
      dataUrl: `data:${mimeType};base64,${imageBytes.toString('base64')}`,
      mimeType,
      mode: session.mode,
    },
  };
}

function payloadAad(
  session: { id: string; mode: PhoneSignatureMode; expiresAt: number },
  messageId: Buffer,
  plaintextLength: number,
): Buffer {
  const expiry = Buffer.alloc(8);
  expiry.writeBigUInt64BE(BigInt(session.expiresAt));
  const length = Buffer.alloc(4);
  length.writeUInt32BE(plaintextLength);
  return Buffer.concat([
    AAD_DOMAIN,
    Buffer.from([1]),
    createHash('sha256').update(session.id, 'ascii').digest(),
    expiry,
    Buffer.from([session.mode === 'draw' ? 1 : 2]),
    messageId,
    length,
  ]);
}

function decodeCanonicalDataUrl(dataUrl: string, mimeType: PhoneSignatureImage['mimeType']): Buffer {
  const prefix = `data:${mimeType};base64,`;
  if (!dataUrl.startsWith(prefix)) throw new Error('The phone signature image encoding is invalid.');
  const encodedImage = dataUrl.slice(prefix.length);
  const imageBytes = Buffer.from(encodedImage, 'base64');
  if (imageBytes.byteLength === 0 || imageBytes.byteLength > MAX_PHONE_SIGNATURE_BYTES
    || imageBytes.toString('base64') !== encodedImage
    || !hasExpectedImageSignature(imageBytes, mimeType)) {
    throw new Error('The phone signature is not a valid PNG or JPEG image.');
  }
  return imageBytes;
}

function hashToken(token: Buffer): string {
  return createHash('sha256').update(token).digest('base64url');
}

function hasExpectedImageSignature(bytes: Buffer, mimeType: PhoneSignatureImage['mimeType']): boolean {
  if (mimeType === 'image/png') {
    return bytes.length >= 8
      && bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47
      && bytes[4] === 0x0d && bytes[5] === 0x0a && bytes[6] === 0x1a && bytes[7] === 0x0a;
  }
  return bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff;
}

function clearSecrets(...secrets: Buffer[]): void {
  for (const secret of secrets) secret.fill(0);
}

async function readRelayError(response: Response, fallback: string): Promise<string> {
  try {
    const message = (await response.text()).trim();
    return message && message.length <= 240 ? message : fallback;
  } catch {
    return fallback;
  }
}
