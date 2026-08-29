import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import { loadMaterializedComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import { electronMultiDocumentMissingBenefitMetricsV5 } from "./decision-contract-v5.mjs";
import { canonicalSha256 } from "./run-paired-v4.mjs";
import {
  assessRawBenefitEvidenceV5,
  assessV5Launch,
  buildAnalyzerInputV5,
  buildHardComponentReportV5,
  buildRunnerInvocationV5,
  buildV5ComparisonPlan,
  buildV5DryRun,
  buildV5ExecutionSchedule,
  calibrationPairCountV5,
  estimateV5PaidLease,
  electronPropertyOutcomeAcceptedV5,
  expectedDecisionContractSha256V5,
  expectedWorkloadArtifactSha256V5,
  expectedWorkloadByteSha256V5,
  finalPairCountV5,
  parseV5Arguments,
  validateExactCandidateHashesV5,
  validateGpuSamplesV5,
  verifyV5FixturesAndReferences,
} from "./run-paired-v5.mjs";
import { validateHardComponentReportV5 } from "./summarize-paired-v5.mjs";

const digest = "a".repeat(64);

function commonBenefitTimingBoundary() {
  return {
    schema_version: 1,
    boundary_id: "x11-present-complete-after-xtest-v1",
    input_clock: "CLOCK_MONOTONIC",
    completion_clock: "CLOCK_MONOTONIC",
    completion_signal: "X11-PresentCompleteNotify",
    observer_process_independent: true,
    physical_scanout_observed: false,
    passed: true,
    decision_timing_eligible: true,
    sample_count: 60,
    input_to_present_complete_p95_ms: 14.5,
  };
}

function eligibleNativeRawReport() {
  return {
    iterations: [
      {
        native_input: {
          input_lane: "native-x11-xtest",
          execution_status: "passed",
          real_gui_run: true,
          decision_timing_eligible: true,
          evidence: {
            common_benefit_timing_boundary: commonBenefitTimingBoundary(),
          },
        },
      },
    ],
  };
}

test("benefit evidence rejects semantic lanes and implementation-specific application clocks", () => {
  const nativeRun = {
    benefit_metrics_eligible: true,
    input_lane: "native-x11-xtest",
  };
  assert.equal(
    assessRawBenefitEvidenceV5(eligibleNativeRawReport(), nativeRun).eligible,
    true,
  );

  const semantic = eligibleNativeRawReport();
  assert.equal(
    assessRawBenefitEvidenceV5(semantic, {
      ...nativeRun,
      input_lane: "semantic-diagnostic",
    }).eligible,
    false,
  );

  const applicationClockOnly = eligibleNativeRawReport();
  delete applicationClockOnly.iterations[0].native_input.evidence
    .common_benefit_timing_boundary;
  applicationClockOnly.iterations[0].native_input.evidence = {
    receipt_scope:
      "gpui-input-latency-histogram-to-platform-draw-submission-not-physical-scanout",
  };
  assert.equal(
    assessRawBenefitEvidenceV5(applicationClockOnly, nativeRun).eligible,
    false,
  );
});

function argumentList(extra = []) {
  return [
    "--output",
    "/tmp/bp-v5-output",
    "--electron",
    "/tmp/electron",
    "--gpui-binary",
    "/tmp/gpui",
    "--electron-candidate-artifact",
    "/tmp/electron-candidate.json",
    "--gpui-candidate-artifact",
    "/tmp/gpui-candidate.json",
    "--electron-candidate-sha256",
    digest,
    "--gpui-candidate-sha256",
    "b".repeat(64),
    "--hourly-usd",
    "1.50",
    ...extra,
  ];
}

function planOptions(workload, overrides = {}) {
  return {
    output: "/tmp/bp-v5-output",
    electron: "/tmp/electron",
    gpuiBinary: "/tmp/gpui",
    electronCandidateArtifact: "/tmp/electron-candidate.json",
    gpuiCandidateArtifact: "/tmp/gpui-candidate.json",
    electronCandidateSha256: digest,
    gpuiCandidateSha256: "b".repeat(64),
    hourlyUsd: 1.5,
    calibrationPairs: 6,
    finalPairs: 24,
    timeoutMs: 120_000,
    cooldownMs: 2_000,
    cleanupGraceMs: 900_000,
    taskHeadroomPercent: 35,
    seed: 19,
    fixtures: new Map(
      workload.fixtures.map((fixture) => [
        fixture.id,
        `/fixtures/${fixture.id}.pdf`,
      ]),
    ),
    expectedDurations: new Map(),
    referenceCropDirectory: "/references",
    mode: "plan",
    ...overrides,
  };
}

function testLaunchBinding(workload, { implementation, journey, component }) {
  const plan = buildV5ComparisonPlan(workload);
  const journeyPlan = plan.journeys.find(
    ({ scenario }) => scenario === journey,
  );
  const fixtureIds = journeyPlan.component_fixture_ids[component] ?? [
    "bp-annotation-density-v1",
  ];
  const launchId = `test-${implementation}-${component}`;
  return {
    schema_version: 1,
    launch_id: launchId,
    schedule_index: 0,
    phase: "correctness",
    inference_eligible: false,
    journey,
    pair: 0,
    pair_position: "first",
    implementation,
    component,
    component_index: 0,
    input_lane: "native-x11-xtest",
    candidate_manifest_sha256: digest,
    fixture_sha256_by_id: Object.fromEntries(
      fixtureIds.map((fixtureId) => [
        fixtureId,
        journeyPlan.fixture_sha256_by_id[fixtureId],
      ]),
    ),
    raw_report_path: `/run/${launchId}.json`,
    started_at: "2026-08-23T00:00:00.000Z",
    ended_at: "2026-08-23T00:00:01.000Z",
    started_monotonic_ms: 1,
    ended_monotonic_ms: 2,
  };
}

test("freezes six calibration and 24 final pairs and requires reviewed execute TTLs", () => {
  const planned = parseV5Arguments(argumentList());
  assert.equal(planned.mode, "plan");
  assert.equal(planned.calibrationPairs, calibrationPairCountV5);
  assert.equal(planned.finalPairs, finalPairCountV5);
  assert.throws(
    () => parseV5Arguments(argumentList(["--calibration-pairs", "8"])),
    /frozen at 6/,
  );
  assert.throws(
    () => parseV5Arguments(argumentList(["--final-pairs", "28"])),
    /frozen at 24/,
  );
  assert.throws(
    () => parseV5Arguments(argumentList(["--execute"])),
    /task-limit-ms/,
  );
  const executing = parseV5Arguments(
    argumentList([
      "--execute",
      "--task-limit-ms",
      "1000000",
      "--lease-ttl-ms",
      "1900000",
    ]),
  );
  assert.equal(executing.mode, "execute");
});

test("builds the exact final v5 six-journey plan and excludes property from benefits", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const plan = buildV5ComparisonPlan(workload);
  assert.equal(plan.ready, true);
  assert.deepEqual(plan.blockers, []);
  assert.equal(plan.journeys.length, 6);
  assert.equal(plan.workload_artifact_sha256, expectedWorkloadArtifactSha256V5);
  assert.equal(plan.workload_byte_sha256, expectedWorkloadByteSha256V5);
  assert.equal(plan.decision_contract_sha256, expectedDecisionContractSha256V5);
  assert.deepEqual(plan.hard_components, [
    "multi-document-session",
    "native-property-edit-undo",
    "native-snap-transform-120hz",
    "viewer-dynamic-fidelity",
  ]);
  const dense = plan.journeys.find(
    ({ scenario }) => scenario === "dense-mixed-editing",
  );
  assert(!dense.component_order.includes("native-property-edit-undo"));
  assert(dense.component_order.includes("native-snap-transform-120hz"));
  assert.equal(dense.excluded_correctness_components.length, 1);
  assert(
    Math.abs(
      dense.component_weights.reduce((sum, weight) => sum + weight, 0) - 1,
    ) < 1e-12,
  );
});

