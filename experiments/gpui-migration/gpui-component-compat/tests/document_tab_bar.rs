use std::{cell::RefCell, rc::Rc};

use butter_paper_gpui_component_compat::{
    cad_view_control::{CadViewControl, CadViewOrganisation},
    continuous_view_control::{ContinuousViewControl, WheelBehavior},
    document_tab_bar::{
        BUILT_IN_TEMPLATES, DIRTY_CLOSE_CANCEL_ID, DIRTY_CLOSE_CONFIRMATION_ID,
        DIRTY_CLOSE_DESCRIPTION, DIRTY_CLOSE_DESCRIPTION_ID, DIRTY_CLOSE_DISCARD_ID,
        DIRTY_CLOSE_SAVE_ID, DIRTY_CLOSE_TITLE_ID, DOCUMENT_TAB_ACTIONS_ID, DOCUMENT_TAB_BAR_ID,
        DOCUMENT_TAB_CLOSE_FOCUS_MASK_GAP, DOCUMENT_TAB_CONTENT_ID,
        DOCUMENT_TAB_HOVER_MASK_SOLID_TAIL, DOCUMENT_TAB_HOVER_MASK_WIDTH,
        DOCUMENT_TAB_LIST_ACCESSIBLE_NAME, DOCUMENT_TAB_LIST_ID, DOCUMENT_TAB_MAX_WIDTH,
        DOCUMENT_TAB_OPEN_ID, DOCUMENT_TAB_REORDER_DESCRIPTION, DOCUMENT_TAB_REORDER_KEYSHORTCUTS,
        DOCUMENT_TAB_REORDER_STATUS_ID, DirtyCloseConfirmationEvent, DirtyCloseConfirmationIntent,
        DocumentTabActivationOrigin, DocumentTabBarTemplateSeam, DocumentTabEvent,
        DocumentTabReorderEvent, DocumentTabReorderOrigin, TEMPLATE_CONTROL_GROUP_ID,
        TEMPLATE_CREATE_ID, TEMPLATE_ITEM_IDS, TEMPLATE_MANAGE_ID, TEMPLATE_PICKER_ID,
        TEMPLATE_PICKER_POPOVER_ID, TEMPLATE_PRIMARY_ID, TEMPLATE_SAVE_DOCUMENT_ID,
        TemplateCatalogItem, TemplateCreationOrigin, TemplateSplitControl, TemplateSplitEvent,
        dirty_close_title, document_tab_close_accessible_label, document_tab_close_id,
        document_tab_hover_mask_id, document_tab_id, document_tab_label_id,
        format_document_tab_label,
    },
    page_view_control::{PageViewControl, PageViewMode},
    viewer_toolbar_strip::{VIEWER_TOOLBAR_CONTENT_ID, ViewerToolbarStrip},
    zoom_control::{DEFAULT_VIEWER_ZOOM, ZoomControl},
};
use gpui::{
    AppContext as _, Bounds, Context, Entity, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, ParentElement as _, Pixels, Render,
    ScrollDelta, ScrollWheelEvent, Styled as _, TestAppContext, Window, point, px,
};
use gpui_component::Root;

const NORMAL_WIDTH: f32 = 1200.;
const MINIMUM_WIDTH: f32 = 900.;
const CONSTRAINED_WIDTH: f32 = 320.;
const DOCUMENT_TAB_REQUIRED_WIDTH: f32 = 380.;
const VIEWER_TOOLBAR_REQUIRED_WIDTH: f32 = 667.;
const DOCUMENT_TAB_POINTER_DRAG_THRESHOLD: f32 = 6.;
const DOCUMENT_TAB_DRAG_SITE_PLAN_ID: &str = "document-tab-drag-site-plan";
const DOCUMENT_TAB_DROP_SITE_PLAN_ID: &str = "document-tab-drop-target-site-plan";
const DOCUMENT_TAB_DROP_STRUCTURAL_DETAILS_ID: &str = "document-tab-drop-target-structural-details";

struct TabBarHarness {
    width: Pixels,
    document_tabs: Entity<DocumentTabBarTemplateSeam>,
    viewer_toolbar: Entity<ViewerToolbarStrip>,
}

struct TemplateSplitHarness {
    control: Entity<TemplateSplitControl>,
    events: Vec<TemplateSplitEvent>,
}

impl TemplateSplitHarness {
    fn new(control: Entity<TemplateSplitControl>, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&control, |harness, _, event: &TemplateSplitEvent, cx| {
            harness.events.push(event.clone());
            cx.notify();
        })
        .detach();
        Self {
            control,
            events: Vec::new(),
        }
    }
}

impl Render for TemplateSplitHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .w(px(320.))
            .h(px(160.))
            .child(self.control.clone())
    }
}

impl Render for TabBarHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .w(self.width)
            .h(px(240.))
            .child(self.document_tabs.clone())
            .child(self.viewer_toolbar.clone())
    }
}

struct HarnessEntities {
    harness: Entity<TabBarHarness>,
    document_tabs: Entity<DocumentTabBarTemplateSeam>,
    viewer_toolbar: Entity<ViewerToolbarStrip>,
    continuous: Entity<ContinuousViewControl>,
    single_page: Entity<PageViewControl>,
    zoom: Entity<ZoomControl>,
    cad: Entity<CadViewControl>,
}

fn open_harness(cx: &mut TestAppContext) -> (&mut gpui::VisualTestContext, HarnessEntities) {
    cx.update(gpui_component::init);
    let entities = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let entities = entities.clone();
        move |window, cx| {
            let document_tabs = cx.new(DocumentTabBarTemplateSeam::new);
            let continuous = cx.new(|_| ContinuousViewControl::new());
            let single_page = cx.new(|_| PageViewControl::single_page());
            let zoom = cx.new(|_| ZoomControl::new());
            let cad = cx.new(|cx| CadViewControl::new(window, cx));
            let viewer_toolbar = cx.new(|cx| {
                ViewerToolbarStrip::new_with_cad_view(
                    continuous.clone(),
                    single_page.clone(),
                    zoom.clone(),
                    cad.clone(),
                    cx,
                )
            });
            let harness = cx.new(|_| TabBarHarness {
                width: px(NORMAL_WIDTH),
                document_tabs: document_tabs.clone(),
                viewer_toolbar: viewer_toolbar.clone(),
            });
            entities.replace(Some(HarnessEntities {
                harness: harness.clone(),
                document_tabs,
                viewer_toolbar,
                continuous,
                single_page,
                zoom,
                cad,
            }));
            Root::new(harness, window, cx)
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let entities = entities
        .borrow_mut()
        .take()
        .expect("the document-tab harness entities must be retained");
    (cx, entities)
}

fn bounds(cx: &mut gpui::VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must participate in rendered layout"))
}

fn click_target(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let position = bounds(cx, selector).center();
    cx.simulate_click(position, Modifiers::default());
}

fn double_click_target(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let position = bounds(cx, selector).center();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::default(),
        click_count: 2,
    });
}

fn press_button_key(cx: &mut gpui::VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).expect("the test key must parse");
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
}

fn press_modified_key(cx: &mut gpui::VisualTestContext, key: &str, modifiers: Modifiers) {
    let keystroke = Keystroke {
        modifiers,
        key: key.into(),
        key_char: None,
    };
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
}

fn hover_target(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let position = bounds(cx, selector).center();
    cx.simulate_mouse_move(position, None, Modifiers::default());
    draw(cx);
}

fn pointer_down(cx: &mut gpui::VisualTestContext, position: gpui::Point<Pixels>) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
}

fn pointer_up(cx: &mut gpui::VisualTestContext, position: gpui::Point<Pixels>) {
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
}

