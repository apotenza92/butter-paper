import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import { scenarioContractVersion } from "./scenario-contract.mjs";
import {
  protocolVersionV4,
  representativeScenarioDefinitionsV4,
  representativeTimedScenarioIdsV4,
  scenarioContractVersionV4,
} from "./scenario-contract-v4.mjs";
import {
  fullTimedRepresentativeScenarios,
  untimedCorrectnessPreflights,
} from "./run-paired.mjs";
import {
  fiveJourneyScenarioMap,
  loadPairedEntries,
  pairedBootstrap,
  pairedEvidenceAnalysisSchemaVersion,
  summarizePairs,
} from "./summarize-paired.mjs";

const fixtureHashes = {
  "open-pdf": "68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049",
  zoom: "f058179e193ccbc15ca662feff3554102f64ff2114436a5ce6116d6fa5d2a6e2",
};

test("paired bootstrap uses the shared geometric-mean decision statistic", () => {
  assert.deepEqual(pairedBootstrap([0.5, 0.5, 0.5, 0.5], 100), {
    method: "paired percentile bootstrap of the geometric mean ratio",
    estimate: 0.5,
    samples: 100,
    seed: 0x4250_5632,
    lower_95: 0.5,
    upper_95: 0.5,
  });
});

function report(implementation, scenario, {
  decisionEligible = true,
  success = true,
  sha256 = fixtureHashes[scenario],
  peakRss = 100,
} = {}) {
  const visibleEvent = implementation === "electron" ? "first-page-visible" : "viewport-visible";
  return {
    implementation,
    scenario,
    protocol_version: "bp-perf-v2",
    scenario_contract_version: scenarioContractVersion,
    pdf: { bytes: scenario === "zoom" ? 456 : 123, sha256 },
    comparison_workload: {
      manifest_id: "bp-perf-v3-decision-3",
      decision_timing_eligible: decisionEligible,
    },
    workload: { scenario },
    iterations: [{
      success,
      error: success ? null : "renderer did not settle",
      events: [{ event: visibleEvent, duration_ms: 10, t_ms: 20 }],
    }],
    summary: {
      successful_iterations: success ? 1 : 0,
      failed_iterations: success ? 0 : 1,
      wall_duration_ms: { median: 25 },
      process_tree: {
        peak_rss_kb: peakRss,
        peak_cpu_percent: 10,
        cpu_seconds: { median: 1 },
      },
    },
  };
}

function entry(implementation, scenario, pair, position, options = {}) {
  return {
    name: `${scenario}-pair${pair}-${position}-${implementation}`,
    scenario,
    pair,
    position,
    implementation,
    measured: true,
    report: options.report ?? report(implementation, scenario, options),
    run: options.run,
    report_error: options.reportError,
  };
}

test("derives scenarios and pair ids from entries and validates each scenario fixture", () => {
  const summary = summarizePairs([
    entry("gpui", "open-pdf", 2, "first"),
    entry("electron", "open-pdf", 2, "second"),
    entry("electron", "zoom", 7, "first"),
    entry("gpui", "zoom", 7, "second"),
  ]);

  assert.deepEqual(Object.keys(summary.scenarios), ["open-pdf", "zoom"]);
  assert.equal(summary.scenarios["open-pdf"].expected_pair_count, 1);
  assert.equal(summary.scenarios["open-pdf"].pairs[0].pair, 2);
  assert.equal(summary.scenarios.zoom.expected_pair_count, 1);
  assert.deepEqual(summary.corpora, {
    "open-pdf": { bytes: 123, sha256: fixtureHashes["open-pdf"] },
    zoom: { bytes: 456, sha256: fixtureHashes.zoom },
  });
  assert.equal(summary.complete, true);
});

test("validates the retained randomized pair order instead of assuming odd-even alternation", () => {
  const manifest = {
    schema_version: 1,
    settings: {
      scenarios: ["open-pdf"],
      pairs: 1,
      pair_orders: [["gpui", "electron"]],
    },
    scenario_fixtures: {
      "open-pdf": {
        fixture_id: "nasa-apollo-summary-526-v1",
        fixture_sha256: fixtureHashes["open-pdf"],
      },
    },
    runs: [],
  };
  const summary = summarizePairs([
    entry("gpui", "open-pdf", 1, "first"),
    entry("electron", "open-pdf", 1, "second"),
  ], manifest);

  assert.equal(summary.complete, true);
  assert.equal(summary.decision_ready, false);
  assert.equal(summary.scenarios["open-pdf"].valid_pair_count, 1);
  assert.equal(summary.scenarios["open-pdf"].pairs[0].order, "gpui-electron");
});

