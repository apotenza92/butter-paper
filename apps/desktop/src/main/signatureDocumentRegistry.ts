import { createHash, randomBytes } from 'node:crypto';
import { constants, type BigIntStats } from 'node:fs';
import { lstat, open, realpath, type FileHandle } from 'node:fs/promises';
import { isAbsolute, resolve } from 'node:path';
import {
  isPdfSignatureValidationReport,
  type PdfSignatureValidationReport,
  type SignaturePresence,
} from '@butter-paper/core';

const DEFAULT_MAX_ENTRIES = 64;
const DEFAULT_MAX_INPUT_BYTES = 512 * 1024 * 1024;
const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const POLICY_ID_PATTERN = /^[A-Za-z0-9._:-]{1,128}$/;
const HANDLE_PATTERN = /^sigdoc_[A-Za-z0-9_-]{32,128}$/;

export type SignatureDocumentRegistryErrorCode =
  | 'INVALID_REQUEST'
  | 'UNSAFE_SOURCE'
  | 'VALIDATION_MISMATCH'
  | 'TRUST_SNAPSHOT_MISMATCH'
  | 'STALE_DOCUMENT'
  | 'NOT_FOUND'
  | 'LIMIT_EXCEEDED';

export class SignatureDocumentRegistryError extends Error {
  constructor(readonly code: SignatureDocumentRegistryErrorCode, message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'SignatureDocumentRegistryError';
  }
}

export interface SignatureDocumentTrustSnapshot {
  readonly policyId: string;
  readonly policyVersion: number;
  readonly configurationSha256: string;
}

export interface RegisterSignatureDocumentRequest {
  /** Privileged main-process source path; never include this value in a renderer result. */
  readonly sourcePath: string;
  readonly validationReport: PdfSignatureValidationReport;
  readonly trust: SignatureDocumentTrustSnapshot;
  readonly capabilities: SignatureDocumentCapabilities;
}

export interface SignatureDocumentCapabilities {
  readonly createUnsignedCopy: boolean;
  readonly certificateSign: boolean;
  readonly certify: boolean;
  readonly signatureIncrementalWrite: boolean;
  readonly signedIncrementalEdit: boolean;
}

/** Safe to cross preload: it deliberately contains no source path or validation certificate data. */
export interface SignatureDocumentDescriptor {
  readonly handle: string;
  readonly inputSha256: string;
  readonly byteLength: number;
  readonly signaturePresence: SignaturePresence;
  readonly trust: SignatureDocumentTrustSnapshot;
  readonly registeredAt: string;
  readonly capabilities: SignatureDocumentCapabilities;
}

/** Privileged resolution result. This type must never be exposed by preload. */
export interface ResolvedSignatureDocument {
  readonly sourcePath: string;
  readonly inputSha256: string;
  readonly byteLength: number;
  readonly validationReport: PdfSignatureValidationReport;
  readonly trust: SignatureDocumentTrustSnapshot;
}

interface StableFileIdentity {
  readonly dev: bigint;
  readonly ino: bigint;
  readonly size: bigint;
  readonly mtimeMs: bigint;
  readonly ctimeMs: bigint;
}

interface RegistryEntry {
  readonly descriptor: SignatureDocumentDescriptor;
  readonly sourcePath: string;
  readonly validationReport: PdfSignatureValidationReport;
  readonly identity: StableFileIdentity;
}

export interface SignatureDocumentRegistryOptions {
  readonly maxEntries?: number;
  readonly maxInputBytes?: number;
  readonly now?: () => Date;
  readonly createHandle?: () => string;
}

/**
 * Main-owned capability registry for signature-sensitive operations. Renderer
 * callers receive only random handles; every privileged resolution reopens and
 * re-hashes the source before returning its canonical path.
 */
export class SignatureDocumentRegistry {
  private readonly entries = new Map<string, RegistryEntry>();
  private operationQueue: Promise<void> = Promise.resolve();
  private readonly maxEntries: number;
  private readonly maxInputBytes: number;
  private readonly now: () => Date;
  private readonly createHandle: () => string;

