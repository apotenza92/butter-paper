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
  type FileHandle,
} from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, resolve } from 'node:path';
import {
  isPdfSignatureValidationReport,
  type PdfSignatureValidationReport,
} from '@butter-paper/core';
import { assertPdfPublicationDirectory, capturePdfPublicationTarget } from './pdfPublication';

const DEFAULT_MAX_INPUT_BYTES = 512 * 1024 * 1024;
const DEFAULT_MAX_OUTPUT_BYTES = 512 * 1024 * 1024;
const MAX_REMOVAL_COUNT = 1_000_000;
const MAX_WARNINGS = 64;
const MAX_WARNING_LENGTH = 2048;
const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const POLICY_ID_PATTERN = /^[A-Za-z0-9._:-]{1,128}$/;
const ENGINE_VERSION_PATTERN = /^[0-9A-Za-z][0-9A-Za-z._+-]{0,127}$/;

export const PDF_UNSIGNED_COPY_REMOVAL_POLICY_ID = 'butter-paper-structurally-unsigned-copy';
export const PDF_UNSIGNED_COPY_REMOVAL_POLICY_VERSION = 1;

export type PdfUnsignedCopyErrorCode =
  | 'INVALID_REQUEST'
  | 'UNSAFE_SOURCE'
  | 'UNSAFE_TARGET'
  | 'TARGET_EXISTS'
  | 'SOURCE_CHANGED'
  | 'CONVERTER_FAILED'
  | 'INVALID_CONVERTER_RESULT'
  | 'OUTPUT_UNSAFE'
  | 'WORKSPACE_SECURITY_FAILED'
  | 'OUTPUT_CHANGED'
  | 'STRUCTURAL_VALIDATION_FAILED'
  | 'SIGNATURE_VALIDATION_FAILED'
  | 'PUBLICATION_FAILED'
  | 'CLEANUP_FAILED';

export class PdfUnsignedCopyError extends Error {
  constructor(readonly code: PdfUnsignedCopyErrorCode, message: string) {
    super(message);
    this.name = 'PdfUnsignedCopyError';
  }
}

export interface PdfUnsignedCopyConverterRequest {
  /** Private main-owned snapshot, never the signed original path. */
  readonly inputSnapshotPath: string;
  /** Exclusive 0600 output file inside the private operation workspace. */
  readonly outputPath: string;
  readonly expectedInputSha256: string;
}

export interface PdfUnsignedCopyRemovalCounts {
  readonly signatureValues: number;
  readonly signatureFields: number;
  readonly signatureWidgets: number;
  readonly certificationReferences: number;
  readonly fieldMdpReferences: number;
  readonly validationEvidenceEntries: number;
}

export interface PdfUnsignedCopyConversionResult {
  readonly engineVersion: string;
  readonly inputSha256: string;
  readonly outputSha256: string;
  readonly removalPolicyId: string;
  readonly removalPolicyVersion: number;
  readonly removed: PdfUnsignedCopyRemovalCounts;
  readonly warnings: readonly string[];
}

export interface PdfUnsignedCopyStructuralVerification {
  readonly structurallyReadable: true;
  readonly byteRangeMarkerCount: number;
  readonly signatureDictionaryCount: number;
  readonly signedSignatureFieldCount: number;
  readonly docMdpReferenceCount: number;
  readonly fieldMdpReferenceCount: number;
  readonly dssOrVriEntryCount: number;
}

export interface PdfUnsignedCopyResult {
  readonly engineVersion: string;
  readonly inputSha256: string;
  readonly outputSha256: string;
  readonly bytesWritten: number;
  readonly removalPolicyId: string;
  readonly removalPolicyVersion: number;
  readonly removed: PdfUnsignedCopyRemovalCounts;
  readonly warnings: readonly string[];
  readonly validatedUnsigned: true;
}

export interface CreatePdfUnsignedCopyOptions {
  readonly sourcePath: string;
  /** Hash bound to the prior privileged validation/capability registration. */
  readonly expectedSourceSha256: string;
  readonly targetPath: string;
  readonly convert: (request: PdfUnsignedCopyConverterRequest) => Promise<PdfUnsignedCopyConversionResult>;
  readonly validate: (outputPath: string) => Promise<PdfSignatureValidationReport>;
  readonly verifyStructure: (outputPath: string) => Promise<PdfUnsignedCopyStructuralVerification>;
  /** Required on Windows; invoked before any snapshot bytes enter the workspace. */
  readonly secureWorkspace?: (workspacePath: string) => Promise<void>;
  /** Required on Windows; rechecks the protected workspace after each privileged subprocess. */
  readonly verifyWorkspace?: (workspacePath: string) => Promise<void>;
  readonly maxInputBytes?: number;
  readonly maxOutputBytes?: number;
}

