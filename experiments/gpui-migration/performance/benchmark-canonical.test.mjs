import assert from "node:assert/strict";
import test from "node:test";

import {
  assertBenchmarkStyleContract,
  benchmarkStyleMatches,
  canonicalizeBenchmarkMarkups,
  compareNativeBenchmarkGeometry,
  createBenchmarkAliasMap,
} from "./benchmark-canonical.mjs";

test("requires the explicit native UI style contract without projecting away fields", () => {
  const command = {
    id: "rectangle:create-sparse",
    style: { stroke: "#ff0000ff", fill: null, width_pt: 1, dash: "solid", opacity: 1 },
  };
  assert.deepEqual(assertBenchmarkStyleContract(command), command.style);
  assert.deepEqual(benchmarkStyleMatches(command, {
    stroke: { color: "#FF0000", widthPt: 1 },
    fill: { color: null },
    opacity: 1,
  }), { expected: command.style, observed: command.style, matched: true });
  assert.throws(
    () => assertBenchmarkStyleContract({ ...command, style: { ...command.style, width_pt: 2 } }),
    /style differs/,
  );
});

test("makes native pixel quantization and highlight smoothing explicit", () => {
  const surface = { pixels_per_point_x: 0.75, pixels_per_point_y: 0.75 };
  const rectangle = compareNativeBenchmarkGeometry({
    id: "rectangle:create-sparse",
    pointer_path: { start: { x: 72, y: 144 }, finish: { x: 252, y: 240 } },
  }, {
    kind: "rectangle",
    rect: { x: 72.5, y: 143.5, width: 179, height: 97 },
  }, [], surface);
  assert.equal(rectangle.matched, true);
  assert.equal(rectangle.tolerance_pdf_points.position, 2 / 3);
  assert.equal(rectangle.tolerance_pdf_points.extent, 4 / 3);

  const expected = Array.from({ length: 361 }, (_, index) => ({ x: index, y: 0 }));
  const observed = Array.from({ length: 100 }, (_, index) => ({ x: index * 360 / 99, y: 1 }));
  const highlight = compareNativeBenchmarkGeometry(
    { id: "highlight:create" }, { kind: "highlight", paths: [observed] }, expected, surface,
  );
  assert.equal(highlight.matched, true);
  assert.equal(highlight.canonical_resample_count, 64);
  assert.equal(highlight.maximum_centerline_deviation_pdf_points, 1);
  assert.equal(highlight.smoothing_tolerance_pdf_points, 2 / 0.75);
  assert.equal(
    highlight.coordinate_quantization_allowance_pdf_points,
    Math.SQRT1_2 / 0.75,
  );

  const nativeScale = { pixels_per_point_x: 1, pixels_per_point_y: 1 };
  const atQuantizedBoundary = observed.map((point) => ({ ...point, y: 2.7 }));
  const beyondQuantizedBoundary = observed.map((point) => ({ ...point, y: 2.8 }));
  assert.equal(compareNativeBenchmarkGeometry(
    { id: "highlight:create" },
    { kind: "highlight", paths: [atQuantizedBoundary] },
    expected,
    nativeScale,
  ).matched, true);
  assert.equal(compareNativeBenchmarkGeometry(
    { id: "highlight:create" },
    { kind: "highlight", paths: [beyondQuantizedBoundary] },
    expected,
    nativeScale,
  ).matched, false);
});

test("checks every highlight style field including Multiply", () => {
  const command = {
    id: "highlight:create",
    style: { color: "#ffff00ff", width_pt: 12, opacity: 1, blend: "multiply" },
  };
  assert.equal(benchmarkStyleMatches(command, {
    stroke: { color: "#ffff00", widthPt: 12 }, opacity: 1, blendMode: "multiply",
  }).matched, true);
  assert.equal(benchmarkStyleMatches(command, {
    stroke: { color: "#ffff00", widthPt: 12 }, opacity: 1, blendMode: "normal",
  }).matched, false);
});

test("uses an explicit one-to-one alias map for application-generated ids", () => {
  const aliases = createBenchmarkAliasMap([
    { command_id: "rectangle:create-sparse", canonical_id: "comparison:rectangle:sparse:1", observed_id: "rect-generated" },
    { command_id: "highlight:create", canonical_id: "comparison:highlight:1", observed_id: "highlight-generated" },
  ]);
  assert.equal(aliases.observedId("comparison:rectangle:sparse:1"), "rect-generated");
  assert.deepEqual(canonicalizeBenchmarkMarkups([
    { id: "highlight-generated", kind: "highlight" },
    { id: "rect-generated", kind: "rectangle" },
  ], aliases), [
    { id: "comparison:highlight:1", kind: "highlight" },
    { id: "comparison:rectangle:sparse:1", kind: "rectangle" },
  ]);
  assert.throws(() => aliases.canonicalId("unknown"), /no benchmark alias/);
  assert.throws(() => createBenchmarkAliasMap([
    { canonical_id: "one", observed_id: "same" },
    { canonical_id: "two", observed_id: "same" },
  ]), /not one-to-one/);
});
