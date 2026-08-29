import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { longbridgeCompatProfile } from "./compat-evidence-validator.mjs";
import {
  acceptedComponentCandidateFileSha256,
  acceptedElectronCandidateFileSha256,
  authenticateAcceptedComponentCandidate,
  authenticateAcceptedElectronCandidate,
  buildComponentShortQualificationPlan,
  componentShortQualificationPreflight,
  parseComponentShortQualificationArguments,
  runComponentShortQualification,
} from "./component-short-qualification.mjs";

const repositoryRoot = path.resolve(import.meta.dirname, "../../..");
const acceptedCandidatePath = path.join(
  repositoryRoot,
  "test-results/portable-ubuntu24-candidate-provenance-20260828T0649Z/candidate.json",
);
const acceptedElectronCandidatePath = path.join(
  repositoryRoot,
  "test-results/gpui-v6-candidates-gpu-native-20260824/electron-optimized-candidate-v4.json",
);

test("authenticates only the accepted portable component candidate and its artifacts", async () => {
  const accepted = await authenticateAcceptedComponentCandidate({
    candidatePath: acceptedCandidatePath,
    repositoryRoot,
  });
  assert.equal(accepted.file_sha256, acceptedComponentCandidateFileSha256);
  assert.equal(
    accepted.manifest.profile,
    "bp-perf-longbridge-component-development-candidate-1",
  );
  assert.equal(accepted.manifest.optimized, false);
  assert.equal(accepted.manifest.timingEligible, false);
  assert.equal(
    accepted.manifest.runtime.binary.sha256,
    "c1f28ef31f3f6da6ce8373d7e78edca34abef15212fbee7c51504b4cb382e26a",
  );

  const temporaryDirectory = await mkdtemp(
    path.join(os.tmpdir(), "bp-component-candidate-drift-"),
  );
  try {
    const changed = JSON.parse(await readFile(acceptedCandidatePath, "utf8"));
    changed.timingEligible = true;
    await writeFile(
      path.join(temporaryDirectory, "candidate.json"),
      `${JSON.stringify(changed)}\n`,
    );
    await assert.rejects(
      authenticateAcceptedComponentCandidate({
        candidatePath: path.join(temporaryDirectory, "candidate.json"),
        repositoryRoot,
      }),
      /accepted component candidate file hash differs/,
    );
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
});

test("authenticates the exact Electron candidate file and sealed artifacts", async () => {
  const accepted = await authenticateAcceptedElectronCandidate({
    candidatePath: acceptedElectronCandidatePath,
    repositoryRoot,
  });
  assert.equal(accepted.file_sha256, acceptedElectronCandidateFileSha256);
  assert.equal(accepted.manifest.implementation, "electron");
  assert.equal(accepted.manifest.build.node_env, "production");
  assert.equal(accepted.manifest.build.packaged, false);

  const temporaryDirectory = await mkdtemp(
    path.join(os.tmpdir(), "bp-electron-candidate-drift-"),
  );
  try {
    const bytes = await readFile(acceptedElectronCandidatePath);
    await writeFile(
      path.join(temporaryDirectory, "candidate.json"),
      Buffer.concat([bytes, Buffer.from("\n")]),
    );
    await assert.rejects(
      authenticateAcceptedElectronCandidate({
        candidatePath: path.join(temporaryDirectory, "candidate.json"),
        repositoryRoot,
      }),
      /accepted Electron candidate file hash differs/,
    );
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
});

test("constructs one fixed native Electron-then-component pair without V6 acceptance", () => {
  const root = "/bundle";
  const output = "/evidence/short";
  const plan = buildComponentShortQualificationPlan({ root, output });
  assert.equal(plan.profile, "bp-perf-component-short-qualification-v1");
  assert.equal(plan.timing_eligible, false);
  assert.equal(plan.v6_acceptance, false);
  assert.deepEqual(
    plan.launches.map(({ implementation }) => implementation),
    ["electron", "gpui"],
  );
  const gpui = plan.launches[1];
  assert.deepEqual(gpui.argv.slice(-8), [
    "--binary",
    "/bundle/experiments/gpui-migration/.build-targets/gpui-component-portable-u24/debug/component_story",
    "--v4-scenario",
    "small-shell-open",
    "--v6-scenario",
    "small-shell-open",
    "--compat-profile",
    longbridgeCompatProfile,
  ]);
  assert.ok(gpui.argv.includes("--evidence-directory"));
  assert.equal(gpui.environment.BP_PERF_REQUIRE_NVIDIA, "1");
  assert.equal(gpui.environment.BP_PERF_COMMON_DAMAGE_OBSERVER, "1");
  assert.ok(
    plan.launches.every(({ argv }) => !argv.join("\0").includes("sh -c")),
  );
});

test("blocks before launch when the paid GPU desktop prerequisites are absent", () => {
  const preflight = componentShortQualificationPreflight({
    platform: "linux",
    architecture: "x64",
    environment: {},
    availableCommands: new Set(),
  });
  assert.equal(preflight.ready, false);
  assert.equal(preflight.launches, 0);
  assert.deepEqual(preflight.blockers.slice(0, 2), [
    "DISPLAY is required",
    "one shared D-Bus desktop session is required",
  ]);
  assert.ok(
    preflight.blockers.some((blocker) => blocker.includes("nvidia-smi")),
  );
});

test("fixed CLI accepts only one absolute fresh output path", () => {
  assert.deepEqual(
    parseComponentShortQualificationArguments(["--output", "/tmp/evidence"]),
    { output: "/tmp/evidence" },
  );
  for (const argv of [
    [],
    ["--output", "relative"],
    ["--output", "/tmp/out", "--candidate", "/tmp/fake"],
    ["--output", "/tmp/out;touch /tmp/pwned"],
  ]) {
    assert.throws(
      () => parseComponentShortQualificationArguments(argv),
      /fixed component qualification usage/,
    );
  }
});

test("writes an authenticated zero-launch blocker before process creation", async () => {
  const output = path.join(
    await mkdtemp(path.join(os.tmpdir(), "bp-component-short-parent-")),
    "fresh-output",
  );
  let launchCount = 0;
  try {
    const result = await runComponentShortQualification({
      root: repositoryRoot,
      output,
      componentCandidatePath: acceptedCandidatePath,
      electronCandidatePath: acceptedElectronCandidatePath,
      environment: {},
      availableCommands: new Set(),
      launchRunner: async () => {
        launchCount += 1;
        throw new Error("must not launch");
      },
    });
    assert.equal(result.status, "BLOCKED");
    assert.equal(result.launches, 0);
    assert.equal(result.timing_eligible, false);
    assert.equal(result.v6_acceptance, false);
    assert.equal(launchCount, 0);
    const retained = JSON.parse(
      await readFile(
        path.join(output, "component-short-qualification.json"),
        "utf8",
      ),
    );
    assert.equal(
      retained.authentication.component_candidate_file_sha256,
      acceptedComponentCandidateFileSha256,
    );
    assert.equal(
      retained.authentication.electron_candidate_file_sha256,
      acceptedElectronCandidateFileSha256,
    );
  } finally {
    await rm(path.dirname(output), { recursive: true, force: true });
  }
});

test("stops the fixed pair after the first malformed runner report", async () => {
  const output = path.join(
    await mkdtemp(path.join(os.tmpdir(), "bp-component-short-fail-parent-")),
    "fresh-output",
  );
  const launches = [];
  try {
    const result = await runComponentShortQualification({
      root: repositoryRoot,
      output,
      componentCandidatePath: acceptedCandidatePath,
      electronCandidatePath: acceptedElectronCandidatePath,
      environment: {
        DISPLAY: ":99",
        DBUS_SESSION_BUS_ADDRESS: "unix:path=/tmp/test-bus",
      },
      availableCommands: new Set([
        "cc",
        "convert",
        "nvidia-smi",
        "vulkaninfo",
        "xdotool",
        "xdpyinfo",
        "xprop",
      ]),
      cgroupPreflight: async () => ({ ready: true, blockers: [] }),
      launchRunner: async (launch) => {
        launches.push(launch.implementation);
        return { exit_code: 0, timed_out: false, report: null };
      },
    });
    assert.equal(result.status, "BLOCKED");
    assert.equal(result.launches, 1);
    assert.deepEqual(launches, ["electron"]);
    assert.equal(result.failed_launch.implementation, "electron");
    assert.ok(
      result.failed_launch.blockers.includes(
        "runner report did not pass one exact open-pdf iteration",
      ),
    );
  } finally {
    await rm(path.dirname(output), { recursive: true, force: true });
  }
});

test("treats a returned cgroup accounting blocker as zero-launch", async () => {
  const output = path.join(
    await mkdtemp(path.join(os.tmpdir(), "bp-component-cgroup-parent-")),
    "fresh-output",
  );
  let launchCount = 0;
  try {
    const result = await runComponentShortQualification({
      root: repositoryRoot,
      output,
      componentCandidatePath: acceptedCandidatePath,
      electronCandidatePath: acceptedElectronCandidatePath,
      environment: {
        DISPLAY: ":99",
        DBUS_SESSION_BUS_ADDRESS: "unix:path=/tmp/test-bus",
      },
      availableCommands: new Set([
        "cc",
        "convert",
        "nvidia-smi",
        "vulkaninfo",
        "xdotool",
        "xdpyinfo",
        "xprop",
      ]),
      cgroupPreflight: async () => ({
        ready: false,
        blockers: ["memory.peak is unavailable"],
      }),
      launchRunner: async () => {
        launchCount += 1;
        return { exit_code: 0, timed_out: false, report: null };
      },
    });
    assert.equal(result.status, "BLOCKED");
    assert.equal(result.launches, 0);
    assert.equal(launchCount, 0);
    assert.ok(result.preflight.blockers.includes("memory.peak is unavailable"));
  } finally {
    await rm(path.dirname(output), { recursive: true, force: true });
  }
});