  constructor(options: SignatureDocumentRegistryOptions = {}) {
    this.maxEntries = boundedPositiveInteger(options.maxEntries, DEFAULT_MAX_ENTRIES);
    this.maxInputBytes = boundedPositiveInteger(options.maxInputBytes, DEFAULT_MAX_INPUT_BYTES);
    this.now = options.now ?? (() => new Date());
    this.createHandle = options.createHandle ?? (() => `sigdoc_${randomBytes(32).toString('base64url')}`);
  }

  async register(request: RegisterSignatureDocumentRequest): Promise<SignatureDocumentDescriptor> {
    return this.exclusive(() => this.registerUnlocked(request));
  }

  private async registerUnlocked(request: RegisterSignatureDocumentRequest): Promise<SignatureDocumentDescriptor> {
    assertRegisterRequest(request);
    if (this.entries.size >= this.maxEntries) {
      throw new SignatureDocumentRegistryError(
        'LIMIT_EXCEEDED',
        'The signature document registry has reached its active-document limit.',
      );
    }

    const sourcePath = await canonicalRegularPath(request.sourcePath);
    const snapshot = await snapshotSource(sourcePath, this.maxInputBytes);
    if (snapshot.sha256 !== request.validationReport.inputSha256) {
      throw new SignatureDocumentRegistryError(
        'VALIDATION_MISMATCH',
        'The signature validation report does not describe the current source bytes.',
      );
    }

    const handle = this.createHandle();
    if (!HANDLE_PATTERN.test(handle) || this.entries.has(handle)) {
      throw new SignatureDocumentRegistryError('INVALID_REQUEST', 'A unique opaque document handle could not be created.');
    }
    const trust = cloneTrustSnapshot(request.trust);
    const validationReport = structuredClone(request.validationReport);
    const descriptor: SignatureDocumentDescriptor = {
      handle,
      inputSha256: snapshot.sha256,
      byteLength: Number(snapshot.identity.size),
      signaturePresence: validationReport.inventory.presence,
      trust,
      registeredAt: this.now().toISOString(),
      capabilities: cloneCapabilities(request.capabilities),
    };
    this.entries.set(handle, {
      descriptor,
      sourcePath,
      validationReport,
      identity: snapshot.identity,
    });
    return cloneDescriptor(descriptor);
  }

  describe(handle: string): SignatureDocumentDescriptor {
    const entry = this.requireEntry(handle);
    return cloneDescriptor(entry.descriptor);
  }

  list(): readonly SignatureDocumentDescriptor[] {
    return [...this.entries.values()]
      .map((entry) => cloneDescriptor(entry.descriptor))
      .sort((left, right) => left.registeredAt.localeCompare(right.registeredAt));
  }

  async resolve(
    handle: string,
    currentTrustConfigurationSha256: string,
  ): Promise<ResolvedSignatureDocument> {
    return this.exclusive(() => this.resolveUnlocked(handle, currentTrustConfigurationSha256));
  }

  private async resolveUnlocked(
    handle: string,
    currentTrustConfigurationSha256: string,
  ): Promise<ResolvedSignatureDocument> {
    const entry = this.requireEntry(handle);
    assertSha256(currentTrustConfigurationSha256, 'A current trust-configuration SHA-256 is required.');
    if (currentTrustConfigurationSha256 !== entry.descriptor.trust.configurationSha256) {
      this.entries.delete(handle);
      throw new SignatureDocumentRegistryError(
        'TRUST_SNAPSHOT_MISMATCH',
        'The local trust configuration changed after this document was validated.',
      );
    }

    try {
      const canonicalPath = await canonicalRegularPath(entry.sourcePath);
      const snapshot = await snapshotSource(canonicalPath, this.maxInputBytes);
      if (canonicalPath !== entry.sourcePath
        || !sameIdentity(entry.identity, snapshot.identity)
        || snapshot.sha256 !== entry.descriptor.inputSha256
        || snapshot.sha256 !== entry.validationReport.inputSha256) {
        throw staleDocument();
      }
    } catch (error) {
      this.entries.delete(handle);
      if (error instanceof SignatureDocumentRegistryError && error.code === 'STALE_DOCUMENT') throw error;
      throw staleDocument(error);
    }

    return {
      sourcePath: entry.sourcePath,
      inputSha256: entry.descriptor.inputSha256,
      byteLength: entry.descriptor.byteLength,
      validationReport: structuredClone(entry.validationReport),
      trust: cloneTrustSnapshot(entry.descriptor.trust),
    };
  }

