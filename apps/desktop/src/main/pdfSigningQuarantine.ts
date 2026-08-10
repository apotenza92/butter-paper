import { createHash, randomUUID } from 'node:crypto';
import { constants } from 'node:fs';
import { chmod, lstat, mkdir, open, readdir, realpath, rm } from 'node:fs/promises';
import { isAbsolute, join, resolve } from 'node:path';

const ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const RECORD_PATTERN = /^quarantine-([0-9a-f-]{36})\.json$/;
const PDF_PATTERN = /^quarantine-([0-9a-f-]{36})\.pdf$/;
const CREATE_TOMBSTONE_PATTERN = /^\.creating-quarantine-([0-9a-f-]{36})\.json$/;
const DELETE_TOMBSTONE_PATTERN = /^\.deleting-quarantine-([0-9a-f-]{36})\.json$/;
const DEFAULT_MAX_ENTRIES = 8;
const DEFAULT_MAX_ENTRY_BYTES = 64 * 1024 * 1024;
const DEFAULT_MAX_TOTAL_BYTES = 256 * 1024 * 1024;

export type PdfSigningQuarantineReason =
  | 'ENGINE_FAILURE'
  | 'PREFIX_MISMATCH'
  | 'POSTVALIDATION_FAILURE'
  | 'PUBLICATION_FAILURE';

export interface PdfSigningQuarantineDescriptor {
  readonly id: string;
  readonly reason: PdfSigningQuarantineReason;
  readonly byteLength: number;
  readonly sha256: string;
  readonly createdAt: string;
}

export interface PdfSigningQuarantineEntry extends PdfSigningQuarantineDescriptor {
  /** Fresh main-process copy. No storage path is exposed. */
  readonly bytes: Uint8Array;
}

export interface PdfSigningQuarantineOptions {
  readonly now?: () => Date;
  readonly createOpaqueId?: () => string;
  readonly maxEntries?: number;
  readonly maxEntryBytes?: number;
  readonly maxTotalBytes?: number;
  readonly platform?: NodeJS.Platform;
  /** Mandatory Windows ACL application seam. */
  readonly secureRoot?: (rootPath: string) => Promise<void>;
  /** Mandatory Windows ACL verification seam. */
  readonly verifyRoot?: (rootPath: string) => Promise<void>;
}

export class PdfSigningQuarantineError extends Error {
  constructor(readonly code: 'INVALID_INPUT' | 'LIMIT_EXCEEDED' | 'UNSAFE_STORAGE' | 'CORRUPT_STORAGE' | 'NOT_FOUND', message: string) {
    super(message);
    this.name = 'PdfSigningQuarantineError';
  }
}

interface StoredRecord extends PdfSigningQuarantineDescriptor {
  readonly fileName: string;
}

/**
 * Bounded private retention for failed PDF signing outputs. The API accepts
 * only PDF-like output bytes and a fixed reason; it has no credential or
 * identity-file fields and returns opaque IDs rather than filesystem paths.
 */
export class PdfSigningQuarantine {
  private queue: Promise<void> = Promise.resolve();
  private readonly root: string;
  private readonly now: () => Date;
  private readonly createOpaqueId: () => string;
  private readonly maxEntries: number;
  private readonly maxEntryBytes: number;
  private readonly maxTotalBytes: number;
  private readonly platform: NodeJS.Platform;
  private readonly secureRoot?: (rootPath: string) => Promise<void>;
  private readonly verifyRoot?: (rootPath: string) => Promise<void>;

