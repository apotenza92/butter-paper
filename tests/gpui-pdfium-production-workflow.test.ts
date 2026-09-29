import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import YAML from "yaml";
import { describe, expect, it } from "vitest";

const path = ".github/workflows/build-gpui-pdfium-production.yml";
const source = readFileSync(path, "utf8");
const metadataScript = readFileSync(
  "experiments/gpui-migration/gpui-migration/scripts/prepare-pdfium-production-candidate.mjs",
  "utf8",
);
const workflow = YAML.parse(source);
const sharedLibraryPatchPath =
  "experiments/gpui-migration/gpui-migration/patches/pdfium-production-shared-library.patch";
const sharedLibraryPatch = readFileSync(sharedLibraryPatchPath, "utf8");
const sharedLibraryPatchSha256 = createHash("sha256")
  .update(sharedLibraryPatch)
  .digest("hex");
const dependencyPolicyPatchPath =
  "experiments/gpui-migration/gpui-migration/patches/pdfium-production-local-deps.patch";
const dependencyPolicyPatch = readFileSync(dependencyPolicyPatchPath, "utf8");
const dependencyPolicyPatchSha256 = createHash("sha256")
  .update(dependencyPolicyPatch)
  .digest("hex");
const targets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-pc-windows-msvc",
  "x86_64-pc-windows-msvc",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
];

