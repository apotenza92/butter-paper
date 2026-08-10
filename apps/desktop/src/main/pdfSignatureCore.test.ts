import { createHash } from 'node:crypto';
import { EventEmitter } from 'node:events';
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { PassThrough } from 'node:stream';
import type { spawn } from 'node:child_process';
import { absentValidationEvidence } from '../testFixtures/pdfSignatureValidation';
import {
  PdfSignatureCoreClient,
  PdfSignatureCoreError,
  type PdfSignatureCoreClientOptions,
  scrubSidecarStderr,
} from './pdfSignatureCore';
import {
  PDF_SIGNATURE_CORE_JAVA_VERSION,
  verifyPdfSignatureCorePackageFixtureForTesting,
  type PdfSignatureCoreArchitecture,
  type PdfSignatureCorePlatform,
} from './pdfSignatureCorePackage';
import {
  OFFLINE_SIGNATURE_TRUST_POLICY_ID,
  OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
  offlineSignatureTrustConfigurationSha256,
} from './signatureTrustPolicy';

const platform = process.platform as PdfSignatureCorePlatform;
const arch = process.arch as PdfSignatureCoreArchitecture;
const fixtureSourceBytes = Buffer.from('fixture DSS 6.4 source archive\n');
const fixtureSourceIdentity = {
  bytes: fixtureSourceBytes.byteLength,
  resolvedCommit: 'b'.repeat(40),
  sha256: createHash('sha256').update(fixtureSourceBytes).digest('hex'),
};
const fixturePackageVerifier = (
  packageRoot: string,
  expected: { platform: NodeJS.Platform; arch: string },
) => verifyPdfSignatureCorePackageFixtureForTesting(packageRoot, expected, fixtureSourceIdentity);