  constructor(root: string, options: PdfSigningQuarantineOptions = {}) {
    if (!isAbsolute(root)) throw new TypeError('PDF signing quarantine requires an absolute main-process path.');
    this.root = resolve(root);
    this.now = options.now ?? (() => new Date());
    this.createOpaqueId = options.createOpaqueId ?? randomUUID;
    this.maxEntries = boundedLimit(options.maxEntries, DEFAULT_MAX_ENTRIES, DEFAULT_MAX_ENTRIES);
    this.maxEntryBytes = boundedLimit(options.maxEntryBytes, DEFAULT_MAX_ENTRY_BYTES, DEFAULT_MAX_ENTRY_BYTES);
    this.maxTotalBytes = boundedLimit(options.maxTotalBytes, DEFAULT_MAX_TOTAL_BYTES, DEFAULT_MAX_TOTAL_BYTES);
    this.platform = options.platform ?? process.platform;
    this.secureRoot = options.secureRoot;
    this.verifyRoot = options.verifyRoot;
    if (this.platform === 'win32' && (!this.secureRoot || !this.verifyRoot)) {
      throw new TypeError('Windows PDF signing quarantine requires fail-closed ACL hooks.');
    }
  }

  retain(bytes: Uint8Array, reason: PdfSigningQuarantineReason): Promise<PdfSigningQuarantineDescriptor> {
    return this.exclusive(async () => {
      assertReason(reason);
      if (!(bytes instanceof Uint8Array) || bytes.byteLength < 8 || bytes.byteLength > this.maxEntryBytes) {
        throw new PdfSigningQuarantineError('INVALID_INPUT', 'The failed PDF output is invalid.');
      }
      const copy = Buffer.from(bytes);
      try {
        if (!copy.subarray(0, 5).equals(Buffer.from('%PDF-', 'ascii'))) {
          throw new PdfSigningQuarantineError('INVALID_INPUT', 'The failed output is not a PDF candidate.');
        }
        const records = await this.loadAll();
        if (records.length >= this.maxEntries
          || records.reduce((sum, record) => sum + record.byteLength, 0) + copy.byteLength > this.maxTotalBytes) {
          throw new PdfSigningQuarantineError('LIMIT_EXCEEDED', 'The private PDF signing quarantine is full.');
        }
        const id = this.createOpaqueId();
        if (!ID_PATTERN.test(id) || records.some((record) => record.id === id)) {
          throw new PdfSigningQuarantineError('UNSAFE_STORAGE', 'A secure quarantine identifier could not be created.');
        }
        const fileName = `quarantine-${id}.pdf`;
        const record: StoredRecord = {
          id,
          reason,
          byteLength: copy.byteLength,
          sha256: createHash('sha256').update(copy).digest('hex'),
          createdAt: this.now().toISOString(),
          fileName,
        };
        const assetPath = join(this.root, fileName);
        const recordPath = join(this.root, `quarantine-${id}.json`);
        const tombstonePath = join(this.root, `.creating-quarantine-${id}.json`);
        await writePrivateJsonExclusive(tombstonePath, { schemaVersion: 1 });
        await this.verifyStorage();
        const asset = await open(assetPath, constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY, 0o600);
        try {
          await asset.writeFile(copy);
          await asset.sync();
        } finally {
          await asset.close();
        }
        await chmod(assetPath, 0o600);
        try {
          await writePrivateJsonExclusive(recordPath, record);
        } catch (error) {
          throw error;
        }
        await this.verifyStorage();
        await rm(tombstonePath);
        return publicDescriptor(record);
      } finally {
        copy.fill(0);
      }
    });
  }

  list(): Promise<readonly PdfSigningQuarantineDescriptor[]> {
    return this.exclusive(async () => (await this.loadAll()).map(publicDescriptor));
  }

