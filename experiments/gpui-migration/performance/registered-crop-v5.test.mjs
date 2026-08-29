import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import sharp from "sharp";

import {
  inspectPngRasterArtifact,
  mapPdfRectToImagePixels,
  registerAndCompareCapturedCrop,
  registerAndComparePresentedCropV2,
  renderReferenceCrop,
} from "./registered-crop-v5.mjs";

test("accepts a decodable PNG raster and rejects a PPM artifact", async () => {
  const output = await mkdtemp(resolve(tmpdir(), "bp-raster-format-test-"));
  try {
    const pngPath = resolve(output, "raster.png");
    const ppmPath = resolve(output, "raster.ppm");
    const pixels = Buffer.alloc(2 * 3 * 3, 127);
    await sharp(pixels, { raw: { width: 2, height: 3, channels: 3 } })
      .png()
      .toFile(pngPath);
    await writeFile(
      ppmPath,
      Buffer.concat([Buffer.from("P6\n2 3\n255\n"), pixels]),
    );
    assert.deepEqual(await inspectPngRasterArtifact(pngPath), {
      format: "png",
      width: 2,
      height: 3,
      channels: 3,
    });
    await assert.rejects(
      inspectPngRasterArtifact(ppmPath),
      /must be a decodable RGB or RGBA PNG raster/,
    );
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("keeps the presented crop native and downsamples only the reference", async () => {
  const output = await mkdtemp(resolve(tmpdir(), "bp-presented-crop-test-"));
  try {
    const screenshotPath = resolve(output, "screen.png");
    const referencePath = resolve(output, "reference.png");
    const candidatePath = resolve(output, "candidate.png");
    const registeredReferencePath = resolve(output, "reference-registered.png");
    await sharp(Buffer.alloc(612 * 792 * 3, 240), {
      raw: { width: 612, height: 792, channels: 3 },
    })
      .png()
      .toFile(screenshotPath);
    await sharp(Buffer.alloc(1076 * 252 * 3, 240), {
      raw: { width: 1076, height: 252, channels: 3 },
    })
      .png()
      .toFile(referencePath);
    const receipt = await registerAndComparePresentedCropV2({
      screenshotPath,
      pageBoundsPx: { x: 0, y: 0, width: 612, height: 792 },
      pageSizePt: { width: 612, height: 792 },
      pdfRect: { x: 36, y: 612, width: 538, height: 126 },
      referencePath,
      outputCandidatePath: candidatePath,
      outputRegisteredReferencePath: registeredReferencePath,
    });
    assert.deepEqual(receipt.candidate_dimensions, {
      width: 538,
      height: 126,
    });
    assert.deepEqual(receipt.reference_original_dimensions, {
      width: 1076,
      height: 252,
    });
    assert.deepEqual(receipt.registered_reference_dimensions, {
      width: 538,
      height: 126,
    });
    assert.equal(receipt.candidate_resampled, false);
    assert.equal(receipt.reference_resampled, true);
    assert.equal(receipt.reference_resampling, "downsample-only-lanczos3");
    assert.equal(receipt.metric.passed, true);
    assert.equal(
      receipt.metric.algorithm,
      "bp-cross-engine-binary-scan-fidelity-v2",
    );
    assert.deepEqual(
      await sharp(candidatePath)
        .metadata()
        .then(({ width, height }) => ({ width, height })),
      {
        width: 538,
        height: 126,
      },
    );
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("rejects a reference smaller than the native presented crop", async () => {
  const output = await mkdtemp(resolve(tmpdir(), "bp-presented-crop-test-"));
  try {
    const screenshotPath = resolve(output, "screen.png");
    const referencePath = resolve(output, "reference.png");
    await sharp(Buffer.alloc(612 * 792 * 3, 240), {
      raw: { width: 612, height: 792, channels: 3 },
    })
      .png()
      .toFile(screenshotPath);
    await sharp(Buffer.alloc(500 * 120 * 3, 240), {
      raw: { width: 500, height: 120, channels: 3 },
    })
      .png()
      .toFile(referencePath);
    await assert.rejects(
      registerAndComparePresentedCropV2({
        screenshotPath,
        pageBoundsPx: { x: 0, y: 0, width: 612, height: 792 },
        pageSizePt: { width: 612, height: 792 },
        pdfRect: { x: 36, y: 612, width: 538, height: 126 },
        referencePath,
        outputCandidatePath: resolve(output, "candidate.png"),
        outputRegisteredReferencePath: resolve(output, "registered.png"),
      }),
      /must not be smaller/,
    );
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("maps bottom-left PDF points into top-left image pixels", () => {
  assert.deepEqual(
    mapPdfRectToImagePixels(
      { x: 36, y: 612, width: 538, height: 126 },
      { width: 612, height: 792 },
      { x: 100, y: 40, width: 1224, height: 1584 },
    ),
    { left: 172, top: 148, width: 1076, height: 252 },
  );
});

test("maps a fully visible crop from a page with an offscreen origin", () => {
  assert.deepEqual(
    mapPdfRectToImagePixels(
      { x: 55, y: 54, width: 500, height: 126 },
      { width: 610, height: 792 },
      { x: 425, y: -5, width: 610, height: 792 },
    ),
    { left: 480, top: 607, width: 500, height: 126 },
  );
});

test("renders a locked PDF reference crop at an exact DPI", async () => {
  const output = await mkdtemp(resolve(tmpdir(), "bp-crop-test-"));
  try {
    const cropPath = resolve(output, "reference.png");
    const receipt = await renderReferenceCrop({
      pdfPath: resolve(
        import.meta.dirname,
        "results/public-fixtures-v1/bp-single-page-v1.pdf",
      ),
      pageNumber: 1,
      pdfRect: { x: 36, y: 612, width: 538, height: 126 },
      dpi: 144,
      outputPath: cropPath,
    });
    assert.deepEqual(receipt.page_size_points, { width: 612, height: 792 });
    assert.equal(receipt.width, 1076);
    assert.equal(receipt.height, 252);
    assert.match(receipt.sha256, /^[0-9a-f]{64}$/);
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("registers an application crop to the reference dimensions and compares it", async () => {
  const output = await mkdtemp(resolve(tmpdir(), "bp-crop-test-"));
  try {
    const screenshotPath = resolve(output, "screen.png");
    const referencePath = resolve(output, "reference.png");
    const capturedPath = resolve(output, "captured.png");
    const page = Buffer.alloc(612 * 792 * 3, 240);
    await sharp(page, { raw: { width: 612, height: 792, channels: 3 } })
      .png()
      .toFile(screenshotPath);
    await sharp(page, { raw: { width: 612, height: 792, channels: 3 } })
      .extract({ left: 36, top: 54, width: 538, height: 126 })
      .png()
      .toFile(referencePath);
    const receipt = await registerAndCompareCapturedCrop({
      screenshotPath,
      pageBoundsPx: { x: 0, y: 0, width: 612, height: 792 },
      pageSizePt: { width: 612, height: 792 },
      pdfRect: { x: 36, y: 612, width: 538, height: 126 },
      referencePath,
      outputPath: capturedPath,
    });
    assert.equal(receipt.ssim_luma, 1);
    assert.equal(
      receipt.ssim_algorithm,
      "bp-registered-crop-global-luma-ssim-v1",
    );
    assert.deepEqual(receipt.reference_dimensions, {
      width: 538,
      height: 126,
    });
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("retains the three exact transferred v5 reference crops", async () => {
  const directory = resolve(import.meta.dirname, "fixtures/reference-crops-v5");
  const manifest = JSON.parse(
    await readFile(resolve(directory, "manifest.json"), "utf8"),
  );
  assert.equal(manifest.receipts.length, 3);
  for (const receipt of manifest.receipts) {
    const bytes = await readFile(resolve(directory, receipt.file));
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      receipt.reference_crop_sha256,
    );
  }
});
