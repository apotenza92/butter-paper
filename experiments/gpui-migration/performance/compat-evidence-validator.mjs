import { isDeepStrictEqual } from "node:util";

import {
  crossEngineScanFidelityAlgorithmV2,
  crossEngineScanFidelityParametersV2,
} from "./scan-fidelity-v2.mjs";

export const longbridgeCompatProfile = "longbridge-gpui-component-v1";

const sha256Pattern = /^[a-f0-9]{64}$/;
const terminalEvents = new Set([
  "scenario-complete",
  "scenario-error",
  "scenario-failed",
]);
const smallOpenCrop = Object.freeze({
  fixtureId: "bp-single-page-v1",
  commandId: "small:open-settle",
  cropId: "single-registration",
  pageId: "bp-single-page-v1:page:001",
  registrationSha256:
    "cc231d7d5da2ef403509e58565a19fb1855fea3da6aca1436d56dbc38ce218ef",
  pageSizePoints: Object.freeze({ width: 612, height: 792 }),
  pdfRect: Object.freeze({ x: 36, y: 36, width: 540, height: 720 }),
  minimumDensity: 1,
});

function positiveDimensions(value) {
  return (
    Number.isInteger(value?.width) &&
    value.width > 0 &&
    Number.isInteger(value?.height) &&
    value.height > 0
  );
}

function pixelBounds(value) {
  return (
    Number.isInteger(value?.x) &&
    value.x >= 0 &&
    Number.isInteger(value?.y) &&
    value.y >= 0 &&
    positiveDimensions(value)
  );
}

function mappedPixelBounds(value) {
  return (
    Number.isFinite(value?.x) &&
    value.x >= 0 &&
    Number.isFinite(value?.y) &&
    value.y >= 0 &&
    Number.isFinite(value?.width) &&
    value.width > 0 &&
    Number.isFinite(value?.height) &&
    value.height > 0
  );
}

function extractedBoundsEncloseMapped(mapped, extracted) {
  return (
    extracted.x === Math.floor(mapped.x) &&
    extracted.y === Math.floor(mapped.y) &&
    extracted.x + extracted.width === Math.ceil(mapped.x + mapped.width) &&
    extracted.y + extracted.height === Math.ceil(mapped.y + mapped.height)
  );
}

function exactPixelProof(event) {
  return (
    event.exact_pixel_match === true &&
    sha256Pattern.test(event.registered_reference_crop_sha256 ?? "") &&
    event.candidate_crop_sha256 === event.registered_reference_crop_sha256
  );
}

function registeredMetricProof(event) {
  const metric = event.metric;
  return (
    sha256Pattern.test(event.registered_reference_crop_sha256 ?? "") &&
    metric?.algorithm === crossEngineScanFidelityAlgorithmV2 &&
    isDeepStrictEqual(metric.parameters, crossEngineScanFidelityParametersV2) &&
    Number.isInteger(metric?.phase_offset_px?.dx) &&
    Math.abs(metric.phase_offset_px.dx) <=
      crossEngineScanFidelityParametersV2.maximum_phase_offset_px &&
    Number.isInteger(metric?.phase_offset_px?.dy) &&
    Math.abs(metric.phase_offset_px.dy) <=
      crossEngineScanFidelityParametersV2.maximum_phase_offset_px &&
    Number.isFinite(metric?.filtered_ssim_luma) &&
    metric.filtered_ssim_luma >=
      crossEngineScanFidelityParametersV2.minimum_filtered_ssim &&
    Number.isFinite(metric?.dark_content?.precision) &&
    metric.dark_content.precision >=
      crossEngineScanFidelityParametersV2.minimum_dark_precision &&
    Number.isFinite(metric?.dark_content?.recall) &&
    metric.dark_content.recall >=
      crossEngineScanFidelityParametersV2.minimum_dark_recall &&
    Number.isFinite(metric?.dark_content?.f1) &&
    metric.dark_content.f1 >=
      crossEngineScanFidelityParametersV2.minimum_dark_f1 &&
    metric.passed === true
  );
}

