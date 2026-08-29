const EPSILON = 1e-12;

export const registeredCropSsimAlgorithmV5 =
  "bp-registered-crop-global-luma-ssim-v1";

const dynamicObserverNominalRateHzV5 = 60;
const dynamicObserverIntervalMsV5 = 1000 / dynamicObserverNominalRateHzV5;

/**
 * Frozen freshness and clock-quality limits for decision-grade v5 samples.
 *
 * State may be at most two nominal 60 Hz display periods old. This permits
 * clock phase and one delayed application frame, but rejects evidence that
 * missed two complete display periods. Observer clock error may be at most
 * one nominal display period, both from the run origin and between adjacent
 * ticks.
 */
export const dynamicObserverTimingPolicyV5 = Object.freeze({
  nominal_rate_hz: dynamicObserverNominalRateHzV5,
  maximum_state_age_ms: Number((dynamicObserverIntervalMsV5 * 2).toFixed(9)),
  maximum_schedule_error_ms: Number(dynamicObserverIntervalMsV5.toFixed(9)),
  maximum_interval_error_ms: Number(dynamicObserverIntervalMsV5.toFixed(9)),
});

const TIMING_EPSILON_MS = 0.001;

function finiteNonnegative(value, name) {
  if (!Number.isFinite(value) || value < 0) {
    throw new TypeError(`${name} must be finite and nonnegative`);
  }
  return value;
}

/**
 * Reduces one fixed-cadence visibility observation to the three v5 quality
 * fields. Missing or stale raster coverage contributes zero quality.
 *
 * Each visible page supplies its visible intersection area in CSS pixels. A
 * current raster also supplies the fraction of that intersection which is
 * ready and the raster's device-pixel density relative to CSS pixel area.
 */
export function measureVisibleRasterFidelity(visiblePages) {
  if (!Array.isArray(visiblePages) || visiblePages.length === 0) {
    throw new TypeError("visiblePages must contain at least one page");
  }

  let visibleArea = 0;
  let readyArea = 0;
  let densityArea = 0;
  let readyPages = 0;

  for (const [index, page] of visiblePages.entries()) {
    const area = finiteNonnegative(
      page?.visible_intersection_area_css_px2,
      `visiblePages[${index}].visible_intersection_area_css_px2`,
    );
    if (area <= EPSILON) {
      throw new TypeError(
        `visiblePages[${index}] must have positive visible area`,
      );
    }
    const readyFraction = finiteNonnegative(
      page?.current_raster_ready_area_fraction,
      `visiblePages[${index}].current_raster_ready_area_fraction`,
    );
    if (readyFraction > 1) {
      throw new RangeError(`visiblePages[${index}] ready fraction exceeds 1`);
    }
    const density = finiteNonnegative(
      page?.current_raster_device_pixels_per_css_pixel ?? 0,
      `visiblePages[${index}].current_raster_device_pixels_per_css_pixel`,
    );

    visibleArea += area;
    readyArea += area * readyFraction;
    densityArea += area * readyFraction * density;
    if (readyFraction >= 1 - EPSILON) readyPages += 1;
  }

  return Object.freeze({
    visible_page_ready_fraction: readyPages / visiblePages.length,
    visible_raster_ready_area_fraction: readyArea / visibleArea,
    visible_raster_pixel_density: densityArea / visibleArea,
  });
}

/** Builds the exact inclusive monotonic sample grid frozen by the v5 contract. */
export function fixedCadenceOffsets(durationMs, rateHz) {
  if (!Number.isInteger(durationMs) || durationMs <= 0) {
    throw new TypeError("durationMs must be a positive integer");
  }
  if (!Number.isInteger(rateHz) || rateHz <= 0) {
    throw new TypeError("rateHz must be a positive integer");
  }
  const intervals = (durationMs * rateHz) / 1000;
  if (!Number.isInteger(intervals)) {
    throw new RangeError(
      "duration and rate must produce an integral interval count",
    );
  }
  return Object.freeze(
    Array.from({ length: intervals + 1 }, (_, index) =>
      Number(((index * 1000) / rateHz).toFixed(9)),
    ),
  );
}

