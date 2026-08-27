//! Coherent gpui-component recreation of a representative Butter Paper shell.
//!
//! This binary is intentionally separate from the PDF gallery. It proves the
//! component-system boundary without changing the existing renderer spike.

use std::{
    env,
    io::{self, Write},
    time::Instant,
};

use gpui::{
    App, Application, Bounds, Context, IntoElement, Render, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, size,
};
use gpui_component::{
    ActiveTheme, Disableable, IconName, Root, Selectable, Sizable, Size, StyledExt,
    button::{Button, ButtonGroup, ButtonVariants},
    tab::{Tab, TabBar},
};
use gpui_component_assets::Assets;
use serde_json::json;

#[path = "../component_model.rs"]
mod component_model;

use component_model::{FitMode, MAX_ZOOM_PERCENT, MIN_ZOOM_PERCENT, ShellModel};

struct PerfProbe {
    enabled: bool,
    origin: Instant,
    first_frame_seen: bool,
    frame_scheduled: bool,
    last_frame_at: Option<Instant>,
    pending_interaction: Option<(&'static str, Instant)>,
    render_count: u64,
}

impl PerfProbe {
    fn from_environment() -> Self {
        Self {
            enabled: env::var_os("BP_GPUI_COMPONENT_PERF").is_some(),
            origin: Instant::now(),
            first_frame_seen: false,
            frame_scheduled: false,
            last_frame_at: None,
            pending_interaction: None,
            render_count: 0,
        }
    }

    fn mark_interaction(&mut self, name: &'static str) {
        if self.enabled {
            self.pending_interaction = Some((name, Instant::now()));
        }
    }

    fn emit(&self, event: &str, details: serde_json::Value) {
        if !self.enabled {
            return;
        }
        println!(
            "{}",
            json!({
                "schema_version": 1,
                "runtime": "gpui-component",
                "scenario": "component-shell",
                "event": event,
                "t_ms": self.origin.elapsed().as_secs_f64() * 1000.0,
                "pid": std::process::id(),
                "details": details,
            })
        );
        let _ = io::stdout().flush();
    }
}

struct ComponentMilestone {
    model: ShellModel,
    perf: PerfProbe,
}

impl ComponentMilestone {
    fn new() -> Self {
        Self {
            model: ShellModel::default(),
            perf: PerfProbe::from_environment(),
        }
    }

    fn schedule_frame_probe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.perf.enabled || self.perf.frame_scheduled {
            return;
        }
        self.perf.frame_scheduled = true;
        cx.on_next_frame(window, |this, _, _| {
            this.perf.frame_scheduled = false;
            let now = Instant::now();
            let interval_ms = this
                .perf
                .last_frame_at
                .replace(now)
                .map(|previous| now.duration_since(previous).as_secs_f64() * 1000.0);
            if !this.perf.first_frame_seen {
                this.perf.first_frame_seen = true;
                this.perf.emit("first-frame", json!({}));
            }
            if let Some((interaction, started_at)) = this.perf.pending_interaction.take() {
                this.perf.emit(
                    "interaction-visible",
                    json!({
                        "interaction": interaction,
                        "duration_ms": now.duration_since(started_at).as_secs_f64() * 1000.0,
                    }),
                );
            }
            if let Some(interval_ms) = interval_ms {
                this.perf
                    .emit("frame", json!({ "interval_ms": interval_ms }));
            }
        });
    }

    fn primitive_open_button(&self) -> Button {
        Button::new("open-document")
            .primary()
            .icon(IconName::FolderOpen)
            .label("Open")
            .tooltip("Open a PDF")
    }

