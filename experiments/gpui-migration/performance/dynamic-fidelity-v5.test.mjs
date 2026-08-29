import assert from "node:assert/strict";
import test from "node:test";

import {
  dynamicObserverTimingPolicyV5,
  fixedCadenceOffsets,
  measureVisibleRasterFidelity,
  registeredCropSsimAlgorithmV5,
  registeredCropSsimLuma,
  validateDynamicFidelitySeries,
} from "./dynamic-fidelity-v5.mjs";

test("measures missing, partial, and dense current raster coverage", () => {
  assert.deepEqual(
    measureVisibleRasterFidelity([
      {
        visible_intersection_area_css_px2: 600,
        current_raster_ready_area_fraction: 1,
        current_raster_device_pixels_per_css_pixel: 4,
      },
      {
        visible_intersection_area_css_px2: 300,
        current_raster_ready_area_fraction: 0.5,
        current_raster_device_pixels_per_css_pixel: 1,
      },
      {
        visible_intersection_area_css_px2: 100,
        current_raster_ready_area_fraction: 0,
        current_raster_device_pixels_per_css_pixel: 0,
      },
    ]),
    {
      visible_page_ready_fraction: 1 / 3,
      visible_raster_ready_area_fraction: 0.75,
      visible_raster_pixel_density: 2.55,
    },
  );
});

test("builds exact inclusive 60Hz and 120Hz v5 cadence grids", () => {
  const observer = fixedCadenceOffsets(32_000, 60);
  const pointer = fixedCadenceOffsets(32_000, 120);
  assert.equal(observer.length, 1_921);
  assert.equal(pointer.length, 3_841);
  assert.equal(observer[0], 0);
  assert.equal(observer.at(-1), 32_000);
  assert.equal(pointer.at(-1), 32_000);
});

test("computes deterministic registered crop luma SSIM", () => {
  assert.equal(
    registeredCropSsimAlgorithmV5,
    "bp-registered-crop-global-luma-ssim-v1",
  );
  const reference = {
    width: 2,
    height: 2,
    channels: 3,
    data: Uint8Array.from([0, 0, 0, 255, 255, 255, 64, 64, 64, 192, 192, 192]),
  };
  assert.equal(registeredCropSsimLuma(reference, reference), 1);
  const changed = {
    ...reference,
    data: Uint8Array.from(reference.data, (value) => Math.min(255, value + 20)),
  };
  const score = registeredCropSsimLuma(reference, changed);
  assert.ok(score < 1 && score > 0.98);
  assert.throws(
    () => registeredCropSsimLuma(reference, { ...changed, width: 1 }),
    /registered crop dimensions differ|packed RGB/,
  );
});

test("fails closed on missing or off-cadence fidelity samples", () => {
  const contract = {
    duration_ms: 1_000,
    observer: { rate_hz: 2, expected_sample_count: 3 },
    required_sample_fields: [
      "visible_page_ready_fraction",
      "visible_raster_ready_area_fraction",
      "visible_raster_pixel_density",
    ],
  };
  const samples = fixedCadenceOffsets(1_000, 2).map(
    (scheduled_offset_ms, sample_index) => ({
      sample_index,
      scheduled_offset_ms,
      observed_monotonic_ms: 100 + scheduled_offset_ms,
      observer_tick_actual_offset_ms: scheduled_offset_ms,
      observer_tick_schedule_error_ms: 0,
      application_state_observed_monotonic_ms:
        99 + scheduled_offset_ms,
      application_state_age_ms: 1,
      visible_page_ready_fraction: 1,
      visible_raster_ready_area_fraction: 1,
      visible_raster_pixel_density: 1,
    }),
  );
  assert.deepEqual(validateDynamicFidelitySeries(samples, contract), {
    expected_sample_count: 3,
    observed_sample_count: 3,
    duration_ms: 1_000,
    observer_rate_hz: 2,
    timing_policy: dynamicObserverTimingPolicyV5,
  });
  assert.throws(
    () => validateDynamicFidelitySeries(samples.slice(1), contract),
    /exactly 3/,
  );
  assert.throws(
    () =>
      validateDynamicFidelitySeries(
        samples.map((sample, index) =>
          index === 1 ? { ...sample, scheduled_offset_ms: 501 } : sample,
        ),
        contract,
      ),
    /not on the fixed cadence/,
  );
});

test("fails closed on stale application state and actual observer clock drift", () => {
  const contract = {
    duration_ms: 1_000,
    observer: { rate_hz: 2, expected_sample_count: 3 },
    required_sample_fields: [
      "visible_page_ready_fraction",
      "visible_raster_ready_area_fraction",
      "visible_raster_pixel_density",
    ],
  };
  const samples = fixedCadenceOffsets(1_000, 2).map(
    (scheduled_offset_ms, sample_index) => ({
      sample_index,
      scheduled_offset_ms,
      observed_monotonic_ms: 100 + scheduled_offset_ms,
      observer_tick_actual_offset_ms: scheduled_offset_ms,
      observer_tick_schedule_error_ms: 0,
      application_state_observed_monotonic_ms:
        99 + scheduled_offset_ms,
      application_state_age_ms: 1,
      visible_page_ready_fraction: 1,
      visible_raster_ready_area_fraction: 1,
      visible_raster_pixel_density: 1,
    }),
  );
  assert.throws(
    () =>
      validateDynamicFidelitySeries(
        samples.map((sample, index) =>
          index === 2
            ? {
                ...sample,
                application_state_observed_monotonic_ms: 500,
                application_state_age_ms: 600,
              }
            : sample,
        ),
        contract,
      ),
    /application state age exceeds 33\.333333333 ms/,
  );
  assert.throws(
    () =>
      validateDynamicFidelitySeries(
        samples.map((sample, index) =>
          index === 2
            ? {
                ...sample,
                observed_monotonic_ms: 1_118,
                observer_tick_actual_offset_ms: 1_018,
                observer_tick_schedule_error_ms: 18,
                application_state_observed_monotonic_ms: 1_117,
              }
            : sample,
        ),
        contract,
      ),
    /observer schedule error exceeds 16\.666666667 ms/,
  );
});
