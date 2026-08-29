#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  buildDirectionalSafetyScheduleV7,
  classifyDirectionalSafetyV7,
  directionalSafetyExpectedPlanSha256V7,
  directionalSafetyLaunchCountV7,
  directionalSafetyPlanSha256V7,
  directionalSafetyPlanV7,
  directionalSafetyProtocolVersionV7,
  directionalSafetyThresholdsV7,
} from "./directional-safety-v7.mjs";
import { canonicalSha256 } from "./run-paired-v4.mjs";
import { validateCandidateLaunchSealV6 } from "./run-paired-v6.mjs";

function percentile(values, quantile) {
  const sorted = values
    .filter(Number.isFinite)
    .sort((left, right) => left - right);
  if (sorted.length === 0) return null;
  return sorted[Math.ceil(sorted.length * quantile) - 1];
}

function summary(values) {
  const finite = values.filter(Number.isFinite);
  return finite.length === 0
    ? null
    : {
        count: finite.length,
        min: Math.min(...finite),
        median: percentile(finite, 0.5),
        max: Math.max(...finite),
      };
}

function exactScheduleFailures(manifest, expected) {
  const failures = [];
  if (manifest?.schedule?.length !== directionalSafetyLaunchCountV7) {
    failures.push("frozen schedule must contain exactly 12 launches");
    return failures;
  }
  if (manifest?.launches?.length !== directionalSafetyLaunchCountV7) {
    failures.push("retained launch evidence must contain exactly 12 launches");
  }
  if (manifest?.trials?.length !== directionalSafetyLaunchCountV7) {
    failures.push("retained trial evidence must contain exactly 12 trials");
  }
  for (let index = 0; index < expected.length; index += 1) {
    const frozen = expected[index];
    const scheduled = manifest.schedule?.[index];
    const launch = manifest.launches?.[index];
    const trial = manifest.trials?.[index];
    for (const field of [
      "schedule_index",
      "phase",
      "pair",
      "pair_position",
      "implementation",
      "journey",
      "component",
      "input_lane",
      "benefit_metrics_eligible",
      "inference_eligible",
    ]) {
      if (scheduled?.[field] !== frozen[field]) {
        failures.push(
          `schedule ${index} ${field} differs from the frozen plan`,
        );
      }
      if (launch && launch[field] !== frozen[field]) {
        failures.push(`launch ${index} ${field} differs from the frozen plan`);
      }
    }
    if (
      trial &&
      (trial.pair !== frozen.pair ||
        trial.implementation !== frozen.implementation)
    ) {
      failures.push(`trial ${index} identity differs from the frozen plan`);
    }
    if (launch?.termination?.bounded_wait_complete !== true) {
      failures.push(`launch ${index} does not prove bounded process teardown`);
    }
  }
  return failures;
}

async function artifactFailures(manifest) {
  const failures = [];
  try {
    const bytes = await readFile(manifest.qualification.path);
    const observed = createHash("sha256").update(bytes).digest("hex");
    if (observed !== manifest.qualification.sha256) {
      failures.push("qualification receipt file hash differs");
    }
  } catch {
    failures.push("qualification receipt file is absent");
  }
  const paths = new Set();
  for (let index = 0; index < (manifest.launches?.length ?? 0); index += 1) {
    const launch = manifest.launches[index];
    const trial = manifest.trials?.[index];
    if (paths.has(launch.raw_report_path)) {
      failures.push(`launch ${index} reused a raw report path`);
    }
    paths.add(launch.raw_report_path);
    if (
      launch.pre_launch_health?.passed !== true ||
      launch.post_launch_health?.passed !== true
    ) {
      failures.push(`launch ${index} pre/post host health did not pass`);
    }
    try {
      validateCandidateLaunchSealV6({
        seal: launch.candidate_prelaunch_seal,
        launchId: launch.identity,
        candidates: manifest.candidates,
      });
    } catch {
      failures.push(`launch ${index} top-level candidate seal is invalid`);
    }
    if (launch.raw_report_sha256) {
      try {
        const bytes = await readFile(launch.raw_report_path);
        const observed = createHash("sha256").update(bytes).digest("hex");
        if (observed !== launch.raw_report_sha256) {
          failures.push(`launch ${index} raw report hash differs`);
        }
        const report = JSON.parse(bytes);
        if (
          JSON.stringify(report.launch_binding_v7) !==
          JSON.stringify(launch.launch_binding_v7)
        ) {
          failures.push(`launch ${index} report binding differs`);
        }
      } catch {
        failures.push(`launch ${index} raw report is absent or invalid`);
      }
    } else if (trial?.outcome !== "FAIL") {
      failures.push(`launch ${index} lacks an authenticated raw report`);
    }
    const binding = launch.launch_binding_v7;
    if (binding) {
      const { evidence_sha256: evidenceSha256, ...payload } = binding;
      if (
        evidenceSha256 !== canonicalSha256(payload) ||
        binding.protocol_version !== directionalSafetyProtocolVersionV7 ||
        binding.schedule_index !== launch.schedule_index ||
        binding.qualification_receipt_sha256 !== manifest.qualification.sha256
      ) {
        failures.push(`launch ${index} V7 binding is invalid`);
      }
      try {
        validateCandidateLaunchSealV6({
          seal: binding.candidate_prelaunch_seal,
          launchId: launch.identity,
          candidates: manifest.candidates,
        });
      } catch {
        failures.push(`launch ${index} candidate prelaunch seal is invalid`);
      }
    } else if (trial?.outcome !== "FAIL") {
      failures.push(`launch ${index} V7 binding is absent`);
    }
  }
  return failures;
}

