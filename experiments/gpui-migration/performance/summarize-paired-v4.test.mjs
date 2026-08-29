import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import { evaluateMigrationDecision } from "./decision-evaluator.mjs";
import { loadMaterializedComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import { componentInputLaneV4 } from "./run-paired-v4.mjs";
import {
  buildScenarioContractV4,
  representativeScenarioDefinitionsV4,
  representativeTimedScenarioIdsV4,
  scenarioContractVersionV4,
} from "./scenario-contract-v4.mjs";
import {
  baselineAdjustedGpuRatioV4,
  buildDecisionEvidenceV4,
  canonicalSha256V4,
  extractComponentMeasurementsV4,
  loadVerifiedV4Run,
  parseSummarizeV4Arguments,
  summarizeV4Run,
  summarizeVerifiedV4Run,
} from "./summarize-paired-v4.mjs";

const digest = "a".repeat(64);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function writeJson(path, value) {
  const bytes = Buffer.from(`${JSON.stringify(value, null, 2)}\n`);
  await writeFile(path, bytes);
  return sha256(bytes);
}

function electronReceipt(scenario, component, command) {
  const receipt = {
    parent_scenario: scenario,
    component_scenario: component,
    command_id: command.id,
    source_command_id: command.id,
    mapping_status: "exact-semantic-map",
    component_execution_passed: true,
    proven_milestones: [...command.expected_milestones],
    missing_milestones: [],
    live: true,
    passed: true,
  };
  const fields = [
    "parent_scenario",
    "component_scenario",
    "command_id",
    "source_command_id",
    "mapping_status",
    "component_execution_passed",
    "proven_milestones",
    "missing_milestones",
  ];
  receipt.evidence_sha256 = canonicalSha256V4(
    Object.fromEntries(fields.map((field) => [field, receipt[field]])),
  );
  return receipt;
}

async function syntheticRunDirectory() {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-summary-v4-"));
  const workload = await loadMaterializedComparisonWorkloadV4();
  const scenario = "small-shell-open";
  const component = "open-pdf";
  const contract = buildScenarioContractV4(workload, scenario);
  const commands = contract.commands.filter(({ id }) =>
    contract.component_command_ids[component].includes(id),
  );
  const receipts = commands.map((command) =>
    electronReceipt(scenario, component, command),
  );
  const rawName =
    "final-small-shell-open-pair1-electron-component1-open-pdf.json";
  const rawPath = resolve(directory, rawName);
  const report = {
    implementation: "electron",
    scenario: component,
    requested_iterations: 1,
    cache_class: "app-cold",
    pdf: {
      sha256: representativeScenarioDefinitionsV4[scenario].fixture_sha256,
    },
    iterations: [{ iteration: 1, success: true, events: [] }],
    summary: {
      successful_iterations: 1,
      failed_iterations: 0,
      wall_duration_ms: { median: 100 },
      process_tree: {
        cpu_seconds: { median: 1 },
        cgroup_memory_peak_bytes: { median: 100_000 },
      },
      application_frame_intervals_ms: { p95: 10 },
      native_input_to_application_frame_ack_ms: { p95: 20 },
      native_application_frame_acknowledgement_proxy: {
        receipt_scope:
          "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout",
        physical_scanout_observed: false,
        sample_count: 1,
      },
      gpu_whole_device_baseline_adjusted: {
        memory_used_mib: { max: 10 },
        utilization_percent: { p95: 15 },
      },
    },
    v4_parent_execution: {
      manifest_id: workload.manifest_id,
      scenario_contract_version: scenarioContractVersionV4,
      parent_scenario: scenario,
      component_scenario: component,
      component_receipts_passed: true,
      command_receipts_by_iteration: [{ iteration: 1, receipts }],
    },
  };
  const rawSha256 = await writeJson(rawPath, report);
  const candidate = { path: "/candidate/electron", bytes: 1, sha256: digest };
  const fixture = {
    path: "/fixture/small.pdf",
    bytes: 1,
    sha256: representativeScenarioDefinitionsV4[scenario].fixture_sha256,
  };
  const bundleId = "final-small-shell-open-pair1-electron";
  const bundleName = `${bundleId}-bundle-manifest.json`;
  const bundlePath = resolve(directory, bundleName);
  const bundle = {
    schema_version: 1,
    protocol_version: "bp-perf-v4",
    decision_contract_version: "bp-perf-v4-decision-1",
    scenario_contract_version: scenarioContractVersionV4,
    workload_artifact_sha256: digest,
    phase: "final",
    inference_eligible: true,
    journey: scenario,
    journey_id: representativeScenarioDefinitionsV4[scenario].journey_id,
    pair: 1,
    pair_position: "first",
    implementation: "electron",
    candidate_artifact: candidate,
    fixture,
    cache_class: "app-cold",
    process_model: "one fresh process per component",
    component_aggregation: {
      order: [component],
      weights: [1],
      benefit_metric_method: "equal-weight geometric mean",
      non_inferiority_method: "conjunctive every component",
      compensating_regressions_allowed: false,
    },
    components: [
      {
        component,
        component_index: 0,
        component_weight: 1,
        input_lane: "native-x11-xtest",
        raw_report_path: `/remote/output/${rawName}`,
        raw_report_sha256: rawSha256,
        command_receipts: receipts.map(
          ({ command_id: commandId, evidence_sha256: evidenceSha256 }) => ({
            command_id: commandId,
            evidence_sha256: evidenceSha256,
          }),
        ),
      },
    ],
    command_ids: receipts.map(({ command_id: commandId }) => commandId),
    passed: true,
  };
  const bundleSha256 = await writeJson(bundlePath, bundle);
  const journeys = representativeTimedScenarioIdsV4.map((scenarioName) => {
    const definition = representativeScenarioDefinitionsV4[scenarioName];
    return {
      scenario: scenarioName,
      journey_id: definition.journey_id,
      fixture_id: definition.fixture_id,
      fixture_sha256: definition.fixture_sha256,
      component_order: [...definition.current_runner_components],
      component_weights: [...definition.component_weights],
      component_command_ids: structuredClone(definition.component_command_ids),
      command_ids: [...definition.command_ids],
      inference_eligible: true,
    };
  });
  const manifest = {
    schema_version: 1,
    plan: {
      protocol_version: "bp-perf-v4",
      decision_contract_version: "bp-perf-v4-decision-1",
      scenario_contract_version: scenarioContractVersionV4,
      manifest_id: workload.manifest_id,
      ready: true,
      blockers: [],
      journeys,
    },
    workload: {
      manifest_id: workload.manifest_id,
      canonical_artifact_sha256: digest,
    },
    candidates: {
      electron: candidate,
      gpui: { path: "/candidate/gpui", bytes: 1, sha256: digest },
    },
    fixtures: Object.fromEntries(
      journeys.map((journey) => [
        journey.fixture_id,
        {
          path: `/fixture/${journey.fixture_id}.pdf`,
          bytes: 1,
          sha256: journey.fixture_sha256,
        },
      ]),
    ),
    settings: {
      calibration_pairs_per_journey: 6,
      calibration_inference_eligible: false,
      final_pairs_per_journey: 24,
      final_pair_orders: Array.from({ length: 24 }, (_, index) =>
        index % 2 === 0 ? ["electron", "gpui"] : ["gpui", "electron"],
      ),
    },
    excluded_lanes: {
      usgs_large_sheet_stress: "non-inferential and not scheduled",
      private_hibbeler_935: "blocked-not-transferred and not scheduled",
    },
    bundles: [
      {
        bundle_id: bundleId,
        phase: "final",
        inference_eligible: true,
        journey: scenario,
        pair: 1,
        pair_position: "first",
        implementation: "electron",
        path: `/remote/output/${bundleName}`,
        sha256: bundleSha256,
        passed: true,
      },
    ],
    complete: true,
    outcome: "passed",
    expected_bundle_count: 300,
    observed_bundle_count: 1,
  };
  const manifestPath = resolve(directory, "run-manifest-v4.json");
  const manifestSha256 = await writeJson(manifestPath, manifest);
  await writeFile(
    resolve(directory, "run-manifest-v4.sha256"),
    `${manifestSha256}  run-manifest-v4.json\n`,
  );
  return { directory, rawPath };
}

function completeSyntheticVerifiedRun() {
  const journeys = representativeTimedScenarioIdsV4.map((scenario) => {
    const definition = representativeScenarioDefinitionsV4[scenario];
    return {
      scenario,
      journey_id: definition.journey_id,
      fixture_id: definition.fixture_id,
      fixture_sha256: definition.fixture_sha256,
      component_order: [...definition.current_runner_components],
      component_weights: [...definition.component_weights],
      component_command_ids: structuredClone(definition.component_command_ids),
      command_ids: [...definition.command_ids],
      inference_eligible: true,
    };
  });
  const orders = Array.from({ length: 24 }, (_, index) =>
    index % 2 === 0 ? ["electron", "gpui"] : ["gpui", "electron"],
  );
  const candidates = {
    electron: { sha256: "e".repeat(64) },
    gpui: { sha256: "f".repeat(64) },
  };
  const fixtures = Object.fromEntries(
    journeys.map((journey) => [
      journey.fixture_id,
      { sha256: journey.fixture_sha256 },
    ]),
  );
  const bundles = [];
  for (const phase of ["calibration", "final"]) {
    const pairCount = phase === "calibration" ? 6 : 24;
    for (const journey of journeys) {
      for (let pair = 1; pair <= pairCount; pair += 1) {
        for (const implementation of ["electron", "gpui"]) {
          const first =
            phase === "final"
              ? orders[pair - 1][0]
              : pair <= 3
                ? "electron"
                : "gpui";
          const components = journey.component_order.map(
            (component, componentIndex) => {
              const native =
                componentInputLaneV4(component) === "native-x11-xtest";
              const gpui = implementation === "gpui";
              const values = {
                cpu_seconds: gpui ? 0.7 : 1,
                cgroup_peak_memory_bytes: gpui ? 700 : 1_000,
                product_wall_ms: 10,
                product_latency_ms: 10,
                product_wall_or_latency_ms: 10,
                product_wall_or_latency_source: "product-latency",
                baseline_adjusted_gpu_peak_memory_mib: 0,
                baseline_adjusted_gpu_utilization_p95_percent: 0,
                ...(native
                  ? {
                      application_frame_interval_p95_ms: 10,
                      native_input_to_application_frame_ack_p95_ms: 20,
                    }
                  : {}),
                ...(["close-reopen", "cache-pressure-recovery"].includes(
                  component,
                )
                  ? { recovery_released_render_bytes: 1_000 }
                  : {}),
              };
              return {
                component,
                component_index: componentIndex,
                component_weight: journey.component_weights[componentIndex],
                input_lane: componentInputLaneV4(component),
                raw_report_sha256: digest,
                command_receipts: [],
                report_valid: true,
                measurements: { values, missing: [] },
              };
            },
          );
          bundles.push({
            entry: {
              phase,
              journey: journey.scenario,
              pair,
              implementation,
              sha256: digest,
            },
            bundle: {
              pair_position: implementation === first ? "first" : "second",
              candidate_artifact: candidates[implementation],
              fixture: fixtures[journey.fixture_id],
            },
            components,
            failures: [],
          });
        }
      }
    }
  }
  return {
    manifest: {
      schema_version: 1,
      complete: true,
      outcome: "passed",
      plan: { journeys },
      candidates,
      fixtures,
      workload: { manifest_id: "bp-perf-v4-decision-1" },
      settings: {
        calibration_pairs_per_journey: 6,
        calibration_inference_eligible: false,
        final_pairs_per_journey: 24,
        final_pair_orders: orders,
      },
    },
    bundles,
    failures: [],
  };
}

test("extracts process, product, native, baseline-adjusted GPU, and recovery measurements", () => {
  const report = {
    implementation: "gpui",
    iterations: [
      {
        events: [
          { event: "comparison-memory-recovery", released_render_bytes: 4096 },
        ],
      },
    ],
    summary: {
      wall_duration_ms: { median: 100 },
      product_latency_ms: { p95: 20 },
      process_tree: {
        cpu_seconds: { median: 1.5 },
        cgroup_memory_peak_bytes: { median: 2048 },
      },
      application_frame_intervals_ms: { p95: 12 },
      native_input_to_application_frame_ack_ms: { p95: 22 },
      native_application_frame_acknowledgement_proxy: {
        receipt_scope:
          "gpui-input-latency-histogram-to-platform-draw-submission-not-physical-scanout",
        physical_scanout_observed: false,
        sample_count: 1,
      },
      gpu_whole_device_baseline_adjusted: {
        memory_used_mib: { max: 30 },
        utilization_percent: { p95: 40 },
      },
    },
  };
  const result = extractComponentMeasurementsV4(report, {
    nativeComponent: true,
  });
  assert.equal(result.values.cpu_seconds, 1.5);
  assert.equal(result.values.product_wall_or_latency_source, "product-latency");
  assert.equal(result.values.native_input_to_application_frame_ack_p95_ms, 22);
  assert.equal(result.values.baseline_adjusted_gpu_peak_memory_mib, 30);
  assert.equal(result.values.recovery_released_render_bytes, 4096);
  assert.deepEqual(result.missing, []);
});

test("does not treat app-open visibility as continuous-scroll latency", () => {
  const report = {
    implementation: "electron",
    scenario: "continuous-scroll",
    iterations: [
      {
        events: [
          { event: "first-page-visible", duration_ms: 2_500 },
          { event: "operation-visible", duration_ms: 12 },
        ],
      },
    ],
    summary: {
      wall_duration_ms: { median: 4_000 },
      process_tree: {
        cpu_seconds: { median: 1 },
        cgroup_memory_peak_bytes: { median: 1 },
      },
      gpu_whole_device_baseline_adjusted: {
        memory_used_mib: { max: 0 },
        utilization_percent: { p95: 0 },
      },
    },
  };
  const result = extractComponentMeasurementsV4(report);
  assert.equal(result.values.product_latency_ms, 12);
  assert.equal(result.values.product_wall_or_latency_ms, 12);
  report.iterations[0].events.pop();
  const wallFallback = extractComponentMeasurementsV4(report);
  assert.equal(
    wallFallback.values.product_wall_or_latency_source,
    "component-wall",
  );
  assert.equal(wallFallback.values.product_wall_or_latency_ms, 4_000);
});

test("retains zero adjusted GPU observations with declared resolution floors", () => {
  assert.deepEqual(
    baselineAdjustedGpuRatioV4(
      "baseline_adjusted_gpu_utilization_p95_percent",
      0,
      0,
    ),
    {
      ratio: 1,
      floor: 0.1,
      policy: "max(observed, measurement-resolution-floor) before paired ratio",
    },
  );
  assert.equal(
    baselineAdjustedGpuRatioV4("baseline_adjusted_gpu_peak_memory_mib", 0, 10)
      .ratio,
    10,
  );
  const extracted = extractComponentMeasurementsV4({
    summary: {
      gpu_whole_device_baseline_adjusted: {
        memory_used_mib: { max: 0 },
        utilization_percent: { p95: 0 },
      },
    },
  });
  assert.equal(extracted.values.baseline_adjusted_gpu_peak_memory_mib, 0);
  assert.equal(
    extracted.values.baseline_adjusted_gpu_utilization_p95_percent,
    0,
  );
});

test("does not synthesize native application-frame timing from generic frame evidence", () => {
  const result = extractComponentMeasurementsV4(
    {
      summary: {
        frame_intervals_ms: { p95: 10 },
        wall_duration_ms: { median: 100 },
        process_tree: {
          cpu_seconds: { median: 1 },
          cgroup_memory_peak_bytes: { median: 1 },
        },
        gpu_whole_device_baseline_adjusted: {
          memory_used_mib: { max: 0 },
          utilization_percent: { p95: 0 },
        },
      },
    },
    { nativeComponent: true },
  );
  assert(result.missing.includes("application_frame_interval_p95_ms"));
  assert(
    result.missing.includes("native_input_to_application_frame_ack_p95_ms"),
  );
  assert.equal(result.values.application_frame_interval_p95_ms, undefined);
});

test("produces evaluator-compatible decision-ready analysis from 24 complete synthetic pairs", () => {
  const summary = summarizeVerifiedV4Run(completeSyntheticVerifiedRun());
  assert.equal(
    summary.complete,
    true,
    JSON.stringify(summary.comparison_readiness.failures),
  );
  assert.equal(summary.decision_ready, true);
  assert.equal(summary.analysis.bootstrap.samples, 100_000);
  assert.equal(summary.analysis.reliability.electron.attempts, 120);
  assert.equal(summary.analysis.reliability.gpui.failures, 0);
  assert.equal(
    summary.analysis.metric_families.sustained_cpu_work.paired_ratio.upper_95,
    0.7,
  );
  assert.equal(
    summary.analysis.metric_families.gpu_resource_pressure.paired_ratio
      .upper_95,
    1,
  );
  assert.equal(
    summary.analysis.metric_families.sustained_cpu_work.component_noninferiority
      .compensating_regressions_allowed,
    false,
  );
  for (const refs of Object.values(summary.analysis.hard_evidence_refs)) {
    assert(refs.length > 0);
  }
  const decision = evaluateMigrationDecision(buildDecisionEvidenceV4(summary));
  assert.equal(decision.eligibility, "decision-ready");
  assert.equal(decision.decision, "yes");
});

test("conjunctive component non-inferiority prevents compensating CPU gains", () => {
  const verified = completeSyntheticVerifiedRun();
  for (const record of verified.bundles.filter(
    ({ entry }) => entry.phase === "final" && entry.implementation === "gpui",
  )) {
    for (const component of record.components) {
      component.measurements.values.cpu_seconds = 0.5;
    }
    if (record.entry.journey === "small-shell-open") {
      record.components[0].measurements.values.cpu_seconds = 1.2;
    }
  }
  const summary = summarizeVerifiedV4Run(verified);
  const family = summary.analysis.metric_families.sustained_cpu_work;
  assert.equal(summary.decision_ready, true);
  assert(family.equal_weight_journey_aggregate.upper_95 < 0.8);
  assert.equal(family.paired_ratio.upper_95, 1.2);
  assert.equal(family.component_noninferiority.passed, false);
  const decision = evaluateMigrationDecision(buildDecisionEvidenceV4(summary));
  assert.equal(decision.decision, "no");
});

test("fails closed with exact blockers when native timing is not instrumented", () => {
  const verified = completeSyntheticVerifiedRun();
  const component = verified.bundles
    .find(({ entry }) => entry.phase === "final")
    .components.find(
      ({ input_lane: inputLane }) => inputLane === "native-x11-xtest",
    );
  delete component.measurements.values.application_frame_interval_p95_ms;
  delete component.measurements.values
    .native_input_to_application_frame_ack_p95_ms;
  component.measurements.missing.push(
    "application_frame_interval_p95_ms",
    "native_input_to_application_frame_ack_p95_ms",
  );
  const summary = summarizeVerifiedV4Run(verified);
  assert.equal(summary.decision_ready, false);
  assert.equal(
    summary.analysis.metric_families.native_interaction_and_frame_pacing.status,
    "missing-measurements",
  );
  assert.deepEqual(
    summary.analysis.hard_evidence_refs[
      "native-application-frame-traces-passed"
    ],
    [],
  );
  assert(
    summary.analysis.explicit_missing_measurements.some((missing) =>
      missing.endsWith(":application_frame_interval_p95_ms"),
    ),
  );
  assert(
    summary.analysis.explicit_missing_measurements.some((missing) =>
      missing.endsWith(":native_input_to_application_frame_ack_p95_ms"),
    ),
  );
});

test("fails closed when only unadjusted whole-device GPU data exists", () => {
  const verified = completeSyntheticVerifiedRun();
  const component = verified.bundles.find(
    ({ entry }) => entry.phase === "final",
  ).components[0];
  delete component.measurements.values.baseline_adjusted_gpu_peak_memory_mib;
  delete component.measurements.values
    .baseline_adjusted_gpu_utilization_p95_percent;
  component.measurements.missing.push(
    "baseline_adjusted_gpu_peak_memory_mib",
    "baseline_adjusted_gpu_utilization_p95_percent",
  );
  const summary = summarizeVerifiedV4Run(verified);
  assert.equal(summary.decision_ready, false);
  assert.equal(
    summary.analysis.metric_families.gpu_resource_pressure.status,
    "missing-measurements",
  );
  assert.deepEqual(
    summary.analysis.hard_evidence_refs["resource-observations-complete"],
    [],
  );
});

test("hash-verifies retained bundle and raw report while excluding incomplete evidence", async (context) => {
  const fixture = await syntheticRunDirectory();
  context.after(() => rm(fixture.directory, { recursive: true, force: true }));
  const verified = await loadVerifiedV4Run(fixture.directory);
  assert.equal(verified.bundles.length, 1);
  assert.equal(verified.bundles[0].components[0].report_valid, true);
  assert(
    verified.failures.some((failure) =>
      failure.includes("retained bundle count"),
    ),
  );

  const summary = await summarizeV4Run(fixture.directory, {
    bootstrapSamples: 10,
  });
  assert.equal(summary.decision_ready, false);
  assert.equal(summary.scenarios["small-shell-open"].expected_pair_count, 24);
  assert.equal(summary.scenarios["small-shell-open"].valid_pair_count, 0);
  assert.equal(
    summary.scenarios["small-shell-open"].calibration_included_in_inference,
    false,
  );
  assert.equal(
    summary.excluded_lanes.usgs_large_sheet_stress,
    "excluded-non-inferential",
  );
  assert.equal(
    summary.excluded_lanes.private_hibbeler_935,
    "blocked-not-transferred",
  );
  const decision = evaluateMigrationDecision(buildDecisionEvidenceV4(summary));
  assert.equal(decision.decision, "not-decision-ready");
});

test("fails closed when a raw component report no longer matches its retained hash", async (context) => {
  const fixture = await syntheticRunDirectory();
  context.after(() => rm(fixture.directory, { recursive: true, force: true }));
  const report = JSON.parse(await readFile(fixture.rawPath, "utf8"));
  report.summary.wall_duration_ms.median = 999;
  await writeFile(fixture.rawPath, `${JSON.stringify(report, null, 2)}\n`);
  const verified = await loadVerifiedV4Run(fixture.directory);
  assert(
    verified.failures.some((failure) =>
      failure.includes("raw report: SHA-256 mismatch"),
    ),
  );
});

test("parses only explicit v4 summarizer paths", () => {
  const parsed = parseSummarizeV4Arguments([
    "--input",
    ".",
    "--output",
    "./test-results",
  ]);
  assert.equal(parsed.input, resolve("."));
  assert.equal(parsed.output, resolve("./test-results"));
  assert.throws(
    () => parseSummarizeV4Arguments(["--input", ".", "--samples", "1"]),
    /unknown option/,
  );
});
