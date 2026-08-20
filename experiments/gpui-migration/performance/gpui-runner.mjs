#!/usr/bin/env node

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, readdir, rm, stat, writeFile } from "node:fs/promises";
import { hostname, arch, cpus, freemem, platform, release, totalmem, type } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const performanceDirectory = dirname(fileURLToPath(import.meta.url));
const migrationDirectory = dirname(performanceDirectory);
const repositoryDirectory = resolve(performanceDirectory, "../../..");
const defaultBinary = resolve(
  migrationDirectory,
  "gpui-gallery/target/Butter Paper GPUI.app/Contents/MacOS/ButterPaperGPUI",
);
const sampleIntervalMs = 100;
const defaultTimeoutMs = 120_000;
const stderrLimitBytes = 1_000_000;

function usage() {
  return `Usage:
  node gpui-runner.mjs --scenario <name> --pdf <file> [options]

Required:
  --scenario <name>       GPUI deterministic performance scenario
  --pdf <file>            PDF opened by each application process

Options:
  --iterations <count>    Independent process runs (default: 3)
  --output <file>         JSON report path (default: beside this runner)
  --timeout-ms <ms>       Timeout for each iteration (default: 120000)
  --binary <file>         Override the built GPUI executable
  -h, --help              Show this help
`;
}

function fail(message) {
  process.stderr.write(`gpui-runner: ${message}\n\n${usage()}`);
  process.exitCode = 2;
}

function parsePositiveInteger(value, option) {
  if (!/^\d+$/.test(value ?? "") || Number(value) < 1) {
    throw new Error(`${option} must be a positive integer`);
  }
  return Number(value);
}

function parseArguments(argv) {
  const options = {
    iterations: 3,
    timeoutMs: defaultTimeoutMs,
    binary: defaultBinary,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "-h" || option === "--help") {
      options.help = true;
      continue;
    }
    const valueOptions = new Set([
      "--scenario",
      "--pdf",
      "--iterations",
      "--output",
      "--timeout-ms",
      "--binary",
    ]);
    if (!valueOptions.has(option)) {
      throw new Error(`unknown option: ${option}`);
    }
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`${option} requires a value`);
    }
    index += 1;
    if (option === "--scenario") options.scenario = value;
    if (option === "--pdf") options.pdf = resolve(value);
    if (option === "--iterations") {
      options.iterations = parsePositiveInteger(value, option);
    }
    if (option === "--output") options.output = resolve(value);
    if (option === "--timeout-ms") {
      options.timeoutMs = parsePositiveInteger(value, option);
    }
    if (option === "--binary") options.binary = resolve(value);
  }

  if (options.help) return options;
  if (!options.scenario) throw new Error("--scenario is required");
  if (!options.pdf) throw new Error("--pdf is required");
  if (!options.output) {
    const safeScenario = options.scenario.replace(/[^a-zA-Z0-9._-]+/g, "-");
    options.output = resolve(performanceDirectory, `gpui-${safeScenario}.json`);
  }
  return options;
}

async function fileProvenance(path) {
  const metadata = await stat(path);
  if (!metadata.isFile()) throw new Error(`not a file: ${path}`);
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const input = createReadStream(path);
    input.on("data", (chunk) => hash.update(chunk));
    input.on("error", rejectPromise);
    input.on("end", resolvePromise);
  });
  return {
    path,
    bytes: metadata.size,
    sha256: hash.digest("hex"),
    modified_at: metadata.mtime.toISOString(),
  };
}

async function optionalCommand(command, args, cwd) {
  try {
    const { stdout } = await execFileAsync(command, args, {
      encoding: "utf8",
      cwd,
      timeout: 5_000,
      maxBuffer: 256_000,
    });
    return stdout.trim() || null;
  } catch {
    return null;
  }
}

