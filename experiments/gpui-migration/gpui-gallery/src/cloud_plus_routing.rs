use crate::annotation_model::{AnnotationError, PdfPoint, PdfRect};

#[derive(Clone, Debug, PartialEq)]
pub enum CloudPlusObstacle {
    Rect {
        id: Option<String>,
        rect: PdfRect,
    },
    Polyline {
        id: Option<String>,
        points: Vec<PdfPoint>,
    },
    Polygon {
        id: Option<String>,
        points: Vec<PdfPoint>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudPlusLeaderSide {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CloudPlusLeaderRoute {
    pub points: Vec<PdfPoint>,
    pub side: Option<CloudPlusLeaderSide>,
    pub score: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CloudPlusRoutingContext {
    pub page_bounds: Option<PdfRect>,
    pub obstacles: Vec<CloudPlusObstacle>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InitialCloudPlusPlacement {
    pub text_box: PdfRect,
    pub leader: CloudPlusLeaderRoute,
}

pub fn route_cloud_plus_leader(
    control_path: &[PdfPoint],
    visible_path: &[PdfPoint],
    text_box: PdfRect,
    previous_leader: &[PdfPoint],
    context: &CloudPlusRoutingContext,
) -> Result<CloudPlusLeaderRoute, AnnotationError> {
    validate_polygon(control_path)?;
    validate_polyline(visible_path, "Cloud+ visible path")?;
    if is_rect_wholly_inside_polygon(text_box, control_path) {
        return Ok(CloudPlusLeaderRoute {
            points: Vec::new(),
            side: None,
            score: 0.,
        });
    }

    let cloud_bounds = points_bounds(control_path)?;
    let previous_side = infer_leader_side(previous_leader, text_box);
    let detour = (cloud_bounds.width.max(cloud_bounds.height) * 0.75).clamp(32., 96.);
    let offsets: &[f64] = if context.obstacles.is_empty() {
        &[0.]
    } else {
        &[0., -detour, detour]
    };
    let mut candidates = Vec::new();
    for side in [CloudPlusLeaderSide::Left, CloudPlusLeaderSide::Right] {
        for offset in offsets {
            candidates.push(build_route_candidate(
                control_path,
                visible_path,
                text_box,
                cloud_bounds,
                side,
                previous_side,
                *offset,
                context,
            ));
        }
    }
    candidates
        .into_iter()
        .min_by(|left, right| left.score.total_cmp(&right.score))
        .ok_or_else(|| AnnotationError::InvalidGeometry("Cloud+ route has no candidate".into()))
}

#[allow(clippy::too_many_arguments)]
pub fn place_initial_cloud_plus_text_box(
    control_path: &[PdfPoint],
    visible_path: &[PdfPoint],
    width: f64,
    height: f64,
    gap: f64,
    context: &CloudPlusRoutingContext,
) -> Result<InitialCloudPlusPlacement, AnnotationError> {
    validate_polygon(control_path)?;
    validate_polyline(visible_path, "Cloud+ visible path")?;
    if !width.is_finite() || !height.is_finite() || !gap.is_finite() || width < 2. || height < 2. {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ initial text-box dimensions and gap must be finite".into(),
        ));
    }
    let gap = gap.max(0.);
    let cloud = points_bounds(control_path)?;
    let candidates = [
        PdfRect::new(
            cloud.x + cloud.width + gap,
            cloud.y + (cloud.height - height) * 0.5,
            width,
            height,
        )?,
        PdfRect::new(
            cloud.x - gap - width,
            cloud.y + (cloud.height - height) * 0.5,
            width,
            height,
        )?,
        PdfRect::new(
            cloud.x + (cloud.width - width) * 0.5,
            cloud.y + cloud.height + gap,
            width,
            height,
        )?,
        PdfRect::new(
            cloud.x + (cloud.width - width) * 0.5,
            cloud.y - gap - height,
            width,
            height,
        )?,
    ];
    let mut scored = Vec::new();
    for (index, text_box) in candidates.into_iter().enumerate() {
        let leader = route_cloud_plus_leader(control_path, visible_path, text_box, &[], context)?;
        let obstacle_overlap = context
            .obstacles
            .iter()
            .map(|obstacle| rect_obstacle_overlap(text_box, obstacle))
            .sum::<f64>();
        let page_overflow = context
            .page_bounds
            .map_or(0., |bounds| rect_overflow_area(text_box, bounds));
        let collision_class = if obstacle_overlap > EPSILON {
            1_000_000.
        } else {
            0.
        };
        let score = leader.score
            + collision_class
            + obstacle_overlap * 100.
            + page_overflow * PAGE_OVERFLOW_PENALTY
            + index as f64 * EPSILON;
        scored.push((score, InitialCloudPlusPlacement { text_box, leader }));
    }
    scored
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, placement)| placement)
        .ok_or_else(|| AnnotationError::InvalidGeometry("Cloud+ placement has no candidate".into()))
}

pub fn snap_cloud_plus_leader_tip(
    visible_path: &[PdfPoint],
    target: PdfPoint,
) -> Result<PdfPoint, AnnotationError> {
    validate_polyline(visible_path, "Cloud+ visible path")?;
    if !target.x.is_finite() || !target.y.is_finite() {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ leader target must be finite".into(),
        ));
    }
    Ok(closest_point_on_polyline(visible_path, target).unwrap_or(target))
}

