import assert from "node:assert/strict";
import test from "node:test";

import { buildDirectionalSafetyScheduleV7 } from "./directional-safety-v7.mjs";
import {
  buildDirectionalSafetyInvocationV7,
  executeDirectionalSafetyLaunchesV7,
} from "./run-directional-safety-v7.mjs";

function options() {
  return {
    output: "/tmp/bp-v7-safety",
    timeoutMs: 120_000,
    electron: "/candidate/electron",
    gpuiBinary: "/candidate/gpui",
    fixtures: new Map([
      ["bp-engineering-sheet-v1", "/fixtures/engineering.pdf"],
    ]),
    referenceCropDirectory: "/fixtures/reference-crops",
  };
}

test("builds twelve unique existing high-zoom runner invocations", () => {
  const invocations = buildDirectionalSafetyScheduleV7().map((run) =>
    buildDirectionalSafetyInvocationV7(run, options()),
  );
  assert.equal(new Set(invocations.map(({ identity }) => identity)).size, 12);
  assert.equal(
    new Set(invocations.map(({ raw_report_path: path }) => path)).size,
    12,
  );
  for (const invocation of invocations) {
    assert(invocation.argv.includes("high-zoom-pan"));
    assert(invocation.argv.includes("engineering-sheet"));
    assert(
      invocation.argv.includes(
        invocation.implementation === "electron"
          ? "cdp-input-diagnostic"
          : "semantic-diagnostic",
      ),
    );
    assert.equal(
      invocation.argv[invocation.argv.indexOf("--output") + 1],
      invocation.raw_report_path,
    );
  }
});

test("continues after measured product failures but aborts structural evidence failure", async () => {
  const schedule = buildDirectionalSafetyScheduleV7().slice(0, 4);
  const visited = [];
  const completed = await executeDirectionalSafetyLaunchesV7(schedule, {
    runLaunch: async (run) => {
      visited.push(run.schedule_index);
      return {
        pair: run.pair,
        implementation: run.implementation,
        outcome: run.schedule_index === 1 ? "FAIL" : "PASS",
        structural_failures: [],
      };
    },
  });
  assert.deepEqual(visited, [0, 1, 2, 3]);
  assert.equal(completed.length, 4);

  visited.length = 0;
  await assert.rejects(
    executeDirectionalSafetyLaunchesV7(schedule, {
      runLaunch: async (run) => {
        visited.push(run.schedule_index);
        return {
          pair: run.pair,
          implementation: run.implementation,
          outcome: run.schedule_index === 1 ? "ABORT" : "PASS",
          structural_failures:
            run.schedule_index === 1 ? ["raw report is absent"] : [],
        };
      },
    }),
    /ABORTED directional safety structural evidence failure/,
  );
  assert.deepEqual(visited, [0, 1]);
});
