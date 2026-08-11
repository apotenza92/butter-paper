import { createHash, randomUUID } from 'node:crypto';
import { mkdir, open, readdir, rename, rm } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';
import {
  createSignatureAppearanceAsset,
  type SignatureAppearanceAsset,
} from '@butter-paper/core';
import type { RecentSignature, RecentSignaturesSnapshot } from '../shared/protocol';

export const RECENT_SIGNATURES_FILE_NAME = 'recent-signatures.v1.enc';

const SCHEMA_VERSION = 2;
const MAX_RECENT_SIGNATURES = 5;
const MAX_SIGNATURE_DIMENSION = 4096;
const MAX_SIGNATURE_PIXELS = 16 * 1024 * 1024;
const MAX_PLAINTEXT_BYTES = 12 * 1024 * 1024;
const MAX_ENCRYPTED_BYTES = MAX_PLAINTEXT_BYTES + 1024 * 1024;
const MAX_REMEMBERED_DATA_URL_LENGTH = 2 * 1024 * 1024;
const MAX_PENDING_OPERATIONS = 3;
const SIGNATURE_ID_PATTERN = /^[A-Za-z0-9_-]{43}$/;

interface SecureStorage {
  isEncryptionAvailable(): boolean;
  isAsyncEncryptionAvailable(): Promise<boolean>;
  encryptString(value: string): Buffer;
  encryptStringAsync(value: string): Promise<Buffer>;
  decryptString(value: Buffer): string;
  decryptStringAsync(value: Buffer): Promise<{ result: string; shouldReEncrypt: boolean }>;
  getSelectedStorageBackend(): string;
}

interface PersistedRecentSignatures {
  schemaVersion: 2;
  signatures: RecentSignature[];
}

interface ParsedRecentSignatures {
  schemaVersion: 1 | 2;
  signatures: RecentSignature[];
}

interface RecentSignatureStoreOptions {
  statePath: string;
  secureStorage: SecureStorage;
  now?: () => number;
  platform?: NodeJS.Platform;
  sanitizeAsset?: (asset: SignatureAppearanceAsset) => Promise<SignatureAppearanceAsset>;
  temporaryToken?: () => string;
}

export class RecentSignatureStore {
  readonly #statePath: string;
  readonly #secureStorage: SecureStorage;
  readonly #now: () => number;
  readonly #platform: NodeJS.Platform;
  readonly #sanitizeAsset: (asset: SignatureAppearanceAsset) => Promise<SignatureAppearanceAsset>;
  readonly #temporaryToken: () => string;
  #operation = Promise.resolve();
  #pendingOperations = 0;

  constructor({
    statePath,
    secureStorage,
    now = Date.now,
    platform = process.platform,
    sanitizeAsset = async (asset) => asset,
    temporaryToken = randomUUID,
  }: RecentSignatureStoreOptions) {
    this.#statePath = statePath;
    this.#secureStorage = secureStorage;
    this.#now = now;
    this.#platform = platform;
    this.#sanitizeAsset = sanitizeAsset;
    this.#temporaryToken = temporaryToken;
  }

