import { createHash } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { symlinkSync, unlinkSync } from 'node:fs';
import { mkdtemp, readFile, realpath, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PassThrough } from 'node:stream';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  PDF_SIGNATURE_PHASE3_CAPABILITIES,
  PdfSignatureCoreSigningClient,
  adaptPdfSignatureCoreSigningResult,
  decodePdfSignatureCoreV2Response,
  encodePdfSignatureCoreV2Request,
} from './pdfSignatureCoreSigning';

const requestId = 'request-1';
const roots: string[] = [];

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe('PDF signature core framed client', () => {
  it('encodes exact v2 binary frames with hashes and no secret payload fields', () => {
    const pfx = Buffer.from('encrypted-pfx');
    const encoded = encodePdfSignatureCoreV2Request({
      requestId,
      operation: 'inspectPkcs12',
      payload: {},
      frames: [{ id: 'pkcs12', kind: 'pkcs12', bytes: pfx, sensitive: true }],
    });
    expect(encoded.subarray(0, 4).toString('ascii')).toBe('BPS2');
    const headerLength = encoded.readUInt32BE(4);
    const header = JSON.parse(encoded.subarray(8, 8 + headerLength).toString('utf8'));
    expect(header).toMatchObject({
      protocolVersion: 2,
      requestId,
      operation: 'inspectPkcs12',
      payload: {},
      frames: [{ id: 'pkcs12', kind: 'pkcs12', byteLength: pfx.byteLength, sensitive: true }],
    });
    expect(header.frames[0].sha256).toMatch(/^[a-f0-9]{64}$/);
    expect(encoded.readUInt32BE(8 + headerLength)).toBe(pfx.byteLength);
    expect(encoded.subarray(12 + headerLength)).toEqual(pfx);
    expect(JSON.stringify(header)).not.toMatch(/password|passphrase|privateKey/i);
    expect(() => encodePdfSignatureCoreV2Request({
      requestId,
      operation: 'sign',
      payload: { password: 'forbidden' },
      frames: [],
    })).toThrow(/payload is invalid/);
    expect(() => encodePdfSignatureCoreV2Request({
      requestId,
      operation: 'sign',
      payload: { ...validSignPayload(), appearance: 'visible', field: {
        kind: 'new', name: 'Signature1',
        widget: { pageIndex: 0, x: 0, y: 0, width: 100, height: 50, pageRotation: 0, coordinateSpace: 'unrotated-pdf-default-user-space' },
      } },
      frames: [
        { id: 'pkcs12', kind: 'pkcs12', bytes: Buffer.from('pfx'), sensitive: true },
        { id: 'appearance', kind: 'appearance', bytes: Buffer.from('png'), sensitive: false },
      ],
    })).toThrow(/appearance frame must be marked sensitive/i);
    expect(() => encodePdfSignatureCoreV2Request({
      requestId,
      operation: 'sign',
      payload: { ...validSignPayload(), pfx: 'secret-alias' },
      frames: [],
    })).toThrow(/payload is invalid/);
  });

  it('keeps mutation capabilities disabled by default', async () => {
    const spawnProcess = vi.fn();
    const client = new PdfSignatureCoreSigningClient(baseOptions({ spawnProcess: spawnProcess as never }));
    await expect(client.sign({} as never, Buffer.from('pfx'))).rejects.toMatchObject({ code: 'CAPABILITY_DISABLED' });
    await expect(client.certify({} as never, Buffer.from('pfx'))).rejects.toMatchObject({ code: 'CAPABILITY_DISABLED' });
    await expect(client.addSignatureField({} as never)).rejects.toMatchObject({ code: 'CAPABILITY_DISABLED' });
    await expect(client.postvalidateSignedMutation(validPostvalidationPayload()))
      .rejects.toMatchObject({ code: 'CAPABILITY_DISABLED' });
    expect(spawnProcess).not.toHaveBeenCalled();
    expect(Object.values(PDF_SIGNATURE_PHASE3_CAPABILITIES)).toEqual(new Array(7).fill(false));
  });

  it('requires a package-bound signing capability handshake before mutation', async () => {
    const fixture = await packageFixture();
    const pkg = {
      ...fixture,
      manifest: { ...fixture.manifest, engineVersion: '0.1.0', javaVersion: '21.0.12' },
    };
    const operations: string[] = [];
    const fake = fakeProcess((child, request) => {
      const header = requestHeader(request);
      operations.push(header.operation);
      child.stdout.end(response(header.requestId, header.operation, {
        capabilities: { ...PDF_SIGNATURE_PHASE3_CAPABILITIES },
        limits: { frameBytes: 1, headerBytes: 1, totalFrameBytes: 1, signingInputBytes: 1 },
        operations: ['handshake', 'inspectPkcs12', 'addSignatureField', 'sign', 'certify', 'postvalidateSignedMutation'],
        profiles: ['PAdES-B-B'],
        providers: ['pkcs12'],
        versions: { engine: '0.1.0', framedProtocol: 2, java: '21.0.12' },
      }));
      child.emit('close', 0, null);
    });
    const spawnProcess = vi.fn(() => fake.child);
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      allowExperimentalProofOperations: true,
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: spawnProcess as never,
      createRequestId: () => requestId,
    });

    await expect(client.sign(validSignPayload() as never, Buffer.from('pfx')))
      .rejects.toMatchObject({ code: 'CAPABILITY_DISABLED' });
    expect(spawnProcess).toHaveBeenCalledTimes(1);
    expect(operations).toEqual(['handshake']);
  });

  it('rejects a capability handshake from a different engine before mutation', async () => {
    const fixture = await packageFixture();
    const pkg = {
      ...fixture,
      manifest: { ...fixture.manifest, engineVersion: '0.1.0', javaVersion: '21.0.12' },
    };
    const operations: string[] = [];
    const fake = fakeProcess((child, request) => {
      const header = requestHeader(request);
      operations.push(header.operation);
      child.stdout.end(response(header.requestId, header.operation, {
        capabilities: {
          ...PDF_SIGNATURE_PHASE3_CAPABILITIES,
          certificateSign: true,
          certify: true,
          signatureFieldCreate: true,
          signatureIncrementalWrite: true,
        },
        limits: { frameBytes: 1, headerBytes: 1, totalFrameBytes: 1, signingInputBytes: 1 },
        operations: ['handshake', 'inspectPkcs12', 'addSignatureField', 'sign', 'certify', 'postvalidateSignedMutation'],
        profiles: ['PAdES-B-B'],
        providers: ['pkcs12'],
        versions: { engine: 'replacement-engine', framedProtocol: 2, java: '21.0.12' },
      }));
      child.emit('close', 0, null);
    });
    const spawnProcess = vi.fn(() => fake.child);
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      allowExperimentalProofOperations: true,
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: spawnProcess as never,
      createRequestId: () => requestId,
    });

    await expect(client.certify(validSignPayload() as never, Buffer.from('pfx')))
      .rejects.toMatchObject({ code: 'CAPABILITY_DISABLED' });
    expect(spawnProcess).toHaveBeenCalledTimes(1);
    expect(operations).toEqual(['handshake']);
  });

  it('rejects an unsealed package even when the native handshake advertises signing', async () => {
    const fixture = await packageFixture();
    const pkg = {
      ...fixture,
      manifest: { ...fixture.manifest, engineVersion: '0.1.0', javaVersion: '21.0.12' },
    };
    const fake = fakeProcess((child, request) => {
      const header = requestHeader(request);
      child.stdout.end(response(header.requestId, header.operation, signingHandshake({
        capabilities: {
          ...PDF_SIGNATURE_PHASE3_CAPABILITIES,
          certificateSign: true,
          certify: true,
          signatureFieldCreate: true,
          signatureIncrementalWrite: true,
        },
      })));
      child.emit('close', 0, null);
    });
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      allowExperimentalProofOperations: true,
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: vi.fn(() => fake.child) as never,
      createRequestId: () => requestId,
    });

    await expect(client.sign(validSignPayload() as never, Buffer.from('pfx')))
      .rejects.toMatchObject({ code: 'CAPABILITY_DISABLED' });
  });

  it('accepts bounded decimal signature-widget geometry without rounding it', () => {
    const widget = {
      pageIndex: 0,
      x: 1.25,
      y: 2.5,
      width: 100.75,
      height: 50.125,
      pageRotation: 90 as const,
      coordinateSpace: 'unrotated-pdf-default-user-space' as const,
    };
    const encoded = encodePdfSignatureCoreV2Request({
      requestId,
      operation: 'sign',
      payload: {
        ...validSignPayload(),
        appearance: 'visible',
        field: { kind: 'new', name: '签名 1', widget },
      },
      frames: [
        { id: 'pkcs12', kind: 'pkcs12', bytes: Buffer.from('pfx'), sensitive: true },
        { id: 'appearance', kind: 'appearance', bytes: Buffer.from('png'), sensitive: true },
      ],
    });
    expect(requestHeader(encoded).payload).toMatchObject({ field: { name: '签名 1', widget } });
  });

  it('rejects handshake capability and nested metadata extensions', async () => {
    const pkg = await packageFixture();
    const baseHandshake = {
      versions: { engine: '0.1.0', framedProtocol: 2, java: '21.0.12' },
      operations: ['handshake', 'inspectPkcs12', 'addSignatureField', 'sign', 'certify', 'postvalidateSignedMutation'],
      profiles: ['PAdES-B-B'], providers: ['pkcs12'],
      capabilities: { ...PDF_SIGNATURE_PHASE3_CAPABILITIES },
      limits: { headerBytes: 1, frameBytes: 1, totalFrameBytes: 1, signingInputBytes: 1 },
    };
    const inspect = async (result: Record<string, unknown>) => {
      const fake = fakeProcess((child, request) => {
        const header = requestHeader(request);
        child.stdout.end(response(header.requestId, header.operation, result));
        child.emit('close', 0, null);
      });
      const client = new PdfSignatureCoreSigningClient({
        ...baseOptions(), resolvePackage: vi.fn(async () => pkg) as never,
        spawnProcess: vi.fn(() => fake.child) as never, createRequestId: () => requestId,
      });
      return client.handshake();
    };
    await expect(inspect({ ...baseHandshake, unexpected: true })).rejects.toMatchObject({ code: 'PROTOCOL_ERROR' });
    await expect(inspect({
      ...baseHandshake, capabilities: { ...baseHandshake.capabilities, unexpected: false },
    })).rejects.toMatchObject({ code: 'PROTOCOL_ERROR' });
    await expect(inspect({
      ...baseHandshake, versions: { ...baseHandshake.versions, unexpected: 'value' },
    })).rejects.toMatchObject({ code: 'PROTOCOL_ERROR' });
  });

  it('launches argument-free, returns a bounded result, and does not surface stderr', async () => {
    const pkg = await packageFixture();
    const secretMarker = 'stderr-secret-marker';
    const fake = fakeProcess((child, request) => {
      const header = requestHeader(request);
      child.stderr.write(secretMarker);
      child.stdout.end(response(header.requestId, header.operation, {
        provider: 'pkcs12',
        identities: [{
          certificateSha256: 'a'.repeat(64),
          subject: 'CN=Signer',
          issuer: 'CN=Issuer',
          serialNumber: '1234',
          validFrom: '2026-01-01T00:00:00Z',
          validTo: '2027-01-01T00:00:00Z',
          keyAlgorithm: 'RSA',
          keyBits: 3072,
          chainSha256: ['a'.repeat(64)],
          supportedDigests: ['SHA-256', 'SHA-384', 'SHA-512'],
          hasPrivateKey: true,
        }],
        passwordRemembered: false,
        privateKeyExported: false,
        engineVersion: '0.1.0',
      }));
      child.emit('close', 0, null);
    });
    const spawnProcess = vi.fn(() => fake.child);
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: spawnProcess as never,
      processEnvironment: { LANG: 'C', HTTPS_PROXY: 'https://secret-proxy', SIGNING_PASSWORD: 'must-not-leak' },
      createRequestId: () => requestId,
    });
    await expect(client.inspectPkcs12(Buffer.from('encrypted-pfx'))).resolves.toMatchObject({ identities: [{}] });
    expect(spawnProcess).toHaveBeenCalledWith(pkg.launcherPath, [], {
      env: { BP_SIGNATURE_NETWORK_DISABLED: '1', NO_PROXY: '*', LANG: 'C' },
      stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true,
    });
    expect(JSON.stringify(spawnProcess.mock.calls)).not.toContain('encrypted-pfx');
    expect(JSON.stringify(spawnProcess.mock.calls)).not.toContain(secretMarker);
  });

  it('rejects undeclared nested PKCS#12 identity fields', async () => {
    const pkg = await packageFixture();
    const fake = fakeProcess((child, request) => {
      const header = requestHeader(request);
      child.stdout.end(response(header.requestId, header.operation, {
        provider: 'pkcs12', passwordRemembered: false, privateKeyExported: false, engineVersion: '0.1.0',
        identities: [{
          certificateSha256: 'a'.repeat(64), subject: 'CN=Signer', issuer: 'CN=Issuer', serialNumber: '1',
          validFrom: '2026-01-01T00:00:00Z', validTo: '2027-01-01T00:00:00Z', keyAlgorithm: 'RSA',
          keyBits: 3072, chainSha256: ['a'.repeat(64)], supportedDigests: ['SHA-256'], hasPrivateKey: true,
          privateKey: 'must-not-be-accepted',
        }],
      }));
      child.emit('close', 0, null);
    });
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(), resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: vi.fn(() => fake.child) as never, createRequestId: () => requestId,
    });
    await expect(client.inspectPkcs12(Buffer.from('encrypted-pfx')))
      .rejects.toMatchObject({ code: 'PROTOCOL_ERROR' });
  });

  it('uses process termination as the cancellation and timeout boundary', async () => {
    const pkg = await packageFixture();
    const cancelled = fakeProcess();
    const controller = new AbortController();
    const cancelSpawn = vi.fn(() => cancelled.child);
    const cancelClient = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: cancelSpawn as never,
      createRequestId: () => requestId,
    });
    const cancelling = cancelClient.inspectPkcs12(Buffer.from('encrypted-pfx'), { signal: controller.signal });
    await vi.waitFor(() => expect(cancelSpawn).toHaveBeenCalled());
    controller.abort();
    await expect(cancelling).rejects.toMatchObject({ code: 'CANCELLED' });
    expect(cancelled.kill).toHaveBeenCalledWith('SIGTERM');

    const timed = fakeProcess();
    const timeoutClient = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: vi.fn(() => timed.child) as never,
      createRequestId: () => requestId,
    });
    await expect(timeoutClient.inspectPkcs12(Buffer.from('encrypted-pfx'), { timeoutMs: 5 }))
      .rejects.toMatchObject({ code: 'TIMEOUT' });
    expect(timed.kill).toHaveBeenCalledWith('SIGTERM');
  });

  it('settles a stubborn child within the forced termination bound and tears down every stream', async () => {
    const pkg = await packageFixture();
    const stubborn = fakeProcess(undefined, true);
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: vi.fn(() => stubborn.child) as never,
      createRequestId: () => requestId,
    });
    await expect(client.inspectPkcs12(Buffer.from('encrypted-pfx'), { timeoutMs: 5 }))
      .rejects.toMatchObject({ code: 'TIMEOUT' });
    expect(stubborn.kill).toHaveBeenCalledWith('SIGTERM');
    expect(stubborn.kill).toHaveBeenCalledWith('SIGKILL');
    expect(stubborn.child.unref).toHaveBeenCalled();
    expect(stubborn.child.stdin.destroyed).toBe(true);
    expect(stubborn.child.stdout.destroyed).toBe(true);
    expect(stubborn.child.stderr.destroyed).toBe(true);
  });

  it('never releases private stdin when timeout wins during spawned-image verification', async () => {
    const pkg = await packageFixture();
    const fake = fakeProcess();
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: vi.fn(() => fake.child) as never,
      verifySpawnedLauncher: async () => new Promise((resolvePromise) => setTimeout(resolvePromise, 20)),
      createRequestId: () => requestId,
    });
    await expect(client.inspectPkcs12(Buffer.from('private-frame'), { timeoutMs: 5 }))
      .rejects.toMatchObject({ code: 'TIMEOUT' });
    expect(fake.inputBytes()).toBe(0);
  });

  it.each(['sign', 'certify', 'postvalidateSignedMutation'] as const)(
    'does not begin %s input or settle successfully when aborted before the input gate',
    async (operation) => {
      const pkg = await packageFixture();
      const fake = fakeProcess();
      const controller = new AbortController();
      let releaseVerification!: () => void;
      const verification = new Promise<void>((resolve) => { releaseVerification = resolve; });
      const spawnProcess = vi.fn(() => fake.child);
      const client = new PdfSignatureCoreSigningClient({
        ...baseOptions(),
        allowExperimentalProofOperations: true,
        resolvePackage: vi.fn(async () => pkg) as never,
        spawnProcess: spawnProcess as never,
        verifySpawnedLauncher: async () => verification,
        createRequestId: () => requestId,
      });

      const pending = operation === 'sign'
        ? client.sign(validSignPayload() as never, Buffer.from('private-frame'), undefined, { signal: controller.signal })
        : operation === 'certify'
          ? client.certify({
              ...validSignPayload(),
              certificationPermission: 'form-filling-and-signatures',
            } as never, Buffer.from('private-frame'), undefined, { signal: controller.signal })
          : client.postvalidateSignedMutation(validPostvalidationPayload(), { signal: controller.signal });
      await vi.waitFor(() => expect(spawnProcess).toHaveBeenCalledTimes(1));
      controller.abort();
      releaseVerification();

      await expect(pending).rejects.toMatchObject({ code: 'CANCELLED' });
      expect(fake.inputBytes()).toBe(0);
    },
  );

  it('rejects launcher replacement after spawn before writing private frame bytes', async () => {
    const pkg = await packageFixture();
    const fake = fakeProcess();
    const replacement = join(pkg.packageRoot, 'replacement');
    await writeFile(replacement, 'replacement-launcher');
    const spawnProcess = vi.fn(() => {
      unlinkSync(pkg.launcherPath);
      symlinkSync(replacement, pkg.launcherPath);
      return fake.child;
    });
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(), resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: spawnProcess as never, createRequestId: () => requestId,
    });
    await expect(client.inspectPkcs12(Buffer.from('private-frame'))).rejects.toMatchObject({ code: 'LAUNCH_FAILED' });
    expect(fake.inputBytes()).toBe(0);
  });

  it.skipIf(process.platform === 'win32')('rejects a spawned process whose executable image is not the verified launcher', async () => {
    const pkg = await packageFixture();
    const fake = fakeProcess();
    fake.child.pid = process.pid;
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      verifySpawnedLauncher: undefined,
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: vi.fn(() => fake.child) as never,
      createRequestId: () => requestId,
    });
    await expect(client.inspectPkcs12(Buffer.from('private-frame'))).rejects.toMatchObject({ code: 'LAUNCH_FAILED' });
    expect(fake.inputBytes()).toBe(0);
  });

  it.skipIf(process.platform === 'win32')('accepts the exact verified executable image before releasing stdin', async () => {
    const launcherPath = await realpath('/bin/cat');
    const bytes = await readFile(launcherPath);
    const info = await stat(launcherPath);
    const pkg = {
      packageRoot: await realpath('/bin'),
      launcherPath,
      manifest: {
        launcher: 'cat',
        components: [{
          path: 'cat', size: info.size, sha256: createHash('sha256').update(bytes).digest('hex'), executable: true,
        }],
      },
    };
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      verifySpawnedLauncher: undefined,
      resolvePackage: vi.fn(async () => pkg) as never,
      createRequestId: () => requestId,
    });
    await expect(client.inspectPkcs12(Buffer.from('private-frame'))).rejects.toMatchObject({ code: 'PROTOCOL_ERROR' });
  });

  it.runIf(process.platform === 'win32')('fails closed before private stdin without a native Windows process-image binding', async () => {
    const pkg = await packageFixture();
    const fake = fakeProcess();
    fake.child.pid = process.pid;
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      verifySpawnedLauncher: undefined,
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: vi.fn(() => fake.child) as never,
      createRequestId: () => requestId,
    });
    await expect(client.inspectPkcs12(Buffer.from('private-frame'))).rejects.toMatchObject({ code: 'LAUNCH_FAILED' });
    expect(fake.inputBytes()).toBe(0);
  });

  it('rejects a symlinked verified launcher before spawning the sidecar', async () => {
    const pkg = await packageFixture();
    const replacement = join(pkg.packageRoot, 'same-launcher-bytes');
    await writeFile(replacement, 'verified-launcher');
    unlinkSync(pkg.launcherPath);
    symlinkSync(replacement, pkg.launcherPath);
    const spawnProcess = vi.fn();
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(), resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: spawnProcess as never, createRequestId: () => requestId,
    });
    await expect(client.inspectPkcs12(Buffer.from('private-frame'))).rejects.toMatchObject({ code: 'LAUNCH_FAILED' });
    expect(spawnProcess).not.toHaveBeenCalled();
  });

  it('adapts only the exact Java signing result and rejects mismatches', () => {
    const expected = {
      inputSha256: 'a'.repeat(64), outputSha256: 'b'.repeat(64), fieldName: 'Signature1',
      certificateSha256: 'c'.repeat(64), kind: 'approval' as const, digestAlgorithm: 'SHA-256' as const,
    };
    const raw = javaSigningResult(expected);
    expect(adaptPdfSignatureCoreSigningResult(raw, expected)).toEqual({
      inputSha256: expected.inputSha256, outputSha256: expected.outputSha256,
      fieldName: expected.fieldName, incrementalUpdate: true, inputPrefixPreserved: true,
    });
    expect(() => adaptPdfSignatureCoreSigningResult({ ...raw, validatedOutput: false }, expected))
      .toThrow(/invalid response/);
    expect(() => adaptPdfSignatureCoreSigningResult({ ...raw, unexpected: true }, expected))
      .toThrow();
  });

  it('frames the exact independent postvalidation request without binary frames', async () => {
    const pkg = await packageFixture();
    const payload = validPostvalidationPayload();
    const result = validPostvalidationResult(payload);
    const expectedRequest = encodePdfSignatureCoreV2Request({
      requestId,
      operation: 'postvalidateSignedMutation',
      payload,
      frames: [],
    });
    const fake = fakeProcess((child, request) => {
      const headerLength = request.readUInt32BE(4);
      const header = requestHeader(request);
      expect(request).toEqual(expectedRequest);
      expect(request.byteLength).toBe(8 + headerLength);
      expect(header).toEqual({
        protocolVersion: 2,
        requestId,
        operation: 'postvalidateSignedMutation',
        payload,
        frames: [],
      });
      child.stdout.end(response(header.requestId, header.operation, result));
      child.emit('close', 0, null);
    });
    const spawnProcess = vi.fn(() => fake.child);
    const client = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      allowExperimentalProofOperations: true,
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: spawnProcess as never,
      createRequestId: () => requestId,
    });
    await expect(client.postvalidateSignedMutation(payload)).resolves.toEqual(result);
    expect(spawnProcess).toHaveBeenCalledTimes(1);
    expect(() => encodePdfSignatureCoreV2Request({
      requestId,
      operation: 'postvalidateSignedMutation',
      payload,
      frames: [{ id: 'pkcs12', kind: 'pkcs12', bytes: Buffer.from('pfx'), sensitive: true }],
    })).toThrow(/payload is invalid/);
  });

  it('accepts only the exact independent postvalidation result bound to the request', async () => {
    const payload = validPostvalidationPayload();
    const valid = validPostvalidationResult(payload);
    const rejected = [
      { ...valid, unexpected: true },
      { ...valid, inputSha256: 'c'.repeat(64) },
      { ...valid, outputSha256: 'd'.repeat(64) },
      { ...valid, fieldName: 'Signature2' },
      { ...valid, certificateSha256: 'e'.repeat(64) },
      { ...valid, independentProcess: false },
      { ...valid, cryptographicallyValid: false },
    ];
    for (const result of rejected) {
      const pkg = await packageFixture();
      const fake = fakeProcess((child, request) => {
        const header = requestHeader(request);
        child.stdout.end(response(header.requestId, header.operation, result));
        child.emit('close', 0, null);
      });
      const client = new PdfSignatureCoreSigningClient({
        ...baseOptions(),
        allowExperimentalProofOperations: true,
        resolvePackage: vi.fn(async () => pkg) as never,
        spawnProcess: vi.fn(() => fake.child) as never,
        createRequestId: () => requestId,
      });
      await expect(client.postvalidateSignedMutation(payload))
        .rejects.toMatchObject({ code: 'PROTOCOL_ERROR' });
    }
  });

  it('rejects invalid postvalidation hashes, field names, and payload extensions before launch', async () => {
    const spawnProcess = vi.fn();
    const client = new PdfSignatureCoreSigningClient(baseOptions({
      allowExperimentalProofOperations: true,
      spawnProcess: spawnProcess as never,
    }));
    const payload = validPostvalidationPayload();
    await expect(client.postvalidateSignedMutation({ ...payload, expectedInputSha256: 'A'.repeat(64) }))
      .rejects.toThrow(/payload is invalid/);
    await expect(client.postvalidateSignedMutation({ ...payload, expectedOutputSha256: 'short' }))
      .rejects.toThrow(/payload is invalid/);
    await expect(client.postvalidateSignedMutation({ ...payload, expectedCertificateSha256: 'short' }))
      .rejects.toThrow(/payload is invalid/);
    await expect(client.postvalidateSignedMutation({ ...payload, expectedFieldName: 'bad\nname' }))
      .rejects.toThrow(/payload is invalid/);
    await expect(client.postvalidateSignedMutation({ ...payload, expectedOperation: 'certification' }))
      .rejects.toThrow(/payload is invalid/);
    await expect(client.postvalidateSignedMutation({ ...payload, expectedCertificationPermission: 'no-changes' }))
      .rejects.toThrow(/payload is invalid/);
    await expect(client.postvalidateSignedMutation({ ...payload, expectedAppearance: 'unknown' as never }))
      .rejects.toThrow(/payload is invalid/);
    await expect(client.postvalidateSignedMutation({ ...payload, secret: 'forbidden' } as never))
      .rejects.toThrow(/payload is invalid/);
    expect(spawnProcess).not.toHaveBeenCalled();
  });

  it('maps postvalidation engine errors generically and cancels the fresh process', async () => {
    const pkg = await packageFixture();
    const payload = validPostvalidationPayload();
    const failed = fakeProcess((child, request) => {
      const header = requestHeader(request);
      child.stdout.end(responseEnvelope({
        protocolVersion: 2,
        requestId: header.requestId,
        operation: header.operation,
        engineVersion: '0.1.0',
        event: 'error',
        ok: false,
        error: { code: 'POSTVALIDATION_FAILED', message: 'sensitive validator detail' },
      }));
      child.emit('close', 0, null);
    });
    const errorClient = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      allowExperimentalProofOperations: true,
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: vi.fn(() => failed.child) as never,
      createRequestId: () => requestId,
    });
    try {
      await errorClient.postvalidateSignedMutation(payload);
      throw new Error('expected failure');
    } catch (caught) {
      expect(caught).toMatchObject({ code: 'ENGINE_ERROR', engineCode: 'POSTVALIDATION_FAILED' });
      expect(String(caught)).not.toContain('sensitive validator detail');
    }

    const cancelled = fakeProcess();
    const controller = new AbortController();
    const cancelSpawn = vi.fn(() => cancelled.child);
    const cancelClient = new PdfSignatureCoreSigningClient({
      ...baseOptions(),
      allowExperimentalProofOperations: true,
      resolvePackage: vi.fn(async () => pkg) as never,
      spawnProcess: cancelSpawn as never,
      createRequestId: () => requestId,
    });
    const pending = cancelClient.postvalidateSignedMutation(payload, { signal: controller.signal });
    await vi.waitFor(() => expect(cancelSpawn).toHaveBeenCalled());
    controller.abort();
    await expect(pending).rejects.toMatchObject({ code: 'CANCELLED' });
    expect(cancelled.kill).toHaveBeenCalledWith('SIGTERM');
  });

  it('rejects trailing bytes, mismatched requests, and engine details with generic errors', () => {
    const valid = response(requestId, 'inspectPkcs12', {});
    expect(() => decodePdfSignatureCoreV2Response(Buffer.concat([valid, Buffer.from([0])]), requestId, 'inspectPkcs12'))
      .toThrow(/invalid response/);
    expect(() => decodePdfSignatureCoreV2Response(response('other', 'inspectPkcs12', {}), requestId, 'inspectPkcs12'))
      .toThrow(/invalid response/);
    const error = responseEnvelope({
      protocolVersion: 2, requestId, operation: 'inspectPkcs12', engineVersion: '0.1.0', event: 'error', ok: false,
      error: { code: 'IDENTITY_REJECTED', message: 'provider detail must not escape' },
    });
    try {
      decodePdfSignatureCoreV2Response(error, requestId, 'inspectPkcs12');
      throw new Error('expected failure');
    } catch (caught) {
      expect(caught).toMatchObject({ code: 'ENGINE_ERROR', engineCode: 'IDENTITY_REJECTED' });
      expect(String(caught)).not.toContain('provider detail');
    }
    expect(() => decodePdfSignatureCoreV2Response(responseEnvelope({
      protocolVersion: 2, requestId, operation: 'inspectPkcs12', engineVersion: '0.1.0',
      event: 'result', ok: false, result: {},
    }), requestId, 'inspectPkcs12')).toThrow(/invalid response/);
    expect(() => decodePdfSignatureCoreV2Response(responseEnvelope({
      protocolVersion: 2, requestId, operation: 'inspectPkcs12', engineVersion: '0.1.0',
      event: 'result', ok: true, result: {}, unexpected: true,
    }), requestId, 'inspectPkcs12')).toThrow(/invalid response/);
  });
});

