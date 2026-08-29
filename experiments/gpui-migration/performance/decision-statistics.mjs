const minimumFinalPairs = 24;
const maximumFinalPairs = 40;
const pairBlockSize = 4;
const normalCritical95 = 1.96;
const normalCriticalPower80 = 0.84;
const minimumDetectableRatio = 1.10;

function seededRandom(seed) {
  if (!Number.isInteger(seed)) throw new Error("seed must be an integer");
  let state = seed >>> 0;
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 0x1_0000_0000;
  };
}

function shuffledBlock(random) {
  const block = [
    ["electron", "gpui"],
    ["electron", "gpui"],
    ["gpui", "electron"],
    ["gpui", "electron"],
  ];
  for (let index = block.length - 1; index > 0; index -= 1) {
    const swapIndex = Math.floor(random() * (index + 1));
    [block[index], block[swapIndex]] = [block[swapIndex], block[index]];
  }
  return block;
}

export function createBalancedPairOrders({ pairCount, seed }) {
  if (!Number.isInteger(pairCount) || pairCount < pairBlockSize) {
    throw new Error("pairCount must be an integer of at least 4");
  }
  if (pairCount % pairBlockSize !== 0) {
    throw new Error("pairCount must be a multiple of 4");
  }
  const random = seededRandom(seed);
  return Array.from({ length: pairCount / pairBlockSize }, () => shuffledBlock(random)).flat();
}

export function calculateFinalPairCount(logRatioVariance) {
  if (!Number.isFinite(logRatioVariance) || logRatioVariance < 0) {
    throw new Error("log-ratio variance must be a finite nonnegative number");
  }
  const rawPairs = Math.ceil(
    ((normalCritical95 + normalCriticalPower80) ** 2 * logRatioVariance)
      / Math.log(minimumDetectableRatio) ** 2,
  );
  if (rawPairs <= minimumFinalPairs) {
    return {
      log_ratio_variance: logRatioVariance,
      raw_pairs: rawPairs,
      final_pairs: minimumFinalPairs,
      clamped: "minimum",
    };
  }
  const blockRoundedPairs = Math.ceil(rawPairs / pairBlockSize) * pairBlockSize;
  if (blockRoundedPairs > maximumFinalPairs) {
    return {
      log_ratio_variance: logRatioVariance,
      raw_pairs: rawPairs,
      final_pairs: maximumFinalPairs,
      clamped: "maximum",
    };
  }
  return {
    log_ratio_variance: logRatioVariance,
    raw_pairs: rawPairs,
    final_pairs: blockRoundedPairs,
    clamped: null,
  };
}

function positiveLogRatios(ratios) {
  if (!Array.isArray(ratios) || ratios.length === 0) {
    throw new Error("paired ratios must be a nonempty array");
  }
  if (ratios.some((ratio) => !Number.isFinite(ratio) || ratio <= 0)) {
    throw new Error("paired ratios must contain only positive finite numbers");
  }
  return ratios.map(Math.log);
}

function mean(values) {
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function percentile(values, fraction) {
  const ordered = [...values].sort((left, right) => left - right);
  const position = (ordered.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  return lower === upper
    ? ordered[lower]
    : ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower);
}

export function sampleLogRatioVariance(ratios) {
  const logs = positiveLogRatios(ratios);
  if (logs.length < 2) throw new Error("at least two paired ratios are required");
  const average = mean(logs);
  return logs.reduce((sum, value) => sum + (value - average) ** 2, 0) / (logs.length - 1);
}

export function pairedLogRatioBootstrap(
  ratios,
  { samples = 100_000, seed = 0x4250_5633 } = {},
) {
  const logs = positiveLogRatios(ratios);
  if (!Number.isInteger(samples) || samples < 1) {
    throw new Error("bootstrap samples must be a positive integer");
  }
  const random = seededRandom(seed);
  const estimates = new Array(samples);
  for (let sample = 0; sample < samples; sample += 1) {
    let logSum = 0;
    for (let index = 0; index < logs.length; index += 1) {
      logSum += logs[Math.floor(random() * logs.length)];
    }
    estimates[sample] = Math.exp(logSum / logs.length);
  }
  return {
    method: "paired percentile bootstrap of the geometric mean ratio",
    estimate: Math.exp(mean(logs)),
    samples,
    seed,
    lower_95: percentile(estimates, 0.025),
    upper_95: percentile(estimates, 0.975),
  };
}
