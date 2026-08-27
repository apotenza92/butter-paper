mod model;

use std::time::Instant;

use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::{
    ActiveTheme, Icon, IconName, Root, Selectable, Sizable, StyledExt,
    button::{Button, ButtonVariants, Toggle},
    h_flex,
    scroll::ScrollableElement,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu,
        SidebarMenuItem, SidebarToggleButton,
    },
    tab::TabBar,
    v_flex,
};
use gpui_component_assets::Assets;

use self::model::ShellVariant;

pub struct Prototype {
    variant: ShellVariant,
    sidebar_collapsed: bool,
    active_tool: usize,
    render_count: u64,
    last_compose_micros: u128,
    started_at: Instant,
}

impl Prototype {
    fn new() -> Self {
        Self {
            variant: ShellVariant::Workbench,
            sidebar_collapsed: false,
            active_tool: 0,
            render_count: 0,
            last_compose_micros: 0,
            started_at: Instant::now(),
        }
    }

    pub fn for_capture(index: usize) -> Self {
        let mut prototype = Self::new();
        prototype.variant = ShellVariant::ALL[index.min(ShellVariant::ALL.len() - 1)];
        prototype
    }

    fn icon_button(id: &'static str, icon: IconName, tooltip: &'static str) -> Button {
        Button::new(id).ghost().small().icon(icon).tooltip(tooltip)
    }