function fakeProcess(onFinish?: (child: EventEmitter & Record<string, any>, request: Buffer) => void, stubborn = false) {
  const child = new EventEmitter() as EventEmitter & Record<string, any>;
  child.stdin = new PassThrough();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  const chunks: Buffer[] = [];
  child.stdin.on('data', (chunk: Buffer) => chunks.push(Buffer.from(chunk)));
  child.stdin.on('finish', () => queueMicrotask(() => onFinish?.(child, Buffer.concat(chunks))));
  const kill = vi.fn((signal: string) => {
    if (!stubborn) queueMicrotask(() => child.emit('close', null, signal));
    return true;
  });
  child.kill = kill;
  child.unref = vi.fn();
  return { child, kill, inputBytes: () => chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0) };
}

function baseOptions(overrides: Record<string, unknown> = {}) {
  return {
    isPackaged: true,
    resourcesPath: '/unused-resources',
    appPath: '/unused-app',
    verifySpawnedLauncher: async () => undefined,
    ...overrides,
  };
}

async function packageFixture() {
  const createdRoot = await mkdtemp(join(tmpdir(), 'bp-signing-launcher-'));
  roots.push(createdRoot);
  const packageRoot = await realpath(createdRoot);
  const launcherPath = join(packageRoot, 'pdf-signature-core');
  const bytes = Buffer.from('verified-launcher');
  await writeFile(launcherPath, bytes, { mode: 0o700 });
  return {
    packageRoot,
    launcherPath,
    manifest: {
      launcher: 'pdf-signature-core',
      components: [{ path: 'pdf-signature-core', size: bytes.byteLength, sha256: createHash('sha256').update(bytes).digest('hex') }],
    },
  };
}