fn start_pointer_drag(
    cx: &mut gpui::VisualTestContext,
    source_selector: &'static str,
    target_selector: &'static str,
) -> gpui::Point<Pixels> {
    let start = bounds(cx, source_selector).center();
    let target = bounds(cx, target_selector).center();
    pointer_down(cx, start);
    cx.simulate_mouse_move(
        point(
            start.x + px(DOCUMENT_TAB_POINTER_DRAG_THRESHOLD + 1.),
            start.y,
        ),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    cx.simulate_mouse_move(target, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    target
}

fn is_focused(cx: &mut gpui::VisualTestContext, focus: &gpui::FocusHandle) -> bool {
    cx.update(|window, _| focus.is_focused(window))
}

fn contains_focused(cx: &mut gpui::VisualTestContext, focus: &gpui::FocusHandle) -> bool {
    cx.update(|window, cx| focus.contains_focused(window, cx))
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn set_width(cx: &mut gpui::VisualTestContext, harness: &Entity<TabBarHarness>, width: f32) {
    harness.update(cx, |harness, cx| {
        harness.width = px(width);
        cx.notify();
    });
    draw(cx);
}

fn assert_fully_inside(outer: Bounds<Pixels>, inner: Bounds<Pixels>) {
    assert!(
        inner.left() >= outer.left(),
        "{inner:?} extends left of {outer:?}"
    );
    assert!(
        inner.right() <= outer.right(),
        "{inner:?} extends right of {outer:?}"
    );
    assert!(
        inner.top() >= outer.top(),
        "{inner:?} extends above {outer:?}"
    );
    assert!(
        inner.bottom() <= outer.bottom(),
        "{inner:?} extends below {outer:?}"
    );
}

#[gpui::test]
fn template_split_starts_at_the_frozen_contract_and_rendered_ids(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);

    assert_eq!(DOCUMENT_TAB_BAR_ID, "document-tab-bar");
    assert_eq!(DOCUMENT_TAB_LIST_ID, "document-tab-list");
    assert_eq!(DOCUMENT_TAB_CONTENT_ID, "document-tab-content");
    assert_eq!(DOCUMENT_TAB_ACTIONS_ID, "document-tab-actions");
    assert_eq!(DOCUMENT_TAB_OPEN_ID, "document-tab-open");
    assert_eq!(TEMPLATE_CONTROL_GROUP_ID, "document-tab-template-controls");
    assert_eq!(TEMPLATE_PRIMARY_ID, "document-tab-new-pdf");
    assert_eq!(TEMPLATE_PICKER_ID, "document-tab-template-picker");
    assert_eq!(TEMPLATE_PICKER_POPOVER_ID, "template-picker");
    assert_eq!(TEMPLATE_MANAGE_ID, "template-picker-manage");
    assert_eq!(TEMPLATE_CREATE_ID, "template-picker-create");
    assert_eq!(TEMPLATE_SAVE_DOCUMENT_ID, "template-picker-save-document");
    assert_eq!(BUILT_IN_TEMPLATES.len(), 6);
    assert_eq!(
        BUILT_IN_TEMPLATES.map(|template| (template.id, template.name)),
        [
            ("built-in-blank", "Blank Paper"),
            ("built-in-dots", "Dot Grid"),
            ("built-in-grid", "Square Grid"),
            ("built-in-lined", "Ruled Paper"),
            ("built-in-isometric", "Isometric Grid"),
            ("built-in-triangle", "Triangle Grid"),
        ]
    );

    for stable_id in [
        DOCUMENT_TAB_BAR_ID,
        DOCUMENT_TAB_LIST_ID,
        DOCUMENT_TAB_CONTENT_ID,
        DOCUMENT_TAB_ACTIONS_ID,
        DOCUMENT_TAB_OPEN_ID,
        TEMPLATE_CONTROL_GROUP_ID,
        TEMPLATE_PRIMARY_ID,
        TEMPLATE_PICKER_ID,
    ] {
        let target = bounds(cx, stable_id);
        assert!(target.size.width > px(0.));
        assert!(target.size.height > px(0.));
    }

    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.tabs().len(), 2);
        assert_eq!(tabs.active_tab_index(), 0);
        assert_eq!(tabs.last_template_id(), "built-in-blank");
        assert_eq!(tabs.selected_template_id(), "built-in-blank");
        assert!(!tabs.is_picker_open());
        assert!(!tabs.is_creating());
        assert!(tabs.creation_events().is_empty());
        assert_eq!(tabs.manage_requests(), 0);
    });
}

#[gpui::test]
fn reusable_template_split_emits_semantic_creation_without_owning_document_tabs(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let entities = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let entities = entities.clone();
        move |window, cx| {
            let control = cx.new(TemplateSplitControl::new);
            let harness = cx.new(|cx| TemplateSplitHarness::new(control.clone(), cx));
            entities.replace(Some((harness.clone(), control)));
            Root::new(harness, window, cx)
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (harness, control) = entities.borrow_mut().take().unwrap();

    assert!(
        cx.debug_bounds(TEMPLATE_SAVE_DOCUMENT_ID).is_none(),
        "the save command is inside the closed picker"
    );

    click_target(cx, TEMPLATE_PRIMARY_ID);
    draw(cx);

    assert_eq!(
        harness.read_with(cx, |harness, _| harness.events.clone()),
        vec![TemplateSplitEvent::CreateRequested {
            template_id: "built-in-blank".into(),
            origin: TemplateCreationOrigin::Primary,
        }]
    );
    control.read_with(cx, |control, _| {
        assert_eq!(control.selected_template_id(), "built-in-blank");
        assert_eq!(control.last_template_id(), "built-in-blank");
        assert!(!control.is_picker_open());
    });

    control.update(cx, |control, cx| {
        control.set_creating(true, cx);
        control.set_save_document_enabled(false, cx);
    });
    draw(cx);
    click_target(cx, TEMPLATE_PRIMARY_ID);
    draw(cx);
    assert_eq!(
        harness.read_with(cx, |harness, _| harness.events.len()),
        1,
        "storage-busy suppression must keep the already-recorded event count stable"
    );
}

#[gpui::test]
fn template_split_consumes_dynamic_catalog_and_emits_active_document_save_command(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    let custom_id = "custom-project-grid";
    entities.document_tabs.update(cx, |tabs, cx| {
        tabs.apply_template_catalog(
            vec![
                TemplateCatalogItem::new("built-in-blank", "Blank Paper"),
                TemplateCatalogItem::new(custom_id, "Project Grid"),
            ],
            custom_id,
            cx,
        );
    });
    draw(cx);

    let template_control = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.template_control());
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.last_template_id(), custom_id);
        assert_eq!(tabs.selected_template_id(), custom_id);
    });
    assert_eq!(
        template_control.read_with(cx, |control, _| control.templates().to_vec()),
        vec![
            TemplateCatalogItem::new("built-in-blank", "Blank Paper"),
            TemplateCatalogItem::new(custom_id, "Project Grid"),
        ]
    );

    click_target(cx, TEMPLATE_PRIMARY_ID);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.tabs().last().unwrap().name, "Project Grid");
        assert_eq!(
            tabs.creation_events().last().unwrap().template_id,
            custom_id
        );
    });

    click_target(cx, TEMPLATE_PICKER_ID);
    draw(cx);
    bounds(cx, "template-picker-item-custom-project-grid");
    bounds(cx, TEMPLATE_SAVE_DOCUMENT_ID);
    click_target(cx, TEMPLATE_SAVE_DOCUMENT_ID);
    draw(cx);

    entities.document_tabs.read_with(cx, |tabs, _| {
        assert!(!tabs.is_picker_open());
        assert_eq!(tabs.save_document_as_template_commands().len(), 1);
        assert_eq!(
            tabs.save_document_as_template_commands()[0].tab_id,
            "template-document-1"
        );
        assert_eq!(
            tabs.save_document_as_template_commands()[0].document_name,
            "Project Grid"
        );
    });
}

#[gpui::test]
fn document_tabs_start_with_frozen_selection_and_close_ids_and_bounds(cx: &mut TestAppContext) {
    let (cx, _) = open_harness(cx);

    for stable_id in [
        "document-tab-site-plan",
        "document-tab-close-site-plan",
        "document-tab-structural-details",
        "document-tab-close-structural-details",
    ] {
        let target = bounds(cx, stable_id);
        assert!(target.size.width > px(0.));
        assert!(target.size.height > px(0.));
    }

    for close_id in [
        "document-tab-close-site-plan",
        "document-tab-close-structural-details",
    ] {
        assert_eq!(
            bounds(cx, close_id).size,
            gpui::size(px(24.), px(24.)),
            "the frozen Electron close hit target is 24 by 24 px"
        );
    }

    assert_eq!(DOCUMENT_TAB_LIST_ACCESSIBLE_NAME, "Open documents");
    assert_eq!(document_tab_id("site-plan"), "document-tab-site-plan");
    assert_eq!(
        document_tab_close_id("site-plan"),
        "document-tab-close-site-plan"
    );
    assert_eq!(
        document_tab_close_accessible_label("site-plan.pdf"),
        "Close site-plan.pdf"
    );
    assert_eq!(format_document_tab_label("drawing.pdf"), "drawing");
    assert_eq!(format_document_tab_label("drawing.PDF"), "drawing");
    assert_eq!(format_document_tab_label(".pdf"), ".pdf");
    assert_eq!(
        format_document_tab_label("drawing.pdf.backup"),
        "drawing.pdf.backup"
    );
}

#[gpui::test]
fn document_tab_labels_start_with_stable_rendered_seams_and_no_invented_tooltip(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);

    let tab = bounds(cx, "document-tab-site-plan");
    assert_eq!(
        document_tab_label_id("site-plan"),
        "document-tab-label-site-plan"
    );
    assert_eq!(
        document_tab_hover_mask_id("site-plan"),
        "document-tab-hover-mask-site-plan"
    );
    assert_eq!(DOCUMENT_TAB_HOVER_MASK_WIDTH, 34.);
    assert_eq!(DOCUMENT_TAB_HOVER_MASK_SOLID_TAIL, 14.);
    let label = bounds(cx, "document-tab-label-site-plan");
    let close = bounds(cx, "document-tab-close-site-plan");
    assert_fully_inside(tab, label);
    assert_fully_inside(tab, close);
    assert!(
        cx.debug_bounds("document-tab-hover-mask-site-plan")
            .is_none(),
        "the Electron label mask is absent until pointer hover or close focus"
    );
    assert!(
        cx.debug_bounds("document-tab-tooltip-site-plan").is_none(),
        "Electron defines no document-label tooltip, title, or tooltip ID"
    );

    hover_target(cx, "document-tab-site-plan");
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.hovered_tab_id().map(str::to_owned)),
        Some("site-plan".into())
    );
    let mask = bounds(cx, "document-tab-hover-mask-site-plan");
    assert_eq!(mask.size.width, px(DOCUMENT_TAB_HOVER_MASK_WIDTH));
    assert_fully_inside(tab, mask);
    assert_fully_inside(tab, bounds(cx, "document-tab-close-site-plan"));
    assert!(
        cx.debug_bounds("document-tab-tooltip-site-plan").is_none(),
        "pointer hover must not invent a label tooltip"
    );

    cx.simulate_mouse_move(point(px(1100.), px(220.)), None, Modifiers::default());
    draw(cx);
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.hovered_tab_id().map(str::to_owned)),
        None
    );
    assert!(
        cx.debug_bounds("document-tab-hover-mask-site-plan")
            .is_none(),
        "pointer exit restores the unmasked label"
    );

    let site_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("site-plan"))
        .expect("the active tab retains its focus handle");
    cx.update(|window, cx| {
        site_focus.focus(window, cx);
        window.focus_next(cx);
    });
    draw(cx);
    assert!(
        cx.debug_bounds("document-tab-tooltip-site-plan").is_none(),
        "keyboard focus must not invent a label tooltip"
    );
    assert!(
        cx.debug_bounds("document-tab-hover-mask-site-plan")
            .is_none(),
        "the exact keyboard-focus sibling mask remains an explicit component gap"
    );
    assert_eq!(
        DOCUMENT_TAB_CLOSE_FOCUS_MASK_GAP,
        "Pinned GPUI Component Button does not expose its internal FocusHandle to sibling content"
    );
}

