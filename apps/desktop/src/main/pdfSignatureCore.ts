import { createHash, randomUUID } from 'node:crypto';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { createReadStream } from 'node:fs';
import {
  isPdfSignatureValidationReport,
  type PdfSignatureValidationReport,
} from '@butter-paper/core';
import {
  PDF_SIGNATURE_CORE_PROTOCOL_VERSION,
  assertPdfSignatureCoreInputFile,
  resolvePdfSignatureCorePackage,
  type ResolvePdfSignatureCorePackageOptions,
  type VerifiedPdfSignatureCorePackage,
} from './pdfSignatureCorePackage';
import {
  assertPdfSignatureCoreValidationBoundary,
  createPdfSignatureCoreValidationPayload,
  type PdfSignatureCoreTrustPolicyInput,
} from './pdfSignatureCoreSafety';
import {
  assertPdfSignatureCoreLauncherBindingUnchanged,
  verifyPdfSignatureCoreLauncherBinding,
  verifySpawnedPdfSignatureCoreLauncher,
  type VerifiedPdfSignatureCoreLauncher,
} from './pdfSignatureCoreSigning';
import {
  PDF_UNSIGNED_COPY_REMOVAL_POLICY_ID,
  PDF_UNSIGNED_COPY_REMOVAL_POLICY_VERSION,
  type PdfUnsignedCopyConversionResult,
  type PdfUnsignedCopyStructuralVerification,
} from './pdfUnsignedCopy';

const MAX_NDJSON_LINE_BYTES = 1024 * 1024;
const MAX_STDOUT_BYTES = 16 * 1024 * 1024;
const MAX_STDERR_BYTES = 64 * 1024;
const MAX_JSON_DEPTH = 32;
const MAX_JSON_NODES = 1024;
const MAX_EVENTS = 10_000;
const DEFAULT_TIMEOUT_MS = 30_000;
const MAX_TIMEOUT_MS = 5 * 60_000;
const CANCEL_GRACE_MS = 250;
const TERMINATION_SETTLEMENT_MS = CANCEL_GRACE_MS * 2;
const PHASE_ONE_CAPABILITIES = {
  certificateSign: false,
  certify: false,
  createUnsignedCopy: true,
  inspect: true,
  ltv: false,
  onlineValidation: false,
  pkcs11: false,
  signatureRead: true,
  signatureValidation: true,
  signedIncrementalEdit: false,
} as const;
const PHASE_ONE_OPERATIONS = [
  'handshake',
  'version',
  'inspect',
  'validate',
  'createUnsignedCopy',
  'inspectUnsignedStructure',
  'cancel',
] as const;
const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const EMPTY_SHA256 = createHash('sha256').update(Buffer.alloc(0)).digest('hex');
const MAX_REMOVAL_COUNT = 1_000_000;
const MAX_WARNINGS = 64;
const MAX_WARNING_LENGTH = 2048;

type PdfSignatureCoreOperation = 'handshake' | 'inspect' | 'validate' | 'createUnsignedCopy' | 'inspectUnsignedStructure';

interface PdfSignatureCoreRequest {
  protocolVersion: 1;
  requestId: string;
  operation: PdfSignatureCoreOperation | 'cancel';
  payload: Record<string, unknown>;
}

interface PdfSignatureCoreResponseEnvelope {
  protocolVersion: 1;
  requestId: string;
  operation: string;
  engineVersion: string;
  event: 'progress' | 'result' | 'error';
  progress?: unknown;
  result?: unknown;
  error?: unknown;
}

export interface PdfSignatureCoreHandshake {
  versions: {
    engine: string;
    protocol: 1;
    java: string;
    [key: string]: unknown;
  };
  operations: readonly string[];
  capabilities: Readonly<Record<string, boolean>>;
  [key: string]: unknown;
}

export interface PdfSignatureCoreInspectResult {
  inputSha256: string;
  byteLength: number;
  startsWithPdfHeader: boolean;
  eofMarkerPresent: boolean;
  byteRangeMarkerCount: number;
  signatureDictionaryMarkerCount: number;
  signatureFieldMarkerCount: number;
  structuralOnly: true;
  validationPerformed: false;
  warning: string;
  [key: string]: unknown;
}

export interface PdfSignatureCoreRunOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
  onProgress?: (progress: unknown) => void;
}

export interface PdfSignatureCoreClientOptions extends ResolvePdfSignatureCorePackageOptions {
  processEnvironment?: NodeJS.ProcessEnv;
  /** Test seam only; production callers must use the default argument-free spawn. */
  spawnProcess?: typeof spawn;
  /** Test seam only; production binds the running image to the verified launcher. */
  verifySpawnedLauncher?: typeof verifySpawnedPdfSignatureCoreLauncher;
}