function validSignPayload() {
  return {
    inputPath: '/private/input.pdf', outputPath: '/private/output.pdf',
    expectedInputSha256: 'a'.repeat(64), certificateSha256: 'b'.repeat(64),
    digestAlgorithm: 'SHA-256', profile: 'PAdES-B-B', appearance: 'invisible',
    field: { kind: 'new', name: 'Signature1', widget: null },
  };
}

function validPostvalidationPayload() {
  return {
    inputPath: '/private/input.pdf',
    outputPath: '/private/output.pdf',
    expectedInputSha256: 'a'.repeat(64),
    expectedOutputSha256: 'b'.repeat(64),
    expectedCertificateSha256: 'c'.repeat(64),
    expectedFieldName: 'Parent.Signature1',
    expectedOperation: 'approval' as const,
    expectedAppearance: 'invisible' as const,
  };
}

function validPostvalidationResult(payload: ReturnType<typeof validPostvalidationPayload>) {
  return {
    inputSha256: payload.expectedInputSha256,
    outputSha256: payload.expectedOutputSha256,
    fieldName: payload.expectedFieldName,
    certificateSha256: payload.expectedCertificateSha256,
    inputPrefixPreserved: true,
    addedSignatureCount: 1,
    priorSignaturesPreserved: true,
    newSignatureCoversOutputExceptContents: true,
    cryptographicallyValid: true,
    structurallyReadable: true,
    independentProcess: true,
    validator: 'pdf-signature-core-v1-validate-plus-main-prefix',
  } as const;
}

