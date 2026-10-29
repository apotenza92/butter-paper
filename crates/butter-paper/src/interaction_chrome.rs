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
pub(super) fn locked_handle_colour() -> Hsla {
    rgb(0x94a3b8).into()
}

pub(super) fn hover_colour() -> Hsla {
    rgb(0x93c5fd).into()
}

#[cfg(test)]
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
    /// A locked markup's handles: shown, but inert.
    Locked,
}

/// The state a markup's handles take: locked markups show inert handles in
/// place of active ones.
pub(super) fn handle_state(state: ChromeState, locked: bool) -> ChromeState {
    if locked && state != ChromeState::Draft {
        ChromeState::Locked
    } else {
        state
    }
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
    /// Handle border width.
    pub width: f32,
    /// Handle size; drafts carry no handles.
    pub handle: Option<f32>,
}

/// Style table for the feedback states: every selected markup's handles look
/// the same, including the one with keyboard focus; hover is lighter with
/// smaller handles; locked handles are grey; drafts carry no handles. No state
/// draws a line over or around the markup, so its own colour and thickness
/// stay visible.
pub(super) fn chrome_style(state: ChromeState) -> ChromeStyle {
    match state {
        ChromeState::Selected | ChromeState::Focused => ChromeStyle {
            colour: selection_colour(),
            width: 1.5,
            handle: Some(7.),
        },
        ChromeState::Hover => ChromeStyle {
            colour: hover_colour(),
            width: 1.25,
            handle: Some(6.),
        },
        ChromeState::Locked => ChromeStyle {
            colour: locked_handle_colour(),
            width: 1.25,
            handle: Some(6.),
        },
        ChromeState::Draft => ChromeStyle {
            colour: draft_colour(),
            width: 1.5,
            handle: None,
        },
    }
}

/// The rotation knob's stem starts this far outside the markup.
pub(super) const SELECTION_OUTSET_PX: f32 = crate::annotation_model::SELECTION_OUTSET_CSS_PX as f32;

/// A handle: a small square in the state colour over white, filled while hot.
fn handle_paths(
    center: Point<Pixels>,
    degrees: f64,
    state: ChromeState,
    hot: bool,
) -> Vec<(Path<Pixels>, Hsla)> {
    let style = chrome_style(state);
    let Some(size) = style.handle else {
        return Vec::new();
    };
    let hot = hot && state != ChromeState::Locked;
    let half = (size + if hot { 2. } else { 0. }) / 2.;
    let corners = [(-half, -half), (half, -half), (half, half), (-half, half)]
        .map(|(x, y)| rotate_feedback_point(center + gpui::point(px(x), px(y)), center, degrees));
    let mut result = Vec::with_capacity(2);
    for filled in [true, false] {
        let mut builder = if filled {
            PathBuilder::fill()
        } else {
            PathBuilder::stroke(px(if hot { 2. } else { style.width }))
        };
        builder.move_to(corners[0]);
        for corner in &corners[1..] {
            builder.line_to(*corner);
        }
        builder.close();
        if let Ok(path) = builder.build() {
            result.push((
                path,
                if filled && !hot {
                    halo_colour()
                } else {
                    style.colour
                },
            ));
        }
    }
    result
}

