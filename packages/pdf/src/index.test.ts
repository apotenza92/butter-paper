// @vitest-environment node

import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { PDFArray, PDFDict, PDFDocument, PDFName, StandardFonts, rgb } from 'pdf-lib';
import { extractPdfPageGeometryIndex, openPdfDocument, PdfRenderCache } from './index.js';
import { createArcMarkup, createAreaMarkup, createArrowMarkup, createCalloutMarkup, createCloudMarkup, createCloudPlusMarkup, createCustomPageScale, createDimensionMarkup, createEllipseMarkup, createHighlightMarkup, createImageMarkup, createLengthMarkup, createLineMarkup, createPenMarkup, createPolygonMarkup, createPolylengthMarkup, createPolylineMarkup, createRectangleMarkup, createSnapshotMarkup, createTextBoxMarkup, pdfPoint } from '@butter-paper/core';

const testImageDataUrl = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAAAoCAYAAABOzvzpAAAABHNCSVQICAgIfAhkiAAAAAFzUkdCAK7OHOkAAAIpSURBVGiB7Zi/TxNhHMafO0vvWiBBlIC2ooi1xkT5MZFAHEwMJCwMpotuDqZx4R9QEyWyOJkuYhxcTAcHEkYSEjc3fqXByo9AyJUCRh3s3XE1d06uvO8dtd8v6T3z8z7vk8/dffO+p6QeHXpoYKnUBagVAqAuQK0QAHUBaoUAqAtQKwRAXYBaSumXF54EG1khAOoC1IpQF5CR67n48nsFBWsT36wdFO1tKFCQjl3Gdf0KbsauYqjlNlTF//NkPwRLziFeGm+xaq4f67sVT+Fp4jEuRjt85QsBxO7f8xUokvVpXto79/Mz3pQ/wnJtKX9M1THZ9QDjZ+9I78H2E5j9sYDXex98rbFcG9Ol96h6fzDRfldqDcshaDgHyO3nA6/P7edhOAdSXnYAXHiYMmZgu0eBM2z3CFPGDFyIxxs7AGvWlnDgyWjVXMeatSX0sQNQMDfqmsUOwGLla82yCtam0MMOgExp6SzzFAKopRyvKvSwA5DSu+uaxQ7AtRoCkMliB6Dh34CR1gH06skT5/RoCYy0Dgp97ADoqoZniSyalODXlCYlgheXnkBXo0IvOwAA0Ksnke3MBF6f7cygR0tIedneBjPnRqGpUeTKeenrcMuZOCa7HmKsbVh6H/Y/RPaq3/HKeCc8IQ4038DzZBbnI22+8tkD+Kddp4ylShFLZhHLlSI8eOhvTqMvnkZ/PI1u7UKg3FMD4H+J5RCsp0IA1AWoFQKgLkCtv9cipgMsRDYAAAAAAElFTkSuQmCC';

async function createFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([240, 180]);
  const font = await pdfDoc.embedFont(StandardFonts.Helvetica);
  page.drawRectangle({
    x: 20,
    y: 20,
    width: 80,
    height: 40,
    borderColor: rgb(0.95, 0.3, 0.2),
    borderWidth: 2,
  });
  page.drawText('Butter Paper', {
    x: 20,
    y: 140,
    size: 12,
    font,
    color: rgb(0, 0, 0),
  });

  const dir = await mkdtemp(join(tmpdir(), 'butter-paper-pdf-'));
  const file = join(dir, 'fixture.pdf');
  const bytes = await pdfDoc.save();
  await writeFile(file, bytes);
  return file;
}

