import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import {
  benefitLaunchCountV6,
  assertSessionIdentityV6,
  buildQualificationEnvironmentBindingV6,
  buildQualificationReceiptV6,
  buildImmutableLeaseBindingV6,
  buildRunnerInvocationV6,
  buildV6ComparisonPlan,
  buildV6DryRun,
  buildV6ExecutionSchedule,
  buildV6MainSchedule,
  buildV6QualificationRuns,
  collectSessionIdentityV6,
  expectedWorkloadByteSha256V6,
  exactElectronEngineeringZoomBaselineDefectV6,
  loadComparisonWorkloadV6,
  parseNvidiaAdapterIdentityV6,
  parseDbusBusIdV6,
  parseV6Arguments,
  propertyCorrectnessLaunchCountV6,
  qualificationLeaseTtlMaximumMsV6,
  qualificationTaskLimitMaximumMsV6,
  reusableSchedulePrefixV6,
  runRunnerV6,
  semanticCorrectnessLaunchCountV6,
  totalLaunchCountV6,
  validateCandidateLaunchSealV6,
  validateQualificationReceiptV6,
  validateImmutableLeaseBindingV6,
  v6NativeObserverIntegrationPreflight,
} from "./run-paired-v6.mjs";
import { canonicalSha256 } from "./run-paired-v4.mjs";
import { compareViewStateReceiptsV5 } from "./matched-view-state-v5.mjs";
import {
  benefitEligibleComponentIdsV6,
  semanticCorrectnessOnlyComponentIdsV6,
} from "./scenario-contract-v6.mjs";

async function planAndSchedule() {
  const { workload, byte_sha256: byteSha256 } =
    await loadComparisonWorkloadV6();
  const plan = buildV6ComparisonPlan(workload, byteSha256);
  return { workload, plan, schedule: buildV6ExecutionSchedule(plan) };
}

test("freezes a new v6 workload without changing v5 identities", async () => {
  const { workload, byte_sha256: byteSha256 } =
    await loadComparisonWorkloadV6();
  assert.equal(byteSha256, expectedWorkloadByteSha256V6);
  assert.equal(
    workload.source_v5.workload_artifact_sha256,
    "cc4f8b8940556390b8d16a6baae43e8a5a022541fba90beea08869e692ee920e",
  );
  assert.equal(
    workload.source_v5.workload_byte_sha256,
    "e7b2540c7d455a30e52ee64a6819745fe0ad49a6512f887df4631aac72054f6d",
  );
  const plan = buildV6ComparisonPlan(workload, byteSha256);
  assert.equal(plan.ready, true);
  assert.deepEqual(plan.blockers, []);
});

test("builds exactly 600 native benefit and 24 correctness launches", async () => {
  const { schedule } = await planAndSchedule();
  assert.equal(schedule.length, totalLaunchCountV6);
  assert.equal(
    schedule.filter(({ benefit_metrics_eligible: eligible }) => eligible)
      .length,
    benefitLaunchCountV6,
  );
  assert.equal(
    schedule.filter(
      ({ phase, input_lane: lane }) =>
        phase === "correctness" && lane === "semantic-diagnostic",
    ).length,
    semanticCorrectnessLaunchCountV6,
  );
  assert.equal(
    schedule.filter(
      ({ component }) => component === "native-property-edit-undo",
    ).length,
    propertyCorrectnessLaunchCountV6,
  );
});

test("keeps only the eight real native component names benefit-eligible", async () => {
  const { schedule } = await planAndSchedule();
  const benefit = schedule.filter(
    ({ benefit_metrics_eligible: eligible }) => eligible,
  );
  assert.deepEqual(
    [...new Set(benefit.map(({ component }) => component))].sort(),
    [...benefitEligibleComponentIdsV6].sort(),
  );
  assert(benefit.every(({ input_lane: lane }) => lane === "native-x11-xtest"));
  assert.equal(
    benefit.filter(({ component }) => component === "open-pdf").length,
    180,
  );
  for (const component of benefitEligibleComponentIdsV6.filter(
    (candidate) => candidate !== "open-pdf",
  )) {
    assert.equal(
      benefit.filter((run) => run.component === component).length,
      60,
    );
  }
});