describe('PDF signature core main-process client', () => {
  const temporaryRoots: string[] = [];

  afterEach(() => {
    vi.useRealTimers();
    for (const root of temporaryRoots.splice(0)) rmSync(root, { recursive: true, force: true });
  });

  it('accepts the Java golden handshake while preserving a secret-free, argument-free boundary', async () => {
    const fixture = createFixture(temporaryRoots);
    const spawned: Array<{ file: string; args: readonly string[]; options: Record<string, unknown> }> = [];
    const client = fixture.client((child, request, file, args, options) => {
      spawned.push({ file, args, options });
      child.result(request, goldenHandshake(request.requestId));
    }, { PATH: '/secret/tool-path', SIGNING_PASSWORD: 'never-pass-this', TMPDIR: '/tmp' });

    await expect(client.handshake()).resolves.toMatchObject({
      versions: { engine: '0.1.0', protocol: 1, javaFeature: 21 },
      capabilities: { inspect: true, certificateSign: false },
    });
    expect(spawned).toHaveLength(1);
    expect(spawned[0]?.args).toEqual([]);
    expect(spawned[0]?.file).toBe(realpathSync(fixture.launcherPath));
    expect(spawned[0]?.options).toMatchObject({
      cwd: realpathSync(fixture.packageRoot),
      shell: false,
      windowsHide: true,
      env: {
        BP_SIGNATURE_CORE_NETWORK: 'disabled',
        TMPDIR: '/tmp',
      },
    });
    expect(JSON.stringify(spawned[0]?.options)).not.toContain('never-pass-this');
    expect(JSON.stringify(spawned[0]?.options)).not.toContain('/secret/tool-path');
  });

  it('fails closed when the running launcher cannot be bound to the verified package image', async () => {
    const fixture = createFixture(temporaryRoots);
    let receivedRequest = false;
    const client = fixture.client(
      () => {
        receivedRequest = true;
      },
      {},
      async () => {
        throw new Error('launcher mapping mismatch');
      },
    );

    await expect(client.handshake()).rejects.toMatchObject({ code: 'LAUNCH_FAILED' });
    expect(receivedRequest).toBe(false);
  });

  it.each([
    ['certificateSign', true],
    ['certify', true],
    ['createUnsignedCopy', false],
    ['ltv', true],
    ['onlineValidation', true],
    ['pkcs11', true],
    ['signatureRead', false],
    ['signatureValidation', false],
    ['signedIncrementalEdit', true],
    ['unexpectedCapability', false],
  ])('rejects a non-Phase-1 capability contract: %s=%s', async (capability, enabled) => {
    const fixture = createFixture(temporaryRoots);
    const client = fixture.client((child, request) => {
      const handshake = goldenHandshake(request.requestId);
      child.result(request, {
        ...handshake,
        capabilities: { ...handshake.capabilities, [capability]: enabled },
      });
    });

    await expect(client.handshake()).rejects.toThrow(/does not match the packaged engine/);
  });

  it('re-verifies the package immediately before every operation', async () => {
    const fixture = createFixture(temporaryRoots);
    let spawnCount = 0;
    const client = fixture.client((child, request) => {
      spawnCount += 1;
      child.result(request, goldenHandshake(request.requestId));
    });
    await expect(client.handshake()).resolves.toBeDefined();
    writeFileSync(fixture.runtimePath, 'tampered runtime\n');

    await expect(client.handshake()).rejects.toThrow(/runtime\/release (?:size|checksum)/);
    expect(spawnCount).toBe(1);
  });

  it('passes a trusted canonical input path only over stdin and accepts structural-only inspection', async () => {
    const fixture = createFixture(temporaryRoots);
    let capturedRequest: RequestEnvelope | undefined;
    const client = fixture.client((child, request) => {
      capturedRequest = request;
      child.result(request, inspectResult());
    });

    await expect(client.inspectFile(fixture.inputPath)).resolves.toEqual(inspectResult());
    expect(capturedRequest).toMatchObject({
      protocolVersion: 1,
      operation: 'inspect',
      payload: { inputPath: realpathSync(fixture.inputPath) },
    });
  });

  it('validates offline with an explicit network flag and binds the report to current bytes', async () => {
    const fixture = createFixture(temporaryRoots);
    let capturedRequest: RequestEnvelope | undefined;
    const client = fixture.client((child, request) => {
      capturedRequest = request;
      child.result(request, validationResult(fixture.inputPath));
    });

    await expect(client.validateFile(fixture.inputPath)).resolves.toMatchObject({
      validationMode: 'offline',
      validationTimeProvenance: 'observed-system-utc',
      inventory: { presence: 'unsigned' },
      trust: { onlineSourcesUsed: false },
    });
    expect(capturedRequest).toMatchObject({
      operation: 'validate',
      payload: { inputPath: realpathSync(fixture.inputPath), onlineValidation: false },
    });
  });

  it('sends a fixed reference clock only over stdin and requires an exact returned binding', async () => {
    const fixture = createFixture(temporaryRoots);
    const fixedReferenceValidationTime = '2026-08-05T00:00:00Z';
    let capturedRequest: RequestEnvelope | undefined;
    const client = fixture.client((child, request) => {
      capturedRequest = request;
      child.result(request, {
        ...validationResult(fixture.inputPath),
        validationTime: fixedReferenceValidationTime,
        validationTimeProvenance: 'caller-supplied-fixed-reference',
      });
    });

    await expect(client.validateFile(fixture.inputPath, {
      fixedReferenceValidationTime,
    })).resolves.toMatchObject({
      validationTime: fixedReferenceValidationTime,
      validationTimeProvenance: 'caller-supplied-fixed-reference',
    });
    expect(capturedRequest?.payload).toMatchObject({
      validationClock: { mode: 'fixed-reference', instant: fixedReferenceValidationTime },
    });

    for (const result of [
      { ...validationResult(fixture.inputPath), validationTime: '2026-08-04T00:00:00Z' },
      { ...validationResult(fixture.inputPath), validationTimeProvenance: 'observed-system-utc' },
    ]) {
      const mismatched = fixture.client((child, request) => child.result(request, result));
      await expect(mismatched.validateFile(fixture.inputPath, { fixedReferenceValidationTime }))
        .rejects.toMatchObject({ code: 'VALIDATION_CLOCK_MISMATCH' });
    }
  });

  it.each([
    ['malformed-byte-range', 'indeterminate', 'unable-to-classify', 'malformed'],
    ['changed-signed-byte', 'signed', 'prohibited', 'whole-relevant-revision'],
    ['corrupt-incremental-revision', 'indeterminate', 'unable-to-classify', 'malformed'],
  ] as const)(
    'accepts the raw Java hostile-report contract for %s without strengthening its axes',
    async (_caseId, presence, modificationStatus, coverage) => {
      const fixture = createFixture(temporaryRoots);
      const client = fixture.client((child, request) => {
        child.result(request, hostileValidationResult(
          fixture.inputPath,
          presence,
          modificationStatus,
          coverage,
        ));
      });

      const result = await client.validateFile(fixture.inputPath);

      expect(result.inventory).toMatchObject({
        presence,
        revisionInventoryComplete: presence === 'signed',
        modificationPolicyComplete: false,
      });
      expect(result.signatures[0]).toMatchObject({
        integrity: 'failed',
        identityTrust: 'indeterminate',
        certificateStatus: 'indeterminate',
        signingTime: 'indeterminate',
        modificationStatus,
        coverage,
        evidenceFreshness: {
          source: 'indeterminate',
          producedAt: null,
          nextUpdateAt: null,
        },
      });
      expect(result).not.toHaveProperty('valid');
      expect(JSON.stringify(result)).not.toMatch(/identity (?:is )?trusted/i);
    },
  );

  it('sends public exact-certificate trust only over stdin and rejects a mismatched trust report', async () => {
    const fixture = createFixture(temporaryRoots);
    const certificateDer = Uint8Array.from([1, 2, 3, 4]);
    const fingerprint = createHash('sha256').update(certificateDer).digest('hex');
    const configurationSha256 = offlineSignatureTrustConfigurationSha256([fingerprint]);
    const trustPolicy = {
      policyId: OFFLINE_SIGNATURE_TRUST_POLICY_ID,
      policyVersion: OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
      configurationSha256,
      exactCertificateAnchors: [{ sha256Fingerprint: fingerprint, certificateDer }],
    } as const;
    let capturedRequest: RequestEnvelope | undefined;
    const matching = fixture.client((child, request) => {
      capturedRequest = request;
      child.result(request, {
        ...validationResult(fixture.inputPath),
        trust: {
          ...validationResult(fixture.inputPath).trust,
          policyId: OFFLINE_SIGNATURE_TRUST_POLICY_ID,
          policyVersion: OFFLINE_SIGNATURE_TRUST_POLICY_VERSION,
          configurationSha256,
          configuredExactCertificateFingerprints: [fingerprint],
        },
      });
    });

    await expect(matching.validateFile(fixture.inputPath, { trustPolicy })).resolves.toMatchObject({
      trust: { configurationSha256, configuredExactCertificateFingerprints: [fingerprint] },
    });
    expect(capturedRequest?.payload).toMatchObject({
      trustPolicy: {
        configurationSha256,
        exactCertificateAnchors: [{
          sha256Fingerprint: fingerprint,
          derBase64: Buffer.from(certificateDer).toString('base64'),
        }],
      },
    });

    const mismatched = fixture.client((child, request) => child.result(request, validationResult(fixture.inputPath)));
    await expect(mismatched.validateFile(fixture.inputPath, { trustPolicy })).rejects.toThrow(/trust-policy snapshot/i);
  });

  it('creates an unsigned copy only through existing main-owned files and binds every digest and policy field', async () => {
    const fixture = createFixture(temporaryRoots);
    const outputPath = join(dirname(fixture.inputPath), 'unsigned-output.pdf');
    writeFileSync(outputPath, '');
    let capturedRequest: RequestEnvelope | undefined;
    const client = fixture.client((child, request) => {
      capturedRequest = request;
      const outputBytes = Buffer.from('%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n%%EOF\n');
      writeFileSync(outputPath, outputBytes);
      child.result(request, unsignedCopyResult(fixture.inputPath, outputBytes));
    });

    await expect(client.createUnsignedCopy(
      fixture.inputPath,
      outputPath,
      createHash('sha256').update(readFileSync(fixture.inputPath)).digest('hex'),
    )).resolves.toMatchObject({
      engineVersion: '0.1.0',
      removalPolicyId: 'butter-paper-structurally-unsigned-copy',
      removalPolicyVersion: 1,
      removed: { signatureValues: 1 },
    });
    expect(capturedRequest).toMatchObject({
      operation: 'createUnsignedCopy',
      payload: {
        inputPath: realpathSync(fixture.inputPath),
        outputPath: realpathSync(outputPath),
      },
    });
    expect(JSON.stringify(capturedRequest)).not.toContain('password');
  });

  it('rejects unsigned-copy output, source, policy, and provenance mismatches', async () => {
    const variants = [
      { inputSha256: 'f'.repeat(64) },
      { outputSha256: 'f'.repeat(64) },
      { removalPolicyId: 'unreviewed' },
      { engineVersion: '9.9.9' },
      { structuralPostcheck: { ...cleanStructuralCounts(), signatureDictionaryCount: 1 } },
    ];
    for (const variant of variants) {
      const fixture = createFixture(temporaryRoots);
      const outputPath = join(dirname(fixture.inputPath), `unsigned-${temporaryRoots.length}.pdf`);
      writeFileSync(outputPath, '');
      const outputBytes = Buffer.from('%PDF-1.7\n%%EOF\n');
      const client = fixture.client((child, request) => {
        writeFileSync(outputPath, outputBytes);
        child.result(request, { ...unsignedCopyResult(fixture.inputPath, outputBytes), ...variant });
      });
      await expect(client.createUnsignedCopy(
        fixture.inputPath,
        outputPath,
        createHash('sha256').update(readFileSync(fixture.inputPath)).digest('hex'),
      )).rejects.toMatchObject({ code: 'PROTOCOL_ERROR' });
    }
  });

  it('independently inspects unsigned structure in a fresh operation and rejects stale evidence', async () => {
    const fixture = createFixture(temporaryRoots);
    const matching = fixture.client((child, request) => child.result(request, {
      inputSha256: createHash('sha256').update(readFileSync(fixture.inputPath)).digest('hex'),
      structurallyReadable: true,
      ...cleanStructuralCounts(),
    }));
    await expect(matching.inspectUnsignedStructure(fixture.inputPath)).resolves.toEqual({
      structurallyReadable: true,
      ...cleanStructuralCounts(),
    });

    const stale = fixture.client((child, request) => child.result(request, {
      inputSha256: 'f'.repeat(64),
      structurallyReadable: true,
      ...cleanStructuralCounts(),
    }));
    await expect(stale.inspectUnsignedStructure(fixture.inputPath)).rejects.toMatchObject({ code: 'PROTOCOL_ERROR' });
  });

  it('rejects stale reports and never silently downgrades an authorized online request', async () => {
    const fixture = createFixture(temporaryRoots);
    const stale = fixture.client((child, request) => {
      child.result(request, { ...validationResult(fixture.inputPath), inputSha256: '0'.repeat(64) });
    });
    await expect(stale.validateFile(fixture.inputPath)).rejects.toThrow(/does not describe the current PDF bytes/);

    const offlineOnly = fixture.client((child, request) => {
      child.result(request, validationResult(fixture.inputPath));
    });
    await expect(offlineOnly.validateFile(fixture.inputPath, { onlineValidationAuthorized: true }))
      .rejects.toThrow(/did not perform the explicitly requested online refresh/);

    const outOfBounds = fixture.client((child, request) => child.result(request, {
      ...validationResult(fixture.inputPath),
      validationTime: '2020-01-01T00:00:00Z',
    }));
    await expect(outOfBounds.validateFile(fixture.inputPath))
      .rejects.toMatchObject({ code: 'VALIDATION_CLOCK_MISMATCH' });
  });

  it('rejects version skew, malformed JSON, mismatched IDs, incomplete lines, and returned paths', async () => {
    const cases: Array<[string, (child: FakeChild, request: RequestEnvelope) => void, RegExp]> = [
      ['version skew', (child, request) => child.line({
        ...terminalEnvelope(request),
        protocolVersion: 2,
        result: inspectResult(),
      }), /mismatched protocol metadata/],
      ['malformed', (child) => child.raw('{not json}\n'), /malformed JSON/],
      ['wrong request', (child, request) => child.line({
        ...terminalEnvelope(request),
        requestId: 'other',
        result: inspectResult(),
      }), /mismatched protocol metadata/],
      ['incomplete', (child) => {
        child.raw('{"unfinished":true}');
        child.close(0);
      }, /incomplete NDJSON line/],
      ['path leak', (child, request) => child.result(request, { ...inspectResult(), outputPath: '/private/output.pdf' }), /expose a filesystem path/],
    ];

    for (const [, behavior, expectation] of cases) {
      const fixture = createFixture(temporaryRoots);
      const client = fixture.client(behavior);
      await expect(client.inspectFile(fixture.inputPath)).rejects.toThrow(expectation);
    }
  });

  it.each([
    'path', 'file', 'pathname', 'inputPath', 'outputPath', 'filePath',
    'sourcePath', 'targetPath', 'canonicalPath',
    'sourceFile', 'source_path', 'targetFile', 'target_file', 'canonicalFile', 'fileName', 'directoryPath',
  ])('rejects returned filesystem path key %s', async (pathKey) => {
    const fixture = createFixture(temporaryRoots);
    const client = fixture.client((child, request) => child.result(request, {
      ...inspectResult(),
      [pathKey]: '/private/sidecar-secret/document.pdf',
    }));

    await expect(client.inspectFile(fixture.inputPath))
      .rejects.toThrow(/expose a filesystem path/);
  });

  it('rejects oversized and over-deep messages before deserializing them', async () => {
    const oversizedFixture = createFixture(temporaryRoots);
    const oversized = oversizedFixture.client((child) => child.raw(`${'x'.repeat(1024 * 1024 + 1)}\n`));
    await expect(oversized.inspectFile(oversizedFixture.inputPath)).rejects.toThrow(/larger than 1 MiB/);

    const deepFixture = createFixture(temporaryRoots);
    const deep = deepFixture.client((child) => child.raw(`${'['.repeat(33)}${']'.repeat(33)}\n`));
    await expect(deep.inspectFile(deepFixture.inputPath)).rejects.toThrow(/nesting-depth/);
  });

  it('rejects malformed UTF-8 instead of accepting replacement characters', async () => {
    const fixture = createFixture(temporaryRoots);
    const client = fixture.client((child) => child.rawBytes(Buffer.from([0x7b, 0xff, 0x7d, 0x0a])));

    await expect(client.inspectFile(fixture.inputPath)).rejects.toThrow(/invalid UTF-8/);
  });

  it('continues kill escalation after an early protocol rejection', async () => {
    const fixture = createFixture(temporaryRoots);
    let child: FakeChild | undefined;
    const client = fixture.client((createdChild) => {
      child = createdChild;
      createdChild.ignoredSignals.add('SIGTERM');
      createdChild.rawBytes(Buffer.from([0xff, 0x0a]));
    });

    await expect(client.inspectFile(fixture.inputPath)).rejects.toThrow(/invalid UTF-8/);
    await waitFor(() => child?.killSignals.includes('SIGKILL') === true, 600);
    expect(child?.killSignals).toEqual(['SIGTERM', 'SIGKILL']);
  });

  it('surfaces structured engine errors without treating stderr as protocol', async () => {
    const fixture = createFixture(temporaryRoots);
    const client = fixture.client((child, request) => child.errorResult(request, {
      code: 'MALFORMED_PDF',
      message: 'The document cannot be inspected.\nTry another copy.',
    }));

    await expect(client.inspectFile(fixture.inputPath)).rejects.toMatchObject({
      code: 'ENGINE_ERROR',
      engineCode: 'MALFORMED_PDF',
      message: 'MALFORMED_PDF: The document cannot be inspected. Try another copy.',
    });
  });

  it('redacts request paths from structured engine error messages', async () => {
    const fixture = createFixture(temporaryRoots);
    const client = fixture.client((child, request) => child.errorResult(request, {
      code: 'MALFORMED_PDF',
      message: `Unable to inspect ${fixture.inputPath} (document.pdf).`,
    }));

    const error = await client.inspectFile(fixture.inputPath).catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(PdfSignatureCoreError);
    expect((error as Error).message).toContain('[REDACTED]');
    expect((error as Error).message).not.toContain(fixture.inputPath);
    expect((error as Error).message).not.toContain('document.pdf');
  });

  it('turns a malformed terminal engine error into a protocol rejection', async () => {
    const fixture = createFixture(temporaryRoots);
    const client = fixture.client((child, request) => child.errorResult(request, {
      code: 'lowercase-is-invalid',
      message: 42,
    }));

    await expect(client.inspectFile(fixture.inputPath)).rejects.toMatchObject({
      code: 'PROTOCOL_ERROR',
      message: expect.stringMatching(/error payload is malformed/),
    });
  });

  it('scrubs document paths from bounded crash diagnostics', async () => {
    const fixture = createFixture(temporaryRoots);
    const client = fixture.client((child) => {
      child.stderr.write(`failed to read ${fixture.inputPath}\n`);
      child.close(7);
    });

    const error = await client.inspectFile(fixture.inputPath).catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(PdfSignatureCoreError);
    expect(error).toMatchObject({ code: 'PROCESS_CRASHED' });
    expect((error as Error).message).toContain('[REDACTED]');
    expect((error as Error).message).not.toContain(fixture.inputPath);
    expect((error as Error).message).not.toContain('document.pdf');
  });

  it('uses protocol cancellation then terminates the one-operation process', async () => {
    const fixture = createFixture(temporaryRoots);
    const controller = new AbortController();
    let child: FakeChild | undefined;
    const client = fixture.client((createdChild) => {
      child = createdChild;
    });
    const operation = client.inspectFile(fixture.inputPath, { signal: controller.signal });
    await waitFor(() => child !== undefined);

    controller.abort();

    await expect(operation).rejects.toMatchObject({ code: 'CANCELLED' });
    expect(child?.received.map((request) => request.operation)).toEqual(['inspect', 'cancel']);
    expect(child?.received[1]?.payload).toEqual({ targetRequestId: child?.received[0]?.requestId });
    expect(child?.killSignals[0]).toBe('SIGTERM');
  });

  it('times out a hung process and never reports success', async () => {
    const fixture = createFixture(temporaryRoots);
    const client = fixture.client(() => {});

    await expect(client.inspectFile(fixture.inputPath, { timeoutMs: 10 })).rejects.toMatchObject({ code: 'TIMEOUT' });
  });

  it('escalates cancellation to SIGKILL when SIGTERM is ignored', async () => {
    const fixture = createFixture(temporaryRoots);
    const controller = new AbortController();
    let child: FakeChild | undefined;
    const client = fixture.client((createdChild) => {
      child = createdChild;
      createdChild.ignoredSignals.add('SIGTERM');
    });
    const operation = client.inspectFile(fixture.inputPath, { signal: controller.signal });
    await waitFor(() => child !== undefined);
    controller.abort();

    await expect(operation).rejects.toMatchObject({ code: 'CANCELLED' });
    expect(child?.killSignals).toEqual(['SIGTERM', 'SIGKILL']);
  });

  it('settles cancellation even when forced termination is not confirmed', async () => {
    const fixture = createFixture(temporaryRoots);
    const controller = new AbortController();
    let child: FakeChild | undefined;
    const client = fixture.client((createdChild) => {
      child = createdChild;
      createdChild.ignoredSignals.add('SIGTERM');
      createdChild.ignoredSignals.add('SIGKILL');
    });
    const operation = client.inspectFile(fixture.inputPath, { signal: controller.signal });
    await waitFor(() => child !== undefined);
    controller.abort();

    await expect(operation).rejects.toMatchObject({
      code: 'CANCELLED',
      message: expect.stringMatching(/without confirmed process termination/),
    });
    expect(child?.killSignals).toEqual(['SIGTERM', 'SIGKILL']);
    expect(child?.unrefCount).toBe(1);
  });

  it('settles a timeout even when forced termination is not confirmed', async () => {
    const fixture = createFixture(temporaryRoots);
    let child: FakeChild | undefined;
    const client = fixture.client((createdChild) => {
      child = createdChild;
      createdChild.ignoredSignals.add('SIGTERM');
      createdChild.ignoredSignals.add('SIGKILL');
    });

    await expect(client.inspectFile(fixture.inputPath, { timeoutMs: 50 })).rejects.toMatchObject({
      code: 'TIMEOUT',
      message: expect.stringMatching(/did not confirm termination/),
    });
    expect(child?.killSignals).toEqual(['SIGTERM', 'SIGKILL']);
    expect(child?.unrefCount).toBe(1);
  });

  it('rejects a crash, an absent terminal response, and multiple terminal responses', async () => {
    const fixtures = [createFixture(temporaryRoots), createFixture(temporaryRoots), createFixture(temporaryRoots)];
    await expect(fixtures[0]!.client((child) => child.close(9)).inspectFile(fixtures[0]!.inputPath))
      .rejects.toMatchObject({ code: 'PROCESS_CRASHED' });
    await expect(fixtures[1]!.client((child) => child.close(0)).inspectFile(fixtures[1]!.inputPath))
      .rejects.toThrow(/without a terminal event/);
    await expect(fixtures[2]!.client((child, request) => {
      child.line({ ...terminalEnvelope(request), result: inspectResult() }, false);
      child.result(request, inspectResult());
    }).inspectFile(fixtures[2]!.inputPath)).rejects.toThrow(/after a terminal event/);
  });

  it('scrubs ANSI, control characters, and line injection in isolation', () => {
    expect(scrubSidecarStderr('\u001b[31msecret\u001b[0m\nnext\u0000', ['secret'])).toBe('[REDACTED] next');
  });
});