async function collectProvenance(binary) {
  const cpuList = cpus();
  const [macosVersion, gitRevision] = await Promise.all([
    optionalCommand("/usr/bin/sw_vers", ["-productVersion"]),
    optionalCommand("/usr/bin/git", ["rev-parse", "HEAD"], repositoryDirectory),
  ]);
  return {
    captured_at: new Date().toISOString(),
    host: {
      hostname: hostname(),
      os_type: type(),
      platform: platform(),
      os_release: release(),
      macos_version: macosVersion,
      architecture: arch(),
      logical_cpu_count: cpuList.length,
      cpu_model: cpuList[0]?.model ?? null,
      total_memory_bytes: totalmem(),
      free_memory_bytes_at_start: freemem(),
    },
    runtime: {
      runner: "gpui-runner.mjs",
      node: process.version,
      node_versions: process.versions,
      sample_interval_ms: sampleIntervalMs,
      git_revision: gitRevision,
      binary: await fileProvenance(binary),
    },
  };
}

function percentile(values, fraction) {
  if (values.length === 0) return null;
  const ordered = [...values].sort((left, right) => left - right);
  const position = (ordered.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) return ordered[lower];
  return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower);
}

function numericSummary(values) {
  const valid = values.filter(Number.isFinite);
  if (valid.length === 0) return null;
  const total = valid.reduce((sum, value) => sum + value, 0);
  return {
    count: valid.length,
    min: Math.min(...valid),
    median: percentile(valid, 0.5),
    mean: total / valid.length,
    p95: percentile(valid, 0.95),
    max: Math.max(...valid),
  };
}

async function directorySummary(path) {
  let fileCount = 0;
  let bytes = 0;
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const entryPath = resolve(directory, entry.name);
      if (entry.isDirectory()) await visit(entryPath);
      if (entry.isFile()) {
        const metadata = await stat(entryPath);
        fileCount += 1;
        bytes += metadata.size;
      }
    }
  }
  await visit(path);
  return { file_count: fileCount, bytes };
}

async function sampleProcessTree(rootPid) {
  const { stdout } = await execFileAsync(
    "/bin/ps",
    ["-axo", "pid=,ppid=,%cpu=,rss="],
    { encoding: "utf8", timeout: 2_000, maxBuffer: 4_000_000 },
  );
  const processes = stdout
    .split("\n")
    .map((line) => line.trim().split(/\s+/).map(Number))
    .filter(
      ([pid, parentPid, cpu, rss]) =>
        Number.isInteger(pid) &&
        Number.isInteger(parentPid) &&
        Number.isFinite(cpu) &&
        Number.isFinite(rss),
    )
    .map(([pid, parentPid, cpuPercent, rssKb]) => ({
      pid,
      parentPid,
      cpuPercent,
      rssKb,
    }));

  const included = new Set([rootPid]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const processEntry of processes) {
      if (included.has(processEntry.parentPid) && !included.has(processEntry.pid)) {
        included.add(processEntry.pid);
        changed = true;
      }
    }
  }
  const tree = processes.filter((entry) => included.has(entry.pid));
  if (tree.length === 0) return null;
  return {
    process_count: tree.length,
    cpu_percent: tree.reduce((sum, entry) => sum + entry.cpuPercent, 0),
    rss_kb: tree.reduce((sum, entry) => sum + entry.rssKb, 0),
    pids: tree.map((entry) => entry.pid),
  };
}