function requireSameImageShape(reference, candidate) {
  for (const [name, image] of [
    ["reference", reference],
    ["candidate", candidate],
  ]) {
    if (
      !image ||
      !Number.isInteger(image.width) ||
      image.width <= 0 ||
      !Number.isInteger(image.height) ||
      image.height <= 0 ||
      ![3, 4].includes(image.channels) ||
      !(image.data instanceof Uint8Array) ||
      image.data.length !== image.width * image.height * image.channels
    ) {
      throw new TypeError(`${name} is not a packed RGB or RGBA image`);
    }
  }
  if (
    reference.width !== candidate.width ||
    reference.height !== candidate.height
  ) {
    throw new RangeError("registered crop dimensions differ");
  }
}

function lumaAt(image, pixel) {
  const offset = pixel * image.channels;
  return (
    image.data[offset] * 0.2126 +
    image.data[offset + 1] * 0.7152 +
    image.data[offset + 2] * 0.0722
  );
}

/**
 * Computes the frozen crop-level luma SSIM receipt. This uses population
 * moments over the already registered crop, 8-bit range, and the standard
 * K1=0.01/K2=0.03 stabilizers. Registration and resampling happen before this
 * function so a result is reproducible without a graphics dependency.
 */
export function registeredCropSsimLuma(reference, candidate) {
  requireSameImageShape(reference, candidate);
  const count = reference.width * reference.height;
  let referenceMean = 0;
  let candidateMean = 0;
  for (let pixel = 0; pixel < count; pixel += 1) {
    referenceMean += lumaAt(reference, pixel);
    candidateMean += lumaAt(candidate, pixel);
  }
  referenceMean /= count;
  candidateMean /= count;

  let referenceVariance = 0;
  let candidateVariance = 0;
  let covariance = 0;
  for (let pixel = 0; pixel < count; pixel += 1) {
    const referenceDelta = lumaAt(reference, pixel) - referenceMean;
    const candidateDelta = lumaAt(candidate, pixel) - candidateMean;
    referenceVariance += referenceDelta * referenceDelta;
    candidateVariance += candidateDelta * candidateDelta;
    covariance += referenceDelta * candidateDelta;
  }
  referenceVariance /= count;
  candidateVariance /= count;
  covariance /= count;

  const c1 = (0.01 * 255) ** 2;
  const c2 = (0.03 * 255) ** 2;
  const numerator =
    (2 * referenceMean * candidateMean + c1) * (2 * covariance + c2);
  const denominator =
    (referenceMean ** 2 + candidateMean ** 2 + c1) *
    (referenceVariance + candidateVariance + c2);
  const score = numerator / denominator;
  if (!Number.isFinite(score)) throw new Error("SSIM result is not finite");
  return Math.max(-1, Math.min(1, score));
}

