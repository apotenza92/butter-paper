#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { deflateSync } from "node:zlib";

const execFileAsync = promisify(execFile);
const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const fixtureDirectory = resolve(scriptDirectory, "fixtures");

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value).sort().map((key) => [key, canonicalize(value[key])]),
    );
  }
  return value;
}

function canonicalJson(value) {
  return `${JSON.stringify(canonicalize(value), null, 2)}\n`;
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function pdfObject(objectNumber, body) {
  return `${objectNumber} 0 obj\n${body}\nendobj\n`;
}

function generatedPdf(pageCount, fixtureId = null) {
  const objects = [];
  const pageObjectNumbers = [];
  let nextObject = 3;
  for (let index = 0; index < pageCount; index += 1) {
    pageObjectNumbers.push(nextObject);
    nextObject += 2;
  }
  objects.push(pdfObject(1, "<< /Type /Catalog /Pages 2 0 R >>"));
  objects.push(pdfObject(
    2,
    `<< /Type /Pages /Count ${pageCount} /Kids [${pageObjectNumbers.map((number) => `${number} 0 R`).join(" ")}] >>`,
  ));
  for (let index = 0; index < pageCount; index += 1) {
    const pageObject = pageObjectNumbers[index];
    const contentObject = pageObject + 1;
    const inset = 36 + (index % 5) * 2;
    const content = [
      "q",
      "0.82 G 0.5 w",
      `${inset} ${inset} ${612 - inset * 2} ${792 - inset * 2} re S`,
      "0.65 G 0.75 w",
      "54 108 504 576 re S",
      "Q",
      "",
    ].join("\n");
    objects.push(pdfObject(
      pageObject,
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Contents ${contentObject} 0 R >>`,
    ));
    objects.push(pdfObject(
      contentObject,
      `<< /Length ${Buffer.byteLength(content)} >>\nstream\n${content}endstream`,
    ));
  }
  const infoObject = nextObject;
  objects.push(pdfObject(
    infoObject,
    fixtureId
      ? `<< /Title ${pdfLiteral(`Butter Paper deterministic public fixture: ${fixtureId}`)} /Producer (Butter Paper fixture-oracle v1) >>`
      : "<< /Title (Butter Paper deterministic public fixture) /Producer (Butter Paper fixture-oracle v1) >>",
  ));

  let pdf = "%PDF-1.7\n%\xE2\xE3\xCF\xD3\n";
  const offsets = [0];
  for (const object of objects) {
    offsets.push(Buffer.byteLength(pdf, "binary"));
    pdf += object;
  }
  const xrefOffset = Buffer.byteLength(pdf, "binary");
  pdf += `xref\n0 ${objects.length + 1}\n`;
  pdf += "0000000000 65535 f \n";
  for (const offset of offsets.slice(1)) {
    pdf += `${String(offset).padStart(10, "0")} 00000 n \n`;
  }
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R /Info ${infoObject} 0 R >>\n`;
  pdf += `startxref\n${xrefOffset}\n%%EOF\n`;
  return Buffer.from(pdf, "binary");
}

function engineeringSheetPdf(spec) {
  const [page] = spec.document.pages;
  const inset = 36;
  const drawingBottom = 180;
  const drawingTop = page.height - inset;
  const drawingLeft = inset;
  const drawingRight = page.width - inset;
  const commands = [
    "q",
    "0.10 G 1 w",
    `${inset} ${inset} ${page.width - inset * 2} ${page.height - inset * 2} re S`,
    "0.35 G 0.5 w",
  ];

  for (let x = drawingLeft + 72; x < drawingRight; x += 72) {
    commands.push(`${x} ${drawingBottom} m ${x} ${drawingTop} l S`);
  }
  for (let y = drawingBottom + 72; y < drawingTop; y += 72) {
    commands.push(`${drawingLeft} ${y} m ${drawingRight} ${y} l S`);
  }

  commands.push(
    "0.12 G 2 w",
    `${drawingLeft + 108} ${drawingBottom + 108} m ${drawingRight - 108} ${drawingTop - 108} l S`,
    `${drawingLeft + 108} ${drawingTop - 108} m ${drawingRight - 108} ${drawingBottom + 108} l S`,
    `${drawingLeft + 216} ${drawingBottom + 360} m ${drawingRight - 216} ${drawingBottom + 360} l S`,
    "0.10 G 1 w",
    `${page.width - 612} ${inset} 576 126 re S`,
    `${page.width - 612} ${inset + 42} m ${page.width - inset} ${inset + 42} l S`,
    `${page.width - 612} ${inset + 84} m ${page.width - inset} ${inset + 84} l S`,
    `${page.width - 324} ${inset} m ${page.width - 324} ${inset + 126} l S`,
    "Q",
    "",
  );

  const content = commands.join("\n");
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
    `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${page.width} ${page.height}] /Resources << >> /Contents 4 0 R >>`,
    pdfStream("", content),
    `<< /Title ${pdfLiteral(`Butter Paper deterministic public fixture: ${spec.fixture_id}`)} /Producer (Butter Paper fixture-oracle v1) >>`,
  ];
  return buildPdf(objects, 1, 5);
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, "ascii");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])));
  return Buffer.concat([length, typeBytes, data, checksum]);
}