    fn zoom_control(&self, cx: &mut Context<Self>) -> ButtonGroup {
        let zoom_out_disabled = self.model.zoom_percent <= MIN_ZOOM_PERCENT;
        let zoom_in_disabled = self.model.zoom_percent >= MAX_ZOOM_PERCENT;
        ButtonGroup::new("zoom-control")
            .outline()
            .compact()
            .child(
                Button::new("zoom-out")
                    .icon(IconName::Minus)
                    .tooltip("Zoom out")
                    .disabled(zoom_out_disabled),
            )
            .child(
                Button::new("zoom-value")
                    .label(format!("{:.0}%", self.model.zoom_percent))
                    .disabled(true),
            )
            .child(
                Button::new("zoom-in")
                    .icon(IconName::Plus)
                    .tooltip("Zoom in")
                    .disabled(zoom_in_disabled),
            )
            .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                if selected.contains(&0) {
                    this.model.change_zoom(-1);
                    this.perf.mark_interaction("zoom-out");
                } else if selected.contains(&2) {
                    this.model.change_zoom(1);
                    this.perf.mark_interaction("zoom-in");
                }
                cx.notify();
            }))
    }

    fn fit_control(&self, cx: &mut Context<Self>) -> ButtonGroup {
        ButtonGroup::new("fit-control")
            .outline()
            .compact()
            .child(
                Button::new("fit-width")
                    .label("Fit width")
                    .selected(self.model.fit_mode == FitMode::Width),
            )
            .child(
                Button::new("fit-page")
                    .label("Fit page")
                    .selected(self.model.fit_mode == FitMode::Page),
            )
            .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                if let Some(index) = selected.first() {
                    this.model.select_fit_mode(*index);
                    this.perf.mark_interaction("fit-mode");
                    cx.notify();
                }
            }))
    }

    fn scroll_control(&self, cx: &mut Context<Self>) -> ButtonGroup {
        ButtonGroup::new("scroll-control")
            .outline()
            .compact()
            .child(
                Button::new("continuous")
                    .label("Continuous")
                    .selected(self.model.continuous),
            )
            .child(
                Button::new("single-page")
                    .label("Single page")
                    .selected(!self.model.continuous),
            )
            .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                if let Some(index) = selected.first() {
                    this.model.select_scroll_mode(*index);
                    this.perf.mark_interaction("scroll-mode");
                    cx.notify();
                }
            }))
    }

    fn document_tabs(&self, cx: &mut Context<Self>) -> TabBar {
        TabBar::new("document-tabs")
            .outline()
            .with_size(Size::Small)
            .selected_index(self.model.active_document)
            .child(
                Tab::new()
                    .label("Hibbeler reference")
                    .suffix(IconName::Close),
            )
            .child(Tab::new().label("Review notes").suffix(IconName::Close))
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                this.model.active_document = *index;
                this.perf.mark_interaction("document-tab");
                cx.notify();
            }))
    }

    fn left_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .w(px(52.0))
            .p_2()
            .gap_2()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                Button::new("pages")
                    .ghost()
                    .icon(IconName::File)
                    .tooltip("Pages"),
            )
            .child(
                Button::new("outline")
                    .ghost()
                    .icon(IconName::PanelLeft)
                    .tooltip("Document outline"),
            )
    }

    fn right_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tools = [
            ("select", IconName::Inspector, "Select"),
            ("markup", IconName::Frame, "Rectangle markup"),
            ("measure", IconName::ResizeCorner, "Measure"),
        ];
        div()
            .v_flex()
            .w(px(60.0))
            .p_2()
            .gap_2()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .children(
                tools
                    .into_iter()
                    .enumerate()
                    .map(|(index, (id, icon, label))| {
                        Button::new(id)
                            .ghost()
                            .icon(icon)
                            .tooltip(label)
                            .selected(self.model.active_tool == index)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.active_tool = index;
                                this.perf.mark_interaction("tool-selection");
                                cx.notify();
                            }))
                    }),
            )
    }

    fn document_surface(&self, cx: &App) -> impl IntoElement {
        div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().muted)
            .child(
                div()
                    .w(px(430.0))
                    .h(px(560.0))
                    .p_8()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .shadow_lg()
                    .child(div().text_lg().font_semibold().child("Document surface boundary"))
                    .child(
                        div()
                            .mt_3()
                            .text_color(cx.theme().muted_foreground)
                            .child("PDF raster, annotations, selection handles, and two-axis scrolling remain Butter Paper domain UI."),
                    ),
            )
    }

    fn shell(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .v_flex()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .h(px(44.0))
                    .px_3()
                    .h_flex()
                    .gap_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(self.primitive_open_button())
                    .child(self.document_tabs(cx)),
            )
            .child(
                div()
                    .h(px(48.0))
                    .px_3()
                    .h_flex()
                    .justify_center()
                    .gap_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(self.zoom_control(cx))
                    .child(self.fit_control(cx))
                    .child(self.scroll_control(cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .h_flex()
                    .child(self.left_rail(cx))
                    .child(self.document_surface(cx))
                    .child(self.right_rail(cx)),
            )
    }
}

impl Render for ComponentMilestone {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started_at = Instant::now();
        self.schedule_frame_probe(window, cx);
        let shell = self.shell(cx).into_any_element();
        self.perf.render_count += 1;
        self.perf.emit(
            "render-tree",
            json!({
                "duration_ms": started_at.elapsed().as_secs_f64() * 1000.0,
                "render_count": self.perf.render_count,
            }),
        );
        shell
    }
}

fn main() {
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        gpui_component::init(cx);
        let bounds = Bounds::centered(None, size(px(1100.0), px(720.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let milestone = cx.new(|_| ComponentMilestone::new());
                cx.new(|cx| Root::new(milestone, window, cx))
            },
        )
        .expect("failed to open gpui-component milestone window");
        cx.activate(true);
    });
}
