import { execFile } from "node:child_process";
import { access, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const performanceDirectory = dirname(fileURLToPath(import.meta.url));
const sourcePath = resolve(
  performanceDirectory,
  "native-x11-damage-observer.c",
);
const outputDirectory = resolve(
  performanceDirectory,
  "../../../test-results/gpui-migration-native-input",
);
const binaryPath = resolve(outputDirectory, "native-x11-damage-observer");
let buildPromise;
let activeCollection = null;

export const x11DamageDevelopmentDependency = Object.freeze({
  pkg_config_package: "xdamage",
  header: "X11/extensions/Xdamage.h",
  debian_ubuntu_package: "libxdamage-dev",
});

export const x11DamageObserverIntegrationV6 = Object.freeze({
  ready: true,
  activation_environment: "BP_PERF_COMMON_DAMAGE_OBSERVER=1",
  raw_evidence_path:
    "iterations[0].native_input.evidence.common_benefit_timing_boundary",
  implementations: Object.freeze(["electron", "gpui"]),
  actions: Object.freeze(["click", "key", "pointer", "wheel"]),
  benefit_components: Object.freeze([
    "open-pdf",
    "continuous-scroll",
    "viewer-dynamic-fidelity",
    "annotation-create",
    "annotation-transform",
    "editor-create",
    "native-snap-transform-120hz",
    "multi-document-session",
  ]),
});

export function damageObserverCompileArguments({ source, output }) {
  return [
    "-std=c11",
    "-O2",
    "-Wall",
    "-Wextra",
    "-Werror",
    source,
    "-o",
    output,
    "-lXdamage",
    "-lXfixes",
    "-lX11",
    "-l:libXtst.so.6",
    "-lm",
  ];
}

export function damageObserverDependencyBlocker({ header, pkgConfig }) {
  if (header === true && pkgConfig === true) return null;
  return {
    code: "xdamage-development-files-missing",
    dependency: x11DamageDevelopmentDependency,
    message:
      "XDamage observer requires X11/extensions/Xdamage.h and pkg-config xdamage; install libxdamage-dev on Debian/Ubuntu",
  };
}

async function requireDamageDevelopmentFiles() {
  let header = true;
  let pkgConfig = true;
  try {
    await access("/usr/include/X11/extensions/Xdamage.h");
  } catch {
    header = false;
  }
  try {
    await execFileAsync("pkg-config", ["--exists", "xdamage"], {
      timeout: 5_000,
    });
  } catch {
    pkgConfig = false;
  }
  const blocker = damageObserverDependencyBlocker({ header, pkgConfig });
  if (blocker) {
    const error = new Error(blocker.message);
    error.code = blocker.code;
    error.blocker = blocker;
    throw error;
  }
}

export async function buildX11DamageObserver() {
  buildPromise ??= (async () => {
    await requireDamageDevelopmentFiles();
    await mkdir(outputDirectory, { recursive: true });
    await execFileAsync(
      "cc",
      damageObserverCompileArguments({
        source: sourcePath,
        output: binaryPath,
      }),
      { encoding: "utf8", timeout: 15_000, maxBuffer: 64_000 },
    );
    return binaryPath;
  })();
  return buildPromise;
}

function positive(value, label) {
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`${label} must be finite and positive`);
  }
  return value;
}