async function readRawCalloutAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray).asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict);
  const callout = annotations.find((annotation) => String(annotation.get(PDFName.of('IT'))) === '/FreeTextCallout' && readPdfText(annotation.get(PDFName.of('Subj'))) === 'Callout');
  expect(callout).toBeTruthy();
  if (!callout) {
    throw new Error('Expected raw FreeTextCallout annotation');
  }

  const borderStyle = pdfDoc.context.lookup(callout.get(PDFName.of('BS')));
  return {
    subtype: String(callout.get(PDFName.of('Subtype'))),
    intent: String(callout.get(PDFName.of('IT'))),
    subject: readPdfText(callout.get(PDFName.of('Subj'))),
    defaultAppearance: readPdfText(callout.get(PDFName.of('DA'))),
    defaultStyle: readPdfText(callout.get(PDFName.of('DS'))),
    color: readPdfNumberArray(callout.get(PDFName.of('C'))),
    border: readPdfNumberArray(callout.get(PDFName.of('Border'))),
    borderStyleWidth: borderStyle instanceof PDFDict ? Number(borderStyle.get(PDFName.of('W'))) : undefined,
    lineEnding: readPdfNameArray(callout.get(PDFName.of('LE'))),
    calloutLine: readPdfNumberArray(callout.get(PDFName.of('CL'))),
  };
}

async function readRawCloudPlusAnnotations(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray).asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict)
    .filter((annotation) => readPdfText(annotation.get(PDFName.of('Subj'))) === 'Cloud+');
  return annotations.map((annotation) => ({
    subtype: String(annotation.get(PDFName.of('Subtype'))),
    intent: String(annotation.get(PDFName.of('IT'))),
    intentEx: String(annotation.get(PDFName.of('ITEx'))),
    subject: readPdfText(annotation.get(PDFName.of('Subj'))),
    vertices: readPdfNumberArray(annotation.get(PDFName.of('Vertices'))),
    calloutLine: readPdfNumberArray(annotation.get(PDFName.of('CL'))),
    borderEffect: annotation.get(PDFName.of('BE')) ? 'present' : undefined,
    appearance: annotation.get(PDFName.of('AP')) ? 'present' : undefined,
  }));
}

async function readRawImageAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray).asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict);
  const image = annotations.find((annotation) => readPdfText(annotation.get(PDFName.of('Subj'))) === 'Image');
  expect(image).toBeTruthy();
  if (!image) {
    throw new Error('Expected raw image annotation');
  }
  return {
    subtype: String(image.get(PDFName.of('Subtype'))),
    intent: String(image.get(PDFName.of('IT'))),
      subject: readPdfText(image.get(PDFName.of('Subj'))),
    rect: readPdfNumberArray(image.get(PDFName.of('Rect'))),
    imageMimeType: readPdfText(image.get(PDFName.of('BPImageMimeType'))),
    hasAppearance: Boolean(image.get(PDFName.of('AP'))),
  };
}

async function readRawArcAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray).asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict);
  const arc = annotations.find((annotation) => readPdfText(annotation.get(PDFName.of('Subj'))) === 'Arc');
  expect(arc).toBeTruthy();
  if (!arc) {
    throw new Error('Expected raw arc annotation');
  }
  return {
    subtype: String(arc.get(PDFName.of('Subtype'))),
    intent: String(arc.get(PDFName.of('IT'))),
    subject: readPdfText(arc.get(PDFName.of('Subj'))),
    rect: readPdfNumberArray(arc.get(PDFName.of('Rect'))),
    angle1: Number(arc.get(PDFName.of('Angle1'))),
    angle2: Number(arc.get(PDFName.of('Angle2'))),
    rd: readPdfNumberArray(arc.get(PDFName.of('RD'))),
    hasAppearance: Boolean(arc.get(PDFName.of('AP'))),
  };
}