test("runs every semantic component once per candidate and never in inference", async () => {
  const { schedule } = await planAndSchedule();
  for (const component of semanticCorrectnessOnlyComponentIdsV6) {
    const runs = schedule.filter((run) => run.component === component);
    assert.equal(runs.length, 2);
    assert.deepEqual(
      [...runs.map(({ implementation }) => implementation)].sort(),
      ["electron", "gpui"],
    );
    assert(
      runs.every(
        (run) =>
          run.phase === "correctness" &&
          run.inference_eligible === false &&
          run.benefit_metrics_eligible === false &&
          run.input_lane === "semantic-diagnostic",
      ),
    );
  }
});

test("fails full execution preflight on the local graphics-stack boundary blocker", async () => {
  const preflight = v6NativeObserverIntegrationPreflight();
  assert.equal(preflight.ready, false);
  assert.equal(preflight.blockers.length, 1);
  assert(
    preflight.blockers[0].includes("authenticated paid-GPU qualification"),
  );

  const { workload, plan, schedule } = await planAndSchedule();
  const dryRun = buildV6DryRun(plan, schedule, {
    mode: "plan",
    output: "/tmp/v6",
    electron: "/candidate/electron",
    gpuiBinary: "/candidate/gpui",
    fixtures: new Map(
      [
        "bp-single-page-v1",
        "nasa-apollo-summary-526-v1",
        "bp-engineering-sheet-v1",
        "bp-annotation-density-v1",
        "bp-annotation-all-v1",
      ].map((fixture) => [fixture, `/fixtures/${fixture}.pdf`]),
    ),
    referenceCropDirectory: "/references",
  });
  assert.equal(dryRun.schedule_summary.total_launches, 624);
  assert.equal(dryRun.execution_preflight.ready, false);
  assert.equal(workload.schedule.total_launches, 624);
  const inheritedElectronBenefit = dryRun.launches.find(
    (launch) =>
      launch.implementation === "electron" &&
      launch.component === "open-pdf" &&
      launch.phase === "final",
  );
  assert(
    inheritedElectronBenefit.argv.includes("--v6-scenario"),
    "Electron inherited native benefit run must receive a v6 context",
  );
  const inheritedGpuiBenefit = dryRun.launches.find(
    (launch) =>
      launch.implementation === "gpui" &&
      launch.component === "open-pdf" &&
      launch.phase === "final",
  );
  assert(inheritedGpuiBenefit.argv.includes("--v6-scenario"));
  assert.equal(
    inheritedElectronBenefit.environment.BP_PERF_COMMON_DAMAGE_OBSERVER,
    "1",
  );
  assert.equal(
    inheritedGpuiBenefit.environment.BP_PERF_COMMON_DAMAGE_OBSERVER,
    "1",
  );
});

test("freezes qualification at one short native pair before the 624 launches", async () => {
  const { plan } = await planAndSchedule();
  const runs = buildV6QualificationRuns(plan);
  assert.deepEqual(
    runs.map(
      ({ implementation, journey, component, schedule_index: index }) => ({
        implementation,
        journey,
        component,
        index,
      }),
    ),
    [
      {
        implementation: "electron",
        journey: "small-shell-open",
        component: "open-pdf",
        index: 0,
      },
      {
        implementation: "gpui",
        journey: "small-shell-open",
        component: "open-pdf",
        index: 1,
      },
    ],
  );
  assert.equal(qualificationTaskLimitMaximumMsV6, 8 * 60_000);
  assert.equal(qualificationLeaseTtlMaximumMsV6, 30 * 60_000);
  const invocationOptions = {
    output: "/tmp/bp-v6-qualification",
    timeoutMs: 120_000,
    fixtures: new Map([["bp-single-page-v1", "/fixtures/single.pdf"]]),
    electron: "/candidate/electron",
    gpuiBinary: "/candidate/gpui",
    referenceCropDirectory: "/fixtures/reference-crops",
  };
  const invocations = runs.map((run) =>
    buildRunnerInvocationV6(run, invocationOptions),
  );
  assert.equal(new Set(invocations.map(({ raw_report_path: path }) => path)).size, 2);
  assert.match(invocations[0].raw_report_path, /-electron\.json$/);
  assert.match(invocations[1].raw_report_path, /-gpui\.json$/);
});