export class PdfSignatureCoreError extends Error {
  readonly engineCode?: string;

  constructor(
    readonly code:
      | 'CANCELLED'
      | 'TIMEOUT'
      | 'LAUNCH_FAILED'
      | 'PROCESS_CRASHED'
      | 'PROTOCOL_ERROR'
      | 'ENGINE_ERROR',
    message: string,
    options?: ErrorOptions & { engineCode?: string },
  ) {
    super(message, options);
    this.name = 'PdfSignatureCoreError';
    this.engineCode = options?.engineCode;
  }
}

/**
 * Main-process-only client for the local signature sidecar. It intentionally
 * starts a fresh, argument-free process for every operation.
 */
export class PdfSignatureCoreClient {
  constructor(private readonly options: PdfSignatureCoreClientOptions) {}

  async handshake(options: PdfSignatureCoreRunOptions = {}): Promise<PdfSignatureCoreHandshake> {
    const verifiedPackage = await resolvePdfSignatureCorePackage(this.options);
    const result = await this.run(verifiedPackage, 'handshake', {}, options);
    if (!isRecord(result)
      || !isRecord(result.versions)
      || result.versions.protocol !== PDF_SIGNATURE_CORE_PROTOCOL_VERSION
      || result.versions.engine !== verifiedPackage.manifest.engineVersion
      || typeof result.versions.java !== 'string'
      || !result.versions.java.startsWith(verifiedPackage.manifest.javaVersion)
      || result.versions.javaFeature !== 21
      || !Array.isArray(result.operations)
      || result.operations.some((operation) => typeof operation !== 'string')
      || !matchesExactStringArray(result.operations, PHASE_ONE_OPERATIONS)
      || !hasExactPhaseOneCapabilities(result.capabilities)) {
      throw protocolError('handshake result does not match the packaged engine');
    }
    return result as unknown as PdfSignatureCoreHandshake;
  }

  async inspectFile(
    inputPath: string,
    options: PdfSignatureCoreRunOptions = {},
  ): Promise<PdfSignatureCoreInspectResult> {
    const verifiedInputPath = await assertPdfSignatureCoreInputFile(inputPath);
    const verifiedPackage = await resolvePdfSignatureCorePackage(this.options);
    const result = await this.run(verifiedPackage, 'inspect', { inputPath: verifiedInputPath }, options);
    if (!isRecord(result)
      || typeof result.inputSha256 !== 'string'
      || !/^[a-f0-9]{64}$/.test(result.inputSha256)
      || !isNonnegativeSafeInteger(result.byteLength)
      || typeof result.startsWithPdfHeader !== 'boolean'
      || typeof result.eofMarkerPresent !== 'boolean'
      || !isBoundedCount(result.byteRangeMarkerCount)
      || !isBoundedCount(result.signatureDictionaryMarkerCount)
      || !isBoundedCount(result.signatureFieldMarkerCount)
      || result.structuralOnly !== true
      || result.validationPerformed !== false
      || typeof result.warning !== 'string'
      || result.warning.length === 0
      || result.warning.length > 2048) {
      throw protocolError('inspect result does not match the Phase 0 structural-only contract');
    }
    assertNoReturnedPaths(result);
    return result as PdfSignatureCoreInspectResult;
  }

  async validateFile(
    inputPath: string,
    options: PdfSignatureCoreRunOptions & {
      onlineValidationAuthorized?: boolean;
      /** Main-process-only deterministic/reference seam; never exposed through renderer IPC. */
      fixedReferenceValidationTime?: string;
      trustPolicy?: PdfSignatureCoreTrustPolicyInput;
    } = {},
  ): Promise<PdfSignatureValidationReport> {
    const verifiedInputPath = await assertPdfSignatureCoreInputFile(inputPath);
    const verifiedPackage = await resolvePdfSignatureCorePackage(this.options);
    const expectedInputSha256 = await hashFileSha256(verifiedInputPath);
    const validationStartedAt = new Date().toISOString();
    const result = await this.run(
      verifiedPackage,
      'validate',
      { ...createPdfSignatureCoreValidationPayload(verifiedInputPath, options) },
      options,
    );
    const validationCompletedAt = new Date().toISOString();
    if (!isPdfSignatureValidationReport(result)
      || result.engineVersion !== verifiedPackage.manifest.engineVersion) {
      throw protocolError('validate result does not match the Phase 1 signature report contract');
    }
    assertNoReturnedPaths(result);
    assertPdfSignatureCoreValidationBoundary(result, {
      inputSha256: expectedInputSha256,
      onlineValidationAuthorized: options.onlineValidationAuthorized,
      fixedReferenceValidationTime: options.fixedReferenceValidationTime,
      validationStartedAt,
      validationCompletedAt,
      trustPolicy: options.trustPolicy,
    });
    const currentInputSha256 = await hashFileSha256(verifiedInputPath);
    return assertPdfSignatureCoreValidationBoundary(result, {
      inputSha256: currentInputSha256,
      onlineValidationAuthorized: options.onlineValidationAuthorized,
      fixedReferenceValidationTime: options.fixedReferenceValidationTime,
      validationStartedAt,
      validationCompletedAt,
      trustPolicy: options.trustPolicy,
    });
  }

