#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { pairedLogRatioBootstrap } from "./decision-statistics.mjs";
import {
  decisionContractV4,
  decisionContractVersionV4,
  requiredLiveEvidenceGateIdsV4,
} from "./decision-contract-v4.mjs";
import { loadMaterializedComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import {
  buildScenarioContractV4,
  representativeScenarioDefinitionsV4,
  representativeTimedScenarioIdsV4,
  scenarioContractVersionV4,
} from "./scenario-contract-v4.mjs";
import {
  componentInputLaneV4,
  validateV4ComponentReport,
} from "./run-paired-v4.mjs";

export const pairedSummaryV4SchemaVersion = 4;
export const defaultBootstrapSamplesV4 = 100_000;
const implementations = Object.freeze(["electron", "gpui"]);
const sha256Pattern = /^[0-9a-f]{64}$/;
const gpuMeasurementFloors = Object.freeze({
  baseline_adjusted_gpu_peak_memory_mib: 1,
  baseline_adjusted_gpu_utilization_p95_percent: 0.1,
});
const nativeApplicationAckScopes = new Set([
  "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout",
  "gpui-input-latency-histogram-to-platform-draw-submission-not-physical-scanout",
]);
const familyThresholds = Object.freeze({
  ...decisionContractV4.decision.primary_metric_upper_95_thresholds,
});

const familyMeasurementNames = Object.freeze({
  sustained_cpu_work: Object.freeze(["cpu_seconds"]),
  process_memory: Object.freeze(["cgroup_peak_memory_bytes"]),
  native_interaction_and_frame_pacing: Object.freeze([
    "application_frame_interval_p95_ms",
    "native_input_to_application_frame_ack_p95_ms",
  ]),
  product_latency: Object.freeze(["product_wall_or_latency_ms"]),
  gpu_resource_pressure: Object.freeze([
    "baseline_adjusted_gpu_peak_memory_mib",
    "baseline_adjusted_gpu_utilization_p95_percent",
  ]),
});

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalize(value[key])]),
    );
  }
  return value;
}

export function canonicalSha256V4(value) {
  return createHash("sha256")
    .update(JSON.stringify(canonicalize(value)))
    .digest("hex");
}

function sha256Bytes(value) {
  return createHash("sha256").update(value).digest("hex");
}

function percentile(values, fraction) {
  if (values.length === 0) return null;
  const ordered = [...values].sort((left, right) => left - right);
  const position = (ordered.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  return lower === upper
    ? ordered[lower]
    : ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower);
}

function summarizeValues(values) {
  return values.length === 0
    ? null
    : {
        count: values.length,
        median: percentile(values, 0.5),
        p95: percentile(values, 0.95),
        maximum: Math.max(...values),
      };
}

function geometricMean(values, weights = null) {
  if (
    values.length === 0 ||
    values.some((value) => !Number.isFinite(value) || value <= 0)
  ) {
    return null;
  }
  const actualWeights = weights ?? values.map(() => 1 / values.length);
  const weightTotal = actualWeights.reduce((sum, weight) => sum + weight, 0);
  if (!Number.isFinite(weightTotal) || weightTotal <= 0) return null;
  return Math.exp(
    values.reduce(
      (sum, value, index) =>
        sum + (actualWeights[index] / weightTotal) * Math.log(value),
      0,
    ),
  );
}

function addFinite(target, key, value) {
  if (Number.isFinite(value) && value > 0) target[key] = value;
}

function addNonnegativeFinite(target, key, value) {
  if (Number.isFinite(value) && value >= 0) target[key] = value;
}

function eventValues(report, eventName, field) {
  return (report?.iterations?.[0]?.events ?? [])
    .filter(
      (event) => event.event === eventName && Number.isFinite(event[field]),
    )
    .map((event) => event[field]);
}

function extractProductLatency(report) {
  const candidates = [report?.summary?.product_latency_ms?.p95];
  if (report?.component === "native-snap-transform-120hz") {
    candidates.push(
      ...eventValues(
        report,
        "native-v5-snap-presentation-evidence",
        "duration_ms",
      ),
    );
  } else if (report?.scenario === "open-pdf") {
    const visibleEvent =
      report?.implementation === "electron"
        ? "first-page-visible"
        : "viewport-visible";
    candidates.push(...eventValues(report, visibleEvent, "duration_ms"));
  } else if (report?.scenario === "page-navigation") {
    candidates.push(
      ...eventValues(report, "page-navigation-completed", "duration_ms"),
    );
  } else if (report?.scenario === "zoom") {
    candidates.push(...eventValues(report, "zoom-completed", "duration_ms"));
  } else {
    candidates.push(...eventValues(report, "operation-visible", "duration_ms"));
  }
  const retained = candidates.filter(
    (value) => Number.isFinite(value) && value > 0,
  );
  return retained.length > 0 ? Math.max(...retained) : null;
}

export function extractComponentMeasurementsV4(
  report,
  { nativeComponent = false } = {},
) {
  const values = {};
  const missing = [];
  addFinite(
    values,
    "cpu_seconds",
    report?.summary?.process_tree?.cpu_seconds?.median,
  );
  addFinite(
    values,
    "cgroup_peak_memory_bytes",
    report?.summary?.process_tree?.cgroup_memory_peak_bytes?.median,
  );
  addFinite(
    values,
    "product_wall_ms",
    report?.summary?.wall_duration_ms?.median,
  );
  addFinite(values, "product_latency_ms", extractProductLatency(report));
  if (values.product_latency_ms) {
    values.product_wall_or_latency_ms = values.product_latency_ms;
    values.product_wall_or_latency_source = "product-latency";
  } else if (values.product_wall_ms) {
    values.product_wall_or_latency_ms = values.product_wall_ms;
    values.product_wall_or_latency_source = "component-wall";
    missing.push(
      "product_latency_ms (component wall retained as the declared fallback)",
    );
  }

  if (nativeComponent) {
    addFinite(
      values,
      "application_frame_interval_p95_ms",
      report?.summary?.application_frame_intervals_ms?.p95,
    );
    const nativeProxy =
      report?.summary?.native_application_frame_acknowledgement_proxy;
    if (
      nativeProxy?.physical_scanout_observed === false &&
      Number.isInteger(nativeProxy.sample_count) &&
      nativeProxy.sample_count > 0 &&
      nativeApplicationAckScopes.has(nativeProxy.receipt_scope)
    ) {
      addFinite(
        values,
        "native_input_to_application_frame_ack_p95_ms",
        report?.summary?.native_input_to_application_frame_ack_ms?.p95,
      );
    }
  }

  const gpu = report?.summary?.gpu_whole_device_baseline_adjusted;
  addNonnegativeFinite(
    values,
    "baseline_adjusted_gpu_peak_memory_mib",
    gpu?.memory_used_mib?.max,
  );
  addNonnegativeFinite(
    values,
    "baseline_adjusted_gpu_utilization_p95_percent",
    gpu?.utilization_percent?.p95,
  );
  const recovery = eventValues(
    report,
    "comparison-memory-recovery",
    "released_render_bytes",
  );
  if (recovery.length > 0)
    addFinite(values, "recovery_released_render_bytes", recovery[0]);

  for (const key of [
    "cpu_seconds",
    "cgroup_peak_memory_bytes",
    "product_wall_or_latency_ms",
  ]) {
    if (!Number.isFinite(values[key])) missing.push(key);
  }
  if (nativeComponent) {
    for (const key of familyMeasurementNames.native_interaction_and_frame_pacing) {
      if (!Number.isFinite(values[key])) missing.push(key);
    }
  }
  for (const key of familyMeasurementNames.gpu_resource_pressure) {
    if (!Number.isFinite(values[key])) missing.push(key);
  }
  return { values, missing };
}