export function validateCompatPresentedCrop(
  _events,
  { fixtureId, commandId, driverReceipt },
) {
  const errors = [];
  if (
    fixtureId !== smallOpenCrop.fixtureId ||
    commandId !== smallOpenCrop.commandId
  ) {
    errors.push("unsupported Longbridge compatibility crop contract");
  }
  const candidates =
    driverReceipt?.event === "compat-presented-crop-evidence" &&
    driverReceipt.command_id === commandId
      ? [driverReceipt]
      : [];
  if (candidates.length !== 1) {
    errors.push(
      `expected exactly one independent driver compatibility presented-crop receipt; received ${candidates.length}`,
    );
    return { passed: false, errors, receipt: null };
  }
  const receipt = candidates[0];
  if (
    receipt.fixture_id !== smallOpenCrop.fixtureId ||
    receipt.crop_id !== smallOpenCrop.cropId ||
    receipt.page_id !== smallOpenCrop.pageId ||
    receipt.registration_sha256 !== smallOpenCrop.registrationSha256
  ) {
    errors.push(
      "presented crop is not bound to the frozen fixture registration",
    );
  }
  if (
    receipt.acceptance_source !== "XGetImage-presented-client-drawable" ||
    receipt.candidate_resampled !== false ||
    !sha256Pattern.test(receipt.presented_drawable_artifact_sha256 ?? "") ||
    receipt.retained_ppm_sha256 !==
      receipt.presented_drawable_artifact_sha256 ||
    !sha256Pattern.test(receipt.candidate_crop_sha256 ?? "")
  ) {
    errors.push("candidate crop bytes are not native, hash-bound evidence");
  }
  if (
    !isDeepStrictEqual(
      receipt.page_size_points,
      smallOpenCrop.pageSizePoints,
    ) ||
    !isDeepStrictEqual(receipt.pdf_rect, smallOpenCrop.pdfRect)
  ) {
    errors.push("presented crop PDF geometry drifted from the frozen contract");
  }
  if (
    !positiveDimensions(receipt.candidate_dimensions) ||
    !mappedPixelBounds(receipt.mapped_bounds_pixels) ||
    !pixelBounds(receipt.extracted_bounds_pixels) ||
    !extractedBoundsEncloseMapped(
      receipt.mapped_bounds_pixels,
      receipt.extracted_bounds_pixels,
    ) ||
    receipt.candidate_dimensions.width !==
      receipt.extracted_bounds_pixels.width ||
    receipt.candidate_dimensions.height !==
      receipt.extracted_bounds_pixels.height
  ) {
    errors.push(
      "presented crop extracted pixels do not exactly enclose its mapped float geometry",
    );
  }
  if (
    !Number.isFinite(receipt.rendered_device_pixel_ratio) ||
    receipt.rendered_device_pixel_ratio < smallOpenCrop.minimumDensity ||
    !Number.isFinite(receipt.display_scale_factor) ||
    receipt.display_scale_factor <= 0
  ) {
    errors.push(
      "presented crop scale evidence is missing or below the contract",
    );
  }
  if (
    !Number.isInteger(receipt.painted_render_generation) ||
    receipt.painted_render_generation < 1 ||
    receipt.painted_generation_stable !== true
  ) {
    errors.push("presented crop is not bound to one stable painted generation");
  }
  if (!exactPixelProof(receipt) && !registeredMetricProof(receipt)) {
    errors.push(
      "presented crop has neither an exact hash match nor a passing registered metric",
    );
  }
  return { passed: errors.length === 0, errors, receipt };
}

export function validateCompatEventSequence(events, { scenario, pid }) {
  const errors = [];
  let previousTime = -Infinity;
  const terminalIndexes = [];

  for (const [index, event] of events.entries()) {
    if (event?.schema_version !== 1) {
      errors.push(`event ${index} has the wrong schema_version`);
    }
    if (event?.runtime !== "gpui") {
      errors.push(`event ${index} has the wrong runtime`);
    }
    if (event?.scenario !== scenario) {
      errors.push(`event ${index} has the wrong scenario`);
    }
    if (!Number.isInteger(event?.pid) || event.pid !== pid) {
      errors.push(`event ${index} has the wrong PID`);
    }
    if (!Number.isFinite(event?.t_ms) || event.t_ms < 0) {
      errors.push(`event ${index} has an invalid t_ms`);
    } else {
      if (event.t_ms < previousTime) {
        errors.push(`event ${index} moved backwards in monotonic time`);
      }
      previousTime = event.t_ms;
    }
    if (typeof event?.event !== "string" || event.event.length === 0) {
      errors.push(`event ${index} has no event name`);
    } else if (terminalEvents.has(event.event)) {
      terminalIndexes.push(index);
    }
  }

  if (terminalIndexes.length !== 1) {
    errors.push(
      `expected exactly one terminal event; received ${terminalIndexes.length}`,
    );
  } else if (terminalIndexes[0] !== events.length - 1) {
    errors.push("the terminal event is not the final event");
  }

  return { passed: errors.length === 0, errors };
}
