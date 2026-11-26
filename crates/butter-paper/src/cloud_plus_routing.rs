use crate::annotation_model::{AnnotationError, PdfPoint, PdfRect};

/// The default length of the level landing where a Cloud+ leader meets its
/// text, from the knee to the text box. Revu draws 19.8 pt for both Cloud+
/// and Callout (their `/CL`). The Cloud+ tool's Landing property starts here.
pub const CLOUD_PLUS_LANDING_PT: f64 = 19.8;

/// The longest landing the Landing property takes.
pub const CLOUD_PLUS_MAX_LANDING_PT: f64 = 720.;

/// How far right (or left) of its cloud's points a new Cloud+'s text box
/// starts. Revu's Cloud+ puts it 62.4 pt from the points.
pub const CLOUD_PLUS_TEXT_GAP_PT: f64 = 62.4;

/// How far above the top of its cloud's points a new Cloud+'s leader meets
/// its text, so the leader bends at its knee and the landing shows.
pub const CLOUD_PLUS_TEXT_RISE_PT: f64 = 24.;

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

/// The length of a Cloud+ landing: how far across the page its knee lies
/// from where the leader meets the text. This is the knee-to-connection part
/// of the `/CL` callout line. `None` when the Cloud+ has no leader.
pub fn cloud_plus_landing_pt(leader: &[PdfPoint]) -> Option<f64> {
    let [_, knee, connection] = leader else {
        return None;
    };
    Some((connection.x - knee.x).abs())
}

/// A leader routed again for a moved or refitted text box or cloud. It keeps
/// the landing length of `previous_leader`, or the default for a Cloud+
/// that had no leader.
pub fn reroute_cloud_plus_leader(
    control_path: &[PdfPoint],
    visible_path: &[PdfPoint],
    text_box: PdfRect,
    previous_leader: &[PdfPoint],
    context: &CloudPlusRoutingContext,
) -> Result<CloudPlusLeaderRoute, AnnotationError> {
    route_cloud_plus_leader(
        control_path,
        visible_path,
        text_box,
        previous_leader,
        cloud_plus_landing_pt(previous_leader).unwrap_or(CLOUD_PLUS_LANDING_PT),
        context,
    )
}

/// The leader from the cloud to `text_box`. Its landing runs level for
/// `landing_pt` from the left or right side of the text box, at the box's
/// vertical middle, as a Callout's does. The tip is the point of the drawn
/// cloud nearest the knee. The side is the one that does not cross the
/// cloud, the text box or other markups, does not turn back over the
/// landing, and points towards the cloud.
pub fn route_cloud_plus_leader(
    control_path: &[PdfPoint],
    visible_path: &[PdfPoint],
    text_box: PdfRect,
    previous_leader: &[PdfPoint],
    landing_pt: f64,
    context: &CloudPlusRoutingContext,
) -> Result<CloudPlusLeaderRoute, AnnotationError> {
    validate_polygon(control_path)?;
    validate_polyline(visible_path, "Cloud+ visible path")?;
    validate_landing(landing_pt)?;
    if is_rect_wholly_inside_polygon(text_box, control_path) {
        return Ok(CloudPlusLeaderRoute {
            points: Vec::new(),
            side: None,
            score: 0.,
        });
    }

    let cloud_bounds = points_bounds(control_path)?;
    let outline = closed_outline(visible_path);
    let previous_side = infer_leader_side(previous_leader, text_box);
    [CloudPlusLeaderSide::Left, CloudPlusLeaderSide::Right]
        .into_iter()
        .map(|side| {
            build_route_candidate(
                &outline,
                text_box,
                cloud_bounds,
                side,
                previous_side,
                landing_pt,
                context,
            )
        })
        .min_by(|left, right| left.score.total_cmp(&right.score))
        .ok_or_else(|| AnnotationError::InvalidGeometry("Cloud+ route has no candidate".into()))
}

