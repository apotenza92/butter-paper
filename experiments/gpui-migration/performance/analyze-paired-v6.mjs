#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { pairedLogRatioBootstrap } from "./decision-statistics.mjs";
import { decisionContractV4 } from "./decision-contract-v4.mjs";
import { gpuiMultiDocumentAbsoluteSafetyBudgetsV5 } from "./decision-contract-v5.mjs";
import { analyzeDynamicFidelityPairsV5 } from "./summarize-paired-v5.mjs";
import {
  buildV6ComparisonPlan,
  buildV6ExecutionSchedule,
  expectedWorkloadByteSha256V6,
  loadComparisonWorkloadV6,
  propertyCorrectnessLaunchCountV6,
  semanticCorrectnessLaunchCountV6,
  totalLaunchCountV6,
} from "./run-paired-v6.mjs";
import {
  protocolVersionV6,
  representativeScenarioDefinitionsV6,
  scenarioContractVersionV6,
} from "./scenario-contract-v6.mjs";
import { electronEngineeringZoomBaselineDefectIdV6 } from "./electron-v6-baseline-defect.mjs";

export const defaultBootstrapSamplesV6 = 100_000;

const implementations = Object.freeze(["electron", "gpui"]);
const thresholds = Object.freeze({
  ...decisionContractV4.decision.primary_metric_upper_95_thresholds,
});
const metricFamilies = Object.freeze({
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
const gpuFloors = Object.freeze({
  baseline_adjusted_gpu_peak_memory_mib: 1,
  baseline_adjusted_gpu_utilization_p95_percent: 0.1,
});

function finitePositive(value) {
  return Number.isFinite(value) && value > 0;
}

function geometricMean(values) {
  if (values.length === 0 || values.some((value) => !finitePositive(value))) {
    return null;
  }
  return Math.exp(
    values.reduce((sum, value) => sum + Math.log(value), 0) / values.length,
  );
}

function componentFamilyRatio(electron, gpui, family) {
  const electronValues = electron?.measurements ?? {};
  const gpuiValues = gpui?.measurements ?? {};
  if (family === "product_latency") {
    const metric =
      electronValues.product_wall_or_latency_source === "product-latency" &&
      gpuiValues.product_wall_or_latency_source === "product-latency"
        ? "product_latency_ms"
        : "product_wall_ms";
    const left = electronValues[metric];
    const right = gpuiValues[metric];
    return finitePositive(left) && finitePositive(right)
      ? {
          ratio: right / left,
          metrics: { [metric]: { electron: left, gpui: right } },
        }
      : null;
  }
  const ratios = [];
  const metrics = {};
  for (const metric of metricFamilies[family]) {
    const left = electronValues[metric];
    const right = gpuiValues[metric];
    const floor = gpuFloors[metric];
    if (floor) {
      if (
        ![left, right].every((value) => Number.isFinite(value) && value >= 0)
      ) {
        return null;
      }
      const ratio = Math.max(right, floor) / Math.max(left, floor);
      ratios.push(ratio);
      metrics[metric] = { electron: left, gpui: right, ratio, floor };
    } else {
      if (!finitePositive(left) || !finitePositive(right)) return null;
      const ratio = right / left;
      ratios.push(ratio);
      metrics[metric] = { electron: left, gpui: right, ratio };
    }
  }
  return { ratio: geometricMean(ratios), metrics };
}

function expectedBenefitBundleCount(plan) {
  return (
    plan.journeys.filter(
      ({ benefit_components: components }) => components.length > 0,
    ).length *
    30 *
    2
  );
}

function exactScheduleFailures(manifest, expectedSchedule) {
  const failures = [];
  if (manifest?.launches?.length !== expectedSchedule.length) {
    return [
      `launch count is ${manifest?.launches?.length ?? "missing"}, expected ${expectedSchedule.length}`,
    ];
  }
  for (let index = 0; index < expectedSchedule.length; index += 1) {
    const actual = manifest.launches[index];
    const expected = expectedSchedule[index];
    for (const field of [
      "schedule_index",
      "phase",
      "inference_eligible",
      "benefit_metrics_eligible",
      "journey",
      "pair",
      "pair_position",
      "implementation",
      "component",
      "component_index",
      "input_lane",
      "hard_component",
    ]) {
      if (actual?.[field] !== expected[field]) {
        failures.push(
          `launch ${index} ${field} is not the frozen schedule value`,
        );
      }
    }
    if (actual?.passed !== true) failures.push(`launch ${index} did not pass`);
    if (
      expected.benefit_metrics_eligible === true &&
      actual?.benefit_metrics_eligible !== true
    ) {
      failures.push(
        `launch ${index} is missing decision-eligible benefit evidence`,
      );
    }
  }
  return failures;
}

function exactElectronEngineeringZoomDefectReport(report) {
  return (
    report?.implementation === "electron" &&
    report?.journey === "engineering-sheet" &&
    report?.component === "zoom" &&
    report?.passed === true &&
    report?.correctness_passed === false &&
    report?.known_baseline_defect_id ===
      electronEngineeringZoomBaselineDefectIdV6
  );
}

async function artifactFailures(manifest) {
  const failures = [];
  const paths = new Set();
  for (const entry of [
    ...(manifest.launches ?? []),
    ...(manifest.bundles ?? []),
  ]) {
    for (const [pathField, hashField] of [
      ["raw_report_path", "raw_report_sha256"],
      ["hard_report_path", "hard_report_sha256"],
      ["path", "sha256"],
    ]) {
      const path = entry?.[pathField];
      const expected = entry?.[hashField];
      if (!path) continue;
      if (paths.has(path)) {
        failures.push(`artifact path was reused: ${path}`);
        continue;
      }
      paths.add(path);
      try {
        const bytes = await readFile(path);
        const observed = createHash("sha256").update(bytes).digest("hex");
        if (observed !== expected)
          failures.push(`artifact hash differs: ${path}`);
        if (pathField === "path") {
          const parsed = JSON.parse(bytes);
          const { path: _path, sha256: _sha256, ...payload } = entry;
          if (JSON.stringify(parsed) !== JSON.stringify(payload)) {
            failures.push(
              `retained bundle payload differs from its artifact: ${path}`,
            );
          }
        }
      } catch {
        failures.push(`artifact is missing: ${path}`);
      }
    }
  }
  return failures;
}

function absoluteBudgetChecks(component, measurements, implementation) {
  const checks = [];
  const add = (metric, maximum) => {
    const value = measurements?.[metric];
    checks.push({
      implementation,
      component,
      metric,
      value,
      maximum,
      passed: Number.isFinite(value) && value <= maximum,
    });
  };
  add("cgroup_peak_memory_bytes", 1.5 * 1024 * 1024 * 1024);
  add("application_frame_interval_p95_ms", 25);
  add("native_input_to_application_frame_ack_p95_ms", 1000 / 30);
  add("baseline_adjusted_gpu_peak_memory_mib", 2048);
  add("baseline_adjusted_gpu_utilization_p95_percent", 100);
  if (component === "multi-document-session" && implementation === "gpui") {
    for (const [metric, contract] of Object.entries(
      gpuiMultiDocumentAbsoluteSafetyBudgetsV5.metrics,
    )) {
      add(metric, contract.maximum);
    }
  }
  return checks;
}

export async function analyzeV6Manifest(
  manifest,
  {
    authenticateArtifacts = false,
    bootstrapSamples = defaultBootstrapSamplesV6,
  } = {},
) {
  const { workload, byte_sha256: byteSha256 } =
    await loadComparisonWorkloadV6();
  const plan = buildV6ComparisonPlan(workload, byteSha256);
  const expectedSchedule = buildV6ExecutionSchedule(plan, {
    seed: manifest?.settings?.schedule_seed ?? 0x4250_5636,
  });
  const blockers = [];
  const metricFailures = [];
  if (
    manifest?.protocol_version !== protocolVersionV6 ||
    manifest?.scenario_contract_version !== scenarioContractVersionV6 ||
    manifest?.manifest_id !== workload.manifest_id ||
    manifest?.workload_byte_sha256 !== expectedWorkloadByteSha256V6
  ) {
    blockers.push("run manifest v6 workload or contract identity is not exact");
  }
  if (manifest?.complete !== true || manifest?.outcome !== "passed") {
    blockers.push("run manifest is not a complete passed execution");
  }
  blockers.push(...exactScheduleFailures(manifest, expectedSchedule));
  for (const launch of manifest?.launches ?? []) {
    const binding = launch?.launch_binding_v5;
    if (
      binding?.schedule_index !== launch.schedule_index ||
      binding?.implementation !== launch.implementation ||
      binding?.component !== launch.component ||
      binding?.candidate_manifest_sha256 !==
        manifest?.candidates?.[launch.implementation]?.sha256
    ) {
      blockers.push(
        `launch ${launch?.schedule_index ?? "unknown"} binding is invalid`,
      );
    }
  }
  if (authenticateArtifacts)
    blockers.push(...(await artifactFailures(manifest)));
  const expectedBundles = expectedBenefitBundleCount(plan);
  if (manifest?.bundles?.length !== expectedBundles) {
    blockers.push(
      `benefit bundle count is ${manifest?.bundles?.length ?? "missing"}, expected ${expectedBundles}`,
    );
  }
  const expectedBundleIds = new Set(
    expectedSchedule
      .map(({ bundle_id: bundleId }) => bundleId)
      .filter((bundleId) => typeof bundleId === "string"),
  );
  const actualBundleIds = (manifest?.bundles ?? []).map(
    ({ bundle_id: bundleId }) => bundleId,
  );
  if (
    new Set(actualBundleIds).size !== expectedBundleIds.size ||
    actualBundleIds.some((bundleId) => !expectedBundleIds.has(bundleId))
  ) {
    blockers.push(
      "benefit bundle identities are duplicated, absent, or outside the frozen schedule",
    );
  }
  const correctnessReports = manifest?.correctness_reports ?? [];
  const knownBaselineDefects = correctnessReports.filter(
    exactElectronEngineeringZoomDefectReport,
  );
  if (
    correctnessReports.length !==
      semanticCorrectnessLaunchCountV6 + propertyCorrectnessLaunchCountV6 ||
    correctnessReports.some(
      (report) =>
        report?.passed !== true ||
        (report?.correctness_passed !== true &&
          !exactElectronEngineeringZoomDefectReport(report)) ||
        (report?.known_baseline_defect_id != null &&
          !exactElectronEngineeringZoomDefectReport(report)),
    )
  ) {
    blockers.push("correctness-only reports are incomplete or failed");
  }
  if (
    manifest?.view_state_pairs?.length !== expectedBundles / 2 ||
    manifest.view_state_pairs.some(({ passed }) => passed !== true)
  ) {
    blockers.push("matched view-state pair evidence is incomplete or failed");
  }

  const finalBundles = (manifest?.bundles ?? []).filter(
    ({ phase, inference_eligible: eligible }) =>
      phase === "final" && eligible === true,
  );
  const componentStatistics = [];
  const absoluteChecks = [];
  for (const [journey, definition] of Object.entries(
    representativeScenarioDefinitionsV6,
  )) {
    for (const component of definition.benefit_components) {
      const pairedComponents = [];
      for (let pair = 1; pair <= 24; pair += 1) {
        const pairBundles = Object.fromEntries(
          implementations.map((implementation) => [
            implementation,
            finalBundles.find(
              (bundle) =>
                bundle.journey === journey &&
                bundle.pair === pair &&
                bundle.implementation === implementation,
            ),
          ]),
        );
        const values = Object.fromEntries(
          implementations.map((implementation) => [
            implementation,
            pairBundles[implementation]?.components?.find(
              (entry) => entry.component === component,
            ),
          ]),
        );
        if (
          !values.electron ||
          !values.gpui ||
          values.electron.benefit_metrics_eligible !== true ||
          values.gpui.benefit_metrics_eligible !== true
        ) {
          blockers.push(
            `${journey}:${component}:pair-${pair} is incomplete or benefit-ineligible`,
          );
          continue;
        }
        pairedComponents.push({ pair, ...values });
        absoluteChecks.push(
          ...absoluteBudgetChecks(
            component,
            values.electron.measurements,
            "electron",
          ),
          ...absoluteBudgetChecks(component, values.gpui.measurements, "gpui"),
        );
      }
      for (const family of Object.keys(metricFamilies)) {
        const ratios = [];
        for (const paired of pairedComponents) {
          const result = componentFamilyRatio(
            paired.electron,
            paired.gpui,
            family,
          );
          if (!result) {
            blockers.push(
              `${journey}:${component}:${family}:pair-${paired.pair} measurements are missing`,
            );
          } else {
            ratios.push(result.ratio);
          }
        }
        const complete = ratios.length === 24;
        const pairedRatio = complete
          ? pairedLogRatioBootstrap(ratios, {
              samples: bootstrapSamples,
              seed:
                0x4250_5636 +
                componentStatistics.length +
                Object.keys(metricFamilies).indexOf(family),
            })
          : null;
        const threshold = thresholds[family];
        const passed = pairedRatio ? pairedRatio.upper_95 <= threshold : false;
        if (pairedRatio && !passed) {
          metricFailures.push(
            `${journey}:${component}:${family} upper 95% GPUI/Electron ratio ${pairedRatio.upper_95} exceeds ${threshold}`,
          );
        }
        componentStatistics.push({
          journey,
          component,
          family,
          pair_count: ratios.length,
          ratio_direction: "gpui/electron; lower-is-better",
          threshold_upper_95: threshold,
          paired_ratio: pairedRatio,
          passed,
        });
      }
    }
  }
  const failedAbsoluteChecks = absoluteChecks.filter(({ passed }) => !passed);
  metricFailures.push(
    ...failedAbsoluteChecks.map(
      ({ implementation, component, metric, value, maximum }) =>
        `${implementation}:${component}:${metric} ${value} exceeds or is missing against ${maximum}`,
    ),
  );

  const dynamicPairs = Array.from({ length: 24 }, (_, index) => {
    const pair = index + 1;
    const components = Object.fromEntries(
      implementations.map((implementation) => {
        const bundle = finalBundles.find(
          (candidate) =>
            candidate.journey === "nasa-long-document" &&
            candidate.pair === pair &&
            candidate.implementation === implementation,
        );
        return [
          implementation,
          bundle?.components?.find(
            ({ component }) => component === "viewer-dynamic-fidelity",
          )?.quality_measurements ?? null,
        ];
      }),
    );
    return { pair, ...components };
  });
  const dynamicFidelity = analyzeDynamicFidelityPairsV5(dynamicPairs, {
    bootstrapSamples,
  });
  if (!dynamicFidelity.executable)
    blockers.push(...dynamicFidelity.structural_failures);
  else if (!dynamicFidelity.decision_ready)
    metricFailures.push(...dynamicFidelity.metric_failures);
  if (bootstrapSamples !== defaultBootstrapSamplesV6) {
    blockers.push(
      `decision bootstrap requires ${defaultBootstrapSamplesV6} resamples`,
    );
  }

  const decision = classifyV6Decision({ blockers, metricFailures });
  return {
    schema_version: 1,
    protocol_version: protocolVersionV6,
    manifest_id: workload.manifest_id,
    workload_byte_sha256: expectedWorkloadByteSha256V6,
    outcome_scope:
      "whether measured Linux GPU-host benefit and maintained correctness justify continuing the Butter Paper GPUI migration; not release qualification",
    authenticated_artifacts: authenticateArtifacts,
    schedule: {
      expected_launches: totalLaunchCountV6,
      correctness_only_launches:
        semanticCorrectnessLaunchCountV6 + propertyCorrectnessLaunchCountV6,
      correctness_excluded_from_benefit_statistics: true,
      final_pairs: 24,
    },
    correctness: {
      known_baseline_defects: knownBaselineDefects.map((report) =>
        structuredClone(report),
      ),
    },
    component_statistics: componentStatistics,
    dynamic_fidelity: dynamicFidelity,
    absolute_safety_checks: absoluteChecks,
    structural_blockers: [...new Set(blockers)],
    metric_failures: [...new Set(metricFailures)],
    decision,
  };
}

export function classifyV6Decision({ blockers, metricFailures }) {
  return blockers.length > 0
    ? "BLOCKED"
    : metricFailures.length > 0
      ? "NO"
      : "YES";
}

async function main() {
  const args = process.argv.slice(2);
  const manifestIndex = args.indexOf("--manifest");
  const outputIndex = args.indexOf("--output");
  if (manifestIndex < 0 || !args[manifestIndex + 1]) {
    throw new Error("--manifest <run-manifest-v6.json> is required");
  }
  const manifest = JSON.parse(
    await readFile(resolve(args[manifestIndex + 1]), "utf8"),
  );
  const analysis = await analyzeV6Manifest(manifest, {
    authenticateArtifacts: true,
  });
  const bytes = `${JSON.stringify(analysis, null, 2)}\n`;
  if (outputIndex >= 0 && args[outputIndex + 1]) {
    await writeFile(resolve(args[outputIndex + 1]), bytes);
  } else {
    process.stdout.write(bytes);
  }
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main();
}
