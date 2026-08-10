import { createHash } from 'node:crypto';
import { constants, type BigIntStats } from 'node:fs';
import {
  chmod,
  link,
  lstat,
  mkdtemp,
  open,
  realpath,
  rm,
  unlink,
  type FileHandle,
} from 'node:fs/promises';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { assertPdfPublicationDirectory, capturePdfPublicationTarget } from './pdfPublication';
import {
  PdfSigningQuarantine,
  PdfSigningQuarantineError,
  type PdfSigningQuarantineReason,
} from './pdfSigningQuarantine';

const DEFAULT_MAX_PDF_BYTES = 512 * 1024 * 1024;
const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const FIELD_NAME_PATTERN = /^[A-Za-z0-9_.:-]{1,128}$/;

export type PdfSignedMutationErrorCode =
  | 'INVALID_REQUEST'
  | 'UNSAFE_SOURCE'
  | 'UNSAFE_TARGET'
  | 'TARGET_EXISTS'
  | 'SOURCE_CHANGED'
  | 'MUTATION_FAILED'
  | 'OUTPUT_UNSAFE'
  | 'PREFIX_MISMATCH'
  | 'POSTVALIDATION_FAILED'
  | 'PUBLICATION_FAILED'
  | 'CANCELLED'
  | 'CLEANUP_FAILED';

export class PdfSignedMutationError extends Error {
  constructor(readonly code: PdfSignedMutationErrorCode, message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'PdfSignedMutationError';
  }
}

export interface PdfSignedMutationRequest {
  readonly inputSnapshotPath: string;
  readonly outputPath: string;
  readonly expectedInputSha256: string;
  readonly expectedCertificateSha256: string;
  readonly expectedFieldName: string;
}

export interface PdfSignedMutationEngineResult {
  readonly inputSha256: string;
  readonly outputSha256: string;
  readonly fieldName: string;
  readonly incrementalUpdate: true;
  readonly inputPrefixPreserved: true;
}

export interface PdfSignedMutationPostvalidation {
  readonly inputSha256: string;
  readonly outputSha256: string;
  readonly fieldName: string;
  readonly certificateSha256: string;
  readonly inputPrefixPreserved: true;
  readonly addedSignatureCount: 1;
  readonly priorSignaturesPreserved: true;
  readonly newSignatureCoversOutputExceptContents: true;
  readonly cryptographicallyValid: true;
  readonly structurallyReadable: true;
  /** Must be obtained in a fresh validator process, never copied from mutate(). */
  readonly independentProcess: true;
  readonly validator: 'pdf-signature-core-v1-validate-plus-main-prefix';
}

export interface CreatePdfSignedMutationOptions {
  readonly sourcePath: string;
  readonly expectedSourceSha256: string;
  readonly expectedCertificateSha256: string;
  readonly targetPath: string;
  readonly expectedFieldName: string;
  readonly mutate: (request: PdfSignedMutationRequest) => Promise<PdfSignedMutationEngineResult>;
  readonly postvalidate: (request: PdfSignedMutationRequest & {
    readonly outputSha256: string;
  }) => Promise<PdfSignedMutationPostvalidation>;
  readonly secureWorkspace?: (workspacePath: string) => Promise<void>;
  readonly verifyWorkspace?: (workspacePath: string) => Promise<void>;
  readonly maxInputBytes?: number;
  readonly maxOutputBytes?: number;
  readonly platform?: NodeJS.Platform;
  /** Additional installed-platform readback verification after linking. */
  readonly verifyPublished?: (targetPath: string) => Promise<void>;
  /** Test seam for deterministic cleanup-failure coverage. */
  readonly cleanupWorkspace?: (workspacePath: string) => Promise<void>;
  /** Optional main-owned retention for failed PDF candidates before cleanup. */
  readonly quarantine?: PdfSigningQuarantine;
  readonly signal?: AbortSignal;
}

