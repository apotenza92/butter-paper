import { mkdtemp, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  PdfSigningIdentityRegistry,
  PdfSigningIdentityRegistryError,
  type PdfSigningCertificateDescriptor,
} from './pdfSigningIdentityRegistry';

const roots: string[] = [];
const handle = '123e4567-e89b-42d3-a456-426614174000';
const certificate: PdfSigningCertificateDescriptor = {
  sha256Fingerprint: 'a'.repeat(64),
  serialNumber: '1234',
  subjectDisplayName: 'Signing certificate',
  issuerDisplayName: 'Test issuer',
  notBefore: '2026-01-01T00:00:00Z',
  notAfter: '2027-01-01T00:00:00Z',
  publicKeyAlgorithm: 'RSA',
  publicKeyBits: 3072,
  supportedDigests: ['SHA-256', 'SHA-384', 'SHA-512'],
  suitableForSigning: true,
};
const inspectedIdentity = {
  certificateSha256: certificate.sha256Fingerprint,
  subject: certificate.subjectDisplayName,
  issuer: certificate.issuerDisplayName,
  serialNumber: '1234',
  validFrom: certificate.notBefore,
  validTo: certificate.notAfter,
  keyAlgorithm: certificate.publicKeyAlgorithm,
  keyBits: certificate.publicKeyBits,
  chainSha256: [certificate.sha256Fingerprint],
  supportedDigests: ['SHA-256', 'SHA-384', 'SHA-512'] as const,
  hasPrivateKey: true as const,
};