  async createUnsignedCopy(
    inputPath: string,
    outputPath: string,
    expectedInputSha256: string,
    options: PdfSignatureCoreRunOptions = {},
  ): Promise<PdfUnsignedCopyConversionResult> {
    if (!SHA256_PATTERN.test(expectedInputSha256)) {
      throw new TypeError('Expected unsigned-copy input SHA-256 is invalid.');
    }
    const verifiedInputPath = await assertPdfSignatureCoreInputFile(inputPath);
    const verifiedOutputPath = await assertPdfSignatureCoreInputFile(outputPath);
    if (verifiedInputPath === verifiedOutputPath) {
      throw new TypeError('Unsigned-copy input and output must be different files.');
    }
    if (await hashFileSha256(verifiedInputPath) !== expectedInputSha256) {
      throw new Error('Unsigned-copy input does not match the main-process snapshot.');
    }
    if (await hashFileSha256(verifiedOutputPath) !== EMPTY_SHA256) {
      throw new Error('Unsigned-copy output must be the empty main-process-owned file.');
    }
    const verifiedPackage = await resolvePdfSignatureCorePackage(this.options);
    const result = await this.run(
      verifiedPackage,
      'createUnsignedCopy',
      { inputPath: verifiedInputPath, outputPath: verifiedOutputPath },
      options,
    );
    if (!isUnsignedCopyEngineResult(result)
      || result.engineVersion !== verifiedPackage.manifest.engineVersion
      || result.inputSha256 !== expectedInputSha256
      || result.removalPolicyId !== PDF_UNSIGNED_COPY_REMOVAL_POLICY_ID
      || result.removalPolicyVersion !== PDF_UNSIGNED_COPY_REMOVAL_POLICY_VERSION) {
      throw protocolError('createUnsignedCopy result does not match the reviewed removal contract');
    }
    assertNoReturnedPaths(result);
    if (await hashFileSha256(verifiedInputPath) !== expectedInputSha256) {
      throw protocolError('createUnsignedCopy changed or raced its protected input snapshot');
    }
    if (await hashFileSha256(verifiedOutputPath) !== result.outputSha256) {
      throw protocolError('createUnsignedCopy output digest does not match the current output bytes');
    }
    return {
      engineVersion: result.engineVersion,
      inputSha256: result.inputSha256,
      outputSha256: result.outputSha256,
      removalPolicyId: result.removalPolicyId,
      removalPolicyVersion: result.removalPolicyVersion,
      removed: { ...result.removed },
      warnings: [...result.warnings],
    };
  }

  async inspectUnsignedStructure(
    inputPath: string,
    options: PdfSignatureCoreRunOptions = {},
  ): Promise<PdfUnsignedCopyStructuralVerification> {
    const verifiedInputPath = await assertPdfSignatureCoreInputFile(inputPath);
    const expectedInputSha256 = await hashFileSha256(verifiedInputPath);
    const verifiedPackage = await resolvePdfSignatureCorePackage(this.options);
    const result = await this.run(
      verifiedPackage,
      'inspectUnsignedStructure',
      { inputPath: verifiedInputPath },
      options,
    );
    if (!isUnsignedStructureEngineResult(result) || result.inputSha256 !== expectedInputSha256) {
      throw protocolError('inspectUnsignedStructure result does not match the current output bytes');
    }
    assertNoReturnedPaths(result);
    if (await hashFileSha256(verifiedInputPath) !== expectedInputSha256) {
      throw protocolError('inspectUnsignedStructure input changed during inspection');
    }
    return {
      structurallyReadable: true,
      byteRangeMarkerCount: result.byteRangeMarkerCount,
      signatureDictionaryCount: result.signatureDictionaryCount,
      signedSignatureFieldCount: result.signedSignatureFieldCount,
      docMdpReferenceCount: result.docMdpReferenceCount,
      fieldMdpReferenceCount: result.fieldMdpReferenceCount,
      dssOrVriEntryCount: result.dssOrVriEntryCount,
    };
  }