test("schedules 1,260 paired launches plus exactly two property checks", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const plan = buildV5ComparisonPlan(workload);
  const schedule = buildV5ExecutionSchedule(plan, { seed: 7 });
  assert.equal(schedule.length, 1_262);
  assert.equal(
    schedule.filter(({ phase }) => phase === "correctness").length,
    2,
  );
  assert.equal(
    schedule.filter(({ phase }) => phase === "calibration").length,
    252,
  );
  assert.equal(schedule.filter(({ phase }) => phase === "final").length, 1_008);
  assert(
    schedule
      .filter(({ phase }) => phase !== "correctness")
      .every(({ component }) => component !== "native-property-edit-undo"),
  );
  for (const scenario of plan.journeys.map(({ scenario }) => scenario)) {
    const bundles = [
      ...new Map(
        schedule
          .filter((run) => run.phase === "final" && run.journey === scenario)
          .map((run) => [run.bundle_id, run]),
      ).values(),
    ];
    assert.equal(bundles.length, 48);
    for (let block = 0; block < 24; block += 4) {
      const firsts = bundles
        .filter(({ pair_position: position }) => position === "first")
        .slice(block, block + 4);
      assert.equal(
        firsts.filter(({ implementation }) => implementation === "electron")
          .length,
        2,
      );
    }
  }
});

