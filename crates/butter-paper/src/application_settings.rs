//! Settings presentation; the application owns persistence and update operations.
use crate::{
    application_shell::*,
    native_application::{UpdateMenuState, UpdateMenuStatus},
    native_update_policy::UpdateFrequency,
};
use gpui::{
    AnyElement, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, Styled as _, Subscription, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, StyledExt as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    text::TextView,
    v_flex,
};

#[derive(Clone, Default)]
pub struct ApplicationUpdatePresentation {
    pub state: UpdateMenuState,
    pub message: String,
    pub notes: String,
    /// While downloading: the bytes received and the package's size.
    pub progress: Option<UpdateProgress>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UpdateProgress {
    pub received: u64,
    pub total: u64,
}

impl UpdateProgress {
    pub fn fraction(self) -> f32 {
        if self.total == 0 {
            0.
        } else {
            (self.received as f64 / self.total as f64).clamp(0., 1.) as f32
        }
    }

    /// Whether every byte is in and the update is being prepared.
    pub fn preparing(self) -> bool {
        self.total > 0 && self.received >= self.total
    }

    /// "12.3 MB of 56.1 MB (22%)", or the preparing step.
    pub fn describe(self) -> String {
        if self.preparing() {
            return "Preparing the update…".into();
        }
        format!(
            "{} of {} ({}%)",
            megabytes(self.received),
            megabytes(self.total),
            (self.fraction() * 100.).floor() as u32
        )
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.)
}
impl gpui::Global for ApplicationUpdatePresentation {}

/// Whether windows show the in-app menu bar, which carries the update
/// status. Without it (a macOS choice) the tab strip carries it instead.
#[derive(Clone, Copy, Debug)]
pub struct InAppMenuBarShown(pub bool);
impl gpui::Global for InAppMenuBarShown {}

/// The update status button: checking, downloading with its progress, an
/// update to download, or one ready to install. Idle copies show nothing.
fn update_status_button(
    presentation: &ApplicationUpdatePresentation,
    id: &'static str,
    style: impl FnOnce(Button) -> Button,
) -> Option<AnyElement> {
    let button = style(
        Button::new(id)
            .debug_selector(move || id.into())
            .accessibility_id(id),
    );
    let open_settings = |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
        window.dispatch_action(Box::new(OpenUpdateSettings), cx);
    };
    Some(match presentation.state.status {
        UpdateMenuStatus::Checking => button
            .label("Checking for updates…")
            .loading(true)
            .on_click(open_settings)
            .into_any_element(),
        UpdateMenuStatus::Downloading => button
            .label(match presentation.progress {
                Some(progress) if progress.preparing() => "Preparing update…".to_owned(),
                Some(progress) => format!(
                    "Downloading update {}%",
                    (progress.fraction() * 100.).floor() as u32
                ),
                None => "Downloading update…".to_owned(),
            })
            .loading(true)
            .on_click(open_settings)
            .into_any_element(),
        UpdateMenuStatus::Available(version) => button
            .label("Update available")
            .icon(Icon::default().path(crate::application_assets::DOWNLOAD_ICON))
            .dropdown_menu(move |menu, _, _| {
                menu.label(format!("Butter Paper {version} is available"))
                    .item(
                        PopupMenuItem::new("Download update").on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(DownloadUpdate), cx);
                        }),
                    )
                    .separator()
                    .menu("View release notes", Box::new(OpenUpdateSettings))
            })
            .into_any_element(),
        UpdateMenuStatus::Ready(version) => button
            .label("Update ready")
            .icon(Icon::default().path(crate::application_assets::DOWNLOAD_ICON))
            .dropdown_menu(move |menu, _, _| {
                menu.label(format!("Butter Paper {version} is ready"))
                    .item(
                        PopupMenuItem::new("Restart to update").on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(RestartToUpdate), cx);
                        }),
                    )
                    .separator()
                    .menu("View release notes", Box::new(OpenUpdateSettings))
            })
            .into_any_element(),
        UpdateMenuStatus::Unavailable | UpdateMenuStatus::Store | UpdateMenuStatus::Idle => {
            return None;
        }
    })
}

/// The update status at the right end of the in-app menu bar: as slim as
/// the menus beside it.
pub fn menu_bar_update_status(cx: &App) -> Option<AnyElement> {
    let presentation = cx.try_global::<ApplicationUpdatePresentation>()?;
    let button = update_status_button(presentation, "application-update-status", |button| {
        button.ghost().xsmall()
    })?;
    Some(
        h_flex()
            .id("application-update-status-lane")
            .debug_selector(|| "application-update-status-lane".into())
            .flex_shrink_0()
            .pr_1()
            .child(button)
            .into_any_element(),
    )
}

/// The update status in the tab strip, for macOS windows whose in-app menu
/// bar is turned off.
pub fn update_button(presentation: &ApplicationUpdatePresentation, cx: &App) -> Option<AnyElement> {
    if cx
        .try_global::<InAppMenuBarShown>()
        .is_some_and(|shown| shown.0)
    {
        return None;
    }
    // Medium, as the tab row's other buttons are.
    let button = update_status_button(presentation, "application-update-button", |button| {
        button.outline()
    })?;
    Some(
        h_flex()
            .id("application-update-lane")
            .debug_selector(|| "application-update-lane".into())
            .self_stretch()
            .pl_3()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(button)
            .into_any_element(),
    )
}

pub fn open_about(window: &mut Window, cx: &mut App) {
    let store = cx
        .try_global::<ApplicationUpdatePresentation>()
        .is_some_and(|presentation| presentation.state.status == UpdateMenuStatus::Store);
    window.open_dialog(cx, move |dialog, window, _| {
        dialog
            .title("About Butter Paper")
            .w(px(420.).min(window.viewport_size().width - px(32.)))
            .child(
                v_flex()
                    .id("application-about-content")
                    .debug_selector(|| "application-about-content".into())
                    .gap_3()
                    .child(format!("Butter Paper {}", env!("CARGO_PKG_VERSION")))
                    .child("A native PDF workspace for viewing, drawing and marking up documents.")
                    .child(releases_button("about-releases", store)),
            )
    });
}

/// Store copies link to their Store page; other copies to the GitHub releases.
fn releases_link(store: bool) -> (&'static str, &'static str) {
    if store {
        ("View in Microsoft Store", MICROSOFT_STORE_URL)
    } else {
        ("View releases", APPLICATION_RELEASES_URL)
    }
}

