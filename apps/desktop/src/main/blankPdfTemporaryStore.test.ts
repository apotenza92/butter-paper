import { mkdir, mkdtemp, readFile, realpath, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PDFDocument } from 'pdf-lib';
import { afterEach, describe, expect, it } from 'vitest';
import { BlankPdfTemporaryStore } from './blankPdfTemporaryStore';

const stores: BlankPdfTemporaryStore[] = [];

afterEach(async () => {
  await Promise.all(stores.splice(0).map((store) => store.cleanup()));
});

describe('BlankPdfTemporaryStore', () => {
  it('creates uniquely named one-page PDFs and releases them safely', async () => {
    const root = await mkdtemp(join(tmpdir(), 'butter-paper-blank-test-'));
    const store = new BlankPdfTemporaryStore(root);
    stores.push(store);

    const first = await store.create({ widthMm: 210, heightMm: 297 });
    const second = await store.create({ widthMm: 297, heightMm: 210 });
    expect(first.fileName).toBe('Untitled.pdf');
    expect(second.fileName).toBe('Untitled 2.pdf');
    expect(first.filePath).toBe(await realpath(first.filePath));

    const document = await PDFDocument.load(await readFile(second.filePath));
    expect(document.getPageCount()).toBe(1);
    expect(document.getPage(0).getWidth()).toBeGreaterThan(document.getPage(0).getHeight());

    await store.release(first.temporarySourcePath);
    await expect(stat(first.temporarySourcePath)).rejects.toMatchObject({ code: 'ENOENT' });
    await expect(stat(second.temporarySourcePath)).resolves.toBeDefined();
  });

  it('refuses to release paths outside its session directory', async () => {
    const root = await mkdtemp(join(tmpdir(), 'butter-paper-blank-test-'));
    const store = new BlankPdfTemporaryStore(root);
    stores.push(store);
    await store.create({ widthMm: 210, heightMm: 297 });

    await expect(store.release(join(root, 'unrelated.pdf'))).rejects.toThrow(/outside the active Butter Paper session/);
  });

  it('removes stale matching sessions without touching unrelated temporary directories', async () => {
    const root = await mkdtemp(join(tmpdir(), 'butter-paper-blank-test-'));
    const staleDirectory = join(root, 'butter-paper-stable-blank-stale');
    const unrelatedDirectory = join(root, 'another-application');
    await mkdir(staleDirectory);
    await mkdir(unrelatedDirectory);
    const store = new BlankPdfTemporaryStore(root, 'butter-paper-stable-blank-');
    stores.push(store);

    await store.cleanupStaleSessions();

    await expect(stat(staleDirectory)).rejects.toMatchObject({ code: 'ENOENT' });
    await expect(stat(unrelatedDirectory)).resolves.toBeDefined();
  });
});