test("builds the complete analyzer input from a runner-produced manifest", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const plan = buildV5ComparisonPlan(workload);
  const seed = 7;
  const schedule = buildV5ExecutionSchedule(plan, { seed });
  const launches = schedule
    .filter(({ phase, hard_component: hard }) => phase === "final" && hard)
    .map((run) => ({
      ...run,
      passed: true,
      hard_report_path: `/run/${run.bundle_id}-${run.component}.hard.json`,
      hard_report_sha256: digest,
      measurements: { cpu_seconds: 1 },
      quality_measurements:
        run.component === "viewer-dynamic-fidelity"
          ? { visible_page_ready_fraction: 1 }
          : {},
      benefit_metrics_eligible: true,
      correctness_passed: true,
      known_baseline_defect_id: null,
      dynamic_crop_artifacts: [],
    }));
  const candidates = {
    electron: { path: "/candidate/electron.json", sha256: digest },
    gpui: { path: "/candidate/gpui.json", sha256: "b".repeat(64) },
  };
  const manifest = {
    settings: { schedule_seed: seed },
    candidates,
    references: {
      crop1: { path: "/references/crop1.png", bytes: 1, sha256: digest },
    },
    artifact_tree: {
      run_root: "/run",
      reference_root: "/references",
      references: {
        crop1: { path: "/references/crop1.png", bytes: 1, sha256: digest },
      },
    },
    correctness_reports: [
      { implementation: "electron" },
      { implementation: "gpui" },
    ],
    view_state_pairs: [{ passed: true, evidence_sha256: digest }],
    bundles: schedule
      .filter(
        ({ phase, component_index: index }) =>
          phase !== "correctness" && index === 0,
      )
      .map((run) => ({
        ...run,
        path: `/run/${run.bundle_id}.json`,
        sha256: digest,
      })),
    launches,
  };
  const runManifestEvidence = {
    manifest: { path: "/run/run-manifest-v5.json", bytes: 1, sha256: digest },
    checksum: {
      path: "/run/run-manifest-v5.sha256",
      bytes: 1,
      sha256: digest,
    },
  };
  const input = buildAnalyzerInputV5(manifest, runManifestEvidence);
  assert.equal(input.schedule_seed, seed);
  assert.equal(input.bundles.length, 360);
  assert.equal(input.hard_component_reports.length, 144);
  assert.equal(input.dynamic_fidelity_pairs.length, 24);
  assert.deepEqual(input.view_state_pairs, manifest.view_state_pairs);
  assert.deepEqual(input.run_manifest, runManifestEvidence);
  assert.deepEqual(input.candidate_artifacts, candidates);
  assert(
    input.hard_component_reports.every(
      ({ phase, inference_eligible, input_lane }) =>
        phase === "final" &&
        inference_eligible === true &&
        input_lane === "native-x11-xtest",
    ),
  );
});