interface RequestEnvelope {
  protocolVersion: 1;
  requestId: string;
  operation: string;
  payload: Record<string, unknown>;
}

class FakeChild extends EventEmitter {
  readonly stdin = new PassThrough();
  readonly stdout = new PassThrough();
  readonly stderr = new PassThrough();
  readonly received: RequestEnvelope[] = [];
  readonly killSignals: NodeJS.Signals[] = [];
  readonly ignoredSignals = new Set<NodeJS.Signals>();
  unrefCount = 0;
  exitCode: number | null = null;
  signalCode: NodeJS.Signals | null = null;
  private input = '';
  private closed = false;

  constructor(
    private readonly behavior: (
      child: FakeChild,
      request: RequestEnvelope,
      file: string,
      args: readonly string[],
      options: Record<string, unknown>,
    ) => void,
    private readonly file: string,
    private readonly args: readonly string[],
    private readonly options: Record<string, unknown>,
  ) {
    super();
    this.stdin.on('data', (chunk: Buffer) => {
      this.input += chunk.toString('utf8');
      let newline = this.input.indexOf('\n');
      while (newline >= 0) {
        const line = this.input.slice(0, newline);
        this.input = this.input.slice(newline + 1);
        const request = JSON.parse(line) as RequestEnvelope;
        this.received.push(request);
        if (request.operation !== 'cancel') queueMicrotask(() => this.behavior(this, request, file, args, options));
        newline = this.input.indexOf('\n');
      }
    });
  }