export function validateDynamicFidelitySeries(samples, contract) {
  const offsets = fixedCadenceOffsets(
    contract?.duration_ms,
    contract?.observer?.rate_hz,
  );
  if (contract?.observer?.expected_sample_count !== offsets.length) {
    throw new Error("contract sample count does not match its fixed cadence");
  }
  if (!Array.isArray(samples) || samples.length !== offsets.length) {
    throw new Error(`expected exactly ${offsets.length} fidelity samples`);
  }
  const required = contract.required_sample_fields ?? [];
  const firstObservedMonotonicMs = samples[0]?.observed_monotonic_ms;
  if (!Number.isFinite(firstObservedMonotonicMs)) {
    throw new Error("sample 0 has invalid observed_monotonic_ms");
  }
  let previousObservedMonotonicMs = null;
  let previousScheduledOffsetMs = null;
  let previousStateObservedMonotonicMs = null;
  for (let index = 0; index < samples.length; index += 1) {
    const sample = samples[index];
    if (
      sample?.sample_index !== index ||
      !Number.isFinite(sample?.scheduled_offset_ms) ||
      Math.abs(sample.scheduled_offset_ms - offsets[index]) > 0.001
    ) {
      throw new Error(`sample ${index} is not on the fixed cadence`);
    }
    if (!Number.isFinite(sample.observed_monotonic_ms)) {
      throw new Error(`sample ${index} has invalid observed_monotonic_ms`);
    }
    if (
      previousObservedMonotonicMs !== null &&
      sample.observed_monotonic_ms <= previousObservedMonotonicMs
    ) {
      throw new Error(`sample ${index} observer timestamp is not increasing`);
    }
    const actualOffsetMs =
      sample.observed_monotonic_ms - firstObservedMonotonicMs;
    const scheduleErrorMs = actualOffsetMs - sample.scheduled_offset_ms;
    if (
      !Number.isFinite(sample.observer_tick_actual_offset_ms) ||
      Math.abs(sample.observer_tick_actual_offset_ms - actualOffsetMs) >
        TIMING_EPSILON_MS
    ) {
      throw new Error(`sample ${index} has invalid observer actual offset`);
    }
    if (
      !Number.isFinite(sample.observer_tick_schedule_error_ms) ||
      Math.abs(sample.observer_tick_schedule_error_ms - scheduleErrorMs) >
        TIMING_EPSILON_MS
    ) {
      throw new Error(`sample ${index} has invalid observer schedule error`);
    }
    if (
      Math.abs(scheduleErrorMs) >
      dynamicObserverTimingPolicyV5.maximum_schedule_error_ms +
        TIMING_EPSILON_MS
    ) {
      throw new Error(
        `sample ${index} observer schedule error exceeds ${dynamicObserverTimingPolicyV5.maximum_schedule_error_ms} ms`,
      );
    }
    if (previousObservedMonotonicMs !== null) {
      const actualIntervalMs =
        sample.observed_monotonic_ms - previousObservedMonotonicMs;
      const scheduledIntervalMs =
        sample.scheduled_offset_ms - previousScheduledOffsetMs;
      if (
        Math.abs(actualIntervalMs - scheduledIntervalMs) >
        dynamicObserverTimingPolicyV5.maximum_interval_error_ms +
          TIMING_EPSILON_MS
      ) {
        throw new Error(
          `sample ${index} observer interval error exceeds ${dynamicObserverTimingPolicyV5.maximum_interval_error_ms} ms`,
        );
      }
    }
    if (!Number.isFinite(sample.application_state_observed_monotonic_ms)) {
      throw new Error(
        `sample ${index} has invalid application state timestamp`,
      );
    }
    const stateAgeMs =
      sample.observed_monotonic_ms -
      sample.application_state_observed_monotonic_ms;
    if (
      !Number.isFinite(sample.application_state_age_ms) ||
      Math.abs(sample.application_state_age_ms - stateAgeMs) >
        TIMING_EPSILON_MS ||
      stateAgeMs < 0
    ) {
      throw new Error(`sample ${index} has invalid application state age`);
    }
    if (
      stateAgeMs >
      dynamicObserverTimingPolicyV5.maximum_state_age_ms + TIMING_EPSILON_MS
    ) {
      throw new Error(
        `sample ${index} application state age exceeds ${dynamicObserverTimingPolicyV5.maximum_state_age_ms} ms`,
      );
    }
    if (
      previousStateObservedMonotonicMs !== null &&
      sample.application_state_observed_monotonic_ms <
        previousStateObservedMonotonicMs
    ) {
      throw new Error(
        `sample ${index} application state timestamp moved backwards`,
      );
    }
    for (const field of required) {
      if (!Number.isFinite(sample?.[field]) || sample[field] < 0) {
        throw new Error(`sample ${index} has invalid ${field}`);
      }
    }
    for (const fraction of [
      "visible_page_ready_fraction",
      "visible_raster_ready_area_fraction",
    ]) {
      if (sample[fraction] > 1) {
        throw new Error(`sample ${index} has ${fraction} above 1`);
      }
    }
    previousObservedMonotonicMs = sample.observed_monotonic_ms;
    previousScheduledOffsetMs = sample.scheduled_offset_ms;
    previousStateObservedMonotonicMs =
      sample.application_state_observed_monotonic_ms;
  }
  return Object.freeze({
    expected_sample_count: offsets.length,
    observed_sample_count: samples.length,
    duration_ms: contract.duration_ms,
    observer_rate_hz: contract.observer.rate_hz,
    timing_policy: dynamicObserverTimingPolicyV5,
  });
}
