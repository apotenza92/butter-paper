//! Application-owned semantic snapping for PDF annotation geometry.
//!
//! The module is GPUI-free. It indexes immutable scene geometry and returns a
//! decision; callers retain gesture state, mutate annotations, and paint any
//! transient guide evidence.

use crate::annotation_model::{AnnotationScene, AnnotationSelectionSupplement, MarkupId, PdfPoint};
use std::{collections::HashMap, sync::Arc};

use crate::pdf_content_geometry::{PageSnapGeometry, PdfContentPrimitive};

const DEFAULT_SENSITIVITY_WINDOW_PX: f64 = 8.;
const POINTS_PER_INCH: f64 = 72.;
const MILLIMETRES_PER_INCH: f64 = 25.4;
const MIN_CONSTRUCTION_GRID_SPACING_MM: f64 = 1.;
const MAX_CONSTRUCTION_GRID_SPACING_MM: f64 = 500.;
const MIN_DIMENSION_INCREMENT_MM: f64 = 0.1;
const MAX_DIMENSION_INCREMENT_MM: f64 = 500.;
pub const MAX_CONSTRUCTION_GRID_POINTS: usize = 100_000;
pub const MAX_PAGE_GRID_CANDIDATES: usize = 100_000;

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
    InvalidPageGrid,
    PageGridCandidateLimitExceeded,
    InvalidContentGeometry,
    ContentCandidateLimitExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageGridKind {
    Rectangular,
    Ruled,
    Isometric,
    Triangle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageGridSource {
    Generated,
    Detected,
    Manual,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageGridDefinition {
    pub kind: PageGridKind,
    pub origin: PdfPoint,
    pub spacing: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_degrees: f64,
    pub source: PageGridSource,
}

impl PageGridDefinition {
    pub fn new(
        kind: PageGridKind,
        origin: PdfPoint,
        spacing: f64,
        width: f64,
        height: f64,
        rotation_degrees: f64,
        source: PageGridSource,
    ) -> Result<Self, SemanticSnapError> {
        if !spacing.is_finite()
            || spacing <= 0.
            || !width.is_finite()
            || width <= 0.
            || !height.is_finite()
            || height <= 0.
            || !rotation_degrees.is_finite()
        {
            return Err(SemanticSnapError::InvalidPageGrid);
        }
        let grid = Self {
            kind,
            origin,
            spacing,
            width,
            height,
            rotation_degrees,
            source,
        };
        grid.candidate_count()?;
        Ok(grid)
    }

    fn candidate_count(&self) -> Result<usize, SemanticSnapError> {
        let rows = inclusive_grid_count(self.origin.y, self.height, self.vertical_spacing())?;
        let count = match self.kind {
            PageGridKind::Ruled => Some(rows),
            PageGridKind::Rectangular => {
                inclusive_grid_count(self.origin.x, self.width, self.spacing)?.checked_mul(rows)
            }
            PageGridKind::Isometric | PageGridKind::Triangle => {
                let columns = inclusive_grid_count(
                    self.origin.x - self.spacing * 0.5,
                    self.width,
                    self.spacing,
                )?;
                columns.checked_mul(rows)
            }
        }
        .ok_or(SemanticSnapError::PageGridCandidateLimitExceeded)?;
        if count > MAX_PAGE_GRID_CANDIDATES {
            return Err(SemanticSnapError::PageGridCandidateLimitExceeded);
        }
        Ok(count)
    }

    fn vertical_spacing(&self) -> f64 {
        match self.kind {
            PageGridKind::Isometric | PageGridKind::Triangle => self.spacing * 3_f64.sqrt() * 0.5,
            PageGridKind::Rectangular | PageGridKind::Ruled => self.spacing,
        }
    }
}

fn inclusive_grid_count(start: f64, end: f64, spacing: f64) -> Result<usize, SemanticSnapError> {
    if !start.is_finite() || !end.is_finite() || !spacing.is_finite() || spacing <= 0. {
        return Err(SemanticSnapError::InvalidPageGrid);
    }
    if start > end {
        return Ok(0);
    }
    let count = ((end - start) / spacing).floor() + 1.;
    if !count.is_finite() || count > MAX_PAGE_GRID_CANDIDATES as f64 {
        return Err(SemanticSnapError::PageGridCandidateLimitExceeded);
    }
    Ok(count as usize)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticSnapRole {
    GridPoint,
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
            SemanticSnapRole::GridPoint => true,
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
        source: SemanticSnapSource::ConstructionGrid,
        point_candidate: true,
        distance_window_px,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSnapDecision {
    pub point: PdfPoint,
    pub owner_id: Option<MarkupId>,
    pub role: SemanticSnapRole,
    pub source: SemanticSnapSource,
    pub point_candidate: bool,
    pub distance_window_px: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrthogonalAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AcquiredTrackingPoint {
    pub point: PdfPoint,
    pub source: SemanticSnapSource,
    pub role: SemanticSnapRole,
    pub owner_id: Option<MarkupId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectSnapTrackingGuide {
    pub origin: PdfPoint,
    pub axis: OrthogonalAxis,
    pub source: SemanticSnapSource,
    pub role: SemanticSnapRole,
    pub owner_id: Option<MarkupId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectSnapTrackingResult {
    pub point: PdfPoint,
    pub guides: Vec<ObjectSnapTrackingGuide>,
    pub distance_window_px: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnapGuideRect {
    pub owner_id: MarkupId,
    pub rect: crate::annotation_model::PdfRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EqualSpacingPlacement {
    Before,
    Between,
    After,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RelationshipSnapGuide {
    EqualSize {
        axis: OrthogonalAxis,
        moving: crate::annotation_model::PdfRect,
        reference: SnapGuideRect,
    },
    EqualSpacing {
        axis: OrthogonalAxis,
        placement: EqualSpacingPlacement,
        before: SnapGuideRect,
        moving: crate::annotation_model::PdfRect,
        after: SnapGuideRect,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EqualSizeSnapResult {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub guides: Vec<RelationshipSnapGuide>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EqualSpacingSnapResult {
    pub adjustment: PdfPoint,
    pub guides: Vec<RelationshipSnapGuide>,
}

/// Returns the same page-local axis-aligned geometry bounds used by stable
/// Electron relationship snapping. Preview/draft and excluded identities are
/// filtered by the ordinary annotation candidate builder, while measured
/// caption supplements remain paint-owned inputs.
pub fn annotation_guide_rects(
    scene: &AnnotationScene,
    excluded_owner_ids: &[MarkupId],
    supplement: &AnnotationSelectionSupplement,
) -> Vec<SnapGuideRect> {
    let index = SemanticSnapIndex::from_annotation_scene_with_selection_supplement(
        scene,
        excluded_owner_ids,
        supplement,
    );
    let mut bounds = HashMap::<MarkupId, (f64, f64, f64, f64)>::new();
    for candidate in index.candidates {
        let Some(owner_id) = candidate.owner_id else {
            continue;
        };
        let points = match candidate.geometry {
            CandidateGeometry::Point(point) => [point, point],
            CandidateGeometry::Segment { start, end } => [start, end],
        };
        let entry = bounds.entry(owner_id).or_insert((
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ));
        for point in points {
            entry.0 = entry.0.min(point.x);
            entry.1 = entry.1.min(point.y);
            entry.2 = entry.2.max(point.x);
            entry.3 = entry.3.max(point.y);
        }
    }

    scene
        .annotation_order
        .iter()
        .filter_map(|owner_id| {
            let (min_x, min_y, max_x, max_y) = bounds.remove(owner_id)?;
            Some(SnapGuideRect {
                owner_id: owner_id.clone(),
                rect: crate::annotation_model::PdfRect {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    height: max_y - min_y,
                },
            })
        })
        .collect()
}

pub fn combined_guide_bounds(points: &[PdfPoint]) -> Option<crate::annotation_model::PdfRect> {
    let first = *points.first()?;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first.x, first.y, first.x, first.y);
    for point in points.iter().copied().skip(1) {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    Some(crate::annotation_model::PdfRect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

const MAX_ACQUIRED_TRACKING_POINTS: usize = 4;

fn tracking_point_key(point: PdfPoint) -> (i64, i64) {
    (
        (point.x * 1_000.).round() as i64,
        (point.y * 1_000.).round() as i64,
    )
}

pub fn toggle_acquired_tracking_point(
    acquired: &[AcquiredTrackingPoint],
    candidate: AcquiredTrackingPoint,
) -> Vec<AcquiredTrackingPoint> {
    let key = tracking_point_key(candidate.point);
    if let Some(existing) = acquired
        .iter()
        .position(|point| tracking_point_key(point.point) == key)
    {
        return acquired
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != existing)
            .map(|(_, point)| point.clone())
            .collect();
    }

    let mut next = acquired.to_vec();
    next.push(candidate);
    if next.len() > MAX_ACQUIRED_TRACKING_POINTS {
        next.drain(..next.len() - MAX_ACQUIRED_TRACKING_POINTS);
    }
    next
}

pub fn find_object_snap_tracking_point(
    point: PdfPoint,
    acquired: &[AcquiredTrackingPoint],
    window_pixels_per_pdf_point: f64,
    tolerance_px: f64,
    allowed_axes: &[OrthogonalAxis],
) -> Option<ObjectSnapTrackingResult> {
    if acquired.is_empty()
        || !window_pixels_per_pdf_point.is_finite()
        || window_pixels_per_pdf_point <= 0.
        || !tolerance_px.is_finite()
        || tolerance_px < 0.
    {
        return None;
    }

    let mut horizontal: Option<(ObjectSnapTrackingGuide, f64)> = None;
    let mut vertical: Option<(ObjectSnapTrackingGuide, f64)> = None;
    for acquired in acquired {
        if allowed_axes.contains(&OrthogonalAxis::Horizontal) {
            let distance = (point.y - acquired.point.y).abs() * window_pixels_per_pdf_point;
            if distance <= tolerance_px
                && horizontal
                    .as_ref()
                    .is_none_or(|(_, current)| distance < *current)
            {
                horizontal = Some((
                    ObjectSnapTrackingGuide {
                        origin: acquired.point,
                        axis: OrthogonalAxis::Horizontal,
                        source: acquired.source,
                        role: acquired.role,
                        owner_id: acquired.owner_id.clone(),
                    },
                    distance,
                ));
            }
        }
        if allowed_axes.contains(&OrthogonalAxis::Vertical) {
            let distance = (point.x - acquired.point.x).abs() * window_pixels_per_pdf_point;
            if distance <= tolerance_px
                && vertical
                    .as_ref()
                    .is_none_or(|(_, current)| distance < *current)
            {
                vertical = Some((
                    ObjectSnapTrackingGuide {
                        origin: acquired.point,
                        axis: OrthogonalAxis::Vertical,
                        source: acquired.source,
                        role: acquired.role,
                        owner_id: acquired.owner_id.clone(),
                    },
                    distance,
                ));
            }
        }
    }

    if let (Some((horizontal_guide, _)), Some((vertical_guide, _))) = (&horizontal, &vertical)
        && tracking_point_key(horizontal_guide.origin) != tracking_point_key(vertical_guide.origin)
    {
        let tracked = PdfPoint {
            x: vertical_guide.origin.x,
            y: horizontal_guide.origin.y,
        };
        return Some(ObjectSnapTrackingResult {
            point: tracked,
            guides: vec![horizontal_guide.clone(), vertical_guide.clone()],
            distance_window_px: squared_distance(point, tracked).sqrt()
                * window_pixels_per_pdf_point,
        });
    }

    if let (Some((_, horizontal_distance)), Some((_, vertical_distance))) = (&horizontal, &vertical)
    {
        if horizontal_distance <= vertical_distance {
            vertical = None;
        } else {
            horizontal = None;
        }
    }
    if let Some((guide, distance_window_px)) = horizontal {
        return Some(ObjectSnapTrackingResult {
            point: PdfPoint {
                x: point.x,
                y: guide.origin.y,
            },
            guides: vec![guide],
            distance_window_px,
        });
    }
    vertical.map(|(guide, distance_window_px)| ObjectSnapTrackingResult {
        point: PdfPoint {
            x: guide.origin.x,
            y: point.y,
        },
        guides: vec![guide],
        distance_window_px,
    })
}

pub fn find_equal_size_snap(
    moving: crate::annotation_model::PdfRect,
    references: &[SnapGuideRect],
    window_pixels_per_pdf_point: f64,
    tolerance_px: f64,
) -> Option<EqualSizeSnapResult> {
    if !valid_relationship_inputs(window_pixels_per_pdf_point, tolerance_px) {
        return None;
    }
    let width = nearest_size_match(
        moving.width,
        references,
        OrthogonalAxis::Horizontal,
        window_pixels_per_pdf_point,
        tolerance_px,
    );
    let height = nearest_size_match(
        moving.height,
        references,
        OrthogonalAxis::Vertical,
        window_pixels_per_pdf_point,
        tolerance_px,
    );
    if width.is_none() && height.is_none() {
        return None;
    }
    let mut guides = Vec::new();
    if let Some(reference) = &width {
        guides.push(RelationshipSnapGuide::EqualSize {
            axis: OrthogonalAxis::Horizontal,
            moving,
            reference: reference.clone(),
        });
    }
    if let Some(reference) = &height {
        guides.push(RelationshipSnapGuide::EqualSize {
            axis: OrthogonalAxis::Vertical,
            moving,
            reference: reference.clone(),
        });
    }
    Some(EqualSizeSnapResult {
        width: width.map(|reference| reference.rect.width),
        height: height.map(|reference| reference.rect.height),
        guides,
    })
}

pub fn find_equal_spacing_snap(
    moving: crate::annotation_model::PdfRect,
    references: &[SnapGuideRect],
    window_pixels_per_pdf_point: f64,
    tolerance_px: f64,
) -> Option<EqualSpacingSnapResult> {
    if !valid_relationship_inputs(window_pixels_per_pdf_point, tolerance_px) {
        return None;
    }
    let horizontal = nearest_spacing_match(
        moving,
        references,
        OrthogonalAxis::Horizontal,
        window_pixels_per_pdf_point,
        tolerance_px,
    );
    let vertical = nearest_spacing_match(
        moving,
        references,
        OrthogonalAxis::Vertical,
        window_pixels_per_pdf_point,
        tolerance_px,
    );
    if horizontal.is_none() && vertical.is_none() {
        return None;
    }
    let adjustment = PdfPoint {
        x: horizontal.as_ref().map_or(0., |match_| match_.0),
        y: vertical.as_ref().map_or(0., |match_| match_.0),
    };
    let adjusted = crate::annotation_model::PdfRect {
        x: moving.x + adjustment.x,
        y: moving.y + adjustment.y,
        ..moving
    };
    let guides = horizontal
        .into_iter()
        .chain(vertical)
        .map(
            |(_, axis, placement, before, after)| RelationshipSnapGuide::EqualSpacing {
                axis,
                placement,
                before,
                moving: adjusted,
                after,
            },
        )
        .collect();
    Some(EqualSpacingSnapResult { adjustment, guides })
}

fn valid_relationship_inputs(window_pixels_per_pdf_point: f64, tolerance_px: f64) -> bool {
    window_pixels_per_pdf_point.is_finite()
        && window_pixels_per_pdf_point > 0.
        && tolerance_px.is_finite()
        && tolerance_px >= 0.
}

fn nearest_size_match(
    size: f64,
    references: &[SnapGuideRect],
    axis: OrthogonalAxis,
    window_pixels_per_pdf_point: f64,
    tolerance_px: f64,
) -> Option<SnapGuideRect> {
    let mut best: Option<(SnapGuideRect, f64)> = None;
    for reference in references {
        let reference_size = match axis {
            OrthogonalAxis::Horizontal => reference.rect.width,
            OrthogonalAxis::Vertical => reference.rect.height,
        };
        let distance = (size - reference_size).abs() * window_pixels_per_pdf_point;
        if distance <= tolerance_px && best.as_ref().is_none_or(|(_, current)| distance < *current)
        {
            best = Some((reference.clone(), distance));
        }
    }
    best.map(|(reference, _)| reference)
}

fn nearest_spacing_match(
    moving: crate::annotation_model::PdfRect,
    references: &[SnapGuideRect],
    axis: OrthogonalAxis,
    window_pixels_per_pdf_point: f64,
    tolerance_px: f64,
) -> Option<(
    f64,
    OrthogonalAxis,
    EqualSpacingPlacement,
    SnapGuideRect,
    SnapGuideRect,
)> {
    let (moving_start, moving_end, cross_start, cross_end) = match axis {
        OrthogonalAxis::Horizontal => (
            moving.x,
            moving.x + moving.width,
            moving.y,
            moving.y + moving.height,
        ),
        OrthogonalAxis::Vertical => (
            moving.y,
            moving.y + moving.height,
            moving.x,
            moving.x + moving.width,
        ),
    };
    let eligible = references.iter().filter(|reference| {
        let (reference_cross_start, reference_cross_end) = match axis {
            OrthogonalAxis::Horizontal => {
                (reference.rect.y, reference.rect.y + reference.rect.height)
            }
            OrthogonalAxis::Vertical => (reference.rect.x, reference.rect.x + reference.rect.width),
        };
        cross_end.min(reference_cross_end) >= cross_start.max(reference_cross_start)
    });
    let eligible = eligible.cloned().collect::<Vec<_>>();
    let mut best: Option<(
        f64,
        EqualSpacingPlacement,
        SnapGuideRect,
        SnapGuideRect,
        f64,
    )> = None;
    for before in &eligible {
        let (before_start, before_end) = match axis {
            OrthogonalAxis::Horizontal => (before.rect.x, before.rect.x + before.rect.width),
            OrthogonalAxis::Vertical => (before.rect.y, before.rect.y + before.rect.height),
        };
        for after in &eligible {
            let (after_start, after_end) = match axis {
                OrthogonalAxis::Horizontal => (after.rect.x, after.rect.x + after.rect.width),
                OrthogonalAxis::Vertical => (after.rect.y, after.rect.y + after.rect.height),
            };
            if after_start < before_end {
                continue;
            }
            let existing_gap = after_start - before_end;
            let mut candidates = Vec::with_capacity(3);
            if before_end <= moving_start && after_start >= moving_end {
                candidates.push((
                    EqualSpacingPlacement::Between,
                    ((after_start - moving_end) - (moving_start - before_end)) * 0.5,
                ));
            }
            if after_end <= moving_start {
                candidates.push((
                    EqualSpacingPlacement::After,
                    existing_gap - (moving_start - after_end),
                ));
            }
            if moving_end <= before_start {
                candidates.push((
                    EqualSpacingPlacement::Before,
                    (before_start - moving_end) - existing_gap,
                ));
            }
            for (placement, adjustment) in candidates {
                let distance = adjustment.abs() * window_pixels_per_pdf_point;
                if distance <= tolerance_px
                    && best
                        .as_ref()
                        .is_none_or(|(_, _, _, _, current)| distance < *current)
                {
                    best = Some((
                        adjustment,
                        placement,
                        before.clone(),
                        after.clone(),
                        distance,
                    ));
                }
            }
        }
    }
    best.map(|(adjustment, placement, before, after, _)| {
        (adjustment, axis, placement, before, after)
    })
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
    shared_indexes: Vec<Arc<SemanticSnapIndex>>,
}

impl SemanticSnapIndex {
    /// Appends application-owned candidates extracted from one page's PDF
    /// content stream. Intersections are derived within PDF content only, as
    /// in the stable Electron implementation; annotation/content edge pairs
    /// must not create synthetic cross-source targets.
    pub fn with_page_content(
        mut self,
        geometry: &PageSnapGeometry,
    ) -> Result<Self, SemanticSnapError> {
        const MAX_CONTENT_SNAP_CANDIDATES: usize = 500_000;
        let mut content = Vec::new();
        for primitive in &geometry.primitives {
            match primitive {
                PdfContentPrimitive::Line { start, end } => {
                    if !content_geometry_point_is_finite(*start)
                        || !content_geometry_point_is_finite(*end)
                    {
                        return Err(SemanticSnapError::InvalidContentGeometry);
                    }
                    add_content_open_path_candidates(
                        &mut content,
                        &[content_point(*start), content_point(*end)],
                    );
                }
                PdfContentPrimitive::Rect { rect } => {
                    if !rect.x.is_finite()
                        || !rect.y.is_finite()
                        || !rect.width.is_finite()
                        || !rect.height.is_finite()
                    {
                        return Err(SemanticSnapError::InvalidContentGeometry);
                    }
                    add_content_rectangle_candidates(&mut content, *rect);
                }
                PdfContentPrimitive::Polyline { points, closed } => {
                    if points
                        .iter()
                        .copied()
                        .any(|point| !content_geometry_point_is_finite(point))
                    {
                        return Err(SemanticSnapError::InvalidContentGeometry);
                    }
                    let points = points
                        .iter()
                        .copied()
                        .map(content_point)
                        .collect::<Vec<_>>();
                    if *closed {
                        add_content_closed_path_candidates(&mut content, &points);
                    } else {
                        add_content_open_path_candidates(&mut content, &points);
                    }
                }
            }
            if content.len() > MAX_CONTENT_SNAP_CANDIDATES {
                return Err(SemanticSnapError::ContentCandidateLimitExceeded);
            }
        }
        add_intersection_candidates_for_source(&mut content, SemanticSnapSource::Content);
        if content.len() > MAX_CONTENT_SNAP_CANDIDATES {
            return Err(SemanticSnapError::ContentCandidateLimitExceeded);
        }
        self.candidates.extend(content);
        Ok(self)
    }

    pub fn with_shared_index(mut self, index: Arc<SemanticSnapIndex>) -> Self {
        self.shared_indexes.push(index);
        self
    }

    pub fn with_page_grid(mut self, grid: &PageGridDefinition) -> Result<Self, SemanticSnapError> {
        grid.candidate_count()?;
        match grid.kind {
            PageGridKind::Ruled => {
                let rows = inclusive_grid_count(grid.origin.y, grid.height, grid.spacing)?;
                for row in 0..rows {
                    let y = grid.origin.y + row as f64 * grid.spacing;
                    if y >= 0. {
                        self.candidates.push(Candidate {
                            geometry: CandidateGeometry::Segment {
                                start: PdfPoint { x: 0., y },
                                end: PdfPoint { x: grid.width, y },
                            },
                            owner_id: None,
                            role: SemanticSnapRole::GridPoint,
                            source: SemanticSnapSource::PageGrid,
                        });
                    }
                }
            }
            PageGridKind::Rectangular => {
                let columns = inclusive_grid_count(grid.origin.x, grid.width, grid.spacing)?;
                let rows = inclusive_grid_count(grid.origin.y, grid.height, grid.spacing)?;
                for column in 0..columns {
                    let x = grid.origin.x + column as f64 * grid.spacing;
                    if x < 0. {
                        continue;
                    }
                    for row in 0..rows {
                        let y = grid.origin.y + row as f64 * grid.spacing;
                        if y >= 0. {
                            self.candidates.push(Candidate {
                                geometry: CandidateGeometry::Point(PdfPoint { x, y }),
                                owner_id: None,
                                role: SemanticSnapRole::GridPoint,
                                source: SemanticSnapSource::PageGrid,
                            });
                        }
                    }
                }
            }
            PageGridKind::Isometric | PageGridKind::Triangle => {
                let vertical_spacing = grid.vertical_spacing();
                let rows = inclusive_grid_count(grid.origin.y, grid.height, vertical_spacing)?;
                for row in 0..rows {
                    let y = grid.origin.y + row as f64 * vertical_spacing;
                    let offset = if row % 2 == 0 { 0. } else { grid.spacing * 0.5 };
                    let columns =
                        inclusive_grid_count(grid.origin.x + offset, grid.width, grid.spacing)?;
                    for column in 0..columns {
                        let x = grid.origin.x + offset + column as f64 * grid.spacing;
                        if x >= 0. && y >= 0. {
                            self.candidates.push(Candidate {
                                geometry: CandidateGeometry::Point(PdfPoint { x, y }),
                                owner_id: None,
                                role: SemanticSnapRole::GridPoint,
                                source: SemanticSnapSource::PageGrid,
                            });
                        }
                    }
                }
            }
        }
        Ok(self)
    }

    pub fn from_annotation_scene(scene: &AnnotationScene, excluded_owner_ids: &[MarkupId]) -> Self {
        Self::from_annotation_scene_with_selection_supplement(
            scene,
            excluded_owner_ids,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn from_annotation_scene_with_selection_supplement(
        scene: &AnnotationScene,
        excluded_owner_ids: &[MarkupId],
        supplement: &AnnotationSelectionSupplement,
    ) -> Self {
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
        for ellipse in &scene.ellipses {
            if ellipse.preview || excluded_owner_ids.contains(&ellipse.id) {
                continue;
            }
            add_rectangle_candidates(
                &mut candidates,
                ellipse.rect,
                ellipse.rotation_degrees,
                &ellipse.id,
            );
        }
        for redact in &scene.redacts {
            if redact.draft || excluded_owner_ids.contains(&redact.id) {
                continue;
            }
            add_rectangle_candidates(&mut candidates, redact.rect, 0., &redact.id);
        }
        for arc in &scene.arcs {
            if arc.draft || excluded_owner_ids.contains(&arc.id) {
                continue;
            }
            add_open_path_candidates(&mut candidates, &arc.sampled_path, &arc.id);
        }
        for path in &scene.vertex_paths {
            if path.draft || excluded_owner_ids.contains(&path.id) {
                continue;
            }
            if path.kind == crate::annotation_model::VertexPathKind::Polygon {
                add_closed_path_candidates(&mut candidates, &path.points, &path.id);
            } else {
                add_open_path_candidates(&mut candidates, &path.points, &path.id);
            }
        }
        for measurement in &scene.measurement_paths {
            if measurement.draft || excluded_owner_ids.contains(&measurement.id) {
                continue;
            }
            if measurement.kind == crate::annotation_model::MeasurementPathKind::Area {
                add_closed_path_candidates(&mut candidates, &measurement.points, &measurement.id);
            } else {
                add_open_path_candidates(&mut candidates, &measurement.points, &measurement.id);
            }
        }
        for text_box in &scene.text_boxes {
            if excluded_owner_ids.contains(&text_box.id) {
                continue;
            }
            add_rectangle_candidates(
                &mut candidates,
                text_box.layout_rect,
                text_box.rotation_degrees,
                &text_box.id,
            );
        }
        for image in &scene.images {
            if excluded_owner_ids.contains(&image.id) {
                continue;
            }
            add_rectangle_candidates(
                &mut candidates,
                image.rect,
                image.rotation_degrees,
                &image.id,
            );
        }
        for snapshot in &scene.snapshots {
            if snapshot.draft || excluded_owner_ids.contains(&snapshot.id) {
                continue;
            }
            add_rectangle_candidates(
                &mut candidates,
                snapshot.rect,
                snapshot.rotation_degrees,
                &snapshot.id,
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
        for cloud in &scene.clouds {
            if cloud.draft || excluded_owner_ids.contains(&cloud.id) {
                continue;
            }
            add_closed_path_candidates(&mut candidates, &cloud.points, &cloud.id);
        }
        for callout in &scene.callouts {
            if callout.draft || excluded_owner_ids.contains(&callout.id) {
                continue;
            }
            add_rectangle_candidates(&mut candidates, callout.text_box, 0., &callout.id);
            add_open_path_candidates(&mut candidates, &callout.leader_points, &callout.id);
        }
        for cloud_plus in &scene.cloud_pluses {
            if cloud_plus.draft || excluded_owner_ids.contains(&cloud_plus.id) {
                continue;
            }
            add_closed_path_candidates(&mut candidates, &cloud_plus.cloud_points, &cloud_plus.id);
            add_rectangle_candidates(&mut candidates, cloud_plus.text_box, 0., &cloud_plus.id);
            add_open_path_candidates(&mut candidates, &cloud_plus.leader_points, &cloud_plus.id);
        }
        for (owner_id, corners) in supplement {
            if excluded_owner_ids.contains(owner_id) {
                continue;
            }
            add_quadrilateral_candidates(&mut candidates, corners, owner_id);
        }
        add_intersection_candidates(&mut candidates);
        Self {
            candidates,
            shared_indexes: Vec::new(),
        }
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

        for candidate in self.candidates.iter().chain(
            self.shared_indexes
                .iter()
                .flat_map(|index| index.candidates.iter()),
        ) {
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
                source: candidate.source,
                point_candidate: matches!(candidate.geometry, CandidateGeometry::Point(_)),
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

/// Returns the same point anchors that annotation snapping exposes for a
/// moving selection. Segment projections and derived intersections are target
/// geometry, not stable anchors on the object being translated.
pub fn moving_annotation_snap_anchor_points(
    scene: &AnnotationScene,
    included_owner_ids: &[MarkupId],
    maximum: usize,
) -> Vec<PdfPoint> {
    moving_annotation_snap_anchor_points_with_selection_supplement(
        scene,
        included_owner_ids,
        maximum,
        &AnnotationSelectionSupplement::new(),
    )
}

pub fn moving_annotation_snap_anchor_points_with_selection_supplement(
    scene: &AnnotationScene,
    included_owner_ids: &[MarkupId],
    maximum: usize,
    supplement: &AnnotationSelectionSupplement,
) -> Vec<PdfPoint> {
    if maximum == 0 {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    for line in &scene.straight_lines {
        if !line.draft && included_owner_ids.contains(&line.id) {
            add_open_segment_candidates(&mut candidates, line.start, line.end, &line.id);
        }
    }
    for rectangle in &scene.rectangles {
        if !rectangle.preview && included_owner_ids.contains(&rectangle.id) {
            add_rectangle_candidates(
                &mut candidates,
                rectangle.rect,
                rectangle.rotation_degrees,
                &rectangle.id,
            );
        }
    }
    for ellipse in &scene.ellipses {
        if !ellipse.preview && included_owner_ids.contains(&ellipse.id) {
            add_rectangle_candidates(
                &mut candidates,
                ellipse.rect,
                ellipse.rotation_degrees,
                &ellipse.id,
            );
        }
    }
    for redact in &scene.redacts {
        if !redact.draft && included_owner_ids.contains(&redact.id) {
            add_rectangle_candidates(&mut candidates, redact.rect, 0., &redact.id);
        }
    }
    for arc in &scene.arcs {
        if !arc.draft && included_owner_ids.contains(&arc.id) {
            add_open_path_candidates(&mut candidates, &arc.sampled_path, &arc.id);
        }
    }
    for path in &scene.vertex_paths {
        if !path.draft && included_owner_ids.contains(&path.id) {
            if path.kind == crate::annotation_model::VertexPathKind::Polygon {
                add_closed_path_candidates(&mut candidates, &path.points, &path.id);
            } else {
                add_open_path_candidates(&mut candidates, &path.points, &path.id);
            }
        }
    }
    for measurement in &scene.measurement_paths {
        if !measurement.draft && included_owner_ids.contains(&measurement.id) {
            if measurement.kind == crate::annotation_model::MeasurementPathKind::Area {
                add_closed_path_candidates(&mut candidates, &measurement.points, &measurement.id);
            } else {
                add_open_path_candidates(&mut candidates, &measurement.points, &measurement.id);
            }
        }
    }
    for text_box in &scene.text_boxes {
        if included_owner_ids.contains(&text_box.id) {
            add_rectangle_candidates(
                &mut candidates,
                text_box.layout_rect,
                text_box.rotation_degrees,
                &text_box.id,
            );
        }
    }
    for image in &scene.images {
        if included_owner_ids.contains(&image.id) {
            add_rectangle_candidates(
                &mut candidates,
                image.rect,
                image.rotation_degrees,
                &image.id,
            );
        }
    }
    for snapshot in &scene.snapshots {
        if !snapshot.draft && included_owner_ids.contains(&snapshot.id) {
            add_rectangle_candidates(
                &mut candidates,
                snapshot.rect,
                snapshot.rotation_degrees,
                &snapshot.id,
            );
        }
    }
    for dimension in &scene.dimensions {
        if !dimension.draft && included_owner_ids.contains(&dimension.id) {
            add_open_segment_candidates(
                &mut candidates,
                dimension.start,
                dimension.end,
                &dimension.id,
            );
        }
    }
    for length in &scene.lengths {
        if included_owner_ids.contains(&length.id) {
            add_open_segment_candidates(&mut candidates, length.start, length.end, &length.id);
        }
    }
    for cloud in &scene.clouds {
        if !cloud.draft && included_owner_ids.contains(&cloud.id) {
            add_closed_path_candidates(&mut candidates, &cloud.points, &cloud.id);
        }
    }
    for callout in &scene.callouts {
        if !callout.draft && included_owner_ids.contains(&callout.id) {
            add_rectangle_candidates(&mut candidates, callout.text_box, 0., &callout.id);
            add_open_path_candidates(&mut candidates, &callout.leader_points, &callout.id);
        }
    }
    for cloud_plus in &scene.cloud_pluses {
        if !cloud_plus.draft && included_owner_ids.contains(&cloud_plus.id) {
            add_closed_path_candidates(&mut candidates, &cloud_plus.cloud_points, &cloud_plus.id);
            add_rectangle_candidates(&mut candidates, cloud_plus.text_box, 0., &cloud_plus.id);
            add_open_path_candidates(&mut candidates, &cloud_plus.leader_points, &cloud_plus.id);
        }
    }
    for (owner_id, corners) in supplement {
        if included_owner_ids.contains(owner_id) {
            add_quadrilateral_candidates(&mut candidates, corners, owner_id);
        }
    }

    let mut seen = std::collections::BTreeSet::new();
    let points = candidates
        .into_iter()
        .filter_map(|candidate| match candidate.geometry {
            CandidateGeometry::Point(point) if candidate.role != SemanticSnapRole::Intersection => {
                let key = (
                    (point.x * 1_000.).round() as i64,
                    (point.y * 1_000.).round() as i64,
                );
                seen.insert(key).then_some(point)
            }
            CandidateGeometry::Point(_) | CandidateGeometry::Segment { .. } => None,
        })
        .collect::<Vec<_>>();
    if points.len() <= maximum {
        return points;
    }
    (0..maximum)
        .map(|index| {
            let source_index =
                (index * (points.len() - 1) + (maximum - 1) / 2) / (maximum - 1).max(1);
            points[source_index]
        })
        .collect()
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
    add_intersection_candidates_for_source(candidates, SemanticSnapSource::Annotation);
}

fn add_intersection_candidates_for_source(
    candidates: &mut Vec<Candidate>,
    source: SemanticSnapSource,
) {
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
                    source,
                });
            }
        }
    }
    candidates.extend(intersections);
}

fn content_point(point: crate::pdf_content_geometry::PdfPoint) -> PdfPoint {
    PdfPoint {
        x: point.x,
        y: point.y,
    }
}

fn content_geometry_point_is_finite(point: crate::pdf_content_geometry::PdfPoint) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn add_content_rectangle_candidates(
    candidates: &mut Vec<Candidate>,
    rect: crate::pdf_content_geometry::PdfRect,
) {
    let x = rect.x.min(rect.x + rect.width);
    let y = rect.y.min(rect.y + rect.height);
    let width = rect.width.abs();
    let height = rect.height.abs();
    let center = PdfPoint {
        x: x + width * 0.5,
        y: y + height * 0.5,
    };
    let corners = [
        PdfPoint { x, y },
        PdfPoint { x: x + width, y },
        PdfPoint {
            x: x + width,
            y: y + height,
        },
        PdfPoint { x, y: y + height },
    ];
    candidates.push(content_candidate(center, SemanticSnapRole::Center));
    for point in corners {
        candidates.push(content_candidate(point, SemanticSnapRole::Endpoint));
    }
    add_content_segments(candidates, &corners, true);
}

fn add_content_open_path_candidates(candidates: &mut Vec<Candidate>, points: &[PdfPoint]) {
    if points.is_empty() {
        return;
    }
    for point in points.iter().copied() {
        candidates.push(content_candidate(point, SemanticSnapRole::Endpoint));
    }
    add_content_segments(candidates, points, false);
}

fn add_content_closed_path_candidates(candidates: &mut Vec<Candidate>, points: &[PdfPoint]) {
    if points.is_empty() {
        return;
    }
    for point in points.iter().copied() {
        candidates.push(content_candidate(point, SemanticSnapRole::Endpoint));
    }
    add_content_segments(candidates, points, true);
}

fn add_content_segments(candidates: &mut Vec<Candidate>, points: &[PdfPoint], closed: bool) {
    let segment_count = if closed {
        points.len()
    } else {
        points.len().saturating_sub(1)
    };
    for index in 0..segment_count {
        let start = points[index];
        let end = points[(index + 1) % points.len()];
        candidates.push(content_candidate(
            PdfPoint {
                x: (start.x + end.x) * 0.5,
                y: (start.y + end.y) * 0.5,
            },
            SemanticSnapRole::Midpoint,
        ));
        candidates.push(Candidate {
            geometry: CandidateGeometry::Segment { start, end },
            owner_id: None,
            role: SemanticSnapRole::Nearest,
            source: SemanticSnapSource::Content,
        });
    }
}

fn content_candidate(point: PdfPoint, role: SemanticSnapRole) -> Candidate {
    Candidate {
        geometry: CandidateGeometry::Point(point),
        owner_id: None,
        role,
        source: SemanticSnapSource::Content,
    }
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

    add_quadrilateral_candidates(candidates, &corners, owner_id);
}

fn add_quadrilateral_candidates(
    candidates: &mut Vec<Candidate>,
    corners: &[PdfPoint],
    owner_id: &MarkupId,
) {
    if corners.len() != 4
        || corners
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return;
    }
    let center = PdfPoint {
        x: corners.iter().map(|point| point.x).sum::<f64>() / 4.,
        y: corners.iter().map(|point| point.y).sum::<f64>() / 4.,
    };
    candidates.push(Candidate {
        geometry: CandidateGeometry::Point(center),
        owner_id: Some(owner_id.clone()),
        role: SemanticSnapRole::Center,
        source: SemanticSnapSource::Annotation,
    });
    for corner in corners.iter().copied() {
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

fn add_open_path_candidates(
    candidates: &mut Vec<Candidate>,
    points: &[PdfPoint],
    owner_id: &MarkupId,
) {
    if points.len() < 2 {
        return;
    }
    for point in points.iter().copied() {
        candidates.push(Candidate {
            geometry: CandidateGeometry::Point(point),
            owner_id: Some(owner_id.clone()),
            role: SemanticSnapRole::Endpoint,
            source: SemanticSnapSource::Annotation,
        });
    }
    for segment in points.windows(2) {
        let start = segment[0];
        let end = segment[1];
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

fn add_closed_path_candidates(
    candidates: &mut Vec<Candidate>,
    points: &[PdfPoint],
    owner_id: &MarkupId,
) {
    if points.len() < 2 {
        return;
    }
    for (index, start) in points.iter().copied().enumerate() {
        let end = points[(index + 1) % points.len()];
        candidates.push(Candidate {
            geometry: CandidateGeometry::Point(start),
            owner_id: Some(owner_id.clone()),
            role: SemanticSnapRole::Endpoint,
            source: SemanticSnapSource::Annotation,
        });
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
        SemanticSnapRole::GridPoint | SemanticSnapRole::Intersection => 0.,
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
    fn pdf_content_candidates_match_electron_roles_toggles_and_validation() {
        let geometry = PageSnapGeometry {
            page_index: 0,
            primitives: vec![
                PdfContentPrimitive::Rect {
                    rect: crate::pdf_content_geometry::PdfRect {
                        x: 100.,
                        y: 100.,
                        width: -20.,
                        height: 10.,
                    },
                },
                PdfContentPrimitive::Line {
                    start: crate::pdf_content_geometry::PdfPoint { x: 0., y: 0. },
                    end: crate::pdf_content_geometry::PdfPoint { x: 40., y: 40. },
                },
                PdfContentPrimitive::Line {
                    start: crate::pdf_content_geometry::PdfPoint { x: 0., y: 40. },
                    end: crate::pdf_content_geometry::PdfPoint { x: 40., y: 0. },
                },
                PdfContentPrimitive::Polyline {
                    points: vec![
                        crate::pdf_content_geometry::PdfPoint { x: 60., y: 0. },
                        crate::pdf_content_geometry::PdfPoint { x: 80., y: 0. },
                        crate::pdf_content_geometry::PdfPoint { x: 80., y: 20. },
                    ],
                    closed: false,
                },
                PdfContentPrimitive::Polyline {
                    points: vec![
                        crate::pdf_content_geometry::PdfPoint { x: 60., y: 60. },
                        crate::pdf_content_geometry::PdfPoint { x: 80., y: 60. },
                        crate::pdf_content_geometry::PdfPoint { x: 70., y: 80. },
                    ],
                    closed: true,
                },
            ],
        };
        let index = SemanticSnapIndex::default()
            .with_page_content(&geometry)
            .unwrap();
        let defaults = SemanticSnapSettings::default();
        assert_eq!(
            index
                .resolve_point(PdfPoint { x: 20., y: 20. }, &defaults, 1.)
                .unwrap()
                .role,
            SemanticSnapRole::Intersection
        );
        let endpoint_only = defaults
            .with_target(SemanticSnapTarget::Midpoint, false)
            .with_target(SemanticSnapTarget::Center, false)
            .with_target(SemanticSnapTarget::Intersection, false)
            .with_target(SemanticSnapTarget::Nearest, false);
        assert_eq!(
            index
                .resolve_point(PdfPoint { x: 60., y: 0. }, &endpoint_only, 1.)
                .unwrap()
                .role,
            SemanticSnapRole::Endpoint
        );
        let center_only = endpoint_only
            .with_target(SemanticSnapTarget::Endpoint, false)
            .with_target(SemanticSnapTarget::Center, true);
        assert_eq!(
            index
                .resolve_point(PdfPoint { x: 90., y: 105. }, &center_only, 1.)
                .unwrap()
                .role,
            SemanticSnapRole::Center
        );
        assert!(
            index
                .resolve_point(
                    PdfPoint { x: 20., y: 20. },
                    &defaults.with_source(SemanticSnapSource::Content, false),
                    1.,
                )
                .is_none()
        );

        let malformed = PageSnapGeometry {
            page_index: 0,
            primitives: vec![PdfContentPrimitive::Line {
                start: crate::pdf_content_geometry::PdfPoint { x: f64::NAN, y: 0. },
                end: crate::pdf_content_geometry::PdfPoint { x: 1., y: 1. },
            }],
        };
        assert!(matches!(
            SemanticSnapIndex::default().with_page_content(&malformed),
            Err(SemanticSnapError::InvalidContentGeometry)
        ));
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
    fn measured_caption_supplement_exposes_rectangle_targets_and_moving_anchors() {
        let owner = MarkupId::new("caption:snap-owner").unwrap();
        let corners = vec![
            PdfPoint { x: 10., y: 10. },
            PdfPoint { x: 30., y: 10. },
            PdfPoint { x: 30., y: 20. },
            PdfPoint { x: 10., y: 20. },
        ];
        let supplement = [(owner.clone(), corners.clone())]
            .into_iter()
            .collect::<AnnotationSelectionSupplement>();
        let scene = crate::annotation_model::AnnotationDocument::default().document_scene(0);
        let settings =
            SemanticSnapSettings::default().with_target(SemanticSnapTarget::Nearest, true);
        let index = SemanticSnapIndex::from_annotation_scene_with_selection_supplement(
            &scene,
            &[],
            &supplement,
        );

        for (query, expected, role) in [
            (
                PdfPoint { x: 10.1, y: 10.1 },
                PdfPoint { x: 10., y: 10. },
                SemanticSnapRole::Endpoint,
            ),
            (
                PdfPoint { x: 20.1, y: 10.1 },
                PdfPoint { x: 20., y: 10. },
                SemanticSnapRole::Midpoint,
            ),
            (
                PdfPoint { x: 20.1, y: 15.1 },
                PdfPoint { x: 20., y: 15. },
                SemanticSnapRole::Center,
            ),
            (
                PdfPoint { x: 16., y: 10.1 },
                PdfPoint { x: 16., y: 10. },
                SemanticSnapRole::Nearest,
            ),
        ] {
            let decision = index.resolve_point(query, &settings, 1.).unwrap();
            assert_eq!(decision.point, expected);
            assert_eq!(decision.owner_id.as_ref(), Some(&owner));
            assert_eq!(decision.role, role);
        }

        let anchors = moving_annotation_snap_anchor_points_with_selection_supplement(
            &scene,
            std::slice::from_ref(&owner),
            128,
            &supplement,
        );
        assert_eq!(anchors.len(), 9);
        assert!(anchors.contains(&PdfPoint { x: 20., y: 15. }));
        assert!(anchors.iter().all(|point| point.x >= 10. && point.x <= 30.));

        let excluded = SemanticSnapIndex::from_annotation_scene_with_selection_supplement(
            &scene,
            std::slice::from_ref(&owner),
            &supplement,
        );
        assert!(
            excluded
                .resolve_point(PdfPoint { x: 10.1, y: 10.1 }, &settings, 1.)
                .is_none()
        );
        assert!(
            moving_annotation_snap_anchor_points_with_selection_supplement(
                &scene,
                &[],
                128,
                &supplement,
            )
            .is_empty(),
            "a missing or wrong-page owner must preserve segment-only behaviour"
        );
    }

    #[test]
    fn ellipse_and_redact_expose_rectangle_targets_anchors_and_owner_exclusion() {
        use crate::annotation_model::{
            Annotation, AnnotationCommand, AnnotationDocument, EllipseAnnotation, PdfRect,
            RectangleAppearance, RedactAnnotation,
        };

        let ellipse_id = MarkupId::new("ellipse:semantic-geometry").unwrap();
        let redact_id = MarkupId::new("redact:semantic-geometry").unwrap();
        let mut ellipse = EllipseAnnotation::new(
            ellipse_id.clone(),
            0,
            PdfRect::new(10., 20., 20., 10.).unwrap(),
            RectangleAppearance::default(),
        )
        .unwrap();
        ellipse.rotation_degrees = 90.;
        let redact_appearance = RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
            .unwrap()
            .with_fill_opacity(0.35)
            .unwrap();
        let redact = RedactAnnotation::new(
            redact_id.clone(),
            0,
            PdfRect::new(50., 60., 20., 10.).unwrap(),
            "#000000",
            None::<String>,
            redact_appearance,
        )
        .unwrap();
        let mut document = AnnotationDocument::default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Ellipse(
                ellipse,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Redact(
                redact,
            )))
            .unwrap();
        let scene = document.document_scene(0);
        let settings = SemanticSnapSettings::default();
        let index = SemanticSnapIndex::from_annotation_scene(&scene, &[]);

        for (query, expected, owner, role) in [
            (
                PdfPoint { x: 25.1, y: 15.1 },
                PdfPoint { x: 25., y: 15. },
                &ellipse_id,
                SemanticSnapRole::Endpoint,
            ),
            (
                PdfPoint { x: 20.1, y: 25.1 },
                PdfPoint { x: 20., y: 25. },
                &ellipse_id,
                SemanticSnapRole::Center,
            ),
            (
                PdfPoint { x: 50.1, y: 65.1 },
                PdfPoint { x: 50., y: 65. },
                &redact_id,
                SemanticSnapRole::Midpoint,
            ),
            (
                PdfPoint { x: 60.1, y: 65.1 },
                PdfPoint { x: 60., y: 65. },
                &redact_id,
                SemanticSnapRole::Center,
            ),
        ] {
            let decision = index.resolve_point(query, &settings, 1.).unwrap();
            assert_eq!(decision.point, expected);
            assert_eq!(decision.owner_id.as_ref(), Some(owner));
            assert_eq!(decision.role, role);
        }

        let anchors = moving_annotation_snap_anchor_points(
            &scene,
            &[ellipse_id.clone(), redact_id.clone()],
            128,
        );
        assert_eq!(anchors.len(), 18);
        assert!(anchors.contains(&PdfPoint { x: 25., y: 15. }));
        assert!(anchors.contains(&PdfPoint { x: 60., y: 65. }));

        let excluded = SemanticSnapIndex::from_annotation_scene(
            &scene,
            &[ellipse_id.clone(), redact_id.clone()],
        );
        assert!(
            excluded
                .resolve_point(PdfPoint { x: 25.1, y: 15.1 }, &settings, 1.)
                .is_none()
        );
    }

    #[test]
    fn rotated_text_image_and_snapshot_use_presented_rectangle_snap_geometry() {
        use crate::annotation_model::{
            Annotation, AnnotationCommand, AnnotationDocument, DecodedRgbaAsset, ImageAnnotation,
            PdfRect, SnapshotAnnotation, TextBoxAnnotation, TextBoxStyle,
        };

        let text_id = MarkupId::new("text-box:rotated-semantic-geometry").unwrap();
        let image_id = MarkupId::new("image:rotated-semantic-geometry").unwrap();
        let snapshot_id = MarkupId::new("snapshot:rotated-semantic-geometry").unwrap();
        let text = TextBoxAnnotation::new(
            text_id.clone(),
            0,
            PdfRect::new(10., 20., 20., 10.).unwrap(),
            "Text",
            TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
        )
        .unwrap()
        .with_rotation_degrees(90.)
        .unwrap();
        let image = ImageAnnotation::new(
            image_id.clone(),
            0,
            PdfRect::new(50., 60., 20., 10.).unwrap(),
            DecodedRgbaAsset::new(2, 1, vec![255; 8]).unwrap(),
            false,
        )
        .unwrap()
        .with_rotation_degrees(90.)
        .unwrap();
        let snapshot = SnapshotAnnotation::new(
            snapshot_id.clone(),
            0,
            PdfRect::new(90., 100., 20., 10.).unwrap(),
            DecodedRgbaAsset::new(2, 1, vec![255; 8]).unwrap(),
            1.,
        )
        .unwrap()
        .with_rotation_degrees(90.)
        .unwrap();
        let mut document = AnnotationDocument::default();
        for annotation in [
            Annotation::TextBox(text),
            Annotation::Image(image),
            Annotation::Snapshot(snapshot),
        ] {
            document
                .apply_command(AnnotationCommand::CreateAnnotation(annotation))
                .unwrap();
        }
        let scene = document.document_scene(0);
        let settings = SemanticSnapSettings::default();
        let index = SemanticSnapIndex::from_annotation_scene(&scene, &[]);

        for (query, expected, owner) in [
            (
                PdfPoint { x: 15.1, y: 15.1 },
                PdfPoint { x: 15., y: 15. },
                &text_id,
            ),
            (
                PdfPoint { x: 55.1, y: 55.1 },
                PdfPoint { x: 55., y: 55. },
                &image_id,
            ),
            (
                PdfPoint { x: 95.1, y: 95.1 },
                PdfPoint { x: 95., y: 95. },
                &snapshot_id,
            ),
        ] {
            let decision = index.resolve_point(query, &settings, 1.).unwrap();
            assert_eq!(decision.point, expected);
            assert_eq!(decision.owner_id.as_ref(), Some(owner));
            assert_eq!(decision.role, SemanticSnapRole::Endpoint);
        }

        let anchors = moving_annotation_snap_anchor_points(
            &scene,
            &[text_id.clone(), image_id.clone(), snapshot_id.clone()],
            128,
        );
        assert_eq!(anchors.len(), 27);
        assert!(anchors.contains(&PdfPoint { x: 15., y: 15. }));
        assert!(anchors.contains(&PdfPoint { x: 55., y: 55. }));
        assert!(anchors.contains(&PdfPoint { x: 95., y: 95. }));

        let excluded = SemanticSnapIndex::from_annotation_scene(
            &scene,
            &[text_id.clone(), image_id.clone(), snapshot_id.clone()],
        );
        for point in [
            PdfPoint { x: 15.1, y: 15.1 },
            PdfPoint { x: 55.1, y: 55.1 },
            PdfPoint { x: 95.1, y: 95.1 },
        ] {
            assert!(excluded.resolve_point(point, &settings, 1.).is_none());
        }
    }

    #[test]
    fn vertex_and_measurement_paths_expose_complete_open_and_closed_snap_geometry() {
        use crate::annotation_model::{
            Annotation, AnnotationCommand, AnnotationDocument, LengthCalibration,
            MeasurementPathAnnotation, MeasurementPathKind, RectangleAppearance,
            VertexPathAnnotation, VertexPathKind,
        };

        let polyline_id = MarkupId::new("polyline:semantic-geometry").unwrap();
        let polygon_id = MarkupId::new("polygon:semantic-geometry").unwrap();
        let polylength_id = MarkupId::new("polylength:semantic-geometry").unwrap();
        let area_id = MarkupId::new("area:semantic-geometry").unwrap();
        let calibration = LengthCalibration::new(1., "mm", "Scale", true).unwrap();
        let mut document = AnnotationDocument::default();
        for annotation in [
            Annotation::VertexPath(
                VertexPathAnnotation::new(
                    polyline_id.clone(),
                    0,
                    vec![
                        PdfPoint { x: 10., y: 10. },
                        PdfPoint { x: 30., y: 10. },
                        PdfPoint { x: 30., y: 30. },
                    ],
                    VertexPathKind::Polyline,
                    RectangleAppearance::default(),
                )
                .unwrap(),
            ),
            Annotation::VertexPath(
                VertexPathAnnotation::new(
                    polygon_id.clone(),
                    0,
                    vec![
                        PdfPoint { x: 50., y: 50. },
                        PdfPoint { x: 70., y: 50. },
                        PdfPoint { x: 70., y: 70. },
                    ],
                    VertexPathKind::Polygon,
                    RectangleAppearance::default(),
                )
                .unwrap(),
            ),
            Annotation::MeasurementPath(
                MeasurementPathAnnotation::new(
                    polylength_id.clone(),
                    0,
                    vec![
                        PdfPoint { x: 90., y: 90. },
                        PdfPoint { x: 110., y: 90. },
                        PdfPoint { x: 110., y: 110. },
                    ],
                    MeasurementPathKind::Polylength,
                    calibration.clone(),
                    RectangleAppearance::default(),
                )
                .unwrap(),
            ),
            Annotation::MeasurementPath(
                MeasurementPathAnnotation::new(
                    area_id.clone(),
                    0,
                    vec![
                        PdfPoint { x: 130., y: 130. },
                        PdfPoint { x: 150., y: 130. },
                        PdfPoint { x: 150., y: 150. },
                    ],
                    MeasurementPathKind::Area,
                    calibration,
                    RectangleAppearance::default(),
                )
                .unwrap(),
            ),
        ] {
            document
                .apply_command(AnnotationCommand::CreateAnnotation(annotation))
                .unwrap();
        }
        let scene = document.document_scene(0);
        let settings = SemanticSnapSettings::default();
        let index = SemanticSnapIndex::from_annotation_scene(&scene, &[]);

        for (query, expected, owner, role) in [
            (
                PdfPoint { x: 20.1, y: 10.1 },
                PdfPoint { x: 20., y: 10. },
                &polyline_id,
                SemanticSnapRole::Midpoint,
            ),
            (
                PdfPoint { x: 60.1, y: 60.1 },
                PdfPoint { x: 60., y: 60. },
                &polygon_id,
                SemanticSnapRole::Midpoint,
            ),
            (
                PdfPoint { x: 110.1, y: 100.1 },
                PdfPoint { x: 110., y: 100. },
                &polylength_id,
                SemanticSnapRole::Midpoint,
            ),
            (
                PdfPoint { x: 140.1, y: 140.1 },
                PdfPoint { x: 140., y: 140. },
                &area_id,
                SemanticSnapRole::Midpoint,
            ),
        ] {
            let decision = index.resolve_point(query, &settings, 1.).unwrap();
            assert_eq!(decision.point, expected);
            assert_eq!(decision.owner_id.as_ref(), Some(owner));
            assert_eq!(decision.role, role);
        }

        let owners = [
            polyline_id.clone(),
            polygon_id.clone(),
            polylength_id.clone(),
            area_id.clone(),
        ];
        let anchors = moving_annotation_snap_anchor_points(&scene, &owners, 128);
        assert_eq!(anchors.len(), 22);
        for expected in [
            PdfPoint { x: 20., y: 10. },
            PdfPoint { x: 60., y: 60. },
            PdfPoint { x: 110., y: 100. },
            PdfPoint { x: 140., y: 140. },
        ] {
            assert!(anchors.contains(&expected));
        }
        let excluded = SemanticSnapIndex::from_annotation_scene(&scene, &owners);
        assert!(
            excluded
                .resolve_point(PdfPoint { x: 20.1, y: 10.1 }, &settings, 1.)
                .is_none()
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
    fn page_grid_candidates_match_electron_geometry_and_fail_closed_when_unbounded() {
        let settings = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, true);
        let grid = PageGridDefinition::new(
            PageGridKind::Rectangular,
            PdfPoint { x: 0., y: 0. },
            10.,
            20.,
            20.,
            0.,
            PageGridSource::Generated,
        )
        .unwrap();
        let index = SemanticSnapIndex::default().with_page_grid(&grid).unwrap();
        assert_eq!(index.candidates.len(), 9);
        let decision = index
            .resolve_point(PdfPoint { x: 10.2, y: 9.8 }, &settings, 1.)
            .unwrap();
        assert_eq!(decision.point, PdfPoint { x: 10., y: 10. });
        assert_eq!(decision.role, SemanticSnapRole::GridPoint);
        assert!(
            index
                .resolve_point(
                    PdfPoint { x: 10.2, y: 9.8 },
                    &settings.with_target(SemanticSnapTarget::Intersection, false),
                    1.,
                )
                .is_some()
        );
        assert!(
            index
                .resolve_point(
                    PdfPoint { x: 10.2, y: 9.8 },
                    &settings.with_source(SemanticSnapSource::PageGrid, false),
                    1.,
                )
                .is_none()
        );

        let ruled = PageGridDefinition::new(
            PageGridKind::Ruled,
            PdfPoint { x: 0., y: 0. },
            10.,
            20.,
            20.,
            0.,
            PageGridSource::Generated,
        )
        .unwrap();
        let ruled_index = SemanticSnapIndex::default().with_page_grid(&ruled).unwrap();
        assert_eq!(ruled_index.candidates.len(), 3);
        assert_eq!(
            ruled_index
                .resolve_point(
                    PdfPoint { x: 7., y: 10.2 },
                    &settings.with_target(SemanticSnapTarget::Nearest, false),
                    1.,
                )
                .unwrap()
                .point,
            PdfPoint { x: 7., y: 10. }
        );

        assert_eq!(
            PageGridDefinition::new(
                PageGridKind::Rectangular,
                PdfPoint { x: 0., y: 0. },
                0.001,
                1_000.,
                1_000.,
                0.,
                PageGridSource::Detected,
            ),
            Err(SemanticSnapError::PageGridCandidateLimitExceeded)
        );
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

        let target_toggle_off = settings.with_target(SemanticSnapTarget::Intersection, false);
        assert!(
            resolve_construction_grid_point(
                PdfPoint { x: 0.2, y: 0.2 },
                100.,
                100.,
                &target_toggle_off,
                1.,
            )
            .is_some()
        );

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

    #[test]
    fn arc_sampled_path_exposes_targets_anchors_and_owner_exclusion() {
        use crate::annotation_model::{
            Annotation, AnnotationCommand, AnnotationDocument, ArcAnnotation, RectangleAppearance,
        };

        let arc_id = MarkupId::new("arc:semantic-geometry").unwrap();
        let arc = ArcAnnotation::new(
            arc_id.clone(),
            0,
            PdfPoint { x: 10., y: 10. },
            PdfPoint { x: 50., y: 10. },
            PdfPoint { x: 30., y: 30. },
            RectangleAppearance::default(),
        )
        .unwrap();
        let sampled = arc.sampled_path(64);
        let target = sampled[16];
        let mut document = AnnotationDocument::default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Arc(arc)))
            .unwrap();
        let scene = document.document_scene(0);
        let settings = SemanticSnapSettings::default();
        let index = SemanticSnapIndex::from_annotation_scene(&scene, &[]);
        assert_eq!(
            index
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.owner_id.as_ref() == Some(&arc_id)
                        && candidate.role == SemanticSnapRole::Endpoint
                })
                .count(),
            sampled.len()
        );
        assert_eq!(
            index
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.owner_id.as_ref() == Some(&arc_id)
                        && candidate.role == SemanticSnapRole::Midpoint
                })
                .count(),
            sampled.len() - 1
        );
        assert_eq!(
            index
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.owner_id.as_ref() == Some(&arc_id)
                        && candidate.role == SemanticSnapRole::Nearest
                })
                .count(),
            sampled.len() - 1
        );
        assert!(!index.candidates.iter().any(|candidate| {
            candidate.owner_id.as_ref() == Some(&arc_id)
                && candidate.role == SemanticSnapRole::Center
        }));
        let decision = index
            .resolve_point(
                PdfPoint {
                    x: target.x + 0.2,
                    y: target.y - 0.2,
                },
                &settings,
                1.,
            )
            .unwrap();
        assert_eq!(decision.point, target);
        assert_eq!(decision.owner_id.as_ref(), Some(&arc_id));

        let midpoint = PdfPoint {
            x: (sampled[16].x + sampled[17].x) * 0.5,
            y: (sampled[16].y + sampled[17].y) * 0.5,
        };
        let midpoint_only = settings
            .with_target(SemanticSnapTarget::Endpoint, false)
            .with_target(SemanticSnapTarget::Center, false)
            .with_target(SemanticSnapTarget::Intersection, false)
            .with_target(SemanticSnapTarget::Nearest, false);
        assert_eq!(
            index
                .resolve_point(midpoint, &midpoint_only, 1.)
                .unwrap()
                .role,
            SemanticSnapRole::Midpoint
        );
        let edge_only = midpoint_only
            .with_target(SemanticSnapTarget::Midpoint, false)
            .with_target(SemanticSnapTarget::Nearest, true);
        let quarter = PdfPoint {
            x: sampled[16].x * 0.75 + sampled[17].x * 0.25,
            y: sampled[16].y * 0.75 + sampled[17].y * 0.25,
        };
        assert_eq!(
            index.resolve_point(quarter, &edge_only, 1.).unwrap().role,
            SemanticSnapRole::Nearest
        );

        let anchors =
            moving_annotation_snap_anchor_points(&scene, std::slice::from_ref(&arc_id), 128);
        assert_eq!(anchors.len(), 128);
        assert!(anchors.contains(&target));
        assert!(
            SemanticSnapIndex::from_annotation_scene(&scene, &[arc_id])
                .resolve_point(target, &settings, 1.)
                .is_none()
        );
    }

    fn tracking_point(x: f64, y: f64, role: SemanticSnapRole) -> AcquiredTrackingPoint {
        AcquiredTrackingPoint {
            point: PdfPoint { x, y },
            source: SemanticSnapSource::Annotation,
            role,
            owner_id: Some(MarkupId::new(format!("owner-{x}-{y}")).unwrap()),
        }
    }

    fn guide_rect(owner: &str, x: f64, y: f64, width: f64, height: f64) -> SnapGuideRect {
        SnapGuideRect {
            owner_id: MarkupId::new(owner).unwrap(),
            rect: crate::annotation_model::PdfRect::new(x, y, width, height).unwrap(),
        }
    }

    #[test]
    fn tracking_acquisition_toggles_revisited_points_and_keeps_four_recent_points() {
        let first = tracking_point(10., 20., SemanticSnapRole::Endpoint);
        let second = tracking_point(30., 40., SemanticSnapRole::Midpoint);
        assert_eq!(
            toggle_acquired_tracking_point(&[], first.clone()),
            vec![first.clone()]
        );
        assert_eq!(
            toggle_acquired_tracking_point(&[first.clone(), second.clone()], first),
            vec![second]
        );

        let acquired =
            [10., 20., 30., 40.].map(|x| tracking_point(x, 10., SemanticSnapRole::Endpoint));
        let recent = toggle_acquired_tracking_point(
            &acquired,
            tracking_point(50., 10., SemanticSnapRole::Endpoint),
        );
        assert_eq!(
            recent.iter().map(|point| point.point.x).collect::<Vec<_>>(),
            vec![20., 30., 40., 50.]
        );
    }

    #[test]
    fn tracking_resolves_viewport_tolerance_virtual_intersection_and_axis_limit() {
        let acquired = vec![
            tracking_point(20., 30., SemanticSnapRole::Endpoint),
            tracking_point(80., 90., SemanticSnapRole::Midpoint),
        ];
        let result = find_object_snap_tracking_point(
            PdfPoint { x: 78., y: 33. },
            &acquired,
            1.,
            5.,
            &[OrthogonalAxis::Horizontal, OrthogonalAxis::Vertical],
        )
        .unwrap();
        assert_eq!(result.point, PdfPoint { x: 80., y: 30. });
        assert_eq!(result.guides.len(), 2);
        assert_eq!(result.distance_window_px, 13_f64.sqrt());

        let vertical_only = find_object_snap_tracking_point(
            PdfPoint { x: 78., y: 30. },
            &acquired[1..],
            1.,
            5.,
            &[OrthogonalAxis::Vertical],
        )
        .unwrap();
        assert_eq!(vertical_only.point, PdfPoint { x: 80., y: 30. });
        assert_eq!(vertical_only.guides[0].axis, OrthogonalAxis::Vertical);
        assert!(
            find_object_snap_tracking_point(
                PdfPoint { x: 80., y: 34. },
                &acquired[..1],
                2.,
                5.,
                &[OrthogonalAxis::Horizontal, OrthogonalAxis::Vertical],
            )
            .is_none()
        );
    }

    #[test]
    fn equal_size_snaps_width_and_height_independently_in_viewport_pixels() {
        let moving = crate::annotation_model::PdfRect::new(10., 10., 77., 58.).unwrap();
        let references = vec![
            guide_rect("wide", 100., 20., 80., 30.),
            guide_rect("tall", 20., 100., 40., 60.),
        ];
        let result = find_equal_size_snap(moving, &references, 2., 6.).unwrap();
        assert_eq!((result.width, result.height), (Some(80.), Some(60.)));
        assert!(matches!(
            &result.guides[..],
            [
                RelationshipSnapGuide::EqualSize { axis: OrthogonalAxis::Horizontal, reference: first, .. },
                RelationshipSnapGuide::EqualSize { axis: OrthogonalAxis::Vertical, reference: second, .. }
            ] if first.owner_id.as_str() == "wide" && second.owner_id.as_str() == "tall"
        ));
    }

    #[test]
    fn equal_spacing_snaps_between_and_extends_pairs_but_requires_cross_axis_overlap() {
        let horizontal = vec![
            guide_rect("before", 10., 20., 20., 20.),
            guide_rect("after", 100., 20., 20., 20.),
        ];
        let between = find_equal_spacing_snap(
            crate::annotation_model::PdfRect::new(56., 20., 20., 20.).unwrap(),
            &horizontal,
            1.,
            8.,
        )
        .unwrap();
        assert_eq!(between.adjustment, PdfPoint { x: -1., y: 0. });
        assert!(matches!(
            &between.guides[..],
            [RelationshipSnapGuide::EqualSpacing {
                axis: OrthogonalAxis::Horizontal,
                placement: EqualSpacingPlacement::Between,
                moving,
                ..
            }] if *moving == crate::annotation_model::PdfRect::new(55., 20., 20., 20.).unwrap()
        ));
        assert!(
            find_equal_spacing_snap(
                crate::annotation_model::PdfRect::new(56., 80., 20., 20.).unwrap(),
                &horizontal,
                1.,
                8.,
            )
            .is_none()
        );

        let pair = vec![
            guide_rect("first", 20., 20., 20., 20.),
            guide_rect("second", 60., 20., 20., 20.),
        ];
        for (x, expected, placement) in [
            (-17., -3., EqualSpacingPlacement::Before),
            (97., 3., EqualSpacingPlacement::After),
        ] {
            let result = find_equal_spacing_snap(
                crate::annotation_model::PdfRect::new(x, 20., 20., 20.).unwrap(),
                &pair,
                1.,
                5.,
            )
            .unwrap();
            assert_eq!(result.adjustment.x, expected);
            assert!(matches!(
                result.guides[0],
                RelationshipSnapGuide::EqualSpacing { placement: actual, .. } if actual == placement
            ));
        }

        let vertical = vec![
            guide_rect("below", 20., 10., 20., 20.),
            guide_rect("above", 20., 100., 20., 20.),
        ];
        let result = find_equal_spacing_snap(
            crate::annotation_model::PdfRect::new(20., 56., 20., 20.).unwrap(),
            &vertical,
            1.,
            8.,
        )
        .unwrap();
        assert_eq!(result.adjustment, PdfPoint { x: 0., y: -1. });
        assert!(matches!(
            result.guides[0],
            RelationshipSnapGuide::EqualSpacing {
                axis: OrthogonalAxis::Vertical,
                ..
            }
        ));
    }
}