export interface PdfSignedMutationResult extends PdfSignedMutationPostvalidation {
  readonly bytesWritten: number;
  readonly published: true;
}

interface StableIdentity {
  readonly dev: bigint;
  readonly ino: bigint;
  readonly size: bigint;
  readonly mtimeNs: bigint;
  readonly ctimeNs: bigint;
}

/**
 * Main-only incremental publication boundary. The source is held read-only,
 * the sidecar sees a private immutable snapshot and precreated 0600 output,
 * and the destination appears only through an atomic no-replace hard link
 * after independent prefix, structural and cryptographic postvalidation.
 */
export async function createPdfSignedMutation(
  options: CreatePdfSignedMutationOptions,
): Promise<PdfSignedMutationResult> {
  assertOptions(options);
  throwIfMutationAborted(options.signal);
  const platform = options.platform ?? process.platform;
  if (platform === 'win32' && (!options.secureWorkspace || !options.verifyWorkspace)) {
    throw new PdfSignedMutationError(
      'INVALID_REQUEST',
      'Windows PDF signing requires fail-closed workspace ACL security hooks.',
    );
  }
  const maxInputBytes = boundedLimit(options.maxInputBytes);
  const maxOutputBytes = boundedLimit(options.maxOutputBytes);
  const sourcePath = await canonicalSource(options.sourcePath);
  const selectedTarget = resolve(options.targetPath);
  const publication = await capturePdfPublicationTarget(selectedTarget).catch(() => {
    throw new PdfSignedMutationError('UNSAFE_TARGET', 'The signed PDF destination is unsafe.');
  });
  const requestedTarget = publication.targetPath;
  if (sourcePath === requestedTarget) {
    throw new PdfSignedMutationError('UNSAFE_TARGET', 'The signed PDF destination is unsafe.');
  }
  await assertTargetAbsent(requestedTarget);

  let sourceHandle: FileHandle | undefined;
  let snapshotHandle: FileHandle | undefined;
  let outputHandle: FileHandle | undefined;
  let workspacePath: string | undefined;
  let publishedIdentity: StableIdentity | undefined;
  try {
    sourceHandle = await openNoFollow(sourcePath, constants.O_RDONLY, 'UNSAFE_SOURCE');
    const sourceBefore = await snapshotHandleState(sourceHandle, maxInputBytes, 'UNSAFE_SOURCE');
    if (sourceBefore.sha256 !== options.expectedSourceSha256) throw sourceChanged();

    workspacePath = await mkdtemp(join(dirname(requestedTarget), '.butter-paper-signing-'));
    await chmod(workspacePath, 0o700);
    await assertPrivateDirectory(workspacePath);
    await options.secureWorkspace?.(workspacePath);
    await options.verifyWorkspace?.(workspacePath);

    const snapshotPath = join(workspacePath, 'validated-source.pdf');
    const outputPath = join(workspacePath, 'signed-output.pdf');
    snapshotHandle = await open(snapshotPath, constants.O_CREAT | constants.O_EXCL | constants.O_RDWR, 0o600);
    await copyHandle(sourceHandle, snapshotHandle, Number(sourceBefore.identity.size));
    await snapshotHandle.sync();
    await chmod(snapshotPath, 0o600);
    const snapshotBefore = await snapshotHandleState(snapshotHandle, maxInputBytes, 'OUTPUT_UNSAFE');
    if (snapshotBefore.sha256 !== sourceBefore.sha256) throw outputUnsafe();
    outputHandle = await open(outputPath, constants.O_CREAT | constants.O_EXCL | constants.O_RDWR, 0o600);
    await outputHandle.sync();
    await chmod(outputPath, 0o600);
    const emptyOutput = await outputHandle.stat({ bigint: true });
    assertPrivateSingleLinkFile(emptyOutput, true);
    await options.verifyWorkspace?.(workspacePath);

    const request: PdfSignedMutationRequest = {
      inputSnapshotPath: snapshotPath,
      outputPath,
      expectedInputSha256: sourceBefore.sha256,
      expectedCertificateSha256: options.expectedCertificateSha256,
      expectedFieldName: options.expectedFieldName,
    };
    throwIfMutationAborted(options.signal);
    let engineResult: PdfSignedMutationEngineResult;
    try {
      engineResult = await options.mutate(request);
    } catch (error) {
      throwIfMutationAborted(options.signal);
      throw new PdfSignedMutationError(
        'MUTATION_FAILED',
        'The PDF signing engine could not complete the mutation.',
        { cause: error },
      );
    }
    throwIfMutationAborted(options.signal);
    await options.verifyWorkspace?.(workspacePath);
    const snapshotAfterMutation = await snapshotHandleState(snapshotHandle, maxInputBytes, 'OUTPUT_UNSAFE');
    if (!sameIdentity(snapshotBefore.identity, snapshotAfterMutation.identity)
      || snapshotAfterMutation.sha256 !== sourceBefore.sha256) throw outputUnsafe();
    const outputAfterMutation = await snapshotBoundOutput(outputPath, outputHandle, emptyOutput, maxOutputBytes);
    if (!validEngineResult(engineResult, request, outputAfterMutation.sha256)) throw outputUnsafe();
    if (!(await exactPrefix(snapshotHandle, outputHandle, Number(sourceBefore.identity.size)))) {
      throw new PdfSignedMutationError('PREFIX_MISMATCH', 'The signed PDF did not preserve the complete source prefix.');
    }

    let postvalidation: PdfSignedMutationPostvalidation;
    try {
      postvalidation = await options.postvalidate({ ...request, outputSha256: outputAfterMutation.sha256 });
    } catch (error) {
      throwIfMutationAborted(options.signal);
      throw new PdfSignedMutationError(
        'POSTVALIDATION_FAILED',
        'The signed PDF could not be independently validated.',
        { cause: error },
      );
    }
    throwIfMutationAborted(options.signal);
    if (!validPostvalidation(postvalidation, request, outputAfterMutation.sha256)) {
      throw new PdfSignedMutationError('POSTVALIDATION_FAILED', 'The signed PDF failed independent postvalidation.');
    }
    await options.verifyWorkspace?.(workspacePath);
    const outputAfterValidation = await snapshotBoundOutput(
      outputPath,
      outputHandle,
      outputAfterMutation.identity,
      maxOutputBytes,
    );
    if (outputAfterValidation.sha256 !== outputAfterMutation.sha256
      || !(await exactPrefix(snapshotHandle, outputHandle, Number(sourceBefore.identity.size)))) {
      throw new PdfSignedMutationError('OUTPUT_UNSAFE', 'The signed PDF changed during postvalidation.');
    }
    const sourceAfter = await snapshotHandleState(sourceHandle, maxInputBytes, 'SOURCE_CHANGED');
    const sourceByPath = await openNoFollow(sourcePath, constants.O_RDONLY, 'SOURCE_CHANGED');
    try {
      const currentSource = await snapshotHandleState(sourceByPath, maxInputBytes, 'SOURCE_CHANGED');
      if (!sameIdentity(sourceBefore.identity, sourceAfter.identity)
        || !sameIdentity(sourceBefore.identity, currentSource.identity)
        || sourceAfter.sha256 !== sourceBefore.sha256
        || currentSource.sha256 !== sourceBefore.sha256) throw sourceChanged();
    } finally {
      await sourceByPath.close();
    }
    await assertPdfPublicationDirectory(publication.directoryIdentity);
    await assertTargetAbsent(requestedTarget);
    await assertPrivateDirectory(workspacePath);
    throwIfMutationAborted(options.signal);
    let linked = false;
    try {
      await link(outputPath, requestedTarget);
      linked = true;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'EEXIST') {
        throw new PdfSignedMutationError('TARGET_EXISTS', 'The signed PDF destination already exists.');
      }
      throw new PdfSignedMutationError('PUBLICATION_FAILED', 'The signed PDF could not be published safely.');
    }
    try {
      await assertPdfPublicationDirectory(publication.directoryIdentity);
      await verifyPublishedIdentity(requestedTarget, outputAfterValidation, maxOutputBytes);
      throwIfMutationAborted(options.signal);
      await options.verifyPublished?.(requestedTarget);
      throwIfMutationAborted(options.signal);
      await assertPdfPublicationDirectory(publication.directoryIdentity);
      publishedIdentity = await verifyPublishedIdentity(requestedTarget, outputAfterValidation, maxOutputBytes);
    } catch (error) {
      if (linked) await rollbackPublishedTarget(requestedTarget, outputAfterValidation.identity);
      if (error instanceof PdfSignedMutationError && error.code === 'CANCELLED') throw error;
      throw new PdfSignedMutationError('PUBLICATION_FAILED', 'The signed PDF failed final publication verification.');
    }
    return {
      inputSha256: postvalidation.inputSha256,
      outputSha256: postvalidation.outputSha256,
      fieldName: postvalidation.fieldName,
      certificateSha256: postvalidation.certificateSha256,
      inputPrefixPreserved: true,
      addedSignatureCount: 1,
      priorSignaturesPreserved: true,
      newSignatureCoversOutputExceptContents: true,
      cryptographicallyValid: true,
      structurallyReadable: true,
      independentProcess: true,
      validator: 'pdf-signature-core-v1-validate-plus-main-prefix',
      bytesWritten: Number(outputAfterValidation.identity.size),
      published: true,
    };
  } catch (error) {
    await retainFailedOutput(options, outputHandle, quarantineReason(error));
    throw error;
  } finally {
    let cleanupFailed = false;
    for (const handle of [outputHandle, snapshotHandle, sourceHandle]) {
      try {
        await handle?.close();
      } catch {
        cleanupFailed = true;
      }
    }
    if (workspacePath) {
      try {
        if (options.cleanupWorkspace) await options.cleanupWorkspace(workspacePath);
        else await rm(workspacePath, { recursive: true, force: true });
      } catch {
        cleanupFailed = true;
      }
    }
    if (cleanupFailed) {
      if (publishedIdentity) await rollbackPublishedTarget(requestedTarget, publishedIdentity);
      throw new PdfSignedMutationError('CLEANUP_FAILED', 'The private signing workspace cleanup failed.');
    }
  }
}