function localArtifactPath(input, retainedPath) {
  if (typeof retainedPath !== "string" || retainedPath.length === 0)
    return null;
  return resolve(input, basename(retainedPath));
}

async function loadHashedJson(path, expectedSha256, label) {
  let bytes;
  try {
    bytes = await readFile(path);
  } catch (error) {
    return {
      passed: false,
      error: `${label}: cannot read ${path}: ${error.message}`,
      value: null,
    };
  }
  const observedSha256 = sha256Bytes(bytes);
  if (
    !sha256Pattern.test(expectedSha256 ?? "") ||
    observedSha256 !== expectedSha256
  ) {
    return {
      passed: false,
      error: `${label}: SHA-256 mismatch; expected ${expectedSha256 ?? "missing"}, got ${observedSha256}`,
      value: null,
      observed_sha256: observedSha256,
    };
  }
  try {
    return {
      passed: true,
      value: JSON.parse(bytes),
      observed_sha256: observedSha256,
    };
  } catch (error) {
    return {
      passed: false,
      error: `${label}: invalid JSON: ${error.message}`,
      value: null,
    };
  }
}

async function verifyManifestChecksum(input, manifestBytes) {
  const checksumPath = resolve(input, "run-manifest-v4.sha256");
  let checksum;
  try {
    checksum = await readFile(checksumPath, "utf8");
  } catch (error) {
    return `run manifest checksum is missing: ${error.message}`;
  }
  const match = /^([0-9a-f]{64})  run-manifest-v4\.json\s*$/.exec(checksum);
  if (!match) return "run-manifest-v4.sha256 has an invalid format";
  const observed = sha256Bytes(manifestBytes);
  return observed === match[1]
    ? null
    : `run manifest SHA-256 mismatch; expected ${match[1]}, got ${observed}`;
}

function bundleIdentity({ phase, journey, pair, implementation }) {
  return `${phase}\0${journey}\0${pair}\0${implementation}`;
}

function plannedJourneyErrors(manifest) {
  const errors = [];
  const plans = new Map(
    (manifest?.plan?.journeys ?? []).map((journey) => [
      journey.scenario,
      journey,
    ]),
  );
  for (const scenario of representativeTimedScenarioIdsV4) {
    const observed = plans.get(scenario);
    const expected = representativeScenarioDefinitionsV4[scenario];
    if (!observed) {
      errors.push(`${scenario}: journey plan is missing`);
      continue;
    }
    if (
      observed.journey_id !== expected.journey_id ||
      observed.fixture_id !== expected.fixture_id ||
      observed.fixture_sha256 !== expected.fixture_sha256
    ) {
      errors.push(`${scenario}: journey or fixture identity does not match v4`);
    }
    if (
      JSON.stringify(observed.component_order) !==
        JSON.stringify(expected.current_runner_components) ||
      JSON.stringify(observed.component_weights) !==
        JSON.stringify(expected.component_weights)
    ) {
      errors.push(`${scenario}: component order or weights do not match v4`);
    }
    if (
      JSON.stringify(observed.component_command_ids) !==
      JSON.stringify(expected.component_command_ids)
    ) {
      errors.push(`${scenario}: component command mapping does not match v4`);
    }
    for (const blocker of expected.blocked_commands) {
      errors.push(`${scenario}:${blocker.command_id}: ${blocker.reason}`);
    }
  }
  if (plans.size !== representativeTimedScenarioIdsV4.length) {
    errors.push(
      "run plan must contain exactly the five representative journeys",
    );
  }
  return errors;
}