export async function analyzeDirectionalSafetyManifestV7(
  manifest,
  { authenticateArtifacts = false } = {},
) {
  const blockers = [];
  const expectedSchedule = buildDirectionalSafetyScheduleV7();
  if (
    directionalSafetyPlanSha256V7 !== directionalSafetyExpectedPlanSha256V7 ||
    manifest?.protocol_version !== directionalSafetyProtocolVersionV7 ||
    manifest?.directional_safety_plan_sha256 !==
      directionalSafetyExpectedPlanSha256V7 ||
    canonicalSha256(manifest?.directional_safety_plan) !==
      directionalSafetyExpectedPlanSha256V7 ||
    JSON.stringify(manifest?.source_v6) !==
      JSON.stringify(directionalSafetyPlanV7.source_v6)
  ) {
    blockers.push("V7 plan or frozen V6 source identity is not exact");
  }
  if (manifest?.complete !== true || manifest?.outcome !== "completed") {
    blockers.push("V7 execution is not complete");
  }
  blockers.push(...exactScheduleFailures(manifest, expectedSchedule));
  for (const [index, trial] of (manifest?.trials ?? []).entries()) {
    if (!["PASS", "FAIL", "ABORT"].includes(trial?.outcome)) {
      blockers.push(`trial ${index} outcome is invalid`);
    }
    if (trial?.known_baseline_defect_id != null) {
      blockers.push(`trial ${index} contains a forbidden defect exception`);
    }
    if (trial?.outcome === "ABORT") {
      blockers.push(
        `trial ${index} ABORT: ${trial.structural_failures?.join("; ")}`,
      );
    }
    if (
      trial?.outcome === "PASS" &&
      Object.keys(directionalSafetyThresholdsV7).some(
        (metric) => !Number.isFinite(trial.measurements?.[metric]),
      )
    ) {
      blockers.push(`trial ${index} PASS has missing safety measurements`);
    }
  }
  if (authenticateArtifacts) {
    blockers.push(...(await artifactFailures(manifest)));
  }

  const pairs = Array.from({ length: 6 }, (_, index) => {
    const pair = index + 1;
    return {
      pair,
      electron: manifest?.trials?.find(
        (trial) => trial.pair === pair && trial.implementation === "electron",
      )?.outcome,
      gpui: manifest?.trials?.find(
        (trial) => trial.pair === pair && trial.implementation === "gpui",
      )?.outcome,
    };
  });
  const classified = classifyDirectionalSafetyV7(pairs);
  blockers.push(...classified.structural_failures);
  const descriptiveStatistics = Object.fromEntries(
    Object.keys(directionalSafetyThresholdsV7).map((metric) => [
      metric,
      Object.fromEntries(
        ["electron", "gpui"].map((implementation) => [
          implementation,
          summary(
            (manifest?.trials ?? [])
              .filter((trial) => trial.implementation === implementation)
              .map((trial) => trial.measurements?.[metric]),
          ),
        ]),
      ),
    ]),
  );
  return {
    schema_version: 1,
    protocol_version: directionalSafetyProtocolVersionV7,
    plan_sha256: directionalSafetyExpectedPlanSha256V7,
    source_v6_disposition: "failed-closed-and-not-reclassified",
    authenticated_artifacts: authenticateArtifacts,
    claim_limit: directionalSafetyPlanV7.claim_limit,
    decision: blockers.length > 0 ? "INCONCLUSIVE" : classified.decision,
    pass_counts: classified.pass_counts,
    exact_test: classified.exact_test,
    descriptive_statistics: descriptiveStatistics,
    structural_blockers: [...new Set(blockers)],
  };
}

async function main() {
  const args = process.argv.slice(2);
  const manifestIndex = args.indexOf("--manifest");
  const outputIndex = args.indexOf("--output");
  if (manifestIndex < 0 || !args[manifestIndex + 1]) {
    throw new Error(
      "--manifest <directional-safety-manifest-v7.json> is required",
    );
  }
  const manifest = JSON.parse(
    await readFile(resolve(args[manifestIndex + 1]), "utf8"),
  );
  const analysis = await analyzeDirectionalSafetyManifestV7(manifest, {
    authenticateArtifacts: true,
  });
  const bytes = `${JSON.stringify(analysis, null, 2)}\n`;
  if (outputIndex >= 0 && args[outputIndex + 1]) {
    await writeFile(resolve(args[outputIndex + 1]), bytes);
  } else {
    process.stdout.write(bytes);
  }
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main();
}