async function retainFailedOutput(
  options: CreatePdfSignedMutationOptions,
  outputHandle: FileHandle | undefined,
  reason: PdfSigningQuarantineReason | undefined,
): Promise<void> {
  if (!options.quarantine || !outputHandle || !reason) return;
  const stats = await outputHandle.stat({ bigint: true }).catch(() => undefined);
  const maxOutputBytes = boundedLimit(options.maxOutputBytes);
  if (!stats || !stats.isFile() || stats.size < 8n || stats.size > BigInt(maxOutputBytes)) return;
  const bytes = await outputHandle.readFile().catch(() => undefined);
  if (!bytes || !bytes.subarray(0, 5).equals(Buffer.from('%PDF-', 'ascii'))) {
    bytes?.fill(0);
    return;
  }
  try {
    await options.quarantine.retain(bytes, reason);
  } catch (error) {
    if (error instanceof PdfSigningQuarantineError
      && (error.code === 'INVALID_INPUT' || error.code === 'LIMIT_EXCEEDED')) return;
    throw new PdfSignedMutationError(
      'CLEANUP_FAILED',
      'The failed signed PDF could not be retained safely.',
      { cause: error },
    );
  } finally {
    bytes.fill(0);
  }
}

function quarantineReason(error: unknown): PdfSigningQuarantineReason | undefined {
  if (!(error instanceof PdfSignedMutationError)) return undefined;
  switch (error.code) {
    case 'MUTATION_FAILED':
      return 'ENGINE_FAILURE';
    case 'PREFIX_MISMATCH':
      return 'PREFIX_MISMATCH';
    case 'POSTVALIDATION_FAILED':
    case 'OUTPUT_UNSAFE':
      return 'POSTVALIDATION_FAILURE';
    case 'PUBLICATION_FAILED':
      return 'PUBLICATION_FAILURE';
    default:
      return undefined;
  }
}

