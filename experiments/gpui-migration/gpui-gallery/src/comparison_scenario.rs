//! Deterministic, manifest-backed GPUI comparison scenario plans.
//!
//! This module is the boundary between the checked-in comparison workload and
//! the native app driver. It deliberately exposes only the two implemented
//! development subsets. A scenario cannot declare a milestone that the
//! workload does not assign to that exact command.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use serde_json::Value;

const WORKLOAD_JSON: &str = include_str!("../../performance/comparison-workload.json");
const EXPECTED_SCHEMA_VERSION: &str = "bp-comparison-workload-v1";
const EXPECTED_MANIFEST_ID: &str = "bp-perf-v3-decision-3";
pub const V4_VISIBLE_RASTER_READINESS_MILESTONE: &str = "visible-raster-readiness-observed";
const V3_BLANK_FRAME_MILESTONE: &str = "blank-current-generation-frames-zero";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonScenarioKind {
    ViewerLayout,
    PageNavigation,
    Zoom,
    HighZoomPan,
    CachePressure,
    CloseReopen,
    AnnotationCreate,
    AnnotationTransform,
    AnnotationPropertiesHistory,
    EditorCreate,
    ContinuousScroll,
}

impl ComparisonScenarioKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ViewerLayout => "viewer-layout",
            Self::PageNavigation => "page-navigation",
            Self::Zoom => "zoom",
            Self::HighZoomPan => "high-zoom-pan",
            Self::CachePressure => "cache-pressure",
            Self::CloseReopen => "close-reopen",
            Self::AnnotationCreate => "annotation-create",
            Self::AnnotationTransform => "annotation-transform",
            Self::AnnotationPropertiesHistory => "annotation-properties-history",
            Self::EditorCreate => "editor-create",
            Self::ContinuousScroll => "continuous-scroll",
        }
    }
}

