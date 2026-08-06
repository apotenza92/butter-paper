import { mkdir, mkdtemp, rename, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { assertPdfPublicationDirectory, capturePdfPublicationTarget } from './pdfPublication';

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

describe('PDF publication directory identity', () => {
  it('rejects a selected destination directory that is renamed and replaced', async () => {
    const root = await mkdtemp(join(tmpdir(), 'bp-pdf-publication-'));
    temporaryDirectories.push(root);
    const selectedDirectory = join(root, 'selected');
    const movedDirectory = join(root, 'moved');
    await mkdir(selectedDirectory);
    const target = await capturePdfPublicationTarget(join(selectedDirectory, 'output.pdf'));

    await rename(selectedDirectory, movedDirectory);
    await mkdir(selectedDirectory);

    await expect(assertPdfPublicationDirectory(target.directoryIdentity)).rejects.toThrow(/changed before publication/);
  });
});