test("accepts only the exact validated Electron engineering zoom baseline receipt errors", () => {
  const exact = {
    knownBaselineDefectId:
      "electron-engineering-zoom-density-and-raster-bound-v1",
    retainedReportDefectId:
      "electron-engineering-zoom-density-and-raster-bound-v1",
    retainedIterationDefectId:
      "electron-engineering-zoom-density-and-raster-bound-v1",
    gpuFailures: [],
    fixtureFailures: [],
    receiptErrors: [
      "component receipt summary did not pass",
      "engineering:zoom-sequence: command receipt is not live and passed",
      "engineering:zoom-sequence: Electron component execution did not pass",
      "engineering:zoom-sequence: Electron milestone proof is not exact",
    ],
  };
  assert.equal(exactElectronEngineeringZoomBaselineDefectV6(exact), true);
  assert.equal(
    exactElectronEngineeringZoomBaselineDefectV6({
      ...exact,
      receiptErrors: [...exact.receiptErrors, "component fixture hash does not match"],
    }),
    false,
  );
  assert.equal(
    exactElectronEngineeringZoomBaselineDefectV6({
      ...exact,
      retainedIterationDefectId: null,
    }),
    false,
  );
});

function requiredCliArguments() {
  return [
    "--output",
    "/tmp/bp-v6",
    "--electron",
    "/candidate/electron",
    "--gpui-binary",
    "/candidate/gpui",
    "--electron-candidate-artifact",
    "/candidate/electron.json",
    "--gpui-candidate-artifact",
    "/candidate/gpui.json",
    "--electron-candidate-sha256",
    "a".repeat(64),
    "--gpui-candidate-sha256",
    "b".repeat(64),
    "--hourly-usd",
    "4.41",
    "--fixture",
    "bp-single-page-v1=/fixtures/single.pdf",
  ];
}

test("requires a reviewed short qualification lease and its receipt for execute", async () => {
  const qualification = parseV6Arguments([
    "--qualify",
    ...requiredCliArguments(),
    "--task-limit-ms",
    "480000",
    "--lease-ttl-ms",
    "1380000",
  ]);
  assert.equal(qualification.mode, "qualify");
  assert.equal(
    qualification.qualificationReceipt,
    "/tmp/bp-v6/qualification-receipt-v6.json",
  );
  const { plan } = await planAndSchedule();
  const qualificationSchedule = buildV6MainSchedule(plan, qualification);
  const qualificationDryRun = buildV6DryRun(
    plan,
    qualificationSchedule,
    qualification,
  );
  assert.equal(qualificationDryRun.launches.length, 2);
  assert.equal(qualificationDryRun.lease.launch_count, 2);
  assert.equal(qualificationDryRun.lease.selected_task_limit_ms, 480000);
  assert.equal(
    qualificationDryRun.lease.selected_absolute_lease_ttl_ms,
    1380000,
  );
  assert.throws(
    () =>
      parseV6Arguments([
        "--qualify",
        ...requiredCliArguments(),
        "--task-limit-ms",
        "480001",
        "--lease-ttl-ms",
        "1380001",
      ]),
    /8-minute maximum/,
  );
  assert.throws(
    () =>
      parseV6Arguments([
        "--execute",
        ...requiredCliArguments(),
        "--task-limit-ms",
        "1200000",
        "--lease-ttl-ms",
        "2100000",
      ]),
    /requires --qualification-receipt/,
  );
  const execute = parseV6Arguments([
    "--execute",
    ...requiredCliArguments(),
    "--task-limit-ms",
    "1200000",
    "--lease-ttl-ms",
    "2100000",
    "--qualification-receipt",
    "/evidence/qualification.json",
  ]);
  assert.equal(execute.qualificationReceipt, "/evidence/qualification.json");
});