fn releases_button(id: &'static str, store: bool) -> Button {
    let (label, url) = releases_link(store);
    Button::new(id)
        .debug_selector(move || id.into())
        .label(label)
        .on_click(move |_, _, cx| cx.open_url(url))
}

pub fn open_settings(window: &mut Window, cx: &mut App) {
    open_settings_category(SettingsCategory::Updates, window, cx);
}

pub fn open_update_settings(window: &mut Window, cx: &mut App) {
    open_settings_category(SettingsCategory::Updates, window, cx);
}

/// Settings ▸ View, which a review script opens.
#[cfg(feature = "review-driver")]
pub fn open_view_settings(window: &mut Window, cx: &mut App) {
    open_settings_category(SettingsCategory::View, window, cx);
}

/// Settings ▸ Signatures, where the timestamp server is set.
pub fn open_signature_settings(window: &mut Window, cx: &mut App) {
    open_settings_category(SettingsCategory::Signatures, window, cx);
}

fn open_settings_category(category: SettingsCategory, window: &mut Window, cx: &mut App) {
    if let Some(existing) = cx.try_global::<SettingsWindow>().cloned()
        && existing
            .handle
            .update(cx, |_, window, cx| {
                existing.view.update(cx, |view, cx| {
                    view.category = category;
                    view.category_request += 1;
                    view.scroll.set_offset(gpui::point(px(0.), px(0.)));
                    cx.notify();
                });
                window.activate_window();
            })
            .is_ok()
    {
        return;
    }
    let bounds = crate::window_title_bar::centered_window_bounds(
        crate::window_title_bar::at_least(
            gpui::size(px(960.), px(640.)).min(&window.viewport_size()),
            crate::window_title_bar::SETTINGS_WINDOW_MIN_SIZE,
        ),
        cx,
    );
    let opened = cx.open_window(
        gpui::WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
            window_min_size: Some(crate::window_title_bar::SETTINGS_WINDOW_MIN_SIZE),
            ..crate::window_title_bar::title_bar_window_options()
        },
        |window, cx| {
            window.set_window_title("Settings — Butter Paper");
            let view = cx.new(|cx| SettingsView::new(category, cx));
            view.read(cx).focus.clone().focus(window, cx);
            let root = cx.new(|cx| gpui_component::Root::new(view.clone(), window, cx));
            cx.set_global(SettingsWindow {
                handle: window
                    .window_handle()
                    .downcast::<gpui_component::Root>()
                    .unwrap(),
                view,
            });
            root
        },
    );
    if let Err(error) = opened {
        eprintln!("Couldn’t open Settings: {error}");
    }
}

#[derive(Clone)]
struct SettingsWindow {
    handle: gpui::WindowHandle<gpui_component::Root>,
    view: gpui::Entity<SettingsView>,
}
impl gpui::Global for SettingsWindow {}

#[cfg(feature = "review-driver")]
pub fn review_window(cx: &App) -> Option<gpui::AnyWindowHandle> {
    cx.try_global::<SettingsWindow>()
        .map(|state| state.handle.into())
}

/// The Settings field for the timestamp server signatures ask.
pub const SETTINGS_TIMESTAMP_SERVER_ID: &str = "settings-timestamp-server";
/// The Settings switch for new images' proportions.
pub const SETTINGS_LOCK_IMAGE_PROPORTIONS_ID: &str = "settings-lock-image-proportions";
/// The Settings choice of where PDFs opened from outside the app go.
pub const SETTINGS_OPEN_DOCUMENTS_IN_ID: &str = "settings-open-documents-in";
/// Settings ▸ View: the Page appearance choice.
pub const SETTINGS_PAGE_APPEARANCE_ID: &str = "settings-page-appearance";
/// Settings ▸ Mouse wheel: how much a notch zooms.
pub const SETTINGS_ZOOM_SPEED_ID: &str = "settings-zoom-speed";
/// Settings ▸ Mouse wheel: the Reverse Scroll Zoom switch.
pub const SETTINGS_REVERSE_SCROLL_ZOOM_ID: &str = "settings-reverse-scroll-zoom";
/// Settings ▸ Mouse wheel: how the wheel turns pages in Single Page view.
pub const SETTINGS_PAGE_TURNING_ID: &str = "settings-page-turning";

#[derive(Clone, Copy, Eq, PartialEq)]
enum SettingsCategory {
    Updates,
    #[cfg_attr(not(test), allow(dead_code))]
    Documents,
    #[cfg_attr(not(test), allow(dead_code))]
    View,
    #[cfg_attr(not(test), allow(dead_code))]
    MouseWheel,
    Signatures,
}

struct SettingsView {
    category: SettingsCategory,
    /// Counts requests to show a category, so the stock Settings shows the
    /// requested page afresh each time one arrives.
    category_request: usize,
    scroll: ScrollHandle,
    _updates: Subscription,
    _markups: Subscription,
    _documents: Subscription,
    _mouse_wheel: [Subscription; 3],
    _page_appearance: Subscription,
    focus: gpui::FocusHandle,
    /// Signatures: the timestamp server field, made on first render.
    timestamp_server: Option<gpui::Entity<gpui_component::input::InputState>>,
    timestamp_server_error: Option<&'static str>,
    _timestamp_server: Option<Subscription>,
}
impl SettingsView {
    fn new(category: SettingsCategory, cx: &mut Context<Self>) -> Self {
        Self {
            category,
            category_request: 0,
            scroll: ScrollHandle::new(),
            _updates: cx.observe_global::<ApplicationUpdatePresentation>(|_, cx| cx.notify()),
            _markups: cx.observe_global::<crate::application_shell::LockImageProportions>(
                |_, cx| cx.notify(),
            ),
            _documents: cx.observe_global::<crate::application_shell::OpenDocumentsInPreference>(
                |_, cx| cx.notify(),
            ),
            _mouse_wheel: [
                cx.observe_global::<crate::application_shell::ZoomSpeedPreference>(|_, cx| {
                    cx.notify()
                }),
                cx.observe_global::<crate::application_shell::PageTurningPreference>(|_, cx| {
                    cx.notify()
                }),
                cx.observe_global::<crate::application_shell::ReverseScrollZoom>(|_, cx| {
                    cx.notify()
                }),
            ],
            _page_appearance: cx
                .observe_global::<crate::application_shell::PageAppearancePreference>(|_, cx| {
                    cx.notify()
                }),
            focus: cx.focus_handle(),
            timestamp_server: None,
            timestamp_server_error: None,
            _timestamp_server: None,
        }
    }

