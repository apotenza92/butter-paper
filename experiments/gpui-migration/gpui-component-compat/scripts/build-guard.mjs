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

const runnerModes = Object.freeze({
  "all-targets": Object.freeze({
    name: "all-targets",
    cargoArgs: Object.freeze(["--all-targets"]),
    controlledFailureStatus: null,
  }),
  "document-tab-bar": Object.freeze({
    name: "document-tab-bar",
    cargoArgs: Object.freeze(["--test", "document_tab_bar"]),
    controlledFailureStatus: null,
  }),
  "native-app-shell": Object.freeze({
    name: "native-app-shell",
    cargoArgs: Object.freeze(["--test", "native_app_shell"]),
    controlledFailureStatus: null,
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
  }),
  "document-template": Object.freeze({
    name: "document-template",
    cargoArgs: Object.freeze(["--test", "document_workspace", "generated_template_"]),
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
  }),
  "measurement-path-workspace": Object.freeze({
    name: "measurement-path-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "polylength_area_workspace_",
    ]),
    controlledFailureStatus: null,
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
  }),
  "arc-workspace": Object.freeze({
    name: "arc-workspace",
    cargoArgs: Object.freeze([
      "--test",
      "document_workspace",
      "arc_workspace_",
    ]),
    controlledFailureStatus: null,
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
