#!/usr/bin/env node

import { spawn, execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, mkdtemp, rm, stat, writeFile } from "node:fs/promises";
import { arch, cpus, freemem, hostname, platform, release, tmpdir, totalmem, type } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const performanceDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryDirectory = resolve(performanceDirectory, "../../..");
const defaultElectron = resolve(
  repositoryDirectory,
  "node_modules/electron/dist/Electron.app/Contents/MacOS/Electron",
);
const allowedScenarios = new Set(["open-pdf", "page-navigation", "zoom"]);
const pageSequence = [935, 75, 674, 234, 842, 468, 11, 896, 309, 1];
const zoomSequence = [100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100];
const sampleIntervalMs = 100;
const defaultTimeoutMs = 120_000;
const outputLimitBytes = 1_000_000;

function usage() {
  return `Usage:
  node electron-runner.mjs --scenario <name> --pdf <file> [options]

Required:
  --scenario <name>       open-pdf, page-navigation, or zoom
  --pdf <file>            Exact PDF opened in each isolated Electron process

Options:
  --iterations <count>    Independent process runs (default: 3)
  --output <file>         JSON report path (default: beside this runner)
  --timeout-ms <ms>       Timeout for each iteration (default: 120000)
  --electron <file>       Override the Electron executable
  -h, --help              Show this help
`;
}

function parsePositiveInteger(value, option) {
  if (!/^\d+$/.test(value ?? "") || Number(value) < 1) {
    throw new Error(`${option} must be a positive integer`);
  }
  return Number(value);
}

function parseArguments(argv) {
  const options = { iterations: 3, timeoutMs: defaultTimeoutMs, electron: defaultElectron };
  const valueOptions = new Set([
    "--scenario",
    "--pdf",
    "--iterations",
    "--output",
    "--timeout-ms",
    "--electron",
  ]);
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "-h" || option === "--help") {
      options.help = true;
      continue;
    }
    if (!valueOptions.has(option)) throw new Error(`unknown option: ${option}`);
    const value = argv[++index];
    if (value === undefined || value.startsWith("--")) throw new Error(`${option} requires a value`);
    if (option === "--scenario") options.scenario = value;
    if (option === "--pdf") options.pdf = resolve(value);
    if (option === "--iterations") options.iterations = parsePositiveInteger(value, option);
    if (option === "--output") options.output = resolve(value);
    if (option === "--timeout-ms") options.timeoutMs = parsePositiveInteger(value, option);
    if (option === "--electron") options.electron = resolve(value);
  }
  if (options.help) return options;
  if (!allowedScenarios.has(options.scenario)) {
    throw new Error(`--scenario must be one of ${[...allowedScenarios].join(", ")}`);
  }
  if (!options.pdf) throw new Error("--pdf is required");
  options.output ??= resolve(performanceDirectory, `electron-${options.scenario}.json`);
  return options;
}

async function fileProvenance(path) {
  const metadata = await stat(path);
  if (!metadata.isFile()) throw new Error(`not a file: ${path}`);
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", rejectPromise);
    stream.on("end", resolvePromise);
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
      cwd,
      encoding: "utf8",
      timeout: 5_000,
      maxBuffer: 256_000,
    });
    return stdout.trim() || null;
  } catch {
    return null;
  }
}

async function collectProvenance(electron) {
  const cpuList = cpus();
  const [macosVersion, gitRevision, gitStatus] = await Promise.all([
    optionalCommand("/usr/bin/sw_vers", ["-productVersion"]),
    optionalCommand("/usr/bin/git", ["rev-parse", "HEAD"], repositoryDirectory),
    optionalCommand("/usr/bin/git", ["status", "--porcelain=v1", "--untracked-files=all"], repositoryDirectory),
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
      runner: "electron-runner.mjs",
      node: process.version,
      node_versions: process.versions,
      sample_interval_ms: sampleIntervalMs,
      git_revision: gitRevision,
      git_status_sha256: createHash("sha256").update(gitStatus ?? "").digest("hex"),
      electron: await fileProvenance(electron),
    },
  };
}

function percentile(values, fraction) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const position = (sorted.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  return lower === upper
    ? sorted[lower]
    : sorted[lower] + (sorted[upper] - sorted[lower]) * (position - lower);
}

function numericSummary(values) {
  const valid = values.filter(Number.isFinite);
  if (valid.length === 0) return null;
  return {
    count: valid.length,
    min: Math.min(...valid),
    median: percentile(valid, 0.5),
    mean: valid.reduce((sum, value) => sum + value, 0) / valid.length,
    p95: percentile(valid, 0.95),
    max: Math.max(...valid),
  };
}