async function readRawDimensionAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray).asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict);
  const dimension = annotations.find((annotation) => readPdfText(annotation.get(PDFName.of('Subj'))) === 'Dimension');
  expect(dimension).toBeTruthy();
  if (!dimension) {
    throw new Error('Expected raw Dimension annotation');
  }
  return {
    subtype: String(dimension.get(PDFName.of('Subtype'))),
    intent: String(dimension.get(PDFName.of('IT'))),
    subject: readPdfText(dimension.get(PDFName.of('Subj'))),
    line: readPdfNumberArray(dimension.get(PDFName.of('L'))),
    lineEnding: readPdfNameArray(dimension.get(PDFName.of('LE'))),
    lineLeader: Number(dimension.get(PDFName.of('LL'))),
    lineLeaderExtension: Number(dimension.get(PDFName.of('LLE'))),
    caption: readPdfText(dimension.get(PDFName.of('Cap'))),
    hasAppearance: Boolean(dimension.get(PDFName.of('AP'))),
  };
}

async function readRawMeasurementAnnotations(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray).asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict)
    .filter((annotation) => ['Length Measurement', 'Polylength Measurement', 'Area Measurement'].includes(readPdfText(annotation.get(PDFName.of('Subj'))) ?? ''));
  return annotations.map((annotation) => ({
    subtype: String(annotation.get(PDFName.of('Subtype'))),
    intent: String(annotation.get(PDFName.of('IT'))),
    subject: readPdfText(annotation.get(PDFName.of('Subj'))),
    line: readPdfNumberArray(annotation.get(PDFName.of('L'))),
    vertices: readPdfNumberArray(annotation.get(PDFName.of('Vertices'))),
    contents: readPdfText(annotation.get(PDFName.of('Contents'))),
    caption: readPdfText(annotation.get(PDFName.of('Cap'))),
    hasAppearance: Boolean(annotation.get(PDFName.of('AP'))),
  }));
}

async function readRawSnapshotAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray).asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict);
  const snapshot = annotations.find((annotation) => readPdfText(annotation.get(PDFName.of('Subj'))) === 'Snapshot');
  expect(snapshot).toBeTruthy();
  if (!snapshot) {
    throw new Error('Expected raw snapshot annotation');
  }
  return {
    subtype: String(snapshot.get(PDFName.of('Subtype'))),
    intent: String(snapshot.get(PDFName.of('IT'))),
    subject: readPdfText(snapshot.get(PDFName.of('Subj'))),
    rect: readPdfNumberArray(snapshot.get(PDFName.of('Rect'))),
    snapshotMimeType: readPdfText(snapshot.get(PDFName.of('BPSnapshotMimeType'))),
    hasAppearance: Boolean(snapshot.get(PDFName.of('AP'))),
  };
}

function readPdfText(value: unknown): string | undefined {
  if (!value || typeof value !== 'object') {
    return undefined;
  }
  if ('decodeText' in value && typeof value.decodeText === 'function') {
    return value.decodeText();
  }
  return String(value);
}

function readPdfNumberArray(value: unknown): number[] {
  if (!(value instanceof PDFArray)) {
    return [];
  }
  return value.asArray().map(Number);
}

function readPdfNameArray(value: unknown): string[] {
  if (!(value instanceof PDFArray)) {
    return [];
  }
  return value.asArray().map(String);
}

