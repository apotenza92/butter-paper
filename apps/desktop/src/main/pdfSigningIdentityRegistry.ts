import { createHash, randomUUID } from 'node:crypto';
import { constants, type BigIntStats } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import { lstat, open, realpath, type FileHandle } from 'node:fs/promises';
import type { PdfSignatureCorePkcs12Inspection } from './pdfSignatureCoreSigning';

const MAX_PKCS12_BYTES = 16 * 1024 * 1024;
const DEFAULT_HANDLE_TTL_MS = 5 * 60_000;
const MAX_HANDLE_TTL_MS = 15 * 60_000;
const HANDLE_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const SHA256_PATTERN = /^[a-f0-9]{64}$/;

export interface PdfSigningIdentityPicker {
  /** Main-owned native picker result. The path must never cross renderer IPC. */
  pickPkcs12File(ownerWindowId: number, options?: { readonly signal?: AbortSignal }): Promise<{ readonly canceled: true } | {
    readonly canceled: false;
    readonly filePath: string;
  }>;
}

export interface PdfSigningCertificateDescriptor {
  readonly sha256Fingerprint: string;
  readonly serialNumber: string;
  readonly subjectDisplayName: string;
  readonly issuerDisplayName: string;
  readonly notBefore: string;
  readonly notAfter: string;
  readonly publicKeyAlgorithm: string;
  readonly publicKeyBits: number;
  readonly supportedDigests: readonly ('SHA-256' | 'SHA-384' | 'SHA-512')[];
  readonly suitableForSigning: boolean;
}

export interface PdfSigningIdentityInspector {
  /** The sidecar owns password capture. This interface deliberately has no password parameter. */
  inspectPkcs12(
    pkcs12Frame: Uint8Array,
    options?: { readonly signal?: AbortSignal },
  ): Promise<PdfSignatureCorePkcs12Inspection>;
}

export interface PdfSigningIdentitySelection {
  readonly handle: string;
  readonly expiresAt: string;
  readonly certificates: readonly PdfSigningCertificateDescriptor[];
}

export type PdfSigningIdentityRegistryErrorCode =
  | 'INVALID_REQUEST'
  | 'UNSAFE_IDENTITY_FILE'
  | 'IDENTITY_FILE_TOO_LARGE'
  | 'IDENTITY_CHANGED'
  | 'HANDLE_INVALID'
  | 'HANDLE_EXPIRED'
  | 'HANDLE_WINDOW_MISMATCH'
  | 'OWNER_UNAVAILABLE'
  | 'INSPECTION_FAILED';

export class PdfSigningIdentityRegistryError extends Error {
  constructor(readonly code: PdfSigningIdentityRegistryErrorCode, message: string) {
    super(message);
    this.name = 'PdfSigningIdentityRegistryError';
  }
}

interface StableIdentity {
  readonly dev: bigint;
  readonly ino: bigint;
  readonly size: bigint;
  readonly mtimeNs: bigint;
  readonly ctimeNs: bigint;
  readonly nlink: bigint;
}

interface RegisteredIdentity {
  readonly handle: string;
  readonly ownerWindowId: number;
  readonly ownerGeneration: number;
  readonly canonicalPath: string;
  readonly fileIdentity: StableIdentity;
  readonly sha256: string;
  readonly expiresAtMs: number;
  readonly certificates: readonly PdfSigningCertificateDescriptor[];
}

interface OwnerState {
  readonly generation: number;
  readonly active: boolean;
  readonly controller: AbortController;
}

export interface PdfSigningIdentityOwnerLease {
  readonly ownerWindowId: number;
  readonly generation: number;
  readonly signal: AbortSignal;
}

export interface PdfSigningIdentityRegistryOptions {
  readonly now?: () => number;
  readonly createOpaqueHandle?: () => string;
  readonly handleTtlMs?: number;
  readonly maxPkcs12Bytes?: number;
}

/**
 * Main-process registry for short-lived identity capabilities. It retains only
 * a verified file identity and public certificate metadata. PKCS#12 bytes are
 * read just-in-time, passed through a bounded callback, and cleared afterward.
 */
export class PdfSigningIdentityRegistry {
  private readonly entries = new Map<string, RegisteredIdentity>();
  private readonly ownerStates = new Map<number, OwnerState>();
  private readonly now: () => number;
  private readonly createOpaqueHandle: () => string;
  private readonly handleTtlMs: number;
  private readonly maxPkcs12Bytes: number;