describe("GPUI PDFium production candidate workflow", () => {
  it("does not interpolate dispatch inputs directly into shell scripts", () => {
    for (const job of Object.values(workflow.jobs) as Array<{ steps?: Array<{ run?: unknown }> }>) {
      for (const step of job.steps ?? []) {
        if (typeof step.run === "string") {
          expect(step.run).not.toMatch(/\$\{\{\s*inputs\./);
        }
      }
    }
  });

  it("builds all six required release targets", () => {
    const matrix = workflow.jobs.build.strategy.matrix.include;
    expect(matrix.map(({ target }: { target: string }) => target).sort()).toEqual(
      [...targets].sort(),
    );
    for (const target of matrix) {
      expect(target.runner).toBeTruthy();
      expect(target.cpu).toMatch(/^(arm64|x64)$/);
      expect(target.ninja_jobs).toBeGreaterThanOrEqual(1);
      expect(target.ninja_jobs).toBeLessThanOrEqual(3);
    }
    expect(matrix.find(({ target }: { target: string }) => target === "aarch64-unknown-linux-gnu")?.runner).toBe("ubuntu-24.04");
    expect(matrix.find(({ target }: { target: string }) => target === "aarch64-pc-windows-msvc")?.runner).toBe("windows-11-arm");
    expect(matrix.find(({ target }: { target: string }) => target === "x86_64-apple-darwin")?.optional).toBe(false);
    expect(matrix.filter(({ optional }: { optional: boolean }) => !optional)).toHaveLength(6);
    expect(workflow.jobs.build["continue-on-error"]).toBe("${{ matrix.optional }}");
    expect(workflow.on.workflow_dispatch).toBeDefined();
  });

  it("pins source, build tools, and third party actions", () => {
    expect(source).toContain(
      "PDFIUM_REVISION: 91b9d569b34be4f38eed7b3c49b227356c3aadad",
    );
    expect(source).toMatch(
      /DEPOT_TOOLS_REVISION: 7575b8253e91eec32feb636c07fb515b234285da/,
    );
    expect(workflow.jobs.build.env.DEPOT_TOOLS_UPDATE).toBe("0");
    expect(workflow.jobs.build.env.DEPOT_TOOLS_WIN_TOOLCHAIN).toBe("0");
    expect(source).toContain('git -C depot_tools rev-parse HEAD');
    expect(source).toContain('gclient revinfo --actual --output-json');
    expect(source).toContain('"$DEPOT_TOOLS_DIR/ensure_bootstrap"');
    expect(source).toContain('export DEPOT_TOOLS_DIR="$PWD/depot_tools"');
    expect(source).toContain("pdfium-production-local-deps.patch");
    expect(source).toContain("update-index --assume-unchanged DEPS");
    expect(source).toContain("update-index --no-assume-unchanged DEPS");
    expect(source).toContain('test "$(git -C pdfium diff --name-only)" = "DEPS"');
    expect(source).toContain('git -C pdfium apply --reverse --check "$dependency_patch"');
    expect(source).toContain("depot_tools\\bootstrap\\win_tools.bat");
    expect(source).toContain("Microsoft.VisualStudio.Component.VC.Tools.ARM64");
    expect(source).toContain("Microsoft.VisualStudio.Component.VC.Tools.x86.x64");
    expect(source).toContain("-requires $component -format json | ConvertFrom-Json");
    expect(source).toContain("$instances[0].catalog.productLineVersion");
    expect(source).toContain("'^(17|2022)$' { '2022'; break }");
    expect(source).toContain("'^(18|2026)$' { '2026'; break }");
    expect(source).toContain('"vs${vsVersion}_install=$installation" >> $env:GITHUB_ENV');
    expect(source).toContain('"GYP_MSVS_OVERRIDE_PATH=$installation" >> $env:GITHUB_ENV');
    expect(source).toContain('"GYP_MSVS_VERSION=$vsVersion" >> $env:GITHUB_ENV');
    expect(source).toContain('$debuggerArchitectures = if ("${{ matrix.cpu }}" -eq "arm64")');
    expect(source).toContain('@("arm64", "x64")');
    expect(source).toContain('@("x64")');
    expect(source).toContain('"Windows Kits\\10\\Debuggers\\$debuggerCpu\\dbghelp.dll"');
    expect(source).toContain("Windows SDK Debugging Tools runtime for $debuggerCpu is required");
    expect(source).not.toContain('"Windows Kits\\10\\Debuggers\\${{ matrix.cpu }}\\dbghelp.dll"');
    expect(source).not.toContain("choco install ninja");
    expect(source).not.toContain("ninja-build");
    const actions = [...source.matchAll(/uses: ([^\s]+) # /g)].map((match) => match[1]);
    expect(actions.length).toBeGreaterThanOrEqual(2);
    for (const action of actions) expect(action).toMatch(/@[0-9a-f]{40}$/);
    expect(source).toMatch(/pdf_enable_v8 = false/);
    expect(source).toMatch(/pdf_enable_xfa = false/);
    expect(source).toMatch(/is_debug = false/);
    expect(source).toMatch(/clang_use_chrome_plugins = false/);
    expect(source).toMatch(/use_remoteexec = false/);
    expect(source).toMatch(/pdf_use_skia = false/);
    expect(source).toMatch(/pdf_enable_fontations = false/);
    expect(source).toContain(
      'python3 build/linux/sysroot_scripts/install-sysroot.py --arch="$sysroot_arch"',
    );
    expect(source).toContain("git -c core.autocrlf=false clone");
    expect(source).toContain("git -C pdfium config core.autocrlf false");
    expect(source).toContain("Prepare exact reviewed patch inputs");
    expect(source).toContain("tr -d '\\r'");
    expect(source).toContain("PDFIUM_DEPENDENCY_PATCH");
    expect(source).toContain("PDFIUM_SHARED_PATCH");
    expect(source).toContain('arm64) sysroot_arch="arm64"');
    expect(source).toContain('x64) sysroot_arch="amd64"');
    expect(source).toContain("git -C pdfium apply --check");
    expect(source).toContain("git -C pdfium apply \"$patch\"");
  });

  it("uses the pinned shared-library export patch and records its digest", () => {
    expect(sharedLibraryPatch).toContain('component("pdfium")');
    expect(sharedLibraryPatch).toContain('shared_library("pdfium")');
    expect(sharedLibraryPatch).toContain("index c28cc01ec..7920dc4ac 100644");
    expect(sharedLibraryPatch).toContain("index 4910eac66..5dfae6820 100644");
    expect(sharedLibraryPatch).toContain("#if defined(COMPONENT_BUILD)");
    expect(sharedLibraryPatch).toContain("-#if defined(COMPONENT_BUILD)");
    expect(sharedLibraryPatch).toContain("-#define FPDF_EXPORT");
    expect(metadataScript).toContain("sharedLibraryPatchSha256");
    expect(metadataScript).toContain("dependencyPolicyPatchSha256");
    expect(metadataScript.match(/sharedLibraryPatchSha256:\s*sha256\(sharedPatch\)/g)).toHaveLength(2);
    expect(sharedLibraryPatchSha256).toMatch(/^[0-9a-f]{64}$/);
    expect(dependencyPolicyPatch).toContain("'condition': 'download_remoteexec_cfg'");
    expect(dependencyPolicyPatchSha256).toBe(
      "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b",
    );
  });

  it("expects the shared binary names emitted for each platform", () => {
    const matrix = workflow.jobs.build.strategy.matrix.include;
    const expectedLibraries: Record<string, string> = {
      macos: "libpdfium.dylib",
      windows: "pdfium.dll",
      linux: "libpdfium.so",
    };
    for (const target of matrix) {
      expect(target.library).toBe(expectedLibraries[target.os]);
    }
    expect(source).toContain('autoninja -C out/Production -j "$NINJA_JOBS" pdfium');
    expect(source).toContain("pdfium-gn-version.txt");
    expect(source).toContain("pdfium-ninja-version.txt");
    expect(source).toContain("pdfium-siso-version.txt");
    expect(source).toContain("pdfium-clang-version.txt");
    expect(source).not.toContain('subprocess.check_output(["gn","--version"]');
    expect(source).toContain("out/Production/libpdfium.dylib");
    expect(source).toContain("out/Production/pdfium.dll");
    expect(source).toContain("out/Production/libpdfium.so");
  });

  it("emits review metadata without granting approval, write permissions, or signing keys", () => {
    expect(source).toContain("prepare-pdfium-production-candidate.mjs");
    expect(source).toContain('"$RUNNER_TEMP/pdfium-dependency-revisions.json"');
    expect(source).toContain("--dependency-revisions");
    expect(source).toContain("--runner-image-os");
    expect(source).toContain("--runner-image-version");
    expect(source).toContain("--runner-os");
    expect(source).toContain("--runner-arch");
    expect(source).not.toContain("python3 - <<'PY'");
    expect(source).toContain("actions/upload-artifact@");
    expect(source).toContain("github.run_id");
    expect(workflow.permissions).toEqual({ contents: "read" });
    expect(source).not.toMatch(/secrets\.|id-token:\s*write|contents:\s*write/);
    expect(source).toContain("if-no-files-found: error");
    expect(source).toContain("Record initial runner resources");
    expect(source).toContain("Record synced source resources");
    expect(source).toContain("Record final runner resources");
    expect(metadataScript).toContain("runnerImageVersion");
    expect(metadataScript).toContain("runnerArch");
  });
});
