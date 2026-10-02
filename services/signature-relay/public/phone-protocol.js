export const MAX_IMAGE_BYTES = 1024 * 1024;
export const ENVELOPE_FIXED_OVERHEAD_BYTES = 4 + 16 + 12 + 1 + 4 + 16;
export const MAX_ENVELOPE_BYTES = MAX_IMAGE_BYTES + ENVELOPE_FIXED_OVERHEAD_BYTES;

const MAGIC = new TextEncoder().encode('BPS1');
const AAD_PREFIX = new TextEncoder().encode('ButterPaper.PhoneSignature.phone-to-desktop\0');

function bytesToArrayBuffer(bytes) {
  const buffer = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(buffer).set(bytes);
  return buffer;
}

function writeUint32(value) {
  const bytes = new Uint8Array(4);
  new DataView(bytes.buffer).setUint32(0, value, false);
  return bytes;
}

function writeUint64(value) {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value), false);
  return bytes;
}

function concatenate(parts) {
  const output = new Uint8Array(parts.reduce((total, part) => total + part.byteLength, 0));
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.byteLength;
  }
  return output;
}

function validateBytes(value, length, name) {
  if (!(value instanceof Uint8Array) || (length !== undefined && value.byteLength !== length)) {
    throw new TypeError(`${name} has an invalid length.`);
  }
}

export async function createEncryptedEnvelope({
  rawBytes,
  mediaType,
  sessionId,
  expiresAt,
  mode,
  keyBytes,
  messageId = crypto.getRandomValues(new Uint8Array(16)),
  iv = crypto.getRandomValues(new Uint8Array(12)),
}) {
  validateBytes(rawBytes, undefined, 'Image');
  validateBytes(keyBytes, 32, 'AES key');
  validateBytes(messageId, 16, 'Message ID');
  validateBytes(iv, 12, 'IV');
  if (rawBytes.byteLength > MAX_IMAGE_BYTES) throw new RangeError('The image must be 1 MiB or smaller.');
  if (!Number.isSafeInteger(expiresAt) || expiresAt < 0) throw new TypeError('Expiry is invalid.');

  const mediaTypeByte = mediaType === 'image/png' ? 1 : mediaType === 'image/jpeg' ? 2 : 0;
  const modeByte = mode === 'draw' ? 1 : mode === 'image' ? 2 : 0;
  if (!mediaTypeByte || !modeByte) throw new TypeError('Media type or mode is invalid.');

  const plaintext = concatenate([
    new Uint8Array([mediaTypeByte]),
    writeUint32(rawBytes.byteLength),
    rawBytes,
  ]);
  const sessionHash = new Uint8Array(
    await crypto.subtle.digest('SHA-256', new TextEncoder().encode(sessionId)),
  );
  const additionalData = concatenate([
    AAD_PREFIX,
    new Uint8Array([1]),
    sessionHash,
    writeUint64(expiresAt),
    new Uint8Array([modeByte]),
    messageId,
    writeUint32(plaintext.byteLength),
  ]);
  const key = await crypto.subtle.importKey('raw', bytesToArrayBuffer(keyBytes), { name: 'AES-GCM' }, false, [
    'encrypt',
  ]);
  const ciphertextAndTag = new Uint8Array(
    await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv: bytesToArrayBuffer(iv), additionalData: bytesToArrayBuffer(additionalData) },
      key,
      bytesToArrayBuffer(plaintext),
    ),
  );
  const envelope = concatenate([MAGIC, messageId, iv, ciphertextAndTag]);
  if (envelope.byteLength > MAX_ENVELOPE_BYTES) throw new RangeError('The encrypted image is too large.');
  return { envelope, messageId };
}
