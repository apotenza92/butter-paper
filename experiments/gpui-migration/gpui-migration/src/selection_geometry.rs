pub const MARQUEE_THRESHOLD_CSS_PX: f64 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionPoint {
    pub x: f64,
    pub y: f64,
}

impl SelectionPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionShape {
    Lasso,
    Box,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionKind {
    Window,
    Crossing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionOperation {
    Replace,
    Add,
    Remove,
}

impl SelectionOperation {
    pub const fn from_modifiers(shift: bool, alt: bool) -> Self {
        if alt {
            Self::Remove
        } else if shift {
            Self::Add
        } else {
            Self::Replace
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionMarquee {
    pub pointer_id: Option<u64>,
    pub shape: SelectionShape,
    pub kind: Option<SelectionKind>,
    pub operation: SelectionOperation,
    pub start: SelectionPoint,
    pub current: SelectionPoint,
    pub points: Vec<SelectionPoint>,
    pub active: bool,
}

impl SelectionMarquee {
    pub fn lasso(pointer_id: u64, start: SelectionPoint, operation: SelectionOperation) -> Self {
        Self {
            pointer_id: Some(pointer_id),
            shape: SelectionShape::Lasso,
            kind: None,
            operation,
            start,
            current: start,
            points: vec![start],
            active: false,
        }
    }

    pub fn armed_box(start: SelectionPoint, operation: SelectionOperation) -> Self {
        Self {
            pointer_id: None,
            shape: SelectionShape::Box,
            kind: None,
            operation,
            start,
            current: start,
            points: vec![start],
            active: false,
        }
    }

    pub fn update(&mut self, current: SelectionPoint) {
        let delta_x = current.x - self.start.x;
        let delta_y = current.y - self.start.y;
        if self.shape == SelectionShape::Lasso
            && self.kind.is_none()
            && delta_x.abs() > MARQUEE_THRESHOLD_CSS_PX
        {
            self.kind = Some(selection_kind(self.start, current));
        } else if self.shape == SelectionShape::Box {
            self.kind = Some(selection_kind(self.start, current));
        }
        self.current = current;
        if self.shape == SelectionShape::Lasso {
            self.points.push(current);
        } else {
            self.points = vec![self.start, current];
        }
        self.active |= delta_x.hypot(delta_y) > MARQUEE_THRESHOLD_CSS_PX;
    }

    pub fn resolved_kind(&self) -> SelectionKind {
        self.kind
            .unwrap_or_else(|| selection_kind(self.start, self.current))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionPath {
    pub points: Vec<SelectionPoint>,
    pub closed: bool,
}

impl SelectionPath {
    pub fn new(points: Vec<SelectionPoint>, closed: bool) -> Self {
        Self { points, closed }
    }
}

pub fn selection_kind(start: SelectionPoint, end: SelectionPoint) -> SelectionKind {
    if end.x >= start.x {
        SelectionKind::Window
    } else {
        SelectionKind::Crossing
    }
}

pub fn selection_after<T: Clone + Eq>(
    selected: &[T],
    hits: &[T],
    operation: SelectionOperation,
) -> Vec<T> {
    match operation {
        SelectionOperation::Replace => hits.to_vec(),
        SelectionOperation::Add => selected
            .iter()
            .cloned()
            .chain(hits.iter().filter(|hit| !selected.contains(hit)).cloned())
            .collect(),
        SelectionOperation::Remove => selected
            .iter()
            .filter(|selected| !hits.contains(selected))
            .cloned()
            .collect(),
    }
}

pub fn geometry_selected(paths: &[SelectionPath], marquee: &SelectionMarquee) -> bool {
    let selection_path = match marquee.shape {
        SelectionShape::Box => box_path(marquee.start, marquee.current),
        SelectionShape::Lasso => {
            if marquee.points.len() < 3 {
                return false;
            }
            marquee.points.clone()
        }
    };
    let paths = paths
        .iter()
        .filter(|path| !path.points.is_empty())
        .collect::<Vec<_>>();
    match marquee.resolved_kind() {
        SelectionKind::Window => {
            !paths.is_empty()
                && paths.iter().all(|path| {
                    path.points
                        .iter()
                        .all(|point| point_in_selection(*point, &selection_path, marquee.shape))
                })
        }
        SelectionKind::Crossing => paths
            .iter()
            .any(|path| paths_intersect(path, &selection_path, marquee.shape)),
    }
}

fn box_path(start: SelectionPoint, end: SelectionPoint) -> Vec<SelectionPoint> {
    let left = start.x.min(end.x);
    let right = start.x.max(end.x);
    let top = start.y.min(end.y);
    let bottom = start.y.max(end.y);
    vec![
        SelectionPoint::new(left, top),
        SelectionPoint::new(right, top),
        SelectionPoint::new(right, bottom),
        SelectionPoint::new(left, bottom),
    ]
}

fn point_in_selection(
    point: SelectionPoint,
    selection: &[SelectionPoint],
    shape: SelectionShape,
) -> bool {
    match shape {
        SelectionShape::Box => {
            let bounds = box_path(selection[0], selection[2]);
            point.x >= bounds[0].x
                && point.x <= bounds[1].x
                && point.y >= bounds[0].y
                && point.y <= bounds[2].y
        }
        SelectionShape::Lasso => point_in_polygon(point, selection),
    }
}

fn paths_intersect(
    path: &SelectionPath,
    selection: &[SelectionPoint],
    shape: SelectionShape,
) -> bool {
    if path
        .points
        .iter()
        .any(|point| point_in_selection(*point, selection, shape))
    {
        return true;
    }
    let segment_count = if path.closed {
        path.points.len()
    } else {
        path.points.len().saturating_sub(1)
    };
    (0..segment_count).any(|index| {
        let start = path.points[index];
        let end = path.points[(index + 1) % path.points.len()];
        (0..selection.len()).any(|selection_index| {
            segments_intersect(
                start,
                end,
                selection[selection_index],
                selection[(selection_index + 1) % selection.len()],
            )
        })
    })
}

fn segments_intersect(
    a: SelectionPoint,
    b: SelectionPoint,
    c: SelectionPoint,
    d: SelectionPoint,
) -> bool {
    const EPSILON: f64 = 1e-8;
    let orientation = |p: SelectionPoint, q: SelectionPoint, r: SelectionPoint| {
        (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
    };
    let on_segment = |p: SelectionPoint, q: SelectionPoint, r: SelectionPoint| {
        orientation(p, q, r).abs() <= EPSILON
            && r.x >= p.x.min(q.x) - EPSILON
            && r.x <= p.x.max(q.x) + EPSILON
            && r.y >= p.y.min(q.y) - EPSILON
            && r.y <= p.y.max(q.y) + EPSILON
    };
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    ((ab_c > EPSILON && ab_d < -EPSILON || ab_c < -EPSILON && ab_d > EPSILON)
        && (cd_a > EPSILON && cd_b < -EPSILON || cd_a < -EPSILON && cd_b > EPSILON))
        || on_segment(a, b, c)
        || on_segment(a, b, d)
        || on_segment(c, d, a)
        || on_segment(c, d, b)
}

fn point_in_polygon(point: SelectionPoint, polygon: &[SelectionPoint]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let a = polygon[current];
        let b = polygon[previous];
        let denominator = b.y - a.y;
        let denominator = if denominator.abs() < 1e-9 {
            1e-9
        } else {
            denominator
        };
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / denominator + a.x
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marquee_strict_threshold_and_latched_direction_match_the_electron_contract() {
        let mut marquee = SelectionMarquee::lasso(
            1,
            SelectionPoint::new(10., 10.),
            SelectionOperation::Replace,
        );
        marquee.update(SelectionPoint::new(16., 10.));
        assert!(!marquee.active);
        assert_eq!(marquee.kind, None);
        marquee.update(SelectionPoint::new(17., 30.));
        assert!(marquee.active);
        assert_eq!(marquee.kind, Some(SelectionKind::Window));
        marquee.update(SelectionPoint::new(0., 40.));
        assert_eq!(marquee.resolved_kind(), SelectionKind::Window);
    }

    #[test]
    fn marquee_window_contains_every_point_while_crossing_accepts_an_intersection() {
        let geometry = [SelectionPath::new(
            vec![
                SelectionPoint::new(10., 15.),
                SelectionPoint::new(100., 15.),
            ],
            false,
        )];
        let mut window =
            SelectionMarquee::armed_box(SelectionPoint::new(0., 0.), SelectionOperation::Replace);
        window.update(SelectionPoint::new(50., 30.));
        assert!(!geometry_selected(&geometry, &window));
        let mut crossing =
            SelectionMarquee::armed_box(SelectionPoint::new(50., 30.), SelectionOperation::Replace);
        crossing.update(SelectionPoint::new(0., 0.));
        assert!(geometry_selected(&geometry, &crossing));
    }

    #[test]
    fn marquee_selection_operations_preserve_order_and_alt_wins() {
        assert_eq!(
            selection_after(&["a", "b"], &["b", "c"], SelectionOperation::Add),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            selection_after(&["a", "b"], &["b", "c"], SelectionOperation::Remove),
            vec!["a"]
        );
        assert_eq!(
            SelectionOperation::from_modifiers(true, true),
            SelectionOperation::Remove
        );
    }
}