function terminateProcessGroup(pid, signal) {
  try {
    process.kill(-pid, signal);
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
}

async function runIteration(options, iteration) {
  const startedAt = new Date();
  const startedMonotonic = process.hrtime.bigint();
  const events = [];
  const invalidStdout = [];
  const samples = [];
  let stdoutBuffer = "";
  let stderr = "";
  let sampling = true;
  let sampleInProgress = false;
  let timedOut = false;
  const cacheDirectory = resolve(
    dirname(options.output),
    ".gpui-cold-cache",
    `${Date.now()}-${process.pid}-${iteration}`,
  );
  await mkdir(cacheDirectory, { recursive: true });

  // AppKit can restore a stale crash dialog before GPUI reaches its first
  // window. Keep this benchmark launch deterministic. The GPUI gallery
  // filters the two NSUserDefaults arguments before treating the remaining
  // arguments as document paths.
  const child = spawn(options.binary, [
    "-ApplePersistenceIgnoreState",
    "YES",
    options.pdf,
  ], {
    env: {
      ...process.env,
      BP_GPUI_PERF_SCENARIO: options.scenario,
      BP_GPUI_PERF_ITERATION: String(iteration),
      BP_GPUI_CACHE_DIR: cacheDirectory,
    },
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const outcomePromise = new Promise((resolvePromise) => {
    child.once("error", (error) => resolvePromise({ spawn_error: error.message }));
    child.once("close", (code, signal) => resolvePromise({ exit_code: code, signal }));
  });

  const parseLine = (line) => {
    if (!line.trim()) return;
    try {
      const event = JSON.parse(line);
      if (
        event?.schema_version !== 1 ||
        typeof event.event !== "string" ||
        !Number.isFinite(event.t_ms)
      ) {
        throw new Error("expected schema_version=1, event string, and numeric t_ms");
      }
      events.push(event);
    } catch (error) {
      invalidStdout.push({ line, error: error.message });
    }
  };

  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    stdoutBuffer += chunk;
    const lines = stdoutBuffer.split("\n");
    stdoutBuffer = lines.pop() ?? "";
    for (const line of lines) parseLine(line);
  });
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => {
    if (stderr.length < stderrLimitBytes) {
      stderr += chunk.slice(0, stderrLimitBytes - stderr.length);
    }
  });

  const sample = async () => {
    if (!sampling || sampleInProgress) return;
    sampleInProgress = true;
    try {
      const snapshot = await sampleProcessTree(child.pid);
      if (snapshot) {
        samples.push({
          elapsed_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
          ...snapshot,
        });
      }
    } catch (error) {
      samples.push({
        elapsed_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
        sample_error: error.message,
      });
    } finally {
      sampleInProgress = false;
    }
  };

  await sample();
  const sampleTimer = setInterval(sample, sampleIntervalMs);
  sampleTimer.unref();

  let forceKillTimer;
  const timeoutTimer = setTimeout(() => {
    timedOut = true;
    terminateProcessGroup(child.pid, "SIGTERM");
    forceKillTimer = setTimeout(() => terminateProcessGroup(child.pid, "SIGKILL"), 2_000);
    forceKillTimer.unref();
  }, options.timeoutMs);
  timeoutTimer.unref();

  const outcome = await outcomePromise;
  clearTimeout(timeoutTimer);
  if (forceKillTimer) clearTimeout(forceKillTimer);
  clearInterval(sampleTimer);
  sampling = false;
  while (sampleInProgress) {
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 5));
  }
  if (stdoutBuffer.trim()) parseLine(stdoutBuffer);

  const endedAt = new Date();
  const elapsedMs = Number(process.hrtime.bigint() - startedMonotonic) / 1e6;
  const validSamples = samples.filter((entry) => !entry.sample_error);
  const cache = await directorySummary(cacheDirectory);
  await rm(cacheDirectory, { recursive: true, force: true });
  const success =
    !timedOut &&
    outcome.exit_code === 0 &&
    !outcome.spawn_error &&
    invalidStdout.length === 0 &&
    events.some((event) => event.event === "scenario-complete");

  return {
    iteration,
    started_at: startedAt.toISOString(),
    ended_at: endedAt.toISOString(),
    wall_duration_ms: elapsedMs,
    success,
    timed_out: timedOut,
    ...outcome,
    events,
    invalid_stdout: invalidStdout,
    stderr,
    cache,
    samples,
    resource_summary: {
      sample_count: validSamples.length,
      cpu_percent: numericSummary(validSamples.map((entry) => entry.cpu_percent)),
      rss_kb: numericSummary(validSamples.map((entry) => entry.rss_kb)),
      process_count: numericSummary(validSamples.map((entry) => entry.process_count)),
    },
  };
}

function summarizeDurationEvents(iterations) {
  const byEvent = new Map();
  for (const iteration of iterations) {
    for (const event of iteration.events) {
      if (!Number.isFinite(event.duration_ms)) continue;
      if (!byEvent.has(event.event)) byEvent.set(event.event, []);
      byEvent.get(event.event).push(event.duration_ms);
    }
  }
  return Object.fromEntries(
    [...byEvent.entries()].sort(([left], [right]) => left.localeCompare(right)).map(
      ([event, durations]) => [event, numericSummary(durations)],
    ),
  );
}