pub fn is_rect_wholly_inside_polygon(rect: PdfRect, polygon: &[PdfPoint]) -> bool {
    if polygon.len() < 3 || rect.width < 0. || rect.height < 0. {
        return false;
    }
    let corners = rect_corners(rect);
    if !corners
        .iter()
        .all(|point| point_inside_or_on_polygon(*point, polygon))
    {
        return false;
    }
    let box_edges = closed_segments(&corners);
    let polygon_edges = closed_segments(polygon);
    !polygon_edges.iter().any(|(start, end)| {
        box_edges.iter().any(|(box_start, box_end)| {
            segments_properly_intersect(*start, *end, *box_start, *box_end)
        })
    })
}

const EPSILON: f64 = 0.000_001;
const SIDE_SWITCH_PENALTY: f64 = 24.;
const WRONG_FACING_PENALTY: f64 = 20_000.;
const CLOUD_CROSSING_PENALTY: f64 = 100_000.;
const OBSTACLE_CROSSING_PENALTY: f64 = 25_000.;
const PAGE_OVERFLOW_PENALTY: f64 = 50_000.;

#[allow(clippy::too_many_arguments)]
fn build_route_candidate(
    control_path: &[PdfPoint],
    visible_path: &[PdfPoint],
    text_box: PdfRect,
    cloud_bounds: PdfRect,
    side: CloudPlusLeaderSide,
    previous_side: Option<CloudPlusLeaderSide>,
    knee_offset: f64,
    context: &CloudPlusRoutingContext,
) -> CloudPlusLeaderRoute {
    let connection = connection_point(text_box, side);
    let direction = match side {
        CloudPlusLeaderSide::Left => point(-1., 0.),
        CloudPlusLeaderSide::Right => point(1., 0.),
    };
    let tip = directional_attachment_point(visible_path, connection, direction)
        .or_else(|| closest_point_on_polyline(visible_path, connection))
        .unwrap_or_else(|| rect_center(cloud_bounds));
    let knee = point((tip.x + connection.x) * 0.5, connection.y + knee_offset);
    let points = vec![tip, knee, connection];
    let text_center = rect_center(text_box);
    let cloud_center = rect_center(cloud_bounds);
    let mut score = polyline_length(&points);
    let facing = match side {
        CloudPlusLeaderSide::Left => cloud_center.x <= text_center.x + EPSILON,
        CloudPlusLeaderSide::Right => cloud_center.x >= text_center.x - EPSILON,
    };
    if !facing {
        score += WRONG_FACING_PENALTY;
    }
    score += axis_preference_penalty(text_box, cloud_bounds);
    if leader_crosses_cloud_interior(&points, control_path) {
        score += CLOUD_CROSSING_PENALTY;
    }
    score +=
        route_obstacle_crossings(&points, &context.obstacles) as f64 * OBSTACLE_CROSSING_PENALTY;
    if let Some(bounds) = context.page_bounds {
        score += rect_overflow_area(text_box, bounds) * PAGE_OVERFLOW_PENALTY;
        score += points
            .iter()
            .map(|point| point_outside_distance(*point, bounds))
            .sum::<f64>()
            * 5_000.;
    }
    if previous_side.is_some_and(|previous| previous != side) {
        score += SIDE_SWITCH_PENALTY;
    }
    CloudPlusLeaderRoute {
        points,
        side: Some(side),
        score,
    }
}

fn validate_polygon(points: &[PdfPoint]) -> Result<(), AnnotationError> {
    if points.len() < 3 {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ control path requires at least three points".into(),
        ));
    }
    validate_points(points, "Cloud+ control path")
}

