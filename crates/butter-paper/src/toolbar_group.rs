//! The frame and dividers of a joined group of toolbar controls. A stock
//! button group (Undo and Redo, Fit Width and Fit Page, the page-view and
//! Page Scale splits) draws a 1 px line between every pair of members with
//! its buttons' borders, in the outline buttons' border colour. A group made
//! of other controls (First, Previous, the page field, Next and Last; Zoom
//! Out, Zoom In and the zoom menu) draws the same line with
//! [`toolbar_divider`], so no group has members with no line between them.

use gpui::{AnyElement, App, InteractiveElement as _, IntoElement as _, Styled as _, div, px};
use gpui_component::{ActiveTheme as _, Colorize as _, h_flex};

/// A group of toolbar controls in one frame, as the fit and page-view
/// buttons are. Its buttons reach the frame: the end ones' hover and press
/// fill it to its rounded edge, as a stock button group's do.
pub(crate) fn toolbar_group(cx: &App) -> gpui::Div {
    let theme = cx.theme();
    h_flex()
        .h_8()
        .items_center()
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius)
        .overflow_hidden()
}

/// The debug selector of the divider after member `index` of `group`.
pub fn toolbar_divider_id(group: &str, index: usize) -> String {
    format!("{group}-divider-{index}")
}

/// The 1 px line between two members of a group: the group's full height,
/// in the colour of a stock outline button's border, which is the colour
/// of a stock button group's own lines.
pub(crate) fn toolbar_divider(group: &'static str, index: usize, cx: &App) -> gpui::Div {
    div()
        .debug_selector(move || toolbar_divider_id(group, index))
        .flex_none()
        .w(px(1.))
        .self_stretch()
        .bg(cx.theme().input)
}

/// `members` in order with a divider between every adjacent pair.
pub(crate) fn divided(group: &'static str, members: Vec<AnyElement>, cx: &App) -> Vec<AnyElement> {
    let mut out = Vec::with_capacity(members.len() * 2);
    for (index, member) in members.into_iter().enumerate() {
        if index > 0 {
            out.push(toolbar_divider(group, index - 1, cx).into_any_element());
        }
        out.push(member);
    }
    out
}

/// Where the seam of a page-view split button lies, in rems from its left:
/// the first half is 38 px wide, the seam its last pixel.
pub(crate) const SPLIT_SEAM_LEFT_REMS: f32 = 2.3125;

/// The line at the seam of a selected split button (the page-view buttons,
/// whose two halves are both selected). A stock outline button that is
/// selected fills with its border's colour, so the divider that every joined
/// group shows would vanish into the fill; this line is that colour a step
/// towards the text colour, so the seam still reads. Place it in the split's
/// own relative box: it sits on the first half's right edge.
pub(crate) fn selected_seam(cx: &App) -> gpui::Div {
    let theme = cx.theme();
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(gpui::rems(SPLIT_SEAM_LEFT_REMS))
        .w(px(1.))
        .bg(theme.input.mix_oklab(theme.foreground, 0.8))
}