afterEach(async () => {
  vi.restoreAllMocks();
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe('PDF signing identity registry', () => {
  it('returns only an opaque window-bound expiring handle and clears transient frame copies', async () => {
    const fixture = await identityFixture();
    let now = Date.parse('2026-08-06T00:00:00.000Z');
    let consumed: Uint8Array | undefined;
    const inspector = { inspectPkcs12: vi.fn(async (frame: Uint8Array) => {
      expect(Buffer.from(frame)).toEqual(fixture.bytes);
      return inspectionResult();
    }) };
    const registry = new PdfSigningIdentityRegistry(
      { pickPkcs12File: vi.fn(async () => ({ canceled: false as const, filePath: fixture.path })) },
      inspector,
      { now: () => now, createOpaqueHandle: () => handle, handleTtlMs: 1000 },
    );

    const selection = await registry.choose(7);
    expect(selection).toEqual({
      handle,
      expiresAt: '2026-08-06T00:00:01.000Z',
      certificates: [certificate],
    });
    expect(JSON.stringify(selection)).not.toContain(fixture.path);
    await registry.withPkcs12Frame(handle, 7, async (frame) => {
      consumed = frame;
      expect(Buffer.from(frame)).toEqual(fixture.bytes);
      return undefined;
    });
    expect([...consumed!]).toEqual(new Array(fixture.bytes.byteLength).fill(0));
    expect(() => registry.describe(handle, 8)).toThrowError(PdfSigningIdentityRegistryError);
    now += 1001;
    expect(() => registry.describe(handle, 7)).toThrow(/expired/);
  });

  it('treats picker cancellation as cancellation without inspecting any identity', async () => {
    const inspector = { inspectPkcs12: vi.fn() };
    const registry = new PdfSigningIdentityRegistry(
      { pickPkcs12File: vi.fn(async () => ({ canceled: true as const })) },
      inspector,
    );
    await expect(registry.choose(1)).resolves.toBeNull();
    expect(inspector.inspectPkcs12).not.toHaveBeenCalled();
  });

  it('rejects symlinks and revokes a capability when its selected file changes', async () => {
    const fixture = await identityFixture();
    const link = join(fixture.root, 'linked.pfx');
    await symlink(fixture.path, link);
    const inspector = { inspectPkcs12: vi.fn(async () => inspectionResult()) };
    const unsafe = new PdfSigningIdentityRegistry(
      { pickPkcs12File: async () => ({ canceled: false, filePath: link }) },
      inspector,
    );
    await expect(unsafe.choose(1)).rejects.toMatchObject({ code: 'UNSAFE_IDENTITY_FILE' });

    const registry = new PdfSigningIdentityRegistry(
      { pickPkcs12File: async () => ({ canceled: false, filePath: fixture.path }) },
      inspector,
      { createOpaqueHandle: () => handle },
    );
    await registry.choose(1);
    await writeFile(fixture.path, Buffer.from('changed-pkcs12'));
    await expect(registry.withPkcs12Frame(handle, 1, async () => undefined))
      .rejects.toMatchObject({ code: 'IDENTITY_CHANGED' });
    expect(() => registry.describe(handle, 1)).toThrow(/invalid/);
  });

  it('honors an already-aborted request without opening the picker', async () => {
    const picker = { pickPkcs12File: vi.fn() };
    const registry = new PdfSigningIdentityRegistry(picker, { inspectPkcs12: vi.fn() });
    const controller = new AbortController();
    controller.abort();
    await expect(registry.choose(1, { signal: controller.signal })).rejects.toMatchObject({ name: 'AbortError' });
    expect(picker.pickPkcs12File).not.toHaveBeenCalled();
  });

  it('does not mint a handle when cancellation races the sidecar inspector result', async () => {
    const fixture = await identityFixture();
    let finishInspection!: () => void;
    const inspectionGate = new Promise<void>((resolve) => { finishInspection = resolve; });
    const inspector = { inspectPkcs12: vi.fn(async () => {
      await inspectionGate;
      return inspectionResult();
    }) };
    const registry = new PdfSigningIdentityRegistry(
      { pickPkcs12File: async () => ({ canceled: false, filePath: fixture.path }) },
      inspector,
      { createOpaqueHandle: () => handle },
    );
    const controller = new AbortController();
    const choosing = registry.choose(1, { signal: controller.signal });
    await vi.waitFor(() => expect(inspector.inspectPkcs12).toHaveBeenCalled());
    controller.abort();
    finishInspection();
    await expect(choosing).rejects.toMatchObject({ name: 'AbortError' });
    expect(() => registry.describe(handle, 1)).toThrow(/invalid/);
  });

  it('does not mint a handle when the owner generation is invalidated during the picker', async () => {
    const fixture = await identityFixture();
    let finishPicker!: () => void;
    let pickerStarted!: () => void;
    const pickerGate = new Promise<void>((resolve) => { finishPicker = resolve; });
    const pickerStartedGate = new Promise<void>((resolve) => { pickerStarted = resolve; });
    const inspector = { inspectPkcs12: vi.fn(async () => inspectionResult()) };
    const registry = new PdfSigningIdentityRegistry(
      { pickPkcs12File: vi.fn(async () => {
        pickerStarted();
        await pickerGate;
        return { canceled: false as const, filePath: fixture.path };
      }) },
      inspector,
      { createOpaqueHandle: () => handle },
    );
    const lease = registry.registerOwner(1);
    const choosing = registry.choose(1, { generation: lease.generation });
    await pickerStartedGate;
    registry.revokeWindow(1);
    const replacementLease = registry.registerOwner(1);
    registry.revokeWindow(1, lease.generation);
    expect(registry.isOwnerGenerationActive(1, replacementLease.generation)).toBe(true);
    finishPicker();

    await expect(choosing).rejects.toMatchObject({ code: 'OWNER_UNAVAILABLE' });
    expect(inspector.inspectPkcs12).not.toHaveBeenCalled();
    expect(() => registry.describe(handle, 1)).toThrow(/invalid/);
  });

  it('invalidates an in-flight identity frame when its owner generation is revoked', async () => {
    const fixture = await identityFixture();
    const registry = new PdfSigningIdentityRegistry(
      { pickPkcs12File: async () => ({ canceled: false as const, filePath: fixture.path }) },
      { inspectPkcs12: async () => inspectionResult() },
      { createOpaqueHandle: () => handle },
    );
    const lease = registry.registerOwner(1);
    await registry.choose(1, { generation: lease.generation });

    let finishConsume!: () => void;
    let consumeStarted!: () => void;
    const consumeGate = new Promise<void>((resolve) => { finishConsume = resolve; });
    const consumeStartedGate = new Promise<void>((resolve) => { consumeStarted = resolve; });
    const consuming = registry.withPkcs12Frame(
      handle,
      1,
      async () => {
        consumeStarted();
        await consumeGate;
        return 'signed';
      },
      { generation: lease.generation },
    );
    await consumeStartedGate;
    registry.revokeWindow(1);
    finishConsume();

    await expect(consuming).rejects.toMatchObject({ code: 'OWNER_UNAVAILABLE' });
    expect(() => registry.describe(handle, 1)).toThrow(/invalid/);
  });

  it('rejects undeclared nested identity metadata from the inspector', async () => {
    const fixture = await identityFixture();
    const registry = new PdfSigningIdentityRegistry(
      { pickPkcs12File: async () => ({ canceled: false, filePath: fixture.path }) },
      { inspectPkcs12: async () => ({
        ...inspectionResult(),
        identities: [{ ...inspectedIdentity, privateKeyMaterial: 'must-not-cross-boundary' }],
      }) as never },
      { createOpaqueHandle: () => handle },
    );
    await expect(registry.choose(1)).rejects.toMatchObject({ code: 'INSPECTION_FAILED' });
    expect(() => registry.describe(handle, 1)).toThrow(/invalid/);
  });

  it('retains an unsupported Java identity as explicitly unsuitable for signing', async () => {
    const fixture = await identityFixture();
    const registry = new PdfSigningIdentityRegistry(
      { pickPkcs12File: async () => ({ canceled: false, filePath: fixture.path }) },
      { inspectPkcs12: async () => ({
        ...inspectionResult(),
        identities: [{ ...inspectedIdentity, keyAlgorithm: 'DSA', supportedDigests: [] }],
      }) as never },
      { createOpaqueHandle: () => handle },
    );
    const selection = await registry.choose(1);
    expect(selection?.certificates).toEqual([{
      ...certificate,
      publicKeyAlgorithm: 'DSA',
      supportedDigests: [],
      suitableForSigning: false,
    }]);
  });
});

async function identityFixture() {
  const root = await mkdtemp(join(tmpdir(), 'bp-signing-identity-'));
  roots.push(root);
  const path = join(root, 'identity.p12');
  const bytes = Buffer.from('bounded-encrypted-pkcs12-fixture');
  await writeFile(path, bytes, { mode: 0o600 });
  return { root, path, bytes };
}

function inspectionResult() {
  return {
    provider: 'pkcs12' as const,
    identities: [inspectedIdentity],
    passwordRemembered: false as const,
    privateKeyExported: false as const,
    engineVersion: '0.1.0',
  };
}
