export const crossEngineScanFidelityAlgorithmV2 =
  "bp-cross-engine-binary-scan-fidelity-v2";

// The locked NASA calibration pages contain 300 DPI, one-bit CCITT scans. At
// the 144 DPI comparison size, Poppler and PDFium place the same source edges
// up to one output pixel apart and use different reconstruction filters. Sigma
// 2 suppresses that renderer-specific edge phase without removing whole thin
// strokes. The separate dark-content gate detects those structural losses.
export const crossEngineScanFidelityParametersV2 = Object.freeze({
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

function requireImage(image, name) {
  if (
    !Number.isInteger(image?.width) ||
    image.width <= 0 ||
    !Number.isInteger(image?.height) ||
    image.height <= 0 ||
    !Number.isInteger(image?.channels) ||
    image.channels < 3 ||
    !image?.data ||
    image.data.length !== image.width * image.height * image.channels
  ) {
    throw new TypeError(`${name} must be a complete RGB or RGBA image`);
  }
}

function requireSameImageShape(reference, candidate) {
  requireImage(reference, "reference");
  requireImage(candidate, "candidate");
  if (
    reference.width !== candidate.width ||
    reference.height !== candidate.height
  ) {
    throw new RangeError("scan fidelity images must have equal dimensions");
  }
}

function requireParameters(parameters) {
  if (
    !Number.isInteger(parameters.maximum_phase_offset_px) ||
    parameters.maximum_phase_offset_px < 0 ||
    parameters.maximum_phase_offset_px > 4
  ) {
    throw new RangeError(
      "maximum_phase_offset_px must be an integer from 0 to 4",
    );
  }
  if (
    !Number.isFinite(parameters.gaussian_sigma_px) ||
    parameters.gaussian_sigma_px <= 0 ||
    !Number.isInteger(parameters.gaussian_radius_px) ||
    parameters.gaussian_radius_px < 1 ||
    parameters.gaussian_radius_px > 12
  ) {
    throw new RangeError("the Gaussian filter parameters are out of range");
  }
  if (
    !Number.isFinite(parameters.dark_luma_max) ||
    parameters.dark_luma_max < 0 ||
    parameters.dark_luma_max > 255 ||
    !Number.isInteger(parameters.dark_match_radius_px) ||
    parameters.dark_match_radius_px < 0 ||
    parameters.dark_match_radius_px > 4
  ) {
    throw new RangeError("the dark-content parameters are out of range");
  }
  for (const threshold of [
    "minimum_filtered_ssim",
    "minimum_dark_precision",
    "minimum_dark_recall",
    "minimum_dark_f1",
  ]) {
    if (
      !Number.isFinite(parameters[threshold]) ||
      parameters[threshold] < 0 ||
      parameters[threshold] > 1
    ) {
      throw new RangeError(`${threshold} must be between zero and one`);
    }
  }
}

function lumaPlane(image) {
  const plane = new Float32Array(image.width * image.height);
  for (let pixel = 0; pixel < plane.length; pixel += 1) {
    const offset = pixel * image.channels;
    plane[pixel] =
      0.2126 * image.data[offset] +
      0.7152 * image.data[offset + 1] +
      0.0722 * image.data[offset + 2];
  }
  return plane;
}

function gaussianKernel(sigma, radius) {
  const kernel = new Float64Array(radius * 2 + 1);
  let total = 0;
  for (let offset = -radius; offset <= radius; offset += 1) {
    const weight = Math.exp(-(offset * offset) / (2 * sigma * sigma));
    kernel[offset + radius] = weight;
    total += weight;
  }
  for (let index = 0; index < kernel.length; index += 1) {
    kernel[index] /= total;
  }
  return kernel;
}

function gaussianFilter(plane, width, height, sigma, radius) {
  const kernel = gaussianKernel(sigma, radius);
  const horizontal = new Float32Array(plane.length);
  const output = new Float32Array(plane.length);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      let value = 0;
      for (let offset = -radius; offset <= radius; offset += 1) {
        const sampleX = Math.max(0, Math.min(width - 1, x + offset));
        value += plane[y * width + sampleX] * kernel[offset + radius];
      }
      horizontal[y * width + x] = value;
    }
  }
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      let value = 0;
      for (let offset = -radius; offset <= radius; offset += 1) {
        const sampleY = Math.max(0, Math.min(height - 1, y + offset));
        value += horizontal[sampleY * width + x] * kernel[offset + radius];
      }
      output[y * width + x] = value;
    }
  }
  return output;
}