  constructor(
    private readonly picker: PdfSigningIdentityPicker,
    private readonly inspector: PdfSigningIdentityInspector,
    options: PdfSigningIdentityRegistryOptions = {},
  ) {
    this.now = options.now ?? Date.now;
    this.createOpaqueHandle = options.createOpaqueHandle ?? randomUUID;
    this.handleTtlMs = boundedPositiveInteger(options.handleTtlMs, DEFAULT_HANDLE_TTL_MS, MAX_HANDLE_TTL_MS);
    this.maxPkcs12Bytes = boundedPositiveInteger(options.maxPkcs12Bytes, MAX_PKCS12_BYTES, MAX_PKCS12_BYTES);
  }

  registerOwner(ownerWindowId: number): PdfSigningIdentityOwnerLease {
    assertWindowId(ownerWindowId);
    const previous = this.ownerStates.get(ownerWindowId);
    previous?.controller.abort();
    const generation = (previous?.generation ?? 0) + 1;
    const controller = new AbortController();
    this.ownerStates.set(ownerWindowId, { generation, active: true, controller });
    for (const [handle, entry] of this.entries) {
      if (entry.ownerWindowId === ownerWindowId) this.entries.delete(handle);
    }
    return Object.freeze({ ownerWindowId, generation, signal: controller.signal });
  }

  isOwnerGenerationActive(ownerWindowId: number, generation: number): boolean {
    assertWindowId(ownerWindowId);
    return this.ownerStates.get(ownerWindowId)?.active === true
      && this.ownerStates.get(ownerWindowId)?.generation === generation;
  }

  async choose(
    ownerWindowId: number,
    options: { readonly generation?: number; readonly signal?: AbortSignal } = {},
  ): Promise<PdfSigningIdentitySelection | null> {
    assertWindowId(ownerWindowId);
    throwIfAborted(options.signal);
    const ownerState = this.getActiveOwnerState(ownerWindowId, options.generation);
    const generation = ownerState.generation;
    const signal = combineAbortSignals(ownerState.controller.signal, options.signal);
    try {
      const picked = await this.picker.pickPkcs12File(ownerWindowId, { signal: signal.signal });
      assertOwnerGeneration(this.ownerStates, ownerWindowId, generation);
      throwIfAborted(signal.signal);
      if (picked.canceled) return null;

      const opened = await openStablePkcs12(picked.filePath, this.maxPkcs12Bytes);
      let bytes: Buffer | undefined;
      try {
        assertOwnerGeneration(this.ownerStates, ownerWindowId, generation);
        bytes = await readStableHandle(opened.handle, opened.identity, this.maxPkcs12Bytes);
        assertOwnerGeneration(this.ownerStates, ownerWindowId, generation);
        throwIfAborted(signal.signal);
        let inspected: Awaited<ReturnType<PdfSigningIdentityInspector['inspectPkcs12']>>;
        try {
          inspected = await this.inspector.inspectPkcs12(bytes, { signal: signal.signal });
        } catch (error) {
          assertOwnerGeneration(this.ownerStates, ownerWindowId, generation);
          if (options.signal?.aborted) throw abortError();
          throw new PdfSigningIdentityRegistryError(
            'INSPECTION_FAILED',
            'The selected signing identity could not be inspected.',
          );
        }
        // The native password prompt may settle concurrently with renderer/window
        // cancellation. Never mint a capability after cancellation won the race.
        assertOwnerGeneration(this.ownerStates, ownerWindowId, generation);
        throwIfAborted(signal.signal);
        const certificates = validateCertificateDescriptors(inspected);
        assertOwnerGeneration(this.ownerStates, ownerWindowId, generation);
        const handle = this.createOpaqueHandle();
        if (!HANDLE_PATTERN.test(handle) || this.entries.has(handle)) {
          throw new PdfSigningIdentityRegistryError('INVALID_REQUEST', 'A secure signing identity handle could not be created.');
        }
        const expiresAtMs = this.now() + this.handleTtlMs;
        const entry: RegisteredIdentity = {
          handle,
          ownerWindowId,
          ownerGeneration: generation,
          canonicalPath: opened.canonicalPath,
          fileIdentity: opened.identity,
          sha256: sha256(bytes),
          expiresAtMs,
          certificates,
        };
        this.entries.set(handle, entry);
        return publicSelection(entry);
      } finally {
        bytes?.fill(0);
        await opened.handle.close();
      }
    } finally {
      signal.dispose();
    }
  }

