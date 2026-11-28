//! Set as Default PDF App…: the command's flow on each platform.
//!
//! macOS and Linux change the default directly and report the result.
//! Windows does not let an app make itself the default, so the command opens
//! a guide first: a sketch of the Default Apps steps and an Open Settings
//! button. The guide stays open, so the steps can be read beside Settings.

use std::{cell::Cell, rc::Rc, sync::Arc};

use gpui::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    PromptLevel, SharedString, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
    h_flex, v_flex,
};

use crate::default_pdf_app::PRODUCT_NAME;
use crate::layout_metrics::scaled_px;

/// The guide's content, for tests and review scripts.
pub const DEFAULT_PDF_APP_GUIDE_ID: &str = "default-pdf-app-guide";
/// The guide's Open Settings button.
pub const DEFAULT_PDF_APP_OPEN_SETTINGS_ID: &str = "default-pdf-app-open-settings";

/// The steps, in the order the picture numbers them.
pub const DEFAULT_PDF_APP_STEPS: [&str; 3] = [
    "Under .pdf, click the app that is shown.",
    "Choose Butter Paper.",
    "Click Set default.",
];

const ERROR_MESSAGE: &str = "Butter Paper could not be set as the default PDF app";

/// Opens Windows Settings at the Default Apps page. Tests and review builds
/// replace the real call, which exists only on Windows.
pub type SettingsOpener = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

/// How Set as Default PDF App… works on this platform.
pub enum DefaultPdfAppFlow {
    /// Change the default and report the result (macOS and Linux).
    Direct,
    /// Show the guide, whose button calls the opener (Windows).
    Guide(SettingsOpener),
}

impl DefaultPdfAppFlow {
    pub fn current() -> Self {
        // A review build shows the guide on any platform with
        // BP_REVIEW_DEFAULT_PDF_GUIDE set; off Windows, its button only logs.
        #[cfg(all(feature = "review-driver", not(target_os = "windows")))]
        if std::env::var_os("BP_REVIEW_DEFAULT_PDF_GUIDE").is_some() {
            return Self::Guide(Arc::new(|| {
                eprintln!("review: Open Settings pressed (Windows Settings opens on Windows)");
                Ok(())
            }));
        }
        if cfg!(target_os = "windows") {
            Self::Guide(Arc::new(crate::default_pdf_app::open_windows_default_apps))
        } else {
            Self::Direct
        }
    }
}

/// Runs Set as Default PDF App… in `window`.
pub fn set_as_default_pdf_app(flow: DefaultPdfAppFlow, window: &mut Window, cx: &mut App) {
    match flow {
        DefaultPdfAppFlow::Guide(opener) => open_guide(opener, window, cx),
        DefaultPdfAppFlow::Direct => {
            let task = cx
                .background_executor()
                .spawn(async { crate::default_pdf_app::set_as_default_pdf_app() });
            let handle = window.window_handle();
            cx.spawn(async move |cx| {
                let result = task.await;
                let _ = handle.update(cx, |_, window, cx| match result {
                    Ok(result) => {
                        // The only button is OK, so the answer is not needed.
                        drop(window.prompt(PromptLevel::Info, &result.message, None, &["OK"], cx));
                    }
                    Err(error) => show_error(&error, window, cx),
                });
            })
            .detach();
        }
    }
}

fn show_error(error: &str, window: &mut Window, cx: &mut App) {
    drop(window.prompt(
        PromptLevel::Warning,
        ERROR_MESSAGE,
        Some(error),
        &["OK"],
        cx,
    ));
}

/// Opens the guide. Its button calls `opener` and leaves the guide open.
pub fn open_guide(opener: SettingsOpener, window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    // Focus starts on Open Settings: the button's own focus handle is
    // private to it, so the guide focuses a wrapper just before it in tab
    // order and moves to the next stop, once the button has painted.
    let button_slot = cx.focus_handle();
    let needs_focus = Rc::new(Cell::new(true));
    window.open_dialog(cx, move |dialog, window, cx| {
        if needs_focus.get() {
            let (slot, needs_focus) = (button_slot.clone(), needs_focus.clone());
            window.defer(cx, move |window, cx| {
                if !needs_focus.get() {
                    return;
                }
                slot.focus(window, cx);
                window.focus_next(cx);
                if slot.contains_focused(window, cx) && !slot.is_focused(window) {
                    needs_focus.set(false);
                }
            });
        }
        // The viewport of a client-decorated window includes its shadow
        // margin, which is outside the frame the user sees.
        let margin = gpui_component::window_paddings(window);
        let visible = window.viewport_size().width - margin.left - margin.right;
        let rem = window.rem_size();
        let width = scaled_px(500., rem).min(visible - px(32.));
        // Below this the two sketches stack, joined by a down arrow.
        let stacked = width < scaled_px(460., rem);
        let opener = opener.clone();
        dialog
            .title(format!("Set {PRODUCT_NAME} as the default PDF app"))
            .w(width)
            .child(
                v_flex()
                    .id(DEFAULT_PDF_APP_GUIDE_ID)
                    .debug_selector(|| DEFAULT_PDF_APP_GUIDE_ID.into())
                    .gap_3()
                    .child("Windows asks you to choose the default PDF app yourself, in Settings.")
                    .child(picture(stacked, cx))
                    .child(steps(cx)),
            )
            .footer(
                DialogFooter::new().child(
                    div().flex_none().track_focus(&button_slot).child(
                        Button::new(DEFAULT_PDF_APP_OPEN_SETTINGS_ID)
                            .debug_selector(|| DEFAULT_PDF_APP_OPEN_SETTINGS_ID.into())
                            .primary()
                            .label("Open Settings")
                            .on_click(move |_, window, cx| {
                                open_settings(opener.clone(), window, cx)
                            }),
                    ),
                ),
            )
    });
}

