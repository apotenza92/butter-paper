//! Screen-space interaction chrome. These colours never become PDF appearance data.
use crate::selection_geometry::{SelectionKind, SelectionMarquee, SelectionPoint, SelectionShape};
use gpui::{Bounds, Hsla, Path, PathBuilder, Pixels, Point, Window, fill, px, rgb};

pub(super) fn selection_colour() -> Hsla {
    rgb(0x2563eb).into()
}
fn halo_colour() -> Hsla {
    rgb(0xffffff).into()
}

/// Locked handles are visibly inert; they do not imply the active yellow affordance.
#[cfg(test)]
pub(super) fn locked_handle_colour() -> Hsla {
    rgb(0x94a3b8).into()
}

pub(super) fn hover_colour() -> Hsla {
    rgb(0x93c5fd).into()
}

pub(super) fn focus_colour() -> Hsla {
    rgb(0x1d4ed8).into()
}

pub(super) fn draft_colour() -> Hsla {
    rgb(0x0f766e).into()
}

/// Feedback state for one annotation. Active marquee candidates are Hover even
/// when selected, focused or locked. Outside a marquee, callers preserve the
/// existing locked, draft, focused, ordinary-hover and selected precedence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ChromeState {
    Selected,
    Focused,
    Hover,
    Draft,
}

/// Routed Electron feedback precedence; lock suppresses handles, not the outline state.
pub(super) fn feedback_state(
    selected: bool,
    focused: bool,
    hovered: bool,
    draft: bool,
    marquee_candidate: bool,
) -> Option<ChromeState> {
    if marquee_candidate {
        Some(ChromeState::Hover)
    } else if draft {
        Some(ChromeState::Draft)
    } else if selected {
        Some(if focused {
            ChromeState::Focused
        } else {
            ChromeState::Selected
        })
    } else if hovered {
        Some(ChromeState::Hover)
    } else {
        None
    }
}

pub(super) struct ChromeStyle {
    pub colour: Hsla,
    pub width: f32,
    pub dash: Option<(f32, f32)>,
    pub halo: f32,
    pub handle: Option<f32>,
}

/// Style table for the feedback states. Values mirror the Electron chrome roles:
/// selected and focused share dash and handle size but differ in colour and
/// weight; hover is lighter with smaller handles; draft carries no handles.
pub(super) fn chrome_style(state: ChromeState) -> ChromeStyle {
    match state {
        ChromeState::Selected => ChromeStyle {
            colour: selection_colour(),
            width: 1.5,
            dash: Some((5., 4.)),
            halo: 5.,
            handle: Some(7.),
        },
        ChromeState::Focused => ChromeStyle {
            colour: focus_colour(),
            width: 1.75,
            dash: Some((5., 4.)),
            halo: 5.,
            handle: Some(7.),
        },
        ChromeState::Hover => ChromeStyle {
            colour: hover_colour(),
            width: 1.25,
            dash: Some((4., 3.)),
            halo: 4.,
            handle: Some(6.),
        },
        ChromeState::Draft => ChromeStyle {
            colour: draft_colour(),
            width: 1.5,
            dash: Some((6., 4.)),
            halo: 5.,
            handle: None,
        },
    }
}

/// Closed selection bounds sit this far outside the item, so the dashes and
/// halo never cover the item's own outline.
pub(super) const SELECTION_OUTSET_PX: f32 =
    crate::annotation_model::SELECTION_OUTSET_CSS_PX as f32;

/// Paint a screen-space control polygon or rectangle with the reference halo and dash roles.
/// Closed bounds are drawn [`SELECTION_OUTSET_PX`] outside the given outline.
pub(super) fn paint_feedback_path(
    points: &[Point<Pixels>],
    closed: bool,
    state: ChromeState,
    window: &mut Window,
) {
    let outset;
    let points = if closed && points.len() >= 3 {
        outset = outset_polygon(points, SELECTION_OUTSET_PX);
        &outset[..]
    } else {
        points
    };
    for (path, colour) in feedback_paths(points, closed, state) {
        window.paint_path(path, colour);
    }
}