  kill(signal: NodeJS.Signals = 'SIGTERM'): boolean {
    this.killSignals.push(signal);
    if (!this.ignoredSignals.has(signal)) queueMicrotask(() => this.close(null, signal));
    return true;
  }

  unref(): void {
    this.unrefCount += 1;
  }

  line(value: unknown, close = true): void {
    this.stdout.write(`${JSON.stringify(value)}\n`);
    if (close) queueMicrotask(() => this.close(0));
  }

  raw(value: string): void {
    this.stdout.write(value);
  }

  rawBytes(value: Uint8Array): void {
    this.stdout.write(value);
  }

  result(request: RequestEnvelope, result: unknown): void {
    this.line({ ...terminalEnvelope(request), result });
  }

  errorResult(request: RequestEnvelope, error: unknown): void {
    this.line({ ...terminalEnvelope(request), event: 'error', error });
  }

  close(code: number | null, signal: NodeJS.Signals | null = null): void {
    if (this.closed) return;
    this.closed = true;
    this.exitCode = code;
    this.signalCode = signal;
    this.stdout.end();
    this.stderr.end();
    this.emit('close', code, signal);
  }
}

function createFixture(temporaryRoots: string[]) {
  if (!['darwin', 'win32', 'linux'].includes(platform) || !['arm64', 'x64'].includes(arch)) {
    throw new Error(`Unsupported test host ${platform}-${arch}`);
  }
  const developmentRoot = mkdtempSync(join(tmpdir(), 'bp-signature-client-'));
  temporaryRoots.push(developmentRoot);
  const packageRoot = join(developmentRoot, `${platform}-${arch}`);
  const launcher = launcherFor(platform);
  const sourceBytes = fixtureSourceBytes;
  const sourceSha256 = createHash('sha256').update(sourceBytes).digest('hex');
  const jarBytes = Buffer.from('fixture DSS dependency jar\n');
  const jarSha256 = createHash('sha256').update(jarBytes).digest('hex');
  const licenceBytes = Buffer.from('fixture LGPL licence text\n');
  const licenceSha256 = createHash('sha256').update(licenceBytes).digest('hex');
  const dssCoordinate = 'eu.europa.ec.joinup.sd-dss:dss-example:6.4';
  const sourcePolicy = `${JSON.stringify({
    schemaVersion: 1,
    licenses: {
      Example: {
        acceptedSbomLicenses: ['GNU Lesser General Public License'],
        file: 'LGPL-2.1.txt',
        sourceUrl: 'https://www.gnu.org/licenses/old-licenses/lgpl-2.1.txt',
        sha256: licenceSha256,
      },
    },
    correspondingSources: [{
      archiveRoot: 'dss-6.4/',
      bytes: sourceBytes.byteLength,
      file: 'dss-6.4-source.tar.gz',
      packagePath: 'source/upstream/dss-6.4-source.tar.gz',
      requiredCoordinatePrefix: 'eu.europa.ec.joinup.sd-dss:',
      resolvedCommit: 'b'.repeat(40),
      sha256: sourceSha256,
      sourceUrl: 'https://github.com/esig/dss/archive/refs/tags/6.4.tar.gz',
      version: '6.4',
    }],
    components: [{ coordinate: dssCoordinate, jar: 'dss-example-6.4.jar', license: 'Example' }],
  }, null, 2)}\n`;
  const dependencyInventory = `${JSON.stringify({
    schemaVersion: 1,
    componentCount: 1,
    allComponentsHaveDeclaredAndHashedEvidence: true,
    correspondingSourceCount: 1,
    allRequiredCorrespondingSourcesPresentAndHashed: true,
    correspondingSources: [{
      archiveRoot: 'dss-6.4/',
      bytes: sourceBytes.byteLength,
      coveredComponents: [dssCoordinate],
      evidenceFile: 'source/upstream/dss-6.4-source.tar.gz',
      evidenceSha256: sourceSha256,
      resolvedCommit: 'b'.repeat(40),
      sourceUrl: 'https://github.com/esig/dss/archive/refs/tags/6.4.tar.gz',
      version: '6.4',
    }],
    legalApproval: false,
    components: [{
      acceptedSbomLicences: ['GNU Lesser General Public License'],
      coordinate: dssCoordinate,
      declaredSbomLicences: ['GNU Lesser General Public License'],
      evidenceFile: 'licenses/LGPL-2.1.txt',
      evidenceSha256: licenceSha256,
      evidenceSourceUrl: 'https://www.gnu.org/licenses/old-licenses/lgpl-2.1.txt',
      jar: 'dss-example-6.4.jar',
      jarSha256,
      retainedJarNotices: [],
    }],
  }, null, 2)}\n`;
  const sbom = `${JSON.stringify({
    bomFormat: 'CycloneDX',
    components: [{
      group: 'eu.europa.ec.joinup.sd-dss',
      name: 'dss-example',
      version: '6.4',
      hashes: [{ alg: 'SHA-256', content: jarSha256 }],
      licenses: [{ license: { name: 'GNU Lesser General Public License' } }],
    }],
  }, null, 2)}\n`;
  const butterSources = [
    'AuthoritativeSignedPolicies.java',
    'BoundedLineReader.java',
    'EngineVersions.java',
    'ExactTrustPolicy.java',
    'FramedProtocolServer.java',
    'InspectionService.java',
    'LicenseEvidenceVerifier.java',
    'Main.java',
    'PackageManifestWriter.java',
    'Pkcs12IdentityService.java',
    'Pkcs12PasswordPrompt.java',
    'Protocol.java',
    'ProtocolServer.java',
    'SafePdfMutation.java',
    'SecretScrubber.java',
    'SignatureFieldService.java',
    'SignatureFieldSpec.java',
    'SignedMutationPostcheck.java',
    'SignedMutationPostvalidationService.java',
    'SigningService.java',
    'UnsignedCopyService.java',
    'ValidationService.java',
  ].map((fileName) => ({
    path: `com/butterpaper/signaturecore/${fileName}`,
    bytes: Buffer.from(`package com.butterpaper.signaturecore; final class ${fileName.replace('.java', '')} {}\n`),
  }));
  const butterTree = createHash('sha256');
  for (const source of butterSources) {
    butterTree.update(`${Buffer.byteLength(source.path)}:`);
    butterTree.update(source.path);
    butterTree.update(`\0${source.bytes.byteLength}:`);
    butterTree.update(source.bytes);
    butterTree.update('\0');
  }
  const sourceDescriptor = `${JSON.stringify({
    schemaVersion: 1,
    distributionId: 'butter-paper-pdf-signature-core-complete-source-v1',
    product: 'Butter Paper',
    core: { name: 'pdf-signature-core', version: '0.1.0' },
    delivery: {
      kind: 'distribution-wide-sibling',
      canonicalFileName: `butter-paper-pdf-signature-core-complete-source-v1-0.1.0-${'a'.repeat(16)}.tar.gz`,
      distributionRelativePath: `pdf-signature-core/source/butter-paper-pdf-signature-core-complete-source-v1-0.1.0-${'a'.repeat(16)}.tar.gz`,
      authenticationRequirement: 'same-release-signed-manifest-or-tuf-target',
      retentionRequirement: 'immutable-and-co-retained-for-corresponding-binary-lifetime',
      requiredForRedistribution: true,
      packageLocalSourcePresent: false,
    },
    artifact: {
      bytes: 4096,
      format: 'tar.gz',
      rootDirectory: 'butter-paper-pdf-signature-core-complete-source-v1-0.1.0',
      sha256: 'a'.repeat(64),
    },
    internalManifest: { path: 'SOURCE-MANIFEST.json', sha256: 'c'.repeat(64) },
    runtimeNotices: [{
      bytes: licenceBytes.byteLength,
      path: 'notices/dependencies/licenses/LGPL-2.1.txt',
      sha256: licenceSha256,
    }],
    sourceIdentity: {
      butter: {
        algorithm: 'sha256-path-length-path-nul-byte-length-bytes-nul-v1',
        fileCount: butterSources.length,
        sha256: butterTree.digest('hex'),
      },
      dss: {
        bytes: sourceBytes.byteLength,
        resolvedCommit: 'b'.repeat(40),
        sha256: sourceSha256,
        version: '6.4',
      },
      policySha256: createHash('sha256').update(sourcePolicy).digest('hex'),
      sbomSha256: createHash('sha256').update(sbom).digest('hex'),
      dependencyInventorySha256: createHash('sha256').update(dependencyInventory).digest('hex'),
    },
    legalApproval: false,
    releaseSealed: false,
  }, null, 2)}\n`;
  const files: Array<[string, Buffer | string, boolean?]> = [
    [launcher, nativeHeader(platform, arch), platform !== 'win32'],
    [libraryJarFor(platform), jarBytes],
    ['complete-source-artifact.json', sourceDescriptor],
    ['notices/dependencies/licenses/LGPL-2.1.txt', licenceBytes],
    ['notices/LGPL-RELINKING.md', 'fixture relinking instructions\n'],
    ['notices/MIT.txt', 'fixture MIT licence\n'],
    ['notices/THIRD-PARTY-NOTICES.md', 'fixture notices\n'],
    ['notices/MICROSOFT-OPENJDK-LICENSE.txt', 'fixture Microsoft OpenJDK licence\n'],
    ['runtime/release', `JAVA_VERSION="${PDF_SIGNATURE_CORE_JAVA_VERSION}"\n`],
    ['sbom/pdf-signature-core.cdx.json', sbom],
  ];
  const components = files.map(([relativePath, contents, executable]) => {
    const absolutePath = join(packageRoot, relativePath);
    mkdirSync(dirname(absolutePath), { recursive: true });
    writeFileSync(absolutePath, contents);
    if (executable) chmodSync(absolutePath, 0o755);
    const bytes = readFileSync(absolutePath);
    return {
      path: relativePath,
      sha256: createHash('sha256').update(bytes).digest('hex'),
      size: bytes.byteLength,
      executable: executable === true,
    };
  }).sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
  writeFileSync(join(packageRoot, 'manifest.json'), `${JSON.stringify({
    schemaVersion: 2,
    protocolVersion: 1,
    engineVersion: '0.1.0',
    javaVersion: PDF_SIGNATURE_CORE_JAVA_VERSION,
    platform,
    arch,
    launcher,
    buildState: 'unsigned-build',
    immutableComponents: components.filter((component) => component.path !== launcher),
    signingMutableComponents: [{ path: launcher, reason: 'platform-signing' }],
    signingMutablePathRules: ['**/_CodeSignature/**', '**/CodeResources'],
    postSignInventory: 'post-sign-inventory.json',
    postSignInventoryRequiredForRelease: true,
    postSignTrust: 'requires-enclosing-signed-or-tuf-verified-package',
  }, null, 2)}\n`);
  writeFileSync(join(packageRoot, 'post-sign-inventory.json'), `${JSON.stringify({
    schemaVersion: 1,
    manifestSha256: createHash('sha256').update(readFileSync(join(packageRoot, 'manifest.json'))).digest('hex'),
    platform,
    arch,
    evidenceState: 'post-nested-signing-unsealed',
    releaseSealed: false,
    components,
    trustRequirement: 'must-be-covered-by-enclosing-signed-or-tuf-verified-package',
  }, null, 2)}\n`);
  const inputPath = join(developmentRoot, 'document.pdf');
  const runtimePath = join(packageRoot, 'runtime/release');
  writeFileSync(inputPath, '%PDF-1.7\n%%EOF\n');
  const launcherPath = join(packageRoot, launcher);
  return {
    packageRoot,
    launcherPath,
    inputPath,
    runtimePath,
    client(
      behavior: ConstructorParameters<typeof FakeChild>[0],
      processEnvironment: NodeJS.ProcessEnv = {},
      verifySpawnedLauncher: PdfSignatureCoreClientOptions['verifySpawnedLauncher'] = async () => undefined,
    ) {
      const spawnProcess = ((file: string, args: readonly string[], options: Record<string, unknown>) => (
        new FakeChild(behavior, file, args, options)
      )) as unknown as typeof spawn;
      return new PdfSignatureCoreClient({
        isPackaged: false,
        resourcesPath: '/unused',
        appPath: '/unused',
        developmentRoot,
        platform,
        arch,
        packageVerifier: fixturePackageVerifier,
        processEnvironment,
        spawnProcess,
        verifySpawnedLauncher,
      });
    },
  };
}