async function canonicalSource(path: string): Promise<string> {
  if (typeof path !== 'string' || !isAbsolute(path)) throw unsafeSource();
  const normalized = resolve(path);
  const info = await lstat(normalized).catch(() => { throw unsafeSource(); });
  if (!info.isFile() || info.isSymbolicLink()) throw unsafeSource();
  const canonical = await realpath(normalized).catch(() => { throw unsafeSource(); });
  return canonical;
}

async function openNoFollow(path: string, flags: number, code: PdfSignedMutationErrorCode): Promise<FileHandle> {
  const handle = await open(path, flags | (constants.O_NOFOLLOW ?? 0)).catch(() => {
    throw new PdfSignedMutationError(code, 'The PDF file boundary is unsafe.');
  });
  try {
    const stats = await handle.stat({ bigint: true });
    if (!stats.isFile() || stats.nlink < 1n) throw new PdfSignedMutationError(code, 'The PDF file boundary is unsafe.');
    return handle;
  } catch (error) {
    await handle.close();
    throw error;
  }
}

async function snapshotHandleState(handle: FileHandle, maxBytes: number, code: PdfSignedMutationErrorCode) {
  const before = await handle.stat({ bigint: true });
  if (!before.isFile() || before.size < 1n || before.size > BigInt(maxBytes)) {
    throw new PdfSignedMutationError(code, 'The PDF file boundary is unsafe.');
  }
  const digest = createHash('sha256');
  const buffer = Buffer.allocUnsafe(64 * 1024);
  let offset = 0;
  while (offset < Number(before.size)) {
    const length = Math.min(buffer.byteLength, Number(before.size) - offset);
    const { bytesRead } = await handle.read(buffer, 0, length, offset);
    if (bytesRead === 0) break;
    digest.update(buffer.subarray(0, bytesRead));
    offset += bytesRead;
  }
  buffer.fill(0);
  const after = await handle.stat({ bigint: true });
  const beforeIdentity = stableIdentity(before);
  if (offset !== Number(before.size) || !sameIdentity(beforeIdentity, stableIdentity(after))) {
    throw new PdfSignedMutationError(code, 'The PDF file changed while it was being verified.');
  }
  return { identity: beforeIdentity, sha256: digest.digest('hex') };
}