/// Moves each edge of a simple polygon outward by `distance`, joining the
/// moved edges with mitres. Exact for rectangles at any rotation.
fn outset_polygon(points: &[Point<Pixels>], distance: f32) -> Vec<Point<Pixels>> {
    let count = points.len();
    let xy = |point: Point<Pixels>| (f32::from(point.x), f32::from(point.y));
    let signed_area: f32 = (0..count)
        .map(|index| {
            let (x0, y0) = xy(points[index]);
            let (x1, y1) = xy(points[(index + 1) % count]);
            x0 * y1 - x1 * y0
        })
        .sum();
    if !signed_area.is_finite() || signed_area == 0. {
        return points.to_vec();
    }
    // Outward normal of the edge leaving `index`, for either winding.
    let normal = |index: usize| {
        let (x0, y0) = xy(points[index]);
        let (x1, y1) = xy(points[(index + 1) % count]);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let length = dx.hypot(dy);
        if length <= f32::EPSILON {
            return None;
        }
        let (nx, ny) = (dy / length, -dx / length);
        Some(if signed_area > 0. { (nx, ny) } else { (-nx, -ny) })
    };
    (0..count)
        .map(|index| {
            let previous = normal((index + count - 1) % count);
            let next = normal(index);
            let (nx, ny) = match (previous, next) {
                (Some((ax, ay)), Some((bx, by))) => {
                    let (sx, sy) = (ax + bx, ay + by);
                    let dot = sx * ax + sy * ay;
                    if dot <= 0.25 {
                        (bx, by)
                    } else {
                        (sx / dot, sy / dot)
                    }
                }
                (Some(normal), None) | (None, Some(normal)) => normal,
                (None, None) => (0., 0.),
            };
            let (x, y) = xy(points[index]);
            gpui::point(px(x + nx * distance), px(y + ny * distance))
        })
        .collect()
}

fn feedback_paths(
    points: &[Point<Pixels>],
    closed: bool,
    state: ChromeState,
) -> Vec<(Path<Pixels>, Hsla)> {
    if points.len() < 2 {
        return Vec::new();
    }
    let style = chrome_style(state);
    let mut result = Vec::with_capacity(2);
    for halo in [true, false] {
        let mut builder = PathBuilder::stroke(px(if halo { style.halo } else { style.width }));
        if !halo && let Some((dash, gap)) = style.dash {
            builder = builder.dash_array(&[px(dash), px(gap)]);
        }
        builder.move_to(points[0]);
        for point in &points[1..] {
            builder.line_to(*point);
        }
        if closed {
            builder.close();
        }
        if let Ok(path) = builder.build() {
            result.push((
                path,
                if halo {
                    halo_colour().opacity(0.92)
                } else {
                    style.colour
                },
            ));
        }
    }
    result
}

/// Electron selectionLineBoundsPathData: extend both caps and both normals by five pixels.
fn line_feedback_points(start: Point<Pixels>, end: Point<Pixels>) -> Option<[Point<Pixels>; 4]> {
    let dx = f32::from(end.x - start.x);
    let dy = f32::from(end.y - start.y);
    let length = dx.hypot(dy);
    if !length.is_finite() || length <= 0. {
        return None;
    }
    let tx = dx / length * 5.;
    let ty = dy / length * 5.;
    Some([
        gpui::point(start.x - px(tx + ty), start.y + px(tx - ty)),
        gpui::point(end.x + px(tx - ty), end.y + px(ty + tx)),
        gpui::point(end.x + px(tx + ty), end.y + px(ty - tx)),
        gpui::point(start.x + px(ty - tx), start.y - px(ty + tx)),
    ])
}

pub(super) fn paint_line_feedback(
    start: Point<Pixels>,
    end: Point<Pixels>,
    state: ChromeState,
    window: &mut Window,
) {
    // The envelope already clears the line, so it is not outset again.
    if let Some(points) = line_feedback_points(start, end) {
        for (path, colour) in feedback_paths(&points, true, state) {
            window.paint_path(path, colour);
        }
    }
}