#[gpui::test]
fn keyboard_reorder_starts_with_a_stable_rendered_polite_status_seam(cx: &mut TestAppContext) {
    let (cx, _) = open_harness(cx);

    assert_eq!(
        DOCUMENT_TAB_REORDER_STATUS_ID,
        "document-tab-reorder-status"
    );
    assert_eq!(
        DOCUMENT_TAB_REORDER_KEYSHORTCUTS,
        "Alt+Shift+ArrowLeft Alt+Shift+ArrowRight"
    );
    assert_eq!(
        DOCUMENT_TAB_REORDER_DESCRIPTION,
        "Drag to reorder. Press Alt+Shift+Left or Alt+Shift+Right to move this tab."
    );
    let status = bounds(cx, DOCUMENT_TAB_REORDER_STATUS_ID);
    assert_eq!(status.size, gpui::size(px(1.), px(1.)));
}

#[gpui::test]
fn pointer_drag_starts_only_beyond_six_pixels_and_renders_stable_drag_tracers(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    let site = bounds(cx, "document-tab-site-plan");
    let structural = bounds(cx, "document-tab-structural-details");
    let start = site.center();
    pointer_down(cx, start);

    cx.simulate_mouse_move(
        point(start.x + px(DOCUMENT_TAB_POINTER_DRAG_THRESHOLD), start.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    assert!(
        cx.debug_bounds(DOCUMENT_TAB_DRAG_SITE_PLAN_ID).is_none(),
        "the installed dnd-kit sensor uses a strict greater-than-six-pixels comparison"
    );
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            ["site-plan", "structural-details"]
        );
        assert!(tabs.reorder_events().is_empty());
    });

    cx.simulate_mouse_move(
        point(
            start.x + px(DOCUMENT_TAB_POINTER_DRAG_THRESHOLD + 1.),
            start.y,
        ),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    let site_tab = bounds(cx, "document-tab-site-plan");
    let drag_trace = bounds(cx, DOCUMENT_TAB_DRAG_SITE_PLAN_ID);
    let drop_trace = bounds(cx, DOCUMENT_TAB_DROP_SITE_PLAN_ID);
    assert_fully_inside(site_tab, drag_trace);
    assert_eq!(drag_trace, drop_trace);
    assert_eq!(drag_trace.left(), site_tab.left() + px(1.));
    assert_eq!(drag_trace.right(), site_tab.right() - px(1.));
    assert_eq!(drag_trace.top(), site_tab.top() + px(1.));
    assert_eq!(drag_trace.bottom(), site_tab.bottom() - px(1.));

    cx.simulate_mouse_move(
        structural.center(),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    let dragged = bounds(cx, DOCUMENT_TAB_DRAG_SITE_PLAN_ID);
    let drop_target = bounds(cx, DOCUMENT_TAB_DROP_STRUCTURAL_DETAILS_ID);
    assert!(dragged.size.width > px(0.));
    assert!(drop_target.size.width > px(0.));
    assert_fully_inside(bounds(cx, "document-tab-site-plan"), dragged);
    assert_fully_inside(bounds(cx, "document-tab-structural-details"), drop_target);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            ["site-plan", "structural-details"],
            "drag preview must not commit before pointer release"
        );
        assert!(tabs.reorder_events().is_empty());
    });
}

#[gpui::test]
fn pointer_release_reorders_both_directions_and_moves_complete_stable_identity(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    let viewer_before = entities.viewer_toolbar.read_with(cx, |toolbar, _| {
        (toolbar.fit_preset(), toolbar.page_view_mode())
    });
    let continuous_before = entities
        .continuous
        .read_with(cx, |control, _| control.wheel_behavior());
    let single_before = entities
        .single_page
        .read_with(cx, |control, _| control.wheel_behavior());
    let zoom_before = entities.zoom.read_with(cx, |zoom, _| zoom.zoom());
    let cad_before = entities.cad.read_with(cx, |cad, _| {
        (cad.is_active(), cad.organisation(), cad.pages_per_column())
    });
    let site_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("site-plan"))
        .expect("the active tab retains stable focus identity");
    cx.update(|window, cx| site_focus.focus(window, cx));

    let target = start_pointer_drag(
        cx,
        "document-tab-site-plan",
        "document-tab-structural-details",
    );
    pointer_up(cx, target);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| (tab.id.as_str(), tab.name.as_str(), tab.dirty))
                .collect::<Vec<_>>(),
            [
                ("structural-details", "structural-details", true),
                ("site-plan", "site-plan", false),
            ]
        );
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
        assert_eq!(
            tabs.reorder_events(),
            [DocumentTabReorderEvent {
                tab_id: "site-plan".into(),
                from_ix: 0,
                to_ix: 1,
                origin: DocumentTabReorderOrigin::Pointer,
                announcement: "Moved site-plan to position 2 of 2.".into(),
            }]
        );
        assert_eq!(tabs.pointer_drag(), None);
    });
    assert!(is_focused(cx, &site_focus));

    let target = start_pointer_drag(
        cx,
        "document-tab-site-plan",
        "document-tab-structural-details",
    );
    pointer_up(cx, target);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            ["site-plan", "structural-details"]
        );
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
        assert_eq!(tabs.reorder_events().len(), 2);
        assert_eq!(
            tabs.reorder_announcement(),
            "Moved site-plan to position 1 of 2."
        );
    });

    click_target(cx, TEMPLATE_PRIMARY_ID);
    draw(cx);
    let template_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("template-document-1"))
        .expect("the temporary tab retains stable focus identity");
    let target = start_pointer_drag(
        cx,
        "document-tab-template-document-1",
        "document-tab-site-plan",
    );
    pointer_up(cx, target);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| (tab.id.as_str(), tab.name.as_str(), tab.dirty))
                .collect::<Vec<_>>(),
            [
                ("template-document-1", "Blank Paper", true),
                ("site-plan", "site-plan", false),
                ("structural-details", "structural-details", true),
            ]
        );
        assert_eq!(tabs.active_tab_id(), Some("template-document-1"));
        assert_eq!(tabs.creation_events().len(), 1);
        assert_eq!(tabs.reorder_events().last().unwrap().from_ix, 2);
        assert_eq!(tabs.reorder_events().last().unwrap().to_ix, 0);
        assert_eq!(
            tabs.reorder_announcement(),
            "Moved Blank Paper to position 1 of 3."
        );
    });
    assert!(is_focused(cx, &template_focus));

    assert_eq!(
        entities.viewer_toolbar.read_with(cx, |toolbar, _| {
            (toolbar.fit_preset(), toolbar.page_view_mode())
        }),
        viewer_before
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        continuous_before
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.wheel_behavior()),
        single_before
    );
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        zoom_before
    );
    assert_eq!(
        entities.cad.read_with(cx, |cad, _| {
            (cad.is_active(), cad.organisation(), cad.pages_per_column())
        }),
        cad_before
    );
}

#[gpui::test]
fn pointer_drag_preserves_click_noop_cancel_button_and_close_isolation(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);

    let structural_start = bounds(cx, "document-tab-structural-details").center();
    pointer_down(cx, structural_start);
    cx.simulate_mouse_move(
        point(
            structural_start.x + px(DOCUMENT_TAB_POINTER_DRAG_THRESHOLD),
            structural_start.y,
        ),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    pointer_up(
        cx,
        point(
            structural_start.x + px(DOCUMENT_TAB_POINTER_DRAG_THRESHOLD),
            structural_start.y,
        ),
    );
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.active_tab_id(), Some("structural-details"));
        assert!(tabs.reorder_events().is_empty());
        assert_eq!(tabs.pointer_drag(), None);
    });

    click_target(cx, "document-tab-site-plan");
    draw(cx);
    let same_target = start_pointer_drag(
        cx,
        "document-tab-structural-details",
        "document-tab-structural-details",
    );
    pointer_up(cx, same_target);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
        assert!(tabs.reorder_events().is_empty());
        assert!(tabs.reorder_announcement().is_empty());
    });

    let target = start_pointer_drag(
        cx,
        "document-tab-site-plan",
        "document-tab-structural-details",
    );
    press_button_key(cx, "escape");
    draw(cx);
    assert!(cx.debug_bounds(DOCUMENT_TAB_DRAG_SITE_PLAN_ID).is_none());
    pointer_up(cx, target);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            ["site-plan", "structural-details"]
        );
        assert!(tabs.reorder_events().is_empty());
    });

    let target = start_pointer_drag(
        cx,
        "document-tab-site-plan",
        "document-tab-structural-details",
    );
    cx.simulate_mouse_move(target, None, Modifiers::default());
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pointer_drag(), None);
        assert!(tabs.reorder_events().is_empty());
    });
    pointer_up(cx, target);
    draw(cx);

    let site = bounds(cx, "document-tab-site-plan").center();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: site,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    let structural_center = bounds(cx, "document-tab-structural-details").center();
    cx.simulate_mouse_move(
        structural_center,
        Some(MouseButton::Right),
        Modifiers::default(),
    );
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Right,
        position: site,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pointer_drag(), None);
        assert!(tabs.reorder_events().is_empty());
    });

    hover_target(cx, "document-tab-close-site-plan");
    let close = bounds(cx, "document-tab-close-site-plan").center();
    pointer_down(cx, close);
    let structural_center = bounds(cx, "document-tab-structural-details").center();
    cx.simulate_mouse_move(
        structural_center,
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pointer_drag(), None);
        assert_eq!(tabs.tabs().len(), 2);
        assert!(tabs.reorder_events().is_empty());
    });
    let release = bounds(cx, "document-tab-structural-details").center();
    pointer_up(cx, release);
    draw(cx);
}

