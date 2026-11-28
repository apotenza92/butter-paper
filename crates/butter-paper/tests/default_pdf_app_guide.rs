//! Set as Default PDF App… on Windows: the guide opens first, its Open
//! Settings button calls the Settings opener and leaves the guide open, and
//! the guide keeps the dismissal contract every dialog keeps.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use butter_paper::default_pdf_app_guide::{
    DEFAULT_PDF_APP_GUIDE_ID, DEFAULT_PDF_APP_OPEN_SETTINGS_ID, DefaultPdfAppFlow, SettingsOpener,
    set_as_default_pdf_app,
};
use gpui::{
    AppContext as _, Context, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers,
    ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window, div, px,
    size,
};
use gpui_component::{Root, WindowExt as _};

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

fn window(cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let (_, cx) = cx.add_window_view(|window, cx| {
        let layer = cx.new(|_| DialogLayer);
        Root::new(layer, window, cx)
    });
    cx
}

fn counting_opener(result: Result<(), String>) -> (SettingsOpener, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let opener: SettingsOpener = Arc::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        result.clone()
    });
    (opener, calls)
}

fn open(cx: &mut VisualTestContext, opener: &SettingsOpener) {
    let opener = opener.clone();
    cx.update(move |window, cx| {
        set_as_default_pdf_app(DefaultPdfAppFlow::Guide(opener), window, cx)
    });
}

fn is_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| window.has_active_dialog(cx))
}

/// Presses Space on the focused control. A button acts on the key-up,
/// which `simulate_keystrokes` does not send.
fn press_space(cx: &mut VisualTestContext) {
    let keystroke = Keystroke::parse("space").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    cx.run_until_parked();
    dialog_contract::settle(cx);
}

#[gpui::test]
fn the_default_pdf_app_guide_keeps_the_dialog_dismissal_contract(cx: &mut TestAppContext) {
    let cx = window(cx);
    let (opener, calls) = counting_opener(Ok(()));
    open(cx, &opener);
    let reopen = move |cx: &mut VisualTestContext| open(cx, &opener);
    dialog_contract::assert_dialog_dismissal(
        cx,
        "Default PDF app guide",
        DEFAULT_PDF_APP_GUIDE_ID,
        gpui_component::dialog::DIALOG_CLOSE_BUTTON_SELECTOR,
        dialog_contract::Outside::Closes,
        reopen,
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "closing never opens Settings"
    );
}

#[gpui::test]
fn the_windows_flow_opens_the_guide_and_its_button_opens_settings(cx: &mut TestAppContext) {
    let cx = window(cx);
    let (opener, calls) = counting_opener(Ok(()));
    open(cx, &opener);
    dialog_contract::settle(cx);
    assert!(is_open(cx), "the guide opens first");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "Settings waits for the button"
    );

    let button = cx
        .debug_bounds(DEFAULT_PDF_APP_OPEN_SETTINGS_ID)
        .expect("Open Settings is painted");
    cx.simulate_click(button.center(), Modifiers::none());
    cx.run_until_parked();
    dialog_contract::settle(cx);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "Open Settings calls the opener"
    );
    assert!(is_open(cx), "the guide stays open beside Settings");
}

#[gpui::test]
fn the_guide_starts_on_open_settings_and_tab_reaches_every_control(cx: &mut TestAppContext) {
    let cx = window(cx);
    let (opener, calls) = counting_opener(Ok(()));
    open(cx, &opener);
    dialog_contract::settle(cx);
    press_space(cx);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "focus starts on Open Settings"
    );
    assert!(is_open(cx));

    // Tab moves to the X, the guide's only other control: Space no longer
    // presses Open Settings. (The stock X closes on a pointer click; Escape
    // closes the guide from the keyboard.)
    cx.simulate_keystrokes("tab");
    dialog_contract::settle(cx);
    press_space(cx);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "Tab leaves Open Settings");
    assert!(is_open(cx), "focus stays in the guide");

    // A second Tab comes back to Open Settings: focus cycles in the guide.
    cx.simulate_keystrokes("tab");
    dialog_contract::settle(cx);
    press_space(cx);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "Tab cycles back to Open Settings"
    );
    assert!(is_open(cx));

    cx.simulate_keystrokes("escape");
    dialog_contract::settle(cx);
    assert!(!is_open(cx), "Escape closes it");
}

#[gpui::test]
fn a_failed_settings_opener_reports_the_error_and_keeps_the_guide(cx: &mut TestAppContext) {
    let cx = window(cx);
    let (opener, calls) = counting_opener(Err("error 5".into()));
    open(cx, &opener);
    dialog_contract::settle(cx);
    let button = cx.debug_bounds(DEFAULT_PDF_APP_OPEN_SETTINGS_ID).unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(cx.has_pending_prompt(), "a failure shows the error prompt");
    cx.simulate_prompt_answer("OK");
    dialog_contract::settle(cx);
    assert!(is_open(cx), "the guide stays open after the error");
}

#[gpui::test]
fn the_guide_stays_inside_a_narrow_window(cx: &mut TestAppContext) {
    let cx = window(cx);
    cx.simulate_resize(size(px(360.), px(560.)));
    let (opener, _) = counting_opener(Ok(()));
    open(cx, &opener);
    dialog_contract::settle(cx);
    let viewport = cx.update(|window, _| window.viewport_size());
    for selector in [
        DEFAULT_PDF_APP_GUIDE_ID,
        DEFAULT_PDF_APP_OPEN_SETTINGS_ID,
        gpui_component::dialog::DIALOG_CLOSE_BUTTON_SELECTOR,
    ] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(
            bounds.left() >= px(0.) && bounds.right() <= viewport.width,
            "{selector} stays inside a 360 px window: {bounds:?}"
        );
    }
    // The body scrolls; the X and Open Settings stay on screen.
    for selector in [
        DEFAULT_PDF_APP_OPEN_SETTINGS_ID,
        gpui_component::dialog::DIALOG_CLOSE_BUTTON_SELECTOR,
    ] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(
            bounds.top() >= px(0.) && bounds.bottom() <= viewport.height,
            "{selector} stays on screen in a 560 px tall window: {bounds:?}"
        );
    }
}
