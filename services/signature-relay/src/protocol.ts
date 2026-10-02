export const SESSION_TTL_MS = 5 * 60 * 1_000;
export const MAX_IMAGE_BYTES = 1024 * 1024;
export const ENVELOPE_FIXED_OVERHEAD_BYTES = 4 + 16 + 12 + 1 + 4 + 16;
export const MAX_ENVELOPE_BYTES = MAX_IMAGE_BYTES + ENVELOPE_FIXED_OVERHEAD_BYTES;
export const MAX_UPLOAD_ATTEMPTS = 12;
export const MAX_RETRIEVAL_POLLS = 180;
export const MAX_ACK_ATTEMPTS = 12;

export function isRelayEnabled(value: unknown): value is 'true' {
  return value === 'true';
}