  private async run(
    verifiedPackage: VerifiedPdfSignatureCorePackage,
    operation: PdfSignatureCoreOperation,
    payload: Record<string, unknown>,
    options: PdfSignatureCoreRunOptions,
  ): Promise<unknown> {
    const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > MAX_TIMEOUT_MS) {
      return Promise.reject(new RangeError(`PDF signature core timeout must be between 1 and ${MAX_TIMEOUT_MS} ms.`));
    }
    if (options.signal?.aborted) {
      throw new PdfSignatureCoreError('CANCELLED', 'PDF signature operation was cancelled.');
    }

    let launcherBinding: VerifiedPdfSignatureCoreLauncher;
    try {
      launcherBinding = await verifyPdfSignatureCoreLauncherBinding(verifiedPackage);
    } catch (error) {
      throw new PdfSignatureCoreError(
        'LAUNCH_FAILED',
        'The verified PDF signature core launcher is unsafe.',
        { cause: error },
      );
    }

    const requestId = randomUUID();
    const request: PdfSignatureCoreRequest = {
      protocolVersion: PDF_SIGNATURE_CORE_PROTOCOL_VERSION,
      requestId,
      operation,
      payload,
    };
    const requestLine = serializeRequest(request);
    const redactions = collectSensitiveValues(payload);

