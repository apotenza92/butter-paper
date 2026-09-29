import { readFileSync } from "node:fs";
import YAML from "yaml";
import { describe, expect, it } from "vitest";

const path = ".github/workflows/approve-gpui-pdfium-production.yml";
const source = readFileSync(path, "utf8");
const workflow = YAML.parse(source);
const targets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-pc-windows-msvc",
  "x86_64-pc-windows-msvc",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
];

describe("GPUI PDFium approval handoff workflow", () => {
  it("passes dispatch inputs through environment variables before shell use", () => {
    for (const job of Object.values(workflow.jobs) as Array<{ steps?: Array<{ run?: unknown }> }>) {
      for (const step of job.steps ?? []) {
        if (typeof step.run === "string") {
          expect(step.run).not.toMatch(/\$\{\{\s*inputs\./);
        }
      }
    }
  });

  it("accepts exactly six required reviews and stays read-only", () => {
    expect(workflow.on.workflow_dispatch.inputs.source_run_id.required).toBe(true);
    expect(workflow.on.workflow_dispatch.inputs.source_run_attempt.required).toBe(true);
    expect(workflow.on.workflow_dispatch.inputs.review_bundle_base64.required).toBe(true);
    expect(workflow.permissions).toEqual({ actions: "read", contents: "read" });
    expect(source).toContain("${#REVIEW_BUNDLE_BASE64} <= 60000");
    expect(source).toContain("base64 --decode");
    expect(source).toContain("must contain exactly the six required production targets");
    expect(source).not.toContain("requiredAndOptionalKeys");
    expect(source).not.toContain("pdfium-optional-target");
  });

  it("downloads all six required artifacts from the exact same-repository run attempt", () => {
    for (const target of targets) expect(source).toContain(target);
    expect(source).not.toContain("optional_target");
    expect(source).toContain('gh run download "$SOURCE_RUN_ID" --repo "$REPOSITORY"');
    expect(source).toContain(
      "run.path !== '.github/workflows/build-gpui-pdfium-production.yml'",
    );
    expect(source).toContain("run.conclusion !== 'success'");
    expect(source).toContain("run.head_repository?.full_name !== repository");
    expect(source).toContain("${SOURCE_RUN_ID}-${SOURCE_RUN_ATTEMPT}");
    expect(source).toContain('run.event !== \'workflow_dispatch\'');
    expect(source).toContain('String(run.run_attempt) !== expectedAttempt');
    expect(source).toContain("actions: read");
  });

  it("runs both supplied human reviews through approval validation before aggregation", () => {
    expect(source).toContain("redistribution-review.json");
    expect(source).toContain("supplier-review.json");
    expect(source).toContain("approve-pdfium-production-candidate.mjs");
    expect(source).toContain("aggregate-pdfium-approvals.mjs");
    expect(source).toContain(
      'cp "$dir/reviews/redistribution-review.json" "$dir/candidate/reviews/approved-redistribution.json"',
    );
    expect(source).toContain(
      'approvedManifestPath: `${root}/${target}/production-pdfium-approved.json`',
    );
    expect(source).toContain(
      '--output "$dir/production-pdfium-approved.json"',
    );
    expect(source).not.toContain(
      '--output "$dir/candidate/production-pdfium-approved.json"',
    );
    expect(source.indexOf("approve-pdfium-production-candidate.mjs")).toBeLessThan(
      source.indexOf("aggregate-pdfium-approvals.mjs"),
    );
    expect(source).toContain("x86_64-apple-darwin");
    expect(source).toContain("gpui-pdfium-production-approved");
    expect(source).toContain("retention-days: 30");
    expect(source).not.toMatch(/contents:\s*write|id-token:\s*write|environment:/);
  });
});