export async function loadVerifiedV4Run(inputDirectory) {
  const input = resolve(inputDirectory);
  const manifestPath = resolve(input, "run-manifest-v4.json");
  let manifestBytes;
  try {
    manifestBytes = await readFile(manifestPath);
  } catch (error) {
    return {
      input,
      manifest: null,
      bundles: [],
      failures: [`cannot read run-manifest-v4.json: ${error.message}`],
    };
  }
  const failures = [];
  const checksumError = await verifyManifestChecksum(input, manifestBytes);
  if (checksumError) failures.push(checksumError);
  let manifest;
  try {
    manifest = JSON.parse(manifestBytes);
  } catch (error) {
    return {
      input,
      manifest: null,
      bundles: [],
      failures: [...failures, `run manifest is invalid JSON: ${error.message}`],
    };
  }
  if (manifest.complete !== true || manifest.outcome !== "passed") {
    failures.push("run manifest is not complete and passed");
  }
  if (
    manifest.plan?.protocol_version !== "bp-perf-v4" ||
    manifest.plan?.decision_contract_version !== decisionContractVersionV4 ||
    manifest.plan?.scenario_contract_version !== scenarioContractVersionV4
  ) {
    failures.push("run manifest protocol or contract identity is not v4");
  }
  if (
    manifest.plan?.ready !== true ||
    (manifest.plan?.blockers?.length ?? 0) !== 0
  ) {
    failures.push("run manifest plan was not executable without blockers");
  }
  failures.push(...plannedJourneyErrors(manifest));
  if (
    manifest.excluded_lanes?.usgs_large_sheet_stress !==
    "non-inferential and not scheduled"
  ) {
    failures.push("USGS stress must remain excluded from inference");
  }
  if (
    manifest.excluded_lanes?.private_hibbeler_935 !==
    "blocked-not-transferred and not scheduled"
  ) {
    failures.push("Hibbeler must remain blocked-not-transferred");
  }

  const workload = await loadMaterializedComparisonWorkloadV4();
  if (manifest.workload?.manifest_id !== workload.manifest_id) {
    failures.push("materialized workload manifest id does not match the run");
  }
  const scenarioContracts = new Map(
    representativeTimedScenarioIdsV4.map((scenario) => [
      scenario,
      buildScenarioContractV4(workload, scenario),
    ]),
  );
  const bundleRecords = [];
  const identities = new Set();
  const rawPaths = new Set();
  for (const entry of manifest.bundles ?? []) {
    const identity = bundleIdentity(entry);
    const recordFailures = [];
    if (identities.has(identity))
      recordFailures.push("duplicate retained bundle identity");
    identities.add(identity);
    const bundlePath = localArtifactPath(input, entry.path);
    const loadedBundle = bundlePath
      ? await loadHashedJson(
          bundlePath,
          entry.sha256,
          `${entry.bundle_id ?? identity} bundle`,
        )
      : {
          passed: false,
          error: `${entry.bundle_id ?? identity}: bundle path is missing`,
          value: null,
        };
    if (!loadedBundle.passed) recordFailures.push(loadedBundle.error);
    const bundle = loadedBundle.value;
    if (bundle) {
      for (const key of [
        "phase",
        "journey",
        "pair",
        "pair_position",
        "implementation",
      ]) {
        if (bundle[key] !== entry[key])
          recordFailures.push(`bundle ${key} does not match run manifest`);
      }
      if (
        bundle.passed !== true ||
        bundle.inference_eligible !== (bundle.phase === "final")
      ) {
        recordFailures.push("bundle pass or inference phase is invalid");
      }
      if (
        bundle.decision_contract_version !== decisionContractVersionV4 ||
        bundle.scenario_contract_version !== scenarioContractVersionV4
      ) {
        recordFailures.push("bundle contract identity is invalid");
      }
      if (
        bundle.candidate_artifact?.sha256 !==
        manifest.candidates?.[bundle.implementation]?.sha256
      ) {
        recordFailures.push(
          "bundle candidate artifact does not match run manifest",
        );
      }
      const journeyPlan = manifest.plan?.journeys?.find(
        ({ scenario }) => scenario === bundle.journey,
      );
      const scenarioContract = scenarioContracts.get(bundle.journey);
      if (!journeyPlan || !scenarioContract)
        recordFailures.push("bundle journey is not representative");
      if (
        bundle.fixture?.sha256 !== journeyPlan?.fixture_sha256 ||
        bundle.fixture?.sha256 !==
          manifest.fixtures?.[journeyPlan?.fixture_id]?.sha256
      ) {
        recordFailures.push(
          "bundle fixture does not match the locked run fixture",
        );
      }
      if (
        bundle.workload_artifact_sha256 !==
        manifest.workload?.canonical_artifact_sha256
      ) {
        recordFailures.push(
          "bundle workload artifact does not match the run manifest",
        );
      }
      const observedComponents =
        bundle.components?.map(({ component }) => component) ?? [];
      if (
        JSON.stringify(observedComponents) !==
        JSON.stringify(journeyPlan?.component_order ?? [])
      ) {
        recordFailures.push("bundle component order is not exact");
      }
      const componentRecords = [];
      for (const componentEntry of bundle.components ?? []) {
        const expectedIndex = journeyPlan?.component_order?.indexOf(
          componentEntry.component,
        );
        if (
          componentEntry.component_index !== expectedIndex ||
          componentEntry.component_weight !==
            journeyPlan?.component_weights?.[expectedIndex]
        ) {
          recordFailures.push(
            `${componentEntry.component}: component index or weight is not exact`,
          );
        }
        if (
          componentEntry.input_lane !==
          componentInputLaneV4(componentEntry.component)
        ) {
          recordFailures.push(
            `${componentEntry.component}: input lane does not match the v4 runner`,
          );
        }
        const rawPath = localArtifactPath(
          input,
          componentEntry.raw_report_path,
        );
        const rawKey = rawPath ?? `${identity}:${componentEntry.component}`;
        if (rawPaths.has(rawKey))
          recordFailures.push(
            `${componentEntry.component}: raw report path was reused`,
          );
        rawPaths.add(rawKey);
        const loadedReport = rawPath
          ? await loadHashedJson(
              rawPath,
              componentEntry.raw_report_sha256,
              `${entry.bundle_id}:${componentEntry.component} raw report`,
            )
          : {
              passed: false,
              error: `${entry.bundle_id}:${componentEntry.component}: raw path is missing`,
              value: null,
            };
        if (!loadedReport.passed) recordFailures.push(loadedReport.error);
        let assessment = { passed: false, errors: ["raw report did not load"] };
        if (loadedReport.value && scenarioContract) {
          assessment = validateV4ComponentReport({
            report: loadedReport.value,
            implementation: bundle.implementation,
            journey: bundle.journey,
            component: componentEntry.component,
            fixture: { sha256: bundle.fixture?.sha256 },
            scenarioContract,
          });
          recordFailures.push(
            ...assessment.errors.map(
              (error) => `${componentEntry.component}: ${error}`,
            ),
          );
          const compactReceipts = assessment.receipts.map((receipt) => ({
            command_id: receipt.command_id,
            evidence_sha256: receipt.evidence_sha256,
          }));
          if (
            JSON.stringify(componentEntry.command_receipts) !==
            JSON.stringify(compactReceipts)
          ) {
            recordFailures.push(
              `${componentEntry.component}: bundle receipts do not match the raw report`,
            );
          }
        }
        const nativeComponent =
          componentEntry.input_lane === "native-x11-xtest";
        componentRecords.push({
          component: componentEntry.component,
          component_index: componentEntry.component_index,
          component_weight: componentEntry.component_weight,
          input_lane: componentEntry.input_lane,
          raw_report_sha256: componentEntry.raw_report_sha256,
          command_receipts: componentEntry.command_receipts ?? [],
          report_valid: assessment.passed === true,
          measurements: loadedReport.value
            ? extractComponentMeasurementsV4(loadedReport.value, {
                nativeComponent,
              })
            : { values: {}, missing: ["raw report"] },
        });
      }
      bundleRecords.push({
        entry,
        bundle,
        components: componentRecords,
        failures: recordFailures,
      });
      const retainedCommandIds = componentRecords.flatMap(
        ({ command_receipts: receipts }) =>
          receipts.map(({ command_id: commandId }) => commandId),
      );
      if (
        JSON.stringify(bundle.command_ids) !==
          JSON.stringify(retainedCommandIds) ||
        JSON.stringify([...retainedCommandIds].sort()) !==
          JSON.stringify([...(scenarioContract?.command_ids ?? [])].sort())
      ) {
        recordFailures.push(
          "bundle command receipts do not cover the journey exactly once",
        );
      }
    } else {
      bundleRecords.push({
        entry,
        bundle: null,
        components: [],
        failures: recordFailures,
      });
    }
    failures.push(
      ...recordFailures.map(
        (failure) => `${entry.bundle_id ?? identity}: ${failure}`,
      ),
    );
  }
  const expectedBundleCount =
    representativeTimedScenarioIdsV4.length *
    (manifest.settings?.calibration_pairs_per_journey +
      manifest.settings?.final_pairs_per_journey) *
    implementations.length;
  if (
    manifest.bundles?.length !== expectedBundleCount ||
    manifest.expected_bundle_count !== expectedBundleCount ||
    manifest.observed_bundle_count !== expectedBundleCount
  ) {
    failures.push(
      `retained bundle count must be exactly ${expectedBundleCount}`,
    );
  }
  return { input, manifest, bundles: bundleRecords, failures };
}