    return new Promise((resolvePromise, rejectPromise) => {
      let child: ChildProcessWithoutNullStreams;
      try {
        child = (this.options.spawnProcess ?? spawn)(verifiedPackage.launcherPath, [], {
          cwd: verifiedPackage.packageRoot,
          env: createSidecarEnvironment(this.options.processEnvironment ?? process.env),
          shell: false,
          windowsHide: true,
          stdio: ['pipe', 'pipe', 'pipe'],
        });
      } catch (error) {
        rejectPromise(new PdfSignatureCoreError('LAUNCH_FAILED', launchFailureMessage(error), { cause: error }));
        return;
      }

      let stdoutBuffer = Buffer.alloc(0);
      let stdoutBytes = 0;
      let stderrBuffer = Buffer.alloc(0);
      let eventCount = 0;
      let terminal: PdfSignatureCoreResponseEnvelope | null = null;
      let termination: 'cancelled' | 'timeout' | null = null;
      let forceKillTimer: NodeJS.Timeout | undefined;
      let terminationSettleTimer: NodeJS.Timeout | undefined;
      let settled = false;

      const failLauncherVerification = (error: unknown) => {
        if (settled) return;
        termination = 'cancelled';
        cleanupProcess();
        child.kill('SIGKILL');
        rejectOnce(new PdfSignatureCoreError(
          'LAUNCH_FAILED',
          'The running PDF signature core image could not be bound to the verified launcher.',
          { cause: error },
        ));
      };

      const cleanupPromise = () => {
        clearTimeout(timeoutTimer);
        options.signal?.removeEventListener('abort', cancelOperation);
      };
      const cleanupProcess = () => {
        if (forceKillTimer) clearTimeout(forceKillTimer);
        if (terminationSettleTimer) clearTimeout(terminationSettleTimer);
      };
      const rejectOnce = (error: unknown) => {
        if (settled) return;
        settled = true;
        cleanupPromise();
        rejectPromise(error);
      };
      const resolveOnce = (result: unknown) => {
        if (settled) return;
        settled = true;
        cleanupPromise();
        resolvePromise(result);
      };
      const terminate = (reason: 'cancelled' | 'timeout') => {
        if (termination || settled) return;
        termination = reason;
        const cancelRequest: PdfSignatureCoreRequest = {
          protocolVersion: PDF_SIGNATURE_CORE_PROTOCOL_VERSION,
          requestId: `${requestId}:cancel`,
          operation: 'cancel',
          payload: { targetRequestId: requestId },
        };
        if (child.stdin.writable) {
          child.stdin.write(`${JSON.stringify(cancelRequest)}\n`);
          child.stdin.end();
        }
        child.kill('SIGTERM');
        forceKillTimer = setTimeout(() => {
          if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
        }, CANCEL_GRACE_MS);
        forceKillTimer.unref?.();
        terminationSettleTimer = setTimeout(() => {
          if (child.exitCode === null && child.signalCode === null) {
            child.stdin.destroy();
            child.stdout.destroy();
            child.stderr.destroy();
            child.unref();
          }
          if (reason === 'timeout') {
            rejectOnce(new PdfSignatureCoreError('TIMEOUT', `PDF signature operation exceeded ${timeoutMs} ms and did not confirm termination.`));
          } else {
            rejectOnce(new PdfSignatureCoreError('CANCELLED', 'PDF signature operation was cancelled without confirmed process termination.'));
          }
        }, TERMINATION_SETTLEMENT_MS);
        terminationSettleTimer.unref?.();
      };
      const cancelOperation = () => terminate('cancelled');
      const timeoutTimer = setTimeout(() => terminate('timeout'), timeoutMs);
      timeoutTimer.unref?.();
      options.signal?.addEventListener('abort', cancelOperation, { once: true });

      child.once('error', (error) => {
        cleanupProcess();
        rejectOnce(new PdfSignatureCoreError('LAUNCH_FAILED', launchFailureMessage(error), { cause: error }));
      });
      child.stderr.on('data', (chunk: Buffer) => {
        if (stderrBuffer.byteLength >= MAX_STDERR_BYTES) return;
        const remaining = MAX_STDERR_BYTES - stderrBuffer.byteLength;
        stderrBuffer = Buffer.concat([stderrBuffer, chunk.subarray(0, remaining)]);
      });
      child.stdout.on('data', (chunk: Buffer) => {
        if (settled || termination) return;
        stdoutBytes += chunk.byteLength;
        if (stdoutBytes > MAX_STDOUT_BYTES) {
          terminate('cancelled');
          rejectOnce(protocolError('sidecar output exceeds the 16 MiB operation limit'));
          return;
        }
        stdoutBuffer = Buffer.concat([stdoutBuffer, chunk]);
        if (stdoutBuffer.byteLength > MAX_NDJSON_LINE_BYTES && !stdoutBuffer.includes(0x0a)) {
          terminate('cancelled');
          rejectOnce(protocolError('sidecar emitted an NDJSON line larger than 1 MiB'));
          return;
        }
        let newlineIndex = stdoutBuffer.indexOf(0x0a);
        while (newlineIndex >= 0 && !settled && !termination) {
          const line = stdoutBuffer.subarray(0, newlineIndex);
          stdoutBuffer = stdoutBuffer.subarray(newlineIndex + 1);
          try {
            if (line.byteLength === 0) throw protocolError('sidecar emitted an empty NDJSON line');
            if (line.byteLength > MAX_NDJSON_LINE_BYTES) throw protocolError('sidecar emitted an NDJSON line larger than 1 MiB');
            eventCount += 1;
            if (eventCount > MAX_EVENTS) throw protocolError('sidecar emitted too many events');
            const envelope = parseResponseLine(decodeProtocolLine(line), requestId, operation, verifiedPackage.manifest.engineVersion);
            if (terminal) throw protocolError('sidecar emitted data after a terminal event');
            if (envelope.event === 'progress') {
              options.onProgress?.(envelope.progress);
            } else {
              terminal = envelope;
              child.stdin.end();
            }
          } catch (error) {
            terminate('cancelled');
            rejectOnce(error);
            return;
          }
          newlineIndex = stdoutBuffer.indexOf(0x0a);
        }
      });
      child.once('close', (code, signal) => {
        cleanupProcess();
        if (settled) return;
        if (termination === 'timeout') {
          rejectOnce(new PdfSignatureCoreError('TIMEOUT', `PDF signature operation exceeded ${timeoutMs} ms.`));
          return;
        }
        if (termination === 'cancelled') {
          rejectOnce(new PdfSignatureCoreError('CANCELLED', 'PDF signature operation was cancelled.'));
          return;
        }
        const stderr = scrubSidecarStderr(stderrBuffer.toString('utf8'), redactions);
        if (code !== 0 || signal) {
          rejectOnce(new PdfSignatureCoreError(
            'PROCESS_CRASHED',
            `PDF signature core exited unexpectedly${stderr ? `: ${stderr}` : '.'}`,
          ));
          return;
        }
        if (stdoutBuffer.byteLength !== 0) {
          rejectOnce(protocolError('sidecar ended with an incomplete NDJSON line'));
          return;
        }
        if (!terminal) {
          rejectOnce(protocolError('sidecar exited without a terminal event'));
          return;
        }
        if (terminal.event === 'error') {
          try {
            const engineError = parseEngineError(terminal.error, redactions);
            rejectOnce(new PdfSignatureCoreError(
              'ENGINE_ERROR',
              `${engineError.code}: ${engineError.message}`,
              { engineCode: engineError.code },
            ));
          } catch (error) {
            rejectOnce(error);
          }
          return;
        }
        resolveOnce(terminal.result);
      });

      child.stdin.once('error', (error: NodeJS.ErrnoException) => {
        if (error.code !== 'EPIPE' && !termination) {
          terminate('cancelled');
          rejectOnce(new PdfSignatureCoreError('PROCESS_CRASHED', 'PDF signature core stdin failed.', { cause: error }));
        }
      });
      void (async () => {
        try {
          await (this.options.verifySpawnedLauncher ?? verifySpawnedPdfSignatureCoreLauncher)(child, launcherBinding);
          await assertPdfSignatureCoreLauncherBindingUnchanged(launcherBinding);
          if (settled || termination) return;
          child.stdin.write(requestLine);
        } catch (error) {
          failLauncherVerification(error);
        }
      })();
    });
  }
}

