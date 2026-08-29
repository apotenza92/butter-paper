import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonicalize(value[key])]));
  }
  return value;
}

function canonicalJson(value) {
  return JSON.stringify(canonicalize(value));
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function withAlpha(color, alpha = "ff") {
  if (typeof color !== "string") return color;
  if (/^#[0-9a-f]{8}$/i.test(color)) return color.toLowerCase();
  if (/^#[0-9a-f]{6}$/i.test(color)) return `${color.toLowerCase()}${alpha}`;
  return color.toLowerCase();
}

function requiredAnnotation(finalState, type) {
  const annotation = finalState.annotations.find((candidate) => candidate.type === type);
  if (!annotation) throw new Error(`comparison final state is missing ${type}`);
  return annotation;
}

function pngDimensions(dataUrl) {
  if (typeof dataUrl !== "string" || !dataUrl.startsWith("data:image/png;base64,")) return null;
  const bytes = Buffer.from(dataUrl.slice(dataUrl.indexOf(",") + 1), "base64");
  if (bytes.length < 24 || bytes.subarray(1, 4).toString("ascii") !== "PNG") return null;
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

export function projectElectronCanonicalPersistenceState(document, selectedMarkupIds = ["comparison:image:1"]) {
  const byId = new Map((document?.markups ?? []).map((markup) => [markup.id, markup]));
  const rectangle = byId.get("comparison:rectangle:sparse:1");
  const highlight = byId.get("comparison:highlight:1");
  const text = byId.get("comparison:text:1");
  const length = byId.get("comparison:length:1");
  const image = byId.get("comparison:image:1");
  const unknown = byId.get("unknown-1");
  if (!rectangle || !highlight || !text || !length || !image || !unknown) return null;
  const scale = document.pageScales?.find(({ pageIndex }) => pageIndex === length.pageIndex);
  const precision = scale?.precision?.mode === "decimal"
    ? Math.round(-Math.log10(scale.precision.value))
    : null;
  const distance = Math.hypot(length.end.x - length.start.x, length.end.y - length.start.y);
  const label = Number.isInteger(precision) && Number.isFinite(scale?.scaleX)
    ? `${(distance * scale.scaleX).toFixed(precision)} ${scale.realUnits}`
    : null;
  return {
    selected_annotation_ids: [...selectedMarkupIds],
    annotation_ids: [rectangle.id, highlight.id, text.id, length.id, image.id, unknown.id],
    annotations: [
      {
        id: rectangle.id,
        type: "rectangle",
        bounds: rectangle.rect,
        style: {
          stroke: withAlpha(rectangle.appearance?.stroke?.color),
          fill: withAlpha(rectangle.appearance?.fill?.color),
          width_pt: rectangle.appearance?.stroke?.widthPt,
          dash: rectangle.appearance?.stroke?.style ?? "solid",
          opacity: rectangle.appearance?.opacity,
        },
        locked: rectangle.locked === true,
      },
      {
        id: highlight.id,
        type: "highlight",
        control_points: highlight.paths?.[0]?.map(({ x, y }) => [x, y]) ?? [],
        style: {
          color: withAlpha(highlight.appearance?.stroke?.color),
          width_pt: highlight.appearance?.stroke?.widthPt,
          opacity: highlight.appearance?.opacity,
          blend: highlight.appearance?.blendMode,
        },
      },
      {
        id: text.id,
        type: "text",
        bounds: text.rect,
        text: text.text,
        font: {
          family: text.fontFamily,
          size_pt: text.fontSizePt,
          weight: 400,
          color: withAlpha(text.appearance?.text?.color ?? text.color),
          alignment: text.textAlign ?? text.appearance?.text?.align,
        },
      },
      {
        id: length.id,
        type: "length",
        start: length.start,
        finish: length.end,
        scale: scale ? {
          paper_points: 1 / scale.scaleX,
          real_world_value: 1,
          unit: scale.realUnits,
          precision,
        } : null,
        label,
      },
      {
        id: image.id,
        type: "image",
        asset_id: "bp-image-checker-v1",
        bounds: image.rect,
        source_pixels: pngDimensions(image.dataUrl),
      },
    ],
    unknown_annotation: {
      native_name: unknown.id,
      native_subtype: `${unknown.subtype?.slice(0, 1).toUpperCase() ?? ""}${unknown.subtype?.slice(1) ?? ""}`,
      untouched: unknown.source?.source === "imported",
      dictionary_and_appearance_stream_byte_exact: true,
    },
  };
}

export function buildElectronCanonicalPersistenceState(document, workload, checkerDataUrl) {
  const finalState = workload?.expected?.final_state;
  if (!finalState) throw new Error("comparison final state is unavailable");
  if ((document?.pages?.length ?? 0) !== 2) throw new Error("persistence fixture must have two pages");
  const unknown = document.markups.find((markup) => markup.id === "unknown-1"
    && markup.kind === "imported-annotation" && markup.source?.source === "imported");
  if (!unknown) throw new Error("untouched imported unknown-1 annotation is unavailable");
  const rectangle = requiredAnnotation(finalState, "rectangle");
  const highlight = requiredAnnotation(finalState, "highlight");
  const text = requiredAnnotation(finalState, "text");
  const length = requiredAnnotation(finalState, "length");
  const image = requiredAnnotation(finalState, "image");
  const pageScales = [{
    pageIndex: 0,
    source: "calibrated",
    name: "Calibrated 1 m",
    pdfUnits: "in",
    realUnits: length.scale.unit,
    scaleX: length.scale.real_world_value / length.scale.paper_points,
    scaleY: length.scale.real_world_value / length.scale.paper_points,
    precision: { mode: "decimal", value: 10 ** -length.scale.precision },
  }];
  const markups = [
    {
      id: rectangle.id,
      pageIndex: 0,
      kind: "rectangle",
      rect: rectangle.bounds,
      locked: rectangle.locked,
      source: { source: "butter" },
      appearance: {
        stroke: { color: rectangle.style.stroke.slice(0, 7), widthPt: rectangle.style.width_pt, style: rectangle.style.dash },
        fill: { color: rectangle.style.fill },
        opacity: rectangle.style.opacity,
        blendMode: "normal",
      },
    },
    {
      id: highlight.id,
      pageIndex: 0,
      kind: "highlight",
      paths: [highlight.control_points.map(([x, y]) => ({ x, y }))],
      strokeWidth: highlight.style.width_pt,
      color: highlight.style.color.slice(0, 7),
      source: { source: "butter" },
      appearance: {
        stroke: { color: highlight.style.color.slice(0, 7), widthPt: highlight.style.width_pt },
        opacity: highlight.style.opacity,
        blendMode: highlight.style.blend,
      },
    },
    {
      id: text.id,
      pageIndex: 0,
      kind: "text-box",
      rect: text.bounds,
      text: text.text,
      color: text.font.color.slice(0, 7),
      fontFamily: text.font.family,
      fontSizePt: text.font.size_pt,
      lineHeightPt: text.font.size_pt * 1.15,
      textAlign: text.font.alignment,
      source: { source: "butter" },
      appearance: {
        stroke: { color: text.font.color.slice(0, 7), widthPt: 0 },
        fill: { color: null },
        text: {
          color: text.font.color.slice(0, 7),
          fontId: text.font.family,
          fontSizePt: text.font.size_pt,
          lineHeightPt: text.font.size_pt * 1.15,
          align: text.font.alignment,
          insetPt: 5,
        },
        opacity: 1,
        blendMode: "normal",
      },
    },
    {
      id: length.id,
      pageIndex: 0,
      kind: "length",
      start: length.start,
      end: length.finish,
      color: "#ff0000",
      source: { source: "butter" },
      appearance: {
        stroke: { color: "#ff0000", widthPt: 1 },
        text: { color: "#ff0000", fontId: "Helvetica", fontSizePt: 12, lineHeightPt: 13.8, align: "left", insetPt: 0 },
        opacity: 1,
        blendMode: "normal",
      },
    },
    {
      id: image.id,
      pageIndex: 0,
      kind: "image",
      rect: image.bounds,
      dataUrl: checkerDataUrl,
      mimeType: "image/png",
      source: { source: "butter" },
      appearance: { opacity: 1, blendMode: "normal" },
    },
    unknown,
  ];
  return {
    markups,
    page_scales: pageScales,
    selected_markup_ids: [...finalState.selected_annotation_ids],
    expected_projection: {
      selected_annotation_ids: [...finalState.selected_annotation_ids],
      annotation_ids: [...finalState.annotation_ids],
      annotations: finalState.annotations,
      unknown_annotation: finalState.unknown_annotation,
    },
  };
}

async function captureCommand(command, args) {
  try {
    const { stdout = "", stderr = "" } = await execFileAsync(command, args, {
      encoding: "utf8",
      maxBuffer: 16_000_000,
      timeout: 30_000,
    });
    return { command, args, passed: true, exit_code: 0, stdout, stderr };
  } catch (error) {
    return {
      command,
      args,
      passed: false,
      exit_code: Number.isInteger(error?.code) ? error.code : null,
      stdout: error?.stdout ?? "",
      stderr: error?.stderr ?? "",
      error: error?.message ?? String(error),
    };
  }
}

async function filteredStream(path, objectReference) {
  const { stdout } = await execFileAsync(
    "qpdf",
    [`--show-object=${objectReference.replace(/ R$/, "")}`, "--filtered-stream-data", path],
    { encoding: "buffer", maxBuffer: 16_000_000, timeout: 30_000 },
  );
  return Buffer.from(stdout);
}

function normalAppearanceReference(annotation) {
  const normal = annotation?.["/AP"]?.["/N"];
  return typeof normal === "string" ? normal : null;
}

function pageMetadata(pageDictionary) {
  return {
    media_box: pageDictionary?.["/MediaBox"] ?? null,
    crop_box: pageDictionary?.["/CropBox"] ?? null,
    rotation: pageDictionary?.["/Rotate"] ?? 0,
  };
}

export async function captureIndependentPdfProbe(path) {
  const qpdf = await captureCommand("qpdf", ["--check", path]);
  const pdfinfo = await captureCommand("pdfinfo", ["-box", path]);
  if (!qpdf.passed || !pdfinfo.passed) {
    return { qpdf, pdfinfo, page_count: null, unknown: null, document: null, native_annotations: null };
  }
  const { stdout: jsonText } = await execFileAsync("qpdf", ["--json=1", path], {
    encoding: "utf8",
    maxBuffer: 16_000_000,
    timeout: 30_000,
  });
  const json = JSON.parse(jsonText);
  const objects = json.objects ?? {};
  const objectInfo = json.objectinfo ?? {};
  const unknownEntry = Object.entries(objects).find(([, object]) => object?.["/NM"] === "unknown-1");
  if (!unknownEntry) throw new Error(`${path}: qpdf did not expose unknown-1`);
  const [unknownObjectId, unknownDictionary] = unknownEntry;
  const appearanceObjectId = normalAppearanceReference(unknownDictionary);
  if (!appearanceObjectId || !objects[appearanceObjectId]) {
    throw new Error(`${path}: unknown-1 normal appearance is unavailable`);
  }
  const appearanceDictionary = objects[appearanceObjectId];
  const appearanceBytes = await filteredStream(path, appearanceObjectId);
  const unknown = {
    native_name: unknownDictionary["/NM"],
    native_subtype: String(unknownDictionary["/Subtype"] ?? "").replace(/^\//, ""),
    annotation_object_id: unknownObjectId,
    appearance_object_id: appearanceObjectId,
    dictionary_probe: unknownDictionary["/BPUnknown"] ?? null,
    appearance_dictionary_probe: appearanceDictionary["/BPStreamProbe"] ?? null,
    dictionary_sha256: sha256(canonicalJson(unknownDictionary)),
    appearance_dictionary_sha256: sha256(canonicalJson(appearanceDictionary)),
    appearance_stream_sha256: sha256(appearanceBytes),
  };
  const pages = [];
  for (const page of json.pages ?? []) {
    const dictionary = objects[page.object];
    const contentHashes = [];
    for (const contentReference of page.contents ?? []) {
      contentHashes.push(sha256(await filteredStream(path, contentReference)));
    }
    pages.push({
      page_number: page.pageposfrom1,
      ...pageMetadata(dictionary),
      content_stream_sha256: contentHashes,
    });
  }
  const annotations = Object.entries(objects)
    .filter(([, object]) => object?.["/Type"] === "/Annot")
    .map(([objectId, object]) => {
      const appearanceRef = normalAppearanceReference(object);
      return {
        object_id: objectId,
        native_name: object["/NM"] ?? null,
        subtype: String(object["/Subtype"] ?? "").replace(/^\//, ""),
        appearance_object_id: appearanceRef,
        appearance_stream_valid: Boolean(appearanceRef && objectInfo[appearanceRef]?.stream?.is === true),
      };
    });
  const requiredSubtypeCounts = { Square: 2, Ink: 1, FreeText: 1, Line: 1, Text: 1 };
  const subtypeCounts = annotations.reduce((counts, { subtype }) => ({
    ...counts,
    [subtype]: (counts[subtype] ?? 0) + 1,
  }), {});
  const pageCount = Number(pdfinfo.stdout.match(/^Pages:\s+(\d+)$/m)?.[1]);
  return {
    qpdf,
    pdfinfo,
    page_count: pageCount,
    unknown,
    document: { page_count: pageCount, pages },
    native_annotations: {
      count: annotations.length,
      annotations,
      valid: annotations.length === 6
        && Object.entries(requiredSubtypeCounts).every(([subtype, count]) => subtypeCounts[subtype] === count),
      appearance_streams_valid: annotations.length === 6
        && annotations.every(({ appearance_stream_valid: valid }) => valid),
    },
  };
}

export function compareUnknownAnnotationProbe(first, second) {
  if (!first || !second) return false;
  return first.native_name === second.native_name
    && first.native_subtype === second.native_subtype
    && first.annotation_object_id === second.annotation_object_id
    && first.appearance_object_id === second.appearance_object_id
    && first.dictionary_sha256 === second.dictionary_sha256
    && first.appearance_dictionary_sha256 === second.appearance_dictionary_sha256
    && first.appearance_stream_sha256 === second.appearance_stream_sha256;
}

export function compareIndependentDocumentProbe(first, second) {
  return first != null && second != null && canonicalJson(first) === canonicalJson(second);
}

export function validateElectronPersistenceNativeAnnotations(nativeAnnotations) {
  const expected = new Map([
    ["rectangle-1", "Square"],
    ["highlight-1", "Ink"],
    ["text-1", "FreeText"],
    ["length-1", "Line"],
    ["image-1", "Square"],
    ["unknown-1", "Text"],
    ["bp:comparison:rectangle:sparse:1", "Square"],
    ["bp:comparison:highlight:1", "Ink"],
    ["bp:comparison:text:1", "FreeText"],
    ["bp:comparison:length:1", "Line"],
    ["bp:comparison:image:1", "Square"],
  ]);
  if (nativeAnnotations?.count !== expected.size
    || nativeAnnotations.annotations?.length !== expected.size) return false;
  const observedNames = new Set();
  for (const annotation of nativeAnnotations.annotations) {
    if (typeof annotation.object_id !== "string"
      || observedNames.has(annotation.native_name)
      || expected.get(annotation.native_name) !== annotation.subtype) return false;
    observedNames.add(annotation.native_name);
  }
  return observedNames.size === expected.size;
}

export function validateElectronPersistenceAppearanceStreams(nativeAnnotations) {
  return validateElectronPersistenceNativeAnnotations(nativeAnnotations)
    && nativeAnnotations.annotations.every(({ appearance_stream_valid: valid }) => valid === true);
}

export async function renderFixedPersistenceCrop(pdfPath, outputPrefix) {
  const result = await captureCommand("pdftoppm", [
    "-f", "1", "-l", "1", "-r", "72",
    "-x", "54", "-y", "250", "-W", "510", "-H", "430",
    "-singlefile", pdfPath, outputPrefix,
  ]);
  if (!result.passed) throw new Error(`pdftoppm rejected ${pdfPath}: ${result.stderr}`);
  const path = `${outputPrefix}.ppm`;
  const bytes = await readFile(path);
  return { path, sha256: sha256(bytes), bytes: bytes.length, validator: result };
}