impl FromStr for ComparisonScenarioKind {
    type Err = ComparisonScenarioError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "viewer-layout" => Ok(Self::ViewerLayout),
            "page-navigation" => Ok(Self::PageNavigation),
            "zoom" => Ok(Self::Zoom),
            "high-zoom-pan" => Ok(Self::HighZoomPan),
            "cache-pressure" => Ok(Self::CachePressure),
            "close-reopen" => Ok(Self::CloseReopen),
            "annotation-create" => Ok(Self::AnnotationCreate),
            "annotation-transform" => Ok(Self::AnnotationTransform),
            "annotation-properties-history" => Ok(Self::AnnotationPropertiesHistory),
            "editor-create" => Ok(Self::EditorCreate),
            "continuous-scroll" => Ok(Self::ContinuousScroll),
            other => Err(ComparisonScenarioError::Invalid(format!(
                "unsupported comparison scenario {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinearPointerCommand {
    pub command_id: String,
    pub annotation_id: String,
    pub sample_count: usize,
    pub rate_hz: u32,
    pub duration_ms: u64,
    pub start: [f64; 2],
    pub finish: [f64; 2],
    pub expected_milestones: Vec<String>,
}

impl LinearPointerCommand {
    pub fn samples(&self) -> Vec<[f64; 2]> {
        inclusive_linear_samples(self.start, self.finish, self.sample_count)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatmullRomPointerCommand {
    pub command_id: String,
    pub annotation_id: String,
    pub sample_count: usize,
    pub rate_hz: u32,
    pub duration_ms: u64,
    pub control_points: Vec<[f64; 2]>,
    pub expected_milestones: Vec<String>,
}

impl CatmullRomPointerCommand {
    pub fn samples(&self) -> Vec<[f64; 2]> {
        inclusive_catmull_rom_samples(&self.control_points, self.sample_count)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HighlightGeometryComparison {
    pub matched: bool,
    pub expected_input_point_count: usize,
    pub observed_model_point_count: usize,
    pub canonical_resample_count: usize,
    pub maximum_centerline_deviation_pdf_points: f64,
    pub tolerance_pdf_points: f64,
    pub smoothing_tolerance_pdf_points: f64,
    pub coordinate_quantization_allowance_pdf_points: f64,
    pub contract_version: &'static str,
    pub canonicalization: &'static str,
}

/// Compares a committed highlight to the maintained native benchmark oracle.
///
/// This intentionally mirrors `compareNativeBenchmarkGeometry` in the shared
/// performance harness: both paths are resampled to 64 points by arc length,
/// then the maximum centerline deviation must fit the maintained two-native-
/// pixel smoothing budget plus the unavoidable half-pixel-per-axis diagonal
/// introduced when XTEST rounds both coordinates to the native pixel grid.
pub fn compare_highlight_geometry(
    expected_samples: &[[f64; 2]],
    observed_points: &[[f64; 2]],
    pixels_per_point: f64,
) -> Result<HighlightGeometryComparison, ComparisonScenarioError> {
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return Err(ComparisonScenarioError::Invalid(
            "highlight geometry comparison requires a verified positive PDF-to-pixel scale".into(),
        ));
    }
    const RESAMPLE_COUNT: usize = 64;
    let expected = resample_polyline(expected_samples, RESAMPLE_COUNT)?;
    let observed = resample_polyline(observed_points, RESAMPLE_COUNT)?;
    let maximum_deviation = expected
        .iter()
        .zip(observed.iter())
        .map(|(expected, observed)| (expected[0] - observed[0]).hypot(expected[1] - observed[1]))
        .fold(0.0_f64, f64::max);
    let smoothing_tolerance = 2.0 / pixels_per_point;
    let coordinate_quantization_allowance = std::f64::consts::FRAC_1_SQRT_2 / pixels_per_point;
    let tolerance = smoothing_tolerance + coordinate_quantization_allowance;
    Ok(HighlightGeometryComparison {
        matched: maximum_deviation <= tolerance,
        expected_input_point_count: expected_samples.len(),
        observed_model_point_count: observed_points.len(),
        canonical_resample_count: RESAMPLE_COUNT,
        maximum_centerline_deviation_pdf_points: maximum_deviation,
        tolerance_pdf_points: tolerance,
        smoothing_tolerance_pdf_points: smoothing_tolerance,
        coordinate_quantization_allowance_pdf_points: coordinate_quantization_allowance,
        contract_version: "bp-native-ui-geometry-v1",
        canonicalization: "arc-length 64-point centerline; two native pixels after maintained smoothing plus one half-pixel-per-axis XTEST quantization diagonal",
    })
}

fn resample_polyline(
    points: &[[f64; 2]],
    count: usize,
) -> Result<Vec<[f64; 2]>, ComparisonScenarioError> {
    if points.len() < 2
        || points
            .iter()
            .flatten()
            .any(|coordinate| !coordinate.is_finite())
    {
        return Err(ComparisonScenarioError::Invalid(
            "highlight geometry comparison requires at least two finite points".into(),
        ));
    }
    let mut cumulative = Vec::with_capacity(points.len());
    cumulative.push(0.0);
    for pair in points.windows(2) {
        let segment = (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
        cumulative.push(cumulative.last().copied().unwrap_or_default() + segment);
    }
    let total = cumulative.last().copied().unwrap_or_default();
    if total <= 0.0 {
        return Err(ComparisonScenarioError::Invalid(
            "highlight geometry comparison requires a positive-length path".into(),
        ));
    }
    let mut result = Vec::with_capacity(count);
    for sample_index in 0..count {
        let distance = total * sample_index as f64 / (count - 1) as f64;
        let mut point_index = 1;
        while point_index < cumulative.len() - 1 && cumulative[point_index] < distance {
            point_index += 1;
        }
        let start = points[point_index - 1];
        let finish = points[point_index];
        let span = cumulative[point_index] - cumulative[point_index - 1];
        let fraction = if span == 0.0 {
            0.0
        } else {
            (distance - cumulative[point_index - 1]) / span
        };
        result.push([
            start[0] + (finish[0] - start[0]) * fraction,
            start[1] + (finish[1] - start[1]) * fraction,
        ]);
    }
    Ok(result)
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationCreatePlan {
    pub rectangle: LinearPointerCommand,
    pub highlight: CatmullRomPointerCommand,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContinuousScrollPlan {
    pub command_id: String,
    pub input_rate_hz: u32,
    pub forward_duration_ms: u64,
    pub forward_viewport_heights: f64,
    pub pause_duration_ms: u64,
    pub reverse_duration_ms: u64,
    pub finish_page: usize,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutCommand {
    pub command_id: String,
    pub layout: String,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewerLayoutPlan {
    pub single: LayoutCommand,
    pub continuous: LayoutCommand,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageNavigationPlan {
    pub command_id: String,
    pub page_formula: Vec<String>,
    pub deduplicate_after_clamp: bool,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ZoomPlan {
    pub command_id: String,
    pub percent: Vec<u32>,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HighZoomPanPlan {
    pub command_id: String,
    pub zoom_percent: u32,
    pub duration_ms: u64,
    pub rate_hz: u32,
    pub normalized_viewport_points: Vec<[f64; 2]>,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CachePressurePlan {
    pub command_id: String,
    pub cycles: usize,
    pub sequence: Vec<String>,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CloseReopenPlan {
    pub command_id: String,
    pub reopen_cache_class: String,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExactCommandPlan {
    pub command_id: String,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum ScenarioPayload {
    ViewerLayout(ViewerLayoutPlan),
    PageNavigation(PageNavigationPlan),
    Zoom(ZoomPlan),
    HighZoomPan(HighZoomPanPlan),
    CachePressure(CachePressurePlan),
    CloseReopen(CloseReopenPlan),
    AnnotationCreate(AnnotationCreatePlan),
    AnnotationTransform(Vec<ExactCommandPlan>),
    AnnotationPropertiesHistory(Vec<ExactCommandPlan>),
    EditorCreate(Vec<ExactCommandPlan>),
    ContinuousScroll(ContinuousScrollPlan),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonScenarioPlan {
    manifest_id: String,
    payload: ScenarioPayload,
}

impl ComparisonScenarioPlan {
    pub fn embedded(kind: ComparisonScenarioKind) -> Result<Self, ComparisonScenarioError> {
        Self::from_json(kind, WORKLOAD_JSON)
    }

    pub fn from_json(
        kind: ComparisonScenarioKind,
        workload_json: &str,
    ) -> Result<Self, ComparisonScenarioError> {
        let workload: Value = serde_json::from_str(workload_json)
            .map_err(|error| ComparisonScenarioError::Invalid(error.to_string()))?;
        require_string(&workload, "schema_version", EXPECTED_SCHEMA_VERSION)?;
        require_string(&workload, "manifest_id", EXPECTED_MANIFEST_ID)?;
        require_string(&workload, "coordinate_space", "pdf-points-bottom-left")?;
        let manifest_id = EXPECTED_MANIFEST_ID.to_string();
        let payload = match kind {
            ComparisonScenarioKind::ViewerLayout => {
                ScenarioPayload::ViewerLayout(parse_viewer_layout(&workload)?)
            }
            ComparisonScenarioKind::PageNavigation => {
                ScenarioPayload::PageNavigation(parse_page_navigation(&workload)?)
            }
            ComparisonScenarioKind::Zoom => ScenarioPayload::Zoom(parse_zoom(&workload)?),
            ComparisonScenarioKind::HighZoomPan => {
                ScenarioPayload::HighZoomPan(parse_high_zoom_pan(&workload)?)
            }
            ComparisonScenarioKind::CachePressure => {
                ScenarioPayload::CachePressure(parse_cache_pressure(&workload)?)
            }
            ComparisonScenarioKind::CloseReopen => {
                ScenarioPayload::CloseReopen(parse_close_reopen(&workload)?)
            }
            ComparisonScenarioKind::AnnotationCreate => {
                ScenarioPayload::AnnotationCreate(parse_annotation_create(&workload)?)
            }
            ComparisonScenarioKind::AnnotationTransform => {
                ScenarioPayload::AnnotationTransform(parse_exact_commands(
                    &workload,
                    &[("rectangle-v1", "rectangle:select-move-resize")],
                )?)
            }
            ComparisonScenarioKind::AnnotationPropertiesHistory => {
                ScenarioPayload::AnnotationPropertiesHistory(parse_exact_commands(
                    &workload,
                    &[("rectangle-v1", "rectangle:properties-history")],
                )?)
            }
            ComparisonScenarioKind::EditorCreate => {
                ScenarioPayload::EditorCreate(parse_exact_commands(
                    &workload,
                    &[
                        ("text-v1", "text:create"),
                        ("length-v1", "length:set-scale"),
                        ("length-v1", "length:create"),
                        ("image-v1", "image:create"),
                    ],
                )?)
            }
            ComparisonScenarioKind::ContinuousScroll => {
                ScenarioPayload::ContinuousScroll(parse_continuous_scroll(&workload)?)
            }
        };
        Ok(Self {
            manifest_id,
            payload,
        })
    }

    pub fn manifest_id(&self) -> &str {
        &self.manifest_id
    }

    pub fn annotation_create(&self) -> Option<&AnnotationCreatePlan> {
        match &self.payload {
            ScenarioPayload::AnnotationCreate(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn continuous_scroll(&self) -> Option<&ContinuousScrollPlan> {
        match &self.payload {
            ScenarioPayload::ContinuousScroll(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn use_v4_continuous_raster_readiness(&mut self) -> Result<(), ComparisonScenarioError> {
        let ScenarioPayload::ContinuousScroll(plan) = &mut self.payload else {
            return Err(ComparisonScenarioError::Invalid(
                "v4 raster readiness override requires continuous scroll".into(),
            ));
        };
        let Some(index) = plan
            .expected_milestones
            .iter()
            .position(|milestone| milestone == V3_BLANK_FRAME_MILESTONE)
        else {
            return Err(ComparisonScenarioError::Invalid(
                "continuous scroll no longer contains the frozen v3 blank-frame milestone".into(),
            ));
        };
        plan.expected_milestones[index] = V4_VISIBLE_RASTER_READINESS_MILESTONE.to_string();
        Ok(())
    }

    pub fn annotation_transform(&self) -> Option<&[ExactCommandPlan]> {
        match &self.payload {
            ScenarioPayload::AnnotationTransform(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn annotation_properties_history(&self) -> Option<&[ExactCommandPlan]> {
        match &self.payload {
            ScenarioPayload::AnnotationPropertiesHistory(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn viewer_layout(&self) -> Option<&ViewerLayoutPlan> {
        match &self.payload {
            ScenarioPayload::ViewerLayout(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn page_navigation(&self) -> Option<&PageNavigationPlan> {
        match &self.payload {
            ScenarioPayload::PageNavigation(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn zoom(&self) -> Option<&ZoomPlan> {
        match &self.payload {
            ScenarioPayload::Zoom(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn high_zoom_pan(&self) -> Option<&HighZoomPanPlan> {
        match &self.payload {
            ScenarioPayload::HighZoomPan(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn cache_pressure(&self) -> Option<&CachePressurePlan> {
        match &self.payload {
            ScenarioPayload::CachePressure(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn close_reopen(&self) -> Option<&CloseReopenPlan> {
        match &self.payload {
            ScenarioPayload::CloseReopen(plan) => Some(plan),
            _ => None,
        }
    }

    fn expected_milestones(&self) -> BTreeMap<String, BTreeSet<String>> {
        let entries: Vec<(&str, &[String])> = match &self.payload {
            ScenarioPayload::ViewerLayout(plan) => vec![
                (&plan.single.command_id, &plan.single.expected_milestones),
                (
                    &plan.continuous.command_id,
                    &plan.continuous.expected_milestones,
                ),
            ],
            ScenarioPayload::PageNavigation(plan) => {
                vec![(&plan.command_id, &plan.expected_milestones)]
            }
            ScenarioPayload::Zoom(plan) => {
                vec![(&plan.command_id, &plan.expected_milestones)]
            }
            ScenarioPayload::HighZoomPan(plan) => {
                vec![(&plan.command_id, &plan.expected_milestones)]
            }
            ScenarioPayload::CachePressure(plan) => {
                vec![(&plan.command_id, &plan.expected_milestones)]
            }
            ScenarioPayload::CloseReopen(plan) => {
                vec![(&plan.command_id, &plan.expected_milestones)]
            }
            ScenarioPayload::AnnotationCreate(plan) => vec![
                (
                    &plan.rectangle.command_id,
                    &plan.rectangle.expected_milestones,
                ),
                (
                    &plan.highlight.command_id,
                    &plan.highlight.expected_milestones,
                ),
            ],
            ScenarioPayload::AnnotationTransform(plan) => plan
                .iter()
                .map(|command| {
                    (
                        command.command_id.as_str(),
                        command.expected_milestones.as_slice(),
                    )
                })
                .collect(),
            ScenarioPayload::AnnotationPropertiesHistory(plan) => plan
                .iter()
                .map(|command| {
                    (
                        command.command_id.as_str(),
                        command.expected_milestones.as_slice(),
                    )
                })
                .collect(),
            ScenarioPayload::EditorCreate(plan) => plan
                .iter()
                .map(|command| {
                    (
                        command.command_id.as_str(),
                        command.expected_milestones.as_slice(),
                    )
                })
                .collect(),
            ScenarioPayload::ContinuousScroll(plan) => {
                vec![(&plan.command_id, &plan.expected_milestones)]
            }
        };
        entries
            .into_iter()
            .map(|(command, milestones)| {
                (
                    command.to_string(),
                    milestones.iter().cloned().collect::<BTreeSet<_>>(),
                )
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct MilestoneGate {
    expected: BTreeMap<String, BTreeSet<String>>,
    observed: BTreeMap<String, BTreeSet<String>>,
}

impl MilestoneGate {
    pub fn new(plan: &ComparisonScenarioPlan) -> Self {
        Self {
            expected: plan.expected_milestones(),
            observed: BTreeMap::new(),
        }
    }

    pub fn from_command(command_id: &str, expected_milestones: &[String]) -> Self {
        Self {
            expected: BTreeMap::from([(
                command_id.to_string(),
                expected_milestones.iter().cloned().collect(),
            )]),
            observed: BTreeMap::new(),
        }
    }

    pub fn record(
        &mut self,
        command_id: &str,
        milestone: &str,
    ) -> Result<(), ComparisonScenarioError> {
        if !self
            .expected
            .get(command_id)
            .is_some_and(|milestones| milestones.contains(milestone))
        {
            return Err(ComparisonScenarioError::Invalid(format!(
                "{milestone} is not declared for {command_id}"
            )));
        }
        self.observed
            .entry(command_id.to_string())
            .or_default()
            .insert(milestone.to_string());
        Ok(())
    }

    pub fn missing(&self) -> Vec<(String, String)> {
        self.expected
            .iter()
            .flat_map(|(command, milestones)| {
                milestones
                    .iter()
                    .filter(move |milestone| {
                        !self
                            .observed
                            .get(command)
                            .is_some_and(|observed| observed.contains(*milestone))
                    })
                    .map(move |milestone| (command.clone(), milestone.clone()))
            })
            .collect()
    }

    pub fn is_complete(&self) -> bool {
        self.missing().is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComparisonScenarioError {
    Invalid(String),
}

impl fmt::Display for ComparisonScenarioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ComparisonScenarioError {}

fn parse_annotation_create(
    workload: &Value,
) -> Result<AnnotationCreatePlan, ComparisonScenarioError> {
    let rectangle = command(workload, "rectangle-v1", "rectangle:create-sparse")?;
    let rectangle_path = field(rectangle, "pointer_path")?;
    require_string(rectangle_path, "coordinate_space", "pdf-points-bottom-left")?;
    require_string(rectangle_path, "interpolation", "linear-inclusive")?;
    let rectangle = LinearPointerCommand {
        command_id: string_field(rectangle, "id")?.to_string(),
        annotation_id: string_field(rectangle, "annotation_id")?.to_string(),
        sample_count: usize_field(rectangle_path, "expected_sample_count")?,
        rate_hz: u32_field(rectangle_path, "rate_hz")?,
        duration_ms: u64_field(rectangle_path, "duration_ms")?,
        start: point_field(rectangle_path, "start")?,
        finish: point_field(rectangle_path, "finish")?,
        expected_milestones: milestones(rectangle)?,
    };
    validate_sample_count(
        &rectangle.command_id,
        rectangle.sample_count,
        rectangle.rate_hz,
        rectangle.duration_ms,
    )?;

    let highlight = command(workload, "highlight-v1", "highlight:create")?;
    let highlight_path = field(highlight, "pointer_path")?;
    require_string(highlight_path, "coordinate_space", "pdf-points-bottom-left")?;
    require_string(highlight_path, "interpolation", "catmull-rom-inclusive")?;
    let control_points = array_field(highlight_path, "control_points")?
        .iter()
        .map(point_array)
        .collect::<Result<Vec<_>, _>>()?;
    if control_points.len() < 2 {
        return Err(ComparisonScenarioError::Invalid(
            "highlight:create requires at least two control points".into(),
        ));
    }
    let highlight = CatmullRomPointerCommand {
        command_id: string_field(highlight, "id")?.to_string(),
        annotation_id: string_field(highlight, "annotation_id")?.to_string(),
        sample_count: usize_field(highlight_path, "expected_sample_count")?,
        rate_hz: u32_field(highlight_path, "rate_hz")?,
        duration_ms: u64_field(highlight_path, "duration_ms")?,
        control_points,
        expected_milestones: milestones(highlight)?,
    };
    validate_sample_count(
        &highlight.command_id,
        highlight.sample_count,
        highlight.rate_hz,
        highlight.duration_ms,
    )?;
    Ok(AnnotationCreatePlan {
        rectangle,
        highlight,
    })
}

fn parse_viewer_layout(workload: &Value) -> Result<ViewerLayoutPlan, ComparisonScenarioError> {
    let parse = |command_id: &str| -> Result<LayoutCommand, ComparisonScenarioError> {
        let source = command(workload, "viewer-v1", command_id)?;
        Ok(LayoutCommand {
            command_id: string_field(source, "id")?.to_string(),
            layout: string_field(source, "layout")?.to_string(),
            expected_milestones: milestones(source)?,
        })
    };
    let single = parse("viewer:layout-single")?;
    let continuous = parse("viewer:layout-continuous")?;
    if single.layout != "single-page" || continuous.layout != "continuous" {
        return Err(ComparisonScenarioError::Invalid(
            "viewer layout commands must retain single-page and continuous values".into(),
        ));
    }
    Ok(ViewerLayoutPlan { single, continuous })
}

fn parse_page_navigation(workload: &Value) -> Result<PageNavigationPlan, ComparisonScenarioError> {
    let source = command(workload, "viewer-v1", "viewer:navigate-normalized")?;
    require_string(source, "clamp", "1..page-count")?;
    Ok(PageNavigationPlan {
        command_id: string_field(source, "id")?.to_string(),
        page_formula: string_array_field(source, "page_formula")?,
        deduplicate_after_clamp: bool_field(source, "deduplicate_after_clamp")?,
        expected_milestones: milestones(source)?,
    })
}

fn parse_zoom(workload: &Value) -> Result<ZoomPlan, ComparisonScenarioError> {
    let source = command(workload, "viewer-v1", "viewer:zoom-sequence")?;
    require_string(source, "fixture_id", "usgs-usa-geology-sheet-v1")?;
    Ok(ZoomPlan {
        command_id: string_field(source, "id")?.to_string(),
        percent: array_field(source, "percent")?
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        ComparisonScenarioError::Invalid(
                            "zoom percent must contain unsigned 32-bit integers".into(),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?,
        expected_milestones: milestones(source)?,
    })
}

fn parse_high_zoom_pan(workload: &Value) -> Result<HighZoomPanPlan, ComparisonScenarioError> {
    let source = command(workload, "viewer-v1", "viewer:pan-usgs")?;
    require_string(source, "fixture_id", "usgs-usa-geology-sheet-v1")?;
    Ok(HighZoomPanPlan {
        command_id: string_field(source, "id")?.to_string(),
        zoom_percent: u32_field(source, "zoom_percent")?,
        duration_ms: u64_field(source, "duration_ms")?,
        rate_hz: u32_field(source, "rate_hz")?,
        normalized_viewport_points: array_field(source, "normalized_viewport_points")?
            .iter()
            .map(point_array)
            .collect::<Result<Vec<_>, _>>()?,
        expected_milestones: milestones(source)?,
    })
}

fn parse_cache_pressure(workload: &Value) -> Result<CachePressurePlan, ComparisonScenarioError> {
    let source = command(workload, "viewer-v1", "viewer:cache-pressure")?;
    Ok(CachePressurePlan {
        command_id: string_field(source, "id")?.to_string(),
        cycles: usize_field(source, "cycles")?,
        sequence: string_array_field(source, "sequence")?,
        expected_milestones: milestones(source)?,
    })
}

fn parse_close_reopen(workload: &Value) -> Result<CloseReopenPlan, ComparisonScenarioError> {
    let source = command(workload, "viewer-v1", "viewer:close-recover-reopen")?;
    Ok(CloseReopenPlan {
        command_id: string_field(source, "id")?.to_string(),
        reopen_cache_class: string_field(source, "reopen_cache_class")?.to_string(),
        expected_milestones: milestones(source)?,
    })
}

fn parse_continuous_scroll(
    workload: &Value,
) -> Result<ContinuousScrollPlan, ComparisonScenarioError> {
    let scroll = command(workload, "viewer-v1", "viewer:continuous-scroll")?;
    let path = field(scroll, "path")?;
    Ok(ContinuousScrollPlan {
        command_id: string_field(scroll, "id")?.to_string(),
        input_rate_hz: u32_field(scroll, "input_rate_hz")?,
        forward_duration_ms: u64_field(path, "forward_duration_ms")?,
        forward_viewport_heights: number_field(path, "forward_viewport_heights")?,
        pause_duration_ms: u64_field(path, "pause_duration_ms")?,
        reverse_duration_ms: u64_field(path, "reverse_duration_ms")?,
        finish_page: usize_field(path, "finish_page")?,
        expected_milestones: milestones(scroll)?,
    })
}

fn validate_sample_count(
    command_id: &str,
    sample_count: usize,
    rate_hz: u32,
    duration_ms: u64,
) -> Result<(), ComparisonScenarioError> {
    let expected = duration_ms
        .checked_mul(u64::from(rate_hz))
        .and_then(|value| value.checked_div(1_000))
        .and_then(|value| value.checked_add(1))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            ComparisonScenarioError::Invalid(format!("{command_id} sample count overflows"))
        })?;
    if sample_count != expected {
        return Err(ComparisonScenarioError::Invalid(format!(
            "{command_id} declares {sample_count} samples, expected {expected}"
        )));
    }
    Ok(())
}

fn parse_exact_commands(
    workload: &Value,
    commands: &[(&str, &str)],
) -> Result<Vec<ExactCommandPlan>, ComparisonScenarioError> {
    commands
        .iter()
        .map(|(journey_id, command_id)| {
            let source = command(workload, journey_id, command_id)?;
            Ok(ExactCommandPlan {
                command_id: (*command_id).to_string(),
                expected_milestones: milestones(source)?,
            })
        })
        .collect()
}

fn command<'a>(
    workload: &'a Value,
    journey_id: &str,
    command_id: &str,
) -> Result<&'a Value, ComparisonScenarioError> {
    let journey = array_field(workload, "journeys")?
        .iter()
        .find(|journey| journey.get("id").and_then(Value::as_str) == Some(journey_id))
        .ok_or_else(|| ComparisonScenarioError::Invalid(format!("missing journey {journey_id}")))?;
    array_field(journey, "commands")?
        .iter()
        .find(|candidate| candidate.get("id").and_then(Value::as_str) == Some(command_id))
        .ok_or_else(|| {
            ComparisonScenarioError::Invalid(format!(
                "missing command {command_id} in {journey_id}"
            ))
        })
}

fn milestones(command: &Value) -> Result<Vec<String>, ComparisonScenarioError> {
    let milestones = array_field(command, "expected_milestones")?
        .iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                ComparisonScenarioError::Invalid("expected_milestones must contain strings".into())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if milestones.is_empty() {
        return Err(ComparisonScenarioError::Invalid(
            "expected_milestones must not be empty".into(),
        ));
    }
    Ok(milestones)
}

fn inclusive_linear_samples(
    start: [f64; 2],
    finish: [f64; 2],
    sample_count: usize,
) -> Vec<[f64; 2]> {
    if sample_count <= 1 {
        return vec![start];
    }
    let denominator = (sample_count - 1) as f64;
    (0..sample_count)
        .map(|index| {
            let t = index as f64 / denominator;
            [
                start[0] + (finish[0] - start[0]) * t,
                start[1] + (finish[1] - start[1]) * t,
            ]
        })
        .collect()
}

fn inclusive_catmull_rom_samples(
    control_points: &[[f64; 2]],
    sample_count: usize,
) -> Vec<[f64; 2]> {
    if control_points.len() < 2 || sample_count <= 1 {
        return control_points.first().copied().into_iter().collect();
    }
    let segments = control_points.len() - 1;
    let denominator = (sample_count - 1) as f64;
    (0..sample_count)
        .map(|index| {
            let global = index as f64 / denominator * segments as f64;
            let segment = (global.floor() as usize).min(segments - 1);
            let t = if index + 1 == sample_count {
                1.0
            } else {
                global - segment as f64
            };
            let p0 = control_points[segment.saturating_sub(1)];
            let p1 = control_points[segment];
            let p2 = control_points[segment + 1];
            let p3 = control_points[(segment + 2).min(control_points.len() - 1)];
            [
                catmull_rom(p0[0], p1[0], p2[0], p3[0], t),
                catmull_rom(p0[1], p1[1], p2[1], p3[1], t),
            ]
        })
        .collect()
}

fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

fn require_string(
    value: &Value,
    name: &str,
    expected: &str,
) -> Result<(), ComparisonScenarioError> {
    let actual = string_field(value, name)?;
    if actual != expected {
        return Err(ComparisonScenarioError::Invalid(format!(
            "{name} must be {expected}, received {actual}"
        )));
    }
    Ok(())
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, ComparisonScenarioError> {
    value
        .get(name)
        .ok_or_else(|| ComparisonScenarioError::Invalid(format!("missing {name}")))
}

fn array_field<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], ComparisonScenarioError> {
    field(value, name)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| ComparisonScenarioError::Invalid(format!("{name} must be an array")))
}

fn string_array_field(value: &Value, name: &str) -> Result<Vec<String>, ComparisonScenarioError> {
    array_field(value, name)?
        .iter()
        .map(|entry| {
            entry.as_str().map(str::to_string).ok_or_else(|| {
                ComparisonScenarioError::Invalid(format!("{name} must contain strings"))
            })
        })
        .collect()
}

fn bool_field(value: &Value, name: &str) -> Result<bool, ComparisonScenarioError> {
    field(value, name)?
        .as_bool()
        .ok_or_else(|| ComparisonScenarioError::Invalid(format!("{name} must be boolean")))
}

fn string_field<'a>(value: &'a Value, name: &str) -> Result<&'a str, ComparisonScenarioError> {
    field(value, name)?
        .as_str()
        .ok_or_else(|| ComparisonScenarioError::Invalid(format!("{name} must be a string")))
}

fn number_field(value: &Value, name: &str) -> Result<f64, ComparisonScenarioError> {
    field(value, name)?
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| ComparisonScenarioError::Invalid(format!("{name} must be finite")))
}

fn u64_field(value: &Value, name: &str) -> Result<u64, ComparisonScenarioError> {
    field(value, name)?.as_u64().ok_or_else(|| {
        ComparisonScenarioError::Invalid(format!("{name} must be an unsigned integer"))
    })
}

fn u32_field(value: &Value, name: &str) -> Result<u32, ComparisonScenarioError> {
    u32::try_from(u64_field(value, name)?)
        .map_err(|_| ComparisonScenarioError::Invalid(format!("{name} exceeds u32")))
}

fn usize_field(value: &Value, name: &str) -> Result<usize, ComparisonScenarioError> {
    usize::try_from(u64_field(value, name)?)
        .map_err(|_| ComparisonScenarioError::Invalid(format!("{name} exceeds usize")))
}

fn point_field(value: &Value, name: &str) -> Result<[f64; 2], ComparisonScenarioError> {
    let point = field(value, name)?;
    Ok([number_field(point, "x")?, number_field(point, "y")?])
}

fn point_array(value: &Value) -> Result<[f64; 2], ComparisonScenarioError> {
    let coordinates = value
        .as_array()
        .filter(|coordinates| coordinates.len() == 2)
        .ok_or_else(|| ComparisonScenarioError::Invalid("point must contain x and y".into()))?;
    let coordinate = |index: usize| {
        coordinates[index]
            .as_f64()
            .filter(|number| number.is_finite())
            .ok_or_else(|| ComparisonScenarioError::Invalid("point must be finite".into()))
    };
    Ok([coordinate(0)?, coordinate(1)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_samples_are_inclusive_and_have_the_frozen_count() {
        let plan =
            ComparisonScenarioPlan::embedded(ComparisonScenarioKind::AnnotationCreate).unwrap();
        let annotation = plan.annotation_create().unwrap();

        let rectangle = annotation.rectangle.samples();
        assert_eq!(rectangle.len(), 361);
        assert_eq!(rectangle.first(), Some(&[72.0, 144.0]));
        assert_eq!(rectangle.last(), Some(&[252.0, 240.0]));

        let highlight = annotation.highlight.samples();
        assert_eq!(highlight.len(), 361);
        assert_eq!(highlight.first(), Some(&[90.0, 330.0]));
        assert_eq!(highlight.last(), Some(&[300.0, 334.0]));
    }

    #[test]
    fn viewer_plans_are_derived_from_the_embedded_manifest() {
        let layout = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::ViewerLayout)
            .unwrap()
            .viewer_layout()
            .unwrap()
            .clone();
        assert_eq!(layout.single.command_id, "viewer:layout-single");
        assert_eq!(layout.single.layout, "single-page");
        assert_eq!(layout.continuous.command_id, "viewer:layout-continuous");
        assert_eq!(layout.continuous.layout, "continuous");

        let navigation = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::PageNavigation)
            .unwrap()
            .page_navigation()
            .unwrap()
            .clone();
        assert_eq!(navigation.page_formula[0], "last");
        assert_eq!(navigation.page_formula[9], "1");
        assert!(navigation.deduplicate_after_clamp);

        let zoom = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::Zoom)
            .unwrap()
            .zoom()
            .unwrap()
            .clone();
        assert_eq!(
            zoom.percent,
            [100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100]
        );

        let pan = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::HighZoomPan)
            .unwrap()
            .high_zoom_pan()
            .unwrap()
            .clone();
        assert_eq!(pan.zoom_percent, 1600);
        assert_eq!(pan.rate_hz, 120);
        assert_eq!(pan.normalized_viewport_points.first(), Some(&[0.5, 0.5]));
        assert_eq!(pan.normalized_viewport_points.last(), Some(&[0.5, 0.5]));

        let cache = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::CachePressure)
            .unwrap()
            .cache_pressure()
            .unwrap()
            .clone();
        assert_eq!(cache.cycles, 5);
        assert_eq!(cache.sequence, ["navigate", "zoom", "pan", "return-page-1"]);

        let reopen = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::CloseReopen)
            .unwrap()
            .close_reopen()
            .unwrap()
            .clone();
        assert_eq!(reopen.reopen_cache_class, "declared-warm");
    }

    #[test]
    fn v4_scroll_override_leaves_the_embedded_v3_manifest_unchanged() {
        let frozen =
            ComparisonScenarioPlan::embedded(ComparisonScenarioKind::ContinuousScroll).unwrap();
        assert!(
            frozen
                .continuous_scroll()
                .unwrap()
                .expected_milestones
                .iter()
                .any(|milestone| milestone == V3_BLANK_FRAME_MILESTONE)
        );

        let mut v4 = frozen.clone();
        v4.use_v4_continuous_raster_readiness().unwrap();
        assert!(
            v4.continuous_scroll()
                .unwrap()
                .expected_milestones
                .iter()
                .any(|milestone| milestone == V4_VISIBLE_RASTER_READINESS_MILESTONE)
        );
        assert!(
            frozen
                .continuous_scroll()
                .unwrap()
                .expected_milestones
                .iter()
                .any(|milestone| milestone == V3_BLANK_FRAME_MILESTONE)
        );
    }

    #[test]
    fn viewer_milestone_gate_rejects_partial_generic_timing() {
        let plan = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::Zoom).unwrap();
        let mut gate = MilestoneGate::new(&plan);
        let zoom = plan.zoom().unwrap();
        for milestone in &zoom.expected_milestones[..3] {
            gate.record(&zoom.command_id, milestone).unwrap();
        }
        assert!(!gate.is_complete());
        assert_eq!(gate.missing().len(), zoom.expected_milestones.len() - 3);
    }

    #[test]
    fn editor_create_has_an_exact_fail_closed_milestone_gate() {
        let plan = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::EditorCreate).unwrap();
        let mut gate = MilestoneGate::new(&plan);
        let missing = gate.missing();
        for command_id in [
            "text:create",
            "length:set-scale",
            "length:create",
            "image:create",
        ] {
            assert!(
                missing.iter().any(|(command, _)| command == command_id),
                "missing exact editor-create command {command_id}"
            );
        }
        let error = gate
            .record("text:create", "not-declared")
            .expect_err("an undeclared editor milestone must fail closed");
        assert_eq!(
            error.to_string(),
            "not-declared is not declared for text:create"
        );
    }
}
