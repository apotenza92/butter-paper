//! Application-owned semantic snapping for PDF annotation geometry.
//!
//! The module is GPUI-free. It indexes immutable scene geometry and returns a
//! decision; callers retain gesture state, mutate annotations, and paint any
//! transient guide evidence.

use crate::annotation_model::{AnnotationScene, MarkupId, PdfPoint};

const DEFAULT_SENSITIVITY_WINDOW_PX: f64 = 8.;
const POINTS_PER_INCH: f64 = 72.;
const MILLIMETRES_PER_INCH: f64 = 25.4;
const MIN_CONSTRUCTION_GRID_SPACING_MM: f64 = 1.;
const MAX_CONSTRUCTION_GRID_SPACING_MM: f64 = 500.;
const MIN_DIMENSION_INCREMENT_MM: f64 = 0.1;
const MAX_DIMENSION_INCREMENT_MM: f64 = 500.;
pub const MAX_CONSTRUCTION_GRID_POINTS: usize = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticSnapSource {
    Content,
    Annotation,
    PageGrid,
    ConstructionGrid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticSnapGuideType {
    Alignment,
    EqualSize,
    EqualSpacing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticSnapError {
    InvalidPageDimensions,
    InvalidConstructionGridSpacing,
    ConstructionGridPointLimitExceeded,
    InvalidDimensionIncrement,
    InvalidMeasuredDistance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticSnapRole {
    Endpoint,
    Midpoint,
    Center,
    Intersection,
    Nearest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticSnapTarget {
    Endpoint,
    Midpoint,
    Center,
    Intersection,
    Nearest,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticSnapSettings {
    content_enabled: bool,
    annotations_enabled: bool,
    page_grid_enabled: bool,
    construction_grid_enabled: bool,
    construction_grid_visible: bool,
    construction_grid_spacing_mm: f64,
    dimension_increment_enabled: bool,
    dimension_increment_mm: f64,
    guides_enabled: bool,
    alignment_guides: bool,
    equal_size_guides: bool,
    equal_spacing_guides: bool,
    sensitivity_window_px: f64,
    endpoint: bool,
    midpoint: bool,
    center: bool,
    intersection: bool,
    nearest: bool,
}

impl SemanticSnapSettings {
    pub fn is_source_enabled(self, source: SemanticSnapSource) -> bool {
        match source {
            SemanticSnapSource::Content => self.content_enabled,
            SemanticSnapSource::Annotation => self.annotations_enabled,
            SemanticSnapSource::PageGrid => self.page_grid_enabled,
            SemanticSnapSource::ConstructionGrid => self.construction_grid_enabled,
        }
    }

    pub fn with_source(mut self, source: SemanticSnapSource, enabled: bool) -> Self {
        match source {
            SemanticSnapSource::Content => self.content_enabled = enabled,
            SemanticSnapSource::Annotation => self.annotations_enabled = enabled,
            SemanticSnapSource::PageGrid => self.page_grid_enabled = enabled,
            SemanticSnapSource::ConstructionGrid => self.construction_grid_enabled = enabled,
        }
        self
    }

    pub fn annotations_enabled(self) -> bool {
        self.annotations_enabled
    }

    pub fn with_annotation_source(mut self, enabled: bool) -> Self {
        self.annotations_enabled = enabled;
        self
    }

    pub fn construction_grid_visible(self) -> bool {
        self.construction_grid_visible
    }

    pub fn with_construction_grid_visible(mut self, visible: bool) -> Self {
        self.construction_grid_visible = visible;
        self
    }

    pub fn construction_grid_spacing_mm(self) -> f64 {
        self.construction_grid_spacing_mm
    }

    pub fn with_construction_grid_spacing_mm(mut self, spacing_mm: f64) -> Self {
        self.construction_grid_spacing_mm = if spacing_mm.is_finite() {
            spacing_mm.clamp(
                MIN_CONSTRUCTION_GRID_SPACING_MM,
                MAX_CONSTRUCTION_GRID_SPACING_MM,
            )
        } else {
            10.
        };
        self
    }

    pub fn dimension_increment_enabled(self) -> bool {
        self.dimension_increment_enabled
    }

    pub fn with_dimension_increment_enabled(mut self, enabled: bool) -> Self {
        self.dimension_increment_enabled = enabled;
        self
    }

    pub fn dimension_increment_mm(self) -> f64 {
        self.dimension_increment_mm
    }

    pub fn with_dimension_increment_mm(mut self, increment_mm: f64) -> Self {
        self.dimension_increment_mm = if increment_mm.is_finite() {
            increment_mm.clamp(MIN_DIMENSION_INCREMENT_MM, MAX_DIMENSION_INCREMENT_MM)
        } else {
            5.
        };
        self
    }

    pub fn guides_enabled(self) -> bool {
        self.guides_enabled
    }

    pub fn with_guides_enabled(mut self, enabled: bool) -> Self {
        self.guides_enabled = enabled;
        self
    }

    pub fn is_guide_enabled(self, guide: SemanticSnapGuideType) -> bool {
        match guide {
            SemanticSnapGuideType::Alignment => self.alignment_guides,
            SemanticSnapGuideType::EqualSize => self.equal_size_guides,
            SemanticSnapGuideType::EqualSpacing => self.equal_spacing_guides,
        }
    }

    pub fn with_guide(mut self, guide: SemanticSnapGuideType, enabled: bool) -> Self {
        match guide {
            SemanticSnapGuideType::Alignment => self.alignment_guides = enabled,
            SemanticSnapGuideType::EqualSize => self.equal_size_guides = enabled,
            SemanticSnapGuideType::EqualSpacing => self.equal_spacing_guides = enabled,
        }
        self
    }

    pub fn validate(self) -> Result<(), SemanticSnapError> {
        if !self.construction_grid_spacing_mm.is_finite()
            || !(MIN_CONSTRUCTION_GRID_SPACING_MM..=MAX_CONSTRUCTION_GRID_SPACING_MM)
                .contains(&self.construction_grid_spacing_mm)
        {
            return Err(SemanticSnapError::InvalidConstructionGridSpacing);
        }
        if !self.dimension_increment_mm.is_finite()
            || !(MIN_DIMENSION_INCREMENT_MM..=MAX_DIMENSION_INCREMENT_MM)
                .contains(&self.dimension_increment_mm)
        {
            return Err(SemanticSnapError::InvalidDimensionIncrement);
        }
        Ok(())
    }

    pub fn sensitivity_window_px(self) -> f64 {
        self.sensitivity_window_px
    }

    pub fn is_target_enabled(self, role: SemanticSnapRole) -> bool {
        match role {
            SemanticSnapRole::Endpoint => self.endpoint,
            SemanticSnapRole::Midpoint => self.midpoint,
            SemanticSnapRole::Center => self.center,
            SemanticSnapRole::Intersection => self.intersection,
            SemanticSnapRole::Nearest => self.nearest,
        }
    }

    pub fn with_target(mut self, target: SemanticSnapTarget, enabled: bool) -> Self {
        match target {
            SemanticSnapTarget::Endpoint => self.endpoint = enabled,
            SemanticSnapTarget::Midpoint => self.midpoint = enabled,
            SemanticSnapTarget::Center => self.center = enabled,
            SemanticSnapTarget::Intersection => self.intersection = enabled,
            SemanticSnapTarget::Nearest => self.nearest = enabled,
        }
        self
    }

    pub fn is_target_selected(self, target: SemanticSnapTarget) -> bool {
        match target {
            SemanticSnapTarget::Endpoint => self.endpoint,
            SemanticSnapTarget::Midpoint => self.midpoint,
            SemanticSnapTarget::Center => self.center,
            SemanticSnapTarget::Intersection => self.intersection,
            SemanticSnapTarget::Nearest => self.nearest,
        }
    }
}

impl Default for SemanticSnapSettings {
    fn default() -> Self {
        Self {
            content_enabled: true,
            annotations_enabled: true,
            page_grid_enabled: true,
            construction_grid_enabled: false,
            construction_grid_visible: true,
            construction_grid_spacing_mm: 10.,
            dimension_increment_enabled: false,
            dimension_increment_mm: 5.,
            guides_enabled: true,
            alignment_guides: true,
            equal_size_guides: true,
            equal_spacing_guides: true,
            sensitivity_window_px: DEFAULT_SENSITIVITY_WINDOW_PX,
            endpoint: true,
            midpoint: true,
            center: true,
            intersection: true,
            nearest: false,
        }
    }
}

pub fn construction_grid_points(
    page_width_pdf_points: f64,
    page_height_pdf_points: f64,
    spacing_mm: f64,
) -> Result<Vec<PdfPoint>, SemanticSnapError> {
    if !page_width_pdf_points.is_finite()
        || !page_height_pdf_points.is_finite()
        || page_width_pdf_points < 0.
        || page_height_pdf_points < 0.
    {
        return Err(SemanticSnapError::InvalidPageDimensions);
    }
    if !spacing_mm.is_finite()
        || !(MIN_CONSTRUCTION_GRID_SPACING_MM..=MAX_CONSTRUCTION_GRID_SPACING_MM)
            .contains(&spacing_mm)
    {
        return Err(SemanticSnapError::InvalidConstructionGridSpacing);
    }
    let spacing_pdf_points = spacing_mm * POINTS_PER_INCH / MILLIMETRES_PER_INCH;
    let columns_f64 = (page_width_pdf_points / spacing_pdf_points).floor() + 1.;
    let rows_f64 = (page_height_pdf_points / spacing_pdf_points).floor() + 1.;
    if columns_f64 > MAX_CONSTRUCTION_GRID_POINTS as f64
        || rows_f64 > MAX_CONSTRUCTION_GRID_POINTS as f64
    {
        return Err(SemanticSnapError::ConstructionGridPointLimitExceeded);
    }
    let columns = columns_f64 as usize;
    let rows = rows_f64 as usize;
    let point_count = columns
        .checked_mul(rows)
        .ok_or(SemanticSnapError::ConstructionGridPointLimitExceeded)?;
    if point_count > MAX_CONSTRUCTION_GRID_POINTS {
        return Err(SemanticSnapError::ConstructionGridPointLimitExceeded);
    }

    let mut points = Vec::with_capacity(point_count);
    for column in 0..columns {
        for row in 0..rows {
            points.push(PdfPoint {
                x: column as f64 * spacing_pdf_points,
                y: row as f64 * spacing_pdf_points,
            });
        }
    }
    Ok(points)
}

pub fn quantize_pdf_distance_to_mm_increment(
    distance_pdf_points: f64,
    increment_mm: f64,
) -> Result<f64, SemanticSnapError> {
    if !distance_pdf_points.is_finite() || distance_pdf_points < 0. {
        return Err(SemanticSnapError::InvalidMeasuredDistance);
    }
    if !increment_mm.is_finite()
        || !(MIN_DIMENSION_INCREMENT_MM..=MAX_DIMENSION_INCREMENT_MM).contains(&increment_mm)
    {
        return Err(SemanticSnapError::InvalidDimensionIncrement);
    }
    if distance_pdf_points == 0. {
        return Ok(0.);
    }
    let distance_mm = distance_pdf_points * MILLIMETRES_PER_INCH / POINTS_PER_INCH;
    let quantized_mm = (distance_mm / increment_mm).round().max(1.) * increment_mm;
    Ok(quantized_mm * POINTS_PER_INCH / MILLIMETRES_PER_INCH)
}

pub fn resolve_construction_grid_point(
    point: PdfPoint,
    page_width_pdf_points: f64,
    page_height_pdf_points: f64,
    settings: &SemanticSnapSettings,
    window_pixels_per_pdf_point: f64,
) -> Option<SemanticSnapDecision> {
    if !settings.is_source_enabled(SemanticSnapSource::ConstructionGrid)
        || !settings.is_target_enabled(SemanticSnapRole::Intersection)
        || !page_width_pdf_points.is_finite()
        || !page_height_pdf_points.is_finite()
        || page_width_pdf_points < 0.
        || page_height_pdf_points < 0.
        || !window_pixels_per_pdf_point.is_finite()
        || window_pixels_per_pdf_point <= 0.
    {
        return None;
    }
    let spacing = settings.construction_grid_spacing_mm() * POINTS_PER_INCH / MILLIMETRES_PER_INCH;
    let max_column = (page_width_pdf_points / spacing).floor();
    let max_row = (page_height_pdf_points / spacing).floor();
    let snapped = PdfPoint {
        x: (point.x / spacing).round().clamp(0., max_column) * spacing,
        y: (point.y / spacing).round().clamp(0., max_row) * spacing,
    };
    let distance_window_px = squared_distance(point, snapped).sqrt() * window_pixels_per_pdf_point;
    (distance_window_px <= settings.sensitivity_window_px()).then_some(SemanticSnapDecision {
        point: snapped,
        owner_id: None,
        role: SemanticSnapRole::Intersection,
        distance_window_px,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSnapDecision {
    pub point: PdfPoint,
    pub owner_id: Option<MarkupId>,
    pub role: SemanticSnapRole,
    pub distance_window_px: f64,
}

#[derive(Clone, Debug)]
enum CandidateGeometry {
    Point(PdfPoint),
    Segment { start: PdfPoint, end: PdfPoint },
}

#[derive(Clone, Debug)]
struct Candidate {
    geometry: CandidateGeometry,
    owner_id: Option<MarkupId>,
    role: SemanticSnapRole,
    source: SemanticSnapSource,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticSnapIndex {
    candidates: Vec<Candidate>,
}

impl SemanticSnapIndex {
    pub fn from_annotation_scene(scene: &AnnotationScene, excluded_owner_ids: &[MarkupId]) -> Self {
        let mut candidates = Vec::new();
        for line in &scene.straight_lines {
            if line.draft || excluded_owner_ids.contains(&line.id) {
                continue;
            }
            add_open_segment_candidates(&mut candidates, line.start, line.end, &line.id);
        }
        for rectangle in &scene.rectangles {
            if rectangle.preview || excluded_owner_ids.contains(&rectangle.id) {
                continue;
            }
            add_rectangle_candidates(
                &mut candidates,
                rectangle.rect,
                rectangle.rotation_degrees,
                &rectangle.id,
            );
        }
        for dimension in &scene.dimensions {
            if dimension.draft || excluded_owner_ids.contains(&dimension.id) {
                continue;
            }
            add_open_segment_candidates(
                &mut candidates,
                dimension.start,
                dimension.end,
                &dimension.id,
            );
        }
        for length in &scene.lengths {
            if excluded_owner_ids.contains(&length.id) {
                continue;
            }
            add_open_segment_candidates(&mut candidates, length.start, length.end, &length.id);
        }
        add_intersection_candidates(&mut candidates);
        Self { candidates }
    }

    pub fn with_construction_grid(
        mut self,
        page_width_pdf_points: f64,
        page_height_pdf_points: f64,
        spacing_mm: f64,
    ) -> Result<Self, SemanticSnapError> {
        self.candidates.extend(
            construction_grid_points(page_width_pdf_points, page_height_pdf_points, spacing_mm)?
                .into_iter()
                .map(|point| Candidate {
                    geometry: CandidateGeometry::Point(point),
                    owner_id: None,
                    role: SemanticSnapRole::Intersection,
                    source: SemanticSnapSource::ConstructionGrid,
                }),
        );
        Ok(self)
    }

    pub fn resolve_point(
        &self,
        point: PdfPoint,
        settings: &SemanticSnapSettings,
        window_pixels_per_pdf_point: f64,
    ) -> Option<SemanticSnapDecision> {
        self.resolve_point_with_orthogonal_anchor(
            point,
            settings,
            window_pixels_per_pdf_point,
            None,
        )
    }

    pub fn resolve_point_with_orthogonal_anchor(
        &self,
        point: PdfPoint,
        settings: &SemanticSnapSettings,
        window_pixels_per_pdf_point: f64,
        orthogonal_anchor: Option<PdfPoint>,
    ) -> Option<SemanticSnapDecision> {
        if !window_pixels_per_pdf_point.is_finite() || window_pixels_per_pdf_point <= 0. {
            return None;
        }
        let constraint = orthogonal_anchor.map(|anchor| OrthogonalConstraint::new(anchor, point));
        let point = constraint
            .as_ref()
            .map_or(point, |constraint| constraint.point);
        let tolerance_pdf = settings.sensitivity_window_px() / window_pixels_per_pdf_point;
        let tolerance_pdf_squared = tolerance_pdf * tolerance_pdf;
        let mut best: Option<(f64, SemanticSnapDecision)> = None;

        for candidate in &self.candidates {
            if !settings.is_source_enabled(candidate.source) {
                continue;
            }
            if !settings.is_target_enabled(candidate.role) {
                continue;
            }
            let resolved = match candidate.geometry {
                CandidateGeometry::Point(point) => point,
                CandidateGeometry::Segment { start, end } => {
                    project_point_to_segment(point, start, end)
                }
            };
            if constraint
                .as_ref()
                .is_some_and(|constraint| !constraint.contains(resolved))
            {
                continue;
            }
            let distance_pdf_squared = squared_distance(point, resolved);
            if distance_pdf_squared > tolerance_pdf_squared {
                continue;
            }
            let score = distance_pdf_squared
                + role_priority(candidate.role) * tolerance_pdf_squared * 0.015;
            let decision = SemanticSnapDecision {
                point: resolved,
                owner_id: candidate.owner_id.clone(),
                role: candidate.role,
                distance_window_px: distance_pdf_squared.sqrt() * window_pixels_per_pdf_point,
            };
            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score < *best_score)
            {
                best = Some((score, decision));
            }
        }

        best.map(|(_, decision)| decision)
    }
}

#[derive(Clone, Copy, Debug)]
enum OrthogonalAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug)]
struct OrthogonalConstraint {
    anchor: PdfPoint,
    point: PdfPoint,
    axis: OrthogonalAxis,
}

impl OrthogonalConstraint {
    fn new(anchor: PdfPoint, point: PdfPoint) -> Self {
        let dx = point.x - anchor.x;
        let dy = point.y - anchor.y;
        if dx.abs() >= dy.abs() {
            Self {
                anchor,
                point: PdfPoint {
                    x: point.x,
                    y: anchor.y,
                },
                axis: OrthogonalAxis::Horizontal,
            }
        } else {
            Self {
                anchor,
                point: PdfPoint {
                    x: anchor.x,
                    y: point.y,
                },
                axis: OrthogonalAxis::Vertical,
            }
        }
    }

    fn contains(self, point: PdfPoint) -> bool {
        const EPSILON: f64 = 0.000_1;
        match self.axis {
            OrthogonalAxis::Horizontal => (point.y - self.anchor.y).abs() <= EPSILON,
            OrthogonalAxis::Vertical => (point.x - self.anchor.x).abs() <= EPSILON,
        }
    }
}

fn add_intersection_candidates(candidates: &mut Vec<Candidate>) {
    const MAX_INTERSECTION_EDGE_PAIRS: usize = 50_000;
    let edges = candidates
        .iter()
        .filter_map(|candidate| match candidate.geometry {
            CandidateGeometry::Segment { start, end } => Some((start, end)),
            CandidateGeometry::Point(_) => None,
        })
        .collect::<Vec<_>>();
    let pair_count = edges.len().saturating_mul(edges.len().saturating_sub(1)) / 2;
    if pair_count > MAX_INTERSECTION_EDGE_PAIRS {
        return;
    }

    let mut seen = std::collections::BTreeSet::new();
    let mut intersections = Vec::new();
    for left_ix in 0..edges.len().saturating_sub(1) {
        for right_ix in left_ix + 1..edges.len() {
            let Some(point) = segment_intersection(edges[left_ix], edges[right_ix]) else {
                continue;
            };
            let key = (
                (point.x * 1_000.).round() as i64,
                (point.y * 1_000.).round() as i64,
            );
            if seen.insert(key) {
                intersections.push(Candidate {
                    geometry: CandidateGeometry::Point(point),
                    owner_id: None,
                    role: SemanticSnapRole::Intersection,
                    source: SemanticSnapSource::Annotation,
                });
            }
        }
    }
    candidates.extend(intersections);
}

fn add_rectangle_candidates(
    candidates: &mut Vec<Candidate>,
    rect: crate::annotation_model::PdfRect,
    rotation_degrees: f64,
    owner_id: &MarkupId,
) {
    let center = PdfPoint {
        x: rect.x + rect.width * 0.5,
        y: rect.y + rect.height * 0.5,
    };
    let corners = [
        PdfPoint {
            x: rect.x,
            y: rect.y,
        },
        PdfPoint {
            x: rect.x + rect.width,
            y: rect.y,
        },
        PdfPoint {
            x: rect.x + rect.width,
            y: rect.y + rect.height,
        },
        PdfPoint {
            x: rect.x,
            y: rect.y + rect.height,
        },
    ]
    .map(|point| rotate_point(point, center, rotation_degrees));

    candidates.push(Candidate {
        geometry: CandidateGeometry::Point(center),
        owner_id: Some(owner_id.clone()),
        role: SemanticSnapRole::Center,
        source: SemanticSnapSource::Annotation,
    });
    for corner in corners {
        candidates.push(Candidate {
            geometry: CandidateGeometry::Point(corner),
            owner_id: Some(owner_id.clone()),
            role: SemanticSnapRole::Endpoint,
            source: SemanticSnapSource::Annotation,
        });
    }
    for index in 0..corners.len() {
        let start = corners[index];
        let end = corners[(index + 1) % corners.len()];
        candidates.push(Candidate {
            geometry: CandidateGeometry::Point(PdfPoint {
                x: (start.x + end.x) * 0.5,
                y: (start.y + end.y) * 0.5,
            }),
            owner_id: Some(owner_id.clone()),
            role: SemanticSnapRole::Midpoint,
            source: SemanticSnapSource::Annotation,
        });
        candidates.push(Candidate {
            geometry: CandidateGeometry::Segment { start, end },
            owner_id: Some(owner_id.clone()),
            role: SemanticSnapRole::Nearest,
            source: SemanticSnapSource::Annotation,
        });
    }
}

fn add_open_segment_candidates(
    candidates: &mut Vec<Candidate>,
    start: PdfPoint,
    end: PdfPoint,
    owner_id: &MarkupId,
) {
    for point in [start, end] {
        candidates.push(Candidate {
            geometry: CandidateGeometry::Point(point),
            owner_id: Some(owner_id.clone()),
            role: SemanticSnapRole::Endpoint,
            source: SemanticSnapSource::Annotation,
        });
    }
    candidates.push(Candidate {
        geometry: CandidateGeometry::Point(PdfPoint {
            x: (start.x + end.x) * 0.5,
            y: (start.y + end.y) * 0.5,
        }),
        owner_id: Some(owner_id.clone()),
        role: SemanticSnapRole::Midpoint,
        source: SemanticSnapSource::Annotation,
    });
    candidates.push(Candidate {
        geometry: CandidateGeometry::Segment { start, end },
        owner_id: Some(owner_id.clone()),
        role: SemanticSnapRole::Nearest,
        source: SemanticSnapSource::Annotation,
    });
}

fn project_point_to_segment(point: PdfPoint, start: PdfPoint, end: PdfPoint) -> PdfPoint {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0. {
        return start;
    }
    let t = (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared).clamp(0., 1.);
    PdfPoint {
        x: start.x + dx * t,
        y: start.y + dy * t,
    }
}

fn segment_intersection(
    left: (PdfPoint, PdfPoint),
    right: (PdfPoint, PdfPoint),
) -> Option<PdfPoint> {
    let left_dx = left.1.x - left.0.x;
    let left_dy = left.1.y - left.0.y;
    let right_dx = right.1.x - right.0.x;
    let right_dy = right.1.y - right.0.y;
    let denominator = left_dx * right_dy - left_dy * right_dx;
    if denominator.abs() < 0.000_001 {
        return None;
    }
    let start_dx = right.0.x - left.0.x;
    let start_dy = right.0.y - left.0.y;
    let left_t = (start_dx * right_dy - start_dy * right_dx) / denominator;
    let right_t = (start_dx * left_dy - start_dy * left_dx) / denominator;
    if !(-0.000_1..=1.000_1).contains(&left_t) || !(-0.000_1..=1.000_1).contains(&right_t) {
        return None;
    }
    let point = PdfPoint {
        x: left.0.x + left_t * left_dx,
        y: left.0.y + left_t * left_dy,
    };
    if [left.0, left.1, right.0, right.1]
        .into_iter()
        .any(|endpoint| squared_distance(point, endpoint) < 0.000_001)
    {
        return None;
    }
    Some(point)
}

fn squared_distance(left: PdfPoint, right: PdfPoint) -> f64 {
    let dx = left.x - right.x;
    let dy = left.y - right.y;
    dx * dx + dy * dy
}

fn rotate_point(point: PdfPoint, center: PdfPoint, degrees: f64) -> PdfPoint {
    if degrees == 0. {
        return point;
    }
    let radians = degrees.to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    PdfPoint {
        x: center.x + dx * cosine - dy * sine,
        y: center.y + dx * sine + dy * cosine,
    }
}

fn role_priority(role: SemanticSnapRole) -> f64 {
    match role {
        SemanticSnapRole::Intersection => 0.,
        SemanticSnapRole::Endpoint => 1.,
        SemanticSnapRole::Midpoint => 2.,
        SemanticSnapRole::Center => 3.,
        SemanticSnapRole::Nearest => 8.,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pdf_points_for_mm(millimetres: f64) -> f64 {
        millimetres * POINTS_PER_INCH / MILLIMETRES_PER_INCH
    }

    #[test]
    fn electron_defaults_and_toggles_are_independent() {
        let defaults = SemanticSnapSettings::default();
        assert!(defaults.is_source_enabled(SemanticSnapSource::Content));
        assert!(defaults.is_source_enabled(SemanticSnapSource::Annotation));
        assert!(defaults.is_source_enabled(SemanticSnapSource::PageGrid));
        assert!(!defaults.is_source_enabled(SemanticSnapSource::ConstructionGrid));
        assert!(defaults.construction_grid_visible());
        assert_eq!(defaults.construction_grid_spacing_mm(), 10.);
        assert!(!defaults.dimension_increment_enabled());
        assert_eq!(defaults.dimension_increment_mm(), 5.);
        assert!(defaults.guides_enabled());
        for guide in [
            SemanticSnapGuideType::Alignment,
            SemanticSnapGuideType::EqualSize,
            SemanticSnapGuideType::EqualSpacing,
        ] {
            assert!(defaults.is_guide_enabled(guide));
        }

        let changed = defaults
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::ConstructionGrid, true)
            .with_guide(SemanticSnapGuideType::EqualSize, false);
        assert!(!changed.is_source_enabled(SemanticSnapSource::Content));
        assert!(changed.is_source_enabled(SemanticSnapSource::Annotation));
        assert!(changed.is_source_enabled(SemanticSnapSource::PageGrid));
        assert!(changed.is_source_enabled(SemanticSnapSource::ConstructionGrid));
        assert!(changed.is_guide_enabled(SemanticSnapGuideType::Alignment));
        assert!(!changed.is_guide_enabled(SemanticSnapGuideType::EqualSize));
        assert!(changed.is_guide_enabled(SemanticSnapGuideType::EqualSpacing));
    }

    #[test]
    fn numeric_settings_clamp_and_validate_boundaries() {
        let low = SemanticSnapSettings::default()
            .with_construction_grid_spacing_mm(-4.)
            .with_dimension_increment_mm(0.01);
        assert_eq!(low.construction_grid_spacing_mm(), 1.);
        assert_eq!(low.dimension_increment_mm(), 0.1);
        assert_eq!(low.validate(), Ok(()));

        let high = SemanticSnapSettings::default()
            .with_construction_grid_spacing_mm(501.)
            .with_dimension_increment_mm(5_000.);
        assert_eq!(high.construction_grid_spacing_mm(), 500.);
        assert_eq!(high.dimension_increment_mm(), 500.);
        assert_eq!(high.validate(), Ok(()));

        let mut invalid = SemanticSnapSettings::default();
        invalid.construction_grid_spacing_mm = f64::NAN;
        assert_eq!(
            invalid.validate(),
            Err(SemanticSnapError::InvalidConstructionGridSpacing)
        );
        invalid.construction_grid_spacing_mm = 10.;
        invalid.dimension_increment_mm = f64::INFINITY;
        assert_eq!(
            invalid.validate(),
            Err(SemanticSnapError::InvalidDimensionIncrement)
        );
    }

    #[test]
    fn construction_grid_is_bounded_to_asymmetric_page_dimensions() {
        let points = construction_grid_points(pdf_points_for_mm(25.), pdf_points_for_mm(12.), 10.)
            .expect("valid grid");
        assert_eq!(points.len(), 6);
        assert_eq!(points[0], PdfPoint { x: 0., y: 0. });
        assert_eq!(
            points[1],
            PdfPoint {
                x: 0.,
                y: pdf_points_for_mm(10.)
            }
        );
        assert_eq!(
            points[5],
            PdfPoint {
                x: pdf_points_for_mm(20.),
                y: pdf_points_for_mm(10.)
            }
        );
        assert!(points.iter().all(|point| {
            point.x <= pdf_points_for_mm(25.) && point.y <= pdf_points_for_mm(12.)
        }));
    }

    #[test]
    fn construction_grid_rejects_invalid_inputs_and_excessive_candidates() {
        assert_eq!(
            construction_grid_points(100., -1., 10.),
            Err(SemanticSnapError::InvalidPageDimensions)
        );
        assert_eq!(
            construction_grid_points(100., 100., 0.99),
            Err(SemanticSnapError::InvalidConstructionGridSpacing)
        );

        let spacing = pdf_points_for_mm(1.);
        assert_eq!(
            construction_grid_points(399. * spacing, 250. * spacing, 1.),
            Err(SemanticSnapError::ConstructionGridPointLimitExceeded)
        );
        assert_eq!(
            construction_grid_points(f64::MAX, 100., 1.),
            Err(SemanticSnapError::ConstructionGridPointLimitExceeded)
        );
    }

    #[test]
    fn construction_grid_resolves_without_materializing_large_grids() {
        let settings = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::ConstructionGrid, true)
            .with_construction_grid_spacing_mm(1.);
        let decision = resolve_construction_grid_point(
            PdfPoint {
                x: pdf_points_for_mm(838.2),
                y: pdf_points_for_mm(1187.1),
            },
            pdf_points_for_mm(841.),
            pdf_points_for_mm(1189.),
            &settings,
            1.,
        )
        .expect("nearby A0 grid intersection");
        assert!((decision.point.x - pdf_points_for_mm(838.)).abs() < 0.000_001);
        assert!((decision.point.y - pdf_points_for_mm(1187.)).abs() < 0.000_001);
        assert_eq!(decision.role, SemanticSnapRole::Intersection);

        let disabled = settings.with_source(SemanticSnapSource::ConstructionGrid, false);
        assert!(resolve_construction_grid_point(
            PdfPoint { x: 0.2, y: 0.2 },
            100.,
            100.,
            &disabled,
            1.,
        )
        .is_none());
    }

    #[test]
    fn distance_quantization_uses_pdf_units_and_increment_boundaries() {
        let quantized = quantize_pdf_distance_to_mm_increment(pdf_points_for_mm(13.2), 5.)
            .expect("valid increment");
        assert!((quantized - pdf_points_for_mm(15.)).abs() < 0.000_001);
        assert_eq!(quantize_pdf_distance_to_mm_increment(0., 0.1), Ok(0.));
        assert_eq!(
            quantize_pdf_distance_to_mm_increment(pdf_points_for_mm(0.2), 5.),
            Ok(pdf_points_for_mm(5.))
        );
        assert_eq!(
            quantize_pdf_distance_to_mm_increment(pdf_points_for_mm(749.), 500.),
            Ok(pdf_points_for_mm(500.))
        );
        assert_eq!(
            quantize_pdf_distance_to_mm_increment(72., 0.09),
            Err(SemanticSnapError::InvalidDimensionIncrement)
        );
        assert_eq!(
            quantize_pdf_distance_to_mm_increment(-1., 5.),
            Err(SemanticSnapError::InvalidMeasuredDistance)
        );
    }
}
