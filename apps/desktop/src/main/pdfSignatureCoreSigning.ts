import { createHash, randomUUID } from 'node:crypto';
import { execFile, spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { constants, type BigIntStats } from 'node:fs';
import { lstat, open, realpath, type FileHandle } from 'node:fs/promises';
import { isAbsolute } from 'node:path';
import {
  resolvePdfSignatureCorePackage,
  type ResolvePdfSignatureCorePackageOptions,
  type VerifiedPdfSignatureCorePackage,
} from './pdfSignatureCorePackage';
import type { PdfSignedMutationPostvalidation } from './pdfSignedMutationWorkflow';

const MAGIC = Buffer.from('BPS2', 'ascii');
const MAX_HEADER_BYTES = 1024 * 1024;
const MAX_FRAME_BYTES = 16 * 1024 * 1024;
const MAX_TOTAL_FRAME_BYTES = 32 * 1024 * 1024;
const MAX_RESPONSE_BYTES = 16 * 1024 * 1024;
const MAX_STDERR_BYTES = 64 * 1024;
const DEFAULT_TIMEOUT_MS = 30_000;
const MAX_TIMEOUT_MS = 5 * 60_000;
const TERMINATION_GRACE_MS = 100;
const TERMINATION_SETTLEMENT_MS = 500;
const REQUEST_ID_PATTERN = /^[A-Za-z0-9._:-]{1,128}$/;

export const PDF_SIGNATURE_PHASE3_CAPABILITIES = Object.freeze({
  certificateSign: false,
  certify: false,
  signatureFieldCreate: false,
  signatureIncrementalWrite: false,
  signedIncrementalEdit: false,
  timestamp: false,
  onlineValidation: false,
});

export type PdfSignatureCoreSigningOperation =
  | 'handshake'
  | 'inspectPkcs12'
  | 'addSignatureField'
  | 'sign'
  | 'certify'
  | 'postvalidateSignedMutation';

export interface PdfSignatureCoreSigningRunOptions {
  readonly signal?: AbortSignal;
  readonly timeoutMs?: number;
}

export interface PdfSignatureCoreSigningClientOptions extends ResolvePdfSignatureCorePackageOptions {
  readonly processEnvironment?: NodeJS.ProcessEnv;
  readonly spawnProcess?: typeof spawn;
  /** Test seam only. Production always resolves and verifies the package. */
  readonly resolvePackage?: typeof resolvePdfSignatureCorePackage;
  /** Experimental proof seam only. Production remains disabled until the external gates pass. */
  readonly allowExperimentalProofOperations?: boolean;
  readonly createRequestId?: () => string;
  /** Test seam only. Production verifies the executable image of the spawned process. */
  readonly verifySpawnedLauncher?: typeof verifySpawnedPdfSignatureCoreLauncher;
}

export interface PdfSignatureCoreFrame {
  readonly id: 'pkcs12' | 'appearance';
  readonly kind: 'pkcs12' | 'appearance';
  readonly bytes: Uint8Array;
  readonly sensitive: boolean;
}

export interface PdfSignatureCorePkcs12Identity {
  readonly certificateSha256: string;
  readonly subject: string;
  readonly issuer: string;
  readonly serialNumber: string;
  readonly validFrom: string;
  readonly validTo: string;
  readonly keyAlgorithm: string;
  readonly keyBits: number;
  readonly chainSha256: readonly string[];
  readonly supportedDigests: readonly ('SHA-256' | 'SHA-384' | 'SHA-512')[];
  readonly hasPrivateKey: true;
}

export interface PdfSignatureCorePkcs12Inspection {
  readonly provider: 'pkcs12';
  readonly identities: readonly PdfSignatureCorePkcs12Identity[];
  readonly passwordRemembered: false;
  readonly privateKeyExported: false;
  readonly engineVersion: string;
}

export interface PdfSignatureFieldWidget {
  readonly pageIndex: number;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly pageRotation: 0 | 90 | 180 | 270;
  readonly coordinateSpace: 'unrotated-pdf-default-user-space';
}

export type PdfSignatureFieldSpec = {
  readonly kind: 'existing';
  readonly name: string;
} | {
  readonly kind: 'new';
  readonly name: string;
  readonly widget: PdfSignatureFieldWidget | null;
  readonly lock?: {
    readonly action: 'all' | 'include' | 'exclude';
    readonly fieldNames: readonly string[];
  };
};

export interface PdfSignatureCoreAddFieldPayload {
  readonly inputPath: string;
  readonly outputPath: string;
  readonly expectedInputSha256: string;
  readonly field: Exclude<PdfSignatureFieldSpec, { readonly kind: 'existing' }>;
}

export interface PdfSignatureCoreSignPayload {
  readonly inputPath: string;
  readonly outputPath: string;
  readonly expectedInputSha256: string;
  readonly certificateSha256: string;
  readonly digestAlgorithm: 'SHA-256' | 'SHA-384' | 'SHA-512';
  readonly profile: 'PAdES-B-B';
  readonly field: PdfSignatureFieldSpec;
  readonly reason?: string;
  readonly location?: string;
  readonly contact?: string;
  readonly appearance: 'visible' | 'invisible';
}

export interface PdfSignatureCoreCertifyPayload extends PdfSignatureCoreSignPayload {
  readonly certificationPermission:
    | 'no-changes'
    | 'form-filling-and-signatures'
    | 'form-filling-signatures-and-annotations';
}

export interface PdfSignatureCorePostvalidateSignedMutationPayload {
  readonly inputPath: string;
  readonly outputPath: string;
  readonly expectedInputSha256: string;
  readonly expectedOutputSha256: string;
  readonly expectedCertificateSha256: string;
  readonly expectedFieldName: string;
  readonly expectedOperation: 'approval' | 'certification';
  readonly expectedAppearance: 'visible' | 'invisible';
  readonly expectedCertificationPermission?: PdfSignatureCoreCertifyPayload['certificationPermission'];
}

export const PDF_SIGNATURE_PHASE3_INDEPENDENT_POSTVALIDATION_OPERATION = 'postvalidateSignedMutation' as const;

interface FrameDescriptor {
  readonly id: PdfSignatureCoreFrame['id'];
  readonly kind: PdfSignatureCoreFrame['kind'];
  readonly byteLength: number;
  readonly sha256: string;
  readonly sensitive: boolean;
}

interface FramedRequestHeader {
  readonly protocolVersion: 2;
  readonly requestId: string;
  readonly operation: PdfSignatureCoreSigningOperation;
  readonly payload: Record<string, unknown>;
  readonly frames: readonly FrameDescriptor[];
}

export class PdfSignatureCoreSigningError extends Error {
  constructor(
    readonly code: 'CANCELLED' | 'TIMEOUT' | 'LAUNCH_FAILED' | 'PROCESS_CRASHED' | 'PROTOCOL_ERROR' | 'ENGINE_ERROR' | 'CAPABILITY_DISABLED',
    message: string,
    readonly engineCode?: string,
  ) {
    super(message);
    this.name = 'PdfSignatureCoreSigningError';
  }
}

/**
 * Argument-free, one-request-per-process client for protocol v2. Passwords are
 * intentionally absent from every public type; the sidecar prompts for them.
 */
export class PdfSignatureCoreSigningClient {
  private readonly spawnProcess: typeof spawn;
  private readonly createRequestId: () => string;

  constructor(private readonly options: PdfSignatureCoreSigningClientOptions) {
    this.spawnProcess = options.spawnProcess ?? spawn;
    this.createRequestId = options.createRequestId ?? randomUUID;
  }

  async handshake(options: PdfSignatureCoreSigningRunOptions = {}): Promise<Record<string, unknown>> {
    const result = await this.run('handshake', {}, [], options);
    if (!isHandshakeResult(result)) {
      throw protocolError();
    }
    return result;
  }

  async inspectPkcs12(
    pkcs12Frame: Uint8Array,
    options: PdfSignatureCoreSigningRunOptions = {},
  ): Promise<PdfSignatureCorePkcs12Inspection> {
    const result = await this.run('inspectPkcs12', {}, [{
      id: 'pkcs12',
      kind: 'pkcs12',
      bytes: pkcs12Frame,
      sensitive: true,
    }], options);
    if (!isPkcs12Inspection(result)) throw protocolError();
    return result;
  }

  async addSignatureField(
    payload: PdfSignatureCoreAddFieldPayload,
    options: PdfSignatureCoreSigningRunOptions = {},
  ): Promise<Record<string, unknown>> {
    this.assertExperimentalProofEnabled();
    return this.run('addSignatureField', payload as unknown as Record<string, unknown>, [], options);
  }

  async sign(
    payload: PdfSignatureCoreSignPayload,
    pkcs12Frame: Uint8Array,
    appearanceFrame?: Uint8Array,
    options: PdfSignatureCoreSigningRunOptions = {},
  ): Promise<Record<string, unknown>> {
    this.assertExperimentalProofEnabled();
    const verifiedPackage = await this.resolveVerifiedPackage();
    await this.assertSigningCapabilities(verifiedPackage, options);
    const frames: PdfSignatureCoreFrame[] = [{ id: 'pkcs12', kind: 'pkcs12', bytes: pkcs12Frame, sensitive: true }];
    if (appearanceFrame) frames.push({ id: 'appearance', kind: 'appearance', bytes: appearanceFrame, sensitive: true });
    return this.run('sign', payload as unknown as Record<string, unknown>, frames, options, verifiedPackage);
  }

  async certify(
    payload: PdfSignatureCoreCertifyPayload,
    pkcs12Frame: Uint8Array,
    appearanceFrame?: Uint8Array,
    options: PdfSignatureCoreSigningRunOptions = {},
  ): Promise<Record<string, unknown>> {
    this.assertExperimentalProofEnabled();
    const verifiedPackage = await this.resolveVerifiedPackage();
    await this.assertSigningCapabilities(verifiedPackage, options);
    const frames: PdfSignatureCoreFrame[] = [{ id: 'pkcs12', kind: 'pkcs12', bytes: pkcs12Frame, sensitive: true }];
    if (appearanceFrame) frames.push({ id: 'appearance', kind: 'appearance', bytes: appearanceFrame, sensitive: true });
    return this.run('certify', payload as unknown as Record<string, unknown>, frames, options, verifiedPackage);
  }

  async postvalidateSignedMutation(
    payload: PdfSignatureCorePostvalidateSignedMutationPayload,
    options: PdfSignatureCoreSigningRunOptions = {},
  ): Promise<PdfSignedMutationPostvalidation> {
    this.assertExperimentalProofEnabled();
    const result = await this.run(
      PDF_SIGNATURE_PHASE3_INDEPENDENT_POSTVALIDATION_OPERATION,
      payload as unknown as Record<string, unknown>,
      [],
      options,
    );
    if (!isSignedMutationPostvalidation(result, payload)) throw protocolError();
    return result;
  }

  private assertExperimentalProofEnabled(): void {
    if (this.options.allowExperimentalProofOperations !== true) {
      throw new PdfSignatureCoreSigningError(
        'CAPABILITY_DISABLED',
        'PDF certificate signing remains disabled until its external gates pass.',
      );
    }
  }

  private async resolveVerifiedPackage(): Promise<VerifiedPdfSignatureCorePackage> {
    return (this.options.resolvePackage ?? resolvePdfSignatureCorePackage)(this.options);
  }

  private async assertSigningCapabilities(
    verifiedPackage: VerifiedPdfSignatureCorePackage,
    options: PdfSignatureCoreSigningRunOptions,
  ): Promise<void> {
    const handshake = await this.run('handshake', {}, [], options, verifiedPackage);
    if (!isHandshakeResult(handshake) || !isRecord(handshake.versions)
      || handshake.versions.engine !== verifiedPackage.manifest.engineVersion
      || handshake.versions.java !== verifiedPackage.manifest.javaVersion
      || !isRecord(handshake.capabilities)
      || handshake.capabilities.certificateSign !== true
      || handshake.capabilities.certify !== true
      || handshake.capabilities.signatureFieldCreate !== true
      || handshake.capabilities.signatureIncrementalWrite !== true
      || Boolean(verifiedPackage.postSignInventory?.releaseSealed) !== true) {
      throw new PdfSignatureCoreSigningError(
        'CAPABILITY_DISABLED',
        'The verified PDF signing package is not an approved release-sealed signing package.',
      );
    }
  }

  private async run(
    operation: PdfSignatureCoreSigningOperation,
    payload: Record<string, unknown>,
    frames: readonly PdfSignatureCoreFrame[],
    options: PdfSignatureCoreSigningRunOptions,
    verifiedPackage?: VerifiedPdfSignatureCorePackage,
  ): Promise<Record<string, unknown>> {
    assertOperationPayload(operation, payload);
    const requestId = this.createRequestId();
    if (!REQUEST_ID_PATTERN.test(requestId)) throw new TypeError('The PDF signing request ID is invalid.');
    const encoded = encodePdfSignatureCoreV2Request({ requestId, operation, payload, frames });
    try {
      const packageToRun = verifiedPackage
        ?? await (this.options.resolvePackage ?? resolvePdfSignatureCorePackage)(this.options);
      const launcher = await verifyPdfSignatureCoreLauncherBinding(packageToRun);
      const responseBytes = await runShortLivedSidecar({
        launcher,
        environment: strictSidecarEnvironment(this.options.processEnvironment ?? process.env),
        spawnProcess: this.spawnProcess,
        requestBytes: encoded,
        signal: options.signal,
        timeoutMs: boundedTimeout(options.timeoutMs),
        verifySpawnedLauncher: this.options.verifySpawnedLauncher ?? verifySpawnedPdfSignatureCoreLauncher,
      });
      return decodePdfSignatureCoreV2Response(responseBytes, requestId, operation);
    } finally {
      encoded.fill(0);
    }
  }
}

export function adaptPdfSignatureCoreSigningResult(
  value: unknown,
  expected: {
    readonly inputSha256: string;
    readonly outputSha256: string;
    readonly fieldName: string;
    readonly certificateSha256: string;
    readonly kind: 'approval' | 'certification';
    readonly digestAlgorithm: 'SHA-256' | 'SHA-384' | 'SHA-512';
  },
): {
  readonly inputSha256: string;
  readonly outputSha256: string;
  readonly fieldName: string;
  readonly incrementalUpdate: true;
  readonly inputPrefixPreserved: true;
} {
  if (!isRecord(value)) throw protocolError();
  try {
    assertExactKeys(value, [
      'appendOnly', 'certificateSha256', 'digestAlgorithm', 'engineVersion', 'fieldName',
      'inputSha256', 'kind', 'outputBytes', 'outputSha256', 'postcheck', 'profile',
      'sourcePreserved', 'validatedOutput',
    ]);
  } catch {
    throw protocolError();
  }
  if (value.inputSha256 !== expected.inputSha256 || value.outputSha256 !== expected.outputSha256
    || value.fieldName !== expected.fieldName || value.certificateSha256 !== expected.certificateSha256
    || value.kind !== expected.kind || value.digestAlgorithm !== expected.digestAlgorithm
    || value.profile !== 'PAdES-B-B' || value.sourcePreserved !== true
    || value.validatedOutput !== true || value.appendOnly !== true
    || !Number.isSafeInteger(value.outputBytes) || (value.outputBytes as number) < 1
    || !boundedText(value.engineVersion, 128) || !validSigningPostcheck(value.postcheck, expected)) {
    throw protocolError();
  }
  return {
    inputSha256: expected.inputSha256,
    outputSha256: expected.outputSha256,
    fieldName: expected.fieldName,
    incrementalUpdate: true,
    inputPrefixPreserved: true,
  };
}

function validSigningPostcheck(
  value: unknown,
  expected: { readonly fieldName: string; readonly certificateSha256: string; readonly kind: 'approval' | 'certification' },
): boolean {
  if (!isRecord(value)) return false;
  try {
    assertExactKeys(value, [
      'advancedSignatureBindingExact', 'appendOnly', 'appendedBytes', 'certificateSha256',
      'certificationPermission', 'contentsGapExact', 'cryptographicIntegrity', 'fieldBindingExact',
      'fieldName', 'newSignatureByteRange', 'outputSignatureCount', 'priorSignatureCount',
      'sourceBytesPreserved', 'wholeRevisionCovered',
    ]);
  } catch {
    return false;
  }
  return value.appendOnly === true && value.advancedSignatureBindingExact === true
    && value.contentsGapExact === true && value.fieldBindingExact === true
    && value.wholeRevisionCovered === true && value.cryptographicIntegrity === 'intact'
    && value.fieldName === expected.fieldName && value.certificateSha256 === expected.certificateSha256
    && boundedNonnegative(value.appendedBytes) && boundedNonnegative(value.priorSignatureCount)
    && boundedNonnegative(value.outputSignatureCount) && boundedNonnegative(value.sourceBytesPreserved)
    && value.outputSignatureCount === (value.priorSignatureCount as number) + 1
    && Array.isArray(value.newSignatureByteRange) && value.newSignatureByteRange.length === 4
    && value.newSignatureByteRange.every(boundedNonnegative)
    && (expected.kind === 'approval'
      ? value.certificationPermission === null
      : value.certificationPermission === 1 || value.certificationPermission === 2 || value.certificationPermission === 3);
}

export function encodePdfSignatureCoreV2Request(options: {
  readonly requestId: string;
  readonly operation: PdfSignatureCoreSigningOperation;
  readonly payload: Record<string, unknown>;
  readonly frames: readonly PdfSignatureCoreFrame[];
}): Buffer {
  if (!REQUEST_ID_PATTERN.test(options.requestId)) throw new TypeError('The PDF signing request ID is invalid.');
  assertOperationPayload(options.operation, options.payload);
  assertOperationFrames(options.operation, options.payload, options.frames);
  if (options.frames.length > 3) throw new TypeError('The PDF signing request has too many frames.');
  const seen = new Set<string>();
  let totalFrameBytes = 0;
  const frameCopies: Buffer[] = [];
  try {
    const descriptors = options.frames.map((frame): FrameDescriptor => {
      if (frame.id !== frame.kind || (frame.id !== 'pkcs12' && frame.id !== 'appearance') || seen.has(frame.id)) {
        throw new TypeError('The PDF signing request frame declaration is invalid.');
      }
      seen.add(frame.id);
      if (!(frame.bytes instanceof Uint8Array) || frame.bytes.byteLength < 1 || frame.bytes.byteLength > MAX_FRAME_BYTES) {
        throw new TypeError('The PDF signing request frame size is invalid.');
      }
      if (frame.id === 'pkcs12' && frame.sensitive !== true) {
        throw new TypeError('The PKCS#12 frame must be marked sensitive.');
      }
      if (frame.id === 'appearance' && frame.sensitive !== true) {
        throw new TypeError('The appearance frame must be marked sensitive.');
      }
      totalFrameBytes += frame.bytes.byteLength;
      if (totalFrameBytes > MAX_TOTAL_FRAME_BYTES) throw new TypeError('The PDF signing request frames exceed their total limit.');
      const copy = Buffer.from(frame.bytes);
      frameCopies.push(copy);
      return {
        id: frame.id,
        kind: frame.kind,
        byteLength: copy.byteLength,
        sha256: createHash('sha256').update(copy).digest('hex'),
        sensitive: frame.sensitive,
      };
    });
    const header: FramedRequestHeader = {
      protocolVersion: 2,
      requestId: options.requestId,
      operation: options.operation,
      payload: options.payload,
      frames: descriptors,
    };
    const headerBytes = Buffer.from(JSON.stringify(header), 'utf8');
    if (headerBytes.byteLength > MAX_HEADER_BYTES) throw new TypeError('The PDF signing request header exceeds its limit.');
    const total = MAGIC.byteLength + 4 + headerBytes.byteLength
      + frameCopies.reduce((sum, frame) => sum + 4 + frame.byteLength, 0);
    const output = Buffer.allocUnsafe(total);
    let offset = 0;
    MAGIC.copy(output, offset);
    offset += MAGIC.byteLength;
    output.writeUInt32BE(headerBytes.byteLength, offset);
    offset += 4;
    headerBytes.copy(output, offset);
    offset += headerBytes.byteLength;
    for (const frame of frameCopies) {
      output.writeUInt32BE(frame.byteLength, offset);
      offset += 4;
      frame.copy(output, offset);
      offset += frame.byteLength;
    }
    return output;
  } finally {
    for (const frame of frameCopies) frame.fill(0);
  }
}

export function decodePdfSignatureCoreV2Response(
  bytes: Uint8Array,
  expectedRequestId: string,
  expectedOperation: PdfSignatureCoreSigningOperation,
): Record<string, unknown> {
  const input = Buffer.from(bytes);
  if (input.byteLength < 8 || !input.subarray(0, 4).equals(MAGIC)) throw protocolError();
  const length = input.readUInt32BE(4);
  if (length < 2 || length > MAX_RESPONSE_BYTES || input.byteLength !== 8 + length) throw protocolError();
  let value: unknown;
  try {
    const text = new TextDecoder('utf-8', { fatal: true }).decode(input.subarray(8));
    value = JSON.parse(text);
  } catch {
    throw protocolError();
  }
  if (!isRecord(value)
    || value.protocolVersion !== 2
    || value.requestId !== expectedRequestId
    || value.operation !== expectedOperation
    || (value.event !== 'result' && value.event !== 'error')) {
    throw protocolError();
  }
  if (value.event === 'error') {
    if (!hasExactKeys(value, ['engineVersion', 'error', 'event', 'ok', 'operation', 'protocolVersion', 'requestId'])
      || value.ok !== false || !boundedText(value.engineVersion, 128)
      || !isRecord(value.error) || !hasExactKeys(value.error, ['code', 'message'])
      || !boundedText(value.error.message, 512)) throw protocolError();
    const engineCode = typeof value.error.code === 'string'
      && /^[A-Z][A-Z0-9_]{0,127}$/.test(value.error.code)
      ? value.error.code
      : undefined;
    if (!engineCode) throw protocolError();
    throw new PdfSignatureCoreSigningError('ENGINE_ERROR', 'The PDF signing engine rejected the request.', engineCode);
  }
  if (!hasExactKeys(value, ['engineVersion', 'event', 'ok', 'operation', 'protocolVersion', 'requestId', 'result'])
    || value.ok !== true || !boundedText(value.engineVersion, 128) || !isRecord(value.result)) throw protocolError();
  return value.result;
}

async function runShortLivedSidecar(options: {
  readonly launcher: VerifiedPdfSignatureCoreLauncher;
  readonly environment?: NodeJS.ProcessEnv;
  readonly spawnProcess: typeof spawn;
  readonly requestBytes: Buffer;
  readonly signal?: AbortSignal;
  readonly timeoutMs: number;
  readonly verifySpawnedLauncher: typeof verifySpawnedPdfSignatureCoreLauncher;
}): Promise<Buffer> {
  if (options.signal?.aborted) throw cancelledError();
  let child: ChildProcessWithoutNullStreams;
  try {
    child = options.spawnProcess(options.launcher.path, [], {
      env: options.environment,
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
    });
  } catch {
    throw new PdfSignatureCoreSigningError('LAUNCH_FAILED', 'The PDF signing engine could not be started.');
  }

  let beginInput: (() => void) | undefined;
  let abandonBeforeInput: ((error: PdfSignatureCoreSigningError) => void) | undefined;
  let canBeginInput: (() => boolean) | undefined;
  const completion = new Promise<Buffer>((resolvePromise, rejectPromise) => {
    const stdout: Buffer[] = [];
    let stdoutBytes = 0;
    let stderrBytes = 0;
    let settled = false;
    let forcedError: PdfSignatureCoreSigningError | undefined;
    let forceTimer: ReturnType<typeof setTimeout> | undefined;
    let settlementTimer: ReturnType<typeof setTimeout> | undefined;
    const timer = setTimeout(() => {
      forcedError = new PdfSignatureCoreSigningError('TIMEOUT', 'The PDF signing engine timed out.');
      forceTermination(forcedError);
    }, options.timeoutMs);
    const aborted = () => {
      forceTermination(cancelledError());
    };
    const cleanup = () => {
      clearTimeout(timer);
      if (forceTimer) clearTimeout(forceTimer);
      if (settlementTimer) clearTimeout(settlementTimer);
      options.signal?.removeEventListener('abort', aborted);
      child.stdin.destroy();
      child.stdout.destroy();
      child.stderr.destroy();
      child.unref();
    };
    const rejectOnce = (error: PdfSignatureCoreSigningError) => {
      if (settled) return;
      settled = true;
      cleanup();
      rejectPromise(error);
    };
    abandonBeforeInput = rejectOnce;
    canBeginInput = () => !settled && forcedError === undefined;
    const forceTermination = (error: PdfSignatureCoreSigningError) => {
      if (settled) return;
      forcedError = error;
      child.kill('SIGTERM');
      forceTimer ??= setTimeout(() => child.kill('SIGKILL'), TERMINATION_GRACE_MS);
      settlementTimer ??= setTimeout(() => rejectOnce(error), TERMINATION_SETTLEMENT_MS);
    };
    options.signal?.addEventListener('abort', aborted, { once: true });
    child.once('error', () => rejectOnce(new PdfSignatureCoreSigningError('LAUNCH_FAILED', 'The PDF signing engine could not be started.')));
    child.stdout.on('data', (chunk: Buffer | string) => {
      const copy = Buffer.from(chunk);
      stdoutBytes += copy.byteLength;
      if (stdoutBytes > MAX_RESPONSE_BYTES + 8) {
        forceTermination(protocolError());
        return;
      }
      stdout.push(copy);
    });
    child.stderr.on('data', (chunk: Buffer | string) => {
      stderrBytes += Buffer.byteLength(chunk);
      if (stderrBytes > MAX_STDERR_BYTES) {
        forceTermination(protocolError());
      }
    });
    child.once('close', (code, signal) => {
      if (settled) return;
      settled = true;
      cleanup();
      if (forcedError) {
        rejectPromise(forcedError);
      } else if (code !== 0 || signal !== null) {
        rejectPromise(new PdfSignatureCoreSigningError('PROCESS_CRASHED', 'The PDF signing engine stopped unexpectedly.'));
      } else {
        resolvePromise(Buffer.concat(stdout, stdoutBytes));
      }
    });
    child.stdin.once('error', () => {
      forceTermination(new PdfSignatureCoreSigningError('PROCESS_CRASHED', 'The PDF signing engine stopped unexpectedly.'));
    });
    beginInput = () => child.stdin.end(options.requestBytes);
  });
  // Image verification is asynchronous and can outlive an early timeout or
  // child error. Install a rejection observer immediately; callers still await
  // the original promise below.
  void completion.catch(() => undefined);
  try {
    await options.verifySpawnedLauncher(child, options.launcher);
    await assertPdfSignatureCoreLauncherBindingUnchanged(options.launcher);
    if (options.signal?.aborted) throw cancelledError();
    if (!canBeginInput!()) return completion;
    beginInput!();
  } catch (error) {
    // The process completion promise was installed before the post-spawn
    // launcher/abort gate. Mark its eventual close rejection handled while the
    // gate error remains the authoritative failure.
    child.kill('SIGKILL');
    abandonBeforeInput!(error instanceof PdfSignatureCoreSigningError ? error : launcherUnsafe());
    throw error;
  }
  return completion;
}

export interface VerifiedPdfSignatureCoreLauncher {
  readonly path: string;
  readonly dev: bigint;
  readonly ino: bigint;
  readonly size: bigint;
  readonly mtimeNs: bigint;
  readonly ctimeNs: bigint;
  readonly sha256: string;
}

export async function verifyPdfSignatureCoreLauncherBinding(
  pkg: VerifiedPdfSignatureCorePackage,
): Promise<VerifiedPdfSignatureCoreLauncher> {
  const component = pkg.manifest.components.find((candidate) => candidate.path === pkg.manifest.launcher);
  if (!component) throw new PdfSignatureCoreSigningError('LAUNCH_FAILED', 'The verified PDF signing launcher is unavailable.');
  const observed = await inspectLauncher(pkg.launcherPath);
  if (observed.size !== BigInt(component.size) || observed.sha256 !== component.sha256) {
    throw new PdfSignatureCoreSigningError('LAUNCH_FAILED', 'The verified PDF signing launcher changed.');
  }
  return observed;
}

export async function assertPdfSignatureCoreLauncherBindingUnchanged(
  expected: VerifiedPdfSignatureCoreLauncher,
): Promise<void> {
  const observed = await inspectLauncher(expected.path);
  if (!sameLauncherIdentity(expected, observed) || expected.sha256 !== observed.sha256) {
    throw new PdfSignatureCoreSigningError('LAUNCH_FAILED', 'The verified PDF signing launcher changed.');
  }
}

export async function verifySpawnedPdfSignatureCoreLauncher(
  child: ChildProcessWithoutNullStreams,
  expected: VerifiedPdfSignatureCoreLauncher,
): Promise<void> {
  if (!Number.isSafeInteger(child.pid) || (child.pid as number) < 1) throw launcherUnsafe();
  if (process.platform === 'linux') {
    const observed = await inspectOpenExecutable(`/proc/${child.pid}/exe`);
    if (!sameLauncherIdentity(expected, observed) || expected.sha256 !== observed.sha256) throw launcherUnsafe();
    return;
  }
  if (process.platform === 'darwin') {
    const mappings = await macOsTextMappings(child.pid as number);
    if (!mappings.some((mapping) => mapping.path === expected.path
      && mapping.dev === expected.dev && mapping.ino === expected.ino)) throw launcherUnsafe();
    return;
  }
  // Node exposes no Windows API that binds a ChildProcess to the executable
  // file object which created it. A pathname recheck is not sufficient if a
  // running image can be renamed and the verified path restored. Remain
  // fail-closed until the native Windows process-image verifier is packaged.
  throw launcherUnsafe();
}

async function inspectOpenExecutable(path: string): Promise<VerifiedPdfSignatureCoreLauncher> {
  let handle: FileHandle | undefined;
  try {
    handle = await open(path, constants.O_RDONLY);
    const before = await handle.stat({ bigint: true });
    if (!before.isFile() || before.nlink < 1n) throw launcherUnsafe();
    const digest = createHash('sha256');
    const buffer = Buffer.allocUnsafe(64 * 1024);
    let offset = 0;
    try {
      while (offset < Number(before.size)) {
        const read = await handle.read(buffer, 0, Math.min(buffer.byteLength, Number(before.size) - offset), offset);
        if (read.bytesRead === 0) break;
        digest.update(buffer.subarray(0, read.bytesRead));
        offset += read.bytesRead;
      }
    } finally {
      buffer.fill(0);
    }
    const after = await handle.stat({ bigint: true });
    if (offset !== Number(before.size) || !sameLauncherStats(before, after)) throw launcherUnsafe();
    return {
      path,
      dev: before.dev,
      ino: before.ino,
      size: before.size,
      mtimeNs: before.mtimeNs,
      ctimeNs: before.ctimeNs,
      sha256: digest.digest('hex'),
    };
  } catch (error) {
    if (error instanceof PdfSignatureCoreSigningError) throw error;
    throw launcherUnsafe();
  } finally {
    await handle?.close().catch(() => undefined);
  }
}

interface MacOsTextMapping {
  readonly path: string;
  readonly dev: bigint;
  readonly ino: bigint;
}

async function macOsTextMappings(pid: number): Promise<readonly MacOsTextMapping[]> {
  const output = await new Promise<string>((resolvePromise, rejectPromise) => {
    execFile('/usr/sbin/lsof', ['-a', '-p', String(pid), '-d', 'txt', '-F', 'Dfin'], {
      encoding: 'utf8', maxBuffer: 1024 * 1024, timeout: 5_000, windowsHide: true,
    }, (error, stdout) => {
      if (error) rejectPromise(launcherUnsafe());
      else resolvePromise(stdout);
    });
  });
  const mappings: MacOsTextMapping[] = [];
  let current: Partial<{ path: string; dev: bigint; ino: bigint }> = {};
  const finish = () => {
    if (current.path !== undefined && current.dev !== undefined && current.ino !== undefined) {
      mappings.push(current as MacOsTextMapping);
    }
    current = {};
  };
  for (const line of output.split('\n')) {
    if (line.startsWith('f')) {
      finish();
    } else if (line.startsWith('D')) {
      const value = line.slice(1);
      if (/^(?:0x)?[0-9a-f]+$/i.test(value)) current.dev = BigInt(value.startsWith('0x') ? value : `0x${value}`);
    } else if (line.startsWith('i') && /^\d+$/.test(line.slice(1))) {
      current.ino = BigInt(line.slice(1));
    } else if (line.startsWith('n')) {
      current.path = line.slice(1);
    }
  }
  finish();
  return mappings;
}

async function inspectLauncher(path: string): Promise<VerifiedPdfSignatureCoreLauncher> {
  const pathInfo = await lstat(path, { bigint: true }).catch(() => { throw launcherUnsafe(); });
  if (!pathInfo.isFile() || pathInfo.isSymbolicLink() || pathInfo.nlink !== 1n) throw launcherUnsafe();
  const canonical = await realpath(path).catch(() => { throw launcherUnsafe(); });
  if (canonical !== path) throw launcherUnsafe();
  let handle: FileHandle | undefined;
  try {
    handle = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
    const before = await handle.stat({ bigint: true });
    if (!before.isFile() || before.nlink !== 1n || !sameLauncherStats(pathInfo, before)) throw launcherUnsafe();
    const digest = createHash('sha256');
    const buffer = Buffer.allocUnsafe(64 * 1024);
    let offset = 0;
    try {
      while (offset < Number(before.size)) {
        const read = await handle.read(buffer, 0, Math.min(buffer.byteLength, Number(before.size) - offset), offset);
        if (read.bytesRead === 0) break;
        digest.update(buffer.subarray(0, read.bytesRead));
        offset += read.bytesRead;
      }
    } finally {
      buffer.fill(0);
    }
    const after = await handle.stat({ bigint: true });
    if (offset !== Number(before.size) || !sameLauncherStats(before, after)) throw launcherUnsafe();
    return {
      path,
      dev: before.dev,
      ino: before.ino,
      size: before.size,
      mtimeNs: before.mtimeNs,
      ctimeNs: before.ctimeNs,
      sha256: digest.digest('hex'),
    };
  } catch (error) {
    if (error instanceof PdfSignatureCoreSigningError) throw error;
    throw launcherUnsafe();
  } finally {
    await handle?.close().catch(() => { throw launcherUnsafe(); });
  }
}

function sameLauncherStats(left: BigIntStats, right: BigIntStats): boolean {
  return left.dev === right.dev && left.ino === right.ino && left.size === right.size
    && left.mtimeNs === right.mtimeNs && left.ctimeNs === right.ctimeNs;
}

function sameLauncherIdentity(left: VerifiedPdfSignatureCoreLauncher, right: VerifiedPdfSignatureCoreLauncher): boolean {
  return left.dev === right.dev && left.ino === right.ino && left.size === right.size
    && left.mtimeNs === right.mtimeNs && left.ctimeNs === right.ctimeNs;
}

function launcherUnsafe(): PdfSignatureCoreSigningError {
  return new PdfSignatureCoreSigningError('LAUNCH_FAILED', 'The verified PDF signing launcher is unsafe.');
}

function strictSidecarEnvironment(source: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const allowed = [
    'PATH', 'SystemRoot', 'WINDIR', 'TEMP', 'TMP', 'LANG', 'LC_ALL',
    'DISPLAY', 'WAYLAND_DISPLAY', 'XAUTHORITY',
  ] as const;
  const output: NodeJS.ProcessEnv = {
    BP_SIGNATURE_NETWORK_DISABLED: '1',
    NO_PROXY: '*',
  };
  for (const key of allowed) {
    const value = source[key];
    if (value !== undefined && value.length <= 16_384 && !value.includes('\0')) output[key] = value;
  }
  return output;
}

function assertOperationPayload(operation: PdfSignatureCoreSigningOperation, payload: Record<string, unknown>): void {
  if (!isRecord(payload)) throw invalidPayload();
  if (operation === 'handshake' || operation === 'inspectPkcs12') {
    assertExactKeys(payload, []);
    return;
  }
  if (operation === 'addSignatureField') {
    assertExactKeys(payload, ['expectedInputSha256', 'field', 'inputPath', 'outputPath']);
    assertPath(payload.inputPath);
    assertPath(payload.outputPath);
    assertSha(payload.expectedInputSha256);
    assertField(payload.field, false);
    return;
  }
  if (operation === 'postvalidateSignedMutation') {
    assertExactKeys(payload, [
      'expectedCertificateSha256', 'expectedFieldName', 'expectedInputSha256',
      'expectedOutputSha256', 'expectedOperation', 'expectedAppearance', 'inputPath', 'outputPath',
    ], ['expectedCertificationPermission']);
    if (payload.expectedOperation !== 'approval' && payload.expectedOperation !== 'certification') throw invalidPayload();
    if (payload.expectedAppearance !== 'visible' && payload.expectedAppearance !== 'invisible') throw invalidPayload();
    if (payload.expectedOperation === 'certification') {
      if (payload.expectedCertificationPermission !== 'no-changes'
        && payload.expectedCertificationPermission !== 'form-filling-and-signatures'
        && payload.expectedCertificationPermission !== 'form-filling-signatures-and-annotations') throw invalidPayload();
    } else if (Object.hasOwn(payload, 'expectedCertificationPermission')) {
      throw invalidPayload();
    }
    assertPath(payload.inputPath);
    assertPath(payload.outputPath);
    assertSha(payload.expectedInputSha256);
    assertSha(payload.expectedOutputSha256);
    assertSha(payload.expectedCertificateSha256);
    assertFieldName(payload.expectedFieldName, 512, true);
    return;
  }
  const certification = operation === 'certify';
  const required = [
    'appearance', 'certificateSha256', 'digestAlgorithm', 'expectedInputSha256',
    'field', 'inputPath', 'outputPath', 'profile',
  ];
  const optional = ['contact', 'location', 'reason'];
  if (certification) required.push('certificationPermission');
  assertExactKeys(payload, required, optional);
  assertPath(payload.inputPath);
  assertPath(payload.outputPath);
  assertSha(payload.expectedInputSha256);
  assertSha(payload.certificateSha256);
  if (payload.digestAlgorithm !== 'SHA-256' && payload.digestAlgorithm !== 'SHA-384' && payload.digestAlgorithm !== 'SHA-512') throw invalidPayload();
  if (payload.profile !== 'PAdES-B-B') throw invalidPayload();
  assertField(payload.field, true);
  for (const [key, max] of [['reason', 1024], ['location', 512], ['contact', 512]] as const) {
    if (payload[key] !== undefined) assertText(payload[key], max);
  }
  if (payload.appearance !== 'visible' && payload.appearance !== 'invisible') throw invalidPayload();
  const field = payload.field as PdfSignatureFieldSpec;
  if (field.kind === 'new' && (payload.appearance === 'visible') !== (field.widget !== null)) throw invalidPayload();
  if (certification && payload.certificationPermission !== 'no-changes'
    && payload.certificationPermission !== 'form-filling-and-signatures'
    && payload.certificationPermission !== 'form-filling-signatures-and-annotations') throw invalidPayload();
}

function assertOperationFrames(
  operation: PdfSignatureCoreSigningOperation,
  payload: Record<string, unknown>,
  frames: readonly PdfSignatureCoreFrame[],
): void {
  const ids = frames.map((frame) => frame.id).sort();
  if ((operation === 'handshake' || operation === 'addSignatureField' || operation === 'postvalidateSignedMutation')
    && ids.length !== 0) throw invalidPayload();
  if (operation === 'inspectPkcs12' && ids.join(',') !== 'pkcs12') throw invalidPayload();
  if (operation === 'sign' || operation === 'certify') {
    const expected = payload.appearance === 'visible' ? 'appearance,pkcs12' : 'pkcs12';
    if (ids.join(',') !== expected) throw invalidPayload();
  }
}

function assertField(value: unknown, allowExisting: boolean): asserts value is PdfSignatureFieldSpec {
  if (!isRecord(value) || (value.kind !== 'new' && value.kind !== 'existing')) throw invalidPayload();
  if (value.kind === 'existing') {
    if (!allowExisting) throw invalidPayload();
    assertExactKeys(value, ['kind', 'name']);
    assertFieldName(value.name, 512, true);
    return;
  }
  assertExactKeys(value, ['kind', 'name', 'widget'], ['lock']);
  assertFieldName(value.name, 128, false);
  if (value.widget !== null) {
    if (!isRecord(value.widget)) throw invalidPayload();
    assertExactKeys(value.widget, ['coordinateSpace', 'height', 'pageIndex', 'pageRotation', 'width', 'x', 'y']);
    if (!boundedInteger(value.widget.pageIndex, 0, 100_000)
      || !boundedFiniteNumber(value.widget.x, -1_000_000, 1_000_000)
      || !boundedFiniteNumber(value.widget.y, -1_000_000, 1_000_000)
      || !boundedPositiveFiniteNumber(value.widget.width, 1_000_000)
      || !boundedPositiveFiniteNumber(value.widget.height, 1_000_000)
      || (value.widget as Record<string, unknown>).x as number + ((value.widget as Record<string, unknown>).width as number) > 1_000_000
      || (value.widget as Record<string, unknown>).y as number + ((value.widget as Record<string, unknown>).height as number) > 1_000_000
      || (value.widget.pageRotation !== 0 && value.widget.pageRotation !== 90 && value.widget.pageRotation !== 180 && value.widget.pageRotation !== 270)
      || value.widget.coordinateSpace !== 'unrotated-pdf-default-user-space') throw invalidPayload();
  }
  if (value.lock !== undefined) {
    if (!isRecord(value.lock)) throw invalidPayload();
    assertExactKeys(value.lock, ['action', 'fieldNames']);
    if (value.lock.action !== 'all' && value.lock.action !== 'include' && value.lock.action !== 'exclude') throw invalidPayload();
    if (!Array.isArray(value.lock.fieldNames) || value.lock.fieldNames.length > 256) throw invalidPayload();
    const seen = new Set<string>();
    for (const name of value.lock.fieldNames) {
      assertFieldName(name, 512, true);
      if (seen.has(name)) throw invalidPayload();
      seen.add(name);
    }
    if ((value.lock.action === 'all') !== (value.lock.fieldNames.length === 0)) throw invalidPayload();
  }
}

function assertExactKeys(value: Record<string, unknown>, required: readonly string[], optional: readonly string[] = []): void {
  const actual = Object.keys(value).sort();
  const permitted = new Set([...required, ...optional]);
  if (required.some((key) => !Object.hasOwn(value, key)) || actual.some((key) => !permitted.has(key))) throw invalidPayload();
}

function assertPath(value: unknown): asserts value is string {
  if (typeof value !== 'string' || !isAbsolute(value) || value.length > 4096 || /[\u0000-\u001f\u007f]/u.test(value)) throw invalidPayload();
}

function assertSha(value: unknown): asserts value is string {
  if (typeof value !== 'string' || !/^[a-f0-9]{64}$/.test(value)) throw invalidPayload();
}

function assertText(value: unknown, maximum: number): asserts value is string {
  if (typeof value !== 'string' || value.trim().length === 0 || value.length > maximum || /[\u0000-\u001f\u007f]/u.test(value)) throw invalidPayload();
}

function assertFieldName(value: unknown, maximum: number, allowDots: boolean): asserts value is string {
  assertText(value, maximum);
  if (!allowDots && value.includes('.')) throw invalidPayload();
}

function boundedInteger(value: unknown, minimum: number, maximum: number): value is number {
  return Number.isSafeInteger(value) && (value as number) >= minimum && (value as number) <= maximum;
}

function boundedFiniteNumber(value: unknown, minimum: number, maximum: number): value is number {
  return typeof value === 'number' && Number.isFinite(value) && value >= minimum && value <= maximum;
}

function boundedPositiveFiniteNumber(value: unknown, maximum: number): value is number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 && value <= maximum;
}