function summarizeFrames(intervals) {
  return {
    interval_ms: numericSummary(intervals),
    over_8_33_ms: intervals.filter((value) => value > 8.33).length,
    over_16_67_ms: intervals.filter((value) => value > 16.67).length,
    over_33_33_ms: intervals.filter((value) => value > 33.33).length,
  };
}

async function availablePort() {
  const configured = Number(process.env.BP_ELECTRON_CDP_PORT);
  if (Number.isInteger(configured) && configured > 0 && configured < 65536) {
    return configured;
  }
  // The runner executes sequentially. Avoid a bind probe because restricted
  // Codex sandboxes can deny listen(2) even though an Electron child can use
  // its own loopback CDP listener.
  return 42000 + (process.pid % 1000);
}

async function waitForTarget(port, child, output, deadlineMs) {
  const deadline = performance.now() + deadlineMs;
  while (performance.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`Electron exited before CDP was ready (${child.exitCode}).\n${output}`);
    }
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      if (response.ok) {
        const targets = await response.json();
        const page = targets.find((target) => target.type === "page");
        if (page?.webSocketDebuggerUrl) return page;
      }
    } catch {
      // Electron has not opened the debugging endpoint yet.
    }
    await delay(20);
  }
  throw new Error(`Timed out waiting for Electron CDP on port ${port}.\n${output}`);
}

async function createCdpClient(url) {
  const socket = new WebSocket(url);
  const pending = new Map();
  let nextId = 1;
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(String(event.data));
    if (!message.id) return;
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) request.reject(new Error(JSON.stringify(message.error)));
    else request.resolve(message.result);
  });
  await new Promise((resolvePromise, rejectPromise) => {
    socket.addEventListener("open", resolvePromise, { once: true });
    socket.addEventListener("error", rejectPromise, { once: true });
  });
  return {
    send(method, params = {}) {
      const id = nextId++;
      socket.send(JSON.stringify({ id, method, params }));
      return new Promise((resolvePromise, rejectPromise) =>
        pending.set(id, { resolve: resolvePromise, reject: rejectPromise }),
      );
    },
    async evaluate(expression) {
      const result = await this.send("Runtime.evaluate", {
        expression,
        returnByValue: true,
        awaitPromise: true,
      });
      if (result.exceptionDetails) {
        throw new Error(result.exceptionDetails.exception?.description ?? "Runtime evaluation failed");
      }
      return result.result.value;
    },
    close() {
      socket.close();
    },
  };
}