test("dry run exposes exact commands, report paths, paid time, TTL, and cost", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const options = planOptions(workload);
  const dryRun = buildV5DryRun(workload, options);
  assert.equal(dryRun.matrix.launch_count, 1_262);
  assert.equal(dryRun.lease.measured_launch_count, 1_260);
  assert(dryRun.lease.expected_task_ms > 0);
  assert(
    dryRun.lease.recommended_absolute_lease_ttl_ms >
      dryRun.lease.recommended_task_limit_ms,
  );
  assert(dryRun.lease.maximum_cost_usd > dryRun.lease.expected_cost_usd);
  assert.equal(dryRun.launches.length, 1_262);
  assert(dryRun.launches.every(({ command }) => command.includes("--output")));
  assert(
    dryRun.launches.every(({ raw_report_path: path }) =>
      path.startsWith(options.output),
    ),
  );
  const electronProperty = dryRun.launches.find(
    (run) => run.phase === "correctness" && run.implementation === "electron",
  );
  assert.match(electronProperty.command, /--v5-scenario dense-mixed-editing/);
  const gpuiProperty = dryRun.launches.find(
    (run) => run.phase === "correctness" && run.implementation === "gpui",
  );
  assert(!gpuiProperty.command.includes("--v5-scenario"));

  const overridden = estimateV5PaidLease(
    buildV5ExecutionSchedule(dryRun.plan, { seed: options.seed }),
    {
      ...options,
      expectedDurations: new Map([["viewer-dynamic-fidelity", 60_000]]),
    },
  );
  assert.equal(
    overridden.expected_component_duration_ms["viewer-dynamic-fidelity"],
    60_000,
  );
});

test("runner invocation preserves ordered multi-document fixtures", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const options = planOptions(workload);
  const plan = buildV5ComparisonPlan(workload);
  const run = buildV5ExecutionSchedule(plan, { seed: 3 }).find(
    ({ component }) => component === "multi-document-session",
  );
  const invocation = buildRunnerInvocationV5(run, options);
  const observedPdfs = invocation.argv.flatMap((value, index, all) =>
    all[index - 1] === "--pdf" ? [value] : [],
  );
  assert.deepEqual(
    observedPdfs,
    run.fixture_ids.map((fixtureId) => options.fixtures.get(fixtureId)),
  );
  assert.equal(invocation.environment.BP_PERF_REQUIRE_NVIDIA, "1");
});

function validGpuReport() {
  const samples = [{ index: 0 }];
  const baselineSamples = [{ index: 0 }, { index: 0 }, { index: 0 }];
  return {
    requested_iterations: 1,
    iterations: [
      {
        gpu: {
          qualification: { required: true, passed: true },
          baseline: { sample_count: 3, samples: baselineSamples },
          run: { sample_count: 1, samples },
          baseline_adjusted: { sample_count: 1, samples },
        },
      },
    ],
    summary: {
      gpu_whole_device_baseline_adjusted: {
        qualification_passed: true,
        sample_count: 1,
      },
    },
  };
}

test("fails closed when NVIDIA qualification or samples are missing", () => {
  assert.equal(validateGpuSamplesV5(validGpuReport()).passed, true);
  const missing = validGpuReport();
  missing.iterations[0].gpu.run.samples = [];
  assert.equal(validateGpuSamplesV5(missing).passed, false);
  const unqualified = validGpuReport();
  unqualified.iterations[0].gpu.qualification.passed = false;
  assert.equal(validateGpuSamplesV5(unqualified).passed, false);
});