interface StableFileIdentity {
  readonly dev: bigint;
  readonly ino: bigint;
  readonly size: bigint;
  readonly mtimeMs: bigint;
  readonly ctimeMs: bigint;
}

interface FileSnapshot {
  readonly identity: StableFileIdentity;
  readonly sha256: string;
}

/**
 * Creates and publishes a new unsigned PDF without ever offering the signed
 * source to the converter. The destination is created only after the output
 * and the unchanged source have independently passed every validation gate.
 */
export async function createPdfUnsignedCopy(
  options: CreatePdfUnsignedCopyOptions,
): Promise<PdfUnsignedCopyResult> {
  assertOptions(options);
  const maxInputBytes = boundedPositiveInteger(options.maxInputBytes, DEFAULT_MAX_INPUT_BYTES);
  const maxOutputBytes = boundedPositiveInteger(options.maxOutputBytes, DEFAULT_MAX_OUTPUT_BYTES);
  const sourcePath = await canonicalRegularSource(options.sourcePath);
  const targetPath = await canonicalNewTarget(options.targetPath);
  const publicationTarget = await capturePdfPublicationTarget(targetPath).catch(() => {
    throw unsafeTarget('The unsigned-copy destination directory is unsafe.');
  });
  if (publicationTarget.targetPath !== targetPath) throw unsafeTarget();
  if (sourcePath === targetPath) throw unsafeTarget('Create Unsigned Copy cannot overwrite the signed source.');

  const targetDirectory = dirname(targetPath);
  let workspacePath: string | undefined;
  let sourceHandle: FileHandle | undefined;
  let outputHandle: FileHandle | undefined;
  let publishedIdentity: StableFileIdentity | undefined;
  let result: PdfUnsignedCopyResult | undefined;
  let operationError: unknown;

  try {
    sourceHandle = await openRegularNoFollow(sourcePath, 'source');
    const sourceBefore = await snapshotFileHandle(sourceHandle, maxInputBytes, 'source');
    if (sourceBefore.sha256 !== options.expectedSourceSha256) {
      throw new PdfUnsignedCopyError('SOURCE_CHANGED', 'The signed source no longer matches its validated capability.');
    }

    workspacePath = await mkdtemp(join(targetDirectory, '.butter-paper-unsigned-copy-'));
    await chmod(workspacePath, 0o700);
    await secureWorkspace(options, workspacePath);
    const workspaceIdentity = await privateWorkspaceIdentity(workspacePath);
    const snapshotPath = join(workspacePath, 'validated-source.pdf');
    const outputPath = join(workspacePath, 'unsigned-output.pdf');
    await copySourceSnapshot(sourceHandle, sourceBefore, snapshotPath);
    const protectedSnapshot = await snapshotRegularPath(snapshotPath, maxInputBytes, 'output');
    outputHandle = await open(outputPath, 'wx+', 0o600);
    await outputHandle.sync();
    await chmod(outputPath, 0o600);
    const emptyOutputIdentity = stableIdentity(await outputHandle.stat({ bigint: true }));
    await verifyWorkspace(options, workspacePath);

    let conversion: PdfUnsignedCopyConversionResult;
    try {
      conversion = await options.convert({
        inputSnapshotPath: snapshotPath,
        outputPath,
        expectedInputSha256: sourceBefore.sha256,
      });
    } catch {
      throw new PdfUnsignedCopyError('CONVERTER_FAILED', 'The unsigned-copy converter could not complete the operation.');
    }
    if (!isConversionResult(conversion)) {
      throw new PdfUnsignedCopyError('INVALID_CONVERTER_RESULT', 'The unsigned-copy converter returned an invalid result.');
    }
    if (conversion.inputSha256 !== sourceBefore.sha256) {
      throw new PdfUnsignedCopyError('INVALID_CONVERTER_RESULT', 'The converter result does not describe the validated source snapshot.');
    }
    if (conversion.removalPolicyId !== PDF_UNSIGNED_COPY_REMOVAL_POLICY_ID
      || conversion.removalPolicyVersion !== PDF_UNSIGNED_COPY_REMOVAL_POLICY_VERSION) {
      throw new PdfUnsignedCopyError(
        'INVALID_CONVERTER_RESULT',
        'The converter did not use the reviewed unsigned-copy removal policy.',
      );
    }
    await verifyWorkspace(options, workspacePath);

    const snapshotAfterConversion = await snapshotRegularPath(snapshotPath, maxInputBytes, 'output');
    if (!sameIdentity(protectedSnapshot.identity, snapshotAfterConversion.identity)
      || snapshotAfterConversion.sha256 !== sourceBefore.sha256) {
      throw new PdfUnsignedCopyError('OUTPUT_UNSAFE', 'The converter changed the protected source snapshot.');
    }
    const outputAfterConversion = await snapshotBoundOutput(
      outputPath,
      outputHandle,
      emptyOutputIdentity,
      maxOutputBytes,
      false,
    );
    if (outputAfterConversion.sha256 !== conversion.outputSha256) {
      throw new PdfUnsignedCopyError('INVALID_CONVERTER_RESULT', 'The converter output digest does not match the produced bytes.');
    }

    let structural: PdfUnsignedCopyStructuralVerification;
    try {
      structural = await options.verifyStructure(outputPath);
    } catch {
      throw new PdfUnsignedCopyError(
        'STRUCTURAL_VALIDATION_FAILED',
        'The unsigned-copy structure could not be independently verified.',
      );
    }
    if (!isCleanUnsignedStructure(structural)) {
      throw new PdfUnsignedCopyError(
        'STRUCTURAL_VALIDATION_FAILED',
        'The converted PDF still contains signature or certification remnants.',
      );
    }
    await verifyWorkspace(options, workspacePath);

    let validationReport: PdfSignatureValidationReport;
    try {
      validationReport = await options.validate(outputPath);
    } catch {
      throw new PdfUnsignedCopyError(
        'SIGNATURE_VALIDATION_FAILED',
        'The converted PDF could not be independently validated as unsigned.',
      );
    }

    const outputAfterValidation = await snapshotBoundOutput(
      outputPath,
      outputHandle,
      outputAfterConversion.identity,
      maxOutputBytes,
      true,
    );
    if (outputAfterValidation.sha256 !== outputAfterConversion.sha256) {
      throw new PdfUnsignedCopyError('OUTPUT_CHANGED', 'The converted PDF changed during output validation.');
    }
    assertUnsignedValidationReport(validationReport, outputAfterValidation.sha256);
    await verifyWorkspace(options, workspacePath);

    const sourceAfter = await snapshotFileHandle(sourceHandle, maxInputBytes, 'source');
    const currentSource = await snapshotRegularPath(sourcePath, maxInputBytes, 'source');
    const protectedSnapshotBeforePublication = await snapshotRegularPath(snapshotPath, maxInputBytes, 'output');
    if (!sameIdentity(sourceBefore.identity, sourceAfter.identity)
      || !sameIdentity(sourceBefore.identity, currentSource.identity)
      || sourceAfter.sha256 !== sourceBefore.sha256
      || currentSource.sha256 !== sourceBefore.sha256
      || !sameIdentity(protectedSnapshot.identity, protectedSnapshotBeforePublication.identity)
      || protectedSnapshotBeforePublication.sha256 !== sourceBefore.sha256) {
      throw new PdfUnsignedCopyError('SOURCE_CHANGED', 'The signed source changed before unsigned-copy publication.');
    }
    await assertPrivateWorkspace(workspacePath, workspaceIdentity);
    await verifyWorkspace(options, workspacePath);

    // Close the writable handle before publication, then reopen read-only and
    // bind publication to the same inode and hash.
    await outputHandle.sync();
    await outputHandle.close();
    outputHandle = undefined;
    outputHandle = await openRegularNoFollow(outputPath, 'output');
    const finalizedOutput = await snapshotBoundOutput(
      outputPath,
      outputHandle,
      outputAfterValidation.identity,
      maxOutputBytes,
      true,
    );
    if (finalizedOutput.sha256 !== outputAfterValidation.sha256) {
      throw new PdfUnsignedCopyError('OUTPUT_CHANGED', 'The converted PDF changed before publication.');
    }
    publishedIdentity = finalizedOutput.identity;
    try {
      await assertPdfPublicationDirectory(publicationTarget.directoryIdentity);
      await link(outputPath, targetPath);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'EEXIST') {
        throw new PdfUnsignedCopyError('TARGET_EXISTS', 'Create Unsigned Copy will not replace an existing destination.');
      }
      throw new PdfUnsignedCopyError('PUBLICATION_FAILED', 'The validated unsigned copy could not be safely published.');
    }

    try {
      await assertPdfPublicationDirectory(publicationTarget.directoryIdentity);
    } catch {
      throw new PdfUnsignedCopyError('PUBLICATION_FAILED', 'The unsigned-copy destination changed during publication.');
    }

    await assertPublishedTarget(targetPath, outputHandle, publishedIdentity, finalizedOutput.sha256);
    await outputHandle.close();
    outputHandle = undefined;
    await rm(outputPath);
    const finalTarget = await snapshotRegularPath(targetPath, maxOutputBytes, 'output');
    if (!samePublishedFile(publishedIdentity, finalTarget.identity)
      || finalTarget.sha256 !== finalizedOutput.sha256) {
      throw new PdfUnsignedCopyError('PUBLICATION_FAILED', 'The published unsigned copy failed its final verification.');
    }
    const finalSource = await snapshotRegularPath(sourcePath, maxInputBytes, 'source');
    if (!sameIdentity(sourceBefore.identity, finalSource.identity)
      || finalSource.sha256 !== sourceBefore.sha256) {
      throw new PdfUnsignedCopyError('SOURCE_CHANGED', 'The signed source changed during unsigned-copy publication.');
    }
    await syncDirectory(targetDirectory);

    result = {
      engineVersion: conversion.engineVersion,
      inputSha256: sourceBefore.sha256,
      outputSha256: finalizedOutput.sha256,
      bytesWritten: Number(finalizedOutput.identity.size),
      removalPolicyId: conversion.removalPolicyId,
      removalPolicyVersion: conversion.removalPolicyVersion,
      removed: { ...conversion.removed },
      warnings: sanitizeWarnings(conversion.warnings, [
        options.sourcePath,
        options.targetPath,
        sourcePath,
        targetPath,
        workspacePath,
        snapshotPath,
        outputPath,
      ]),
      validatedUnsigned: true,
    };
  } catch (error) {
    operationError = normalizeOperationError(error);
  } finally {
    await sourceHandle?.close().catch(() => undefined);
    await outputHandle?.close().catch(() => undefined);
    if (operationError && publishedIdentity) {
      await removePublishedTargetIfOurs(targetPath, publishedIdentity).catch(() => undefined);
    }
    let cleanupFailed = false;
    if (workspacePath) {
      try {
        await rm(workspacePath, { recursive: true, force: true });
        await syncDirectory(targetDirectory);
      } catch {
        cleanupFailed = true;
      }
    }
    if (cleanupFailed) {
      if (result && publishedIdentity) {
        await removePublishedTargetIfOurs(targetPath, publishedIdentity).catch(() => undefined);
        result = undefined;
      }
      operationError = new PdfUnsignedCopyError(
        'CLEANUP_FAILED',
        'Create Unsigned Copy could not safely clean its private operation workspace.',
      );
    }
  }

  if (operationError) throw operationError;
  if (!result) throw new PdfUnsignedCopyError('PUBLICATION_FAILED', 'The validated unsigned copy was not published.');
  return result;
}

