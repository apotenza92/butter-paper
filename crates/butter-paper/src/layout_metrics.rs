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
}
