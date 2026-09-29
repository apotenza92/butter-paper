import { spawn } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { execFile } from "node:child_process";

const execFileAsync = promisify(execFile);
const sourcePath = fileURLToPath(
  new URL("./macos-libproc-accounting.c", import.meta.url),
);
const outputLimitBytes = 1024 * 1024;
const requestedSampleIntervalMs = 10;
const requiredMaximumGapMs = 100;
const absoluteTimeoutMs = 15_000;
const monitorTimeoutMs = 5_000;
const terminationGraceMs = 2_000;

export const macosRegisteredLifecycleProtocol = Object.freeze({
  version: 1,
  identity: "pid-plus-proc-start-abstime",
  ordered_events: [
    "owner-child-spawn",
    "child-register",
    "child-final-self-receipt",
    "owner-child-reaped",
  ],
  integration_seam:
    "implemented in pdf_worker.rs WorkerProcessClient and the butter-paper-pdf-worker entrypoint over dedicated inherited descriptor 199; packaged-app measurement orchestration remains",
});

function parseJsonLines(text) {
  const records = [];
  const invalid = [];
  for (const line of text.split("\n")) {
    if (!line.trim()) continue;
    try {
      records.push(JSON.parse(line));
    } catch {
      invalid.push(line.slice(0, 512));
    }
  }
  return { records, invalid };
}

function processIdentity(entry) {
  return `${entry.pid}:${entry.start_abstime}`;
}