#[gpui::test]
fn pointer_drag_keeps_pending_dirty_confirmation_attached_to_document_identity(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);

    let start = bounds(cx, "document-tab-structural-details").center();
    let target = bounds(cx, "document-tab-site-plan").center();
    cx.simulate_mouse_move(start, None, Modifiers::default());
    draw(cx);
    pointer_down(cx, start);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pointer_drag().map(|drag| drag.activated), Some(false));
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
    });
    cx.simulate_mouse_move(
        point(
            start.x + px(DOCUMENT_TAB_POINTER_DRAG_THRESHOLD + 1.),
            start.y,
        ),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
        assert_eq!(tabs.pointer_drag().map(|drag| drag.activated), Some(true));
    });
    cx.simulate_mouse_move(target, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
        assert_eq!(tabs.pointer_drag().map(|drag| drag.activated), Some(true));
    });
    pointer_up(cx, target);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
        assert_eq!(tabs.tabs()[0].id, "structural-details");
        assert!(tabs.tabs()[0].dirty);
        assert_eq!(tabs.tabs()[1].id, "site-plan");
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
        assert_eq!(
            tabs.reorder_events().last().unwrap().origin,
            DocumentTabReorderOrigin::Pointer
        );
    });
}

#[gpui::test]
fn pointer_drag_preview_preserves_tab_strip_geometry_at_all_frozen_widths(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);
    for width in [NORMAL_WIDTH, MINIMUM_WIDTH, CONSTRAINED_WIDTH] {
        set_width(cx, &entities.harness, width);
        let content_before = bounds(cx, DOCUMENT_TAB_CONTENT_ID);
        let site_before = bounds(cx, "document-tab-site-plan");
        let structural_before = bounds(cx, "document-tab-structural-details");
        let close_before = bounds(cx, "document-tab-close-site-plan");
        let target = start_pointer_drag(
            cx,
            "document-tab-site-plan",
            "document-tab-structural-details",
        );
        let content_during = bounds(cx, DOCUMENT_TAB_CONTENT_ID);
        let site_during = bounds(cx, "document-tab-site-plan");
        let structural_during = bounds(cx, "document-tab-structural-details");
        let close_during = bounds(cx, "document-tab-close-site-plan");

        assert_eq!(content_during.size, content_before.size);
        assert_eq!(site_during.size, site_before.size);
        assert_eq!(structural_during.size, structural_before.size);
        assert_eq!(close_during.size, close_before.size);
        assert!(structural_during.right() <= site_during.left());
        assert_fully_inside(content_during, site_during);
        assert_fully_inside(content_during, structural_during);
        assert_fully_inside(site_during, close_during);

        press_button_key(cx, "escape");
        pointer_up(cx, target);
        draw(cx);
        assert_eq!(
            bounds(cx, DOCUMENT_TAB_CONTENT_ID).size,
            content_before.size
        );
        assert_eq!(bounds(cx, "document-tab-site-plan"), site_before);
        assert_eq!(
            bounds(cx, "document-tab-structural-details"),
            structural_before
        );
    }
    assert_eq!(
        bounds(cx, VIEWER_TOOLBAR_CONTENT_ID).size.width,
        px(VIEWER_TOOLBAR_REQUIRED_WIDTH)
    );
}

#[gpui::test]
fn alt_shift_right_reorders_the_focused_tab_by_stable_rendered_identity(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);
    let site_before = bounds(cx, "document-tab-site-plan");
    let structural_before = bounds(cx, "document-tab-structural-details");
    assert!(site_before.left() < structural_before.left());

    let site_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("site-plan"))
        .expect("the active tab retains its focus handle");
    cx.update(|window, cx| site_focus.focus(window, cx));
    press_button_key(cx, "alt-shift-right");
    draw(cx);

    let site_after = bounds(cx, "document-tab-site-plan");
    let structural_after = bounds(cx, "document-tab-structural-details");
    assert!(structural_after.left() < site_after.left());
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.tabs()[0].id, "structural-details");
        assert_eq!(tabs.tabs()[1].id, "site-plan");
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
        assert_eq!(
            tabs.reorder_events(),
            [DocumentTabReorderEvent {
                tab_id: "site-plan".into(),
                from_ix: 0,
                to_ix: 1,
                origin: DocumentTabReorderOrigin::Keyboard,
                announcement: "Moved site-plan to position 2 of 2.".into(),
            }]
        );
        assert_eq!(
            tabs.reorder_announcement(),
            "Moved site-plan to position 2 of 2."
        );
    });
    assert!(is_focused(cx, &site_focus));

    press_button_key(cx, "alt-shift-left");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.tabs()[0].id, "site-plan");
        assert_eq!(tabs.tabs()[1].id, "structural-details");
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
        assert_eq!(tabs.reorder_events().len(), 2);
        assert_eq!(
            tabs.reorder_announcement(),
            "Moved site-plan to position 1 of 2."
        );
    });
    assert!(is_focused(cx, &site_focus));
}

#[gpui::test]
fn keyboard_reorder_preserves_identity_boundaries_modifiers_and_other_feature_state(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    let viewer_before = entities.viewer_toolbar.read_with(cx, |toolbar, _| {
        (toolbar.fit_preset(), toolbar.page_view_mode())
    });
    let continuous_before = entities
        .continuous
        .read_with(cx, |control, _| control.wheel_behavior());
    let single_before = entities
        .single_page
        .read_with(cx, |control, _| control.wheel_behavior());
    let zoom_before = entities.zoom.read_with(cx, |zoom, _| zoom.zoom());
    let cad_before = entities.cad.read_with(cx, |cad, _| {
        (cad.is_active(), cad.organisation(), cad.pages_per_column())
    });

    click_target(cx, TEMPLATE_PRIMARY_ID);
    draw(cx);
    let template_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("template-document-1"))
        .expect("the template-created tab retains stable focus identity");
    cx.update(|window, cx| template_focus.focus(window, cx));
    press_button_key(cx, "alt-shift-left");
    press_button_key(cx, "alt-shift-left");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| (tab.id.as_str(), tab.name.as_str(), tab.dirty))
                .collect::<Vec<_>>(),
            [
                ("template-document-1", "Blank Paper", true),
                ("site-plan", "site-plan", false),
                ("structural-details", "structural-details", true),
            ]
        );
        assert_eq!(tabs.active_tab_id(), Some("template-document-1"));
        assert_eq!(tabs.creation_events().len(), 1);
        assert_eq!(tabs.manage_requests(), 0);
        assert_eq!(
            tabs.reorder_announcement(),
            "Moved Blank Paper to position 1 of 3."
        );
    });
    assert!(is_focused(cx, &template_focus));

    let events_at_first_boundary = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.reorder_events().len());
    press_button_key(cx, "alt-shift-left");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.reorder_events().len(), events_at_first_boundary);
        assert_eq!(
            tabs.reorder_announcement(),
            "Moved Blank Paper to position 1 of 3."
        );
    });

    let structural_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("structural-details"))
        .expect("the dirty inactive tab retains stable focus identity");
    cx.update(|window, cx| structural_focus.focus(window, cx));
    for modifiers in [
        Modifiers {
            alt: true,
            ..Default::default()
        },
        Modifiers {
            shift: true,
            ..Default::default()
        },
        Modifiers::default(),
    ] {
        press_modified_key(cx, "left", modifiers);
    }
    press_modified_key(
        cx,
        "home",
        Modifiers {
            alt: true,
            shift: true,
            ..Default::default()
        },
    );
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            ["template-document-1", "site-plan", "structural-details"]
        );
        assert_eq!(tabs.active_tab_id(), Some("template-document-1"));
    });
    cx.update(|window, cx| structural_focus.focus(window, cx));

    press_modified_key(
        cx,
        "left",
        Modifiers {
            control: true,
            alt: true,
            shift: true,
            ..Default::default()
        },
    );
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            ["template-document-1", "structural-details", "site-plan"]
        );
        assert_eq!(tabs.active_tab_id(), Some("template-document-1"));
        assert!(tabs.tabs()[1].dirty);
    });
    assert!(is_focused(cx, &structural_focus));

    press_modified_key(
        cx,
        "right",
        Modifiers {
            alt: true,
            shift: true,
            platform: true,
            ..Default::default()
        },
    );
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(
            tabs.tabs()
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            ["template-document-1", "site-plan", "structural-details"]
        );
        assert_eq!(tabs.active_tab_id(), Some("template-document-1"));
        assert_eq!(tabs.pending_dirty_close_id(), None);
    });
    assert!(is_focused(cx, &structural_focus));

    assert_eq!(
        entities.viewer_toolbar.read_with(cx, |toolbar, _| {
            (toolbar.fit_preset(), toolbar.page_view_mode())
        }),
        viewer_before
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        continuous_before
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.wheel_behavior()),
        single_before
    );
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        zoom_before
    );
    assert_eq!(
        entities.cad.read_with(cx, |cad, _| {
            (cad.is_active(), cad.organisation(), cad.pages_per_column())
        }),
        cad_before
    );
}

