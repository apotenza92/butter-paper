import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PDFDocument } from 'pdf-lib';
import { describe, expect, it } from 'vitest';
import { PdfTemplateStore } from './pdfTemplateStore';

describe('PdfTemplateStore', () => {
  it('copies an imported PDF into managed storage and can recreate its bytes', async () => {
    const root = await mkdtemp(join(tmpdir(), 'butter-paper-template-store-'));
    const source = join(root, 'My Site Grid.pdf');
    const document = await PDFDocument.create();
    document.addPage([300, 200]);
    document.addPage([300, 200]);
    await writeFile(source, await document.save());

    const store = new PdfTemplateStore(join(root, 'user-data'));
    const imported = await store.importPdf(source);
    await writeFile(source, new Uint8Array([1, 2, 3]));

    expect(imported).toMatchObject({ name: 'My Site Grid', kind: 'imported-pdf', pageCount: 2 });
    expect(await store.list()).toEqual([imported]);
    expect((await PDFDocument.load(await store.readSource(imported.id))).getPageCount()).toBe(2);
    expect(JSON.parse(await readFile(join(root, 'user-data', 'templates', 'library.json'), 'utf8'))).toMatchObject({ version: 1 });
  });

  it('removes only a validated managed template', async () => {
    const root = await mkdtemp(join(tmpdir(), 'butter-paper-template-remove-'));
    const source = join(root, 'Template.pdf');
    const document = await PDFDocument.create();
    document.addPage();
    await writeFile(source, await document.save());
    const store = new PdfTemplateStore(join(root, 'user-data'));
    const imported = await store.importPdf(source);

    await expect(store.remove('../outside')).rejects.toThrow(/identifier is invalid/);
    await store.remove(imported.id);
    expect(await store.list()).toEqual([]);
  });
});