function checkerImage() {
  const width = 512;
  const height = 384;
  const rgb = Buffer.alloc(width * height * 3);
  const colors = [[29, 110, 216], [245, 238, 218]];
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const color = colors[(Math.floor(x / 32) + Math.floor(y / 32)) % 2];
      const offset = (y * width + x) * 3;
      rgb[offset] = color[0];
      rgb[offset + 1] = color[1];
      rgb[offset + 2] = color[2];
      if (Math.abs(x - width / 2) < 3 || Math.abs(y - height / 2) < 3) {
        rgb[offset] = 220;
        rgb[offset + 1] = 38;
        rgb[offset + 2] = 38;
      }
    }
  }
  const scanlines = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y += 1) {
    const target = y * (width * 3 + 1);
    scanlines[target] = 0;
    rgb.copy(scanlines, target + 1, y * width * 3, (y + 1) * width * 3);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const png = Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(scanlines, { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
  return { id: "bp-image-checker-v1", width, height, rgb, png };
}

function pdfLiteral(value) {
  return `(${String(value).replaceAll("\\", "\\\\").replaceAll("(", "\\(").replaceAll(")", "\\)")})`;
}

function pdfStream(dictionary, bytes) {
  const data = Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes, "binary");
  return Buffer.concat([
    Buffer.from(`<< ${dictionary} /Length ${data.length} >>\nstream\n`, "binary"),
    data,
    Buffer.from("endstream", "binary"),
  ]);
}

function buildPdf(objects, rootObject, infoObject) {
  const chunks = [Buffer.from("%PDF-1.7\n%\xE2\xE3\xCF\xD3\n", "binary")];
  const offsets = [0];
  let byteOffset = chunks[0].length;
  for (let index = 0; index < objects.length; index += 1) {
    const objectNumber = index + 1;
    const body = Buffer.isBuffer(objects[index])
      ? objects[index]
      : Buffer.from(objects[index], "binary");
    const header = Buffer.from(`${objectNumber} 0 obj\n`, "binary");
    const footer = Buffer.from("\nendobj\n", "binary");
    offsets.push(byteOffset);
    chunks.push(header, body, footer);
    byteOffset += header.length + body.length + footer.length;
  }
  const xrefOffset = byteOffset;
  let xref = `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const offset of offsets.slice(1)) xref += `${String(offset).padStart(10, "0")} 00000 n \n`;
  xref += `trailer\n<< /Size ${objects.length + 1} /Root ${rootObject} 0 R /Info ${infoObject} 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`;
  chunks.push(Buffer.from(xref, "binary"));
  return Buffer.concat(chunks);
}

function annotationAllPdf(spec, checker) {
  const objects = [];
  const reserve = () => (objects.push(null), objects.length);
  const add = (body) => (objects.push(body), objects.length);
  const set = (number, body) => { objects[number - 1] = body; };
  const catalogObject = reserve();
  const pagesObject = reserve();
  const fontObject = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
  const imageData = deflateSync(checker.rgb, { level: 9 });
  const imageObject = add(pdfStream(
    `/Type /XObject /Subtype /Image /Width ${checker.width} /Height ${checker.height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode`,
    imageData,
  ));

  const annotationObjects = [];
  for (const annotation of spec.native_annotations) {
    const { x, y, width, height } = annotation.bounds;
    const rect = `[${x} ${y} ${x + width} ${y + height}]`;
    let appearance;
    let dictionary;
    if (annotation.native_name === "rectangle-1") {
      appearance = pdfStream(
        `/Type /XObject /Subtype /Form /BBox [0 0 ${width} ${height}] /Resources << >>`,
        `q 0.113725 0.431373 0.847059 RG 2 w 1 1 ${width - 2} ${height - 2} re S Q\n`,
      );
      const appearanceObject = add(appearance);
      dictionary = `<< /Type /Annot /Subtype /Square /Rect ${rect} /NM ${pdfLiteral(annotation.native_name)} /C [0.113725 0.431373 0.847059] /BS << /Type /Border /W 2 /S /S >> /AP << /N ${appearanceObject} 0 R >> >>`;
    } else if (annotation.native_name === "highlight-1") {
      const localPoints = annotation.ink_points.map(([px, py]) => [px - x, py - y]);
      const path = localPoints.map(([px, py], index) => `${px} ${py} ${index === 0 ? "m" : "l"}`).join(" ");
      appearance = pdfStream(
        `/Type /XObject /Subtype /Form /BBox [0 0 ${width} ${height}] /Resources << /ExtGState << /GS1 << /Type /ExtGState /CA 0.35 /ca 0.35 /BM /Multiply >> >> >> /Group << /S /Transparency /CS /DeviceRGB >>`,
        `q /GS1 gs 0.980392 0.8 0.082353 RG 16 w 1 J 1 j ${path} S Q\n`,
      );
      const appearanceObject = add(appearance);
      const inkList = annotation.ink_points.flat().join(" ");
      dictionary = `<< /Type /Annot /Subtype /Ink /Rect ${rect} /NM ${pdfLiteral(annotation.native_name)} /InkList [[${inkList}]] /C [0.980392 0.8 0.082353] /CA 0.35 /BM /Multiply /BS << /W 16 /S /S >> /AP << /N ${appearanceObject} 0 R >> >>`;
    } else if (annotation.native_name === "text-1") {
      appearance = pdfStream(
        `/Type /XObject /Subtype /Form /BBox [0 0 ${width} ${height}] /Resources << /Font << /Helv ${fontObject} 0 R >> >>`,
        `q BT /Helv 14 Tf 0.067 0.094 0.153 rg 6 48 Td ${pdfLiteral(annotation.contents)} Tj ET Q\n`,
      );
      const appearanceObject = add(appearance);
      dictionary = `<< /Type /Annot /Subtype /FreeText /Rect ${rect} /NM ${pdfLiteral(annotation.native_name)} /Contents ${pdfLiteral(annotation.contents)} /DA ${pdfLiteral(annotation.default_appearance)} /Q 0 /AP << /N ${appearanceObject} 0 R >> >>`;
    } else if (annotation.native_name === "length-1") {
      appearance = pdfStream(
        `/Type /XObject /Subtype /Form /BBox [0 0 ${width} ${height}] /Resources << /Font << /Helv ${fontObject} 0 R >> >>`,
        `q 0.067 0.094 0.153 RG 2 w 18 18 m 270 18 l S BT /Helv 10 Tf 0.067 0.094 0.153 rg 112 22 Td ${pdfLiteral(annotation.contents)} Tj ET Q\n`,
      );
      const appearanceObject = add(appearance);
      dictionary = `<< /Type /Annot /Subtype /Line /Rect ${rect} /NM ${pdfLiteral(annotation.native_name)} /L [${annotation.line.join(" ")}] /Contents ${pdfLiteral(annotation.contents)} /LE [/None /OpenArrow] /Measure << /Type /Measure /R ${pdfLiteral(annotation.measure.ratio)} /X [<< /U ${pdfLiteral(annotation.measure.unit)} /C 0.013888889 /D ${annotation.measure.precision} >>] >> /AP << /N ${appearanceObject} 0 R >> >>`;
    } else if (annotation.native_name === "image-1") {
      appearance = pdfStream(
        `/Type /XObject /Subtype /Form /BBox [0 0 ${width} ${height}] /Resources << /XObject << /Im1 ${imageObject} 0 R >> >>`,
        `q ${width} 0 0 ${height} 0 0 cm /Im1 Do Q\n`,
      );
      const appearanceObject = add(appearance);
      dictionary = `<< /Type /Annot /Subtype /Square /Rect ${rect} /NM ${pdfLiteral(annotation.native_name)} /BPAssetId ${pdfLiteral(annotation.asset_id)} /AP << /N ${appearanceObject} 0 R >> >>`;
    } else {
      appearance = pdfStream(
        `/Type /XObject /Subtype /Form /BBox [0 0 ${width} ${height}] /Resources << >> /BPStreamProbe ${pdfLiteral(annotation.probes.appearance_dictionary)}`,
        annotation.probes.appearance_content_ascii,
      );
      const appearanceObject = add(appearance);
      dictionary = `<< /Type /Annot /Subtype /Text /Rect ${rect} /NM ${pdfLiteral(annotation.native_name)} /Contents ${pdfLiteral(annotation.contents)} /BPUnknown ${pdfLiteral(annotation.probes.dictionary)} /AP << /N ${appearanceObject} 0 R >> >>`;
    }
    annotationObjects.push(add(dictionary));
  }

  const pageObjects = [];
  for (const [index, page] of spec.document.pages.entries()) {
    const content = `q 0.82 G 0.5 w 36 36 ${page.width - 72} ${page.height - 72} re S 0.65 G 0.75 w 54 108 ${page.width - 108} ${page.height - 216} re S Q\n`;
    const contentObject = add(pdfStream("", content));
    const annotations = index === 0
      ? ` /Annots [${annotationObjects.map((number) => `${number} 0 R`).join(" ")}]`
      : "";
    const cropBox = page.crop_box ? ` /CropBox [${page.crop_box.join(" ")}]` : "";
    const rotate = page.rotation ? ` /Rotate ${page.rotation}` : "";
    const userUnit = page.user_unit ? ` /UserUnit ${page.user_unit}` : "";
    pageObjects.push(add(`<< /Type /Page /Parent ${pagesObject} 0 R /MediaBox [0 0 ${page.width} ${page.height}]${cropBox}${rotate}${userUnit} /Resources << >> /Contents ${contentObject} 0 R${annotations} >>`));
  }
  set(pagesObject, `<< /Type /Pages /Count ${pageObjects.length} /Kids [${pageObjects.map((number) => `${number} 0 R`).join(" ")}] >>`);
  set(catalogObject, `<< /Type /Catalog /Pages ${pagesObject} 0 R >>`);
  const infoObject = add("<< /Title (Butter Paper all-annotation public fixture) /Producer (Butter Paper fixture-oracle v1) /BPMetadataProbe (preserve-document-metadata) >>");
  return buildPdf(objects, catalogObject, infoObject);
}

async function loadFixture(id) {
  return JSON.parse(await readFile(resolve(fixtureDirectory, `${id}.fixture.json`), "utf8"));
}

function pageId(fixtureId, pageNumber) {
  return `${fixtureId}:page:${String(pageNumber).padStart(3, "0")}`;
}

function densityBounds(pageNumber, localIndex) {
  if (pageNumber === 2) {
    return {
      x: 48 + (localIndex % 10) * 51,
      y: 96 + Math.floor(localIndex / 10) * 60,
      width: 36,
      height: 24,
    };
  }
  if (pageNumber <= 20) {
    return {
      x: 72 + (localIndex % 5) * 96,
      y: 180 + Math.floor(localIndex / 5) * 180,
      width: 48,
      height: 36,
    };
  }
  return {
    x: 96 + (localIndex % 3) * 156,
    y: 144 + Math.floor(localIndex / 3) * 216,
    width: 60,
    height: 48,
  };
}

function densityStyle(globalIndex) {
  const colors = [
    [0.113725, 0.431373, 0.847059, 1],
    [0.862745, 0.14902, 0.14902, 1],
    [0.078431, 0.647059, 0.352941, 1],
    [0.694118, 0.333333, 0.870588, 1],
  ];
  const stroke = colors[globalIndex % colors.length];
  return {
    stroke_rgba: stroke,
    fill_rgba: [...stroke.slice(0, 3), 0.08 + (globalIndex % 3) * 0.02],
    stroke_width_pt: 1 + (globalIndex % 3) * 0.5,
    stroke_style: globalIndex % 5 === 0 ? "dashed" : "solid",
  };
}

function materializeDensitySpec(spec) {
  const annotations = [];
  const commands = [];
  let globalIndex = 0;
  for (let pageNumber = 2; pageNumber <= spec.document.page_count; pageNumber += 1) {
    const count = pageNumber === 2 ? 100 : pageNumber <= 20 ? 10 : 9;
    for (let localIndex = 0; localIndex < count; localIndex += 1) {
      const page = pageId(spec.fixture_id, pageNumber);
      const suffix = String(localIndex + 1).padStart(4, "0");
      const annotationId = `${spec.fixture_id}:p${String(pageNumber).padStart(3, "0")}:rectangle:${suffix}`;
      const bounds = densityBounds(pageNumber, localIndex);
      const style = densityStyle(globalIndex);
      commands.push({
        command_id: `density:create:p${String(pageNumber).padStart(3, "0")}:${suffix}`,
        operation: "create-rectangle",
        annotation_id: annotationId,
        page_id: page,
        bounds_pdf: bounds,
        style,
      });
      annotations.push({
        annotation_id: annotationId,
        type: "rectangle",
        page_id: page,
        bounds,
        style,
      });
      globalIndex += 1;
    }
  }
  if (annotations.length !== spec.annotation_generator.total_annotation_count) {
    throw new Error(`${spec.fixture_id}: annotation recipe produced ${annotations.length} entries`);
  }
  return {
    ...spec,
    commands,
    canonical_expected: {
      ...spec.canonical_expected,
      state: {
        schema_version: "bp-canonical-annotation-state-v1",
        fixture_id: spec.fixture_id,
        document: {
          page_count: spec.document.page_count,
          page_ids: Array.from(
            { length: spec.document.page_count },
            (_, index) => pageId(spec.fixture_id, index + 1),
          ),
        },
        annotations,
        selected_annotation_ids: [],
        history: { undo_depth: annotations.length, redo_depth: 0 },
        dirty: true,
      },
    },
  };
}

function materializeSpec(spec) {
  if (spec.annotation_generator) return materializeDensitySpec(spec);
  if (spec.document_generator?.expand_page_ids) {
    return {
      ...spec,
      canonical_expected: {
        ...spec.canonical_expected,
        state: {
          ...spec.canonical_expected.state,
          document: {
            ...spec.canonical_expected.state.document,
            page_ids: Array.from(
              { length: spec.document.page_count },
              (_, index) => pageId(spec.fixture_id, index + 1),
            ),
          },
        },
      },
    };
  }
  if (spec.native_annotation_generator?.copy_annotations_to_canonical_state) {
    return {
      ...spec,
      canonical_expected: {
        ...spec.canonical_expected,
        state: {
          ...spec.canonical_expected.state,
          annotations: spec.native_annotations.map((annotation) => ({ ...annotation })),
        },
      },
    };
  }
  return spec;
}

function commandStream(spec) {
  return {
    schema_version: "bp-pdf-command-stream-v1",
    fixture_id: spec.fixture_id,
    coordinate_space: spec.coordinate_space,
    commands: spec.commands,
  };
}

async function writeFixture(spec, output, assets) {
  const expected = spec.canonical_expected.state;
  const expectedBytes = canonicalJson(expected);
  const expectedHash = sha256(expectedBytes);
  const commands = commandStream(spec);
  const commandsBytes = canonicalJson(commands);
  const cropsBytes = canonicalJson({
    schema_version: "bp-visual-crop-oracle-v1",
    fixture_id: spec.fixture_id,
    coordinate_space: spec.coordinate_space,
    crops: spec.visual_crops,
  });
  const pdfBytes = spec.native_annotations
    ? annotationAllPdf(spec, assets.checker)
    : spec.document_generator?.kind === "moderate-engineering-sheet-v1"
      ? engineeringSheetPdf(spec)
      : generatedPdf(
          spec.document.page_count,
          ["bp-single-page-v1", "bp-multi-page-v1"].includes(spec.fixture_id)
            ? spec.fixture_id
            : null,
        );
  const prefix = resolve(output, spec.fixture_id);
  const writes = [
    writeFile(`${prefix}.pdf`, pdfBytes),
    writeFile(`${prefix}.commands.json`, commandsBytes, "utf8"),
    writeFile(`${prefix}.expected.json`, expectedBytes, "utf8"),
    writeFile(`${prefix}.crops.json`, cropsBytes, "utf8"),
  ];
  let nativeArtifact;
  if (spec.native_annotations) {
    const nativeBytes = canonicalJson({
      schema_version: "bp-native-pdf-annotation-oracle-v1",
      fixture_id: spec.fixture_id,
      annotations: spec.native_annotations,
    });
    writes.push(writeFile(`${prefix}.native.json`, nativeBytes, "utf8"));
    nativeArtifact = {
      file: `${spec.fixture_id}.native.json`,
      bytes: Buffer.byteLength(nativeBytes),
      sha256: sha256(nativeBytes),
    };
  }
  await Promise.all(writes);
  return {
    fixture_id: spec.fixture_id,
    page_count: spec.document.page_count,
    annotation_count: expected.annotations.length,
    artifacts: {
      pdf: { file: `${spec.fixture_id}.pdf`, bytes: pdfBytes.length, sha256: sha256(pdfBytes) },
      commands: { file: `${spec.fixture_id}.commands.json`, bytes: Buffer.byteLength(commandsBytes), sha256: sha256(commandsBytes) },
      expected: { file: `${spec.fixture_id}.expected.json`, bytes: Buffer.byteLength(expectedBytes), sha256: expectedHash },
      crops: { file: `${spec.fixture_id}.crops.json`, bytes: Buffer.byteLength(cropsBytes), sha256: sha256(cropsBytes) },
      ...(nativeArtifact ? { native: nativeArtifact } : {}),
    },
  };
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function pageDimensions(spec, page) {
  if (Array.isArray(spec.document.pages)) {
    const record = spec.document.pages.find((candidate) => candidate.page_id === page);
    return record ? { width: record.width, height: record.height } : null;
  }
  const pageNumber = Number(page.slice(-3));
  if (!Number.isInteger(pageNumber) || pageNumber < 1 || pageNumber > spec.document.page_count) {
    return null;
  }
  return { width: spec.document.page_width, height: spec.document.page_height };
}

function assertPdfRect(spec, page, rect, label) {
  const dimensions = pageDimensions(spec, page);
  assert(dimensions, `${spec.fixture_id}: ${label} uses unknown page ${page}`);
  for (const field of ["x", "y", "width", "height"]) {
    assert(Number.isFinite(rect?.[field]), `${spec.fixture_id}: ${label}.${field} must be finite`);
  }
  assert(rect.width > 0 && rect.height > 0, `${spec.fixture_id}: ${label} must have positive size`);
  assert(rect.x >= 0 && rect.y >= 0, `${spec.fixture_id}: ${label} must start inside the page`);
  assert(
    rect.x + rect.width <= dimensions.width && rect.y + rect.height <= dimensions.height,
    `${spec.fixture_id}: ${label} exceeds its PDF page bounds`,
  );
}

function validateSpec(spec) {
  assert(spec.source?.contains_private_material === false, `${spec.fixture_id}: fixture must be public`);
  assert(spec.coordinate_space === "pdf-points-bottom-left", `${spec.fixture_id}: unsupported coordinate space`);
  const expectedBytes = canonicalJson(spec.canonical_expected.state);
  const expectedHash = sha256(expectedBytes);
  assert(
    spec.canonical_expected.sha256 === expectedHash,
    `${spec.fixture_id}: canonical_expected.sha256 does not match canonical state`,
  );

  const commandIds = new Set();
  for (const command of spec.commands) {
    assert(!commandIds.has(command.command_id), `${spec.fixture_id}: duplicate command ID ${command.command_id}`);
    commandIds.add(command.command_id);
    if (command.bounds_pdf) {
      assertPdfRect(spec, command.page_id, command.bounds_pdf, `${command.command_id}.bounds_pdf`);
    }
    if (command.pointer_path_pdf) {
      for (const [index, point] of command.pointer_path_pdf.entries()) {
        const dimensions = pageDimensions(spec, command.page_id);
        assert(
          dimensions && Number.isFinite(point.x) && Number.isFinite(point.y)
            && point.x >= 0 && point.x <= dimensions.width
            && point.y >= 0 && point.y <= dimensions.height,
          `${spec.fixture_id}: ${command.command_id}.pointer_path_pdf[${index}] is outside the page`,
        );
      }
    }
  }

  const annotationIds = new Set();
  for (const annotation of spec.canonical_expected.state.annotations) {
    assert(!annotationIds.has(annotation.annotation_id), `${spec.fixture_id}: duplicate annotation ID ${annotation.annotation_id}`);
    annotationIds.add(annotation.annotation_id);
    assertPdfRect(spec, annotation.page_id, annotation.bounds, `${annotation.annotation_id}.bounds`);
  }
  for (const crop of spec.visual_crops) {
    assertPdfRect(spec, crop.page_id, crop.pdf_rect, `${crop.crop_id}.pdf_rect`);
  }
  if (spec.tiling_oracle) {
    const oracle = spec.tiling_oracle;
    const dimensions = pageDimensions(spec, oracle.page_id);
    assert(dimensions, `${spec.fixture_id}: tiling oracle uses an unknown page`);
    const scale = oracle.zoom_percent / 100 * oracle.display_scale_factor;
    const pagePixelWidth = Math.ceil(dimensions.width * scale);
    const pagePixelHeight = Math.ceil(dimensions.height * scale);
    const viewportPixelWidth = Math.max(
      256,
      Math.min(pagePixelWidth, oracle.viewport_pixel_width_cap),
    );
    assert(
      pagePixelWidth === oracle.expected_page_pixel_width
        && pagePixelHeight === oracle.expected_page_pixel_height,
      `${spec.fixture_id}: tiling oracle page-pixel dimensions are stale`,
    );
    assert(
      viewportPixelWidth === oracle.expected_viewport_pixel_width,
      `${spec.fixture_id}: tiling oracle viewport width is stale`,
    );
    assert(
      (viewportPixelWidth >= oracle.tiled_render_threshold_pixels)
        === oracle.expected_tiled_rendering,
      `${spec.fixture_id}: tiling oracle does not reach the declared threshold`,
    );
    assert(
      Number.isInteger(oracle.maximum_pdf_bytes) && oracle.maximum_pdf_bytes > 0,
      `${spec.fixture_id}: tiling oracle must declare a positive maximum PDF size`,
    );
  }
  if (spec.annotation_generator) {
    assert(
      annotationIds.size === spec.annotation_generator.total_annotation_count,
      `${spec.fixture_id}: canonical annotation count does not match recipe`,
    );
  }
  for (const command of spec.commands.filter((entry) => entry.operation === "assert-canonical-state")) {
    assert(
      command.expected_sha256 === expectedHash,
      `${spec.fixture_id}: ${command.command_id} has stale expected_sha256`,
    );
  }
}

async function validateArtifact(output, artifact) {
  const bytes = await readFile(resolve(output, artifact.file));
  assert(sha256(bytes) === artifact.sha256, `${artifact.file}: sha256 mismatch`);
  assert(bytes.length === artifact.bytes, `${artifact.file}: byte length mismatch`);
  return bytes;
}

const fixtureIds = Object.freeze([
  "bp-rectangle-v1",
  "bp-annotation-density-v1",
  "bp-single-page-v1",
  "bp-multi-page-v1",
  "bp-engineering-sheet-v1",
  "bp-annotation-all-v1",
  "bp-coordinate-space-v1",
]);

async function loadSpecs() {
  return Promise.all(fixtureIds.map(async (id) => materializeSpec(await loadFixture(id))));
}

async function requirePdfValidators() {
  const missing = [];
  for (const [command, versionArguments] of [["qpdf", ["--version"]], ["pdfinfo", ["-v"]]]) {
    try {
      await execFileAsync(command, versionArguments, { timeout: 5_000, maxBuffer: 64_000 });
    } catch (error) {
      if (error.code === "ENOENT") missing.push(command);
      else if (command === "pdfinfo") {
        // Poppler writes its version to stderr and can use a nonzero exit code.
        if (!String(error.stderr ?? "").includes("pdfinfo version")) throw error;
      } else {
        throw error;
      }
    }
  }
  if (missing.length > 0) {
    throw new Error(`BLOCKED: required independent PDF validators unavailable: ${missing.join(", ")}`);
  }
}

async function independentlyValidatePdfs(output, specs) {
  await requirePdfValidators();
  for (const spec of specs) {
    const pdfPath = resolve(output, `${spec.fixture_id}.pdf`);
    await execFileAsync("qpdf", ["--check", pdfPath], { maxBuffer: 256_000 });
    const { stdout: info } = await execFileAsync("pdfinfo", [pdfPath], { maxBuffer: 256_000 });
    const pages = info.match(/^Pages:\s+(\d+)$/m);
    assert(
      Number(pages?.[1]) === spec.document.page_count,
      `${spec.fixture_id}: pdfinfo page count mismatch`,
    );
    if (spec.tiling_oracle) {
      const dimensions = pageDimensions(spec, spec.tiling_oracle.page_id);
      const pageSize = info.match(/^Page size:\s+([\d.]+) x ([\d.]+) pts/m);
      assert(
        Number(pageSize?.[1]) === dimensions.width
          && Number(pageSize?.[2]) === dimensions.height,
        `${spec.fixture_id}: pdfinfo page dimensions do not match the tiling oracle`,
      );
    }
    if (spec.native_annotations?.length > 0) {
      const qdfPath = resolve(output, `.${spec.fixture_id}.qdf.pdf`);
      try {
        await execFileAsync(
          "qpdf",
          ["--qdf", "--object-streams=disable", pdfPath, qdfPath],
          { maxBuffer: 256_000 },
        );
        const qdf = await readFile(qdfPath, "latin1");
        const qdfObjects = qdf.split(/(?=\n?\d+ 0 obj\n)/);
        for (const annotation of spec.native_annotations) {
          const annotationObject = qdfObjects.find((object) =>
            object.includes(`/NM (${annotation.native_name})`),
          );
          assert(
            annotationObject,
            `${spec.fixture_id}: qpdf did not expose native annotation ${annotation.native_name}`,
          );
          assert(
            annotationObject.includes(`/Subtype /${annotation.subtype}`),
            `${spec.fixture_id}: ${annotation.native_name} did not keep subtype ${annotation.subtype}`,
          );
          assert(annotationObject.includes("/AP <<"), `${spec.fixture_id}: ${annotation.native_name} has no appearance dictionary`);
        }
        const unknown = spec.native_annotations.find(({ native_name }) => native_name === "unknown-1");
        const unknownObject = qdfObjects.find((object) => object.includes("/NM (unknown-1)"));
        assert(unknownObject.includes(`/BPUnknown (${unknown.probes.dictionary})`), `${spec.fixture_id}: unknown dictionary probe missing`);
        const appearanceObjectNumber = unknownObject.match(/\/N\s+(\d+) 0 R/)?.[1];
        const appearanceStart = qdf.indexOf(`\n${appearanceObjectNumber} 0 obj\n`);
        const appearanceEnd = qdf.indexOf("\nendobj", appearanceStart);
        const appearanceObject = appearanceStart >= 0 && appearanceEnd > appearanceStart
          ? qdf.slice(appearanceStart, appearanceEnd)
          : null;
        assert(appearanceObject, `${spec.fixture_id}: unknown appearance object missing`);
        assert(appearanceObject.includes(`/BPStreamProbe (${unknown.probes.appearance_dictionary})`), `${spec.fixture_id}: unknown appearance probe missing`);
        assert(appearanceObject.includes(unknown.probes.appearance_content_ascii.trim()), `${spec.fixture_id}: unknown appearance content missing`);
      } finally {
        await rm(qdfPath, { force: true });
      }
    }
  }
  process.stdout.write(`qpdf and pdfinfo independently accepted ${specs.length} PDFs\n`);
}

async function validate(output) {
  const indexPath = resolve(output, "fixture-index.json");
  const index = JSON.parse(await readFile(indexPath, "utf8"));
  assert(index.schema_version === "bp-fixture-bundle-v1", "fixture-index.json: unsupported schema");
  const specs = await loadSpecs();
  assert(index.fixtures.length === specs.length, "fixture-index.json: fixture count mismatch");
  for (const spec of specs) {
    validateSpec(spec);
    const record = index.fixtures.find((fixture) => fixture.fixture_id === spec.fixture_id);
    assert(record, `fixture-index.json: missing ${spec.fixture_id}`);
    assert(record.page_count === spec.document.page_count, `${spec.fixture_id}: page count mismatch`);
    assert(
      record.annotation_count === spec.canonical_expected.state.annotations.length,
      `${spec.fixture_id}: annotation count mismatch`,
    );
    for (const artifact of Object.values(record.artifacts)) {
      await validateArtifact(output, artifact);
    }
    for (const [kind, lockedHash] of Object.entries(spec.artifact_sha256)) {
      assert(
        record.artifacts[kind]?.sha256 === lockedHash,
        `${spec.fixture_id}: ${kind} artifact differs from locked hash`,
      );
    }
    assert(
      record.artifacts.expected.sha256 === spec.canonical_expected.sha256,
      `${spec.fixture_id}: generated canonical hash differs from locked hash`,
    );
    const pdf = await readFile(resolve(output, record.artifacts.pdf.file));
    if (spec.tiling_oracle) {
      assert(
        pdf.length <= spec.tiling_oracle.maximum_pdf_bytes,
        `${spec.fixture_id}: generated PDF exceeds its moderate-size limit`,
      );
    }
    assert(pdf.subarray(0, 8).toString("ascii") === "%PDF-1.7", `${spec.fixture_id}: invalid PDF header`);
    assert(pdf.subarray(-6).toString("ascii") === "%%EOF\n", `${spec.fixture_id}: invalid PDF trailer`);
  }
  const checkerSpec = specs.find(({ fixture_id }) => fixture_id === "bp-annotation-all-v1");
  assert(index.assets?.length === 1, "fixture-index.json: checker asset count mismatch");
  const checkerRecord = index.assets[0];
  const checkerBytes = await validateArtifact(output, checkerRecord);
  assert(checkerRecord.id === "bp-image-checker-v1", "fixture-index.json: checker asset ID mismatch");
  assert(
    checkerSpec.asset_sha256[checkerRecord.id] === checkerRecord.sha256,
    "bp-image-checker-v1: asset differs from locked hash",
  );
  await independentlyValidatePdfs(output, specs);
  process.stdout.write(`Validated ${specs.length} deterministic fixture oracles in ${output}\n`);
}

function parseArguments(argv) {
  const [operation, ...rest] = argv;
  if (operation !== "generate" && operation !== "validate") {
    throw new Error("usage: fixture-oracle.mjs <generate|validate> --output <directory>");
  }
  const outputIndex = rest.indexOf("--output");
  if (outputIndex === -1 || !rest[outputIndex + 1]) throw new Error("--output is required");
  return { operation, output: resolve(rest[outputIndex + 1]) };
}

async function generate(output) {
  await mkdir(output, { recursive: true });
  const specs = await loadSpecs();
  const checker = checkerImage();
  await writeFile(resolve(output, `${checker.id}.png`), checker.png);
  const assets = { checker };
  const fixtures = [];
  for (const spec of specs) fixtures.push(await writeFixture(spec, output, assets));
  await writeFile(resolve(output, "fixture-index.json"), canonicalJson({
    schema_version: "bp-fixture-bundle-v1",
    assets: [{
      id: checker.id,
      file: `${checker.id}.png`,
      bytes: checker.png.length,
      sha256: sha256(checker.png),
      width_pixels: checker.width,
      height_pixels: checker.height,
    }],
    fixtures,
  }), "utf8");
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (options.operation === "validate") {
    await validate(options.output);
    return;
  }
  await generate(options.output);
}

main().catch((error) => {
  process.stderr.write(`fixture-oracle: ${error.message}\n`);
  process.exitCode = 1;
});