function terminalEnvelope(request: RequestEnvelope) {
  return {
    protocolVersion: 1,
    requestId: request.requestId,
    operation: request.operation,
    engineVersion: '0.1.0',
    event: 'result',
  };
}

function goldenHandshake(requestId: string) {
  return {
    capabilities: {
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
    },
    limits: { inputBytes: 536870912, jsonDepth: 32, lineBytes: 1048576, requestIdLength: 128 },
    operations: [
      'handshake', 'version', 'inspect', 'validate', 'createUnsignedCopy',
      'inspectUnsignedStructure', 'cancel',
    ],
    profiles: [],
    providers: [],
    versions: {
      dss: '6.4',
      engine: '0.1.0',
      jackson: '2.21.1',
      java: '21.0.12+8-LTS',
      javaFeature: 21,
      pdfBox: '3.0.6',
      protocol: 1,
    },
    requestId,
  };
}

function cleanStructuralCounts() {
  return {
    byteRangeMarkerCount: 0,
    signatureDictionaryCount: 0,
    signedSignatureFieldCount: 0,
    docMdpReferenceCount: 0,
    fieldMdpReferenceCount: 0,
    dssOrVriEntryCount: 0,
  };
}

function unsignedCopyResult(inputPath: string, outputBytes: Buffer) {
  return {
    engineVersion: '0.1.0',
    inputSha256: createHash('sha256').update(readFileSync(inputPath)).digest('hex'),
    outputSha256: createHash('sha256').update(outputBytes).digest('hex'),
    outputBytes: outputBytes.byteLength,
    pageCount: 1,
    removalPolicyId: 'butter-paper-structurally-unsigned-copy',
    removalPolicyVersion: 1,
    removed: {
      signatureValues: 1,
      signatureFields: 1,
      signatureWidgets: 1,
      certificationReferences: 0,
      fieldMdpReferences: 0,
      validationEvidenceEntries: 0,
    },
    structuralPostcheck: cleanStructuralCounts(),
    warnings: ['Signatures were removed from this new copy.'],
    sourcePreserved: true,
    validatedUnsigned: true,
  };
}

