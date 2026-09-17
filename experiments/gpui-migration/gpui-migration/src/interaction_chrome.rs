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

pub(super) fn outline_colour(locked: bool) -> Hsla {
    if locked {
        locked_handle_colour()
    } else {
        selection_colour()
    }
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
        assert_eq!(outline_colour(true), locked_handle_colour());
        assert_eq!(outline_colour(false), selection_colour());
    }
}