async function snapshotBoundOutput(path: string, handle: FileHandle, expected: BigIntStats | StableIdentity, maxBytes: number) {
  const byPath = await lstat(path, { bigint: true }).catch(() => { throw outputUnsafe(); });
  const handleStats = await handle.stat({ bigint: true });
  const expectedIdentity = 'birthtimeNs' in expected ? stableIdentity(expected as BigIntStats) : expected as StableIdentity;
  if (!byPath.isFile() || byPath.isSymbolicLink()
    || handleStats.dev !== expectedIdentity.dev || handleStats.ino !== expectedIdentity.ino
    || byPath.dev !== handleStats.dev || byPath.ino !== handleStats.ino
    || handleStats.nlink !== 1n) throw outputUnsafe();
  assertPrivateSingleLinkFile(handleStats, false);
  if (process.platform !== 'win32' && (byPath.mode & 0o077n) !== 0n) throw outputUnsafe();
  return snapshotHandleState(handle, maxBytes, 'OUTPUT_UNSAFE');
}

async function verifyPublishedIdentity(
  path: string,
  expected: { readonly identity: StableIdentity; readonly sha256: string },
  maxBytes: number,
): Promise<StableIdentity> {
  const published = await openNoFollow(path, constants.O_RDONLY, 'PUBLICATION_FAILED');
  try {
    const before = await published.stat({ bigint: true });
    if (before.nlink !== 2n || (process.platform !== 'win32' && (before.mode & 0o077n) !== 0n)) {
      throw new PdfSignedMutationError('PUBLICATION_FAILED', 'The published signed PDF permissions are unsafe.');
    }
    const state = await snapshotHandleState(published, maxBytes, 'PUBLICATION_FAILED');
    if (!sameIdentityIgnoringLinkCount(expected.identity, state.identity) || state.sha256 !== expected.sha256) {
      throw new PdfSignedMutationError('PUBLICATION_FAILED', 'The published signed PDF changed at publication.');
    }
    return state.identity;
  } finally {
    await published.close();
  }
}