function decodeProtocolLine(line: Uint8Array): string {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(line);
  } catch (error) {
    throw protocolError('sidecar emitted invalid UTF-8', error);
  }
}

function launchFailureMessage(error: unknown): string {
  if (error && typeof error === 'object'
    && 'code' in error && typeof error.code === 'string') {
    return `PDF signature core could not be launched (${error.code}).`;
  }
  return 'PDF signature core could not be launched.';
}

function serializeRequest(request: PdfSignatureCoreRequest): string {
  assertStructureLimits(request);
  const line = `${JSON.stringify(request)}\n`;
  if (Buffer.byteLength(line) > MAX_NDJSON_LINE_BYTES) {
    throw protocolError('request exceeds the 1 MiB NDJSON line limit');
  }
  return line;
}

function parseResponseLine(
  line: string,
  requestId: string,
  operation: PdfSignatureCoreOperation,
  engineVersion: string,
): PdfSignatureCoreResponseEnvelope {
  assertJsonTextDepth(line);
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch (error) {
    throw protocolError('sidecar emitted malformed JSON', error);
  }
  assertStructureLimits(value);
  if (!isRecord(value)
    || value.protocolVersion !== PDF_SIGNATURE_CORE_PROTOCOL_VERSION
    || value.requestId !== requestId
    || value.operation !== operation
    || value.engineVersion !== engineVersion
    || (value.event !== 'progress' && value.event !== 'result' && value.event !== 'error')) {
    throw protocolError('sidecar emitted an envelope with mismatched protocol metadata');
  }
  if (value.event === 'progress' && !('progress' in value)) throw protocolError('progress event has no progress payload');
  if (value.event === 'result' && !('result' in value)) throw protocolError('result event has no result payload');
  if (value.event === 'error' && !('error' in value)) throw protocolError('error event has no error payload');
  assertNoReturnedPaths(value);
  return value as unknown as PdfSignatureCoreResponseEnvelope;
}

function parseEngineError(value: unknown, redactions: readonly string[]): { code: string; message: string } {
  if (!isRecord(value)
    || typeof value.code !== 'string'
    || !/^[A-Z][A-Z0-9_]{0,63}$/.test(value.code)
    || typeof value.message !== 'string'
    || value.message.length === 0
    || value.message.length > 2048) {
    throw protocolError('sidecar error payload is malformed');
  }
  return { code: value.code, message: redactAndSanitizeDiagnostic(value.message, redactions, 2048) };
}

function assertJsonTextDepth(text: string): void {
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (const character of text) {
    if (inString) {
      if (escaped) escaped = false;
      else if (character === '\\') escaped = true;
      else if (character === '"') inString = false;
      continue;
    }
    if (character === '"') inString = true;
    else if (character === '{' || character === '[') {
      depth += 1;
      if (depth > MAX_JSON_DEPTH) throw protocolError('sidecar JSON exceeds the nesting-depth limit');
    } else if (character === '}' || character === ']') {
      depth -= 1;
      if (depth < 0) throw protocolError('sidecar JSON nesting is malformed');
    }
  }
}

function assertStructureLimits(value: unknown): void {
  let nodes = 0;
  const visit = (current: unknown, depth: number): void => {
    nodes += 1;
    if (nodes > MAX_JSON_NODES) throw protocolError('JSON structure exceeds the node-count limit');
    if (depth > MAX_JSON_DEPTH) throw protocolError('JSON structure exceeds the nesting-depth limit');
    if (typeof current === 'string' && Buffer.byteLength(current) > MAX_NDJSON_LINE_BYTES) {
      throw protocolError('JSON string exceeds the value-size limit');
    }
    if (Array.isArray(current)) current.forEach((entry) => visit(entry, depth + 1));
    else if (isRecord(current)) Object.entries(current).forEach(([key, entry]) => {
      if (key.length > 256) throw protocolError('JSON object key exceeds the length limit');
      visit(entry, depth + 1);
    });
  };
  visit(value, 0);
}