test("normalizes and accepts only the exact Electron property baseline defect", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const command = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "annotation:native-property-edit-undo");
  const payload = {
    command_id: command.id,
    live: true,
    passed: true,
    proven_milestones: [...command.expected_milestones],
  };
  const receipt = { ...payload, evidence_sha256: canonicalSha256(payload) };
  const raw = {
    implementation: "electron",
    comparison_workload: {
      manifest_id: workload.manifest_id,
      artifact_sha256: expectedWorkloadArtifactSha256V5,
      byte_sha256: expectedWorkloadByteSha256V5,
    },
    comparison_v5: {
      manifest_id: workload.manifest_id,
      workload_artifact_sha256: expectedWorkloadArtifactSha256V5,
      workload_byte_sha256: expectedWorkloadByteSha256V5,
      iterations: [
        {
          command_receipts: [receipt],
          semantic_summary: {
            trusted_native_input: true,
            property: "stroke_width_points",
            before: 1.5,
            committed: 4,
            after_undo: 4,
            effective_history_revision_delta: 2,
            application_undo_count: 1,
            known_baseline_defect_id:
              "electron-numeric-property-input-blur-duplicate-history-v1",
            canonical_state_restored: false,
            native_presentation_acknowledged: true,
            thumbnail_current: true,
          },
        },
      ],
    },
    summary: {},
  };
  raw.launch_binding_v5 = testLaunchBinding(workload, {
    implementation: "electron",
    journey: "dense-mixed-editing",
    component: "native-property-edit-undo",
  });
  const report = buildHardComponentReportV5({
    workload,
    rawReport: raw,
    run: {
      implementation: "electron",
      journey: "dense-mixed-editing",
      component: "native-property-edit-undo",
    },
    candidateArtifactSha256: digest,
  });
  const assessment = validateHardComponentReportV5(workload, report);
  assert.equal(assessment.passed, true);
  assert.equal(assessment.correctness_passed, false);
  assert.equal(
    assessment.known_baseline_defect_id,
    "electron-numeric-property-input-blur-duplicate-history-v1",
  );
  assert.equal(electronPropertyOutcomeAcceptedV5(assessment), true);

  const fixedRaw = structuredClone(raw);
  Object.assign(fixedRaw.comparison_v5.iterations[0].semantic_summary, {
    after_undo: 1.5,
    effective_history_revision_delta: 1,
    known_baseline_defect_id: null,
    canonical_state_restored: true,
  });
  const fixed = validateHardComponentReportV5(
    workload,
    buildHardComponentReportV5({
      workload,
      rawReport: fixedRaw,
      run: {
        implementation: "electron",
        journey: "dense-mixed-editing",
        component: "native-property-edit-undo",
      },
      candidateArtifactSha256: digest,
    }),
  );
  assert.equal(fixed.correctness_passed, true);
  assert.equal(electronPropertyOutcomeAcceptedV5(fixed), true);

  raw.comparison_v5.iterations[0].command_receipts[0].evidence_sha256 =
    "0".repeat(64);
  assert.throws(
    () =>
      buildHardComponentReportV5({
        workload,
        rawReport: raw,
        run: {
          implementation: "electron",
          journey: "dense-mixed-editing",
          component: "native-property-edit-undo",
        },
        candidateArtifactSha256: digest,
      }),
    /forged command receipt/,
  );
});

test("reconstructs hard metrics from raw observations and rejects every tampered embedded projection", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const command = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "annotation:native-property-edit-undo");
  const payload = {
    command_id: command.id,
    live: true,
    passed: true,
    proven_milestones: [...command.expected_milestones],
  };
  const raw = {
    implementation: "gpui",
    requested_iterations: 1,
    iterations: [
      {
        iteration: 1,
        success: true,
        wall_duration_ms: 100,
        events: [
          { event: "operation-visible", duration_ms: 10 },
          { event: "frame", interval_ms: 9 },
          {
            event: "native-application-draw-acknowledgement",
            physical_scanout_observed: false,
            gpui_platform_draw_submitted: true,
            input_latency_samples_before: 0,
            input_latency_samples_after: 1,
            input_to_application_draw_ack_p95_ns: 18_000_000,
          },
        ],
        cgroup: { cpu_seconds: 0.7, memory_peak_bytes: 700_000 },
        gpu: validGpuReport().iterations[0].gpu,
      },
    ],
    comparison_workload: {
      manifest_id: workload.manifest_id,
      artifact_sha256: expectedWorkloadArtifactSha256V5,
      byte_sha256: expectedWorkloadByteSha256V5,
    },
    comparison_v5: {
      manifest_id: workload.manifest_id,
      workload_artifact_sha256: expectedWorkloadArtifactSha256V5,
      workload_byte_sha256: expectedWorkloadByteSha256V5,
      iterations: [
        {
          command_receipts: [
            { ...payload, evidence_sha256: canonicalSha256(payload) },
          ],
          semantic_summary: {
            trusted_native_input: true,
            property: "stroke_width_points",
            before: 1.5,
            committed: 4,
            after_undo: 1.5,
            effective_history_revision_delta: 1,
            application_undo_count: 1,
            known_baseline_defect_id: null,
            canonical_state_restored: true,
            native_presentation_acknowledged: true,
            thumbnail_current: true,
          },
        },
      ],
    },
  };
  const run = {
    implementation: "gpui",
    journey: "dense-mixed-editing",
    component: "native-property-edit-undo",
  };
  const authoritative = buildHardComponentReportV5({
    workload,
    rawReport: raw,
    run,
    candidateArtifactSha256: digest,
  });
  assert.equal(authoritative.summary.process_tree.cpu_seconds.median, 0.7);
  assert.equal(
    authoritative.summary.process_tree.cgroup_memory_peak_bytes.median,
    700_000,
  );
  assert.equal(authoritative.summary.product_latency_ms.p95, 10);
  assert.equal(authoritative.summary.application_frame_intervals_ms.p95, 9);
  assert.equal(
    authoritative.summary.native_input_to_application_frame_ack_ms.p95,
    18,
  );

  const mutations = [
    (projection) => (projection.summary.process_tree.cpu_seconds.median = 99),
    (projection) =>
      (projection.summary.process_tree.cgroup_memory_peak_bytes.median = 99),
    (projection) => (projection.summary.product_latency_ms.p95 = 99),
    (projection) =>
      (projection.summary.application_frame_intervals_ms.p95 = 99),
    (projection) =>
      (projection.summary.native_input_to_application_frame_ack_ms.p95 = 99),
    (projection) =>
      (projection.summary.gpu_whole_device_baseline_adjusted = {
        memory_used_mib: { max: 99 },
      }),
  ];
  for (const mutate of mutations) {
    const tampered = structuredClone(raw);
    tampered.hard_component_report_v5 = structuredClone(authoritative);
    mutate(tampered.hard_component_report_v5);
    assert.throws(
      () =>
        buildHardComponentReportV5({
          workload,
          rawReport: tampered,
          run,
          candidateArtifactSha256: digest,
        }),
      /embedded hard-component projection does not match authoritative raw observations/,
    );
  }
});