export function assessMacosLibprocAccounting(input) {
  const blockers = [];
  const samples = input.records.filter((record) => record.type === "sample");
  const lifecycleEvents = input.lifecycleEvents ?? [];
  const identities = new Map();
  const startsByPid = new Map();
  let simultaneousPeakBytes = 0;

  for (const sample of samples) {
    let simultaneousBytes = 0;
    for (const entry of sample.processes ?? []) {
      simultaneousBytes += entry.phys_footprint_bytes ?? 0;
      const identity = processIdentity(entry);
      const previous = identities.get(identity);
      identities.set(identity, {
        ...entry,
        maximum_sampled_phys_footprint_bytes: Math.max(
          previous?.maximum_sampled_phys_footprint_bytes ?? 0,
          entry.phys_footprint_bytes ?? 0,
        ),
        lifetime_max_phys_footprint_bytes: Math.max(
          previous?.lifetime_max_phys_footprint_bytes ?? 0,
          entry.lifetime_max_phys_footprint_bytes ?? 0,
        ),
      });
      const starts = startsByPid.get(entry.pid) ?? new Set();
      starts.add(entry.start_abstime);
      startsByPid.set(entry.pid, starts);
    }
    simultaneousPeakBytes = Math.max(simultaneousPeakBytes, simultaneousBytes);
  }

  const sampleGapsMs = samples
    .slice(1)
    .map(
      (sample, index) =>
        (sample.monotonic_ns - samples[index].monotonic_ns) / 1_000_000,
    );
  const maximumObservedGapMs =
    sampleGapsMs.length > 0 ? Math.max(...sampleGapsMs) : null;
  const rootEntries = [...identities.values()].filter(
    (entry) => entry.pid === input.rootPid,
  );
  const childEntries = [...identities.values()].filter(
    (entry) => entry.pid === input.childPid,
  );
  const decoyEntries = [...identities.values()].filter(
    (entry) => entry.pid === input.decoyPid,
  );
  const finalRoot = rootEntries.at(-1);
  const rootChildCpuNs = finalRoot
    ? finalRoot.child_user_ns + finalRoot.child_system_ns
    : 0;
  const declarations = lifecycleEvents.filter(
    (event) => event.type === "child-spawn",
  );
  const registrations = lifecycleEvents.filter(
    (event) => event.type === "child-register",
  );
  const finalReceipts = lifecycleEvents.filter(
    (event) => event.type === "child-final",
  );
  const reaps = lifecycleEvents.filter(
    (event) => event.type === "child-reaped",
  );
  const registeredIdentities = new Set();
  const registeredChildLifetimeMaxByIdentity = new Map();
  let registeredChildSelfCpuNs = 0;

  for (const declaration of declarations) {
    const token = declaration.token;
    const registrationMatches = registrations.filter(
      (event) => event.token === token,
    );
    const finalMatches = finalReceipts.filter((event) => event.token === token);
    const reapMatches = reaps.filter((event) => event.token === token);
    if (!token || !Number.isInteger(declaration.pid) || declaration.pid <= 0) {
      blockers.push("an owned-child spawn declaration was malformed");
      continue;
    }
    if (registrationMatches.length !== 1) {
      blockers.push(
        `owned child ${token} did not provide exactly one registration`,
      );
      continue;
    }
    const registration = registrationMatches[0];
    if (
      registration.pid !== declaration.pid ||
      !Number.isInteger(registration.start_abstime) ||
      registration.start_abstime <= 0 ||
      registration.ppid !== input.rootPid
    ) {
      blockers.push(`owned child ${token} registration identity was invalid`);
      continue;
    }
    const identity = processIdentity(registration);
    registeredIdentities.add(identity);
    const sampledEntry = identities.get(identity);
    if (!sampledEntry) {
      blockers.push(
        `registered child ${token} was missed by resource sampling`,
      );
    } else if (sampledEntry.ppid !== input.rootPid) {
      blockers.push(`registered child ${token} sampled parent did not match`);
    }
    if (finalMatches.length !== 1) {
      blockers.push(
        `registered child ${token} did not provide exactly one final self receipt`,
      );
      continue;
    }
    const finalReceipt = finalMatches[0];
    if (
      finalReceipt.pid !== registration.pid ||
      finalReceipt.start_abstime !== registration.start_abstime ||
      !Number.isFinite(finalReceipt.user_ns) ||
      !Number.isFinite(finalReceipt.system_ns) ||
      finalReceipt.user_ns + finalReceipt.system_ns <= 0 ||
      !Number.isFinite(finalReceipt.lifetime_max_phys_footprint_bytes) ||
      finalReceipt.lifetime_max_phys_footprint_bytes <= 0 ||
      (sampledEntry &&
        finalReceipt.lifetime_max_phys_footprint_bytes <
          sampledEntry.maximum_sampled_phys_footprint_bytes)
    ) {
      blockers.push(`registered child ${token} final self receipt was invalid`);
      continue;
    }
    registeredChildSelfCpuNs += finalReceipt.user_ns + finalReceipt.system_ns;
    registeredChildLifetimeMaxByIdentity.set(
      identity,
      finalReceipt.lifetime_max_phys_footprint_bytes,
    );
    if (sampledEntry) {
      sampledEntry.lifetime_max_phys_footprint_bytes = Math.max(
        sampledEntry.lifetime_max_phys_footprint_bytes,
        finalReceipt.lifetime_max_phys_footprint_bytes,
      );
    }
    if (
      reapMatches.length !== 1 ||
      reapMatches[0].pid !== registration.pid ||
      reapMatches[0].code !== 0
    ) {
      blockers.push(`registered child ${token} was not cleanly reaped`);
      continue;
    }
    const declarationIndex = lifecycleEvents.indexOf(declaration);
    const registrationIndex = lifecycleEvents.indexOf(registration);
    const finalIndex = lifecycleEvents.indexOf(finalReceipt);
    const reapIndex = lifecycleEvents.indexOf(reapMatches[0]);
    if (
      !(
        declarationIndex < registrationIndex &&
        registrationIndex < finalIndex &&
        finalIndex < reapIndex
      )
    ) {
      blockers.push(`registered child ${token} lifecycle order was invalid`);
    }
  }

  const declaredTokens = new Set(declarations.map((event) => event.token));
  if (declarations.length === 0)
    blockers.push("no Butter-owned child lifecycle was declared");
  if (declaredTokens.size !== declarations.length)
    blockers.push("an owned-child registration token was reused");
  if (
    [...registrations, ...finalReceipts, ...reaps].some(
      (event) => !declaredTokens.has(event.token),
    )
  )
    blockers.push("an undeclared child emitted a lifecycle event");
  const unregisteredSampledChildren = [...identities.keys()].filter(
    (identity) =>
      !identity.startsWith(`${input.rootPid}:`) &&
      !registeredIdentities.has(identity),
  );
  if (unregisteredSampledChildren.length > 0)
    blockers.push("a sampled descendant lacked an owned-child registration");

  if (input.platformSupported !== true)
    blockers.push("macOS libproc accounting is unavailable on this platform");
  if (input.compilePassed !== true)
    blockers.push("libproc helper did not compile");
  if (input.helperExitCode !== 0)
    blockers.push(
      `libproc helper exited ${input.helperExitCode ?? "without code"}`,
    );
  if (input.probeExitCode !== 0)
    blockers.push(`probe exited ${input.probeExitCode ?? "without code"}`);
  if (input.invalidHelperLines?.length)
    blockers.push("libproc helper emitted invalid JSON lines");
  if (input.invalidProbeLines?.length)
    blockers.push("lifecycle probe emitted invalid JSON lines");
  if (input.outputTruncated === true)
    blockers.push("diagnostic output exceeded the 1 MiB capture budget");
  if (!input.rootPid || rootEntries.length === 0)
    blockers.push("root process was not observed");
  if (!input.childPid || childEntries.length === 0)
    blockers.push("registered short-lived child was not observed and retained");
  if (decoyEntries.length > 0)
    blockers.push(
      "negative-control decoy was incorrectly included in the tree",
    );
  if ([...identities.values()].some((entry) => entry.start_abstime <= 0))
    blockers.push(
      "a process was observed without a stable start-time identity",
    );
  if ([...startsByPid.values()].some((starts) => starts.size > 1))
    blockers.push("a PID was observed with multiple process-start identities");
  if (samples.length < 2)
    blockers.push("fewer than two resource samples were recorded");
  if (
    maximumObservedGapMs === null ||
    maximumObservedGapMs > requiredMaximumGapMs
  )
    blockers.push(
      `resource sample gap exceeded ${requiredMaximumGapMs} ms or was unavailable`,
    );
  if (input.rootExited !== true)
    blockers.push("root process was not confirmed exited and reaped");
  if (input.cleanupComplete !== true)
    blockers.push(
      "probe, decoy, helper, or temporary build cleanup was incomplete",
    );

  return {
    preflight_id: "bp-macos-registered-lifecycle-accounting-v2",
    ready: blockers.length === 0,
    release_qualified: false,
    lifecycle_protocol: macosRegisteredLifecycleProtocol,
    qualification_blockers: [
      "The packaged-app measurement runner has not yet requested the real PDF-worker lifecycle lane, completed owner clean-reap finalization, and aggregated that receipt with candidate sampling.",
    ],
    accounting_scope:
      "registered-owned-child-self-receipts-plus-libproc-simultaneous-sampling",
    sample_interval_requested_ms: requestedSampleIntervalMs,
    maximum_sample_gap_budget_ms: requiredMaximumGapMs,
    maximum_observed_sample_gap_ms: maximumObservedGapMs,
    observed_process_identities: [...identities.keys()],
    sampled_simultaneous_tree_phys_footprint_peak_bytes: simultaneousPeakBytes,
    per_process_lifetime_max_phys_footprint_bytes: Object.fromEntries(
      [...identities.entries()].map(([identity, entry]) => [
        identity,
        entry.lifetime_max_phys_footprint_bytes,
      ]),
    ),
    root_cumulative_reaped_child_cpu_ns: rootChildCpuNs,
    registered_child_self_cpu_ns: registeredChildSelfCpuNs,
    registered_child_identities: [...registeredIdentities],
    registered_child_lifetime_max_phys_footprint_bytes: Object.fromEntries(
      registeredChildLifetimeMaxByIdentity,
    ),
    blockers,
    limitations: [
      "This protocol proves only explicitly declared Butter-owned child lifecycles; it does not claim arbitrary process-tree completeness.",
      "A declared child that cannot register and publish a final self receipt before exit fails closed; an unrelated process is excluded rather than inferred from ancestry alone.",
      "Sampled simultaneous tree physical footprint and per-process lifetime maxima are distinct metrics; lifetime maxima are never summed as concurrent memory.",
      "This feasibility receipt does not provide native input, presentation, GPU, idle, soak, or packaged-candidate evidence.",
    ],
  };
}