function bootstrapRatios(ratios, label, bootstrapSamples) {
  if (
    ratios.length === 0 ||
    ratios.some((ratio) => !Number.isFinite(ratio) || ratio <= 0)
  ) {
    return null;
  }
  const seed = Number.parseInt(canonicalSha256V4(label).slice(0, 8), 16);
  if (ratios.every((ratio) => ratio === ratios[0])) {
    return {
      method: "paired percentile bootstrap of the geometric mean ratio",
      estimate: ratios[0],
      samples: bootstrapSamples,
      seed,
      lower_95: ratios[0],
      upper_95: ratios[0],
    };
  }
  return pairedLogRatioBootstrap(ratios, { samples: bootstrapSamples, seed });
}

export function baselineAdjustedGpuRatioV4(metric, electron, gpui) {
  const floor = gpuMeasurementFloors[metric];
  if (!Number.isFinite(floor))
    throw new Error(`unknown baseline-adjusted GPU metric ${metric}`);
  if (![electron, gpui].every((value) => Number.isFinite(value) && value >= 0))
    return null;
  return {
    ratio: Math.max(gpui, floor) / Math.max(electron, floor),
    floor,
    policy: "max(observed, measurement-resolution-floor) before paired ratio",
  };
}

function componentFamilyPair(componentRuns, family) {
  const metricNames = familyMeasurementNames[family];
  const electronValues = componentRuns.electron?.measurements?.values ?? {};
  const gpuiValues = componentRuns.gpui?.measurements?.values ?? {};
  if (family === "product_latency") {
    const sameLatencySource =
      electronValues.product_wall_or_latency_source === "product-latency" &&
      gpuiValues.product_wall_or_latency_source === "product-latency";
    const selected = sameLatencySource
      ? "product_latency_ms"
      : "product_wall_ms";
    const electron = electronValues[selected];
    const gpui = gpuiValues[selected];
    if (![electron, gpui].every((value) => Number.isFinite(value) && value > 0))
      return null;
    return {
      ratio: gpui / electron,
      metrics: { [selected]: { electron, gpui, ratio: gpui / electron } },
      selected_metric: selected,
    };
  }
  const metrics = {};
  const ratios = [];
  for (const metric of metricNames) {
    const electron = electronValues[metric];
    const gpui = gpuiValues[metric];
    const gpuRatio = gpuMeasurementFloors[metric]
      ? baselineAdjustedGpuRatioV4(metric, electron, gpui)
      : null;
    if (
      !gpuRatio &&
      ![electron, gpui].every((value) => Number.isFinite(value) && value > 0)
    ) {
      return null;
    }
    const ratio = gpuRatio?.ratio ?? gpui / electron;
    metrics[metric] = {
      electron,
      gpui,
      ratio,
      ...(gpuRatio
        ? { ratio_floor: gpuRatio.floor, ratio_policy: gpuRatio.policy }
        : {}),
    };
    ratios.push(ratio);
  }
  return { ratio: geometricMean(ratios), metrics, selected_metric: null };
}

function absoluteBudgetFor(family, component, metric) {
  if (family === "process_memory") return 1.5 * 1024 * 1024 * 1024;
  if (
    family === "gpu_resource_pressure" &&
    metric === "baseline_adjusted_gpu_peak_memory_mib"
  ) {
    return 2_048;
  }
  if (family === "native_interaction_and_frame_pacing") {
    return metric === "application_frame_interval_p95_ms" ? 25 : 1000 / 30;
  }
  if (family !== "product_latency") return null;
  if (metric === "product_wall_ms") return null;
  return (
    {
      "open-pdf": 3_000,
      "viewer-layout": 1_000,
      "page-navigation": 1_000,
      "continuous-scroll": 25,
      "cache-pressure": 3_000,
      "close-reopen": 3_000,
      zoom: 1_500,
      "high-zoom-pan": 1_500,
      "annotation-create": 1000 / 30,
      "annotation-transform": 1000 / 30,
      "annotation-properties-history": 1_000,
      "editor-create": 1_000,
      "editor-workload": 1_000,
      "persistence-workload": 3_000,
    }[component] ?? null
  );
}

function summarizeComponentFamily({
  scenario,
  component,
  family,
  pairs,
  bootstrapSamples,
}) {
  const retainedPairs = [];
  const missingMeasurements = [];
  for (const pair of pairs) {
    if (!pair.valid) {
      missingMeasurements.push(
        `${scenario}:pair-${pair.pair}:${component}:invalid-pair`,
      );
      continue;
    }
    const componentRuns = {
      electron: pair.runs.electron?.components?.find(
        (entry) => entry.component === component,
      ),
      gpui: pair.runs.gpui?.components?.find(
        (entry) => entry.component === component,
      ),
    };
    if (!componentRuns.electron || !componentRuns.gpui) {
      missingMeasurements.push(
        `${scenario}:pair-${pair.pair}:${component}:missing-component`,
      );
      continue;
    }
    const result = componentFamilyPair(componentRuns, family);
    if (!result) {
      missingMeasurements.push(
        `${scenario}:pair-${pair.pair}:${component}:${family}`,
      );
      continue;
    }
    retainedPairs.push({ pair: pair.pair, ...result });
  }
  const complete =
    missingMeasurements.length === 0 &&
    retainedPairs.length === pairs.length &&
    pairs.length >= 24 &&
    pairs.length <= 40;
  const ratios = retainedPairs.map(({ ratio }) => ratio);
  const bootstrap = complete
    ? bootstrapRatios(
        ratios,
        `${scenario}:${component}:${family}`,
        bootstrapSamples,
      )
    : null;
  const metricNames = [
    ...new Set(retainedPairs.flatMap(({ metrics }) => Object.keys(metrics))),
  ];
  const measurements = Object.fromEntries(
    metricNames.map((metric) => {
      const electron = retainedPairs
        .map((pair) => pair.metrics[metric]?.electron)
        .filter(Number.isFinite);
      const gpui = retainedPairs
        .map((pair) => pair.metrics[metric]?.gpui)
        .filter(Number.isFinite);
      return [
        metric,
        { electron: summarizeValues(electron), gpui: summarizeValues(gpui) },
      ];
    }),
  );
  const budgetChecks = [];
  for (const [metric, retained] of Object.entries(measurements)) {
    const budget = absoluteBudgetFor(family, component, metric);
    if (!Number.isFinite(budget)) continue;
    for (const implementation of implementations) {
      const maximum = retained[implementation]?.maximum ?? null;
      budgetChecks.push({
        implementation,
        metric,
        maximum,
        budget,
        passed: Number.isFinite(maximum) && maximum <= budget,
      });
    }
  }
  return {
    status: complete ? "complete" : "missing-measurements",
    missing_measurements: missingMeasurements,
    pair_count: retainedPairs.length,
    paired_ratios: ratios,
    paired_ratio: bootstrap,
    measurements,
    absolute_budget: {
      status:
        budgetChecks.length === 0
          ? "not-applicable"
          : budgetChecks.every(({ passed }) => passed)
            ? "passed"
            : "failed",
      passed: budgetChecks.every(({ passed }) => passed),
      checks: budgetChecks,
    },
  };
}