#[gpui::test]
fn keyboard_reorder_keeps_pending_dirty_confirmation_and_close_semantics_with_identity(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    let content_size_before = bounds(cx, DOCUMENT_TAB_CONTENT_ID).size;
    let template_before = entities.document_tabs.read_with(cx, |tabs, _| {
        (
            tabs.selected_template_id(),
            tabs.last_template_id(),
            tabs.creation_events().len(),
            tabs.manage_requests(),
        )
    });
    let viewer_before = entities.viewer_toolbar.read_with(cx, |toolbar, _| {
        (toolbar.fit_preset(), toolbar.page_view_mode())
    });
    let zoom_before = entities.zoom.read_with(cx, |zoom, _| zoom.zoom());
    let cad_before = entities.cad.read_with(cx, |cad, _| {
        (cad.is_active(), cad.organisation(), cad.pages_per_column())
    });

    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);
    entities.document_tabs.update(cx, |tabs, cx| {
        assert!(
            tabs.move_tab_by_keyboard("structural-details", -1, cx)
                .is_some()
        );
    });
    draw(cx);

    assert!(
        bounds(cx, "document-tab-structural-details").left()
            < bounds(cx, "document-tab-site-plan").left()
    );
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
        assert_eq!(tabs.tabs()[0].id, "structural-details");
        assert!(tabs.tabs()[0].dirty);
        assert_eq!(tabs.tabs()[1].id, "site-plan");
        assert_eq!(
            tabs.reorder_announcement(),
            "Moved structural-details to position 1 of 2."
        );
    });
    assert_eq!(
        bounds(cx, DOCUMENT_TAB_CONTENT_ID).size.width,
        content_size_before.width
    );

    click_target(cx, DIRTY_CLOSE_CANCEL_ID);
    draw(cx);
    entities.document_tabs.update(cx, |tabs, cx| {
        tabs.set_tab_dirty("structural-details", false, cx);
    });
    draw(cx);
    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), None);
        assert_eq!(tabs.tabs().len(), 1);
        assert_eq!(tabs.tabs()[0].id, "site-plan");
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
        assert_eq!(
            tabs.tab_events().last(),
            Some(&DocumentTabEvent::CleanClosed {
                tab_id: "structural-details".into(),
                was_active: false,
                post_close_active_tab_id: Some("site-plan".into()),
            })
        );
    });

    assert_eq!(
        entities.document_tabs.read_with(cx, |tabs, _| {
            (
                tabs.selected_template_id(),
                tabs.last_template_id(),
                tabs.creation_events().len(),
                tabs.manage_requests(),
            )
        }),
        template_before
    );
    assert_eq!(
        entities.viewer_toolbar.read_with(cx, |toolbar, _| {
            (toolbar.fit_preset(), toolbar.page_view_mode())
        }),
        viewer_before
    );
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        zoom_before
    );
    assert_eq!(
        entities.cad.read_with(cx, |cad, _| {
            (cad.is_active(), cad.organisation(), cad.pages_per_column())
        }),
        cad_before
    );
}

#[gpui::test]
fn document_tab_labels_cap_long_text_without_shrinking_targets_or_other_state(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    let short_tab_before = bounds(cx, "document-tab-site-plan");
    let toolbar_before = entities.viewer_toolbar.read_with(cx, |toolbar, _| {
        (toolbar.fit_preset(), toolbar.page_view_mode())
    });
    let continuous_before = entities
        .continuous
        .read_with(cx, |control, _| control.wheel_behavior());
    let single_before = entities
        .single_page
        .read_with(cx, |control, _| control.wheel_behavior());
    let zoom_before = entities.zoom.read_with(cx, |zoom, _| zoom.zoom());
    let template_before = entities.document_tabs.read_with(cx, |tabs, _| {
        (
            tabs.selected_template_id(),
            tabs.last_template_id(),
            tabs.creation_events().len(),
            tabs.manage_requests(),
        )
    });
    let cad_before = entities.cad.read_with(cx, |cad, _| {
        (cad.is_active(), cad.organisation(), cad.pages_per_column())
    });

    entities.document_tabs.update(cx, |tabs, cx| {
        tabs.set_tab_name(
            "structural-details",
            "structural-details-for-the-north-elevation-and-foundation.pdf",
            cx,
        );
    });
    draw(cx);

    let long_tab = bounds(cx, "document-tab-structural-details");
    let long_label = bounds(cx, "document-tab-label-structural-details");
    let long_close = bounds(cx, "document-tab-close-structural-details");
    assert_eq!(long_tab.size.width, px(DOCUMENT_TAB_MAX_WIDTH));
    assert!(long_label.size.width < long_tab.size.width);
    assert_eq!(long_close.size, gpui::size(px(24.), px(24.)));
    assert_fully_inside(long_tab, long_label);
    assert_fully_inside(long_tab, long_close);
    assert_eq!(bounds(cx, "document-tab-site-plan"), short_tab_before);

    let normal_long_tab = long_tab;
    let normal_long_label = long_label;
    let normal_long_close = long_close;
    set_width(cx, &entities.harness, MINIMUM_WIDTH);
    assert_eq!(
        bounds(cx, "document-tab-structural-details").size,
        normal_long_tab.size
    );
    assert_eq!(
        bounds(cx, "document-tab-label-structural-details").size,
        normal_long_label.size
    );
    assert_eq!(
        bounds(cx, "document-tab-close-structural-details").size,
        normal_long_close.size
    );

    set_width(cx, &entities.harness, CONSTRAINED_WIDTH);
    let constrained_content = bounds(cx, DOCUMENT_TAB_CONTENT_ID);
    let constrained_long_tab = bounds(cx, "document-tab-structural-details");
    let constrained_long_label = bounds(cx, "document-tab-label-structural-details");
    let constrained_long_close = bounds(cx, "document-tab-close-structural-details");
    assert_eq!(constrained_long_tab.size, normal_long_tab.size);
    assert_eq!(constrained_long_label.size, normal_long_label.size);
    assert_eq!(constrained_long_close.size, normal_long_close.size);
    assert_fully_inside(constrained_content, constrained_long_tab);
    assert_fully_inside(constrained_long_tab, constrained_long_label);
    assert_fully_inside(constrained_long_tab, constrained_long_close);
    assert!(constrained_content.size.width >= px(DOCUMENT_TAB_REQUIRED_WIDTH));

    hover_target(cx, "document-tab-structural-details");
    assert_eq!(
        bounds(cx, "document-tab-structural-details").size,
        normal_long_tab.size
    );
    assert_eq!(
        bounds(cx, "document-tab-close-structural-details").size,
        normal_long_close.size
    );
    assert_eq!(
        bounds(cx, "document-tab-hover-mask-structural-details")
            .size
            .width,
        px(DOCUMENT_TAB_HOVER_MASK_WIDTH)
    );
    assert!(
        cx.debug_bounds("document-tab-tooltip-structural-details")
            .is_none()
    );

    assert_eq!(
        bounds(cx, VIEWER_TOOLBAR_CONTENT_ID).size.width,
        px(VIEWER_TOOLBAR_REQUIRED_WIDTH)
    );
    assert_eq!(
        entities.viewer_toolbar.read_with(cx, |toolbar, _| {
            (toolbar.fit_preset(), toolbar.page_view_mode())
        }),
        toolbar_before
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        continuous_before
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.wheel_behavior()),
        single_before
    );
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        zoom_before
    );
    assert_eq!(
        entities.document_tabs.read_with(cx, |tabs, _| {
            (
                tabs.selected_template_id(),
                tabs.last_template_id(),
                tabs.creation_events().len(),
                tabs.manage_requests(),
            )
        }),
        template_before
    );
    assert_eq!(
        entities.cad.read_with(cx, |cad, _| {
            (cad.is_active(), cad.organisation(), cad.pages_per_column())
        }),
        cad_before
    );
}

#[gpui::test]
fn dirty_close_confirmation_starts_at_frozen_rendered_ids_and_bounds(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);

    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);

    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
        assert_eq!(tabs.tabs().len(), 2);
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
    });

    assert_eq!(DIRTY_CLOSE_CONFIRMATION_ID, "confirmation-popover");
    assert_eq!(DIRTY_CLOSE_TITLE_ID, "dirty-close-confirmation-title");
    assert_eq!(
        DIRTY_CLOSE_DESCRIPTION_ID,
        "dirty-close-confirmation-description"
    );
    assert_eq!(DIRTY_CLOSE_CANCEL_ID, "dirty-close-confirmation-cancel");
    assert_eq!(DIRTY_CLOSE_DISCARD_ID, "dirty-close-confirmation-discard");
    assert_eq!(DIRTY_CLOSE_SAVE_ID, "dirty-close-confirmation-save");
    assert_eq!(
        dirty_close_title("site-plan.pdf"),
        "Save changes to site-plan.pdf?"
    );
    assert_eq!(
        DIRTY_CLOSE_DESCRIPTION,
        "Your changes will be lost if you close this tab without saving."
    );

    for stable_id in [
        DIRTY_CLOSE_CONFIRMATION_ID,
        DIRTY_CLOSE_TITLE_ID,
        DIRTY_CLOSE_DESCRIPTION_ID,
        DIRTY_CLOSE_CANCEL_ID,
        DIRTY_CLOSE_DISCARD_ID,
        DIRTY_CLOSE_SAVE_ID,
    ] {
        let target = bounds(cx, stable_id);
        assert!(target.size.width > px(0.));
        assert!(target.size.height > px(0.));
    }
}

