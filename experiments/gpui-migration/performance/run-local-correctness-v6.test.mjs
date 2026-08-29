import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import {
  buildLocalCorrectnessInvocationV6,
  buildLocalCorrectnessScheduleV6,
  executeLocalCorrectnessV6,
  localCorrectnessEvidenceScopeV6,
  localCorrectnessReceiptTypeV6,
  parseLocalCorrectnessArgumentsV6,
  verifyLocalCorrectnessFixturesV6,
} from "./run-local-correctness-v6.mjs";
import {
  buildV6ComparisonPlan,
  loadComparisonWorkloadV6,
} from "./run-paired-v6.mjs";

function cliArguments(output = "/tmp/bp-local-correctness") {
  return [
    "--output",
    output,
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
    "--fixture",
    "nasa-apollo-summary-526-v1=/fixtures/nasa.pdf",
    "--fixture",
    "bp-engineering-sheet-v1=/fixtures/engineering.pdf",
    "--fixture",
    "bp-annotation-density-v1=/fixtures/density.pdf",
    "--fixture",
    "bp-annotation-all-v1=/fixtures/all.pdf",
  ];
}

async function correctnessSchedule() {
  const { workload, byte_sha256: byteSha256 } =
    await loadComparisonWorkloadV6();
  const plan = buildV6ComparisonPlan(workload, byteSha256);
  return { plan, schedule: buildLocalCorrectnessScheduleV6(plan) };
}

test("parses only the local non-decision candidate and four-fixture interface", () => {
  const options = parseLocalCorrectnessArgumentsV6(cliArguments());
  assert.equal(options.output, "/tmp/bp-local-correctness");
  assert.equal(options.fixtures.size, 4);
  assert.equal(options.timeoutMs, 120_000);
  assert.throws(
    () =>
      parseLocalCorrectnessArgumentsV6([
        ...cliArguments(),
        "--qualification-receipt",
        "/paid/receipt.json",
      ]),
    /unknown local correctness option/,
  );
  assert.throws(
    () =>
      parseLocalCorrectnessArgumentsV6([
        ...cliArguments(),
        "--hourly-usd",
        "4.41",
      ]),
    /unknown local correctness option/,
  );
});

test("filters the frozen v6 schedule to exactly 22 semantic and two property launches", async () => {
  const { schedule } = await correctnessSchedule();
  assert.equal(schedule.length, 24);
  assert.equal(
    schedule.filter(({ input_lane: lane }) => lane === "semantic-diagnostic")
      .length,
    22,
  );
  assert.equal(
    schedule.filter(
      ({ component }) => component === "native-property-edit-undo",
    ).length,
    2,
  );
  assert(
    schedule.every(
      ({
        phase,
        inference_eligible: inference,
        benefit_metrics_eligible: benefit,
      }) => phase === "correctness" && inference === false && benefit === false,
    ),
  );
});

test("uses the real runners while explicitly disabling NVIDIA and benefit observation", async () => {
  const { schedule } = await correctnessSchedule();
  const options = parseLocalCorrectnessArgumentsV6(cliArguments());
  const semantic = buildLocalCorrectnessInvocationV6(schedule[0], options);
  assert.match(semantic.argv[1], /gpui-runner\.mjs$|electron-runner\.mjs$/);
  assert.equal(semantic.environment.BP_PERF_REQUIRE_NVIDIA, "0");
  assert.equal(semantic.environment.BP_PERF_COMMON_DAMAGE_OBSERVER, "0");
  assert.equal(semantic.benefit_metrics_eligible, false);
  if (semantic.implementation === "gpui") {
    assert.equal(semantic.environment.GPUI_X11_SCALE_FACTOR, "1");
  }
  const property = buildLocalCorrectnessInvocationV6(
    schedule.find(({ component }) => component === "native-property-edit-undo"),
    options,
  );
  assert(property.argv.includes("--v5-scenario"));
  assert.equal(
    property.hard_report_path.endsWith("-hard-report-v5.json"),
    true,
  );
});

test("validates exactly the four schedule fixtures and their frozen hashes", async () => {
  const { schedule } = await correctnessSchedule();
  const fixtures = new Map([
    ["nasa-apollo-summary-526-v1", "/fixtures/nasa.pdf"],
    ["bp-engineering-sheet-v1", "/fixtures/engineering.pdf"],
    ["bp-annotation-density-v1", "/fixtures/density.pdf"],
    ["bp-annotation-all-v1", "/fixtures/all.pdf"],
  ]);
  const workload = {
    fixtures: [...fixtures].map(([id, path], index) => ({
      id,
      sha256: String(index + 1).repeat(64),
      path,
    })),
  };
  const verified = await verifyLocalCorrectnessFixturesV6({
    workload,
    schedule,
    fixtures,
    readArtifact: async (path) => ({
      path,
      bytes: 10,
      sha256: workload.fixtures.find((fixture) => fixture.path === path).sha256,
    }),
  });
  assert.deepEqual(Object.keys(verified).sort(), [...fixtures.keys()].sort());
  await assert.rejects(
    verifyLocalCorrectnessFixturesV6({
      workload,
      schedule,
      fixtures,
      readArtifact: async (path) => ({
        path,
        bytes: 10,
        sha256: "f".repeat(64),
      }),
    }),
    /fixture SHA-256 mismatch/,
  );
});