fn validate_polyline(points: &[PdfPoint], name: &str) -> Result<(), AnnotationError> {
    if points.len() < 2 {
        return Err(AnnotationError::InvalidGeometry(format!(
            "{name} requires at least two points"
        )));
    }
    validate_points(points, name)
}

fn validate_points(points: &[PdfPoint], name: &str) -> Result<(), AnnotationError> {
    if points
        .iter()
        .all(|point| point.x.is_finite() && point.y.is_finite())
    {
        Ok(())
    } else {
        Err(AnnotationError::InvalidGeometry(format!(
            "{name} points must be finite"
        )))
    }
}

fn infer_leader_side(points: &[PdfPoint], text_box: PdfRect) -> Option<CloudPlusLeaderSide> {
    let connection = points.last()?;
    let left = (connection.x - text_box.x).abs();
    let right = (connection.x - (text_box.x + text_box.width)).abs();
    Some(if left <= right {
        CloudPlusLeaderSide::Left
    } else {
        CloudPlusLeaderSide::Right
    })
}

fn connection_point(text_box: PdfRect, side: CloudPlusLeaderSide) -> PdfPoint {
    let center = rect_center(text_box);
    match side {
        CloudPlusLeaderSide::Left => point(text_box.x, center.y),
        CloudPlusLeaderSide::Right => point(text_box.x + text_box.width, center.y),
    }
}

fn directional_attachment_point(
    points: &[PdfPoint],
    target: PdfPoint,
    direction: PdfPoint,
) -> Option<PdfPoint> {
    let mut intersections = Vec::new();
    for (start, end) in open_segments(points) {
        if direction.x.abs() > 0. {
            if target.y < start.y.min(end.y) - EPSILON || target.y > start.y.max(end.y) + EPSILON {
                continue;
            }
            if (end.y - start.y).abs() <= EPSILON {
                if (target.y - start.y).abs() <= EPSILON {
                    intersections.extend([start, end]);
                }
            } else {
                let amount = (target.y - start.y) / (end.y - start.y);
                if (-EPSILON..=1. + EPSILON).contains(&amount) {
                    intersections.push(point(start.x + (end.x - start.x) * amount, target.y));
                }
            }
        }
    }
    intersections
        .into_iter()
        .filter(|candidate| dot(subtract(*candidate, target), direction) >= -EPSILON)
        .min_by(|left, right| {
            squared_distance(*left, target).total_cmp(&squared_distance(*right, target))
        })
}

fn axis_preference_penalty(text_box: PdfRect, cloud_bounds: PdfRect) -> f64 {
    let text = rect_center(text_box);
    let cloud = rect_center(cloud_bounds);
    let horizontal =
        (text.x - cloud.x).abs() / ((cloud_bounds.width + text_box.width) * 0.5).max(1.);
    let vertical =
        (text.y - cloud.y).abs() / ((cloud_bounds.height + text_box.height) * 0.5).max(1.);
    (vertical - horizontal).max(0.) * 5_000.
}

fn leader_crosses_cloud_interior(points: &[PdfPoint], polygon: &[PdfPoint]) -> bool {
    let boundary = closed_segments(polygon);
    open_segments(points).into_iter().any(|(start, end)| {
        point_in_polygon(start, polygon)
            || point_in_polygon(end, polygon)
            || boundary.iter().any(|(edge_start, edge_end)| {
                segments_properly_intersect(start, end, *edge_start, *edge_end)
            })
            || point_in_polygon(
                point((start.x + end.x) * 0.5, (start.y + end.y) * 0.5),
                polygon,
            )
    })
}

fn route_obstacle_crossings(points: &[PdfPoint], obstacles: &[CloudPlusObstacle]) -> usize {
    obstacles
        .iter()
        .map(|obstacle| {
            open_segments(points)
                .into_iter()
                .map(|(start, end)| match obstacle {
                    CloudPlusObstacle::Rect { rect, .. } => {
                        usize::from(segment_intersects_rect_interior(start, end, *rect))
                    }
                    CloudPlusObstacle::Polyline {
                        points: obstacle, ..
                    } => open_segments(obstacle)
                        .into_iter()
                        .filter(|(a, b)| segments_intersect(start, end, *a, *b))
                        .count(),
                    CloudPlusObstacle::Polygon {
                        points: obstacle, ..
                    } => closed_segments(obstacle)
                        .into_iter()
                        .filter(|(a, b)| segments_intersect(start, end, *a, *b))
                        .count(),
                })
                .sum::<usize>()
        })
        .sum()
}