function assertOptions(options: CreatePdfUnsignedCopyOptions): void {
  if (!options || typeof options !== 'object'
    || typeof options.sourcePath !== 'string'
    || typeof options.targetPath !== 'string'
    || typeof options.expectedSourceSha256 !== 'string'
    || !SHA256_PATTERN.test(options.expectedSourceSha256)
    || typeof options.convert !== 'function'
    || typeof options.validate !== 'function'
    || typeof options.verifyStructure !== 'function') {
    throw new PdfUnsignedCopyError('INVALID_REQUEST', 'Create Unsigned Copy requires a complete privileged request.');
  }
}

async function canonicalRegularSource(sourcePath: string): Promise<string> {
  if (!isAbsolute(sourcePath)) throw new PdfUnsignedCopyError('INVALID_REQUEST', 'The signed source path must be absolute.');
  const normalized = resolve(sourcePath);
  try {
    const before = await lstat(normalized);
    if (!before.isFile() || before.isSymbolicLink()) throw unsafeSource();
    const canonical = await realpath(normalized);
    const after = await lstat(canonical);
    if (!after.isFile() || after.isSymbolicLink()) throw unsafeSource();
    return canonical;
  } catch (error) {
    if (error instanceof PdfUnsignedCopyError) throw error;
    throw unsafeSource();
  }
}

