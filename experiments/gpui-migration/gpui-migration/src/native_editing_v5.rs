//! Pure domain seam for the v5 native property-edit and snap workloads.
//!
//! The GPUI adapter owns native input, presentation, and evidence timestamps.
//! This module owns deterministic plan validation, transaction boundaries, and
//! exact committed/undone/redone annotation state.

use std::{error::Error, fmt};

use serde_json::Value;

use crate::annotation_model::{
    AnnotationDocument, AnnotationError, CommitOutcome, MarkupId, PdfPoint, PdfRect,
    RectangleAnnotation, RectangleAppearance,
};

pub const EMBEDDED_V5_WORKLOAD: &str =
    include_str!("../../performance/comparison-workload-v5.materialized.json");
pub const V5_DECISION_ID: &str = "bp-perf-v5-decision-1";

const PROPERTY_OPERATION: &str = "annotation.property-edit-undo-native";
const SNAP_OPERATION: &str = "annotation.snap-transform-native";

const PROPERTY_MILESTONES: &[&str] = &[
    "trusted-native-input-complete",
    "property-user-gesture-complete",
    "property-state-current-before-undo",
    "native-property-presentation-acknowledged",
    "application-undo-applied-once",
    "implementation-history-outcome-recorded",
    "thumbnail-current",
];

const SNAP_MILESTONES: &[&str] = &[
    "timestamped-native-input-complete",
    "snap-target-acquired",
    "snap-guide-presented",
    "snapped-geometry-exact",
    "gesture-committed-once",
    "undo-redo-exact",
    "thumbnail-current",
];

#[derive(Clone, Debug, PartialEq)]
pub struct NativeEditingV5Plan {
    pub property_edit: PropertyEditPlan,
    pub snap_transform: SnapTransformPlan,
}

impl NativeEditingV5Plan {
    pub fn embedded() -> Result<Self, NativeEditingV5Error> {
        Self::from_json(EMBEDDED_V5_WORKLOAD)
    }