test("normalizes the exact Electron second-NASA defect as metric-ineligible", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const journey = workload.journeys.find(
    ({ id }) => id === "multi-document-session-v1",
  );
  const receipts = journey.commands.map((command, index) => {
    const payload = {
      command_id: command.id,
      live: index === 0,
      passed: false,
      proven_milestones:
        index === 0 ? command.expected_milestones.slice(0, 1) : [],
    };
    return { ...payload, evidence_sha256: canonicalSha256(payload) };
  });
  const raw = {
    implementation: "electron",
    comparison_workload: {
      manifest_id: workload.manifest_id,
      artifact_sha256: expectedWorkloadArtifactSha256V5,
      byte_sha256: expectedWorkloadByteSha256V5,
    },
    comparison_v5: {
      manifest_id: workload.manifest_id,
      workload_artifact_sha256: expectedWorkloadArtifactSha256V5,
      workload_byte_sha256: expectedWorkloadByteSha256V5,
      iterations: [
        {
          command_receipts: receipts,
          semantic_summary: {
            known_baseline_defect_id:
              "electron-multi-document-second-nasa-visible-pages-empty-v1",
            activated_fixture_id: "nasa-apollo-summary-526-v1",
            activation_ordinal: 2,
            visible_page_indices: [],
            queued_raster_count: 0,
            inflight_raster_count: 0,
            visible_raster_presented: false,
            error_presented: false,
            benchmark_metrics_eligible: false,
            benchmark_metrics_missing: [
              ...electronMultiDocumentMissingBenefitMetricsV5,
            ],
          },
        },
      ],
    },
    summary: {},
  };
  raw.launch_binding_v5 = testLaunchBinding(workload, {
    implementation: "electron",
    journey: "multi-document-session",
    component: "multi-document-session",
  });
  const report = buildHardComponentReportV5({
    workload,
    rawReport: raw,
    run: {
      implementation: "electron",
      journey: "multi-document-session",
      component: "multi-document-session",
    },
    candidateArtifactSha256: digest,
  });
  const assessment = validateHardComponentReportV5(workload, report);
  assert.equal(assessment.passed, true);
  assert.equal(assessment.correctness_passed, false);
  assert.equal(assessment.benefit_metrics_eligible, false);
  assert.deepEqual(assessment.measurements, {});

  raw.comparison_v5.iterations[0].semantic_summary.queued_raster_count = 1;
  assert.equal(
    validateHardComponentReportV5(
      workload,
      buildHardComponentReportV5({
        workload,
        rawReport: raw,
        run: {
          implementation: "electron",
          journey: "multi-document-session",
          component: "multi-document-session",
        },
        candidateArtifactSha256: digest,
      }),
    ).passed,
    false,
  );
});

