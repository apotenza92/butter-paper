import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { loadComparisonWorkload } from "./comparison-workload.mjs";
import {
  buildElectronCanonicalPersistenceState,
  captureIndependentPdfProbe,
  compareIndependentDocumentProbe,
  compareUnknownAnnotationProbe,
  projectElectronCanonicalPersistenceState,
  validateElectronPersistenceAppearanceStreams,
  validateElectronPersistenceNativeAnnotations,
} from "./electron-persistence-evidence.mjs";

const workload = await loadComparisonWorkload();
const fixturePdf = new URL(
  "./results/public-fixtures-v1/bp-annotation-all-v1.pdf",
  import.meta.url,
).pathname;
const checkerPng = new URL("./results/public-fixtures-v1/bp-image-checker-v1.png", import.meta.url).pathname;

test("builds the exact five typed annotations while preserving the imported unknown object", async () => {
  const unknown = {
    id: "unknown-1",
    pageIndex: 0,
    kind: "imported-annotation",
    subtype: "text",
    rect: { x: 360, y: 144, width: 24, height: 24 },
    source: { source: "imported", annotationId: "unknown-1", originalFingerprint: "locked" },
  };
  const state = buildElectronCanonicalPersistenceState({
    pages: [{ index: 0 }, { index: 1 }],
    markups: [unknown],
    pageScales: [],
  }, workload, `data:image/png;base64,${(await readFile(checkerPng)).toString("base64")}`);

  assert.equal(state.markups.length, 6);
  assert.equal(state.markups.at(-1), unknown);
  assert.deepEqual(state.markups.map(({ id }) => id), workload.expected.final_state.annotation_ids);
  assert.deepEqual(state.selected_markup_ids, ["comparison:image:1"]);
  assert.deepEqual(state.page_scales, [{
    pageIndex: 0,
    source: "calibrated",
    name: "Calibrated 1 m",
    pdfUnits: "in",
    realUnits: "m",
    scaleX: 1 / 72,
    scaleY: 1 / 72,
    precision: { mode: "decimal", value: 0.01 },
  }]);
  assert.deepEqual(
    projectElectronCanonicalPersistenceState({
      pages: [{ index: 0 }, { index: 1 }],
      markups: state.markups,
      pageScales: state.page_scales,
    }),
    state.expected_projection,
  );
});

test("independently snapshots the locked unknown dictionary, appearance, page data, and native annotations", async () => {
  const probe = await captureIndependentPdfProbe(fixturePdf);

  assert.equal(probe.qpdf.passed, true);
  assert.equal(probe.pdfinfo.passed, true);
  assert.equal(probe.page_count, 2);
  assert.equal(probe.unknown.native_name, "unknown-1");
  assert.equal(probe.unknown.native_subtype, "Text");
  assert.equal(probe.unknown.dictionary_probe, "unknown-dictionary-preserve-me");
  assert.equal(probe.unknown.appearance_dictionary_probe, "unknown-stream-preserve-me");
  assert.equal(probe.unknown.appearance_stream_sha256.length, 64);
  assert.equal(probe.native_annotations.valid, true);
  assert.equal(probe.native_annotations.appearance_streams_valid, true);
  assert.equal(compareUnknownAnnotationProbe(probe.unknown, probe.unknown), true);
  assert.equal(compareIndependentDocumentProbe(probe.document, probe.document), true);
});

test("validates the source annotations plus the five canonical Electron annotations as one exact inventory", () => {
  const source = [
    ["rectangle-1", "Square"],
    ["highlight-1", "Ink"],
    ["text-1", "FreeText"],
    ["length-1", "Line"],
    ["image-1", "Square"],
    ["unknown-1", "Text"],
  ];
  const canonical = [
    ["bp:comparison:rectangle:sparse:1", "Square"],
    ["bp:comparison:highlight:1", "Ink"],
    ["bp:comparison:text:1", "FreeText"],
    ["bp:comparison:length:1", "Line"],
    ["bp:comparison:image:1", "Square"],
  ];
  const nativeAnnotations = {
    count: 11,
    annotations: [...source, ...canonical].map(([nativeName, subtype], index) => ({
      object_id: `${index + 1} 0 R`,
      native_name: nativeName,
      subtype,
      appearance_object_id: `${index + 20} 0 R`,
      appearance_stream_valid: true,
    })),
  };

  assert.equal(validateElectronPersistenceNativeAnnotations(nativeAnnotations), true);
  assert.equal(validateElectronPersistenceAppearanceStreams(nativeAnnotations), true);
  assert.equal(validateElectronPersistenceNativeAnnotations({
    ...nativeAnnotations,
    annotations: nativeAnnotations.annotations.slice(1),
    count: 10,
  }), false);
  assert.equal(validateElectronPersistenceAppearanceStreams({
    ...nativeAnnotations,
    annotations: nativeAnnotations.annotations.map((annotation, index) => (
      index === 0 ? { ...annotation, appearance_stream_valid: false } : annotation
    )),
  }), false);
});