export function parseX11DamageObserverSample(stdout, { candidatePid } = {}) {
  const lines = stdout.trim().split("\n");
  if (lines.length !== 1) throw new Error("observer must emit one JSON sample");
  let sample;
  try {
    sample = JSON.parse(lines[0]);
  } catch {
    throw new Error("observer sample is not valid JSON");
  }
  if (
    sample?.schema_version !== 3 ||
    sample?.observer !== "native-x11-damage-observer-v1" ||
    sample?.input_api !== "XTEST" ||
    !["click", "key", "pointer", "wheel"].includes(sample?.action) ||
    !/^[A-Za-z0-9_.:-]{1,96}$/.test(sample?.action_token ?? "") ||
    !Number.isInteger(sample?.action_sequence) ||
    sample.action_sequence < 0 ||
    !Number.isInteger(sample?.action_event_count) ||
    sample.action_event_count < 1 ||
    sample?.action_position !== "terminal" ||
    sample?.correlation_method !==
      "observer-owned-terminal-XTEST-action-to-first-target-DamageNotify-after-damage-reset" ||
    sample?.input_clock !== "CLOCK_MONOTONIC" ||
    sample?.completion_clock !== "CLOCK_MONOTONIC" ||
    sample?.completion_signal !== "X11-DamageNotify" ||
    sample?.observation_scope !==
      "x11-server-drawable-damage-not-presentation-completion" ||
    sample?.server_observed_drawable_damage !== true ||
    sample?.presentation_completion_observed !== false ||
    sample?.physical_scanout_observed !== false ||
    !Number.isInteger(sample?.observer_pid) ||
    sample.observer_pid < 1 ||
    (Number.isInteger(candidatePid) && sample.observer_pid === candidatePid) ||
    !/^\d+$/.test(sample?.window_id ?? "") ||
    !/^\d+$/.test(sample?.input_window_id ?? "") ||
    !/^\d+$/.test(sample?.verified_input_window_id ?? "") ||
    !/^\d+$/.test(sample?.damage_drawable_id ?? "") ||
    sample.damage_drawable_id !== sample.window_id ||
    !["same-window", "verified-distinct-window"].includes(
      sample?.input_target_relation,
    ) ||
    (sample.input_window_id === sample.damage_drawable_id) !==
      (sample.input_target_relation === "same-window") ||
    (sample.input_target_relation === "same-window" &&
      sample.verified_input_window_id !== sample.damage_drawable_id) ||
    (sample.input_target_relation === "verified-distinct-window" &&
      !["click", "key"].includes(sample.action)) ||
    sample?.target_viewable_before_action !== true ||
    !Number.isInteger(sample?.target_width) ||
    sample.target_width < 1 ||
    !Number.isInteger(sample?.target_height) ||
    sample.target_height < 1 ||
    !Number.isInteger(sample?.damage_extension_major) ||
    sample.damage_extension_major < 1 ||
    !Number.isInteger(sample?.damage_extension_minor) ||
    sample.damage_extension_minor < 0 ||
    sample?.damage_report_level !== "XDamageReportNonEmpty" ||
    !/^\d+$/.test(sample?.damage_handle_id ?? "") ||
    !Number.isInteger(sample?.damage_server_timestamp) ||
    sample.damage_server_timestamp < 0 ||
    sample?.damage_area?.width < 1 ||
    sample?.damage_area?.height < 1 ||
    sample?.damage_geometry?.width < 1 ||
    sample?.damage_geometry?.height < 1 ||
    typeof sample?.damage_more !== "boolean"
  ) {
    throw new Error("observer sample identity or XDamage fields are invalid");
  }
  const input = positive(sample.input_monotonic_ms, "input timestamp");
  const completed = positive(
    sample.damage_notify_received_monotonic_ms,
    "damage timestamp",
  );
  const latency = positive(
    sample.input_to_damage_notify_ms,
    "input-to-DamageNotify latency",
  );
  const actionCompleted = positive(
    sample.action_completed_monotonic_ms,
    "action completion timestamp",
  );
  if (
    actionCompleted < input ||
    completed <= actionCompleted ||
    Math.abs(completed - input - latency) > 0.002
  ) {
    throw new Error("observer sample latency does not match its timestamps");
  }
  if (!Array.isArray(sample.injected_samples)) {
    throw new Error("observer sample injected action trace is invalid");
  }
  if (sample.action === "pointer") {
    if (
      sample.injected_samples.length < 2 ||
      sample.injected_samples.some(
        (entry, index) =>
          entry?.sample_index !== index ||
          !Number.isFinite(entry?.observed_monotonic_ms) ||
          entry.observed_monotonic_ms <= 0 ||
          entry?.action !== (index === 0 ? "down" : "move"),
      )
    ) {
      throw new Error("observer pointer injection trace is invalid");
    }
  } else if (sample.injected_samples.length !== 0) {
    throw new Error("observer non-pointer action has an unexpected trace");
  }
  return sample;
}

function percentile(values, quantile) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil(sorted.length * quantile) - 1];
}

