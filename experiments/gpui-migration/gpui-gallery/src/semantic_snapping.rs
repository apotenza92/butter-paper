//! Application-owned semantic snapping for PDF annotation geometry.
//!
//! The module is GPUI-free. It indexes immutable scene geometry and returns a
//! decision; callers retain gesture state, mutate annotations, and paint any
//! transient guide evidence.

use crate::annotation_model::{AnnotationScene, MarkupId, PdfPoint};

const DEFAULT_SENSITIVITY_WINDOW_PX: f64 = 8.;

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
    annotations_enabled: bool,
    sensitivity_window_px: f64,
    endpoint: bool,
    midpoint: bool,
    center: bool,
    intersection: bool,
    nearest: bool,
}

impl SemanticSnapSettings {
    pub fn annotations_enabled(self) -> bool {
        self.annotations_enabled
    }

    pub fn with_annotation_source(mut self, enabled: bool) -> Self {
        self.annotations_enabled = enabled;
        self
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
            annotations_enabled: true,
            sensitivity_window_px: DEFAULT_SENSITIVITY_WINDOW_PX,
            endpoint: true,
            midpoint: true,
            center: true,
            intersection: true,
            nearest: false,
        }
    }
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
            add_open_segment_candidates(
                &mut candidates,
                length.start,
                length.end,
                &length.id,
            );
        }
        add_intersection_candidates(&mut candidates);
        Self { candidates }
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
        if !settings.annotations_enabled() {
            return None;
        }
        if !window_pixels_per_pdf_point.is_finite() || window_pixels_per_pdf_point <= 0. {
            return None;
        }
        let constraint = orthogonal_anchor.map(|anchor| OrthogonalConstraint::new(anchor, point));
        let point = constraint.as_ref().map_or(point, |constraint| constraint.point);
        let tolerance_pdf = settings.sensitivity_window_px() / window_pixels_per_pdf_point;
        let tolerance_pdf_squared = tolerance_pdf * tolerance_pdf;
        let mut best: Option<(f64, SemanticSnapDecision)> = None;

        for candidate in &self.candidates {
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
                distance_window_px: distance_pdf_squared.sqrt()
                    * window_pixels_per_pdf_point,
            };
            if best.as_ref().is_none_or(|(best_score, _)| score < *best_score) {
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
    });
    for corner in corners {
        candidates.push(Candidate {
            geometry: CandidateGeometry::Point(corner),
            owner_id: Some(owner_id.clone()),
            role: SemanticSnapRole::Endpoint,
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
        });
        candidates.push(Candidate {
            geometry: CandidateGeometry::Segment { start, end },
            owner_id: Some(owner_id.clone()),
            role: SemanticSnapRole::Nearest,
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
        });
    }
    candidates.push(Candidate {
        geometry: CandidateGeometry::Point(PdfPoint {
            x: (start.x + end.x) * 0.5,
            y: (start.y + end.y) * 0.5,
        }),
        owner_id: Some(owner_id.clone()),
        role: SemanticSnapRole::Midpoint,
    });
    candidates.push(Candidate {
        geometry: CandidateGeometry::Segment { start, end },
        owner_id: Some(owner_id.clone()),
        role: SemanticSnapRole::Nearest,
    });
}

fn project_point_to_segment(point: PdfPoint, start: PdfPoint, end: PdfPoint) -> PdfPoint {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0. {
        return start;
    }
    let t = (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared)
        .clamp(0., 1.);
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
    if !(-0.000_1..=1.000_1).contains(&left_t)
        || !(-0.000_1..=1.000_1).contains(&right_t)
    {
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