function gpuReport(implementation) {
  const samples = [{ index: 0 }];
  return {
    provenance: {
      host: {
        hostname: "gpu-host",
        os_release: "6.8.0",
        nvidia_gpu: "NVIDIA H100 PCIe, GPU-abc-123, 570.1, 81559",
        vulkan_summary:
          "vendorID           = 0x10de\ndeviceName         = NVIDIA H100 PCIe",
        display_mode: "Screen 0: 1280 x 960",
      },
    },
    iterations: [
      {
        active_gpu_adapter: {
          receipt_type: "bp-active-renderer-adapter-v1",
          passed: true,
          implementation,
          selection_source:
            implementation === "electron"
              ? "chromium-system-info-active-gl-renderer"
              : "gpui-window-gpu-specs",
          device_uuid: "GPU-abc-123",
        },
        gpu: {
          baseline: { samples },
          run: { samples },
          baseline_adjusted: { samples },
        },
        renderer:
          implementation === "electron"
            ? {
                browser_gpu_info: {
                  gpu: {
                    devices: [
                      {
                        vendorId: 0x10de,
                        deviceString: "NVIDIA H100 PCIe",
                      },
                    ],
                  },
                },
              }
            : null,
      },
    ],
  };
}

test("binds both runner reports and samples to one NVIDIA adapter", () => {
  assert.deepEqual(
    parseNvidiaAdapterIdentityV6(
      "0, NVIDIA H100 PCIe, GPU-abc-123, 570.1, 81559",
      { withIndex: true },
    ),
    {
      index: 0,
      name: "NVIDIA H100 PCIe",
      uuid: "GPU-abc-123",
      driver_version: "570.1",
      memory_total_mib: 81559,
    },
  );
  const electron = gpuReport("electron");
  const gpui = gpuReport("gpui");
  const hash = (value) => createHash("sha256").update(value).digest("hex");
  const binding = buildQualificationEnvironmentBindingV6({
    environmentEvidence: {
      hostname: "gpu-host",
      os_release: "6.8.0",
      display: ":99",
      dbus_session_address_sha256: "d".repeat(64),
      adapter: {
        index: 0,
        name: "NVIDIA H100 PCIe",
        uuid: "GPU-abc-123",
        driver_version: "570.1",
        memory_total_mib: 81559,
      },
      vulkan_summary_sha256: hash(electron.provenance.host.vulkan_summary),
      display_mode_sha256: hash(electron.provenance.host.display_mode),
      session_identity: {
        x_server: {
          pid: 91,
          started_at: "Sun Aug 23 20:00:00 2026",
          command_sha256: "9".repeat(64),
          socket_path: "/tmp/.X11-unix/X99",
        },
        dbus_bus_id: "bus-123",
      },
      raw: {},
    },
    electronReport: electron,
    gpuiReport: gpui,
  });
  assert.equal(binding.adapter.uuid, "GPU-abc-123");
  assert.equal(binding.dbus_session_address_sha256, "d".repeat(64));
});

function commonDamageReceipt() {
  return {
    schema_version: 2,
    boundary_id: "x11-damage-notify-after-xtest-v1",
    input_clock: "CLOCK_MONOTONIC",
    completion_clock: "CLOCK_MONOTONIC",
    completion_signal: "X11-DamageNotify",
    observation_scope: "x11-server-drawable-damage-not-presentation-completion",
    observer_process_independent: true,
    server_observed_drawable_damage: true,
    presentation_completion_observed: false,
    physical_scanout_observed: false,
    passed: true,
    decision_timing_eligible: true,
    temporal_action_binding: true,
    correlation_method:
      "observer-owned-terminal-XTEST-action-to-first-target-DamageNotify-after-damage-reset",
    sample_count: 1,
    input_to_damage_notify_p95_ms: 4.2,
    samples: [
      {
        schema_version: 3,
        observer: "native-x11-damage-observer-v1",
        observer_pid: 220,
        window_id: "1234",
        input_window_id: "1234",
        verified_input_window_id: "1234",
        damage_drawable_id: "1234",
        input_target_relation: "same-window",
        input_api: "XTEST",
        action: "click",
        action_token: "qualification-open-pdf:0",
        action_sequence: 0,
        action_event_count: 1,
        action_position: "terminal",
        correlation_method:
          "observer-owned-terminal-XTEST-action-to-first-target-DamageNotify-after-damage-reset",
        input_clock: "CLOCK_MONOTONIC",
        completion_clock: "CLOCK_MONOTONIC",
        completion_signal: "X11-DamageNotify",
        observation_scope:
          "x11-server-drawable-damage-not-presentation-completion",
        server_observed_drawable_damage: true,
        presentation_completion_observed: false,
        physical_scanout_observed: false,
        target_viewable_before_action: true,
        target_width: 1200,
        target_height: 800,
        damage_extension_major: 1,
        damage_extension_minor: 1,
        damage_report_level: "XDamageReportNonEmpty",
        input_monotonic_ms: 1000,
        action_completed_monotonic_ms: 1000.1,
        damage_notify_received_monotonic_ms: 1004.2,
        input_to_damage_notify_ms: 4.2,
        damage_handle_id: "88",
        damage_server_timestamp: 10,
        damage_area: { x: 1, y: 2, width: 300, height: 200 },
        damage_geometry: { x: 0, y: 0, width: 1200, height: 800 },
        damage_more: false,
        injected_samples: [],
      },
    ],
  };
}