  read(id: string): Promise<PdfSigningQuarantineEntry> {
    return this.exclusive(async () => {
      assertId(id);
      const record = (await this.loadAll()).find((candidate) => candidate.id === id);
      if (!record) throw notFound();
      const handle = await openNoFollow(join(this.root, record.fileName));
      try {
        const before = await handle.stat({ bigint: true });
        if (!before.isFile() || before.nlink !== 1n || before.size !== BigInt(record.byteLength)
          || (process.platform !== 'win32' && (before.mode & 0o077n) !== 0n)) throw corrupt();
        const bytes = await handle.readFile();
        const after = await handle.stat({ bigint: true });
        if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size
          || createHash('sha256').update(bytes).digest('hex') !== record.sha256) {
          bytes.fill(0);
          throw corrupt();
        }
        const publicBytes = new Uint8Array(bytes);
        bytes.fill(0);
        return { ...publicDescriptor(record), bytes: publicBytes };
      } finally {
        await handle.close();
      }
    });
  }

  remove(id: string): Promise<void> {
    return this.exclusive(async () => {
      assertId(id);
      const record = (await this.loadAll()).find((candidate) => candidate.id === id);
      if (!record) throw notFound();
      const tombstonePath = join(this.root, `.deleting-quarantine-${id}.json`);
      await writePrivateJsonExclusive(tombstonePath, { schemaVersion: 1 });
      await this.verifyStorage();
      await rm(join(this.root, `quarantine-${id}.json`));
      await rm(join(this.root, record.fileName));
      await rm(tombstonePath);
    });
  }

  private exclusive<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.queue.then(operation, operation);
    this.queue = result.then(() => undefined, () => undefined);
    return result;
  }

  private async loadAll(): Promise<readonly StoredRecord[]> {
    await this.prepareStorage();
    const names = await readdir(this.root);
    const recordNames = names.filter((name) => RECORD_PATTERN.test(name));
    const assetNames = new Set(names.filter((name) => PDF_PATTERN.test(name)));
    if (recordNames.length > this.maxEntries) throw corrupt();
    const records: StoredRecord[] = [];
    for (const name of recordNames) {
      const handle = await openNoFollow(join(this.root, name));
      let parsed: unknown;
      try {
        const info = await handle.stat({ bigint: true });
        if (!info.isFile() || info.nlink !== 1n || info.size < 2n || info.size > 16_384n
          || (process.platform !== 'win32' && (info.mode & 0o077n) !== 0n)) throw corrupt();
        parsed = JSON.parse(await handle.readFile('utf8'));
      } catch {
        throw corrupt();
      } finally {
        await handle.close();
      }
      const record = validateRecord(parsed, name, this.maxEntryBytes);
      if (!assetNames.delete(record.fileName)) throw corrupt();
      records.push(record);
    }
    if (assetNames.size > 0) throw corrupt();
    if (records.reduce((sum, record) => sum + record.byteLength, 0) > this.maxTotalBytes) throw corrupt();
    return records.sort((left, right) => left.createdAt.localeCompare(right.createdAt));
  }

  private async prepareStorage(): Promise<void> {
    await ensurePrivateRoot(this.root);
    await this.secureRoot?.(this.root);
    await this.verifyStorage();
    await recoverTransactions(this.root);
    await this.verifyStorage();
  }

  private async verifyStorage(): Promise<void> {
    if (this.platform === 'win32') await this.verifyRoot!(this.root);
    else await this.verifyRoot?.(this.root);
  }
}

async function recoverTransactions(root: string): Promise<void> {
  const names = await readdir(root);
  for (const name of names.filter((candidate) => CREATE_TOMBSTONE_PATTERN.test(candidate)).sort()) {
    const id = await tombstoneId(root, name, CREATE_TOMBSTONE_PATTERN);
    const recordPath = join(root, `quarantine-${id}.json`);
    const assetPath = join(root, `quarantine-${id}.pdf`);
    const recordExists = await regularPathExists(recordPath);
    const assetExists = await regularPathExists(assetPath);
    if (!(recordExists && assetExists)) {
      await rm(recordPath, { force: true });
      await rm(assetPath, { force: true });
    }
    await rm(join(root, name));
  }
  const afterCreate = await readdir(root);
  for (const name of afterCreate.filter((candidate) => DELETE_TOMBSTONE_PATTERN.test(candidate)).sort()) {
    const id = await tombstoneId(root, name, DELETE_TOMBSTONE_PATTERN);
    await rm(join(root, `quarantine-${id}.json`), { force: true });
    await rm(join(root, `quarantine-${id}.pdf`), { force: true });
    await rm(join(root, name));
  }
}

async function tombstoneId(
  root: string,
  name: string,
  pattern: RegExp,
): Promise<string> {
  const match = pattern.exec(name);
  if (!match) throw corrupt();
  const handle = await openNoFollow(join(root, name));
  try {
    const info = await handle.stat({ bigint: true });
    if (!info.isFile() || info.nlink !== 1n || info.size > 16_384n
      || (process.platform !== 'win32' && (info.mode & 0o077n) !== 0n)) throw corrupt();
    return match[1]!;
  } catch {
    throw corrupt();
  } finally {
    await handle.close();
  }
}

