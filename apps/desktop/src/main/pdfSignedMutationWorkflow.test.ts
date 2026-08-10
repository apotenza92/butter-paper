import { createHash } from 'node:crypto';
import { chmod, lstat, mkdtemp, readFile, rm, symlink, unlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createPdfSignedMutation } from './pdfSignedMutationWorkflow';
import { PdfSigningQuarantine } from './pdfSigningQuarantine';

const roots: string[] = [];
const sourceBytes = Buffer.from('%PDF-1.7\nsource-revision\n%%EOF\n');
const incrementalBytes = Buffer.from('\nnew-signature-revision\n%%EOF\n');
const fieldName = 'ButterPaper.Signature.1';
const certificateSha256 = 'c'.repeat(64);

afterEach(async () => {
  vi.restoreAllMocks();
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe('signed PDF mutation workflow', () => {
  it('publishes only a prefix-preserving independently validated private 0600 output', async () => {
    const fixture = await pdfFixture();
    const calls: string[] = [];
    const result = await createPdfSignedMutation({
      ...fixture,
      secureWorkspace: async (path) => {
        calls.push('secure');
        if (process.platform !== 'win32') expect((await lstat(path)).mode & 0o077).toBe(0);
      },
      verifyWorkspace: async () => { calls.push('verify'); },
      mutate: async (request) => {
        calls.push('mutate');
        expect(await readFile(request.inputSnapshotPath)).toEqual(sourceBytes);
        if (process.platform !== 'win32') {
          expect((await lstat(request.inputSnapshotPath)).mode & 0o077).toBe(0);
          expect((await lstat(request.outputPath)).mode & 0o077).toBe(0);
        }
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(request.outputPath, output);
        return engineResult(output);
      },
      postvalidate: async (request) => {
        calls.push('postvalidate');
        expect(await readFile(request.inputSnapshotPath)).toEqual(sourceBytes);
        expect(request.expectedCertificateSha256).toBe(certificateSha256);
        return postvalidation(request.outputSha256);
      },
    });
    expect(result).toMatchObject({ published: true, addedSignatureCount: 1, cryptographicallyValid: true });
    expect(await readFile(fixture.sourcePath)).toEqual(sourceBytes);
    expect(await readFile(fixture.targetPath)).toEqual(Buffer.concat([sourceBytes, incrementalBytes]));
    expect(calls).toEqual(['secure', 'verify', 'verify', 'mutate', 'verify', 'postvalidate', 'verify']);
  });

  it('rejects a symlink source and never invokes the mutation engine', async () => {
    const fixture = await pdfFixture();
    const linked = join(fixture.root, 'linked.pdf');
    await symlink(fixture.sourcePath, linked);
    const mutate = vi.fn();
    await expect(createPdfSignedMutation({
      ...fixture,
      sourcePath: linked,
      mutate,
      postvalidate: vi.fn(),
    })).rejects.toMatchObject({ code: 'UNSAFE_SOURCE' });
    expect(mutate).not.toHaveBeenCalled();
  });

  it('rejects a non-prefix mutation and does not publish it', async () => {
    const fixture = await pdfFixture();
    const bad = Buffer.from('%PDF-1.7\nrewritten-not-incremental\n%%EOF\n');
    await expect(createPdfSignedMutation({
      ...fixture,
      mutate: async ({ outputPath }) => {
        await writeFile(outputPath, bad);
        return engineResult(bad);
      },
      postvalidate: vi.fn(),
    })).rejects.toMatchObject({ code: 'PREFIX_MISMATCH' });
    await expect(lstat(fixture.targetPath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('retains a failed PDF candidate in the main-owned quarantine before cleanup', async () => {
    const fixture = await pdfFixture();
    const quarantine = new PdfSigningQuarantine(join(fixture.root, 'quarantine'), {
      createOpaqueId: () => '11111111-1111-4111-8111-111111111111',
    });
    const bad = Buffer.from('%PDF-1.7\nrewritten-not-incremental\n%%EOF\n');
    await expect(createPdfSignedMutation({
      ...fixture,
      quarantine,
      mutate: async ({ outputPath }) => {
        await writeFile(outputPath, bad);
        return engineResult(bad);
      },
      postvalidate: vi.fn(),
    })).rejects.toMatchObject({ code: 'PREFIX_MISMATCH' });

    const entries = await quarantine.list();
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({ reason: 'PREFIX_MISMATCH', byteLength: bad.byteLength });
    const retained = await quarantine.read(entries[0]!.id);
    expect(Buffer.from(retained.bytes)).toEqual(bad);
    retained.bytes.fill(0);
    await expect(lstat(fixture.targetPath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it.skipIf(process.platform === 'win32')('rejects a sidecar output that becomes group or world readable', async () => {
    const fixture = await pdfFixture();
    await expect(createPdfSignedMutation({
      ...fixture,
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        await chmod(outputPath, 0o644);
        return engineResult(output);
      },
      postvalidate: async ({ outputSha256 }) => postvalidation(outputSha256),
    })).rejects.toMatchObject({ code: 'OUTPUT_UNSAFE' });
    await expect(lstat(fixture.targetPath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('does not publish after cancellation wins before the publication boundary', async () => {
    const fixture = await pdfFixture();
    const controller = new AbortController();
    await expect(createPdfSignedMutation({
      ...fixture,
      signal: controller.signal,
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        controller.abort();
        return engineResult(output);
      },
      postvalidate: async ({ outputSha256 }) => postvalidation(outputSha256),
    })).rejects.toMatchObject({ code: 'CANCELLED' });
    await expect(lstat(fixture.targetPath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('fails closed when the destination races publication and preserves the competing file', async () => {
    const fixture = await pdfFixture();
    const competing = Buffer.from('competing-user-file');
    await expect(createPdfSignedMutation({
      ...fixture,
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        await writeFile(fixture.targetPath, competing);
        return engineResult(output);
      },
      postvalidate: async ({ outputSha256 }) => postvalidation(outputSha256),
    })).rejects.toMatchObject({ code: 'TARGET_EXISTS' });
    expect(await readFile(fixture.targetPath)).toEqual(competing);
    expect(await readFile(fixture.sourcePath)).toEqual(sourceBytes);
  });

  it('rejects a dishonest postvalidator and preserves the original', async () => {
    const fixture = await pdfFixture();
    await expect(createPdfSignedMutation({
      ...fixture,
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        return engineResult(output);
      },
      postvalidate: async ({ outputSha256 }) => ({
        ...postvalidation(outputSha256), cryptographicallyValid: false as never,
      }),
    })).rejects.toMatchObject({ code: 'POSTVALIDATION_FAILED' });
    expect(await readFile(fixture.sourcePath)).toEqual(sourceBytes);
    await expect(lstat(fixture.targetPath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('rejects postvalidation bound to a different signing certificate', async () => {
    const fixture = await pdfFixture();
    await expect(createPdfSignedMutation({
      ...fixture,
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        return engineResult(output);
      },
      postvalidate: async ({ outputSha256 }) => ({
        ...postvalidation(outputSha256),
        certificateSha256: 'd'.repeat(64),
      }),
    })).rejects.toMatchObject({ code: 'POSTVALIDATION_FAILED' });
    await expect(lstat(fixture.targetPath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('rejects undeclared engine and postvalidation fields', async () => {
    const engineFixture = await pdfFixture();
    await expect(createPdfSignedMutation({
      ...engineFixture,
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        return { ...engineResult(output), privateDetail: 'reject-me' };
      },
      postvalidate: async ({ outputSha256 }) => postvalidation(outputSha256),
    })).rejects.toMatchObject({ code: 'OUTPUT_UNSAFE' });

    const validationFixture = await pdfFixture();
    await expect(createPdfSignedMutation({
      ...validationFixture,
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        return engineResult(output);
      },
      postvalidate: async ({ outputSha256 }) => ({ ...postvalidation(outputSha256), privateDetail: 'reject-me' }),
    })).rejects.toMatchObject({ code: 'POSTVALIDATION_FAILED' });
  });

  it('requires and invokes fail-closed Windows workspace ACL hooks', async () => {
    const fixture = await pdfFixture();
    await expect(createPdfSignedMutation({
      ...fixture, platform: 'win32', mutate: vi.fn(), postvalidate: vi.fn(),
    })).rejects.toMatchObject({ code: 'INVALID_REQUEST' });

    const calls: string[] = [];
    await createPdfSignedMutation({
      ...fixture,
      platform: 'win32',
      secureWorkspace: async () => { calls.push('secure'); },
      verifyWorkspace: async () => { calls.push('verify'); },
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        return engineResult(output);
      },
      postvalidate: async ({ outputSha256 }) => postvalidation(outputSha256),
    });
    expect(calls[0]).toBe('secure');
    expect(calls.filter((call) => call === 'verify').length).toBeGreaterThanOrEqual(4);
  });

  it('rolls back only the identity it linked after final readback verification fails', async () => {
    const fixture = await pdfFixture();
    await expect(createPdfSignedMutation({
      ...fixture,
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        return engineResult(output);
      },
      postvalidate: async ({ outputSha256 }) => postvalidation(outputSha256),
      verifyPublished: async () => { throw new Error('installed verifier failed'); },
    })).rejects.toMatchObject({ code: 'PUBLICATION_FAILED' });
    await expect(lstat(fixture.targetPath)).rejects.toMatchObject({ code: 'ENOENT' });

    const second = await pdfFixture();
    const competing = Buffer.from('replacement-after-link');
    await expect(createPdfSignedMutation({
      ...second,
      mutate: async ({ outputPath }) => {
        const output = Buffer.concat([sourceBytes, incrementalBytes]);
        await writeFile(outputPath, output);
        return engineResult(output);
      },
      postvalidate: async ({ outputSha256 }) => postvalidation(outputSha256),
      verifyPublished: async (path) => {
        await unlink(path);
        await writeFile(path, competing);
        throw new Error('replacement race');
      },
    })).rejects.toMatchObject({ code: 'PUBLICATION_FAILED' });
    expect(await readFile(second.targetPath)).toEqual(competing);
  });

  it('surfaces private workspace cleanup failure with a stable pathless error', async () => {
    const fixture = await pdfFixture();
    try {
      await createPdfSignedMutation({
        ...fixture,
        mutate: async ({ outputPath }) => {
          const output = Buffer.concat([sourceBytes, incrementalBytes]);
          await writeFile(outputPath, output);
          return engineResult(output);
        },
        postvalidate: async ({ outputSha256 }) => postvalidation(outputSha256),
        cleanupWorkspace: async () => { throw new Error('private path detail'); },
      });
      throw new Error('expected cleanup failure');
    } catch (error) {
      expect(error).toMatchObject({ code: 'CLEANUP_FAILED' });
      expect(String(error)).not.toContain(fixture.root);
      expect(String(error)).not.toContain('private path detail');
    }
    await expect(lstat(fixture.targetPath)).rejects.toMatchObject({ code: 'ENOENT' });
  });
});

async function pdfFixture() {
  const root = await mkdtemp(join(tmpdir(), 'bp-signed-mutation-'));
  roots.push(root);
  const sourcePath = join(root, 'source.pdf');
  const targetPath = join(root, 'signed.pdf');
  await writeFile(sourcePath, sourceBytes);
  return {
    root,
    sourcePath,
    targetPath,
    expectedSourceSha256: sha256(sourceBytes),
    expectedCertificateSha256: certificateSha256,
    expectedFieldName: fieldName,
  };
}

function engineResult(output: Uint8Array) {
  return {
    inputSha256: sha256(sourceBytes),
    outputSha256: sha256(output),
    fieldName,
    incrementalUpdate: true as const,
    inputPrefixPreserved: true as const,
  };
}

function postvalidation(outputSha256: string) {
  return {
    inputSha256: sha256(sourceBytes),
    outputSha256,
    fieldName,
    certificateSha256,
    inputPrefixPreserved: true as const,
    addedSignatureCount: 1 as const,
    priorSignaturesPreserved: true as const,
    newSignatureCoversOutputExceptContents: true as const,
    cryptographicallyValid: true as const,
    structurallyReadable: true as const,
    independentProcess: true as const,
    validator: 'pdf-signature-core-v1-validate-plus-main-prefix' as const,
  };
}

function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}