#[gpui::test]
fn dirty_close_confirmation_traces_actions_dismissal_busy_focus_and_target_identity(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    let initial_tabs = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tabs().to_vec());
    let initial_template = entities.document_tabs.read_with(cx, |tabs, _| {
        (
            tabs.selected_template_id(),
            tabs.last_template_id(),
            tabs.creation_events().len(),
            tabs.manage_requests(),
        )
    });
    let initial_toolbar = entities.viewer_toolbar.read_with(cx, |toolbar, _| {
        (toolbar.fit_preset(), toolbar.page_view_mode())
    });
    let initial_zoom = entities.zoom.read_with(cx, |zoom, _| zoom.zoom());
    let initial_cad = entities.cad.read_with(cx, |cad, _| {
        (cad.is_active(), cad.organisation(), cad.pages_per_column())
    });
    let site_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("site-plan"))
        .expect("the active clean tab retains focus");
    let confirmation_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.confirmation_focus_handle());

    cx.update(|window, cx| site_focus.focus(window, cx));
    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);
    assert!(contains_focused(cx, &confirmation_focus));
    press_button_key(cx, "enter");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), None);
        assert_eq!(
            tabs.confirmation_events(),
            &[DirtyCloseConfirmationEvent {
                tab_id: "structural-details".into(),
                intent: DirtyCloseConfirmationIntent::Cancel,
            }]
        );
        assert_eq!(tabs.tabs(), initial_tabs);
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
    });
    assert!(is_focused(cx, &site_focus));

    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);
    cx.update(|window, cx| {
        window.focus_next(cx);
        window.focus_next(cx);
        window.focus_next(cx);
    });
    assert!(
        !contains_focused(cx, &confirmation_focus),
        "Electron's standard Popover is non-modal and does not trap focus"
    );
    cx.simulate_click(point(px(1100.), px(220.)), Modifiers::default());
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), None);
        assert_eq!(
            tabs.confirmation_events().last().map(|event| event.intent),
            Some(DirtyCloseConfirmationIntent::Cancel)
        );
    });

    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);
    click_target(cx, DIRTY_CLOSE_DISCARD_ID);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), None);
        assert_eq!(
            tabs.confirmation_events().last().map(|event| event.intent),
            Some(DirtyCloseConfirmationIntent::Discard)
        );
        assert_eq!(tabs.tabs(), initial_tabs);
    });

    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);
    click_target(cx, DIRTY_CLOSE_SAVE_ID);
    draw(cx);
    let event_count_while_busy = entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
        assert!(tabs.is_confirmation_busy());
        assert_eq!(
            tabs.confirmation_events().last().map(|event| event.intent),
            Some(DirtyCloseConfirmationIntent::Save)
        );
        assert_eq!(tabs.tabs(), initial_tabs);
        tabs.confirmation_events().len()
    });
    for action in [
        DIRTY_CLOSE_CANCEL_ID,
        DIRTY_CLOSE_DISCARD_ID,
        DIRTY_CLOSE_SAVE_ID,
    ] {
        click_target(cx, action);
    }
    cx.simulate_keystrokes("escape");
    cx.simulate_click(point(px(1100.), px(220.)), Modifiers::default());
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
        assert!(tabs.is_confirmation_busy());
        assert_eq!(tabs.confirmation_events().len(), event_count_while_busy);
        assert_eq!(tabs.tabs(), initial_tabs);
    });

    entities.document_tabs.update(cx, |tabs, cx| {
        tabs.set_confirmation_busy(false, cx);
    });
    draw(cx);
    cx.simulate_keystrokes("escape");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), None);
        assert_eq!(
            tabs.confirmation_events().last().map(|event| event.intent),
            Some(DirtyCloseConfirmationIntent::Cancel)
        );
    });

    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);
    cx.update(|window, cx| window.focus_next(cx));
    press_button_key(cx, "space");
    draw(cx);
    assert_eq!(
        entities.document_tabs.read_with(cx, |tabs, _| {
            tabs.confirmation_events().last().map(|event| event.intent)
        }),
        Some(DirtyCloseConfirmationIntent::Discard)
    );

    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);
    cx.update(|window, cx| {
        window.focus_next(cx);
        window.focus_next(cx);
    });
    press_button_key(cx, "enter");
    draw(cx);
    assert_eq!(
        entities.document_tabs.read_with(cx, |tabs, _| {
            tabs.confirmation_events().last().map(|event| event.intent)
        }),
        Some(DirtyCloseConfirmationIntent::Save)
    );

    entities.document_tabs.update(cx, |tabs, cx| {
        tabs.set_confirmation_busy(false, cx);
    });
    draw(cx);
    cx.simulate_keystrokes("escape");
    draw(cx);
    click_target(cx, "document-tab-structural-details");
    draw(cx);
    let structural_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("structural-details"))
        .expect("the dirty tab retains focus when active");
    cx.update(|window, cx| {
        structural_focus.focus(window, cx);
        window.focus_next(cx);
    });
    press_button_key(cx, "enter");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
        assert_eq!(tabs.active_tab_id(), Some("structural-details"));
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    press_button_key(cx, "enter");
    draw(cx);
    assert_eq!(
        entities.document_tabs.read_with(cx, |tabs, _| tabs
            .pending_dirty_close_id()
            .map(str::to_owned)),
        Some("structural-details".into()),
        "keyboard dismissal returns focus to the active close trigger"
    );

    let deferred_count = entities.document_tabs.read_with(cx, |tabs, _| {
        tabs.tab_events()
            .iter()
            .filter(|event| matches!(event, DocumentTabEvent::DirtyCloseDeferred { .. }))
            .count()
    });
    press_button_key(cx, "enter");
    draw(cx);
    assert_eq!(
        entities.document_tabs.read_with(cx, |tabs, _| {
            tabs.tab_events()
                .iter()
                .filter(|event| matches!(event, DocumentTabEvent::DirtyCloseDeferred { .. }))
                .count()
        }),
        deferred_count,
        "toggling an already-open trigger cancels without duplicating the close request"
    );

    assert_eq!(
        entities.document_tabs.read_with(cx, |tabs, _| {
            (
                tabs.selected_template_id(),
                tabs.last_template_id(),
                tabs.creation_events().len(),
                tabs.manage_requests(),
            )
        }),
        initial_template
    );
    assert_eq!(
        entities.viewer_toolbar.read_with(cx, |toolbar, _| {
            (toolbar.fit_preset(), toolbar.page_view_mode())
        }),
        initial_toolbar
    );
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        initial_zoom
    );
    assert_eq!(
        entities.cad.read_with(cx, |cad, _| {
            (cad.is_active(), cad.organisation(), cad.pages_per_column())
        }),
        initial_cad
    );
}

#[gpui::test]
fn dirty_close_confirmation_preserves_dialog_tab_strip_and_toolbar_geometry(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);

    let surface_ids = [
        DIRTY_CLOSE_TITLE_ID,
        DIRTY_CLOSE_DESCRIPTION_ID,
        DIRTY_CLOSE_CANCEL_ID,
        DIRTY_CLOSE_DISCARD_ID,
        DIRTY_CLOSE_SAVE_ID,
    ];
    let normal_popup = bounds(cx, DIRTY_CLOSE_CONFIRMATION_ID);
    assert_eq!(normal_popup.size.width, px(296.));
    let normal_surfaces = surface_ids.map(|id| bounds(cx, id));
    for surface in normal_surfaces {
        assert_fully_inside(normal_popup, surface);
    }
    assert!(normal_surfaces[2].right() <= normal_surfaces[3].left());
    assert!(normal_surfaces[3].right() <= normal_surfaces[4].left());

    set_width(cx, &entities.harness, MINIMUM_WIDTH);
    let minimum_popup = bounds(cx, DIRTY_CLOSE_CONFIRMATION_ID);
    assert_eq!(minimum_popup.size, normal_popup.size);
    for (normal, minimum) in normal_surfaces
        .iter()
        .zip(surface_ids.map(|id| bounds(cx, id)).iter())
    {
        assert_eq!(normal.size, minimum.size);
        assert_fully_inside(minimum_popup, *minimum);
    }

    set_width(cx, &entities.harness, CONSTRAINED_WIDTH);
    let constrained_popup = bounds(cx, DIRTY_CLOSE_CONFIRMATION_ID);
    assert_eq!(constrained_popup.size, normal_popup.size);
    assert!(constrained_popup.left() >= px(0.));
    assert!(constrained_popup.right() <= px(CONSTRAINED_WIDTH));
    let constrained_surfaces = surface_ids.map(|id| bounds(cx, id));
    for (normal, constrained) in normal_surfaces.iter().zip(constrained_surfaces.iter()) {
        assert_eq!(normal.size, constrained.size);
        assert_fully_inside(constrained_popup, *constrained);
    }
    assert!(constrained_surfaces[2].right() <= constrained_surfaces[3].left());
    assert!(constrained_surfaces[3].right() <= constrained_surfaces[4].left());

    for close_id in [
        "document-tab-close-site-plan",
        "document-tab-close-structural-details",
    ] {
        assert_eq!(bounds(cx, close_id).size, gpui::size(px(24.), px(24.)));
    }
    assert_eq!(
        bounds(cx, DOCUMENT_TAB_CONTENT_ID).size.width,
        px(DOCUMENT_TAB_REQUIRED_WIDTH)
    );
    assert_eq!(
        bounds(cx, VIEWER_TOOLBAR_CONTENT_ID).size.width,
        px(VIEWER_TOOLBAR_REQUIRED_WIDTH)
    );
}