    pub fn from_json(json: &str) -> Result<Self, NativeEditingV5Error> {
        let root: Value = serde_json::from_str(json)
            .map_err(|error| NativeEditingV5Error::InvalidPlan(error.to_string()))?;
        require_string(&root, "manifest_id", V5_DECISION_ID)?;
        require_string(&root, "decision_contract_version", V5_DECISION_ID)?;
        Ok(Self {
            property_edit: PropertyEditPlan::parse(find_command(&root, PROPERTY_OPERATION)?)?,
            snap_transform: SnapTransformPlan::parse(find_command(&root, SNAP_OPERATION)?)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyEditPlan {
    pub command_id: String,
    pub target_id: MarkupId,
    pub setup_rect: PdfRect,
    pub original_stroke_width_pt: f64,
    pub edited_stroke_width_pt: f64,
    pub candidate_policy: PropertyCandidatePolicy,
    pub electron_baseline_policy: ElectronBaselinePolicy,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyCandidatePolicy {
    pub effective_history_revision_delta: usize,
    pub application_undo_count: usize,
    pub canonical_state_restored: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElectronBaselinePolicy {
    pub allowed_known_defect_id: String,
    pub effective_history_revision_delta: usize,
    pub application_undo_count: usize,
    pub final_stroke_width_pt: f64,
}

impl PropertyEditPlan {
    fn parse(command: &Value) -> Result<Self, NativeEditingV5Error> {
        require_string(command, "id", "annotation:native-property-edit-undo")?;
        require_string(command, "input_lane", "native-replay")?;
        require_string_at(command, "/property_edit/property", "stroke_width_points")?;
        let setup_rect = parse_corners(value_at(command, "/setup/rectangle")?)?;
        let original_stroke_width_pt = number_at(command, "/property_edit/from")?;
        let edited_stroke_width_pt = number_at(command, "/property_edit/to")?;
        require_number_at(
            command,
            "/setup/stroke_width_points",
            original_stroke_width_pt,
        )?;
        require_number_at(
            command,
            "/candidate_policy/effective_history_revision_delta",
            1.0,
        )?;
        require_number_at(command, "/candidate_policy/application_undo_count", 1.0)?;
        require_bool_at(command, "/candidate_policy/canonical_state_restored", true)?;
        require_string_at(
            command,
            "/electron_baseline_policy/allowed_known_defect_id",
            "electron-numeric-property-input-blur-duplicate-history-v1",
        )?;
        require_number_at(
            command,
            "/electron_baseline_policy/effective_history_revision_delta",
            2.0,
        )?;
        require_number_at(
            command,
            "/electron_baseline_policy/application_undo_count",
            1.0,
        )?;
        require_number_at(
            command,
            "/electron_baseline_policy/final_stroke_width_points",
            edited_stroke_width_pt,
        )?;
        if original_stroke_width_pt != 1.5 || edited_stroke_width_pt != 4.0 {
            return Err(invalid_plan(
                "native property edit must change stroke width from 1.5 pt to 4 pt",
            ));
        }
        let expected_milestones = parse_milestones(command, PROPERTY_MILESTONES)?;
        Ok(Self {
            command_id: string_at(command, "/id")?.to_owned(),
            target_id: MarkupId::new(string_at(command, "/setup/annotation_id")?)?,
            setup_rect,
            original_stroke_width_pt,
            edited_stroke_width_pt,
            candidate_policy: PropertyCandidatePolicy {
                effective_history_revision_delta: 1,
                application_undo_count: 1,
                canonical_state_restored: true,
            },
            electron_baseline_policy: ElectronBaselinePolicy {
                allowed_known_defect_id:
                    "electron-numeric-property-input-blur-duplicate-history-v1".into(),
                effective_history_revision_delta: 2,
                application_undo_count: 1,
                final_stroke_width_pt: edited_stroke_width_pt,
            },
            expected_milestones,
        })
    }

    /// Starts the exact frozen edit with the 4 pt value already staged.
    pub fn begin_transaction(
        &self,
        document: &AnnotationDocument,
    ) -> Result<StrokeWidthEditTransaction, NativeEditingV5Error> {
        let annotation = find_rectangle(document, &self.target_id)?;
        if annotation.rect != self.setup_rect
            || annotation.appearance.stroke_width_pt() != self.original_stroke_width_pt
        {
            return Err(NativeEditingV5Error::TargetChanged(self.target_id.clone()));
        }
        let mut transaction = StrokeWidthEditTransaction::begin(document, &self.target_id)?;
        transaction.stage_stroke_width(self.edited_stroke_width_pt)?;
        Ok(transaction)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnapTransformPlan {
    pub command_id: String,
    pub target_id: MarkupId,
    pub page_index: u32,
    pub setup_rect: PdfRect,
    pub start: PdfPoint,
    pub unsnapped_end: PdfPoint,
    pub expected_snapped_delta: Translation,
    pub expected_final_rect: PdfRect,
    pub sample_count: usize,
    pub rate_hz: u32,
    pub duration_ms: u64,
    pub grid_spacing_pt: f64,
    pub sensitivity_css_px: f64,
    pub expected_milestones: Vec<String>,
}

impl SnapTransformPlan {
    fn parse(command: &Value) -> Result<Self, NativeEditingV5Error> {
        require_string(command, "id", "annotation:native-snap-transform-120hz")?;
        require_string(command, "input_lane", "native-replay")?;
        require_string_at(
            command,
            "/pointer_path/coordinate_space",
            "pdf-points-bottom-left",
        )?;
        require_bool_at(command, "/snap/enabled", true)?;
        require_number_at(command, "/snap/sensitivity/value", 8.0)?;
        require_string_at(command, "/snap/sensitivity/unit", "css-px")?;
        require_string_at(
            command,
            "/snap/sensitivity/threshold_norm",
            "per-axis-l-infinity",
        )?;
        require_bool_at(command, "/snap/sensitivity/inclusive", true)?;
        require_string_at(command, "/snap/mode", "translate-nearest-grid")?;

        let setup_rect = parse_corners(value_at(command, "/setup/rectangle")?)?;
        let start = parse_point(value_at(command, "/pointer_path/start")?)?;
        let unsnapped_end = parse_point(value_at(command, "/pointer_path/unsnapped_end")?)?;
        let expected_snapped_delta =
            parse_translation(value_at(command, "/pointer_path/expected_snapped_delta")?)?;
        let expected_final_rect = parse_corners(value_at(command, "/expected_final_rectangle")?)?;
        let sample_count = usize_at(command, "/expected_sample_count")?;
        let rate_hz = u32_at(command, "/rate_hz")?;
        let duration_ms = u64_at(command, "/duration_ms")?;
        let grid_spacing_pt = number_at(command, "/snap/grid_spacing_points")?;
        let sensitivity_css_px = number_at(command, "/snap/sensitivity/value")?;
        let expected_milestones = parse_milestones(command, SNAP_MILESTONES)?;

        let raw = Translation::new(unsnapped_end.x - start.x, unsnapped_end.y - start.y)?;
        let candidate = Translation::new(
            (raw.x / grid_spacing_pt).round() * grid_spacing_pt,
            (raw.y / grid_spacing_pt).round() * grid_spacing_pt,
        )?;
        let correction = Translation::new(candidate.x - raw.x, candidate.y - raw.y)?;
        if sample_count != 361
            || rate_hz != 120
            || duration_ms != 3_000
            || raw != (Translation { x: 97.0, y: 83.0 })
            || candidate != expected_snapped_delta
            || correction != (Translation { x: -7.0, y: 7.0 })
            || translated_rect(setup_rect, candidate)? != expected_final_rect
        {
            return Err(invalid_plan(
                "native snap command does not match the frozen 120 Hz inclusive L-infinity case",
            ));
        }

        Ok(Self {
            command_id: string_at(command, "/id")?.to_owned(),
            target_id: MarkupId::new(string_at(command, "/setup/annotation_id")?)?,
            page_index: 0,
            setup_rect,
            start,
            unsnapped_end,
            expected_snapped_delta,
            expected_final_rect,
            sample_count,
            rate_hz,
            duration_ms,
            grid_spacing_pt,
            sensitivity_css_px,
            expected_milestones,
        })
    }

    /// Starts the exact frozen snap gesture and requires all 361 updates before commit.
    pub fn begin_gesture(
        &self,
        document: &mut AnnotationDocument,
        pointer_id: u64,
        tolerance_pt: f64,
        observed_pixels_per_point: f64,
    ) -> Result<SnapTranslateGesture, NativeEditingV5Error> {
        let annotation = find_rectangle(document, &self.target_id)?;
        if annotation.page_index != self.page_index || annotation.rect != self.setup_rect {
            return Err(NativeEditingV5Error::TargetChanged(self.target_id.clone()));
        }
        SnapTranslateGesture::begin_internal(
            document,
            &self.target_id,
            pointer_id,
            self.page_index,
            self.start,
            tolerance_pt,
            InclusiveLInfGridSnap::from_css_pixels(
                self.grid_spacing_pt,
                self.sensitivity_css_px,
                observed_pixels_per_point,
            )?,
            Some(self.sample_count),
            Some(self.expected_final_rect),
        )
    }

    /// Starts the live GUI gesture. X11 may coalesce repeated physical-pixel
    /// positions, so the native injector owns the exact 361-sample receipt and
    /// this transaction verifies the observed first/final geometry and commit.
    pub fn begin_observed_gesture(
        &self,
        document: &mut AnnotationDocument,
        pointer_id: u64,
        tolerance_pt: f64,
        observed_pixels_per_point: f64,
    ) -> Result<SnapTranslateGesture, NativeEditingV5Error> {
        let annotation = find_rectangle(document, &self.target_id)?;
        if annotation.page_index != self.page_index || annotation.rect != self.setup_rect {
            return Err(NativeEditingV5Error::TargetChanged(self.target_id.clone()));
        }
        SnapTranslateGesture::begin_internal(
            document,
            &self.target_id,
            pointer_id,
            self.page_index,
            self.start,
            tolerance_pt,
            InclusiveLInfGridSnap::from_css_pixels(
                self.grid_spacing_pt,
                self.sensitivity_css_px,
                observed_pixels_per_point,
            )?,
            None,
            Some(self.expected_final_rect),
        )
    }
}

/// A native property panel may stage many values. Only `commit` mutates history.
#[derive(Clone, Debug)]
pub struct StrokeWidthEditTransaction {
    target_id: MarkupId,
    original_rect: PdfRect,
    original_appearance: RectangleAppearance,
    staged_appearance: RectangleAppearance,
    history_before: (usize, usize),
    revision_before: u64,
}

impl StrokeWidthEditTransaction {
    pub fn begin(
        document: &AnnotationDocument,
        target_id: &MarkupId,
    ) -> Result<Self, NativeEditingV5Error> {
        let annotation = find_rectangle(document, target_id)?;
        Ok(Self {
            target_id: target_id.clone(),
            original_rect: annotation.rect,
            original_appearance: annotation.appearance.clone(),
            staged_appearance: annotation.appearance.clone(),
            history_before: document.history_depths(),
            revision_before: document.snapshot().revision,
        })
    }

    pub fn stage_stroke_width(
        &mut self,
        stroke_width_pt: f64,
    ) -> Result<&mut Self, NativeEditingV5Error> {
        self.staged_appearance =
            appearance_with_stroke_width(&self.original_appearance, stroke_width_pt)?;
        Ok(self)
    }

    pub fn staged_appearance(&self) -> &RectangleAppearance {
        &self.staged_appearance
    }

    pub fn commit(
        self,
        document: &mut AnnotationDocument,
    ) -> Result<PropertyEditCommit, NativeEditingV5Error> {
        let current = find_rectangle(document, &self.target_id)?;
        if current.rect != self.original_rect
            || current.appearance != self.original_appearance
            || document.history_depths() != self.history_before
            || document.snapshot().revision != self.revision_before
        {
            return Err(NativeEditingV5Error::TargetChanged(self.target_id));
        }
        if self.staged_appearance == self.original_appearance {
            return Err(NativeEditingV5Error::NoChange);
        }
        if !document.select(&self.target_id) {
            return Err(NativeEditingV5Error::TargetNotFound(self.target_id));
        }
        if !document.set_selected_appearance(self.staged_appearance.clone())? {
            return Err(NativeEditingV5Error::NoChange);
        }
        let history_after = document.history_depths();
        require_single_commit(self.history_before, history_after)?;
        let receipt = PropertyEditCommit {
            target_id: self.target_id,
            rect: self.original_rect,
            before: self.original_appearance,
            after: self.staged_appearance,
            history_before: self.history_before,
            history_after,
        };
        receipt.verify_applied(document)?;
        Ok(receipt)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyEditCommit {
    pub target_id: MarkupId,
    pub rect: PdfRect,
    pub before: RectangleAppearance,
    pub after: RectangleAppearance,
    pub history_before: (usize, usize),
    pub history_after: (usize, usize),
}

impl PropertyEditCommit {
    pub fn verify_applied(
        &self,
        document: &AnnotationDocument,
    ) -> Result<(), NativeEditingV5Error> {
        verify_rectangle(document, &self.target_id, self.rect, &self.after)
    }

    pub fn verify_undone(&self, document: &AnnotationDocument) -> Result<(), NativeEditingV5Error> {
        verify_rectangle(document, &self.target_id, self.rect, &self.before)?;
        if document.history_depths() != (self.history_before.0, 1) {
            return Err(history_error(
                "property undo did not restore the exact history boundary",
            ));
        }
        Ok(())
    }

    pub fn verify_redone(&self, document: &AnnotationDocument) -> Result<(), NativeEditingV5Error> {
        self.verify_applied(document)?;
        if document.history_depths() != self.history_after {
            return Err(history_error(
                "property redo did not restore the committed history boundary",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Translation {
    pub x: f64,
    pub y: f64,
}

impl Translation {
    pub fn new(x: f64, y: f64) -> Result<Self, NativeEditingV5Error> {
        if !x.is_finite() || !y.is_finite() {
            return Err(invalid_plan("translation components must be finite"));
        }
        Ok(Self {
            x: canonical_zero(x),
            y: canonical_zero(y),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InclusiveLInfGridSnap {
    pub grid_spacing_pt: f64,
    pub sensitivity_css_px: f64,
    pub observed_pixels_per_point: f64,
    pub derived_threshold_pt: f64,
}

impl InclusiveLInfGridSnap {
    pub fn from_css_pixels(
        grid_spacing_pt: f64,
        sensitivity_css_px: f64,
        observed_pixels_per_point: f64,
    ) -> Result<Self, NativeEditingV5Error> {
        if !grid_spacing_pt.is_finite()
            || !sensitivity_css_px.is_finite()
            || !observed_pixels_per_point.is_finite()
            || grid_spacing_pt <= 0.0
            || sensitivity_css_px < 0.0
            || observed_pixels_per_point <= 0.0
        {
            return Err(invalid_plan(
                "grid spacing and pixel scale must be positive and snap sensitivity must be nonnegative",
            ));
        }
        Ok(Self {
            grid_spacing_pt,
            sensitivity_css_px,
            observed_pixels_per_point,
            derived_threshold_pt: sensitivity_css_px / observed_pixels_per_point,
        })
    }

    /// Snaps the complete translation only when both axis corrections fit the
    /// inclusive threshold. This is `max(abs(dx), abs(dy)) <= threshold`.
    pub fn resolve(self, raw: Translation) -> Result<SnapResolution, NativeEditingV5Error> {
        let candidate = Translation::new(
            (raw.x / self.grid_spacing_pt).round() * self.grid_spacing_pt,
            (raw.y / self.grid_spacing_pt).round() * self.grid_spacing_pt,
        )?;
        let correction = Translation::new(candidate.x - raw.x, candidate.y - raw.y)?;
        let acquired = correction.x.abs().max(correction.y.abs()) <= self.derived_threshold_pt;
        Ok(SnapResolution {
            raw,
            candidate,
            correction,
            acquired,
            applied: if acquired { candidate } else { raw },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapResolution {
    pub raw: Translation,
    pub candidate: Translation,
    pub correction: Translation,
    pub acquired: bool,
    pub applied: Translation,
}

/// One pointer-down/up transaction. Updates change only the annotation preview.
#[derive(Debug)]
pub struct SnapTranslateGesture {
    target_id: MarkupId,
    pointer_id: u64,
    start: PdfPoint,
    original_rect: PdfRect,
    original_appearance: RectangleAppearance,
    snap: InclusiveLInfGridSnap,
    history_before: (usize, usize),
    revision_before: u64,
    sample_count: usize,
    expected_sample_count: Option<usize>,
    expected_final_rect: Option<PdfRect>,
    latest: Option<SnapResolution>,
}

impl SnapTranslateGesture {
    pub fn begin(
        document: &mut AnnotationDocument,
        target_id: &MarkupId,
        pointer_id: u64,
        page_index: u32,
        start: PdfPoint,
        tolerance_pt: f64,
        snap: InclusiveLInfGridSnap,
    ) -> Result<Self, NativeEditingV5Error> {
        Self::begin_internal(
            document,
            target_id,
            pointer_id,
            page_index,
            start,
            tolerance_pt,
            snap,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn begin_internal(
        document: &mut AnnotationDocument,
        target_id: &MarkupId,
        pointer_id: u64,
        page_index: u32,
        start: PdfPoint,
        tolerance_pt: f64,
        snap: InclusiveLInfGridSnap,
        expected_sample_count: Option<usize>,
        expected_final_rect: Option<PdfRect>,
    ) -> Result<Self, NativeEditingV5Error> {
        let annotation = find_rectangle(document, target_id)?.clone();
        let history_before = document.history_depths();
        let revision_before = document.snapshot().revision;
        let acquired_id = document.begin_move(pointer_id, page_index, start, tolerance_pt)?;
        if acquired_id.as_ref() != Some(target_id) {
            if acquired_id.is_some() {
                document.cancel_gesture(pointer_id)?;
            }
            return Err(NativeEditingV5Error::GestureInvariant(
                "pointer down did not acquire the requested rectangle body".into(),
            ));
        }
        Ok(Self {
            target_id: target_id.clone(),
            pointer_id,
            start,
            original_rect: annotation.rect,
            original_appearance: annotation.appearance,
            snap,
            history_before,
            revision_before,
            sample_count: 0,
            expected_sample_count,
            expected_final_rect,
            latest: None,
        })
    }

    pub fn update(
        &mut self,
        document: &mut AnnotationDocument,
        raw_endpoint: PdfPoint,
    ) -> Result<SnapResolution, NativeEditingV5Error> {
        self.require_uncommitted_document(document)?;
        let raw = Translation::new(raw_endpoint.x - self.start.x, raw_endpoint.y - self.start.y)?;
        let resolution = self.snap.resolve(raw)?;
        let adjusted_endpoint = PdfPoint::new(
            self.start.x + resolution.applied.x,
            self.start.y + resolution.applied.y,
        )?;
        let preview = document.update_gesture(self.pointer_id, adjusted_endpoint)?;
        let expected_rect = translated_rect(self.original_rect, resolution.applied)?;
        if preview.annotation.id != self.target_id || preview.annotation.rect != expected_rect {
            return Err(NativeEditingV5Error::GestureInvariant(
                "annotation preview does not match the resolved translation".into(),
            ));
        }
        self.sample_count += 1;
        self.latest = Some(resolution);
        self.require_uncommitted_document(document)?;
        Ok(resolution)
    }

    pub fn sample_count(&self) -> usize {
        self.sample_count
    }

    pub fn latest_resolution(&self) -> Option<SnapResolution> {
        self.latest
    }

    pub fn cancel(self, document: &mut AnnotationDocument) -> Result<(), NativeEditingV5Error> {
        document.cancel_gesture(self.pointer_id)?;
        if document.history_depths() != self.history_before
            || document.snapshot().revision != self.revision_before
        {
            return Err(history_error(
                "cancelled snap gesture changed committed history",
            ));
        }
        Ok(())
    }

    pub fn commit(
        self,
        document: &mut AnnotationDocument,
    ) -> Result<SnapGestureCommit, NativeEditingV5Error> {
        self.require_uncommitted_document(document)?;
        let Some(resolution) = self.latest else {
            return self.abort_commit(document, "snap gesture has no pointer samples");
        };
        if let Some(expected) = self.expected_sample_count
            && self.sample_count != expected
        {
            let message = format!(
                "snap gesture has {} samples, expected {expected}",
                self.sample_count
            );
            return self.abort_commit(document, message);
        }
        let final_rect = translated_rect(self.original_rect, resolution.applied)?;
        if self
            .expected_final_rect
            .is_some_and(|expected| expected != final_rect)
        {
            return self.abort_commit(
                document,
                "snap gesture final rectangle differs from the frozen plan",
            );
        }
        match document.commit_gesture(self.pointer_id)? {
            CommitOutcome::Updated(id) if id == self.target_id => {}
            outcome => {
                return Err(NativeEditingV5Error::GestureInvariant(format!(
                    "snap commit returned {outcome:?}"
                )));
            }
        }
        let history_after = document.history_depths();
        require_single_commit(self.history_before, history_after)?;
        let receipt = SnapGestureCommit {
            target_id: self.target_id,
            original_rect: self.original_rect,
            final_rect,
            appearance: self.original_appearance,
            resolution,
            sample_count: self.sample_count,
            sensitivity_css_px: self.snap.sensitivity_css_px,
            observed_pixels_per_point: self.snap.observed_pixels_per_point,
            derived_threshold_pt: self.snap.derived_threshold_pt,
            history_before: self.history_before,
            history_after,
        };
        receipt.verify_applied(document)?;
        Ok(receipt)
    }

    fn abort_commit(
        self,
        document: &mut AnnotationDocument,
        message: impl Into<String>,
    ) -> Result<SnapGestureCommit, NativeEditingV5Error> {
        document.cancel_gesture(self.pointer_id)?;
        if document.history_depths() != self.history_before
            || document.snapshot().revision != self.revision_before
        {
            return Err(history_error(
                "rejected snap commit changed committed history",
            ));
        }
        Err(NativeEditingV5Error::GestureInvariant(message.into()))
    }

    fn require_uncommitted_document(
        &self,
        document: &AnnotationDocument,
    ) -> Result<(), NativeEditingV5Error> {
        if document.history_depths() != self.history_before
            || document.snapshot().revision != self.revision_before
        {
            return Err(history_error(
                "snap pointer updates changed committed state before pointer up",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnapGestureCommit {
    pub target_id: MarkupId,
    pub original_rect: PdfRect,
    pub final_rect: PdfRect,
    pub appearance: RectangleAppearance,
    pub resolution: SnapResolution,
    pub sample_count: usize,
    pub sensitivity_css_px: f64,
    pub observed_pixels_per_point: f64,
    pub derived_threshold_pt: f64,
    pub history_before: (usize, usize),
    pub history_after: (usize, usize),
}

impl SnapGestureCommit {
    pub fn verify_applied(
        &self,
        document: &AnnotationDocument,
    ) -> Result<(), NativeEditingV5Error> {
        verify_rectangle(document, &self.target_id, self.final_rect, &self.appearance)
    }

    pub fn verify_undone(&self, document: &AnnotationDocument) -> Result<(), NativeEditingV5Error> {
        verify_rectangle(
            document,
            &self.target_id,
            self.original_rect,
            &self.appearance,
        )?;
        if document.history_depths() != (self.history_before.0, 1) {
            return Err(history_error(
                "snap undo did not restore the exact history boundary",
            ));
        }
        Ok(())
    }

    pub fn verify_redone(&self, document: &AnnotationDocument) -> Result<(), NativeEditingV5Error> {
        self.verify_applied(document)?;
        if document.history_depths() != self.history_after {
            return Err(history_error(
                "snap redo did not restore the committed history boundary",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeEditingV5Error {
    InvalidPlan(String),
    Annotation(AnnotationError),
    TargetNotFound(MarkupId),
    TargetChanged(MarkupId),
    NoChange,
    HistoryInvariant(String),
    GestureInvariant(String),
}

impl fmt::Display for NativeEditingV5Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlan(message) => write!(formatter, "invalid v5 editing plan: {message}"),
            Self::Annotation(error) => error.fmt(formatter),
            Self::TargetNotFound(id) => write!(formatter, "rectangle {id} does not exist"),
            Self::TargetChanged(id) => write!(formatter, "rectangle {id} changed during editing"),
            Self::NoChange => write!(formatter, "the edit has no committed change"),
            Self::HistoryInvariant(message) => write!(formatter, "history invariant: {message}"),
            Self::GestureInvariant(message) => write!(formatter, "gesture invariant: {message}"),
        }
    }
}

impl Error for NativeEditingV5Error {}

impl From<AnnotationError> for NativeEditingV5Error {
    fn from(error: AnnotationError) -> Self {
        Self::Annotation(error)
    }
}

fn appearance_with_stroke_width(
    original: &RectangleAppearance,
    stroke_width_pt: f64,
) -> Result<RectangleAppearance, NativeEditingV5Error> {
    Ok(RectangleAppearance::new(
        original.stroke_color(),
        stroke_width_pt,
        original.fill_color().map(str::to_owned),
        original.opacity(),
    )?
    .with_fill_opacity(original.fill_opacity())?
    .with_stroke_style(original.stroke_style()))
}

fn find_rectangle<'a>(
    document: &'a AnnotationDocument,
    target_id: &MarkupId,
) -> Result<&'a RectangleAnnotation, NativeEditingV5Error> {
    document
        .rectangles()
        .iter()
        .find(|annotation| &annotation.id == target_id)
        .ok_or_else(|| NativeEditingV5Error::TargetNotFound(target_id.clone()))
}

fn verify_rectangle(
    document: &AnnotationDocument,
    target_id: &MarkupId,
    rect: PdfRect,
    appearance: &RectangleAppearance,
) -> Result<(), NativeEditingV5Error> {
    let annotation = find_rectangle(document, target_id)?;
    if annotation.rect != rect || &annotation.appearance != appearance {
        return Err(NativeEditingV5Error::TargetChanged(target_id.clone()));
    }
    Ok(())
}

fn require_single_commit(
    before: (usize, usize),
    after: (usize, usize),
) -> Result<(), NativeEditingV5Error> {
    if after.0 != before.0 + 1 || after.1 != 0 {
        return Err(history_error(
            "one commit must add exactly one undo entry and clear redo history",
        ));
    }
    Ok(())
}

fn translated_rect(
    rect: PdfRect,
    translation: Translation,
) -> Result<PdfRect, NativeEditingV5Error> {
    Ok(PdfRect::new(
        rect.x + translation.x,
        rect.y + translation.y,
        rect.width,
        rect.height,
    )?)
}

fn find_command<'a>(root: &'a Value, operation: &str) -> Result<&'a Value, NativeEditingV5Error> {
    root.get("journeys")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|journey| journey.get("commands").and_then(Value::as_array))
        .flatten()
        .find(|command| command.get("operation").and_then(Value::as_str) == Some(operation))
        .ok_or_else(|| invalid_plan(format!("missing {operation} command")))
}

fn parse_milestones(
    command: &Value,
    expected: &[&str],
) -> Result<Vec<String>, NativeEditingV5Error> {
    let actual = command
        .get("expected_milestones")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_plan("expected_milestones must be an array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid_plan("milestone names must be strings"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if actual.iter().map(String::as_str).collect::<Vec<_>>() != expected {
        return Err(invalid_plan("expected milestone sequence changed"));
    }
    Ok(actual)
}

fn parse_corners(value: &Value) -> Result<PdfRect, NativeEditingV5Error> {
    let x1 = number_at(value, "/x1")?;
    let y1 = number_at(value, "/y1")?;
    let x2 = number_at(value, "/x2")?;
    let y2 = number_at(value, "/y2")?;
    if x2 < x1 || y2 < y1 {
        return Err(invalid_plan("rectangle corners must be ordered"));
    }
    Ok(PdfRect::new(x1, y1, x2 - x1, y2 - y1)?)
}

fn parse_point(value: &Value) -> Result<PdfPoint, NativeEditingV5Error> {
    Ok(PdfPoint::new(
        number_at(value, "/x")?,
        number_at(value, "/y")?,
    )?)
}

fn parse_translation(value: &Value) -> Result<Translation, NativeEditingV5Error> {
    Translation::new(number_at(value, "/x")?, number_at(value, "/y")?)
}

fn value_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a Value, NativeEditingV5Error> {
    value
        .pointer(pointer)
        .ok_or_else(|| invalid_plan(format!("missing {pointer}")))
}

fn string_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, NativeEditingV5Error> {
    value_at(value, pointer)?
        .as_str()
        .ok_or_else(|| invalid_plan(format!("{pointer} must be a string")))
}

fn number_at(value: &Value, pointer: &str) -> Result<f64, NativeEditingV5Error> {
    value_at(value, pointer)?
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| invalid_plan(format!("{pointer} must be a finite number")))
}

fn usize_at(value: &Value, pointer: &str) -> Result<usize, NativeEditingV5Error> {
    value_at(value, pointer)?
        .as_u64()
        .and_then(|number| usize::try_from(number).ok())
        .ok_or_else(|| invalid_plan(format!("{pointer} must be an unsigned integer")))
}

fn u32_at(value: &Value, pointer: &str) -> Result<u32, NativeEditingV5Error> {
    value_at(value, pointer)?
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| invalid_plan(format!("{pointer} must be a u32")))
}

fn u64_at(value: &Value, pointer: &str) -> Result<u64, NativeEditingV5Error> {
    value_at(value, pointer)?
        .as_u64()
        .ok_or_else(|| invalid_plan(format!("{pointer} must be an unsigned integer")))
}

fn require_string(value: &Value, key: &str, expected: &str) -> Result<(), NativeEditingV5Error> {
    require_string_at(value, &format!("/{key}"), expected)
}

fn require_string_at(
    value: &Value,
    pointer: &str,
    expected: &str,
) -> Result<(), NativeEditingV5Error> {
    if string_at(value, pointer)? != expected {
        return Err(invalid_plan(format!("{pointer} must equal {expected}")));
    }
    Ok(())
}

fn require_number_at(
    value: &Value,
    pointer: &str,
    expected: f64,
) -> Result<(), NativeEditingV5Error> {
    if number_at(value, pointer)? != expected {
        return Err(invalid_plan(format!("{pointer} must equal {expected}")));
    }
    Ok(())
}

fn require_bool_at(
    value: &Value,
    pointer: &str,
    expected: bool,
) -> Result<(), NativeEditingV5Error> {
    if value_at(value, pointer)?.as_bool() != Some(expected) {
        return Err(invalid_plan(format!("{pointer} must equal {expected}")));
    }
    Ok(())
}

fn canonical_zero(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

fn invalid_plan(message: impl Into<String>) -> NativeEditingV5Error {
    NativeEditingV5Error::InvalidPlan(message.into())
}

fn history_error(message: impl Into<String>) -> NativeEditingV5Error {
    NativeEditingV5Error::HistoryInvariant(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation_model::{
        Annotation, AnnotationCommand, RectangleAnnotation, StrokeStyle,
    };

    #[test]
    fn embedded_plan_binds_the_frozen_native_edit_commands() {
        let plan = NativeEditingV5Plan::embedded().expect("embedded v5 plan should parse");

        assert_eq!(plan.property_edit.original_stroke_width_pt, 1.5);
        assert_eq!(plan.property_edit.edited_stroke_width_pt, 4.0);
        assert_eq!(
            plan.property_edit.candidate_policy,
            PropertyCandidatePolicy {
                effective_history_revision_delta: 1,
                application_undo_count: 1,
                canonical_state_restored: true,
            }
        );
        assert_eq!(
            plan.property_edit
                .electron_baseline_policy
                .effective_history_revision_delta,
            2
        );
        assert_eq!(plan.property_edit.expected_milestones.len(), 7);
        assert_eq!(plan.snap_transform.sample_count, 361);
        assert_eq!(plan.snap_transform.rate_hz, 120);
        assert_eq!(plan.snap_transform.duration_ms, 3_000);
        assert_eq!(
            plan.snap_transform.expected_snapped_delta,
            Translation { x: 90.0, y: 90.0 }
        );
        assert_eq!(plan.snap_transform.grid_spacing_pt, 18.0);
        assert_eq!(plan.snap_transform.sensitivity_css_px, 8.0);
    }

    #[test]
    fn parser_rejects_changed_snap_semantics() {
        let mut value: Value = serde_json::from_str(EMBEDDED_V5_WORKLOAD).unwrap();
        let command = find_command_mut(&mut value, SNAP_OPERATION);
        command["snap"]["sensitivity"]["threshold_norm"] = Value::String("euclidean".into());

        let error = NativeEditingV5Plan::from_json(&serde_json::to_string(&value).unwrap())
            .expect_err("changed norm must fail");
        assert!(error.to_string().contains("threshold_norm"));
    }

    #[test]
    fn inclusive_l_infinity_snaps_the_exact_boundary() {
        let snap = InclusiveLInfGridSnap::from_css_pixels(18.0, 8.0, 8.0 / 7.0).unwrap();
        let resolution = snap.resolve(Translation::new(97.0, 83.0).unwrap()).unwrap();

        assert!(resolution.acquired);
        assert_eq!(resolution.candidate, Translation { x: 90.0, y: 90.0 });
        assert_eq!(resolution.correction, Translation { x: -7.0, y: 7.0 });
        assert_eq!(resolution.applied, resolution.candidate);

        let below_boundary = InclusiveLInfGridSnap::from_css_pixels(18.0, 8.0, 8.0 / 6.999)
            .unwrap()
            .resolve(Translation::new(97.0, 83.0).unwrap())
            .unwrap();
        assert!(!below_boundary.acquired);
        assert_eq!(below_boundary.applied, below_boundary.raw);
    }

    #[test]
    fn l_infinity_requires_both_axis_corrections_to_fit() {
        let resolution = InclusiveLInfGridSnap::from_css_pixels(18.0, 8.0, 8.0 / 6.0)
            .unwrap()
            .resolve(Translation::new(95.0, 83.0).unwrap())
            .unwrap();

        assert_eq!(resolution.correction, Translation { x: -5.0, y: 7.0 });
        assert!(!resolution.acquired);
        assert_eq!(resolution.applied, Translation { x: 95.0, y: 83.0 });
    }

    #[test]
    fn invalid_snap_configuration_is_rejected() {
        assert!(InclusiveLInfGridSnap::from_css_pixels(0.0, 8.0, 1.0).is_err());
        assert!(InclusiveLInfGridSnap::from_css_pixels(18.0, -0.1, 1.0).is_err());
        assert!(InclusiveLInfGridSnap::from_css_pixels(18.0, 8.0, 0.0).is_err());
        assert!(InclusiveLInfGridSnap::from_css_pixels(f64::NAN, 8.0, 1.0).is_err());
    }

    #[test]
    fn many_property_stages_commit_once_and_undo_redo_exactly() {
        let plan = NativeEditingV5Plan::embedded().unwrap().property_edit;
        let mut document = document_with_rectangle(
            plan.target_id.clone(),
            plan.setup_rect,
            plan.original_stroke_width_pt,
        );
        let history_before = document.history_depths();
        let mut edit = plan.begin_transaction(&document).unwrap();
        edit.stage_stroke_width(2.0).unwrap();
        edit.stage_stroke_width(3.0).unwrap();
        edit.stage_stroke_width(plan.edited_stroke_width_pt)
            .unwrap();
        assert_eq!(document.history_depths(), history_before);

        let receipt = edit.commit(&mut document).unwrap();
        assert_eq!(document.history_depths(), (history_before.0 + 1, 0));
        assert_eq!(receipt.after.stroke_width_pt(), 4.0);
        assert_eq!(
            find_rectangle(&document, &plan.target_id).unwrap().rect,
            plan.setup_rect
        );

        assert!(document.undo().unwrap());
        receipt.verify_undone(&document).unwrap();
        assert_eq!(
            find_rectangle(&document, &plan.target_id)
                .unwrap()
                .appearance
                .stroke_width_pt(),
            1.5
        );

        assert!(document.redo().unwrap());
        receipt.verify_redone(&document).unwrap();
        assert_eq!(
            find_rectangle(&document, &plan.target_id)
                .unwrap()
                .appearance
                .stroke_width_pt(),
            4.0
        );
    }

    #[test]
    fn property_noop_adds_no_history() {
        let plan = NativeEditingV5Plan::embedded().unwrap().property_edit;
        let mut document = document_with_rectangle(
            plan.target_id.clone(),
            plan.setup_rect,
            plan.original_stroke_width_pt,
        );
        let before = document.history_depths();
        let mut edit = StrokeWidthEditTransaction::begin(&document, &plan.target_id).unwrap();
        edit.stage_stroke_width(plan.original_stroke_width_pt)
            .unwrap();

        assert_eq!(
            edit.commit(&mut document),
            Err(NativeEditingV5Error::NoChange)
        );
        assert_eq!(document.history_depths(), before);
    }

    #[test]
    fn frozen_snap_updates_preview_361_times_and_commits_once() {
        let plan = NativeEditingV5Plan::embedded().unwrap().snap_transform;
        let mut document = document_with_rectangle(plan.target_id.clone(), plan.setup_rect, 1.5);
        let history_before = document.history_depths();
        let mut gesture = plan.begin_gesture(&mut document, 91, 1.0, 0.76).unwrap();

        for sample in 0..plan.sample_count {
            let t = sample as f64 / (plan.sample_count - 1) as f64;
            let point = PdfPoint::new(
                plan.start.x + (plan.unsnapped_end.x - plan.start.x) * t,
                plan.start.y + (plan.unsnapped_end.y - plan.start.y) * t,
            )
            .unwrap();
            gesture.update(&mut document, point).unwrap();
            assert_eq!(document.history_depths(), history_before);
        }
        assert_eq!(gesture.sample_count(), 361);
        let final_resolution = gesture.latest_resolution().unwrap();
        assert!(final_resolution.acquired);
        assert_eq!(final_resolution.correction, Translation { x: -7.0, y: 7.0 });

        let receipt = gesture.commit(&mut document).unwrap();
        assert_eq!(receipt.final_rect, plan.expected_final_rect);
        assert_eq!(receipt.sample_count, plan.sample_count);
        assert_eq!(receipt.sensitivity_css_px, 8.0);
        assert_eq!(receipt.observed_pixels_per_point, 0.76);
        assert_eq!(receipt.derived_threshold_pt, 8.0 / 0.76);
        assert_eq!(document.history_depths(), (history_before.0 + 1, 0));

        assert!(document.undo().unwrap());
        receipt.verify_undone(&document).unwrap();
        assert!(document.redo().unwrap());
        receipt.verify_redone(&document).unwrap();
    }

    #[test]
    fn rejected_frozen_snap_commit_clears_preview_without_history() {
        let plan = NativeEditingV5Plan::embedded().unwrap().snap_transform;
        let mut document = document_with_rectangle(plan.target_id.clone(), plan.setup_rect, 1.5);
        let history_before = document.history_depths();
        let mut gesture = plan.begin_gesture(&mut document, 92, 1.0, 0.76).unwrap();
        gesture.update(&mut document, plan.unsnapped_end).unwrap();

        let error = gesture
            .commit(&mut document)
            .expect_err("one sample must not satisfy the frozen 361-sample workload");
        assert!(error.to_string().contains("1 samples, expected 361"));
        assert!(document.active_preview().is_none());
        assert_eq!(document.history_depths(), history_before);
        assert_eq!(
            find_rectangle(&document, &plan.target_id).unwrap().rect,
            plan.setup_rect
        );
    }

    #[test]
    fn out_of_threshold_translation_commits_the_raw_delta_once() {
        let id = MarkupId::new("test:raw-snap").unwrap();
        let rect = PdfRect::new(10.0, 20.0, 30.0, 40.0).unwrap();
        let mut document = document_with_rectangle(id.clone(), rect, 1.0);
        let before = document.history_depths();
        let start = PdfPoint::new(20.0, 30.0).unwrap();
        let mut gesture = SnapTranslateGesture::begin(
            &mut document,
            &id,
            7,
            0,
            start,
            1.0,
            InclusiveLInfGridSnap::from_css_pixels(18.0, 8.0, 8.0).unwrap(),
        )
        .unwrap();
        let resolution = gesture
            .update(&mut document, PdfPoint::new(25.0, 37.0).unwrap())
            .unwrap();
        assert!(!resolution.acquired);

        let receipt = gesture.commit(&mut document).unwrap();
        assert_eq!(
            receipt.final_rect,
            PdfRect::new(15.0, 27.0, 30.0, 40.0).unwrap()
        );
        assert_eq!(document.history_depths(), (before.0 + 1, 0));
    }

    fn document_with_rectangle(
        id: MarkupId,
        rect: PdfRect,
        stroke_width_pt: f64,
    ) -> AnnotationDocument {
        let appearance =
            RectangleAppearance::new("#123456", stroke_width_pt, Some("#abcdef"), 0.75)
                .unwrap()
                .with_fill_opacity(0.25)
                .unwrap()
                .with_stroke_style(StrokeStyle::Dashed);
        let mut document = AnnotationDocument::default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id,
                    page_index: 0,
                    rect,
                    rotation_degrees: 0.0,
                    appearance,
                    locked: false,
                },
            )))
            .unwrap();
        document
    }

    fn find_command_mut<'a>(root: &'a mut Value, operation: &str) -> &'a mut Value {
        root.get_mut("journeys")
            .and_then(Value::as_array_mut)
            .unwrap()
            .iter_mut()
            .filter_map(|journey| journey.get_mut("commands").and_then(Value::as_array_mut))
            .flatten()
            .find(|command| command.get("operation").and_then(Value::as_str) == Some(operation))
            .unwrap()
    }
}