/// Resize and point handles are not painted: the item's own corners, edges
/// and vertices are the grab targets, announced by the pointer cursor, and
/// the outset selection outline leaves them visible. Hit-testing is unchanged.
pub(super) fn paint_feedback_handle(
    _center: Point<Pixels>,
    _state: ChromeState,
    _hot: bool,
    _window: &mut Window,
) {
}

pub(super) fn rotate_feedback_point(
    sample: Point<Pixels>,
    center: Point<Pixels>,
    degrees: f64,
) -> Point<Pixels> {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let dx = f64::from(f32::from(sample.x - center.x));
    let dy = f64::from(f32::from(sample.y - center.y));
    gpui::point(
        center.x + px((dx * cos - dy * sin) as f32),
        center.y + px((dx * sin + dy * cos) as f32),
    )
}

/// See [`paint_feedback_handle`]: rotated resize handles are not painted either.
pub(super) fn paint_rotated_feedback_handle(
    _center: Point<Pixels>,
    _degrees: f64,
    _state: ChromeState,
    _hot: bool,
    _window: &mut Window,
) {
}

pub(super) fn rotation_feedback_radius(state: ChromeState, hot: bool) -> f32 {
    ((chrome_style(state).handle.unwrap_or(0.) + if hot { 1. } else { 0. }) * 0.55).max(4.)
}

fn rotation_feedback_quad(
    center: Point<Pixels>,
    state: ChromeState,
    hot: bool,
) -> Option<gpui::PaintQuad> {
    // The rotation knob belongs to the blue selection outline: outlined in
    // the state colour over the halo, and filled with it while hot.
    let style = chrome_style(state);
    style.handle?;
    let border = if hot { 2. } else { style.width };
    let stroke = style.colour;
    let fill_colour = if hot { style.colour } else { halo_colour() };
    let radius = rotation_feedback_radius(state, hot) + border / 2.;
    Some(
        fill(
            Bounds::new(
                center - gpui::point(px(radius), px(radius)),
                gpui::size(px(radius * 2.), px(radius * 2.)),
            ),
            fill_colour,
        )
        .corner_radii(px(radius))
        .border_widths(px(border))
        .border_color(stroke),
    )
}

/// Connector follows the descriptor projection, including its page-rotation quirks.
/// Unlike bounds it has no halo. Caller suppresses both primitives when locked.
pub(super) fn paint_rotation_feedback(
    start: Point<Pixels>,
    end: Point<Pixels>,
    center: Point<Pixels>,
    state: ChromeState,
    hot: bool,
    window: &mut Window,
) {
    let style = chrome_style(state);
    // The stem leaves from the outset selection outline, not the item itself.
    let (dx, dy) = (f32::from(end.x - start.x), f32::from(end.y - start.y));
    let length = dx.hypot(dy);
    if length > SELECTION_OUTSET_PX {
        let scale = SELECTION_OUTSET_PX / length;
        let start = gpui::point(start.x + px(dx * scale), start.y + px(dy * scale));
        let mut builder = PathBuilder::stroke(px(style.width));
        builder.move_to(start);
        builder.line_to(end);
        if let Ok(path) = builder.build() {
            window.paint_path(path, style.colour);
        }
    }
    if let Some(quad) = rotation_feedback_quad(center, state, hot) {
        window.paint_quad(quad);
    }
}

fn marquee_style(kind: SelectionKind) -> (Hsla, f32, bool) {
    match kind {
        SelectionKind::Window => (selection_colour(), 0.13, false),
        SelectionKind::Crossing => (rgb(0x22c55e).into(), 0.14, true),
    }
}

pub(super) fn paint_marquee(
    marquee: &SelectionMarquee,
    project: impl Fn(SelectionPoint) -> Point<Pixels>,
    window: &mut Window,
) {
    for (path, colour) in marquee_paths(marquee, project) {
        window.paint_path(path, colour);
    }
}

