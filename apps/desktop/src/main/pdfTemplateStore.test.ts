import { link, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PDFDocument } from 'pdf-lib';
import { describe, expect, it } from 'vitest';
import { PdfTemplateStore } from './pdfTemplateStore';

describe('PdfTemplateStore', () => {
  it('returns an empty strict migration snapshot for a clean profile', async () => {
    const root = await mkdtemp(join(tmpdir(), 'butter-paper-template-clean-'));
    const store = new PdfTemplateStore(join(root, 'user-data'));

    await expect(store.snapshotForMigration()).resolves.toEqual({ records: [], sources: [] });
  });

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

  it('returns a strict migration snapshot containing records and owned bytes', async () => {
    const root = await mkdtemp(join(tmpdir(), 'butter-paper-template-migration-'));
    const source = join(root, 'Template.pdf');
    const document = await PDFDocument.create();
    document.addPage([250, 150]);
    const expected = await document.save();
    await writeFile(source, expected);
    const store = new PdfTemplateStore(join(root, 'user-data'));
    const imported = await store.importPdf(source);

    const snapshot = await store.snapshotForMigration();
    expect(snapshot.records).toEqual([imported]);
    expect(snapshot.sources.map(({ id, bytes }) => ({ id, bytes: [...bytes] }))).toEqual([
      { id: imported.id, bytes: [...expected] },
    ]);
  });

  it('fails closed on malformed index metadata and unrecognised storage entries', async () => {
    const root = await mkdtemp(join(tmpdir(), 'butter-paper-template-invalid-'));
    const userData = join(root, 'user-data');
    const templateRoot = join(userData, 'templates');
    await mkdir(templateRoot, { recursive: true });
    const store = new PdfTemplateStore(userData);
    await expect(store.snapshotForMigration()).rejects.toThrow(/cannot be opened safely/);
    await writeFile(join(templateRoot, 'library.json'), '{');
    await expect(store.snapshotForMigration()).rejects.toThrow(/invalid JSON/);

    await writeFile(join(templateRoot, 'library.json'), JSON.stringify({ version: 1, templates: [], extra: true }));
    await expect(store.snapshotForMigration()).rejects.toThrow(/index is invalid/);
    await writeFile(join(templateRoot, 'library.json'), JSON.stringify({ version: 1, templates: [] }));
    await writeFile(join(templateRoot, 'unexpected'), 'data');
    await expect(store.snapshotForMigration()).rejects.toThrow(/unrecognised entries/);
  });

  it('rejects symlinked and hard-linked migration sources', async () => {
    const root = await mkdtemp(join(tmpdir(), 'butter-paper-template-links-'));
    const source = join(root, 'Template.pdf');
    const document = await PDFDocument.create();
    document.addPage();
    await writeFile(source, await document.save());
    const userData = join(root, 'user-data');
    const store = new PdfTemplateStore(userData);
    const imported = await store.importPdf(source);
    const managed = join(userData, 'templates', imported.id, 'source.pdf');
    const original = join(root, 'original.pdf');
    await writeFile(original, await readFile(managed));
    await rm(managed);
    await symlink(original, managed);
    await expect(store.snapshotForMigration()).rejects.toThrow(/opened safely/);

    await rm(managed);
    await link(original, managed);
    await expect(store.snapshotForMigration()).rejects.toThrow(/private regular file/);
  });
});