fn rect_obstacle_overlap(rect: PdfRect, obstacle: &CloudPlusObstacle) -> f64 {
    let obstacle_bounds = match obstacle {
        CloudPlusObstacle::Rect { rect, .. } => *rect,
        CloudPlusObstacle::Polyline { points, .. } | CloudPlusObstacle::Polygon { points, .. } => {
            match points_bounds(points) {
                Ok(bounds) => bounds,
                Err(_) => return 0.,
            }
        }
    };
    intersection_area(rect, obstacle_bounds)
}

fn rect_overflow_area(rect: PdfRect, bounds: PdfRect) -> f64 {
    (rect.width * rect.height - intersection_area(rect, bounds)).max(0.)
}

fn point_outside_distance(point: PdfPoint, bounds: PdfRect) -> f64 {
    let dx = if point.x < bounds.x {
        bounds.x - point.x
    } else if point.x > bounds.x + bounds.width {
        point.x - (bounds.x + bounds.width)
    } else {
        0.
    };
    let dy = if point.y < bounds.y {
        bounds.y - point.y
    } else if point.y > bounds.y + bounds.height {
        point.y - (bounds.y + bounds.height)
    } else {
        0.
    };
    dx.hypot(dy)
}

fn intersection_area(left: PdfRect, right: PdfRect) -> f64 {
    let width = ((left.x + left.width).min(right.x + right.width) - left.x.max(right.x)).max(0.);
    let height = ((left.y + left.height).min(right.y + right.height) - left.y.max(right.y)).max(0.);
    width * height
}

fn segment_intersects_rect_interior(start: PdfPoint, end: PdfPoint, rect: PdfRect) -> bool {
    point_strictly_in_rect(start, rect)
        || point_strictly_in_rect(end, rect)
        || closed_segments(&rect_corners(rect))
            .iter()
            .any(|(a, b)| segments_properly_intersect(start, end, *a, *b))
}

fn point_strictly_in_rect(point: PdfPoint, rect: PdfRect) -> bool {
    point.x > rect.x + EPSILON
        && point.x < rect.x + rect.width - EPSILON
        && point.y > rect.y + EPSILON
        && point.y < rect.y + rect.height - EPSILON
}

fn point_inside_or_on_polygon(point: PdfPoint, polygon: &[PdfPoint]) -> bool {
    closed_segments(polygon)
        .iter()
        .any(|(start, end)| point_on_segment(point, *start, *end))
        || point_in_polygon(point, polygon)
}