export function summarizeX11DamageObserverSamples(
  samples,
  { candidatePid } = {},
) {
  if (!Array.isArray(samples) || samples.length < 1) {
    throw new Error("at least one XDamage observer sample is required");
  }
  const validated = samples.map((sample) =>
    parseX11DamageObserverSample(JSON.stringify(sample), { candidatePid }),
  );
  const tokens = new Set();
  const windowId = validated[0].window_id;
  for (const [index, sample] of validated.entries()) {
    const identity = `${sample.action_token}:${sample.action_sequence}`;
    if (tokens.has(identity)) {
      throw new Error("XDamage observer action correlation is duplicated");
    }
    if (sample.window_id !== windowId || sample.action_sequence !== index) {
      throw new Error(
        "XDamage observer action correlation changed window or sequence",
      );
    }
    tokens.add(identity);
  }
  return {
    schema_version: 2,
    boundary_id: "x11-damage-notify-after-xtest-v1",
    input_clock: "CLOCK_MONOTONIC",
    completion_clock: "CLOCK_MONOTONIC",
    completion_signal: "X11-DamageNotify",
    observation_scope: "x11-server-drawable-damage-not-presentation-completion",
    server_observed_drawable_damage: true,
    presentation_completion_observed: false,
    observer_process_independent: true,
    physical_scanout_observed: false,
    passed: true,
    decision_timing_eligible: true,
    temporal_action_binding: true,
    correlation_method:
      "observer-owned-terminal-XTEST-action-to-first-target-DamageNotify-after-damage-reset",
    sample_count: validated.length,
    input_to_damage_notify_p95_ms: percentile(
      validated.map((sample) => sample.input_to_damage_notify_ms),
      0.95,
    ),
    samples: validated,
  };
}

export const commonX11DamageTimingBoundaryV6 = Object.freeze({
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
});

export function commonX11DamageTimingBoundaryPassedV6(receipt) {
  const headerPassed =
    Object.entries(commonX11DamageTimingBoundaryV6).every(
      ([field, expected]) => receipt?.[field] === expected,
    ) &&
    receipt?.passed === true &&
    receipt?.decision_timing_eligible === true &&
    receipt?.temporal_action_binding === true &&
    Number.isInteger(receipt?.sample_count) &&
    receipt.sample_count > 0 &&
    Number.isFinite(receipt?.input_to_damage_notify_p95_ms) &&
    receipt.input_to_damage_notify_p95_ms > 0 &&
    receipt?.correlation_method ===
      "observer-owned-terminal-XTEST-action-to-first-target-DamageNotify-after-damage-reset";
  if (!headerPassed || receipt.samples?.length !== receipt.sample_count) {
    return false;
  }
  try {
    const validated = receipt.samples.map((sample) =>
      parseX11DamageObserverSample(JSON.stringify(sample)),
    );
    return (
      percentile(
        validated.map((sample) => sample.input_to_damage_notify_ms),
        0.95,
      ) === receipt.input_to_damage_notify_p95_ms
    );
  } catch {
    return false;
  }
}