  describe(
    handle: string,
    ownerWindowId: number,
    options: { readonly generation?: number } = {},
  ): PdfSigningIdentitySelection {
    return publicSelection(this.resolveEntry(handle, ownerWindowId, options.generation));
  }

  async withPkcs12Frame<T>(
    handle: string,
    ownerWindowId: number,
    consume: (pkcs12Frame: Uint8Array) => Promise<T>,
    options: { readonly generation?: number; readonly signal?: AbortSignal } = {},
  ): Promise<T> {
    const entry = this.resolveEntry(handle, ownerWindowId, options.generation);
    const ownerState = this.getActiveOwnerState(ownerWindowId, entry.ownerGeneration);
    const signal = combineAbortSignals(ownerState.controller.signal, options.signal);
    try {
      assertOwnerGeneration(this.ownerStates, ownerWindowId, entry.ownerGeneration);
      throwIfAborted(signal.signal);
      const opened = await openStablePkcs12(entry.canonicalPath, this.maxPkcs12Bytes);
      let bytes: Buffer | undefined;
      try {
        assertOwnerGeneration(this.ownerStates, ownerWindowId, entry.ownerGeneration);
        if (!sameIdentity(entry.fileIdentity, opened.identity) || opened.canonicalPath !== entry.canonicalPath) {
          this.entries.delete(handle);
          throw new PdfSigningIdentityRegistryError('IDENTITY_CHANGED', 'The selected signing identity changed.');
        }
        bytes = await readStableHandle(opened.handle, opened.identity, this.maxPkcs12Bytes);
        assertOwnerGeneration(this.ownerStates, ownerWindowId, entry.ownerGeneration);
        throwIfAborted(signal.signal);
        if (sha256(bytes) !== entry.sha256) {
          this.entries.delete(handle);
          throw new PdfSigningIdentityRegistryError('IDENTITY_CHANGED', 'The selected signing identity changed.');
        }
        const result = await consume(bytes);
        assertOwnerGeneration(this.ownerStates, ownerWindowId, entry.ownerGeneration);
        throwIfAborted(signal.signal);
        return result;
      } finally {
        bytes?.fill(0);
        await opened.handle.close();
      }
    } finally {
      signal.dispose();
    }
  }

  revoke(handle: string, ownerWindowId: number): void {
    this.resolveEntry(handle, ownerWindowId);
    this.entries.delete(handle);
  }

  revokeWindow(ownerWindowId: number, generation?: number): void {
    assertWindowId(ownerWindowId);
    const previous = this.ownerStates.get(ownerWindowId);
    if (generation !== undefined
      && (!previous || !previous.active || previous.generation !== generation)) {
      return;
    }
    previous?.controller.abort();
    const controller = new AbortController();
    controller.abort();
    this.ownerStates.set(ownerWindowId, {
      generation: (previous?.generation ?? 0) + 1,
      active: false,
      controller,
    });
    for (const [handle, entry] of this.entries) {
      if (entry.ownerWindowId === ownerWindowId) this.entries.delete(handle);
    }
  }

  purgeExpired(): number {
    const now = this.now();
    let removed = 0;
    for (const [handle, entry] of this.entries) {
      if (entry.expiresAtMs <= now) {
        this.entries.delete(handle);
        removed += 1;
      }
    }
    return removed;
  }

  private resolveEntry(handle: string, ownerWindowId: number, generation?: number): RegisteredIdentity {
    assertWindowId(ownerWindowId);
    if (!HANDLE_PATTERN.test(handle)) {
      throw new PdfSigningIdentityRegistryError('HANDLE_INVALID', 'The signing identity handle is invalid.');
    }
    const entry = this.entries.get(handle);
    if (!entry) throw new PdfSigningIdentityRegistryError('HANDLE_INVALID', 'The signing identity handle is invalid.');
    if (entry.ownerWindowId !== ownerWindowId) {
      throw new PdfSigningIdentityRegistryError(
        'HANDLE_WINDOW_MISMATCH',
        'The signing identity handle belongs to a different window.',
      );
    }
    const ownerState = this.getActiveOwnerState(ownerWindowId, generation);
    if (entry.ownerGeneration !== ownerState.generation) {
      throw new PdfSigningIdentityRegistryError('OWNER_UNAVAILABLE', 'The signing identity owner is unavailable.');
    }
    if (entry.expiresAtMs <= this.now()) {
      this.entries.delete(handle);
      throw new PdfSigningIdentityRegistryError('HANDLE_EXPIRED', 'The signing identity handle expired.');
    }
    return entry;
  }

