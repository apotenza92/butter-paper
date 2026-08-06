// @vitest-environment node

import { createHash } from 'node:crypto';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createCanvas, loadImage } from '@napi-rs/canvas';
import { describe, expect, it } from 'vitest';
import { decodePDFRawStream, PDFArray, PDFDict, PDFDocument, PDFName, PDFNumber, PDFRawStream, PDFString, StandardFonts, rgb, type PDFRef } from 'pdf-lib';
import { extractPdfPageGeometryIndex, openPdfDocument, PdfAnnotationWriter, PdfRenderCache } from './index.js';
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

async function createBluebeamNativeFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([300, 220]);
  const rectangle = pdfDoc.context.obj({
    Type: PDFName.of('Annot'),
    Subtype: PDFName.of('Square'),
    Rect: [20, 20, 100, 70],
    C: [1, 0, 0],
    NM: PDFString.of('BB-RECT'),
    T: PDFString.of('A. Reviewer'),
    Subj: PDFString.of('Structural review'),
    CreationDate: PDFString.of('D:20260803101500+10\'00\''),
    M: PDFString.of('D:20260803103000+10\'00\''),
    Contents: PDFString.of('Keep this independent comment'),
    F: 4,
    StateModel: PDFString.of('Review'),
    State: PDFString.of('Accepted'),
    Rotation: PDFNumber.of(15),
    BPProbe: PDFString.of('preserve-me'),
  });
  const cloud = pdfDoc.context.obj({
    Type: PDFName.of('Annot'),
    Subtype: PDFName.of('Polygon'),
    Rect: [120, 20, 225, 95],
    Vertices: [130, 30, 130, 80, 210, 80, 210, 30],
    C: [1, 0, 0],
    BE: pdfDoc.context.obj({ S: PDFName.of('C'), I: 2 }),
    IT: PDFName.of('PolygonCloud'),
    ITEx: PDFName.of('PolyText'),
    NM: PDFString.of('BB-CLOUD'),
    Subj: PDFString.of('Custom cloud subject'),
  });
  const text = pdfDoc.context.obj({
    Type: PDFName.of('Annot'),
    Subtype: PDFName.of('FreeText'),
    IT: PDFName.of('FreeTextCallout'),
    // Revu removes ITEx from this half after independently moving Cloud+ text.
    Rect: [204.5, 29.5, 295.5, 80.5],
    RD: [20.5, 5.5, 5.5, 5.5],
    Contents: PDFString.of('Native Cloud+'),
    CL: [210, 55, 220, 55, 225, 55],
    LE: [PDFName.of('None'), PDFName.of('None')],
    NM: PDFString.of('BB-TEXT'),
    Subj: PDFString.of('Custom cloud subject'),
    GroupNesting: [PDFString.of('Cloud+'), PDFName.of('BB-TEXT'), PDFName.of('BB-CLOUD')],
  });
  const rectangleRef = pdfDoc.context.register(rectangle);
  const cloudRef = pdfDoc.context.register(cloud);
  const textRef = pdfDoc.context.register(text);
  cloud.set(PDFName.of('IRT'), textRef);
  cloud.set(PDFName.of('RT'), PDFName.of('Group'));
  const reply = pdfDoc.context.obj({
    Type: PDFName.of('Annot'),
    Subtype: PDFName.of('Text'),
    Rect: [20, 20, 40, 40],
    NM: PDFString.of('BB-REPLY'),
    T: PDFString.of('B. Reviewer'),
    Contents: PDFString.of('Reply stays attached'),
    IRT: rectangleRef,
    RT: PDFName.of('Reply'),
    F: 4,
  });
  page.node.set(PDFName.of('Annots'), pdfDoc.context.obj([
    rectangleRef,
    cloudRef,
    textRef,
    pdfDoc.context.register(reply),
  ]));

  const dir = await mkdtemp(join(tmpdir(), 'butter-paper-bluebeam-native-'));
  const file = join(dir, 'bluebeam-native.pdf');
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createRevuNativeMediaFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([320, 220]);
  const canvas = createCanvas(13, 9);
  const context = canvas.getContext('2d');
  context.fillStyle = '#1234c8';
  context.fillRect(0, 0, 13, 9);
  context.fillStyle = '#f2b51d';
  context.fillRect(1, 1, 5, 4);
  context.fillStyle = '#28a861';
  context.fillRect(8, 3, 4, 5);
  const pngBytes = canvas.toBuffer('image/png');
  const jpegBytes = canvas.toBuffer('image/jpeg', 92);
  const pngImage = await pdfDoc.embedPng(pngBytes);
  const jpegImage = await pdfDoc.embedJpg(jpegBytes);

  const createAppearance = (imageRef: PDFRef, nested: boolean) => {
    const imageForm = pdfDoc.context.flateStream('q 13 0 0 9 0 0 cm /Payload Do Q', {
      Type: PDFName.of('XObject'),
      Subtype: PDFName.of('Form'),
      FormType: PDFNumber.of(1),
      BBox: [0, 0, 13, 9],
      Resources: pdfDoc.context.obj({ XObject: { Payload: imageRef } }),
    });
    const imageFormRef = pdfDoc.context.register(imageForm);
    if (!nested) {
      return imageFormRef;
    }
    const wrapper = pdfDoc.context.flateStream('q /MediaLayer Do Q', {
      Type: PDFName.of('XObject'),
      Subtype: PDFName.of('Form'),
      FormType: PDFNumber.of(1),
      BBox: [0, 0, 13, 9],
      Resources: pdfDoc.context.obj({ XObject: { MediaLayer: imageFormRef } }),
    });
    return pdfDoc.context.register(wrapper);
  };

  const imageAppearance = createAppearance(pngImage.ref, false);
  const snapshotAppearance = createAppearance(jpegImage.ref, true);
  const image = pdfDoc.context.obj({
    Type: PDFName.of('Annot'),
    Subtype: PDFName.of('Square'),
    Rect: [30, 110, 160, 200],
    IT: PDFName.of('SquareImage'),
    Subj: PDFString.of('Imported site photograph'),
    NM: PDFString.of('REVU-IMAGE-1'),
    F: PDFNumber.of(4),
    AP: pdfDoc.context.obj({ N: imageAppearance }),
  });
  const snapshot = pdfDoc.context.obj({
    Type: PDFName.of('Annot'),
    Subtype: PDFName.of('Stamp'),
    Rect: [175, 110, 305, 200],
    IT: PDFName.of('StampSnapshot'),
    Subj: PDFString.of('Imported plan snapshot'),
    NM: PDFString.of('REVU-SNAPSHOT-1'),
    F: PDFNumber.of(4),
    AS: PDFName.of('Visible'),
    AP: pdfDoc.context.obj({ N: pdfDoc.context.obj({ Visible: snapshotAppearance }) }),
  });
  page.node.set(PDFName.of('Annots'), pdfDoc.context.obj([
    pdfDoc.context.register(image),
    pdfDoc.context.register(snapshot),
  ]));

  const dir = await mkdtemp(join(tmpdir(), 'butter-paper-revu-media-'));
  const file = join(dir, 'revu-native-media.pdf');
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function readNativeMediaContracts(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const annots = pdfDoc.context.lookup(pdfDoc.getPage(0).node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  return (annots as PDFArray).asArray().map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict)
    .filter((annotation) => ['SquareImage', 'StampSnapshot'].includes(String(annotation.get(PDFName.of('IT'))).replace(/^\//, '')))
    .map((annotation) => {
      const normal = readTestNormalAppearance(pdfDoc, annotation);
      const streams = normal ? collectTestAppearanceStreams(pdfDoc, normal) : [];
      const appearanceHash = createHash('sha256');
      for (const stream of streams) {
        appearanceHash.update(String(stream.dict.get(PDFName.of('Subtype'))));
        appearanceHash.update(stream.contents);
      }
      const payloadHashes = streams
        .filter((stream) => String(stream.dict.get(PDFName.of('Subtype'))) === '/Image')
        .map((stream) => createHash('sha256').update(stream.contents).digest('hex'))
        .sort();
      return {
        name: readPdfText(annotation.get(PDFName.of('NM'))),
        subtype: String(annotation.get(PDFName.of('Subtype'))),
        intent: String(annotation.get(PDFName.of('IT'))),
        rect: readPdfNumberArray(annotation.get(PDFName.of('Rect'))),
        appearanceState: String(annotation.get(PDFName.of('AS')) ?? ''),
        appearanceHash: appearanceHash.digest('hex'),
        payloadHashes,
        hasPrivateData: annotation.has(PDFName.of('BPImageData')) || annotation.has(PDFName.of('BPSnapshotData')),
      };
    });
}

function readTestNormalAppearance(pdfDoc: PDFDocument, annotation: PDFDict): PDFRawStream | undefined {
  const ap = pdfDoc.context.lookup(annotation.get(PDFName.of('AP')));
  if (!(ap instanceof PDFDict)) return undefined;
  const normal = pdfDoc.context.lookup(ap.get(PDFName.of('N')));
  if (normal instanceof PDFRawStream) return normal;
  if (!(normal instanceof PDFDict)) return undefined;
  const state = pdfDoc.context.lookup(annotation.get(PDFName.of('AS')));
  const selected = state instanceof PDFName ? pdfDoc.context.lookup(normal.get(state)) : undefined;
  if (selected instanceof PDFRawStream) return selected;
  for (const key of normal.keys()) {
    const candidate = pdfDoc.context.lookup(normal.get(key));
    if (candidate instanceof PDFRawStream) return candidate;
  }
  return undefined;
}

function collectTestAppearanceStreams(pdfDoc: PDFDocument, root: PDFRawStream): readonly PDFRawStream[] {
  const streams: PDFRawStream[] = [];
  const visited = new Set<PDFRawStream>();
  const visit = (stream: PDFRawStream) => {
    if (visited.has(stream)) return;
    visited.add(stream);
    streams.push(stream);
    const resources = pdfDoc.context.lookup(stream.dict.get(PDFName.of('Resources')));
    const xObjects = resources instanceof PDFDict ? pdfDoc.context.lookup(resources.get(PDFName.of('XObject'))) : undefined;
    if (!(xObjects instanceof PDFDict)) return;
    for (const key of [...xObjects.keys()].sort((left, right) => String(left).localeCompare(String(right)))) {
      const child = pdfDoc.context.lookup(xObjects.get(key));
      if (child instanceof PDFRawStream) visit(child);
    }
  };
  visit(root);
  return streams;
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
  const appearance = readAppearanceGeometry(pdfDoc, callout);
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
    rect: readPdfNumberArray(callout.get(PDFName.of('Rect'))),
    rectangleDifferences: readPdfNumberArray(callout.get(PDFName.of('RD'))),
    richContent: readPdfText(callout.get(PDFName.of('RC'))),
    ...appearance,
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
    lineEnding: readPdfNameArray(annotation.get(PDFName.of('LE'))),
    borderEffect: annotation.get(PDFName.of('BE')) ? 'present' : undefined,
    appearance: annotation.get(PDFName.of('AP')) ? 'present' : undefined,
    name: readPdfText(annotation.get(PDFName.of('NM'))),
    groupNesting: readPdfMixedArray(annotation.get(PDFName.of('GroupNesting'))),
    color: readPdfNumberArray(annotation.get(PDFName.of('C'))),
    rect: readPdfNumberArray(annotation.get(PDFName.of('Rect'))),
    rectangleDifferences: readPdfNumberArray(annotation.get(PDFName.of('RD'))),
    richContent: readPdfText(annotation.get(PDFName.of('RC'))),
    ...readAppearanceGeometry(pdfDoc, annotation),
  }));
}