function expectedFamilyComponents(journeyPlan, pairs, family) {
  if (!journeyPlan) return [];
  if (family !== "native_interaction_and_frame_pacing")
    return [...journeyPlan.component_order];
  const native = new Set();
  for (const pair of pairs) {
    for (const implementation of implementations) {
      for (const component of pair.runs[implementation]?.components ?? []) {
        if (component.input_lane === "native-x11-xtest")
          native.add(component.component);
      }
    }
  }
  return journeyPlan.component_order.filter((component) =>
    native.has(component),
  );
}

function summarizeRecovery(component, scenario, pairs, bootstrapSamples) {
  const retained = [];
  for (const pair of pairs) {
    const electron = pair.runs.electron?.components?.find(
      (entry) => entry.component === component,
    )?.measurements?.values?.recovery_released_render_bytes;
    const gpui = pair.runs.gpui?.components?.find(
      (entry) => entry.component === component,
    )?.measurements?.values?.recovery_released_render_bytes;
    if (
      [electron, gpui].every((value) => Number.isFinite(value) && value > 0)
    ) {
      retained.push({
        pair: pair.pair,
        electron,
        gpui,
        electron_over_gpui: electron / gpui,
      });
    }
  }
  return {
    status:
      retained.length === pairs.length ? "complete" : "missing-measurements",
    direction:
      "higher released bytes is better; electron_over_gpui is the loss ratio",
    missing_pair_count: pairs.length - retained.length,
    pairs: retained,
    paired_ratio:
      retained.length === pairs.length
        ? bootstrapRatios(
            retained.map(({ electron_over_gpui: ratio }) => ratio),
            `${scenario}:${component}:recovery`,
            bootstrapSamples,
          )
        : null,
  };
}

function retainedBundle(record) {
  if (!record) return null;
  return {
    bundle_sha256: record.entry.sha256 ?? null,
    candidate_artifact_sha256:
      record.bundle?.candidate_artifact?.sha256 ?? null,
    fixture_sha256: record.bundle?.fixture?.sha256 ?? null,
    pair_position:
      record.bundle?.pair_position ?? record.entry.pair_position ?? null,
    valid: record.failures.length === 0,
    validation_errors: [...record.failures],
    components: record.components,
  };
}

function buildScenarioSummaries(verified, bootstrapSamples) {
  const scenarios = {};
  const finalPairs = verified.manifest?.settings?.final_pairs_per_journey;
  for (const scenario of representativeTimedScenarioIdsV4) {
    const journeyPlan = verified.manifest?.plan?.journeys?.find(
      (journey) => journey.scenario === scenario,
    );
    const pairs = [];
    for (let pair = 1; pair <= finalPairs; pair += 1) {
      const matching = verified.bundles.filter(
        (record) =>
          record.entry.phase === "final" &&
          record.entry.journey === scenario &&
          record.entry.pair === pair,
      );
      const pairErrors = [];
      const runs = {};
      for (const implementation of implementations) {
        const candidates = matching.filter(
          (record) => record.entry.implementation === implementation,
        );
        if (candidates.length !== 1)
          pairErrors.push(
            `${implementation}: expected one bundle, got ${candidates.length}`,
          );
        runs[implementation] = retainedBundle(candidates[0]);
      }
      const expectedFirst =
        verified.manifest?.settings?.final_pair_orders?.[pair - 1]?.[0];
      const retainedPositions = implementations
        .map((implementation) => runs[implementation]?.pair_position)
        .sort();
      if (
        JSON.stringify(retainedPositions) !==
        JSON.stringify(["first", "second"])
      ) {
        pairErrors.push(
          "pair must retain exactly one first and one second run",
        );
      }
      const first = implementations.find(
        (implementation) => runs[implementation]?.pair_position === "first",
      );
      if (first !== expectedFirst)
        pairErrors.push(
          `pair order mismatch; expected ${expectedFirst}, got ${first ?? "missing"}`,
        );
      if (
        implementations.some(
          (implementation) => runs[implementation]?.valid !== true,
        )
      ) {
        pairErrors.push("one or both implementation bundles are invalid");
      }
      pairs.push({
        pair,
        valid: pairErrors.length === 0,
        validation_errors: pairErrors,
        runs,
      });
    }
    const validPairs = pairs.filter(({ valid }) => valid);
    const componentFamilies = {};
    for (const component of journeyPlan?.component_order ?? []) {
      componentFamilies[component] = {};
      for (const family of Object.keys(familyMeasurementNames)) {
        const expected = expectedFamilyComponents(journeyPlan, pairs, family);
        if (!expected.includes(component)) continue;
        componentFamilies[component][family] = summarizeComponentFamily({
          scenario,
          component,
          family,
          pairs,
          bootstrapSamples,
        });
      }
      if (["close-reopen", "cache-pressure-recovery"].includes(component)) {
        componentFamilies[component].recovery = summarizeRecovery(
          component,
          scenario,
          pairs,
          bootstrapSamples,
        );
      }
    }
    const journeyFamilies = {};
    for (const family of Object.keys(familyMeasurementNames)) {
      const expectedComponents = expectedFamilyComponents(
        journeyPlan,
        pairs,
        family,
      );
      const components = expectedComponents.map(
        (component) => componentFamilies[component]?.[family],
      );
      const missingMeasurements = expectedComponents.flatMap(
        (component, index) =>
          components[index]?.status === "complete"
            ? []
            : (components[index]?.missing_measurements ?? [
                `${scenario}:${component}:${family}`,
              ]),
      );
      const aggregateRatios = pairs.map((_, pairIndex) =>
        geometricMean(
          components.map((component) => component?.paired_ratios?.[pairIndex]),
        ),
      );
      const complete =
        expectedComponents.length > 0 &&
        missingMeasurements.length === 0 &&
        aggregateRatios.every((ratio) => Number.isFinite(ratio) && ratio > 0);
      const componentChecks = expectedComponents.map((component, index) => ({
        component,
        upper_95: components[index]?.paired_ratio?.upper_95 ?? null,
        threshold: familyThresholds[family],
        passed:
          Number.isFinite(components[index]?.paired_ratio?.upper_95) &&
          components[index].paired_ratio.upper_95 <= familyThresholds[family],
      }));
      journeyFamilies[family] = {
        status:
          expectedComponents.length === 0
            ? "not-applicable"
            : complete
              ? "complete"
              : "missing-measurements",
        expected_components: expectedComponents,
        missing_measurements: missingMeasurements,
        equal_weight_component_aggregate: complete
          ? bootstrapRatios(
              aggregateRatios,
              `${scenario}:${family}:journey`,
              bootstrapSamples,
            )
          : null,
        component_noninferiority: {
          method: "conjunctive every component",
          compensating_regressions_allowed: false,
          passed: complete && componentChecks.every(({ passed }) => passed),
          checks: componentChecks,
        },
        paired_ratios: complete ? aggregateRatios : [],
      };
    }
    scenarios[scenario] = {
      journey_id: journeyPlan?.journey_id ?? null,
      fixture_id: journeyPlan?.fixture_id ?? null,
      fixture_sha256: journeyPlan?.fixture_sha256 ?? null,
      expected_pair_count: finalPairs,
      valid_pair_count: validPairs.length,
      rejected_pair_count: pairs.length - validPairs.length,
      calibration_pair_count:
        verified.bundles.filter(
          (record) =>
            record.entry.phase === "calibration" &&
            record.entry.journey === scenario,
        ).length / implementations.length,
      calibration_included_in_inference: false,
      component_order: journeyPlan?.component_order ?? [],
      component_weights: journeyPlan?.component_weights ?? [],
      pairs,
      components: componentFamilies,
      metric_families: journeyFamilies,
    };
  }
  return scenarios;
}