  release(handle: string): boolean {
    assertHandle(handle);
    return this.entries.delete(handle);
  }

  clear(): void {
    this.entries.clear();
  }

  get size(): number {
    return this.entries.size;
  }

  private exclusive<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.operationQueue.then(operation, operation);
    this.operationQueue = result.then(() => undefined, () => undefined);
    return result;
  }

  private requireEntry(handle: string): RegistryEntry {
    assertHandle(handle);
    const entry = this.entries.get(handle);
    if (!entry) throw new SignatureDocumentRegistryError('NOT_FOUND', 'The opaque signature document handle is unknown or expired.');
    return entry;
  }
}

async function canonicalRegularPath(sourcePath: string): Promise<string> {
  if (typeof sourcePath !== 'string' || !isAbsolute(sourcePath)) {
    throw new SignatureDocumentRegistryError('INVALID_REQUEST', 'A main-owned absolute source path is required.');
  }
  const normalized = resolve(sourcePath);
  try {
    const linkInfo = await lstat(normalized);
    if (linkInfo.isSymbolicLink() || !linkInfo.isFile()) throw unsafeSource();
    const canonical = await realpath(normalized);
    const canonicalInfo = await lstat(canonical);
    if (canonicalInfo.isSymbolicLink() || !canonicalInfo.isFile()) throw unsafeSource();
    return canonical;
  } catch (error) {
    if (error instanceof SignatureDocumentRegistryError) throw error;
    throw unsafeSource(error);
  }
}