function inspectResult() {
  return {
    byteLength: 17,
    byteRangeMarkerCount: 0,
    eofMarkerPresent: true,
    inputSha256: createHash('sha256').update('%PDF-1.7\n%%EOF\n').digest('hex'),
    signatureDictionaryMarkerCount: 0,
    signatureFieldMarkerCount: 0,
    startsWithPdfHeader: true,
    structuralOnly: true as const,
    validationPerformed: false as const,
    warning: 'Phase 0 inspection does not establish signature presence, integrity, identity, trust, or validity.',
  };
}

function validationResult(inputPath: string) {
  return {
    schemaVersion: 1,
    inputSha256: createHash('sha256').update(readFileSync(inputPath)).digest('hex'),
    validationMode: 'offline',
    validationTime: new Date().toISOString().replace('.000Z', 'Z'),
    validationTimeProvenance: 'observed-system-utc',
    engineVersion: '0.1.0',
    inventory: {
      presence: 'unsigned',
      certificationPermission: 'not-certified',
      currentRevision: 1,
      totalRevisions: 1,
      revisionInventoryComplete: true,
      fields: [],
      modificationPolicyComplete: true,
      validationEvidence: absentValidationEvidence,
    },
    signatures: [],
    trust: {
      policyId: 'butter-paper-local-explicit-certificates',
      policyVersion: 1,
      configurationSha256: '65621a8373d3e6869d50a8572da7d20ae5c4d7c91a915eeda34493187f071f0e',
      policyName: 'dss-6.4-offline-no-trust-store',
      configuredExactCertificateFingerprints: [],
      onlineSourcesUsed: false,
      limitations: ['No online evidence was requested.'],
    },
    limitations: [],
    issues: [],
  };
}