function summarizeEventTimestamps(iterations) {
  const byEvent = new Map();
  for (const iteration of iterations) {
    for (const event of iteration.events) {
      if (!byEvent.has(event.event)) byEvent.set(event.event, []);
      byEvent.get(event.event).push(event.t_ms);
    }
  }
  return Object.fromEntries(
    [...byEvent.entries()].sort(([left], [right]) => left.localeCompare(right)).map(
      ([event, timestamps]) => [event, numericSummary(timestamps)],
    ),
  );
}

function summarizeReport(iterations) {
  const allSamples = iterations.flatMap((iteration) =>
    iteration.samples.filter((sample) => !sample.sample_error),
  );
  const eventTimes = iterations.flatMap((iteration) =>
    iteration.events.map((event) => event.t_ms),
  );
  const frameIntervals = iterations.flatMap((iteration) =>
    iteration.events
      .filter((event) => event.event === "frame" && Number.isFinite(event.interval_ms))
      .map((event) => event.interval_ms),
  );
  return {
    successful_iterations: iterations.filter((iteration) => iteration.success).length,
    failed_iterations: iterations.filter((iteration) => !iteration.success).length,
    wall_duration_ms: numericSummary(iterations.map((iteration) => iteration.wall_duration_ms)),
    event_t_ms: numericSummary(eventTimes),
    event_timestamps_ms: summarizeEventTimestamps(iterations),
    duration_events_ms: summarizeDurationEvents(iterations),
    frame_intervals_ms: numericSummary(frameIntervals),
    frame_interval_thresholds: {
      over_8_33_ms: frameIntervals.filter((value) => value > 8.33).length,
      over_16_67_ms: frameIntervals.filter((value) => value > 16.67).length,
      over_33_33_ms: frameIntervals.filter((value) => value > 33.33).length,
    },
    cache: {
      files: numericSummary(iterations.map((iteration) => iteration.cache.file_count)),
      bytes: numericSummary(iterations.map((iteration) => iteration.cache.bytes)),
    },
    process_tree: {
      cpu_percent: numericSummary(allSamples.map((sample) => sample.cpu_percent)),
      rss_kb: numericSummary(allSamples.map((sample) => sample.rss_kb)),
      peak_cpu_percent:
        allSamples.length > 0 ? Math.max(...allSamples.map((sample) => sample.cpu_percent)) : null,
      median_cpu_percent: percentile(
        allSamples.map((sample) => sample.cpu_percent),
        0.5,
      ),
      peak_rss_kb:
        allSamples.length > 0 ? Math.max(...allSamples.map((sample) => sample.rss_kb)) : null,
      median_rss_kb: percentile(
        allSamples.map((sample) => sample.rss_kb),
        0.5,
      ),
    },
  };
}

async function main() {
  let options;
  try {
    options = parseArguments(process.argv.slice(2));
  } catch (error) {
    fail(error.message);
    return;
  }
  if (options.help) {
    process.stdout.write(usage());
    return;
  }

  let pdf;
  let provenance;
  try {
    [pdf, provenance] = await Promise.all([
      fileProvenance(options.pdf),
      collectProvenance(options.binary),
    ]);
  } catch (error) {
    fail(error.message);
    return;
  }

  const iterations = [];
  for (let iteration = 1; iteration <= options.iterations; iteration += 1) {
    process.stderr.write(
      `GPUI ${options.scenario}: iteration ${iteration}/${options.iterations}\n`,
    );
    iterations.push(await runIteration(options, iteration));
  }

  const report = {
    schema_version: 1,
    implementation: "gpui",
    scenario: options.scenario,
    requested_iterations: options.iterations,
    timeout_ms_per_iteration: options.timeoutMs,
    pdf,
    provenance,
    summary: summarizeReport(iterations),
    iterations,
  };
  await mkdir(dirname(options.output), { recursive: true });
  await writeFile(options.output, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  process.stderr.write(`Wrote ${options.output}\n`);
  if (report.summary.failed_iterations > 0) process.exitCode = 1;
}

await main();