test("runs serially, retains raw hashes, and fails immediately on an unexpected result", async () => {
  const directory = await mkdtemp(
    resolve(tmpdir(), "bp-local-correctness-test-"),
  );
  const output = resolve(directory, "output");
  const { plan, schedule } = await correctnessSchedule();
  const options = parseLocalCorrectnessArgumentsV6(cliArguments(output));
  let running = 0;
  let maximumRunning = 0;
  let launches = 0;
  const outerTimeouts = [];
  try {
    await assert.rejects(
      executeLocalCorrectnessV6(
        {
          plan,
          schedule,
          options,
          candidates: {
            electron: { sha256: "a".repeat(64) },
            gpui: { sha256: "b".repeat(64) },
          },
          fixtures: {},
          v4Workload: {},
          v4Plan: {},
          v5Workload: {},
        },
        {
          environment: {
            DISPLAY: ":99",
            DBUS_SESSION_BUS_ADDRESS: "unix:path=/tmp/bus",
          },
          sealCandidate: async () => ({ evidence_sha256: "c".repeat(64) }),
          runRunner: async (invocation, outerTimeoutMs) => {
            outerTimeouts.push(outerTimeoutMs);
            running += 1;
            maximumRunning = Math.max(maximumRunning, running);
            launches += 1;
            await writeFile(
              invocation.raw_report_path,
              `${JSON.stringify({ launch: launches })}\n`,
            );
            running -= 1;
            return {
              exit_code: 0,
              timed_out: false,
              started_at: "2026-08-24T00:00:00.000Z",
              ended_at: "2026-08-24T00:00:01.000Z",
              started_monotonic_ms: launches,
              ended_monotonic_ms: launches + 1,
              report: { launch: launches },
              stdout: "",
              stderr: "",
            };
          },
          assessRun: ({ run }) => ({
            passed: run.schedule_index < 2,
            failures:
              run.schedule_index < 2 ? [] : ["unexpected semantic failure"],
            correctness_passed: run.schedule_index < 2,
            known_baseline_defect_id: null,
            receipts: [],
            hard_report: null,
          }),
        },
      ),
      /unexpected semantic failure/,
    );
    assert.equal(maximumRunning, 1);
    assert.equal(launches, 3);
    assert.deepEqual(outerTimeouts, [135_000, 135_000, 135_000]);
    const manifest = JSON.parse(
      await readFile(resolve(output, "local-correctness-manifest-v6.json")),
    );
    assert.equal(manifest.receipt_type, localCorrectnessReceiptTypeV6);
    assert.equal(manifest.evidence_scope, localCorrectnessEvidenceScopeV6);
    assert.equal(manifest.decision_eligible, false);
    assert.equal(manifest.status, "failed-closed");
    assert.equal(manifest.launches.length, 3);
    assert.match(manifest.launches[0].raw_report_sha256, /^[0-9a-f]{64}$/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("completes all 24 launches while preserving known defects as correctness failures", async () => {
  const directory = await mkdtemp(
    resolve(tmpdir(), "bp-local-correctness-complete-"),
  );
  const output = resolve(directory, "output");
  const { plan, schedule } = await correctnessSchedule();
  const options = parseLocalCorrectnessArgumentsV6(cliArguments(output));
  const defectId = "electron-engineering-zoom-density-and-raster-bound-v1";
  try {
    const manifest = await executeLocalCorrectnessV6(
      {
        plan,
        schedule,
        options,
        candidates: {
          electron: { sha256: "a".repeat(64) },
          gpui: { sha256: "b".repeat(64) },
        },
        fixtures: {},
        v4Workload: {},
        v4Plan: {},
        v5Workload: {},
      },
      {
        environment: {
          DISPLAY: ":99",
          DBUS_SESSION_BUS_ADDRESS: "unix:path=/tmp/bus",
        },
        sealCandidate: async () => ({ evidence_sha256: "c".repeat(64) }),
        runRunner: async (invocation) => {
          await writeFile(invocation.raw_report_path, "{}\n");
          return {
            exit_code: 0,
            timed_out: false,
            started_at: "2026-08-24T00:00:00.000Z",
            ended_at: "2026-08-24T00:00:01.000Z",
            started_monotonic_ms: 1,
            ended_monotonic_ms: 2,
            report: {},
            stdout: "",
            stderr: "",
          };
        },
        assessRun: ({ run }) => {
          const known =
            run.implementation === "electron" &&
            run.journey === "engineering-sheet" &&
            run.component === "zoom";
          return {
            passed: true,
            failures: [],
            correctness_passed: !known,
            known_baseline_defect_id: known ? defectId : null,
            receipts: [],
            hard_report: null,
          };
        },
      },
    );
    assert.equal(manifest.complete, true);
    assert.equal(manifest.status, "completed-with-known-baseline-defects");
    assert.equal(manifest.correctness_passed, false);
    assert.equal(manifest.launches.length, 24);
    assert.deepEqual(manifest.known_baseline_defects, [
      {
        implementation: "electron",
        journey: "engineering-sheet",
        component: "zoom",
        known_baseline_defect_id: defectId,
      },
    ]);
    assert(
      manifest.launches.every(
        ({ benefit_metrics_eligible: eligible }) => eligible === false,
      ),
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