export async function observeX11DamageAfterXTest({
  windowId,
  action,
  timeoutMs = 5_000,
  display = process.env.DISPLAY,
  candidatePid,
  actionToken = "benchmark-action",
  actionSequence = 0,
}) {
  if (!/^\d+$/.test(String(windowId)) || Number(windowId) < 1) {
    throw new Error("windowId must be a positive X11 window ID");
  }
  if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 60_000) {
    throw new Error("timeoutMs must be an integer from 1 through 60000");
  }
  if (!/^[A-Za-z0-9_.:-]{1,96}$/.test(actionToken)) {
    throw new Error(
      "actionToken must be a portable non-empty correlation token",
    );
  }
  if (!Number.isInteger(actionSequence) || actionSequence < 0) {
    throw new Error("actionSequence must be a non-negative integer");
  }
  let actionArguments;
  if (action?.type === "click") {
    if (
      !Number.isInteger(action.button) ||
      action.button < 1 ||
      action.button > 7
    ) {
      throw new Error("XTEST button must be an integer from 1 through 7");
    }
    if (!Number.isInteger(action.x) || !Number.isInteger(action.y)) {
      throw new Error("XTEST click requires integer screen coordinates");
    }
    if (
      action.inputWindowId !== undefined &&
      (!/^\d+$/.test(String(action.inputWindowId)) ||
        Number(action.inputWindowId) < 1)
    ) {
      throw new Error("XTEST click input window ID must be positive");
    }
    actionArguments = [
      "click",
      String(action.x),
      String(action.y),
      String(action.button),
      ...(action.inputWindowId === undefined
        ? []
        : [String(action.inputWindowId)]),
    ];
  } else if (action?.type === "key") {
    if (!/^[A-Za-z0-9_]{1,32}$/.test(action.keysym ?? "")) {
      throw new Error("XTEST key requires a portable X11 keysym");
    }
    if (
      !/^\d+$/.test(String(action.inputWindowId)) ||
      Number(action.inputWindowId) < 1
    ) {
      throw new Error("XTEST key requires a verified input window ID");
    }
    actionArguments = ["key", String(action.inputWindowId), action.keysym];
  } else if (action?.type === "pointer") {
    if (
      !Number.isInteger(action.durationMs) ||
      action.durationMs < 1 ||
      !Number.isInteger(action.button) ||
      action.button < 1 ||
      action.button > 3 ||
      !Array.isArray(action.screenSamples) ||
      action.screenSamples.length < 2 ||
      action.screenSamples.some(
        (point) => !Number.isInteger(point?.x) || !Number.isInteger(point?.y),
      )
    ) {
      throw new Error("XTEST pointer action has an invalid bounded schedule");
    }
    actionArguments = [
      "pointer",
      String(action.durationMs),
      String(action.screenSamples.length),
      String(action.button),
      ...action.screenSamples.flatMap(({ x, y }) => [String(x), String(y)]),
    ];
  } else if (action?.type === "wheel") {
    const integerFields = [
      action.forwardDurationMs,
      action.pauseDurationMs,
      action.reverseDurationMs,
      action.rateHz,
      action.reverseNotches,
      action.x,
      action.y,
    ];
    if (
      integerFields.some((value) => !Number.isInteger(value)) ||
      action.forwardDurationMs < 1 ||
      action.pauseDurationMs < 0 ||
      action.reverseDurationMs < 1 ||
      action.rateHz < 1 ||
      action.reverseNotches < 1
    ) {
      throw new Error("XTEST wheel action has an invalid bounded schedule");
    }
    actionArguments = [
      "wheel",
      String(action.forwardDurationMs),
      String(action.pauseDurationMs),
      String(action.reverseDurationMs),
      String(action.rateHz),
      String(action.reverseNotches),
      String(action.x),
      String(action.y),
    ];
  } else {
    throw new Error(
      "action must be a bounded XTEST click, key, pointer, or wheel action",
    );
  }
  const binary = await buildX11DamageObserver();
  const { stdout, stderr } = await execFileAsync(
    binary,
    [
      "sample",
      String(windowId),
      String(timeoutMs),
      actionToken,
      String(actionSequence),
      ...actionArguments,
    ],
    {
      encoding: "utf8",
      timeout: timeoutMs + 5_000,
      maxBuffer: 64_000,
      env: { ...process.env, DISPLAY: display },
    },
  );
  if (stderr !== "") throw new Error(`XDamage observer stderr: ${stderr}`);
  return parseX11DamageObserverSample(stdout, { candidatePid });
}

export async function runX11DamageObservedAction(options) {
  const started = process.hrtime.bigint();
  const sample = await observeX11DamageAfterXTest(options);
  return {
    duration_ms: Number(process.hrtime.bigint() - started) / 1e6,
    sample,
  };
}

export function beginX11DamageObserverCollection({ candidatePid }) {
  if (!Number.isInteger(candidatePid) || candidatePid < 1) {
    throw new Error("XDamage collection requires a candidate PID");
  }
  if (activeCollection !== null) {
    throw new Error("XDamage collection is already active");
  }
  activeCollection = { candidatePid, nextSequence: 0, samples: [] };
}

export function x11DamageObserverActionContext(actionLabel) {
  if (activeCollection === null) return null;
  const portable = String(actionLabel ?? "benchmark-action").replace(
    /[^A-Za-z0-9_.:-]/g,
    "-",
  );
  const sequence = activeCollection.nextSequence;
  activeCollection.nextSequence += 1;
  return {
    candidatePid: activeCollection.candidatePid,
    actionToken: `${portable.slice(0, 80)}:${sequence}`,
    actionSequence: sequence,
  };
}

export function retainX11DamageObserverSample(sample) {
  if (activeCollection === null) {
    throw new Error("XDamage collection is not active");
  }
  activeCollection.samples.push(sample);
  return sample;
}

export function finishX11DamageObserverCollection() {
  if (activeCollection === null) return null;
  const collection = activeCollection;
  activeCollection = null;
  return summarizeX11DamageObserverSamples(collection.samples, {
    candidatePid: collection.candidatePid,
  });
}

export function abortX11DamageObserverCollection() {
  activeCollection = null;
}
