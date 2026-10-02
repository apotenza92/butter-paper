import { readFileSync } from "node:fs";
import YAML from "yaml";
import { describe, expect, it } from "vitest";

// The Homebrew tap's registry names this file as Butter Paper's release
// workflow; renaming it breaks Homebrew publication.
const path = ".github/workflows/release.yml";
const source = readFileSync(path, "utf8");
const workflow = YAML.parse(source);
// The build matrix comes from this table (macOS only for beta tags).
const releaseTargets = JSON.parse(
  readFileSync(".github/release-targets.json", "utf8"),
);

const targets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-pc-windows-msvc",
  "x86_64-pc-windows-msvc",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
];

describe("Release workflow", () => {
  it("keeps production binary messages clear of release-verifier marker strings", () => {
    for (const nativeSource of [
      "experiments/gpui-migration/gpui-migration/src/bin/gpui-migration.rs",
      "experiments/gpui-migration/gpui-migration/src/native_runtime_layout.rs",
    ]) {
      const nativeText = readFileSync(nativeSource, "utf8");
      expect(nativeText).not.toContain("development PDFium overrides");
      expect(nativeText).not.toContain("PDFium override basename");
    }
  });

  it("releases from a pushed vX.Y.Z or vX.Y.Z-beta.N tag", () => {
    expect(workflow.on).toEqual({ push: { tags: ["v*"] } });
    expect(source).toContain("^v([0-9]+)\\.([0-9]+)\\.([0-9]+)$");
    expect(source).toContain("-beta\\.([1-9][0-9]{0,3})$");
    // As Macsimize: a release builds above its own betas.
    expect(source).toContain(
      "build_number=$(( ((major * 1000000) + (minor * 1000) + patch) * 100000 + stage ))",
    );
    expect(workflow.concurrency).toEqual({
      group: "butter-paper-release",
      "cancel-in-progress": false,
    });
  });

  it("packages all six targets, and Butter Paper Beta for macOS, in one release", () => {
    expect(workflow.jobs.package.strategy.matrix.include).toBe(
      "${{ fromJSON(needs.validate.outputs.targets) }}",
    );
    expect(
      releaseTargets.map(({ target }: { target: string }) => target).sort(),
    ).toEqual([...targets].sort());
    const entry = (target: string) =>
      releaseTargets.find((candidate: { target: string }) => candidate.target === target);
    expect(entry("aarch64-pc-windows-msvc")?.runner).toBe("windows-11-arm");
    expect(entry("aarch64-unknown-linux-gnu")?.runner).toBe("ubuntu-24.04-arm");
    // Intel macOS is cross-compiled on the Apple silicon runner.
    expect(entry("x86_64-apple-darwin")?.runner).toBe("macos-26");
    expect(entry("x86_64-apple-darwin")?.node_arch).toBe("arm64");
    // Each macOS job packages both identities from one build; a beta tag
    // builds Butter Paper Beta for macOS only.
    expect(source).toContain("for channel in ${{ matrix.channels }}; do");
    expect(source).toContain("channels: beta ? 'beta' : 'stable beta'");
    expect(source).toContain(".filter((target) => !beta || target.os === 'macos')");
    expect(source).toContain("name: gpui-package-${{ matrix.label }}-beta");
    expect(workflow.jobs.aggregate.if).toContain("always()");
    expect(source).toContain("aggregate-stable-candidate.mjs");
  });

  it("caches Cargo downloads only, never build output", () => {
    const cache = workflow.jobs.package.steps.find(
      ({ name }: { name?: string }) => name === "Cache Cargo downloads",
    );
    expect(cache.with.path.trim().split("\n")).toEqual([
      "~/.cargo/registry/index",
      "~/.cargo/registry/cache",
      "~/.cargo/git/db",
    ]);
    expect(source).not.toMatch(/path:[^\n]*cargo-target/);
  });

  it("binds the tag's commit on main and the human-approved PDFium handoff", () => {
    expect(source).toContain('test "$(git rev-parse "$TAG^{commit}")" = "$SOURCE_REVISION"');
    expect(source).toContain("git merge-base --is-ancestor HEAD origin/main");
    expect(source).toContain('grep -qxF "## [$version]" CHANGELOG.md');
    expect(source).toContain("require('./package.json').version");
    expect(
      source.match(/ref: \$\{\{ github\.sha \}\}/g)?.length,
    ).toBeGreaterThanOrEqual(4);
    expect(source).toContain("production-pdfium-approved.json");
    expect(source).toContain("vars.BP_PDFIUM_APPROVAL_RUN_ID");
    expect(source).toContain("run.path !== '.github/workflows/approve-gpui-pdfium-production.yml'");
    expect(source).toContain("run.head_repository?.full_name !== repository");
    expect(source).toContain("approved.expired");
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

  it("fetches the locked Rust graph before enforcing offline source verification", () => {
    const fetch =
      'cargo +1.97.1 fetch --locked --manifest-path "$MIGRATION_ROOT/Cargo.toml"';
    expect(source).toContain(fetch);
    expect(source.indexOf(fetch)).toBeLessThan(
      source.indexOf('node "$MIGRATION_ROOT/scripts/verify-cargo-graph.mjs"'),
    );
  });

  it("provides pinned Python for the cross-platform phone helper preparation", () => {
    expect(source).toContain(
      "actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1",
    );
    expect(source).toContain("python-version: 3.12.10");
    const steps = workflow.jobs.package.steps;
    const setupPythonIndex = steps.findIndex((step: { uses?: string }) =>
      step.uses?.startsWith("actions/setup-python@"),
    );
    const buildStep = steps.find(
      (step: { name?: string }) =>
        step.name === "Build release binaries and local phone helper",
    );
    expect(setupPythonIndex).toBeGreaterThanOrEqual(0);
    expect(buildStep?.shell).toBe("bash");
    expect(buildStep?.run).toContain('if [[ "$RUNNER_OS" == "Windows" ]]');
    expect(buildStep?.run).toContain(
      'python_executable="${pythonLocation//\\\\//}/python.exe"',
    );
    expect(buildStep?.run).toContain('"$python_executable" "$PHONE_ROOT/prepare.py"');
    expect(buildStep?.run).toContain('python "$PHONE_ROOT/prepare.py"');
    expect(buildStep?.run.indexOf('if [[ "$RUNNER_OS" == "Windows" ]]')).toBeLessThan(
      buildStep?.run.indexOf('python "$PHONE_ROOT/prepare.py"'),
    );
    expect(setupPythonIndex).toBeLessThan(steps.indexOf(buildStep));
  });

  it("installs Linux native build prerequisites before building", () => {
    expect(source).toContain("Install Linux native build prerequisites");
    for (const dependency of [
      "libdbus-1-dev",
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
    expect(
      source.indexOf("Install Linux native build prerequisites"),
    ).toBeLessThan(
      source.indexOf("Build release binaries and local phone helper"),
    );
  });

  it("creates nested per-target candidate output directories before packaging", () => {
    expect(source).toContain('mkdir -p "$input" "$output"');
    expect(source).toContain('mkdir -p "$work" "$output"');
    expect(source).toContain('chmod 700 "$output"');
    expect(source).toContain('chmod 700 "$signing"');
  });

  it("activates architecture-matched MSVC and LLVM tools before Windows builds", () => {
    const matrix = releaseTargets;
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
    expect(source).toContain(
      '"CARGO_TARGET_${cargoTargetKey}_LINKER=$($resolvedTools[\'link.exe\'])"',
    );
    expect(source).toContain("where.exe $tool");
    expect(source).toContain("$gitBash = 'C:\\Program Files\\Git\\bin\\bash.exe'");
    expect(source).toContain("Split-Path -Parent $gitBash");
    expect(source).toContain("& $gitBash --version");
    // Later GITHUB_PATH entries are searched first: Git Bash must be added
    // after the VsDevCmd path, whose System32 holds WSL's bash.exe.
    expect(source.indexOf("Split-Path -Parent $gitBash |")).toBeGreaterThan(
      source.indexOf("$pathValue.Split(';'")
    );
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

  it("passes tag and approval values through environment variables before using them in shell scripts", () => {
    expect(
      workflow.jobs.validate.steps.find(
        ({ id }: { id?: string }) => id === "identity",
      )?.env,
    ).toMatchObject({
      APPROVED_PDFIUM_RUN_ID: "${{ vars.BP_PDFIUM_APPROVAL_RUN_ID }}",
      APPROVED_PDFIUM_RUN_ATTEMPT: "${{ vars.BP_PDFIUM_APPROVAL_RUN_ATTEMPT }}",
      SOURCE_REVISION: "${{ github.sha }}",
      TAG: "${{ github.ref_name }}",
    });
    expect(workflow.jobs.package.env).toMatchObject({
      BUILD_VERSION: "${{ needs.validate.outputs.build_number }}",
      SOURCE_REVISION: "${{ github.sha }}",
    });
    for (const job of Object.values(workflow.jobs) as Array<{
      steps?: Array<{ run?: unknown }>;
    }>) {
      for (const step of job.steps ?? []) {
        if (typeof step.run === "string") {
          expect(step.run).not.toMatch(/\$\{\{\s*(inputs|github\.ref|github\.head_ref|vars)\b/);
        }
      }
    }
  });

  it("publishes one complete immutable release, then hands Homebrew to the tap", () => {
    expect(source).toContain("aggregate-stable-candidate.mjs");
    expect(source).toContain("butter-paper/stable-candidate-input");
    expect(workflow.jobs.aggregate.needs).toEqual(["validate", "package"]);
    expect(workflow.jobs.aggregate.if).toContain(
      "needs.package.result == 'success'",
    );
    expect(workflow.permissions).toEqual({ actions: "read", contents: "read" });
    // Only the publish job can write a release, and only after every package
    // is signed or verified and aggregated; signing jobs never can.
    // (The homebrew job's app token writes to the tap repository only; its
    // own permissions are checked below.)
    for (const [name, job] of Object.entries(workflow.jobs) as [string, any][]) {
      if (name === "publish" || name === "homebrew") continue;
      expect(JSON.stringify(job)).not.toMatch(/contents"?:\s*"?write|gh release|id-token/);
    }
    expect(workflow.jobs.publish.needs).toEqual(["validate", "aggregate"]);
    expect(workflow.jobs.publish.permissions).toEqual({
      contents: "write",
      "id-token": "write",
      attestations: "write",
    });
    expect(workflow.jobs.publish).not.toHaveProperty("environment");
    const publish = JSON.stringify(workflow.jobs.publish);
    // Immutable releases: a draft carries every asset, which is checked
    // before the single publication.
    expect(publish).toContain("--draft --verify-tag");
    expect(publish).toContain("--draft=false");
    expect(publish).toContain("already exists; releases are immutable");
    expect(publish).toContain("SHA256SUMS.txt");
    expect(publish).toContain("build-homebrew-publication.mjs");
    expect(publish).toContain("homebrew-publication.tar.gz");
    expect(publish).toContain("actions/attest@");
    expect(publish).toContain(".immutable");
    expect(publish).not.toContain("--clobber");
    // The tap is asked to publish with the dispatch-only app token.
    expect(workflow.jobs.homebrew.needs).toEqual(["validate", "publish"]);
    expect(workflow.jobs.homebrew.environment).toBe("homebrew-dispatch");
    expect(workflow.jobs.homebrew.permissions).toEqual({ contents: "read" });
    const homebrew = JSON.stringify(workflow.jobs.homebrew);
    expect(homebrew).toContain("publish-homebrew-v1");
    expect(homebrew).toContain("repos/apotenza92/homebrew-tap/dispatches");
    expect(homebrew).toContain("--arg product butter-paper");
    const actions = [...source.matchAll(/uses: ([^\s]+) # /g)].map(
      (match) => match[1],
    );
    expect(actions.length).toBeGreaterThan(5);
    for (const action of actions) expect(action).toMatch(/@[0-9a-f]{40}$/);
  });
});
