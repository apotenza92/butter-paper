use std::{collections::BTreeMap, path::PathBuf};

use serde_json::json;

use crate::{
    document_workspace::{DocumentId, DocumentWorkspaceEvidenceSnapshot},
    perf_protocol::{PerfFields, fields},
};

#[derive(Clone, Debug, PartialEq)]
pub struct QualificationEvent {
    pub name: &'static str,
    pub fields: PerfFields,
}

const COMPAT_NATIVE_COMMAND_ID: &str = "viewer:open-each";
const COMPAT_COMPARISON_COMMAND_ID: &str = "small:open-settle";
const COMPAT_FIXTURE_ID: &str = "bp-single-page-v1";
const COMPAT_PAGE_ID: &str = "bp-single-page-v1:page:001";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSizePoints {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentedCropEvidence {
    pub command_id: String,
    pub comparison_command_id: String,
    pub fixture_id: String,
    pub page_id: String,
    pub page_size_points: PageSizePoints,
    pub painted_outer_page_bounds_window_logical: LogicalBounds,
    pub window_logical_size: LogicalSize,
    pub display_scale_factor: f64,
    pub rendered_device_pixel_ratio: f64,
    pub painted_request_generation: u64,
    pub painted_resource_generation: u64,
    pub painted_render_generation: u64,
    pub painted_state_sequence: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentedCropEvidenceInput {
    pub comparison_command_id: String,
    pub fixture_id: String,
    pub page_index: u32,
    pub page_size_points: PageSizePoints,
    pub painted_outer_page_bounds_window_logical: LogicalBounds,
    pub window_logical_size: LogicalSize,
    pub display_scale_factor: f64,
    pub rendered_device_pixel_ratio: f64,
    pub painted_request_generation: u64,
    pub painted_resource_generation: u64,
    pub painted_render_generation: u64,
    pub painted_state_sequence: u64,
}

pub fn map_presented_crop_evidence(
    input: PresentedCropEvidenceInput,
) -> Result<PresentedCropEvidence, PresentedCropError> {
    let page_number = input
        .page_index
        .checked_add(1)
        .ok_or(PresentedCropError::InvalidEvidence("page_index"))?;
    let evidence = PresentedCropEvidence {
        command_id: COMPAT_NATIVE_COMMAND_ID.into(),
        comparison_command_id: input.comparison_command_id,
        page_id: format!("{}:page:{page_number:03}", input.fixture_id),
        fixture_id: input.fixture_id,
        page_size_points: input.page_size_points,
        painted_outer_page_bounds_window_logical: input.painted_outer_page_bounds_window_logical,
        window_logical_size: input.window_logical_size,
        display_scale_factor: input.display_scale_factor,
        rendered_device_pixel_ratio: input.rendered_device_pixel_ratio,
        painted_request_generation: input.painted_request_generation,
        painted_resource_generation: input.painted_resource_generation,
        painted_render_generation: input.painted_render_generation,
        painted_state_sequence: input.painted_state_sequence,
    };
    evidence.validate()?;
    Ok(evidence)
}

impl PresentedCropEvidence {
    fn validate(&self) -> Result<(), PresentedCropError> {
        if self.command_id != COMPAT_NATIVE_COMMAND_ID {
            return Err(PresentedCropError::InvalidEvidence("command_id"));
        }
        if self.comparison_command_id != COMPAT_COMPARISON_COMMAND_ID {
            return Err(PresentedCropError::InvalidEvidence("comparison_command_id"));
        }
        if self.fixture_id != COMPAT_FIXTURE_ID {
            return Err(PresentedCropError::InvalidEvidence("fixture_id"));
        }
        if self.page_id != COMPAT_PAGE_ID {
            return Err(PresentedCropError::InvalidEvidence("page_id"));
        }
        if self.page_size_points
            != (PageSizePoints {
                width: 612.,
                height: 792.,
            })
        {
            return Err(PresentedCropError::InvalidEvidence("page_size_points"));
        }
        let bounds = self.painted_outer_page_bounds_window_logical;
        if !bounds.x.is_finite()
            || !bounds.y.is_finite()
            || !bounds.width.is_finite()
            || !bounds.height.is_finite()
            || bounds.x < 0.
            || bounds.y < 0.
            || bounds.width <= 0.
            || bounds.height <= 0.
        {
            return Err(PresentedCropError::InvalidEvidence(
                "painted_outer_page_bounds_window_logical",
            ));
        }
        let window = self.window_logical_size;
        if !window.width.is_finite()
            || !window.height.is_finite()
            || window.width <= 0.
            || window.height <= 0.
        {
            return Err(PresentedCropError::InvalidEvidence("window_logical_size"));
        }
        if bounds.x + bounds.width > window.width || bounds.y + bounds.height > window.height {
            return Err(PresentedCropError::InvalidEvidence(
                "painted_outer_page_bounds_window_logical",
            ));
        }
        if !self.display_scale_factor.is_finite() || self.display_scale_factor <= 0. {
            return Err(PresentedCropError::InvalidEvidence("display_scale_factor"));
        }
        if !self.rendered_device_pixel_ratio.is_finite() || self.rendered_device_pixel_ratio < 1. {
            return Err(PresentedCropError::InvalidEvidence(
                "rendered_device_pixel_ratio",
            ));
        }
        if self.painted_request_generation == 0 {
            return Err(PresentedCropError::InvalidEvidence(
                "painted_request_generation",
            ));
        }
        if self.painted_resource_generation == 0 {
            return Err(PresentedCropError::InvalidEvidence(
                "painted_resource_generation",
            ));
        }
        if self.painted_render_generation == 0 {
            return Err(PresentedCropError::InvalidEvidence(
                "painted_render_generation",
            ));
        }
        if self.painted_state_sequence == 0 {
            return Err(PresentedCropError::InvalidEvidence(
                "painted_state_sequence",
            ));
        }
        Ok(())
    }

    fn event(&self, name: &'static str) -> QualificationEvent {
        QualificationEvent {
            name,
            fields: fields([
                ("command_id", json!(self.command_id)),
                ("comparison_command_id", json!(self.comparison_command_id)),
                ("fixture_id", json!(self.fixture_id)),
                ("page_id", json!(self.page_id)),
                (
                    "page_size_points",
                    json!({
                        "width": self.page_size_points.width,
                        "height": self.page_size_points.height,
                    }),
                ),
                (
                    "painted_outer_page_bounds_window_logical",
                    json!({
                        "x": self.painted_outer_page_bounds_window_logical.x,
                        "y": self.painted_outer_page_bounds_window_logical.y,
                        "width": self.painted_outer_page_bounds_window_logical.width,
                        "height": self.painted_outer_page_bounds_window_logical.height,
                    }),
                ),
                (
                    "window_logical_size",
                    json!({
                        "width": self.window_logical_size.width,
                        "height": self.window_logical_size.height,
                    }),
                ),
                ("display_scale_factor", json!(self.display_scale_factor)),
                (
                    "rendered_device_pixel_ratio",
                    json!(self.rendered_device_pixel_ratio),
                ),
                (
                    "painted_request_generation",
                    json!(self.painted_request_generation),
                ),
                (
                    "painted_resource_generation",
                    json!(self.painted_resource_generation),
                ),
                (
                    "painted_render_generation",
                    json!(self.painted_render_generation),
                ),
                ("painted_state_sequence", json!(self.painted_state_sequence)),
            ]),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentedCropSignalDisposition {
    NoSignal,
    ScheduleNextFrame,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentedCropError {
    InvalidEvidence(&'static str),
    InvalidCropOpenEvent(&'static str),
    MissingQualifiedOpenEvent,
    MultipleQualifiedOpenEvents,
    ConflictingOpenField(String),
    InvariantViolation(&'static str),
    AlreadyArmed,
    NotArmed,
    DuplicateSignal,
    FrameNotRequested,
    PaintedEvidenceDrift,
    AlreadyConfirmed,
    PresentationNotConfirmed,
    CleanupAlreadyAuthorized,
}

#[derive(Clone, Debug, PartialEq)]
enum PresentedCropState {
    Unarmed,
    AwaitingSignal(PresentedCropEvidence),
    FramePending(PresentedCropEvidence),
    Confirmed(PresentedCropEvidence),
    CleanupAuthorized(PresentedCropEvidence),
}

pub struct PresentedCropHandshake {
    state: PresentedCropState,
}

impl PresentedCropHandshake {
    pub const fn new() -> Self {
        Self {
            state: PresentedCropState::Unarmed,
        }
    }

    pub fn arm(
        &mut self,
        evidence: PresentedCropEvidence,
    ) -> Result<QualificationEvent, PresentedCropError> {
        if self.state != PresentedCropState::Unarmed {
            return Err(PresentedCropError::AlreadyArmed);
        }
        evidence.validate()?;
        let event = evidence.event("viewer-native-open-evidence");
        self.state = PresentedCropState::AwaitingSignal(evidence);
        Ok(event)
    }

    pub fn observe_signal(
        &mut self,
        pending: bool,
    ) -> Result<PresentedCropSignalDisposition, PresentedCropError> {
        if !pending {
            return Ok(PresentedCropSignalDisposition::NoSignal);
        }
        match &self.state {
            PresentedCropState::Unarmed => Err(PresentedCropError::NotArmed),
            PresentedCropState::AwaitingSignal(evidence) => {
                self.state = PresentedCropState::FramePending(evidence.clone());
                Ok(PresentedCropSignalDisposition::ScheduleNextFrame)
            }
            PresentedCropState::FramePending(_) => Err(PresentedCropError::DuplicateSignal),
            PresentedCropState::Confirmed(_) | PresentedCropState::CleanupAuthorized(_) => {
                Err(PresentedCropError::AlreadyConfirmed)
            }
        }
    }

    pub fn confirm_next_frame(
        &mut self,
        evidence: &PresentedCropEvidence,
    ) -> Result<QualificationEvent, PresentedCropError> {
        let expected = match &self.state {
            PresentedCropState::Unarmed => return Err(PresentedCropError::NotArmed),
            PresentedCropState::AwaitingSignal(_) => {
                return Err(PresentedCropError::FrameNotRequested);
            }
            PresentedCropState::FramePending(expected) => expected.clone(),
            PresentedCropState::Confirmed(_) | PresentedCropState::CleanupAuthorized(_) => {
                return Err(PresentedCropError::AlreadyConfirmed);
            }
        };
        if evidence != &expected {
            return Err(PresentedCropError::PaintedEvidenceDrift);
        }
        let event = expected.event("viewer-native-presented-state");
        self.state = PresentedCropState::Confirmed(expected);
        Ok(event)
    }

    pub fn authorize_cleanup(&mut self) -> Result<(), PresentedCropError> {
        match &self.state {
            PresentedCropState::Confirmed(evidence) => {
                self.state = PresentedCropState::CleanupAuthorized(evidence.clone());
                Ok(())
            }
            PresentedCropState::CleanupAuthorized(_) => {
                Err(PresentedCropError::CleanupAlreadyAuthorized)
            }
            PresentedCropState::Unarmed
            | PresentedCropState::AwaitingSignal(_)
            | PresentedCropState::FramePending(_) => {
                Err(PresentedCropError::PresentationNotConfirmed)
            }
        }
    }

    pub const fn is_cleanup_authorized(&self) -> bool {
        matches!(self.state, PresentedCropState::CleanupAuthorized(_))
    }
}

impl Default for PresentedCropHandshake {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureCleanupReason {
    QualifiedSuccess,
    Failure,
}

pub struct CaptureOrchestrationCoordinator {
    handshake: PresentedCropHandshake,
    next_frame_pending: bool,
    next_frame_scheduled: bool,
    cleanup_reason: Option<CaptureCleanupReason>,
    cleanup_complete: bool,
}

impl CaptureOrchestrationCoordinator {
    pub const fn new() -> Self {
        Self {
            handshake: PresentedCropHandshake::new(),
            next_frame_pending: false,
            next_frame_scheduled: false,
            cleanup_reason: None,
            cleanup_complete: false,
        }
    }

    pub fn arm(
        &mut self,
        evidence: PresentedCropEvidence,
    ) -> Result<QualificationEvent, PresentedCropError> {
        self.handshake.arm(evidence)
    }

    pub fn observe_signal(
        &mut self,
        pending: bool,
    ) -> Result<PresentedCropSignalDisposition, PresentedCropError> {
        let disposition = self.handshake.observe_signal(pending)?;
        if disposition == PresentedCropSignalDisposition::ScheduleNextFrame {
            self.next_frame_pending = true;
        }
        Ok(disposition)
    }

    pub fn take_next_frame_request(&mut self) -> bool {
        if self.next_frame_pending && !self.next_frame_scheduled {
            self.next_frame_scheduled = true;
            true
        } else {
            false
        }
    }

    pub fn confirm_next_frame(
        &mut self,
        evidence: &PresentedCropEvidence,
    ) -> Result<QualificationEvent, PresentedCropError> {
        let event = self.handshake.confirm_next_frame(evidence)?;
        self.next_frame_pending = false;
        self.next_frame_scheduled = false;
        Ok(event)
    }

    pub fn authorize_success_cleanup(&mut self) -> Result<(), PresentedCropError> {
        self.handshake.authorize_cleanup()?;
        self.cleanup_reason = Some(CaptureCleanupReason::QualifiedSuccess);
        Ok(())
    }

    pub fn authorize_uncaptured_success_cleanup(&mut self) -> Result<(), PresentedCropError> {
        if self.cleanup_reason.is_some() {
            return Err(PresentedCropError::CleanupAlreadyAuthorized);
        }
        self.cleanup_reason = Some(CaptureCleanupReason::QualifiedSuccess);
        Ok(())
    }

    pub fn begin_failure_cleanup(&mut self) {
        self.next_frame_pending = false;
        self.next_frame_scheduled = false;
        self.cleanup_reason = Some(CaptureCleanupReason::Failure);
    }

    pub const fn cleanup_reason(&self) -> Option<CaptureCleanupReason> {
        self.cleanup_reason
    }

    pub fn complete_resource_cleanup(
        &mut self,
    ) -> Result<CaptureCleanupReason, PresentedCropError> {
        let reason = self
            .cleanup_reason
            .ok_or(PresentedCropError::PresentationNotConfirmed)?;
        if self.cleanup_complete {
            return Err(PresentedCropError::CleanupAlreadyAuthorized);
        }
        self.cleanup_complete = true;
        Ok(reason)
    }

    pub const fn success_qualification_allowed(&self) -> bool {
        matches!(
            self.cleanup_reason,
            Some(CaptureCleanupReason::QualifiedSuccess)
        )
    }
}

impl Default for CaptureOrchestrationCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

fn required_string_field<'a>(
    event: &'a QualificationEvent,
    name: &'static str,
) -> Result<&'a str, PresentedCropError> {
    event
        .fields
        .get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or(PresentedCropError::InvariantViolation(name))
}

fn required_u64_field(
    event: &QualificationEvent,
    name: &'static str,
) -> Result<u64, PresentedCropError> {
    event
        .fields
        .get(name)
        .and_then(serde_json::Value::as_u64)
        .ok_or(PresentedCropError::InvariantViolation(name))
}

pub fn merge_presented_crop_open_events(
    mut qualified_events: Vec<QualificationEvent>,
    crop_open: QualificationEvent,
) -> Result<Vec<QualificationEvent>, PresentedCropError> {
    if crop_open.name != "viewer-native-open-evidence" {
        return Err(PresentedCropError::InvalidCropOpenEvent(crop_open.name));
    }
    let open_indices = qualified_events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| (event.name == "viewer-native-open-evidence").then_some(index))
        .collect::<Vec<_>>();
    let open_index = match open_indices.as_slice() {
        [] => return Err(PresentedCropError::MissingQualifiedOpenEvent),
        [index] => *index,
        _ => return Err(PresentedCropError::MultipleQualifiedOpenEvents),
    };
    let comparison_completion = qualified_events
        .iter()
        .filter(|event| event.name == "comparison-command-complete")
        .collect::<Vec<_>>();
    if comparison_completion.len() != 1
        || required_string_field(comparison_completion[0], "command_id")?
            != COMPAT_COMPARISON_COMMAND_ID
    {
        return Err(PresentedCropError::InvariantViolation(
            "comparison-command-complete.command_id",
        ));
    }
    let qualified_open = &qualified_events[open_index];
    if required_string_field(qualified_open, "command_id")? != COMPAT_NATIVE_COMMAND_ID
        || required_string_field(&crop_open, "command_id")? != COMPAT_NATIVE_COMMAND_ID
    {
        return Err(PresentedCropError::InvariantViolation("command_id"));
    }
    if required_string_field(qualified_open, "comparison_command_id")?
        != COMPAT_COMPARISON_COMMAND_ID
        || required_string_field(&crop_open, "comparison_command_id")?
            != COMPAT_COMPARISON_COMMAND_ID
    {
        return Err(PresentedCropError::InvariantViolation(
            "comparison_command_id",
        ));
    }
    let requested_open_generation =
        required_u64_field(qualified_open, "requested_open_generation")?;
    let completed_open_generation =
        required_u64_field(qualified_open, "completed_open_generation")?;
    if requested_open_generation != completed_open_generation {
        return Err(PresentedCropError::InvariantViolation(
            "requested_open_generation",
        ));
    }
    if required_u64_field(&crop_open, "painted_request_generation")? != completed_open_generation {
        return Err(PresentedCropError::InvariantViolation(
            "painted_request_generation",
        ));
    }
    if required_u64_field(&crop_open, "painted_render_generation")?
        != required_u64_field(qualified_open, "preview_generation")?
    {
        return Err(PresentedCropError::InvariantViolation(
            "painted_render_generation",
        ));
    }
    let qualified_open = &mut qualified_events[open_index];
    for (name, value) in crop_open.fields {
        if let Some(existing) = qualified_open.fields.get(&name) {
            if existing != &value {
                return Err(PresentedCropError::ConflictingOpenField(name));
            }
        } else {
            qualified_open.fields.insert(name, value);
        }
    }
    Ok(qualified_events)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerfRunConfig {
    pub scenario: String,
    pub iteration: u32,
    pub input_lane: String,
    pub compat_profile: String,
    pub v4_manifest_id: String,
    pub fixture_ids: Vec<String>,
    pub command_id: String,
    pub pdfs: Vec<PathBuf>,
    pub cache_directory: PathBuf,
    pub worker_executable: PathBuf,
    pub pdfium_library: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PerfRunConfigError {
    Missing(&'static str),
    InvalidIteration,
    InvalidFixtureIds,
    UnsupportedScenario(String),
    UnsupportedInputLane(String),
    UnsupportedCompatProfile(String),
    UnsupportedV4Manifest(String),
    UnsupportedFixture(String),
    ExpectedOnePdf,
}

impl PerfRunConfig {
    pub fn requires_presented_crop_signal(&self) -> bool {
        self.input_lane == "native-x11-xtest"
            && self.compat_profile == "longbridge-gpui-component-v1"
    }

    pub fn parse(
        environment: BTreeMap<String, String>,
        pdfs: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Self, PerfRunConfigError> {
        let required = |name: &'static str| {
            environment
                .get(name)
                .filter(|value| !value.is_empty())
                .cloned()
                .ok_or(PerfRunConfigError::Missing(name))
        };
        let scenario = required("BP_GPUI_PERF_SCENARIO")?;
        if scenario != "open-pdf" {
            return Err(PerfRunConfigError::UnsupportedScenario(scenario));
        }
        let iteration = required("BP_GPUI_PERF_ITERATION")?
            .parse::<u32>()
            .ok()
            .filter(|iteration| *iteration > 0)
            .ok_or(PerfRunConfigError::InvalidIteration)?;
        let input_lane = required("BP_GPUI_INPUT_LANE")?;
        if input_lane != "native-x11-xtest" && input_lane != "semantic-diagnostic" {
            return Err(PerfRunConfigError::UnsupportedInputLane(input_lane));
        }
        let compat_profile = required("BP_GPUI_COMPAT_PROFILE")?;
        if compat_profile != "longbridge-gpui-component-v1" {
            return Err(PerfRunConfigError::UnsupportedCompatProfile(compat_profile));
        }
        let v4_manifest_id = required("BP_GPUI_V4_MANIFEST_ID")?;
        if v4_manifest_id != "bp-perf-v4-decision-1" {
            return Err(PerfRunConfigError::UnsupportedV4Manifest(v4_manifest_id));
        }
        let fixture_ids = serde_json::from_str::<Vec<String>>(&required("BP_GPUI_FIXTURE_IDS")?)
            .map_err(|_| PerfRunConfigError::InvalidFixtureIds)?;
        if fixture_ids.len() != 1 {
            return Err(PerfRunConfigError::InvalidFixtureIds);
        }
        let command_id = match fixture_ids[0].as_str() {
            "bp-single-page-v1" => "small:open-settle".to_owned(),
            fixture => return Err(PerfRunConfigError::UnsupportedFixture(fixture.to_owned())),
        };
        let pdfs = pdfs.into_iter().collect::<Vec<_>>();
        if pdfs.len() != 1 {
            return Err(PerfRunConfigError::ExpectedOnePdf);
        }
        Ok(Self {
            scenario,
            iteration,
            input_lane,
            compat_profile,
            v4_manifest_id,
            fixture_ids,
            command_id,
            pdfs,
            cache_directory: PathBuf::from(required("BP_GPUI_CACHE_DIR")?),
            worker_executable: PathBuf::from(required("BP_PDF_WORKER_EXE")?),
            pdfium_library: PathBuf::from(required("BP_PDFIUM_LIBRARY")?),
        })
    }

    pub fn from_process() -> Result<Option<Self>, PerfRunConfigError> {
        let Ok(scenario) = std::env::var("BP_GPUI_PERF_SCENARIO") else {
            return Ok(None);
        };
        let names = [
            "BP_GPUI_PERF_ITERATION",
            "BP_GPUI_INPUT_LANE",
            "BP_GPUI_COMPAT_PROFILE",
            "BP_GPUI_V4_MANIFEST_ID",
            "BP_GPUI_FIXTURE_IDS",
            "BP_GPUI_CACHE_DIR",
            "BP_PDF_WORKER_EXE",
            "BP_PDFIUM_LIBRARY",
        ];
        let mut environment = BTreeMap::from([("BP_GPUI_PERF_SCENARIO".to_owned(), scenario)]);
        for name in names {
            if let Ok(value) = std::env::var(name) {
                environment.insert(name.to_owned(), value);
            }
        }
        let pdfs = std::env::args_os()
            .skip(1)
            .filter(|argument| {
                let argument = argument.to_string_lossy();
                argument != "-ApplePersistenceIgnoreState" && argument != "YES"
            })
            .map(PathBuf::from);
        Self::parse(environment, pdfs).map(Some)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QualificationError {
    AlreadyComplete,
    WrongDocument {
        expected: DocumentId,
        actual: DocumentId,
    },
    StaleGeneration {
        expected: u64,
        actual: u64,
    },
    DocumentNotReady,
    MissingWorker,
    MissingRealPixels,
    NotSettled,
    PresentedGenerationChanged {
        expected: u64,
        actual: u64,
    },
    AlreadyPresented,
    CleanupIncomplete,
}

pub struct OpenPdfQualification {
    document_id: DocumentId,
    request_generation: u64,
    command_id: String,
    stable_evidence: Option<(StableOpenEvidence, f64)>,
    settled_evidence: Option<StableOpenEvidence>,
    settled_current_generation_ms: Option<f64>,
    opened_emitted: bool,
    presented: bool,
    complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StableOpenEvidence {
    viewer_generation: u64,
    current_page: u32,
    raster_width: u32,
    raster_height: u32,
    raster_bytes: usize,
    rendered_device_pixel_ratio: f32,
    worker_pid: u32,
}

impl OpenPdfQualification {
    pub fn new(
        document_id: DocumentId,
        request_generation: u64,
        command_id: impl Into<String>,
    ) -> Self {
        Self {
            document_id,
            request_generation,
            command_id: command_id.into(),
            stable_evidence: None,
            settled_evidence: None,
            settled_current_generation_ms: None,
            opened_emitted: false,
            presented: false,
            complete: false,
        }
    }

    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    fn validate_snapshot_authority(
        &self,
        snapshot: &DocumentWorkspaceEvidenceSnapshot,
    ) -> Result<StableOpenEvidence, QualificationError> {
        if snapshot.document_id != self.document_id {
            return Err(QualificationError::WrongDocument {
                expected: self.document_id,
                actual: snapshot.document_id,
            });
        }
        if snapshot.request_generation != self.request_generation {
            return Err(QualificationError::StaleGeneration {
                expected: self.request_generation,
                actual: snapshot.request_generation,
            });
        }
        if !snapshot.ready {
            return Err(QualificationError::DocumentNotReady);
        }
        if !snapshot.resource_present || snapshot.worker_pid.is_none() {
            return Err(QualificationError::MissingWorker);
        }
        if snapshot.current_raster_width == 0
            || snapshot.current_raster_height == 0
            || snapshot.current_raster_bytes == 0
            || !snapshot.current_raster_has_spatial_variation
        {
            return Err(QualificationError::MissingRealPixels);
        }
        let rendered_device_pixel_ratio = snapshot
            .rendered_device_pixel_ratio
            .filter(|ratio| ratio.is_finite() && *ratio >= 1.)
            .ok_or(QualificationError::DocumentNotReady)?;
        Ok(StableOpenEvidence {
            viewer_generation: snapshot.viewer_generation,
            current_page: snapshot.current_page,
            raster_width: snapshot.current_raster_width,
            raster_height: snapshot.current_raster_height,
            raster_bytes: snapshot.current_raster_bytes,
            rendered_device_pixel_ratio,
            worker_pid: snapshot
                .worker_pid
                .expect("worker presence was checked above"),
        })
    }

    pub fn observe(
        &mut self,
        now_ms: f64,
        snapshot: &DocumentWorkspaceEvidenceSnapshot,
    ) -> Result<Vec<QualificationEvent>, QualificationError> {
        if self.complete {
            return Err(QualificationError::AlreadyComplete);
        }
        let current_evidence = self.validate_snapshot_authority(snapshot)?;
        let stable_since_ms = match self.stable_evidence {
            Some((evidence, since_ms)) if evidence == current_evidence => since_ms,
            _ => {
                self.stable_evidence = Some((current_evidence, now_ms));
                now_ms
            }
        };
        let mut events = Vec::new();
        if !self.opened_emitted {
            self.opened_emitted = true;
            events.push(QualificationEvent {
                name: "pdf-open-completed",
                fields: fields([
                    ("document_id", json!(snapshot.document_id.value())),
                    ("pages", json!(snapshot.page_count)),
                    ("worker_pid", json!(snapshot.worker_pid)),
                ]),
            });
            events.push(QualificationEvent {
                name: "viewport-raster-completed",
                fields: fields([
                    ("document_id", json!(snapshot.document_id.value())),
                    ("generation", json!(snapshot.viewer_generation)),
                    ("page", json!(snapshot.current_page)),
                    ("surface_kind", json!("in-memory-bgra")),
                    ("pixel_width", json!(snapshot.current_raster_width)),
                    ("pixel_height", json!(snapshot.current_raster_height)),
                    ("pixel_bytes", json!(snapshot.current_raster_bytes)),
                    (
                        "rendered_device_pixel_ratio",
                        json!(current_evidence.rendered_device_pixel_ratio),
                    ),
                ]),
            });
        }

        let settled_ms = now_ms - stable_since_ms;
        if settled_ms + f64::EPSILON >= 250. && self.settled_evidence.is_none() {
            self.settled_evidence = Some(current_evidence);
            self.settled_current_generation_ms = Some(settled_ms);
            events.push(QualificationEvent {
                name: "viewer-generation-settled",
                fields: fields([
                    ("command_id", json!(self.command_id)),
                    ("settled_current_generation_ms", json!(settled_ms)),
                    ("generation", json!(snapshot.viewer_generation)),
                ]),
            });
        }
        Ok(events)
    }

    pub fn confirm_presented(
        &mut self,
        now_ms: f64,
        snapshot: &DocumentWorkspaceEvidenceSnapshot,
    ) -> Result<Vec<QualificationEvent>, QualificationError> {
        if self.complete {
            return Err(QualificationError::AlreadyComplete);
        }
        if self.presented {
            return Err(QualificationError::AlreadyPresented);
        }
        let settled = self
            .settled_evidence
            .ok_or(QualificationError::NotSettled)?;
        let settled_current_generation_ms = self
            .settled_current_generation_ms
            .ok_or(QualificationError::NotSettled)?;
        let current = self.validate_snapshot_authority(snapshot)?;
        if current.viewer_generation != settled.viewer_generation {
            return Err(QualificationError::PresentedGenerationChanged {
                expected: settled.viewer_generation,
                actual: current.viewer_generation,
            });
        }
        if current != settled {
            return Err(QualificationError::DocumentNotReady);
        }
        self.presented = true;
        Ok(vec![
            QualificationEvent {
                name: "viewer-native-open-evidence",
                fields: fields([
                    ("command_id", json!("viewer:open-each")),
                    ("comparison_command_id", json!(self.command_id)),
                    ("document_opened", json!(true)),
                    ("preview_current_generation", json!(true)),
                    ("gpui_next_frame_observed", json!(true)),
                    ("presented_at_ms", json!(now_ms)),
                    ("requested_open_generation", json!(self.request_generation)),
                    (
                        "completed_open_generation",
                        json!(snapshot.request_generation),
                    ),
                    ("preview_generation", json!(snapshot.viewer_generation)),
                    (
                        "settled_current_generation_ms",
                        json!(settled_current_generation_ms),
                    ),
                ]),
            },
            QualificationEvent {
                name: "comparison-command-complete",
                fields: fields([("command_id", json!(self.command_id))]),
            },
        ])
    }

    pub fn confirm_cleanup(
        &mut self,
        worker_exited: bool,
        surfaces_released: bool,
    ) -> Result<Vec<QualificationEvent>, QualificationError> {
        if self.complete {
            return Err(QualificationError::AlreadyComplete);
        }
        if !self.presented {
            return Err(QualificationError::NotSettled);
        }
        if !worker_exited || !surfaces_released {
            return Err(QualificationError::CleanupIncomplete);
        }
        self.complete = true;
        Ok(vec![
            QualificationEvent {
                name: "resource-cleanup-complete",
                fields: fields([
                    ("worker_exited", json!(true)),
                    ("mapped_surfaces_released", json!(true)),
                ]),
            },
            QualificationEvent {
                name: "scenario-complete",
                fields: PerfFields::new(),
            },
        ])
    }
}