function readAppearanceGeometry(pdfDoc: PDFDocument, annotation: PDFDict) {
  const appearanceDictionary = pdfDoc.context.lookup(annotation.get(PDFName.of('AP')));
  const normalAppearance = appearanceDictionary instanceof PDFDict
    ? pdfDoc.context.lookup(appearanceDictionary.get(PDFName.of('N')))
    : undefined;
  return normalAppearance instanceof PDFRawStream ? {
    appearanceBounds: readPdfNumberArray(normalAppearance.dict.get(PDFName.of('BBox'))),
    appearanceMatrix: readPdfNumberArray(normalAppearance.dict.get(PDFName.of('Matrix'))),
    appearanceContent: Buffer.from(decodePDFRawStream(normalAppearance).decode()).toString('latin1'),
  } : {};
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

async function readRawAnnotationContracts(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const annots = pdfDoc.context.lookup(pdfDoc.getPage(0).node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  return (annots as PDFArray).asArray().map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict)
    .map((annotation) => ({
      name: readPdfText(annotation.get(PDFName.of('NM'))),
      subtype: String(annotation.get(PDFName.of('Subtype'))),
      subject: readPdfText(annotation.get(PDFName.of('Subj'))),
      intent: String(annotation.get(PDFName.of('IT')) ?? ''),
      intentEx: String(annotation.get(PDFName.of('ITEx')) ?? ''),
      hasAppearance: Boolean(annotation.get(PDFName.of('AP'))),
    }));
}

async function readRawAnnotationMetadata(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const annots = pdfDoc.context.lookup(pdfDoc.getPage(0).node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  return (annots as PDFArray).asArray().map((ref) => pdfDoc.context.lookup(ref))
    .filter((annotation): annotation is PDFDict => annotation instanceof PDFDict)
    .map((annotation) => {
      const inReplyTo = pdfDoc.context.lookup(annotation.get(PDFName.of('IRT')));
      return {
        name: readPdfText(annotation.get(PDFName.of('NM'))),
        author: readPdfText(annotation.get(PDFName.of('T'))),
        subject: readPdfText(annotation.get(PDFName.of('Subj'))),
        creationDate: readPdfText(annotation.get(PDFName.of('CreationDate'))),
        modificationDate: readPdfText(annotation.get(PDFName.of('M'))),
        contents: readPdfText(annotation.get(PDFName.of('Contents'))),
        flags: Number(String(annotation.get(PDFName.of('F')) ?? 'NaN')),
        stateModel: readPdfText(annotation.get(PDFName.of('StateModel'))),
        state: readPdfText(annotation.get(PDFName.of('State'))),
        replyType: String(annotation.get(PDFName.of('RT')) ?? ''),
        inReplyTo: inReplyTo instanceof PDFDict ? readPdfText(inReplyTo.get(PDFName.of('NM'))) : undefined,
        unsafeProbe: readPdfText(annotation.get(PDFName.of('BPProbe'))),
      };
    });
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

function readPdfMixedArray(value: unknown): string[] {
  if (!(value instanceof PDFArray)) {
    return [];
  }
  return value.asArray().map((item) => readPdfText(item) ?? String(item));
}

function imageBytesForTest(dataUrl: string): Uint8Array {
  return Uint8Array.from(Buffer.from(dataUrl.split(',').at(-1) ?? '', 'base64'));
}

describe('pdf package', () => {
  it('extracts and losslessly reuses Revu-native Image and Snapshot appearances without Butter private keys', async () => {
    const file = await createRevuNativeMediaFixturePdf();
    const sourceContracts = await readNativeMediaContracts(file);
    expect(sourceContracts).toHaveLength(2);
    expect(sourceContracts).toEqual(expect.arrayContaining([
      expect.objectContaining({
        subtype: '/Square',
        intent: '/SquareImage',
        rect: [30, 110, 160, 200],
        hasPrivateData: false,
      }),
      expect.objectContaining({
        subtype: '/Stamp',
        intent: '/StampSnapshot',
        rect: [175, 110, 305, 200],
        appearanceState: '/Visible',
        hasPrivateData: false,
      }),
    ]));

    const handle = await openPdfDocument(file);
    const imported = await handle.annotations.readPageAnnotations(0);
    const image = imported.find((markup) => markup.kind === 'image');
    const snapshot = imported.find((markup) => markup.kind === 'snapshot');
    expect(imported.map((markup) => markup.kind)).toEqual(['image', 'snapshot']);
    if (!image || image.kind !== 'image' || !snapshot || snapshot.kind !== 'snapshot') {
      throw new Error('Expected native Image and Snapshot markups');
    }
    expect(image.mimeType).toBe('image/png');
    expect(snapshot.mimeType).toBe('image/jpeg');
    expect(image.dataUrl).not.toBe(testImageDataUrl);
    expect(imageBytesForTest(image.dataUrl).slice(0, 8)).toEqual(Uint8Array.of(137, 80, 78, 71, 13, 10, 26, 10));
    expect(imageBytesForTest(snapshot.dataUrl).slice(0, 2)).toEqual(Uint8Array.of(0xff, 0xd8));
    const decodedImage = await loadImage(image.dataUrl);
    const decodedSnapshot = await loadImage(snapshot.dataUrl);
    expect([decodedImage.width, decodedImage.height]).toEqual([13, 9]);
    expect([decodedSnapshot.width, decodedSnapshot.height]).toEqual([13, 9]);

    const firstOutput = file.replace(/\.pdf$/i, '.first-edit.pdf');
    const firstEdited = [
      { ...image, rect: { x: 45, y: 85, width: 156, height: 108 } },
      { ...snapshot, rect: { x: 168, y: 75, width: 143, height: 99 } },
    ];
    await handle.writer.save(handle, firstEdited, 'saveAs', firstOutput);
    const firstContracts = await readNativeMediaContracts(firstOutput);
    expect(firstContracts.map((contract) => contract.appearanceHash)).toEqual(sourceContracts.map((contract) => contract.appearanceHash));
    expect(firstContracts.map((contract) => contract.payloadHashes)).toEqual(sourceContracts.map((contract) => contract.payloadHashes));
    expect(firstContracts).toEqual(expect.arrayContaining([
      expect.objectContaining({ subtype: '/Square', intent: '/SquareImage', rect: [45, 85, 201, 193], hasPrivateData: true }),
      expect.objectContaining({ subtype: '/Stamp', intent: '/StampSnapshot', rect: [168, 75, 311, 174], appearanceState: '/Visible', hasPrivateData: true }),
    ]));

    const firstReopened = await openPdfDocument(firstOutput);
    const firstImported = await firstReopened.annotations.readPageAnnotations(0);
    const firstImage = firstImported.find((markup) => markup.kind === 'image');
    const firstSnapshot = firstImported.find((markup) => markup.kind === 'snapshot');
    if (!firstImage || firstImage.kind !== 'image' || !firstSnapshot || firstSnapshot.kind !== 'snapshot') {
      throw new Error('Expected native media after first edit');
    }
    expect(firstImage.dataUrl).toBe(image.dataUrl);
    expect(firstSnapshot.dataUrl).toBe(snapshot.dataUrl);

    const secondOutput = file.replace(/\.pdf$/i, '.second-edit.pdf');
    await firstReopened.writer.save(firstReopened, [
      { ...firstImage, rect: { ...firstImage.rect, x: firstImage.rect.x + 9 } },
      { ...firstSnapshot, rect: { ...firstSnapshot.rect, y: firstSnapshot.rect.y - 7 } },
    ], 'saveAs', secondOutput);
    const secondContracts = await readNativeMediaContracts(secondOutput);
    expect(secondContracts.map((contract) => contract.appearanceHash)).toEqual(sourceContracts.map((contract) => contract.appearanceHash));
    expect(secondContracts.map((contract) => contract.payloadHashes)).toEqual(sourceContracts.map((contract) => contract.payloadHashes));

    const secondReopened = await openPdfDocument(secondOutput);
    const deletedOutput = file.replace(/\.pdf$/i, '.deleted.pdf');
    await secondReopened.writer.save(secondReopened, [], 'saveAs', deletedOutput);
    expect(await readNativeMediaContracts(deletedOutput)).toEqual([]);

    await secondReopened.close();
    await firstReopened.close();
    await handle.close();
  });

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

  it('persists page rotation when saving', async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, '.rotated.pdf');

    await handle.writer.save(handle, [], 'saveAs', output, [], [{ pageIndex: 0, rotation: 90 }]);

    const reopened = await openPdfDocument(output);
    const pageInfo = await reopened.getPageInfo(0);
    expect(pageInfo.rotation).toBe(90);
    expect(pageInfo.width).toBeCloseTo(180);
    expect(pageInfo.height).toBeCloseTo(240);

    await handle.close();
    await reopened.close();
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
      appearancePath: 'M 35 35 L 35 70 L 100 70 L 100 35 Z',
    });
    const cloudPlus = createCloudPlusMarkup({
      id: 'cloud-plus-1',
      pageIndex: 0,
      cloud: {
        controlPath: [pdfPoint(30, 85), pdfPoint(30, 120), pdfPoint(95, 120), pdfPoint(95, 85)],
        borderEffectIntensity: 2,
        appearancePath: 'M 30 85 L 30 120 L 95 120 L 95 85 Z',
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
    const rawContracts = await readRawAnnotationContracts(output);
    expect(rawCallout).toMatchObject({
      subtype: '/FreeText',
      intent: '/FreeTextCallout',
      subject: 'Callout',
      defaultAppearance: '1 0 0 rg /Helv 12 Tf',
      defaultStyle: 'font: Helvetica 12pt; text-align:left; margin:3pt; line-height:13.8pt; color:#FF0000',
      color: [],
      border: [0, 0, 0],
      borderStyleWidth: 0,
      lineEnding: ['/None', '/OpenArrow'],
      calloutLine: [30, 30, 60, 60, 100, 90],
      rect: [24.5, 24.5, 205.5, 105.5],
      rectangleDifferences: [85.5, 35.5, 5.5, 5.5],
      appearanceBounds: [24.5, 24.5, 205.5, 105.5],
      appearanceMatrix: [1, 0, 0, 1, -24.5, -24.5],
    });
    expect(rawCallout.richContent).toContain('<p>Need to check</p>');
    expect(rawCloudPlus).toEqual(expect.arrayContaining([
      expect.objectContaining({
        subtype: '/Polygon',
        intent: '/PolygonCloud',
        intentEx: '/PolyText',
        subject: 'Cloud+',
        borderEffect: 'present',
        vertices: [30, 85, 30, 120, 95, 120, 95, 85],
        name: 'bp:cloud-plus-1:cloud',
        groupNesting: [],
      }),
      expect.objectContaining({
        subtype: '/FreeText',
        intent: '/FreeTextCallout',
        intentEx: '/PolyText',
        subject: 'Cloud+',
        calloutLine: [95, 102, 115, 102, 135, 102],
        lineEnding: ['/None', '/None'],
        appearance: 'present',
        name: 'bp:cloud-plus-1:text',
        groupNesting: ['Cloud+', 'bp:cloud-plus-1:text', 'bp:cloud-plus-1:cloud'],
        color: [],
        rect: [89.5, 76.5, 230.5, 127.5],
        rectangleDifferences: [45.5, 5.5, 5.5, 5.5],
        appearanceBounds: [89.5, 76.5, 230.5, 127.5],
        appearanceMatrix: [1, 0, 0, 1, -89.5, -76.5],
      }),
    ]));
    expect(rawCloudPlus.find((annotation) => annotation.subtype === '/FreeText')?.richContent).toContain('<p>Cloud plus</p>');
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
    expect(rawContracts).toEqual(expect.arrayContaining([
      { name: 'bp:rect-1', subtype: '/Square', subject: 'Rectangle', intent: '', intentEx: '', hasAppearance: false },
      { name: 'bp:ellipse-1', subtype: '/Circle', subject: 'Ellipse', intent: '', intentEx: '', hasAppearance: false },
      { name: 'bp:arc-1', subtype: '/Circle', subject: 'Arc', intent: '/CircleArc', intentEx: '', hasAppearance: true },
      { name: 'bp:line-1', subtype: '/Line', subject: 'Line', intent: '', intentEx: '', hasAppearance: false },
      { name: 'bp:arrow-1', subtype: '/Line', subject: 'Arrow', intent: '/LineArrow', intentEx: '', hasAppearance: false },
      { name: 'bp:dimension-1', subtype: '/Line', subject: 'Dimension', intent: '/LineDimension', intentEx: '', hasAppearance: true },
      { name: 'bp:length-1', subtype: '/Line', subject: 'Length Measurement', intent: '/LineDimension', intentEx: '', hasAppearance: true },
      { name: 'bp:polylength-1', subtype: '/PolyLine', subject: 'Polylength Measurement', intent: '/PolyLineDimension', intentEx: '', hasAppearance: true },
      { name: 'bp:area-1', subtype: '/Polygon', subject: 'Area Measurement', intent: '/PolygonDimension', intentEx: '', hasAppearance: true },
      { name: 'bp:polyline-1', subtype: '/PolyLine', subject: 'PolyLine', intent: '', intentEx: '', hasAppearance: true },
      { name: 'bp:polygon-1', subtype: '/Polygon', subject: 'Polygon', intent: '', intentEx: '', hasAppearance: true },
      { name: 'bp:pen-1', subtype: '/Ink', subject: 'Pen', intent: '', intentEx: '', hasAppearance: false },
      { name: 'bp:highlight-1', subtype: '/Ink', subject: 'Highlight', intent: '', intentEx: '', hasAppearance: false },
      { name: 'bp:cloud-1', subtype: '/Polygon', subject: 'Cloud', intent: '/PolygonCloud', intentEx: '', hasAppearance: true },
      { name: 'bp:cloud-plus-1:cloud', subtype: '/Polygon', subject: 'Cloud+', intent: '/PolygonCloud', intentEx: '/PolyText', hasAppearance: true },
      { name: 'bp:cloud-plus-1:text', subtype: '/FreeText', subject: 'Cloud+', intent: '/FreeTextCallout', intentEx: '/PolyText', hasAppearance: true },
      { name: 'bp:image-1', subtype: '/Square', subject: 'Image', intent: '/SquareImage', intentEx: '', hasAppearance: true },
      { name: 'bp:snapshot-1', subtype: '/Stamp', subject: 'Snapshot', intent: '/StampSnapshot', intentEx: '', hasAppearance: true },
      { name: 'bp:text-1', subtype: '/FreeText', subject: 'Text Box', intent: '', intentEx: '', hasAppearance: true },
      { name: 'bp:callout-1', subtype: '/FreeText', subject: 'Callout', intent: '/FreeTextCallout', intentEx: '', hasAppearance: true },
    ]));

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

    const secondOutput = file.replace(/\.pdf$/i, '.second-roundtrip.annotated.pdf');
    await reopened.writer.save(reopened, annotations, 'saveAs', secondOutput, [measurementScale]);
    const secondRawPdf = await PDFDocument.load(await readFile(secondOutput));
    const secondRawAnnots = secondRawPdf.context.lookup(secondRawPdf.getPage(0).node.Annots());
    expect(secondRawAnnots).toBeInstanceOf(PDFArray);
    expect((secondRawAnnots as PDFArray).size()).toBe(20);
    const secondReopened = await openPdfDocument(secondOutput);
    expect(await secondReopened.annotations.readPageAnnotations(0)).toHaveLength(19);

    await handle.close();
    await reopened.close();
    await secondReopened.close();
  });

  it('preserves untouched Bluebeam annotations and replaces complete logical objects on edit or delete', async () => {
    const file = await createBluebeamNativeFixturePdf();
    const handle = await openPdfDocument(file);
    const imported = await handle.annotations.readPageAnnotations(0);
    expect(imported).toHaveLength(3);
    expect(imported.map((markup) => markup.kind)).toEqual(['cloud-plus', 'rectangle', 'imported-annotation']);
    const cloudPlus = imported.find((markup) => markup.kind === 'cloud-plus');
    const rectangle = imported.find((markup) => markup.kind === 'rectangle');
    const reply = imported.find((markup) => markup.kind === 'imported-annotation');
    expect(cloudPlus?.source?.annotationIds).toEqual(['nm:BB-CLOUD', 'nm:BB-TEXT']);
    expect(cloudPlus).toMatchObject({ textBox: { x: 225, y: 35, width: 65, height: 40 } });
    expect(cloudPlus?.source?.annotationMetadata?.map((metadata) => metadata.role)).toEqual(['cloud', 'text']);
    expect(rectangle?.source?.annotationMetadata).toEqual([expect.objectContaining({
      annotationId: 'nm:BB-RECT',
      role: 'primary',
      author: 'A. Reviewer',
      subject: 'Structural review',
      creationDate: 'D:20260803101500+10\'00\'',
      modificationDate: 'D:20260803103000+10\'00\'',
      contents: 'Keep this independent comment',
      flags: 4,
      statusModel: 'Review',
      status: 'Accepted',
    })]);
    expect(rectangle).toMatchObject({ kind: 'rectangle', rotation: 15 });

    const untouchedOutput = file.replace(/\.pdf$/i, '.untouched.pdf');
    await handle.writer.save(handle, imported, 'saveAs', untouchedOutput);
    const untouchedPdf = await PDFDocument.load(await readFile(untouchedOutput));
    const untouchedAnnots = untouchedPdf.context.lookup(untouchedPdf.getPage(0).node.Annots()) as PDFArray;
    expect(untouchedAnnots.size()).toBe(4);
    const untouchedDicts = untouchedAnnots.asArray().map((ref) => untouchedPdf.context.lookup(ref)) as PDFDict[];
    expect(untouchedDicts.map((annot) => readPdfText(annot.get(PDFName.of('NM'))))).toEqual(['BB-RECT', 'BB-CLOUD', 'BB-TEXT', 'BB-REPLY']);
    expect(readPdfText(untouchedDicts[0]?.get(PDFName.of('BPProbe')))).toBe('preserve-me');

    if (!cloudPlus || cloudPlus.kind !== 'cloud-plus') {
      throw new Error('Expected imported Cloud+');
    }
    const editedCloudPlus = { ...cloudPlus, text: 'Edited in Butter Paper' };
    const editedOutput = file.replace(/\.pdf$/i, '.edited.pdf');
    await handle.writer.save(handle, [rectangle!, reply!, editedCloudPlus], 'saveAs', editedOutput);
    const editedPdf = await PDFDocument.load(await readFile(editedOutput));
    const editedAnnots = editedPdf.context.lookup(editedPdf.getPage(0).node.Annots()) as PDFArray;
    expect(editedAnnots.size()).toBe(4);
    const editedNames = editedAnnots.asArray().map((ref) => {
      const annot = editedPdf.context.lookup(ref) as PDFDict;
      return readPdfText(annot.get(PDFName.of('NM')));
    });
    expect(editedNames).toContain('BB-RECT');
    expect(editedNames).not.toContain('BB-CLOUD');
    expect(editedNames).not.toContain('BB-TEXT');
    expect(editedNames).toEqual(expect.arrayContaining([
      `bp:${cloudPlus.id}:cloud`,
      `bp:${cloudPlus.id}:text`,
    ]));
    const editedCloudMetadata = await readRawAnnotationMetadata(editedOutput);
    expect(editedCloudMetadata).toEqual(expect.arrayContaining([
      expect.objectContaining({ name: `bp:${cloudPlus.id}:cloud`, subject: 'Custom cloud subject', replyType: '/Group', inReplyTo: `bp:${cloudPlus.id}:text` }),
      expect.objectContaining({ name: `bp:${cloudPlus.id}:text`, subject: 'Custom cloud subject', replyType: '', inReplyTo: undefined }),
    ]));

    if (!rectangle || rectangle.kind !== 'rectangle' || !reply) {
      throw new Error('Expected imported rectangle and reply');
    }
    const editedRectangle = {
      ...rectangle,
      rect: { ...rectangle.rect, x: rectangle.rect.x + 10 },
    };
    const metadataOutput = file.replace(/\.pdf$/i, '.metadata-edited.pdf');
    const fixedClockWriter = new PdfAnnotationWriter(file, () => new Date('2026-08-04T01:02:03.000Z'));
    await fixedClockWriter.save(handle, [cloudPlus, reply, editedRectangle], 'saveAs', metadataOutput);
    const editedMetadata = await readRawAnnotationMetadata(metadataOutput);
    expect(editedMetadata).toEqual(expect.arrayContaining([
      expect.objectContaining({
        name: `bp:${rectangle.id}`,
        author: 'A. Reviewer',
        subject: 'Structural review',
        creationDate: 'D:20260803101500+10\'00\'',
        modificationDate: 'D:20260804010203Z',
        contents: 'Keep this independent comment',
        flags: 4,
        stateModel: 'Review',
        state: 'Accepted',
        unsafeProbe: undefined,
      }),
      expect.objectContaining({
        name: 'BB-REPLY',
        replyType: '/Reply',
        inReplyTo: `bp:${rectangle.id}`,
      }),
    ]));

    const deletedOutput = file.replace(/\.pdf$/i, '.deleted.pdf');
    await handle.writer.save(handle, [], 'saveAs', deletedOutput);
    const deletedPdf = await PDFDocument.load(await readFile(deletedOutput));
    const deletedAnnots = deletedPdf.context.lookup(deletedPdf.getPage(0).node.Annots()) as PDFArray;
    expect(deletedAnnots.size()).toBe(0);

    await handle.close();
  });

  it('classifies native measurement tools from intent and Measure dictionaries when subjects are customized', async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, '.subject-independent-measurements.pdf');
    const scale = createCustomPageScale({
      pageIndex: 0,
      name: '1:1',
      pdfUnits: 'cm',
      realUnits: 'm',
      scaleX: 1,
      scaleY: 1,
    });
    await handle.writer.save(handle, [
      createLengthMarkup({ id: 'custom-subject-length', pageIndex: 0, start: pdfPoint(20, 20), end: pdfPoint(100, 20) }),
      createPolylengthMarkup({ id: 'custom-subject-polylength', pageIndex: 0, points: [pdfPoint(20, 50), pdfPoint(60, 80), pdfPoint(100, 50)] }),
      createAreaMarkup({ id: 'custom-subject-area', pageIndex: 0, points: [pdfPoint(140, 20), pdfPoint(220, 20), pdfPoint(220, 80), pdfPoint(140, 80)] }),
    ], 'saveAs', output, [scale]);

    const raw = await PDFDocument.load(await readFile(output));
    const annots = raw.context.lookup(raw.getPage(0).node.Annots()) as PDFArray;
    for (const ref of annots.asArray()) {
      const annotation = raw.context.lookup(ref);
      if (annotation instanceof PDFDict) {
        annotation.set(PDFName.of('Subj'), PDFString.of('Alex custom review subject'));
      }
    }
    await writeFile(output, await raw.save());

    const reopened = await openPdfDocument(output);
    expect((await reopened.annotations.readPageAnnotations(0)).map((markup) => markup.kind)).toEqual([
      'length',
      'polylength',
      'area',
    ]);
    await reopened.close();
    await handle.close();
  });

  it('round-trips a native Cloud+ inline label without a visible leader', async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, '.inline-cloud-plus.pdf');
    const inline = createCloudPlusMarkup({
      id: 'inline-cloud-plus',
      pageIndex: 0,
      cloud: {
        controlPath: [pdfPoint(30, 30), pdfPoint(30, 120), pdfPoint(210, 120), pdfPoint(210, 30)],
      },
      leader: { points: [] },
      textBox: { x: 70, y: 58, width: 100, height: 34 },
      text: 'Inside cloud',
    });

    await handle.writer.save(handle, [inline], 'saveAs', output);
    const raw = await readRawCloudPlusAnnotations(output);
    expect(raw.find((annotation) => annotation.subtype === '/FreeText')).toMatchObject({
      calloutLine: [120, 75, 120, 75, 120, 75],
      color: [],
      rect: [64.5, 52.5, 175.5, 97.5],
      rectangleDifferences: [5.5, 5.5, 5.5, 5.5],
      appearanceBounds: [64.5, 52.5, 175.5, 97.5],
      appearanceMatrix: [1, 0, 0, 1, -64.5, -52.5],
    });
    const reopened = await openPdfDocument(output);
    const imported = await reopened.annotations.readPageAnnotations(0);
    expect(imported).toHaveLength(1);
    expect(imported[0]).toMatchObject({
      kind: 'cloud-plus',
      leader: { points: [] },
      text: 'Inside cloud',
    });

    await handle.close();
    await reopened.close();
  });

  it.each([
    {
      name: 'two-point',
      points: [pdfPoint(90, 50), pdfPoint(130, 50)],
      expected: [90, 50, 110, 50, 130, 50],
    },
    {
      name: 'four-point',
      points: [pdfPoint(90, 50), pdfPoint(100, 60), pdfPoint(115, 55), pdfPoint(130, 50)],
      expected: [90, 50, 100, 60, 130, 50],
    },
  ])('normalizes $name edited Cloud+ leaders to three native CL points', async ({ points, expected }) => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, `.cloud-plus-${points.length}.pdf`);
    const cloudPlus = createCloudPlusMarkup({
      id: `cloud-plus-${points.length}`,
      pageIndex: 0,
      cloud: {
        controlPath: [pdfPoint(10, 10), pdfPoint(10, 90), pdfPoint(90, 90), pdfPoint(90, 10)],
      },
      leader: { points },
      textBox: { x: 130, y: 28, width: 100, height: 44 },
      text: 'Canonical leader',
    });

    await handle.writer.save(handle, [cloudPlus], 'saveAs', output);
    const raw = await readRawCloudPlusAnnotations(output);
    expect(raw.find((annotation) => annotation.subtype === '/FreeText')?.calloutLine).toEqual(expected);
    await handle.close();
  });

  it.each([
    {
      name: 'two-point',
      points: [pdfPoint(90, 50), pdfPoint(130, 50)],
      expected: [90, 50, 130, 50],
      appearanceSegment: '130 50 m 90 50 l',
    },
    {
      name: 'four-point',
      points: [pdfPoint(90, 50), pdfPoint(100, 60), pdfPoint(115, 55), pdfPoint(130, 50)],
      expected: [90, 50, 100, 60, 130, 50],
      appearanceSegment: '130 50 m 100 60 l 90 50 l',
    },
  ])('normalizes $name edited Callout leaders to native CL geometry and matching AP', async ({ points, expected, appearanceSegment }) => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, `.callout-${points.length}.pdf`);
    const callout = createCalloutMarkup({
      id: `callout-${points.length}`,
      pageIndex: 0,
      leader: { points },
      textBox: { x: 130, y: 28, width: 100, height: 44 },
      text: 'Canonical callout',
    });

    await handle.writer.save(handle, [callout], 'saveAs', output);
    const raw = await readRawCalloutAnnotation(output);
    expect(raw.calloutLine).toEqual(expected);
    expect(raw.appearanceContent).toContain(appearanceSegment);
    if (points.length === 4) expect(raw.appearanceContent).not.toContain('115 55 l');
    await handle.close();
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