    /// Signatures: the timestamp server digital signatures ask for a
    /// trusted time, none at first as in Acrobat and Revu.
    fn render_signature_sections(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<(&'static str, AnyElement)> {
        use crate::signature_settings::{SignatureSettings, timestamp_server_from_text};
        use gpui_component::input::{Input, InputEvent, InputState};
        let input = match &self.timestamp_server {
            Some(input) => input.clone(),
            None => {
                let current = SignatureSettings::timestamp_server(cx).unwrap_or_default();
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("None")
                        .default_value(current)
                });
                // Kept when the field is left or Enter is pressed, so a
                // half-typed address is not called wrong.
                self._timestamp_server = Some(cx.subscribe_in(
                    &input,
                    window,
                    |view, input, event: &InputEvent, _, cx| match event {
                        InputEvent::Blur | InputEvent::PressEnter { .. } => {
                            match timestamp_server_from_text(input.read(cx).value().as_ref()) {
                                Ok(server) => {
                                    view.timestamp_server_error = None;
                                    SignatureSettings::set_timestamp_server(server, cx);
                                }
                                Err(error) => view.timestamp_server_error = Some(error),
                            }
                            cx.notify();
                        }
                        InputEvent::Change if view.timestamp_server_error.take().is_some() => {
                            cx.notify();
                        }
                        _ => {}
                    },
                ));
                self.timestamp_server = Some(input.clone());
                input
            }
        };
        let timestamps = v_flex()
            .id("settings-timestamps-section")
            .debug_selector(|| "settings-timestamps-section".into())
            .gap_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_medium().child("Timestamp server"))
                    .child(
                        div()
                            .id(SETTINGS_TIMESTAMP_SERVER_ID)
                            .debug_selector(|| SETTINGS_TIMESTAMP_SERVER_ID.into())
                            .child(Input::new(&input)),
                    ),
            )
            .when_some(self.timestamp_server_error, |section, error| {
                section.child(gpui_component::alert::Alert::error(
                    "settings-timestamp-server-error",
                    error,
                ))
            })
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Digital signatures ask this server for a trusted time, which keeps them \
                         verifiable after the signer's certificate expires. Only a fingerprint of \
                         the signature is sent. Leave it empty for none, or use your \
                         organisation's server or a public one such as \
                         http://timestamp.digicert.com.",
                    ),
            )
            .into_any_element();
        vec![("Timestamps", timestamps)]
    }

    /// Markups: how new images are placed and resized.
    /// Documents: where a PDF opened from outside the app goes.
    fn render_document_sections(&self, cx: &mut Context<Self>) -> Vec<(&'static str, AnyElement)> {
        use crate::application_shell::{
            OpenDocumentsIn, OpenDocumentsInNewTab, OpenDocumentsInNewWindow,
            OpenDocumentsInPreference,
        };
        let current = OpenDocumentsInPreference::current(cx);
        let opening = v_flex()
            .id("settings-opening-section")
            .debug_selector(|| "settings-opening-section".into())
            .gap_3()
            .child(
                div()
                    .id(SETTINGS_OPEN_DOCUMENTS_IN_ID)
                    .debug_selector(|| SETTINGS_OPEN_DOCUMENTS_IN_ID.into())
                    .child(
                        gpui_component::radio::RadioGroup::vertical(
                            "settings-open-documents-in-group",
                        )
                        .child(
                            gpui_component::radio::Radio::new("settings-open-documents-new-tab")
                                .label("New tab"),
                        )
                        .child(
                            gpui_component::radio::Radio::new("settings-open-documents-new-window")
                                .label("New window"),
                        )
                        .selected_index(Some(match current {
                            OpenDocumentsIn::NewTab => 0,
                            OpenDocumentsIn::NewWindow => 1,
                        }))
                        .on_click(|index, window, cx| {
                            let action: Box<dyn gpui::Action> = if *index == 0 {
                                Box::new(OpenDocumentsInNewTab)
                            } else {
                                Box::new(OpenDocumentsInNewWindow)
                            };
                            window.dispatch_action(action, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "For PDFs opened from outside Butter Paper, such as from the file \
                         manager or the command line. A new tab goes in the window you used \
                         last. PDFs dropped on a window always open as tabs in that window.",
                    ),
            )
            .into_any_element();
        vec![("Open documents in", opening)]
    }

    /// View: whether pages draw dark.
    fn render_view_sections(&self, cx: &mut Context<Self>) -> Vec<(&'static str, AnyElement)> {
        use crate::application_shell::{PageAppearancePreference, page_appearance_action};
        use crate::paper_appearance::PageAppearance;
        let muted = cx.theme().muted_foreground;
        let current = PageAppearancePreference::current(cx);
        let choice = |id: &'static str, appearance: PageAppearance| {
            gpui_component::radio::Radio::new(id)
                .label(appearance.label())
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(appearance.description()),
                )
        };
        let appearance = v_flex()
            .id("settings-page-appearance-section")
            .debug_selector(|| "settings-page-appearance-section".into())
            .gap_3()
            .child(
                div()
                    .id(SETTINGS_PAGE_APPEARANCE_ID)
                    .debug_selector(|| SETTINGS_PAGE_APPEARANCE_ID.into())
                    .child(
                        gpui_component::radio::RadioGroup::vertical(
                            "settings-page-appearance-group",
                        )
                        .children(PageAppearance::ALL.map(|appearance| {
                            choice(
                                match appearance {
                                    PageAppearance::Light => "settings-page-appearance-light",
                                    PageAppearance::Dark => "settings-page-appearance-dark",
                                    PageAppearance::MatchSystem => {
                                        "settings-page-appearance-match-system"
                                    }
                                },
                                appearance,
                            )
                        }))
                        .selected_index(
                            PageAppearance::ALL.iter().position(|item| *item == current),
                        )
                        .on_click(|index, window, cx| {
                            if let Some(appearance) = PageAppearance::ALL.get(*index) {
                                window.dispatch_action(page_appearance_action(*appearance), cx);
                            }
                        }),
                    ),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "The pages only. Saved files, exports and copies keep the document’s own \
                 colours. {} switches between Light and Dark.",
                crate::shortcut_label::shortcut_label(if cfg!(target_os = "macos") {
                    "Cmd+Shift+D"
                } else {
                    "Ctrl+Shift+D"
                }),
            )))
            .into_any_element();
        vec![("Page appearance", appearance)]
    }

    /// Mouse wheel: how far a notch zooms, and how the wheel turns pages in
    /// Single Page view.
    fn render_mouse_wheel_sections(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<(&'static str, AnyElement)> {
        use crate::application_shell::{
            PageTurningPreference, ReverseScrollZoom, ToggleReverseScrollZoom, ZoomSpeedPreference,
            page_turning_action, zoom_speed_action,
        };
        use crate::mouse_wheel::{PageTurning, ZoomSpeed};
        let muted = cx.theme().muted_foreground;
        let choice = |id: &'static str, label: &'static str, description: &'static str| {
            gpui_component::radio::Radio::new(id)
                .label(label)
                .child(div().text_sm().text_color(muted).child(description))
        };
        let speed = ZoomSpeedPreference::current(cx);
        let reverse = cx
            .try_global::<ReverseScrollZoom>()
            .is_some_and(|reverse| reverse.0);
        let zoom = v_flex()
            .id("settings-zoom-section")
            .debug_selector(|| "settings-zoom-section".into())
            .gap_3()
            // The Zoom group holds the Zoom speed choices and the Reverse
            // scroll zoom switch (owner decision, 2026-11-25).
            .child(
                gpui_component::label::Label::new("Zoom speed")
                    .text_sm()
                    .font_medium(),
            )
            .child(
                div()
                    .id(SETTINGS_ZOOM_SPEED_ID)
                    .debug_selector(|| SETTINGS_ZOOM_SPEED_ID.into())
                    .child(
                        gpui_component::radio::RadioGroup::vertical("settings-zoom-speed-group")
                            .children(ZoomSpeed::ALL.map(|speed| {
                                choice(
                                    match speed {
                                        ZoomSpeed::Fine => "settings-zoom-speed-fine",
                                        ZoomSpeed::Standard => "settings-zoom-speed-standard",
                                        ZoomSpeed::Coarse => "settings-zoom-speed-coarse",
                                    },
                                    speed.label(),
                                    speed.description(),
                                )
                            }))
                            .selected_index(ZoomSpeed::ALL.iter().position(|item| *item == speed))
                            .on_click(|index, window, cx| {
                                if let Some(speed) = ZoomSpeed::ALL.get(*index) {
                                    window.dispatch_action(zoom_speed_action(*speed), cx);
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .id(SETTINGS_REVERSE_SCROLL_ZOOM_ID)
                    .debug_selector(|| SETTINGS_REVERSE_SCROLL_ZOOM_ID.into())
                    .child(
                        gpui_component::switch::Switch::new("settings-reverse-scroll-zoom-switch")
                            .label("Reverse scroll zoom")
                            .checked(reverse)
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleReverseScrollZoom), cx);
                            }),
                    ),
            )
            .child(div().text_sm().text_color(muted).child(
                "Scrolling up zooms in. Reverse it for a mouse whose scrolling a utility \
                 reverses. A trackpad pinch always follows your fingers.",
            ))
            .into_any_element();
        let turning = PageTurningPreference::current(cx);
        let pages = v_flex()
            .id("settings-page-turning-section")
            .debug_selector(|| "settings-page-turning-section".into())
            .gap_3()
            .child(
                div()
                    .id(SETTINGS_PAGE_TURNING_ID)
                    .debug_selector(|| SETTINGS_PAGE_TURNING_ID.into())
                    .child(
                        gpui_component::radio::RadioGroup::vertical("settings-page-turning-group")
                            .children(PageTurning::ALL.map(|turning| {
                                choice(
                                    match turning {
                                        PageTurning::Deliberate => {
                                            "settings-page-turning-deliberate"
                                        }
                                        PageTurning::Balanced => "settings-page-turning-balanced",
                                        PageTurning::Responsive => {
                                            "settings-page-turning-responsive"
                                        }
                                    },
                                    turning.label(),
                                    turning.description(),
                                )
                            }))
                            .selected_index(
                                PageTurning::ALL.iter().position(|item| *item == turning),
                            )
                            .on_click(|index, window, cx| {
                                if let Some(turning) = PageTurning::ALL.get(*index) {
                                    window.dispatch_action(page_turning_action(*turning), cx);
                                }
                            }),
                    ),
            )
            .child(div().text_sm().text_color(muted).child(
                "In Single Page view, with the mouse wheel set to Scroll. A trackpad \
                 turns one page for each swipe.",
            ))
            .into_any_element();
        vec![("Zoom", zoom), ("Page turning", pages)]
    }

    fn render_markup_sections(&self, cx: &mut Context<Self>) -> Vec<(&'static str, AnyElement)> {
        let locked = crate::application_shell::LockImageProportions::current(cx);
        let images = v_flex()
            .id("settings-images-section")
            .debug_selector(|| "settings-images-section".into())
            .gap_3()
            .child(
                div()
                    .id(SETTINGS_LOCK_IMAGE_PROPORTIONS_ID)
                    .debug_selector(|| SETTINGS_LOCK_IMAGE_PROPORTIONS_ID.into())
                    .child(
                        gpui_component::switch::Switch::new(
                            "settings-lock-image-proportions-switch",
                        )
                        .label("Keep proportions when resizing")
                        .checked(locked)
                        .on_click(|_, window, cx| {
                            window.dispatch_action(
                                Box::new(crate::application_shell::ToggleLockImageProportions),
                                cx,
                            );
                        }),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "New images resize from their corners and keep their shape. \
                         Change it for one image with Lock proportions in its properties.",
                    ),
            )
            .into_any_element();
        vec![("Images", images)]
    }

    fn render_update_sections(&self, cx: &mut Context<Self>) -> Vec<(&'static str, AnyElement)> {
        let presentation = cx
            .try_global::<ApplicationUpdatePresentation>()
            .cloned()
            .unwrap_or_default();
        let state = presentation.state;
        let frequency = state.frequency;
        let available = matches!(state.status, UpdateMenuStatus::Available(_));
        let ready = matches!(state.status, UpdateMenuStatus::Ready(_));
        let busy = matches!(
            state.status,
            UpdateMenuStatus::Checking | UpdateMenuStatus::Downloading
        );
        let message = if presentation.message.is_empty() {
            match state.status {
                UpdateMenuStatus::Unavailable => "Automatic updates are available in installed release builds. Download a release to update this copy.".into(),
                UpdateMenuStatus::Store => "The Microsoft Store keeps Butter Paper up to date.".into(),
                UpdateMenuStatus::Idle => "Check for a newer version of Butter Paper.".into(),
                UpdateMenuStatus::Checking => "Checking for updates…".into(),
                UpdateMenuStatus::Available(version) => format!("Butter Paper {version} is available."),
                UpdateMenuStatus::Downloading => "Downloading and preparing the update… You can keep working.".into(),
                UpdateMenuStatus::Ready(version) => format!("Butter Paper {version} is ready. Restart to install, or it installs when you quit. You’ll be asked to save any unsaved work."),
            }
        } else {
            presentation.message
        };
        let action = Button::new("settings-update-action")
            .debug_selector(|| "settings-update-action".into())
            .accessibility_id("settings-update-action")
            .primary()
            .label(match state.status {
                UpdateMenuStatus::Checking => "Checking…",
                UpdateMenuStatus::Downloading => "Downloading…",
                UpdateMenuStatus::Ready(_) => "Restart to update",
                UpdateMenuStatus::Available(_) => "Download update",
                _ => "Check for updates",
            })
            .disabled(busy || state.status == UpdateMenuStatus::Unavailable)
            .on_click(move |_, window, cx| {
                let action: Box<dyn gpui::Action> = if ready {
                    Box::new(RestartToUpdate)
                } else if available {
                    Box::new(DownloadUpdate)
                } else {
                    Box::new(CheckForUpdates)
                };
                window.dispatch_action(action, cx);
            });
        let schedule = Button::new("settings-update-frequency")
            .debug_selector(|| "settings-update-frequency".into())
            .outline()
            .disabled(frequency.is_none())
            .label(frequency_label(frequency))
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                for (label, value, action) in frequency_options() {
                    menu = menu.menu_with_check(label, frequency == Some(value), action);
                }
                menu
            });
        let store = state.status == UpdateMenuStatus::Store;
        let version = v_flex()
            .id("settings-version-section")
            .debug_selector(|| "settings-version-section".into())
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Installed version {}", env!("CARGO_PKG_VERSION"))),
            )
            // The update status reads at the section's text size, in full
            // colour as it is news rather than a description.
            .child(div().text_sm().child(message))
            .when_some(presentation.progress, |section, progress| {
                section.child(
                    v_flex()
                        .id("settings-update-progress")
                        .debug_selector(|| "settings-update-progress".into())
                        .gap_1()
                        .child(
                            gpui_component::progress::Progress::new("settings-update-progress-bar")
                                .value(progress.fraction() * 100.)
                                .loading(progress.preparing()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(progress.describe()),
                        ),
                )
            })
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .when(!store, |row| row.child(action))
                    .child(releases_button("settings-releases", store)),
            )
            .into_any_element();
        // The Store updates its copies itself: no checks or downloads here.
        if store {
            return vec![("Version", version)];
        }
        let automatic = v_flex().id("settings-automatic-section").debug_selector(|| "settings-automatic-section".into()).gap_3()
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Choose how often Butter Paper looks for updates."))
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Updates download automatically after a successful check. You choose when to restart."))
            .child(schedule).into_any_element();
        let mut sections = vec![("Automatic checks", automatic), ("Version", version)];
        // What's new is for an update there is to install.
        if (available || ready || state.status == UpdateMenuStatus::Downloading)
            && !presentation.notes.is_empty()
        {
            sections.push((
                "What’s new",
                v_flex()
                    .id("settings-notes-section")
                    .debug_selector(|| "settings-notes-section".into())
                    .gap_3()
                    .child(TextView::markdown(
                        "update-release-notes",
                        presentation.notes,
                    ))
                    .into_any_element(),
            ));
        }
        sections
    }
}
impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::setting::{
            SelectIndex, SettingGroup, SettingItem, SettingPage, Settings,
        };
        // Each section is one stock group, titled by its heading; the stock
        // Settings lays out the pages, their navigation and search.
        let page = |title: &'static str, icon: Icon, sections: Vec<(&'static str, AnyElement)>| {
            SettingPage::new(title)
                .icon(icon)
                .resettable(false)
                .groups(sections.into_iter().map(|(heading, section)| {
                    // Built for this frame, so drawn once.
                    let section = std::rc::Rc::new(std::cell::RefCell::new(Some(section)));
                    SettingGroup::new().title(heading).item(
                        SettingItem::render(move |_, _, _| {
                            section
                                .borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element())
                        })
                        .keywords([heading, title]),
                    )
                }))
        };
        let pages = [
            page(
                "Updates",
                Icon::default().path(crate::application_assets::DOWNLOAD_ICON),
                self.render_update_sections(cx),
            ),
            page(
                "Documents",
                Icon::default().path(crate::application_assets::NEW_DOCUMENT_ICON),
                self.render_document_sections(cx),
            ),
            page(
                "View",
                Icon::default().path("icons/eye.svg"),
                self.render_view_sections(cx),
            ),
            page(
                "Mouse wheel",
                Icon::default().path(crate::application_assets::MOUSE_WHEEL_ICON),
                self.render_mouse_wheel_sections(cx),
            ),
            page(
                "Markups",
                Icon::default().path("icons/rail/image.svg"),
                self.render_markup_sections(cx),
            ),
            page(
                "Signatures",
                Icon::default().path("icons/rail/signature.svg"),
                self.render_signature_sections(window, cx),
            ),
        ];
        let page_ix = match self.category {
            SettingsCategory::Updates => 0,
            SettingsCategory::Documents => 1,
            SettingsCategory::View => 2,
            SettingsCategory::MouseWheel => 3,
            // Markups, page 4, opens from the Settings sidebar.
            SettingsCategory::Signatures => 5,
        };
        let body = div()
            .id("application-settings-screen")
            .debug_selector(|| "application-settings-screen".into())
            .size_full()
            .child(
                Settings::new(("application-settings", self.category_request))
                    .default_selected_index(SelectIndex {
                        page_ix,
                        ..Default::default()
                    })
                    .pages(pages),
            );
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .key_context("Settings")
            .track_focus(&self.focus)
            .on_action(|_: &crate::document_workspace::CloseDocument, window, _| {
                window.remove_window()
            })
            .on_action(
                |_: &crate::application_close_workspace::RequestApplicationClose, window, _| {
                    window.remove_window()
                },
            )
            .children(
                crate::window_title_bar::uses_window_title_bar(window).then(|| {
                    crate::window_title_bar::window_title_bar(
                        "Settings — Butter Paper",
                        window.viewport_size().width,
                        cx.theme().background,
                    )
                }),
            )
            .child(div().flex_1().min_h_0().size_full().child(body))
            .children(gpui_component::Root::render_dialog_layer(window, cx))
            .children(gpui_component::Root::render_sheet_layer(window, cx))
    }
}