function viewStateReceipt(implementation) {
  const snapshot = (checkpoint, opened) => ({
    checkpoint,
    observation_source: "live-application-render-state",
    live: true,
    window_bounds_window_logical: { x: 0, y: 0, width: 1200, height: 800 },
    viewport_bounds_window_logical: {
      x: 48,
      y: 161,
      width: 1064,
      height: 639,
    },
    display_scale_factor: 1,
    layout_mode: "continuous",
    zoom_mode: "manual",
    zoom_percent: 100,
    left_sidebar: { visible: false, width_logical: 0 },
    right_sidebar: { visible: false, width_logical: 0 },
    active_document: {
      fixture_id: opened ? "bp-single-page-v1" : null,
      tab_index: opened ? 0 : null,
      open_document_count: opened ? 1 : 0,
    },
  });
  const payload = {
    schema_version: 1,
    implementation,
    journey: "small-shell-open",
    component: "open-pdf",
    fixture_ids: ["bp-single-page-v1"],
    snapshots: [
      snapshot("measurement-start", false),
      snapshot("measurement-end", true),
    ],
  };
  return { ...payload, evidence_sha256: canonicalSha256(payload) };
}

function candidateLaunchSeal(launchId, candidates) {
  const payload = {
    schema_version: 1,
    candidate_profile: "bp-perf-v4-optimized-candidate-1",
    launch_id: launchId,
    revalidated_immediately_before_launch: true,
    electron_manifest_sha256: candidates.electron.sha256,
    electron_executable_sha256: candidates.electron.executable.sha256,
    electron_bundle_tree_sha256: candidates.electron.bundle_tree_sha256,
    electron_runtime_dependency_closure_tree_sha256:
      candidates.electron.runtime_dependency_closure_tree_sha256,
    gpui_manifest_sha256: candidates.gpui.sha256,
    gpui_executable_sha256: candidates.gpui.executable.sha256,
    gpui_pdf_worker_sha256: candidates.gpui.pdf_worker.sha256,
    started_monotonic_ms: 10,
    ended_monotonic_ms: 11,
  };
  return { ...payload, evidence_sha256: canonicalSha256(payload) };
}

function passingCgroupAccountingPreflight() {
  return {
    preflight_id: "bp-linux-cgroup-v2-accounting-v1",
    ready: true,
    accounting_scope: "cgroup-v2-child-process-tree",
    required_metrics: ["cpu.stat:usage_usec", "memory.peak"],
    substitution_policy: "no-rss-substitution",
    blockers: [],
    probe: { passed: true, exit_code: 0 },
    metrics: {
      supported: true,
      cpu_seconds: 0.05,
      memory_peak_supported: true,
      memory_peak_bytes: 8 * 1024 * 1024,
    },
    cleanup: { removed: true },
  };
}