fn marquee_paths(
    marquee: &SelectionMarquee,
    project: impl Fn(SelectionPoint) -> Point<Pixels>,
) -> Vec<(Path<Pixels>, Hsla)> {
    if !marquee.active {
        return Vec::new();
    }
    let points = match marquee.shape {
        SelectionShape::Box => vec![
            marquee.start,
            SelectionPoint::new(marquee.current.x, marquee.start.y),
            marquee.current,
            SelectionPoint::new(marquee.start.x, marquee.current.y),
        ],
        SelectionShape::Lasso => marquee.points.clone(),
    };
    if points.len() < 2 {
        return Vec::new();
    }
    let mut paths = Vec::with_capacity(2);
    let (colour, opacity, dashed) = marquee_style(marquee.resolved_kind());
    for filled in [true, false] {
        let mut path = if filled {
            PathBuilder::fill()
        } else {
            PathBuilder::stroke(px(1.))
        };
        if dashed && !filled {
            path = path.dash_array(&[px(7.), px(5.)]);
        }
        path.move_to(project(points[0]));
        for point in &points[1..] {
            path.line_to(project(*point));
        }
        path.close();
        if let Ok(path) = path.build() {
            paths.push((
                path,
                if filled {
                    colour.opacity(opacity)
                } else {
                    colour
                },
            ));
        }
    }
    paths
}

/// Active marquee hits take Hover precedence even over primary selection.
/// The locked flag continues to suppress handles in each domain painter.
#[cfg(test)]
pub(super) fn outline_for_marquee_candidate(
    selected: bool,
    focused: bool,
    hovered: bool,
    draft: bool,
    locked: bool,
    marquee_candidate: bool,
) -> Option<Hsla> {
    if marquee_candidate {
        Some(hover_colour())
    } else {
        outline_for(selected, focused, hovered, draft, locked)
    }
}