#[gpui::test]
fn document_tabs_trace_pointer_selection_clean_close_and_dirty_boundary(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);
    let template_before = entities.document_tabs.read_with(cx, |tabs, _| {
        (
            tabs.selected_template_id(),
            tabs.last_template_id(),
            tabs.creation_events().len(),
            tabs.manage_requests(),
        )
    });
    let toolbar_before = entities.viewer_toolbar.read_with(cx, |toolbar, _| {
        (toolbar.fit_preset(), toolbar.page_view_mode())
    });
    let zoom_before = entities.zoom.read_with(cx, |zoom, _| zoom.zoom());
    let cad_before = entities.cad.read_with(cx, |cad, _| {
        (cad.is_active(), cad.organisation(), cad.pages_per_column())
    });

    click_target(cx, "document-tab-structural-details");
    draw(cx);
    let structural_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("structural-details"))
        .expect("the structural tab keeps a retained focus handle");
    assert!(is_focused(cx, &structural_focus));
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.active_tab_id(), Some("structural-details"));
        assert_eq!(
            tabs.tab_events(),
            &[DocumentTabEvent::Selected {
                tab_id: "structural-details".into(),
                origin: DocumentTabActivationOrigin::Pointer,
            }]
        );
    });

    double_click_target(cx, "document-tab-site-plan");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.active_tab_id(), Some("site-plan"));
        assert_eq!(tabs.tab_events().len(), 2);
        assert_eq!(
            tabs.tab_events()[1],
            DocumentTabEvent::Selected {
                tab_id: "site-plan".into(),
                origin: DocumentTabActivationOrigin::Pointer,
            },
            "Electron defines no separate document-tab double-click command"
        );
    });

    click_target(cx, "document-tab-close-site-plan");
    draw(cx);
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.tabs().len()),
        2,
        "the close action is pointer-inert until its parent tab is hovered"
    );

    hover_target(cx, "document-tab-close-site-plan");
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.hovered_tab_id().map(str::to_owned)),
        Some("site-plan".into())
    );
    assert_eq!(
        entities.document_tabs.read_with(cx, |tabs, _| {
            tabs.hovered_close_tab_id().map(str::to_owned)
        }),
        Some("site-plan".into())
    );
    click_target(cx, "document-tab-close-site-plan");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.tabs().len(), 1);
        assert_eq!(tabs.active_tab_id(), Some("structural-details"));
        assert_eq!(
            tabs.tab_events()[2],
            DocumentTabEvent::CleanClosed {
                tab_id: "site-plan".into(),
                was_active: true,
                post_close_active_tab_id: Some("structural-details".into()),
            }
        );
    });
    assert!(is_focused(cx, &structural_focus));

    hover_target(cx, "document-tab-close-structural-details");
    click_target(cx, "document-tab-close-structural-details");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.tabs().len(), 1);
        assert_eq!(tabs.active_tab_id(), Some("structural-details"));
        assert_eq!(tabs.pending_dirty_close_id(), Some("structural-details"));
        assert_eq!(
            tabs.tab_events()[3],
            DocumentTabEvent::DirtyCloseDeferred {
                tab_id: "structural-details".into(),
            }
        );
    });

    assert_eq!(
        entities.document_tabs.read_with(cx, |tabs, _| {
            (
                tabs.selected_template_id(),
                tabs.last_template_id(),
                tabs.creation_events().len(),
                tabs.manage_requests(),
            )
        }),
        template_before
    );
    assert_eq!(
        entities.viewer_toolbar.read_with(cx, |toolbar, _| {
            (toolbar.fit_preset(), toolbar.page_view_mode())
        }),
        toolbar_before
    );
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        zoom_before
    );
    assert_eq!(
        entities.cad.read_with(cx, |cad, _| {
            (cad.is_active(), cad.organisation(), cad.pages_per_column())
        }),
        cad_before
    );
}

#[gpui::test]
fn document_tabs_trace_inactive_clean_close_and_keyboard_successors(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);

    click_target(cx, "document-tab-structural-details");
    draw(cx);
    let structural_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("structural-details"))
        .expect("the active tab must be focusable");
    hover_target(cx, "document-tab-close-site-plan");
    click_target(cx, "document-tab-close-site-plan");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.active_tab_id(), Some("structural-details"));
        assert_eq!(tabs.tabs().len(), 1);
        assert_eq!(
            tabs.tab_events()[1],
            DocumentTabEvent::CleanClosed {
                tab_id: "site-plan".into(),
                was_active: false,
                post_close_active_tab_id: Some("structural-details".into()),
            }
        );
    });
    assert!(is_focused(cx, &structural_focus));

    entities.document_tabs.update(cx, |tabs, cx| {
        tabs.set_tab_dirty("structural-details", false, cx);
    });
    draw(cx);
    cx.update(|window, cx| window.focus_next(cx));
    press_button_key(cx, "enter");
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert!(tabs.tabs().is_empty());
        assert_eq!(tabs.active_tab_id(), None);
        assert_eq!(
            tabs.tab_events()[2],
            DocumentTabEvent::CleanClosed {
                tab_id: "structural-details".into(),
                was_active: true,
                post_close_active_tab_id: None,
            }
        );
    });
}

#[gpui::test]
fn document_tabs_trace_activate_on_focus_keyboard_contract(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);
    let site_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("site-plan"))
        .expect("the initial active tab must be focusable");
    cx.update(|window, cx| site_focus.focus(window, cx));

    for (key, expected) in [
        ("right", "structural-details"),
        ("left", "site-plan"),
        ("end", "structural-details"),
        ("home", "site-plan"),
        ("left", "structural-details"),
    ] {
        press_button_key(cx, key);
        draw(cx);
        assert_eq!(
            entities
                .document_tabs
                .read_with(cx, |tabs, _| { tabs.active_tab_id().map(str::to_owned) }),
            Some(expected.into())
        );
        let expected_focus = entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.tab_focus_handle(expected))
            .expect("each retained tab owns a focus handle");
        assert!(is_focused(cx, &expected_focus));
    }

    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.tab_events().len(), 5);
        assert!(tabs.tab_events().iter().all(|event| matches!(
            event,
            DocumentTabEvent::Selected {
                origin: DocumentTabActivationOrigin::Keyboard,
                ..
            }
        )));
    });
}