function createSidecarEnvironment(source: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const allowedKeys = process.platform === 'win32'
    ? ['SYSTEMROOT', 'WINDIR', 'COMSPEC', 'PATHEXT', 'TEMP', 'TMP', 'LANG', 'LC_ALL']
    : ['TMPDIR', 'TMP', 'TEMP', 'LANG', 'LC_ALL'];
  const environment: NodeJS.ProcessEnv = {
    BP_SIGNATURE_CORE_NETWORK: 'disabled',
  };
  for (const key of allowedKeys) {
    if (source[key]) environment[key] = source[key];
  }
  return environment;
}

function collectSensitiveValues(value: unknown): string[] {
  const values = new Set<string>();
  const visit = (current: unknown, key = ''): void => {
    if (typeof current === 'string'
      && current.length >= 3
      && /(?:path|password|passphrase|pin|secret|token|private|content)/i.test(key)) {
      values.add(current);
      const segments = current.split(/[\\/]/);
      const leaf = segments.at(-1);
      if (leaf && leaf.length >= 3) values.add(leaf);
    } else if (Array.isArray(current)) {
      current.forEach((entry) => visit(entry, key));
    } else if (isRecord(current)) {
      Object.entries(current).forEach(([childKey, entry]) => visit(entry, childKey));
    }
  };
  visit(value);
  return [...values].sort((left, right) => right.length - left.length);
}

export function scrubSidecarStderr(stderr: string, redactions: readonly string[] = []): string {
  return redactAndSanitizeDiagnostic(stderr, redactions, MAX_STDERR_BYTES);
}