/// Resolves the outline colour for one annotation. Precedence: locked-inert,
/// draft, focused primary selection, ordinary hover, plain selection.
/// Returns None when the annotation carries no feedback state. Widths, dashes
/// and handle geometry are a separate parity slice; this resolves colour only.
#[cfg(test)]
pub(super) fn outline_for(
    selected: bool,
    focused: bool,
    hovered: bool,
    draft: bool,
    locked: bool,
) -> Option<Hsla> {
    if locked && (selected || focused || hovered || draft) {
        return Some(locked_handle_colour());
    }
    if draft {
        return Some(draft_colour());
    }
    if focused {
        return Some(focus_colour());
    }
    if hovered {
        return Some(hover_colour());
    }
    if selected {
        return Some(selection_colour());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection_geometry::SelectionOperation;

    #[test]
    fn rotation_knob_is_part_of_the_blue_selection_outline() {
        let center = gpui::point(px(20.), px(30.));
        let circle = rotation_feedback_quad(center, ChromeState::Selected, false).unwrap();
        assert_eq!(circle.border_color, selection_colour());
        assert_eq!(circle.corner_radii.top_left, circle.bounds.size.width / 2.);
        let hot = rotation_feedback_quad(center, ChromeState::Selected, true).unwrap();
        assert_eq!(hot.border_color, selection_colour());
        assert_eq!(hot.border_widths.top, px(2.));
        assert!(rotation_feedback_quad(center, ChromeState::Draft, false).is_none());
    }

    #[test]
    fn closed_selection_bounds_sit_outside_the_item_at_any_rotation() {
        let square = [
            gpui::point(px(10.), px(10.)),
            gpui::point(px(30.), px(10.)),
            gpui::point(px(30.), px(30.)),
            gpui::point(px(10.), px(30.)),
        ];
        let expected = [
            gpui::point(px(4.), px(4.)),
            gpui::point(px(36.), px(4.)),
            gpui::point(px(36.), px(36.)),
            gpui::point(px(4.), px(36.)),
        ];
        assert_eq!(outset_polygon(&square, 6.), expected);
        let mut reversed = square;
        reversed.reverse();
        let mut reversed_expected = expected;
        reversed_expected.reverse();
        assert_eq!(outset_polygon(&reversed, 6.), reversed_expected, "either winding grows outward");
        let diamond = [
            gpui::point(px(20.), px(0.)),
            gpui::point(px(40.), px(20.)),
            gpui::point(px(20.), px(40.)),
            gpui::point(px(0.), px(20.)),
        ];
        let grown = outset_polygon(&diamond, 6.);
        let diagonal = 6. * 2_f32.sqrt();
        assert!((f32::from(grown[0].y) - (0. - diagonal)).abs() < 0.001);
        assert!((f32::from(grown[1].x) - (40. + diagonal)).abs() < 0.001);
    }

    #[test]
    fn feedback_state_preserves_selection_before_ordinary_hover() {
        assert_eq!(
            feedback_state(true, true, true, false, false),
            Some(ChromeState::Focused)
        );
        assert_eq!(
            feedback_state(true, false, true, false, false),
            Some(ChromeState::Selected)
        );
        assert_eq!(feedback_state(false, true, false, false, false), None);
        assert_eq!(
            feedback_state(true, true, true, true, false),
            Some(ChromeState::Draft)
        );
        assert_eq!(
            feedback_state(true, true, true, true, true),
            Some(ChromeState::Hover)
        );
    }

    #[test]
    fn feedback_geometry_emits_closed_line_envelope_and_halo_before_dashes() {
        let start = gpui::point(px(20.), px(30.));
        let end = gpui::point(px(120.), px(30.));
        let corners = line_feedback_points(start, end).unwrap();
        assert_eq!(
            corners,
            [
                gpui::point(px(15.), px(35.)),
                gpui::point(px(125.), px(35.)),
                gpui::point(px(125.), px(25.)),
                gpui::point(px(15.), px(25.))
            ]
        );
        assert!(line_feedback_points(start, start).is_none());
        let oblique = line_feedback_points(start, gpui::point(px(50.), px(70.))).unwrap();
        assert_eq!(oblique[0], gpui::point(px(13.), px(29.)));
        assert_eq!(oblique[2], gpui::point(px(57.), px(71.)));
        let paths = feedback_paths(&corners, true, ChromeState::Focused);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].1, halo_colour().opacity(0.92));
        assert_eq!(paths[1].1, focus_colour());
        assert!(!paths[0].0.vertices.is_empty());
        assert!(
            paths[1].0.vertices.len() > paths[0].0.vertices.len(),
            "dashes must produce distinct stroked segments"
        );
        assert!(
            paths[0].0.bounds.size.height > paths[1].0.bounds.size.height,
            "halo must extend outside the narrower feedback stroke"
        );
        let open = feedback_paths(&corners, false, ChromeState::Focused);
        assert!(
            paths[0].0.vertices.len() > open[0].0.vertices.len(),
            "closed envelope includes the return edge"
        );
    }

    #[test]
    fn marquee_paint_paths_begin_only_after_activation() {
        let project = |point: SelectionPoint| gpui::point(px(point.x as f32), px(point.y as f32));
        for shape in [SelectionShape::Lasso, SelectionShape::Box] {
            let start = SelectionPoint::new(10., 10.);
            let mut marquee = match shape {
                SelectionShape::Lasso => {
                    SelectionMarquee::lasso(1, start, SelectionOperation::Replace)
                }
                SelectionShape::Box => {
                    SelectionMarquee::armed_box(start, SelectionOperation::Replace)
                }
            };
            marquee.update(SelectionPoint::new(13., 10.));
            marquee.update(SelectionPoint::new(13., 13.));
            assert!(!marquee.active);
            assert!(
                marquee_paths(&marquee, project).is_empty(),
                "inactive {shape:?} must emit no fill or stroke"
            );
            marquee.update(SelectionPoint::new(30., 30.));
            let paths = marquee_paths(&marquee, project);
            assert_eq!(paths.len(), 2);
            assert!(paths.iter().all(|(path, _)| !path.vertices.is_empty()));
        }
    }

    #[test]
    fn marquee_paint_paths_preserve_projected_geometry_and_crossing_style() {
        let project = |point: SelectionPoint| {
            gpui::point(
                px(100. + point.x as f32 * 2.),
                px(50. + point.y as f32 * 2.),
            )
        };
        let mut stroke_vertex_counts = Vec::new();
        for (start, end, colour, alpha) in [
            ((10., 10.), (40., 30.), selection_colour(), 0.13),
            (
                (40., 30.),
                (10., 10.),
                marquee_style(SelectionKind::Crossing).0,
                0.14,
            ),
        ] {
            let mut marquee = SelectionMarquee::armed_box(
                SelectionPoint::new(start.0, start.1),
                SelectionOperation::Replace,
            );
            marquee.update(SelectionPoint::new(end.0, end.1));
            let paths = marquee_paths(&marquee, project);
            assert_eq!(paths.len(), 2);
            assert_eq!(paths[0].1, colour.opacity(alpha));
            assert_eq!(paths[1].1, colour);
            assert_eq!(
                paths[0].0.bounds,
                Bounds::new(gpui::point(px(120.), px(70.)), gpui::size(px(60.), px(40.)))
            );
            assert!(!paths[0].0.vertices.is_empty());
            assert!(!paths[1].0.vertices.is_empty());
            stroke_vertex_counts.push(paths[1].0.vertices.len());
        }
        assert!(
            stroke_vertex_counts[1] > stroke_vertex_counts[0],
            "crossing dashes must produce separate stroked segments"
        );
    }

    #[test]
    fn containment_and_crossing_have_distinct_non_colour_cues() {
        let (window, alpha, dashed) = marquee_style(SelectionKind::Window);
        assert_eq!(window, selection_colour());
        assert_eq!(alpha, 0.13);
        assert!(!dashed);
        let (crossing, alpha, dashed) = marquee_style(SelectionKind::Crossing);
        assert_ne!(crossing, window);
        assert_eq!(alpha, 0.14);
        assert!(dashed);
        assert_ne!(locked_handle_colour(), selection_colour());
    }

    #[test]
    fn feedback_states_carry_distinct_reference_roles() {
        assert_ne!(hover_colour(), selection_colour());
        assert_ne!(focus_colour(), selection_colour());
        assert_ne!(draft_colour(), selection_colour());
        assert_ne!(draft_colour(), hover_colour());

        let selected = chrome_style(ChromeState::Selected);
        assert_eq!(selected.colour, selection_colour());
        assert_eq!(selected.width, 1.5);
        assert_eq!(selected.dash, Some((5., 4.)));
        assert_eq!(selected.handle, Some(7.));

        let focused = chrome_style(ChromeState::Focused);
        assert_eq!(focused.colour, focus_colour());
        assert_eq!(focused.width, 1.75);
        assert_eq!(focused.dash, selected.dash);
        assert_eq!(focused.handle, selected.handle);

        let hover = chrome_style(ChromeState::Hover);
        assert_eq!(hover.colour, hover_colour());
        assert_eq!(hover.width, 1.25);
        assert_eq!(hover.dash, Some((4., 3.)));
        assert_eq!(hover.handle, Some(6.));

        let draft = chrome_style(ChromeState::Draft);
        assert_eq!(draft.colour, draft_colour());
        assert_eq!(draft.handle, None);
    }

    #[test]
    fn marquee_candidate_feedback_overrides_focus_selection_and_lock_only_for_hits() {
        for selected in [false, true] {
            for focused in [false, true] {
                for locked in [false, true] {
                    assert_eq!(
                        outline_for_marquee_candidate(
                            selected, focused, false, false, locked, true
                        ),
                        Some(hover_colour())
                    );
                    assert_eq!(
                        outline_for_marquee_candidate(
                            selected, focused, false, false, locked, false
                        ),
                        outline_for(selected, focused, false, false, locked)
                    );
                }
            }
        }
    }

    #[test]
    fn outline_resolution_follows_feedback_precedence() {
        assert_eq!(outline_for(false, false, false, false, false), None);
        assert_eq!(
            outline_for(true, false, false, false, false),
            Some(selection_colour())
        );
        assert_eq!(
            outline_for(true, true, true, false, false),
            Some(focus_colour())
        );
        assert_eq!(
            outline_for(true, false, true, false, false),
            Some(hover_colour())
        );
        assert_eq!(
            outline_for(true, false, false, true, false),
            Some(draft_colour())
        );
        assert_eq!(
            outline_for(true, true, true, true, true),
            Some(locked_handle_colour())
        );
        assert_eq!(outline_for(false, false, false, false, true), None);
    }
}