/// The leader with its knee dragged to `pointer`. The landing stays level on
/// the side of the text it meets, and takes the pointer's distance out from
/// that side as its length (0 at the side or inside the box). It stops short
/// of the cloud. The tip stays, unless the leader then crosses the cloud or
/// the text or turns back over the landing: then it moves to the point of
/// the cloud nearest the knee.
pub fn drag_cloud_plus_knee(
    visible_path: &[PdfPoint],
    text_box: PdfRect,
    leader: &[PdfPoint],
    pointer: PdfPoint,
) -> Result<Vec<PdfPoint>, AnnotationError> {
    validate_polyline(visible_path, "Cloud+ visible path")?;
    if !pointer.x.is_finite() || !pointer.y.is_finite() {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ knee target must be finite".into(),
        ));
    }
    let [tip, _, connection] = *leader else {
        return Ok(leader.to_vec());
    };
    let side = infer_leader_side(leader, text_box).unwrap_or(CloudPlusLeaderSide::Left);
    let direction = side.outward();
    let outline = closed_outline(visible_path);
    let landing = fit_landing(
        connection,
        direction,
        ((pointer.x - connection.x) * direction).max(0.),
        &outline,
    );
    let knee = point(connection.x + direction * landing, connection.y);
    let kept = [tip, knee, connection];
    if !leader_is_clear(&kept, &outline, text_box, direction) {
        let nearest = closest_point_on_polyline(&outline, knee).unwrap_or(tip);
        return Ok(vec![nearest, knee, connection]);
    }
    Ok(kept.to_vec())
}