#[gpui::test]
fn template_split_traces_pointer_keyboard_dismissal_disabled_and_independence(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);

    let fit_before = entities
        .viewer_toolbar
        .read_with(cx, |toolbar, _| toolbar.fit_preset());
    let page_mode_before = entities
        .viewer_toolbar
        .read_with(cx, |toolbar, _| toolbar.page_view_mode());
    let continuous_before = entities
        .continuous
        .read_with(cx, |control, _| control.wheel_behavior());
    let single_before = entities
        .single_page
        .read_with(cx, |control, _| control.wheel_behavior());
    let zoom_before = entities.zoom.read_with(cx, |zoom, _| zoom.zoom());
    let cad_before = entities.cad.read_with(cx, |cad, _| {
        (cad.is_active(), cad.organisation(), cad.pages_per_column())
    });

    click_target(cx, TEMPLATE_PRIMARY_ID);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.tabs().len(), 3);
        assert_eq!(tabs.active_tab_index(), 2);
        assert_eq!(tabs.tabs()[0].id, "site-plan");
        assert_eq!(tabs.tabs()[1].id, "structural-details");
        assert!(tabs.tabs()[2].dirty);
        assert_eq!(tabs.creation_events().len(), 1);
        assert_eq!(tabs.creation_events()[0].template_id, "built-in-blank");
        assert_eq!(
            tabs.creation_events()[0].origin,
            TemplateCreationOrigin::Primary
        );
    });

    double_click_target(cx, TEMPLATE_PRIMARY_ID);
    draw(cx);
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.creation_events().len()),
        2,
        "Electron defines no extra primary double-click command"
    );

    let owner_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.focus_handle());
    cx.update(|window, cx| owner_focus.focus(window, cx));
    click_target(cx, TEMPLATE_PICKER_ID);
    draw(cx);
    assert!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.is_picker_open())
    );
    bounds(cx, TEMPLATE_PICKER_POPOVER_ID);
    for stable_id in TEMPLATE_ITEM_IDS {
        bounds(cx, stable_id);
    }
    bounds(cx, TEMPLATE_MANAGE_ID);
    bounds(cx, TEMPLATE_CREATE_ID);

    for (template, stable_id) in BUILT_IN_TEMPLATES.into_iter().zip(TEMPLATE_ITEM_IDS) {
        click_target(cx, stable_id);
        draw(cx);
        entities.document_tabs.read_with(cx, |tabs, _| {
            assert_eq!(tabs.selected_template_id(), template.id);
            assert_eq!(tabs.creation_events().len(), 2);
            assert_eq!(tabs.last_template_id(), "built-in-blank");
            assert!(tabs.is_picker_open());
        });
    }

    click_target(cx, TEMPLATE_CREATE_ID);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.creation_events().len(), 3);
        assert_eq!(tabs.creation_events()[2].template_id, "built-in-triangle");
        assert_eq!(
            tabs.creation_events()[2].origin,
            TemplateCreationOrigin::Create
        );
        assert_eq!(tabs.last_template_id(), "built-in-triangle");
        assert!(!tabs.is_picker_open());
    });

    cx.update(|window, cx| owner_focus.focus(window, cx));
    click_target(cx, TEMPLATE_PICKER_ID);
    draw(cx);
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.selected_template_id()),
        "built-in-triangle",
        "transient selection resets from the persisted last template on every open"
    );
    double_click_target(cx, TEMPLATE_ITEM_IDS[2]);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.creation_events().len(), 4);
        assert_eq!(tabs.creation_events()[3].template_id, "built-in-grid");
        assert_eq!(
            tabs.creation_events()[3].origin,
            TemplateCreationOrigin::RowDoubleClick
        );
        assert_eq!(tabs.last_template_id(), "built-in-grid");
        assert!(!tabs.is_picker_open());
    });

    click_target(cx, TEMPLATE_PICKER_ID);
    draw(cx);
    click_target(cx, TEMPLATE_MANAGE_ID);
    draw(cx);
    entities.document_tabs.read_with(cx, |tabs, _| {
        assert_eq!(tabs.manage_requests(), 1);
        assert_eq!(tabs.creation_events().len(), 4);
        assert!(!tabs.is_picker_open());
    });

    entities.document_tabs.update(cx, |tabs, cx| {
        tabs.set_creating(true, cx);
    });
    draw(cx);
    cx.update(|window, cx| owner_focus.focus(window, cx));
    click_target(cx, TEMPLATE_PICKER_ID);
    draw(cx);
    click_target(cx, TEMPLATE_CREATE_ID);
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.creation_events().len()),
        4,
        "the only Electron disabled state suppresses Create while creation is pending"
    );
    entities.document_tabs.update(cx, |tabs, cx| {
        tabs.set_creating(false, cx);
    });

    draw(cx);
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        !entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.is_picker_open())
    );
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&owner_focus),
        "Escape returns focus to the prior Document Tab Bar owner"
    );

    click_target(cx, TEMPLATE_PICKER_ID);
    draw(cx);
    let picker_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.picker_focus_handle());
    cx.update(|window, cx| {
        picker_focus.focus(window, cx);
        window.focus_next(cx);
        window.focus_next(cx);
    });
    press_button_key(cx, "enter");
    draw(cx);
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.selected_template_id()),
        "built-in-dots",
        "the Electron row contract uses standard Tab traversal and Enter activation"
    );
    cx.update(|window, cx| window.focus_next(cx));
    press_button_key(cx, "space");
    draw(cx);
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.selected_template_id()),
        "built-in-grid",
        "the standard template row also preserves Space activation"
    );

    cx.simulate_keystrokes("escape");
    draw(cx);
    let document_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.focus_handle());
    cx.update(|window, cx| {
        document_focus.focus(window, cx);
        // Active tab, active close, Open PDF, then the template primary.
        for _ in 0..4 {
            window.focus_next(cx);
        }
    });
    press_button_key(cx, "enter");
    draw(cx);
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.creation_events().len()),
        5,
        "the primary action remains Enter-activatable"
    );
    press_button_key(cx, "space");
    draw(cx);
    assert_eq!(
        entities
            .document_tabs
            .read_with(cx, |tabs, _| tabs.creation_events().len()),
        6,
        "the primary action remains Space-activatable"
    );

    assert_eq!(
        entities
            .viewer_toolbar
            .read_with(cx, |toolbar, _| toolbar.fit_preset()),
        fit_before
    );
    assert_eq!(
        entities
            .viewer_toolbar
            .read_with(cx, |toolbar, _| toolbar.page_view_mode()),
        page_mode_before
    );
    assert_eq!(page_mode_before, PageViewMode::Continuous);
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        continuous_before
    );
    assert_eq!(continuous_before, WheelBehavior::Scroll);
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.wheel_behavior()),
        single_before
    );
    assert_eq!(single_before, WheelBehavior::Zoom);
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        zoom_before
    );
    assert_eq!(zoom_before, DEFAULT_VIEWER_ZOOM);
    assert_eq!(
        entities.cad.read_with(cx, |cad, _| {
            (cad.is_active(), cad.organisation(), cad.pages_per_column())
        }),
        cad_before
    );
    assert_eq!(cad_before.1, CadViewOrganisation::Columns);
}

#[gpui::test]
fn document_tab_bar_preserves_targets_at_normal_minimum_and_overflow_widths(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_harness(cx);
    let target_ids = [
        DOCUMENT_TAB_OPEN_ID,
        TEMPLATE_PRIMARY_ID,
        TEMPLATE_PICKER_ID,
        "document-tab-site-plan",
        "document-tab-close-site-plan",
        "document-tab-structural-details",
        "document-tab-close-structural-details",
    ];
    let normal_bar = bounds(cx, DOCUMENT_TAB_BAR_ID);
    assert_eq!(normal_bar.size.width, px(NORMAL_WIDTH));
    let normal_content = bounds(cx, DOCUMENT_TAB_CONTENT_ID);
    let normal_targets_before_reorder = target_ids.map(|id| bounds(cx, id));
    for target in normal_targets_before_reorder {
        assert_fully_inside(normal_bar, target);
        assert_fully_inside(normal_content, target);
    }
    assert!(normal_targets_before_reorder[0].right() <= normal_targets_before_reorder[1].left());
    assert!(normal_targets_before_reorder[1].right() <= normal_targets_before_reorder[2].left());
    assert_fully_inside(
        normal_targets_before_reorder[3],
        normal_targets_before_reorder[4],
    );
    assert_fully_inside(
        normal_targets_before_reorder[5],
        normal_targets_before_reorder[6],
    );

    let site_focus = entities
        .document_tabs
        .read_with(cx, |tabs, _| tabs.tab_focus_handle("site-plan"))
        .expect("the active tab retains its focus handle");
    cx.update(|window, cx| site_focus.focus(window, cx));
    press_button_key(cx, "alt-shift-right");
    draw(cx);
    let normal_targets = target_ids.map(|id| bounds(cx, id));
    for (before, reordered) in normal_targets_before_reorder
        .iter()
        .zip(normal_targets.iter())
    {
        assert_eq!(before.size, reordered.size);
    }
    assert!(normal_targets[5].left() < normal_targets[3].left());
    assert_fully_inside(normal_targets[3], normal_targets[4]);
    assert_fully_inside(normal_targets[5], normal_targets[6]);

    set_width(cx, &entities.harness, MINIMUM_WIDTH);
    let minimum_bar = bounds(cx, DOCUMENT_TAB_BAR_ID);
    assert_eq!(minimum_bar.size.width, px(MINIMUM_WIDTH));
    let minimum_content = bounds(cx, DOCUMENT_TAB_CONTENT_ID);
    let minimum_targets = target_ids.map(|id| bounds(cx, id));
    for (normal, minimum) in normal_targets.iter().zip(minimum_targets.iter()) {
        assert_eq!(normal.size, minimum.size);
        assert_fully_inside(minimum_bar, *minimum);
        assert_fully_inside(minimum_content, *minimum);
    }

    set_width(cx, &entities.harness, CONSTRAINED_WIDTH);
    let constrained_bar = bounds(cx, DOCUMENT_TAB_BAR_ID);
    assert_eq!(constrained_bar.size.width, px(CONSTRAINED_WIDTH));
    let constrained_content = bounds(cx, DOCUMENT_TAB_CONTENT_ID);
    assert_eq!(
        constrained_content.size.width,
        px(DOCUMENT_TAB_REQUIRED_WIDTH)
    );
    let constrained_targets = target_ids.map(|id| bounds(cx, id));
    for (normal, constrained) in normal_targets.iter().zip(constrained_targets.iter()) {
        assert_eq!(normal.size, constrained.size);
        assert_fully_inside(constrained_content, *constrained);
    }
    assert_fully_inside(constrained_targets[3], constrained_targets[4]);
    assert_fully_inside(constrained_targets[5], constrained_targets[6]);
    assert!(constrained_targets[2].right() > constrained_bar.right());

    cx.simulate_event(ScrollWheelEvent {
        position: constrained_bar.center(),
        delta: ScrollDelta::Pixels(point(px(-900.), px(0.))),
        ..Default::default()
    });
    draw(cx);
    let scrolled_bar = bounds(cx, DOCUMENT_TAB_BAR_ID);
    let scrolled_targets = target_ids.map(|id| bounds(cx, id));
    for (normal, scrolled) in normal_targets.iter().zip(scrolled_targets.iter()) {
        assert_eq!(normal.size, scrolled.size);
    }
    assert_fully_inside(scrolled_bar, scrolled_targets[1]);
    assert_fully_inside(scrolled_bar, scrolled_targets[2]);

    assert_eq!(
        bounds(cx, VIEWER_TOOLBAR_CONTENT_ID).size.width,
        px(VIEWER_TOOLBAR_REQUIRED_WIDTH),
        "the previously accepted 667 px viewer-toolbar contract is unchanged"
    );
}