function redactAndSanitizeDiagnostic(
  value: string,
  redactions: readonly string[],
  maximumLength: number,
): string {
  let scrubbed = value.replace(/\u001b\[[0-9;]*m/g, '');
  for (const sensitiveValue of redactions) {
    scrubbed = scrubbed.split(sensitiveValue).join('[REDACTED]');
  }
  return sanitizeDiagnostic(scrubbed).slice(0, maximumLength);
}

function sanitizeDiagnostic(value: string): string {
  return value
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, '')
    .replace(/[\r\n]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function assertNoReturnedPaths(value: unknown): void {
  if (Array.isArray(value)) {
    value.forEach(assertNoReturnedPaths);
    return;
  }
  if (!isRecord(value)) return;
  for (const [key, child] of Object.entries(value)) {
    if (isPathBearingKey(key)) {
      throw protocolError('sidecar response attempted to expose a filesystem path');
    }
    assertNoReturnedPaths(child);
  }
}

function isPathBearingKey(key: string): boolean {
  const tokens = key
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[^A-Za-z0-9]+/g, ' ')
    .trim()
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  if (tokens.length === 0) return false;

  const pathTokens = new Set([
    'path', 'paths', 'filepath', 'filepaths', 'filename', 'filenames',
    'pathname', 'pathnames', 'file', 'files',
    'directory', 'directories', 'dirname', 'dirnames', 'folder', 'folders',
    'location', 'locations', 'uri', 'uris',
  ]);
  if (tokens.some((token) => pathTokens.has(token))) return true;
  if (tokens.includes('file') && tokens.includes('name')) return true;

  const pathRoles = new Set([
    'input', 'output', 'source', 'target', 'canonical', 'original',
    'destination', 'working', 'temporary', 'temp', 'resolved', 'absolute',
    'relative',
  ]);
  const pathNouns = new Set(['file', 'files', 'dir', 'directory', 'folder', 'location', 'uri']);
  return tokens.some((token) => pathRoles.has(token))
    && tokens.some((token) => pathNouns.has(token));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactPhaseOneCapabilities(value: unknown): value is typeof PHASE_ONE_CAPABILITIES {
  if (!isRecord(value)) return false;
  const expectedEntries = Object.entries(PHASE_ONE_CAPABILITIES);
  return Object.keys(value).length === expectedEntries.length
    && expectedEntries.every(([capability, enabled]) => value[capability] === enabled);
}

function matchesExactStringArray(
  value: readonly string[],
  expected: readonly string[],
): boolean {
  return value.length === expected.length && value.every((entry, index) => entry === expected[index]);
}

interface UnsignedCopyEngineResult extends PdfUnsignedCopyConversionResult {
  readonly outputBytes: number;
  readonly pageCount: number;
  readonly structuralPostcheck: Omit<PdfUnsignedCopyStructuralVerification, 'structurallyReadable'>;
  readonly sourcePreserved: true;
  readonly validatedUnsigned: true;
}

function isUnsignedCopyEngineResult(value: unknown): value is UnsignedCopyEngineResult {
  if (!isRecord(value)) return false;
  const keys = [
    'engineVersion', 'inputSha256', 'outputSha256', 'outputBytes', 'pageCount',
    'removalPolicyId', 'removalPolicyVersion', 'removed', 'structuralPostcheck',
    'warnings', 'sourcePreserved', 'validatedUnsigned',
  ];
  return hasExactKeys(value, keys)
    && typeof value.engineVersion === 'string' && /^[0-9A-Za-z][0-9A-Za-z._+-]{0,127}$/.test(value.engineVersion)
    && typeof value.inputSha256 === 'string' && SHA256_PATTERN.test(value.inputSha256)
    && typeof value.outputSha256 === 'string' && SHA256_PATTERN.test(value.outputSha256)
    && isNonnegativeSafeInteger(value.outputBytes) && value.outputBytes > 0
    && isNonnegativeSafeInteger(value.pageCount) && value.pageCount > 0
    && typeof value.removalPolicyId === 'string'
    && isNonnegativeSafeInteger(value.removalPolicyVersion) && value.removalPolicyVersion > 0
    && isRemovalCounts(value.removed)
    && isUnsignedStructuralCounts(value.structuralPostcheck)
    && Object.values(value.structuralPostcheck).every((count) => count === 0)
    && isWarnings(value.warnings)
    && value.sourcePreserved === true
    && value.validatedUnsigned === true;
}

interface UnsignedStructureEngineResult extends Omit<PdfUnsignedCopyStructuralVerification, 'structurallyReadable'> {
  readonly inputSha256: string;
  readonly structurallyReadable: true;
}

function isUnsignedStructureEngineResult(value: unknown): value is UnsignedStructureEngineResult {
  if (!isRecord(value)) return false;
  const countKeys = [
    'byteRangeMarkerCount', 'signatureDictionaryCount', 'signedSignatureFieldCount',
    'docMdpReferenceCount', 'fieldMdpReferenceCount', 'dssOrVriEntryCount',
  ];
  return hasExactKeys(value, ['inputSha256', 'structurallyReadable', ...countKeys])
    && typeof value.inputSha256 === 'string' && SHA256_PATTERN.test(value.inputSha256)
    && value.structurallyReadable === true
    && countKeys.every((key) => isBoundedCount(value[key]));
}

function isUnsignedStructuralCounts(value: unknown): value is UnsignedCopyEngineResult['structuralPostcheck'] {
  if (!isRecord(value)) return false;
  const keys = [
    'byteRangeMarkerCount', 'signatureDictionaryCount', 'signedSignatureFieldCount',
    'docMdpReferenceCount', 'fieldMdpReferenceCount', 'dssOrVriEntryCount',
  ];
  return hasExactKeys(value, keys) && keys.every((key) => isBoundedCount(value[key]));
}

function isRemovalCounts(value: unknown): value is PdfUnsignedCopyConversionResult['removed'] {
  if (!isRecord(value)) return false;
  const keys = [
    'signatureValues', 'signatureFields', 'signatureWidgets', 'certificationReferences',
    'fieldMdpReferences', 'validationEvidenceEntries',
  ];
  return hasExactKeys(value, keys)
    && keys.every((key) => isNonnegativeSafeInteger(value[key]) && (value[key] as number) <= MAX_REMOVAL_COUNT);
}

function isWarnings(value: unknown): value is readonly string[] {
  return Array.isArray(value)
    && value.length <= MAX_WARNINGS
    && value.every((warning) => typeof warning === 'string' && warning.length <= MAX_WARNING_LENGTH);
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  return actual.length === sortedExpected.length
    && actual.every((key, index) => key === sortedExpected[index]);
}

async function hashFileSha256(filePath: string): Promise<string> {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(filePath)) hash.update(chunk as Buffer);
  return hash.digest('hex');
}

function isNonnegativeSafeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}

function isBoundedCount(value: unknown): value is number {
  return isNonnegativeSafeInteger(value) && value <= 4096;
}

function protocolError(message: string, cause?: unknown): PdfSignatureCoreError {
  return new PdfSignatureCoreError(
    'PROTOCOL_ERROR',
    `PDF signature core protocol error: ${message}.`,
    cause === undefined ? undefined : { cause },
  );
}
