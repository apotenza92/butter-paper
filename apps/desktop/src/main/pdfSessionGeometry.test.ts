import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { degrees, PDFDocument, PDFName, PDFNumber } from 'pdf-lib';
import { loadDocumentPayload } from './pdfSession';

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => rm(directory, {
    recursive: true,
    force: true,
  })));
});

describe('PDF session page geometry', () => {
  it('retains PDF.js CropBox, UserUnit, rotation, and visible size in the main payload', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bp-pdf-session-geometry-'));
    temporaryDirectories.push(directory);
    const source = join(directory, 'offset-crop.pdf');
    const pdf = await PDFDocument.create();
    const page = pdf.addPage([800, 1_000]);
    page.setCropBox(36, 72, 540, 720);
    page.setRotation(degrees(270));
    page.node.set(PDFName.of('UserUnit'), PDFNumber.of(2));
    await writeFile(source, await pdf.save());

    const loaded = await loadDocumentPayload(source);

    expect(loaded.document.pages).toEqual([{
      id: `${source}#page-1`,
      index: 0,
      viewBox: { x: 36, y: 72, width: 540, height: 720 },
      userUnit: 2,
      size: { width: 1_440, height: 1_080 },
      rotation: 270,
    }]);
  });
});