    fn brand(&self, compact: bool, cx: &App) -> AnyElement {
        h_flex()
            .gap_2()
            .items_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size_7()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().primary)
                    .text_color(cx.theme().primary_foreground)
                    .child(Icon::new(IconName::BookOpen).size_4()),
            )
            .when(!compact, |this| {
                this.child(
                    v_flex()
                        .line_height(relative(1.15))
                        .child(div().text_sm().font_semibold().child("Butter Paper"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("GPUI 0.5.2 prototype"),
                        ),
                )
            })
            .into_any_element()
    }

    fn document_page(&self, wide: bool, cx: &App) -> AnyElement {
        let page_width = if wide { px(660.) } else { px(570.) };

        v_flex()
            .w(page_width)
            .min_h(px(700.))
            .flex_shrink_0()
            .gap_5()
            .p_8()
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .shadow_lg()
            .text_color(cx.theme().foreground)
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().primary)
                            .child("CHAPTER 12  ·  SAMPLE DOCUMENT"),
                    )
                    .child(div().text_2xl().font_semibold().child("Structural Analysis"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("A representative page for shell and interaction review"),
                    ),
            )
            .child(
                div()
                    .h(px(1.))
                    .w_full()
                    .bg(cx.theme().border),
            )
            .child(
                v_flex()
                    .gap_3()
                    .child(div().font_semibold().child("12.3  The Displacement Method"))
                    .child(
                        div()
                            .text_sm()
                            .line_height(relative(1.6))
                            .text_color(cx.theme().muted_foreground)
                            .child("Determine the reactions and draw the shear and moment diagrams for the beam. Assume a constant flexural rigidity throughout the member."),
                    ),
            )
            .child(
                v_flex()
                    .gap_3()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().muted.opacity(0.35))
                    .p_5()
                    .child(
                        h_flex()
                            .justify_between()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("20 kN/m")
                            .child("8 kN"),
                    )
                    .child(
                        h_flex()
                            .items_end()
                            .justify_center()
                            .gap_0()
                            .child(div().size_3().rounded_full().bg(cx.theme().foreground))
                            .child(div().w(px(330.)).h(px(5.)).bg(cx.theme().foreground))
                            .child(div().size_3().rounded_full().bg(cx.theme().foreground)),
                    )
                    .child(
                        h_flex()
                            .justify_center()
                            .gap_8()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("A")
                            .child("4 m")
                            .child("B"),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_2()
                            .child(div().text_sm().font_semibold().child("Equilibrium"))
                            .child(
                                div()
                                    .rounded(cx.theme().radius)
                                    .bg(cx.theme().muted.opacity(0.45))
                                    .p_3()
                                    .font_family("monospace")
                                    .text_sm()
                                    .child("ΣMₐ = 0\nRᵦ(4) − 20(4)(2) = 0"),
                            ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_2()
                            .child(div().text_sm().font_semibold().child("Result"))
                            .child(
                                div()
                                    .rounded(cx.theme().radius)
                                    .border_1()
                                    .border_color(cx.theme().primary.opacity(0.35))
                                    .bg(cx.theme().primary.opacity(0.08))
                                    .p_3()
                                    .text_sm()
                                    .child("Rₐ = 40 kN   Rᵦ = 40 kN"),
                            ),
                    ),
            )
            .child(
                div()
                    .mt_auto()
                    .text_center()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("128"),
            )
            .into_any_element()
    }

    fn tool_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let tools = [
            ("tool-select", IconName::Eye, "Select"),
            ("tool-text", IconName::ALargeSmall, "Text"),
            ("tool-note", IconName::Info, "Comment"),
            ("tool-frame", IconName::Frame, "Area"),
        ];

        h_flex()
            .gap_1()
            .children(
                tools
                    .into_iter()
                    .enumerate()
                    .map(|(index, (id, icon, label))| {
                        Toggle::new(id)
                            .icon(icon)
                            .tooltip(label)
                            .checked(self.active_tool == index)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active_tool = index;
                                cx.notify();
                            }))
                    }),
            )
            .into_any_element()
    }

    fn workbench(&self, cx: &mut Context<Self>) -> AnyElement {
        let icon_collapsed = self.sidebar_collapsed;
        let sidebar = Sidebar::new("documents-sidebar")
            .collapsible(SidebarCollapsible::Icon)
            .collapsed(icon_collapsed)
            .w(px(246.))
            .header(SidebarHeader::new().child(self.brand(icon_collapsed, cx)))
            .child(
                SidebarGroup::new("Workspace").child(
                    SidebarMenu::new().children([
                        SidebarMenuItem::new("Sample document")
                            .icon(IconName::File)
                            .active(true),
                        SidebarMenuItem::new("Recent").icon(IconName::FolderOpen),
                        SidebarMenuItem::new("Starred").icon(IconName::Star),
                    ]),
                ),
            )
            .child(
                SidebarGroup::new("Pages").child(
                    SidebarMenu::new().children([
                        SidebarMenuItem::new("126  Introduction").icon(IconName::BookOpen),
                        SidebarMenuItem::new("127  Method").icon(IconName::BookOpen),
                        SidebarMenuItem::new("128  Analysis")
                            .icon(IconName::BookOpen)
                            .active(true),
                        SidebarMenuItem::new("129  Example").icon(IconName::BookOpen),
                    ]),
                ),
            )
            .footer(
                SidebarFooter::new().child(
                    h_flex()
                        .gap_2()
                        .child(IconName::HardDrive)
                        .when(!icon_collapsed, |this| this.child("Local workspace")),
                ),
            );

        h_flex()
            .size_full()
            .child(sidebar)
            .child(
                v_flex()
                    .h_full()
                    .min_w_0()
                    .flex_1()
                    .child(
                        h_flex()
                            .h_12()
                            .px_3()
                            .gap_2()
                            .items_center()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().background)
                            .child(
                                SidebarToggleButton::new()
                                    .collapsed(icon_collapsed)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sidebar_collapsed = !this.sidebar_collapsed;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                TabBar::new("document-tabs")
                                    .segmented()
                                    .small()
                                    .selected_index(0)
                                    .children(["Sample document.pdf", "Notes"]),
                            )
                            .child(div().flex_1())
                            .child(self.tool_strip(cx))
                            .child(Self::icon_button("search", IconName::Search, "Search"))
                            .child(Self::icon_button("share", IconName::ExternalLink, "Share")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .justify_center()
                            .bg(cx.theme().muted.opacity(0.42))
                            .p_6()
                            .child(
                                div()
                                    .size_full()
                                    .overflow_y_scrollbar()
                                    .flex()
                                    .justify_center()
                                    .pb_12()
                                    .child(self.document_page(false, cx)),
                            ),
                    )
                    .child(self.status_bar(cx)),
            )
            .into_any_element()
    }

    fn focus(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .bg(cx.theme().muted.opacity(0.32))
            .child(
                h_flex()
                    .h(px(56.))
                    .px_5()
                    .gap_3()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .child(self.brand(false, cx))
                    .child(div().flex_1())
                    .child(
                        Button::new("page-back")
                            .ghost()
                            .small()
                            .icon(IconName::ChevronLeft),
                    )
                    .child(div().text_sm().font_medium().child("128 / 642"))
                    .child(
                        Button::new("page-next")
                            .ghost()
                            .small()
                            .icon(IconName::ChevronRight),
                    )
                    .child(div().w_2())
                    .child(
                        Button::new("zoom-out")
                            .outline()
                            .small()
                            .icon(IconName::Minus),
                    )
                    .child(Button::new("zoom-level").outline().small().label("92%"))
                    .child(
                        Button::new("zoom-in")
                            .outline()
                            .small()
                            .icon(IconName::Plus),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .justify_center()
                    .p_8()
                    .pb_16()
                    .child(self.document_page(true, cx))
                    .child(
                        div()
                            .absolute()
                            .bottom_5()
                            .left_0()
                            .right_0()
                            .flex()
                            .justify_center()
                            .child(
                                h_flex()
                                    .gap_1()
                                    .p_1()
                                    .rounded(cx.theme().radius_lg)
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .bg(cx.theme().background)
                                    .shadow_lg()
                                    .child(self.tool_strip(cx))
                                    .child(div().mx_1().h_5().w(px(1.)).bg(cx.theme().border))
                                    .child(Self::icon_button(
                                        "focus-search",
                                        IconName::Search,
                                        "Search document",
                                    ))
                                    .child(Self::icon_button(
                                        "focus-more",
                                        IconName::EllipsisVertical,
                                        "More actions",
                                    )),
                            ),
                    ),
            )
            .child(self.status_bar(cx))
            .into_any_element()
    }

    fn review(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .h(px(56.))
                    .px_4()
                    .gap_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .child(self.brand(false, cx))
                    .child(div().w_4())
                    .child(
                        TabBar::new("review-tabs")
                            .underline()
                            .selected_index(1)
                            .children(["Document", "Review", "History"]),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("resolve-all")
                            .outline()
                            .small()
                            .label("Resolve all"),
                    )
                    .child(
                        Button::new("export-review")
                            .primary()
                            .small()
                            .label("Export review"),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .w(px(288.))
                            .h_full()
                            .gap_3()
                            .p_4()
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().background)
                            .child(
                                h_flex()
                                    .justify_between()
                                    .child(div().font_semibold().child("Open comments"))
                                    .child(
                                        div()
                                            .rounded_full()
                                            .bg(cx.theme().primary)
                                            .text_color(cx.theme().primary_foreground)
                                            .px_2()
                                            .py_0p5()
                                            .text_xs()
                                            .child("3"),
                                    ),
                            )
                            .children([
                                self.comment_card(
                                    "AP",
                                    "Alex",
                                    "Can we make this result more prominent?",
                                    "Page 128",
                                    true,
                                    cx,
                                ),
                                self.comment_card(
                                    "MK",
                                    "Mina",
                                    "The diagram label needs a unit.",
                                    "Page 128",
                                    false,
                                    cx,
                                ),
                                self.comment_card(
                                    "JS",
                                    "Jordan",
                                    "Check this equation before export.",
                                    "Page 129",
                                    false,
                                    cx,
                                ),
                            ]),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .size_full()
                            .min_w_0()
                            .overflow_y_scrollbar()
                            .justify_center()
                            .bg(cx.theme().muted.opacity(0.42))
                            .p_6()
                            .pb_12()
                            .child(self.document_page(false, cx)),
                    )
                    .child(
                        v_flex()
                            .w(px(248.))
                            .h_full()
                            .gap_5()
                            .p_4()
                            .border_l_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().background)
                            .child(div().font_semibold().child("Review details"))
                            .child(self.detail_row("Status", "In review", cx))
                            .child(self.detail_row("Owner", "Alex Potenza", cx))
                            .child(self.detail_row("Modified", "Just now", cx))
                            .child(div().h(px(1.)).bg(cx.theme().border))
                            .child(div().text_sm().font_medium().child("Markup"))
                            .child(self.tool_strip(cx))
                            .child(div().flex_1())
                            .child(
                                Button::new("add-comment")
                                    .primary()
                                    .label("Add comment")
                                    .w_full(),
                            ),
                    ),
            )
            .child(self.status_bar(cx))
            .into_any_element()
    }

    fn comment_card(
        &self,
        initials: &'static str,
        author: &'static str,
        body: &'static str,
        page: &'static str,
        active: bool,
        cx: &App,
    ) -> AnyElement {
        v_flex()
            .gap_2()
            .p_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(if active {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .bg(if active {
                cx.theme().primary.opacity(0.06)
            } else {
                cx.theme().background
            })
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size_6()
                            .rounded_full()
                            .bg(cx.theme().muted)
                            .text_xs()
                            .font_semibold()
                            .child(initials),
                    )
                    .child(div().text_sm().font_medium().child(author))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(page),
                    ),
            )
            .child(div().text_sm().line_height(relative(1.4)).child(body))
            .into_any_element()
    }

    fn detail_row(&self, label: &'static str, value: &'static str, cx: &App) -> AnyElement {
        h_flex()
            .justify_between()
            .text_sm()
            .child(div().text_color(cx.theme().muted_foreground).child(label))
            .child(div().font_medium().child(value))
            .into_any_element()
    }

    fn status_bar(&self, cx: &App) -> AnyElement {
        h_flex()
            .h_7()
            .px_3()
            .gap_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("Native GPUI")
            .child("•")
            .child("gpui-component 0.5.2-dev")
            .child(div().flex_1())
            .child(format!("compose {} µs", self.last_compose_micros))
            .child("•")
            .child(format!("render {}", self.render_count))
            .child("•")
            .child(format!(
                "uptime {:.1}s",
                self.started_at.elapsed().as_secs_f32()
            ))
            .into_any_element()
    }

    fn prototype_switcher(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .h_11()
            .px_3()
            .gap_3()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .w(px(250.))
                    .child(div().text_sm().font_semibold().child("Shell direction"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.variant.description()),
                    ),
            )
            .child(h_flex().gap_1().children(ShellVariant::ALL.map(|variant| {
                Button::new(variant.label())
                    .small()
                    .label(variant.label())
                    .selected(self.variant == variant)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.variant = variant;
                        cx.notify();
                    }))
            })))
            .child(div().flex_1())
            .child(
                Button::new("next-shell")
                    .ghost()
                    .small()
                    .label("Next direction")
                    .icon(IconName::ArrowRight)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.variant = this.variant.next();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}

impl Render for Prototype {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compose_started = Instant::now();
        self.render_count += 1;

        let shell = match self.variant {
            ShellVariant::Workbench => self.workbench(cx),
            ShellVariant::Focus => self.focus(cx),
            ShellVariant::Review => self.review(cx),
        };

        let root = v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(div().flex_1().min_h_0().child(shell))
            .child(self.prototype_switcher(cx));

        self.last_compose_micros = compose_started.elapsed().as_micros();
        eprintln!(
            "BP_PERF variant={} render={} compose_us={}",
            self.variant.label(),
            self.render_count,
            self.last_compose_micros
        );

        root
    }
}

fn main() {
    let app = gpui_platform::application().with_assets(Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1320.), px(860.)), cx)),
            window_min_size: Some(size(px(980.), px(680.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Butter Paper · GPUI Next Prototype".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|_| Prototype::new());
                cx.new(|cx| Root::new(view, window, cx).bg(cx.theme().background))
            })
            .expect("failed to open Butter Paper GPUI prototype window");
        })
        .detach();
    });
}
