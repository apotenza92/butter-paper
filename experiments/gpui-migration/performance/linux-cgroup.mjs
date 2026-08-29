import { spawn } from "node:child_process";
import { mkdir, readFile, rmdir } from "node:fs/promises";
import { platform } from "node:os";

const cgroupRoot = "/sys/fs/cgroup";
const accountingPreflightId = "bp-linux-cgroup-v2-accounting-v1";

function remediationEvidence() {
  return {
    summary:
      "Run the paid benchmark in a writable unified cgroup v2 hierarchy with both CPU and memory controllers delegated to the benchmark process.",
    commands: [
      "stat -fc %T /sys/fs/cgroup",
      "cat /sys/fs/cgroup/cgroup.controllers",
      "cat /sys/fs/cgroup/cgroup.subtree_control",
    ],
    expected: [
      "The filesystem type is cgroup2fs.",
      "cgroup.controllers lists cpu and memory.",
      "A newly created child cgroup exposes cpu.stat, memory.peak, and memory.events.",
      "The benchmark account can create a child cgroup and write a probe PID to cgroup.procs.",
    ],
  };
}

export function parseKeyValueCounters(text) {
  return Object.fromEntries(
    text
      .trim()
      .split("\n")
      .map((line) => line.trim().split(/\s+/, 2))
      .filter(([key, value]) => key && /^\d+$/.test(value ?? ""))
      .map(([key, value]) => [key, Number(value)]),
  );
}

export async function createLinuxCgroup(label) {
  if (platform() !== "linux") {
    return { supported: false, reason: "cgroup-v2-accounting-is-linux-only" };
  }
  try {
    await readFile(`${cgroupRoot}/cgroup.controllers`, "utf8");
    const safeLabel = label.replace(/[^a-zA-Z0-9._-]+/g, "-");
    const path = `${cgroupRoot}/butter-paper-perf-${safeLabel}`;
    await mkdir(path);
    return { supported: true, path };
  } catch (error) {
    return {
      supported: false,
      reason: `cgroup-v2-unavailable: ${error.message}`,
    };
  }
}

export function assessRequiredCgroupV2Accounting(metrics) {
  const blockers = [];
  if (metrics?.supported !== true) {
    blockers.push(metrics?.reason ?? "cgroup-v2-accounting-unavailable");
  } else {
    if (!Number.isFinite(metrics.cpu_seconds) || metrics.cpu_seconds <= 0) {
      blockers.push(
        "cpu.stat did not record a positive usage_usec value for the child probe",
      );
    }
    if (
      metrics.memory_peak_supported !== true ||
      !Number.isFinite(metrics.memory_peak_bytes) ||
      metrics.memory_peak_bytes <= 0
    ) {
      blockers.push(
        "memory.peak is absent or did not record a positive child-cgroup peak",
      );
    }
  }

  return {
    preflight_id: accountingPreflightId,
    ready: blockers.length === 0,
    accounting_scope: "cgroup-v2-child-process-tree",
    required_metrics: ["cpu.stat:usage_usec", "memory.peak"],
    substitution_policy: "no-rss-substitution",
    blockers,
    remediation: remediationEvidence(),
  };
}

export function cgroupLaunch(cgroup, executable, args) {
  if (!cgroup.supported) return { executable, args };
  return {
    executable: "/bin/sh",
    args: [
      "-c",
      'printf "%s\\n" "$$" > "$1/cgroup.procs" || exit 125; shift; exec "$@"',
      "butter-paper-cgroup",
      cgroup.path,
      executable,
      ...args,
    ],
  };
}