test("authenticates qualification evidence and rejects any changed binding", async () => {
  const { workload } = await loadComparisonWorkloadV6();
  const candidateProfile = "bp-perf-v4-optimized-candidate-1";
  const candidates = {
    electron: {
      candidate_profile: candidateProfile,
      sha256: "a".repeat(64),
      executable: { sha256: "1".repeat(64) },
      bundle_tree_sha256: "2".repeat(64),
      runtime_dependency_closure_tree_sha256: "3".repeat(64),
    },
    gpui: {
      candidate_profile: candidateProfile,
      sha256: "b".repeat(64),
      executable: { sha256: "4".repeat(64) },
      pdf_worker: { sha256: "5".repeat(64) },
    },
  };
  const verified = {
    fixtures: { "bp-single-page-v1": { sha256: "c".repeat(64) } },
    references: {},
  };
  const environmentBinding = {
    hostname: "gpu-host",
    os_release: "6.8.0",
    display: ":99",
    dbus_session_address_sha256: "d".repeat(64),
    adapter: {
      index: 0,
      name: "NVIDIA H100 PCIe",
      uuid: "GPU-abc-123",
      driver_version: "570.1",
      memory_total_mib: 81559,
    },
    vulkan_summary_sha256: "e".repeat(64),
    display_mode_sha256: "f".repeat(64),
    session_identity: {
      x_server: {
        pid: 91,
        started_at: "Sun Aug 23 20:00:00 2026",
        command_sha256: "9".repeat(64),
        socket_path: "/tmp/.X11-unix/X99",
      },
      dbus_bus_id: "bus-123",
    },
  };
  const views = [viewStateReceipt("electron"), viewStateReceipt("gpui")];
  const matched = compareViewStateReceiptsV5(...views);
  const launches = ["electron", "gpui"].map((implementation, index) => ({
    implementation,
    journey: "small-shell-open",
    component: "open-pdf",
    passed: true,
    benefit_metrics_eligible: true,
    raw_report_path: `/evidence/${implementation}.json`,
    raw_report_sha256: String(index + 1).repeat(64),
    candidate_manifest_sha256: candidates[implementation].sha256,
    launch_binding: {
      launch_id: `qualification-${implementation}`,
      candidate_manifest_sha256: candidates[implementation].sha256,
      protocol_version: "bp-perf-v6",
      workload_byte_sha256: expectedWorkloadByteSha256V6,
      dbus_session_address_sha256: "d".repeat(64),
      session_identity_sha256: canonicalSha256(
        environmentBinding.session_identity,
      ),
      candidate_prelaunch_seal: candidateLaunchSeal(
        `qualification-${implementation}`,
        candidates,
      ),
    },
    common_benefit_timing_boundary: commonDamageReceipt(),
    gpu_evidence: {
      qualification: { required: true, passed: true },
      adapter_index: 0,
      baseline_sample_count: 5,
      run_sample_count: 3,
      baseline_adjusted_sample_count: 3,
    },
    active_gpu_adapter: {
      receipt_type: "bp-active-renderer-adapter-v1",
      passed: true,
      implementation,
      selection_source:
        implementation === "electron"
          ? "chromium-system-info-active-gl-renderer"
          : "gpui-window-gpu-specs",
      device_uuid: "GPU-abc-123",
    },
    view_state_receipt: views[index],
  }));
  const receipt = buildQualificationReceiptV6({
    workload,
    candidates,
    verified,
    environmentBinding,
    launches,
    matchedViewState: matched,
    leaseBinding: buildImmutableLeaseBindingV6({
      mode: "qualify",
      startedAtMs: Date.parse("2026-08-23T00:00:00.000Z"),
      lease: { maximum_cost_usd: 1.7, launch_count: 2 },
      options: {
        taskLimitMs: 480000,
        cleanupGraceMs: 900000,
        leaseTtlMs: 1380000,
        hourlyUsd: 4.41,
      },
    }),
    cgroupAccountingPreflight: passingCgroupAccountingPreflight(),
    createdAt: "2026-08-23T00:00:00.000Z",
  });
  assert.deepEqual(
    validateQualificationReceiptV6({
      receipt,
      workload,
      candidates,
      verified,
      environmentBinding,
    }),
    { passed: true, payload_sha256: receipt.authentication.payload_sha256 },
  );
  const changed = structuredClone(receipt);
  changed.environment_binding.display = ":100";
  assert.throws(
    () =>
      validateQualificationReceiptV6({
        receipt: changed,
        workload,
        candidates,
        verified,
        environmentBinding,
      }),
    /authentication is invalid/,
  );
  const changedSeal = structuredClone(
    launches[0].launch_binding.candidate_prelaunch_seal,
  );
  changedSeal.electron_executable_sha256 = "0".repeat(64);
  assert.throws(
    () =>
      validateCandidateLaunchSealV6({
        seal: changedSeal,
        launchId: "qualification-electron",
        candidates,
      }),
    /candidate launch seal is invalid/,
  );
});