pub fn place_initial_cloud_plus_text_box(
    control_path: &[PdfPoint],
    visible_path: &[PdfPoint],
    width: f64,
    height: f64,
    gap: f64,
    landing_pt: f64,
    context: &CloudPlusRoutingContext,
) -> Result<InitialCloudPlusPlacement, AnnotationError> {
    validate_polygon(control_path)?;
    validate_polyline(visible_path, "Cloud+ visible path")?;
    validate_landing(landing_pt)?;
    if !width.is_finite() || !height.is_finite() || !gap.is_finite() || width < 2. || height < 2. {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ initial text-box dimensions and gap must be finite".into(),
        ));
    }
    let gap = gap.max(0.);
    let cloud = points_bounds(control_path)?;
    // Beside the cloud, raised (or else lowered) so the leader bends at its
    // knee and its landing shows; then above and below it.
    let raised = cloud.y + cloud.height + CLOUD_PLUS_TEXT_RISE_PT - height * 0.5;
    let lowered = cloud.y - CLOUD_PLUS_TEXT_RISE_PT - height * 0.5;
    let right = cloud.x + cloud.width + gap;
    let left = cloud.x - gap - width;
    let candidates = [
        PdfRect::new(right, raised, width, height)?,
        PdfRect::new(left, raised, width, height)?,
        PdfRect::new(right, lowered, width, height)?,
        PdfRect::new(left, lowered, width, height)?,
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
        let leader = route_cloud_plus_leader(
            control_path,
            visible_path,
            text_box,
            &[],
            landing_pt,
            context,
        )?;
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
    Ok(closest_point_on_polyline(&closed_outline(visible_path), target).unwrap_or(target))
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
/// The least room a landing leaves between its knee and the cloud when the
/// cloud is in its way.
const LANDING_CLEARANCE_PT: f64 = 6.;
const SIDE_SWITCH_PENALTY: f64 = 24.;
const WRONG_FACING_PENALTY: f64 = 20_000.;
const DOUBLE_BACK_PENALTY: f64 = 500_000.;
const TEXT_CROSSING_PENALTY: f64 = 1_000_000.;
const CLOUD_CROSSING_PENALTY: f64 = 1_000_000.;
const OBSTACLE_CROSSING_PENALTY: f64 = 25_000.;
const PAGE_OVERFLOW_PENALTY: f64 = 50_000.;

impl CloudPlusLeaderSide {
    /// The direction across the page that the landing runs from this side.
    fn outward(self) -> f64 {
        match self {
            Self::Left => -1.,
            Self::Right => 1.,
        }
    }
}

fn validate_landing(landing_pt: f64) -> Result<(), AnnotationError> {
    if landing_pt.is_finite() && landing_pt >= 0. {
        Ok(())
    } else {
        Err(AnnotationError::InvalidGeometry(
            "Cloud+ landing must be a finite length of 0 or more".into(),
        ))
    }
}

fn build_route_candidate(
    outline: &[PdfPoint],
    text_box: PdfRect,
    cloud_bounds: PdfRect,
    side: CloudPlusLeaderSide,
    previous_side: Option<CloudPlusLeaderSide>,
    landing_pt: f64,
    context: &CloudPlusRoutingContext,
) -> CloudPlusLeaderRoute {
    let connection = connection_point(text_box, side);
    let direction = side.outward();
    let landing = fit_landing(connection, direction, landing_pt, outline);
    let knee = point(connection.x + direction * landing, connection.y);
    let tip = closest_point_on_polyline(outline, knee).unwrap_or_else(|| rect_center(cloud_bounds));
    let points = vec![tip, knee, connection];
    let text_center = rect_center(text_box);
    let cloud_center = rect_center(cloud_bounds);
    let mut score = polyline_length(&points);
    if (cloud_center.x - text_center.x) * direction < -EPSILON {
        score += WRONG_FACING_PENALTY;
    }
    score += axis_preference_penalty(text_box, cloud_bounds);
    if leader_crosses_outline(&points, outline) {
        score += CLOUD_CROSSING_PENALTY;
    }
    if segment_intersects_rect_interior(tip, knee, text_box) {
        score += TEXT_CROSSING_PENALTY;
    }
    let turn_back = (knee.x - tip.x) * direction;
    if turn_back > EPSILON {
        score += DOUBLE_BACK_PENALTY + turn_back;
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

/// `landing_pt`, shortened where the cloud lies across the landing's path so
/// the knee stays outside the cloud with room for the leader.
fn fit_landing(connection: PdfPoint, direction: f64, landing_pt: f64, outline: &[PdfPoint]) -> f64 {
    if point_in_polygon(connection, outline) {
        return landing_pt;
    }
    let Some(hit) = ray_hit_distance(connection, direction, outline) else {
        return landing_pt;
    };
    let room = if hit > LANDING_CLEARANCE_PT * 2. {
        hit - LANDING_CLEARANCE_PT
    } else {
        hit * 0.5
    };
    landing_pt.min(room)
}

/// How far along the level ray from `origin` (rightwards for a `direction`
/// of 1, leftwards for -1) the first segment of `outline` lies.
fn ray_hit_distance(origin: PdfPoint, direction: f64, outline: &[PdfPoint]) -> Option<f64> {
    open_segments(outline)
        .into_iter()
        .filter_map(|(start, end)| {
            if (end.y - start.y).abs() <= EPSILON {
                if (origin.y - start.y).abs() > EPSILON {
                    return None;
                }
                let near = [start.x, end.x]
                    .into_iter()
                    .map(|x| (x - origin.x) * direction)
                    .filter(|distance| *distance > EPSILON)
                    .fold(f64::INFINITY, f64::min);
                return near.is_finite().then_some(near);
            }
            if origin.y < start.y.min(end.y) || origin.y > start.y.max(end.y) {
                return None;
            }
            let amount = (origin.y - start.y) / (end.y - start.y);
            let distance = (start.x + (end.x - start.x) * amount - origin.x) * direction;
            (distance > EPSILON).then_some(distance)
        })
        .min_by(f64::total_cmp)
}

/// Whether the leader is clear of the cloud and the text, and does not turn
/// back over its landing.
fn leader_is_clear(
    points: &[PdfPoint; 3],
    outline: &[PdfPoint],
    text_box: PdfRect,
    direction: f64,
) -> bool {
    let [tip, knee, _] = *points;
    !leader_crosses_outline(points, outline)
        && !segment_intersects_rect_interior(tip, knee, text_box)
        && (knee.x - tip.x) * direction <= EPSILON
}

/// The drawn cloud as a closed path.
fn closed_outline(visible_path: &[PdfPoint]) -> Vec<PdfPoint> {
    let mut outline = visible_path.to_vec();
    if let (Some(first), Some(last)) = (outline.first().copied(), outline.last().copied())
        && squared_distance(first, last) > EPSILON
    {
        outline.push(first);
    }
    outline
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

fn axis_preference_penalty(text_box: PdfRect, cloud_bounds: PdfRect) -> f64 {
    let text = rect_center(text_box);
    let cloud = rect_center(cloud_bounds);
    let horizontal =
        (text.x - cloud.x).abs() / ((cloud_bounds.width + text_box.width) * 0.5).max(1.);
    let vertical =
        (text.y - cloud.y).abs() / ((cloud_bounds.height + text_box.height) * 0.5).max(1.);
    (vertical - horizontal).max(0.) * 5_000.
}

/// Whether the leader (tip, knee, connection) crosses the drawn cloud or
/// runs inside it. The tip sits on the outline, so it only touches it.
fn leader_crosses_outline(points: &[PdfPoint], outline: &[PdfPoint]) -> bool {
    let boundary = open_segments(outline);
    points
        .iter()
        .skip(1)
        .any(|point| point_in_polygon(*point, outline))
        || open_segments(points).into_iter().any(|(start, end)| {
            squared_distance(start, end) > EPSILON
                && (boundary.iter().any(|(edge_start, edge_end)| {
                    segments_properly_intersect(start, end, *edge_start, *edge_end)
                }) || point_in_polygon(
                    point((start.x + end.x) * 0.5, (start.y + end.y) * 0.5),
                    outline,
                ))
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

    fn distance_to_segment(target: PdfPoint, start: PdfPoint, end: PdfPoint) -> f64 {
        closest_point_on_polyline(&[start, end], target).map_or(f64::INFINITY, |closest| {
            squared_distance(closest, target).sqrt()
        })
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
            CLOUD_PLUS_LANDING_PT,
            &CloudPlusRoutingContext {
                page_bounds: Some(rect(0., 0., 612., 792.)),
                obstacles: Vec::new(),
            },
        )
        .unwrap();

        // Right of the cloud, its leader meeting the text above the cloud.
        assert_eq!(
            placement.text_box,
            rect(114., 70. + CLOUD_PLUS_TEXT_RISE_PT - 22., 150., 44.)
        );
        assert_eq!(placement.leader.side, Some(CloudPlusLeaderSide::Left));
        assert_eq!(placement.leader.points.len(), 3);
        assert_eq!(
            placement.leader.points[2],
            point(114., 70. + CLOUD_PLUS_TEXT_RISE_PT)
        );
    }

    #[test]
    fn a_cloud_plus_leader_meets_its_text_with_a_level_landing_of_the_default_length() {
        let control = vec![
            point(10., 10.),
            point(90., 10.),
            point(90., 70.),
            point(10., 70.),
        ];
        let mut visible = control.clone();
        visible.push(control[0]);
        // The text sits above and to the right of the cloud.
        let route = route_cloud_plus_leader(
            &control,
            &visible,
            rect(180., 90., 60., 44.),
            &[],
            CLOUD_PLUS_LANDING_PT,
            &CloudPlusRoutingContext::default(),
        )
        .unwrap();
        let [tip, knee, connection] = route.points.as_slice() else {
            panic!("a Cloud+ leader has a tip, a knee and a connection");
        };
        assert_eq!(*connection, point(180., 112.));
        assert_eq!(knee.y, connection.y, "the landing is level");
        assert!(
            (connection.x - knee.x - CLOUD_PLUS_LANDING_PT).abs() < 1e-9,
            "the landing has the default length: {knee:?} {connection:?}"
        );
        assert!(
            tip.y < knee.y - 10.,
            "the leader bends down to the cloud, so the landing shows: {tip:?} {knee:?}"
        );
        assert!(
            visible
                .windows(2)
                .any(|edge| distance_to_segment(*tip, edge[0], edge[1]) < 1e-9),
            "the tip is on the cloud: {tip:?}"
        );
    }

    #[test]
    fn a_new_cloud_plus_shows_a_landing_of_the_default_length_and_a_bend() {
        let control = vec![
            point(110., 310.),
            point(250., 310.),
            point(250., 420.),
            point(110., 420.),
        ];
        let mut visible = control.clone();
        visible.push(control[0]);
        let placement = place_initial_cloud_plus_text_box(
            &control,
            &visible,
            50.,
            44.,
            CLOUD_PLUS_TEXT_GAP_PT,
            CLOUD_PLUS_LANDING_PT,
            &CloudPlusRoutingContext {
                page_bounds: Some(rect(0., 0., 612., 792.)),
                obstacles: Vec::new(),
            },
        )
        .unwrap();
        let [tip, knee, connection] = placement.leader.points.as_slice() else {
            panic!("a new Cloud+ has a leader");
        };
        assert!(
            placement.text_box.x > 250.,
            "the text is right of the cloud"
        );
        assert_eq!(knee.y, connection.y);
        assert!((connection.x - knee.x - CLOUD_PLUS_LANDING_PT).abs() < 1e-9);
        assert!(
            knee.y - tip.y > 8.,
            "a new Cloud+'s leader is not one straight line: {:?}",
            placement.leader.points
        );
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
            CLOUD_PLUS_LANDING_PT,
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
            CLOUD_PLUS_LANDING_PT,
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

    #[test]
    fn cloud_plus_initial_placement_stays_on_page_near_the_right_edge() {
        let control = vec![
            point(500., 320.),
            point(590., 320.),
            point(590., 390.),
            point(500., 390.),
        ];
        let mut visible = control.clone();
        visible.push(control[0]);
        let placement = place_initial_cloud_plus_text_box(
            &control,
            &visible,
            150.,
            44.,
            24.,
            CLOUD_PLUS_LANDING_PT,
            &CloudPlusRoutingContext {
                page_bounds: Some(rect(0., 0., 612., 792.)),
                obstacles: Vec::new(),
            },
        )
        .unwrap();

        assert!(placement.text_box.x < 500.);
        assert!(placement.text_box.x >= 0.);
        assert!(placement.text_box.x + placement.text_box.width <= 612.);
        assert_eq!(placement.leader.side, Some(CloudPlusLeaderSide::Right));
    }

    #[test]
    fn a_blocking_line_never_bends_the_landing() {
        let control = vec![
            point(10., 10.),
            point(90., 10.),
            point(90., 70.),
            point(10., 70.),
        ];
        let mut visible = control.clone();
        visible.push(control[0]);
        let text_box = rect(180., 18., 150., 44.);
        let rerouted = route_cloud_plus_leader(
            &control,
            &visible,
            text_box,
            &[],
            CLOUD_PLUS_LANDING_PT,
            &CloudPlusRoutingContext {
                page_bounds: Some(rect(-300., -300., 1_000., 1_000.)),
                obstacles: vec![CloudPlusObstacle::Polyline {
                    id: Some("blocking-line".into()),
                    points: vec![point(130., 0.), point(130., 100.)],
                }],
            },
        )
        .unwrap();

        assert_eq!(rerouted.points[1].y, rerouted.points[2].y);
        assert!((rerouted.points[2].x - rerouted.points[1].x - CLOUD_PLUS_LANDING_PT).abs() < 1e-9);
    }

    /// A square cloud's outline, closed, as a scalloped cloud's sampled path
    /// is.
    fn square_cloud(left: f64, bottom: f64, size: f64) -> (Vec<PdfPoint>, Vec<PdfPoint>) {
        let control = vec![
            point(left, bottom),
            point(left + size, bottom),
            point(left + size, bottom + size),
            point(left, bottom + size),
        ];
        let mut visible = control.clone();
        visible.push(control[0]);
        (control, visible)
    }

    fn assert_clear_route(
        name: &str,
        control: &[PdfPoint],
        visible: &[PdfPoint],
        text_box: PdfRect,
        landing: f64,
    ) -> CloudPlusLeaderRoute {
        let route = route_cloud_plus_leader(
            control,
            visible,
            text_box,
            &[],
            landing,
            &CloudPlusRoutingContext::default(),
        )
        .unwrap();
        let [tip, knee, connection] = route.points.as_slice() else {
            panic!("{name}: a leader has a tip, a knee and a connection");
        };
        let outline = closed_outline(visible);
        assert_eq!(knee.y, connection.y, "{name}: the landing is level");
        assert!(
            (connection.x - text_box.x).abs() < 1e-9
                || (connection.x - text_box.x - text_box.width).abs() < 1e-9,
            "{name}: the landing meets a side of the text: {connection:?}"
        );
        assert!(
            (connection.y - (text_box.y + text_box.height * 0.5)).abs() < 1e-9,
            "{name}: the landing meets the text at its middle"
        );
        assert!(
            !leader_crosses_outline(&route.points, &outline),
            "{name}: the leader does not cross the cloud: {:?}",
            route.points
        );
        assert!(
            !segment_intersects_rect_interior(*tip, *knee, text_box),
            "{name}: the leader does not cross the text: {:?}",
            route.points
        );
        let direction = if (connection.x - text_box.x).abs() < 1e-9 {
            -1.
        } else {
            1.
        };
        assert!(
            (knee.x - tip.x) * direction <= 1e-9,
            "{name}: the leader does not turn back over the landing: {:?}",
            route.points
        );
        assert!(
            outline
                .windows(2)
                .any(|edge| distance_to_segment(*tip, edge[0], edge[1]) < 1e-9),
            "{name}: the tip is on the cloud: {tip:?}"
        );
        let nearest = closest_point_on_polyline(&outline, *knee).unwrap();
        assert!(
            squared_distance(nearest, *tip) < 1e-9,
            "{name}: the tip is the cloud point nearest the knee"
        );
        route
    }

    #[test]
    fn a_leader_never_crosses_the_cloud_or_its_text_and_never_turns_back() {
        let (control, visible) = square_cloud(200., 200., 100.);
        let cases = [
            (
                "right",
                rect(380., 260., 60., 30.),
                Some(CloudPlusLeaderSide::Left),
            ),
            (
                "left",
                rect(60., 260., 60., 30.),
                Some(CloudPlusLeaderSide::Right),
            ),
            // Centred over or under the cloud, either side points at it.
            ("above", rect(220., 360., 60., 30.), None),
            ("below", rect(220., 110., 60., 30.), None),
            (
                "above right",
                rect(330., 340., 60., 30.),
                Some(CloudPlusLeaderSide::Left),
            ),
            (
                "below left",
                rect(110., 120., 60., 30.),
                Some(CloudPlusLeaderSide::Right),
            ),
            // Overlapping the cloud's width, the landing reaches over it.
            (
                "above, offset right",
                rect(270., 340., 80., 30.),
                Some(CloudPlusLeaderSide::Left),
            ),
            (
                "above, offset left",
                rect(150., 340., 80., 30.),
                Some(CloudPlusLeaderSide::Right),
            ),
        ];
        for (name, text_box, side) in cases {
            let route =
                assert_clear_route(name, &control, &visible, text_box, CLOUD_PLUS_LANDING_PT);
            if let Some(side) = side {
                assert_eq!(
                    route.side,
                    Some(side),
                    "{name}: the landing points at the cloud"
                );
            }
            assert!(
                ((route.points[2].x - route.points[1].x).abs() - CLOUD_PLUS_LANDING_PT).abs()
                    < 1e-9,
                "{name}: the landing keeps its length: {:?}",
                route.points
            );
        }
    }

    #[test]
    fn a_landing_near_the_cloud_stops_short_of_it() {
        let (control, visible) = square_cloud(200., 200., 100.);
        // Level with the cloud, 14 pt from it: too close for a full landing.
        let near = rect(314., 260., 60., 30.);
        let route = assert_clear_route("near", &control, &visible, near, CLOUD_PLUS_LANDING_PT);
        let landing = route.points[2].x - route.points[1].x;
        assert!(
            (landing - (14. - LANDING_CLEARANCE_PT)).abs() < 1e-9,
            "the landing leaves room for the leader: {:?}",
            route.points
        );
        // Very close, the knee lies halfway.
        let touching = rect(304., 260., 60., 30.);
        let route = assert_clear_route("touching", &control, &visible, touching, 40.);
        assert!((route.points[2].x - route.points[1].x - 2.).abs() < 1e-9);
        // A long landing level with the cloud stops before it too.
        let route =
            assert_clear_route("long", &control, &visible, rect(380., 260., 60., 30.), 200.);
        assert!((route.points[1].x - (300. + LANDING_CLEARANCE_PT)).abs() < 1e-9);
    }

    #[test]
    fn a_landing_of_zero_meets_the_text_at_the_knee() {
        let (control, visible) = square_cloud(200., 200., 100.);
        let route = assert_clear_route("zero", &control, &visible, rect(380., 340., 60., 30.), 0.);
        assert_eq!(route.points[1], route.points[2]);
        assert!(
            route_cloud_plus_leader(
                &control,
                &visible,
                rect(380., 340., 60., 30.),
                &[],
                -1.,
                &CloudPlusRoutingContext::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn a_rerouted_leader_keeps_its_landing_length() {
        let (control, visible) = square_cloud(200., 200., 100.);
        let first = route_cloud_plus_leader(
            &control,
            &visible,
            rect(380., 340., 60., 30.),
            &[],
            37.5,
            &CloudPlusRoutingContext::default(),
        )
        .unwrap();
        assert_eq!(cloud_plus_landing_pt(&first.points), Some(37.5));
        let moved = reroute_cloud_plus_leader(
            &control,
            &visible,
            rect(60., 120., 60., 30.),
            &first.points,
            &CloudPlusRoutingContext::default(),
        )
        .unwrap();
        assert_eq!(moved.side, Some(CloudPlusLeaderSide::Right));
        assert!((cloud_plus_landing_pt(&moved.points).unwrap() - 37.5).abs() < 1e-9);
        // A Cloud+ with no leader takes the default landing.
        let fresh = reroute_cloud_plus_leader(
            &control,
            &visible,
            rect(380., 340., 60., 30.),
            &[],
            &CloudPlusRoutingContext::default(),
        )
        .unwrap();
        assert!(
            (cloud_plus_landing_pt(&fresh.points).unwrap() - CLOUD_PLUS_LANDING_PT).abs() < 1e-9
        );
    }

    #[test]
    fn dragging_the_knee_sets_a_level_landing_on_the_text_side() {
        let (control, visible) = square_cloud(200., 200., 100.);
        let text_box = rect(380., 340., 60., 30.);
        let route = route_cloud_plus_leader(
            &control,
            &visible,
            text_box,
            &[],
            CLOUD_PLUS_LANDING_PT,
            &CloudPlusRoutingContext::default(),
        )
        .unwrap();
        let [tip, _, connection] = route.points.as_slice() else {
            panic!("a leader");
        };
        // Out and up from the text: the landing takes the distance out only.
        let dragged =
            drag_cloud_plus_knee(&visible, text_box, &route.points, point(330., 400.)).unwrap();
        assert_eq!(dragged[2], *connection);
        assert_eq!(dragged[1], point(330., connection.y));
        assert_eq!(cloud_plus_landing_pt(&dragged), Some(50.));
        assert_eq!(dragged[0], *tip, "a clear tip stays where it is");
        // Into the text: no landing.
        let flat =
            drag_cloud_plus_knee(&visible, text_box, &route.points, point(400., 300.)).unwrap();
        assert_eq!(flat[1], flat[2]);
        // Past the cloud's side, the tip moves so the leader stays clear.
        let far =
            drag_cloud_plus_knee(&visible, text_box, &route.points, point(150., 355.)).unwrap();
        assert_eq!(far[1], point(150., 355.));
        assert!(!leader_crosses_outline(&far, &closed_outline(&visible)));
        assert!((far[0].x - 200.).abs() < 1e-9 && (far[0].y - 300.).abs() < 1e-9);
        // Level with the cloud, it stops short of the cloud.
        let level = rect(380., 260., 60., 30.);
        let route = route_cloud_plus_leader(
            &control,
            &visible,
            level,
            &[],
            CLOUD_PLUS_LANDING_PT,
            &CloudPlusRoutingContext::default(),
        )
        .unwrap();
        let blocked =
            drag_cloud_plus_knee(&visible, level, &route.points, point(250., 275.)).unwrap();
        assert_eq!(blocked[1].x, 300. + LANDING_CLEARANCE_PT);
    }

    #[test]
    fn cloud_plus_routing_is_translation_invariant_with_page_and_obstacles() {
        let control = vec![
            point(10., 10.),
            point(90., 10.),
            point(90., 70.),
            point(10., 70.),
        ];
        let mut visible = control.clone();
        visible.push(control[0]);
        let text_box = rect(180., 18., 150., 44.);
        let obstacle = rect(115., 20., 18., 40.);
        let original = route_cloud_plus_leader(
            &control,
            &visible,
            text_box,
            &[],
            CLOUD_PLUS_LANDING_PT,
            &CloudPlusRoutingContext {
                page_bounds: Some(rect(-200., -200., 700., 700.)),
                obstacles: vec![CloudPlusObstacle::Rect {
                    id: Some("obstacle".into()),
                    rect: obstacle,
                }],
            },
        )
        .unwrap();
        let delta = point(57., -31.);
        let translate_point = |value: PdfPoint| point(value.x + delta.x, value.y + delta.y);
        let translate_rect = |value: PdfRect| {
            rect(
                value.x + delta.x,
                value.y + delta.y,
                value.width,
                value.height,
            )
        };
        let translated = route_cloud_plus_leader(
            &control
                .iter()
                .copied()
                .map(translate_point)
                .collect::<Vec<_>>(),
            &visible
                .iter()
                .copied()
                .map(translate_point)
                .collect::<Vec<_>>(),
            translate_rect(text_box),
            &[],
            CLOUD_PLUS_LANDING_PT,
            &CloudPlusRoutingContext {
                page_bounds: Some(translate_rect(rect(-200., -200., 700., 700.))),
                obstacles: vec![CloudPlusObstacle::Rect {
                    id: Some("obstacle".into()),
                    rect: translate_rect(obstacle),
                }],
            },
        )
        .unwrap();

        assert_eq!(translated.side, original.side);
        assert_eq!(
            translated.points,
            original
                .points
                .iter()
                .copied()
                .map(translate_point)
                .collect::<Vec<_>>()
        );
        assert!((translated.score - original.score).abs() <= EPSILON);
    }
}
