export const benchmarkStyleContractVersion = "bp-native-ui-style-v1";
export const benchmarkGeometryContractVersion = "bp-native-ui-geometry-v1";

export const benchmarkStyleContract = Object.freeze({
  "rectangle:create-sparse": Object.freeze({
    stroke: "#ff0000ff",
    fill: null,
    width_pt: 1,
    dash: "solid",
    opacity: 1,
  }),
  "highlight:create": Object.freeze({
    color: "#ffff00ff",
    width_pt: 12,
    opacity: 1,
    blend: "multiply",
  }),
});

function normalizedHex(value) {
  if (value === null) return null;
  if (typeof value !== "string" || !/^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(value)) {
    throw new Error(`benchmark color is not a six- or eight-digit hex color: ${value}`);
  }
  return `${value.slice(0, 7).toLowerCase()}${value.length === 7 ? "ff" : value.slice(7).toLowerCase()}`;
}

export function assertBenchmarkStyleContract(command) {
  const expected = benchmarkStyleContract[command?.id];
  if (!expected) throw new Error(`no benchmark style contract for ${command?.id ?? "unknown command"}`);
  if (JSON.stringify(command.style) !== JSON.stringify(expected)) {
    throw new Error(`${command.id} style differs from ${benchmarkStyleContractVersion}`);
  }
  return expected;
}

export function observedBenchmarkStyle(commandId, appearance) {
  if (commandId === "rectangle:create-sparse") {
    return {
      stroke: normalizedHex(appearance?.stroke?.color),
      fill: appearance?.fill?.color == null ? null : normalizedHex(appearance.fill.color),
      width_pt: appearance?.stroke?.widthPt,
      dash: appearance?.stroke?.dash ?? "solid",
      opacity: appearance?.opacity,
    };
  }
  if (commandId === "highlight:create") {
    return {
      color: normalizedHex(appearance?.stroke?.color),
      width_pt: appearance?.stroke?.widthPt,
      opacity: appearance?.opacity,
      blend: appearance?.blendMode,
    };
  }
  throw new Error(`no observed style projection for ${commandId}`);
}

export function benchmarkStyleMatches(command, appearance) {
  const expected = assertBenchmarkStyleContract(command);
  const observed = observedBenchmarkStyle(command.id, appearance);
  return { expected, observed, matched: JSON.stringify(observed) === JSON.stringify(expected) };
}

export function createBenchmarkAliasMap(entries) {
  if (!Array.isArray(entries)) throw new Error("benchmark aliases must be an array");
  const canonicalToObserved = new Map();
  const observedToCanonical = new Map();
  for (const entry of entries) {
    const canonical = entry?.canonical_id;
    const observed = entry?.observed_id;
    if (typeof canonical !== "string" || canonical.length === 0) {
      throw new Error("benchmark canonical alias id must be a non-empty string");
    }
    if (typeof observed !== "string" || observed.length === 0) {
      throw new Error("benchmark observed alias id must be a non-empty string");
    }
    if (canonicalToObserved.has(canonical) || observedToCanonical.has(observed)) {
      throw new Error(`benchmark alias is not one-to-one: ${canonical} -> ${observed}`);
    }
    canonicalToObserved.set(canonical, observed);
    observedToCanonical.set(observed, canonical);
  }
  return Object.freeze({
    schema_version: "bp-benchmark-alias-map-v1",
    entries: Object.freeze(entries.map((entry) => Object.freeze({ ...entry }))),
    canonicalId(observedId) {
      const canonical = observedToCanonical.get(observedId);
      if (!canonical) throw new Error(`observed annotation id has no benchmark alias: ${observedId}`);
      return canonical;
    },
    observedId(canonicalId) {
      const observed = canonicalToObserved.get(canonicalId);
      if (!observed) throw new Error(`canonical annotation id has no benchmark alias: ${canonicalId}`);
      return observed;
    },
  });
}

export function canonicalizeBenchmarkMarkups(markups, aliasMap) {
  return [...markups]
    .map((markup) => ({ ...markup, id: aliasMap.canonicalId(markup.id) }))
    .sort((left, right) => left.id.localeCompare(right.id));
}