test("rejects any candidate manifest hash other than the declared exact bytes", () => {
  const candidates = {
    electron: { sha256: digest },
    gpui: { sha256: "b".repeat(64) },
  };
  assert.equal(
    validateExactCandidateHashesV5(candidates, {
      electron: digest,
      gpui: "b".repeat(64),
    }),
    candidates,
  );
  assert.throws(
    () =>
      validateExactCandidateHashesV5(candidates, {
        electron: "c".repeat(64),
        gpui: "b".repeat(64),
      }),
    /Electron candidate manifest/i,
  );
});

test("verifies fixture and transferred reference bytes before execution", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-v5-preflight-"));
  try {
    const fixtureBytes = "fixture";
    const referenceBytes = "reference";
    const fixtureHash = createHash("sha256").update(fixtureBytes).digest("hex");
    const referenceHash = createHash("sha256")
      .update(referenceBytes)
      .digest("hex");
    const fixturePath = resolve(directory, "fixture.pdf");
    const referenceDirectory = resolve(directory, "references");
    const referencePath = resolve(referenceDirectory, "crop.png");
    await mkdir(referenceDirectory);
    await writeFile(fixturePath, fixtureBytes);
    await writeFile(referencePath, referenceBytes);
    const workload = {
      fixtures: [{ id: "fixture-v1", sha256: fixtureHash }],
      journeys: [
        {
          commands: [
            {
              id: "viewer:dynamic-fidelity-scroll",
              registered_crops: [
                {
                  crop_id: "crop",
                  reference_raster: {
                    reference_crop_sha256: referenceHash,
                  },
                },
              ],
            },
          ],
        },
      ],
    };
    const verified = await verifyV5FixturesAndReferences(workload, {
      fixtures: new Map([["fixture-v1", fixturePath]]),
      referenceCropDirectory: referenceDirectory,
    });
    assert.equal(verified.fixtures["fixture-v1"].sha256, fixtureHash);
    assert.equal(verified.references.crop.sha256, referenceHash);
    await writeFile(referencePath, "mutated");
    await assert.rejects(
      verifyV5FixturesAndReferences(workload, {
        fixtures: new Map([["fixture-v1", fixturePath]]),
        referenceCropDirectory: referenceDirectory,
      }),
      /reference SHA-256 mismatch/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("hard launch assessment also rejects missing GPU samples", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const command = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "annotation:native-property-edit-undo");
  const payload = {
    command_id: command.id,
    live: true,
    passed: true,
    proven_milestones: [...command.expected_milestones],
  };
  const raw = {
    implementation: "gpui",
    requested_iterations: 1,
    iterations: [{}],
    comparison_workload: {
      manifest_id: workload.manifest_id,
      artifact_sha256: expectedWorkloadArtifactSha256V5,
      byte_sha256: expectedWorkloadByteSha256V5,
    },
    comparison_v5: {
      manifest_id: workload.manifest_id,
      workload_artifact_sha256: expectedWorkloadArtifactSha256V5,
      workload_byte_sha256: expectedWorkloadByteSha256V5,
      iterations: [
        {
          command_receipts: [
            { ...payload, evidence_sha256: canonicalSha256(payload) },
          ],
          semantic_summary: {
            trusted_native_input: true,
            property: "stroke_width_points",
            before: 1.5,
            committed: 4,
            after_undo: 1.5,
            effective_history_revision_delta: 1,
            application_undo_count: 1,
            known_baseline_defect_id: null,
            canonical_state_restored: true,
            native_presentation_acknowledged: true,
            thumbnail_current: true,
          },
        },
      ],
    },
    summary: {},
  };
  const assessment = assessV5Launch({
    workload,
    v4Workload: null,
    rawReport: raw,
    run: {
      hard_component: true,
      implementation: "gpui",
      journey: "dense-mixed-editing",
      component: "native-property-edit-undo",
    },
    candidateArtifactSha256: digest,
  });
  assert.equal(assessment.passed, false);
  assert(assessment.failures.some((failure) => failure.includes("NVIDIA")));
});
