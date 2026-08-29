import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { promisify } from "node:util";

import sharp from "sharp";

import {
  registeredCropSsimAlgorithmV5,
  registeredCropSsimLuma,
} from "./dynamic-fidelity-v5.mjs";
import { measureCrossEngineScanFidelityV2 } from "./scan-fidelity-v2.mjs";

const execFileAsync = promisify(execFile);

export async function inspectPngRasterArtifact(path) {
  let metadata;
  try {
    metadata = await sharp(path).metadata();
  } catch (error) {
    throw new Error(`${path} must be a decodable RGB or RGBA PNG raster`, {
      cause: error,
    });
  }
  if (
    metadata.format !== "png" ||
    !Number.isInteger(metadata.width) ||
    metadata.width <= 0 ||
    !Number.isInteger(metadata.height) ||
    metadata.height <= 0 ||
    !Number.isInteger(metadata.channels) ||
    metadata.channels < 3
  ) {
    throw new Error(`${path} must be a decodable RGB or RGBA PNG raster`);
  }
  return Object.freeze({
    format: metadata.format,
    width: metadata.width,
    height: metadata.height,
    channels: metadata.channels,
  });
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function requireRect(rect, name, { allowNegativeOrigin = false } = {}) {
  for (const field of ["x", "y", "width", "height"]) {
    const negativeOrigin =
      (field === "x" || field === "y") && allowNegativeOrigin;
    if (
      !Number.isFinite(rect?.[field]) ||
      (!negativeOrigin && rect[field] < 0)
    ) {
      throw new TypeError(
        `${name}.${field} must be finite${negativeOrigin ? "" : " and nonnegative"}`,
      );
    }
  }
  if (rect.width === 0 || rect.height === 0) {
    throw new RangeError(`${name} must have positive dimensions`);
  }
}

export function mapPdfRectToImagePixels(pdfRect, pageSizePt, pageBoundsPx) {
  requireRect(pdfRect, "pdfRect");
  requireRect(pageBoundsPx, "pageBoundsPx", { allowNegativeOrigin: true });
  if (
    !Number.isFinite(pageSizePt?.width) ||
    pageSizePt.width <= 0 ||
    !Number.isFinite(pageSizePt?.height) ||
    pageSizePt.height <= 0
  ) {
    throw new TypeError("pageSizePt must be positive and finite");
  }
  if (
    pdfRect.x + pdfRect.width > pageSizePt.width + 1e-9 ||
    pdfRect.y + pdfRect.height > pageSizePt.height + 1e-9
  ) {
    throw new RangeError("pdfRect is outside the PDF page");
  }
  const scaleX = pageBoundsPx.width / pageSizePt.width;
  const scaleY = pageBoundsPx.height / pageSizePt.height;
  return Object.freeze({
    left: pageBoundsPx.x + pdfRect.x * scaleX,
    top:
      pageBoundsPx.y +
      (pageSizePt.height - pdfRect.y - pdfRect.height) * scaleY,
    width: pdfRect.width * scaleX,
    height: pdfRect.height * scaleY,
  });
}

function enclosingIntegerRect(rect, imageWidth, imageHeight) {
  const left = Math.max(0, Math.floor(rect.left));
  const top = Math.max(0, Math.floor(rect.top));
  const right = Math.min(imageWidth, Math.ceil(rect.left + rect.width));
  const bottom = Math.min(imageHeight, Math.ceil(rect.top + rect.height));
  if (right <= left || bottom <= top) {
    throw new RangeError("registered crop does not intersect the image");
  }
  return { left, top, width: right - left, height: bottom - top };
}

async function pageSizePoints(pdfPath, pageNumber) {
  const { stdout } = await execFileAsync("pdfinfo", [
    "-f",
    String(pageNumber),
    "-l",
    String(pageNumber),
    pdfPath,
  ]);
  const escaped = String(pageNumber).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pagePattern = new RegExp(
    `(?:Page\\s+${escaped}\\s+size|Page size):\\s*([0-9.]+) x ([0-9.]+) pts`,
  );
  const match = pagePattern.exec(stdout);
  if (!match) throw new Error(`pdfinfo did not report page ${pageNumber} size`);
  return { width: Number(match[1]), height: Number(match[2]) };
}

export async function renderReferenceCrop({
  pdfPath,
  pageNumber,
  pdfRect,
  dpi,
  outputPath,
}) {
  if (!Number.isInteger(pageNumber) || pageNumber < 1) {
    throw new TypeError("pageNumber must be a positive integer");
  }
  if (!Number.isInteger(dpi) || dpi <= 0) {
    throw new TypeError("dpi must be a positive integer");
  }
  const pageSizePt = await pageSizePoints(pdfPath, pageNumber);
  const temporary = await mkdtemp(resolve(tmpdir(), "bp-reference-crop-v5-"));
  try {
    const prefix = resolve(temporary, "page");
    await execFileAsync("pdftoppm", [
      "-f",
      String(pageNumber),
      "-l",
      String(pageNumber),
      "-singlefile",
      "-r",
      String(dpi),
      "-png",
      pdfPath,
      prefix,
    ]);
    const renderedPath = `${prefix}.png`;
    const metadata = await sharp(renderedPath).metadata();
    const mapped = mapPdfRectToImagePixels(pdfRect, pageSizePt, {
      x: 0,
      y: 0,
      width: metadata.width,
      height: metadata.height,
    });
    const extraction = enclosingIntegerRect(
      mapped,
      metadata.width,
      metadata.height,
    );
    await mkdir(dirname(outputPath), { recursive: true });
    await sharp(renderedPath)
      .extract(extraction)
      .removeAlpha()
      .png()
      .toFile(outputPath);
    const bytes = await readFile(outputPath);
    const outputMetadata = await sharp(bytes).metadata();
    return Object.freeze({
      page_number: pageNumber,
      page_size_points: pageSizePt,
      dpi,
      mapped_bounds_pixels: mapped,
      extracted_bounds_pixels: extraction,
      width: outputMetadata.width,
      height: outputMetadata.height,
      sha256: sha256(bytes),
    });
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
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

export async function compareCrossEngineScanFidelityV2({
  referencePath,
  candidatePath,
}) {
  const [reference, candidate] = await Promise.all([
    rawRgb(referencePath),
    rawRgb(candidatePath),
  ]);
  return measureCrossEngineScanFidelityV2(reference, candidate);
}

export async function registerAndCompareCapturedCrop({
  screenshotPath,
  pageBoundsPx,
  pageSizePt,
  pdfRect,
  referencePath,
  outputPath,
}) {
  const screenshot = await sharp(screenshotPath).metadata();
  const reference = await sharp(referencePath).metadata();
  const mapped = mapPdfRectToImagePixels(pdfRect, pageSizePt, pageBoundsPx);
  const extraction = enclosingIntegerRect(
    mapped,
    screenshot.width,
    screenshot.height,
  );
  await mkdir(dirname(outputPath), { recursive: true });
  await sharp(screenshotPath)
    .extract(extraction)
    .resize(reference.width, reference.height, {
      fit: "fill",
      kernel: sharp.kernel.lanczos3,
    })
    .removeAlpha()
    .png()
    .toFile(outputPath);

  const [referenceRaw, capturedRaw, referenceBytes, capturedBytes] =
    await Promise.all([
      rawRgb(referencePath),
      rawRgb(outputPath),
      readFile(referencePath),
      readFile(outputPath),
    ]);
  return Object.freeze({
    ssim_algorithm: registeredCropSsimAlgorithmV5,
    ssim_luma: registeredCropSsimLuma(referenceRaw, capturedRaw),
    reference_crop_sha256: sha256(referenceBytes),
    captured_crop_sha256: sha256(capturedBytes),
    mapped_bounds_pixels: mapped,
    extracted_bounds_pixels: extraction,
    reference_dimensions: {
      width: reference.width,
      height: reference.height,
    },
  });
}

export async function registerAndComparePresentedCropV2({
  screenshotPath,
  pageBoundsPx,
  pageSizePt,
  pdfRect,
  referencePath,
  outputCandidatePath,
  outputRegisteredReferencePath,
}) {
  const [screenshot, reference] = await Promise.all([
    inspectPngRasterArtifact(screenshotPath),
    inspectPngRasterArtifact(referencePath),
  ]);
  const mapped = mapPdfRectToImagePixels(pdfRect, pageSizePt, pageBoundsPx);
  const extraction = enclosingIntegerRect(
    mapped,
    screenshot.width,
    screenshot.height,
  );
  if (
    reference.width < extraction.width ||
    reference.height < extraction.height
  ) {
    throw new RangeError(
      "the reference crop must not be smaller than the native presented crop",
    );
  }
  await Promise.all([
    mkdir(dirname(outputCandidatePath), { recursive: true }),
    mkdir(dirname(outputRegisteredReferencePath), { recursive: true }),
  ]);
  await sharp(screenshotPath)
    .extract(extraction)
    .removeAlpha()
    .png()
    .toFile(outputCandidatePath);
  const referenceResampled =
    reference.width !== extraction.width ||
    reference.height !== extraction.height;
  let referencePipeline = sharp(referencePath);
  if (referenceResampled) {
    referencePipeline = referencePipeline.resize(
      extraction.width,
      extraction.height,
      { fit: "fill", kernel: sharp.kernel.lanczos3 },
    );
  }
  await referencePipeline
    .removeAlpha()
    .png()
    .toFile(outputRegisteredReferencePath);

  const [
    metric,
    screenshotBytes,
    candidateBytes,
    referenceBytes,
    registeredReferenceBytes,
  ] = await Promise.all([
    compareCrossEngineScanFidelityV2({
      referencePath: outputRegisteredReferencePath,
      candidatePath: outputCandidatePath,
    }),
    readFile(screenshotPath),
    readFile(outputCandidatePath),
    readFile(referencePath),
    readFile(outputRegisteredReferencePath),
  ]);
  return Object.freeze({
    mapped_bounds_pixels: mapped,
    extracted_bounds_pixels: extraction,
    candidate_dimensions: Object.freeze({
      width: extraction.width,
      height: extraction.height,
    }),
    reference_original_dimensions: Object.freeze({
      width: reference.width,
      height: reference.height,
    }),
    registered_reference_dimensions: Object.freeze({
      width: extraction.width,
      height: extraction.height,
    }),
    candidate_resampled: false,
    reference_resampled: referenceResampled,
    reference_resampling: "downsample-only-lanczos3",
    screenshot_sha256: sha256(screenshotBytes),
    candidate_crop_sha256: sha256(candidateBytes),
    reference_crop_sha256: sha256(referenceBytes),
    registered_reference_crop_sha256: sha256(registeredReferenceBytes),
    metric,
  });
}
