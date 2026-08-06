import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import {
  decodePDFRawStream,
  PDFArray, PDFBool, PDFDict, PDFDocument, PDFHexString, PDFName, PDFNull,
  PDFNumber, PDFRawStream, PDFRef, PDFStream, PDFString,
} from 'pdf-lib';

export async function inspectAnnotations(pdfPath) {
  const pdf = await PDFDocument.load(await readFile(pdfPath), { updateMetadata: false });
  return {
    schema: 'butter-paper/annotation-inspection',
    version: 1,
    pages: pdf.getPages().map((page, pageIndex) => {
      const annots = page.node.Annots();
      const annotations = annots ? annots.asArray().map((value, annotationIndex) => {
        const resolved = pdf.context.lookup(value);
        if (!(resolved instanceof PDFDict)) return { index: annotationIndex, invalid: canonicalObject(resolved, pdf.context) };
        return {
          index: annotationIndex,
          ref: value instanceof PDFRef ? value.toString() : null,
          subtype: nameValue(resolved.get(PDFName.of('Subtype'))),
          intent: nameValue(resolved.get(PDFName.of('IT'))),
          intentEx: nameValue(resolved.get(PDFName.of('ITEx'))),
          blendMode: nameValue(resolved.get(PDFName.of('BM'))),
          measure: Boolean(resolved.get(PDFName.of('Measure'))),
          name: textValue(resolved.get(PDFName.of('NM'))),
          inReplyTo: annotationReferenceName(resolved.get(PDFName.of('IRT')), pdf.context),
          replyType: nameValue(resolved.get(PDFName.of('RT'))),
          groupNesting: textArrayValue(resolved.get(PDFName.of('GroupNesting')), pdf.context),
          canonical: canonicalObject(resolved, pdf.context, { omitKeys: new Set(['AP']) }),
          canonicalHash: hashCanonical(canonicalObject(resolved, pdf.context, { omitKeys: new Set(['AP']) })),
          appearances: inspectAppearances(resolved.get(PDFName.of('AP')), pdf.context),
        };
      }) : [];
      return { page: pageIndex + 1, annotations };
    }),
  };
}

export function canonicalObject(value, context, options = {}, state = { refs: new Map() }) {
  if (value instanceof PDFRef) {
    const key = value.toString();
    const resolved = context.lookup(value);
    const stableTarget = stableReferenceTarget(resolved);
    if (stableTarget) return stableTarget;
    if (state.refs.has(key)) return { $ref: state.refs.get(key) };
    const ordinal = state.refs.size + 1;
    state.refs.set(key, ordinal);
    return { $indirect: ordinal, value: canonicalObject(resolved, context, options, state) };
  }
  if (value instanceof PDFDict) {
    const result = {};
    const omitKeys = options.omitKeys ?? new Set();
    for (const key of [...value.keys()].sort((a, b) => a.asString().localeCompare(b.asString()))) {
      const name = key.asString().replace(/^\//, '');
      if (!omitKeys.has(name)) result[name] = canonicalObject(value.get(key), context, options, state);
    }
    return result;
  }
  if (value instanceof PDFArray) return value.asArray().map((item) => canonicalObject(item, context, options, state));
  if (value instanceof PDFName) return { $name: value.asString().replace(/^\//, '') };
  if (value instanceof PDFString || value instanceof PDFHexString) return { $string: value.decodeText() };
  if (value instanceof PDFNumber) return value.asNumber();
  if (value instanceof PDFBool) return value.asBoolean();
  if (value === PDFNull) return null;
  if (value instanceof PDFStream) return { $stream: streamDescriptor(value, context) };
  return value === undefined ? null : { $pdf: String(value) };
}

export function hashCanonical(value) {
  return sha256(Buffer.from(stableStringify(value)));
}

function inspectAppearances(value, context) {
  if (!value) return [];
  const resolved = value instanceof PDFRef ? context.lookup(value) : value;
  const results = [];
  walkAppearance(resolved, context, 'AP', new Set(), results);
  return results.sort((a, b) => a.path.localeCompare(b.path));
}

function walkAppearance(value, context, path, seen, results) {
  if (value instanceof PDFRef) {
    const key = value.toString();
    if (seen.has(key)) return;
    const nextSeen = new Set(seen); nextSeen.add(key);
    walkAppearance(context.lookup(value), context, `${path}@${key}`, nextSeen, results);
  } else if (value instanceof PDFStream) {
    results.push({ path, ...streamDescriptor(value, context) });
  } else if (value instanceof PDFDict) {
    for (const key of [...value.keys()].sort((a, b) => a.asString().localeCompare(b.asString()))) {
      walkAppearance(value.get(key), context, `${path}/${key.asString().replace(/^\//, '')}`, seen, results);
    }
  }
}

function streamDescriptor(stream, context) {
  const stored = stream instanceof PDFRawStream ? stream.contents : (stream.getContents?.() ?? Buffer.from(String(stream)));
  let decoded = stored;
  if (stream instanceof PDFRawStream) {
    try { decoded = decodePDFRawStream(stream).decode(); } catch { /* Preserve an exact stored-stream hash when a proprietary filter cannot be decoded. */ }
  }
  return {
    sha256: sha256(decoded),
    storedSha256: sha256(stored),
    length: decoded.length,
    storedLength: stored.length,
    dictionaryHash: hashCanonical(canonicalObject(stream.dict, context)),
  };
}

function stableReferenceTarget(value) {
  if (!(value instanceof PDFDict)) return null;
  const type = nameValue(value.get(PDFName.of('Type')));
  if (type === 'Page') return { $refType: 'Page' };
  const subtype = nameValue(value.get(PDFName.of('Subtype')));
  if (subtype) {
    const name = textValue(value.get(PDFName.of('NM')));
    if (name) return { $refType: 'Annotation', name, subtype };
  }
  return null;
}

function stableStringify(value) {
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${stableStringify(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
}

function textValue(value) {
  return value instanceof PDFString || value instanceof PDFHexString ? value.decodeText() : null;
}

function nameValue(value) {
  return value instanceof PDFName ? value.asString().replace(/^\//, '') : null;
}

function annotationReferenceName(value, context) {
  if (!value) return null;
  const resolved = value instanceof PDFRef ? context.lookup(value) : value;
  return resolved instanceof PDFDict ? textValue(resolved.get(PDFName.of('NM'))) : null;
}

function textArrayValue(value, context) {
  const resolved = value instanceof PDFRef ? context.lookup(value) : value;
  if (!(resolved instanceof PDFArray)) return [];
  return resolved.asArray().map((item) => {
    const entry = item instanceof PDFRef ? context.lookup(item) : item;
    return textValue(entry) ?? nameValue(entry);
  }).filter((entry) => entry !== null);
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}
