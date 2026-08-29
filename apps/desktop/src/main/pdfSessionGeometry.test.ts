import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { degrees, PDFDocument, PDFName, PDFNumber, PDFString } from 'pdf-lib';
import { identifyStorageSource, loadDocumentPayload, readPdfBytesWithProgress } from './pdfSession';
import type { PdfOpenProgress } from '../shared/protocol';

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

    expect(loaded.documentBytes.byteLength).toBeGreaterThan(0);
    expect(loaded.document.pages).toEqual([{
      id: `${source}#page-1`,
      index: 0,
      viewBox: { x: 36, y: 72, width: 540, height: 720 },
      userUnit: 2,
      size: { width: 1_440, height: 1_080 },
      rotation: 270,
    }]);
  });

  it('loads a maintained calibrated page scale into the document model', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bp-pdf-session-scale-'));
    temporaryDirectories.push(directory);
    const source = join(directory, 'calibrated.pdf');
    const pdf = await PDFDocument.create();
    const page = pdf.addPage([612, 792]);
    const scale = {
      pageIndex: 0,
      source: 'calibrated',
      name: 'Calibrated 1 m',
      pdfUnits: 'in',
      realUnits: 'm',
      scaleX: 1 / 72,
      scaleY: 1 / 72,
      precision: { mode: 'decimal', value: 0.01 },
    } as const;
    page.node.set(PDFName.of('BPPageScale'), PDFString.of(JSON.stringify(scale)));
    await writeFile(source, await pdf.save());

    const loaded = await loadDocumentPayload(source);

    expect(loaded.document.pageScales).toEqual([scale]);
  });

  it('reports logical size and bytes read while streaming a cloud-backed source', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bp-pdf-session-progress-'));
    temporaryDirectories.push(directory);
    const oneDriveDirectory = join(directory, 'OneDrive - Example');
    await mkdir(oneDriveDirectory);
    const source = join(oneDriveDirectory, 'drawing.pdf');
    const bytes = Buffer.alloc(256 * 1024, 7);
    await writeFile(source, bytes);
    const progress: PdfOpenProgress[] = [];

    const loaded = await readPdfBytesWithProgress(source, (event) => progress.push(event));

    expect(loaded).toEqual(new Uint8Array(bytes));
    expect(progress[0]).toMatchObject({
      fileName: 'drawing.pdf',
      sourceName: 'OneDrive',
      totalBytes: bytes.byteLength,
      bytesRead: 0,
      phase: 'reading',
    });
    expect(progress.at(-1)).toMatchObject({
      totalBytes: bytes.byteLength,
      bytesRead: bytes.byteLength,
    });
  });

  it('identifies common storage providers without guessing for local paths', () => {
    expect(identifyStorageSource('C:\\Cloud\\OneDrive\\drawing.pdf')).toBe('OneDrive');
    expect(identifyStorageSource('/Library/Mobile Documents/com~apple~CloudDocs/drawing.pdf')).toBe('iCloud Drive');
    expect(identifyStorageSource('/var/local/drawing.pdf')).toBeNull();
  });
});