function buildReliabilityEvidence(scenarios) {
  return Object.fromEntries(
    implementations.map((implementation) => [
      implementation,
      representativeTimedScenarioIdsV4.flatMap((scenario) =>
        scenarios[scenario].pairs.map((pair) => ({
          journey: scenario,
          pair: pair.pair,
          passed: pair.runs[implementation]?.valid === true,
          bundle_sha256: pair.runs[implementation]?.bundle_sha256 ?? null,
        })),
      ),
    ]),
  );
}

function buildReliabilityAnalysis(retainedAttempts) {
  return Object.fromEntries(
    implementations.map((implementation) => {
      const attempts = retainedAttempts[implementation];
      const failureIndexes = attempts.flatMap((attempt, index) =>
        attempt.passed ? [] : [index],
      );
      return [
        implementation,
        {
          attempts: attempts.length,
          failures: failureIndexes.length,
          attempt_refs: attempts.map(
            (_, index) => `#/retained_attempts/${implementation}/${index}`,
          ),
          failure_refs: failureIndexes.map(
            (index) => `#/retained_attempts/${implementation}/${index}`,
          ),
        },
      ];
    }),
  );
}

function buildJourneyAnalysis(scenarios) {
  return Object.fromEntries(
    representativeTimedScenarioIdsV4.map((scenario) => {
      const retained = scenarios[scenario];
      const recoveryResults = Object.entries(retained.components)
        .filter(([, families]) => families.recovery)
        .map(([component, families]) => ({ component, ...families.recovery }));
      const complete =
        retained.expected_pair_count >= 24 &&
        retained.expected_pair_count <= 40 &&
        retained.valid_pair_count === retained.expected_pair_count &&
        retained.calibration_pair_count === 6 &&
        Object.values(retained.metric_families).every(({ status }) =>
          ["complete", "not-applicable"].includes(status),
        ) &&
        recoveryResults.every(({ status }) => status === "complete");
      return [
        retained.journey_id,
        {
          status: complete ? "complete" : "missing-evidence",
          required_scenarios: [scenario],
          evidence_refs: [`#/scenarios/${scenario}`],
          missing_measurements: Object.entries(retained.metric_families)
            .flatMap(([family, result]) =>
              ["complete", "not-applicable"].includes(result.status)
                ? []
                : result.missing_measurements.map(
                    (item) => `${family}:${item}`,
                  ),
            )
            .concat(
              recoveryResults.flatMap(
                ({
                  component,
                  status,
                  missing_pair_count: missingPairCount,
                }) =>
                  status === "complete"
                    ? []
                    : [
                        `recovery:${component}:${missingPairCount}-missing-pairs`,
                      ],
              ),
            ),
        },
      ];
    }),
  );
}

function buildMetricFamilyAnalysis(scenarios, bootstrapSamples) {
  return Object.fromEntries(
    Object.keys(familyMeasurementNames).map((family) => {
      const applicableScenarios = representativeTimedScenarioIdsV4.filter(
        (scenario) =>
          scenarios[scenario].metric_families[family].status !==
          "not-applicable",
      );
      const journeyResults = applicableScenarios.map(
        (scenario) => scenarios[scenario].metric_families[family],
      );
      const missingMeasurements = applicableScenarios.flatMap(
        (scenario, index) =>
          journeyResults[index].status === "complete"
            ? []
            : journeyResults[index].missing_measurements.map(
                (item) => `${scenario}:${item}`,
              ),
      );
      const pairCount =
        scenarios[representativeTimedScenarioIdsV4[0]].expected_pair_count;
      const aggregateRatios = Array.from(
        { length: pairCount },
        (_, pairIndex) =>
          geometricMean(
            journeyResults.map((journey) => journey.paired_ratios[pairIndex]),
          ),
      );
      const complete =
        applicableScenarios.length > 0 &&
        missingMeasurements.length === 0 &&
        aggregateRatios.every((ratio) => Number.isFinite(ratio) && ratio > 0);
      const aggregate = complete
        ? bootstrapRatios(
            aggregateRatios,
            `${family}:all-journeys`,
            bootstrapSamples,
          )
        : null;
      const componentChecks = applicableScenarios.flatMap((scenario, index) =>
        journeyResults[index].component_noninferiority.checks.map((check) => ({
          scenario,
          ...check,
        })),
      );
      const componentUpper95 = componentChecks
        .map(({ upper_95: value }) => value)
        .filter(Number.isFinite);
      const decisionUpper95 =
        complete && componentUpper95.length === componentChecks.length
          ? Math.max(aggregate.upper_95, ...componentUpper95)
          : null;
      const evidenceRefs = applicableScenarios.flatMap((scenario) =>
        scenarios[scenario].metric_families[family].expected_components.map(
          (component) =>
            `#/scenarios/${scenario}/components/${component}/${family}`,
        ),
      );
      const budgetChecks = representativeTimedScenarioIdsV4.flatMap(
        (scenario) =>
          Object.entries(scenarios[scenario].components).flatMap(
            ([component, families]) =>
              (families[family]?.absolute_budget?.checks ?? []).map(
                (check) => ({
                  scenario,
                  component,
                  ...check,
                  evidence_ref: `#/scenarios/${scenario}/components/${component}/${family}/measurements/${check.metric}`,
                }),
              ),
          ),
      );
      return [
        family,
        {
          status: complete ? "complete" : "missing-measurements",
          missing_measurements: missingMeasurements,
          evidence_refs: complete
            ? evidenceRefs
            : evidenceRefs.filter((ref) => {
                const [, , scenario, , component] = ref.split("/");
                return (
                  scenarios[scenario]?.components?.[component]?.[family]
                    ?.status === "complete"
                );
              }),
          paired_ratio: complete
            ? {
                method:
                  "maximum of the equal-weight journey aggregate and every component upper bound",
                estimate: aggregate.estimate,
                lower_95: aggregate.lower_95,
                upper_95: decisionUpper95,
                aggregate_upper_95: aggregate.upper_95,
                samples: bootstrapSamples,
                seed: aggregate.seed,
              }
            : null,
          equal_weight_journey_aggregate: aggregate,
          component_noninferiority: {
            method: "conjunctive every component; no compensating regressions",
            compensating_regressions_allowed: false,
            threshold: familyThresholds[family],
            passed: complete && componentChecks.every(({ passed }) => passed),
            checks: componentChecks,
          },
          absolute_budget:
            budgetChecks.length === 0
              ? { status: "not-applicable", passed: true, checks: [] }
              : {
                  status: budgetChecks.every(({ passed }) => passed)
                    ? "passed"
                    : "failed",
                  passed: budgetChecks.every(({ passed }) => passed),
                  checks: budgetChecks,
                },
        },
      ];
    }),
  );
}