function boundedNonnegative(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0 && (value as number) <= Number.MAX_SAFE_INTEGER;
}

function invalidPayload(): TypeError {
  return new TypeError('The PDF signing operation payload is invalid.');
}

function hasDisabledCapabilities(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return hasExactKeys(value, Object.keys(PDF_SIGNATURE_PHASE3_CAPABILITIES))
    && Object.keys(PDF_SIGNATURE_PHASE3_CAPABILITIES)
    .every((key) => typeof value[key] === 'boolean');
}

function isSignedMutationPostvalidation(
  value: unknown,
  expected: PdfSignatureCorePostvalidateSignedMutationPayload,
): value is PdfSignedMutationPostvalidation {
  return isRecord(value)
    && hasExactKeys(value, [
      'addedSignatureCount', 'certificateSha256', 'cryptographicallyValid', 'fieldName', 'independentProcess',
      'inputPrefixPreserved', 'inputSha256', 'newSignatureCoversOutputExceptContents',
      'outputSha256', 'priorSignaturesPreserved', 'structurallyReadable', 'validator',
    ])
    && value.inputSha256 === expected.expectedInputSha256
    && value.outputSha256 === expected.expectedOutputSha256
    && value.fieldName === expected.expectedFieldName
    && value.certificateSha256 === expected.expectedCertificateSha256
    && value.inputPrefixPreserved === true
    && value.addedSignatureCount === 1
    && value.priorSignaturesPreserved === true
    && value.newSignatureCoversOutputExceptContents === true
    && value.cryptographicallyValid === true
    && value.structurallyReadable === true
    && value.independentProcess === true
    && value.validator === 'pdf-signature-core-v1-validate-plus-main-prefix';
}

