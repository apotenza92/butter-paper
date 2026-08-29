#!/usr/bin/env node

import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { basename, dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { pairedLogRatioBootstrap } from "./decision-statistics.mjs";
import {
  decisionContractVersionV4,
  requiredLiveEvidenceGateIdsV4,
} from "./decision-contract-v4.mjs";
import {
  assessUntimedPreflightRuns,
  fullTimedRepresentativeScenarios,
} from "./run-paired.mjs";
import {
  lockedFixtureForScenario,
  protocolVersion,
  scenarioContractVersion,
} from "./scenario-contract.mjs";
import {
  protocolVersionV4,
  representativeScenarioDefinitionsV4,
  representativeTimedScenarioIdsV4,
  scenarioContractVersionV4,
} from "./scenario-contract-v4.mjs";

const measuredFilenamePattern = /^(.+)-pair(\d+)-(first|second)-(electron|gpui)\.json$/;

export const pairedEvidenceAnalysisSchemaVersion = 4;

export const fiveJourneyScenarioMap = Object.freeze({
  "small-shell-open-v1": Object.freeze(["small-shell-open"]),
  "nasa-long-document-v1": Object.freeze(["nasa-long-document"]),
  "engineering-sheet-v1": Object.freeze(["engineering-sheet"]),
  "dense-mixed-editing-v1": Object.freeze(["dense-mixed-editing"]),
  "persistence-v1": Object.freeze(["persistence"]),
});

const legacyFamilyMetricRequirements = Object.freeze({
  sustained_cpu_work: Object.freeze({
    scenarios: fullTimedRepresentativeScenarios,
    metrics: Object.freeze(["process_tree_cpu_seconds"]),
  }),
  process_memory: Object.freeze({
    scenarios: fullTimedRepresentativeScenarios,
    metrics: Object.freeze(["cgroup_peak_memory_bytes"]),
  }),
  native_interaction_and_frame_pacing: Object.freeze({
    scenarios: Object.freeze([
      "page-navigation",
      "zoom",
      "high-zoom-pan",
      "continuous-scroll",
      "annotation-create",
      "annotation-transform",
      "editor-create",
    ]),
    metrics: Object.freeze([
      "native_presentation_interval_p95_ms",
      "native_input_to_present_p95_ms",
    ]),
  }),
  product_latency: Object.freeze({
    scenarios: Object.freeze([
      "open-pdf",
      "page-navigation",
      "zoom",
      "high-zoom-pan",
      "continuous-scroll",
      "annotation-create",
      "annotation-transform",
      "editor-create",
      "close-reopen",
    ]),
    metrics: Object.freeze(["product_latency_ms"]),
  }),
  gpu_resource_pressure: Object.freeze({
    scenarios: fullTimedRepresentativeScenarios,
    metrics: Object.freeze([
      "whole_gpu_peak_memory_mib",
      "whole_gpu_utilization_p95_percent",
    ]),
  }),
});

const v4FamilyMetricRequirements = Object.freeze(Object.fromEntries(
  Object.entries(legacyFamilyMetricRequirements).map(([family, requirement]) => [family, {
    ...requirement,
    scenarios: representativeTimedScenarioIdsV4,
  }]),
));

function percentile(values, fraction) {
  const ordered = [...values].sort((left, right) => left - right);
  const position = (ordered.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  return lower === upper
    ? ordered[lower]
    : ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower);
}

function median(values) {
  return percentile(values, 0.5);
}

export function pairedBootstrap(ratios, samples = 10_000, seed = 0x4250_5632) {
  return pairedLogRatioBootstrap(ratios, { samples, seed });
}

function eventValues(report, name, field) {
  return (report.iterations?.[0]?.events ?? [])
    .filter((event) => event.event === name && Number.isFinite(event[field]))
    .map((event) => event[field]);
}

function addFinite(metrics, name, value) {
  if (Number.isFinite(value)) metrics[name] = value;
}

function extractMetrics(report) {
  const metrics = {};
  const visibleEvent = report.implementation === "electron"
    ? "first-page-visible"
    : "viewport-visible";
  addFinite(metrics, "scenario_wall_ms", report.summary?.wall_duration_ms?.median);
  addFinite(metrics, "open_to_visible_ms", eventValues(report, visibleEvent, "duration_ms")[0]);
  addFinite(metrics, "process_to_visible_ms", eventValues(report, visibleEvent, "t_ms")[0]);
  addFinite(metrics, "peak_rss_kb", report.summary?.process_tree?.peak_rss_kb);
  addFinite(metrics, "peak_cpu_percent_diagnostic", report.summary?.process_tree?.peak_cpu_percent);
  addFinite(metrics, "process_tree_cpu_seconds", report.summary?.process_tree?.cpu_seconds?.median);
  addFinite(
    metrics,
    "cgroup_peak_memory_bytes",
    report.summary?.process_tree?.cgroup_memory_peak_bytes?.median,
  );
  addFinite(
    metrics,
    "whole_gpu_peak_memory_mib",
    report.summary?.gpu_whole_device?.memory_used_mib?.max,
  );
  addFinite(
    metrics,
    "whole_gpu_utilization_p95_percent",
    report.summary?.gpu_whole_device?.utilization_percent?.p95,
  );
  addFinite(
    metrics,
    "application_frame_interval_p95_ms_diagnostic",
    report.summary?.frame_intervals_ms?.p95,
  );
  addFinite(
    metrics,
    "native_presentation_interval_p95_ms",
    report.summary?.native_presentation_intervals_ms?.p95,
  );
  addFinite(
    metrics,
    "native_input_to_present_p95_ms",
    report.summary?.native_input_to_present_ms?.p95,
  );

  const recoveryBytes = eventValues(report, "comparison-memory-recovery", "released_render_bytes");
  if (recoveryBytes.length > 0) addFinite(metrics, "released_render_bytes", recoveryBytes[0]);

  const operationEvents = {
    "page-navigation": report.implementation === "electron"
      ? "page-navigation-completed"
      : "operation-visible",
    zoom: report.implementation === "electron" ? "zoom-completed" : "operation-visible",
  };
  const operationEvent = operationEvents[report.scenario];
  if (operationEvent) {
    const durations = eventValues(report, operationEvent, "duration_ms");
    if (durations.length > 0) addFinite(metrics, `${report.scenario}_median_ms`, median(durations));
  }
  const productLatencyCandidates = [
    metrics.open_to_visible_ms,
    metrics[`${report.scenario}_median_ms`],
    report.summary?.product_latency_ms?.p95,
  ].filter(Number.isFinite);
  if (productLatencyCandidates.length > 0) {
    addFinite(metrics, "product_latency_ms", Math.max(...productLatencyCandidates));
  }
  return metrics;
}

function summarizeValues(values) {
  return {
    count: values.length,
    median: median(values),
    p95: percentile(values, 0.95),
    maximum: Math.max(...values),
  };
}

function reportIterationFailures(report) {
  return (report?.iterations ?? [])
    .filter((iteration) => iteration.success === false || iteration.error)
    .map((iteration) => ({
      iteration: iteration.iteration ?? null,
      error: iteration.error ?? "iteration reported failure",
    }));
}

function retainedRun(entry) {
  if (!entry) return null;
  const run = entry.run ?? {};
  return {
    name: entry.name,
    implementation: entry.implementation,
    position: entry.position,
    measured: entry.measured !== false,
    exit_code: run.exit_code ?? null,
    signal: run.signal ?? null,
    spawn_error: run.spawn_error ?? null,
    stderr: run.stderr || null,
    report_loaded: Boolean(entry.report),
    report_error: entry.report_error ?? null,
    successful_iterations: entry.report?.summary?.successful_iterations ?? null,
    failed_iterations: entry.report?.summary?.failed_iterations ?? null,
    iteration_failures: reportIterationFailures(entry.report),
    decision_timing_eligible:
      entry.report?.comparison_workload?.decision_timing_eligible === true,
    candidate_artifact_sha256:
      entry.report?.provenance?.runtime?.binary?.sha256
      ?? entry.report?.provenance?.runtime?.electron?.sha256
      ?? null,
  };
}

function scenarioNames(entries, manifest) {
  const configured = manifest?.settings?.scenarios;
  if (Array.isArray(configured) && configured.length > 0) return [...new Set(configured)];
  return [...new Set(entries
    .filter((entry) => entry.measured !== false && entry.scenario)
    .map((entry) => entry.scenario))];
}

function scenarioPairIds(entries, manifest, scenario) {
  const configuredPairs = manifest?.settings?.pairs;
  const configuredScenarios = manifest?.settings?.scenarios;
  if (
    Number.isInteger(configuredPairs)
    && configuredPairs > 0
    && Array.isArray(configuredScenarios)
    && configuredScenarios.includes(scenario)
  ) {
    return Array.from({ length: configuredPairs }, (_, index) => index + 1);
  }
  return [...new Set(entries
    .filter((entry) => entry.measured !== false && entry.scenario === scenario)
    .map((entry) => entry.pair)
    .filter((pair) => Number.isInteger(pair) && pair > 0))]
    .sort((left, right) => left - right);
}

function expectedFixture(manifest, scenario) {
  const v4 = representativeScenarioDefinitionsV4[scenario];
  return manifest?.scenario_fixtures?.[scenario] ?? (v4
    ? { fixture_id: v4.fixture_id, fixture_sha256: v4.fixture_sha256 }
    : lockedFixtureForScenario(scenario));
}

function lockedScenarioFixture(scenario) {
  const v4 = representativeScenarioDefinitionsV4[scenario];
  return v4
    ? { fixture_id: v4.fixture_id, fixture_sha256: v4.fixture_sha256 }
    : lockedFixtureForScenario(scenario);
}

function sameWorkload(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function addFailure(failures, validationErrors, context, message) {
  validationErrors.push(message);
  failures.push({ ...context, message });
}

function validateReport(entry, fixture, failures, pairErrors) {
  const context = {
    scenario: entry.scenario,
    pair: entry.pair,
    implementation: entry.implementation,
    run: entry.name,
  };
  const reject = (message) => addFailure(failures, pairErrors, context, message);
  const report = entry.report;
  if (!report) {
    reject(`${entry.name}: report missing or invalid${entry.report_error ? `: ${entry.report_error}` : ""}`);
    return;
  }
  if (entry.run?.exit_code !== undefined && entry.run.exit_code !== 0) {
    reject(`${entry.name}: process exited with ${entry.run.exit_code ?? entry.run.signal ?? "spawn failure"}`);
  }
  const v4 = Object.hasOwn(representativeScenarioDefinitionsV4, entry.scenario);
  const expectedProtocol = v4 ? protocolVersionV4 : protocolVersion;
  const expectedContract = v4 ? scenarioContractVersionV4 : scenarioContractVersion;
  if (report.protocol_version !== expectedProtocol) reject(`${entry.name}: protocol mismatch`);
  if (report.scenario_contract_version !== expectedContract) {
    reject(`${entry.name}: contract mismatch; expected ${expectedContract}`);
  }
  if (report.implementation !== entry.implementation || report.scenario !== entry.scenario) {
    reject(`${entry.name}: report identity mismatch`);
  }
  if (report.summary?.successful_iterations !== 1 || report.summary?.failed_iterations !== 0) {
    reject(`${entry.name}: iteration did not pass`);
  }
  if (report.comparison_workload?.decision_timing_eligible !== true) {
    reject(`${entry.name}: decision timing ineligible`);
  }
  if (entry.run?.fixture_id && entry.run.fixture_id !== fixture.fixture_id) {
    reject(`${entry.name}: fixture ID mismatch; expected ${fixture.fixture_id}`);
  }
  if (report.pdf?.sha256 !== fixture.fixture_sha256) {
    reject(`${entry.name}: fixture identity mismatch; expected ${fixture.fixture_sha256}`);
  }

  const densityEvents = (report.iterations?.[0]?.events ?? []).filter((event) =>
    [
      "first-page-visible",
      "page-navigation-completed",
      "zoom-completed",
      "viewport-raster-completed",
      "operation-raster-completed",
    ].includes(event.event));
  for (const event of densityEvents) {
    if (
      Object.hasOwn(event, "rendered_device_pixel_ratio")
      && (!Number.isFinite(event.rendered_device_pixel_ratio)
        || event.rendered_device_pixel_ratio < 0.75)
    ) {
      reject(`${entry.name}: ${event.event} has unacceptable pixel density`);
    }
  }
}

function manifestExecutionFailures(manifest) {
  return (manifest?.runs ?? []).filter((run) =>
    (Object.hasOwn(run, "exit_code") && run.exit_code !== 0)
    || run.spawn_error
    || run.report?.report_missing_or_invalid
    || run.report?.failed_iterations > 0)
    .map((run) => ({
      scenario: run.scenario ?? null,
      pair: null,
      implementation: run.implementation ?? null,
      run: run.name ?? null,
      message: `${run.name ?? "manifest run"}: runner failure retained from run manifest`,
      execution: {
        measured: run.measured,
        exit_code: run.exit_code ?? null,
        signal: run.signal ?? null,
        spawn_error: run.spawn_error ?? null,
        stderr: run.stderr || null,
        report: run.report ?? null,
      },
    }));
}

function comparisonReadiness(manifest) {
  if (!Number.isInteger(manifest?.schema_version)
    || manifest.schema_version < 2
    || !manifest.comparison_plan) {
    return {
      enforced: false,
      ready: false,
      failures: ["full paired decision comparison plan is absent"],
    };
  }
  const failures = [];
  const configuredScenarios = manifest.settings?.scenarios ?? [];
  const v4 = representativeTimedScenarioIdsV4.some((scenario) =>
    configuredScenarios.includes(scenario));
  const requiredScenarios = v4
    ? representativeTimedScenarioIdsV4
    : fullTimedRepresentativeScenarios;
  for (const scenario of requiredScenarios) {
    if (!configuredScenarios.includes(scenario)) {
      failures.push(`required timed scenario ${scenario} is missing`);
    }
  }
  const plan = manifest.comparison_plan;
  if (plan.ready !== true || (plan.blockers?.length ?? 0) > 0) {
    failures.push("paired comparison plan retained blockers");
  }
  for (const implementation of ["electron", "gpui"]) {
    const coverage = plan.global_command_coverage?.find((candidate) =>
      candidate.implementation === implementation);
    if (coverage?.ready !== true) {
      failures.push(`${implementation} global command coverage is incomplete`);
    }
  }
  if (!v4) failures.push(...assessUntimedPreflightRuns(plan, manifest.runs ?? []).failures);
  const pairOrders = manifest.settings?.pair_orders;
  if (
    !Array.isArray(pairOrders)
    || pairOrders.length !== manifest.settings?.pairs
    || pairOrders.some((order) =>
      !Array.isArray(order)
      || order.length !== 2
      || new Set(order).size !== 2
      || !order.includes("electron")
      || !order.includes("gpui"))
  ) {
    failures.push("recorded pair orders are missing or invalid");
  }
  return { enforced: true, ready: failures.length === 0, failures };
}

function implementationReliability(scenarios, implementation) {
  let attempts = 0;
  let successes = 0;
  const failureRefs = [];
  const attemptRefs = [];
  for (const [scenarioName, scenario] of Object.entries(scenarios)) {
    if (scenario.fixture_id === "usgs-usa-geology-sheet-v1") continue;
    attempts += scenario.expected_pair_count;
    for (let index = 0; index < scenario.pairs.length; index += 1) {
      const run = scenario.pairs[index].runs[implementation];
      const ref = `#/scenarios/${scenarioName}/pairs/${index}/runs/${implementation}`;
      attemptRefs.push(ref);
      if (
        (run?.exit_code === 0 || run?.exit_code === null)
        && run.report_loaded === true
        && run.successful_iterations === 1
        && run.failed_iterations === 0
      ) {
        successes += 1;
      } else {
        failureRefs.push(ref);
      }
    }
    for (let missing = scenario.pairs.length; missing < scenario.expected_pair_count; missing += 1) {
      failureRefs.push(`#/scenarios/${scenarioName}/pairs/missing-${missing + 1}/runs/${implementation}`);
    }
  }
  return {
    attempts,
    successes,
    failures: attempts - successes,
    attempt_refs: attemptRefs,
    failure_refs: failureRefs,
  };
}

function latencyBudgetMs(scenario, metric) {
  if (metric === "native_presentation_interval_p95_ms") return 25;
  if (metric === "native_input_to_present_p95_ms") return 1000 / 30;
  if (metric !== "product_latency_ms") return null;
  return {
    "open-pdf": 3_000,
    "page-navigation": 1_000,
    zoom: 1_500,
    "high-zoom-pan": 1_500,
    "continuous-scroll": 25,
    "annotation-create": 1000 / 30,
    "annotation-transform": 1000 / 30,
    "editor-create": 1_000,
    "close-reopen": 3_000,
  }[scenario] ?? null;
}

function absoluteBudget(family, scenario, metric, value) {
  if (family === "process_memory") return 1.5 * 1024 * 1024 * 1024;
  if (family === "gpu_resource_pressure" && metric === "whole_gpu_peak_memory_mib") return 2_048;
  return latencyBudgetMs(scenario, metric);
}

function deriveMetricFamilies(scenarios) {
  const v4 = representativeTimedScenarioIdsV4.some((scenario) => scenarios[scenario]);
  const requirements = v4 ? v4FamilyMetricRequirements : legacyFamilyMetricRequirements;
  return Object.fromEntries(Object.entries(requirements).map(([family, requirement]) => {
    const missing = [];
    const evidenceRefs = [];
    const ratios = [];
    const budgetChecks = [];
    for (const scenarioName of requirement.scenarios) {
      const scenario = scenarios[scenarioName];
      if (!scenario) {
        for (const metric of requirement.metrics) missing.push(`${scenarioName}:${metric}`);
        continue;
      }
      if (scenario.fixture_id === "usgs-usa-geology-sheet-v1") {
        for (const metric of requirement.metrics) {
          missing.push(`${scenarioName}:${metric}:non-inferential-usgs-stress-lane`);
        }
        continue;
      }
      for (const metric of requirement.metrics) {
        const result = scenario.metrics[metric];
        if (!result || !Array.isArray(result.gpui_over_electron?.paired_ratios)) {
          missing.push(`${scenarioName}:${metric}`);
          continue;
        }
        evidenceRefs.push(`#/scenarios/${scenarioName}/metrics/${metric}`);
        ratios.push(...result.gpui_over_electron.paired_ratios);
        const budget = absoluteBudget(family, scenarioName, metric);
        if (Number.isFinite(budget)) {
          for (const implementation of ["electron", "gpui"]) {
            const maximum = result[implementation]?.maximum;
            budgetChecks.push({
              scenario: scenarioName,
              metric,
              implementation,
              maximum: Number.isFinite(maximum) ? maximum : null,
              budget,
              passed: Number.isFinite(maximum) && maximum <= budget,
              evidence_ref: `#/scenarios/${scenarioName}/metrics/${metric}/${implementation}`,
            });
          }
        }
      }
    }
    const complete = missing.length === 0 && ratios.length > 0;
    return [family, {
      status: complete ? "complete" : "missing-measurements",
      missing,
      evidence_refs: evidenceRefs,
      paired_ratio: complete ? pairedLogRatioBootstrap(ratios) : null,
      absolute_budget: budgetChecks.length === 0
        ? { status: "not-applicable", passed: true, checks: [] }
        : {
            status: budgetChecks.every(({ passed }) => passed) ? "passed" : "failed",
            passed: budgetChecks.every(({ passed }) => passed),
            checks: budgetChecks,
          },
    }];
  }));
}

function deriveJourneys(scenarios) {
  return Object.fromEntries(Object.entries(fiveJourneyScenarioMap).map(([journey, required]) => {
    const missing = required.filter((scenarioName) => {
      const scenario = scenarios[scenarioName];
      return !scenario
        || scenario.valid_pair_count !== scenario.expected_pair_count
        || scenario.expected_pair_count === 0;
    });
    return [journey, {
      status: missing.length === 0 ? "complete" : "missing-evidence",
      required_scenarios: required,
      missing_scenarios: missing,
      evidence_refs: required
        .filter((scenarioName) => scenarios[scenarioName])
        .map((scenarioName) => `#/scenarios/${scenarioName}`),
      metrics: Object.fromEntries(required
        .filter((scenarioName) => scenarios[scenarioName])
        .map((scenarioName) => [scenarioName, Object.fromEntries(
          Object.entries(scenarios[scenarioName].metrics).map(([metric, result]) => [metric, {
            paired_ratio: result.gpui_over_electron,
            electron: result.electron,
            gpui: result.gpui,
            evidence_ref: `#/scenarios/${scenarioName}/metrics/${metric}`,
          }]),
        )])),
    }];
  }));
}

function deriveCandidateArtifacts(scenarios) {
  return Object.fromEntries(["electron", "gpui"].map((implementation) => {
    const hashes = [];
    const evidenceRefs = [];
    let attempts = 0;
    for (const [scenarioName, scenario] of Object.entries(scenarios)) {
      attempts += scenario.expected_pair_count;
      for (let pairIndex = 0; pairIndex < scenario.pairs.length; pairIndex += 1) {
        const hash = scenario.pairs[pairIndex].runs[implementation]?.candidate_artifact_sha256;
        if (!/^[0-9a-f]{64}$/i.test(hash ?? "")) continue;
        hashes.push(hash);
        evidenceRefs.push(
          `#/scenarios/${scenarioName}/pairs/${pairIndex}/runs/${implementation}`
          + "/candidate_artifact_sha256",
        );
      }
    }
    const uniqueHashes = [...new Set(hashes)];
    return [implementation, {
      frozen: attempts > 0 && evidenceRefs.length === attempts && uniqueHashes.length === 1,
      sha256: uniqueHashes.length === 1 ? uniqueHashes[0] : null,
      evidence_refs: evidenceRefs,
    }];
  }));
}

function deriveEvidenceAnalysis(scenarios, readiness, decisionReady, corpora) {
  const reliability = Object.fromEntries(
    ["electron", "gpui"].map((implementation) => [
      implementation,
      implementationReliability(scenarios, implementation),
    ]),
  );
  const journeys = deriveJourneys(scenarios);
  const metricFamilies = deriveMetricFamilies(scenarios);
  const candidateArtifacts = deriveCandidateArtifacts(scenarios);
  const candidateArtifactRefs = Object.values(candidateArtifacts)
    .every(({ frozen }) => frozen)
    ? Object.values(candidateArtifacts).flatMap(({ evidence_refs: refs }) => refs)
    : [];
  const fixtureRefs = Object.keys(corpora).map((scenario) => `#/corpora/${scenario}/sha256`);
  const journeyRefs = Object.values(journeys).flatMap(({ evidence_refs: refs }) => refs);
  const resourceRefs = ["sustained_cpu_work", "process_memory", "gpu_resource_pressure"]
    .flatMap((family) => metricFamilies[family]?.evidence_refs ?? []);
  const hardEvidenceRefs = {
    "candidate-artifacts-frozen": candidateArtifactRefs,
    "fixture-bundle-verified": fixtureRefs,
    "representative-command-receipts-exact": journeyRefs,
    "semantic-visual-persistence-oracles-passed": [
      "#/comparison_readiness",
      "#/analysis/journeys/persistence-v1",
    ],
    "native-presentation-traces-passed":
      metricFamilies.native_interaction_and_frame_pacing.evidence_refs,
    "resource-observations-complete": resourceRefs,
  };
  const complete = decisionReady
    && Object.values(journeys).every(({ status }) => status === "complete")
    && Object.values(metricFamilies).every(({ status }) => status === "complete")
    && requiredLiveEvidenceGateIdsV4.every((gate) => hardEvidenceRefs[gate]?.length > 0);
  return {
    schema_version: pairedEvidenceAnalysisSchemaVersion,
    contract_version: decisionContractVersionV4,
    eligibility: complete ? "decision-ready" : "not-decision-ready",
    complete,
    reliability,
    candidate_artifacts: candidateArtifacts,
    journeys,
    metric_families: metricFamilies,
    hard_evidence_refs: hardEvidenceRefs,
  };
}

export function summarizePairs(entries, manifest = null) {
  const failures = manifestExecutionFailures(manifest);
  const validationErrors = failures.map(({ message }) => message);
  const readiness = comparisonReadiness(manifest);
  if (readiness.enforced) {
    for (const message of readiness.failures) {
      addFailure(
        failures,
        validationErrors,
        { scenario: null, pair: null, implementation: null },
        message,
      );
    }
  }
  if (manifest?.complete === false) {
    addFailure(failures, validationErrors, { scenario: null, pair: null, implementation: null },
      "run manifest is incomplete");
  }

  const scenarios = {};
  const corpora = {};
  const requestedScenarios = scenarioNames(entries, manifest);
  if (requestedScenarios.length === 0) {
    addFailure(failures, validationErrors, { scenario: null, pair: null, implementation: null },
      "no measured scenarios were found");
  }
  for (const scenario of requestedScenarios) {
    const fixture = expectedFixture(manifest, scenario);
    const lockedFixture = lockedScenarioFixture(scenario);
    const fixtureValidationError = (
      fixture.fixture_id !== lockedFixture.fixture_id
      || fixture.fixture_sha256 !== lockedFixture.fixture_sha256
    ) ? `${scenario}: run manifest fixture does not match the locked scenario fixture` : null;
    if (fixtureValidationError) {
      addFailure(
        failures,
        validationErrors,
        { scenario, pair: null, implementation: null },
        fixtureValidationError,
      );
    }
    const pairIds = scenarioPairIds(entries, manifest, scenario);
    const pairs = [];
    for (const pair of pairIds) {
      const pairErrors = fixtureValidationError ? [fixtureValidationError] : [];
      const context = { scenario, pair, implementation: null };
      const matching = entries.filter((entry) =>
        entry.measured !== false && entry.scenario === scenario && entry.pair === pair);
      const electronEntries = matching.filter((entry) => entry.implementation === "electron");
      const gpuiEntries = matching.filter((entry) => entry.implementation === "gpui");
      const electron = electronEntries[0];
      const gpui = gpuiEntries[0];
      if (!electron || !gpui) {
        addFailure(failures, pairErrors, context, `${scenario} pair ${pair} is incomplete`);
      }
      if (electronEntries.length > 1 || gpuiEntries.length > 1) {
        addFailure(failures, pairErrors, context, `${scenario} pair ${pair} has duplicate runs`);
      }

      const expectedFirst = manifest?.settings?.pair_orders?.[pair - 1]?.[0]
        ?? (pair % 2 === 1 ? "electron" : "gpui");
      const actualFirst = matching.find((entry) => entry.position === "first")?.implementation;
      if (actualFirst !== expectedFirst) {
        addFailure(
          failures,
          pairErrors,
          context,
          `${scenario} pair ${pair} expected ${expectedFirst} first, got ${actualFirst ?? "none"}`,
        );
      }
      for (const entry of [electron, gpui].filter(Boolean)) {
        validateReport(entry, fixture, failures, pairErrors);
      }
      if (electron?.report && gpui?.report) {
        if (electron.report.pdf?.bytes !== gpui.report.pdf?.bytes) {
          addFailure(failures, pairErrors, context, `${scenario} pair ${pair}: fixture byte count mismatch`);
        }
        if (!sameWorkload(electron.report.workload, gpui.report.workload)) {
          addFailure(failures, pairErrors, context, `${scenario} pair ${pair}: workload mismatch`);
        }
      }

      const valid = pairErrors.length === 0;
      const electronMetrics = valid ? extractMetrics(electron.report) : {};
      const gpuiMetrics = valid ? extractMetrics(gpui.report) : {};
      pairs.push({
        pair,
        order: actualFirst === "electron" ? "electron-gpui"
          : actualFirst === "gpui" ? "gpui-electron" : null,
        valid,
        validation_errors: pairErrors,
        runs: {
          electron: retainedRun(electron),
          gpui: retainedRun(gpui),
        },
        electron: electronMetrics,
        gpui: gpuiMetrics,
      });
    }

    const corpusReport = entries.find((entry) =>
      entry.scenario === scenario && entry.report?.pdf?.sha256 === fixture.fixture_sha256)?.report;
    corpora[scenario] = corpusReport
      ? { bytes: corpusReport.pdf.bytes, sha256: corpusReport.pdf.sha256 }
      : { bytes: null, sha256: fixture.fixture_sha256 };

    const validPairs = pairs.filter((pair) => pair.valid);
    const metricNames = validPairs.length === 0 ? [] : Object.keys(validPairs[0].electron).filter(
      (metric) => validPairs.every((pair) =>
        Number.isFinite(pair.electron[metric])
        && pair.electron[metric] > 0
        && Number.isFinite(pair.gpui[metric])
        && pair.gpui[metric] > 0),
    );
    const metrics = {};
    for (const metric of metricNames) {
      const electronValues = validPairs.map((pair) => pair.electron[metric]);
      const gpuiValues = validPairs.map((pair) => pair.gpui[metric]);
      const ratios = validPairs.map((pair) => pair.gpui[metric] / pair.electron[metric]);
      metrics[metric] = {
        electron: summarizeValues(electronValues),
        gpui: summarizeValues(gpuiValues),
        gpui_over_electron: {
          paired_ratios: ratios,
          ...pairedLogRatioBootstrap(ratios),
        },
      };
    }
    scenarios[scenario] = {
      fixture_id: fixture.fixture_id,
      fixture_sha256: fixture.fixture_sha256,
      expected_pair_count: pairIds.length,
      valid_pair_count: validPairs.length,
      rejected_pair_count: pairs.length - validPairs.length,
      pairs,
      metrics,
    };
  }

  const complete = failures.length === 0;
  const decisionReady = complete && readiness.ready;
  const analysis = deriveEvidenceAnalysis(scenarios, readiness, decisionReady, corpora);
  return {
    protocol_version: protocolVersion,
    scenario_contract_version: scenarioContractVersion,
    evidence_level: decisionReady
      ? "decision-eligible-development-runtime-pairs"
      : "rejected-or-incomplete-development-runtime-pairs",
    complete,
    decision_ready: decisionReady,
    comparison_readiness: readiness,
    validation_errors: validationErrors,
    failures,
    corpora,
    scenarios,
    analysis,
    limitations: [
      "Only reports that explicitly declare decision_timing_eligible=true contribute paired metrics.",
      "Development-runtime evidence does not qualify packaged candidates or public releases.",
      "Linux cgroup-v2 CPU-seconds and peak memory include each application process tree.",
      "NVIDIA samples cover the whole otherwise-idle GPU, including the X server and window manager.",
      "Native presentation timing remains a separate requirement from application frame callbacks.",
    ],
  };
}

function parseMeasuredName(name) {
  const match = measuredFilenamePattern.exec(name);
  if (!match) return null;
  return {
    scenario: match[1],
    pair: Number(match[2]),
    position: match[3],
    implementation: match[4],
  };
}

async function readReport(path) {
  try {
    return { report: JSON.parse(await readFile(path, "utf8")), report_error: null };
  } catch (error) {
    return { report: null, report_error: error.message };
  }
}

export async function loadPairedEntries(input, manifest = null) {
  if (manifest?.runs) {
    const entries = [];
    for (const run of manifest.runs.filter(({ measured }) => measured === true)) {
      const identity = parseMeasuredName(`${run.name}.json`);
      if (!identity) continue;
      const outputName = run.output ? basename(run.output) : `${run.name}.json`;
      const localPath = resolve(input, outputName);
      const loaded = await readReport(localPath);
      entries.push({
        name: run.name,
        measured: true,
        run,
        ...identity,
        ...loaded,
      });
    }
    return entries;
  }

  const entries = [];
  for (const name of await readdir(input)) {
    const identity = parseMeasuredName(name);
    if (!identity) continue;
    entries.push({
      name: name.slice(0, -".json".length),
      measured: true,
      ...identity,
      ...await readReport(resolve(input, name)),
    });
  }
  return entries;
}

async function main() {
  const inputIndex = process.argv.indexOf("--input");
  const outputIndex = process.argv.indexOf("--output");
  if (inputIndex < 0 || outputIndex < 0 || !process.argv[inputIndex + 1] || !process.argv[outputIndex + 1]) {
    throw new Error("usage: node summarize-paired.mjs --input <directory> --output <file>");
  }
  const input = resolve(process.argv[inputIndex + 1]);
  const output = resolve(process.argv[outputIndex + 1]);
  let manifest = null;
  try {
    manifest = JSON.parse(await readFile(resolve(input, "run-manifest.json"), "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const entries = await loadPairedEntries(input, manifest);
  const summary = summarizePairs(entries, manifest);
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  if (!summary.complete) process.exitCode = 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