function shiftedValue(plane, width, height, x, y, dx, dy) {
  const sourceX = x - dx;
  const sourceY = y - dy;
  if (sourceX < 0 || sourceX >= width || sourceY < 0 || sourceY >= height) {
    return 255;
  }
  return plane[sourceY * width + sourceX];
}

function shiftedGlobalSsim(reference, candidate, width, height, dx, dy) {
  const count = width * height;
  let referenceMean = 0;
  let candidateMean = 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      referenceMean += reference[y * width + x];
      candidateMean += shiftedValue(candidate, width, height, x, y, dx, dy);
    }
  }
  referenceMean /= count;
  candidateMean /= count;
  let referenceVariance = 0;
  let candidateVariance = 0;
  let covariance = 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const referenceDelta = reference[y * width + x] - referenceMean;
      const candidateDelta =
        shiftedValue(candidate, width, height, x, y, dx, dy) - candidateMean;
      referenceVariance += referenceDelta * referenceDelta;
      candidateVariance += candidateDelta * candidateDelta;
      covariance += referenceDelta * candidateDelta;
    }
  }
  referenceVariance /= count;
  candidateVariance /= count;
  covariance /= count;
  const c1 = (0.01 * 255) ** 2;
  const c2 = (0.03 * 255) ** 2;
  return (
    ((2 * referenceMean * candidateMean + c1) * (2 * covariance + c2)) /
    ((referenceMean ** 2 + candidateMean ** 2 + c1) *
      (referenceVariance + candidateVariance + c2))
  );
}

function phaseOrder(dx, dy) {
  return [Math.abs(dx) + Math.abs(dy), Math.abs(dy), Math.abs(dx), dy, dx];
}

function isEarlierPhase(left, right) {
  const leftOrder = phaseOrder(left.dx, left.dy);
  const rightOrder = phaseOrder(right.dx, right.dy);
  for (let index = 0; index < leftOrder.length; index += 1) {
    if (leftOrder[index] !== rightOrder[index]) {
      return leftOrder[index] < rightOrder[index];
    }
  }
  return false;
}

function selectPhase(reference, candidate, width, height, maximumOffset) {
  let best;
  for (let dy = -maximumOffset; dy <= maximumOffset; dy += 1) {
    for (let dx = -maximumOffset; dx <= maximumOffset; dx += 1) {
      const score = shiftedGlobalSsim(
        reference,
        candidate,
        width,
        height,
        dx,
        dy,
      );
      const phase = { dx, dy, score };
      if (
        !best ||
        score > best.score + Number.EPSILON ||
        (Math.abs(score - best.score) <= Number.EPSILON &&
          isEarlierPhase(phase, best))
      ) {
        best = phase;
      }
    }
  }
  return best;
}

function darkMask(plane, maximumLuma) {
  return Uint8Array.from(plane, (value) => (value <= maximumLuma ? 1 : 0));
}

function shiftedMask(mask, width, height, dx, dy) {
  const shifted = new Uint8Array(mask.length);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const sourceX = x - dx;
      const sourceY = y - dy;
      if (sourceX >= 0 && sourceX < width && sourceY >= 0 && sourceY < height) {
        shifted[y * width + x] = mask[sourceY * width + sourceX];
      }
    }
  }
  return shifted;
}