test("uses the run manifest to retain failed and missing measured runs", () => {
  const failedRun = {
    implementation: "electron",
    scenario: "open-pdf",
    fixture_id: "nasa-apollo-summary-526-v1",
    name: "open-pdf-pair2-first-electron",
    measured: true,
    exit_code: 1,
    signal: null,
    spawn_error: null,
    stderr: "renderer did not settle",
  };
  const manifest = {
    schema_version: 1,
    settings: { scenarios: ["open-pdf"], pairs: 2 },
    scenario_fixtures: {
      "open-pdf": {
        fixture_id: "nasa-apollo-summary-526-v1",
        fixture_sha256: fixtureHashes["open-pdf"],
      },
    },
    runs: [failedRun],
  };
  const summary = summarizePairs([
    entry("electron", "open-pdf", 1, "first"),
    entry("gpui", "open-pdf", 1, "second"),
    entry("electron", "open-pdf", 2, "first", {
      report: report("electron", "open-pdf", { success: false }),
      run: failedRun,
    }),
  ], manifest);

  assert.equal(summary.complete, false);
  assert.equal(summary.scenarios["open-pdf"].expected_pair_count, 2);
  assert.equal(summary.scenarios["open-pdf"].valid_pair_count, 1);
  assert.equal(summary.scenarios["open-pdf"].rejected_pair_count, 1);
  assert.equal(summary.scenarios["open-pdf"].pairs[1].runs.electron.exit_code, 1);
  assert.equal(summary.scenarios["open-pdf"].pairs[1].runs.electron.stderr, "renderer did not settle");
  assert.equal(summary.scenarios["open-pdf"].pairs[1].runs.gpui, null);
  assert(summary.failures.some(({ message }) => message.includes("pair 2 is incomplete")));
  assert(summary.failures.some(({ message }) => message.includes("iteration did not pass")));
});

test("rejects an old timed subset and missing untimed or global readiness as decision evidence", () => {
  const manifest = {
    schema_version: 2,
    settings: {
      scenarios: [
        "open-pdf",
        "viewer-layout",
        "page-navigation",
        "zoom",
        "close-reopen",
        "annotation-create",
        "continuous-scroll",
      ],
      pairs: 1,
      pair_orders: [["electron", "gpui"]],
    },
    comparison_plan: {
      ready: false,
      timed_scenarios: [],
      untimed_preflights: untimedCorrectnessPreflights,
      global_command_coverage: [
        { implementation: "electron", ready: false },
        { implementation: "gpui", ready: false },
      ],
      blockers: [{ reason: "global-command-coverage-incomplete" }],
    },
    runs: [],
  };
  const summary = summarizePairs([], manifest);

  assert.equal(summary.decision_ready, false);
  assert.equal(summary.comparison_readiness.ready, false);
  for (const scenario of fullTimedRepresentativeScenarios.filter(
    (scenario) => !manifest.settings.scenarios.includes(scenario),
  )) {
    assert(summary.comparison_readiness.failures.includes(
      `required timed scenario ${scenario} is missing`,
    ));
  }
  assert(summary.comparison_readiness.failures.some((failure) =>
    failure.includes("electron:open-pdf") && failure.includes("native-launch-open")));
  assert(summary.comparison_readiness.failures.includes(
    "electron global command coverage is incomplete",
  ));
  assert(summary.comparison_readiness.failures.includes(
    "gpui global command coverage is incomplete",
  ));
});

test("rejects decision-ineligible reports and excludes their timing and resource metrics", () => {
  const summary = summarizePairs([
    entry("electron", "open-pdf", 1, "first", { decisionEligible: false, peakRss: 800 }),
    entry("gpui", "open-pdf", 1, "second", { peakRss: 200 }),
  ]);

  const scenario = summary.scenarios["open-pdf"];
  assert.equal(scenario.valid_pair_count, 0);
  assert.equal(scenario.rejected_pair_count, 1);
  assert.deepEqual(scenario.metrics, {});
  assert(scenario.pairs[0].validation_errors.some((error) => error.includes("decision timing ineligible")));
});

