import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";

import sharp from "sharp";

import { compareCrossEngineScanFidelityV2 } from "./registered-crop-v5.mjs";
import {
  crossEngineScanFidelityAlgorithmV2,
  crossEngineScanFidelityParametersV2,
  measureCrossEngineScanFidelityV2,
} from "./scan-fidelity-v2.mjs";

function rgbImage(width, height, paint) {
  const data = Buffer.alloc(width * height * 3, 255);
  const set = (x, y, value = 0) => {
    for (let channel = 0; channel < 3; channel += 1) {
      data[(y * width + x) * 3 + channel] = value;
    }
  };
  paint(set);
  return { width, height, channels: 3, data };
}

function thinStrokeField({ missingStroke, addedStroke } = {}) {
  return rgbImage(240, 120, (set) => {
    for (let stroke = 0; stroke < 40; stroke += 1) {
      if (stroke === missingStroke) continue;
      const x = 3 + stroke * 5;
      for (let y = 20; y < 100; y += 1) set(x, y);
    }
    if (addedStroke) {
      for (let y = 20; y < 100; y += 1) set(220, y);
    }
  });
}

function thinGlyphField({ missingCrossbarGlyphs = [] } = {}) {
  return rgbImage(240, 120, (set) => {
    for (let glyph = 0; glyph < 8; glyph += 1) {
      const left = 20 + glyph * 25;
      const right = left + 10;
      for (let y = 35; y < 85; y += 1) {
        set(left, y);
        set(right, y);
      }
      if (!missingCrossbarGlyphs.includes(glyph)) {
        for (let x = left; x <= right; x += 1) set(x, 60);
      }
    }
  });
}

async function rawRgb(path) {
  const result = await sharp(path)
    .removeAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  return {
    width: result.info.width,
    height: result.info.height,
    channels: result.info.channels,
    data: result.data,
  };
}

test("uses the frozen v2 scan-fidelity parameters", () => {
  assert.equal(
    crossEngineScanFidelityAlgorithmV2,
    "bp-cross-engine-binary-scan-fidelity-v2",
  );
  assert.deepEqual(crossEngineScanFidelityParametersV2, {
    source_scan_dpi: 300,
    comparison_dpi: 144,
    maximum_phase_offset_px: 1,
    gaussian_sigma_px: 2,
    gaussian_radius_px: 6,
    dark_luma_max: 192,
    dark_match_radius_px: 1,
    minimum_filtered_ssim: 0.97,
    minimum_dark_precision: 0.99,
    minimum_dark_recall: 0.99,
    minimum_dark_f1: 0.99,
  });
});

test("fails closed on unequal images or invalid phase and threshold parameters", () => {
  const reference = thinStrokeField();
  assert.throws(
    () =>
      measureCrossEngineScanFidelityV2(
        reference,
        rgbImage(reference.width - 1, reference.height, () => {}),
      ),
    /equal dimensions/,
  );
  assert.throws(
    () =>
      measureCrossEngineScanFidelityV2(reference, reference, {
        maximum_phase_offset_px: 1.5,
      }),
    /maximum_phase_offset_px/,
  );
  assert.throws(
    () =>
      measureCrossEngineScanFidelityV2(reference, reference, {
        minimum_dark_recall: -0.1,
      }),
    /minimum_dark_recall/,
  );
});