async function regularPathExists(path: string): Promise<boolean> {
  try {
    const info = await lstat(path);
    if (!info.isFile() || info.isSymbolicLink()) throw corrupt();
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false;
    throw error;
  }
}

async function writePrivateJsonExclusive(path: string, value: unknown): Promise<void> {
  const handle = await open(path, constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY, 0o600);
  try {
    await handle.writeFile(`${JSON.stringify(value)}\n`, 'utf8');
    await handle.sync();
  } finally {
    await handle.close();
  }
  await chmod(path, 0o600);
}

async function ensurePrivateRoot(root: string): Promise<void> {
  try {
    const existing = await lstat(root);
    if (!existing.isDirectory() || existing.isSymbolicLink()) throw unsafeStorage();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    await mkdir(root, { recursive: true, mode: 0o700 });
  }
  await chmod(root, 0o700);
  const canonical = await realpath(root);
  const info = await lstat(root);
  if (!isAbsolute(canonical) || !info.isDirectory() || info.isSymbolicLink()
    || (process.platform !== 'win32' && (info.mode & 0o077) !== 0)) throw unsafeStorage();
}

async function openNoFollow(path: string) {
  return open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0)).catch(() => { throw corrupt(); });
}

function validateRecord(value: unknown, recordName: string, maxBytes: number): StoredRecord {
  if (!isRecord(value)
    || !hasExactKeys(value, ['byteLength', 'createdAt', 'fileName', 'id', 'reason', 'sha256'])
    || !ID_PATTERN.test(String(value.id))
    || recordName !== `quarantine-${value.id}.json`
    || value.fileName !== `quarantine-${value.id}.pdf`
    || !isReason(value.reason)
    || !Number.isSafeInteger(value.byteLength) || (value.byteLength as number) < 8 || (value.byteLength as number) > maxBytes
    || typeof value.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(value.sha256)
    || typeof value.createdAt !== 'string' || !canonicalInstant(value.createdAt)) throw corrupt();
  return value as unknown as StoredRecord;
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const required = [...expected].sort();
  return actual.length === required.length && actual.every((key, index) => key === required[index]);
}

function publicDescriptor(record: StoredRecord): PdfSigningQuarantineDescriptor {
  return Object.freeze({
    id: record.id,
    reason: record.reason,
    byteLength: record.byteLength,
    sha256: record.sha256,
    createdAt: record.createdAt,
  });
}

function assertReason(value: string): asserts value is PdfSigningQuarantineReason {
  if (!isReason(value)) throw new PdfSigningQuarantineError('INVALID_INPUT', 'The quarantine reason is invalid.');
}

function isReason(value: unknown): value is PdfSigningQuarantineReason {
  return value === 'ENGINE_FAILURE' || value === 'PREFIX_MISMATCH'
    || value === 'POSTVALIDATION_FAILURE' || value === 'PUBLICATION_FAILURE';
}

function assertId(id: string): void {
  if (!ID_PATTERN.test(id)) throw notFound();
}

function boundedLimit(value: number | undefined, fallback: number, maximum: number): number {
  const result = value ?? fallback;
  if (!Number.isSafeInteger(result) || result < 1 || result > maximum) throw new TypeError('Quarantine limit is invalid.');
  return result;
}

function canonicalInstant(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(value) && !Number.isNaN(Date.parse(value));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function unsafeStorage(): PdfSigningQuarantineError {
  return new PdfSigningQuarantineError('UNSAFE_STORAGE', 'The private PDF signing quarantine is unsafe.');
}

function corrupt(): PdfSigningQuarantineError {
  return new PdfSigningQuarantineError('CORRUPT_STORAGE', 'The private PDF signing quarantine is corrupt.');
}

function notFound(): PdfSigningQuarantineError {
  return new PdfSigningQuarantineError('NOT_FOUND', 'The quarantined PDF output was not found.');
}