function hostileValidationResult(
  inputPath: string,
  presence: 'signed' | 'indeterminate',
  modificationStatus: 'prohibited' | 'unable-to-classify',
  coverage: 'whole-relevant-revision' | 'malformed',
) {
  const base = validationResult(inputPath);
  const revisionInventoryComplete = presence === 'signed';
  return {
    ...base,
    inventory: {
      presence,
      certificationPermission: 'not-certified',
      currentRevision: revisionInventoryComplete ? 1 : null,
      totalRevisions: revisionInventoryComplete ? 1 : null,
      revisionInventoryComplete,
      fields: [{
        id: 'field-1',
        name: 'Signature1',
        signed: true,
        widgets: [{ pageIndex: 0, rect: [0, 0, 0, 0] }],
      }],
      modificationPolicyComplete: false,
      validationEvidence: absentValidationEvidence,
    },
    signatures: [{
      id: 'signature-1',
      fieldNames: ['Signature1'],
      kind: 'approval',
      signedRevision: revisionInventoryComplete ? 1 : null,
      byteRange: {
        segments: coverage === 'malformed' ? [] : [[0, 10], [20, 10]],
        coveredRevisionEnd: coverage === 'malformed' ? null : 30,
        structurallyValid: coverage !== 'malformed',
      },
      transforms: [],
      certificates: [],
      timestamps: [],
      signerClaim: null,
      claimedSigningTime: null,
      integrity: 'failed',
      identityTrust: 'indeterminate',
      certificateStatus: 'indeterminate',
      signingTime: 'indeterminate',
      modificationStatus,
      coverage,
      evidenceFreshness: { source: 'indeterminate', producedAt: null, nextUpdateAt: null },
      qualification: {
        padesProfile: 'unknown',
        claimedCompliant: false,
        limitations: ['Profile material was inventoried without an online trust or revocation decision.'],
      },
      fieldLock: null,
      issues: [],
    }],
    issues: coverage === 'malformed' ? [{
      code: 'INVALID_BYTE_RANGE',
      severity: 'error',
      message: 'A malformed signature byte range makes the PDF revision inventory unreliable.',
    }] : [],
  };
}

