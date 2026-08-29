import assert from "node:assert/strict";
import { access, mkdtemp, mkdir, readFile, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import {
  allocatedKiB,
  assertAllowlistedTarget,
  cleanupTarget,
  cleanupDisposition,
  evaluatePreflight,
  evaluateRuntimeSample,
  initializeTarget,
  loadBuildGuardPolicy,
  resolveRunnerMode,
} from "../scripts/build-guard.mjs";

const GiB_IN_KiB = 1024 * 1024;
const policy = {
  preflightFreeKiB: 30 * GiB_IN_KiB,
  runtimeStopFreeKiB: 20 * GiB_IN_KiB,
  absoluteMinFreeKiB: 18 * GiB_IN_KiB,
  maxTargetKiB: 5 * GiB_IN_KiB,
  minMemoryKiB: 350_000,
};
const nativeRuntimeSourceHashes = [
  "Cargo.toml",
  "Cargo.lock",
  "src/lib.rs",
  "src/native_runtime_layout.rs",
  "src/bin/component_story.rs",
  "src/bin/butter-paper-pdf-worker.rs",
  "../gpui-gallery/src/pdf_worker.rs",
  "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
  "scripts/run-native-reader.sh",
  "scripts/build-guard.mjs",
  "scripts/run-bounded-button-probe.sh",
];
const saveAsPersistenceSourceHashes = [
  "Cargo.toml",
  "Cargo.lock",
  "../gpui-gallery/Cargo.toml",
  "../gpui-gallery/Cargo.lock",
  "../gpui-gallery/src/pdf_file_authority.rs",
  "../gpui-gallery/src/annotation_model.rs",
  "../gpui-gallery/src/pdf_engine.rs",
  "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
  "../gpui-gallery/tests/pdf_persistence.rs",
  "tests/document_workspace.rs",
  "scripts/build-guard.mjs",
  "scripts/run-bounded-button-probe.sh",
];
const nativeShellCoreEditorSourceHashes = [
  "Cargo.toml",
  "Cargo.lock",
  "src/lib.rs",
  "src/application_close.rs",
  "src/application_close_workspace.rs",
  "src/bin/component_story.rs",
  "src/document_resource.rs",
  "src/document_session.rs",
  "src/document_viewer.rs",
  "src/document_workspace.rs",
  "src/ink_property_inspector.rs",
  "src/native_document_view_state.rs",
  "src/straight_line_property_inspector.rs",
  "src/text_box_property_inspector.rs",
  "../gpui-gallery/Cargo.toml",
  "../gpui-gallery/Cargo.lock",
  "../gpui-gallery/src/annotation_adapter.rs",
  "../gpui-gallery/src/annotation_model.rs",
  "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
  "../gpui-gallery/src/image_asset_decode.rs",
  "../gpui-gallery/src/pdf_engine.rs",
  "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
  "../gpui-gallery/src/pdf_file_authority.rs",
  "../gpui-gallery/src/pdf_worker.rs",
  "tests/application_close_integration.rs",
  "tests/build-guard.test.mjs",
  "scripts/build-guard.mjs",
  "scripts/run-bounded-button-probe.sh",
  "../performance/results/public-fixtures-v1/fixture-index.json",
  "../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf",
  "../performance/results/public-fixtures-v1/bp-image-checker-v1.png",
];

test("the runner anchors Cargo to the pinned probe toolchain", async () => {
  const runner = await readFile(
    new URL("../scripts/run-bounded-button-probe.sh", import.meta.url),
    "utf8",
  );
  const unsetOverride = runner.indexOf("unset RUSTUP_TOOLCHAIN");
  const enterProbe = runner.indexOf('cd "$probe_dir" || exit 2');
  const cargoProcessGroup = runner.indexOf("setsid env \\");

  assert.ok(unsetOverride >= 0, "the caller toolchain override must be cleared");
  assert.ok(enterProbe >= 0, "the runner must enter the pinned probe directory");
  assert.ok(cargoProcessGroup >= 0, "the Cargo process group launch must exist");
  assert.ok(unsetOverride < cargoProcessGroup);
  assert.ok(enterProbe < cargoProcessGroup);
  assert.match(
    runner.slice(cargoProcessGroup, cargoProcessGroup + 160),
    /setsid env \\\n\s+-u RUSTUP_TOOLCHAIN \\/,
  );
  assert.match(runner, /test\|build\|check/);
  assert.match(runner, /sourceHashes/);
});

test("the runner waits for the complete Cargo process group before safety cleanup", async () => {
  const runner = await readFile(
    new URL("../scripts/run-bounded-button-probe.sh", import.meta.url),
    "utf8",
  );
  assert.match(runner, /kill -0 -- "-\$build_pgid"/);
  assert.match(runner, /while \[\[ -n "\$build_pgid"[\s\S]+sleep 0\.1/);
});

test("the focused real-PDF runner binds inputs and hashes exact runtime artifacts", async () => {
  const runner = await readFile(
    new URL("../scripts/run-bounded-button-probe.sh", import.meta.url),
    "utf8",
  );
  assert.match(runner, /native-shell-rectangle-real\|native-shell-pen-highlight-real\|native-shell-text-box-real\|native-shell-core-editor-real\|native-shell-focused-real/);
  assert.match(runner, /fixture-index\.json/);
  assert.match(runner, /pdfium-development-binaries\.json/);
  assert.match(runner, /\.butter-paper-pdfium-development\.json/);
  assert.match(runner, /BP_PDFIUM_LIBRARY is not bound to the reviewed development receipt/);
  assert.match(runner, /fixture\.artifacts\.pdf\.sha256 !== fixtureSha256/);
  assert.match(runner, /bp-annotation-all-v1\.pdf/);
  assert.match(runner, /regularPng\.sha256 !== regularPngSha256/);
  assert.match(runner, /assets: \{[\s\S]+regularPng:/);
  assert.match(runner, /runtimeVerification/);
  assert.match(runner, /runtimeArtifactHashes/);
  assert.match(runner, /artifact_hash_status/);
  assert.match(runner, /runtime-artifact-hash/);
  assert.match(runner, /post_success_failure_status/);
  assert.doesNotMatch(
    runner,
    /runtime_artifact_hashes_json=\$\([\s\S]+?NODE\n  \) \|\| exit 2/,
  );
  assert.match(runner, /src\/dimension_property_inspector\.rs/);
  assert.match(runner, /expected one exact \$\{testTarget\} test binary/);
  assert.match(runner, /callout-cutover-real\|cloud-plus-cutover-real\|dimension-cutover-real\|arc-cutover-real\|measurement-path-cutover-real/);
  assert.match(runner, /testTarget = runnerMode\.startsWith\("native-shell-"\)/);
  assert.doesNotMatch(runner, /fetch-pdfium-development/);
});

test("the native reader launcher is fixed to reviewed development artifacts", async () => {
  const runner = await readFile(
    new URL("../scripts/run-native-reader.sh", import.meta.url),
    "utf8",
  );
  assert.match(runner, /if \(\( \$# > 1 \)\)/);
  assert.match(runner, /\.build-targets\/gpui-component-compat\/debug/);
  assert.match(runner, /story="\$target_dir\/component_story"/);
  assert.match(runner, /butter-paper-pdf-worker/);
  assert.match(runner, /pdfium-development-binaries\.json/);
  assert.match(runner, /\.butter-paper-pdfium-development\.json/);
  assert.match(runner, /bp-multi-page-v1\.pdf/);
  assert.match(runner, /node --input-type=module -/);
  assert.match(runner, /exec env[\s\\]+BP_NATIVE_DEVELOPMENT=1/);
  assert.match(runner, /BP_PDFIUM_LIBRARY="\$pdfium_library"/);
  assert.match(runner, /BP_GPUI_PERF_\*/);
  assert.match(runner, /BP_PDF_WORKER_EXE/);
  assert.match(
    runner,
    /if \(\( \$# == 0 \)\); then[\s\S]+default public multi-page fixture is missing[\s\S]+else/,
  );
  assert.doesNotMatch(runner, /fetch-pdfium-development/);
  assert.doesNotMatch(runner, /benchmark-evidence/);
});

test("checked-in policy preserves the reviewed host limits", async () => {
  const actual = await loadBuildGuardPolicy();
  assert.equal(actual.preflightFreeKiB, 30 * GiB_IN_KiB);
  assert.equal(actual.runtimeStopFreeKiB, 20 * GiB_IN_KiB);
  assert.equal(actual.absoluteMinFreeKiB, 18 * GiB_IN_KiB);
  assert.equal(actual.maxTargetKiB, 5 * GiB_IN_KiB);
  assert.equal(actual.cargoBuildJobs, 1);
  assert.equal(actual.cargoIncremental, 0);
  assert.equal(actual.rustMinStackBytes, 16 * 1024 * 1024);
  assert.equal(
    actual.targetRelativeToMigration,
    ".build-targets/gpui-component-compat",
  );
  assert.deepEqual(actual.additionalTargetRelativesToMigration, [
    ".build-targets/gpui-gallery-backend",
    ".build-targets/gpui-component-performance",
  ]);
});

test("cold Rust profiles use one codegen unit to stay under the five GiB target cap", async () => {
  const manifest = await readFile(
    new URL("../Cargo.toml", import.meta.url),
    "utf8",
  );
  const profileBlocks = ["dev", "test"].map((profile) => {
    const match = manifest.match(
      new RegExp(`\\[profile\\.${profile}\\]([\\s\\S]*?)(?=\\n\\[|$)`),
    );
    assert.ok(match, `profile.${profile} must be explicit`);
    return match[1];
  });
  for (const block of profileBlocks) {
    assert.match(block, /^codegen-units = 1$/m);
    assert.match(block, /^incremental = false$/m);
    assert.match(block, /^debug = 0$/m);
    assert.match(block, /^strip = "symbols"$/m);
  }
});

test("preflight refuses less than thirty GiB without starting a build", () => {
  assert.deepEqual(
    evaluatePreflight(
      { freeKiB: 30 * GiB_IN_KiB - 1, memoryAvailableKiB: 2 * GiB_IN_KiB },
      policy,
    ),
    { ok: false, reason: "preflight-free-space" },
  );
  assert.deepEqual(
    evaluatePreflight(
      { freeKiB: 30 * GiB_IN_KiB, memoryAvailableKiB: 2 * GiB_IN_KiB },
      policy,
    ),
    { ok: true, reason: null },
  );
});

test("runtime sample stops at twenty GiB to protect the eighteen GiB absolute floor", () => {
  assert.deepEqual(
    evaluateRuntimeSample(
      {
        freeKiB: 20 * GiB_IN_KiB - 1,
        targetKiB: GiB_IN_KiB,
        memoryAvailableKiB: 2 * GiB_IN_KiB,
      },
      policy,
    ),
    { ok: false, reason: "runtime-free-space" },
  );
  assert.deepEqual(
    evaluateRuntimeSample(
      {
        freeKiB: 20 * GiB_IN_KiB,
        targetKiB: GiB_IN_KiB,
        memoryAvailableKiB: 2 * GiB_IN_KiB,
      },
      policy,
    ),
    { ok: true, reason: null },
  );
  assert.deepEqual(
    evaluateRuntimeSample(
      {
        freeKiB: 30 * GiB_IN_KiB,
        targetKiB: 5 * GiB_IN_KiB + 1,
        memoryAvailableKiB: 2 * GiB_IN_KiB,
      },
      policy,
    ),
    { ok: false, reason: "target-size" },
  );
});

test("ordinary failures and interruptions retain the owned Cargo target", () => {
  assert.deepEqual(cleanupDisposition({ status: 0 }), {
    action: "retain",
    reason: "successful-run",
  });
  assert.deepEqual(cleanupDisposition({ status: 101 }), {
    action: "retain",
    reason: "cargo-status-101",
  });
  assert.deepEqual(
    cleanupDisposition({ status: 124, limitReason: "wall-time" }),
    {
      action: "retain",
      reason: "wall-time",
    },
  );
  assert.deepEqual(
    cleanupDisposition({ status: 125, limitReason: "interrupted-TERM" }),
    {
      action: "retain",
      reason: "interrupted-TERM",
    },
  );
  assert.deepEqual(
    cleanupDisposition({ status: 125, limitReason: "available-memory" }),
    {
      action: "retain",
      reason: "available-memory",
    },
  );
});

test("only explicit cleanup and disk-safety conditions clean the owned target", () => {
  for (const reason of [
    "preflight-free-space",
    "runtime-free-space",
    "target-size",
  ]) {
    assert.deepEqual(cleanupDisposition({ status: 125, limitReason: reason }), {
      action: "clean",
      reason,
    });
  }
  assert.deepEqual(cleanupDisposition({ status: 101, explicitCleanup: true }), {
    action: "clean",
    reason: "explicit-cleanup",
  });
});

test("runner modes expose only fixed reviewed Cargo argument arrays", () => {
  assert.deepEqual(resolveRunnerMode("all-targets"), {
    name: "all-targets",
    cargoArgs: ["--all-targets"],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      ...nativeRuntimeSourceHashes,
      "src/application_close_workspace.rs",
      "src/document_workspace.rs",
      "tests/application_close_integration.rs",
    ],
  });
  assert.deepEqual(resolveRunnerMode("ink-properties"), {
    name: "ink-properties",
    cargoArgs: ["--test", "document_workspace", "ink_property_inspector_"],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "src/ink_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("engineering-visual-properties"), {
    name: "engineering-visual-properties",
    cargoArgs: ["--test", "document_workspace", "engineering_visual_inspector_"],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "src/engineering_visual_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("engineering-visual-properties-real"), {
    name: "engineering-visual-properties-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_engineering_visual_properties_create_edit_save_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/engineering_visual_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("text-box-properties"), {
    name: "text-box-properties",
    cargoArgs: ["--test", "document_workspace", "text_box_property_inspector_"],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "src/text_box_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("measurement-properties"), {
    name: "measurement-properties",
    cargoArgs: ["--test", "document_workspace", "measurement_property_"],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "src/measurement_property_inspector.rs",
      "src/page_scale_control.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("document-tab-bar"), {
    name: "document-tab-bar",
    cargoArgs: ["--test", "document_tab_bar"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("pointer-drag"), {
    name: "pointer-drag",
    cargoArgs: ["--test", "document_tab_bar", "pointer_drag"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-spine"), {
    name: "document-spine",
    cargoArgs: ["--test", "document_workspace"],
    controlledFailureStatus: null,
    sourceHashRelatives: saveAsPersistenceSourceHashes,
  });
  assert.deepEqual(resolveRunnerMode("compat-signature"), {
    name: "compat-signature",
    cargoArgs: ["--test", "document_workspace", "local_signature_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("signature-real"), {
    name: "signature-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_signature_image_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-image-real"), {
    name: "document-image-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_regular_png_image_create_move_resize_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/lib.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/native_application.rs",
      "src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/image_asset_decode.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "tests/document_workspace.rs",
      "tests/build-guard.test.mjs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
      "../performance/results/public-fixtures-v1/fixture-index.json",
      "../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf",
      "../performance/results/public-fixtures-v1/bp-image-checker-v1.png",
    ],
  });
  assert.deepEqual(resolveRunnerMode("document-coordinate-space"), {
    name: "document-coordinate-space",
    cargoArgs: [
      "--test",
      "document_workspace",
      "opened_document_retains_coordinate_space",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-coordinate-space-real"), {
    name: "document-coordinate-space-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_user_unit_coordinate_space_renders_edits_saves_reopens_and_releases",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("session-tab-strip"), {
    name: "session-tab-strip",
    cargoArgs: ["--test", "document_workspace", "real_session_tab_strip_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-save"), {
    name: "document-save",
    cargoArgs: ["--test", "document_workspace", "in_place_save_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-save-real"), {
    name: "document-save-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_in_place_save_replaces_the_opened_pdf_and_reopens_cleanly",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-save-collision-real"), {
    name: "document-save-collision-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_save_as_collision_recovers_to_fresh_target_and_reopens",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("two-document-save-failure-real"), {
    name: "two-document-save-failure-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_two_document_dirty_save_as_failure_is_isolated_and_recovers",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/lib.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/native_document_view_state.rs",
      "src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "tests/document_workspace.rs",
      "tests/build-guard.test.mjs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("viewer-state-real"), {
    name: "viewer-state-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_preserves_independent_view_state_through_fit_scroll_thumbnail_zoom_and_document_switch",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/lib.rs",
      "src/accessible_button.rs",
      "src/adaptive_performance.rs",
      "src/cad_view_control.rs",
      "src/continuous_view_control.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_tab_bar.rs",
      "src/document_viewer.rs",
      "src/document_workspace.rs",
      "src/native_document_view_state.rs",
      "src/page_view_control.rs",
      "src/viewer_icons.rs",
      "src/viewer_toolbar_strip.rs",
      "src/zoom_control.rs",
      "src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/highlight_compositor.rs",
      "../gpui-gallery/src/page_geometry.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "../gpui-gallery/src/viewer.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "tests/document_workspace.rs",
      "tests/build-guard.test.mjs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
      "../performance/results/public-fixtures-v1/fixture-index.json",
      "../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf",
    ],
  });
  assert.deepEqual(resolveRunnerMode("document-template"), {
    name: "document-template",
    cargoArgs: ["--test", "document_workspace", "generated_template_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("template-manager"), {
    name: "template-manager",
    cargoArgs: ["--test", "template_manager"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("line-arrow-workspace"), {
    name: "line-arrow-workspace",
    cargoArgs: ["--test", "document_workspace", "line_arrow_workspace_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("line-arrow-pointer-workspace-exact"), {
    name: "line-arrow-pointer-workspace-exact",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "line_arrow_workspace_pointer_create_body_move_and_endpoint_edit_share_one_history_contract",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("polyline-polygon-pointer-workspace-exact"), {
    name: "polyline-polygon-pointer-workspace-exact",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "polyline_polygon_workspace_pointer_create_finish_cancel_move_vertex_lock_history",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("straight-line-properties-exact"), {
    name: "straight-line-properties-exact",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "straight_line_inspector_renders_exact_line_arrow_controls_and_revalidates_each_event",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/lib.rs",
      "src/document_workspace.rs",
      "src/straight_line_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("vertex-path-properties-exact"), {
    name: "vertex-path-properties-exact",
    cargoArgs: [
      "--test", "document_workspace", "--", "--exact",
      "vertex_path_inspector_exact_controls_revalidate_and_preserve_hidden_state",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/lib.rs",
      "src/document_workspace.rs",
      "src/vertex_path_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("vertex-path-adapter-properties-exact"), {
    name: "vertex-path-adapter-properties-exact",
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    cargoArgs: [
      "--test", "annotation_adapter", "--", "--exact",
      "selected_vertex_path_properties_preserve_hidden_appearance_and_are_exact",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/tests/annotation_adapter.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("line-arrow-save-reopen-real-exact"), {
    name: "line-arrow-save-reopen-real-exact",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_line_arrow_save_as_two_reopens_preserve_pixels_identity_and_resources",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/lib.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/straight_line_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("multi-selection-workspace"), {
    name: "multi-selection-workspace",
    cargoArgs: ["--test", "document_workspace", "multi_selection_workspace_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-spine-real"), {
    name: "document-spine-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_pdfium_worker_opens_navigates_and_exits_without_an_orphan",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("rectangle-cutover-real"), {
    name: "rectangle-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_rectangle_property_inspector_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
  });

  assert.deepEqual(resolveRunnerMode("vertex-path-cutover-real"), {
    name: "vertex-path-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_polyline_polygon_edit_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/lib.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/vertex_path_property_inspector.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("vertex-path-workspace"), {
    name: "vertex-path-workspace",
    cargoArgs: [
      "--test",
      "document_workspace",
      "polyline_polygon_workspace_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("measurement-path-cutover-real"), {
    name: "measurement-path-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_polylength_area_edit_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/vertex_path_property_inspector.rs",
      "src/measurement_property_inspector.rs",
      "src/page_scale_control.rs",
      "src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("measurement-path-workspace"), {
    name: "measurement-path-workspace",
    cargoArgs: [
      "--test",
      "document_workspace",
      "polylength_area_workspace_",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "src/vertex_path_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("cloud-cutover-real"), {
    name: "cloud-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_cloud_edit_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("cloud-workspace"), {
    name: "cloud-workspace",
    cargoArgs: [
      "--test",
      "document_workspace",
      "cloud_workspace_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("callout-cutover-real"), {
    name: "callout-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_callout_edit_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("callout-workspace"), {
    name: "callout-workspace",
    cargoArgs: ["--test", "document_workspace", "callout_workspace_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("cloud-plus-workspace"), {
    name: "cloud-plus-workspace",
    cargoArgs: ["--test", "document_workspace", "cloud_plus_workspace_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("dimension-workspace"), {
    name: "dimension-workspace",
    cargoArgs: ["--test", "document_workspace", "dimension_workspace_"],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/lib.rs",
      "src/document_workspace.rs",
      "src/dimension_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("engineering-pointer-workspace"), {
    name: "engineering-pointer-workspace",
    cargoArgs: [
      "--test",
      "document_workspace",
      "workspace_engineering_pointer_",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("arc-workspace"), {
    name: "arc-workspace",
    cargoArgs: ["--test", "document_workspace", "arc_workspace_"],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("cloud-plus-cutover-real"), {
    name: "cloud-plus-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_cloud_plus_edit_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("dimension-cutover-real"), {
    name: "dimension-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_dimension_edit_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "src/dimension_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("arc-cutover-real"), {
    name: "arc-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_arc_edit_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("length-workspace"), {
    name: "length-workspace",
    cargoArgs: [
      "--test",
      "document_workspace",
      "length_uses_two_click_placement_scale_guard_preview_and_shift_constraint",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("text-box-workspace"), {
    name: "text-box-workspace",
    cargoArgs: ["--test", "document_workspace", "text_box_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("gallery-dimension-model"), {
    name: "gallery-dimension-model",
    cargoArgs: ["--no-default-features", "--lib", "dimension_"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-dimension-adapter"), {
    name: "gallery-dimension-adapter",
    cargoArgs: ["--no-default-features", "--test", "annotation_adapter", "dimension_"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-dimension-persistence"), {
    name: "gallery-dimension-persistence",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "dimension_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("gallery-arc-model"), {
    name: "gallery-arc-model",
    cargoArgs: ["--no-default-features", "--test", "arc_model"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-arc-adapter"), {
    name: "gallery-arc-adapter",
    cargoArgs: ["--no-default-features", "--test", "annotation_adapter", "arc_"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/tests/annotation_adapter.rs",
    ],
  });
  assert.deepEqual(resolveRunnerMode("gallery-arc-persistence"), {
    name: "gallery-arc-persistence",
    cargoArgs: ["--no-default-features", "--test", "pdf_persistence", "arc_"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("gallery-redact-model"), {
    name: "gallery-redact-model",
    cargoArgs: ["--no-default-features", "--test", "redact_model"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-redact-adapter"), {
    name: "gallery-redact-adapter",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "redact_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-redact-persistence"), {
    name: "gallery-redact-persistence",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "redact_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-snapshot-model"), {
    name: "gallery-snapshot-model",
    cargoArgs: ["--no-default-features", "--test", "snapshot_model"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-snapshot-adapter"), {
    name: "gallery-snapshot-adapter",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "snapshot_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-snapshot-persistence"), {
    name: "gallery-snapshot-persistence",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "snapshot_create_edit_delete_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("redact-workspace"), {
    name: "redact-workspace",
    cargoArgs: ["--test", "document_workspace", "redact_workspace_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("redact-cutover-real"), {
    name: "redact-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_redact_edit_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/lib.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/page_geometry.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "tests/document_workspace.rs",
      "tests/build-guard.test.mjs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
      "../performance/results/public-fixtures-v1/fixture-index.json",
      "../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf",
    ],
  });
  assert.deepEqual(resolveRunnerMode("snapshot-workspace"), {
    name: "snapshot-workspace",
    cargoArgs: ["--test", "document_workspace", "snapshot_workspace_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("snapshot-cutover-real"), {
    name: "snapshot-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_snapshot_capture_edit_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/lib.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/native_application.rs",
      "src/engineering_visual_property_inspector.rs",
      "src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/page_geometry.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "tests/document_workspace.rs",
      "tests/build-guard.test.mjs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
      "../performance/results/public-fixtures-v1/fixture-index.json",
      "../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf",
    ],
  });
  assert.deepEqual(resolveRunnerMode("gallery-semantic-snapping"), {
    name: "gallery-semantic-snapping",
    cargoArgs: ["--no-default-features", "--test", "semantic_snapping"],
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("gallery-semantic-snapping-adapter"), {
    name: "gallery-semantic-snapping-adapter",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "semantic_snapping_",
    ],
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("semantic-snapping-workspace"), {
    name: "semantic-snapping-workspace",
    cargoArgs: [
      "--test",
      "document_workspace",
      "semantic_snapping_workspace_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("semantic-snapping-cutover-real"), {
    name: "semantic-snapping-cutover-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_semantic_snapping_line_and_length_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/lib.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/semantic_snapping.rs",
      "../gpui-gallery/src/page_geometry.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "tests/document_workspace.rs",
      "tests/build-guard.test.mjs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
      "../performance/results/public-fixtures-v1/fixture-index.json",
      "../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf",
    ],
  });
  assert.deepEqual(resolveRunnerMode("rectangle-inspector"), {
    name: "rectangle-inspector",
    cargoArgs: [
      "--test",
      "document_workspace",
      "rectangle_property_inspector_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("rectangle-inspector-real"), {
    name: "rectangle-inspector-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_rectangle_property_inspector_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("shared-shape-inspector"), {
    name: "shared-shape-inspector",
    cargoArgs: [
      "--test",
      "document_workspace",
      "shared_shape_property_inspector_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("shared-shape-inspector-real"), {
    name: "shared-shape-inspector-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_shared_shape_property_inspector_save_close_and_fresh_workspace_reopen",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-recovery"), {
    name: "document-recovery",
    cargoArgs: [
      "--test",
      "document_workspace",
      "document_worker_recovery_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-recovery-real"), {
    name: "document-recovery-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_worker_crash_recovery_preserves_dirty_document_and_releases_resources",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-file-authority"), {
    name: "document-file-authority",
    cargoArgs: ["--test", "document_workspace", "native_file_authority_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("viewer-navigation"), {
    name: "viewer-navigation",
    cargoArgs: ["--test", "document_workspace", "native_view_navigation_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("viewer-cad-state"), {
    name: "viewer-cad-state",
    cargoArgs: ["--lib", "cad_view_state_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("viewer-cad-workspace"), {
    name: "viewer-cad-workspace",
    cargoArgs: [
      "--test",
      "document_workspace",
      "cad_workspace_routes_real_controls_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("viewer-thumbnails"), {
    name: "viewer-thumbnails",
    cargoArgs: [
      "--test",
      "document_workspace",
      "virtualized_thumbnail_rail_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("viewer-quality"), {
    name: "viewer-quality",
    cargoArgs: ["--lib", "viewer_quality_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("viewer-quality-workspace"), {
    name: "viewer-quality-workspace",
    cargoArgs: [
      "--test",
      "document_workspace",
      "rendered_viewer_quality_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("viewer-adaptive-runtime"), {
    name: "viewer-adaptive-runtime",
    cargoArgs: [
      "--test",
      "document_workspace",
      "viewer_adaptive_runtime_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("viewer-status"), {
    name: "viewer-status",
    cargoArgs: [
      "--test",
      "document_workspace",
      "viewer_status_surfaces_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("viewer-render-recovery"), {
    name: "viewer-render-recovery",
    cargoArgs: [
      "--test",
      "document_workspace",
      "viewer_render_failure_",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("gallery-viewer-cad"), {
    name: "gallery-viewer-cad",
    cargoArgs: ["--no-default-features", "--test", "viewer", "cad_layout_"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("page-scale"), {
    name: "page-scale",
    cargoArgs: ["--test", "document_workspace", "page_scale_dialog_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("ellipse-workspace"), {
    name: "ellipse-workspace",
    cargoArgs: ["--test", "document_workspace", "ellipse_workspace_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("page-rotation"), {
    name: "page-rotation",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "page_rotation_is_one_document_revision_rejects_stale_pixels_and_preserves_failure_state",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("application-close"), {
    name: "application-close",
    cargoArgs: ["--test", "application_close"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("application-close-integration"), {
    name: "application-close-integration",
    cargoArgs: ["--test", "application_close_integration"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("application-close-real"), {
    name: "application-close-real",
    cargoArgs: [
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_mixed_document_application_close_saves_generated_target_and_releases_resources",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("native-shell-rectangle-real"), {
    name: "native-shell-rectangle-real",
    cargoArgs: [
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_rectangle_edit_save_close_and_fresh_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/application_close_workspace.rs",
      "src/bin/component_story.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_viewer.rs",
      "src/document_workspace.rs",
      "src/native_document_view_state.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "tests/application_close_integration.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("native-shell-pen-highlight-real"), {
    name: "native-shell-pen-highlight-real",
    cargoArgs: [
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_pen_highlight_create_undo_redo_save_close_and_fresh_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/application_close_workspace.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/ink_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "tests/application_close_integration.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("native-shell-text-box-real"), {
    name: "native-shell-text-box-real",
    cargoArgs: [
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_text_box_create_type_escape_save_close_and_fresh_reopen",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "src/application_close_workspace.rs",
      "src/document_resource.rs",
      "src/document_session.rs",
      "src/document_workspace.rs",
      "src/text_box_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_worker.rs",
      "tests/application_close_integration.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("native-shell-core-editor-real"), {
    name: "native-shell-core-editor-real",
    cargoArgs: [
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_all_eight_families_edit_history_save_save_as_close_and_two_reopens",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: nativeShellCoreEditorSourceHashes,
  });
  assert.deepEqual(resolveRunnerMode("native-shell-focused-real"), {
    name: "native-shell-focused-real",
    cargoArgs: [
      "--test",
      "application_close_integration",
      "real_native_shell_",
      "--",
      "--ignored",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: nativeShellCoreEditorSourceHashes,
  });
  assert.deepEqual(resolveRunnerMode("perf-protocol"), {
    name: "perf-protocol",
    cargoArgs: ["--test", "perf_protocol"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("perf-capture-signal"), {
    name: "perf-capture-signal",
    cargoArgs: ["--test", "perf_capture_signal"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("perf-story"), {
    name: "perf-story",
    cargoArgs: ["--test", "perf_story"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("native-reader-build"), {
    name: "native-reader-build",
    cargoSubcommand: "build",
    cargoArgs: [
      "--bin",
      "component_story",
      "--bin",
      "butter-paper-pdf-worker",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: nativeRuntimeSourceHashes,
  });
  assert.deepEqual(resolveRunnerMode("perf-story-build"), {
    name: "perf-story-build",
    cargoSubcommand: "build",
    cargoArgs: [
      "--features",
      "benchmark-evidence",
      "--bin",
      "component_story",
      "--bin",
      "butter-paper-pdf-worker",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("perf-story-release"), {
    name: "perf-story-release",
    cargoSubcommand: "build",
    cargoArgs: [
      "--release",
      "--features",
      "benchmark-evidence",
      "--bin",
      "component_story",
      "--bin",
      "butter-paper-pdf-worker",
    ],
    controlledFailureStatus: null,
    targetRelativeToMigration: ".build-targets/gpui-component-performance",
  });
  assert.deepEqual(resolveRunnerMode("gallery-pdf-persistence"), {
    name: "gallery-pdf-persistence",
    cargoArgs: ["--no-default-features", "--test", "pdf_persistence"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: saveAsPersistenceSourceHashes,
  });
  assert.deepEqual(resolveRunnerMode("legacy-length-preserve-until-edit-exact"), {
    name: "legacy-length-preserve-until-edit-exact",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "legacy_length_preserves_external_identity_until_edit_and_rejects_ambiguity",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/document_workspace.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "tests/document_workspace.rs",
      "tests/build-guard.test.mjs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
      "../performance/results/public-fixtures-v1/fixture-index.json",
      "../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf",
    ],
  });
  assert.deepEqual(resolveRunnerMode("legacy-length-hardening-exact"), {
    name: "legacy-length-hardening-exact",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "legacy_length_hardening_preserves_unnamed_and_ambiguous_inputs_and_cleans_owned_graphs",
    ],
    controlledFailureStatus: null,
    sourceHashRelatives: [
      "Cargo.toml",
      "Cargo.lock",
      "src/document_workspace.rs",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "tests/document_workspace.rs",
      "tests/build-guard.test.mjs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
      "../performance/results/public-fixtures-v1/fixture-index.json",
      "../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf",
    ],
  });
  assert.deepEqual(resolveRunnerMode("gallery-measurement-path-text-encoding-exact"), {
    name: "gallery-measurement-path-text-encoding-exact",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "measurement_path_pdf_text_strings_use_pdfdocencoding_or_utf16be",
      "--",
      "--exact",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("gallery-straight-line-ap-exact"), {
    name: "gallery-straight-line-ap-exact",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "edited_created_and_deleted_straight_lines_rebuild_only_the_owned_safe_dictionary",
      "--",
      "--exact",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: saveAsPersistenceSourceHashes,
  });
  assert.deepEqual(resolveRunnerMode("gallery-straight-line-object-graph-exact"), {
    name: "gallery-straight-line-object-graph-exact",
    cargoArgs: [
      "--no-default-features",
      "--lib",
      "pdf_engine::straight_line_object_graph_tests::malformed_annotation_array_rejects_before_allocating_any_pdf_object",
      "--",
      "--exact",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("gallery-straight-line-unchanged-exact"), {
    name: "gallery-straight-line-unchanged-exact",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "unchanged_straight_line_preserves_custom_dictionary_and_appearance_graph_exactly",
      "--",
      "--exact",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("document-image"), {
    name: "document-image",
    cargoArgs: ["--test", "document_workspace", "regular_png_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("native-app-shell"), {
    name: "native-app-shell",
    cargoArgs: ["--test", "native_app_shell"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("native-runtime-layout"), {
    name: "native-runtime-layout",
    cargoArgs: ["--lib", "native_runtime_layout::tests"],
    controlledFailureStatus: null,
    sourceHashRelatives: nativeRuntimeSourceHashes,
  });
  assert.deepEqual(resolveRunnerMode("native-open"), {
    name: "native-open",
    cargoArgs: ["--test", "document_workspace", "native_open_"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("native-application"), {
    name: "native-application",
    cargoArgs: ["--test", "native_application"],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("gallery-highlight-compositor"), {
    name: "gallery-highlight-compositor",
    cargoArgs: ["--no-default-features", "--test", "highlight_compositor"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-annotation-adapter"), {
    name: "gallery-annotation-adapter",
    cargoArgs: ["--no-default-features", "--test", "annotation_adapter"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-text-box-style"), {
    name: "gallery-text-box-style",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "--",
      "--exact",
      "exact_selected_text_box_style_is_atomic_noop_safe_locked_and_undoable",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/tests/annotation_adapter.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("gallery-all-targets"), {
    name: "gallery-all-targets",
    cargoArgs: ["--no-default-features", "--all-targets"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/tests/annotation_adapter.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ],
  });
  assert.deepEqual(resolveRunnerMode("gallery-length-endpoint-preview"), {
    name: "gallery-length-endpoint-preview",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "--",
      "--exact",
      "length_endpoint_drag_previews_without_committing_history",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-generated-document"), {
    name: "gallery-generated-document",
    cargoArgs: ["--no-default-features", "--test", "generated_document"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-template-library"), {
    name: "gallery-template-library",
    cargoArgs: ["--no-default-features", "--test", "template_library"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("document-template-real"), {
    name: "document-template-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_generated_template_creates_opens_saves_reopens_and_releases",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-template-library"), {
    name: "document-template-library",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "imported_template_library_restart_materializes_an_independent_dirty_workspace_document",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("document-template-library-real"), {
    name: "document-template-library-real",
    cargoArgs: [
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_imported_template_library_renders_saves_reopens_and_releases",
    ],
    controlledFailureStatus: null,
  });
  assert.deepEqual(resolveRunnerMode("gallery-selection-geometry"), {
    name: "gallery-selection-geometry",
    cargoArgs: [
      "--no-default-features",
      "--lib",
      "marquee",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-annotation-model-order"), {
    name: "gallery-annotation-model-order",
    cargoArgs: [
      "--no-default-features",
      "--lib",
      "snapshot_preserves_stable_order",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-annotation-model-geometry"), {
    name: "gallery-annotation-model-geometry",
    cargoArgs: [
      "--no-default-features",
      "--lib",
      "rendered_pointer_rectangle_matches_pdf_edge_reconstruction_only_within_pdf_tolerance",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-measurement-path-model"), {
    name: "gallery-measurement-path-model",
    cargoArgs: [
      "--no-default-features",
      "--lib",
      "measurement_path_contract",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-cloud-model"), {
    name: "gallery-cloud-model",
    cargoArgs: [
      "--no-default-features",
      "--lib",
      "cloud_annotation_contract",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-callout-model"), {
    name: "gallery-callout-model",
    cargoArgs: [
      "--no-default-features",
      "--lib",
      "callout_annotation_contract",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-callout-adapter"), {
    name: "gallery-callout-adapter",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "callout_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-callout-persistence"), {
    name: "gallery-callout-persistence",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "callout_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-cloud-plus-model"), {
    name: "gallery-cloud-plus-model",
    cargoArgs: ["--no-default-features", "--lib", "cloud_plus_"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-cloud-plus-adapter"), {
    name: "gallery-cloud-plus-adapter",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "cloud_plus_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-cloud-plus-persistence"), {
    name: "gallery-cloud-plus-persistence",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "cloud_plus_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-rectangle-properties"), {
    name: "gallery-rectangle-properties",
    cargoArgs: [
      "--no-default-features",
      "--lib",
      "rectangle_property_edits_commit_typed_geometry_rotation_and_lock_history",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-ellipse-properties"), {
    name: "gallery-ellipse-properties",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "ellipse_property_",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-page-coordinate-space"), {
    name: "gallery-page-coordinate-space",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "page_coordinate_space",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-page-geometry-protocol"), {
    name: "gallery-page-geometry-protocol",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "page_geometry_protocol",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-pdf-worker-build"), {
    name: "gallery-pdf-worker-build",
    cargoArgs: [
      "--no-default-features",
      "--features",
      "pdfium-worker",
      "--bin",
      "butter-paper-pdf-worker",
    ],
    cargoSubcommand: "build",
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("pdf-worker-contract"), {
    name: "pdf-worker-contract",
    cargoArgs: ["--no-default-features", "--test", "pdf_worker"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("windows-worker-check"), {
    name: "windows-worker-check",
    cargoArgs: [
      "--target",
      "x86_64-pc-windows-msvc",
      "--no-default-features",
      "--features",
      "pdfium-worker",
      "--lib",
      "--bin",
      "butter-paper-pdf-worker",
    ],
    cargoSubcommand: "check",
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "Cargo.lock",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/pdf_worker.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
    ],
  });
  assert.deepEqual(resolveRunnerMode("windows-save-as-authority-check"), {
    name: "windows-save-as-authority-check",
    cargoArgs: [
      "--target",
      "x86_64-pc-windows-msvc",
      "--no-default-features",
      "--lib",
    ],
    cargoSubcommand: "check",
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
    ],
  });
  assert.deepEqual(resolveRunnerMode("windows-save-as-authority-tests-check"), {
    name: "windows-save-as-authority-tests-check",
    cargoArgs: [
      "--target",
      "x86_64-pc-windows-msvc",
      "--no-default-features",
      "--test",
      "pdf_persistence",
    ],
    cargoSubcommand: "check",
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: [
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
    ],
  });
  assert.deepEqual(resolveRunnerMode("component-pdf-worker-build"), {
    name: "component-pdf-worker-build",
    cargoArgs: [
      "--no-default-features",
      "--bin",
      "butter-paper-pdf-worker",
    ],
    cargoSubcommand: "build",
    controlledFailureStatus: null,
    targetRelativeToMigration: ".build-targets/gpui-component-compat",
  });
  assert.deepEqual(resolveRunnerMode("gallery-page-scale-model"), {
    name: "gallery-page-scale-model",
    cargoArgs: [
      "--no-default-features",
      "--lib",
      "page_scale_contract",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-image-decode"), {
    name: "gallery-image-decode",
    cargoArgs: ["--no-default-features", "--test", "image_asset_decode"],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("gallery-editor-comparison"), {
    name: "gallery-editor-comparison",
    cargoArgs: [
      "--no-default-features",
      "--test",
      "editor_comparison_scenario",
    ],
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  });
  assert.deepEqual(resolveRunnerMode("retention-proof"), {
    name: "retention-proof",
    cargoArgs: ["--test", "document_tab_bar", "pointer_drag"],
    controlledFailureStatus: 101,
  });
  assert.throws(
    () => resolveRunnerMode("pointer-drag; rm -rf target"),
    /unknown probe runner mode/,
  );
});

test("target sampling tolerates a Cargo directory disappearing during traversal", async () => {
  const transientPath = "/disposable-target/debug/deps/rmeta-transient";
  const fsOps = {
    async lstat(path) {
      assert.equal(path, transientPath);
      return {
        blocks: 0,
        isDirectory: () => true,
        isSymbolicLink: () => false,
      };
    },
    async readdir(path) {
      assert.equal(path, transientPath);
      throw Object.assign(
        new Error("Cargo removed its temporary metadata directory"),
        {
          code: "ENOENT",
        },
      );
    },
  };

  assert.equal(await allocatedKiB(transientPath, fsOps), 0);
});

test("only the exact disposable component and backend targets are allowlisted", () => {
  const component =
    "/workspace/experiments/gpui-migration/.build-targets/gpui-component-compat";
  const backend =
    "/workspace/experiments/gpui-migration/.build-targets/gpui-gallery-backend";
  const performance =
    "/workspace/experiments/gpui-migration/.build-targets/gpui-component-performance";
  const allowed = [component, backend, performance];
  assert.equal(assertAllowlistedTarget(component, allowed), component);
  assert.equal(assertAllowlistedTarget(backend, allowed), backend);
  assert.equal(assertAllowlistedTarget(performance, allowed), performance);
  assert.throws(
    () => assertAllowlistedTarget(dirname(component), allowed),
    /not an allowlisted disposable target/,
  );
  assert.throws(
    () => assertAllowlistedTarget(`${component}-other`, allowed),
    /not an allowlisted disposable target/,
  );
  assert.throws(
    () =>
      assertAllowlistedTarget(
        "/workspace/experiments/gpui-migration/gpui-gallery/target",
        allowed,
      ),
    /not an allowlisted disposable target/,
  );
});

test("explicit cleanup removes only the allowlisted disposable target", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-build-guard-"));
  const allowed = join(root, ".build-targets", "gpui-component-compat");
  const preserved = join(root, "preserved", "evidence.log");
  await initializeTarget(allowed, allowed);
  await mkdir(join(allowed, "debug", "deps"), { recursive: true });
  await writeFile(join(allowed, "debug", "deps", "artifact"), "generated\n");
  await mkdir(dirname(preserved), { recursive: true });
  await writeFile(preserved, "keep\n");

  await cleanupTarget(allowed, allowed);
  await assert.rejects(access(allowed));
  await assert.doesNotReject(access(preserved));
  await assert.rejects(
    cleanupTarget(dirname(allowed), allowed),
    /not an allowlisted disposable target/,
  );
  await assert.doesNotReject(access(preserved));
});

test("owned target cleanup configures retries for transient ENOTEMPTY races", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-build-guard-retry-"));
  const allowed = join(root, ".build-targets", "gpui-component-compat");
  await initializeTarget(allowed, allowed);
  const calls = [];

  await cleanupTarget(allowed, allowed, {
    async rm(path, options) {
      calls.push({ path, options });
      await import("node:fs/promises").then((fs) => fs.rm(path, options));
    },
  });

  assert.equal(calls.length, 1);
  assert.equal(calls[0].path, allowed);
  assert.equal(calls[0].options.recursive, true);
  assert.equal(calls[0].options.force, true);
  assert.ok(calls[0].options.maxRetries >= 5);
  assert.ok(calls[0].options.retryDelay >= 100);
  await assert.rejects(access(allowed));
});

test("cleanup refuses an unowned directory or a symlink target", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-build-guard-ownership-"));
  const allowed = join(root, ".build-targets", "gpui-component-compat");
  await mkdir(allowed, { recursive: true });
  await assert.rejects(cleanupTarget(allowed, allowed), /ownership sentinel/);
  await writeFile(join(allowed, "preserved"), "keep\n");
  await assert.doesNotReject(access(join(allowed, "preserved")));

  const symlinkRoot = await mkdtemp(join(tmpdir(), "bp-build-guard-symlink-"));
  const symlinkTarget = join(symlinkRoot, "gpui-component-compat");
  await symlink(allowed, symlinkTarget);
  await assert.rejects(
    cleanupTarget(symlinkTarget, symlinkTarget),
    /symbolic link/,
  );
  await assert.doesNotReject(access(join(allowed, "preserved")));
});