  async list(): Promise<RecentSignaturesSnapshot> {
    return await this.#runExclusive(async () => {
      const available = await this.#isAvailable();
      if (!available) return this.#snapshot([], false);
      await this.#removeTemporaryFilesUnlocked();
      return this.#snapshot(await this.#readUnlocked(), true);
    });
  }

  async remember(value: unknown): Promise<RecentSignaturesSnapshot> {
    return await this.#runExclusive(async () => {
      await this.#assertAvailable();
      const candidate = parseSignatureAppearanceAsset(value);
      const existing = await this.#readUnlocked();
      const candidateId = signatureId(candidate);
      const storedCandidate = existing.find((entry) => entry.id === candidateId)?.asset;
      const asset = storedCandidate ?? parseSignatureAppearanceAsset(await this.#sanitizeAsset(candidate));
      const id = signatureId(asset);
      const signatures = [{ id, lastUsedAt: this.#now(), asset }, ...existing.filter((entry) => entry.id !== id)]
        .slice(0, MAX_RECENT_SIGNATURES);
      await this.#writeUnlocked(signatures);
      return this.#snapshot(signatures, true);
    });
  }

  async remove(value: unknown): Promise<RecentSignaturesSnapshot> {
    return await this.#runExclusive(async () => {
      await this.#assertAvailable();
      const id = parseSignatureId(value);
      const signatures = (await this.#readUnlocked()).filter((entry) => entry.id !== id);
      await this.#writeUnlocked(signatures);
      return this.#snapshot(signatures, true);
    });
  }

  async clear(): Promise<RecentSignaturesSnapshot> {
    return await this.#runExclusive(async () => {
      await rm(this.#statePath, { force: true });
      await this.#removeTemporaryFilesUnlocked();
      return this.#snapshot([], await this.#isAvailable());
    });
  }

  async #runExclusive<T>(operation: () => Promise<T>): Promise<T> {
    if (this.#pendingOperations >= MAX_PENDING_OPERATIONS) {
      throw new Error('Too many recent signature storage operations are pending.');
    }
    this.#pendingOperations += 1;
    const previous = this.#operation;
    let release: () => void = () => undefined;
    this.#operation = new Promise<void>((resolve) => { release = resolve; });
    await previous;
    try {
      return await operation();
    } finally {
      this.#pendingOperations -= 1;
      release();
    }
  }

  async #assertAvailable(): Promise<void> {
    if (!await this.#isAvailable()) {
      throw new Error('Secure signature storage is not available on this device.');
    }
  }

  async #readUnlocked(): Promise<RecentSignature[]> {
    let handle;
    try {
      handle = await open(this.#statePath, 'r');
      const metadata = await handle.stat();
      if (!metadata.isFile() || metadata.size > MAX_ENCRYPTED_BYTES) {
        throw new Error('Encrypted recent signature state is invalid.');
      }
      const bounded = Buffer.allocUnsafe(MAX_ENCRYPTED_BYTES + 1);
      let totalBytesRead = 0;
      while (totalBytesRead < bounded.length) {
        const { bytesRead } = await handle.read(
          bounded,
          totalBytesRead,
          bounded.length - totalBytesRead,
          totalBytesRead,
        );
        if (bytesRead === 0) break;
        totalBytesRead += bytesRead;
      }
      if (totalBytesRead > MAX_ENCRYPTED_BYTES || totalBytesRead !== metadata.size) {
        throw new Error('Encrypted recent signature state changed or is too large.');
      }
      const encrypted = bounded.subarray(0, totalBytesRead);
      const decrypted = await this.#decryptString(encrypted);
      const plaintext = decrypted.result;
      if (Buffer.byteLength(plaintext, 'utf8') > MAX_PLAINTEXT_BYTES) {
        throw new Error('Decrypted recent signature state is too large.');
      }
      const parsed = parsePersistedRecentSignatures(JSON.parse(plaintext));
      const signatures = parsed.schemaVersion === 1
        ? await this.#sanitizePersistedSignatures(parsed.signatures)
        : parsed.signatures;
      if (parsed.schemaVersion === 1 || decrypted.shouldReEncrypt) await this.#writeUnlocked(signatures);
      return signatures;
    } catch (error) {
      if (isFileNotFoundError(error)) return [];
      throw new Error('Recent signatures could not be read securely.', { cause: error });
    } finally {
      await handle?.close().catch(() => undefined);
    }
  }

  async #writeUnlocked(signatures: RecentSignature[]): Promise<void> {
    const plaintext = JSON.stringify({ schemaVersion: SCHEMA_VERSION, signatures } satisfies PersistedRecentSignatures);
    if (Buffer.byteLength(plaintext, 'utf8') > MAX_PLAINTEXT_BYTES) {
      throw new Error('This signature is too large to remember securely.');
    }
    const encrypted = await this.#encryptString(plaintext);
    await mkdir(dirname(this.#statePath), { recursive: true, mode: 0o700 });
    const temporaryPath = `${this.#statePath}.${this.#temporaryToken()}.tmp`;
    let temporaryHandle;
    try {
      temporaryHandle = await open(temporaryPath, 'wx', 0o600);
      if (!(await temporaryHandle.stat()).isFile()) throw new Error('Recent signature temporary state is not a regular file.');
      await temporaryHandle.writeFile(encrypted);
      await temporaryHandle.sync();
      await temporaryHandle.close();
      temporaryHandle = undefined;
      await rename(temporaryPath, this.#statePath);
    } finally {
      await temporaryHandle?.close().catch(() => undefined);
      await rm(temporaryPath, { force: true });
    }
  }

  #snapshot(signatures: RecentSignature[], available: boolean): RecentSignaturesSnapshot {
    return {
      available,
      signatures,
    };
  }

  async #isAvailable(): Promise<boolean> {
    if (this.#platform === 'linux') {
      return this.#secureStorage.isEncryptionAvailable()
        && this.#secureStorage.getSelectedStorageBackend() !== 'basic_text';
    }
    return await this.#secureStorage.isAsyncEncryptionAvailable();
  }

  async #encryptString(value: string): Promise<Buffer> {
    return this.#platform === 'linux'
      ? this.#secureStorage.encryptString(value)
      : await this.#secureStorage.encryptStringAsync(value);
  }

  async #decryptString(value: Buffer): Promise<{ result: string; shouldReEncrypt: boolean }> {
    return this.#platform === 'linux'
      ? { result: this.#secureStorage.decryptString(value), shouldReEncrypt: false }
      : await this.#secureStorage.decryptStringAsync(value);
  }

  async #removeTemporaryFilesUnlocked(): Promise<void> {
    const directory = dirname(this.#statePath);
    const prefix = `${basename(this.#statePath)}.`;
    let names: string[];
    try {
      names = await readdir(directory);
    } catch (error) {
      if (isFileNotFoundError(error)) return;
      throw error;
    }
    await Promise.all(names
      .filter((name) => name.startsWith(prefix) && name.endsWith('.tmp'))
      .map((name) => rm(join(directory, name), { force: true })));
  }

  async #sanitizePersistedSignatures(signatures: RecentSignature[]): Promise<RecentSignature[]> {
    const sanitized: RecentSignature[] = [];
    const ids = new Set<string>();
    for (const entry of signatures) {
      const asset = parseSignatureAppearanceAsset(await this.#sanitizeAsset(entry.asset));
      const id = signatureId(asset);
      if (ids.has(id)) continue;
      ids.add(id);
      sanitized.push({ id, lastUsedAt: entry.lastUsedAt, asset });
    }
    return sanitized;
  }
}