fn frequency_label(frequency: Option<UpdateFrequency>) -> &'static str {
    frequency_options()
        .into_iter()
        .find(|(_, value, _)| Some(*value) == frequency)
        .map(|(label, _, _)| label)
        .unwrap_or("Unavailable")
}
fn frequency_options() -> [(&'static str, UpdateFrequency, Box<dyn gpui::Action>); 8] {
    use UpdateFrequency::*;
    [
        ("Never", Never, Box::new(SetUpdateFrequencyNever)),
        ("At startup", Startup, Box::new(SetUpdateFrequencyAtStartup)),
        ("Hourly", Hourly, Box::new(SetUpdateFrequencyHourly)),
        (
            "Every 6 hours",
            SixHours,
            Box::new(SetUpdateFrequencyEverySixHours),
        ),
        (
            "Every 12 hours",
            TwelveHours,
            Box::new(SetUpdateFrequencyEveryTwelveHours),
        ),
        ("Daily", Daily, Box::new(SetUpdateFrequencyDaily)),
        ("Weekly", Weekly, Box::new(SetUpdateFrequencyWeekly)),
        ("Monthly", Monthly, Box::new(SetUpdateFrequencyMonthly)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    #[gpui::test]
    fn settings_update_command_tracks_live_state_and_blocks_duplicate_work(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let checks = Rc::new(Cell::new(0));
        let downloads = Rc::new(Cell::new(0));
        let restarts = Rc::new(Cell::new(0));
        cx.update(|cx| {
            let checks = checks.clone();
            cx.on_action(move |_: &CheckForUpdates, _| checks.set(checks.get() + 1));
            let downloads = downloads.clone();
            cx.on_action(move |_: &DownloadUpdate, _| downloads.set(downloads.get() + 1));
            let restarts = restarts.clone();
            cx.on_action(move |_: &RestartToUpdate, _| restarts.set(restarts.get() + 1));
        });
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::Updates, cx));
            gpui_component::Root::new(view, window, cx)
        });
        let version = crate::native_updater::ReleaseVersion::stable(0, 1, 5);
        for (status, expected) in [
            (UpdateMenuStatus::Unavailable, (0, 0, 0)),
            (UpdateMenuStatus::Idle, (1, 0, 0)),
            (UpdateMenuStatus::Checking, (1, 0, 0)),
            (UpdateMenuStatus::Available(version), (1, 1, 0)),
            (UpdateMenuStatus::Downloading, (1, 1, 0)),
            (UpdateMenuStatus::Ready(version), (1, 1, 1)),
        ] {
            cx.update(|window, cx| {
                cx.set_global(ApplicationUpdatePresentation {
                    state: UpdateMenuState {
                        status,
                        frequency: Some(UpdateFrequency::Daily),
                    },
                    ..Default::default()
                });
                window.draw(cx).clear(cx);
            });
            let bounds = cx.debug_bounds("settings-update-action").unwrap();
            cx.simulate_click(bounds.center(), Modifiers::default());
            cx.run_until_parked();
            assert_eq!((checks.get(), downloads.get(), restarts.get()), expected);
        }
    }

    #[gpui::test]
    fn store_copies_show_their_version_without_update_controls(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::Updates, cx));
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            cx.set_global(ApplicationUpdatePresentation {
                state: UpdateMenuState {
                    status: UpdateMenuStatus::Store,
                    frequency: None,
                },
                ..Default::default()
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("settings-version-section").is_some());
        assert!(cx.debug_bounds("settings-update-action").is_none());
        assert!(cx.debug_bounds("settings-releases").is_some());
        assert!(cx.debug_bounds("settings-automatic-section").is_none());
        assert!(cx.debug_bounds("settings-notes-section").is_none());
    }

    #[gpui::test]
    fn the_stock_settings_page_shows_every_group_of_its_category(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            // What's new shows for an update there is to install.
            cx.set_global(ApplicationUpdatePresentation {
                state: UpdateMenuState {
                    status: UpdateMenuStatus::Available(
                        crate::native_updater::ReleaseVersion::stable(0, 1, 5),
                    ),
                    frequency: Some(UpdateFrequency::Daily),
                },
                notes: "Release detail.".into(),
                ..Default::default()
            });
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::Updates, cx));
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        for id in [
            "settings-automatic-section",
            "settings-version-section",
            "settings-notes-section",
        ] {
            assert!(
                cx.debug_bounds(id).is_some(),
                "{id} shows on the Updates page"
            );
        }
        // The other categories' groups are on their own pages.
        assert!(cx.debug_bounds("settings-images-section").is_none());
        assert!(cx.debug_bounds("settings-timestamps-section").is_none());
        for id in [
            "settings-interface-smaller",
            "settings-interface-reset",
            "settings-interface-bigger",
        ] {
            assert!(cx.debug_bounds(id).is_none());
        }
    }

    /// Settings ▸ Documents chooses where PDFs opened from outside the app
    /// go, with the stock radio buttons, and shows the choice in force.
    #[gpui::test]
    fn the_documents_page_chooses_a_new_tab_or_a_new_window(cx: &mut TestAppContext) {
        use crate::application_shell::{
            OpenDocumentsIn, OpenDocumentsInNewTab, OpenDocumentsInNewWindow,
            OpenDocumentsInPreference,
        };
        cx.update(gpui_component::init);
        let chosen = Rc::new(std::cell::RefCell::new(Vec::new()));
        cx.update(|cx| {
            let tab = chosen.clone();
            cx.on_action(move |_: &OpenDocumentsInNewTab, cx| {
                tab.borrow_mut().push(OpenDocumentsIn::NewTab);
                cx.set_global(OpenDocumentsInPreference(OpenDocumentsIn::NewTab));
            });
            let window = chosen.clone();
            cx.on_action(move |_: &OpenDocumentsInNewWindow, cx| {
                window.borrow_mut().push(OpenDocumentsIn::NewWindow);
                cx.set_global(OpenDocumentsInPreference(OpenDocumentsIn::NewWindow));
            });
        });
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::Documents, cx));
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let choice = cx
            .debug_bounds(SETTINGS_OPEN_DOCUMENTS_IN_ID)
            .expect("the Documents page shows the choice");
        if let Some(tree) = cx.update(|window, _| window.debug_a11y_tree_json()) {
            assert!(tree.contains("New tab") && tree.contains("New window"));
        }
        // The rows are stacked: New tab, then New window.
        let new_window = gpui::point(choice.left() + px(8.), choice.bottom() - px(8.));
        let new_tab = gpui::point(choice.left() + px(8.), choice.top() + px(8.));
        cx.simulate_click(new_window, Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_click(new_tab, Modifiers::default());
        cx.run_until_parked();
        assert_eq!(
            *chosen.borrow(),
            [OpenDocumentsIn::NewWindow, OpenDocumentsIn::NewTab]
        );
    }

    /// Settings ▸ View chooses Light, Dark or Match system with the stock
    /// radio buttons, shows the choice in force and names the shortcut.
    #[gpui::test]
    fn the_view_page_chooses_the_page_appearance(cx: &mut TestAppContext) {
        use crate::application_shell::{
            PageAppearancePreference, SetPageAppearanceDark, SetPageAppearanceLight,
            SetPageAppearanceMatchSystem,
        };
        use crate::paper_appearance::PageAppearance;
        cx.update(gpui_component::init);
        let chosen = Rc::new(std::cell::RefCell::new(Vec::new()));
        cx.update(|cx| {
            for (appearance, record) in [
                (PageAppearance::Light, 0),
                (PageAppearance::Dark, 1),
                (PageAppearance::MatchSystem, 2),
            ] {
                let chosen = chosen.clone();
                let set = move |cx: &mut App| {
                    chosen.borrow_mut().push(appearance);
                    cx.set_global(PageAppearancePreference(appearance));
                };
                match record {
                    0 => {
                        cx.on_action(move |_: &SetPageAppearanceLight, cx| set(cx));
                    }
                    1 => {
                        cx.on_action(move |_: &SetPageAppearanceDark, cx| set(cx));
                    }
                    _ => {
                        cx.on_action(move |_: &SetPageAppearanceMatchSystem, cx| set(cx));
                    }
                }
            }
        });
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::View, cx));
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let choice = cx
            .debug_bounds(SETTINGS_PAGE_APPEARANCE_ID)
            .expect("the View page shows the choice");
        if let Some(tree) = cx.update(|window, _| window.debug_a11y_tree_json()) {
            assert!(
                tree.contains("Light") && tree.contains("Dark") && tree.contains("Match system")
            );
        }
        // The rows are stacked: Light, Dark, Match system.
        let row = |index: usize| {
            let height = choice.size.height / 3.;
            gpui::point(
                choice.left() + px(8.),
                choice.top() + height * (index as f32 + 0.5),
            )
        };
        for index in [1, 2, 0] {
            cx.simulate_click(row(index), Modifiers::default());
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        assert_eq!(
            *chosen.borrow(),
            [
                PageAppearance::Dark,
                PageAppearance::MatchSystem,
                PageAppearance::Light
            ]
        );
        cx.update(|_, cx| {
            assert_eq!(PageAppearancePreference::current(cx), PageAppearance::Light);
        });
    }

    /// Settings ▸ Mouse wheel chooses the Zoom speed and the Page turning
    /// with stock radio buttons, and groups the Zoom speed with the Reverse
    /// scroll zoom switch under Zoom. It shows the choices in force.
    #[gpui::test]
    fn the_mouse_wheel_page_chooses_zoom_speed_and_page_turning(cx: &mut TestAppContext) {
        use crate::application_shell::{
            PageTurningPreference, ReverseScrollZoom, ToggleReverseScrollZoom, ZoomSpeedPreference,
        };
        use crate::mouse_wheel::{PageTurning, ZoomSpeed};
        cx.update(gpui_component::init);
        cx.update(|cx| {
            use crate::application_shell::{
                SetPageTurningBalanced, SetPageTurningDeliberate, SetPageTurningResponsive,
                SetZoomSpeedCoarse, SetZoomSpeedFine, SetZoomSpeedStandard,
            };
            cx.on_action(|_: &SetZoomSpeedFine, cx| {
                cx.set_global(ZoomSpeedPreference(ZoomSpeed::Fine))
            });
            cx.on_action(|_: &SetZoomSpeedStandard, cx| {
                cx.set_global(ZoomSpeedPreference(ZoomSpeed::Standard))
            });
            cx.on_action(|_: &SetZoomSpeedCoarse, cx| {
                cx.set_global(ZoomSpeedPreference(ZoomSpeed::Coarse))
            });
            cx.on_action(|_: &SetPageTurningDeliberate, cx| {
                cx.set_global(PageTurningPreference(PageTurning::Deliberate))
            });
            cx.on_action(|_: &SetPageTurningBalanced, cx| {
                cx.set_global(PageTurningPreference(PageTurning::Balanced))
            });
            cx.on_action(|_: &SetPageTurningResponsive, cx| {
                cx.set_global(PageTurningPreference(PageTurning::Responsive))
            });
            cx.on_action(|_: &ToggleReverseScrollZoom, cx| {
                let reverse = cx.try_global::<ReverseScrollZoom>().is_some_and(|r| r.0);
                cx.set_global(ReverseScrollZoom(!reverse));
            });
        });
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::MouseWheel, cx));
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        for id in [
            SETTINGS_ZOOM_SPEED_ID,
            SETTINGS_REVERSE_SCROLL_ZOOM_ID,
            SETTINGS_PAGE_TURNING_ID,
        ] {
            assert!(cx.debug_bounds(id).is_some(), "{id}");
        }
        // The Zoom group holds both the Zoom speed and the Reverse scroll
        // zoom switch.
        let zoom = cx.debug_bounds("settings-zoom-section").unwrap();
        for id in [SETTINGS_ZOOM_SPEED_ID, SETTINGS_REVERSE_SCROLL_ZOOM_ID] {
            let bounds = cx.debug_bounds(id).unwrap();
            assert!(
                zoom.contains(&bounds.origin) && zoom.contains(&bounds.center()),
                "{id}"
            );
        }
        if let Some(tree) = cx.update(|window, _| window.debug_a11y_tree_json()) {
            assert!(tree.contains("Zoom speed"), "{tree}");
            for turning in PageTurning::ALL {
                assert!(tree.contains(turning.label()), "{tree}");
            }
            for speed in ZoomSpeed::ALL {
                assert!(tree.contains(speed.label()), "{tree}");
            }
        }
        // Each group stacks its three choices, a label and a line of
        // description each: the choice's radio is at the top left of its
        // third.
        for (group, row, check) in [
            (SETTINGS_ZOOM_SPEED_ID, 2., 0),
            (SETTINGS_PAGE_TURNING_ID, 0., 1),
            (SETTINGS_PAGE_TURNING_ID, 1., 2),
            (SETTINGS_ZOOM_SPEED_ID, 0., 3),
        ] {
            let id = group;
            let bounds = cx.debug_bounds(group).unwrap();
            let third = bounds.size.height / 3.;
            cx.simulate_click(
                gpui::point(bounds.left() + px(8.), bounds.top() + third * row + px(8.)),
                Modifiers::default(),
            );
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let (speed, turning) = cx.update(|_, cx| {
                (
                    ZoomSpeedPreference::current(cx),
                    PageTurningPreference::current(cx),
                )
            });
            let expected = [
                (ZoomSpeed::Coarse, PageTurning::Responsive),
                (ZoomSpeed::Coarse, PageTurning::Deliberate),
                (ZoomSpeed::Coarse, PageTurning::Balanced),
                (ZoomSpeed::Fine, PageTurning::Balanced),
            ][check];
            assert_eq!((speed, turning), expected, "{id}");
        }
        let switch = cx.debug_bounds(SETTINGS_REVERSE_SCROLL_ZOOM_ID).unwrap();
        cx.simulate_click(switch.center(), Modifiers::default());
        cx.run_until_parked();
        assert!(cx.update(|_, cx| cx.global::<ReverseScrollZoom>().0));
    }

    struct EmptyView;
    impl Render for EmptyView {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .children(gpui_component::Root::render_dialog_layer(window, cx))
        }
    }

    #[gpui::test]
    fn settings_stays_inside_small_windows(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::Updates, cx));
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        for width in [1200., 700., 600.] {
            cx.simulate_resize(gpui::size(px(width), px(520.)));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let screen = cx
                .debug_bounds("application-settings-screen")
                .unwrap_or_else(|| panic!("missing settings at {width}"));
            assert_eq!(screen.left(), px(0.));
            assert_eq!(screen.right(), px(width));
            assert!(screen.top() >= px(0.) && screen.bottom() <= px(520.));
            assert!(
                cx.debug_bounds("settings-version-section")
                    .is_some_and(|section| section.right() <= px(width)),
                "the page stays inside the window at {width}"
            );
        }
    }

    #[gpui::test]
    fn about_stays_inside_a_narrow_window(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| EmptyView);
            gpui_component::Root::new(view, window, cx)
        });
        cx.simulate_resize(gpui::size(px(320.), px(520.)));
        cx.update(|window, cx| {
            open_about(window, cx);
            window.draw(cx).clear(cx);
        });
        let about = cx.debug_bounds("application-about-content").unwrap();
        assert!(about.left() >= px(0.) && about.right() <= px(320.));
        assert!(about.top() >= px(0.) && about.bottom() <= px(520.));
    }

    #[gpui::test]
    fn settings_reuses_its_window_and_reopens_after_close(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| EmptyView);
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            open_settings(window, cx);
            let first = cx.global::<SettingsWindow>().clone();
            open_update_settings(window, cx);
            assert_eq!(first.handle, cx.global::<SettingsWindow>().handle);
            assert!(first.view.read(cx).category == SettingsCategory::Updates);
            first
                .handle
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let first = cx.global::<SettingsWindow>().handle;
            open_settings(window, cx);
            assert_ne!(first, cx.global::<SettingsWindow>().handle);
            assert!(!window.has_active_dialog(cx));
        });
    }

    #[test]
    fn store_copies_link_to_the_store_instead_of_github_releases() {
        assert_eq!(
            releases_link(true),
            ("View in Microsoft Store", MICROSOFT_STORE_URL)
        );
        assert_eq!(
            releases_link(false),
            ("View releases", APPLICATION_RELEASES_URL)
        );
    }

    #[gpui::test]
    fn update_status_shows_only_while_there_is_update_work(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let version = crate::native_updater::ReleaseVersion::stable(0, 1, 5);
            let presentation = |status| ApplicationUpdatePresentation {
                state: UpdateMenuState {
                    status,
                    frequency: None,
                },
                ..Default::default()
            };
            for status in [
                UpdateMenuStatus::Idle,
                UpdateMenuStatus::Unavailable,
                UpdateMenuStatus::Store,
            ] {
                assert!(update_button(&presentation(status), cx).is_none());
            }
            for status in [
                UpdateMenuStatus::Checking,
                UpdateMenuStatus::Available(version),
                UpdateMenuStatus::Downloading,
                UpdateMenuStatus::Ready(version),
            ] {
                assert!(update_button(&presentation(status), cx).is_some());
            }
            cx.set_global(InAppMenuBarShown(true));
            assert!(
                update_button(&presentation(UpdateMenuStatus::Ready(version)), cx).is_none(),
                "the in-app menu bar carries the status instead"
            );
        });
    }

    #[test]
    fn download_progress_reads_as_size_and_percent_then_preparing() {
        let progress = UpdateProgress {
            received: 12_345_678,
            total: 56_100_000,
        };
        assert_eq!(progress.describe(), "12.3 MB of 56.1 MB (22%)");
        assert!(!progress.preparing());
        let done = UpdateProgress {
            received: 56_100_000,
            total: 56_100_000,
        };
        assert!(done.preparing());
        assert_eq!(done.describe(), "Preparing the update…");
        assert_eq!(UpdateProgress::default().fraction(), 0.);
    }
}
