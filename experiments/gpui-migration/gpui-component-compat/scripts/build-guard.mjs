#!/usr/bin/env node

import {
  lstat,
  mkdir,
  readFile,
  readdir,
  rm,
  statfs,
  writeFile,
} from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const probeDirectory = resolve(scriptDirectory, "..");
const migrationDirectory = resolve(probeDirectory, "..");
export const guardPolicyPath = join(probeDirectory, "build-guard-policy.json");
export const ownershipSentinel = ".butter-paper-disposable-target.json";

const diskSafetyCleanupReasons = new Set([
  "preflight-free-space",
  "runtime-free-space",
  "target-size",
]);

const nativeRuntimeSourceHashes = Object.freeze([
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
]);

const saveAsPersistenceSourceHashes = Object.freeze([
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
]);

const nativeShellCoreEditorSourceHashes = Object.freeze([
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
]);

const runnerModes = Object.freeze({
  "all-targets": Object.freeze({
    name: "all-targets",
    cargoArgs: Object.freeze(["--all-targets"]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      ...nativeRuntimeSourceHashes,
      "src/application_close_workspace.rs",
      "src/document_workspace.rs",
      "tests/application_close_integration.rs",
    ]),
  }),
  "ink-properties": Object.freeze({
    name: "ink-properties",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "ink_property_inspector_",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "src/ink_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "engineering-visual-properties": Object.freeze({
    name: "engineering-visual-properties",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "engineering_visual_inspector_",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "src/engineering_visual_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "engineering-visual-properties-real": Object.freeze({
    name: "engineering-visual-properties-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_engineering_visual_properties_create_edit_save_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "text-box-properties": Object.freeze({
    name: "text-box-properties",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "text_box_property_inspector_",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "src/text_box_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "measurement-properties": Object.freeze({
    name: "measurement-properties",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "measurement_property_",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "src/measurement_property_inspector.rs",
      "src/page_scale_control.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "document-tab-bar": Object.freeze({
    name: "document-tab-bar",
    cargoArgs: Object.freeze(["--test", "document_tab_bar"]),
    controlledFailureStatus: null,
  }),
  "viewer-toolbar-strip": Object.freeze({
    name: "viewer-toolbar-strip",
    cargoArgs: Object.freeze(["--test", "viewer_toolbar_strip"]),
    controlledFailureStatus: null,
  }),
  "native-app-shell": Object.freeze({
    name: "native-app-shell",
    cargoArgs: Object.freeze(["--test", "native_app_shell"]),
    controlledFailureStatus: null,
  }),
  "native-runtime-layout": Object.freeze({
    name: "native-runtime-layout",
    cargoArgs: Object.freeze([
      "--lib",
      "native_runtime_layout::tests",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: nativeRuntimeSourceHashes,
  }),
  "native-open": Object.freeze({
    name: "native-open",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "native_open_",
    ]),
    controlledFailureStatus: null,
  }),
  "native-application": Object.freeze({
    name: "native-application",
    cargoArgs: Object.freeze(["--test", "native_application"]),
    controlledFailureStatus: null,
  }),
  "pointer-drag": Object.freeze({
    name: "pointer-drag",
    cargoArgs: Object.freeze(["--test", "document_tab_bar", "pointer_drag"]),
    controlledFailureStatus: null,
  }),
  "document-spine": Object.freeze({
    name: "document-spine",
    cargoArgs: Object.freeze(["--test", "document_workspace"]),
    controlledFailureStatus: null,
    sourceHashRelatives: saveAsPersistenceSourceHashes,
  }),
  "compat-signature": Object.freeze({
    name: "compat-signature",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "local_signature_",
    ]),
    controlledFailureStatus: null,
  }),
  "signature-real": Object.freeze({
    name: "signature-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_signature_image_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
  }),
  "document-image-real": Object.freeze({
    name: "document-image-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_regular_png_image_create_move_resize_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "document-coordinate-space": Object.freeze({
    name: "document-coordinate-space",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "opened_document_retains_coordinate_space",
    ]),
    controlledFailureStatus: null,
  }),
  "document-coordinate-space-real": Object.freeze({
    name: "document-coordinate-space-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_user_unit_coordinate_space_renders_edits_saves_reopens_and_releases",
    ]),
    controlledFailureStatus: null,
  }),
  "session-tab-strip": Object.freeze({
    name: "session-tab-strip",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "real_session_tab_strip_",
    ]),
    controlledFailureStatus: null,
  }),
  "document-save": Object.freeze({
    name: "document-save",
    cargoArgs: Object.freeze(["--test", "document_workspace", "in_place_save_"]),
    controlledFailureStatus: null,
  }),
  "document-save-real": Object.freeze({
    name: "document-save-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_in_place_save_replaces_the_opened_pdf_and_reopens_cleanly",
    ]),
    controlledFailureStatus: null,
  }),
  "document-save-collision-real": Object.freeze({
    name: "document-save-collision-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_save_as_collision_recovers_to_fresh_target_and_reopens",
    ]),
    controlledFailureStatus: null,
  }),
  "two-document-save-failure-real": Object.freeze({
    name: "two-document-save-failure-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_two_document_dirty_save_as_failure_is_isolated_and_recovers",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "viewer-state-real": Object.freeze({
    name: "viewer-state-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_preserves_independent_view_state_through_fit_scroll_thumbnail_zoom_and_document_switch",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "document-template": Object.freeze({
    name: "document-template",
    cargoArgs: Object.freeze(["--test", "document_workspace", "generated_template_"]),
    controlledFailureStatus: null,
  }),
  "template-manager": Object.freeze({
    name: "template-manager",
    cargoArgs: Object.freeze(["--test", "template_manager"]),
    controlledFailureStatus: null,
  }),
  "line-arrow-workspace": Object.freeze({
    name: "line-arrow-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "line_arrow_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "line-arrow-pointer-workspace-exact": Object.freeze({
    name: "line-arrow-pointer-workspace-exact",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "line_arrow_workspace_pointer_create_body_move_and_endpoint_edit_share_one_history_contract",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "polyline-polygon-pointer-workspace-exact": Object.freeze({
    name: "polyline-polygon-pointer-workspace-exact",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "polyline_polygon_workspace_pointer_create_finish_cancel_move_vertex_lock_history",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "straight-line-properties-exact": Object.freeze({
    name: "straight-line-properties-exact",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "straight_line_inspector_renders_exact_line_arrow_controls_and_revalidates_each_event",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/lib.rs",
      "src/document_workspace.rs",
      "src/straight_line_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "vertex-path-properties-exact": Object.freeze({
    name: "vertex-path-properties-exact",
    cargoArgs: Object.freeze([
      "--test", "document_workspace", "--", "--exact",
      "vertex_path_inspector_exact_controls_revalidate_and_preserve_hidden_state",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/lib.rs", "src/document_workspace.rs", "src/vertex_path_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs", "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs", "scripts/build-guard.mjs", "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "vertex-path-adapter-properties-exact": Object.freeze({
    name: "vertex-path-adapter-properties-exact",
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    cargoArgs: Object.freeze([
      "--test", "annotation_adapter", "--", "--exact",
      "selected_vertex_path_properties_preserve_hidden_appearance_and_are_exact",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/src/annotation_adapter.rs", "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/tests/annotation_adapter.rs", "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "line-arrow-save-reopen-real-exact": Object.freeze({
    name: "line-arrow-save-reopen-real-exact",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_line_arrow_save_as_two_reopens_preserve_pixels_identity_and_resources",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "multi-selection-workspace": Object.freeze({
    name: "multi-selection-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "multi_selection_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "document-spine-real": Object.freeze({
    name: "document-spine-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_pdfium_worker_opens_navigates_and_exits_without_an_orphan",
    ]),
    controlledFailureStatus: null,
  }),
  "rectangle-cutover-real": Object.freeze({
    name: "rectangle-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_rectangle_property_inspector_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
  }),
  "vertex-path-cutover-real": Object.freeze({
    name: "vertex-path-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_polyline_polygon_edit_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "vertex-path-workspace": Object.freeze({
    name: "vertex-path-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "polyline_polygon_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "measurement-path-cutover-real": Object.freeze({
    name: "measurement-path-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_polylength_area_edit_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "measurement-path-workspace": Object.freeze({
    name: "measurement-path-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "polylength_area_workspace_",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "src/vertex_path_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "cloud-cutover-real": Object.freeze({
    name: "cloud-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_cloud_edit_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
  }),
  "cloud-workspace": Object.freeze({
    name: "cloud-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "cloud_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "callout-cutover-real": Object.freeze({
    name: "callout-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_callout_edit_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "callout-workspace": Object.freeze({
    name: "callout-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "callout_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "cloud-plus-cutover-real": Object.freeze({
    name: "cloud-plus-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_cloud_plus_edit_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "dimension-cutover-real": Object.freeze({
    name: "dimension-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_dimension_edit_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "src/dimension_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "arc-cutover-real": Object.freeze({
    name: "arc-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_arc_edit_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "cloud-plus-workspace": Object.freeze({
    name: "cloud-plus-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "cloud_plus_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "dimension-workspace": Object.freeze({
    name: "dimension-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "dimension_workspace_",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/lib.rs",
      "src/document_workspace.rs",
      "src/dimension_property_inspector.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "engineering-pointer-workspace": Object.freeze({
    name: "engineering-pointer-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "workspace_engineering_pointer_",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
      "src/document_workspace.rs",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "tests/document_workspace.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "arc-workspace": Object.freeze({
    name: "arc-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "arc_workspace_",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "snapshot-workspace": Object.freeze({
    name: "snapshot-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "snapshot_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "snapshot-cutover-real": Object.freeze({
    name: "snapshot-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_snapshot_capture_edit_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "length-workspace": Object.freeze({
    name: "length-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "length_uses_two_click_placement_scale_guard_preview_and_shift_constraint",
    ]),
    controlledFailureStatus: null,
  }),
  "text-box-workspace": Object.freeze({
    name: "text-box-workspace",
    cargoArgs: Object.freeze(["--test", "document_workspace", "text_box_"]),
    controlledFailureStatus: null,
  }),
  "rectangle-inspector": Object.freeze({
    name: "rectangle-inspector",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "rectangle_property_inspector_",
    ]),
    controlledFailureStatus: null,
  }),
  "rectangle-inspector-real": Object.freeze({
    name: "rectangle-inspector-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_rectangle_property_inspector_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
  }),
  "shared-shape-inspector": Object.freeze({
    name: "shared-shape-inspector",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "shared_shape_property_inspector_",
    ]),
    controlledFailureStatus: null,
  }),
  "shared-shape-inspector-real": Object.freeze({
    name: "shared-shape-inspector-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_shared_shape_property_inspector_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
  }),
  "document-recovery": Object.freeze({
    name: "document-recovery",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "document_worker_recovery_",
    ]),
    controlledFailureStatus: null,
  }),
  "document-recovery-real": Object.freeze({
    name: "document-recovery-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_worker_crash_recovery_preserves_dirty_document_and_releases_resources",
    ]),
    controlledFailureStatus: null,
  }),
  "document-template-real": Object.freeze({
    name: "document-template-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_generated_template_creates_opens_saves_reopens_and_releases",
    ]),
    controlledFailureStatus: null,
  }),
  "document-template-library": Object.freeze({
    name: "document-template-library",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "imported_template_library_restart_materializes_an_independent_dirty_workspace_document",
    ]),
    controlledFailureStatus: null,
  }),
  "document-template-library-real": Object.freeze({
    name: "document-template-library-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_imported_template_library_renders_saves_reopens_and_releases",
    ]),
    controlledFailureStatus: null,
  }),
  "document-image": Object.freeze({
    name: "document-image",
    cargoArgs: Object.freeze(["--test", "document_workspace", "regular_png_"]),
    controlledFailureStatus: null,
  }),
  "document-file-authority": Object.freeze({
    name: "document-file-authority",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "native_file_authority_",
    ]),
    controlledFailureStatus: null,
  }),
  "viewer-navigation": Object.freeze({
    name: "viewer-navigation",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "native_view_navigation_",
    ]),
    controlledFailureStatus: null,
  }),
  "viewer-cad-state": Object.freeze({
    name: "viewer-cad-state",
    cargoArgs: Object.freeze(["--lib", "cad_view_state_"]),
    controlledFailureStatus: null,
  }),
  "viewer-cad-workspace": Object.freeze({
    name: "viewer-cad-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "cad_workspace_routes_real_controls_",
    ]),
    controlledFailureStatus: null,
  }),
  "viewer-thumbnails": Object.freeze({
    name: "viewer-thumbnails",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "virtualized_thumbnail_rail_",
    ]),
    controlledFailureStatus: null,
  }),
  "viewer-quality": Object.freeze({
    name: "viewer-quality",
    cargoArgs: Object.freeze(["--lib", "viewer_quality_"]),
    controlledFailureStatus: null,
  }),
  "viewer-quality-workspace": Object.freeze({
    name: "viewer-quality-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "rendered_viewer_quality_",
    ]),
    controlledFailureStatus: null,
  }),
  "viewer-adaptive-runtime": Object.freeze({
    name: "viewer-adaptive-runtime",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "viewer_adaptive_runtime_",
    ]),
    controlledFailureStatus: null,
  }),
  "viewer-status": Object.freeze({
    name: "viewer-status",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "viewer_status_surfaces_",
    ]),
    controlledFailureStatus: null,
  }),
  "viewer-render-recovery": Object.freeze({
    name: "viewer-render-recovery",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "viewer_render_failure_",
    ]),
    controlledFailureStatus: null,
  }),
  "gallery-viewer-cad": Object.freeze({
    name: "gallery-viewer-cad",
    cargoArgs: Object.freeze(["--no-default-features", "--test", "viewer", "cad_layout_"]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "page-scale": Object.freeze({
    name: "page-scale",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "page_scale_dialog_",
    ]),
    controlledFailureStatus: null,
  }),
  "ellipse-workspace": Object.freeze({
    name: "ellipse-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "ellipse_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "page-rotation": Object.freeze({
    name: "page-rotation",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "page_rotation_is_one_document_revision_rejects_stale_pixels_and_preserves_failure_state",
    ]),
    controlledFailureStatus: null,
  }),
  "application-close": Object.freeze({
    name: "application-close",
    cargoArgs: Object.freeze(["--test", "application_close"]),
    controlledFailureStatus: null,
  }),
  "application-close-integration": Object.freeze({
    name: "application-close-integration",
    cargoArgs: Object.freeze(["--test", "application_close_integration"]),
    controlledFailureStatus: null,
  }),
  "application-close-real": Object.freeze({
    name: "application-close-real",
    cargoArgs: Object.freeze([
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_mixed_document_application_close_saves_generated_target_and_releases_resources",
    ]),
    controlledFailureStatus: null,
  }),
  "native-shell-rectangle-real": Object.freeze({
    name: "native-shell-rectangle-real",
    cargoArgs: Object.freeze([
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_rectangle_edit_save_close_and_fresh_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "native-shell-pen-highlight-real": Object.freeze({
    name: "native-shell-pen-highlight-real",
    cargoArgs: Object.freeze([
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_pen_highlight_create_undo_redo_save_close_and_fresh_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "native-shell-text-box-real": Object.freeze({
    name: "native-shell-text-box-real",
    cargoArgs: Object.freeze([
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_text_box_create_type_escape_save_close_and_fresh_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "native-shell-core-editor-real": Object.freeze({
    name: "native-shell-core-editor-real",
    cargoArgs: Object.freeze([
      "--test",
      "application_close_integration",
      "--",
      "--ignored",
      "--exact",
      "real_native_shell_all_eight_families_edit_history_save_save_as_close_and_two_reopens",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: nativeShellCoreEditorSourceHashes,
  }),
  "native-shell-focused-real": Object.freeze({
    name: "native-shell-focused-real",
    cargoArgs: Object.freeze([
      "--test",
      "application_close_integration",
      "real_native_shell_",
      "--",
      "--ignored",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: nativeShellCoreEditorSourceHashes,
  }),
  "perf-protocol": Object.freeze({
    name: "perf-protocol",
    cargoArgs: Object.freeze(["--test", "perf_protocol"]),
    controlledFailureStatus: null,
  }),
  "perf-capture-signal": Object.freeze({
    name: "perf-capture-signal",
    cargoArgs: Object.freeze(["--test", "perf_capture_signal"]),
    controlledFailureStatus: null,
  }),
  "perf-story": Object.freeze({
    name: "perf-story",
    cargoArgs: Object.freeze(["--test", "perf_story"]),
    controlledFailureStatus: null,
  }),
  "native-reader-build": Object.freeze({
    name: "native-reader-build",
    cargoSubcommand: "build",
    cargoArgs: Object.freeze([
      "--bin",
      "component_story",
      "--bin",
      "butter-paper-pdf-worker",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: nativeRuntimeSourceHashes,
  }),
  "perf-story-build": Object.freeze({
    name: "perf-story-build",
    cargoSubcommand: "build",
    cargoArgs: Object.freeze([
      "--features",
      "benchmark-evidence",
      "--bin",
      "component_story",
      "--bin",
      "butter-paper-pdf-worker",
    ]),
    controlledFailureStatus: null,
  }),
  "perf-story-release": Object.freeze({
    name: "perf-story-release",
    cargoSubcommand: "build",
    cargoArgs: Object.freeze([
      "--release",
      "--features",
      "benchmark-evidence",
      "--bin",
      "component_story",
      "--bin",
      "butter-paper-pdf-worker",
    ]),
    controlledFailureStatus: null,
    targetRelativeToMigration: ".build-targets/gpui-component-performance",
  }),
  "gallery-pdf-persistence": Object.freeze({
    name: "gallery-pdf-persistence",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: saveAsPersistenceSourceHashes,
  }),
  "legacy-length-preserve-until-edit-exact": Object.freeze({
    name: "legacy-length-preserve-until-edit-exact",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "legacy_length_preserves_external_identity_until_edit_and_rejects_ambiguity",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "legacy-length-hardening-exact": Object.freeze({
    name: "legacy-length-hardening-exact",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--exact",
      "legacy_length_hardening_preserves_unnamed_and_ambiguous_inputs_and_cleans_owned_graphs",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "gallery-measurement-path-text-encoding-exact": Object.freeze({
    name: "gallery-measurement-path-text-encoding-exact",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "measurement_path_pdf_text_strings_use_pdfdocencoding_or_utf16be",
      "--",
      "--exact",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "gallery-straight-line-ap-exact": Object.freeze({
    name: "gallery-straight-line-ap-exact",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "edited_created_and_deleted_straight_lines_rebuild_only_the_owned_safe_dictionary",
      "--",
      "--exact",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: saveAsPersistenceSourceHashes,
  }),
  "gallery-straight-line-object-graph-exact": Object.freeze({
    name: "gallery-straight-line-object-graph-exact",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "pdf_engine::straight_line_object_graph_tests::malformed_annotation_array_rejects_before_allocating_any_pdf_object",
      "--",
      "--exact",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "gallery-straight-line-unchanged-exact": Object.freeze({
    name: "gallery-straight-line-unchanged-exact",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "unchanged_straight_line_preserves_custom_dictionary_and_appearance_graph_exactly",
      "--",
      "--exact",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "gallery-highlight-compositor": Object.freeze({
    name: "gallery-highlight-compositor",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "highlight_compositor",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-annotation-adapter": Object.freeze({
    name: "gallery-annotation-adapter",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-text-box-style": Object.freeze({
    name: "gallery-text-box-style",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "--",
      "--exact",
      "exact_selected_text_box_style_is_atomic_noop_safe_locked_and_undoable",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/tests/annotation_adapter.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "gallery-all-targets": Object.freeze({
    name: "gallery-all-targets",
    cargoArgs: Object.freeze(["--no-default-features", "--all-targets"]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/tests/annotation_adapter.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "gallery-length-endpoint-preview": Object.freeze({
    name: "gallery-length-endpoint-preview",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "--",
      "--exact",
      "length_endpoint_drag_previews_without_committing_history",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-generated-document": Object.freeze({
    name: "gallery-generated-document",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "generated_document",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-template-library": Object.freeze({
    name: "gallery-template-library",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "template_library",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-selection-geometry": Object.freeze({
    name: "gallery-selection-geometry",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "marquee",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-annotation-model-order": Object.freeze({
    name: "gallery-annotation-model-order",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "snapshot_preserves_stable_order",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-annotation-model-geometry": Object.freeze({
    name: "gallery-annotation-model-geometry",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "rendered_pointer_rectangle_matches_pdf_edge_reconstruction_only_within_pdf_tolerance",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-measurement-path-model": Object.freeze({
    name: "gallery-measurement-path-model",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "measurement_path_contract",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-cloud-model": Object.freeze({
    name: "gallery-cloud-model",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "cloud_annotation_contract",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-callout-model": Object.freeze({
    name: "gallery-callout-model",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "callout_annotation_contract",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-callout-adapter": Object.freeze({
    name: "gallery-callout-adapter",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "callout_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-callout-persistence": Object.freeze({
    name: "gallery-callout-persistence",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "callout_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-cloud-plus-model": Object.freeze({
    name: "gallery-cloud-plus-model",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "cloud_plus_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-dimension-model": Object.freeze({
    name: "gallery-dimension-model",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "dimension_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-dimension-adapter": Object.freeze({
    name: "gallery-dimension-adapter",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "dimension_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-dimension-persistence": Object.freeze({
    name: "gallery-dimension-persistence",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "dimension_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "gallery-arc-model": Object.freeze({
    name: "gallery-arc-model",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "arc_model",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-arc-adapter": Object.freeze({
    name: "gallery-arc-adapter",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "arc_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/src/annotation_adapter.rs",
      "../gpui-gallery/tests/annotation_adapter.rs",
    ]),
  }),
  "gallery-arc-persistence": Object.freeze({
    name: "gallery-arc-persistence",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "arc_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/src/annotation_model.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
      "scripts/build-guard.mjs",
      "scripts/run-bounded-button-probe.sh",
    ]),
  }),
  "gallery-redact-model": Object.freeze({
    name: "gallery-redact-model",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "redact_model",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-redact-adapter": Object.freeze({
    name: "gallery-redact-adapter",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "redact_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-redact-persistence": Object.freeze({
    name: "gallery-redact-persistence",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "redact_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-snapshot-model": Object.freeze({
    name: "gallery-snapshot-model",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "snapshot_model",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-snapshot-adapter": Object.freeze({
    name: "gallery-snapshot-adapter",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "snapshot_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-snapshot-persistence": Object.freeze({
    name: "gallery-snapshot-persistence",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "snapshot_create_edit_delete_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-semantic-snapping": Object.freeze({
    name: "gallery-semantic-snapping",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "semantic_snapping",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-semantic-snapping-adapter": Object.freeze({
    name: "gallery-semantic-snapping-adapter",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "semantic_snapping_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "semantic-snapping-workspace": Object.freeze({
    name: "semantic-snapping-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "semantic_snapping_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "semantic-snapping-cutover-real": Object.freeze({
    name: "semantic-snapping-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_semantic_snapping_line_and_length_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "redact-workspace": Object.freeze({
    name: "redact-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "redact_workspace_",
    ]),
    controlledFailureStatus: null,
  }),
  "redact-cutover-real": Object.freeze({
    name: "redact-cutover-real",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "--",
      "--ignored",
      "--exact",
      "real_redact_edit_save_close_and_fresh_workspace_reopen",
    ]),
    controlledFailureStatus: null,
    sourceHashRelatives: Object.freeze([
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
    ]),
  }),
  "gallery-cloud-plus-adapter": Object.freeze({
    name: "gallery-cloud-plus-adapter",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "annotation_adapter",
      "cloud_plus_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-cloud-plus-persistence": Object.freeze({
    name: "gallery-cloud-plus-persistence",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "cloud_plus_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-rectangle-properties": Object.freeze({
    name: "gallery-rectangle-properties",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "rectangle_property_edits_commit_typed_geometry_rotation_and_lock_history",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-ellipse-properties": Object.freeze({
    name: "gallery-ellipse-properties",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_persistence",
      "ellipse_property_",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-page-coordinate-space": Object.freeze({
    name: "gallery-page-coordinate-space",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "page_coordinate_space",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-page-geometry-protocol": Object.freeze({
    name: "gallery-page-geometry-protocol",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "page_geometry_protocol",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-pdf-worker-build": Object.freeze({
    name: "gallery-pdf-worker-build",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--features",
      "pdfium-worker",
      "--bin",
      "butter-paper-pdf-worker",
    ]),
    cargoSubcommand: "build",
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "pdf-worker-contract": Object.freeze({
    name: "pdf-worker-contract",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "pdf_worker",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "windows-worker-check": Object.freeze({
    name: "windows-worker-check",
    cargoArgs: Object.freeze([
      "--target",
      "x86_64-pc-windows-msvc",
      "--no-default-features",
      "--features",
      "pdfium-worker",
      "--lib",
      "--bin",
      "butter-paper-pdf-worker",
    ]),
    cargoSubcommand: "check",
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "Cargo.lock",
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/pdf_worker.rs",
      "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
    ]),
  }),
  "windows-save-as-authority-check": Object.freeze({
    name: "windows-save-as-authority-check",
    cargoArgs: Object.freeze([
      "--target",
      "x86_64-pc-windows-msvc",
      "--no-default-features",
      "--lib",
    ]),
    cargoSubcommand: "check",
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
    ]),
  }),
  "windows-save-as-authority-tests-check": Object.freeze({
    name: "windows-save-as-authority-tests-check",
    cargoArgs: Object.freeze([
      "--target",
      "x86_64-pc-windows-msvc",
      "--no-default-features",
      "--test",
      "pdf_persistence",
    ]),
    cargoSubcommand: "check",
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
    sourceHashRelatives: Object.freeze([
      "../gpui-gallery/Cargo.toml",
      "../gpui-gallery/Cargo.lock",
      "../gpui-gallery/src/pdf_file_authority.rs",
      "../gpui-gallery/src/pdf_engine.rs",
      "../gpui-gallery/tests/pdf_persistence.rs",
    ]),
  }),
  "component-pdf-worker-build": Object.freeze({
    name: "component-pdf-worker-build",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--bin",
      "butter-paper-pdf-worker",
    ]),
    cargoSubcommand: "build",
    controlledFailureStatus: null,
    targetRelativeToMigration: ".build-targets/gpui-component-compat",
  }),
  "gallery-page-scale-model": Object.freeze({
    name: "gallery-page-scale-model",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--lib",
      "page_scale_contract",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-image-decode": Object.freeze({
    name: "gallery-image-decode",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "image_asset_decode",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "gallery-editor-comparison": Object.freeze({
    name: "gallery-editor-comparison",
    cargoArgs: Object.freeze([
      "--no-default-features",
      "--test",
      "editor_comparison_scenario",
    ]),
    controlledFailureStatus: null,
    manifestRelativeToProbe: "../gpui-gallery/Cargo.toml",
    targetRelativeToMigration: ".build-targets/gpui-gallery-backend",
  }),
  "retention-proof": Object.freeze({
    name: "retention-proof",
    cargoArgs: Object.freeze(["--test", "document_tab_bar", "pointer_drag"]),
    controlledFailureStatus: 101,
  }),
});

export function cleanupDisposition({
  status,
  limitReason = null,
  explicitCleanup = false,
}) {
  if (explicitCleanup) {
    return { action: "clean", reason: "explicit-cleanup" };
  }
  if (diskSafetyCleanupReasons.has(limitReason)) {
    return { action: "clean", reason: limitReason };
  }
  if (limitReason) {
    return { action: "retain", reason: limitReason };
  }
  if (status === 0) {
    return { action: "retain", reason: "successful-run" };
  }
  return { action: "retain", reason: `cargo-status-${status}` };
}

export function resolveRunnerMode(name) {
  const mode = runnerModes[name];
  if (!mode) throw new Error(`unknown probe runner mode ${name}`);
  const resolved = {
    name: mode.name,
    cargoArgs: [...mode.cargoArgs],
    controlledFailureStatus: mode.controlledFailureStatus,
  };
  if (mode.cargoSubcommand) {
    resolved.cargoSubcommand = mode.cargoSubcommand;
  }
  if (mode.manifestRelativeToProbe) {
    resolved.manifestRelativeToProbe = mode.manifestRelativeToProbe;
  }
  if (mode.targetRelativeToMigration) {
    resolved.targetRelativeToMigration = mode.targetRelativeToMigration;
  }
  if (mode.sourceHashRelatives) {
    resolved.sourceHashRelatives = [...mode.sourceHashRelatives];
  }
  return resolved;
}

export async function loadBuildGuardPolicy() {
  return JSON.parse(await readFile(guardPolicyPath, "utf8"));
}

export function allowlistedTarget(policy) {
  return resolve(migrationDirectory, policy.targetRelativeToMigration);
}

export function allowlistedTargets(policy) {
  return [
    policy.targetRelativeToMigration,
    ...(policy.additionalTargetRelativesToMigration ?? []),
  ].map((relative) => resolve(migrationDirectory, relative));
}

export function assertAllowlistedTarget(candidate, allowed) {
  const resolvedCandidate = resolve(candidate);
  const resolvedAllowed = (Array.isArray(allowed) ? allowed : [allowed]).map(
    (path) => resolve(path),
  );
  if (!resolvedAllowed.includes(resolvedCandidate)) {
    throw new Error(
      `${resolvedCandidate} is not an allowlisted disposable target`,
    );
  }
  return resolvedCandidate;
}

export function evaluatePreflight(metrics, policy) {
  if (metrics.freeKiB < policy.preflightFreeKiB) {
    return { ok: false, reason: "preflight-free-space" };
  }
  if (metrics.memoryAvailableKiB < policy.minMemoryKiB) {
    return { ok: false, reason: "preflight-memory" };
  }
  return { ok: true, reason: null };
}

export function evaluateRuntimeSample(metrics, policy) {
  if (metrics.targetKiB > policy.maxTargetKiB) {
    return { ok: false, reason: "target-size" };
  }
  if (metrics.freeKiB < policy.runtimeStopFreeKiB) {
    return { ok: false, reason: "runtime-free-space" };
  }
  if (metrics.memoryAvailableKiB < policy.minMemoryKiB) {
    return { ok: false, reason: "available-memory" };
  }
  return { ok: true, reason: null };
}

export async function allocatedKiB(path, fsOps = { lstat, readdir }) {
  let stat;
  try {
    stat = await fsOps.lstat(path);
  } catch (error) {
    if (error.code === "ENOENT") return 0;
    throw error;
  }
  if (stat.isSymbolicLink()) return Math.ceil((stat.blocks * 512) / 1024);
  if (!stat.isDirectory()) return Math.ceil((stat.blocks * 512) / 1024);

  let total = 0;
  let entries;
  try {
    entries = await fsOps.readdir(path);
  } catch (error) {
    if (error.code === "ENOENT") return 0;
    throw error;
  }
  for (const entry of entries) {
    total += await allocatedKiB(join(path, entry), fsOps);
  }
  return total;
}

async function existingAncestor(path) {
  let current = resolve(path);
  while (true) {
    try {
      await lstat(current);
      return current;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
      const parent = dirname(current);
      if (parent === current) throw error;
      current = parent;
    }
  }
}

async function memoryAvailableKiB() {
  const match = (await readFile("/proc/meminfo", "utf8")).match(
    /^MemAvailable:\s+(\d+)\s+kB$/m,
  );
  if (!match) throw new Error("could not read MemAvailable from /proc/meminfo");
  return Number(match[1]);
}

export async function collectMetrics(target) {
  const filesystemPath = await existingAncestor(target);
  const filesystem = await statfs(filesystemPath, { bigint: true });
  return {
    freeKiB: Number((filesystem.bavail * filesystem.bsize) / 1024n),
    targetKiB: await allocatedKiB(target),
    memoryAvailableKiB: await memoryAvailableKiB(),
  };
}

async function verifyOwnedTarget(target, allowed) {
  const safeTarget = assertAllowlistedTarget(target, allowed);
  let stat;
  try {
    stat = await lstat(safeTarget);
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
  if (stat.isSymbolicLink()) {
    throw new Error(
      `allowlisted disposable target is a symbolic link: ${safeTarget}`,
    );
  }
  if (!stat.isDirectory()) {
    throw new Error(
      `allowlisted disposable target is not a directory: ${safeTarget}`,
    );
  }

  let sentinel;
  try {
    sentinel = JSON.parse(
      await readFile(join(safeTarget, ownershipSentinel), "utf8"),
    );
  } catch (error) {
    throw new Error(
      `allowlisted disposable target has no valid ownership sentinel: ${error.message}`,
    );
  }
  if (
    sentinel.kind !== "butter-paper-gpui-component-build-target" ||
    sentinel.target !== safeTarget
  ) {
    throw new Error(
      "allowlisted disposable target ownership sentinel does not match",
    );
  }
  return true;
}

export async function initializeTarget(target, allowed) {
  const safeTarget = assertAllowlistedTarget(target, allowed);
  const parent = dirname(safeTarget);
  await mkdir(parent, { recursive: true });
  const parentStat = await lstat(parent);
  if (parentStat.isSymbolicLink()) {
    throw new Error(`disposable target parent is a symbolic link: ${parent}`);
  }

  try {
    await mkdir(safeTarget);
    await writeFile(
      join(safeTarget, ownershipSentinel),
      `${JSON.stringify({ kind: "butter-paper-gpui-component-build-target", target: safeTarget })}\n`,
      { flag: "wx" },
    );
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
    await verifyOwnedTarget(safeTarget, allowed);
  }
  return safeTarget;
}

export async function cleanupTarget(target, allowed, fsOps = { rm }) {
  const safeTarget = assertAllowlistedTarget(target, allowed);
  if (!(await verifyOwnedTarget(safeTarget, allowed))) return;
  await fsOps.rm(safeTarget, {
    recursive: true,
    force: true,
    maxRetries: 8,
    retryDelay: 150,
  });
}

async function main() {
  const command = process.argv[2];
  if (!command) return;

  if (command === "runner-mode") {
    process.stdout.write(
      `${JSON.stringify(resolveRunnerMode(process.argv[3] ?? "all-targets"))}\n`,
    );
    return;
  }

  if (command === "disposition") {
    const status = Number(process.argv[3]);
    if (!Number.isInteger(status))
      throw new Error("disposition status must be an integer");
    const limitReason =
      process.argv[4] && process.argv[4] !== "-" ? process.argv[4] : null;
    const explicitCleanup = process.argv[5] === "true";
    process.stdout.write(
      `${JSON.stringify(cleanupDisposition({ status, limitReason, explicitCleanup }))}\n`,
    );
    return;
  }

  const policy = await loadBuildGuardPolicy();
  const allowed = allowlistedTargets(policy);
  const target = assertAllowlistedTarget(
    process.argv[3] ?? allowlistedTarget(policy),
    allowed,
  );

  if (command === "cleanup") {
    await cleanupTarget(target, allowed);
    process.stdout.write(`${JSON.stringify({ status: "cleaned", target })}\n`);
    return;
  }

  if (command === "initialize") {
    await initializeTarget(target, allowed);
    process.stdout.write(
      `${JSON.stringify({ status: "initialized", target })}\n`,
    );
    return;
  }

  if (command === "sample") {
    await verifyOwnedTarget(target, allowed);
  }

  const metrics = await collectMetrics(target);
  const evaluation =
    command === "preflight"
      ? evaluatePreflight(metrics, policy)
      : command === "sample"
        ? evaluateRuntimeSample(metrics, policy)
        : command === "metrics"
          ? { ok: true, reason: null }
          : null;
  if (!evaluation) throw new Error(`unknown command ${command}`);

  process.stdout.write(
    `${JSON.stringify({ ...evaluation, ...metrics, target })}\n`,
  );
  if (!evaluation.ok) process.exitCode = 3;
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await main();
}