function parsePersistedRecentSignatures(value: unknown): ParsedRecentSignatures {
  if (!isRecord(value) || (value.schemaVersion !== 1 && value.schemaVersion !== SCHEMA_VERSION) || !Array.isArray(value.signatures)
    || value.signatures.length > MAX_RECENT_SIGNATURES) {
    throw new TypeError('Encrypted recent signature state has an invalid schema.');
  }
  const signatures: RecentSignature[] = [];
  const ids = new Set<string>();
  for (const candidate of value.signatures.slice(0, MAX_RECENT_SIGNATURES)) {
    if (!isRecord(candidate) || !SIGNATURE_ID_PATTERN.test(String(candidate.id))) {
      throw new TypeError('Encrypted recent signature entry is invalid.');
    }
    if (typeof candidate.lastUsedAt !== 'number' || !Number.isSafeInteger(candidate.lastUsedAt) || candidate.lastUsedAt < 0) {
      throw new TypeError('Encrypted recent signature timestamp is invalid.');
    }
    const asset = parseSignatureAppearanceAsset(candidate.asset);
    const id = signatureId(asset);
    if (id !== candidate.id || ids.has(id)) throw new TypeError('Encrypted recent signature identifier is invalid.');
    ids.add(id);
    signatures.push({ id, lastUsedAt: candidate.lastUsedAt, asset });
  }
  return { schemaVersion: value.schemaVersion, signatures };
}

function parseSignatureAppearanceAsset(value: unknown): SignatureAppearanceAsset {
  if (!isRecord(value)) throw new TypeError('Recent signature must be an image asset.');
  if (typeof value.dataUrl !== 'string' || value.dataUrl.length > MAX_REMEMBERED_DATA_URL_LENGTH) {
    throw new TypeError('This signature is too large to remember securely.');
  }
  const asset = createSignatureAppearanceAsset({
    dataUrl: value.dataUrl as string,
    mimeType: value.mimeType as SignatureAppearanceAsset['mimeType'],
    width: value.width as number,
    height: value.height as number,
    source: value.source as SignatureAppearanceAsset['source'],
  });
  if (asset.width > MAX_SIGNATURE_DIMENSION || asset.height > MAX_SIGNATURE_DIMENSION
    || asset.width * asset.height > MAX_SIGNATURE_PIXELS) {
    throw new TypeError('Recent signature dimensions are too large.');
  }
  return asset;
}

function parseSignatureId(value: unknown): string {
  if (typeof value !== 'string' || !SIGNATURE_ID_PATTERN.test(value)) {
    throw new TypeError('Recent signature identifier is invalid.');
  }
  return value;
}

function signatureId(asset: SignatureAppearanceAsset): string {
  return createHash('sha256')
    .update(asset.dataUrl)
    .update('\0')
    .update(String(asset.width))
    .update('\0')
    .update(String(asset.height))
    .digest('base64url');
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isFileNotFoundError(value: unknown): boolean {
  return value instanceof Error && 'code' in value && value.code === 'ENOENT';
}