async function copyHandle(source: FileHandle, target: FileHandle, size: number): Promise<void> {
  const buffer = Buffer.allocUnsafe(64 * 1024);
  let offset = 0;
  try {
    while (offset < size) {
      const length = Math.min(buffer.byteLength, size - offset);
      const read = await source.read(buffer, 0, length, offset);
      if (read.bytesRead === 0) throw sourceChanged();
      const written = await target.write(buffer, 0, read.bytesRead, offset);
      if (written.bytesWritten !== read.bytesRead) throw outputUnsafe();
      offset += read.bytesRead;
    }
  } finally {
    buffer.fill(0);
  }
}

async function exactPrefix(source: FileHandle, output: FileHandle, size: number): Promise<boolean> {
  const left = Buffer.allocUnsafe(64 * 1024);
  const right = Buffer.allocUnsafe(64 * 1024);
  let offset = 0;
  try {
    while (offset < size) {
      const length = Math.min(left.byteLength, size - offset);
      const [a, b] = await Promise.all([
        source.read(left, 0, length, offset),
        output.read(right, 0, length, offset),
      ]);
      if (a.bytesRead !== length || b.bytesRead !== length
        || !left.subarray(0, length).equals(right.subarray(0, length))) return false;
      offset += length;
    }
    return true;
  } finally {
    left.fill(0);
    right.fill(0);
  }
}

async function assertPrivateDirectory(path: string): Promise<void> {
  const info = await lstat(path);
  if (!info.isDirectory() || info.isSymbolicLink() || (process.platform !== 'win32' && (info.mode & 0o077) !== 0)) {
    throw outputUnsafe();
  }
}

function assertPrivateSingleLinkFile(stats: BigIntStats, allowEmpty: boolean): void {
  if (!stats.isFile() || stats.nlink !== 1n || (!allowEmpty && stats.size < 1n)
    || (process.platform !== 'win32' && (stats.mode & 0o077n) !== 0n)) throw outputUnsafe();
}

async function assertTargetAbsent(path: string): Promise<void> {
  try {
    await lstat(path);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return;
    throw new PdfSignedMutationError('UNSAFE_TARGET', 'The signed PDF destination is unsafe.');
  }
  throw new PdfSignedMutationError('TARGET_EXISTS', 'The signed PDF destination already exists.');
}

function assertOptions(options: CreatePdfSignedMutationOptions): void {
  if (!isAbsolute(options.targetPath)
    || !SHA256_PATTERN.test(options.expectedSourceSha256)
    || !SHA256_PATTERN.test(options.expectedCertificateSha256)
    || !FIELD_NAME_PATTERN.test(options.expectedFieldName)
    || typeof options.mutate !== 'function'
    || typeof options.postvalidate !== 'function') {
    throw new PdfSignedMutationError('INVALID_REQUEST', 'The signed PDF mutation request is invalid.');
  }
}

function validEngineResult(value: unknown, request: PdfSignedMutationRequest, outputSha256: string): value is PdfSignedMutationEngineResult {
  return isRecord(value)
    && exactKeys(value, ['fieldName', 'incrementalUpdate', 'inputPrefixPreserved', 'inputSha256', 'outputSha256'])
    && value.inputSha256 === request.expectedInputSha256
    && value.outputSha256 === outputSha256
    && value.fieldName === request.expectedFieldName
    && value.incrementalUpdate === true
    && value.inputPrefixPreserved === true;
}