describe('pdf package', () => {
  it('opens metadata and page information', async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);

    const metadata = await handle.getMetadata();
    expect(metadata.pageCount).toBe(1);

    const pageInfo = await handle.getPageInfo(0);
    expect(pageInfo.width).toBeGreaterThan(0);
    expect(pageInfo.height).toBeGreaterThan(0);

    await handle.close();
  }, 15_000);

  it('extracts simple page-content snap geometry', async () => {
    const file = await createFixturePdf();
    const index = await extractPdfPageGeometryIndex(file, 0);

    expect(index.pageIndex).toBe(0);
    expect(index.buildMs).toBeGreaterThanOrEqual(0);
    expect(index.primitives).toEqual(expect.arrayContaining([
      {
        kind: 'rect',
        rect: { x: 20, y: 20, width: 80, height: 40 },
      },
    ]));
  });

  it('renders a page and caches the surface', async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const rendered = await handle.renderPage({ pageIndex: 0, scale: 1 });

    expect(rendered.width).toBeGreaterThan(0);
    expect(rendered.height).toBeGreaterThan(0);
    expect(handle.cache.stats().entries).toBe(1);

    const cached = handle.cache.get(`0:1:${(await handle.getPageInfo(0)).rotation}`);
    expect(cached).toBeDefined();

    await handle.close();
  });

  it('keeps the cache within limits', async () => {
    const cache = new PdfRenderCache({ maxEntries: 2, maxBytes: 1000 });
    const canvas = { width: 10, height: 10, getContext: () => null } as const;
    cache.set('a', { pageIndex: 0, width: 10, height: 10, canvas: canvas as never });
    cache.set('b', { pageIndex: 0, width: 10, height: 10, canvas: canvas as never });
    cache.set('c', { pageIndex: 0, width: 10, height: 10, canvas: canvas as never });
    expect(cache.stats().entries).toBeLessThanOrEqual(2);
  });

  it('round-trips rectangle, text box and callout annotations', async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, '.annotated.pdf');
    const rectangle = createRectangleMarkup({
      id: 'rect-1',
      pageIndex: 0,
      rect: { x: 20, y: 20, width: 80, height: 40 },
    });
    const callout = createCalloutMarkup({
      id: 'callout-1',
      pageIndex: 0,
      leader: { points: [pdfPoint(30, 30), pdfPoint(60, 60), pdfPoint(100, 90)] },
      textBox: { x: 110, y: 60, width: 90, height: 40 },
      text: 'Need to check',
    });
    const textBox = createTextBoxMarkup({
      id: 'text-1',
      pageIndex: 0,
      rect: { x: 20, y: 110, width: 90, height: 24 },
      text: 'Default text',
    });
    const ellipse = createEllipseMarkup({
      id: 'ellipse-1',
      pageIndex: 0,
      rect: { x: 120, y: 20, width: 70, height: 35 },
    });
    const arc = createArcMarkup({
      id: 'arc-1',
      pageIndex: 0,
      rect: { x: 120, y: 110, width: 70, height: 55 },
      angle1: 90,
      angle2: 180,
    });
    const line = createLineMarkup({
      id: 'line-1',
      pageIndex: 0,
      start: pdfPoint(20, 70),
      end: pdfPoint(100, 95),
    });
    const arrow = createArrowMarkup({
      id: 'arrow-1',
      pageIndex: 0,
      start: pdfPoint(120, 70),
      end: pdfPoint(200, 95),
    });
    const dimension = createDimensionMarkup({
      id: 'dimension-1',
      pageIndex: 0,
      start: pdfPoint(20, 50),
      end: pdfPoint(120, 50),
      dimensionLineOffset: 24,
      text: '100 ft',
    });
    const polyline = createPolylineMarkup({
      id: 'polyline-1',
      pageIndex: 0,
      points: [pdfPoint(20, 130), pdfPoint(80, 160), pdfPoint(140, 120)],
    });
    const polygon = createPolygonMarkup({
      id: 'polygon-1',
      pageIndex: 0,
      points: [pdfPoint(150, 130), pdfPoint(200, 160), pdfPoint(220, 120), pdfPoint(170, 110)],
    });
    const pen = createPenMarkup({
      id: 'pen-1',
      pageIndex: 0,
      paths: [[pdfPoint(20, 150), pdfPoint(60, 170), pdfPoint(100, 150)]],
    });
    const highlight = createHighlightMarkup({
      id: 'highlight-1',
      pageIndex: 0,
      paths: [[pdfPoint(120, 150), pdfPoint(220, 150)]],
    });
    const cloud = createCloudMarkup({
      id: 'cloud-1',
      pageIndex: 0,
      controlPath: [pdfPoint(35, 35), pdfPoint(35, 70), pdfPoint(100, 70), pdfPoint(100, 35)],
    });
    const cloudPlus = createCloudPlusMarkup({
      id: 'cloud-plus-1',
      pageIndex: 0,
      cloud: {
        controlPath: [pdfPoint(30, 85), pdfPoint(30, 120), pdfPoint(95, 120), pdfPoint(95, 85)],
        borderEffectIntensity: 2,
      },
      leader: { points: [pdfPoint(95, 102), pdfPoint(115, 102), pdfPoint(135, 102)] },
      textBox: { x: 135, y: 82, width: 90, height: 40 },
      text: 'Cloud plus',
    });
    const image = createImageMarkup({
      id: 'image-1',
      pageIndex: 0,
      rect: { x: 20, y: 75, width: 64, height: 40 },
      dataUrl: testImageDataUrl,
      mimeType: 'image/png',
    });
    const snapshot = createSnapshotMarkup({
      id: 'snapshot-1',
      pageIndex: 0,
      rect: { x: 90, y: 75, width: 64, height: 40 },
      dataUrl: testImageDataUrl,
      mimeType: 'image/png',
    });
    const length = createLengthMarkup({
      id: 'length-1',
      pageIndex: 0,
      start: pdfPoint(20, 130),
      end: pdfPoint(120, 130),
      displayUnit: 'cm',
    });
    const polylength = createPolylengthMarkup({
      id: 'polylength-1',
      pageIndex: 0,
      points: [pdfPoint(20, 140), pdfPoint(70, 140), pdfPoint(70, 170)],
    });
    const area = createAreaMarkup({
      id: 'area-1',
      pageIndex: 0,
      points: [pdfPoint(130, 125), pdfPoint(190, 125), pdfPoint(190, 165), pdfPoint(130, 165)],
    });
    const measurementScale = createCustomPageScale({
      pageIndex: 0,
      name: '1:100',
      pdfUnits: 'cm',
      realUnits: 'm',
      scaleX: 0.01,
      scaleY: 0.01,
      precision: { mode: 'decimal', value: 0.01 },
    });

    await handle.writer.save(handle, [rectangle, ellipse, arc, line, arrow, dimension, length, polylength, area, polyline, polygon, pen, highlight, cloud, cloudPlus, image, snapshot, textBox, callout], 'saveAs', output, [measurementScale]);
    const rawCallout = await readRawCalloutAnnotation(output);
    const rawCloudPlus = await readRawCloudPlusAnnotations(output);
    const rawArc = await readRawArcAnnotation(output);
    const rawDimension = await readRawDimensionAnnotation(output);
    const rawMeasurements = await readRawMeasurementAnnotations(output);
    const rawImage = await readRawImageAnnotation(output);
    const rawSnapshot = await readRawSnapshotAnnotation(output);
    expect(rawCallout).toMatchObject({
      subtype: '/FreeText',
      intent: '/FreeTextCallout',
      subject: 'Callout',
      defaultAppearance: '1 0 0 rg /Helv 12 Tf',
      defaultStyle: 'font: Helvetica 12pt; text-align:left; margin:3pt; line-height:13.8pt; color:#FF0000',
      color: [1, 0, 0],
      border: [0, 0, 0],
      borderStyleWidth: 0,
      lineEnding: ['/None', '/OpenArrow'],
      calloutLine: [30, 30, 60, 60, 100, 90],
    });
    expect(rawCloudPlus).toEqual(expect.arrayContaining([
      expect.objectContaining({
        subtype: '/Polygon',
        intent: '/PolygonCloud',
        intentEx: '/PolyText',
        subject: 'Cloud+',
        borderEffect: 'present',
        vertices: [30, 85, 30, 120, 95, 120, 95, 85],
      }),
      expect.objectContaining({
        subtype: '/FreeText',
        intent: '/FreeTextCallout',
        intentEx: '/PolyText',
        subject: 'Cloud+',
        calloutLine: [95, 102, 115, 102, 135, 102],
        appearance: 'present',
      }),
    ]));
    expect(rawImage).toMatchObject({
      subtype: '/Square',
      intent: '/SquareImage',
      subject: 'Image',
      rect: [20, 75, 84, 115],
      imageMimeType: 'image/png',
      hasAppearance: true,
    });
    expect(rawArc).toMatchObject({
      subtype: '/Circle',
      intent: '/CircleArc',
      subject: 'Arc',
      rect: [120, 110, 190, 165],
      angle1: 90,
      angle2: 180,
      rd: [0.5, 0.5, 0.5, 0.5],
      hasAppearance: true,
    });
    expect(rawDimension).toMatchObject({
      subtype: '/Line',
      intent: '/LineDimension',
      subject: 'Dimension',
      line: [20, 50, 120, 50],
      lineEnding: ['/ClosedArrow', '/ClosedArrow'],
      lineLeader: 24,
      lineLeaderExtension: 4,
      caption: '100 ft',
      hasAppearance: true,
    });
    expect(rawMeasurements).toEqual(expect.arrayContaining([
      expect.objectContaining({
        subtype: '/Line',
        intent: '/LineDimension',
        subject: 'Length Measurement',
        line: [20, 130, 120, 130],
        contents: '100.00 cm',
        caption: 'true',
        hasAppearance: true,
      }),
      expect.objectContaining({
        subtype: '/PolyLine',
        intent: '/PolyLineDimension',
        subject: 'Polylength Measurement',
        vertices: [20, 140, 70, 140, 70, 170],
        contents: '0.80 m',
        hasAppearance: true,
      }),
      expect.objectContaining({
        subtype: '/Polygon',
        intent: '/PolygonDimension',
        subject: 'Area Measurement',
        vertices: [130, 125, 190, 125, 190, 165, 130, 165],
        contents: '0.24 m^2',
        hasAppearance: true,
      }),
    ]));
    expect(rawSnapshot).toMatchObject({
      subtype: '/Stamp',
      intent: '/StampSnapshot',
      subject: 'Snapshot',
      rect: [90, 75, 154, 115],
      snapshotMimeType: 'image/png',
      hasAppearance: true,
    });

    const reopened = await openPdfDocument(output);
    const annotations = await reopened.annotations.readPageAnnotations(0);
    const allAnnotations = await reopened.annotations.readAllPageAnnotations();

    expect(annotations).toHaveLength(19);
    expect(annotations.some((markup) => markup.kind === 'rectangle')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'ellipse')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'arc')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'line')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'arrow')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'dimension' && markup.text === '100 ft')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'length')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'polylength')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'area')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'polyline')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'polygon')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'pen')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'highlight')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'cloud')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'cloud-plus' && markup.text === 'Cloud plus')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'image')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'snapshot')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'text-box' && markup.text === 'Default text')).toBe(true);
    expect(annotations.some((markup) => markup.kind === 'callout' && markup.text === 'Need to check')).toBe(true);
    for (const original of [rectangle, ellipse, arc, line, arrow, dimension, length, polylength, area, polyline, polygon, pen, highlight, cloud, cloudPlus, image, snapshot, textBox, callout]) {
      expect(annotations.find((markup) => markup.id === original.id)?.appearance, original.id).toEqual(original.appearance);
    }
    expect(allAnnotations).toHaveLength(1);
    expect(allAnnotations[0]).toHaveLength(19);
    expect(allAnnotations[0]).toEqual(annotations);

    await handle.close();
    await reopened.close();
  });

  it('round-trips text box inline rich text runs', async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, '.rich-text.annotated.pdf');
    const textBox = createTextBoxMarkup({
      id: 'rich-text-1',
      pageIndex: 0,
      rect: { x: 20, y: 80, width: 180, height: 48 },
      text: 'Normal bold italic red',
      richTextRuns: [
        { text: 'Normal ' },
        { text: 'bold ', bold: true },
        { text: 'italic ', italic: true },
        { text: 'red', color: '#0080ff', fontSizePt: 14 },
      ],
      color: '#ff0000',
      borderColor: '#ff0000',
      borderWidth: 1,
      fontSizePt: 12,
    });

    await handle.writer.save(handle, [textBox], 'saveAs', output);
    const reopened = await openPdfDocument(output);
    const annotations = await reopened.annotations.readPageAnnotations(0);
    const richText = annotations.find((markup) => markup.kind === 'text-box');

    expect(richText).toMatchObject({
      kind: 'text-box',
      text: 'Normal bold italic red',
      richTextRuns: [
        { text: 'Normal ', color: '#ff0000', fontSizePt: 12 },
        { text: 'bold ', bold: true, color: '#ff0000', fontSizePt: 12 },
        { text: 'italic ', italic: true, color: '#ff0000', fontSizePt: 12 },
        { text: 'red', color: '#0080ff', fontSizePt: 14 },
      ],
    });

    await handle.close();
    await reopened.close();
  });

  it('writes and reopens non-default appearance supplied directly in markup data', async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, '.custom-appearance.annotated.pdf');
    const rectangle = createRectangleMarkup({
      id: 'custom-rect',
      pageIndex: 0,
      rect: { x: 20, y: 20, width: 80, height: 40 },
      appearance: {
        stroke: { color: '#123456', widthPt: 3.25 },
        fill: { color: '#abcdef' },
        opacity: 0.35,
      },
    });
    const textBox = createTextBoxMarkup({
      id: 'custom-text',
      pageIndex: 0,
      rect: { x: 20, y: 80, width: 160, height: 60 },
      text: 'One explicit line\nSecond line',
      appearance: {
        stroke: { color: '#102030', widthPt: 2 },
        text: {
          color: '#654321',
          fontId: 'Helvetica',
          fontSizePt: 20,
          lineHeightPt: 25,
          align: 'right',
          insetPt: 9,
        },
        opacity: 0.6,
      },
    });

    await handle.writer.save(handle, [rectangle, textBox], 'saveAs', output);
    const rawPdf = await PDFDocument.load(await readFile(output));
    const rawAnnots = rawPdf.context.lookup(rawPdf.getPage(0).node.Annots()) as PDFArray;
    const annotations = rawAnnots.asArray().map((ref) => rawPdf.context.lookup(ref)).filter((value): value is PDFDict => value instanceof PDFDict);
    const rawRectangle = annotations.find((annotation) => readPdfText(annotation.get(PDFName.of('NM'))) === 'bp:custom-rect');
    const rawTextBox = annotations.find((annotation) => readPdfText(annotation.get(PDFName.of('NM'))) === 'bp:custom-text');

    expect(readPdfNumberArray(rawRectangle?.get(PDFName.of('C')))).toEqual([0x12 / 255, 0x34 / 255, 0x56 / 255]);
    expect(readPdfNumberArray(rawRectangle?.get(PDFName.of('IC')))).toEqual([0xab / 255, 0xcd / 255, 0xef / 255]);
    expect(Number(rawRectangle?.get(PDFName.of('CA')))).toBe(0.35);
    expect(readPdfText(rawTextBox?.get(PDFName.of('DA')))).toBe('0.3961 0.2627 0.1294 rg /Helv 20 Tf');
    expect(readPdfText(rawTextBox?.get(PDFName.of('DS')))).toContain('text-align:right; margin:9pt; line-height:25pt; color:#654321');

    const reopened = await openPdfDocument(output);
    const markups = await reopened.annotations.readPageAnnotations(0);
    expect(markups.find((markup) => markup.id === rectangle.id)?.appearance).toEqual(rectangle.appearance);
    expect(markups.find((markup) => markup.id === textBox.id)?.appearance).toEqual(textBox.appearance);

    await handle.close();
    await reopened.close();
  });
});
