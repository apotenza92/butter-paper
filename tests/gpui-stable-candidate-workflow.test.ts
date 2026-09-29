import { readFileSync } from "node:fs";
import YAML from "yaml";
import { describe, expect, it } from "vitest";

const path = ".github/workflows/build-gpui-stable-candidate.yml";
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

describe("GPUI stable candidate workflow", () => {
  it("packages all six targets as required release artifacts", () => {
    const matrix = workflow.jobs.package.strategy.matrix.include;
    expect(
      matrix.map(({ target }: { target: string }) => target).sort(),
    ).toEqual([...targets].sort());
    expect(
      matrix.find(
        ({ target }: { target: string }) => target === "x86_64-apple-darwin",
      )?.label,
    ).toBe("macos-x64");
    expect(
      matrix.find(
        ({ target }: { target: string }) => target === "x86_64-apple-darwin",
      )?.optional,
    ).toBe(false);
    expect(
      matrix.filter(({ optional }: { optional: boolean }) => !optional),
    ).toHaveLength(6);
    expect(workflow.jobs.package["continue-on-error"]).toBe(
      "${{ matrix.optional }}",
    );
    expect(workflow.jobs.aggregate.if).toContain("always()");
    expect(source).toContain("aggregate-stable-candidate.mjs");
    expect(
      matrix.find(
        ({ target }: { target: string }) =>
          target === "aarch64-pc-windows-msvc",
      )?.runner,
    ).toBe("windows-11-arm");
    expect(
      matrix.find(
        ({ target }: { target: string }) =>
          target === "aarch64-unknown-linux-gnu",
      )?.runner,
    ).toBe("ubuntu-24.04-arm");
  });

  it("binds an exact main-reachable source commit and human-approved PDFium handoff", () => {
    expect(workflow.on.workflow_dispatch.inputs.source_revision.required).toBe(
      true,
    );
    expect(
      workflow.on.workflow_dispatch.inputs.approved_pdfium_run_id.required,
    ).toBe(true);
    expect(
      workflow.on.workflow_dispatch.inputs.approved_pdfium_run_attempt.required,
    ).toBe(true);
    expect(workflow.on.workflow_dispatch.inputs).not.toHaveProperty(
      "approved_pdfium_artifact",
    );
    expect(source).toContain('[[ "$SOURCE_REVISION" =~ ^[0-9a-f]{40}$ ]]');
    expect(source).toContain("git merge-base --is-ancestor HEAD origin/main");
    expect(
      source.match(/ref: \$\{\{ inputs\.source_revision \}\}/g)?.length,
    ).toBeGreaterThanOrEqual(2);
    expect(source).toContain("production-pdfium-approved.json");
    expect(source).toContain("approved_pdfium_run_id");
    expect(source).toContain("run.path !== '.github/workflows/approve-gpui-pdfium-production.yml'");
    expect(source).toContain("run.head_repository?.full_name !== repository");
    expect(source).toContain("name: gpui-pdfium-production-approved");
    expect(source).toContain("stage-pdfium-production.mjs");
    expect(source).toContain("stage-nonmac-pdfium-production.mjs");
  });

  it("uses release-only native binaries and packages the phone helper everywhere", () => {
    expect(source).toContain("--release --no-default-features");
    expect(source).not.toContain("development-pdfium-override");
    expect(source).toContain("-trimpath -buildvcs=false");
    expect(source).toContain("CGO_ENABLED=0");
    expect(source).toContain("butter-paper-signature-phone.exe");
    expect(source).toContain('butter-paper-signature-phone"');
    expect(source).toContain("SignatureCamera.swift");
  });

  it("keeps Linux build prerequisites in packaging and runtime prerequisites in clean-host smoke", () => {
    expect(source).toContain("Install Linux native build prerequisites");
    for (const dependency of [
      "libfontconfig-dev",
      "libvulkan-dev",
      "libx11-dev",
      "libxcb1-dev",
      "libxcb-xkb-dev",
      "libxkbcommon-dev",
      "libxkbcommon-x11-dev",
      "pkg-config",
    ]) {
      expect(source).toContain(dependency);
    }
    expect(source).toContain("Install Linux runtime smoke prerequisites");
    for (const dependency of [
      "xvfb",
      "xauth",
      "xdotool",
      "x11-utils",
      "openbox",
      "xz-utils",
      "qpdf",
    ]) {
      expect(source).toContain(dependency);
    }
    expect(source).toContain('Xvfb "$DISPLAY"');
    expect(source).toContain('openbox >"$evidence/openbox.stdout.log"');
    expect(source).toContain('xdpyinfo -display "$DISPLAY"');
    expect(source).toContain("trap cleanup_linux_desktop EXIT");
    expect(source).toContain("Install Windows runtime smoke prerequisites");
    expect(source).toContain("choco install qpdf --yes --no-progress");
    expect(
      source.indexOf("Install Linux native build prerequisites"),
    ).toBeLessThan(
      source.indexOf("Build release binaries and local phone helper"),
    );
    expect(
      source.indexOf("Install Linux runtime smoke prerequisites"),
    ).toBeGreaterThan(source.indexOf("smoke:"));
  });

  it("activates architecture-matched MSVC and LLVM tools before Windows builds", () => {
    const matrix = workflow.jobs.package.strategy.matrix.include;
    expect(
      matrix.find(
        ({ target }: { target: string }) =>
          target === "aarch64-pc-windows-msvc",
      )?.msvc_arch,
    ).toBe("arm64");
    expect(
      matrix.find(
        ({ target }: { target: string }) => target === "x86_64-pc-windows-msvc",
      )?.msvc_arch,
    ).toBe("x64");
    expect(source).toContain(
      "Activate matching Windows MSVC and LLVM toolchain",
    );
    expect(source).toContain("Microsoft.VisualStudio.Component.VC.Tools.ARM64");
    expect(source).toContain(
      "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
    );
    expect(source).toContain("-arch=$arch -host_arch=$arch");
    expect(source).toContain('"CC_$targetKey=$llvmBin\\clang.exe"');
    expect(source).toContain('"AR_$targetKey=lib.exe"');
    expect(source).toContain("where.exe $tool");
    expect(
      source.indexOf("Activate matching Windows MSVC and LLVM toolchain"),
    ).toBeLessThan(
      source.indexOf("Build release binaries and local phone helper"),
    );
  });

  it("signs macOS and integrity-verifies the explicitly unsigned Windows and Linux packages", () => {
    expect(source).toContain("verify-windows-unsigned-production.mjs");
    expect(source).toContain("intentionally unsigned");
    expect(source).not.toContain("WINDOWS_SIGNING_CERTIFICATE_PFX_BASE64");
    expect(source).not.toContain("Import-PfxCertificate");
    expect(source).toContain("sign-notarize-macos-production.mjs");
    expect(source).toContain("package-macos-signed-production.mjs");
    expect(source).toContain("verify-linux-production-package.mjs");
    expect(source).toContain("notarytool store-credentials");
  });

  it("passes dispatch inputs through environment variables before using them in shell scripts", () => {
    expect(
      workflow.jobs.validate.steps.find(
        ({ id }: { id?: string }) => id === "identity",
      )?.env,
    ).toMatchObject({
      APPROVED_PDFIUM_RUN_ID: "${{ inputs.approved_pdfium_run_id }}",
      APPROVED_PDFIUM_RUN_ATTEMPT:
        "${{ inputs.approved_pdfium_run_attempt }}",
      BUILD_VERSION: "${{ inputs.build_version }}",
      SOURCE_REVISION: "${{ inputs.source_revision }}",
    });
    expect(workflow.jobs.package.env).toMatchObject({
      BUILD_VERSION: "${{ inputs.build_version }}",
      SOURCE_REVISION: "${{ inputs.source_revision }}",
    });
    for (const job of Object.values(workflow.jobs) as Array<{
      steps?: Array<{ run?: unknown }>;
    }>) {
      for (const step of job.steps ?? []) {
        if (typeof step.run === "string") {
          expect(step.run).not.toMatch(/\$\{\{\s*inputs\./);
        }
      }
    }
  });

  it("smokes exact uploaded packages on fresh native hosts without signing or installation authority", () => {
    const packageJob = workflow.jobs.package;
    const smokeJob = workflow.jobs.smoke;
    const packageSteps = packageJob.steps.map(
      ({ name }: { name?: string }) => name ?? "",
    );
    expect(packageSteps.some((name: string) => /Smoke/i.test(name))).toBe(
      false,
    );
    expect(packageSteps).toContain("Upload verified target candidate");

    const smokeMatrix = smokeJob.strategy.matrix.include;
    expect(smokeMatrix).toEqual(
      packageJob.strategy.matrix.include.map(
        ({
          label,
          runner,
          os,
          arch,
          optional,
        }: {
          label: string;
          runner: string;
          os: string;
          arch: string;
          optional: boolean;
        }) => ({ label, runner, os, arch, optional }),
      ),
    );
    expect(
      smokeMatrix.filter(({ optional }: { optional: boolean }) => !optional),
    ).toHaveLength(6);
    expect(smokeJob["continue-on-error"]).toBe("${{ matrix.optional }}");
    expect(smokeJob.needs).toEqual(["validate", "package"]);
    expect(smokeJob).not.toHaveProperty("environment");

    const smokeSource = JSON.stringify(smokeJob);
    expect(smokeJob.steps[0].with.ref).toBe("${{ inputs.source_revision }}");
    const download = smokeJob.steps.find(
      ({ name }: { name?: string }) =>
        name === "Download exact package from this workflow run",
    );
    expect(download?.with).toMatchObject({
      name: "gpui-package-${{ matrix.label }}",
      "run-id": "${{ github.run_id }}",
    });
    expect(smokeSource).toContain("smoke-nonmac-production-package.mjs");
    expect(smokeSource).toContain("smoke-signed-macos-package.mjs");
    expect(smokeSource).toContain("tests/fixtures/generated/single-page.pdf");
    expect(smokeSource).toContain("gpui-runtime-smoke-${{ matrix.label }}");
    expect(smokeSource).not.toMatch(
      /secrets\.|signing|notary|Import-PfxCertificate|package-(?:linux|windows|macos)|install-user|install\.ps1/i,
    );
    expect(smokeSource).not.toMatch(
      /gh release|create-release|contents:\\?"?write/i,
    );
  });

  it("aggregates only after required clean-host smoke and packages without publishing a release", () => {
    expect(source).toContain("aggregate-stable-candidate.mjs");
    expect(source).toContain("butter-paper/stable-candidate-input");
    expect(workflow.jobs.aggregate.needs).toEqual([
      "validate",
      "package",
      "smoke",
    ]);
    expect(workflow.jobs.aggregate.if).toContain(
      "needs.package.result == 'success'",
    );
    expect(workflow.jobs.aggregate.if).toContain(
      "needs.smoke.result == 'success'",
    );
    const aggregateDownloads = workflow.jobs.aggregate.steps.filter(
      ({ uses }: { uses?: string }) =>
        uses?.startsWith("actions/download-artifact@"),
    );
    expect(
      aggregateDownloads.map(
        ({ with: options }: { with: { pattern?: string } }) => options.pattern,
      ),
    ).toEqual(["gpui-package-*", "gpui-runtime-smoke-*"]);
    const aggregateSource = JSON.stringify(workflow.jobs.aggregate);
    expect(aggregateSource).toContain("runtimeEvidence");
    expect(aggregateSource).toContain("runtime-smoke.json");
    expect(aggregateSource).toContain("schemaVersion: 2");
    expect(aggregateSource).toContain("runtime?.passed === true");
    expect(aggregateSource).not.toContain("target === 'macos-x64'");
    expect(workflow.permissions).toEqual({ actions: "read", contents: "read" });
    expect(source).not.toMatch(
      /contents:\s*write|id-token:\s*write|gh release|create-release|publish never/,
    );
    const actions = [...source.matchAll(/uses: ([^\s]+) # /g)].map(
      (match) => match[1],
    );
    expect(actions.length).toBeGreaterThan(5);
    for (const action of actions) expect(action).toMatch(/@[0-9a-f]{40}$/);
  });
});