function candidateArtifactAnalysis(manifest) {
  return Object.fromEntries(
    implementations.map((implementation) => [
      implementation,
      {
        frozen: sha256Pattern.test(
          manifest?.candidates?.[implementation]?.sha256 ?? "",
        ),
        sha256: manifest?.candidates?.[implementation]?.sha256 ?? null,
        evidence_refs: [
          `#/source_manifest/candidates/${implementation}/sha256`,
        ],
      },
    ]),
  );
}

function buildHardEvidenceRefs({
  summary,
  journeys,
  metricFamilies,
  candidateArtifacts,
}) {
  const allJourneysComplete = Object.values(journeys).every(
    ({ status }) => status === "complete",
  );
  const candidateRefs = Object.values(candidateArtifacts).every(
    ({ frozen }) => frozen,
  )
    ? Object.values(candidateArtifacts).flatMap(
        ({ evidence_refs: refs }) => refs,
      )
    : [];
  const fixtureRefs = representativeTimedScenarioIdsV4.map(
    (scenario) => `#/corpora/${scenario}/sha256`,
  );
  const journeyRefs = representativeTimedScenarioIdsV4.map(
    (scenario) => `#/scenarios/${scenario}`,
  );
  const nativeRefs =
    metricFamilies.native_interaction_and_frame_pacing.status === "complete"
      ? metricFamilies.native_interaction_and_frame_pacing.evidence_refs
      : [];
  const resourceFamilies = [
    "sustained_cpu_work",
    "process_memory",
    "gpu_resource_pressure",
  ];
  const recoveryRefs = representativeTimedScenarioIdsV4.flatMap((scenario) =>
    Object.entries(summary.scenarios[scenario].components)
      .filter(([, families]) => families.recovery?.status === "complete")
      .map(
        ([component]) =>
          `#/scenarios/${scenario}/components/${component}/recovery`,
      ),
  );
  const expectedRecoveryCount = representativeTimedScenarioIdsV4.reduce(
    (count, scenario) =>
      count +
      Object.values(summary.scenarios[scenario].components).filter(
        (families) => families.recovery,
      ).length,
    0,
  );
  const resourceRefs =
    resourceFamilies.every(
      (family) => metricFamilies[family].status === "complete",
    ) && recoveryRefs.length === expectedRecoveryCount
      ? [
          ...resourceFamilies.flatMap(
            (family) => metricFamilies[family].evidence_refs,
          ),
          ...recoveryRefs,
        ]
      : [];
  return {
    "candidate-artifacts-frozen": candidateRefs,
    "fixture-bundle-verified":
      summary.validation_errors.length === 0 ? fixtureRefs : [],
    "representative-command-receipts-exact": allJourneysComplete
      ? journeyRefs
      : [],
    "semantic-visual-persistence-oracles-passed": allJourneysComplete
      ? [...journeyRefs, "#/analysis/journeys/persistence-v1"]
      : [],
    "native-application-frame-traces-passed": nativeRefs,
    "resource-observations-complete": resourceRefs,
  };
}