async function sampleProcessTree(rootPid) {
  const { stdout } = await execFileAsync("/bin/ps", ["-axo", "pid=,ppid=,%cpu=,rss="], {
    encoding: "utf8",
    timeout: 2_000,
    maxBuffer: 4_000_000,
  });
  const processes = stdout
    .split("\n")
    .map((line) => line.trim().split(/\s+/).map(Number))
    .filter(([pid, ppid, cpu, rss]) =>
      Number.isInteger(pid) && Number.isInteger(ppid) && Number.isFinite(cpu) && Number.isFinite(rss),
    )
    .map(([pid, parentPid, cpuPercent, rssKb]) => ({ pid, parentPid, cpuPercent, rssKb }));
  const included = new Set([rootPid]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const entry of processes) {
      if (included.has(entry.parentPid) && !included.has(entry.pid)) {
        included.add(entry.pid);
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

async function waitForExit(child, timeoutMs = 3_000) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  const exited = await Promise.race([
    new Promise((resolvePromise) => child.once("exit", () => resolvePromise(true))),
    delay(timeoutMs).then(() => false),
  ]);
  if (!exited) {
    terminateProcessGroup(child.pid, "SIGKILL");
    await new Promise((resolvePromise) => child.once("exit", resolvePromise));
  }
}

async function runRendererScenario(cdp, options, event) {
  await cdp.evaluate(`(async () => {
    const deadline = performance.now() + 20000;
    while (performance.now() < deadline) {
      if (window.__butterPaperTestHooks && document.querySelector('[data-testid="app-root"]')) return;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    throw new Error('Butter Paper test hooks did not become ready');
  })()`);
  const identity = await cdp.evaluate(`(async () => ({
    title: document.title,
    href: location.href,
    has_app_root: Boolean(document.querySelector('[data-testid="app-root"]')),
    metadata: await window.butterPaper.application.getMetadata(),
  }))()`);
  if (!identity.has_app_root || identity.metadata?.development !== true) {
    throw new Error(`CDP target is not the Butter Paper development renderer: ${JSON.stringify(identity)}`);
  }
  await cdp.evaluate(`window.__butterPaperTestHooks.setWindowBounds({ width: 1200, height: 800 })`);
  event("shell-ready", { identity });

  await cdp.evaluate(`(() => {
    window.__electronPerfFrames = [];
    window.__electronPerfFrameActive = true;
    let previous;
    const sample = (time) => {
      if (!window.__electronPerfFrameActive) return;
      if (previous !== undefined) window.__electronPerfFrames.push(time - previous);
      previous = time;
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  })()`);

  const openStarted = performance.now();
  event("pdf-open-requested");
  await cdp.evaluate(`window.__butterPaperTestHooks.openDocumentPath(${JSON.stringify(options.pdf)})`);
  const ready = await cdp.evaluate(`(async () => {
    const deadline = performance.now() + ${Math.min(options.timeoutMs, 90_000)};
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      if (diagnostics.documentPath && diagnostics.pageRenderReady) return diagnostics;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    throw new Error('Timed out waiting for pageRenderReady');
  })()`);
  await cdp.evaluate(`new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`);
  event("first-page-visible", {
    duration_ms: performance.now() - openStarted,
    page_count: ready.pageCount,
  });

  if (options.scenario === "page-navigation") {
    if (ready.pageCount < Math.max(...pageSequence)) {
      throw new Error(`page-navigation requires at least 935 pages; PDF has ${ready.pageCount}`);
    }
    for (const pageNumber of pageSequence) {
      const started = performance.now();
      const result = await cdp.evaluate(`(async () => {
        const pageNumber = ${pageNumber};
        window.__butterPaperTestHooks.resetPerfSnapshot();
        const list = document.querySelector('[data-testid="page-thumbnail-list"]');
        if (!list) throw new Error('Thumbnail list is unavailable');
        const fraction = (pageNumber - 1) / Math.max(1, window.__butterPaperTestHooks.getDiagnostics().pageCount - 1);
        list.scrollTop = fraction * Math.max(0, list.scrollHeight - list.clientHeight);
        list.dispatchEvent(new Event('scroll', { bubbles: true }));
        const deadline = performance.now() + 5000;
        let button;
        while (performance.now() < deadline) {
          button = document.querySelector('[data-testid="page-thumbnail-select-' + pageNumber + '"]');
          if (button) break;
          await new Promise((resolve) => requestAnimationFrame(resolve));
        }
        if (!button) throw new Error('Page ' + pageNumber + ' thumbnail did not virtualize');
        button.click();
        const settleDeadline = performance.now() + 10000;
        while (performance.now() < settleDeadline) {
          const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
          const perf = window.__butterPaperTestHooks.getPerfSnapshot();
          const raster = perf.renderPage;
          const rasterResolved = raster.completed + raster.hits > 0;
          const targetVisible = perf.pageImageVisibility[String(pageNumber - 1)]?.firstVisibleMs != null;
          if (diagnostics.currentPage === pageNumber - 1 && rasterResolved && targetVisible) {
            await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
            return { diagnostics, render_page: raster, image_visibility: perf.pageImageVisibility[String(pageNumber - 1)] };
          }
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
        throw new Error('Page ' + pageNumber + ' did not become current');
      })()`);
      event("page-navigation-completed", {
        duration_ms: performance.now() - started,
        page_number: pageNumber,
        visible_page_indices: result.diagnostics.visiblePageIndices,
        render_page: result.render_page,
        image_visibility: result.image_visibility,
      });
    }
  }

  if (options.scenario === "zoom") {
    for (const percent of zoomSequence) {
      const started = performance.now();
      const result = await cdp.evaluate(`(async () => {
        const interactionStarted = performance.now();
        const zoom = ${percent / 100};
        const previousZoom = window.__butterPaperTestHooks.getDiagnostics().zoom;
        const noOp = Math.abs(previousZoom - zoom) < 0.0001;
        window.__butterPaperTestHooks.resetPerfSnapshot();
        window.__butterPaperTestHooks.setZoom(zoom);
        const deadline = performance.now() + 15000;
        while (performance.now() < deadline) {
          const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
          if (Math.abs(diagnostics.zoom - zoom) < 0.0001) {
            await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
            const responseDurationMs = performance.now() - interactionStarted;
            // Keep successive requests from collapsing into a single render wave.
            await new Promise((resolve) => setTimeout(resolve, 250));
            const perf = window.__butterPaperTestHooks.getPerfSnapshot();
            return { diagnostics: window.__butterPaperTestHooks.getDiagnostics(), no_op: noOp, response_duration_ms: responseDurationMs, render_page: perf.renderPage, image_visibility: perf.pageImageVisibility[String(diagnostics.currentPage)] ?? null };
          }
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
        throw new Error('Zoom ${percent}% did not settle');
      })()`);
      event("zoom-completed", {
        duration_ms: result.response_duration_ms,
        operation_wall_ms: performance.now() - started,
        zoom_percent: percent,
        visible_page_indices: result.diagnostics.visiblePageIndices,
        no_op: result.no_op,
        completion_basis: "zoom-state-and-two-animation-frames",
        render_page: result.render_page,
        image_visibility: result.image_visibility,
      });
    }
  }

  return cdp.evaluate(`(async () => {
    window.__electronPerfFrameActive = false;
    const processMetrics = await window.butterPaper.test.getProcessMetrics();
    return {
      diagnostics: window.__butterPaperTestHooks.getDiagnostics(),
      app_perf: window.__butterPaperTestHooks.getPerfSnapshot(),
      process_metrics: processMetrics,
      frame_intervals_ms: window.__electronPerfFrames,
      navigation: performance.getEntriesByType('navigation')[0]?.toJSON() ?? null,
    };
  })()`);
}

async function runIteration(options, iteration) {
  const userDataDirectory = await mkdtemp(resolve(tmpdir(), "butter-paper-electron-perf-"));
  const port = await availablePort();
  const startedAt = new Date();
  const startedMonotonic = process.hrtime.bigint();
  const events = [];
  const samples = [];
  let output = "";
  let sampleInProgress = false;
  let timedOut = false;
  let cdp;
  let evidence;
  let browserMetrics;
  let domCounters;
  let heapUsage;
  let failure;
  const event = (name, fields = {}) => events.push({
    schema_version: 1,
    runtime: "electron",
    scenario: options.scenario,
    event: name,
    t_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
    ...fields,
  });
  const child = spawn(options.electron, [`--remote-debugging-port=${port}`, "apps/desktop"], {
    cwd: repositoryDirectory,
    detached: true,
    env: {
      ...process.env,
      BP_TEST_MODE: "1",
      BP_TEST_THEME: "light",
      BP_OPEN_SAMPLE_PDF: "0",
      BP_TEST_USER_DATA_DIR: userDataDirectory,
      // The tracked test bridge permits sources only below this explicit root.
      // Point it at the selected corpus directory without copying the 128 MB PDF.
      BP_TEST_FIXTURE_DIR: dirname(options.pdf),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  const capture = (chunk) => {
    if (output.length < outputLimitBytes) output += chunk.slice(0, outputLimitBytes - output.length);
  };
  child.stdout.on("data", capture);
  child.stderr.on("data", capture);
  const sample = async () => {
    if (sampleInProgress) return;
    sampleInProgress = true;
    try {
      const snapshot = await sampleProcessTree(child.pid);
      if (snapshot) samples.push({
        elapsed_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
        ...snapshot,
      });
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
  let rejectTimeout;
  const timeoutPromise = new Promise((_, rejectPromise) => { rejectTimeout = rejectPromise; });
  const timeoutTimer = setTimeout(() => {
    timedOut = true;
    rejectTimeout(new Error(`Iteration exceeded ${options.timeoutMs} ms`));
  }, options.timeoutMs);
  timeoutTimer.unref();

  try {
    await Promise.race([(async () => {
      event("process-start");
      const target = await waitForTarget(port, child, output, Math.min(options.timeoutMs, 20_000));
      event("cdp-target-ready");
      cdp = await createCdpClient(target.webSocketDebuggerUrl);
      await cdp.send("Runtime.enable");
      await cdp.send("Performance.enable");
      evidence = await runRendererScenario(cdp, options, event);
      browserMetrics = Object.fromEntries(
        (await cdp.send("Performance.getMetrics")).metrics.map(({ name, value }) => [name, value]),
      );
      domCounters = await cdp.send("Memory.getDOMCounters");
      await cdp.send("HeapProfiler.collectGarbage");
      heapUsage = await cdp.send("Runtime.getHeapUsage");
      event("scenario-complete");
    })(), timeoutPromise]);
  } catch (error) {
    failure = error instanceof Error ? error.stack ?? error.message : String(error);
  } finally {
    clearTimeout(timeoutTimer);
    clearInterval(sampleTimer);
    cdp?.close();
    terminateProcessGroup(child.pid, "SIGTERM");
    await waitForExit(child);
    while (sampleInProgress) await delay(5);
    await rm(userDataDirectory, { recursive: true, force: true });
  }

  const validSamples = samples.filter((entry) => !entry.sample_error);
  return {
    iteration,
    started_at: startedAt.toISOString(),
    ended_at: new Date().toISOString(),
    wall_duration_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
    success: !failure && !timedOut,
    timed_out: timedOut,
    failure: failure ?? null,
    events,
    output,
    renderer: evidence ? {
      ...evidence,
      frame_summary: summarizeFrames(evidence.frame_intervals_ms),
      browser_metrics: browserMetrics,
      dom_counters: domCounters,
      heap_usage_after_gc: heapUsage,
    } : null,
    samples,
    resource_summary: {
      sample_count: validSamples.length,
      cpu_percent: numericSummary(validSamples.map((entry) => entry.cpu_percent)),
      rss_kb: numericSummary(validSamples.map((entry) => entry.rss_kb)),
      process_count: numericSummary(validSamples.map((entry) => entry.process_count)),
    },
  };
}

function summarizeEvents(iterations, field) {
  const byEvent = new Map();
  for (const iteration of iterations) {
    for (const event of iteration.events) {
      if (!Number.isFinite(event[field])) continue;
      if (!byEvent.has(event.event)) byEvent.set(event.event, []);
      byEvent.get(event.event).push(event[field]);
    }
  }
  return Object.fromEntries([...byEvent.entries()].map(([name, values]) => [name, numericSummary(values)]));
}

function summarizeReport(iterations) {
  const samples = iterations.flatMap((iteration) => iteration.samples.filter((entry) => !entry.sample_error));
  const frames = iterations.flatMap((iteration) => iteration.renderer?.frame_intervals_ms ?? []);
  return {
    successful_iterations: iterations.filter((iteration) => iteration.success).length,
    failed_iterations: iterations.filter((iteration) => !iteration.success).length,
    wall_duration_ms: numericSummary(iterations.map((iteration) => iteration.wall_duration_ms)),
    event_timestamps_ms: summarizeEvents(iterations, "t_ms"),
    duration_events_ms: summarizeEvents(iterations, "duration_ms"),
    frames: summarizeFrames(frames),
    process_tree: {
      cpu_percent: numericSummary(samples.map((entry) => entry.cpu_percent)),
      rss_kb: numericSummary(samples.map((entry) => entry.rss_kb)),
      peak_cpu_percent: samples.length ? Math.max(...samples.map((entry) => entry.cpu_percent)) : null,
      peak_rss_kb: samples.length ? Math.max(...samples.map((entry) => entry.rss_kb)) : null,
    },
  };
}

function delay(ms) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, ms));
}

async function main() {
  let options;
  try {
    options = parseArguments(process.argv.slice(2));
    if (options.help) {
      process.stdout.write(usage());
      return;
    }
    const [pdf, provenance] = await Promise.all([
      fileProvenance(options.pdf),
      collectProvenance(options.electron),
    ]);
    const iterations = [];
    for (let iteration = 1; iteration <= options.iterations; iteration += 1) {
      process.stderr.write(`Electron ${options.scenario}: iteration ${iteration}/${options.iterations}\n`);
      iterations.push(await runIteration(options, iteration));
    }
    const report = {
      schema_version: 1,
      implementation: "electron",
      scenario: options.scenario,
      requested_iterations: options.iterations,
      timeout_ms_per_iteration: options.timeoutMs,
      workload: options.scenario === "page-navigation" ? { page_sequence: pageSequence }
        : options.scenario === "zoom" ? { zoom_sequence_percent: zoomSequence } : {},
      pdf,
      provenance,
      summary: summarizeReport(iterations),
      iterations,
    };
    await mkdir(dirname(options.output), { recursive: true });
    await writeFile(options.output, `${JSON.stringify(report, null, 2)}\n`, "utf8");
    process.stderr.write(`Wrote ${options.output}\n`);
    if (report.summary.failed_iterations > 0) process.exitCode = 1;
  } catch (error) {
    process.stderr.write(`electron-runner: ${error.message}\n\n${usage()}`);
    process.exitCode = 2;
  }
}

await main();