fn point_in_polygon(point: PdfPoint, polygon: &[PdfPoint]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let a = polygon[current];
        let b = polygon[previous];
        if ((a.y > point.y) != (b.y > point.y))
            && point.x < ((b.x - a.x) * (point.y - a.y)) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn closest_point_on_polyline(points: &[PdfPoint], target: PdfPoint) -> Option<PdfPoint> {
    open_segments(points)
        .into_iter()
        .map(|(start, end)| {
            let vector = subtract(end, start);
            let denominator = dot(vector, vector);
            let amount = if denominator <= EPSILON {
                0.
            } else {
                (dot(subtract(target, start), vector) / denominator).clamp(0., 1.)
            };
            point(start.x + vector.x * amount, start.y + vector.y * amount)
        })
        .min_by(|left, right| {
            squared_distance(*left, target).total_cmp(&squared_distance(*right, target))
        })
        .or_else(|| points.first().copied())
}

fn segments_intersect(a: PdfPoint, b: PdfPoint, c: PdfPoint, d: PdfPoint) -> bool {
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    ((ab_c > EPSILON && ab_d < -EPSILON) || (ab_c < -EPSILON && ab_d > EPSILON))
        && ((cd_a > EPSILON && cd_b < -EPSILON) || (cd_a < -EPSILON && cd_b > EPSILON))
        || ab_c.abs() <= EPSILON && point_on_segment(c, a, b)
        || ab_d.abs() <= EPSILON && point_on_segment(d, a, b)
        || cd_a.abs() <= EPSILON && point_on_segment(a, c, d)
        || cd_b.abs() <= EPSILON && point_on_segment(b, c, d)
}

fn segments_properly_intersect(a: PdfPoint, b: PdfPoint, c: PdfPoint, d: PdfPoint) -> bool {
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    ((ab_c > EPSILON && ab_d < -EPSILON) || (ab_c < -EPSILON && ab_d > EPSILON))
        && ((cd_a > EPSILON && cd_b < -EPSILON) || (cd_a < -EPSILON && cd_b > EPSILON))
}

fn point_on_segment(point: PdfPoint, start: PdfPoint, end: PdfPoint) -> bool {
    orientation(start, end, point).abs() <= EPSILON
        && point.x >= start.x.min(end.x) - EPSILON
        && point.x <= start.x.max(end.x) + EPSILON
        && point.y >= start.y.min(end.y) - EPSILON
        && point.y <= start.y.max(end.y) + EPSILON
}

fn orientation(a: PdfPoint, b: PdfPoint, c: PdfPoint) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn points_bounds(points: &[PdfPoint]) -> Result<PdfRect, AnnotationError> {
    validate_points(points, "Cloud+ bounds")?;
    let min_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

fn rect_corners(rect: PdfRect) -> [PdfPoint; 4] {
    [
        point(rect.x, rect.y),
        point(rect.x + rect.width, rect.y),
        point(rect.x + rect.width, rect.y + rect.height),
        point(rect.x, rect.y + rect.height),
    ]
}

fn rect_center(rect: PdfRect) -> PdfPoint {
    point(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
}

fn polyline_length(points: &[PdfPoint]) -> f64 {
    open_segments(points)
        .into_iter()
        .map(|(start, end)| (end.x - start.x).hypot(end.y - start.y))
        .sum()
}

fn open_segments(points: &[PdfPoint]) -> Vec<(PdfPoint, PdfPoint)> {
    points.windows(2).map(|pair| (pair[0], pair[1])).collect()
}

fn closed_segments(points: &[PdfPoint]) -> Vec<(PdfPoint, PdfPoint)> {
    if points.is_empty() {
        return Vec::new();
    }
    points
        .iter()
        .enumerate()
        .map(|(index, point)| (*point, points[(index + 1) % points.len()]))
        .collect()
}

fn point(x: f64, y: f64) -> PdfPoint {
    PdfPoint { x, y }
}

fn subtract(left: PdfPoint, right: PdfPoint) -> PdfPoint {
    point(left.x - right.x, left.y - right.y)
}

fn dot(left: PdfPoint, right: PdfPoint) -> f64 {
    left.x * right.x + left.y * right.y
}

fn squared_distance(left: PdfPoint, right: PdfPoint) -> f64 {
    (left.x - right.x).powi(2) + (left.y - right.y).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> PdfPoint {
        PdfPoint::new(x, y).unwrap()
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> PdfRect {
        PdfRect::new(x, y, width, height).unwrap()
    }

    #[test]
    fn cloud_plus_routing_places_right_by_default_and_uses_three_points() {
        let control = vec![
            point(10., 10.),
            point(90., 10.),
            point(90., 70.),
            point(10., 70.),
        ];
        let mut visible = control.clone();
        visible.push(control[0]);
        let placement = place_initial_cloud_plus_text_box(
            &control,
            &visible,
            150.,
            44.,
            24.,
            &CloudPlusRoutingContext {
                page_bounds: Some(rect(0., 0., 612., 792.)),
                obstacles: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(placement.text_box, rect(114., 18., 150., 44.));
        assert_eq!(placement.leader.side, Some(CloudPlusLeaderSide::Left));
        assert_eq!(placement.leader.points.len(), 3);
        assert_eq!(placement.leader.points[2], point(114., 40.));
    }

    #[test]
    fn cloud_plus_routing_hides_inline_leader_and_avoids_a_blocked_default_side() {
        let control = vec![
            point(10., 10.),
            point(210., 10.),
            point(210., 110.),
            point(10., 110.),
        ];
        let mut visible = control.clone();
        visible.push(control[0]);
        let inline = route_cloud_plus_leader(
            &control,
            &visible,
            rect(40., 30., 100., 44.),
            &[],
            &CloudPlusRoutingContext::default(),
        )
        .unwrap();
        assert!(inline.points.is_empty());
        assert_eq!(inline.side, None);

        let placement = place_initial_cloud_plus_text_box(
            &control,
            &visible,
            150.,
            44.,
            24.,
            &CloudPlusRoutingContext {
                page_bounds: Some(rect(-300., -300., 1_000., 1_000.)),
                obstacles: vec![CloudPlusObstacle::Rect {
                    id: Some("right-blocker".into()),
                    rect: rect(230., 0., 200., 180.),
                }],
            },
        )
        .unwrap();
        assert!(placement.text_box.x < 230.);
        assert!(placement.leader.side.is_some());
    }

    #[test]
    fn cloud_plus_routing_snaps_to_the_sampled_visible_edge() {
        let visible = vec![point(0., 0.), point(100., 0.), point(100., 100.)];
        assert_eq!(
            snap_cloud_plus_leader_tip(&visible, point(72., 18.)).unwrap(),
            point(72., 0.)
        );
    }
}