  private getActiveOwnerState(ownerWindowId: number, generation?: number): OwnerState {
    let state = this.ownerStates.get(ownerWindowId);
    if (!state) {
      state = { generation: 1, active: true, controller: new AbortController() };
      this.ownerStates.set(ownerWindowId, state);
    }
    assertOwnerGeneration(this.ownerStates, ownerWindowId, generation ?? state.generation);
    return state;
  }
}

async function openStablePkcs12(path: string, maxBytes: number): Promise<{
  readonly handle: FileHandle;
  readonly identity: StableIdentity;
  readonly canonicalPath: string;
}> {
  if (typeof path !== 'string' || !isAbsolute(path)) throw unsafeIdentity();
  const normalized = resolve(path);
  const before = await lstat(normalized, { bigint: true }).catch(() => { throw unsafeIdentity(); });
  if (!before.isFile() || before.isSymbolicLink()) throw unsafeIdentity();
  const canonicalPath = await realpath(normalized).catch(() => { throw unsafeIdentity(); });
  const noFollow = constants.O_NOFOLLOW ?? 0;
  const handle = await open(normalized, constants.O_RDONLY | noFollow).catch(() => { throw unsafeIdentity(); });
  try {
    const stats = await handle.stat({ bigint: true });
    if (!stats.isFile() || stats.nlink !== 1n) throw unsafeIdentity();
    if (stats.size < 1n) throw unsafeIdentity();
    if (stats.size > BigInt(maxBytes)) {
      throw new PdfSigningIdentityRegistryError('IDENTITY_FILE_TOO_LARGE', 'The selected signing identity exceeds its size limit.');
    }
    const identity = stableIdentity(stats);
    if (!sameIdentity(identity, stableIdentity(before))) throw unsafeIdentity();
    return { handle, identity, canonicalPath };
  } catch (error) {
    await handle.close();
    throw error;
  }
}

async function readStableHandle(handle: FileHandle, expected: StableIdentity, maxBytes: number): Promise<Buffer> {
  const before = stableIdentity(await handle.stat({ bigint: true }));
  if (!sameIdentity(before, expected) || before.size > BigInt(maxBytes)) throw changedIdentity();
  const bytes = Buffer.alloc(Number(before.size));
  let offset = 0;
  while (offset < bytes.byteLength) {
    const result = await handle.read(bytes, offset, bytes.byteLength - offset, offset);
    if (result.bytesRead === 0) break;
    offset += result.bytesRead;
  }
  const after = stableIdentity(await handle.stat({ bigint: true }));
  if (offset !== bytes.byteLength || !sameIdentity(before, after)) {
    bytes.fill(0);
    throw changedIdentity();
  }
  return bytes;
}

function validateCertificateDescriptors(value: unknown): readonly PdfSigningCertificateDescriptor[] {
  if (!isRecord(value)
    || Object.keys(value).sort().join(',') !== 'engineVersion,identities,passwordRemembered,privateKeyExported,provider'
    || value.provider !== 'pkcs12' || value.passwordRemembered !== false || value.privateKeyExported !== false
    || !boundedText(value.engineVersion, 128)
    || !Array.isArray(value.identities) || value.identities.length < 1 || value.identities.length > 32) {
    throw new PdfSigningIdentityRegistryError('INSPECTION_FAILED', 'The signing identity inspection result is invalid.');
  }
  return Object.freeze(value.identities.map((candidate) => {
    if (!isRecord(candidate)
      || Object.keys(candidate).sort().join(',') !== [
        'certificateSha256', 'chainSha256', 'hasPrivateKey', 'issuer', 'keyAlgorithm',
        'keyBits', 'serialNumber', 'subject', 'supportedDigests', 'validFrom', 'validTo',
      ].sort().join(',')
      || typeof candidate.certificateSha256 !== 'string' || !SHA256_PATTERN.test(candidate.certificateSha256)
      || !boundedText(candidate.subject, 512) || !boundedText(candidate.issuer, 512)
      || !boundedText(candidate.serialNumber, 512)
      || !canonicalInstant(candidate.validFrom) || !canonicalInstant(candidate.validTo)
      || !boundedText(candidate.keyAlgorithm, 64)
      || !Number.isSafeInteger(candidate.keyBits) || (candidate.keyBits as number) < 0 || (candidate.keyBits as number) > 65_536
      || !Array.isArray(candidate.chainSha256) || candidate.chainSha256.length < 1 || candidate.chainSha256.length > 32
      || candidate.chainSha256.some((digest) => typeof digest !== 'string' || !SHA256_PATTERN.test(digest))
      || !Array.isArray(candidate.supportedDigests) || candidate.supportedDigests.length > 3
      || candidate.supportedDigests.some((digest) => digest !== 'SHA-256' && digest !== 'SHA-384' && digest !== 'SHA-512')
      || candidate.hasPrivateKey !== true) {
      throw new PdfSigningIdentityRegistryError('INSPECTION_FAILED', 'The signing identity inspection result is invalid.');
    }
    return Object.freeze({
      sha256Fingerprint: candidate.certificateSha256,
      serialNumber: candidate.serialNumber,
      subjectDisplayName: candidate.subject,
      issuerDisplayName: candidate.issuer,
      notBefore: candidate.validFrom,
      notAfter: candidate.validTo,
      publicKeyAlgorithm: candidate.keyAlgorithm,
      publicKeyBits: candidate.keyBits,
      supportedDigests: Object.freeze([...candidate.supportedDigests]),
      suitableForSigning: candidate.supportedDigests.includes('SHA-256'),
    }) as PdfSigningCertificateDescriptor;
  }));
}

