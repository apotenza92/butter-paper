import { readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { Markup, MarkupAppearance, PageScale, PdfPoint, ResolvedMarkupAppearance, TextBoxRichTextRun } from '@butter-paper/core';
import { convertScaledValueUnit, createArcMarkup, createAreaMarkup, createArrowMarkup, createCalloutMarkup, createCloudMarkup, createCloudPlusMarkup, createDimensionMarkup, createEllipseMarkup, createHighlightMarkup, createImageMarkup, createImportedAnnotationMarkup, createLengthMarkup, createLineMarkup, createPenMarkup, createPolygonMarkup, createPolylengthMarkup, createPolylineMarkup, createRectangleMarkup, createSnapshotMarkup, createTextBoxMarkup, formatScaledAreaLabel, formatScaledLengthLabel, measureScaledLength, measureScaledPolygonArea, measureScaledPolyline, pdfPoint, resolveMarkupAppearance } from '@butter-paper/core';
import fontkit from '@pdf-lib/fontkit';
import { decodePDFRawStream, PDFArray, PDFBool, PDFDocument, PDFHexString, PDFName, PDFNumber, PDFRawStream, PDFString, StandardFonts, type PDFDict, type PDFFont, type PDFImage, type PDFPage, type PDFRef } from 'pdf-lib';
import { PdfRenderCache } from './cache.js';
import { normalizePdfRect, pointArrayToPdfPoints } from './geometry.js';
import { createBrowserCanvas, createNodeCanvasFactory } from './canvas.js';
import type {
  ImportedPdfMarkup,
  PdfCanvasLike,
  PdfDocumentMetadata,
  PdfPageInfo,
  PdfRenderRequest,
  PdfRenderedPage,
  PdfSaveMode,
  PdfSaveResult,
} from './types.js';

const require = createRequire(import.meta.url);
const pdfjsPackageRoot = dirname(require.resolve('pdfjs-dist/package.json'));
const standardFontDataUrl = `${pathToFileURL(join(pdfjsPackageRoot, 'standard_fonts')).toString()}/`;
let pdfjsModulePromise: Promise<any> | undefined;
let pdfjsGlobalsReady = false;
const managedAnnotationPrefix = 'bp:';
const managedAnnotationSubject = 'Butter Paper';
const bluebeamTextBoxFontSizePt = 12;
const bluebeamTextBoxInsetPt = 5;
const bluebeamTextBoxLineHeightRatio = 1.15;
const bluebeamTextBoxFirstBaselineOffsetRatio = 14.3146 / 12;
const unicodeFontPaths = [
  '/System/Library/Fonts/Supplemental/Arial Unicode.ttf',
  '/System/Library/Fonts/Hiragino Sans GB.ttc',
];

interface PdfExportFonts {
  readonly helvetica: PDFFont;
  readonly helveticaBold: PDFFont;
  readonly helveticaOblique: PDFFont;
  readonly helveticaBoldOblique: PDFFont;
  readonly unicode?: PDFFont;
}

interface PdfTextBoxFont {
  readonly font: PDFFont;
  readonly resourceName: 'Helv' | 'HelvBold' | 'HelvOblique' | 'HelvBoldOblique' | 'BPUnicode';
  readonly styleName: 'Helvetica' | 'Arial Unicode MS';
  readonly usesEmbeddedUnicodeFont: boolean;
}

interface TextBoxAppearanceRun extends TextBoxRichTextRun {
  readonly text: string;
}

interface TextBoxAppearanceLine {
  readonly runs: readonly TextBoxAppearanceRun[];
}

interface PdfJsDocumentLike {
  numPages: number;
  getMetadata(): Promise<{ info?: Record<string, unknown> }>;
  getPage(pageNumber: number): Promise<PdfJsPageLike>;
  destroy(): Promise<void>;
}

interface PdfJsPageLike {
  rotate: number;
  getViewport(params: { scale: number; rotation?: number }): { width: number; height: number };
  render(params: { canvasContext: CanvasRenderingContext2D; viewport: { width: number; height: number }; annotationMode?: number }): { promise: Promise<void> };
  getAnnotations(params: { intent: 'display' }): Promise<readonly PdfJsAnnotation[]>;
}

interface PdfJsAnnotation {
  id?: string;
  subtype?: string;
  intent?: string;
  it?: string;
  rect?: readonly number[];
  contents?: string;
  contentsObj?: { readonly str?: string };
  richText?: {
    readonly str?: string;
    readonly html?: {
      readonly attributes?: {
        readonly style?: {
          readonly textAlign?: string;
        };
      };
    };
  };
  textContent?: readonly string[];
  borderColor?: ArrayLike<number> | null;
  borderStyle?: {
    readonly width?: number;
    readonly rawWidth?: number;
  };
  defaultAppearanceData?: {
    readonly fontSize?: number;
    readonly fontColor?: ArrayLike<number>;
  };
  calloutLine?: readonly number[];
  lineCoordinates?: readonly number[];
  cl?: readonly number[];
  inkLists?: readonly (readonly number[])[];
  inkList?: readonly (readonly number[])[];
}

export class PdfDocumentHandle {
  readonly annotations: PdfAnnotationAdapter;
  readonly writer: PdfAnnotationWriter;
  readonly cache: PdfRenderCache;

  private constructor(
    readonly path: string,
    private readonly document: PdfJsDocumentLike,
    cacheLimits = { maxEntries: 6, maxBytes: 32 * 1024 * 1024 },
  ) {
    this.annotations = new PdfAnnotationAdapter(path);
    this.writer = new PdfAnnotationWriter(path);
    this.cache = new PdfRenderCache(cacheLimits);
  }

  get pageCount(): number {
    return this.document.numPages;
  }

  static async open(path: string, options?: { cacheLimits?: { maxEntries: number; maxBytes: number } }): Promise<PdfDocumentHandle> {
    const data = new Uint8Array(await readFile(path));
    const pdfjs = await loadPdfJsModule();
    const loadingTask = pdfjs.getDocument({
      data,
      standardFontDataUrl,
      useSystemFonts: true,
      disableWorker: true,
    } as never);
    const document = (await loadingTask.promise) as unknown as PdfJsDocumentLike;
    return new PdfDocumentHandle(path, document, options?.cacheLimits);
  }

  async getMetadata(): Promise<PdfDocumentMetadata> {
    const metadata = await this.document.getMetadata().catch(() => undefined);
    const info = metadata?.info ?? {};
    return {
      pageCount: this.document.numPages,
      title: stringOrUndefined(info.Title),
      author: stringOrUndefined(info.Author),
      subject: stringOrUndefined(info.Subject),
      creator: stringOrUndefined(info.Creator),
      producer: stringOrUndefined(info.Producer),
    };
  }

  async getPageInfo(pageIndex: number): Promise<PdfPageInfo> {
    const page = await this.document.getPage(pageIndex + 1);
    const rotation = normalizeRotation(page.rotate);
    const viewport = page.getViewport({ scale: 1, rotation });
    return {
      index: pageIndex,
      width: viewport.width,
      height: viewport.height,
      rotation,
    };
  }

  async renderPage(request: PdfRenderRequest): Promise<PdfRenderedPage> {
    const pageInfo = await this.getPageInfo(request.pageIndex);
    const rotation = request.rotation ?? pageInfo.rotation;
    const cacheKey = `${request.pageIndex}:${request.scale}:${rotation}`;
    const cached = this.cache.get(cacheKey);
    if (cached) {
      return cached;
    }

    const page = await this.document.getPage(request.pageIndex + 1);
    const viewport = page.getViewport({ scale: request.scale, rotation });
    const canvas = request.canvas ?? (await createRenderCanvas(viewport.width, viewport.height));
    canvas.width = Math.ceil(viewport.width);
    canvas.height = Math.ceil(viewport.height);

    const context = canvas.getContext('2d');
    if (!context) {
      throw new Error('Unable to create 2D rendering context');
    }

    await page.render({ canvasContext: context, viewport, annotationMode: request.renderAnnotations ? 1 : 0 }).promise;

    const rendered: PdfRenderedPage = {
      pageIndex: request.pageIndex,
      width: canvas.width,
      height: canvas.height,
      canvas,
    };
    this.cache.set(cacheKey, rendered);
    return rendered;
  }

  async close(): Promise<void> {
    await this.document.destroy();
  }
}

export async function openPdfDocument(path: string): Promise<PdfDocumentHandle> {
  return PdfDocumentHandle.open(path);
}

export class PdfAnnotationAdapter {
  constructor(private readonly sourcePath: string) {}

  async readPageAnnotations(pageIndex: number): Promise<ImportedPdfMarkup[]> {
    const sourceBytes = new Uint8Array(await readFile(this.sourcePath));
    const pdfDoc = await PDFDocument.load(sourceBytes);
    return readPageAnnotationMarkups(pdfDoc, pageIndex);
  }

  async readAllPageAnnotations(): Promise<ImportedPdfMarkup[][]> {
    const sourceBytes = new Uint8Array(await readFile(this.sourcePath));
    const pdfDoc = await PDFDocument.load(sourceBytes);
    const annotationsByPage: ImportedPdfMarkup[][] = [];

    for (let pageIndex = 0; pageIndex < pdfDoc.getPageCount(); pageIndex += 1) {
      annotationsByPage.push(readPageAnnotationMarkups(pdfDoc, pageIndex));
    }

    return annotationsByPage;
  }
}

export class PdfAnnotationWriter {
  constructor(private readonly sourcePath: string) {}

  async save(_document: PdfDocumentHandle, markups: readonly Markup[], mode: PdfSaveMode, targetPath?: string, pageScales: readonly PageScale[] = []): Promise<PdfSaveResult> {
    const sourceBytes = await readFile(this.sourcePath);
    const pdfDoc = await PDFDocument.load(sourceBytes, { updateMetadata: false });
    const pagesByIndex = groupMarkupsByPage(markups);
    const fonts = await createPdfExportFonts(pdfDoc, markups);

    for (let pageIndex = 0; pageIndex < pdfDoc.getPageCount(); pageIndex += 1) {
      stripManagedAnnotations(pdfDoc, pdfDoc.getPage(pageIndex));
    }

    for (const [pageIndex, pageMarkups] of pagesByIndex.entries()) {
      const page = pdfDoc.getPage(pageIndex);
      stripManagedAnnotations(pdfDoc, page);
      for (const markup of pageMarkups) {
        await addMarkupAnnotation(pdfDoc, page, markup, fonts, pageScales);
      }
    }

    const bytes = await pdfDoc.save();
    const outputPath = mode === 'saveAs' ? targetPath : this.sourcePath;
    if (!outputPath) {
      throw new Error('saveAs requires a targetPath');
    }

    await writeFile(outputPath, bytes);
    return {
      path: outputPath,
      bytesWritten: bytes.length,
    };
  }
}

async function createPdfExportFonts(pdfDoc: PDFDocument, markups: readonly Markup[]): Promise<PdfExportFonts> {
  const helvetica = await pdfDoc.embedFont(StandardFonts.Helvetica);
  const helveticaBold = await pdfDoc.embedFont(StandardFonts.HelveticaBold);
  const helveticaOblique = await pdfDoc.embedFont(StandardFonts.HelveticaOblique);
  const helveticaBoldOblique = await pdfDoc.embedFont(StandardFonts.HelveticaBoldOblique);
  const standardFonts = {
    helvetica,
    helveticaBold,
    helveticaOblique,
    helveticaBoldOblique,
  };
  if (!markups.some((markup) => markup.kind === 'text-box' && needsUnicodeTextBoxFont(markup))) {
    return standardFonts;
  }

  const unicodeBytes = await readFirstAvailableFile(unicodeFontPaths);
  if (!unicodeBytes) {
    throw new Error(`Unable to export Unicode text box: no compatible Unicode font found in ${unicodeFontPaths.join(', ')}`);
  }

  pdfDoc.registerFontkit(fontkit);
  return {
    ...standardFonts,
    unicode: await pdfDoc.embedFont(unicodeBytes, { subset: true }),
  };
}

async function readFirstAvailableFile(paths: readonly string[]): Promise<Uint8Array | undefined> {
  for (const path of paths) {
    const bytes = await readFile(path).catch(() => undefined);
    if (bytes) {
      return bytes;
    }
  }
  return undefined;
}

async function createRenderCanvas(width: number, height: number): Promise<PdfCanvasLike> {
  if (typeof document !== 'undefined') {
    return createBrowserCanvas(width, height);
  }

  const factory = await createNodeCanvasFactory();
  return factory.create(width, height);
}

function mapAnnotationToMarkup(pageIndex: number, annotation: PdfJsAnnotation): ImportedPdfMarkup | undefined {
  const subtype = String(annotation.subtype ?? '').toLowerCase();

  if (subtype === 'square' || subtype === 'rect') {
    return createRectangleMarkup({
      id: annotation.id ?? `page-${pageIndex}-rectangle`,
      pageIndex,
      rect: normalizePdfRect(annotation.rect ?? [0, 0, 0, 0]),
      source: {
        annotationId: annotation.id,
        source: 'imported',
      },
    });
  }

  const intent = String(annotation.intent ?? annotation.it ?? '').toLowerCase();
  if (subtype === 'circle' && intent !== 'circlearc') {
    return createEllipseMarkup({
      id: annotation.id ?? `page-${pageIndex}-ellipse`,
      pageIndex,
      rect: normalizePdfRect(annotation.rect ?? [0, 0, 0, 0]),
      source: {
        annotationId: annotation.id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'line') {
    const linePoints = pointArrayToPdfPoints(annotation.lineCoordinates ?? annotation.cl);
    const [start = pdfPoint((annotation.rect?.[0] ?? 0), (annotation.rect?.[1] ?? 0)), end = pdfPoint((annotation.rect?.[2] ?? 0), (annotation.rect?.[3] ?? 0))] = linePoints;
    const subject = String((annotation as { subject?: string }).subject ?? '').toLowerCase();
    if (isLengthMeasurementSubject(subject)) {
      return createLengthMarkup({
        id: annotation.id ?? `page-${pageIndex}-length`,
        pageIndex,
        start,
        end,
        color: colorToHex(annotation.borderColor),
        source: {
          annotationId: annotation.id,
          source: 'imported',
        },
      });
    }
    if (intent === 'linedimension') {
      return createDimensionMarkup({
        id: annotation.id ?? `page-${pageIndex}-dimension`,
        pageIndex,
        start,
        end,
        dimensionLineOffset: end.x >= start.x ? 24 : -24,
        text: String(annotation.contents ?? 'Dimension'),
        color: colorToHex(annotation.borderColor),
        source: {
          annotationId: annotation.id,
          source: 'imported',
        },
      });
    }
    const createMarkup = intent === 'linearrow' ? createArrowMarkup : createLineMarkup;
    return createMarkup({
      id: annotation.id ?? `page-${pageIndex}-${intent === 'linearrow' ? 'arrow' : 'line'}`,
      pageIndex,
      start,
      end,
      color: colorToHex(annotation.borderColor),
      source: {
        annotationId: annotation.id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'polyline') {
    const points = pointArrayToPdfPoints((annotation as { vertices?: readonly number[] }).vertices);
    const subject = String((annotation as { subject?: string }).subject ?? '').toLowerCase();
    const createMarkup = isPolylengthMeasurementSubject(subject) ? createPolylengthMarkup : createPolylineMarkup;
    return createMarkup({
      id: annotation.id ?? `page-${pageIndex}-${isPolylengthMeasurementSubject(subject) ? 'polylength' : 'polyline'}`,
      pageIndex,
      points: points.length >= 2 ? points : pointArrayToPdfPoints(annotation.rect),
      color: colorToHex(annotation.borderColor),
      source: {
        annotationId: annotation.id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'polygon' && intent === 'polygoncloud') {
    const points = pointArrayToPdfPoints((annotation as { vertices?: readonly number[] }).vertices);
    return createCloudMarkup({
      id: annotation.id ?? `page-${pageIndex}-cloud`,
      pageIndex,
      controlPath: points.length >= 3 ? points : pointArrayToPdfPoints(annotation.rect),
      color: colorToHex(annotation.borderColor),
      borderEffectIntensity: 2,
      scallopRadius: readScallopRadiusFromRect(annotation.rect, points.length >= 3 ? points : pointArrayToPdfPoints(annotation.rect)),
      source: {
        annotationId: annotation.id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'polygon') {
    const points = pointArrayToPdfPoints((annotation as { vertices?: readonly number[] }).vertices);
    const subject = String((annotation as { subject?: string }).subject ?? '').toLowerCase();
    const createMarkup = isAreaMeasurementSubject(subject) ? createAreaMarkup : createPolygonMarkup;
    return createMarkup({
      id: annotation.id ?? `page-${pageIndex}-${isAreaMeasurementSubject(subject) ? 'area' : 'polygon'}`,
      pageIndex,
      points: points.length >= 3 ? points : pointArrayToPdfPoints(annotation.rect),
      color: colorToHex(annotation.borderColor),
      source: {
        annotationId: annotation.id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'ink') {
    const subject = String((annotation as { subject?: string }).subject ?? annotation.contents ?? '').toLowerCase();
    const paths = readPdfJsInkLists(annotation.inkLists ?? annotation.inkList, annotation.rect);
    const createMarkup = subject === 'highlight' ? createHighlightMarkup : createPenMarkup;
    return createMarkup({
      id: annotation.id ?? `page-${pageIndex}-${subject === 'highlight' ? 'highlight' : 'pen'}`,
      pageIndex,
      paths,
      strokeWidth: annotation.borderStyle?.width ?? annotation.borderStyle?.rawWidth,
      color: colorToHex(annotation.borderColor),
      source: {
        annotationId: annotation.id,
        source: 'imported',
      },
    });
  }

  const hasCalloutLine = Boolean(annotation.calloutLine || annotation.lineCoordinates || annotation.cl);
  if (subtype === 'freetext' && (intent === 'freetextcallout' || hasCalloutLine)) {
    const textBox = normalizePdfRect(annotation.rect ?? [0, 0, 0, 0]);
    const linePoints = pointArrayToPdfPoints(annotation.calloutLine ?? annotation.lineCoordinates ?? annotation.cl);
    return createCalloutMarkup({
      id: annotation.id ?? `page-${pageIndex}-callout`,
      pageIndex,
      leader: {
        points: linePoints.length > 0 ? linePoints : [pdfPoint(textBox.x, textBox.y), pdfPoint(textBox.x + textBox.width, textBox.y + textBox.height)],
      },
      textBox,
      text: String(annotation.contents ?? ''),
      source: {
        annotationId: annotation.id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'freetext') {
    const textColor = colorToHex(annotation.defaultAppearanceData?.fontColor);
    const borderColor = colorToHex(annotation.borderColor) ?? textColor;
    return createTextBoxMarkup({
      id: annotation.id ?? `page-${pageIndex}-text-box`,
      pageIndex,
      rect: normalizePdfRect(annotation.rect ?? [0, 0, 0, 0]),
      text: getAnnotationText(annotation),
      richTextRuns: readRichTextRuns(getAnnotationRichText(annotation)),
      appearanceTextLines: annotation.textContent,
      color: textColor,
      borderColor,
      borderWidth: annotation.borderStyle?.width ?? annotation.borderStyle?.rawWidth ?? 0,
      fontFamily: readTextBoxFontFamily(annotation),
      fontSizePt: annotation.defaultAppearanceData?.fontSize,
      textAlign: readPdfJsTextAlign(annotation),
      source: {
        annotationId: annotation.id,
        source: 'imported',
      },
    });
  }

  return undefined;
}

function getAnnotationText(annotation: PdfJsAnnotation): string {
  return String(annotation.contents ?? annotation.contentsObj?.str ?? annotation.richText?.str ?? '');
}

function getAnnotationRichText(annotation: PdfJsAnnotation): string | undefined {
  const richText = annotation.richText as { readonly html?: unknown; readonly str?: string } | undefined;
  return typeof richText?.html === 'string' ? richText.html : undefined;
}

function colorToHex(color: ArrayLike<number> | null | undefined): string | undefined {
  if (!color || color.length < 3) {
    return undefined;
  }

  const [red, green, blue] = [color[0], color[1], color[2]].map((value) => {
    const channel = value <= 1 ? value * 255 : value;
    return Math.max(0, Math.min(255, Math.round(channel))).toString(16).padStart(2, '0');
  });

  return `#${red}${green}${blue}`;
}

function readPdfJsTextAlign(annotation: PdfJsAnnotation): 'left' | 'center' | 'right' | undefined {
  const style = annotation.richText?.html?.attributes?.style;
  const align = typeof style?.textAlign === 'string' ? style.textAlign.toLowerCase() : undefined;
  if (align === 'center' || align === 'right' || align === 'left') {
    return align;
  }
  return undefined;
}

function readTextBoxFontFamily(annotation: PdfJsAnnotation): 'Helvetica' | 'ArialUnicode' {
  const style = annotation.richText?.html?.attributes?.style;
  const font = typeof (style as { font?: unknown } | undefined)?.font === 'string' ? (style as { font: string }).font : '';
  return font.toLowerCase().includes('arial unicode') ? 'ArialUnicode' : 'Helvetica';
}

function groupMarkupsByPage(markups: readonly Markup[]): Map<number, readonly Markup[]> {
  const pages = new Map<number, Markup[]>();
  for (const markup of markups) {
    const list = pages.get(markup.pageIndex) ?? [];
    list.push(markup);
    pages.set(markup.pageIndex, list);
  }
  return pages;
}

function stripManagedAnnotations(pdfDoc: PDFDocument, page: PDFPage): void {
  const annots = page.node.Annots();
  if (!annots) {
    return;
  }

  const filtered = annots.asArray().filter((annotRef) => {
    const annot = pdfDoc.context.lookup(annotRef);
    if (!isPdfDict(annot)) {
      return true;
    }

    const annotationName = readText(annot.get(PDFName.of('NM')));
    const subject = readText(annot.get(PDFName.of('Subj')));
    return !(annotationName?.startsWith(managedAnnotationPrefix) || subject === managedAnnotationSubject);
  });

  page.node.set(PDFName.of('Annots'), pdfDoc.context.obj(filtered));
}

async function addMarkupAnnotation(pdfDoc: PDFDocument, page: PDFPage, markup: Markup, fonts: PdfExportFonts, pageScales: readonly PageScale[] = []): Promise<void> {
  const resolvedAppearance = resolveMarkupAppearance(markup);
  const stroke = resolvedAppearance.stroke;
  const fill = resolvedAppearance.fill;
  const textAppearance = resolvedAppearance.text;
  const opacityFields = pdfAnnotationOpacityFields(
    resolvedAppearance.opacity,
    stroke?.color,
    fill?.color ?? textAppearance?.color,
  );
  switch (markup.kind) {
    case 'rectangle': {
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Square'),
        Rect: [markup.rect.x, markup.rect.y, markup.rect.x + markup.rect.width, markup.rect.y + markup.rect.height],
        Border: [0, 0, stroke?.widthPt ?? 0],
        BS: pdfDoc.context.obj({ W: PDFNumber.of(stroke?.widthPt ?? 0), S: PDFName.of('S'), Type: PDFName.of('Border') }),
        C: pdfColorArray(stroke?.color),
        ...(fill?.color ? { IC: pdfColorArray(fill.color) } : {}),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of(managedAnnotationSubject),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
      });
      if (markup.rotation) {
        annot.set(PDFName.of('Rotation'), PDFNumber.of(normalizeFreeRotation(markup.rotation)));
      }
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'ellipse': {
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Circle'),
        Rect: [markup.rect.x, markup.rect.y, markup.rect.x + markup.rect.width, markup.rect.y + markup.rect.height],
        Border: [0, 0, stroke?.widthPt ?? 0],
        BS: pdfDoc.context.obj({ W: PDFNumber.of(stroke?.widthPt ?? 0), S: PDFName.of('S'), Type: PDFName.of('Border') }),
        C: pdfColorArray(stroke?.color),
        ...(fill?.color ? { IC: pdfColorArray(fill.color) } : {}),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Ellipse'),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
      });
      if (markup.rotation) {
        annot.set(PDFName.of('Rotation'), PDFNumber.of(normalizeFreeRotation(markup.rotation)));
      }
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'arc': {
      const appearance = createArcAppearance(pdfDoc, markup);
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Circle'),
        Rect: [markup.rect.x, markup.rect.y, markup.rect.x + markup.rect.width, markup.rect.y + markup.rect.height],
        C: pdfColorArray(stroke?.color),
        Border: [0, 0, stroke?.widthPt ?? 0],
        ...opacityFields,
        RD: [0.5, 0.5, 0.5, 0.5],
        Angle1: PDFNumber.of(markup.angle1),
        Angle2: PDFNumber.of(markup.angle2),
        IT: PDFName.of('CircleArc'),
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Arc'),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'line':
    case 'arrow': {
      const isArrow = markup.kind === 'arrow';
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Line'),
        Rect: lineAnnotationRect(markup.start, markup.end),
        L: [markup.start.x, markup.start.y, markup.end.x, markup.end.y],
        Border: [0, 0, stroke?.widthPt ?? 0],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(stroke?.widthPt ?? 0),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of(isArrow ? 'Arrow' : 'Line'),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
        ...(isArrow
          ? {
              IT: PDFName.of('LineArrow'),
              LE: [PDFName.of('None'), PDFName.of('ClosedArrow')],
              IC: pdfColorArray(stroke?.color),
            }
          : {}),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'dimension': {
      const appearance = createDimensionAppearance(pdfDoc, markup, fonts);
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Line'),
        Rect: dimensionAnnotationRect(markup, fonts),
        L: [markup.start.x, markup.start.y, markup.end.x, markup.end.y],
        Border: [0, 0, stroke?.widthPt ?? 0],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(stroke?.widthPt ?? 0),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        IT: PDFName.of('LineDimension'),
        LE: [PDFName.of('ClosedArrow'), PDFName.of('ClosedArrow')],
        LL: PDFNumber.of(markup.dimensionLineOffset),
        LLE: PDFNumber.of(4),
        Cap: PDFString.of(markup.text),
        Contents: PDFString.of(markup.text),
        DA: PDFString.of(pdfTextDefaultAppearance(textAppearance)),
        DS: PDFString.of(pdfTextDefaultStyle(textAppearance)),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Dimension'),
        F: PDFNumber.of(4),
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'length': {
      const pageScale = pageScales.find((scale) => scale.pageIndex === markup.pageIndex);
      const label = measurementLabel(markup, pageScale);
      const appearance = createLengthAppearance(pdfDoc, markup, label, fonts);
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Line'),
        Rect: rectToPdfArray(measurementLengthBounds(markup, label, fonts)),
        L: [markup.start.x, markup.start.y, markup.end.x, markup.end.y],
        Border: [0, 0, stroke?.widthPt ?? 0],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(stroke?.widthPt ?? 0),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        IT: PDFName.of('LineDimension'),
        LE: [PDFName.of('ClosedArrow'), PDFName.of('ClosedArrow')],
        LL: PDFNumber.of(10),
        LLE: PDFNumber.of(2),
        Cap: PDFBool.True,
        MeasurementTypes: PDFNumber.of(130),
        Measure: createMeasurementScaleDictionary(pdfDoc, pageScale, markup.displayUnit),
        Contents: PDFString.of(label),
        RC: PDFString.of(createMeasurementRichContent(label, textAppearance)),
        Label: PDFString.of(''),
        DA: PDFString.of(pdfTextDefaultAppearance(textAppearance)),
        DS: PDFString.of(pdfTextDefaultStyle(textAppearance, 'center')),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Length Measurement'),
        F: PDFNumber.of(4),
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'polylength': {
      const pageScale = pageScales.find((scale) => scale.pageIndex === markup.pageIndex);
      const label = measurementLabel(markup, pageScale);
      const appearance = createPathMeasurementAppearance(pdfDoc, markup, false, label, fonts);
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('PolyLine'),
        Rect: rectToPdfArray(measurementPathBounds(markup, false, label, fonts)),
        Vertices: markup.points.flatMap((point) => [point.x, point.y]),
        Border: [0, 0, stroke?.widthPt ?? 0],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(stroke?.widthPt ?? 0),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        IT: PDFName.of('PolyLineDimension'),
        Cap: PDFBool.True,
        AlignOnSegment: PDFBool.True,
        MeasurementTypes: PDFNumber.of(130),
        Measure: createMeasurementScaleDictionary(pdfDoc, pageScale, markup.displayUnit),
        Contents: PDFString.of(label),
        RC: PDFString.of(createMeasurementRichContent(label, textAppearance)),
        Label: PDFString.of(''),
        DA: PDFString.of(pdfTextDefaultAppearance(textAppearance)),
        DS: PDFString.of(pdfTextDefaultStyle(textAppearance, 'center')),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Polylength Measurement'),
        F: PDFNumber.of(4),
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'area': {
      const pageScale = pageScales.find((scale) => scale.pageIndex === markup.pageIndex);
      const label = measurementLabel(markup, pageScale);
      const appearance = createPathMeasurementAppearance(pdfDoc, markup, true, label, fonts);
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Polygon'),
        Rect: rectToPdfArray(measurementPathBounds(markup, true, label, fonts)),
        Vertices: markup.points.flatMap((point) => [point.x, point.y]),
        Border: [0, 0, stroke?.widthPt ?? 0],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(stroke?.widthPt ?? 0),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        ...(fill?.color ? { IC: pdfColorArray(fill.color) } : {}),
        IT: PDFName.of('PolygonDimension'),
        Cap: PDFBool.True,
        AlignOnSegment: PDFBool.True,
        MeasurementTypes: PDFNumber.of(129),
        Measure: createMeasurementScaleDictionary(pdfDoc, pageScale, markup.displayUnit),
        Contents: PDFString.of(label),
        RC: PDFString.of(createMeasurementRichContent(label, textAppearance)),
        Label: PDFString.of(''),
        DA: PDFString.of(pdfTextDefaultAppearance(textAppearance)),
        DS: PDFString.of(pdfTextDefaultStyle(textAppearance, 'center')),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Area Measurement'),
        F: PDFNumber.of(4),
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'polyline': {
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('PolyLine'),
        Rect: paddedPointBounds(markup.points, Math.max(4, (stroke?.widthPt ?? 0) * 0.5)),
        Vertices: markup.points.flatMap((point) => [point.x, point.y]),
        Border: [0, 0, stroke?.widthPt ?? 0],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(stroke?.widthPt ?? 0),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('PolyLine'),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'polygon': {
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Polygon'),
        Rect: paddedPointBounds(markup.points, Math.max(4, (stroke?.widthPt ?? 0) * 0.5)),
        Vertices: markup.points.flatMap((point) => [point.x, point.y]),
        Border: [0, 0, stroke?.widthPt ?? 0],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(stroke?.widthPt ?? 0),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        ...(fill?.color ? { IC: pdfColorArray(fill.color) } : {}),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Polygon'),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'cloud': {
      const intensity = markup.borderEffectIntensity ?? 2;
      const strokeWidth = stroke?.widthPt ?? 0;
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Polygon'),
        Rect: paddedPointBounds(markup.controlPath, cloudPadding(markup.scallopRadius)),
        Vertices: markup.controlPath.flatMap((point) => [point.x, point.y]),
        Border: [0, 0, strokeWidth],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(strokeWidth),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        ...(fill?.color ? { IC: pdfColorArray(fill.color) } : {}),
        ...opacityFields,
        BE: pdfDoc.context.obj({
          S: PDFName.of('C'),
          I: PDFNumber.of(intensity),
        }),
        IT: PDFName.of('PolygonCloud'),
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Cloud'),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'cloud-plus': {
      const intensity = markup.cloud.borderEffectIntensity ?? 2;
      const strokeWidth = stroke?.widthPt ?? 0;
      const groupName = toManagedAnnotationId(markup.id);
      const cloudAnnot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Polygon'),
        Rect: paddedPointBounds(markup.cloud.controlPath, cloudPadding(markup.cloud.scallopRadius)),
        Vertices: markup.cloud.controlPath.flatMap((point) => [point.x, point.y]),
        Border: [0, 0, strokeWidth],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(strokeWidth),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        ...(fill?.color ? { IC: pdfColorArray(fill.color) } : {}),
        ...opacityFields,
        BE: pdfDoc.context.obj({
          S: PDFName.of('C'),
          I: PDFNumber.of(intensity),
        }),
        IT: PDFName.of('PolygonCloud'),
        ITEx: PDFName.of('PolyText'),
        GroupNesting: [PDFString.of('Cloud+'), PDFString.of(groupName)],
        NM: PDFString.of(`${groupName}:cloud`),
        Subj: PDFString.of('Cloud+'),
        Contents: PDFString.of(markup.text),
        F: PDFNumber.of(4),
      });
      appendAnnotation(page, pdfDoc, cloudAnnot, resolvedAppearance);

      const flattenedPoints = markup.leader.points.flatMap((point) => [point.x, point.y]);
      const appearance = createCalloutAppearance(pdfDoc, {
        kind: 'callout',
        id: markup.id,
        pageIndex: markup.pageIndex,
        leader: markup.leader,
        textBox: markup.textBox,
        text: markup.text,
        color: markup.color,
        opacity: markup.opacity,
        appearance: markup.appearance,
        source: markup.source,
      }, fonts);
      const textAnnot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('FreeText'),
        IT: PDFName.of('FreeTextCallout'),
        ITEx: PDFName.of('PolyText'),
        Rect: [markup.textBox.x, markup.textBox.y, markup.textBox.x + markup.textBox.width, markup.textBox.y + markup.textBox.height],
        Contents: PDFString.of(markup.text),
        CL: flattenedPoints,
        LE: [PDFName.of('None'), PDFName.of('OpenArrow')],
        Q: PDFNumber.of(0),
        DA: PDFString.of(pdfTextDefaultAppearance(textAppearance)),
        DS: PDFString.of(pdfTextDefaultStyle(textAppearance)),
        Border: [0, 0, 0],
        BS: pdfDoc.context.obj({ W: PDFNumber.of(0), S: PDFName.of('S'), Type: PDFName.of('Border') }),
        NM: PDFString.of(`${groupName}:text`),
        Subj: PDFString.of('Cloud+'),
        F: PDFNumber.of(4),
        C: pdfColorArray(stroke?.color),
        ...opacityFields,
        P: page.ref,
        GroupNesting: [PDFString.of('Cloud+'), PDFString.of(groupName)],
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      appendAnnotation(page, pdfDoc, textAnnot, resolvedAppearance);
      return;
    }
    case 'pen':
    case 'highlight': {
      const isHighlight = markup.kind === 'highlight';
      const strokeWidth = stroke?.widthPt ?? 0;
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Ink'),
        Rect: paddedPointBounds(markup.paths.flat(), Math.max(1, strokeWidth * 0.5)),
        InkList: markup.paths.map((path) => path.flatMap((point) => [point.x, point.y])),
        Border: [0, 0, strokeWidth],
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(strokeWidth),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        C: pdfColorArray(stroke?.color),
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of(isHighlight ? 'Highlight' : 'Pen'),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
        ...(resolvedAppearance.blendMode === 'multiply' ? { BM: PDFName.of('Multiply') } : {}),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'text-box': {
      const textBoxFont = getTextBoxFont(markup, fonts, {});
      const appearance = createTextBoxAppearance(pdfDoc, markup, fonts);
      const annotationRect = getTextBoxAnnotationRect(markup);
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('FreeText'),
        Rect: [annotationRect.x, annotationRect.y, annotationRect.x + annotationRect.width, annotationRect.y + annotationRect.height],
        Contents: createPdfTextString(markup.text),
        Q: PDFNumber.of(textAlignToPdfQ(textAppearance?.align)),
        DA: PDFString.of(`${rgbToPdfOperator(textAppearance?.color ?? '#ff0000')} /${textBoxFont.resourceName} ${formatPdfNumber(getTextBoxFontSize(markup))} Tf`),
        DS: PDFString.of(createTextBoxDefaultStyle(markup, textBoxFont)),
        RC: createPdfTextString(createTextBoxRichContent(markup.text, markup, textBoxFont)),
        BS: pdfDoc.context.obj({
          W: PDFNumber.of(stroke?.widthPt ?? 0),
          S: PDFName.of('S'),
          Type: PDFName.of('Border'),
        }),
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Text Box'),
        F: PDFNumber.of(4),
        C: pdfColorArray(stroke?.color),
        ...opacityFields,
        P: page.ref,
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      if (markup.rotation) {
        annot.set(PDFName.of('Rotation'), PDFNumber.of(normalizeFreeRotation(markup.rotation)));
      }
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'callout': {
      const flattenedPoints = markup.leader.points.reduce<number[]>((points, point) => {
        points.push(point.x, point.y);
        return points;
      }, []);
      const appearance = createCalloutAppearance(pdfDoc, markup, fonts);
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('FreeText'),
        IT: PDFName.of('FreeTextCallout'),
        Rect: [markup.textBox.x, markup.textBox.y, markup.textBox.x + markup.textBox.width, markup.textBox.y + markup.textBox.height],
        Contents: PDFString.of(markup.text),
        CL: flattenedPoints,
        LE: [PDFName.of('None'), PDFName.of('OpenArrow')],
        Q: PDFNumber.of(textAlignToPdfQ(textAppearance?.align)),
        DA: PDFString.of(pdfTextDefaultAppearance(textAppearance)),
        DS: PDFString.of(pdfTextDefaultStyle(textAppearance)),
        Border: [0, 0, 0],
        BS: pdfDoc.context.obj({ W: PDFNumber.of(0), S: PDFName.of('S'), Type: PDFName.of('Border') }),
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Callout'),
        F: PDFNumber.of(4),
        C: pdfColorArray(stroke?.color),
        ...opacityFields,
        P: page.ref,
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'image': {
      const embeddedImage = await embedMarkupImage(pdfDoc, markup);
      const appearance = createImageAppearance(pdfDoc, markup, embeddedImage);
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Square'),
        Rect: rectToPdfArray(imageAnnotationRect(markup)),
        BS: pdfDoc.context.obj({ W: PDFNumber.of(0), S: PDFName.of('S'), Type: PDFName.of('Border') }),
        Border: [0, 0, 0],
        RD: [0, 0, 0, 0],
        C: [],
        ...opacityFields,
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Image'),
        IT: PDFName.of('SquareImage'),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
        BPImageData: PDFString.of(markup.dataUrl),
        BPImageMimeType: PDFString.of(markup.mimeType),
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      if (markup.rotation) {
        annot.set(PDFName.of('Rotation'), PDFNumber.of(normalizeFreeRotation(markup.rotation)));
      }
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'snapshot': {
      const embeddedImage = await embedMarkupImage(pdfDoc, markup);
      const appearance = createImageAppearance(pdfDoc, markup, embeddedImage);
      const annot = pdfDoc.context.obj({
        Type: PDFName.of('Annot'),
        Subtype: PDFName.of('Stamp'),
        Rect: rectToPdfArray(imageAnnotationRect(markup)),
        NM: PDFString.of(toManagedAnnotationId(markup.id)),
        Subj: PDFString.of('Snapshot'),
        IT: PDFName.of('StampSnapshot'),
        Contents: PDFString.of(''),
        F: PDFNumber.of(4),
        ...opacityFields,
        BPSnapshotData: PDFString.of(markup.dataUrl),
        BPSnapshotMimeType: PDFString.of(markup.mimeType),
        AP: pdfDoc.context.obj({
          N: appearance.ref,
        }),
      });
      if (markup.rotation) {
        annot.set(PDFName.of('Rotation'), PDFNumber.of(normalizeFreeRotation(markup.rotation)));
      }
      appendAnnotation(page, pdfDoc, annot, resolvedAppearance);
      return;
    }
    case 'imported-annotation':
      return;
  }
}

function lineAnnotationRect(start: PdfPoint, end: PdfPoint): readonly number[] {
  const padding = 4;
  return [
    Math.min(start.x, end.x) - padding,
    Math.min(start.y, end.y) - padding,
    Math.max(start.x, end.x) + padding,
    Math.max(start.y, end.y) + padding,
  ];
}

function imageAnnotationRect(markup: Extract<Markup, { kind: 'image' | 'snapshot' }>): { x: number; y: number; width: number; height: number } {
  if (!markup.rotation) {
    return markup.rect;
  }
  return boundsForRotatedRect(markup.rect, normalizeFreeRotation(markup.rotation));
}

function dimensionAnnotationRect(markup: Extract<Markup, { kind: 'dimension' }>, fonts: PdfExportFonts): readonly number[] {
  const bounds = dimensionBounds(markup, fonts);
  return [bounds.x, bounds.y, bounds.x + bounds.width, bounds.y + bounds.height];
}

function createDimensionAppearance(pdfDoc: PDFDocument, markup: Extract<Markup, { kind: 'dimension' }>, fonts: PdfExportFonts): { ref: PDFRef } {
  const geometry = dimensionGeometry(markup, fonts);
  const leftArrow = closedArrowHeadPoints(geometry.dimensionStart, geometry.dimensionEnd, 9, 7);
  const rightArrow = closedArrowHeadPoints(geometry.dimensionEnd, geometry.dimensionStart, 9, 7);
  const caption = dimensionCaptionRect(markup, fonts, geometry.captionCenter);
  const bounds = dimensionBounds(markup, fonts);
  const appearance = resolveMarkupAppearance(markup);
  const strokeAppearance = appearance.stroke!;
  const textAppearance = appearance.text!;
  const stroke = colorToRgb(strokeAppearance.color);
  const text = colorToRgb(textAppearance.color);
  const baselineOffset = textAppearance.fontSizePt * (13 / 12);
  const commands = [
    'q /GS0 gs 1 0 0 1 0 0 cm',
    `${formatPdfNumber(stroke.red)} ${formatPdfNumber(stroke.green)} ${formatPdfNumber(stroke.blue)} RG ${formatPdfNumber(stroke.red)} ${formatPdfNumber(stroke.green)} ${formatPdfNumber(stroke.blue)} rg ${formatPdfNumber(strokeAppearance.widthPt)} w`,
    ...geometry.dimensionLineSegments.map((segment) => `${formatPdfNumber(segment.start.x)} ${formatPdfNumber(segment.start.y)} m ${formatPdfNumber(segment.end.x)} ${formatPdfNumber(segment.end.y)} l S`),
    `${formatPdfNumber(geometry.extensionStartInner.x)} ${formatPdfNumber(geometry.extensionStartInner.y)} m ${formatPdfNumber(geometry.extensionStartOuter.x)} ${formatPdfNumber(geometry.extensionStartOuter.y)} l S`,
    `${formatPdfNumber(geometry.extensionEndInner.x)} ${formatPdfNumber(geometry.extensionEndInner.y)} m ${formatPdfNumber(geometry.extensionEndOuter.x)} ${formatPdfNumber(geometry.extensionEndOuter.y)} l S`,
    polygonPath(leftArrow),
    'f',
    polygonPath(rightArrow),
    'f',
    `BT ${formatPdfNumber(text.red)} ${formatPdfNumber(text.green)} ${formatPdfNumber(text.blue)} rg /Helv ${formatPdfNumber(textAppearance.fontSizePt)} Tf 1 0 0 1 ${formatPdfNumber(caption.x + textAppearance.insetPt)} ${formatPdfNumber(caption.y + caption.height - baselineOffset)} Tm (${escapePdfLiteralString(markup.text)}) Tj ET`,
    'Q',
  ];
  const stream = pdfDoc.context.flateStream(`${commands.join(' ')} `, {
    Type: PDFName.of('XObject'),
    Subtype: PDFName.of('Form'),
    FormType: PDFNumber.of(1),
    BBox: [bounds.x, bounds.y, bounds.x + bounds.width, bounds.y + bounds.height],
    Resources: pdfDoc.context.obj({
      ProcSet: [PDFName.of('PDF'), PDFName.of('Text')],
      Font: {
        Helv: {
          Type: PDFName.of('Font'),
          Subtype: PDFName.of('Type1'),
          BaseFont: PDFName.of('Helvetica'),
          Encoding: PDFName.of('WinAnsiEncoding'),
        },
      },
      ExtGState: {
        GS0: pdfDoc.context.obj({
          Type: PDFName.of('ExtGState'),
          CA: PDFNumber.of(appearance.opacity * stroke.alpha),
          ca: PDFNumber.of(appearance.opacity * text.alpha),
        }),
      },
    } as any),
  });
  return { ref: pdfDoc.context.register(stream) as PDFRef };
}

function dimensionGeometry(markup: Extract<Markup, { kind: 'dimension' }>, fonts: PdfExportFonts) {
  const basis = dimensionBasis(markup.start, markup.end);
  const sign = markup.dimensionLineOffset >= 0 ? 1 : -1;
  const offset = scalePdfPoint(basis.normal, markup.dimensionLineOffset);
  const dimensionStart = addPdfPoint(markup.start, offset);
  const dimensionEnd = addPdfPoint(markup.end, offset);
  const captionCenter = midpointPdfPoint(dimensionStart, dimensionEnd);
  const caption = dimensionCaptionRect(markup, fonts, captionCenter);
  return {
    dimensionStart,
    dimensionEnd,
    dimensionLineSegments: splitDimensionLineAroundCaption(dimensionStart, dimensionEnd, captionCenter, basis, caption.width),
    extensionStartInner: addPdfPoint(markup.start, scalePdfPoint(basis.normal, sign * 2)),
    extensionEndInner: addPdfPoint(markup.end, scalePdfPoint(basis.normal, sign * 2)),
    extensionStartOuter: addPdfPoint(dimensionStart, scalePdfPoint(basis.normal, sign * 4)),
    extensionEndOuter: addPdfPoint(dimensionEnd, scalePdfPoint(basis.normal, sign * 4)),
    captionCenter,
  };
}

function dimensionCaptionRect(markup: Extract<Markup, { kind: 'dimension' }>, fonts: PdfExportFonts, center: PdfPoint): { x: number; y: number; width: number; height: number } {
  const text = resolveMarkupAppearance(markup).text!;
  const measuredWidth = fonts.helvetica.widthOfTextAtSize(markup.text, text.fontSizePt);
  const width = Math.max(text.fontSizePt, measuredWidth + text.insetPt * 2);
  const height = Math.max(text.lineHeightPt, text.fontSizePt);
  return { x: center.x - width * 0.5, y: center.y - height * 0.5, width, height };
}

function dimensionBounds(markup: Extract<Markup, { kind: 'dimension' }>, fonts: PdfExportFonts): { x: number; y: number; width: number; height: number } {
  const geometry = dimensionGeometry(markup, fonts);
  const caption = dimensionCaptionRect(markup, fonts, geometry.captionCenter);
  const xs = [
    markup.start.x,
    markup.end.x,
    geometry.dimensionStart.x,
    geometry.dimensionEnd.x,
    geometry.extensionStartOuter.x,
    geometry.extensionEndOuter.x,
    caption.x,
    caption.x + caption.width,
  ];
  const ys = [
    markup.start.y,
    markup.end.y,
    geometry.dimensionStart.y,
    geometry.dimensionEnd.y,
    geometry.extensionStartOuter.y,
    geometry.extensionEndOuter.y,
    caption.y,
    caption.y + caption.height,
  ];
  const minX = Math.min(...xs);
  const minY = Math.min(...ys);
  const maxX = Math.max(...xs);
  const maxY = Math.max(...ys);
  return { x: minX, y: minY, width: Math.max(1, maxX - minX), height: Math.max(1, maxY - minY) };
}

function dimensionBasis(start: PdfPoint, end: PdfPoint): { unit: PdfPoint; normal: PdfPoint; length: number } {
  const delta = pdfPoint(end.x - start.x, end.y - start.y);
  const length = Math.hypot(delta.x, delta.y);
  if (length === 0) {
    return {
      unit: pdfPoint(1, 0),
      normal: pdfPoint(0, 1),
      length: 0,
    };
  }
  const unit = pdfPoint(delta.x / length, delta.y / length);
  return {
    unit,
    normal: pdfPoint(-unit.y, unit.x),
    length,
  };
}

function splitDimensionLineAroundCaption(start: PdfPoint, end: PdfPoint, center: PdfPoint, basis: { unit: PdfPoint; length: number }, captionWidth: number): readonly { start: PdfPoint; end: PdfPoint }[] {
  const halfGap = Math.min(captionWidth * 0.5 + 4, Math.max(0, (basis.length * 0.5) - 1));
  if (halfGap <= 0) {
    return [{ start, end }];
  }
  return [
    { start, end: addPdfPoint(center, scalePdfPoint(basis.unit, -halfGap)) },
    { start: addPdfPoint(center, scalePdfPoint(basis.unit, halfGap)), end },
  ];
}

function midpointPdfPoint(start: PdfPoint, end: PdfPoint): PdfPoint {
  return pdfPoint((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
}

function addPdfPoint(point: PdfPoint, offset: PdfPoint): PdfPoint {
  return pdfPoint(point.x + offset.x, point.y + offset.y);
}

function scalePdfPoint(point: PdfPoint, scale: number): PdfPoint {
  return pdfPoint(point.x * scale, point.y * scale);
}

function closedArrowHeadPoints(tip: PdfPoint, tail: PdfPoint, length: number, width: number): readonly PdfPoint[] {
  const dx = tail.x - tip.x;
  const dy = tail.y - tip.y;
  const distance = Math.hypot(dx, dy);
  if (distance === 0) {
    return [tip, tip, tip];
  }
  const ux = dx / distance;
  const uy = dy / distance;
  const base = pdfPoint(tip.x + ux * length, tip.y + uy * length);
  const perpendicular = { x: -uy, y: ux };
  return [
    tip,
    pdfPoint(base.x + perpendicular.x * width * 0.5, base.y + perpendicular.y * width * 0.5),
    pdfPoint(base.x - perpendicular.x * width * 0.5, base.y - perpendicular.y * width * 0.5),
  ];
}

function polygonPath(points: readonly PdfPoint[]): string {
  const [first = pdfPoint(0, 0), ...rest] = points;
  return [
    `${formatPdfNumber(first.x)} ${formatPdfNumber(first.y)} m`,
    ...rest.map((point) => `${formatPdfNumber(point.x)} ${formatPdfNumber(point.y)} l`),
    'h',
  ].join(' ');
}

function measurementLabel(markup: Extract<Markup, { kind: 'length' | 'polylength' | 'area' }>, pageScale: PageScale | undefined): string {
  if (!pageScale) {
    return 'Scale not set';
  }
  if (markup.kind === 'length') {
    return formatScaledLengthLabel(measureScaledLength(markup.start, markup.end, pageScale), pageScale, markup.displayUnit);
  }
  if (markup.kind === 'polylength') {
    return formatScaledLengthLabel(measureScaledPolyline(markup.points, pageScale), pageScale, markup.displayUnit);
  }
  return formatScaledAreaLabel(measureScaledPolygonArea(markup.points, pageScale), pageScale, markup.displayUnit);
}

function createLengthAppearance(pdfDoc: PDFDocument, markup: Extract<Markup, { kind: 'length' }>, label: string, fonts: PdfExportFonts): { ref: PDFRef } {
  return createPathMeasurementAppearance(pdfDoc, markup, false, label, fonts);
}

function createPathMeasurementAppearance(
  pdfDoc: PDFDocument,
  markup: Extract<Markup, { kind: 'length' | 'polylength' | 'area' }>,
  closed: boolean,
  label: string,
  fonts: PdfExportFonts,
): { ref: PDFRef } {
  const points = markup.kind === 'length' ? [markup.start, markup.end] : markup.points;
  const text = resolveMarkupAppearance(markup).text;
  const labelBox = measurementLabelRect(closed ? polygonCentroid(points) : pathMidpoint(points), label, text, fonts);
  const bounds = measurementPathBounds(markup, closed, label, fonts);
  const appearance = resolveMarkupAppearance(markup);
  const stroke = appearance.stroke;
  const fill = appearance.fill;
  const strokeRgb = colorToRgb(stroke?.color ?? '#ff0000');
  const fillRgb = colorToRgb(fill?.color ?? stroke?.color ?? '#ff0000');
  const textRgb = colorToRgb(text?.color ?? '#ff0000');
  const opacity = appearance.opacity;
  const fontSize = text?.fontSizePt ?? 12;
  const baselineOffset = fontSize * (13 / 12);
  const pathCommands = points.length === 0
    ? []
    : [
        `${formatPdfNumber(points[0].x)} ${formatPdfNumber(points[0].y)} m`,
        ...points.slice(1).map((point) => `${formatPdfNumber(point.x)} ${formatPdfNumber(point.y)} l`),
        ...(closed ? ['h'] : []),
      ];
  const commands = [
    'q /GSPath gs',
    `${formatPdfNumber(strokeRgb.red)} ${formatPdfNumber(strokeRgb.green)} ${formatPdfNumber(strokeRgb.blue)} RG ${formatPdfNumber(fillRgb.red)} ${formatPdfNumber(fillRgb.green)} ${formatPdfNumber(fillRgb.blue)} rg ${formatPdfNumber(stroke?.widthPt ?? 0)} w`,
    ...pathCommands,
    closed ? 'B' : 'S',
    'Q q /GSText gs',
    `BT ${rgbToPdfOperator(text?.color ?? '#ff0000')} /Helv ${formatPdfNumber(fontSize)} Tf 1 0 0 1 ${formatPdfNumber(labelBox.x + (text?.insetPt ?? 0))} ${formatPdfNumber(labelBox.y + labelBox.height - baselineOffset)} Tm (${escapePdfLiteralString(label)}) Tj ET`,
    'Q',
  ];
  const stream = pdfDoc.context.flateStream(`${commands.join(' ')} `, {
    Type: PDFName.of('XObject'),
    Subtype: PDFName.of('Form'),
    FormType: PDFNumber.of(1),
    BBox: rectToPdfArray(bounds),
    Resources: pdfDoc.context.obj({
      ProcSet: [PDFName.of('PDF'), PDFName.of('Text')],
      Font: {
        Helv: {
          Type: PDFName.of('Font'),
          Subtype: PDFName.of('Type1'),
          BaseFont: PDFName.of('Helvetica'),
          Encoding: PDFName.of('WinAnsiEncoding'),
        },
      },
      ExtGState: {
        GSPath: pdfDoc.context.obj({
          Type: PDFName.of('ExtGState'),
          CA: PDFNumber.of(opacity * strokeRgb.alpha),
          ca: PDFNumber.of(opacity * fillRgb.alpha),
        }),
        GSText: pdfDoc.context.obj({
          Type: PDFName.of('ExtGState'),
          CA: PDFNumber.of(opacity * textRgb.alpha),
          ca: PDFNumber.of(opacity * textRgb.alpha),
        }),
      },
    } as any),
  });
  return { ref: pdfDoc.context.register(stream) as PDFRef };
}

function createMeasurementScaleDictionary(pdfDoc: PDFDocument, pageScale: PageScale | undefined, displayUnit = pageScale?.realUnits) {
  const unit = displayUnit ?? 'm';
  const scale = pageScale ? convertScaledValueUnit(pageScale.scaleX, pageScale.realUnits, unit) : 1;
  const areaUnit = unit === 'm' ? 'sq m' : `${unit}^2`;
  const volumeUnit = unit === 'm' ? 'cu m' : `${unit}^3`;
  return pdfDoc.context.obj({
    Type: PDFName.of('Measure'),
    Subtype: PDFName.of('RL'),
    R: PDFString.of(pageScale?.name ?? 'Scale not set'),
    X: [measurementNumberFormat(pdfDoc, unit, scale)],
    D: [measurementNumberFormat(pdfDoc, unit, 1)],
    A: [measurementNumberFormat(pdfDoc, areaUnit, 1, true)],
    T: [measurementNumberFormat(pdfDoc, '°', 1, true)],
    V: [measurementNumberFormat(pdfDoc, volumeUnit, 1, true)],
    TargetUnitConversion: PDFNumber.of(scale),
  });
}

function measurementNumberFormat(pdfDoc: PDFDocument, unit: string, conversion: number, forceDecimal = false) {
  return pdfDoc.context.obj({
    Type: PDFName.of('NumberFormat'),
    U: PDFString.of(unit),
    C: PDFNumber.of(conversion),
    D: PDFNumber.of(100),
    ...(forceDecimal ? { FD: PDFBool.True } : {}),
    SS: PDFString.of(''),
  });
}

function createMeasurementRichContent(label: string, text: ResolvedMarkupAppearance['text']): string {
  return `<?xml version="1.0"?><body xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/" xfa:contentType="text/html" xfa:APIVersion="BluebeamPDFRevu:2018" xfa:spec="2.2.0" style="${pdfTextDefaultStyle(text, 'center')}" xmlns="http://www.w3.org/1999/xhtml"><p>${escapeXml(label)}</p></body>`;
}

function measurementLengthBounds(markup: Extract<Markup, { kind: 'length' }>, label: string, fonts: PdfExportFonts): { x: number; y: number; width: number; height: number } {
  const appearance = resolveMarkupAppearance(markup);
  return unionBounds(
    pointsBounds([markup.start, markup.end], Math.max(8, (appearance.stroke?.widthPt ?? 0) * 0.5)),
    measurementLabelRect(midpointPdfPoint(markup.start, markup.end), label, appearance.text, fonts),
  );
}

function measurementPathBounds(
  markup: Extract<Markup, { kind: 'length' | 'polylength' | 'area' }>,
  closed: boolean,
  label: string,
  fonts: PdfExportFonts,
): { x: number; y: number; width: number; height: number } {
  const points = markup.kind === 'length' ? [markup.start, markup.end] : markup.points;
  const appearance = resolveMarkupAppearance(markup);
  return unionBounds(
    pointsBounds(points, Math.max(8, (appearance.stroke?.widthPt ?? 0) * 0.5)),
    measurementLabelRect(closed ? polygonCentroid(points) : pathMidpoint(points), label, appearance.text, fonts),
  );
}

function measurementLabelRect(
  anchor: PdfPoint,
  label: string,
  text: ResolvedMarkupAppearance['text'],
  fonts: PdfExportFonts,
): { x: number; y: number; width: number; height: number } {
  const fontSize = text?.fontSizePt ?? 12;
  const lineHeight = text?.lineHeightPt ?? fontSize * 1.15;
  const inset = text?.insetPt ?? 0;
  const width = fonts.helvetica.widthOfTextAtSize(label, fontSize);
  return {
    x: anchor.x + 6,
    y: anchor.y + 6,
    width: Math.max(fontSize * (56 / 12), width + inset * 2),
    height: Math.max(lineHeight, fontSize * 1.5),
  };
}

function pathMidpoint(points: readonly PdfPoint[]): PdfPoint {
  if (points.length === 0) {
    return pdfPoint(0, 0);
  }
  if (points.length === 1) {
    return points[0];
  }
  const total = pathLength(points);
  let travelled = 0;
  for (let index = 1; index < points.length; index += 1) {
    const start = points[index - 1];
    const end = points[index];
    const segmentLength = Math.hypot(end.x - start.x, end.y - start.y);
    if (travelled + segmentLength >= total * 0.5 && segmentLength > 0) {
      const t = ((total * 0.5) - travelled) / segmentLength;
      return pdfPoint(start.x + (end.x - start.x) * t, start.y + (end.y - start.y) * t);
    }
    travelled += segmentLength;
  }
  return points[points.length - 1];
}

function polygonCentroid(points: readonly PdfPoint[]): PdfPoint {
  if (points.length === 0) {
    return pdfPoint(0, 0);
  }
  const total = points.reduce((sum, point) => pdfPoint(sum.x + point.x, sum.y + point.y), pdfPoint(0, 0));
  return pdfPoint(total.x / points.length, total.y / points.length);
}

function pathLength(points: readonly PdfPoint[]): number {
  let total = 0;
  for (let index = 1; index < points.length; index += 1) {
    total += Math.hypot(points[index].x - points[index - 1].x, points[index].y - points[index - 1].y);
  }
  return total;
}

function pointsBounds(points: readonly PdfPoint[], padding = 0): { x: number; y: number; width: number; height: number } {
  if (points.length === 0) {
    return { x: 0, y: 0, width: 1, height: 1 };
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const minX = Math.min(...xs) - padding;
  const minY = Math.min(...ys) - padding;
  const maxX = Math.max(...xs) + padding;
  const maxY = Math.max(...ys) + padding;
  return { x: minX, y: minY, width: Math.max(1, maxX - minX), height: Math.max(1, maxY - minY) };
}

function unionBounds(...bounds: readonly { x: number; y: number; width: number; height: number }[]): { x: number; y: number; width: number; height: number } {
  const minX = Math.min(...bounds.map((box) => box.x));
  const minY = Math.min(...bounds.map((box) => box.y));
  const maxX = Math.max(...bounds.map((box) => box.x + box.width));
  const maxY = Math.max(...bounds.map((box) => box.y + box.height));
  return { x: minX, y: minY, width: Math.max(1, maxX - minX), height: Math.max(1, maxY - minY) };
}

function rectToPdfArray(box: { x: number; y: number; width: number; height: number }): readonly number[] {
  return [box.x, box.y, box.x + box.width, box.y + box.height];
}

function createArcAppearance(pdfDoc: PDFDocument, markup: Extract<Markup, { kind: 'arc' }>): { ref: PDFRef } {
  const appearance = resolveMarkupAppearance(markup);
  const strokeAppearance = appearance.stroke!;
  const stroke = colorToRgb(strokeAppearance.color);
  const path = arcPdfPathCommands(insetRect(markup.rect, strokeAppearance.widthPt * 0.5), markup.angle1, markup.angle2);
  const commands = [
    'q /GS0 gs',
    `${formatPdfNumber(stroke.red)} ${formatPdfNumber(stroke.green)} ${formatPdfNumber(stroke.blue)} RG`,
    `${formatPdfNumber(strokeAppearance.widthPt)} w`,
    ...path,
    'S',
    'Q',
  ];
  const stream = pdfDoc.context.flateStream(`${commands.join(' ')} `, {
    Type: PDFName.of('XObject'),
    Subtype: PDFName.of('Form'),
    FormType: PDFNumber.of(1),
    BBox: [markup.rect.x, markup.rect.y, markup.rect.x + markup.rect.width, markup.rect.y + markup.rect.height],
    Resources: pdfDoc.context.obj({
      ExtGState: {
        GS0: pdfDoc.context.obj({
          Type: PDFName.of('ExtGState'),
          CA: PDFNumber.of(appearance.opacity * stroke.alpha),
          ca: PDFNumber.of(appearance.opacity * stroke.alpha),
        }),
      },
    } as any),
  });
  return { ref: pdfDoc.context.register(stream) as PDFRef };
}

function insetRect(rect: { readonly x: number; readonly y: number; readonly width: number; readonly height: number }, inset: number) {
  return {
    x: rect.x + inset,
    y: rect.y + inset,
    width: Math.max(0, rect.width - inset * 2),
    height: Math.max(0, rect.height - inset * 2),
  };
}

function arcPdfPathCommands(rect: { readonly x: number; readonly y: number; readonly width: number; readonly height: number }, angle1: number, angle2: number): string[] {
  const segments = arcBezierSegments(rect, angle1, angle2);
  if (segments.length === 0) {
    return [];
  }
  const commands = [
    `${formatPdfNumber(segments[0].start.x)} ${formatPdfNumber(segments[0].start.y)} m`,
  ];
  for (const segment of segments) {
    commands.push(`${formatPdfNumber(segment.control1.x)} ${formatPdfNumber(segment.control1.y)} ${formatPdfNumber(segment.control2.x)} ${formatPdfNumber(segment.control2.y)} ${formatPdfNumber(segment.end.x)} ${formatPdfNumber(segment.end.y)} c`);
  }
  return commands;
}

function arcBezierSegments(rect: { readonly x: number; readonly y: number; readonly width: number; readonly height: number }, angle1: number, angle2: number) {
  const delta = normalizeArcDelta(angle1, angle2);
  const segmentCount = Math.max(1, Math.ceil(Math.abs(delta) / 22.5));
  const segmentDelta = delta / segmentCount;
  return Array.from({ length: segmentCount }, (_, index) => {
    const startAngle = angle1 + segmentDelta * index;
    const endAngle = startAngle + segmentDelta;
    return arcBezierSegment(rect, startAngle, endAngle);
  });
}

function arcBezierSegment(rect: { readonly x: number; readonly y: number; readonly width: number; readonly height: number }, startAngle: number, endAngle: number) {
  const rx = rect.width * 0.5;
  const ry = rect.height * 0.5;
  const cx = rect.x + rx;
  const cy = rect.y + ry;
  const start = (startAngle * Math.PI) / 180;
  const end = (endAngle * Math.PI) / 180;
  const alpha = (4 / 3) * Math.tan((end - start) / 4);
  const startPoint = { x: cx + rx * Math.cos(start), y: cy + ry * Math.sin(start) };
  const endPoint = { x: cx + rx * Math.cos(end), y: cy + ry * Math.sin(end) };
  return {
    start: startPoint,
    control1: { x: startPoint.x - alpha * rx * Math.sin(start), y: startPoint.y + alpha * ry * Math.cos(start) },
    control2: { x: endPoint.x + alpha * rx * Math.sin(end), y: endPoint.y - alpha * ry * Math.cos(end) },
    end: endPoint,
  };
}

function normalizeArcDelta(angle1: number, angle2: number): number {
  let delta = angle2 - angle1;
  while (delta <= -360) delta += 360;
  while (delta > 360) delta -= 360;
  return delta;
}

function paddedPointBounds(points: readonly PdfPoint[], padding = 4): readonly number[] {
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  return [
    Math.min(...xs) - padding,
    Math.min(...ys) - padding,
    Math.max(...xs) + padding,
    Math.max(...ys) + padding,
  ];
}

function cloudPadding(scallopRadius = 14.28): number {
  return scallopRadius * 0.6831;
}

function createTextBoxAppearance(pdfDoc: PDFDocument, markup: Extract<Markup, { kind: 'text-box' }>, fonts: PdfExportFonts): { ref: PDFRef; lines: readonly string[] } {
  const defaultFont = getTextBoxFont(markup, fonts, {});
  const richLines = markup.richTextRuns
    ? layoutExplicitRichTextLines(markup.richTextRuns)
    : undefined;
  const lines = richLines?.map((line) => line.runs.map((run) => run.text).join(''))
    ?? markup.appearanceTextLines
    ?? splitPdfTextLines(markup.text);
  const x = markup.rect.x;
  const y = markup.rect.y;
  const width = markup.rect.width;
  const height = markup.rect.height;
  const fontSize = getTextBoxFontSize(markup);
  const lineHeight = getTextBoxLineHeight(markup);
  const resolvedAppearance = resolveMarkupAppearance(markup);
  const strokeAppearance = resolvedAppearance.stroke!;
  const fillAppearance = resolvedAppearance.fill;
  const textAppearance = resolvedAppearance.text!;
  const commands: string[] = [];
  const borderWidth = strokeAppearance.widthPt;

  if (fillAppearance?.color) {
    const fillColor = colorToRgb(fillAppearance.color);
    commands.push(
      'q /GSFill gs',
      `${formatPdfNumber(fillColor.red)} ${formatPdfNumber(fillColor.green)} ${formatPdfNumber(fillColor.blue)} rg`,
      `${formatPdfNumber(x)} ${formatPdfNumber(y)} ${formatPdfNumber(width)} ${formatPdfNumber(height)} re f Q`,
    );
  }

  if (borderWidth > 0) {
    const borderColor = colorToRgb(strokeAppearance.color);
    commands.push(
      `q /GSStroke gs ${formatPdfNumber(borderColor.red)} ${formatPdfNumber(borderColor.green)} ${formatPdfNumber(borderColor.blue)} RG ${formatPdfNumber(borderWidth)} w`,
      `${formatPdfNumber(x + borderWidth * 0.5)} ${formatPdfNumber(y + borderWidth * 0.5)} ${formatPdfNumber(Math.max(0, width - borderWidth))} ${formatPdfNumber(Math.max(0, height - borderWidth))} re S Q`,
    );
  }

  const textColor = colorToRgb(textAppearance.color);
  commands.push(
    'q /GSText gs',
    'BT',
    `${formatPdfNumber(textColor.red)} ${formatPdfNumber(textColor.green)} ${formatPdfNumber(textColor.blue)} rg /${defaultFont.resourceName} ${formatPdfNumber(fontSize)} Tf`,
  );

  const firstBaselineY = y + height - getTextBoxFirstBaselineOffset(markup);
  const appearanceLines: readonly TextBoxAppearanceLine[] = richLines ?? lines.map((line) => ({ runs: [{ text: line }] }));
  appearanceLines.forEach((line, index) => {
    const lineText = line.runs.map((run) => run.text).join('');
    let textX = getAlignedTextX(lineText, markup, fonts);
    const textY = firstBaselineY - index * lineHeight;
    for (const run of line.runs) {
      if (run.text.length === 0) {
        continue;
      }
      const runFont = getTextBoxFont(markup, fonts, run);
      const runFontSize = getTextBoxRunFontSize(markup, run);
      const runColor = colorToRgb(run.color ?? textAppearance.color);
      commands.push(
        `${formatPdfNumber(runColor.red)} ${formatPdfNumber(runColor.green)} ${formatPdfNumber(runColor.blue)} rg /${runFont.resourceName} ${formatPdfNumber(runFontSize)} Tf`,
        `1 0 0 1 ${formatPdfNumber(textX)} ${formatPdfNumber(textY)} Tm ${encodePdfTextShow(run.text, runFont)} Tj`,
      );
      textX += measureText(run.text, runFont.font, runFontSize);
    }
  });
  commands.push('ET Q');

  const stream = pdfDoc.context.flateStream(`${commands.join(' ')} `, {
    Type: PDFName.of('XObject'),
    Subtype: PDFName.of('Form'),
    FormType: PDFNumber.of(1),
    BBox: [x, y, x + width, y + height],
    Matrix: getTextBoxAppearanceMatrix(markup),
    Resources: pdfDoc.context.obj({
      ProcSet: [PDFName.of('PDF'), PDFName.of('Text')],
      Font: createTextBoxAppearanceFontResources(fonts, defaultFont),
      ExtGState: createTextBoxAppearanceOpacityResources(pdfDoc, resolvedAppearance),
    } as any),
  });

  return {
    ref: pdfDoc.context.register(stream) as PDFRef,
    lines,
  };
}

function createTextBoxAppearanceOpacityResources(pdfDoc: PDFDocument, appearance: ResolvedMarkupAppearance) {
  const opacity = appearance.opacity;
  const strokeAlpha = colorToRgb(appearance.stroke?.color ?? '#ff0000').alpha;
  const fillAlpha = appearance.fill?.color ? colorToRgb(appearance.fill.color).alpha : 1;
  const textAlpha = colorToRgb(appearance.text?.color ?? '#ff0000').alpha;
  return {
    GSStroke: pdfDoc.context.obj({ Type: PDFName.of('ExtGState'), CA: PDFNumber.of(opacity * strokeAlpha), ca: PDFNumber.of(opacity * strokeAlpha) }),
    GSFill: pdfDoc.context.obj({ Type: PDFName.of('ExtGState'), CA: PDFNumber.of(opacity * fillAlpha), ca: PDFNumber.of(opacity * fillAlpha) }),
    GSText: pdfDoc.context.obj({ Type: PDFName.of('ExtGState'), CA: PDFNumber.of(opacity * textAlpha), ca: PDFNumber.of(opacity * textAlpha) }),
  };
}

function createTextBoxAppearanceFontResources(fonts: PdfExportFonts, defaultFont: PdfTextBoxFont): Record<string, unknown> {
  const resources: Record<string, unknown> = {
    Helv: {
      Type: PDFName.of('Font'),
      Subtype: PDFName.of('Type1'),
      BaseFont: PDFName.of('Helvetica'),
      Encoding: PDFName.of('WinAnsiEncoding'),
    },
    HelvBold: {
      Type: PDFName.of('Font'),
      Subtype: PDFName.of('Type1'),
      BaseFont: PDFName.of('Helvetica-Bold'),
      Encoding: PDFName.of('WinAnsiEncoding'),
    },
    HelvOblique: {
      Type: PDFName.of('Font'),
      Subtype: PDFName.of('Type1'),
      BaseFont: PDFName.of('Helvetica-Oblique'),
      Encoding: PDFName.of('WinAnsiEncoding'),
    },
    HelvBoldOblique: {
      Type: PDFName.of('Font'),
      Subtype: PDFName.of('Type1'),
      BaseFont: PDFName.of('Helvetica-BoldOblique'),
      Encoding: PDFName.of('WinAnsiEncoding'),
    },
  };

  if (defaultFont.usesEmbeddedUnicodeFont && fonts.unicode) {
    resources.BPUnicode = fonts.unicode.ref;
  }

  return resources;
}

function createCalloutAppearance(pdfDoc: PDFDocument, markup: Extract<Markup, { kind: 'callout' }>, fonts: PdfExportFonts): { ref: PDFRef } {
  const points = markup.leader.points;
  const tip = points[0] ?? pdfPoint(markup.textBox.x, markup.textBox.y);
  const afterTip = points[1] ?? tip;
  const connection = points[points.length - 1] ?? tip;
  const arrow = openArrowHeadPoints(afterTip, tip, 10, 7);
  const bounds = calloutAppearanceBounds(markup, arrow);
  const appearance = resolveMarkupAppearance(markup);
  const strokeAppearance = appearance.stroke!;
  const textAppearance = appearance.text!;
  const stroke = colorToRgb(strokeAppearance.color);
  const text = colorToRgb(textAppearance.color);
  const inset = textAppearance.insetPt;
  const lines = wrapTextBoxText(markup.text, markup.textBox.width, fonts.helvetica, textAppearance.fontSizePt, inset);
  const firstBaseline = markup.textBox.y + markup.textBox.height - inset - textAppearance.fontSizePt * (13 / 12);
  const textCommands = lines.flatMap((line, index) => {
    const measuredWidth = fonts.helvetica.widthOfTextAtSize(line, textAppearance.fontSizePt);
    const x = textAppearance.align === 'center'
      ? markup.textBox.x + markup.textBox.width * 0.5 - measuredWidth * 0.5
      : textAppearance.align === 'right'
        ? markup.textBox.x + markup.textBox.width - inset - measuredWidth
        : markup.textBox.x + inset;
    return [`1 0 0 1 ${formatPdfNumber(x)} ${formatPdfNumber(firstBaseline - index * textAppearance.lineHeightPt)} Tm (${escapePdfLiteralString(line)}) Tj`];
  });
  const commands = [
    'q /GS0 gs 1 0 0 1 0 0 cm',
    `${formatPdfNumber(stroke.red)} ${formatPdfNumber(stroke.green)} ${formatPdfNumber(stroke.blue)} RG ${formatPdfNumber(strokeAppearance.widthPt)} w`,
    `${formatPdfNumber(connection.x)} ${formatPdfNumber(connection.y)} m`,
    ...points.slice(0, -1).reverse().map((point) => `${formatPdfNumber(point.x)} ${formatPdfNumber(point.y)} l`),
    'S',
    `${formatPdfNumber(arrow[0].x)} ${formatPdfNumber(arrow[0].y)} m ${formatPdfNumber(tip.x)} ${formatPdfNumber(tip.y)} l ${formatPdfNumber(arrow[2].x)} ${formatPdfNumber(arrow[2].y)} l S`,
    `0 w BT ${formatPdfNumber(text.red)} ${formatPdfNumber(text.green)} ${formatPdfNumber(text.blue)} rg /Helv ${formatPdfNumber(textAppearance.fontSizePt)} Tf`,
    ...textCommands,
    'ET Q',
  ];

  const stream = pdfDoc.context.flateStream(`${commands.join(' ')} `, {
    Type: PDFName.of('XObject'),
    Subtype: PDFName.of('Form'),
    FormType: PDFNumber.of(1),
    BBox: bounds,
    Resources: pdfDoc.context.obj({
      ProcSet: [PDFName.of('PDF'), PDFName.of('Text')],
      Font: {
        Helv: {
          Type: PDFName.of('Font'),
          Subtype: PDFName.of('Type1'),
          BaseFont: PDFName.of('Helvetica'),
          Encoding: PDFName.of('WinAnsiEncoding'),
        },
      },
      ExtGState: {
        GS0: pdfDoc.context.obj({
          Type: PDFName.of('ExtGState'),
          CA: PDFNumber.of(appearance.opacity * stroke.alpha),
          ca: PDFNumber.of(appearance.opacity * text.alpha),
        }),
      },
    } as any),
  });

  return { ref: pdfDoc.context.register(stream) as PDFRef };
}

function calloutAppearanceBounds(markup: Extract<Markup, { kind: 'callout' }>, arrow: readonly PdfPoint[]): readonly number[] {
  const xs = [
    markup.textBox.x,
    markup.textBox.x + markup.textBox.width,
    ...markup.leader.points.map((point) => point.x),
    ...arrow.map((point) => point.x),
  ];
  const ys = [
    markup.textBox.y,
    markup.textBox.y + markup.textBox.height,
    ...markup.leader.points.map((point) => point.y),
    ...arrow.map((point) => point.y),
  ];
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

async function embedMarkupImage(pdfDoc: PDFDocument, markup: Extract<Markup, { kind: 'image' | 'snapshot' }>): Promise<PDFImage> {
  const bytes = imageBytesFromDataUrl(markup.dataUrl);
  return markup.mimeType === 'image/jpeg'
    ? await pdfDoc.embedJpg(bytes)
    : await pdfDoc.embedPng(bytes);
}

function createImageAppearance(pdfDoc: PDFDocument, markup: Extract<Markup, { kind: 'image' | 'snapshot' }>, image: PDFImage): { ref: PDFRef } {
  const bounds = imageAnnotationRect(markup);
  const radians = ((markup.rotation ?? 0) * Math.PI) / 180;
  const cosine = Math.cos(radians);
  const sine = Math.sin(radians);
  const a = markup.rect.width * cosine;
  const b = markup.rect.width * sine;
  const c = -markup.rect.height * sine;
  const d = markup.rect.height * cosine;
  const centerX = markup.rect.x + markup.rect.width * 0.5;
  const centerY = markup.rect.y + markup.rect.height * 0.5;
  const e = centerX - a * 0.5 - c * 0.5;
  const f = centerY - b * 0.5 - d * 0.5;
  const opacity = resolveMarkupAppearance(markup).opacity;
  const commands = [
    'q /GS0 gs',
    `${formatPdfNumber(a)} ${formatPdfNumber(b)} ${formatPdfNumber(c)} ${formatPdfNumber(d)} ${formatPdfNumber(e)} ${formatPdfNumber(f)} cm`,
    '/Im0 Do',
    'Q',
  ];
  const stream = pdfDoc.context.flateStream(`${commands.join(' ')} `, {
    Type: PDFName.of('XObject'),
    Subtype: PDFName.of('Form'),
    FormType: PDFNumber.of(1),
    BBox: rectToPdfArray(bounds),
    Resources: pdfDoc.context.obj({
      XObject: {
        Im0: image.ref,
      },
      ExtGState: {
        GS0: pdfDoc.context.obj({
          Type: PDFName.of('ExtGState'),
          CA: PDFNumber.of(opacity),
          ca: PDFNumber.of(opacity),
        }),
      },
    } as any),
  });
  return { ref: pdfDoc.context.register(stream) as PDFRef };
}

function imageBytesFromDataUrl(dataUrl: string): Uint8Array {
  const base64 = dataUrl.includes(',') ? dataUrl.split(',').at(-1) ?? '' : dataUrl;
  return Uint8Array.from(Buffer.from(base64, 'base64'));
}

function fallbackImageDataUrl(): string {
  return 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAAAoCAYAAABOzvzpAAAABHNCSVQICAgIfAhkiAAAAAFzUkdCAK7OHOkAAAIpSURBVGiB7Zi/TxNhHMafO0vvWiBBlIC2ooi1xkT5MZFAHEwMJCwMpotuDqZx4R9QEyWyOJkuYhxcTAcHEkYSEjc3fqXByo9AyJUCRh3s3XE1d06uvO8dtd8v6T3z8z7vk8/dffO+p6QeHXpoYKnUBagVAqAuQK0QAHUBaoUAqAtQKwRAXYBaSumXF54EG1khAOoC1IpQF5CR67n48nsFBWsT36wdFO1tKFCQjl3Gdf0KbsauYqjlNlTF//NkPwRLziFeGm+xaq4f67sVT+Fp4jEuRjt85QsBxO7f8xUokvVpXto79/Mz3pQ/wnJtKX9M1THZ9QDjZ+9I78H2E5j9sYDXex98rbFcG9Ol96h6fzDRfldqDcshaDgHyO3nA6/P7edhOAdSXnYAXHiYMmZgu0eBM2z3CFPGDFyIxxs7AGvWlnDgyWjVXMeatSX0sQNQMDfqmsUOwGLla82yCtam0MMOgExp6SzzFAKopRyvKvSwA5DSu+uaxQ7AtRoCkMliB6Dh34CR1gH06skT5/RoCYy0Dgp97ADoqoZniSyalODXlCYlgheXnkBXo0IvOwAA0Ksnke3MBF6f7cygR0tIedneBjPnRqGpUeTKeenrcMuZOCa7HmKsbVh6H/Y/RPaq3/HKeCc8IQ4038DzZBbnI22+8tkD+Kddp4ylShFLZhHLlSI8eOhvTqMvnkZ/PI1u7UKg3FMD4H+J5RCsp0IA1AWoFQKgLkCtv9cipgMsRDYAAAAAAElFTkSuQmCC';
}

function openArrowHeadPoints(start: PdfPoint, end: PdfPoint, length: number, width: number): readonly PdfPoint[] {
  const dx = start.x - end.x;
  const dy = start.y - end.y;
  const distance = Math.hypot(dx, dy);
  if (distance === 0) {
    return [end, end, end];
  }
  const ux = dx / distance;
  const uy = dy / distance;
  const base = pdfPoint(end.x + ux * length, end.y + uy * length);
  const perpendicular = { x: -uy, y: ux };
  return [
    pdfPoint(base.x + perpendicular.x * width * 0.5, base.y + perpendicular.y * width * 0.5),
    end,
    pdfPoint(base.x - perpendicular.x * width * 0.5, base.y - perpendicular.y * width * 0.5),
  ];
}

function getTextBoxFont(markup: Extract<Markup, { kind: 'text-box' }>, fonts: PdfExportFonts, run: Partial<TextBoxRichTextRun>): PdfTextBoxFont {
  if (fonts.unicode && needsUnicodeTextBoxFont(markup)) {
    return {
      font: fonts.unicode,
      resourceName: 'BPUnicode',
      styleName: 'Arial Unicode MS',
      usesEmbeddedUnicodeFont: true,
    };
  }

  if (run.bold && run.italic) {
    return {
      font: fonts.helveticaBoldOblique,
      resourceName: 'HelvBoldOblique',
      styleName: 'Helvetica',
      usesEmbeddedUnicodeFont: false,
    };
  }
  if (run.bold) {
    return {
      font: fonts.helveticaBold,
      resourceName: 'HelvBold',
      styleName: 'Helvetica',
      usesEmbeddedUnicodeFont: false,
    };
  }
  if (run.italic) {
    return {
      font: fonts.helveticaOblique,
      resourceName: 'HelvOblique',
      styleName: 'Helvetica',
      usesEmbeddedUnicodeFont: false,
    };
  }

  return {
    font: fonts.helvetica,
    resourceName: 'Helv',
    styleName: 'Helvetica',
    usesEmbeddedUnicodeFont: false,
  };
}

function needsUnicodeTextBoxFont(markup: Extract<Markup, { kind: 'text-box' }>): boolean {
  return resolveMarkupAppearance(markup).text!.fontId === 'ArialUnicode'
    || !canEncodeWithWinAnsi(markup.text)
    || Boolean(markup.richTextRuns?.some((run) => !canEncodeWithWinAnsi(run.text)));
}

function canEncodeWithWinAnsi(text: string): boolean {
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    if (code > 0xff) {
      return false;
    }
  }
  return true;
}

function encodePdfTextShow(text: string, textBoxFont: PdfTextBoxFont): string {
  if (textBoxFont.usesEmbeddedUnicodeFont) {
    return textBoxFont.font.encodeText(text).toString();
  }
  return `(${escapePdfLiteralString(text)})`;
}

function getTextBoxAnnotationRect(markup: Extract<Markup, { kind: 'text-box' }>): { x: number; y: number; width: number; height: number } {
  if (!markup.rotation) {
    return markup.rect;
  }

  return boundsForRotatedRect(markup.rect, normalizeFreeRotation(markup.rotation));
}

function getTextBoxAppearanceMatrix(markup: Extract<Markup, { kind: 'text-box' }>): readonly number[] {
  const { x, y, width, height } = markup.rect;
  if (!markup.rotation) {
    return [1, 0, 0, 1, -x, -y];
  }

  const rotation = normalizeFreeRotation(markup.rotation);
  const radians = (rotation * Math.PI) / 180;
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  const centerX = x + width * 0.5;
  const centerY = y + height * 0.5;
  const unshiftedE = centerX - cos * centerX - sin * centerY;
  const unshiftedF = centerY + sin * centerX - cos * centerY;
  const bounds = boundsForRotatedRect(markup.rect, rotation);
  return [
    Number(formatPdfNumber(cos)),
    Number(formatPdfNumber(-sin)),
    Number(formatPdfNumber(sin)),
    Number(formatPdfNumber(cos)),
    Number(formatPdfNumber(unshiftedE - bounds.x)),
    Number(formatPdfNumber(unshiftedF - bounds.y)),
  ];
}

function boundsForRotatedRect(rect: { x: number; y: number; width: number; height: number }, rotation: number): { x: number; y: number; width: number; height: number } {
  const radians = (rotation * Math.PI) / 180;
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  const centerX = rect.x + rect.width * 0.5;
  const centerY = rect.y + rect.height * 0.5;
  const points = [
    { x: rect.x, y: rect.y },
    { x: rect.x + rect.width, y: rect.y },
    { x: rect.x + rect.width, y: rect.y + rect.height },
    { x: rect.x, y: rect.y + rect.height },
  ].map((point) => ({
    x: centerX + cos * (point.x - centerX) + sin * (point.y - centerY),
    y: centerY - sin * (point.x - centerX) + cos * (point.y - centerY),
  }));
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const minX = Math.min(...xs);
  const minY = Math.min(...ys);
  const maxX = Math.max(...xs);
  const maxY = Math.max(...ys);
  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY,
  };
}

function normalizeFreeRotation(rotation: number): number {
  return ((rotation % 360) + 360) % 360;
}

function wrapTextBoxText(text: string, boxWidthPt: number, helvetica: PDFFont, fontSize = bluebeamTextBoxFontSizePt, insetPt = bluebeamTextBoxInsetPt): readonly string[] {
  const maxWidth = Math.max(1, boxWidthPt - insetPt * 2);
  const lines: string[] = [];
  const paragraphs = text.split(/\r\n|\r|\n/);
  for (const paragraph of paragraphs) {
    lines.push(...wrapTextBoxParagraph(paragraph, maxWidth, helvetica, fontSize));
  }
  return lines.length > 0 ? lines : [''];
}

function wrapTextBoxParagraph(paragraph: string, maxWidth: number, helvetica: PDFFont, fontSize = bluebeamTextBoxFontSizePt): readonly string[] {
  if (paragraph.length === 0) {
    return [''];
  }

  const lines: string[] = [];
  const tokens = paragraph.match(/ +|[^ ]+/g) ?? [];
  let line = '';

  for (const token of tokens) {
    const candidate = `${line}${token}`;
    if (measureText(candidate, helvetica, fontSize) <= maxWidth) {
      line = candidate;
      continue;
    }

    if (/^ +$/.test(token)) {
      if (line) {
        const breakResult = breakLineWithOverflowingSpaces(line, token, maxWidth, helvetica, fontSize);
        lines.push(breakResult.previous);
        line = breakResult.nextPrefix;
      } else {
        line = token;
      }
      continue;
    }

    if (line) {
      const breakResult = breakLineBeforeWord(line, maxWidth, helvetica, fontSize);
      lines.push(breakResult.previous);
      line = breakResult.nextPrefix;
    }

    const nextCandidate = `${line}${token}`;
    if (measureText(nextCandidate, helvetica, fontSize) <= maxWidth) {
      line = nextCandidate;
    } else {
      line = appendWordWithCharacterBreaks(token, line, maxWidth, lines, helvetica, fontSize);
    }
  }

  lines.push(line);
  return lines;
}

function breakLineWithOverflowingSpaces(line: string, spaces: string, maxWidth: number, helvetica: PDFFont, fontSize: number): { previous: string; nextPrefix: string } {
  if (spaces.length <= 1) {
    return { previous: line, nextPrefix: '' };
  }
  const distributableSpaces = spaces.length - 1;
  const singleSpaceWidth = measureText(' ', helvetica, fontSize);
  const fittingSpaceCount = Math.floor((maxWidth - measureText(line, helvetica, fontSize)) / singleSpaceWidth);
  const previousSpaceCount = Math.max(0, Math.min(distributableSpaces, fittingSpaceCount - 1));
  const nextSpaceCount = distributableSpaces - previousSpaceCount;
  return {
    previous: `${line}${' '.repeat(previousSpaceCount)}`,
    nextPrefix: ' '.repeat(nextSpaceCount),
  };
}

function breakLineBeforeWord(line: string, maxWidth: number, helvetica: PDFFont, fontSize: number): { previous: string; nextPrefix: string } {
  const trailingSpaces = line.match(/ +$/)?.[0] ?? '';
  if (trailingSpaces.length <= 1) {
    return { previous: line.trimEnd(), nextPrefix: '' };
  }

  const base = line.slice(0, -trailingSpaces.length);
  const distributableSpaces = trailingSpaces.length - 1;
  const singleSpaceWidth = measureText(' ', helvetica, fontSize);
  const fittingSpaceCount = Math.floor((maxWidth - measureText(base, helvetica, fontSize)) / singleSpaceWidth);
  const previousSpaceCount = Math.max(0, Math.min(distributableSpaces, fittingSpaceCount - 1));
  const nextSpaceCount = distributableSpaces - previousSpaceCount;
  return {
    previous: `${base}${' '.repeat(previousSpaceCount)}`,
    nextPrefix: ' '.repeat(nextSpaceCount),
  };
}

function appendWordWithCharacterBreaks(word: string, initialLine: string, maxWidth: number, lines: string[], helvetica: PDFFont, fontSize = bluebeamTextBoxFontSizePt): string {
  let line = initialLine;
  for (const char of word) {
    const candidate = `${line}${char}`;
    if (line && measureText(candidate, helvetica, fontSize) > maxWidth) {
      lines.push(line);
      line = char;
    } else {
      line = candidate;
    }
  }
  return line;
}

function measureText(text: string, helvetica: PDFFont, fontSize = bluebeamTextBoxFontSizePt): number {
  return helvetica.widthOfTextAtSize(text, fontSize);
}

function splitPdfTextLines(text: string): readonly string[] {
  return text.split(/\r\n|\r|\n/);
}

function layoutExplicitRichTextLines(runs: readonly TextBoxRichTextRun[]): readonly TextBoxAppearanceLine[] {
  const lines: TextBoxAppearanceRun[][] = [[]];

  for (const sourceRun of runs) {
    const run = normalizeRichTextRun(sourceRun);
    for (const char of run.text) {
      if (char === '\r') {
        continue;
      }
      if (char === '\n') {
        lines.push([]);
        continue;
      }
      appendRichRunText(lines[lines.length - 1], run, char);
    }
  }

  return lines.map((line) => ({ runs: line.length > 0 ? line : [{ text: '' }] }));
}

function appendRichRunText(line: TextBoxAppearanceRun[], run: TextBoxAppearanceRun, text: string): void {
  const previous = line[line.length - 1];
  if (previous && richRunStyleKey(previous) === richRunStyleKey(run)) {
    line[line.length - 1] = {
      ...previous,
      text: `${previous.text}${text}`,
    };
    return;
  }

  line.push({
    ...run,
    text,
  });
}

function normalizeRichTextRun(run: TextBoxRichTextRun): TextBoxAppearanceRun {
  return {
    ...run,
    text: run.text,
  };
}

function richRunStyleKey(run: Partial<TextBoxRichTextRun>): string {
  return JSON.stringify({
    bold: Boolean(run.bold),
    italic: Boolean(run.italic),
    color: run.color?.toLowerCase(),
    fontSizePt: run.fontSizePt,
  });
}

function measureRichRunText(text: string, run: Partial<TextBoxRichTextRun>, markup: Extract<Markup, { kind: 'text-box' }>, fonts: PdfExportFonts): number {
  const font = getTextBoxFont(markup, fonts, run);
  return measureText(text, font.font, getTextBoxRunFontSize(markup, run));
}

function getTextBoxFontSize(markup: Extract<Markup, { kind: 'text-box' }>): number {
  return resolveMarkupAppearance(markup).text!.fontSizePt;
}

function getTextBoxRunFontSize(markup: Extract<Markup, { kind: 'text-box' }>, run: Partial<TextBoxRichTextRun>): number {
  return run.fontSizePt ?? getTextBoxFontSize(markup);
}

function getTextBoxLineHeight(markup: Extract<Markup, { kind: 'text-box' }>): number {
  return resolveMarkupAppearance(markup).text!.lineHeightPt;
}

function getTextBoxFirstBaselineOffset(markup: Extract<Markup, { kind: 'text-box' }>): number {
  return getTextBoxFontSize(markup) * bluebeamTextBoxFirstBaselineOffsetRatio;
}

function getAlignedTextX(line: string, markup: Extract<Markup, { kind: 'text-box' }>, fonts: PdfExportFonts): number {
  const textWidth = markup.richTextRuns
    ? measureRichLineText(line, markup, fonts)
    : measureText(line, getTextBoxFont(markup, fonts, {}).font, getTextBoxFontSize(markup));
  const textAppearance = resolveMarkupAppearance(markup).text!;
  const align = textAppearance.align;
  if (align === 'center') {
    return markup.rect.x + markup.rect.width * 0.5 - textWidth * 0.5;
  }
  if (align === 'right') {
    return markup.rect.x + markup.rect.width - textAppearance.insetPt - textWidth;
  }
  return markup.rect.x + textAppearance.insetPt;
}

function measureRichLineText(line: string, markup: Extract<Markup, { kind: 'text-box' }>, fonts: PdfExportFonts): number {
  const richLines = markup.richTextRuns ? layoutExplicitRichTextLines(markup.richTextRuns) : [];
  const matchingLine = richLines.find((richLine) => richLine.runs.map((run) => run.text).join('') === line);
  return matchingLine?.runs.reduce((width, run) => width + measureRichRunText(run.text, run, markup, fonts), 0)
    ?? measureText(line, getTextBoxFont(markup, fonts, {}).font, getTextBoxFontSize(markup));
}

function createTextBoxRichContent(text: string, markup?: Extract<Markup, { kind: 'text-box' }>, textBoxFont?: PdfTextBoxFont): string {
  const paragraph = markup?.richTextRuns
    ? markup.richTextRuns.map((run) => `<span style="${createRichTextRunStyle(run, markup)}">${escapeXml(run.text)}</span>`).join('')
    : escapeXml(text);
  return `<?xml version="1.0"?><body xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/" xfa:contentType="text/html" xfa:APIVersion="BluebeamPDFRevu:2018" xfa:spec="2.2.0" style="${createTextBoxDefaultStyle(markup, textBoxFont)}" xmlns="http://www.w3.org/1999/xhtml"><p>${paragraph}</p></body>`;
}

function createRichTextRunStyle(run: TextBoxRichTextRun, markup: Extract<Markup, { kind: 'text-box' }>): string {
  const textAppearance = resolveMarkupAppearance(markup).text!;
  const styles = [
    `font-family:Helvetica`,
    `font-size:${formatPdfNumber(run.fontSizePt ?? getTextBoxFontSize(markup))}pt`,
    `color:${normalizeCssHex(run.color ?? textAppearance.color).toUpperCase()}`,
  ];
  if (run.bold) {
    styles.push('font-weight:bold');
  }
  if (run.italic) {
    styles.push('font-style:italic');
  }
  return styles.join('; ');
}

function createTextBoxDefaultStyle(markup?: Extract<Markup, { kind: 'text-box' }>, textBoxFont?: PdfTextBoxFont): string {
  const fontSize = markup ? getTextBoxFontSize(markup) : bluebeamTextBoxFontSizePt;
  const lineHeight = markup ? getTextBoxLineHeight(markup) : fontSize * bluebeamTextBoxLineHeightRatio;
  const textAppearance = markup ? resolveMarkupAppearance(markup).text : undefined;
  const align = textAppearance?.align ?? 'left';
  const color = normalizeCssHex(textAppearance?.color ?? '#ff0000').toUpperCase();
  const inset = textAppearance?.insetPt ?? bluebeamTextBoxInsetPt;
  return `font: ${textBoxFont?.styleName ?? 'Helvetica'} ${formatPdfNumber(fontSize)}pt; text-align:${align}; margin:${formatPdfNumber(inset)}pt; line-height:${formatPdfNumber(lineHeight)}pt; color:${color}`;
}

function createPdfTextString(text: string): PDFString | PDFHexString {
  return canEncodeWithWinAnsi(text) ? PDFString.of(text) : PDFHexString.fromText(text);
}

function readRichTextRuns(richContent: string | undefined): readonly TextBoxRichTextRun[] | undefined {
  if (!richContent || !richContent.includes('<span')) {
    return undefined;
  }

  const runs: TextBoxRichTextRun[] = [];
  const spanPattern = /<span\b([^>]*)>([\s\S]*?)<\/span>/gi;
  let match: RegExpExecArray | null;
  while ((match = spanPattern.exec(richContent)) !== null) {
    const text = decodeRichTextContent(match[2]);
    if (!text) {
      continue;
    }
    const style = readHtmlAttribute(match[1], 'style');
    runs.push({
      text,
      ...readRichTextRunStyle(style),
    });
  }

  return runs.length > 0 ? runs : undefined;
}

function readRichTextRunStyle(style: string | undefined): Omit<TextBoxRichTextRun, 'text'> {
  const result: Omit<TextBoxRichTextRun, 'text'> = {};
  if (!style) {
    return result;
  }
  const lowerStyle = style.toLowerCase();
  const color = style.match(/(?:^|;)\s*color\s*:\s*(#[0-9a-f]{6})/i)?.[1];
  const fontSize = style.match(/(?:^|;)\s*font-size\s*:\s*([0-9.]+)pt/i)?.[1];
  return {
    bold: /(?:^|;)\s*font-weight\s*:\s*(bold|700)\b/i.test(lowerStyle) || undefined,
    italic: /(?:^|;)\s*font-style\s*:\s*italic\b/i.test(lowerStyle) || undefined,
    color: color?.toLowerCase(),
    fontSizePt: fontSize ? Number(fontSize) : undefined,
  };
}

function readHtmlAttribute(attributes: string, name: string): string | undefined {
  const pattern = new RegExp(`${name}\\s*=\\s*["']([^"']*)["']`, 'i');
  return attributes.match(pattern)?.[1];
}

function decodeRichTextContent(value: string): string {
  return value
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<[^>]+>/g, '')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&amp;/g, '&');
}

function textAlignToPdfQ(align: 'left' | 'center' | 'right' | undefined): number {
  if (align === 'center') {
    return 1;
  }
  if (align === 'right') {
    return 2;
  }
  return 0;
}

function rgbToPdfOperator(color: string): string {
  const rgb = colorToRgb(color);
  return `${formatPdfNumber(rgb.red)} ${formatPdfNumber(rgb.green)} ${formatPdfNumber(rgb.blue)} rg`;
}

function pdfColorArray(color: string | undefined): readonly number[] {
  if (!color) {
    return [];
  }
  const rgb = colorToRgb(color);
  return [rgb.red, rgb.green, rgb.blue];
}

function pdfAnnotationOpacityFields(opacity: number, strokeColor?: string, nonStrokeColor?: string | null): Record<string, PDFNumber> {
  const normalized = Math.max(0, Math.min(1, opacity));
  return {
    CA: PDFNumber.of(normalized * (strokeColor ? colorToRgb(strokeColor).alpha : 1)),
    ca: PDFNumber.of(normalized * (nonStrokeColor ? colorToRgb(nonStrokeColor).alpha : 1)),
  };
}

function pdfTextDefaultAppearance(text: ResolvedMarkupAppearance['text']): string {
  const color = text?.color ?? '#ff0000';
  const resourceName = text?.fontId === 'ArialUnicode' ? 'BPUnicode' : 'Helv';
  return `${rgbToPdfOperator(color)} /${resourceName} ${formatPdfNumber(text?.fontSizePt ?? 12)} Tf`;
}

function pdfTextDefaultStyle(
  text: ResolvedMarkupAppearance['text'],
  alignOverride?: 'left' | 'center' | 'right',
): string {
  const fontName = text?.fontId === 'ArialUnicode' ? 'Arial Unicode MS' : 'Helvetica';
  const fontSize = text?.fontSizePt ?? 12;
  const align = alignOverride ?? text?.align ?? 'left';
  const lineHeight = text?.lineHeightPt ?? fontSize * bluebeamTextBoxLineHeightRatio;
  const inset = text?.insetPt ?? 0;
  const color = normalizeCssHex(text?.color ?? '#ff0000');
  return [
    `font: ${fontName} ${formatPdfNumber(fontSize)}pt`,
    `text-align:${align}`,
    ...(inset > 0 ? [`margin:${formatPdfNumber(inset)}pt`] : []),
    `line-height:${formatPdfNumber(lineHeight)}pt`,
    `color:${color}`,
  ].join('; ');
}

function escapeXml(text: string): string {
  return text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;')
    .replace(/\r\n|\r|\n/g, '<br/>');
}

function escapePdfLiteralString(text: string): string {
  return text
    .replaceAll('\\', '\\\\')
    .replaceAll('(', '\\(')
    .replaceAll(')', '\\)')
    .replaceAll('\r', '\\r')
    .replaceAll('\n', '\\n');
}

function colorToRgb(color: string): { red: number; green: number; blue: number; alpha: number } {
  const match = color.trim().match(/^#?([0-9a-f]{6})$/i);
  if (match) {
    const value = match[1];
    return {
      red: Number.parseInt(value.slice(0, 2), 16) / 255,
      green: Number.parseInt(value.slice(2, 4), 16) / 255,
      blue: Number.parseInt(value.slice(4, 6), 16) / 255,
      alpha: 1,
    };
  }
  const rgbMatch = color.trim().match(/^rgba?\(\s*([0-9.]+)\s*,\s*([0-9.]+)\s*,\s*([0-9.]+)(?:\s*,\s*([0-9.]+))?\s*\)$/i);
  if (rgbMatch) {
    return {
      red: Math.max(0, Math.min(255, Number(rgbMatch[1]))) / 255,
      green: Math.max(0, Math.min(255, Number(rgbMatch[2]))) / 255,
      blue: Math.max(0, Math.min(255, Number(rgbMatch[3]))) / 255,
      alpha: rgbMatch[4] === undefined ? 1 : Math.max(0, Math.min(1, Number(rgbMatch[4]))),
    };
  }
  return { red: 1, green: 0, blue: 0, alpha: 1 };
}

function normalizeCssHex(color: string): string {
  const match = color.trim().match(/^#?([0-9a-f]{6})$/i);
  return match ? `#${match[1].toUpperCase()}` : color;
}

function formatPdfNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(4).replace(/0+$/, '').replace(/\.$/, '');
}

function readPageAnnotationMarkups(pdfDoc: PDFDocument, pageIndex: number): ImportedPdfMarkup[] {
  const page = pdfDoc.getPage(pageIndex);
  const rawAnnots = (page.node as PDFDict).get(PDFName.of('Annots')) ?? (page.node as any).Annots?.();
  const annots = rawAnnots ? pdfDoc.context.lookup(rawAnnots) : undefined;
  if (!(annots instanceof PDFArray)) {
    return [];
  }

  const markups: ImportedPdfMarkup[] = [];
  const refs = annots.asArray();
  const consumed = new Set<number>();

  for (let index = 0; index < refs.length; index += 1) {
    if (consumed.has(index)) {
      continue;
    }
    const annot = pdfDoc.context.lookup(refs[index]);
    if (!isPdfDict(annot) || !isCloudPlusPart(annot)) {
      continue;
    }

    const matchIndex = refs.findIndex((candidateRef, candidateIndex) => {
      if (candidateIndex === index || consumed.has(candidateIndex)) {
        return false;
      }
      const candidate = pdfDoc.context.lookup(candidateRef);
      return isPdfDict(candidate) && isCloudPlusPart(candidate) && cloudPlusGroupKey(candidate) === cloudPlusGroupKey(annot) && readName(candidate.get(PDFName.of('Subtype'))) !== readName(annot.get(PDFName.of('Subtype')));
    });
    if (matchIndex < 0) {
      continue;
    }

    const match = pdfDoc.context.lookup(refs[matchIndex]);
    if (!isPdfDict(match)) {
      continue;
    }
    const mapped = mapCloudPlusPairToMarkup(pageIndex, annot, match, index);
    if (mapped) {
      markups.push(mapped);
      consumed.add(index);
      consumed.add(matchIndex);
    }
  }

  for (let index = 0; index < refs.length; index += 1) {
    if (consumed.has(index)) {
      continue;
    }
    const annot = pdfDoc.context.lookup(refs[index]);
    if (!isPdfDict(annot)) {
      continue;
    }

    const mapped = mapAnnotationDictToMarkup(pageIndex, annot, index);
    if (mapped) {
      markups.push(mapped);
    }
  }

  return markups;
}

function isCloudPlusPart(annot: PDFDict): boolean {
  const subject = readText(annot.get(PDFName.of('Subj')));
  const intentEx = readName(annot.get(PDFName.of('ITEx')));
  const intent = readName(annot.get(PDFName.of('IT')));
  const subtype = readName(annot.get(PDFName.of('Subtype'))).toLowerCase();
  return subject === 'Cloud+' && intentEx === 'PolyText' && (
    (subtype === 'polygon' && intent === 'PolygonCloud')
    || (subtype === 'freetext' && intent === 'FreeTextCallout')
  );
}

function cloudPlusGroupKey(annot: PDFDict): string {
  const group = readTextArray(annot.get(PDFName.of('GroupNesting')));
  return group.length > 0 ? group.join('|') : 'cloud-plus';
}

function readTextArray(value: unknown): readonly string[] {
  if (!(value instanceof PDFArray)) {
    return [];
  }
  return value.asArray().map((item) => readText(item) ?? readName(item)).filter((item) => item.length > 0);
}

function mapCloudPlusPairToMarkup(pageIndex: number, first: PDFDict, second: PDFDict, fallbackIndex: number): ImportedPdfMarkup | undefined {
  const firstSubtype = readName(first.get(PDFName.of('Subtype'))).toLowerCase();
  const cloudAnnot = firstSubtype === 'polygon' ? first : second;
  const textAnnot = firstSubtype === 'freetext' ? first : second;
  const rect = readRect(textAnnot.get(PDFName.of('Rect'))) ?? { x: 0, y: 0, width: 0, height: 0 };
  const rawName = readText(cloudAnnot.get(PDFName.of('NM'))) ?? readText(textAnnot.get(PDFName.of('NM')));
  const managedId = fromManagedAnnotationId(rawName)?.replace(/:(cloud|text)$/, '');
  const id = managedId ?? rawName ?? `page-${pageIndex}-cloud-plus-${fallbackIndex}`;
  const points = readPointArray(cloudAnnot.get(PDFName.of('Vertices')));
  const linePoints = readPointArray(textAnnot.get(PDFName.of('CL')));
  const cloudAppearance = readPdfAnnotationAppearance(cloudAnnot);
  const textAppearance = readPdfAnnotationAppearance(textAnnot);
  return createCloudPlusMarkup({
    id,
    pageIndex,
    appearance: {
      ...cloudAppearance,
      ...(textAppearance.text ? { text: textAppearance.text } : {}),
      opacity: textAppearance.opacity ?? cloudAppearance.opacity,
    },
    cloud: {
      controlPath: points.length >= 3 ? points : [pdfPoint(rect.x, rect.y), pdfPoint(rect.x + rect.width, rect.y), pdfPoint(rect.x + rect.width, rect.y + rect.height)],
      borderEffectIntensity: readBorderEffectIntensity(cloudAnnot.get(PDFName.of('BE'))),
      scallopRadius: readCloudScallopRadiusFromAppearance(cloudAnnot, points),
      appearancePath: readCloudAppearancePath(cloudAnnot),
    },
    leader: {
      points: linePoints.length > 0 ? linePoints : [pdfPoint(rect.x, rect.y), pdfPoint(rect.x + rect.width, rect.y + rect.height)],
    },
    textBox: rect,
    text: readText(textAnnot.get(PDFName.of('Contents'))) ?? readText(textAnnot.get(PDFName.of('RC'))) ?? '',
    source: {
      annotationId: id,
      source: 'imported',
    },
  });
}

function appendAnnotation(page: PDFPage, pdfDoc: PDFDocument, annot: PDFDict, appearance?: ResolvedMarkupAppearance): void {
  if (appearance) {
    annot.set(PDFName.of('BPAppearance'), PDFString.of(JSON.stringify(appearance)));
  }
  const ref = pdfDoc.context.register(annot) as PDFRef;
  const existing = page.node.Annots();
  if (existing) {
    existing.push(ref);
  } else {
    page.node.set(PDFName.of('Annots'), pdfDoc.context.obj([ref]));
  }
}

function normalizeRotation(rotation: number): 0 | 90 | 180 | 270 {
  const normalized = ((rotation % 360) + 360) % 360;
  if (normalized === 90 || normalized === 180 || normalized === 270) {
    return normalized;
  }
  return 0;
}

function stringOrUndefined(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function isLengthMeasurementSubject(subject: string | null | undefined): boolean {
  const normalized = subject?.toLowerCase() ?? '';
  return normalized === 'length' || normalized === 'length measurement';
}

function isPolylengthMeasurementSubject(subject: string | null | undefined): boolean {
  const normalized = subject?.toLowerCase() ?? '';
  return normalized === 'polylength' || normalized === 'polylength measurement';
}

function isAreaMeasurementSubject(subject: string | null | undefined): boolean {
  const normalized = subject?.toLowerCase() ?? '';
  return normalized === 'area' || normalized === 'area measurement';
}

function isPdfDict(value: unknown): value is PDFDict {
  return Boolean(value && typeof value === 'object' && 'get' in value && 'set' in value);
}

function mapAnnotationDictToMarkup(pageIndex: number, annot: PDFDict, fallbackIndex: number): ImportedPdfMarkup | undefined {
  const subtype = readName(annot.get(PDFName.of('Subtype'))).toLowerCase();
  const rawName = readText(annot.get(PDFName.of('NM')));
  const managedId = fromManagedAnnotationId(rawName);
  const subject = readText(annot.get(PDFName.of('Subj')));
  const intent = readName(annot.get(PDFName.of('IT')));
  const rect = readRect(annot.get(PDFName.of('Rect'))) ?? { x: 0, y: 0, width: 0, height: 0 };
  const importedAppearance = readPdfAnnotationAppearance(annot);

  if (subtype === 'popup') {
    return undefined;
  }

  if (subtype === 'link') {
    return undefined;
  }

  if ((subtype === 'square' || subtype === 'rect') && (subject === 'Image' || intent.toLowerCase() === 'squareimage')) {
    const id = managedId ?? rawName ?? `page-${pageIndex}-image-${fallbackIndex}`;
    const dataUrl = readText(annot.get(PDFName.of('BPImageData'))) ?? '';
    const mimeType = readText(annot.get(PDFName.of('BPImageMimeType'))) === 'image/jpeg' ? 'image/jpeg' : 'image/png';
    return createImageMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      rect,
      dataUrl: dataUrl || fallbackImageDataUrl(),
      mimeType,
      rotation: readOptionalNumber(annot.get(PDFName.of('Rotation'))),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'stamp' && (subject === 'Snapshot' || intent.toLowerCase() === 'stampsnapshot')) {
    const id = managedId ?? rawName ?? `page-${pageIndex}-snapshot-${fallbackIndex}`;
    const dataUrl = readText(annot.get(PDFName.of('BPSnapshotData'))) ?? '';
    const mimeType = readText(annot.get(PDFName.of('BPSnapshotMimeType'))) === 'image/jpeg' ? 'image/jpeg' : 'image/png';
    return createSnapshotMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      rect,
      dataUrl: dataUrl || fallbackImageDataUrl(),
      mimeType,
      rotation: readOptionalNumber(annot.get(PDFName.of('Rotation'))),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'square' || subtype === 'rect') {
    const id = managedId ?? rawName ?? `page-${pageIndex}-rectangle-${fallbackIndex}`;
    return createRectangleMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      rect,
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'circle' && intent.toLowerCase() === 'circlearc') {
    const id = managedId ?? rawName ?? `page-${pageIndex}-arc-${fallbackIndex}`;
    return createArcMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      rect,
      angle1: readOptionalNumber(annot.get(PDFName.of('Angle1'))) ?? 90,
      angle2: readOptionalNumber(annot.get(PDFName.of('Angle2'))) ?? 180,
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'circle' && intent.toLowerCase() !== 'circlearc') {
    const id = managedId ?? rawName ?? `page-${pageIndex}-ellipse-${fallbackIndex}`;
    return createEllipseMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      rect,
      rotation: readOptionalNumber(annot.get(PDFName.of('Rotation'))),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  const normalizedIntent = intent.toLowerCase();
  if ((subtype === 'square' || subtype === 'rect') && (subject === 'Image' || normalizedIntent === 'squareimage')) {
    const id = managedId ?? rawName ?? `page-${pageIndex}-image-${fallbackIndex}`;
    const dataUrl = readText(annot.get(PDFName.of('BPImageData'))) ?? '';
    const mimeType = readText(annot.get(PDFName.of('BPImageMimeType'))) === 'image/jpeg' ? 'image/jpeg' : 'image/png';
    return createImageMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      rect,
      dataUrl: dataUrl || fallbackImageDataUrl(),
      mimeType,
      rotation: readOptionalNumber(annot.get(PDFName.of('Rotation'))),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'line') {
    if (isLengthMeasurementSubject(subject)) {
      const id = managedId ?? rawName ?? `page-${pageIndex}-length-${fallbackIndex}`;
      const linePoints = readPointArray(annot.get(PDFName.of('L')));
      const [start = pdfPoint(rect.x, rect.y), end = pdfPoint(rect.x + rect.width, rect.y + rect.height)] = linePoints;
      return createLengthMarkup({
        id,
        pageIndex,
        appearance: importedAppearance,
        start,
        end,
        color: readColorArray(annot.get(PDFName.of('C'))),
        source: {
          annotationId: id,
          source: 'imported',
        },
      });
    }

    if (normalizedIntent === 'linedimension') {
      const id = managedId ?? rawName ?? `page-${pageIndex}-dimension-${fallbackIndex}`;
      const linePoints = readPointArray(annot.get(PDFName.of('L')));
      const [start = pdfPoint(rect.x, rect.y), end = pdfPoint(rect.x + rect.width, rect.y + rect.height)] = linePoints;
      return createDimensionMarkup({
        id,
        pageIndex,
        appearance: importedAppearance,
        start,
        end,
        dimensionLineOffset: readOptionalNumber(annot.get(PDFName.of('LL'))) ?? (end.x >= start.x ? 24 : -24),
        text: readText(annot.get(PDFName.of('Cap'))) ?? readText(annot.get(PDFName.of('Contents'))) ?? 'Dimension',
        color: readColorArray(annot.get(PDFName.of('C'))),
        source: {
          annotationId: id,
          source: 'imported',
        },
      });
    }

    const id = managedId ?? rawName ?? `page-${pageIndex}-${normalizedIntent === 'linearrow' ? 'arrow' : 'line'}-${fallbackIndex}`;
    const linePoints = readPointArray(annot.get(PDFName.of('L')));
    const [start = pdfPoint(rect.x, rect.y), end = pdfPoint(rect.x + rect.width, rect.y + rect.height)] = linePoints;
    const createMarkup = normalizedIntent === 'linearrow' ? createArrowMarkup : createLineMarkup;
    return createMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      start,
      end,
      color: readColorArray(annot.get(PDFName.of('C'))),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'polyline') {
    const isPolylength = isPolylengthMeasurementSubject(subject);
    const id = managedId ?? rawName ?? `page-${pageIndex}-${isPolylength ? 'polylength' : 'polyline'}-${fallbackIndex}`;
    const points = readPointArray(annot.get(PDFName.of('Vertices')));
    const createMarkup = isPolylength ? createPolylengthMarkup : createPolylineMarkup;
    return createMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      points: points.length >= 2 ? points : [pdfPoint(rect.x, rect.y), pdfPoint(rect.x + rect.width, rect.y + rect.height)],
      color: readColorArray(annot.get(PDFName.of('C'))),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'polygon' && normalizedIntent === 'polygoncloud') {
    const id = managedId ?? rawName ?? `page-${pageIndex}-cloud-${fallbackIndex}`;
    const points = readPointArray(annot.get(PDFName.of('Vertices')));
    return createCloudMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      controlPath: points.length >= 3 ? points : [pdfPoint(rect.x, rect.y), pdfPoint(rect.x + rect.width, rect.y), pdfPoint(rect.x + rect.width, rect.y + rect.height)],
      color: readColorArray(annot.get(PDFName.of('C'))),
      borderEffectIntensity: readBorderEffectIntensity(annot.get(PDFName.of('BE'))),
      scallopRadius: readCloudScallopRadiusFromAppearance(annot, points),
      appearancePath: readCloudAppearancePath(annot),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'polygon') {
    const isArea = isAreaMeasurementSubject(subject);
    const id = managedId ?? rawName ?? `page-${pageIndex}-${isArea ? 'area' : 'polygon'}-${fallbackIndex}`;
    const points = readPointArray(annot.get(PDFName.of('Vertices')));
    const createMarkup = isArea ? createAreaMarkup : createPolygonMarkup;
    return createMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      points: points.length >= 3 ? points : [pdfPoint(rect.x, rect.y), pdfPoint(rect.x + rect.width, rect.y), pdfPoint(rect.x + rect.width, rect.y + rect.height)],
      color: readColorArray(annot.get(PDFName.of('C'))),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'ink') {
    const normalizedSubject = subject?.toLowerCase();
    const isHighlight = normalizedSubject === 'highlight' || readName(annot.get(PDFName.of('BM'))).toLowerCase() === 'multiply';
    const id = managedId ?? rawName ?? `page-${pageIndex}-${isHighlight ? 'highlight' : 'pen'}-${fallbackIndex}`;
    const paths = readInkList(annot.get(PDFName.of('InkList')));
    const createMarkup = isHighlight ? createHighlightMarkup : createPenMarkup;
    return createMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      paths: paths.length > 0 ? paths : [[pdfPoint(rect.x, rect.y), pdfPoint(rect.x + rect.width, rect.y + rect.height)]],
      strokeWidth: readBorderWidth(annot),
      color: readColorArray(annot.get(PDFName.of('C'))),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'freetext' && (normalizedIntent === 'freetextcallout' || annot.get(PDFName.of('CL')))) {
    const id = managedId ?? rawName ?? `page-${pageIndex}-callout-${fallbackIndex}`;
    const textBox = rect;
    const linePoints = readPointArray(annot.get(PDFName.of('CL')));
    const text = readText(annot.get(PDFName.of('Contents'))) ?? readText(annot.get(PDFName.of('RC'))) ?? '';
    return createCalloutMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      leader: {
        points: linePoints.length > 0 ? linePoints : [pdfPoint(textBox.x, textBox.y), pdfPoint(textBox.x + textBox.width, textBox.y + textBox.height)],
      },
      textBox,
      text,
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  if (subtype === 'freetext' && normalizedIntent !== 'freetextcallout' && !annot.get(PDFName.of('CL'))) {
    const id = managedId ?? rawName ?? `page-${pageIndex}-text-box-${fallbackIndex}`;
    const da = annot.get(PDFName.of('DA'));
    const ds = annot.get(PDFName.of('DS'));
    const color = readColor(da) ?? readColorArray(annot.get(PDFName.of('C')));
    const borderWidth = readBorderWidth(annot);
    const rotation = readOptionalNumber(annot.get(PDFName.of('Rotation')));
    return createTextBoxMarkup({
      id,
      pageIndex,
      appearance: importedAppearance,
      rect: rotation ? readFreeTextAppearanceBBox(annot) ?? rect : rect,
      text: readText(annot.get(PDFName.of('Contents'))) ?? readText(annot.get(PDFName.of('RC'))) ?? '',
      rotation,
      richTextRuns: readRichTextRuns(readText(annot.get(PDFName.of('RC')))),
      appearanceTextLines: readFreeTextAppearanceLines(annot),
      color,
      borderColor: color ?? readColorArray(annot.get(PDFName.of('C'))),
      borderWidth,
      fontFamily: readFontFamily(ds, da),
      fontSizePt: readFontSize(da),
      lineHeightPt: readLineHeight(ds),
      textAlign: readTextAlign(annot.get(PDFName.of('Q')), ds),
      source: {
        annotationId: id,
        source: 'imported',
      },
    });
  }

  return createImportedAnnotationMarkup({
    id: rawName ?? `page-${pageIndex}-annotation-${fallbackIndex}`,
    pageIndex,
    rect,
    subtype: subtype || 'unknown',
    subject,
    intent: intent || undefined,
    contents: readText(annot.get(PDFName.of('Contents'))),
    source: {
      annotationId: rawName,
      source: 'imported',
    },
  });
}

function readFreeTextAppearanceBBox(annot: PDFDict): { x: number; y: number; width: number; height: number } | undefined {
  const normalAppearance = getNormalAppearanceStream(annot);
  if (!normalAppearance) {
    return undefined;
  }
  return readRect(normalAppearance.dict.get(PDFName.of('BBox')));
}

function readRect(value: unknown): { x: number; y: number; width: number; height: number } | undefined {
  if (!(value instanceof PDFArray) || value.size() < 4) {
    return undefined;
  }

  const x1 = readNumber(value.get(0));
  const y1 = readNumber(value.get(1));
  const x2 = readNumber(value.get(2));
  const y2 = readNumber(value.get(3));
  return normalizePdfRect([x1, y1, x2, y2]);
}

function readPointArray(value: unknown): readonly PdfPoint[] {
  if (!(value instanceof PDFArray) || value.size() < 4) {
    return [];
  }

  const numbers: number[] = [];
  for (let index = 0; index < value.size(); index += 1) {
    numbers.push(readNumber(value.get(index)));
  }
  return pointArrayToPdfPoints(numbers);
}

function readInkList(value: unknown): readonly (readonly PdfPoint[])[] {
  if (!(value instanceof PDFArray)) {
    return [];
  }

  const paths: PdfPoint[][] = [];
  for (let index = 0; index < value.size(); index += 1) {
    const pathArray = (value as unknown as { lookup(index: number): unknown }).lookup(index);
    if (!(pathArray instanceof PDFArray) || pathArray.size() < 4) {
      continue;
    }
    const numbers: number[] = [];
    for (let pointIndex = 0; pointIndex < pathArray.size(); pointIndex += 1) {
      numbers.push(readNumber(pathArray.get(pointIndex)));
    }
    const path = pointArrayToPdfPoints(numbers);
    if (path.length >= 2) {
      paths.push([...path]);
    }
  }
  return paths;
}

function readPdfJsInkLists(value: readonly (readonly number[])[] | undefined, fallbackRect: readonly number[] | undefined): readonly (readonly PdfPoint[])[] {
  const paths = value
    ?.map((path) => pointArrayToPdfPoints(path))
    .filter((path) => path.length >= 2);
  if (paths && paths.length > 0) {
    return paths;
  }
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = fallbackRect ?? [];
  return [[pdfPoint(x1, y1), pdfPoint(x2, y2)]];
}

function readName(value: unknown): string {
  return String(value ?? '').replace(/^\//, '');
}

function readText(value: unknown): string | undefined {
  if (value instanceof PDFString || value instanceof PDFHexString) {
    return value.decodeText();
  }
  if (typeof value === 'string') {
    return value;
  }
  return undefined;
}

function readFreeTextAppearanceLines(annot: PDFDict): readonly string[] | undefined {
  const normalAppearance = getNormalAppearanceStream(annot);
  if (!normalAppearance) {
    return undefined;
  }

  const stream = decodePDFRawStream(normalAppearance);
  const content = Buffer.from(stream.decode()).toString('latin1');
  const lines = extractPdfTextShowStrings(content);
  return lines.length > 0 ? lines : undefined;
}

function getNormalAppearanceStream(annot: PDFDict): PDFRawStream | undefined {
  const appearance = annot.context.lookup(annot.get(PDFName.of('AP')));
  if (!isPdfDict(appearance)) {
    return undefined;
  }
  const normalAppearance = annot.context.lookup(appearance.get(PDFName.of('N')));
  return normalAppearance instanceof PDFRawStream ? normalAppearance : undefined;
}

function extractPdfTextShowStrings(content: string): readonly string[] {
  const positionedLines = extractPositionedPdfTextShowStrings(content);
  if (positionedLines.length > 0) {
    return positionedLines;
  }

  const lines: string[] = [];
  for (let index = 0; index < content.length; index += 1) {
    const char = content[index];
    if (char === '(') {
      const parsed = readPdfLiteralString(content, index);
      if (parsed && isFollowedByTextShow(content, parsed.nextIndex)) {
        lines.push(parsed.text);
      }
      index = parsed?.nextIndex ?? index;
      continue;
    }

    if (char === '[') {
      const parsed = readPdfTextArray(content, index);
      if (parsed && isFollowedByTextShowArray(content, parsed.nextIndex)) {
        lines.push(parsed.parts.join(''));
      }
      index = parsed?.nextIndex ?? index;
    }
  }
  return lines;
}

function extractPositionedPdfTextShowStrings(content: string): readonly string[] {
  const fragments: Array<{ y: number; text: string }> = [];
  const pattern = /1\s+0\s+0\s+1\s+(-?[0-9.]+)\s+(-?[0-9.]+)\s+Tm\s+\(/g;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(content)) !== null) {
    const literalStart = pattern.lastIndex - 1;
    const parsed = readPdfLiteralString(content, literalStart);
    if (!parsed || !isFollowedByTextShow(content, parsed.nextIndex)) {
      continue;
    }
    fragments.push({
      y: Number(match[2]),
      text: parsed.text,
    });
    pattern.lastIndex = parsed.nextIndex;
  }

  if (fragments.length === 0) {
    return [];
  }

  const lines: string[] = [];
  let currentY: number | undefined;
  let currentLine = '';
  for (const fragment of fragments) {
    if (currentY === undefined || Math.abs(fragment.y - currentY) > 0.01) {
      if (currentY !== undefined) {
        lines.push(currentLine);
      }
      currentY = fragment.y;
      currentLine = fragment.text;
    } else {
      currentLine += fragment.text;
    }
  }
  lines.push(currentLine);
  return lines;
}

function readPdfTextArray(content: string, startIndex: number): { parts: string[]; nextIndex: number } | undefined {
  const parts: string[] = [];
  let index = startIndex + 1;
  while (index < content.length) {
    const char = content[index];
    if (char === ']') {
      return { parts, nextIndex: index + 1 };
    }
    if (char === '(') {
      const parsed = readPdfLiteralString(content, index);
      if (!parsed) {
        return undefined;
      }
      parts.push(parsed.text);
      index = parsed.nextIndex;
      continue;
    }
    index += 1;
  }
  return undefined;
}

function readPdfLiteralString(content: string, startIndex: number): { text: string; nextIndex: number } | undefined {
  let text = '';
  let depth = 1;
  for (let index = startIndex + 1; index < content.length; index += 1) {
    const char = content[index];
    if (char === '\\') {
      const escaped = readPdfEscape(content, index);
      text += escaped.text;
      index = escaped.nextIndex - 1;
      continue;
    }
    if (char === '(') {
      depth += 1;
      text += char;
      continue;
    }
    if (char === ')') {
      depth -= 1;
      if (depth === 0) {
        return { text, nextIndex: index + 1 };
      }
      text += char;
      continue;
    }
    text += char;
  }
  return undefined;
}

function readPdfEscape(content: string, slashIndex: number): { text: string; nextIndex: number } {
  const next = content[slashIndex + 1];
  if (next === undefined) {
    return { text: '', nextIndex: slashIndex + 1 };
  }
  if (next === 'n') return { text: '\n', nextIndex: slashIndex + 2 };
  if (next === 'r') return { text: '\r', nextIndex: slashIndex + 2 };
  if (next === 't') return { text: '\t', nextIndex: slashIndex + 2 };
  if (next === 'b') return { text: '\b', nextIndex: slashIndex + 2 };
  if (next === 'f') return { text: '\f', nextIndex: slashIndex + 2 };
  if (next === '\r' || next === '\n') {
    const hasCrLf = next === '\r' && content[slashIndex + 2] === '\n';
    return { text: '', nextIndex: slashIndex + (hasCrLf ? 3 : 2) };
  }
  if (/[0-7]/.test(next)) {
    const match = content.slice(slashIndex + 1, slashIndex + 4).match(/^[0-7]{1,3}/);
    const octal = match?.[0] ?? next;
    return { text: String.fromCharCode(Number.parseInt(octal, 8)), nextIndex: slashIndex + 1 + octal.length };
  }
  return { text: next, nextIndex: slashIndex + 2 };
}

function isFollowedByTextShow(content: string, index: number): boolean {
  return content.slice(index).match(/^\s*Tj\b/) !== null;
}

function isFollowedByTextShowArray(content: string, index: number): boolean {
  return content.slice(index).match(/^\s*TJ\b/) !== null;
}

function readColor(value: unknown): string | undefined {
  const text = readText(value);
  if (!text) {
    return undefined;
  }
  const match = text.match(/([0-9.]+)\s+([0-9.]+)\s+([0-9.]+)\s+rg\b/);
  if (!match) {
    return undefined;
  }
  return rgbToHex(Number(match[1]), Number(match[2]), Number(match[3]));
}

function readFontSize(value: unknown): number | undefined {
  const text = readText(value);
  if (!text) {
    return undefined;
  }
  const match = text.match(/\/[A-Za-z0-9]+\s+([0-9.]+)\s+Tf\b/);
  return match ? Number(match[1]) : undefined;
}

function readLineHeight(value: unknown): number | undefined {
  const text = readText(value);
  if (!text) {
    return undefined;
  }
  const match = text.match(/line-height\s*:\s*([0-9.]+)pt/i);
  return match ? Number(match[1]) : undefined;
}

function readTextAlign(qValue: unknown, dsValue: unknown): 'left' | 'center' | 'right' | undefined {
  const q = readOptionalNumber(qValue);
  if (q === 1) {
    return 'center';
  }
  if (q === 2) {
    return 'right';
  }
  if (q === 0) {
    return 'left';
  }

  const ds = readText(dsValue);
  const match = ds?.match(/text-align\s*:\s*(left|center|right)/i);
  return match ? match[1].toLowerCase() as 'left' | 'center' | 'right' : undefined;
}

function readFontFamily(dsValue: unknown, daValue: unknown): 'Helvetica' | 'ArialUnicode' {
  const ds = readText(dsValue);
  if (ds?.toLowerCase().includes('arial unicode')) {
    return 'ArialUnicode';
  }
  const da = readText(daValue);
  return da?.includes('/BPUnicode') ? 'ArialUnicode' : 'Helvetica';
}

function readPdfAnnotationAppearance(annot: PDFDict): MarkupAppearance {
  const storedAppearance = readText(annot.get(PDFName.of('BPAppearance')));
  if (storedAppearance) {
    try {
      const parsed = JSON.parse(storedAppearance) as unknown;
      if (parsed && typeof parsed === 'object') {
        return parsed as MarkupAppearance;
      }
    } catch {
      // Fall through to standard PDF appearance fields for malformed or external data.
    }
  }
  const da = annot.get(PDFName.of('DA'));
  const ds = annot.get(PDFName.of('DS'));
  const strokeColor = readColorArray(annot.get(PDFName.of('C')));
  const fillColor = readColorArray(annot.get(PDFName.of('IC')));
  const textColor = readColor(da);
  const fontSizePt = readFontSize(da);
  const lineHeightPt = readLineHeight(ds);
  const align = readTextAlign(annot.get(PDFName.of('Q')), ds);
  const hasTextAppearance = Boolean(textColor || fontSizePt || lineHeightPt || align || readText(ds) || readText(da));
  const opacity = readOptionalNumber(annot.get(PDFName.of('CA')))
    ?? readOptionalNumber(annot.get(PDFName.of('ca')));
  const blendMode = readName(annot.get(PDFName.of('BM'))).toLowerCase() === 'multiply' ? 'multiply' as const : undefined;
  return {
    ...(strokeColor || readBorderWidth(annot) !== undefined ? {
      stroke: {
        ...(strokeColor ? { color: strokeColor } : {}),
        ...(readBorderWidth(annot) !== undefined ? { widthPt: readBorderWidth(annot) } : {}),
      },
    } : {}),
    ...(annot.get(PDFName.of('IC')) ? { fill: { color: fillColor ?? null } } : {}),
    ...(hasTextAppearance ? {
      text: {
        ...(textColor ? { color: textColor } : {}),
        ...(readText(ds) || readText(da) ? { fontId: readFontFamily(ds, da) } : {}),
        ...(fontSizePt !== undefined ? { fontSizePt } : {}),
        ...(lineHeightPt !== undefined ? { lineHeightPt } : {}),
        ...(align ? { align } : {}),
      },
    } : {}),
    ...(opacity !== undefined ? { opacity } : {}),
    ...(blendMode ? { blendMode } : {}),
  };
}

function readColorArray(value: unknown): string | undefined {
  if (!(value instanceof PDFArray) || value.size() < 3) {
    return undefined;
  }
  return rgbToHex(readNumber(value.get(0)), readNumber(value.get(1)), readNumber(value.get(2)));
}

function rgbToHex(red: number, green: number, blue: number): string {
  return `#${[red, green, blue].map((value) => {
    const channel = value <= 1 ? value * 255 : value;
    return Math.max(0, Math.min(255, Math.round(channel))).toString(16).padStart(2, '0');
  }).join('')}`;
}

function readBorderWidth(annot: PDFDict): number | undefined {
  const borderStyle = annot.context.lookup(annot.get(PDFName.of('BS')));
  if (isPdfDict(borderStyle)) {
    return readOptionalNumber(borderStyle.get(PDFName.of('W')));
  }
  const border = annot.context.lookup(annot.get(PDFName.of('Border')));
  if (border instanceof PDFArray && border.size() >= 3) {
    return readOptionalNumber(border.get(2));
  }
  return undefined;
}

function readBorderEffectIntensity(value: unknown): number | undefined {
  const borderEffect = value && typeof value === 'object' && 'get' in value
    ? (value as { get(key: PDFName): unknown })
    : undefined;
  const intensity = borderEffect?.get(PDFName.of('I'));
  return readOptionalNumber(intensity);
}

function readCloudAppearancePath(annot: PDFDict): string | undefined {
  const stream = getNormalAppearanceStream(annot);
  if (!stream) {
    return undefined;
  }
  return pdfAppearancePathToSvgPath(Buffer.from(decodePDFRawStream(stream).decode()).toString('latin1'));
}

function readCloudScallopRadiusFromAppearance(annot: PDFDict, points: readonly PdfPoint[]): number | undefined {
  const pathBounds = readCloudAppearanceBounds(annot);
  if (!pathBounds || points.length === 0) {
    return undefined;
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const paddings = [
    Math.min(...xs) - pathBounds.minX,
    Math.min(...ys) - pathBounds.minY,
    pathBounds.maxX - Math.max(...xs),
    pathBounds.maxY - Math.max(...ys),
  ].filter((value) => Number.isFinite(value) && value > 0);
  if (paddings.length === 0) {
    return undefined;
  }
  const sorted = [...paddings].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] / 0.6831;
}

function readCloudAppearanceBounds(annot: PDFDict): { minX: number; minY: number; maxX: number; maxY: number } | undefined {
  const stream = getNormalAppearanceStream(annot);
  if (!stream) {
    return undefined;
  }
  const commands = parsePdfAppearancePathCommands(Buffer.from(decodePDFRawStream(stream).decode()).toString('latin1'));
  const points = commands.flatMap((command) => {
    const commandPoints: PdfPoint[] = [];
    for (let index = 0; index < command.args.length - 1; index += 2) {
      commandPoints.push(pdfPoint(command.args[index], command.args[index + 1]));
    }
    return commandPoints;
  });
  if (points.length === 0) {
    return undefined;
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  return {
    minX: Math.min(...xs),
    minY: Math.min(...ys),
    maxX: Math.max(...xs),
    maxY: Math.max(...ys),
  };
}

function readScallopRadiusFromRect(rectValue: readonly number[] | undefined, points: readonly PdfPoint[]): number | undefined {
  if (!rectValue || points.length === 0) {
    return undefined;
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const [rectMinX, rectMinY, rectMaxX, rectMaxY] = rectValue;
  const paddings = [
    Math.min(...xs) - rectMinX,
    Math.min(...ys) - rectMinY,
    rectMaxX - Math.max(...xs),
    rectMaxY - Math.max(...ys),
  ].filter((value) => Number.isFinite(value) && value > 0);
  if (paddings.length === 0) {
    return undefined;
  }
  const sorted = [...paddings].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] / 0.6831;
}

function pdfAppearancePathToSvgPath(content: string): string | undefined {
  const commands = parsePdfAppearancePathCommands(content);
  const pathCommands = commands.flatMap((command) => {
    if (command.operator === 'm' && command.args.length >= 2) {
      return [`M ${formatPdfNumber(command.args[0])} ${formatPdfNumber(command.args[1])}`];
    }
    if (command.operator === 'l' && command.args.length >= 2) {
      return [`L ${formatPdfNumber(command.args[0])} ${formatPdfNumber(command.args[1])}`];
    }
    if (command.operator === 'c' && command.args.length >= 6) {
      return [`C ${command.args.map(formatPdfNumber).join(' ')}`];
    }
    if (command.operator === 'h') {
      return ['Z'];
    }
    return [];
  });
  return pathCommands.length > 0 ? pathCommands.join(' ') : undefined;
}

function parsePdfAppearancePathCommands(content: string): Array<{ operator: string; args: number[] }> {
  const tokens = content.match(/\/?[A-Za-z*]+|-?\d*\.?\d+(?:[eE][+-]?\d+)?/g) ?? [];
  const stack: number[] = [];
  const commands: Array<{ operator: string; args: number[] }> = [];
  const arity: Record<string, number> = { m: 2, l: 2, c: 6, v: 4, y: 4, re: 4 };
  const pathOperators = new Set(['m', 'l', 'c', 'v', 'y', 're', 'h', 'S', 's', 'b', 'B', 'n']);
  for (const token of tokens) {
    const number = Number(token);
    if (Number.isFinite(number) && token !== '') {
      stack.push(number);
      continue;
    }
    if (!pathOperators.has(token)) {
      stack.length = 0;
      continue;
    }
    const count = arity[token] ?? 0;
    const args = count > 0 ? stack.splice(Math.max(0, stack.length - count), count) : [];
    commands.push({ operator: token, args });
    stack.length = 0;
  }
  return commands;
}

function readOptionalNumber(value: unknown): number | undefined {
  return value && typeof (value as { asNumber?: unknown }).asNumber === 'function'
    ? (value as { asNumber(): number }).asNumber()
    : undefined;
}

function readNumber(value: unknown): number {
  return (value as { asNumber(): number }).asNumber();
}

function toManagedAnnotationId(id: string): string {
  return `${managedAnnotationPrefix}${id}`;
}

function fromManagedAnnotationId(value: string | undefined): string | undefined {
  if (!value) {
    return undefined;
  }

  return value.startsWith(managedAnnotationPrefix)
    ? value.slice(managedAnnotationPrefix.length)
    : undefined;
}

async function loadPdfJsModule(): Promise<any> {
  await ensurePdfJsGlobals();
  pdfjsModulePromise ??= import('pdfjs-dist/legacy/build/pdf.mjs');
  return pdfjsModulePromise;
}

async function ensurePdfJsGlobals(): Promise<void> {
  if (pdfjsGlobalsReady) {
    return;
  }

  const canvasModule = await import('@napi-rs/canvas');
  if (typeof globalThis.DOMMatrix === 'undefined' && canvasModule.DOMMatrix) {
    globalThis.DOMMatrix = canvasModule.DOMMatrix as never;
  }
  if (typeof globalThis.ImageData === 'undefined' && canvasModule.ImageData) {
    globalThis.ImageData = canvasModule.ImageData as never;
  }
  if (typeof globalThis.Path2D === 'undefined' && canvasModule.Path2D) {
    globalThis.Path2D = canvasModule.Path2D as never;
  }

  pdfjsGlobalsReady = true;
}