test("keeps lease deadlines and cost settings immutable", () => {
  const options = {
    taskLimitMs: 480000,
    cleanupGraceMs: 900000,
    leaseTtlMs: 1380000,
    hourlyUsd: 4.41,
  };
  const startedAtMs = Date.parse("2026-08-23T00:00:00.000Z");
  const binding = buildImmutableLeaseBindingV6({
    mode: "qualify",
    startedAtMs,
    lease: { maximum_cost_usd: 1.6905, launch_count: 2 },
    options,
  });
  assert.equal(
    Date.parse(binding.absolute_task_deadline_at),
    startedAtMs + 480000,
  );
  assert.equal(
    validateImmutableLeaseBindingV6(binding, {
      mode: "qualify",
      options,
    }),
    binding,
  );
  assert.throws(
    () =>
      validateImmutableLeaseBindingV6(binding, {
        mode: "qualify",
        options: { ...options, taskLimitMs: 480001 },
      }),
    /invalid or changed/,
  );
});

test("binds the exact X server process start and D-Bus bus ID", async () => {
  assert.equal(
    parseDbusBusIdV6('method return\n   string "bus-123"'),
    "bus-123",
  );
  const execute = async (command, args) => {
    if (command === "fuser") return { stdout: "91\n" };
    if (command === "dbus-send") {
      return { stdout: 'method return\n   string "bus-123"\n' };
    }
    if (command === "ps" && args.includes("lstart=")) {
      return { stdout: "Sun Aug 23 20:00:00 2026\n" };
    }
    if (command === "ps" && args.includes("args=")) {
      return { stdout: "/usr/lib/xorg/Xorg :99\n" };
    }
    throw new Error(`unexpected command ${command}`);
  };
  const identity = await collectSessionIdentityV6({
    environment: {
      DISPLAY: ":99",
      DBUS_SESSION_BUS_ADDRESS: "unix:path=/tmp/bus",
    },
    execute,
  });
  assert.equal(identity.x_server.pid, 91);
  assert.equal(identity.x_server.socket_path, "/tmp/.X11-unix/X99");
  assert.equal(identity.dbus_bus_id, "bus-123");
  assert.equal(
    assertSessionIdentityV6(identity, structuredClone(identity)),
    true,
  );
  assert.throws(
    () =>
      assertSessionIdentityV6(identity, {
        ...identity,
        dbus_bus_id: "bus-456",
      }),
    /identity changed/,
  );
});

test("terminates a timed-out runner process group with bounded TERM and KILL", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-v6-runner-"));
  try {
    const started = Date.now();
    const result = await runRunnerV6(
      {
        argv: ["/bin/sh", "-c", "trap '' TERM; while :; do sleep 1; done"],
        environment: {},
        raw_report_path: resolve(directory, "absent.json"),
        hard_report_path: null,
      },
      100,
      { terminationGraceMs: 25 },
    );
    assert.equal(result.timed_out, true);
    assert.equal(result.termination.process_group, true);
    assert.equal(result.termination.term_sent, true);
    assert.equal(result.termination.kill_sent, true);
    assert.equal(result.termination.bounded_wait_complete, true);
    assert.equal(result.signal, "SIGKILL");
    assert(Date.now() - started < 1000);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("resume reuses only an authenticated group-aligned passed prefix", async () => {
  const { schedule } = await planAndSchedule();
  const firstBenefit = schedule.findIndex(
    ({ phase }) => phase === "calibration",
  );
  const multiLaunchGroupStart = schedule.findIndex(
    (run, index) =>
      index >= firstBenefit && schedule[index + 1]?.bundle_id === run.bundle_id,
  );
  assert(multiLaunchGroupStart > firstBenefit);
  const retainedLaunches = schedule
    .slice(0, multiLaunchGroupStart + 1)
    .map((run) => ({ ...run, passed: true }));
  const completedBenefitRuns = schedule.slice(
    firstBenefit,
    multiLaunchGroupStart,
  );
  const retained = {
    launches: retainedLaunches,
    correctness_reports: schedule
      .slice(0, firstBenefit)
      .map((run) => ({ ...run, passed: true })),
    bundles: [
      ...new Set(
        completedBenefitRuns.map(({ bundle_id: bundleId }) => bundleId),
      ),
    ].map((bundleId) => ({ bundle_id: bundleId })),
  };
  assert.equal(
    reusableSchedulePrefixV6(retained, schedule),
    multiLaunchGroupStart,
    "the first launch of an incomplete multi-launch bundle must be discarded",
  );
});
