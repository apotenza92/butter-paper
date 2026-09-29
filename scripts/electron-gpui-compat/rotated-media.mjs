#!/usr/bin/env node
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { isAbsolute, join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { tmpdir } from "node:os";

const repositoryRoot = resolve(import.meta.dirname, "../..");
const nativeRoot = join(
  repositoryRoot,
  "experiments/gpui-migration/gpui-migration",
);
const referenceEntries = [
  "package.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "tsconfig.base.json",
  "packages/core/package.json",
  "packages/core/tsconfig.json",
  "packages/core/tsconfig.build.json",
  "packages/core/src",
  "packages/pdf/package.json",
  "packages/pdf/tsconfig.json",
  "packages/pdf/tsconfig.build.json",
  "packages/pdf/src",
];

const stableEditorSource = String.raw`import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { openPdfDocument } from './dist/index.js';

const [seed, first, final] = process.argv.slice(2);
assert(seed && first && final, 'usage: stable-rotated-media.mjs SEED FIRST FINAL');
const ids = new Set(['native-rotated-image', 'native-rotated-snapshot']);

async function rewrite(source, target, translate) {
  const handle = await openPdfDocument(source);
  try {
    const markups = await handle.annotations.readPageAnnotations(0);
    const found = markups.filter((markup) => ids.has(markup.id));
    assert.equal(found.length, 2);
    assert.deepEqual(found.map((markup) => markup.kind).sort(), ['image', 'snapshot']);
    await handle.writer.save(
      handle,
      markups.map((markup) =>
        ids.has(markup.id) && translate
          ? { ...markup, rect: { ...markup.rect, x: markup.rect.x + 12 } }
          : markup,
      ),
      'saveAs',
      target,
    );
  } finally {
    await handle.close();
  }
}

await rewrite(seed, first, true);
await rewrite(first, final, false);
const handle = await openPdfDocument(final);
try {
  const markups = (await handle.annotations.readPageAnnotations(0))
    .filter((markup) => ids.has(markup.id))
    .map((markup) => ({
      id: markup.id,
      kind: markup.kind,
      rect: markup.rect,
      rotation: markup.rotation,
      opacity: markup.opacity ?? markup.appearance?.opacity,
      locked: Boolean(markup.locked),
      aspectRatioLocked: Boolean(markup.aspectRatioLocked),
      mimeType: markup.mimeType,
      dataSha256: markup.dataUrl
        ? createHash('sha256')
            .update(Buffer.from(markup.dataUrl.split(',').at(-1) ?? '', 'base64'))
            .digest('hex')
        : undefined,
    }));
  process.stdout.write(JSON.stringify(markups));
} finally {
  await handle.close();
}
`;

const stableEllipseEditorSource = String.raw`import assert from 'node:assert/strict';
import { openPdfDocument } from './dist/index.js';

const [seed, first, final] = process.argv.slice(2);
assert(seed && first && final, 'usage: stable-rotated-ellipse.mjs SEED FIRST FINAL');
const ids = new Set(['native-rotated-ellipse', 'legacy-ellipse']);

async function rewrite(source, target, translate) {
  const handle = await openPdfDocument(source);
  try {
    const markups = await handle.annotations.readPageAnnotations(0);
    const found = markups.filter((markup) => ids.has(markup.id));
    assert.equal(found.length, 2);
    assert(found.every((markup) => markup.kind === 'ellipse'));
    await handle.writer.save(
      handle,
      markups.map((markup) =>
        markup.id === 'native-rotated-ellipse' && translate
          ? { ...markup, rect: { ...markup.rect, x: markup.rect.x + 12 } }
          : markup,
      ),
      'saveAs',
      target,
    );
  } finally {
    await handle.close();
  }
}

await rewrite(seed, first, true);
await rewrite(first, final, false);
async function inspect(path) {
  const handle = await openPdfDocument(path);
  try {
    return (await handle.annotations.readPageAnnotations(0))
      .filter((markup) => ids.has(markup.id))
      .map((markup) => ({
        id: markup.id,
        kind: markup.kind,
        rect: markup.rect,
        rotation: markup.rotation,
        opacity: markup.opacity ?? markup.appearance?.opacity,
        locked: Boolean(markup.locked),
        stroke: markup.appearance?.stroke,
        fill: markup.appearance?.fill,
      }));
  } finally {
    await handle.close();
  }
}
process.stdout.write(JSON.stringify({ initial: await inspect(seed), final: await inspect(final) }));
`;

const stableInkEditorSource = String.raw`import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { PDFArray, PDFDict, PDFDocument, PDFName } from 'pdf-lib';
import { openPdfDocument } from './dist/index.js';

const [seed, first, final] = process.argv.slice(2);
assert(seed && first && final, 'usage: stable-ink.mjs SEED FIRST FINAL');
const ids = new Set(['compat-highlight', 'compat-pen']);

async function rewrite(source, target, translate) {
  const handle = await openPdfDocument(source);
  try {
    const markups = await handle.annotations.readPageAnnotations(0);
    const found = markups.filter((markup) => ids.has(markup.id));
    assert.deepEqual(found.map(({ id, kind }) => ({ id, kind })), [
      { id: 'compat-highlight', kind: 'highlight' },
      { id: 'compat-pen', kind: 'pen' },
    ]);
    await handle.writer.save(
      handle,
      markups.map((markup) =>
        ids.has(markup.id) && translate
          ? {
              ...markup,
              paths: markup.paths.map((path, pathIndex) =>
                path.map((point) => ({
                  ...point,
                  x: point.x + (markup.id === 'compat-highlight' && pathIndex === 0 ? 0 : 12),
                })),
              ),
            }
          : markup,
      ),
      'saveAs',
      target,
    );
  } finally {
    await handle.close();
  }
}

async function rawFlags(path) {
  const document = await PDFDocument.load(await readFile(path));
  const annotations = document.context.lookup(document.getPage(0).node.Annots(), PDFArray);
  const flags = new Map();
  for (const reference of annotations.asArray()) {
    const annotation = document.context.lookup(reference);
    if (!(annotation instanceof PDFDict)) continue;
    const rawName = annotation.get(PDFName.of('NM'))?.decodeText?.();
    const id = rawName?.startsWith('bp:') ? rawName.slice(3) : rawName;
    if (!id || !ids.has(id)) continue;
    flags.set(id, {
      hasAppearance: annotation.has(PDFName.of('AP')),
      hasCanonicalPointBits: annotation.has(PDFName.of('BPCanonicalPointBits')),
    });
  }
  return flags;
}

async function inspect(path) {
  const flags = await rawFlags(path);
  const handle = await openPdfDocument(path);
  try {
    return (await handle.annotations.readPageAnnotations(0))
      .filter((markup) => ids.has(markup.id))
      .map((markup) => ({
        id: markup.id,
        kind: markup.kind,
        paths: markup.paths,
        color: markup.appearance?.stroke?.color ?? markup.color,
        widthPt: markup.appearance?.stroke?.widthPt ?? markup.strokeWidth,
        opacity: markup.appearance?.opacity ?? markup.opacity,
        locked: Boolean(markup.locked),
        blendMode: markup.appearance?.blendMode ?? markup.blendMode,
        smoothCurves: markup.smoothCurves,
        ...flags.get(markup.id),
      }));
  } finally {
    await handle.close();
  }
}

const initial = await inspect(seed);
await rewrite(seed, first, true);
await rewrite(first, final, false);
process.stdout.write(JSON.stringify({ initial, final: await inspect(final) }));
`;

const stableRedactEditorSource = String.raw`import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { decodePDFRawStream, PDFArray, PDFDict, PDFDocument, PDFName, PDFRawStream } from 'pdf-lib';
import { openPdfDocument } from './dist/index.js';

const [seed, first, final] = process.argv.slice(2);
assert(seed && first && final, 'usage: stable-redact.mjs SEED FIRST FINAL');
const id = 'compat-redact';

async function rewrite(source, target, translate) {
  const handle = await openPdfDocument(source);
  try {
    const markups = await handle.annotations.readPageAnnotations(0);
    const found = markups.filter((markup) => markup.id === id);
    assert.deepEqual(found.map(({ id, kind }) => ({ id, kind })), [{ id, kind: 'redact' }]);
    await handle.writer.save(
      handle,
      markups.map((markup) =>
        markup.id === id && translate
          ? { ...markup, rect: { ...markup.rect, x: markup.rect.x + 12 } }
          : markup,
      ),
      'saveAs',
      target,
    );
  } finally {
    await handle.close();
  }
}

function text(value) {
  return value?.decodeText?.();
}

function numbers(document, value) {
  const array = document.context.lookup(value);
  if (!(array instanceof PDFArray)) return [];
  return array.asArray().map((item) => item.asNumber());
}

async function inspect(path) {
  const document = await PDFDocument.load(await readFile(path));
  const page = document.getPage(0);
  const contents = document.context.lookup(page.node.Contents());
  const entries = contents instanceof PDFArray ? contents.asArray() : contents ? [contents] : [];
  assert(entries.length > 0, 'covered page content is missing');
  const contentHash = createHash('sha256');
  for (const entry of entries) {
    const stream = document.context.lookup(entry);
    assert(stream instanceof PDFRawStream, 'page content must remain a raw stream');
    contentHash.update(decodePDFRawStream(stream).decode()).update('\0');
  }
  const annotations = document.context.lookup(page.node.Annots());
  assert(annotations instanceof PDFArray, 'pending Redact annotation array is missing');
  const raw = annotations.asArray()
    .map((reference) => document.context.lookup(reference))
    .filter((annotation) => annotation instanceof PDFDict)
    .find((annotation) => {
      const rawName = text(annotation.get(PDFName.of('NM')));
      return rawName === id || rawName === 'bp:' + id;
    });
  assert(raw instanceof PDFDict, 'pending Redact dictionary is missing');
  const handle = await openPdfDocument(path);
  try {
    const markups = (await handle.annotations.readPageAnnotations(0))
      .filter((markup) => markup.id === id)
      .map((markup) => ({
        id: markup.id,
        kind: markup.kind,
        rect: markup.rect,
        redactionColor: markup.redactionColor,
        overlayText: markup.overlayText,
        locked: Boolean(markup.locked),
      }));
    return {
      markups,
      contentSha256: contentHash.digest('hex'),
      raw: {
        subtype: String(raw.get(PDFName.of('Subtype'))),
        subject: text(raw.get(PDFName.of('Subj'))),
        contents: text(raw.get(PDFName.of('Contents'))),
        rect: numbers(document, raw.get(PDFName.of('Rect'))),
        quadPoints: numbers(document, raw.get(PDFName.of('QuadPoints'))),
        interiorColor: numbers(document, raw.get(PDFName.of('IC'))),
        overlayText: text(raw.get(PDFName.of('OverlayText'))),
        flags: raw.get(PDFName.of('F'))?.asNumber?.(),
        hasAppearance: raw.has(PDFName.of('AP')),
      },
    };
  } finally {
    await handle.close();
  }
}

const initial = await inspect(seed);
await rewrite(seed, first, true);
await rewrite(first, final, false);
process.stdout.write(JSON.stringify({ initial, final: await inspect(final) }));
`;

const stableRichTextEditorSource = String.raw`import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { decodePDFRawStream, PDFArray, PDFDict, PDFDocument, PDFName, PDFRawStream } from 'pdf-lib';
import { openPdfDocument } from './dist/index.js';

const [seed, first, final] = process.argv.slice(2);
assert(seed && first && final, 'usage: stable-rich-text.mjs SEED FIRST FINAL');
const id = 'native-rich-text';

async function rewrite(source, target, translate) {
  const handle = await openPdfDocument(source);
  try {
    const markups = await handle.annotations.readPageAnnotations(0);
    const found = markups.filter((markup) => markup.id === id);
    assert.deepEqual(found.map(({ id, kind }) => ({ id, kind })), [{ id, kind: 'text-box' }]);
    await handle.writer.save(
      handle,
      markups.map((markup) =>
        markup.id === id && translate
          ? { ...markup, rect: { ...markup.rect, x: markup.rect.x + 12 } }
          : markup,
      ),
      'saveAs',
      target,
    );
  } finally {
    await handle.close();
  }
}

function text(value) {
  return value?.decodeText?.();
}

function numbers(document, value) {
  const array = document.context.lookup(value);
  if (!(array instanceof PDFArray)) return [];
  return array.asArray().map((item) => item.asNumber());
}

function fontNames(document, resourcesValue) {
  const resources = document.context.lookup(resourcesValue);
  const fonts = resources instanceof PDFDict ? document.context.lookup(resources.get(PDFName.of('Font'))) : undefined;
  return fonts instanceof PDFDict
    ? [...fonts.keys()].map((name) => String(name).replace(/^\//, '')).sort()
    : [];
}

function fontObjects(document, resourcesValue) {
  const resources = document.context.lookup(resourcesValue);
  const fonts = resources instanceof PDFDict ? document.context.lookup(resources.get(PDFName.of('Font'))) : undefined;
  if (!(fonts instanceof PDFDict)) return [];
  return [...fonts.entries()].map(([name, value]) => {
    const font = document.context.lookup(value);
    assert(font instanceof PDFDict, 'rich Text Box font resource is not a dictionary');
    const descendants = document.context.lookup(font.get(PDFName.of('DescendantFonts')));
    const descendant = descendants instanceof PDFArray && descendants.size() === 1
      ? document.context.lookup(descendants.get(0))
      : undefined;
    const descriptorOwner = descendant instanceof PDFDict ? descendant : font;
    const descriptor = document.context.lookup(descriptorOwner.get(PDFName.of('FontDescriptor')));
    const program = descriptor instanceof PDFDict
      ? ['FontFile', 'FontFile2', 'FontFile3']
          .map((key) => document.context.lookup(descriptor.get(PDFName.of(key))))
          .find((candidate) => candidate instanceof PDFRawStream)
      : undefined;
    return {
      name: String(name).replace(/^\//, ''),
      object: String(value),
      subtype: String(font.get(PDFName.of('Subtype'))),
      baseFont: String(font.get(PDFName.of('BaseFont'))),
      embedded: program instanceof PDFRawStream,
      programSha256: program instanceof PDFRawStream
        ? createHash('sha256').update(program.contents).digest('hex')
        : undefined,
    };
  }).sort((left, right) => left.name.localeCompare(right.name));
}

function appearanceTextBoundary(content) {
  const fontSelections = [];
  for (const match of content.matchAll(/\/([A-Za-z0-9]+)\s+([0-9]+(?:\.[0-9]+)?)\s+Tf\b/g)) {
    const selection = match[1] + '@' + Number(match[2]);
    if (fontSelections.at(-1) !== selection) fontSelections.push(selection);
  }
  return {
    beginText: (content.match(/(?:^|\s)BT(?:\s|$)/g) ?? []).length,
    endText: (content.match(/(?:^|\s)ET(?:\s|$)/g) ?? []).length,
    textPaints: (content.match(/(?:^|\s)(?:Tj|TJ)(?:\s|$)/g) ?? []).length,
    fontSelections,
  };
}

async function inspect(path) {
  const document = await PDFDocument.load(await readFile(path));
  const page = document.getPage(0);
  const contents = document.context.lookup(page.node.Contents());
  const entries = contents instanceof PDFArray ? contents.asArray() : contents ? [contents] : [];
  assert(entries.length > 0, 'covered page content is missing');
  const contentHash = createHash('sha256');
  for (const entry of entries) {
    const stream = document.context.lookup(entry);
    assert(stream instanceof PDFRawStream, 'page content must remain a raw stream');
    contentHash.update(decodePDFRawStream(stream).decode()).update('\0');
  }
  const annotations = document.context.lookup(page.node.Annots());
  assert(annotations instanceof PDFArray, 'rich Text Box annotation array is missing');
  const raw = annotations.asArray()
    .map((reference) => document.context.lookup(reference))
    .filter((annotation) => annotation instanceof PDFDict)
    .find((annotation) => {
      const rawName = text(annotation.get(PDFName.of('NM')));
      return rawName === id || rawName === 'bp:' + id;
    });
  assert(raw instanceof PDFDict, 'rich Text Box dictionary is missing');
  const appearance = document.context.lookup(raw.get(PDFName.of('AP')));
  const normal = appearance instanceof PDFDict
    ? document.context.lookup(appearance.get(PDFName.of('N')))
    : undefined;
  assert(normal instanceof PDFRawStream, 'rich Text Box normal appearance is missing');
  const appearanceBytes = decodePDFRawStream(normal).decode();
  const appearanceContent = Buffer.from(appearanceBytes).toString('latin1');
  const handle = await openPdfDocument(path);
  try {
    const markups = (await handle.annotations.readPageAnnotations(0))
      .filter((markup) => markup.id === id)
      .map((markup) => ({
        id: markup.id,
        kind: markup.kind,
        rect: markup.rect,
        text: markup.text,
        fontFamily: markup.fontFamily,
        fontSizePt: markup.fontSizePt,
        opacity: markup.appearance?.opacity ?? markup.opacity,
        locked: Boolean(markup.locked),
        richTextRuns: markup.richTextRuns,
      }));
    return {
      markups,
      contentSha256: contentHash.digest('hex'),
      raw: {
        subtype: String(raw.get(PDFName.of('Subtype'))),
        subject: text(raw.get(PDFName.of('Subj'))),
        contents: text(raw.get(PDFName.of('Contents'))),
        rect: numbers(document, raw.get(PDFName.of('Rect'))),
        richContent: text(raw.get(PDFName.of('RC'))),
        defaultAppearance: text(raw.get(PDFName.of('DA'))),
        defaultStyle: text(raw.get(PDFName.of('DS'))),
        flags: raw.get(PDFName.of('F'))?.asNumber?.(),
        opacity: raw.get(PDFName.of('CA'))?.asNumber?.(),
        defaultFonts: fontNames(document, raw.get(PDFName.of('DR'))),
        appearanceFonts: fontNames(document, normal.dict.get(PDFName.of('Resources'))),
        defaultFontObjects: fontObjects(document, raw.get(PDFName.of('DR'))),
        appearanceFontObjects: fontObjects(document, normal.dict.get(PDFName.of('Resources'))),
        indirectObjectCount: [...document.context.enumerateIndirectObjects()].length,
        appearanceBounds: numbers(document, normal.dict.get(PDFName.of('BBox'))),
        appearanceTextBoundary: appearanceTextBoundary(appearanceContent),
        appearanceBytes: appearanceBytes.length,
        appearanceSha256: createHash('sha256').update(appearanceBytes).digest('hex'),
      },
    };
  } finally {
    await handle.close();
  }
}

const initial = await inspect(seed);
await rewrite(seed, first, true);
await rewrite(first, final, false);
process.stdout.write(JSON.stringify({ initial, final: await inspect(final) }));
`;

const stableCoordinateEditorSource = String.raw`import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { decodePDFRawStream, PDFArray, PDFDict, PDFDocument, PDFName, PDFRawStream } from 'pdf-lib';
import { openPdfDocument } from './dist/index.js';

const [seed, first, final] = process.argv.slice(2);
assert(seed && first && final, 'usage: stable-coordinate-space.mjs SEED FIRST FINAL');
const ids = new Set(['coordinate-rectangle', 'coordinate-length']);
const pageScale = {
  pageIndex: 0,
  source: 'calibrated',
  name: 'Coordinate 1 m',
  pdfUnits: 'in',
  realUnits: 'm',
  scaleX: 0.01,
  scaleY: 0.01,
  precision: { mode: 'decimal', value: 0.01 },
};

async function rewrite(source, target, translate) {
  const handle = await openPdfDocument(source);
  try {
    const markups = await handle.annotations.readPageAnnotations(0);
    const found = markups.filter((markup) => ids.has(markup.id));
    assert.deepEqual(found.map(({ id, kind }) => ({ id, kind })), [
      { id: 'coordinate-rectangle', kind: 'rectangle' },
      { id: 'coordinate-length', kind: 'length' },
    ]);
    await handle.writer.save(
      handle,
      markups.map((markup) => {
        if (!translate || !ids.has(markup.id)) return markup;
        if (markup.kind === 'rectangle') {
          return { ...markup, rect: { ...markup.rect, x: markup.rect.x + 12 } };
        }
        assert.equal(markup.kind, 'length');
        return {
          ...markup,
          start: { ...markup.start, x: markup.start.x + 12 },
          end: { ...markup.end, x: markup.end.x + 12 },
        };
      }),
      'saveAs',
      target,
      [pageScale],
    );
  } finally {
    await handle.close();
  }
}

function inherited(document, page, key) {
  let dictionary = page.node;
  for (let depth = 0; depth < 32; depth += 1) {
    if (dictionary.has(key)) return dictionary.get(key);
    const parent = dictionary.get(PDFName.of('Parent'));
    if (!parent) return undefined;
    const resolved = document.context.lookup(parent);
    if (!(resolved instanceof PDFDict)) return undefined;
    dictionary = resolved;
  }
  throw new Error('page inheritance depth exceeded');
}

function numbers(document, value) {
  const array = document.context.lookup(value);
  if (!(array instanceof PDFArray)) return [];
  return array.asArray().map((item) => item.asNumber());
}

function text(value) {
  return value?.decodeText?.();
}

async function inspect(path) {
  const bytes = await readFile(path);
  const document = await PDFDocument.load(bytes);
  const page = document.getPage(0);
  const contents = document.context.lookup(page.node.Contents());
  const entries = contents instanceof PDFArray ? contents.asArray() : contents ? [contents] : [];
  assert(entries.length > 0, 'coordinate fixture page content is missing');
  const contentHash = createHash('sha256');
  for (const entry of entries) {
    const stream = document.context.lookup(entry);
    assert(stream instanceof PDFRawStream, 'coordinate fixture page content must remain a raw stream');
    contentHash.update(decodePDFRawStream(stream).decode()).update('\0');
  }
  const annotations = document.context.lookup(page.node.Annots());
  assert(annotations instanceof PDFArray, 'coordinate fixture annotations are missing');
  const raw = annotations.asArray()
    .map((reference) => document.context.lookup(reference))
    .filter((annotation) => annotation instanceof PDFDict);
  const vendor = raw.find((annotation) => text(annotation.get(PDFName.of('NM'))) === 'vendor-coordinate-probe');
  assert(vendor instanceof PDFDict, 'opaque coordinate vendor annotation is missing');
  const length = raw.find((annotation) => {
    const name = text(annotation.get(PDFName.of('NM')));
    return name === 'coordinate-length' || name === 'bp:coordinate-length';
  });
  assert(length instanceof PDFDict, 'coordinate Length dictionary is missing');
  const handle = await openPdfDocument(path);
  try {
    const markups = (await handle.annotations.readPageAnnotations(0))
      .filter((markup) => ids.has(markup.id))
      .map((markup) => markup.kind === 'rectangle'
        ? { id: markup.id, kind: markup.kind, rect: markup.rect, locked: Boolean(markup.locked) }
        : { id: markup.id, kind: markup.kind, start: markup.start, end: markup.end, locked: Boolean(markup.locked) });
    const pageInfo = await handle.getPageInfo(0);
    return {
      markups,
      page: pageInfo,
      pageContentSha256: contentHash.digest('hex'),
      raw: {
        mediaBox: numbers(document, inherited(document, page, PDFName.of('MediaBox'))),
        cropBox: numbers(document, inherited(document, page, PDFName.of('CropBox'))),
        rotation: inherited(document, page, PDFName.of('Rotate'))?.asNumber?.(),
        userUnit: page.node.get(PDFName.of('UserUnit'))?.asNumber?.(),
        pageOwnsMediaBox: page.node.has(PDFName.of('MediaBox')),
        pageOwnsCropBox: page.node.has(PDFName.of('CropBox')),
        pageOwnsRotation: page.node.has(PDFName.of('Rotate')),
        pageScale: JSON.parse(text(page.node.get(PDFName.of('BPPageScale')))),
        lengthLine: numbers(document, length.get(PDFName.of('L'))),
        vendor: {
          subtype: String(vendor.get(PDFName.of('Subtype'))),
          subject: text(vendor.get(PDFName.of('Subj'))),
          contents: text(vendor.get(PDFName.of('Contents'))),
          rect: numbers(document, vendor.get(PDFName.of('Rect'))),
          probe: text(vendor.get(PDFName.of('VendorProbe'))),
        },
      },
    };
  } finally {
    await handle.close();
  }
}

const initial = await inspect(seed);
await rewrite(seed, first, true);
await rewrite(first, final, false);
process.stdout.write(JSON.stringify({ initial, final: await inspect(final) }));
`;

export function parseArguments(arguments_) {
  const values = new Map();
  for (let index = 0; index < arguments_.length; index += 2) {
    const name = arguments_[index];
    const value = arguments_[index + 1];
    if (
      ![
        "--electron-reference",
        "--seed",
        "--ellipse-seed",
        "--ink-seed",
        "--redact-seed",
        "--rich-text-seed",
        "--coordinate-seed",
        "--output-dir",
        "--developer-dir",
      ].includes(name)
    ) {
      throw new Error(`Unknown argument: ${name ?? "<missing>"}`);
    }
    if (!value || value.startsWith("--"))
      throw new Error(`${name} requires a value`);
    if (values.has(name)) throw new Error(`${name} may be provided only once`);
    values.set(name, value);
  }
  for (const required of ["--electron-reference", "--seed", "--output-dir"]) {
    if (!values.has(required)) throw new Error(`${required} is required`);
  }
  return {
    electronReference: values.get("--electron-reference"),
    seed: values.get("--seed"),
    ellipseSeed: values.get("--ellipse-seed"),
    inkSeed: values.get("--ink-seed"),
    redactSeed: values.get("--redact-seed"),
    richTextSeed: values.get("--rich-text-seed"),
    coordinateSeed: values.get("--coordinate-seed"),
    outputDirectory: values.get("--output-dir"),
    developerDirectory: values.get("--developer-dir"),
  };
}

export async function captureManifest(root, entries = referenceEntries) {
  const canonicalRoot = await realpath(root);
  const files = [];
  for (const entry of entries) await collectFiles(canonicalRoot, entry, files);
  files.sort((left, right) => left.localeCompare(right));
  const manifest = [];
  const aggregate = createHash("sha256");
  for (const name of files) {
    const bytes = await readFile(join(canonicalRoot, name));
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    manifest.push({ file: name, bytes: bytes.length, sha256 });
    aggregate.update(name).update("\0").update(bytes).update("\0");
  }
  return { files: manifest, sha256: aggregate.digest("hex") };
}

export function assertSameManifest(expected, actual, label) {
  assert.deepEqual(
    actual,
    expected,
    `${label} changed during the isolated compatibility run`,
  );
}

export function assertMediaState(state, expectedAspectLocked) {
  assert.deepEqual(
    state.map(({ id, kind }) => ({ id, kind })),
    [
      { id: "native-rotated-image", kind: "image" },
      { id: "native-rotated-snapshot", kind: "snapshot" },
    ],
    "rotated media order or identity changed",
  );
  const [image, snapshot] = state;
  for (const markup of state) {
    assert.equal(markup.rotation, 30, `${markup.kind} rotation changed`);
    assert.equal(markup.opacity, 0.65, `${markup.kind} opacity changed`);
    assert.equal(markup.locked, false, `${markup.kind} lock changed`);
    assert.equal(
      markup.mimeType,
      "image/png",
      `${markup.kind} MIME type changed`,
    );
    assert.match(
      markup.dataSha256,
      /^[a-f0-9]{64}$/,
      `${markup.kind} payload hash is missing`,
    );
    for (const [name, value] of Object.entries(markup.rect)) {
      assert(Number.isFinite(value), `${markup.kind} ${name} must be finite`);
    }
    assert(
      markup.rect.width > 0 && markup.rect.width < 1_000,
      `${markup.kind} width is invalid`,
    );
    assert(
      markup.rect.height > 0 && markup.rect.height < 1_000,
      `${markup.kind} height is invalid`,
    );
  }
  assert.equal(
    image.dataSha256,
    snapshot.dataSha256,
    "Image and Snapshot payloads diverged",
  );
  assert.equal(
    image.aspectRatioLocked,
    expectedAspectLocked,
    "Image aspect-lock state changed",
  );
}

export function assertEllipseState(state) {
  assert.equal(state.length, 2, "expected two Ellipses");
  assert.deepEqual(
    [...state.map(({ id, kind }) => ({ id, kind }))].sort((left, right) =>
      left.id.localeCompare(right.id),
    ),
    [
      { id: "legacy-ellipse", kind: "ellipse" },
      { id: "native-rotated-ellipse", kind: "ellipse" },
    ],
    "rotated Ellipse identity changed",
  );
  const native = state.find(({ id }) => id === "native-rotated-ellipse");
  const legacy = state.find(({ id }) => id === "legacy-ellipse");
  assert(native && legacy, "rotated Ellipse identities are incomplete");
  for (const ellipse of state) {
    for (const [name, value] of Object.entries(ellipse.rect)) {
      assert(Number.isFinite(value), `${ellipse.id} ${name} must be finite`);
    }
    assert(
      ellipse.rect.width > 0 && ellipse.rect.width < 1_000,
      `${ellipse.id} width is invalid`,
    );
    assert(
      ellipse.rect.height > 0 && ellipse.rect.height < 1_000,
      `${ellipse.id} height is invalid`,
    );
  }
  assert.equal(native.rotation, 30, "native Ellipse rotation changed");
  assert.equal(native.opacity, 0.7, "native Ellipse opacity changed");
  assert.equal(native.locked, false, "native Ellipse lock changed");
  assert.equal(
    native.stroke?.widthPt,
    2,
    "native Ellipse stroke width changed",
  );
  assert.equal(
    native.stroke?.style,
    undefined,
    "stable Electron unexpectedly retained Ellipse dash style",
  );
  assert.equal(legacy.rotation, 15, "legacy Ellipse rotation changed");
  assert.equal(legacy.locked, false, "legacy Ellipse lock changed");
  return { native, legacy };
}

export function assertInkState(state, expectedRaw) {
  assert.deepEqual(
    state.map(({ id, kind }) => ({ id, kind })),
    [
      { id: "compat-highlight", kind: "highlight" },
      { id: "compat-pen", kind: "pen" },
    ],
    "Ink order, identity or classification changed",
  );
  const [highlight, pen] = state;
  assert.equal(highlight.color, "#ffcc00", "Highlight colour changed");
  assert.equal(highlight.widthPt, 12, "Highlight width changed");
  assert.equal(highlight.opacity, 0.35, "Highlight opacity changed");
  assert.equal(highlight.locked, false, "Highlight lock changed");
  assert.equal(highlight.blendMode, "multiply", "Highlight blend mode changed");
  assert.equal(
    highlight.smoothCurves,
    undefined,
    "Highlight smoothing unexpectedly changed",
  );
  assert.equal(pen.color, "#1f6feb", "Pen colour changed");
  assert.equal(pen.widthPt, 3.25, "Pen width changed");
  assert.equal(pen.opacity, 0.8, "Pen opacity changed");
  assert.equal(pen.locked, true, "Pen lock changed");
  assert.equal(pen.blendMode, "normal", "Pen blend mode changed");
  assert.equal(pen.smoothCurves, true, "Pen smoothing changed");
  for (const markup of state) {
    assert.equal(
      markup.paths.length,
      2,
      `${markup.kind} path grouping changed`,
    );
    for (const path of markup.paths) {
      assert(path.length >= 2, `${markup.kind} path lost points`);
      for (const point of path) {
        assert(
          Number.isFinite(point.x) && Number.isFinite(point.y),
          `${markup.kind} point is invalid`,
        );
      }
    }
    if (expectedRaw) {
      assert.equal(
        markup.hasAppearance,
        expectedRaw.hasAppearance,
        `${markup.kind} AP state changed`,
      );
      assert.equal(
        markup.hasCanonicalPointBits,
        expectedRaw.hasCanonicalPointBits,
        `${markup.kind} canonical-point state changed`,
      );
    }
  }
  return { highlight, pen };
}

export function assertRedactState(state, expectedLocked) {
  assert.deepEqual(
    state.markups.map(({ id, kind }) => ({ id, kind })),
    [{ id: "compat-redact", kind: "redact" }],
    "pending Redact identity changed",
  );
  const [redact] = state.markups;
  assert.equal(
    redact.redactionColor,
    "#102030",
    "pending Redact colour changed",
  );
  assert.equal(
    redact.overlayText,
    "CONFIDENTIAL",
    "pending Redact overlay text changed",
  );
  assert.equal(redact.locked, expectedLocked, "pending Redact lock changed");
  for (const [name, value] of Object.entries(redact.rect)) {
    assert(Number.isFinite(value), `pending Redact ${name} must be finite`);
  }
  assert(
    redact.rect.width > 2 && redact.rect.height > 2,
    "pending Redact geometry is invalid",
  );
  assert.equal(state.raw.subtype, "/Redact", "pending Redact subtype changed");
  assert.equal(
    state.raw.subject,
    "Redaction",
    "pending Redact subject changed",
  );
  assert.equal(
    state.raw.contents,
    "Marked for redaction",
    "pending Redact contents changed",
  );
  assert.equal(
    state.raw.overlayText,
    "CONFIDENTIAL",
    "raw pending Redact overlay text changed",
  );
  assert.equal(
    state.raw.hasAppearance,
    false,
    "pending Redact must not acquire an appearance",
  );
  assert.equal(
    (state.raw.flags & 4) !== 0,
    true,
    "pending Redact print flag changed",
  );
  assert.equal(
    (state.raw.flags & 128) !== 0,
    expectedLocked,
    "pending Redact raw lock changed",
  );
  const { x, y, width, height } = redact.rect;
  assert.deepEqual(
    state.raw.rect,
    [x, y, x + width, y + height],
    "pending Redact /Rect changed",
  );
  assert.deepEqual(
    state.raw.quadPoints,
    [x, y + height, x + width, y + height, x, y, x + width, y],
    "pending Redact /QuadPoints changed",
  );
  const expectedColor = [0x10 / 255, 0x20 / 255, 0x30 / 255];
  assert.equal(
    state.raw.interiorColor.length,
    expectedColor.length,
    "pending Redact /IC changed",
  );
  state.raw.interiorColor.forEach((value, index) => {
    assert(
      Math.abs(value - expectedColor[index]) < 0.0001,
      "pending Redact /IC changed",
    );
  });
  assert.match(
    state.contentSha256,
    /^[a-f0-9]{64}$/,
    "page content digest is missing",
  );
  return redact;
}

const richTextFamilies = ["Helvetica", "Arimo", "Roboto Mono", "Tinos"];
const expectedRichTextRuns = richTextFamilies.flatMap((fontId, familyIndex) => [
  { text: `${fontId} regular | `, fontId, color: "#2255aa", fontSizePt: 11 },
  { text: "bold | ", fontId, bold: true, color: "#aa1122", fontSizePt: 11 },
  { text: "italic | ", fontId, italic: true, color: "#2255aa", fontSizePt: 13 },
  {
    text: `bold italic${familyIndex < richTextFamilies.length - 1 ? "\n" : ""}`,
    fontId,
    bold: true,
    italic: true,
    color: "#aa1122",
    fontSizePt: 13,
  },
]);

function expectedRichTextFontNames(dialect) {
  const names = ["Helv", "HelvBold", "HelvOblique", "HelvBoldOblique"];
  for (const prefix of ["BPArimo", "BPRobotoMono", "BPTinos"]) {
    names.push(
      prefix,
      `${prefix}Bold`,
      `${prefix}${dialect === "native" ? "Oblique" : "Italic"}`,
      `${prefix}Bold${dialect === "native" ? "Oblique" : "Italic"}`,
    );
  }
  return names.sort();
}

function escapeExpectedRichText(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;")
    .replaceAll("\n", "<br/>");
}

function expectedRichTextContent(dialect) {
  const spans = expectedRichTextRuns
    .map((run) => {
      const separator = dialect === "electron" ? "; " : ";";
      const size =
        dialect === "electron"
          ? String(run.fontSizePt)
          : run.fontSizePt.toFixed(6);
      const styles = [
        `font-family:${run.fontId}`,
        `font-size:${size}pt`,
        `color:${run.color.toUpperCase()}`,
        ...(run.bold ? ["font-weight:bold"] : []),
        ...(run.italic ? ["font-style:italic"] : []),
      ];
      return `<span style="${styles.join(separator)}">${escapeExpectedRichText(run.text)}</span>`;
    })
    .join("");
  if (dialect === "native") {
    return `<?xml version="1.0"?><body xmlns="http://www.w3.org/1999/xhtml"><p>${spans}</p></body>`;
  }
  return (
    '<?xml version="1.0"?><body xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/" ' +
    'xfa:contentType="text/html" xfa:APIVersion="BluebeamPDFRevu:2018" xfa:spec="2.2.0" ' +
    'style="font: Arimo 12pt; text-align:left; margin:5pt; line-height:13.8pt; color:#172B4D" ' +
    `xmlns="http://www.w3.org/1999/xhtml"><p>${spans}</p></body>`
  );
}

function expectedAppearanceFontSelections(dialect) {
  const suffix = (run) =>
    `${run.bold ? "Bold" : ""}${run.italic ? (run.fontId === "Helvetica" || dialect === "native" ? "Oblique" : "Italic") : ""}`;
  const runSelections = expectedRichTextRuns.map((run) => {
    const prefix =
      run.fontId === "Helvetica"
        ? "Helv"
        : `BP${run.fontId.replaceAll(" ", "")}`;
    return `${prefix}${suffix(run)}@${run.fontSizePt}`;
  });
  return dialect === "electron"
    ? ["BPArimo@12", ...runSelections]
    : runSelections;
}

function assertFontObjects(objects, dialect, label) {
  const expectedNames = expectedRichTextFontNames(dialect);
  assert.deepEqual(
    objects.map(({ name }) => name),
    expectedNames,
    `${label} font objects changed`,
  );
  assert.equal(
    new Set(objects.map(({ object }) => object)).size,
    objects.length,
    `${label} aliases a font object`,
  );
  for (const font of objects) {
    if (font.name.startsWith("Helv")) {
      assert.equal(
        font.subtype,
        "/Type1",
        `${label} ${font.name} subtype changed`,
      );
      assert.match(
        font.baseFont,
        /^\/Helvetica(?:-|$)/,
        `${label} ${font.name} base font changed`,
      );
      assert.equal(
        font.embedded,
        false,
        `${label} ${font.name} unexpectedly embeds a font program`,
      );
      assert.equal(
        font.programSha256,
        undefined,
        `${label} ${font.name} has an unexpected font digest`,
      );
    } else {
      assert.equal(
        font.subtype,
        "/Type0",
        `${label} ${font.name} subtype changed`,
      );
      assert.match(
        font.baseFont,
        /^\/(?:Arimo|RobotoMono|Tinos)-/,
        `${label} ${font.name} base font changed`,
      );
      assert.equal(
        font.embedded,
        true,
        `${label} ${font.name} lost its embedded font program`,
      );
      assert.match(
        font.programSha256,
        /^[a-f0-9]{64}$/,
        `${label} ${font.name} font digest is missing`,
      );
    }
  }
}

export function assertRichTextState(state, expectedLocked, dialect) {
  assert(
    ["electron", "native"].includes(dialect),
    "rich Text Box font dialect is invalid",
  );
  assert.deepEqual(
    state.markups.map(({ id, kind }) => ({ id, kind })),
    [{ id: "native-rich-text", kind: "text-box" }],
    "rich Text Box identity changed",
  );
  const [textBox] = state.markups;
  assert.equal(
    textBox.text,
    expectedRichTextRuns.map((run) => run.text).join(""),
    "rich Text Box content changed",
  );
  assert.equal(
    textBox.fontFamily,
    "Arimo",
    "rich Text Box default family changed",
  );
  assert.equal(textBox.fontSizePt, 12, "rich Text Box default size changed");
  assert.equal(textBox.opacity, 1, "rich Text Box opacity changed");
  assert.equal(textBox.locked, expectedLocked, "rich Text Box lock changed");
  assert.deepEqual(
    textBox.richTextRuns,
    expectedRichTextRuns,
    "rich Text Box editable runs changed",
  );
  for (const [name, value] of Object.entries(textBox.rect)) {
    assert(Number.isFinite(value), `rich Text Box ${name} must be finite`);
  }
  assert(
    textBox.rect.width > 2 && textBox.rect.height > 2,
    "rich Text Box geometry is invalid",
  );
  assert.equal(state.raw.subtype, "/FreeText", "rich Text Box subtype changed");
  assert.equal(state.raw.subject, "Text Box", "rich Text Box subject changed");
  assert.equal(
    state.raw.contents,
    textBox.text,
    "raw rich Text Box content changed",
  );
  assert.equal(
    (state.raw.flags & 4) !== 0,
    true,
    "rich Text Box print flag changed",
  );
  assert.equal(
    (state.raw.flags & 128) !== 0,
    expectedLocked,
    "rich Text Box raw lock changed",
  );
  assert(
    Math.abs(state.raw.opacity - 1) < 0.0001,
    "rich Text Box raw opacity changed",
  );
  const { x, y, width, height } = textBox.rect;
  assert.deepEqual(
    state.raw.rect,
    [x, y, x + width, y + height],
    "rich Text Box /Rect changed",
  );
  assert.match(
    state.raw.defaultAppearance,
    /\/BPArimo\s+12(?:\.0+)?\s+Tf/,
    "rich Text Box /DA changed",
  );
  assert.match(
    state.raw.defaultStyle,
    /Arimo\s+12(?:\.0+)?pt/i,
    "rich Text Box /DS changed",
  );
  assert.equal(
    state.raw.richContent,
    expectedRichTextContent(dialect),
    "rich Text Box /RC wrapper or ordered runs changed",
  );
  const expectedFonts = expectedRichTextFontNames(dialect);
  assert.deepEqual(
    state.raw.defaultFonts,
    expectedFonts,
    "rich Text Box /DR fonts changed",
  );
  assert.deepEqual(
    state.raw.appearanceFonts,
    expectedFonts,
    "rich Text Box /AP fonts changed",
  );
  assertFontObjects(state.raw.defaultFontObjects, dialect, "rich Text Box /DR");
  assertFontObjects(
    state.raw.appearanceFontObjects,
    dialect,
    "rich Text Box /AP",
  );
  assert.equal(
    state.raw.indirectObjectCount > 0,
    true,
    "rich Text Box PDF object count is missing",
  );
  assert.equal(
    state.raw.appearanceBounds.length,
    4,
    "rich Text Box /AP /BBox changed",
  );
  assert(
    state.raw.appearanceBounds.every(Number.isFinite),
    "rich Text Box /AP /BBox must be finite",
  );
  assert(
    state.raw.appearanceBounds[2] > state.raw.appearanceBounds[0],
    "rich Text Box /AP /BBox width is invalid",
  );
  assert(
    state.raw.appearanceBounds[3] > state.raw.appearanceBounds[1],
    "rich Text Box /AP /BBox height is invalid",
  );
  assert.deepEqual(
    state.raw.appearanceTextBoundary.fontSelections,
    expectedAppearanceFontSelections(dialect),
    "rich Text Box /AP ordered font operators changed",
  );
  assert.equal(
    state.raw.appearanceTextBoundary.beginText,
    1,
    "rich Text Box /AP BT boundary changed",
  );
  assert.equal(
    state.raw.appearanceTextBoundary.endText,
    1,
    "rich Text Box /AP ET boundary changed",
  );
  assert(
    state.raw.appearanceTextBoundary.textPaints >= 16,
    "rich Text Box /AP no longer paints every run",
  );
  assert(state.raw.appearanceBytes > 100, "rich Text Box appearance is empty");
  assert.match(
    state.raw.appearanceSha256,
    /^[a-f0-9]{64}$/,
    "rich Text Box appearance digest is missing",
  );
  assert.match(
    state.contentSha256,
    /^[a-f0-9]{64}$/,
    "page content digest is missing",
  );
  return textBox;
}

export function assertCoordinateState(state, expectedLocked) {
  assert.deepEqual(
    state.markups.map(({ id, kind }) => ({ id, kind })),
    [
      { id: "coordinate-rectangle", kind: "rectangle" },
      { id: "coordinate-length", kind: "length" },
    ],
    "coordinate annotation order or identity changed",
  );
  const [rectangle, length] = state.markups;
  assert.equal(
    rectangle.locked,
    expectedLocked,
    "coordinate Rectangle lock changed",
  );
  assert.equal(length.locked, false, "coordinate Length lock changed");
  assert.deepEqual(state.page, {
    index: 0,
    width: 1_200,
    height: 800,
    rotation: 90,
    viewBox: { x: 50, y: 100, width: 400, height: 600 },
    userUnit: 2,
  });
  assert.deepEqual(state.raw.mediaBox, [10, 20, 610, 820]);
  assert.deepEqual(state.raw.cropBox, [50, 100, 450, 700]);
  assert.equal(state.raw.rotation, 90);
  assert.equal(state.raw.userUnit, 2);
  assert.equal(state.raw.pageOwnsMediaBox, false);
  assert.equal(state.raw.pageOwnsCropBox, false);
  assert.equal(state.raw.pageOwnsRotation, false);
  assert.deepEqual(state.raw.pageScale, {
    pageIndex: 0,
    source: "calibrated",
    name: "Coordinate 1 m",
    pdfUnits: "in",
    realUnits: "m",
    scaleX: 0.01,
    scaleY: 0.01,
    precision: { mode: "decimal", value: 0.01 },
  });
  assert.deepEqual(
    state.raw.lengthLine,
    [length.start.x, length.start.y, length.end.x, length.end.y],
    "coordinate Length raw line changed",
  );
  assert.deepEqual(state.raw.vendor, {
    subtype: "/Stamp",
    subject: "Independent vendor annotation",
    contents: "Preserve this opaque annotation",
    rect: [350, 150, 390, 190],
    probe: "coordinate-space-sentinel",
  });
  assert.match(
    state.pageContentSha256,
    /^[a-f0-9]{64}$/,
    "coordinate page content digest is missing",
  );
  return { rectangle, length };
}

export function assessRichTextOutputAmplification(stageBytes, stageStates) {
  const orderedStages = [
    ["seed", "sourceSeed", "source"],
    ["stable-first", "stableFirst", "frozen-electron"],
    ["stable-final", "stableFinal", "frozen-electron"],
    ["native-final", "nativeRichText", "native"],
    ["electron-first", "electronRichTextFirst", "frozen-electron"],
    ["electron-final", "electronRichTextFinal", "frozen-electron"],
  ];
  const legs = orderedStages.slice(1).map(([name, key, engine], index) => {
    const [inputName, inputKey] = orderedStages[index];
    const inputBytes = stageBytes[inputKey];
    const outputBytes = stageBytes[key];
    assert(
      Number.isSafeInteger(inputBytes) && inputBytes > 0,
      `${inputName} byte count is invalid`,
    );
    assert(
      Number.isSafeInteger(outputBytes) && outputBytes > 0,
      `${name} byte count is invalid`,
    );
    return {
      input: inputName,
      output: name,
      engine,
      inputBytes,
      outputBytes,
      growthBytes: outputBytes - inputBytes,
      ratio: Number((outputBytes / inputBytes).toFixed(6)),
    };
  });
  const nativeLeg = legs.find(({ engine }) => engine === "native");
  const nativePassed = nativeLeg.growthBytes <= 512 * 1024;
  const unresolvedBlockers = legs
    .filter(
      ({ engine, growthBytes, ratio }) =>
        engine === "frozen-electron" &&
        growthBytes > 1024 * 1024 &&
        ratio > 1.25,
    )
    .map(({ input, output, growthBytes, ratio }) => ({
      code: "frozen-electron-unreachable-object-retention",
      leg: `${input}->${output}`,
      growthBytes,
      ratio,
    }));
  return {
    status:
      nativePassed && unresolvedBlockers.length === 0 ? "passed" : "blocked",
    gate: {
      passed: nativePassed && unresolvedBlockers.length === 0,
      nativePrunePassed: nativePassed,
      maxNativeGrowthBytes: 512 * 1024,
    },
    legs,
    resourceCounts: stageStates.map(([stage, state]) => ({
      stage,
      indirectObjects: state.raw.indirectObjectCount,
      defaultFonts: state.raw.defaultFontObjects.length,
      appearanceFonts: state.raw.appearanceFontObjects.length,
      embeddedAppearanceFonts: state.raw.appearanceFontObjects.filter(
        ({ embedded }) => embedded,
      ).length,
    })),
    unresolvedBlockers,
  };
}

function assertTranslatedPaths(actual, expected, deltaX, deltaY, label) {
  assert.equal(actual.length, expected.length, `${label} path count changed`);
  for (let pathIndex = 0; pathIndex < actual.length; pathIndex += 1) {
    assert.equal(
      actual[pathIndex].length,
      expected[pathIndex].length,
      `${label} point count changed`,
    );
    for (
      let pointIndex = 0;
      pointIndex < actual[pathIndex].length;
      pointIndex += 1
    ) {
      const actualPoint = actual[pathIndex][pointIndex];
      const expectedPoint = expected[pathIndex][pointIndex];
      assert(
        Math.abs(actualPoint.x - expectedPoint.x - deltaX) < 0.0001,
        `${label} x edit changed`,
      );
      assert(
        Math.abs(actualPoint.y - expectedPoint.y - deltaY) < 0.0001,
        `${label} y edit changed`,
      );
    }
  }
}

async function collectFiles(root, entry, files) {
  const absolute = resolve(root, entry);
  const escaped = relative(root, absolute);
  if (escaped === ".." || escaped.startsWith("../") || isAbsolute(escaped)) {
    throw new Error(`Reference entry escapes its root: ${entry}`);
  }
  const info = await lstat(absolute);
  if (info.isSymbolicLink())
    throw new Error(`Reference entry must not be a symlink: ${entry}`);
  if (info.isFile()) {
    files.push(escaped);
    return;
  }
  if (!info.isDirectory())
    throw new Error(`Reference entry is not a file or directory: ${entry}`);
  for (const child of (await readdir(absolute)).sort()) {
    await collectFiles(root, join(entry, child), files);
  }
}

function run(command, arguments_, cwd, { capture = false, environment } = {}) {
  const result = spawnSync(command, arguments_, {
    cwd,
    encoding: "utf8",
    env: environment ? { ...process.env, ...environment } : process.env,
    maxBuffer: 128 * 1024 * 1024,
    stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(
      `${command} exited ${result.status}${result.stderr ? `: ${result.stderr.trim()}` : ""}`,
    );
  }
  return result.stdout?.trim() ?? "";
}

async function validateReference(reference) {
  if (!isAbsolute(reference))
    throw new Error("--electron-reference must be an absolute path");
  const canonical = await realpath(reference);
  if (!(await stat(canonical)).isDirectory())
    throw new Error("--electron-reference must be a directory");
  const topLevel = await realpath(
    run("git", ["rev-parse", "--show-toplevel"], canonical, { capture: true }),
  );
  if (topLevel !== canonical)
    throw new Error("--electron-reference must name the Git worktree root");
  if (canonical === (await realpath(repositoryRoot))) {
    throw new Error(
      "--electron-reference must not name the GPUI migration worktree",
    );
  }
  return canonical;
}

async function copyReference(reference, destination) {
  await mkdir(destination, { recursive: true });
  for (const entry of referenceEntries) {
    const target = join(destination, entry);
    await mkdir(resolve(target, ".."), { recursive: true });
    await cp(join(reference, entry), target, {
      recursive: true,
      errorOnExist: true,
    });
  }
}

async function sha256File(file) {
  return createHash("sha256")
    .update(await readFile(file))
    .digest("hex");
}

async function main() {
  if (process.platform !== "darwin")
    throw new Error("This compatibility harness is scoped to macOS.");
  const options = parseArguments(process.argv.slice(2));
  const reference = await validateReference(options.electronReference);
  const seed = resolve(options.seed);
  if (!(await stat(seed)).isFile())
    throw new Error("--seed must name a regular PDF file");
  const ellipseSeed = options.ellipseSeed
    ? resolve(options.ellipseSeed)
    : undefined;
  if (ellipseSeed && !(await stat(ellipseSeed)).isFile()) {
    throw new Error("--ellipse-seed must name a regular PDF file");
  }
  const inkSeed = options.inkSeed ? resolve(options.inkSeed) : undefined;
  if (inkSeed && !(await stat(inkSeed)).isFile()) {
    throw new Error("--ink-seed must name a regular PDF file");
  }
  const redactSeed = options.redactSeed
    ? resolve(options.redactSeed)
    : undefined;
  if (redactSeed && !(await stat(redactSeed)).isFile()) {
    throw new Error("--redact-seed must name a regular PDF file");
  }
  const richTextSeed = options.richTextSeed
    ? resolve(options.richTextSeed)
    : undefined;
  if (richTextSeed && !(await stat(richTextSeed)).isFile()) {
    throw new Error("--rich-text-seed must name a regular PDF file");
  }
  const coordinateSeed = options.coordinateSeed
    ? resolve(options.coordinateSeed)
    : undefined;
  if (coordinateSeed && !(await stat(coordinateSeed)).isFile()) {
    throw new Error("--coordinate-seed must name a regular PDF file");
  }
  if (!isAbsolute(options.outputDirectory))
    throw new Error("--output-dir must be an absolute path");
  const outputDirectory = resolve(options.outputDirectory);
  await mkdir(outputDirectory);
  const temporaryRoot = await mkdtemp(
    join(tmpdir(), "bp-electron-gpui-rotated-media-"),
  );
  const snapshot = join(temporaryRoot, "reference");
  const before = await captureManifest(reference);
  const bridgeBoundary =
    inkSeed || richTextSeed ? await captureManifest(repositoryRoot) : undefined;
  const bridgeCommit =
    inkSeed || richTextSeed
      ? run("git", ["rev-parse", "HEAD"], repositoryRoot, { capture: true })
      : undefined;
  const bridgeScopedStatus =
    inkSeed || richTextSeed
      ? run(
          "git",
          ["status", "--porcelain=v1", "--", ...referenceEntries],
          repositoryRoot,
          { capture: true },
        )
      : undefined;
  const commit = run("git", ["rev-parse", "HEAD"], reference, {
    capture: true,
  });
  const scopedStatus = run(
    "git",
    ["status", "--porcelain=v1", "--", ...referenceEntries],
    reference,
    {
      capture: true,
    },
  );
  const statusSha256 = createHash("sha256").update(scopedStatus).digest("hex");
  try {
    await copyReference(reference, snapshot);
    assertSameManifest(
      before,
      await captureManifest(snapshot),
      "Reference copy",
    );
    assertSameManifest(
      before,
      await captureManifest(reference),
      "Electron reference",
    );

    run(
      "pnpm",
      [
        "install",
        "--frozen-lockfile",
        "--offline",
        "--ignore-scripts",
        "--filter",
        "@butter-paper/pdf...",
      ],
      snapshot,
    );
    run(
      "pnpm",
      ["exec", "tsc", "-p", "packages/core/tsconfig.build.json"],
      snapshot,
    );
    run(
      "pnpm",
      ["exec", "tsc", "-p", "packages/pdf/tsconfig.build.json"],
      snapshot,
    );
    const editor = join(snapshot, "packages/pdf/stable-rotated-media.mjs");
    await writeFile(editor, stableEditorSource, { flag: "wx", mode: 0o600 });
    const stableFirst = join(outputDirectory, "stable-first.pdf");
    const stableFinal = join(outputDirectory, "stable-final.pdf");
    const stableState = JSON.parse(
      run("node", [editor, seed, stableFirst, stableFinal], snapshot, {
        capture: true,
      }),
    );
    assertMediaState(stableState, true);
    run("qpdf", ["--check", stableFinal], snapshot);

    const developerDirectory =
      options.developerDirectory ?? process.env.DEVELOPER_DIR;
    const xcodeEnvironment = developerDirectory
      ? { DEVELOPER_DIR: developerDirectory }
      : {};
    const sdkRoot = run(
      "/usr/bin/xcrun",
      ["--sdk", "macosx", "--show-sdk-path"],
      nativeRoot,
      {
        capture: true,
        environment: xcodeEnvironment,
      },
    );
    const clang = run(
      "/usr/bin/xcrun",
      ["--sdk", "macosx", "--find", "clang"],
      nativeRoot,
      {
        capture: true,
        environment: xcodeEnvironment,
      },
    );
    const nativeOutput = join(outputDirectory, "native-final.pdf");
    run(
      "python3",
      [
        "scripts/run-native-bounded.py",
        "env",
        ...(developerDirectory ? [`DEVELOPER_DIR=${developerDirectory}`] : []),
        `SDKROOT=${sdkRoot}`,
        `CC=${clang}`,
        `CXX=${clang}`,
        `BP_ELECTRON_EDITED_ROTATED_MEDIA_FIXTURE=${stableFinal}`,
        `BP_NATIVE_ROTATED_MEDIA_OUTPUT=${nativeOutput}`,
        "cargo",
        "test",
        "--lib",
        "pdf_engine::tests::electron_edited_rotated_image_and_snapshot_survive_native_edit_and_two_reopens",
        "--",
        "--ignored",
        "--exact",
      ],
      nativeRoot,
    );
    run("qpdf", ["--check", nativeOutput], nativeRoot);
    const electronFirst = join(outputDirectory, "electron-first.pdf");
    const electronFinal = join(outputDirectory, "electron-final.pdf");
    const electronState = JSON.parse(
      run(
        "node",
        [editor, nativeOutput, electronFirst, electronFinal],
        snapshot,
        { capture: true },
      ),
    );
    assertMediaState(electronState, false);
    assert.deepEqual(
      electronState.map(({ dataSha256 }) => dataSha256),
      stableState.map(({ dataSha256 }) => dataSha256),
      "binary media payload changed across Electron to GPUI to Electron",
    );
    run("qpdf", ["--check", electronFinal], snapshot);

    let ellipseSourceSeed;
    let stableEllipseInputState;
    let stableEllipseState;
    let electronEllipseInputState;
    let electronEllipseState;
    const ellipseOutputs = {};
    if (ellipseSeed) {
      ellipseSourceSeed = {
        bytes: (await stat(ellipseSeed)).size,
        sha256: await sha256File(ellipseSeed),
      };
      const ellipseEditor = join(
        snapshot,
        "packages/pdf/stable-rotated-ellipse.mjs",
      );
      await writeFile(ellipseEditor, stableEllipseEditorSource, {
        flag: "wx",
        mode: 0o600,
      });
      const stableEllipseFirst = join(
        outputDirectory,
        "stable-ellipse-first.pdf",
      );
      const stableEllipseFinal = join(
        outputDirectory,
        "stable-ellipse-final.pdf",
      );
      const stableEllipseJourney = JSON.parse(
        run(
          "node",
          [ellipseEditor, ellipseSeed, stableEllipseFirst, stableEllipseFinal],
          snapshot,
          {
            capture: true,
          },
        ),
      );
      stableEllipseInputState = stableEllipseJourney.initial;
      stableEllipseState = stableEllipseJourney.final;
      const stableEllipseInput = assertEllipseState(stableEllipseInputState);
      const stableEllipse = assertEllipseState(stableEllipseState);
      const radians = Math.PI / 6;
      const firstDowngradeWidth =
        200 * Math.cos(radians) + 110 * Math.sin(radians);
      const firstDowngradeHeight =
        200 * Math.sin(radians) + 110 * Math.cos(radians);
      assert(
        Math.abs(stableEllipse.native.rect.width - firstDowngradeWidth) <
          0.0001,
      );
      assert(
        Math.abs(stableEllipse.native.rect.height - firstDowngradeHeight) <
          0.0001,
      );
      assert.equal(
        stableEllipse.native.rect.width,
        stableEllipseInput.native.rect.width,
      );
      assert.equal(
        stableEllipse.native.rect.height,
        stableEllipseInput.native.rect.height,
      );
      assert(
        Math.abs(
          stableEllipse.native.rect.x - stableEllipseInput.native.rect.x - 12,
        ) < 0.0001,
      );
      assert.equal(
        stableEllipse.native.rect.y,
        stableEllipseInput.native.rect.y,
      );
      run("qpdf", ["--check", stableEllipseFinal], snapshot);

      const nativeEllipse = join(outputDirectory, "native-ellipse-final.pdf");
      run(
        "python3",
        [
          "scripts/run-native-bounded.py",
          "env",
          ...(developerDirectory
            ? [`DEVELOPER_DIR=${developerDirectory}`]
            : []),
          `SDKROOT=${sdkRoot}`,
          `CC=${clang}`,
          `CXX=${clang}`,
          `BP_ELECTRON_EDITED_ROTATED_ELLIPSE_FIXTURE=${stableEllipseFinal}`,
          `BP_NATIVE_ROTATED_ELLIPSE_OUTPUT=${nativeEllipse}`,
          "cargo",
          "test",
          "--lib",
          "pdf_engine::tests::electron_edited_rotated_ellipse_survives_native_edit_and_two_reopens",
          "--",
          "--ignored",
          "--exact",
        ],
        nativeRoot,
      );
      run("qpdf", ["--check", nativeEllipse], nativeRoot);

      const electronEllipseFirst = join(
        outputDirectory,
        "electron-ellipse-first.pdf",
      );
      const electronEllipseFinal = join(
        outputDirectory,
        "electron-ellipse-final.pdf",
      );
      const electronEllipseJourney = JSON.parse(
        run(
          "node",
          [
            ellipseEditor,
            nativeEllipse,
            electronEllipseFirst,
            electronEllipseFinal,
          ],
          snapshot,
          { capture: true },
        ),
      );
      electronEllipseInputState = electronEllipseJourney.initial;
      electronEllipseState = electronEllipseJourney.final;
      const electronEllipseInput = assertEllipseState(
        electronEllipseInputState,
      );
      const electronEllipse = assertEllipseState(electronEllipseState);
      assert.deepEqual(
        electronEllipseState.map(({ id }) => id),
        stableEllipseState.map(({ id }) => id),
        "Ellipse order changed across Electron to GPUI to Electron",
      );
      const stableCentre = {
        x: stableEllipse.native.rect.x + stableEllipse.native.rect.width / 2,
        y: stableEllipse.native.rect.y + stableEllipse.native.rect.height / 2,
      };
      const electronCentre = {
        x:
          electronEllipse.native.rect.x + electronEllipse.native.rect.width / 2,
        y:
          electronEllipse.native.rect.y +
          electronEllipse.native.rect.height / 2,
      };
      const electronInputCentre = {
        x:
          electronEllipseInput.native.rect.x +
          electronEllipseInput.native.rect.width / 2,
        y:
          electronEllipseInput.native.rect.y +
          electronEllipseInput.native.rect.height / 2,
      };
      assert(
        Math.abs(electronInputCentre.x - stableCentre.x - 7) < 0.0001,
        "native Ellipse horizontal edit was lost",
      );
      assert(
        Math.abs(electronInputCentre.y - stableCentre.y - 5) < 0.0001,
        "native Ellipse vertical edit was lost",
      );
      assert.equal(
        electronEllipse.native.rect.width,
        electronEllipseInput.native.rect.width,
      );
      assert.equal(
        electronEllipse.native.rect.height,
        electronEllipseInput.native.rect.height,
      );
      assert(
        Math.abs(electronCentre.x - electronInputCentre.x - 12) < 0.0001,
        "Electron Ellipse edit was lost",
      );
      assert(
        Math.abs(electronCentre.y - electronInputCentre.y) < 0.0001,
        "Electron Ellipse vertical geometry changed",
      );
      assert(
        Math.abs(electronCentre.x - stableCentre.x - 19) < 0.0001,
        "Ellipse horizontal edits were lost",
      );
      assert(
        Math.abs(electronCentre.y - stableCentre.y - 5) < 0.0001,
        "Ellipse vertical edit was lost",
      );
      assert.notEqual(
        electronEllipse.native.rect.width,
        stableEllipse.native.rect.width,
      );
      assert.notEqual(
        electronEllipse.native.rect.height,
        stableEllipse.native.rect.height,
      );
      assert.deepEqual(
        electronEllipse.legacy,
        stableEllipse.legacy,
        "legacy Ellipse changed",
      );
      run("qpdf", ["--check", electronEllipseFinal], snapshot);
      Object.assign(ellipseOutputs, {
        stableEllipseFirst,
        stableEllipseFinal,
        nativeEllipse,
        electronEllipseFirst,
        electronEllipseFinal,
      });
    }

    let inkSourceSeed;
    let stableInkInputState;
    let stableInkState;
    let electronInkInputState;
    let electronInkState;
    let inkBridgeCandidate;
    const inkOutputs = {};
    if (inkSeed) {
      inkSourceSeed = {
        bytes: (await stat(inkSeed)).size,
        sha256: await sha256File(inkSeed),
      };
      const inkEditor = join(snapshot, "packages/pdf/stable-ink.mjs");
      await writeFile(inkEditor, stableInkEditorSource, {
        flag: "wx",
        mode: 0o600,
      });
      const stableInkFirst = join(outputDirectory, "stable-ink-first.pdf");
      const stableInkFinal = join(outputDirectory, "stable-ink-final.pdf");
      const stableInkJourney = JSON.parse(
        run(
          "node",
          [inkEditor, inkSeed, stableInkFirst, stableInkFinal],
          snapshot,
          {
            capture: true,
          },
        ),
      );
      stableInkInputState = stableInkJourney.initial;
      stableInkState = stableInkJourney.final;
      const stableInkInput = assertInkState(stableInkInputState);
      const stableInk = assertInkState(stableInkState, {
        hasAppearance: false,
        hasCanonicalPointBits: false,
      });
      assert.equal(stableInkInput.highlight.hasAppearance, true);
      assert.equal(stableInkInput.pen.hasAppearance, false);
      assert.equal(stableInkInput.highlight.hasCanonicalPointBits, false);
      assert.equal(stableInkInput.pen.hasCanonicalPointBits, false);
      assertTranslatedPaths(
        stableInk.highlight.paths.slice(0, 1),
        stableInkInput.highlight.paths.slice(0, 1),
        0,
        0,
        "stable Highlight edge path",
      );
      assertTranslatedPaths(
        stableInk.highlight.paths.slice(1),
        stableInkInput.highlight.paths.slice(1),
        12,
        0,
        "stable Highlight edited path",
      );
      assertTranslatedPaths(
        stableInk.pen.paths,
        stableInkInput.pen.paths,
        12,
        0,
        "stable Pen",
      );
      run("qpdf", ["--check", stableInkFinal], snapshot);

      const nativeInk = join(outputDirectory, "native-ink-final.pdf");
      run(
        "python3",
        [
          "scripts/run-native-bounded.py",
          "env",
          ...(developerDirectory
            ? [`DEVELOPER_DIR=${developerDirectory}`]
            : []),
          `SDKROOT=${sdkRoot}`,
          `CC=${clang}`,
          `CXX=${clang}`,
          `BP_ELECTRON_EDITED_INK_FIXTURE=${stableInkFinal}`,
          `BP_NATIVE_INK_OUTPUT=${nativeInk}`,
          "cargo",
          "test",
          "--lib",
          "pdf_engine::tests::electron_edited_pen_and_highlight_survive_native_edit_and_two_reopens",
          "--",
          "--ignored",
          "--exact",
        ],
        nativeRoot,
      );
      run("qpdf", ["--check", nativeInk], nativeRoot);

      const bridgeInkFinal = join(
        outputDirectory,
        "current-electron-ink-bridge-final.pdf",
      );
      const bridgeInkFirst = bridgeInkFinal.replace(/\.pdf$/i, ".first.pdf");
      run(
        "pnpm",
        [
          "exec",
          "vitest",
          "run",
          "--config",
          "vitest.config.mts",
          "packages/pdf/src/index.test.ts",
          "-t",
          "edits native-produced Ink through the current Electron bridge",
        ],
        repositoryRoot,
        {
          environment: {
            BP_NATIVE_INK_BRIDGE_FIXTURE: nativeInk,
            BP_ELECTRON_INK_BRIDGE_OUTPUT: bridgeInkFinal,
          },
        },
      );
      run("qpdf", ["--check", bridgeInkFirst], repositoryRoot);
      run("qpdf", ["--check", bridgeInkFinal], repositoryRoot);
      assertSameManifest(
        bridgeBoundary,
        await captureManifest(repositoryRoot),
        "Current Electron Ink bridge source",
      );
      inkBridgeCandidate = {
        classification: "bridge-candidate-only",
        proves:
          "The source-bound current migration Electron writer consumes native-produced multi-path Highlight and Pen annotations, edits them, preserves semantic identity through two saves, and regenerates standard Form appearances with round caps and joins plus one stroke per logical path.",
        doesNotProve:
          "This does not change or qualify public stable Electron 0.0.11, which still removes the Ink appearance after an edit.",
        source: {
          commit: bridgeCommit,
          dirty: bridgeScopedStatus.length > 0,
          statusSha256: createHash("sha256")
            .update(bridgeScopedStatus)
            .digest("hex"),
          boundarySha256: bridgeBoundary.sha256,
          files: bridgeBoundary.files,
        },
        nativeInputSha256: await sha256File(nativeInk),
      };

      const electronInkFirst = join(outputDirectory, "electron-ink-first.pdf");
      const electronInkFinal = join(outputDirectory, "electron-ink-final.pdf");
      const electronInkJourney = JSON.parse(
        run(
          "node",
          [inkEditor, nativeInk, electronInkFirst, electronInkFinal],
          snapshot,
          {
            capture: true,
          },
        ),
      );
      electronInkInputState = electronInkJourney.initial;
      electronInkState = electronInkJourney.final;
      const electronInkInput = assertInkState(electronInkInputState, {
        hasAppearance: true,
        hasCanonicalPointBits: true,
      });
      const electronInk = assertInkState(electronInkState, {
        hasAppearance: false,
        hasCanonicalPointBits: false,
      });
      assertTranslatedPaths(
        electronInkInput.highlight.paths.slice(0, 1),
        stableInk.highlight.paths.slice(0, 1),
        0,
        0,
        "native Highlight edge path",
      );
      assertTranslatedPaths(
        electronInkInput.highlight.paths.slice(1),
        stableInk.highlight.paths.slice(1),
        7,
        5,
        "native Highlight edited path",
      );
      assertTranslatedPaths(
        electronInkInput.pen.paths,
        stableInk.pen.paths,
        0,
        3,
        "native Pen",
      );
      assertTranslatedPaths(
        electronInk.highlight.paths.slice(0, 1),
        electronInkInput.highlight.paths.slice(0, 1),
        0,
        0,
        "final Electron Highlight edge path",
      );
      assertTranslatedPaths(
        electronInk.highlight.paths.slice(1),
        electronInkInput.highlight.paths.slice(1),
        12,
        0,
        "final Electron Highlight edited path",
      );
      assertTranslatedPaths(
        electronInk.pen.paths,
        electronInkInput.pen.paths,
        12,
        0,
        "final Electron Pen",
      );
      run("qpdf", ["--check", electronInkFinal], snapshot);
      Object.assign(inkOutputs, {
        stableInkFirst,
        stableInkFinal,
        nativeInk,
        bridgeInkFirst,
        bridgeInkFinal,
        electronInkFirst,
        electronInkFinal,
      });
    }

    let redactSourceSeed;
    let stableRedactInputState;
    let stableRedactState;
    let electronRedactInputState;
    let electronRedactState;
    const redactOutputs = {};
    if (redactSeed) {
      redactSourceSeed = {
        bytes: (await stat(redactSeed)).size,
        sha256: await sha256File(redactSeed),
      };
      const redactEditor = join(snapshot, "packages/pdf/stable-redact.mjs");
      await writeFile(redactEditor, stableRedactEditorSource, {
        flag: "wx",
        mode: 0o600,
      });
      const stableRedactFirst = join(
        outputDirectory,
        "stable-redact-first.pdf",
      );
      const stableRedactFinal = join(
        outputDirectory,
        "stable-redact-final.pdf",
      );
      const stableRedactJourney = JSON.parse(
        run(
          "node",
          [redactEditor, redactSeed, stableRedactFirst, stableRedactFinal],
          snapshot,
          {
            capture: true,
          },
        ),
      );
      stableRedactInputState = stableRedactJourney.initial;
      stableRedactState = stableRedactJourney.final;
      const stableInput = assertRedactState(stableRedactInputState, false);
      const stableRedact = assertRedactState(stableRedactState, false);
      assert.equal(
        stableRedact.rect.x,
        stableInput.rect.x + 12,
        "stable Redact edit was lost",
      );
      assert.equal(
        stableRedact.rect.y,
        stableInput.rect.y,
        "stable Redact vertical geometry changed",
      );
      assert.equal(
        stableRedactState.contentSha256,
        stableRedactInputState.contentSha256,
        "stable Electron changed covered page content",
      );
      run("qpdf", ["--check", stableRedactFinal], snapshot);

      const nativeRedact = join(outputDirectory, "native-redact-final.pdf");
      run(
        "python3",
        [
          "scripts/run-native-bounded.py",
          "env",
          ...(developerDirectory
            ? [`DEVELOPER_DIR=${developerDirectory}`]
            : []),
          `SDKROOT=${sdkRoot}`,
          `CC=${clang}`,
          `CXX=${clang}`,
          `BP_ELECTRON_EDITED_REDACT_FIXTURE=${stableRedactFinal}`,
          `BP_NATIVE_REDACT_OUTPUT=${nativeRedact}`,
          "cargo",
          "test",
          "--lib",
          "pdf_engine::tests::electron_edited_pending_redact_survives_native_edit_and_two_reopens",
          "--",
          "--ignored",
          "--exact",
        ],
        nativeRoot,
      );
      run("qpdf", ["--check", nativeRedact], nativeRoot);

      const electronRedactFirst = join(
        outputDirectory,
        "electron-redact-first.pdf",
      );
      const electronRedactFinal = join(
        outputDirectory,
        "electron-redact-final.pdf",
      );
      const electronRedactJourney = JSON.parse(
        run(
          "node",
          [
            redactEditor,
            nativeRedact,
            electronRedactFirst,
            electronRedactFinal,
          ],
          snapshot,
          { capture: true },
        ),
      );
      electronRedactInputState = electronRedactJourney.initial;
      electronRedactState = electronRedactJourney.final;
      const electronInput = assertRedactState(electronRedactInputState, true);
      const electronRedact = assertRedactState(electronRedactState, true);
      assert.equal(
        electronInput.rect.x,
        stableRedact.rect.x + 7,
        "native Redact horizontal edit was lost",
      );
      assert.equal(
        electronInput.rect.y,
        stableRedact.rect.y + 5,
        "native Redact vertical edit was lost",
      );
      assert.equal(
        electronRedact.rect.x,
        electronInput.rect.x + 12,
        "final Electron Redact edit was lost",
      );
      assert.equal(
        electronRedact.rect.y,
        electronInput.rect.y,
        "final Electron Redact vertical geometry changed",
      );
      assert.equal(
        electronRedactInputState.contentSha256,
        stableRedactInputState.contentSha256,
        "native changed covered page content",
      );
      assert.equal(
        electronRedactState.contentSha256,
        electronRedactInputState.contentSha256,
        "final Electron changed covered page content",
      );
      run("qpdf", ["--check", electronRedactFinal], snapshot);
      Object.assign(redactOutputs, {
        stableRedactFirst,
        stableRedactFinal,
        nativeRedact,
        electronRedactFirst,
        electronRedactFinal,
      });
    }

    let richTextSourceSeed;
    let stableRichTextInputState;
    let stableRichTextState;
    let electronRichTextInputState;
    let electronRichTextState;
    let richTextBridgeCandidate;
    const richTextOutputs = {};
    if (richTextSeed) {
      richTextSourceSeed = {
        bytes: (await stat(richTextSeed)).size,
        sha256: await sha256File(richTextSeed),
      };
      const richTextEditor = join(
        snapshot,
        "packages/pdf/stable-rich-text.mjs",
      );
      await writeFile(richTextEditor, stableRichTextEditorSource, {
        flag: "wx",
        mode: 0o600,
      });
      const stableRichTextFirst = join(
        outputDirectory,
        "stable-rich-text-first.pdf",
      );
      const stableRichTextFinal = join(
        outputDirectory,
        "stable-rich-text-final.pdf",
      );
      const stableRichTextJourney = JSON.parse(
        run(
          "node",
          [
            richTextEditor,
            richTextSeed,
            stableRichTextFirst,
            stableRichTextFinal,
          ],
          snapshot,
          { capture: true },
        ),
      );
      stableRichTextInputState = stableRichTextJourney.initial;
      stableRichTextState = stableRichTextJourney.final;
      const stableInput = assertRichTextState(
        stableRichTextInputState,
        false,
        "electron",
      );
      const stableRichText = assertRichTextState(
        stableRichTextState,
        false,
        "electron",
      );
      assert.equal(
        stableRichText.rect.x,
        stableInput.rect.x + 12,
        "stable rich Text Box edit was lost",
      );
      assert.equal(
        stableRichText.rect.y,
        stableInput.rect.y,
        "stable rich Text Box vertical geometry changed",
      );
      assert.equal(
        stableRichTextState.contentSha256,
        stableRichTextInputState.contentSha256,
        "stable Electron changed rich Text Box page content",
      );
      run("qpdf", ["--check", stableRichTextFinal], snapshot);

      const nativeRichText = join(
        outputDirectory,
        "native-rich-text-final.pdf",
      );
      run(
        "python3",
        [
          "scripts/run-native-bounded.py",
          "env",
          ...(developerDirectory
            ? [`DEVELOPER_DIR=${developerDirectory}`]
            : []),
          `SDKROOT=${sdkRoot}`,
          `CC=${clang}`,
          `CXX=${clang}`,
          `BP_ELECTRON_EDITED_RICH_TEXT_FIXTURE=${stableRichTextFinal}`,
          `BP_NATIVE_RICH_TEXT_OUTPUT=${nativeRichText}`,
          "cargo",
          "test",
          "--lib",
          "pdf_engine::tests::electron_edited_rich_text_box_survives_native_edit_and_two_reopens",
          "--",
          "--ignored",
          "--exact",
        ],
        nativeRoot,
      );
      run("qpdf", ["--check", nativeRichText], nativeRoot);

      const bridgeRichTextFinal = join(
        outputDirectory,
        "current-electron-rich-text-bridge-final.pdf",
      );
      const bridgeRichTextFirst = bridgeRichTextFinal.replace(
        /\.pdf$/i,
        ".first.pdf",
      );
      run(
        "pnpm",
        [
          "exec",
          "vitest",
          "run",
          "--config",
          "vitest.config.mts",
          "packages/pdf/src/index.test.ts",
          "-t",
          "edits native-produced rich text through the current Electron writer",
        ],
        repositoryRoot,
        {
          environment: {
            BP_NATIVE_RICH_TEXT_BRIDGE_FIXTURE: nativeRichText,
            BP_ELECTRON_RICH_TEXT_BRIDGE_OUTPUT: bridgeRichTextFinal,
          },
        },
      );
      run("qpdf", ["--check", bridgeRichTextFirst], repositoryRoot);
      run("qpdf", ["--check", bridgeRichTextFinal], repositoryRoot);
      assertSameManifest(
        bridgeBoundary,
        await captureManifest(repositoryRoot),
        "Current Electron rich Text Box bridge source",
      );
      richTextBridgeCandidate = {
        classification: "bridge-candidate-only",
        proves:
          "The source-bound current migration Electron writer consumes a native-produced rich Text Box, preserves its complete 16-run Helvetica/Arimo/Roboto Mono/Tinos styling through two saves, removes unreachable PDF object graphs and keeps each rewrite within the bounded growth allowance.",
        doesNotProve:
          "This does not change or qualify public stable Electron 0.0.11, whose first rich Text Box rewrite still retains unreachable font and appearance graphs and exceeds the compatibility growth gate.",
        source: {
          commit: bridgeCommit,
          dirty: bridgeScopedStatus.length > 0,
          statusSha256: createHash("sha256")
            .update(bridgeScopedStatus)
            .digest("hex"),
          boundarySha256: bridgeBoundary.sha256,
          files: bridgeBoundary.files,
        },
        nativeInputSha256: await sha256File(nativeRichText),
        firstOutputSha256: await sha256File(bridgeRichTextFirst),
        finalOutputSha256: await sha256File(bridgeRichTextFinal),
        growth: {
          inputBytes: (await stat(nativeRichText)).size,
          firstOutputBytes: (await stat(bridgeRichTextFirst)).size,
          finalOutputBytes: (await stat(bridgeRichTextFinal)).size,
        },
      };

      const electronRichTextFirst = join(
        outputDirectory,
        "electron-rich-text-first.pdf",
      );
      const electronRichTextFinal = join(
        outputDirectory,
        "electron-rich-text-final.pdf",
      );
      const electronRichTextJourney = JSON.parse(
        run(
          "node",
          [
            richTextEditor,
            nativeRichText,
            electronRichTextFirst,
            electronRichTextFinal,
          ],
          snapshot,
          { capture: true },
        ),
      );
      electronRichTextInputState = electronRichTextJourney.initial;
      electronRichTextState = electronRichTextJourney.final;
      const electronInput = assertRichTextState(
        electronRichTextInputState,
        true,
        "native",
      );
      const electronRichText = assertRichTextState(
        electronRichTextState,
        true,
        "electron",
      );
      assert.equal(
        electronInput.rect.x,
        stableRichText.rect.x + 7,
        "native rich Text Box horizontal edit was lost",
      );
      assert.equal(
        electronInput.rect.y,
        stableRichText.rect.y + 5,
        "native rich Text Box vertical edit was lost",
      );
      assert.equal(
        electronRichText.rect.x,
        electronInput.rect.x + 12,
        "final Electron rich Text Box edit was lost",
      );
      assert.equal(
        electronRichText.rect.y,
        electronInput.rect.y,
        "final Electron rich Text Box vertical geometry changed",
      );
      assert.equal(
        electronRichTextInputState.contentSha256,
        stableRichTextInputState.contentSha256,
        "native changed rich Text Box page content",
      );
      assert.equal(
        electronRichTextState.contentSha256,
        electronRichTextInputState.contentSha256,
        "final Electron changed rich Text Box page content",
      );
      run("qpdf", ["--check", electronRichTextFinal], snapshot);
      Object.assign(richTextOutputs, {
        stableRichTextFirst,
        stableRichTextFinal,
        nativeRichText,
        bridgeRichTextFirst,
        bridgeRichTextFinal,
        electronRichTextFirst,
        electronRichTextFinal,
      });
    }

    let coordinateSourceSeed;
    let stableCoordinateInputState;
    let stableCoordinateState;
    let electronCoordinateInputState;
    let electronCoordinateState;
    const coordinateOutputs = {};
    if (coordinateSeed) {
      coordinateSourceSeed = {
        bytes: (await stat(coordinateSeed)).size,
        sha256: await sha256File(coordinateSeed),
      };
      const coordinateEditor = join(
        snapshot,
        "packages/pdf/stable-coordinate-space.mjs",
      );
      await writeFile(coordinateEditor, stableCoordinateEditorSource, {
        flag: "wx",
        mode: 0o600,
      });
      const stableCoordinateFirst = join(
        outputDirectory,
        "stable-coordinate-first.pdf",
      );
      const stableCoordinateFinal = join(
        outputDirectory,
        "stable-coordinate-final.pdf",
      );
      const stableCoordinateJourney = JSON.parse(
        run(
          "node",
          [
            coordinateEditor,
            coordinateSeed,
            stableCoordinateFirst,
            stableCoordinateFinal,
          ],
          snapshot,
          { capture: true },
        ),
      );
      stableCoordinateInputState = stableCoordinateJourney.initial;
      stableCoordinateState = stableCoordinateJourney.final;
      const stableInput = assertCoordinateState(
        stableCoordinateInputState,
        true,
      );
      const stableCoordinate = assertCoordinateState(
        stableCoordinateState,
        true,
      );
      assert.deepEqual(stableCoordinate.rectangle.rect, {
        ...stableInput.rectangle.rect,
        x: stableInput.rectangle.rect.x + 12,
      });
      assert.deepEqual(stableCoordinate.length.start, {
        ...stableInput.length.start,
        x: stableInput.length.start.x + 12,
      });
      assert.deepEqual(stableCoordinate.length.end, {
        ...stableInput.length.end,
        x: stableInput.length.end.x + 12,
      });
      assert.equal(
        stableCoordinateState.pageContentSha256,
        stableCoordinateInputState.pageContentSha256,
        "stable Electron changed coordinate fixture page content",
      );

      const nativeCoordinate = join(
        outputDirectory,
        "native-coordinate-final.pdf",
      );
      run(
        "python3",
        [
          "scripts/run-native-bounded.py",
          "env",
          ...(developerDirectory
            ? [`DEVELOPER_DIR=${developerDirectory}`]
            : []),
          `SDKROOT=${sdkRoot}`,
          `CC=${clang}`,
          `CXX=${clang}`,
          `BP_ELECTRON_EDITED_COORDINATE_SPACE_FIXTURE=${stableCoordinateFinal}`,
          `BP_NATIVE_COORDINATE_SPACE_OUTPUT=${nativeCoordinate}`,
          "cargo",
          "test",
          "--lib",
          "pdf_engine::tests::electron_edited_coordinate_space_survives_native_edit_and_two_reopens",
          "--",
          "--ignored",
          "--exact",
        ],
        nativeRoot,
      );

      const electronCoordinateFirst = join(
        outputDirectory,
        "electron-coordinate-first.pdf",
      );
      const electronCoordinateFinal = join(
        outputDirectory,
        "electron-coordinate-final.pdf",
      );
      const electronCoordinateJourney = JSON.parse(
        run(
          "node",
          [
            coordinateEditor,
            nativeCoordinate,
            electronCoordinateFirst,
            electronCoordinateFinal,
          ],
          snapshot,
          { capture: true },
        ),
      );
      electronCoordinateInputState = electronCoordinateJourney.initial;
      electronCoordinateState = electronCoordinateJourney.final;
      const electronInput = assertCoordinateState(
        electronCoordinateInputState,
        true,
      );
      const electronCoordinate = assertCoordinateState(
        electronCoordinateState,
        true,
      );
      assert.deepEqual(electronInput.rectangle.rect, {
        ...stableCoordinate.rectangle.rect,
        x: stableCoordinate.rectangle.rect.x + 7,
        y: stableCoordinate.rectangle.rect.y + 5,
      });
      for (const endpoint of ["start", "end"]) {
        assert.deepEqual(electronInput.length[endpoint], {
          ...stableCoordinate.length[endpoint],
          x: stableCoordinate.length[endpoint].x + 7,
          y: stableCoordinate.length[endpoint].y + 5,
        });
        assert.deepEqual(electronCoordinate.length[endpoint], {
          ...electronInput.length[endpoint],
          x: electronInput.length[endpoint].x + 12,
        });
      }
      assert.deepEqual(electronCoordinate.rectangle.rect, {
        ...electronInput.rectangle.rect,
        x: electronInput.rectangle.rect.x + 12,
      });
      assert.equal(
        electronCoordinateInputState.pageContentSha256,
        stableCoordinateInputState.pageContentSha256,
        "native changed coordinate fixture page content",
      );
      assert.equal(
        electronCoordinateState.pageContentSha256,
        stableCoordinateInputState.pageContentSha256,
        "final Electron changed coordinate fixture page content",
      );
      for (const file of [
        stableCoordinateFirst,
        stableCoordinateFinal,
        nativeCoordinate,
        electronCoordinateFirst,
        electronCoordinateFinal,
      ]) {
        run("qpdf", ["--check", file], snapshot);
      }
      Object.assign(coordinateOutputs, {
        stableCoordinateFirst,
        stableCoordinateFinal,
        nativeCoordinate,
        electronCoordinateFirst,
        electronCoordinateFinal,
      });
    }
    assertSameManifest(
      before,
      await captureManifest(reference),
      "Electron reference",
    );

    const sourceSeed = {
      bytes: (await stat(seed)).size,
      sha256: await sha256File(seed),
    };
    const outputs = {};
    for (const [name, file] of Object.entries({
      stableFirst,
      stableFinal,
      nativeOutput,
      electronFirst,
      electronFinal,
      ...ellipseOutputs,
      ...inkOutputs,
      ...redactOutputs,
      ...richTextOutputs,
      ...coordinateOutputs,
    })) {
      outputs[name] = {
        file: relative(outputDirectory, file),
        bytes: (await stat(file)).size,
        sha256: await sha256File(file),
      };
    }
    const richTextAmplification = richTextSourceSeed
      ? assessRichTextOutputAmplification(
          {
            sourceSeed: richTextSourceSeed.bytes,
            stableFirst: outputs.stableRichTextFirst.bytes,
            stableFinal: outputs.stableRichTextFinal.bytes,
            nativeRichText: outputs.nativeRichText.bytes,
            electronRichTextFirst: outputs.electronRichTextFirst.bytes,
            electronRichTextFinal: outputs.electronRichTextFinal.bytes,
          },
          [
            ["seed", stableRichTextInputState],
            ["stable-final", stableRichTextState],
            ["native-final", electronRichTextInputState],
            ["electron-final", electronRichTextState],
          ],
        )
      : undefined;
    const receipt = {
      schema: "butter-paper/electron-gpui-rotated-media",
      version: 4,
      reference: {
        commit,
        dirty: scopedStatus.length > 0,
        statusSha256,
        boundarySha256: before.sha256,
        files: before.files,
      },
      sourceSeed,
      ellipseSourceSeed,
      inkSourceSeed,
      redactSourceSeed,
      richTextSourceSeed,
      coordinateSourceSeed,
      stableState,
      electronState,
      stableEllipseInputState,
      stableEllipseState,
      electronEllipseInputState,
      electronEllipseState,
      stableInkInputState,
      stableInkState,
      electronInkInputState,
      electronInkState,
      inkBridgeCandidate,
      stableRedactInputState,
      stableRedactState,
      electronRedactInputState,
      electronRedactState,
      stableRichTextInputState,
      stableRichTextState,
      electronRichTextInputState,
      electronRichTextState,
      richTextBridgeCandidate,
      stableCoordinateInputState,
      stableCoordinateState,
      electronCoordinateInputState,
      electronCoordinateState,
      richTextAmplification,
      outputs,
    };
    await writeFile(
      join(outputDirectory, "receipt.json"),
      `${JSON.stringify(receipt, null, 2)}\n`,
      {
        flag: "wx",
        mode: 0o600,
      },
    );
    process.stdout.write(`${JSON.stringify(receipt, null, 2)}\n`);
    if (richTextAmplification && !richTextAmplification.gate.passed) {
      const blockers = richTextAmplification.unresolvedBlockers
        .map(({ leg }) => leg)
        .join(", ");
      throw new Error(
        `rich Text Box output amplification gate failed${blockers ? `; unresolved frozen Electron legs: ${blockers}` : ""}`,
      );
    }
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  await main();
}
