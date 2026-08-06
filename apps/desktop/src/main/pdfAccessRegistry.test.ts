import { mkdtemp, realpath, rename, rm, symlink, truncate, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { PdfAccessRegistry, PdfAccessRegistryError } from './pdfAccessRegistry';

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

describe('PdfAccessRegistry', () => {
  it('requires a trusted source grant and replaces renderer paths with an owner-scoped handle', async () => {
    const sourcePath = await createPdf('source.pdf');
    const registry = createRegistry();

    await expectCode(registry.openAuthorizedSource(1, sourcePath), 'UNAUTHORIZED_SOURCE');
    expect(await registry.authorizeSource(1, sourcePath)).toBe(sourcePath);
    const opened = await registry.openAuthorizedSource(1, sourcePath);
    const canonicalSourcePath = await realpath(sourcePath);

    expect(opened.descriptor).toEqual({ handle: 'pdfdoc_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA' });
    expect(opened.sourcePath).toBe(canonicalSourcePath);
    expect(JSON.stringify(opened.descriptor)).not.toContain(sourcePath);
    await expectCode(registry.resolveDocument(2, opened.descriptor.handle), 'NOT_FOUND');
    await expectCode(registry.resolveDocument(1, 'pdfdoc_BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB'), 'NOT_FOUND');
    expect((await registry.resolveDocument(1, opened.descriptor.handle)).sourcePath).toBe(canonicalSourcePath);
  });

  it('consumes source grants and rejects path injection, symlinks, and stale replacement', async () => {
    const directory = await createDirectory();
    const sourcePath = join(directory, 'source.pdf');
    const otherPath = join(directory, 'other.pdf');
    const sourceLink = join(directory, 'link.pdf');
    await writeFile(sourcePath, '%PDF source');
    await writeFile(otherPath, '%PDF other');
    await symlink(sourcePath, sourceLink);
    const registry = createRegistry();

    await expectCode(registry.authorizeSource(1, sourceLink), 'UNSAFE_SOURCE');
    await registry.authorizeSource(1, sourcePath);
    await expectCode(registry.openAuthorizedSource(1, otherPath), 'UNAUTHORIZED_SOURCE');
    const opened = await registry.openAuthorizedSource(1, sourcePath);
    await expectCode(registry.openAuthorizedSource(1, sourcePath), 'UNAUTHORIZED_SOURCE');
    await writeFile(otherPath, '%PDF source');
    await rename(otherPath, sourcePath);
    await expectCode(registry.resolveDocument(1, opened.descriptor.handle), 'STALE_DOCUMENT');
  });

  it('reads bytes through an identity-bound file descriptor and invalidates a replaced source', async () => {
    const directory = await createDirectory();
    const sourcePath = join(directory, 'source.pdf');
    const replacementPath = join(directory, 'replacement.pdf');
    await writeFile(sourcePath, '%PDF original');
    const registry = createRegistry();
    await registry.authorizeSource(1, sourcePath);
    const opened = await registry.openAuthorizedSource(1, sourcePath);

    expect(Buffer.from(await registry.readDocumentBytes(1, opened.descriptor.handle)).toString()).toBe('%PDF original');
    await writeFile(replacementPath, '%PDF original');
    await rename(replacementPath, sourcePath);

    await expectCode(registry.readDocumentBytes(1, opened.descriptor.handle), 'STALE_DOCUMENT');
    await expectCode(registry.resolveDocument(1, opened.descriptor.handle), 'NOT_FOUND');
  });

  it('issues one-shot owner-scoped Save As target grants and rejects forged or changed targets', async () => {
    const directory = await createDirectory();
    const targetPath = join(directory, 'saved.pdf');
    const registry = createRegistry();
    const target = await registry.authorizeSaveTarget(1, targetPath);
    const canonicalTargetPath = join(await realpath(directory), 'saved.pdf');

    expect(target).toEqual({
      targetHandle: 'pdftarget_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
      displayPath: canonicalTargetPath,
    });
    await expectCode(registry.takeSaveTarget(2, target.targetHandle), 'NOT_FOUND');
    expect(await registry.takeSaveTarget(1, target.targetHandle)).toMatchObject({ targetPath: canonicalTargetPath });
    await expectCode(registry.takeSaveTarget(1, target.targetHandle), 'NOT_FOUND');

    const changed = await registry.authorizeSaveTarget(1, join(directory, 'changed.pdf'));
    await symlink(join(directory, 'elsewhere.pdf'), join(directory, 'changed.pdf'));
    await expectCode(registry.takeSaveTarget(1, changed.targetHandle), 'UNSAFE_TARGET');
  });

  it('releases only matching owner handles and returns temporary cleanup paths on release or owner teardown', async () => {
    const firstPath = await createPdf('first.pdf');
    const secondPath = await createPdf('second.pdf');
    let documentIndex = 0;
    const registry = createRegistry({
      createDocumentHandle: () => `pdfdoc_${(documentIndex++ === 0 ? 'A' : 'B').repeat(32)}`,
    });
    await registry.authorizeSource(1, firstPath, { cleanupOnRelease: true });
    await registry.authorizeSource(1, secondPath, { cleanupOnRelease: true });
    const first = await registry.openAuthorizedSource(1, firstPath);
    const canonicalFirstPath = await realpath(firstPath);
    const canonicalSecondPath = await realpath(secondPath);

    expect(() => registry.releaseDocument(2, first.descriptor.handle)).toThrowError(
      expect.objectContaining({ code: 'NOT_FOUND' }),
    );
    expect(registry.releaseDocument(1, first.descriptor.handle)).toBe(canonicalFirstPath);
    expect(registry.clearOwner(1)).toEqual([canonicalSecondPath]);
  });

  it('tombstones teardown before in-flight authorization can repopulate owner capabilities', async () => {
    const sourcePath = await createPdf('teardown-race.pdf');
    let pauseReached!: () => void;
    let resumeSnapshot!: () => void;
    const reached = new Promise<void>((resolve) => { pauseReached = resolve; });
    const resume = new Promise<void>((resolve) => { resumeSnapshot = resolve; });
    const registry = createRegistry({
      beforeSourceCommitForTesting: async () => {
        pauseReached();
        await resume;
      },
    });

    const authorization = registry.authorizeSource(1, sourcePath);
    await reached;
    expect(registry.clearOwner(1)).toEqual([]);
    resumeSnapshot();
    await expectCode(authorization, 'NOT_FOUND');

    registry.registerOwner(1);
    await expectCode(registry.openAuthorizedSource(1, sourcePath), 'UNAUTHORIZED_SOURCE');
  });

  it('rejects a sparse source above the configured byte limit before authorizing any read', async () => {
    const sourcePath = await createPdf('oversized.pdf');
    await truncate(sourcePath, 513 * 1024 * 1024);
    const registry = createRegistry({ maxSourceBytes: 512 * 1024 * 1024 });

    await expectCode(registry.authorizeSource(1, sourcePath), 'LIMIT_EXCEEDED');
  });
});

function createRegistry(overrides: ConstructorParameters<typeof PdfAccessRegistry>[0] = {}): PdfAccessRegistry {
  const registry = new PdfAccessRegistry({
    createDocumentHandle: () => 'pdfdoc_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
    createTargetHandle: () => 'pdftarget_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
    ...overrides,
  });
  registry.registerOwner(1);
  registry.registerOwner(2);
  return registry;
}

async function createDirectory(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'bp-pdf-access-'));
  temporaryDirectories.push(directory);
  return directory;
}

async function createPdf(name: string): Promise<string> {
  const directory = await createDirectory();
  const sourcePath = join(directory, name);
  await writeFile(sourcePath, '%PDF test');
  return sourcePath;
}

async function expectCode(promise: Promise<unknown>, code: PdfAccessRegistryError['code']): Promise<void> {
  await expect(promise).rejects.toThrowError(expect.objectContaining({ code }));
}
