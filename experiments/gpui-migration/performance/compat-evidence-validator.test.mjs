import assert from "node:assert/strict";
import test from "node:test";

import {
  longbridgeCompatProfile,
  validateCompatEventSequence,
  validateCompatPresentedCrop,
} from "./compat-evidence-validator.mjs";

const digest = (character) => character.repeat(64);

function validCropEvidence(overrides = {}) {
  return {
    event: "compat-presented-crop-evidence",
    command_id: "small:open-settle",
    fixture_id: "bp-single-page-v1",
    crop_id: "single-registration",
    page_id: "bp-single-page-v1:page:001",
    registration_sha256:
      "cc231d7d5da2ef403509e58565a19fb1855fea3da6aca1436d56dbc38ce218ef",
    acceptance_source: "XGetImage-presented-client-drawable",
    candidate_resampled: false,
    presented_drawable_artifact_sha256: digest("c"),
    retained_ppm_sha256: digest("c"),
    candidate_crop_sha256: digest("a"),
    registered_reference_crop_sha256: digest("a"),
    candidate_dimensions: { width: 540, height: 720 },
    mapped_bounds_pixels: { x: 100, y: 40, width: 540, height: 720 },
    extracted_bounds_pixels: { x: 100, y: 40, width: 540, height: 720 },
    page_size_points: { width: 612, height: 792 },
    pdf_rect: { x: 36, y: 36, width: 540, height: 720 },
    rendered_device_pixel_ratio: 1,
    display_scale_factor: 1,
    painted_render_generation: 4,
    painted_generation_stable: true,
    exact_pixel_match: true,
    ...overrides,
  };
}

test("Longbridge fixed-crop proof requires real presented candidate evidence", () => {
  assert.equal(longbridgeCompatProfile, "longbridge-gpui-component-v1");
  assert.equal(
    validateCompatPresentedCrop([], {
      fixtureId: "bp-single-page-v1",
      commandId: "small:open-settle",
    }).passed,
    false,
  );

  const accepted = validateCompatPresentedCrop([], {
    fixtureId: "bp-single-page-v1",
    commandId: "small:open-settle",
    driverReceipt: validCropEvidence(),
  });
  assert.equal(accepted.passed, true);
  assert.equal(accepted.receipt.candidate_crop_sha256, digest("a"));
});

test("Longbridge fixed-crop proof rejects hash, geometry, scale, and metric drift", () => {
  const validate = (overrides) =>
    validateCompatPresentedCrop([], {
      fixtureId: "bp-single-page-v1",
      commandId: "small:open-settle",
      driverReceipt: validCropEvidence(overrides),
    });

  assert.equal(
    validate({ registered_reference_crop_sha256: digest("b") }).passed,
    false,
  );
  assert.equal(
    validate({ pdf_rect: { x: 0, y: 0, width: 1, height: 1 } }).passed,
    false,
  );
  assert.equal(validate({ rendered_device_pixel_ratio: 0 }).passed, false);
  assert.equal(validate({ retained_ppm_sha256: digest("d") }).passed, false);
  assert.equal(
    validate({
      exact_pixel_match: false,
      metric: {
        algorithm: "bp-cross-engine-binary-scan-fidelity-v2",
        parameters: {
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
        },
        phase_offset_px: { dx: 0, dy: 0 },
        filtered_ssim_luma: 0.96,
        dark_content: { precision: 1, recall: 1, f1: 1 },
        passed: true,
      },
    }).passed,
    false,
  );
});

test("Longbridge crop preserves mapped float geometry separately from enclosing extracted pixels", () => {
  const accepted = validateCompatPresentedCrop([], {
    fixtureId: "bp-single-page-v1",
    commandId: "small:open-settle",
    driverReceipt: validCropEvidence({
      mapped_bounds_pixels: {
        x: 100.25,
        y: 40.5,
        width: 539.5,
        height: 719.25,
      },
      extracted_bounds_pixels: { x: 100, y: 40, width: 540, height: 720 },
    }),
  });
  assert.equal(accepted.passed, true);

  const rejected = validateCompatPresentedCrop([], {
    fixtureId: "bp-single-page-v1",
    commandId: "small:open-settle",
    driverReceipt: validCropEvidence({
      mapped_bounds_pixels: {
        x: 100.25,
        y: 40.5,
        width: 539.5,
        height: 719.25,
      },
      extracted_bounds_pixels: { x: 101, y: 40, width: 539, height: 720 },
    }),
  });
  assert.equal(rejected.passed, false);
  assert.match(rejected.errors.join("; "), /enclose/);
});

test("Longbridge fixed-crop proof accepts a registered cross-engine metric with distinct hashes", () => {
  const result = validateCompatPresentedCrop([], {
    fixtureId: "bp-single-page-v1",
    commandId: "small:open-settle",
    driverReceipt: validCropEvidence({
      exact_pixel_match: false,
      registered_reference_crop_sha256: digest("b"),
      metric: {
        algorithm: "bp-cross-engine-binary-scan-fidelity-v2",
        parameters: {
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
        },
        phase_offset_px: { dx: 1, dy: -1 },
        filtered_ssim_luma: 0.97,
        dark_content: { precision: 0.99, recall: 0.99, f1: 0.99 },
        passed: true,
      },
    }),
  });
  assert.equal(result.passed, true);
});

test("Longbridge crop proof never trusts an application-authored crop event", () => {
  const result = validateCompatPresentedCrop([validCropEvidence()], {
    fixtureId: "bp-single-page-v1",
    commandId: "small:open-settle",
  });
  assert.equal(result.passed, false);
  assert.match(result.errors.join("; "), /driver/);
});

function event(eventName, tMs, overrides = {}) {
  return {
    schema_version: 1,
    runtime: "gpui",
    scenario: "open-pdf",
    event: eventName,
    t_ms: tMs,
    pid: 4242,
    ...overrides,
  };
}

test("Longbridge event provenance binds one monotonic terminal stream to runtime, scenario, and PID", () => {
  assert.equal(
    validateCompatEventSequence(
      [event("process-start", 0), event("scenario-complete", 250)],
      { scenario: "open-pdf", pid: 4242 },
    ).passed,
    true,
  );

  for (const events of [
    [
      event("process-start", 0, { runtime: "electron" }),
      event("scenario-complete", 1),
    ],
    [
      event("process-start", 0, { scenario: "zoom" }),
      event("scenario-complete", 1),
    ],
    [event("process-start", 0, { pid: 7 }), event("scenario-complete", 1)],
    [event("process-start", -1), event("scenario-complete", 1)],
    [event("process-start", 2), event("scenario-complete", 1)],
    [event("scenario-complete", 1), event("process-start", 2)],
    [event("scenario-complete", 1), event("scenario-complete", 2)],
    [event("scenario-failed", 1), event("scenario-complete", 2)],
  ]) {
    assert.equal(
      validateCompatEventSequence(events, { scenario: "open-pdf", pid: 4242 })
        .passed,
      false,
    );
  }
});