function launcherFor(targetPlatform: PdfSignatureCorePlatform): string {
  if (targetPlatform === 'darwin') return 'pdf-signature-core.app/Contents/MacOS/pdf-signature-core';
  if (targetPlatform === 'win32') return 'pdf-signature-core/pdf-signature-core.exe';
  return 'pdf-signature-core/bin/pdf-signature-core';
}

function libraryJarFor(targetPlatform: PdfSignatureCorePlatform): string {
  if (targetPlatform === 'darwin') return 'pdf-signature-core.app/Contents/app/lib/dss-example-6.4.jar';
  if (targetPlatform === 'win32') return 'pdf-signature-core/app/lib/dss-example-6.4.jar';
  return 'pdf-signature-core/lib/app/lib/dss-example-6.4.jar';
}

function nativeHeader(targetPlatform: PdfSignatureCorePlatform, targetArch: PdfSignatureCoreArchitecture): Buffer {
  const header = Buffer.alloc(128);
  if (targetPlatform === 'linux') {
    Buffer.from([0x7f, 0x45, 0x4c, 0x46]).copy(header);
    header[5] = 1;
    header.writeUInt16LE(targetArch === 'arm64' ? 0xb7 : 0x3e, 18);
  } else if (targetPlatform === 'win32') {
    header.write('MZ', 0, 'ascii');
    header.writeUInt32LE(64, 0x3c);
    header.write('PE\0\0', 64, 'ascii');
    header.writeUInt16LE(targetArch === 'arm64' ? 0xaa64 : 0x8664, 68);
  } else {
    header.writeUInt32BE(0xcffaedfe, 0);
    header.writeUInt32LE(targetArch === 'arm64' ? 0x0100000c : 0x01000007, 4);
  }
  return header;
}

async function waitFor(predicate: () => boolean, timeoutMs = 100): Promise<void> {
  for (let attempt = 0; attempt < timeoutMs; attempt += 1) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 1));
  }
  throw new Error('Timed out waiting for test condition.');
}