function isHandshakeResult(value: unknown): value is Record<string, unknown> {
  if (!isRecord(value)
    || !hasExactKeys(value, ['capabilities', 'limits', 'operations', 'profiles', 'providers', 'versions'])
    || !hasDisabledCapabilities(value.capabilities)
    || !isRecord(value.versions)
    || !hasExactKeys(value.versions, ['engine', 'framedProtocol', 'java'])
    || !boundedText(value.versions.engine, 128)
    || value.versions.framedProtocol !== 2
    || !boundedText(value.versions.java, 128)
    || !Array.isArray(value.operations)
    || value.operations.length !== 6
    || value.operations.join(',') !== 'handshake,inspectPkcs12,addSignatureField,sign,certify,postvalidateSignedMutation'
    || !Array.isArray(value.profiles) || value.profiles.length !== 1 || value.profiles[0] !== 'PAdES-B-B'
    || !Array.isArray(value.providers) || value.providers.length !== 1 || value.providers[0] !== 'pkcs12'
    || !isRecord(value.limits)
    || !hasExactKeys(value.limits, ['frameBytes', 'headerBytes', 'signingInputBytes', 'totalFrameBytes'])) return false;
  return boundedPositiveSafeInteger(value.limits.headerBytes)
    && boundedPositiveSafeInteger(value.limits.frameBytes)
    && boundedPositiveSafeInteger(value.limits.totalFrameBytes)
    && boundedPositiveSafeInteger(value.limits.signingInputBytes);
}