function validPostvalidation(value: unknown, request: PdfSignedMutationRequest, outputSha256: string): value is PdfSignedMutationPostvalidation {
  return isRecord(value)
    && exactKeys(value, [
      'addedSignatureCount', 'certificateSha256', 'cryptographicallyValid', 'fieldName', 'independentProcess',
      'inputPrefixPreserved', 'inputSha256', 'newSignatureCoversOutputExceptContents',
      'outputSha256', 'priorSignaturesPreserved', 'structurallyReadable', 'validator',
    ])
    && value.inputSha256 === request.expectedInputSha256
    && value.outputSha256 === outputSha256
    && value.fieldName === request.expectedFieldName
    && value.certificateSha256 === request.expectedCertificateSha256
    && value.inputPrefixPreserved === true
    && value.addedSignatureCount === 1
    && value.priorSignaturesPreserved === true
    && value.newSignatureCoversOutputExceptContents === true
    && value.cryptographicallyValid === true
    && value.structurallyReadable === true
    && value.independentProcess === true
    && value.validator === 'pdf-signature-core-v1-validate-plus-main-prefix';
}

function exactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const required = [...expected].sort();
  return actual.length === required.length && actual.every((key, index) => key === required[index]);
}

async function rollbackPublishedTarget(path: string, expected: StableIdentity): Promise<void> {
  const current = await lstat(path, { bigint: true }).catch(() => undefined);
  if (!current || !current.isFile() || current.isSymbolicLink()
    || current.dev !== expected.dev || current.ino !== expected.ino) return;
  await unlink(path).catch(() => {
    throw new PdfSignedMutationError('PUBLICATION_FAILED', 'The failed signed PDF publication could not be rolled back safely.');
  });
}

function stableIdentity(stats: BigIntStats): StableIdentity {
  return { dev: stats.dev, ino: stats.ino, size: stats.size, mtimeNs: stats.mtimeNs, ctimeNs: stats.ctimeNs };
}

function sameIdentity(left: StableIdentity, right: StableIdentity): boolean {
  return left.dev === right.dev && left.ino === right.ino && left.size === right.size
    && left.mtimeNs === right.mtimeNs && left.ctimeNs === right.ctimeNs;
}

function sameIdentityIgnoringLinkCount(left: StableIdentity, right: StableIdentity): boolean {
  // Creating the publication hard link legitimately changes ctime and nlink.
  // The device/inode, byte size and content mtime must remain bound.
  return left.dev === right.dev && left.ino === right.ino
    && left.size === right.size && left.mtimeNs === right.mtimeNs;
}

function boundedLimit(value: number | undefined): number {
  const resolved = value ?? DEFAULT_MAX_PDF_BYTES;
  if (!Number.isSafeInteger(resolved) || resolved < 1 || resolved > DEFAULT_MAX_PDF_BYTES) {
    throw new PdfSignedMutationError('INVALID_REQUEST', 'The signed PDF size limit is invalid.');
  }
  return resolved;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function unsafeSource(): PdfSignedMutationError {
  return new PdfSignedMutationError('UNSAFE_SOURCE', 'The PDF source is unsafe.');
}

function sourceChanged(): PdfSignedMutationError {
  return new PdfSignedMutationError('SOURCE_CHANGED', 'The validated PDF source changed.');
}

function outputUnsafe(): PdfSignedMutationError {
  return new PdfSignedMutationError('OUTPUT_UNSAFE', 'The private signed PDF output is unsafe.');
}

function throwIfMutationAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted) {
    throw new PdfSignedMutationError('CANCELLED', 'The signed PDF mutation was cancelled.');
  }
}

export function pdfSignedMutationSha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}