export function summarizeVerifiedV4Run(
  verified,
  { bootstrapSamples = defaultBootstrapSamplesV4 } = {},
) {
  if (!Number.isInteger(bootstrapSamples) || bootstrapSamples < 1) {
    throw new Error("bootstrapSamples must be a positive integer");
  }
  if (!verified.manifest) {
    return {
      protocol_version: "bp-perf-v4",
      scenario_contract_version: scenarioContractVersionV4,
      complete: false,
      decision_ready: false,
      comparison_readiness: { ready: false, failures: verified.failures },
      validation_errors: verified.failures,
      scenarios: {},
      analysis: {
        schema_version: pairedSummaryV4SchemaVersion,
        contract_version: decisionContractVersionV4,
        eligibility: "not-decision-ready",
        complete: false,
        missing_measurements: ["run-manifest-v4.json"],
      },
    };
  }
  const scenarios = buildScenarioSummaries(verified, bootstrapSamples);
  const retainedAttempts = buildReliabilityEvidence(scenarios);
  const reliability = buildReliabilityAnalysis(retainedAttempts);
  const journeys = buildJourneyAnalysis(scenarios);
  const metricFamilies = buildMetricFamilyAnalysis(scenarios, bootstrapSamples);
  const candidateArtifacts = candidateArtifactAnalysis(verified.manifest);
  const corpora = Object.fromEntries(
    representativeTimedScenarioIdsV4.map((scenario) => [
      scenario,
      {
        fixture_id: scenarios[scenario].fixture_id,
        sha256: scenarios[scenario].fixture_sha256,
      },
    ]),
  );
  const summary = {
    protocol_version: "bp-perf-v4",
    scenario_contract_version: scenarioContractVersionV4,
    evidence_level: "rejected-or-incomplete-development-runtime-pairs",
    complete: false,
    decision_ready: false,
    comparison_readiness: { ready: false, failures: [] },
    validation_errors: [...verified.failures],
    excluded_lanes: {
      usgs_large_sheet_stress: "excluded-non-inferential",
      private_hibbeler_935: "blocked-not-transferred",
    },
    source_manifest: {
      schema_version: verified.manifest.schema_version,
      complete: verified.manifest.complete,
      outcome: verified.manifest.outcome,
      candidates: verified.manifest.candidates,
      workload: verified.manifest.workload,
      settings: verified.manifest.settings,
    },
    corpora,
    retained_attempts: retainedAttempts,
    scenarios,
    analysis: {
      schema_version: pairedSummaryV4SchemaVersion,
      contract_version: decisionContractVersionV4,
      eligibility: "not-decision-ready",
      complete: false,
      bootstrap: {
        method: "paired percentile bootstrap of the geometric mean ratio",
        samples: bootstrapSamples,
        confidence_level: 0.95,
      },
      calibration: { pairs_per_journey: 6, included_in_inference: false },
      reliability,
      candidate_artifacts: candidateArtifacts,
      journeys,
      metric_families: metricFamilies,
      hard_evidence_refs: {},
      missing_measurements: Object.entries(metricFamilies).flatMap(
        ([family, retained]) =>
          retained.missing_measurements.map(
            (measurement) => `${family}:${measurement}`,
          ),
      ),
      explicit_missing_measurements: representativeTimedScenarioIdsV4.flatMap(
        (scenario) =>
          scenarios[scenario].pairs.flatMap((pair) =>
            implementations.flatMap((implementation) =>
              (pair.runs[implementation]?.components ?? []).flatMap(
                (component) =>
                  component.measurements.missing.map(
                    (measurement) =>
                      `${scenario}:pair-${pair.pair}:${implementation}:${component.component}:${measurement}`,
                  ),
              ),
            ),
          ),
      ),
    },
    limitations: [
      "Calibration bundles are hash-verified but excluded from every statistical estimate.",
      "USGS is a separate stress lane and is excluded from representative inference.",
      "Hibbeler is blocked-not-transferred and is not evidence for this result.",
      "Whole-device GPU values are accepted only from a baseline-adjusted report field.",
      "Development-runtime evidence does not qualify a packaged candidate or public release.",
    ],
  };
  summary.analysis.hard_evidence_refs = buildHardEvidenceRefs({
    summary,
    journeys,
    metricFamilies,
    candidateArtifacts,
  });
  const samplingValid =
    verified.manifest.settings?.calibration_pairs_per_journey === 6 &&
    verified.manifest.settings?.calibration_inference_eligible === false &&
    Number.isInteger(verified.manifest.settings?.final_pairs_per_journey) &&
    verified.manifest.settings.final_pairs_per_journey >= 24 &&
    verified.manifest.settings.final_pairs_per_journey <= 40 &&
    verified.manifest.settings.final_pairs_per_journey % 4 === 0;
  if (!samplingValid)
    summary.validation_errors.push(
      "sampling is not six excluded calibration pairs and 24-40 final pairs",
    );
  if (bootstrapSamples !== defaultBootstrapSamplesV4) {
    summary.validation_errors.push(
      `decision bootstrap requires ${defaultBootstrapSamplesV4} resamples; got ${bootstrapSamples}`,
    );
  }
  const reliabilityValid = Object.values(reliability).every(
    ({ attempts, failures }) => attempts >= 100 && failures === 0,
  );
  const analysisComplete =
    summary.validation_errors.length === 0 &&
    samplingValid &&
    reliabilityValid &&
    Object.values(journeys).every(({ status }) => status === "complete") &&
    Object.values(metricFamilies).every(
      ({ status }) => status === "complete",
    ) &&
    requiredLiveEvidenceGateIdsV4.every(
      (gate) => summary.analysis.hard_evidence_refs[gate]?.length > 0,
    );
  summary.analysis.complete = analysisComplete;
  summary.analysis.eligibility = analysisComplete
    ? "decision-ready"
    : "not-decision-ready";
  summary.complete = analysisComplete;
  summary.decision_ready = analysisComplete;
  summary.comparison_readiness = {
    ready: analysisComplete,
    failures: analysisComplete
      ? []
      : [
          ...summary.validation_errors,
          ...summary.analysis.missing_measurements,
          ...Object.entries(summary.analysis.hard_evidence_refs)
            .filter(([, refs]) => refs.length === 0)
            .map(([gate]) => `${gate}: retained evidence is incomplete`),
        ],
  };
  summary.evidence_level = analysisComplete
    ? "decision-eligible-development-runtime-pairs"
    : "rejected-or-incomplete-development-runtime-pairs";
  return summary;
}

export async function summarizeV4Run(
  inputDirectory,
  { bootstrapSamples = defaultBootstrapSamplesV4 } = {},
) {
  const verified = await loadVerifiedV4Run(inputDirectory);
  return summarizeVerifiedV4Run(verified, { bootstrapSamples });
}

export function buildDecisionEvidenceV4(summary) {
  return {
    evidence_schema_version: pairedSummaryV4SchemaVersion,
    contract_version: decisionContractVersionV4,
    execution_phase: "final",
    candidate_frozen: Object.values(
      summary?.analysis?.candidate_artifacts ?? {},
    ).every(({ frozen }) => frozen),
    sampling: {
      calibration_pairs: 6,
      calibration_included_in_inference: false,
      planned_final_pairs:
        summary?.source_manifest?.settings?.final_pairs_per_journey ?? null,
      completed_final_pairs: representativeTimedScenarioIdsV4.every(
        (scenario) =>
          summary?.scenarios?.[scenario]?.valid_pair_count ===
          summary?.scenarios?.[scenario]?.expected_pair_count,
      )
        ? (summary?.source_manifest?.settings?.final_pairs_per_journey ?? null)
        : null,
    },
    paired_comparison_summary: summary,
  };
}

export function parseSummarizeV4Arguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "-h" || option === "--help") return { help: true };
    if (!["--input", "--output"].includes(option))
      throw new Error(`unknown option: ${option}`);
    const value = argv[++index];
    if (!value || value.startsWith("--"))
      throw new Error(`${option} requires a value`);
    options[option.slice(2)] = resolve(value);
  }
  if (!options.input) throw new Error("--input is required");
  options.output ??= options.input;
  return options;
}

function usage() {
  return `Usage: node summarize-paired-v4.mjs --input <run-directory> [--output <directory>]

Verifies run-manifest-v4.json, every bundle, and every raw component report.
Writes paired-summary-v4.json and decision-evidence-v4.json. Incomplete evidence
is retained as not-decision-ready and never converted into a technical no.
`;
}

async function main() {
  const options = parseSummarizeV4Arguments(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }
  const summary = await summarizeV4Run(options.input);
  const decisionEvidence = buildDecisionEvidenceV4(summary);
  await mkdir(options.output, { recursive: true });
  await writeFile(
    resolve(options.output, "paired-summary-v4.json"),
    `${JSON.stringify(summary, null, 2)}\n`,
  );
  await writeFile(
    resolve(options.output, "decision-evidence-v4.json"),
    `${JSON.stringify(decisionEvidence, null, 2)}\n`,
  );
  process.stdout.write(
    `${JSON.stringify(
      {
        status: summary.decision_ready
          ? "decision-ready"
          : "not-decision-ready",
        summary: resolve(options.output, "paired-summary-v4.json"),
        evidence: resolve(options.output, "decision-evidence-v4.json"),
        failures: summary.comparison_readiness.failures,
      },
      null,
      2,
    )}\n`,
  );
  if (!summary.decision_ready) process.exitCode = 2;
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main();
}