export async function readLinuxCgroup(cgroup) {
  if (!cgroup.supported) return cgroup;
  try {
    const [cpuText, memoryPeakText, memoryEventsText] = await Promise.all([
      readFile(`${cgroup.path}/cpu.stat`, "utf8"),
      readFile(`${cgroup.path}/memory.peak`, "utf8").catch((error) =>
        error.code === "ENOENT" ? null : Promise.reject(error),
      ),
      readFile(`${cgroup.path}/memory.events`, "utf8"),
    ]);
    const cpu = parseKeyValueCounters(cpuText);
    const memoryPeakBytes =
      memoryPeakText === null ? null : Number(memoryPeakText.trim());
    return {
      supported: true,
      cpu_seconds: Number.isFinite(cpu.usage_usec)
        ? cpu.usage_usec / 1_000_000
        : null,
      user_cpu_seconds: Number.isFinite(cpu.user_usec)
        ? cpu.user_usec / 1_000_000
        : null,
      system_cpu_seconds: Number.isFinite(cpu.system_usec)
        ? cpu.system_usec / 1_000_000
        : null,
      memory_peak_bytes: Number.isFinite(memoryPeakBytes)
        ? memoryPeakBytes
        : null,
      memory_peak_supported:
        memoryPeakText !== null && Number.isFinite(memoryPeakBytes),
      memory_events: parseKeyValueCounters(memoryEventsText),
    };
  } catch (error) {
    return {
      supported: false,
      reason: `cgroup-v2-read-failed: ${error.message}`,
    };
  }
}

export async function removeLinuxCgroup(cgroup) {
  if (!cgroup.supported) return { removed: false, reason: cgroup.reason };
  try {
    await rmdir(cgroup.path);
    return { removed: true };
  } catch (error) {
    return {
      removed: false,
      reason: `cgroup-v2-cleanup-failed: ${error.message}`,
    };
  }
}

async function runAccountingProbe(cgroup) {
  const probe = cgroupLaunch(cgroup, process.execPath, [
    "-e",
    [
      "const retained = Buffer.alloc(8 * 1024 * 1024, 1);",
      "const deadline = performance.now() + 30;",
      "let checksum = 0;",
      "while (performance.now() < deadline) checksum += retained[checksum % retained.length];",
      "if (checksum <= 0) process.exitCode = 1;",
    ].join(" "),
  ]);

  return await new Promise((resolve) => {
    const child = spawn(probe.executable, probe.args, {
      stdio: ["ignore", "ignore", "pipe"],
    });
    let stderr = "";
    child.stderr.on("data", (chunk) => {
      if (stderr.length < 4096)
        stderr += String(chunk).slice(0, 4096 - stderr.length);
    });
    child.once("error", (error) => {
      resolve({
        passed: false,
        exit_code: null,
        reason: `probe-launch-failed: ${error.message}`,
      });
    });
    child.once("close", (code, signal) => {
      resolve({
        passed: code === 0,
        exit_code: code,
        signal,
        stderr: stderr.trim() || undefined,
        reason:
          code === 0
            ? undefined
            : `probe-exited-${code ?? signal ?? "unknown"}`,
      });
    });
  });
}

export async function preflightRequiredCgroupV2Accounting(options = {}) {
  const create = options.create ?? createLinuxCgroup;
  const read = options.read ?? readLinuxCgroup;
  const remove = options.remove ?? removeLinuxCgroup;
  const runProbe = options.runProbe ?? runAccountingProbe;
  const label = options.label ?? `preflight-${process.pid}-${Date.now()}`;
  const cgroup = await create(label);

  if (cgroup.supported !== true) {
    return {
      ...assessRequiredCgroupV2Accounting(cgroup),
      probe: { passed: false, reason: "probe-cgroup-creation-failed" },
      cleanup: { removed: false, reason: "probe-cgroup-was-not-created" },
    };
  }

  let probe;
  let metrics;
  let assessment;
  try {
    probe = await runProbe(cgroup);
    metrics = await read(cgroup);
    assessment = assessRequiredCgroupV2Accounting(metrics);
    if (probe.passed !== true) {
      assessment = {
        ...assessment,
        ready: false,
        blockers: [
          ...assessment.blockers,
          `child-cgroup accounting probe failed: ${probe.reason ?? "unknown failure"}`,
        ],
      };
    }
  } catch (error) {
    probe ??= {
      passed: false,
      reason: `accounting-preflight-failed: ${error.message}`,
    };
    metrics ??= { supported: false, reason: probe.reason };
    assessment = assessRequiredCgroupV2Accounting(metrics);
  }

  const cleanup = await remove(cgroup);
  if (cleanup.removed !== true) {
    assessment = {
      ...assessment,
      ready: false,
      blockers: [
        ...assessment.blockers,
        cleanup.reason ?? "probe-cgroup-cleanup-failed",
      ],
    };
  }

  return {
    ...assessment,
    probe,
    metrics,
    cleanup,
  };
}