function resamplePolyline(points, count) {
  if (!Array.isArray(points) || points.length < 2) throw new Error("benchmark polyline requires two points");
  const cumulative = [0];
  for (let index = 1; index < points.length; index += 1) {
    cumulative.push(cumulative[index - 1] + Math.hypot(
      points[index].x - points[index - 1].x,
      points[index].y - points[index - 1].y,
    ));
  }
  const total = cumulative.at(-1);
  if (!(total > 0)) throw new Error("benchmark polyline must have positive length");
  return Array.from({ length: count }, (_, sampleIndex) => {
    const distance = total * sampleIndex / (count - 1);
    let pointIndex = 1;
    while (pointIndex < cumulative.length - 1 && cumulative[pointIndex] < distance) pointIndex += 1;
    const start = points[pointIndex - 1];
    const finish = points[pointIndex];
    const span = cumulative[pointIndex] - cumulative[pointIndex - 1];
    const fraction = span === 0 ? 0 : (distance - cumulative[pointIndex - 1]) / span;
    return {
      x: start.x + (finish.x - start.x) * fraction,
      y: start.y + (finish.y - start.y) * fraction,
    };
  });
}

export function compareNativeBenchmarkGeometry(command, markup, expectedSamples, surface) {
  const minimumScale = Math.min(surface?.pixels_per_point_x, surface?.pixels_per_point_y);
  if (!(minimumScale > 0)) throw new Error("native geometry comparison requires verified PDF-to-pixel scales");
  if (command.id === "rectangle:create-sparse") {
    const expected = {
      x: Math.min(command.pointer_path.start.x, command.pointer_path.finish.x),
      y: Math.min(command.pointer_path.start.y, command.pointer_path.finish.y),
      width: Math.abs(command.pointer_path.finish.x - command.pointer_path.start.x),
      height: Math.abs(command.pointer_path.finish.y - command.pointer_path.start.y),
    };
    const observed = markup.rect ?? {};
    const positionTolerance = 0.5 / minimumScale;
    const extentTolerance = 1 / minimumScale;
    const errors = Object.fromEntries(Object.keys(expected).map((field) => [
      field,
      Math.abs(observed[field] - expected[field]),
    ]));
    return {
      matched: markup.kind === "rectangle"
        && errors.x <= positionTolerance && errors.y <= positionTolerance
        && errors.width <= extentTolerance && errors.height <= extentTolerance,
      expected,
      observed: markup.rect ?? null,
      errors_pdf_points: errors,
      tolerance_pdf_points: { position: positionTolerance, extent: extentTolerance },
      contract_version: benchmarkGeometryContractVersion,
      canonicalization: "PDF-space endpoints quantized to the verified native pixel grid",
    };
  }
  const observedPath = markup.paths?.[0] ?? [];
  const sampleCount = 64;
  const expected = resamplePolyline(expectedSamples, sampleCount);
  const observed = observedPath.length >= 2 ? resamplePolyline(observedPath, sampleCount) : [];
  const deviations = observed.length === sampleCount
    ? expected.map((point, index) => Math.hypot(point.x - observed[index].x, point.y - observed[index].y))
    : [];
  const maximumDeviation = deviations.length > 0 ? Math.max(...deviations) : null;
  const smoothingTolerance = 2 / minimumScale;
  const coordinateQuantizationAllowance = Math.SQRT1_2 / minimumScale;
  const tolerance = smoothingTolerance + coordinateQuantizationAllowance;
  return {
    matched: markup.kind === "highlight" && maximumDeviation !== null && maximumDeviation <= tolerance,
    expected_input_point_count: expectedSamples.length,
    observed_model_point_count: observedPath.length,
    canonical_resample_count: sampleCount,
    maximum_centerline_deviation_pdf_points: maximumDeviation,
    tolerance_pdf_points: tolerance,
    smoothing_tolerance_pdf_points: smoothingTolerance,
    coordinate_quantization_allowance_pdf_points: coordinateQuantizationAllowance,
    contract_version: benchmarkGeometryContractVersion,
    canonicalization: "arc-length 64-point centerline; two native pixels after maintained smoothing plus one half-pixel-per-axis XTEST quantization diagonal",
  };
}