/// Calls the opener off the main thread (registering runs `reg.exe`) and
/// reports a failure over the guide.
fn open_settings(opener: SettingsOpener, window: &mut Window, cx: &mut App) {
    let task = cx.background_executor().spawn(async move { opener() });
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        if let Err(error) = task.await {
            let _ = handle.update(cx, |_, window, cx| show_error(&error, window, cx));
        }
    })
    .detach();
}

/// A numbered badge, as the picture and the steps both use.
fn badge(number: usize, cx: &App) -> gpui::Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size_5()
        .rounded_full()
        .bg(theme.primary)
        .text_color(theme.primary_foreground)
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .child(SharedString::from(number.to_string()))
}

/// A badge pinned to the top-left corner of `target`.
fn badged(number: usize, target: impl IntoElement, cx: &App) -> gpui::Div {
    div()
        .relative()
        .child(target)
        .child(badge(number, cx).absolute().top(px(-9.)).left(px(-9.)))
}

/// A small square that stands for an app's icon.
fn app_glyph(highlight: bool, cx: &App) -> gpui::Div {
    let theme = cx.theme();
    div().flex_none().size_3p5().rounded_sm().bg(if highlight {
        theme.primary
    } else {
        theme.muted_foreground.opacity(0.5)
    })
}

/// A grey bar that stands for another app's name.
fn name_bar(cx: &App) -> gpui::Div {
    div()
        .h_2()
        .w_16()
        .rounded_full()
        .bg(cx.theme().muted_foreground.opacity(0.35))
}

/// A simplified sketch of the steps, drawn with theme colours: the .pdf row
/// in Settings (1), then the app list with Butter Paper (2) and its Set
/// default button (3).
fn picture(stacked: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let sketch_width = |element: gpui::Div| {
        if stacked {
            element.w_full()
        } else {
            element.flex_1().min_w_0()
        }
    };

    let settings = sketch_width(v_flex().gap_1p5())
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(".pdf"),
        )
        .child(badged(
            1,
            h_flex()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded(theme.radius)
                .bg(theme.background)
                .border_2()
                .border_color(theme.primary)
                .child(app_glyph(false, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .text_color(theme.foreground)
                        .truncate()
                        .child("Current app"),
                )
                .child(
                    Icon::new(IconName::ExternalLink)
                        .small()
                        .text_color(theme.muted_foreground),
                ),
            cx,
        ));

    let other_app = || {
        h_flex()
            .gap_2()
            .px_2()
            .py_1()
            .child(app_glyph(false, cx))
            .child(name_bar(cx))
    };
    let picker = sketch_width(
        v_flex()
            .gap_1()
            .p_2()
            .rounded(theme.radius)
            .bg(theme.background)
            .border_1()
            .border_color(theme.border),
    )
    .child(other_app())
    .child(badged(
        2,
        h_flex()
            .gap_2()
            .px_2()
            .py_1()
            .rounded(theme.radius)
            .bg(theme.list_active)
            .border_1()
            .border_color(theme.primary)
            .child(app_glyph(true, cx))
            .child(
                div()
                    .min_w_0()
                    .text_sm()
                    .text_color(theme.foreground)
                    .truncate()
                    .child(PRODUCT_NAME),
            ),
        cx,
    ))
    .child(other_app())
    .child(
        h_flex().justify_end().pt_1().child(badged(
            3,
            div()
                .px_3()
                .py_1()
                .rounded(theme.radius)
                .bg(theme.primary)
                .text_color(theme.primary_foreground)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Set default"),
            cx,
        )),
    );

    let arrow = Icon::new(if stacked {
        IconName::ArrowDown
    } else {
        IconName::ArrowRight
    })
    .small()
    .text_color(theme.muted_foreground);

    div()
        .flex()
        .when(stacked, |this| this.flex_col())
        .items_center()
        .gap_3()
        .p_4()
        .rounded(theme.radius)
        .bg(theme.muted)
        .border_1()
        .border_color(theme.border)
        .child(settings)
        .child(arrow)
        .child(picker)
        .into_any_element()
}

/// The steps as numbered text, matching the picture's badges.
fn steps(cx: &App) -> impl IntoElement {
    v_flex().gap_2().children(
        DEFAULT_PDF_APP_STEPS
            .iter()
            .enumerate()
            .map(|(index, step)| {
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(badge(index + 1, cx))
                    .child(div().flex_1().min_w_0().child(*step))
            }),
    )
}