function hasDarkNeighbor(mask, width, height, x, y, radius) {
  for (
    let sampleY = Math.max(0, y - radius);
    sampleY <= Math.min(height - 1, y + radius);
    sampleY += 1
  ) {
    for (
      let sampleX = Math.max(0, x - radius);
      sampleX <= Math.min(width - 1, x + radius);
      sampleX += 1
    ) {
      if (mask[sampleY * width + sampleX] === 1) return true;
    }
  }
  return false;
}

function darkContentMetrics(reference, candidate, width, height, radius) {
  let referencePixels = 0;
  let candidatePixels = 0;
  let matchedReferencePixels = 0;
  let matchedCandidatePixels = 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const index = y * width + x;
      if (reference[index] === 1) {
        referencePixels += 1;
        if (hasDarkNeighbor(candidate, width, height, x, y, radius)) {
          matchedReferencePixels += 1;
        }
      }
      if (candidate[index] === 1) {
        candidatePixels += 1;
        if (hasDarkNeighbor(reference, width, height, x, y, radius)) {
          matchedCandidatePixels += 1;
        }
      }
    }
  }
  const recall =
    referencePixels === 0
      ? candidatePixels === 0
        ? 1
        : 0
      : matchedReferencePixels / referencePixels;
  const precision =
    candidatePixels === 0
      ? referencePixels === 0
        ? 1
        : 0
      : matchedCandidatePixels / candidatePixels;
  const f1 =
    precision + recall === 0
      ? 0
      : (2 * precision * recall) / (precision + recall);
  return Object.freeze({
    precision,
    recall,
    f1,
    reference_dark_pixels: referencePixels,
    candidate_dark_pixels: candidatePixels,
    matched_reference_dark_pixels: matchedReferencePixels,
    matched_candidate_dark_pixels: matchedCandidatePixels,
  });
}

export function measureCrossEngineScanFidelityV2(
  reference,
  candidate,
  parameterOverrides = {},
) {
  requireSameImageShape(reference, candidate);
  const parameters = Object.freeze({
    ...crossEngineScanFidelityParametersV2,
    ...parameterOverrides,
  });
  requireParameters(parameters);
  const referenceLuma = lumaPlane(reference);
  const candidateLuma = lumaPlane(candidate);
  const referenceFiltered = gaussianFilter(
    referenceLuma,
    reference.width,
    reference.height,
    parameters.gaussian_sigma_px,
    parameters.gaussian_radius_px,
  );
  const candidateFiltered = gaussianFilter(
    candidateLuma,
    candidate.width,
    candidate.height,
    parameters.gaussian_sigma_px,
    parameters.gaussian_radius_px,
  );
  const phase = selectPhase(
    referenceFiltered,
    candidateFiltered,
    reference.width,
    reference.height,
    parameters.maximum_phase_offset_px,
  );
  const referenceDark = darkMask(referenceLuma, parameters.dark_luma_max);
  const candidateDark = shiftedMask(
    darkMask(candidateLuma, parameters.dark_luma_max),
    candidate.width,
    candidate.height,
    phase.dx,
    phase.dy,
  );
  const darkContent = darkContentMetrics(
    referenceDark,
    candidateDark,
    reference.width,
    reference.height,
    parameters.dark_match_radius_px,
  );
  const passed =
    phase.score >= parameters.minimum_filtered_ssim &&
    darkContent.precision >= parameters.minimum_dark_precision &&
    darkContent.recall >= parameters.minimum_dark_recall &&
    darkContent.f1 >= parameters.minimum_dark_f1;
  return Object.freeze({
    algorithm: crossEngineScanFidelityAlgorithmV2,
    parameters,
    dimensions: Object.freeze({
      width: reference.width,
      height: reference.height,
    }),
    phase_offset_px: Object.freeze({ dx: phase.dx, dy: phase.dy }),
    filtered_ssim_luma: phase.score,
    dark_content: darkContent,
    passed,
  });
}
