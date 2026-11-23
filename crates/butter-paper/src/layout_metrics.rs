//! The app's spacing scale: an 8pt grid.
//!
//! Controls are 32px. Every edge and every gap between controls is
//! [`SPACE`]; groups of controls are [`GROUP_SPACE`] apart. A bar's stated
//! size includes its border line, which is drawn inside the bar (an inside
//! stroke, [`inside_line`]) and takes no layout space, so a 48px bar is
//! 8 + 32 + 8 with the line in the last pixel of one 8px gap.

use gpui::{Div, Hsla, Styled, div, px};

/// The space at every edge and between controls.
pub const SPACE: f32 = 8.;
/// The space between groups of controls: two units.
pub const GROUP_SPACE: f32 = 16.;
/// A medium control: buttons, tabs and toolbar groups.
pub const CONTROL: f32 = 32.;
/// A bar that holds controls: the tab row, the viewer toolbar, and the
/// rails' widths: one unit, a control, one unit.
pub const BAR: f32 = SPACE + CONTROL + SPACE;
/// The status bar: one unit, its 20px extra-small controls, one unit.
pub const STATUS_BAR: f32 = SPACE + 20. + SPACE;
/// The room a clipping region (a scrolling dialog body) leaves inside its
/// edge, taken back with a negative margin so the layout does not move:
/// GPUI Component paints a focused control's 3px ring outside its border,
/// and a clip at the control's edge cuts the ring off.
pub const FOCUS_RING_ROOM: f32 = 4.;

/// The design's size `value`, given in points at the default interface size
/// (16 points to the rem), as a length that scales with the interface size.
/// Bars, rails, tabs and their spacing use it, so they grow with the text
/// and controls they hold.
pub fn scaled(value: f32) -> gpui::Rems {
    gpui::rems(value / 16.)
}

/// As [`scaled`], in pixels for a window whose rem is `rem`, for APIs that
/// take pixels.
pub fn scaled_px(value: f32, rem: gpui::Pixels) -> gpui::Pixels {
    rem * (value / 16.)
}

/// The edge of a bar its line runs along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// A 1px line along `edge`, drawn inside its parent (which must be
/// positioned) without taking layout space.
pub fn inside_line(edge: Edge, color: Hsla) -> Div {
    let line = div().absolute().bg(color);
    // Sized outright, not by opposite offsets alone.
    match edge {
        Edge::Top => line.top_0().left_0().w_full().h(px(1.)),
        Edge::Bottom => line.bottom_0().left_0().w_full().h(px(1.)),
        Edge::Left => line.top_0().left_0().h_full().w(px(1.)),
        Edge::Right => line.top_0().right_0().h_full().w(px(1.)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_whole_units_of_the_grid() {
        assert_eq!(BAR, 48.);
        assert_eq!(STATUS_BAR, 36.);
        for value in [SPACE, GROUP_SPACE, CONTROL, BAR] {
            assert_eq!(value % 8., 0., "{value} is on the 8pt grid");
        }
    }

    /// The interface size stays at its default for now (decisions.md,
    /// "Interface size"), where a rem is 16 pixels. There, every size given
    /// through `scaled` or `scaled_px` is exactly the pixel size it replaced:
    /// dividing and multiplying by 16 loses nothing.
    #[test]
    fn scaled_sizes_are_exact_pixels_at_the_default_interface_size() {
        let rem = gpui::px(16.);
        assert_eq!(
            crate::application_shell::default_application_ui_font_size(),
            rem
        );
        let mut value = -1024.;
        while value <= 2048. {
            assert_eq!(scaled(value).to_pixels(rem), gpui::px(value), "{value}");
            assert_eq!(scaled_px(value, rem), gpui::px(value), "{value}");
            value += 0.0625;
        }
        for value in [5.5, 9.1, 13.3, 18.75, 25.25, 356. + 2. * FOCUS_RING_ROOM] {
            assert_eq!(scaled(value).to_pixels(rem), gpui::px(value), "{value}");
            assert_eq!(scaled_px(value, rem), gpui::px(value), "{value}");
        }
    }
}