function isPkcs12Inspection(value: unknown): value is PdfSignatureCorePkcs12Inspection {
  if (!isRecord(value)
    || Object.keys(value).sort().join(',') !== 'engineVersion,identities,passwordRemembered,privateKeyExported,provider'
    || value.provider !== 'pkcs12' || value.passwordRemembered !== false || value.privateKeyExported !== false
    || !boundedText(value.engineVersion, 128)
    || !Array.isArray(value.identities) || value.identities.length < 1 || value.identities.length > 32) return false;
  return value.identities.every((identity) => isRecord(identity)
    && hasExactKeys(identity, [
      'certificateSha256', 'chainSha256', 'hasPrivateKey', 'issuer', 'keyAlgorithm',
      'keyBits', 'serialNumber', 'subject', 'supportedDigests', 'validFrom', 'validTo',
    ])
    && typeof identity.certificateSha256 === 'string' && /^[a-f0-9]{64}$/.test(identity.certificateSha256)
    && boundedText(identity.subject, 512) && boundedText(identity.issuer, 512)
    && boundedText(identity.serialNumber, 512)
    && canonicalInstant(identity.validFrom) && canonicalInstant(identity.validTo)
    && boundedText(identity.keyAlgorithm, 64)
    && Number.isSafeInteger(identity.keyBits) && (identity.keyBits as number) >= 0 && (identity.keyBits as number) <= 65_536
    && Array.isArray(identity.chainSha256) && identity.chainSha256.length >= 1 && identity.chainSha256.length <= 32
    && identity.chainSha256.every((digest) => typeof digest === 'string' && /^[a-f0-9]{64}$/.test(digest))
    && Array.isArray(identity.supportedDigests) && identity.supportedDigests.length <= 3
    && identity.supportedDigests.every((digest) => digest === 'SHA-256' || digest === 'SHA-384' || digest === 'SHA-512')
    && identity.hasPrivateKey === true);
}

function boundedText(value: unknown, maximum: number): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= maximum && !/[\u0000-\u001f\u007f]/u.test(value);
}

function boundedPositiveSafeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0;
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const expectedKeys = [...expected].sort();
  const actualKeys = Object.keys(value).sort();
  return actualKeys.length === expectedKeys.length
    && actualKeys.every((key, index) => key === expectedKeys[index]);
}

function canonicalInstant(value: unknown): value is string {
  return typeof value === 'string'
    && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/.test(value)
    && !Number.isNaN(Date.parse(value));
}

function boundedTimeout(value: number | undefined): number {
  const result = value ?? DEFAULT_TIMEOUT_MS;
  if (!Number.isSafeInteger(result) || result < 1 || result > MAX_TIMEOUT_MS) {
    throw new TypeError('The PDF signing timeout is invalid.');
  }
  return result;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function protocolError(): PdfSignatureCoreSigningError {
  return new PdfSignatureCoreSigningError('PROTOCOL_ERROR', 'The PDF signing engine returned an invalid response.');
}

function cancelledError(): PdfSignatureCoreSigningError {
  return new PdfSignatureCoreSigningError('CANCELLED', 'The PDF signing operation was cancelled.');
}
