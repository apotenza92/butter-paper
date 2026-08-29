import assert from "node:assert/strict";
import test from "node:test";

import {
  assessRequiredCgroupV2Accounting,
  cgroupLaunch,
  parseKeyValueCounters,
  preflightRequiredCgroupV2Accounting,
} from "./linux-cgroup.mjs";

test("parses cgroup v2 counters without accepting nonnumeric values", () => {
  assert.deepEqual(
    parseKeyValueCounters("usage_usec 1250000\nnr_periods 3\ninvalid max\n"),
    {
      usage_usec: 1_250_000,
      nr_periods: 3,
    },
  );
});

test("wraps a launch so the application enters its cgroup before exec", () => {
  const launch = cgroupLaunch(
    { supported: true, path: "/sys/fs/cgroup/example" },
    "/opt/app",
    ["--flag", "a value"],
  );
  assert.equal(launch.executable, "/bin/sh");
  assert.deepEqual(launch.args.slice(-4), [
    "/sys/fs/cgroup/example",
    "/opt/app",
    "--flag",
    "a value",
  ]);
});

test("rejects a host that records cgroup CPU time but exposes no memory peak", () => {
  const result = assessRequiredCgroupV2Accounting({
    supported: true,
    cpu_seconds: 0.025,
    user_cpu_seconds: 0.02,
    system_cpu_seconds: 0.005,
    memory_peak_bytes: null,
    memory_peak_supported: false,
    memory_events: { oom: 0, oom_kill: 0 },
  });

  assert.equal(result.ready, false);
  assert.equal(result.accounting_scope, "cgroup-v2-child-process-tree");
  assert.equal(result.substitution_policy, "no-rss-substitution");
  assert.ok(result.blockers.some((blocker) => blocker.includes("memory.peak")));
  assert.ok(
    result.remediation.commands.includes(
      "cat /sys/fs/cgroup/cgroup.controllers",
    ),
  );
});

test("accepts exact cgroup v2 CPU and memory peak accounting", () => {
  const result = assessRequiredCgroupV2Accounting({
    supported: true,
    cpu_seconds: 0.025,
    user_cpu_seconds: 0.02,
    system_cpu_seconds: 0.005,
    memory_peak_bytes: 4_194_304,
    memory_peak_supported: true,
    memory_events: { oom: 0, oom_kill: 0 },
  });

  assert.equal(result.ready, true);
  assert.deepEqual(result.blockers, []);
});

test("runs and removes a disposable probe before approving paid accounting", async () => {
  const calls = [];
  const result = await preflightRequiredCgroupV2Accounting({
    label: "unit-test",
    create: async (label) => {
      calls.push(`create:${label}`);
      return { supported: true, path: "/fake/probe" };
    },
    runProbe: async () => {
      calls.push("probe");
      return { passed: true, exit_code: 0 };
    },
    read: async () => {
      calls.push("read");
      return {
        supported: true,
        cpu_seconds: 0.025,
        user_cpu_seconds: 0.02,
        system_cpu_seconds: 0.005,
        memory_peak_bytes: 4_194_304,
        memory_peak_supported: true,
        memory_events: { oom: 0, oom_kill: 0 },
      };
    },
    remove: async () => {
      calls.push("remove");
      return { removed: true };
    },
  });

  assert.equal(result.ready, true);
  assert.deepEqual(calls, ["create:unit-test", "probe", "read", "remove"]);
});

test("denies paid accounting and still cleans up when child memory.peak is unavailable", async () => {
  let removed = false;
  const result = await preflightRequiredCgroupV2Accounting({
    create: async () => ({ supported: true, path: "/fake/probe" }),
    runProbe: async () => ({ passed: true, exit_code: 0 }),
    read: async () => ({
      supported: true,
      cpu_seconds: 0.025,
      user_cpu_seconds: 0.02,
      system_cpu_seconds: 0.005,
      memory_peak_bytes: null,
      memory_peak_supported: false,
      memory_events: { oom: 0, oom_kill: 0 },
    }),
    remove: async () => {
      removed = true;
      return { removed: true };
    },
  });

  assert.equal(result.ready, false);
  assert.equal(removed, true);
  assert.ok(result.blockers.some((blocker) => blocker.includes("memory.peak")));
});