async function canonicalNewTarget(targetPath: string): Promise<string> {
  if (!isAbsolute(targetPath)) throw new PdfUnsignedCopyError('INVALID_REQUEST', 'The unsigned-copy destination must be absolute.');
  const normalized = resolve(targetPath);
  const name = basename(normalized);
  if (name.length === 0 || name === '.' || name === '..') throw unsafeTarget();
  let canonicalDirectory: string;
  try {
    canonicalDirectory = await realpath(dirname(normalized));
    const info = await lstat(canonicalDirectory);
    if (!info.isDirectory() || info.isSymbolicLink()) throw unsafeTarget();
  } catch (error) {
    if (error instanceof PdfUnsignedCopyError) throw error;
    throw unsafeTarget();
  }
  const canonicalTarget = join(canonicalDirectory, name);
  try {
    await lstat(canonicalTarget);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return canonicalTarget;
    throw unsafeTarget();
  }
  throw new PdfUnsignedCopyError('TARGET_EXISTS', 'Create Unsigned Copy will not replace an existing destination.');
}

async function openRegularNoFollow(filePath: string, kind: 'source' | 'output'): Promise<FileHandle> {
  try {
    const handle = await open(filePath, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
    const info = await handle.stat({ bigint: true });
    if (!info.isFile()) {
      await handle.close();
      throw kind === 'source' ? unsafeSource() : outputUnsafe();
    }
    return handle;
  } catch (error) {
    if (error instanceof PdfUnsignedCopyError) throw error;
    throw kind === 'source' ? unsafeSource() : outputUnsafe();
  }
}

async function copySourceSnapshot(
  source: FileHandle,
  expectedSource: FileSnapshot,
  snapshotPath: string,
): Promise<void> {
  const snapshot = await open(snapshotPath, 'wx+', 0o600);
  try {
    const hash = createHash('sha256');
    let position = 0;
    for (;;) {
      const bytes = Buffer.allocUnsafe(256 * 1024);
      const read = await source.read(bytes, 0, bytes.byteLength, position);
      if (read.bytesRead === 0) break;
      position += read.bytesRead;
      const chunk = bytes.subarray(0, read.bytesRead);
      hash.update(chunk);
      await writeComplete(snapshot, chunk, position - read.bytesRead);
    }
    await snapshot.sync();
    const sourceAfter = await source.stat({ bigint: true });
    if (!sameIdentity(expectedSource.identity, stableIdentity(sourceAfter))
      || BigInt(position) !== expectedSource.identity.size
      || hash.digest('hex') !== expectedSource.sha256) {
      throw new PdfUnsignedCopyError('SOURCE_CHANGED', 'The signed source changed while its private snapshot was created.');
    }
  } finally {
    await snapshot.close();
  }
  await chmod(snapshotPath, 0o600);
}

async function snapshotBoundOutput(
  outputPath: string,
  outputHandle: FileHandle,
  expectedIdentity: StableFileIdentity,
  maxBytes: number,
  requireUnchangedIdentity: boolean,
): Promise<FileSnapshot> {
  let pathInfo: BigIntStats;
  try {
    pathInfo = await lstat(outputPath, { bigint: true });
  } catch {
    throw outputUnsafe();
  }
  const handleInfo = await outputHandle.stat({ bigint: true });
  if (!pathInfo.isFile()
    || pathInfo.isSymbolicLink()
    || pathInfo.nlink !== 1n
    || handleInfo.nlink !== 1n
    || (process.platform !== 'win32' && ((pathInfo.mode | handleInfo.mode) & 0o077n) !== 0n)
    || pathInfo.dev !== handleInfo.dev
    || pathInfo.ino !== handleInfo.ino
    || (requireUnchangedIdentity && !sameIdentity(expectedIdentity, stableIdentity(handleInfo)))) {
    throw new PdfUnsignedCopyError(
      requireUnchangedIdentity ? 'OUTPUT_CHANGED' : 'OUTPUT_UNSAFE',
      'The converted PDF output was replaced or mutated unexpectedly.',
    );
  }
  return snapshotFileHandle(outputHandle, maxBytes, 'output');
}

async function snapshotRegularPath(
  filePath: string,
  maxBytes: number,
  kind: 'source' | 'output',
): Promise<FileSnapshot> {
  const handle = await openRegularNoFollow(filePath, kind);
  try {
    const pathInfo = await lstat(filePath, { bigint: true });
    const snapshot = await snapshotFileHandle(handle, maxBytes, kind);
    if (pathInfo.isSymbolicLink()
      || !pathInfo.isFile()
      || pathInfo.dev !== snapshot.identity.dev
      || pathInfo.ino !== snapshot.identity.ino
      || (kind === 'output' && pathInfo.nlink !== 1n)
      || (kind === 'output' && process.platform !== 'win32' && (pathInfo.mode & 0o077n) !== 0n)) {
      throw kind === 'source' ? unsafeSource() : outputUnsafe();
    }
    return snapshot;
  } finally {
    await handle.close();
  }
}

async function snapshotFileHandle(
  handle: FileHandle,
  maxBytes: number,
  kind: 'source' | 'output',
): Promise<FileSnapshot> {
  const before = await handle.stat({ bigint: true });
  if (!before.isFile() || before.size < 1n || before.size > BigInt(maxBytes)) {
    throw kind === 'source' ? unsafeSource() : outputUnsafe();
  }
  const hash = createHash('sha256');
  let position = 0;
  for (;;) {
    const bytes = Buffer.allocUnsafe(256 * 1024);
    const read = await handle.read(bytes, 0, bytes.byteLength, position);
    if (read.bytesRead === 0) break;
    position += read.bytesRead;
    hash.update(bytes.subarray(0, read.bytesRead));
    if (position > maxBytes) throw kind === 'source' ? unsafeSource() : outputUnsafe();
  }
  const after = await handle.stat({ bigint: true });
  if (!sameFileStats(before, after) || BigInt(position) !== before.size) {
    throw kind === 'source'
      ? new PdfUnsignedCopyError('SOURCE_CHANGED', 'The signed source changed while it was being verified.')
      : new PdfUnsignedCopyError('OUTPUT_CHANGED', 'The converted PDF changed while it was being verified.');
  }
  return { identity: stableIdentity(before), sha256: hash.digest('hex') };
}

function assertUnsignedValidationReport(report: unknown, outputSha256: string): asserts report is PdfSignatureValidationReport {
  if (!isPdfSignatureValidationReport(report)
    || report.inputSha256 !== outputSha256
    || report.validationMode !== 'offline'
    || report.trust.onlineSourcesUsed !== false
    || report.inventory.presence !== 'unsigned'
    || report.inventory.certificationPermission !== 'not-certified'
    || report.inventory.revisionInventoryComplete !== true
    || report.inventory.modificationPolicyComplete !== true
    || report.inventory.currentRevision === null
    || report.inventory.totalRevisions === null
    || report.inventory.currentRevision !== report.inventory.totalRevisions
    || report.signatures.length !== 0
    || report.inventory.fields.some((field) => field.signed)
    || report.issues.some((issue) => issue.severity === 'error')) {
    throw new PdfUnsignedCopyError(
      'SIGNATURE_VALIDATION_FAILED',
      'The converted PDF did not produce a complete offline unsigned validation report.',
    );
  }
}

function isCleanUnsignedStructure(value: unknown): value is PdfUnsignedCopyStructuralVerification {
  if (!isRecord(value)) return false;
  const keys = [
    'structurallyReadable',
    'byteRangeMarkerCount',
    'signatureDictionaryCount',
    'signedSignatureFieldCount',
    'docMdpReferenceCount',
    'fieldMdpReferenceCount',
    'dssOrVriEntryCount',
  ];
  return Object.keys(value).length === keys.length
    && Object.keys(value).every((key) => keys.includes(key))
    && value.structurallyReadable === true
    && keys.slice(1).every((key) => value[key] === 0);
}

async function secureWorkspace(options: CreatePdfUnsignedCopyOptions, workspacePath: string): Promise<void> {
  if (process.platform === 'win32' && !options.secureWorkspace) {
    throw new PdfUnsignedCopyError(
      'WORKSPACE_SECURITY_FAILED',
      'Windows private-workspace ACL enforcement is unavailable.',
    );
  }
  if (!options.secureWorkspace) return;
  try {
    await options.secureWorkspace(workspacePath);
  } catch {
    throw new PdfUnsignedCopyError(
      'WORKSPACE_SECURITY_FAILED',
      'The private unsigned-copy workspace could not be secured.',
    );
  }
}

async function verifyWorkspace(options: CreatePdfUnsignedCopyOptions, workspacePath: string): Promise<void> {
  if (process.platform === 'win32' && !options.verifyWorkspace) {
    throw new PdfUnsignedCopyError(
      'WORKSPACE_SECURITY_FAILED',
      'Windows private-workspace ACL verification is unavailable.',
    );
  }
  if (!options.verifyWorkspace) return;
  try {
    await options.verifyWorkspace(workspacePath);
  } catch {
    throw new PdfUnsignedCopyError(
      'WORKSPACE_SECURITY_FAILED',
      'The private unsigned-copy workspace failed its security recheck.',
    );
  }
}

function isConversionResult(value: unknown): value is PdfUnsignedCopyConversionResult {
  if (!isRecord(value)) return false;
  const keys = [
    'engineVersion', 'inputSha256', 'outputSha256', 'removalPolicyId', 'removalPolicyVersion', 'removed', 'warnings',
  ];
  return Object.keys(value).length === keys.length
    && Object.keys(value).every((key) => keys.includes(key))
    && typeof value.engineVersion === 'string' && ENGINE_VERSION_PATTERN.test(value.engineVersion)
    && typeof value.inputSha256 === 'string' && SHA256_PATTERN.test(value.inputSha256)
    && typeof value.outputSha256 === 'string' && SHA256_PATTERN.test(value.outputSha256)
    && typeof value.removalPolicyId === 'string' && POLICY_ID_PATTERN.test(value.removalPolicyId)
    && Number.isSafeInteger(value.removalPolicyVersion) && (value.removalPolicyVersion as number) >= 1
    && isRemovalCounts(value.removed)
    && Array.isArray(value.warnings)
    && value.warnings.length <= MAX_WARNINGS
    && value.warnings.every((warning) => typeof warning === 'string' && warning.length <= MAX_WARNING_LENGTH);
}

function isRemovalCounts(value: unknown): value is PdfUnsignedCopyRemovalCounts {
  if (!isRecord(value)) return false;
  const keys = [
    'signatureValues',
    'signatureFields',
    'signatureWidgets',
    'certificationReferences',
    'fieldMdpReferences',
    'validationEvidenceEntries',
  ];
  return Object.keys(value).length === keys.length
    && Object.keys(value).every((key) => keys.includes(key))
    && keys.every((key) => Number.isSafeInteger(value[key]) && (value[key] as number) >= 0 && (value[key] as number) <= MAX_REMOVAL_COUNT);
}

async function assertPublishedTarget(
  targetPath: string,
  outputHandle: FileHandle,
  expectedIdentity: StableFileIdentity,
  expectedSha256: string,
): Promise<void> {
  let targetInfo: BigIntStats;
  try {
    targetInfo = await lstat(targetPath, { bigint: true });
  } catch {
    throw new PdfUnsignedCopyError('PUBLICATION_FAILED', 'The published unsigned copy could not be verified.');
  }
  const outputInfo = await outputHandle.stat({ bigint: true });
  const outputSnapshot = await snapshotFileHandle(outputHandle, Number(expectedIdentity.size), 'output');
  if (!targetInfo.isFile()
    || targetInfo.isSymbolicLink()
    || targetInfo.dev !== expectedIdentity.dev
    || targetInfo.ino !== expectedIdentity.ino
    || outputInfo.dev !== expectedIdentity.dev
    || outputInfo.ino !== expectedIdentity.ino
    || targetInfo.nlink !== 2n
    || outputInfo.nlink !== 2n
    || (process.platform !== 'win32' && ((targetInfo.mode | outputInfo.mode) & 0o077n) !== 0n)
    || outputSnapshot.sha256 !== expectedSha256) {
    throw new PdfUnsignedCopyError('PUBLICATION_FAILED', 'The published unsigned copy failed its final identity check.');
  }
}

async function privateWorkspaceIdentity(workspacePath: string): Promise<StableFileIdentity> {
  let info: BigIntStats;
  try {
    info = await lstat(workspacePath, { bigint: true });
  } catch {
    throw new PdfUnsignedCopyError('OUTPUT_UNSAFE', 'The private unsigned-copy workspace is unavailable.');
  }
  if (!info.isDirectory()
    || info.isSymbolicLink()
    || (process.platform !== 'win32' && (info.mode & 0o077n) !== 0n)) {
    throw new PdfUnsignedCopyError('OUTPUT_UNSAFE', 'The private unsigned-copy workspace is not secure.');
  }
  return stableIdentity(info);
}

async function assertPrivateWorkspace(
  workspacePath: string,
  expectedIdentity: StableFileIdentity,
): Promise<void> {
  const current = await privateWorkspaceIdentity(workspacePath);
  if (current.dev !== expectedIdentity.dev || current.ino !== expectedIdentity.ino) {
    throw new PdfUnsignedCopyError('OUTPUT_UNSAFE', 'The private unsigned-copy workspace was replaced.');
  }
}

async function removePublishedTargetIfOurs(targetPath: string, identity: StableFileIdentity): Promise<void> {
  try {
    const info = await lstat(targetPath, { bigint: true });
    if (info.isFile() && !info.isSymbolicLink() && info.dev === identity.dev && info.ino === identity.ino) {
      await rm(targetPath);
      await syncDirectory(dirname(targetPath));
    }
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
}

async function writeComplete(file: FileHandle, bytes: Buffer, position: number): Promise<void> {
  let offset = 0;
  while (offset < bytes.byteLength) {
    const written = await file.write(bytes, offset, bytes.byteLength - offset, position + offset);
    if (written.bytesWritten < 1) throw new Error('The private source snapshot could not be written completely.');
    offset += written.bytesWritten;
  }
}

async function syncDirectory(directoryPath: string): Promise<void> {
  if (process.platform === 'win32') return;
  const handle = await open(directoryPath, 'r');
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

function sanitizeWarnings(warnings: readonly string[], sensitiveValues: readonly string[]): readonly string[] {
  return warnings.map((warning) => {
    let sanitized = warning
      .replace(/[\u0000-\u001f\u007f]/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
    for (const value of sensitiveValues
      .flatMap((entry) => [entry, basename(entry)])
      .filter((entry) => entry.length >= 3)
      .sort((left, right) => right.length - left.length)) {
      sanitized = sanitized.split(value).join('[redacted]');
    }
    return sanitized.slice(0, MAX_WARNING_LENGTH);
  });
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

function samePublishedFile(left: StableFileIdentity, right: StableFileIdentity): boolean {
  return left.dev === right.dev
    && left.ino === right.ino
    && left.size === right.size;
}

function sameFileStats(left: BigIntStats, right: BigIntStats): boolean {
  return left.dev === right.dev
    && left.ino === right.ino
    && left.size === right.size
    && left.mtimeMs === right.mtimeMs
    && left.ctimeMs === right.ctimeMs;
}

function boundedPositiveInteger(value: number | undefined, fallback: number): number {
  if (value === undefined) return fallback;
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new PdfUnsignedCopyError('INVALID_REQUEST', 'Unsigned-copy file-size limits must be positive safe integers.');
  }
  return value;
}

function normalizeOperationError(error: unknown): PdfUnsignedCopyError {
  if (error instanceof PdfUnsignedCopyError) return error;
  return new PdfUnsignedCopyError('PUBLICATION_FAILED', 'Create Unsigned Copy failed before publication.');
}

function unsafeSource(message = 'Create Unsigned Copy requires a stable regular non-symlink source PDF.'): PdfUnsignedCopyError {
  return new PdfUnsignedCopyError('UNSAFE_SOURCE', message);
}

function unsafeTarget(message = 'Create Unsigned Copy requires a safe new destination in a regular directory.'): PdfUnsignedCopyError {
  return new PdfUnsignedCopyError('UNSAFE_TARGET', message);
}

function outputUnsafe(): PdfUnsignedCopyError {
  return new PdfUnsignedCopyError('OUTPUT_UNSAFE', 'The converter did not produce a stable private regular output file.');
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
