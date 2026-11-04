//! The About dialog keeps the dismissal contract every dialog keeps.

use gpui::{
    AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext,
    Window, div,
};
use gpui_component::Root;

#[path = "support/dialog_contract.rs"]
mod dialog_contract;

// Paints Root's dialog layer, as the application's windows do.
struct DialogLayer;

impl Render for DialogLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .children(Root::render_dialog_layer(window, cx))
    }
}

#[gpui::test]
fn the_about_dialog_keeps_the_dialog_dismissal_contract(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let layer = cx.new(|_| DialogLayer);
        Root::new(layer, window, cx)
    });
    let reopen = |cx: &mut gpui::VisualTestContext| {
        cx.update(butter_paper::application_settings::open_about);
    };
    reopen(cx);
    dialog_contract::assert_dialog_dismissal(
        cx,
        "About Butter Paper",
        "application-about-content",
        gpui_component::dialog::DIALOG_CLOSE_BUTTON_SELECTOR,
        dialog_contract::Outside::Closes,
        reopen,
    );
}