test("rejects a report that uses another scenario's locked fixture", () => {
  const summary = summarizePairs([
    entry("electron", "open-pdf", 1, "first"),
    entry("gpui", "open-pdf", 1, "second"),
    entry("electron", "zoom", 1, "first"),
    entry("gpui", "zoom", 1, "second", { sha256: fixtureHashes["open-pdf"] }),
  ]);

  assert.equal(summary.scenarios["open-pdf"].valid_pair_count, 1);
  assert.equal(summary.scenarios.zoom.valid_pair_count, 0);
  assert(summary.scenarios.zoom.pairs[0].validation_errors.some(
    (error) => error.includes("fixture identity mismatch"),
  ));
});

test("derives resource, frame, recovery, journey, reliability, and evidence-reference analysis", () => {
  const enriched = (implementation, peakMemory, gpuMemory) => {
    const value = report(implementation, "open-pdf");
    value.iterations[0].events.push({
      event: "comparison-memory-recovery",
      released_render_bytes: implementation === "electron" ? 400 : 300,
    });
    value.summary.process_tree.cgroup_memory_peak_bytes = { median: peakMemory };
    value.summary.gpu_whole_device = {
      memory_used_mib: { max: gpuMemory },
      utilization_percent: { p95: implementation === "electron" ? 40 : 20 },
    };
    value.summary.frame_intervals_ms = { p95: 18 };
    value.summary.native_presentation_intervals_ms = { p95: 17 };
    value.summary.native_input_to_present_ms = { p95: 22 };
    value.provenance = { runtime: {
      [implementation === "electron" ? "electron" : "binary"]: {
        sha256: implementation === "electron" ? "a".repeat(64) : "b".repeat(64),
      },
    } };
    return value;
  };
  const summary = summarizePairs([
    entry("electron", "open-pdf", 1, "first", {
      report: enriched("electron", 800, 200),
    }),
    entry("gpui", "open-pdf", 1, "second", {
      report: enriched("gpui", 400, 100),
    }),
  ]);

  const metrics = summary.scenarios["open-pdf"].metrics;
  assert.equal(metrics.cgroup_peak_memory_bytes.gpui_over_electron.estimate, 0.5);
  assert.equal(metrics.whole_gpu_peak_memory_mib.gpui_over_electron.estimate, 0.5);
  assert.equal(metrics.whole_gpu_utilization_p95_percent.gpui_over_electron.estimate, 0.5);
  assert.equal(metrics.native_presentation_interval_p95_ms.electron.median, 17);
  assert.equal(metrics.native_input_to_present_p95_ms.gpui.median, 22);
  assert.equal(metrics.application_frame_interval_p95_ms_diagnostic.electron.median, 18);
  assert.equal(metrics.released_render_bytes.electron.median, 400);
  assert.equal(summary.analysis.schema_version, pairedEvidenceAnalysisSchemaVersion);
  assert.deepEqual(Object.keys(summary.analysis.journeys), Object.keys(fiveJourneyScenarioMap));
  assert.deepEqual(summary.analysis.reliability.electron, {
    attempts: 1,
    successes: 1,
    failures: 0,
    attempt_refs: ["#/scenarios/open-pdf/pairs/0/runs/electron"],
    failure_refs: [],
  });
  assert(summary.analysis.metric_families.process_memory.evidence_refs.includes(
    "#/scenarios/open-pdf/metrics/cgroup_peak_memory_bytes",
  ));
  assert.equal(summary.analysis.journeys["small-shell-open-v1"].status, "missing-evidence");
  assert.equal(summary.analysis.contract_version, "bp-perf-v4-decision-1");
  assert.equal(
    summary.analysis.hard_evidence_refs["candidate-artifacts-frozen"].length,
    2,
  );
  assert(summary.analysis.metric_families.gpu_resource_pressure.evidence_refs.includes(
    "#/scenarios/open-pdf/metrics/whole_gpu_utilization_p95_percent",
  ));
  assert.equal(summary.analysis.eligibility, "not-decision-ready");
});

