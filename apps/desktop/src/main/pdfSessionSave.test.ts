import { chmod, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { PDFDocument } from 'pdf-lib';
import { saveDocumentPayload } from './pdfSession';

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => rm(directory, {
    recursive: true,
    force: true,
  })));
});

describe('PDF session saving', () => {
  it('atomically replaces an opened PDF during a normal save', async () => {
    const directory = await createTemporaryDirectory();
    const source = join(directory, 'opened.pdf');
    const pdf = await PDFDocument.create();
    pdf.addPage([320, 480]);
    await writeFile(source, await pdf.save());
    await chmod(source, 0o640);

    const result = await saveDocumentPayload(source, [], 'save', undefined, [], [{
      pageIndex: 0,
      rotation: 90,
    }]);

    const saved = await PDFDocument.load(await readFile(source));
    expect(result.path).toBe(source);
    expect(result.bytesWritten).toBeGreaterThan(0);
    expect(saved.getPage(0).getRotation().angle).toBe(90);
    if (process.platform !== 'win32') {
      expect((await stat(source)).mode & 0o777).toBe(0o640);
    }
    expect((await readdir(directory)).filter((name) => name.startsWith('.butter-paper-save-'))).toEqual([]);
  });

  it('keeps Save As non-destructive when the destination already exists', async () => {
    const directory = await createTemporaryDirectory();
    const source = join(directory, 'opened.pdf');
    const destination = join(directory, 'existing.pdf');
    const pdf = await PDFDocument.create();
    pdf.addPage([320, 480]);
    await writeFile(source, await pdf.save());
    await writeFile(destination, 'keep this file');

    await expect(saveDocumentPayload(source, [], 'saveAs', destination)).rejects.toThrow(
      'Save As will not replace an existing destination.',
    );
    expect(await readFile(destination, 'utf8')).toBe('keep this file');
  });
});

async function createTemporaryDirectory(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'bp-pdf-session-save-'));
  temporaryDirectories.push(directory);
  return directory;
}
