import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import test from "node:test";

import {
  assessMacosLibprocAccounting,
  runMacosLibprocPreflight,
  waitForChildrenWithDeadline,
} from "./macos-libproc-accounting.mjs";

function processRecord(pid, ppid, start, overrides = {}) {
  return {
    pid,
    ppid,
    start_abstime: start,
    exit_abstime: 0,
    user_ns: 1,
    system_ns: 1,
    child_user_ns: 0,
    child_system_ns: 0,
    phys_footprint_bytes: 100,
    lifetime_max_phys_footprint_bytes: 120,
    ...overrides,
  };
}

function passingInput() {
  return {
    platformSupported: true,
    compilePassed: true,
    helperExitCode: 0,
    probeExitCode: 0,
    invalidHelperLines: [],
    invalidProbeLines: [],
    rootPid: 100,
    childPid: 101,
    decoyPid: 900,
    rootExited: true,
    cleanupComplete: true,
    lifecycleEvents: [
      { type: "child-spawn", token: "worker-1", pid: 101 },
      {
        type: "child-register",
        token: "worker-1",
        pid: 101,
        ppid: 100,
        start_abstime: 11,
      },
      {
        type: "child-final",
        token: "worker-1",
        pid: 101,
        start_abstime: 11,
        user_ns: 17,
        system_ns: 3,
        lifetime_max_phys_footprint_bytes: 80,
      },
      { type: "child-reaped", token: "worker-1", pid: 101, code: 0 },
    ],
    records: [
      {
        type: "sample",
        monotonic_ns: 1_000_000_000,
        processes: [processRecord(100, 1, 10)],
      },
      {
        type: "sample",
        monotonic_ns: 1_010_000_000,
        processes: [
          processRecord(100, 1, 10),
          processRecord(101, 100, 11, {
            phys_footprint_bytes: 60,
            lifetime_max_phys_footprint_bytes: 80,
          }),
        ],
      },
      {
        type: "sample",
        monotonic_ns: 1_020_000_000,
        processes: [
          processRecord(100, 1, 10, {
            child_user_ns: 1,
            child_system_ns: 4,
          }),
        ],
      },
    ],
  };
}

test("keeps simultaneous and lifetime memory accounting distinct", () => {
  const result = assessMacosLibprocAccounting(passingInput());
  assert.equal(result.ready, true);
  assert.equal(result.release_qualified, false);
  assert.equal(result.sampled_simultaneous_tree_phys_footprint_peak_bytes, 160);
  assert.deepEqual(result.per_process_lifetime_max_phys_footprint_bytes, {
    "100:10": 120,
    "101:11": 80,
  });
  assert.equal(result.registered_child_self_cpu_ns, 20);
  assert.deepEqual(result.registered_child_lifetime_max_phys_footprint_bytes, {
    "101:11": 80,
  });
  assert.equal(result.root_cumulative_reaped_child_cpu_ns, 5);
  assert.match(result.limitations[0], /explicitly declared/);
  assert.match(
    result.qualification_blockers[0],
    /packaged-app measurement runner/,
  );
});

test("fails closed when a registered short child is missed", () => {
  const input = passingInput();
  for (const sample of input.records) {
    sample.processes = sample.processes.filter((entry) => entry.pid !== 101);
  }
  const result = assessMacosLibprocAccounting(input);
  assert.equal(result.ready, false);
  assert.ok(
    result.blockers.some((blocker) => blocker.includes("short-lived child")),
  );
});

test("fails closed for missing registration or final self receipt", () => {
  const missingRegistration = passingInput();
  missingRegistration.lifecycleEvents =
    missingRegistration.lifecycleEvents.filter(
      (event) => event.type !== "child-register",
    );
  const registrationResult = assessMacosLibprocAccounting(missingRegistration);
  assert.equal(registrationResult.ready, false);
  assert.ok(
    registrationResult.blockers.some((blocker) =>
      blocker.includes("exactly one registration"),
    ),
  );

  const missingFinal = passingInput();
  missingFinal.lifecycleEvents = missingFinal.lifecycleEvents.filter(
    (event) => event.type !== "child-final",
  );
  const finalResult = assessMacosLibprocAccounting(missingFinal);
  assert.equal(finalResult.ready, false);
  assert.ok(
    finalResult.blockers.some((blocker) =>
      blocker.includes("final self receipt"),
    ),
  );
});

test("fails closed for identity mismatch and an unregistered sampled child", () => {
  const input = passingInput();
  input.lifecycleEvents.find(
    (event) => event.type === "child-final",
  ).start_abstime = 12;
  input.records[1].processes.push(processRecord(102, 100, 13));
  const result = assessMacosLibprocAccounting(input);
  assert.equal(result.ready, false);
  assert.ok(
    result.blockers.some((blocker) => blocker.includes("receipt was invalid")),
  );
  assert.ok(result.blockers.some((blocker) => blocker.includes("lacked")));
});

test("fails closed for a decoy, late sampling, and PID reuse", () => {
  const input = passingInput();
  input.records[1].monotonic_ns = 1_150_000_000;
  input.records[1].processes.push(processRecord(900, 1, 90));
  input.records[2].processes.push(processRecord(100, 1, 99));
  const result = assessMacosLibprocAccounting(input);
  assert.equal(result.ready, false);
  assert.ok(result.blockers.some((blocker) => blocker.includes("decoy")));
  assert.ok(result.blockers.some((blocker) => blocker.includes("sample gap")));
  assert.ok(result.blockers.some((blocker) => blocker.includes("multiple")));
});

test(
  "absolute deadline escalates termination and awaits child reap",
  { skip: process.platform === "win32", timeout: 5_000 },
  async () => {
    const child = spawn(
      process.execPath,
      [
        "-e",
        'process.on("SIGTERM", () => {}); process.stdout.write("ready\\n"); setInterval(() => {}, 1000);',
      ],
      { stdio: ["ignore", "pipe", "ignore"] },
    );
    child.stdout.setEncoding("utf8");
    await new Promise((resolvePromise, rejectPromise) => {
      child.once("error", rejectPromise);
      child.stdout.once("data", resolvePromise);
    });
    const capture = {
      completed: new Promise((resolvePromise) => {
        child.once("error", (error) =>
          resolvePromise({ code: null, signal: null, error: error.message }),
        );
        child.once("close", (code, signal) =>
          resolvePromise({ code, signal, error: null }),
        );
      }),
    };
    const result = await waitForChildrenWithDeadline(
      [{ child, capture }],
      50,
      50,
    );
    assert.equal(result.deadlineExpired, true);
    assert.equal(result.outcomes[0].signal, "SIGKILL");
    assert.throws(() => process.kill(child.pid, 0), { code: "ESRCH" });
  },
);

test(
  "runs the bounded public-libproc lifecycle preflight",
  { skip: process.platform !== "darwin", timeout: 15_000 },
  async () => {
    const result = await runMacosLibprocPreflight();
    assert.equal(result.release_qualified, false);
    assert.equal(result.cleanup.complete, true);
    assert.ok(result.helper.record_count >= 2);
    assert.ok(result.probe.child_pid > 0);
    if (result.ready) {
      assert.ok(result.registered_child_self_cpu_ns > 5_000_000);
      assert.equal(result.registered_child_identities.length, 1);
    } else {
      assert.fail(JSON.stringify(result.blockers));
    }
    assert.match(result.limitations[0], /does not claim arbitrary/);
  },
);