test("calibrates sigma 1 and sigma 2 against all three retained GPUI crops", async () => {
  const cases = [
    {
      id: "nasa-scroll-start-page-1-header",
      sigma1: 0.9947093484282301,
      sigma2: 0.997262347833718,
      phase: { dx: 1, dy: 0 },
    },
    {
      id: "nasa-scroll-middle-page-15-body",
      sigma1: 0.9831641267834066,
      sigma2: 0.994213076942878,
      phase: { dx: 1, dy: 1 },
    },
    {
      id: "nasa-scroll-forward-apex-page-29-footer",
      sigma1: 0.9715229324086431,
      sigma2: 0.9751306591666944,
      phase: { dx: 1, dy: 1 },
    },
  ];
  const references = resolve(
    import.meta.dirname,
    "fixtures/reference-crops-v5",
  );
  const candidates = resolve(import.meta.dirname, "fixtures/scan-fidelity-v2");
  for (const calibration of cases) {
    const [reference, candidate] = await Promise.all([
      rawRgb(resolve(references, `${calibration.id}.png`)),
      rawRgb(resolve(candidates, `${calibration.id}-pdfium.png`)),
    ]);
    const sigma1 = measureCrossEngineScanFidelityV2(reference, candidate, {
      gaussian_sigma_px: 1,
      gaussian_radius_px: 3,
    });
    const sigma2 = measureCrossEngineScanFidelityV2(reference, candidate);
    assert.ok(Math.abs(sigma1.filtered_ssim_luma - calibration.sigma1) < 1e-6);
    assert.ok(Math.abs(sigma2.filtered_ssim_luma - calibration.sigma2) < 1e-6);
    assert.deepEqual(sigma2.phase_offset_px, calibration.phase);
    assert.ok(sigma2.dark_content.precision >= 0.99);
    assert.ok(sigma2.dark_content.recall >= 0.99);
    assert.ok(sigma2.dark_content.f1 >= 0.99);
    assert.equal(sigma2.passed, true);
  }
});

test("pins the real PDFium calibration crops without treating them as references", async () => {
  const directory = resolve(import.meta.dirname, "fixtures/scan-fidelity-v2");
  const manifest = JSON.parse(
    await readFile(resolve(directory, "manifest.json"), "utf8"),
  );
  assert.equal(manifest.acceptance_reference, false);
  assert.match(manifest.purpose, /not acceptance references/);
  assert.equal(manifest.receipts.length, 3);
  for (const receipt of manifest.receipts) {
    const bytes = await readFile(resolve(directory, receipt.file));
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      receipt.candidate_crop_sha256,
    );
  }
});

test("the registered-crop wrapper returns the same v2 receipt", async () => {
  const id = "nasa-scroll-start-page-1-header";
  const receipt = await compareCrossEngineScanFidelityV2({
    referencePath: resolve(
      import.meta.dirname,
      "fixtures/reference-crops-v5",
      `${id}.png`,
    ),
    candidatePath: resolve(
      import.meta.dirname,
      "fixtures/scan-fidelity-v2",
      `${id}-pdfium.png`,
    ),
  });
  assert.equal(receipt.algorithm, crossEngineScanFidelityAlgorithmV2);
  assert.equal(receipt.passed, true);
});

test("rejects a deleted thin stroke even when filtered SSIM passes", () => {
  const reference = thinStrokeField();
  const candidate = thinStrokeField({ missingStroke: 20 });
  const receipt = measureCrossEngineScanFidelityV2(reference, candidate);
  assert.ok(
    receipt.filtered_ssim_luma >=
      crossEngineScanFidelityParametersV2.minimum_filtered_ssim,
  );
  assert.ok(
    receipt.dark_content.recall <
      crossEngineScanFidelityParametersV2.minimum_dark_recall,
  );
  assert.ok(
    receipt.dark_content.f1 <
      crossEngineScanFidelityParametersV2.minimum_dark_f1,
  );
  assert.equal(receipt.passed, false);
});

test("rejects a deleted one-pixel text crossbar even when filtered SSIM passes", () => {
  const reference = thinGlyphField();
  const candidate = thinGlyphField({ missingCrossbarGlyphs: [3, 4] });
  const receipt = measureCrossEngineScanFidelityV2(reference, candidate);
  assert.ok(
    receipt.filtered_ssim_luma >=
      crossEngineScanFidelityParametersV2.minimum_filtered_ssim,
  );
  assert.ok(
    receipt.dark_content.recall <
      crossEngineScanFidelityParametersV2.minimum_dark_recall,
  );
  assert.equal(receipt.passed, false);
});

test("rejects an added thin stroke even when filtered SSIM passes", () => {
  const reference = thinStrokeField();
  const candidate = thinStrokeField({ addedStroke: true });
  const receipt = measureCrossEngineScanFidelityV2(reference, candidate);
  assert.ok(
    receipt.filtered_ssim_luma >=
      crossEngineScanFidelityParametersV2.minimum_filtered_ssim,
  );
  assert.ok(
    receipt.dark_content.precision <
      crossEngineScanFidelityParametersV2.minimum_dark_precision,
  );
  assert.equal(receipt.passed, false);
});