test("retains the canonical five-journey v4 scenario shape", () => {
  const entries = representativeTimedScenarioIdsV4.flatMap((scenario, index) =>
    ["electron", "gpui"].map((implementation, implementationIndex) => {
      const definition = representativeScenarioDefinitionsV4[scenario];
      const value = report(implementation, "open-pdf", { sha256: definition.fixture_sha256 });
      value.scenario = scenario;
      value.protocol_version = protocolVersionV4;
      value.scenario_contract_version = scenarioContractVersionV4;
      value.pdf.bytes = 1_000 + index;
      value.workload = { scenario };
      value.summary.process_tree.cgroup_memory_peak_bytes = { median: 1_000 };
      value.summary.gpu_whole_device = {
        memory_used_mib: { max: 10 },
        utilization_percent: { p95: 5 },
      };
      value.summary.native_presentation_intervals_ms = { p95: 17 };
      value.summary.native_input_to_present_ms = { p95: 20 };
      value.summary.product_latency_ms = { p95: 25 };
      return {
        name: `${scenario}-pair1-${implementationIndex === 0 ? "first" : "second"}-${implementation}`,
        scenario,
        pair: 1,
        position: implementationIndex === 0 ? "first" : "second",
        implementation,
        measured: true,
        run: { exit_code: 0 },
        report: value,
      };
    }));

  const summary = summarizePairs(entries);

  assert.deepEqual(Object.keys(summary.scenarios), representativeTimedScenarioIdsV4);
  assert.deepEqual(
    Object.values(summary.analysis.journeys).map(({ required_scenarios: scenarios }) => scenarios),
    representativeTimedScenarioIdsV4.map((scenario) => [scenario]),
  );
  assert(Object.values(summary.analysis.journeys).every(({ status }) => status === "complete"));
  assert.equal(
    summary.analysis.journeys["small-shell-open-v1"].metrics["small-shell-open"]
      .cgroup_peak_memory_bytes.evidence_ref,
    "#/scenarios/small-shell-open/metrics/cgroup_peak_memory_bytes",
  );
  assert.equal(summary.analysis.metric_families.process_memory.status, "complete");
  assert.equal(summary.analysis.metric_families.gpu_resource_pressure.status, "complete");
});

test("a bad manifest fixture invalidates the pair instead of producing metrics", () => {
  const manifest = {
    settings: { scenarios: ["zoom"], pairs: 1 },
    scenario_fixtures: {
      zoom: {
        fixture_id: "nasa-apollo-summary-526-v1",
        fixture_sha256: fixtureHashes["open-pdf"],
      },
    },
    runs: [],
  };
  const summary = summarizePairs([
    entry("electron", "zoom", 1, "first", { sha256: fixtureHashes["open-pdf"] }),
    entry("gpui", "zoom", 1, "second", { sha256: fixtureHashes["open-pdf"] }),
  ], manifest);

  assert.equal(summary.scenarios.zoom.valid_pair_count, 0);
  assert.deepEqual(summary.scenarios.zoom.metrics, {});
  assert(summary.scenarios.zoom.pairs[0].validation_errors.some(
    (error) => error.includes("run manifest fixture does not match"),
  ));
});

test("loads measured manifest runs by retained basename and records a missing report", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-paired-summary-"));
  try {
    const electronName = "open-pdf-pair1-first-electron";
    await writeFile(
      resolve(directory, `${electronName}.json`),
      JSON.stringify(report("electron", "open-pdf")),
    );
    const manifest = {
      runs: [
        {
          name: electronName,
          output: `/moved/remote/path/${electronName}.json`,
          measured: true,
          exit_code: 0,
        },
        {
          name: "open-pdf-pair1-second-gpui",
          output: "/moved/remote/path/open-pdf-pair1-second-gpui.json",
          measured: true,
          exit_code: 1,
        },
      ],
    };

    const entries = await loadPairedEntries(directory, manifest);
    assert.equal(entries.length, 2);
    assert.equal(entries[0].report.implementation, "electron");
    assert.equal(entries[1].report, null);
    assert.match(entries[1].report_error, /ENOENT/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