async function snapshotSource(
  sourcePath: string,
  maxInputBytes: number,
): Promise<{ readonly sha256: string; readonly identity: StableFileIdentity }> {
  let file: FileHandle;
  try {
    file = await open(sourcePath, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  } catch (error) {
    throw unsafeSource(error);
  }
  try {
    const before = await file.stat({ bigint: true });
    if (!before.isFile() || before.size < 1n || before.size > BigInt(maxInputBytes)) throw unsafeSource();
    const hash = createHash('sha256');
    let position = 0;
    for (;;) {
      const bytes = Buffer.allocUnsafe(256 * 1024);
      const result = await file.read(bytes, 0, bytes.byteLength, position);
      if (result.bytesRead === 0) break;
      position += result.bytesRead;
      hash.update(bytes.subarray(0, result.bytesRead));
      if (position > maxInputBytes) throw unsafeSource();
    }
    const after = await file.stat({ bigint: true });
    if (!sameFileStats(before, after) || BigInt(position) !== before.size) throw staleDocument();
    return {
      sha256: hash.digest('hex'),
      identity: stableIdentity(before),
    };
  } finally {
    await file.close();
  }
}

function assertRegisterRequest(request: RegisterSignatureDocumentRequest): void {
  if (!request || typeof request !== 'object'
    || !isPdfSignatureValidationReport(request.validationReport)
    || !request.capabilities
    || !isSignatureDocumentCapabilities(request.capabilities)) {
    throw new SignatureDocumentRegistryError('INVALID_REQUEST', 'A valid signature validation snapshot is required.');
  }
  assertTrustSnapshot(request.trust);
}

function isSignatureDocumentCapabilities(value: unknown): value is SignatureDocumentCapabilities {
  if (!value || typeof value !== 'object') return false;
  const record = value as Record<string, unknown>;
  const keys = [
    'certificateSign',
    'certify',
    'createUnsignedCopy',
    'signatureIncrementalWrite',
    'signedIncrementalEdit',
  ];
  return Object.keys(record).sort().join(',') === keys.sort().join(',')
    && keys.every((key) => typeof record[key] === 'boolean');
}

function cloneCapabilities(capabilities: SignatureDocumentCapabilities): SignatureDocumentCapabilities {
  return {
    createUnsignedCopy: capabilities.createUnsignedCopy === true,
    certificateSign: capabilities.certificateSign === true,
    certify: capabilities.certify === true,
    signatureIncrementalWrite: capabilities.signatureIncrementalWrite === true,
    signedIncrementalEdit: capabilities.signedIncrementalEdit === true,
  };
}

function assertTrustSnapshot(trust: SignatureDocumentTrustSnapshot): void {
  if (!trust || typeof trust !== 'object'
    || typeof trust.policyId !== 'string' || !POLICY_ID_PATTERN.test(trust.policyId)
    || !Number.isSafeInteger(trust.policyVersion) || trust.policyVersion < 1
    || typeof trust.configurationSha256 !== 'string' || !SHA256_PATTERN.test(trust.configurationSha256)) {
    throw new SignatureDocumentRegistryError('INVALID_REQUEST', 'A valid offline trust-policy snapshot is required.');
  }
}

function cloneTrustSnapshot(trust: SignatureDocumentTrustSnapshot): SignatureDocumentTrustSnapshot {
  return {
    policyId: trust.policyId,
    policyVersion: trust.policyVersion,
    configurationSha256: trust.configurationSha256,
  };
}

function cloneDescriptor(descriptor: SignatureDocumentDescriptor): SignatureDocumentDescriptor {
  return {
    handle: descriptor.handle,
    inputSha256: descriptor.inputSha256,
    byteLength: descriptor.byteLength,
    signaturePresence: descriptor.signaturePresence,
    trust: cloneTrustSnapshot(descriptor.trust),
    capabilities: cloneCapabilities(descriptor.capabilities),
    registeredAt: descriptor.registeredAt,
  };
}

function stableIdentity(info: BigIntStats): StableFileIdentity {
  return {
    dev: info.dev,
    ino: info.ino,
    size: info.size,
    mtimeMs: info.mtimeMs,
    ctimeMs: info.ctimeMs,
  };
}

function sameIdentity(left: StableFileIdentity, right: StableFileIdentity): boolean {
  return left.dev === right.dev
    && left.ino === right.ino
    && left.size === right.size
    && left.mtimeMs === right.mtimeMs
    && left.ctimeMs === right.ctimeMs;
}

function sameFileStats(
  before: BigIntStats,
  after: BigIntStats,
): boolean {
  return before.dev === after.dev
    && before.ino === after.ino
    && before.size === after.size
    && before.mtimeMs === after.mtimeMs
    && before.ctimeMs === after.ctimeMs;
}

function assertHandle(handle: string): void {
  if (typeof handle !== 'string' || !HANDLE_PATTERN.test(handle)) {
    throw new SignatureDocumentRegistryError('INVALID_REQUEST', 'A valid opaque signature document handle is required.');
  }
}

function assertSha256(value: string, message: string): void {
  if (typeof value !== 'string' || !SHA256_PATTERN.test(value)) {
    throw new SignatureDocumentRegistryError('INVALID_REQUEST', message);
  }
}

function boundedPositiveInteger(value: number | undefined, fallback: number): number {
  if (value === undefined) return fallback;
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new TypeError('Signature document registry limits must be positive safe integers.');
  }
  return value;
}

function unsafeSource(cause?: unknown): SignatureDocumentRegistryError {
  return new SignatureDocumentRegistryError(
    'UNSAFE_SOURCE',
    'The signature document source is not a stable regular non-symlink file.',
    cause === undefined ? undefined : { cause },
  );
}

function staleDocument(cause?: unknown): SignatureDocumentRegistryError {
  return new SignatureDocumentRegistryError(
    'STALE_DOCUMENT',
    'The signature document changed after validation; the handle was invalidated.',
    cause === undefined ? undefined : { cause },
  );
}