/// Paints a resize or point handle at one of the markup's grab targets.
pub(super) fn paint_feedback_handle(
    center: Point<Pixels>,
    state: ChromeState,
    hot: bool,
    window: &mut Window,
) {
    paint_rotated_feedback_handle(center, 0., state, hot, window);
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

/// A handle or rotation knob held back to paint over every markup.
enum DeferredChrome {
    Handle {
        center: Point<Pixels>,
        degrees: f64,
        state: ChromeState,
        hot: bool,
    },
    Rotation {
        start: Point<Pixels>,
        end: Point<Pixels>,
        center: Point<Pixels>,
        state: ChromeState,
        hot: bool,
    },
}

thread_local! {
    static DEFERRED_CHROME: std::cell::RefCell<Option<Vec<DeferredChrome>>> =
        const { std::cell::RefCell::new(None) };
}

/// From now until [`paint_deferred_chrome`], handles and rotation knobs are
/// held back, so a markup painted later never covers another's handles.
pub(super) fn defer_chrome() {
    DEFERRED_CHROME.with(|deferred| *deferred.borrow_mut() = Some(Vec::new()));
}

/// Paints the handles and knobs held back since [`defer_chrome`], above
/// everything painted since.
pub(super) fn paint_deferred_chrome(window: &mut Window) {
    let held = DEFERRED_CHROME.with(|deferred| deferred.borrow_mut().take());
    for chrome in held.into_iter().flatten() {
        match chrome {
            DeferredChrome::Handle {
                center,
                degrees,
                state,
                hot,
            } => paint_handle_now(center, degrees, state, hot, window),
            DeferredChrome::Rotation {
                start,
                end,
                center,
                state,
                hot,
            } => paint_rotation_now(start, end, center, state, hot, window),
        }
    }
}

/// Holds `chrome` back if painting is deferred; otherwise returns it.
fn hold(chrome: DeferredChrome) -> Option<DeferredChrome> {
    DEFERRED_CHROME.with(|deferred| match deferred.borrow_mut().as_mut() {
        Some(held) => {
            held.push(chrome);
            None
        }
        None => Some(chrome),
    })
}

/// A handle turned with its markup's rotation.
pub(super) fn paint_rotated_feedback_handle(
    center: Point<Pixels>,
    degrees: f64,
    state: ChromeState,
    hot: bool,
    window: &mut Window,
) {
    if hold(DeferredChrome::Handle {
        center,
        degrees,
        state,
        hot,
    })
    .is_some()
    {
        paint_handle_now(center, degrees, state, hot, window);
    }
}

fn paint_handle_now(
    center: Point<Pixels>,
    degrees: f64,
    state: ChromeState,
    hot: bool,
    window: &mut Window,
) {
    if degrees.rem_euclid(90.).abs() <= f64::EPSILON {
        if let Some(quad) = handle_quad(center, state, hot) {
            window.paint_quad(quad);
        }
        return;
    }
    for (path, colour) in handle_paths(center, degrees, state, hot) {
        window.paint_path(path, colour);
    }
}

/// An upright handle, as one quad: white with a state-coloured border, or
/// filled while hot.
fn handle_quad(center: Point<Pixels>, state: ChromeState, hot: bool) -> Option<gpui::PaintQuad> {
    let style = chrome_style(state);
    let size = style.handle?;
    let hot = hot && state != ChromeState::Locked;
    let half = (size + if hot { 2. } else { 0. }) / 2.;
    Some(
        fill(
            Bounds::new(
                center - gpui::point(px(half), px(half)),
                gpui::size(px(half * 2.), px(half * 2.)),
            ),
            if hot { style.colour } else { halo_colour() },
        )
        .border_widths(px(if hot { 2. } else { style.width }))
        .border_color(style.colour),
    )
}

pub(super) fn rotation_feedback_radius(state: ChromeState, hot: bool) -> f32 {
    ((chrome_style(state).handle.unwrap_or(0.) + if hot { 1. } else { 0. }) * 0.55).max(4.)
}

fn rotation_feedback_quad(
    center: Point<Pixels>,
    state: ChromeState,
    hot: bool,
) -> Option<gpui::PaintQuad> {
    // The rotation knob matches the handles: outlined in the state colour
    // over white, and filled with it while hot.
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
/// Caller suppresses both primitives when locked.
pub(super) fn paint_rotation_feedback(
    start: Point<Pixels>,
    end: Point<Pixels>,
    center: Point<Pixels>,
    state: ChromeState,
    hot: bool,
    window: &mut Window,
) {
    if hold(DeferredChrome::Rotation {
        start,
        end,
        center,
        state,
        hot,
    })
    .is_some()
    {
        paint_rotation_now(start, end, center, state, hot, window);
    }
}

fn paint_rotation_now(
    start: Point<Pixels>,
    end: Point<Pixels>,
    center: Point<Pixels>,
    state: ChromeState,
    hot: bool,
    window: &mut Window,
) {
    if let Some(path) = rotation_connector_path(start, end, state) {
        window.paint_path(path, chrome_style(state).colour);
    }
    if let Some(quad) = rotation_feedback_quad(center, state, hot) {
        window.paint_quad(quad);
    }
}

fn rotation_connector_path(
    start: Point<Pixels>,
    end: Point<Pixels>,
    state: ChromeState,
) -> Option<Path<Pixels>> {
    // A thin solid stem that leaves from just outside the item, not over it.
    let (dx, dy) = (f32::from(end.x - start.x), f32::from(end.y - start.y));
    let length = dx.hypot(dy);
    if length > SELECTION_OUTSET_PX {
        let scale = SELECTION_OUTSET_PX / length;
        let start = gpui::point(start.x + px(dx * scale), start.y + px(dy * scale));
        chrome_style(state).handle?;
        let mut builder = PathBuilder::stroke(px(1.));
        builder.move_to(start);
        builder.line_to(end);
        if let Ok(path) = builder.build() {
            return Some(path);
        }
    }
    None
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
    fn rotation_connector_is_a_solid_stem_that_clears_the_markup() {
        let start = gpui::point(px(20.), px(100.));
        let end = gpui::point(px(20.), px(0.));
        let path = rotation_connector_path(start, end, ChromeState::Selected).unwrap();
        assert!(
            path.vertices.len() <= 6,
            "the stem is one solid segment, not dashes: {} vertices",
            path.vertices.len()
        );
        assert!(
            f32::from(path.bounds.bottom()) <= 95.,
            "stem starts outside the markup"
        );
        assert!(rotation_connector_path(start, start, ChromeState::Selected).is_none());
        assert!(rotation_connector_path(start, end, ChromeState::Draft).is_none());
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
    fn handles_are_squares_over_white_filled_while_hot_and_inert_when_locked() {
        let center = gpui::point(px(50.), px(50.));
        let paths = handle_paths(center, 0., ChromeState::Selected, false);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].1, halo_colour(), "the square is filled white");
        assert_eq!(
            paths[1].1,
            selection_colour(),
            "and outlined in the selection blue"
        );
        let width = f32::from(paths[0].0.bounds.size.width);
        assert!(
            (width - 7.).abs() < 0.5,
            "a selected handle is 7px: {width}"
        );

        let hot = handle_paths(center, 0., ChromeState::Selected, true);
        assert_eq!(hot[0].1, selection_colour(), "a hot handle is filled");
        assert!(hot[0].0.bounds.size.width > paths[0].0.bounds.size.width);

        let locked = handle_paths(center, 0., ChromeState::Locked, true);
        assert_eq!(
            locked[0].1,
            halo_colour(),
            "a locked handle never turns hot"
        );
        assert_eq!(locked[1].1, locked_handle_colour());

        let turned = handle_paths(center, 45., ChromeState::Selected, false);
        assert!(
            turned[0].0.bounds.size.width > paths[0].0.bounds.size.width,
            "a handle turns with its markup"
        );
        assert!(handle_paths(center, 0., ChromeState::Draft, false).is_empty());
    }

    #[test]
    fn handle_state_shows_locked_markups_as_inert() {
        assert_eq!(
            handle_state(ChromeState::Selected, true),
            ChromeState::Locked
        );
        assert_eq!(handle_state(ChromeState::Hover, true), ChromeState::Locked);
        assert_eq!(
            handle_state(ChromeState::Focused, false),
            ChromeState::Focused
        );
        assert_eq!(handle_state(ChromeState::Draft, true), ChromeState::Draft);
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
        assert_ne!(locked_handle_colour(), selection_colour());

        let selected = chrome_style(ChromeState::Selected);
        assert_eq!(selected.colour, selection_colour());
        assert_eq!(selected.width, 1.5);
        assert_eq!(selected.handle, Some(7.));

        let focused = chrome_style(ChromeState::Focused);
        assert_eq!(
            (focused.colour, focused.width, focused.handle),
            (selected.colour, selected.width, selected.handle),
            "focus must not make one selected markup's handles look different"
        );

        let hover = chrome_style(ChromeState::Hover);
        assert_eq!(hover.colour, hover_colour());
        assert_eq!(hover.handle, Some(6.));

        assert_eq!(
            chrome_style(ChromeState::Locked).colour,
            locked_handle_colour()
        );
        assert_eq!(chrome_style(ChromeState::Draft).handle, None);
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