function capture(child, limit = outputLimitBytes) {
  let stdout = "";
  let stderr = "";
  let truncated = false;
  child.stdout?.setEncoding("utf8");
  child.stderr?.setEncoding("utf8");
  child.stdout?.on("data", (chunk) => {
    if (stdout.length + chunk.length > limit) truncated = true;
    if (stdout.length < limit) stdout += chunk.slice(0, limit - stdout.length);
  });
  child.stderr?.on("data", (chunk) => {
    if (stderr.length + chunk.length > limit) truncated = true;
    if (stderr.length < limit) stderr += chunk.slice(0, limit - stderr.length);
  });
  return {
    output: () => ({ stdout, stderr, truncated }),
    completed: new Promise((resolvePromise) => {
      child.once("error", (error) =>
        resolvePromise({ code: null, signal: null, error: error.message }),
      );
      child.once("close", (code, signal) =>
        resolvePromise({ code, signal, error: null }),
      );
    }),
  };
}

function terminateGroup(child, signal) {
  if (!child?.pid || child.exitCode !== null || child.signalCode !== null)
    return;
  try {
    process.kill(-child.pid, signal);
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
}

function signalChild(entry, signal) {
  const { child, processGroup = false } = entry;
  if (!child?.pid || child.exitCode !== null || child.signalCode !== null)
    return;
  if (processGroup) {
    terminateGroup(child, signal);
    return;
  }
  child.kill(signal);
}

async function terminateAndReap(entries, graceMs) {
  const liveEntries = entries.filter((entry) => entry?.child && entry.capture);
  const completed = Promise.all(
    liveEntries.map((entry) => entry.capture.completed),
  );
  for (const entry of liveEntries) signalChild(entry, "SIGTERM");
  const reapedDuringGrace = await Promise.race([
    completed.then(() => true),
    new Promise((resolvePromise) =>
      setTimeout(() => resolvePromise(false), graceMs),
    ),
  ]);
  if (!reapedDuringGrace) {
    for (const entry of liveEntries) signalChild(entry, "SIGKILL");
  }
  return completed;
}

export function waitForChildrenWithDeadline(entries, timeoutMs, graceMs) {
  const liveEntries = entries.filter((entry) => entry?.child && entry.capture);
  const completed = Promise.all(
    liveEntries.map((entry) => entry.capture.completed),
  );
  return new Promise((resolvePromise, rejectPromise) => {
    let deadlineExpired = false;
    let settled = false;
    const timeout = setTimeout(async () => {
      if (settled) return;
      deadlineExpired = true;
      try {
        const outcomes = await terminateAndReap(liveEntries, graceMs);
        if (!settled) {
          settled = true;
          resolvePromise({ outcomes, deadlineExpired: true });
        }
      } catch (error) {
        if (!settled) {
          settled = true;
          rejectPromise(error);
        }
      }
    }, timeoutMs);
    timeout.unref();
    completed.then(
      (outcomes) => {
        if (deadlineExpired || settled) return;
        settled = true;
        clearTimeout(timeout);
        resolvePromise({ outcomes, deadlineExpired: false });
      },
      (error) => {
        if (deadlineExpired || settled) return;
        settled = true;
        clearTimeout(timeout);
        rejectPromise(error);
      },
    );
  });
}

function pidIsAbsent(pid) {
  if (!pid) return true;
  try {
    process.kill(pid, 0);
    return false;
  } catch (error) {
    return error.code === "ESRCH";
  }
}

function probeSource(helperPath) {
  return `
const { spawn } = require("node:child_process");
const retained = Buffer.alloc(48 * 1024 * 1024, 1);
process.stdout.write(JSON.stringify({ type: "parent-start", pid: process.pid }) + "\\n");
setTimeout(() => {
  const token = "short-child-v1";
  const child = spawn(${JSON.stringify(helperPath)}, ["--registered-child", token], { stdio: ["ignore", "pipe", "pipe"] });
  process.stdout.write(JSON.stringify({ type: "child-spawn", token, pid: child.pid }) + "\\n");
  child.stdout.pipe(process.stdout);
  child.stderr.pipe(process.stderr);
  child.once("close", (code, signal) => {
    process.stdout.write(JSON.stringify({ type: "child-reaped", token, pid: child.pid, code, signal }) + "\\n");
    const end = performance.now() + 75;
    let checksum = 0;
    while (performance.now() < end) checksum += retained[checksum % retained.length];
    setTimeout(() => {
      process.stdout.write(JSON.stringify({ type: "parent-end", pid: process.pid, checksum }) + "\\n");
    }, 300);
  });
}, 200);
`;
}

const decoySource = `
const retained = Buffer.alloc(16 * 1024 * 1024, 3);
process.stdout.write(JSON.stringify({ type: "decoy-start", pid: process.pid }) + "\\n");
setTimeout(() => process.exit(retained[0] === 3 ? 0 : 1), 4000);
`;

export async function runMacosLibprocPreflight() {
  const startedAt = new Date().toISOString();
  const temporaryDirectory = await mkdtemp(
    resolve(tmpdir(), "bp-macos-libproc-accounting-"),
  );
  const helperPath = resolve(temporaryDirectory, "macos-libproc-accounting");
  let compilePassed = false;
  let probe;
  let decoy;
  let helper;
  let probeCapture;
  let decoyCapture;
  let helperCapture;
  let probeOutcome = { code: null, signal: null, error: "not-started" };
  let helperOutcome = { code: null, signal: null, error: "not-started" };
  let cleanupComplete = false;
  let childPid = null;
  const platformSupported = process.platform === "darwin";

  try {
    if (!platformSupported) throw new Error("macOS is required");
    await execFileAsync(
      "/usr/bin/xcrun",
      [
        "--sdk",
        "macosx",
        "clang",
        "-std=c11",
        "-O2",
        "-Wall",
        "-Wextra",
        "-Werror",
        sourcePath,
        "-o",
        helperPath,
      ],
      { timeout: 10_000, maxBuffer: outputLimitBytes },
    );
    compilePassed = true;

    decoy = spawn(process.execPath, ["-e", decoySource], {
      detached: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    decoyCapture = capture(decoy);
    probe = spawn(process.execPath, ["-e", probeSource(helperPath)], {
      detached: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    probeCapture = capture(probe);
    helper = spawn(
      helperPath,
      [
        String(probe.pid),
        String(requestedSampleIntervalMs),
        String(monitorTimeoutMs),
      ],
      { stdio: ["ignore", "pipe", "pipe"] },
    );
    helperCapture = capture(helper);

    const deadline = await waitForChildrenWithDeadline(
      [
        { child: probe, capture: probeCapture, processGroup: true },
        { child: helper, capture: helperCapture },
      ],
      absoluteTimeoutMs,
      terminationGraceMs,
    );
    [probeOutcome, helperOutcome] = deadline.outcomes;
    if (deadline.deadlineExpired)
      throw new Error(
        "absolute lifecycle preflight deadline expired after reap",
      );

    const parsedProbe = parseJsonLines(probeCapture.output().stdout);
    childPid = parsedProbe.records.find(
      (record) => record.type === "child-spawn",
    )?.pid;
  } catch (error) {
    helperOutcome = { code: null, signal: null, error: error.message };
  } finally {
    await terminateAndReap(
      [
        { child: probe, capture: probeCapture, processGroup: true },
        { child: decoy, capture: decoyCapture, processGroup: true },
        { child: helper, capture: helperCapture },
      ],
      terminationGraceMs,
    );
    await rm(temporaryDirectory, { recursive: true, force: true });
    cleanupComplete =
      pidIsAbsent(probe?.pid) &&
      pidIsAbsent(decoy?.pid) &&
      pidIsAbsent(helper?.pid);
  }

  const helperOutput = helperCapture?.output() ?? { stdout: "", stderr: "" };
  const probeOutput = probeCapture?.output() ?? { stdout: "", stderr: "" };
  const parsedHelper = parseJsonLines(helperOutput.stdout);
  const parsedProbe = parseJsonLines(probeOutput.stdout);
  const rootExited = probeOutcome.code === 0 && pidIsAbsent(probe?.pid);
  const assessment = assessMacosLibprocAccounting({
    platformSupported,
    compilePassed,
    helperExitCode: helperOutcome.code,
    probeExitCode: probeOutcome.code,
    invalidHelperLines: parsedHelper.invalid,
    invalidProbeLines: parsedProbe.invalid,
    outputTruncated: helperOutput.truncated || probeOutput.truncated,
    records: parsedHelper.records,
    lifecycleEvents: parsedProbe.records,
    rootPid: probe?.pid,
    childPid,
    decoyPid: decoy?.pid,
    rootExited,
    cleanupComplete,
  });

  return {
    ...assessment,
    started_at: startedAt,
    completed_at: new Date().toISOString(),
    helper: {
      exit_code: helperOutcome.code,
      signal: helperOutcome.signal,
      error: helperOutcome.error,
      stderr: helperOutput.stderr.trim() || undefined,
      output_truncated: helperOutput.truncated,
      record_count: parsedHelper.records.length,
    },
    probe: {
      root_pid: probe?.pid ?? null,
      child_pid: childPid,
      exit_code: probeOutcome.code,
      signal: probeOutcome.signal,
      error: probeOutcome.error,
      invalid_lines: parsedProbe.invalid,
      events: parsedProbe.records,
    },
    negative_control: { decoy_pid: decoy?.pid ?? null },
    cleanup: { complete: cleanupComplete, temporary_build_removed: true },
  };
}

async function main() {
  const outputIndex = process.argv.indexOf("--output");
  const outputPath =
    outputIndex >= 0 && process.argv[outputIndex + 1]
      ? resolve(process.argv[outputIndex + 1])
      : fileURLToPath(
          new URL(
            "./results/macos-registered-lifecycle-preflight.json",
            import.meta.url,
          ),
        );
  const receipt = await runMacosLibprocPreflight();
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(receipt, null, 2)}\n`, "utf8");
  process.stdout.write(`${outputPath}\n`);
  process.exitCode = receipt.ready ? 0 : 1;
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await main();
}