function publicSelection(entry: RegisteredIdentity): PdfSigningIdentitySelection {
  return Object.freeze({
    handle: entry.handle,
    expiresAt: new Date(entry.expiresAtMs).toISOString(),
    certificates: entry.certificates,
  });
}

function stableIdentity(stats: BigIntStats): StableIdentity {
  return {
    dev: stats.dev,
    ino: stats.ino,
    size: stats.size,
    mtimeNs: stats.mtimeNs,
    ctimeNs: stats.ctimeNs,
    nlink: stats.nlink,
  };
}

function sameIdentity(left: StableIdentity, right: StableIdentity): boolean {
  return left.dev === right.dev
    && left.ino === right.ino
    && left.size === right.size
    && left.mtimeNs === right.mtimeNs
    && left.ctimeNs === right.ctimeNs
    && left.nlink === right.nlink;
}

function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

function assertWindowId(value: number): void {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new PdfSigningIdentityRegistryError('INVALID_REQUEST', 'A valid owner window is required.');
  }
}

function boundedPositiveInteger(value: number | undefined, fallback: number, maximum: number): number {
  const resolved = value ?? fallback;
  if (!Number.isSafeInteger(resolved) || resolved < 1 || resolved > maximum) {
    throw new TypeError('Signing identity registry limit is invalid.');
  }
  return resolved;
}

function boundedText(value: unknown, maxLength: number): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= maxLength && !/[\u0000-\u001f\u007f]/u.test(value);
}

function canonicalInstant(value: unknown): value is string {
  return typeof value === 'string'
    && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/.test(value)
    && !Number.isNaN(Date.parse(value));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted) throw abortError();
}

function assertOwnerGeneration(
  ownerStates: ReadonlyMap<number, OwnerState>,
  ownerWindowId: number,
  generation: number,
): void {
  const state = ownerStates.get(ownerWindowId);
  if (!state || !state.active || state.generation !== generation) {
    throw new PdfSigningIdentityRegistryError('OWNER_UNAVAILABLE', 'The signing identity owner is unavailable.');
  }
}

function combineAbortSignals(
  ...signals: readonly (AbortSignal | undefined)[]
): { readonly signal: AbortSignal; readonly dispose: () => void } {
  const controller = new AbortController();
  const listeners: Array<{ readonly signal: AbortSignal; readonly listener: () => void }> = [];
  const abort = () => controller.abort();
  for (const signal of signals) {
    if (!signal) continue;
    if (signal.aborted) {
      controller.abort();
      break;
    }
    signal.addEventListener('abort', abort, { once: true });
    listeners.push({ signal, listener: abort });
  }
  return {
    signal: controller.signal,
    dispose: () => {
      for (const listener of listeners) {
        listener.signal.removeEventListener('abort', listener.listener);
      }
    },
  };
}

function abortError(): Error {
  return new DOMException('The signing identity selection was cancelled.', 'AbortError');
}

function unsafeIdentity(): PdfSigningIdentityRegistryError {
  return new PdfSigningIdentityRegistryError('UNSAFE_IDENTITY_FILE', 'The selected signing identity file is unsafe.');
}

function changedIdentity(): PdfSigningIdentityRegistryError {
  return new PdfSigningIdentityRegistryError('IDENTITY_CHANGED', 'The selected signing identity changed.');
}
