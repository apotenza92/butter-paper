import assert from "node:assert/strict";
import test from "node:test";

import {
  calculateFinalPairCount,
  createBalancedPairOrders,
  pairedLogRatioBootstrap,
  sampleLogRatioVariance,
} from "./decision-statistics.mjs";

test("creates deterministic randomized blocks with two AB and two BA pairs", () => {
  const orders = createBalancedPairOrders({ pairCount: 24, seed: 0x4250_5633 });

  assert.deepEqual(orders, createBalancedPairOrders({ pairCount: 24, seed: 0x4250_5633 }));
  assert.notDeepEqual(orders, createBalancedPairOrders({ pairCount: 24, seed: 0x4250_5634 }));
  for (let index = 0; index < orders.length; index += 4) {
    const block = orders.slice(index, index + 4);
    assert.equal(block.filter((order) => order[0] === "electron").length, 2);
    assert.equal(block.filter((order) => order[0] === "gpui").length, 2);
  }
});

test("rejects an incomplete randomized block", () => {
  assert.throws(
    () => createBalancedPairOrders({ pairCount: 26, seed: 1 }),
    /multiple of 4/,
  );
});

test("calculates and clamps the final paired sample size from log-ratio variance", () => {
  assert.deepEqual(calculateFinalPairCount(0), {
    log_ratio_variance: 0,
    raw_pairs: 0,
    final_pairs: 24,
    clamped: "minimum",
  });
  assert.deepEqual(calculateFinalPairCount(0.04), {
    log_ratio_variance: 0.04,
    raw_pairs: 35,
    final_pairs: 36,
    clamped: null,
  });
  assert.deepEqual(calculateFinalPairCount(1), {
    log_ratio_variance: 1,
    raw_pairs: 864,
    final_pairs: 40,
    clamped: "maximum",
  });
});

test("calculates calibration variance in log-ratio space", () => {
  assert.ok(
    Math.abs(sampleLogRatioVariance([Math.exp(-1), 1, Math.exp(1)]) - 1) < 1e-12,
  );
});

test("runs a deterministic 100,000-resample paired log-ratio bootstrap", () => {
  assert.deepEqual(pairedLogRatioBootstrap([0.5, 0.5, 0.5, 0.5]), {
    method: "paired percentile bootstrap of the geometric mean ratio",
    estimate: 0.5,
    samples: 100_000,
    seed: 0x4250_5633,
    lower_95: 0.5,
    upper_95: 0.5,
  });

  const first = pairedLogRatioBootstrap([0.7, 0.8, 0.9, 1], { samples: 1_000, seed: 7 });
  const second = pairedLogRatioBootstrap([0.7, 0.8, 0.9, 1], { samples: 1_000, seed: 7 });
  assert.deepEqual(first, second);
  assert.ok(first.lower_95 < first.estimate);
  assert.ok(first.upper_95 > first.estimate);
});

test("rejects nonpositive paired ratios", () => {
  assert.throws(() => pairedLogRatioBootstrap([1, 0.5, 0]), /positive finite/);
});