function javaSigningResult(expected: ReturnType<typeof signingExpected>) {
  return {
    inputSha256: expected.inputSha256, outputSha256: expected.outputSha256, outputBytes: 200,
    sourcePreserved: true, validatedOutput: true, appendOnly: true, kind: expected.kind,
    profile: 'PAdES-B-B', digestAlgorithm: expected.digestAlgorithm, fieldName: expected.fieldName,
    certificateSha256: expected.certificateSha256, engineVersion: '0.1.0',
    postcheck: {
      appendOnly: true, sourceBytesPreserved: 100, appendedBytes: 100, priorSignatureCount: 0,
      outputSignatureCount: 1, newSignatureByteRange: [0, 50, 100, 100], wholeRevisionCovered: true,
      contentsGapExact: true, fieldBindingExact: true, advancedSignatureBindingExact: true,
      fieldName: expected.fieldName, certificateSha256: expected.certificateSha256,
      certificationPermission: null, cryptographicIntegrity: 'intact',
    },
  };
}

function signingExpected() {
  return {
    inputSha256: '', outputSha256: '', fieldName: '', certificateSha256: '',
    kind: 'approval' as const, digestAlgorithm: 'SHA-256' as const,
  };
}

function requestHeader(request: Buffer): Record<string, any> {
  const length = request.readUInt32BE(4);
  return JSON.parse(request.subarray(8, 8 + length).toString('utf8'));
}

function response(id: string, operation: string, result: Record<string, unknown>): Buffer {
  return responseEnvelope({
    protocolVersion: 2, requestId: id, operation, engineVersion: '0.1.0', event: 'result', ok: true, result,
  });
}

function signingHandshake(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    capabilities: { ...PDF_SIGNATURE_PHASE3_CAPABILITIES },
    limits: { frameBytes: 1, headerBytes: 1, totalFrameBytes: 1, signingInputBytes: 1 },
    operations: ['handshake', 'inspectPkcs12', 'addSignatureField', 'sign', 'certify', 'postvalidateSignedMutation'],
    profiles: ['PAdES-B-B'],
    providers: ['pkcs12'],
    versions: { engine: '0.1.0', framedProtocol: 2, java: '21.0.12' },
    ...overrides,
  };
}

function responseEnvelope(envelope: Record<string, unknown>): Buffer {
  const json = Buffer.from(JSON.stringify(envelope));
  const output = Buffer.alloc(8 + json.byteLength);
  output.write('BPS2', 0, 'ascii');
  output.writeUInt32BE(json.byteLength, 4);
  json.copy(output, 8);
  return output;
}
