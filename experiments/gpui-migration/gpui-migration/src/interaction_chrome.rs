//! Screen-space interaction chrome. These colours never become PDF appearance data.
use crate::selection_geometry::{SelectionKind, SelectionMarquee, SelectionPoint, SelectionShape};
use gpui::{Bounds, Hsla, PathBuilder, Pixels, Point, Window, fill, px, rgb};

pub(super) fn selection_colour() -> Hsla {
    rgb(0x2563eb).into()
}
fn handle_colour() -> Hsla {
    rgb(0xfacc15).into()
}
fn handle_outline() -> Hsla {
    rgb(0x111827).into()
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

pub(super) fn focus_colour() -> Hsla {
    rgb(0x1d4ed8).into()
}

pub(super) fn draft_colour() -> Hsla {
    rgb(0x0f766e).into()
}

/// Feedback state for one annotation. Precedence (highest first) is resolved by
/// callers: locked-inert, draft, focused primary selection, hover candidate,
/// plain selection. Mirrors the Electron interaction-state roles.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ChromeState {
    Selected,
    Focused,
    Hover,
    Draft,
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

pub(super) fn paint_handle(bounds: Bounds<Pixels>, locked: bool, window: &mut Window) {
    if locked {
        return;
    }
    // A white halo and dark border keep the yellow dot visible on both paper and ink.
    window.paint_quad(fill(bounds.dilate(px(1.)), halo_colour()).corner_radii(px(5.)));
    window.paint_quad(
        fill(bounds, handle_colour())
            .corner_radii(px(4.))
            .border_widths(px(1.))
            .border_color(handle_outline()),
    );
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
        return;
    }
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
            window.paint_path(
                path,
                if filled {
                    colour.opacity(opacity)
                } else {
                    colour
                },
            );
        }
    }
}

/// Resolves the outline colour for one annotation. Precedence: locked-inert,
/// draft, focused primary selection, hover candidate, plain selection.
/// Returns None when the annotation carries no feedback state. Widths, dashes
/// and handle geometry are a separate parity slice; this resolves colour only.
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
        assert_ne!(handle_colour(), selection_colour());
        assert_ne!(handle_outline(), handle_colour());
        assert_ne!(locked_handle_colour(), handle_colour());
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
