use std::{cell::RefCell, rc::Rc};

use butter_paper_gpui_component_compat::{
    cad_view_control::{
        CAD_ORGANISATION_COLUMNS_ID, CAD_ORGANISATION_ROWS_ID, CAD_VIEW_DESCRIPTION,
        CAD_VIEW_GROUP_ID, CAD_VIEW_LABEL, CAD_VIEW_POPOVER_ID, CAD_VIEW_PRIMARY_ID,
        CAD_VIEW_SETTINGS_ID, CAD_VIEW_SETTINGS_LABEL, CadViewControl, CadViewOrganisation,
        DEFAULT_PAGES_PER_COLUMN, MAX_PAGES_PER_COLUMN, MIN_PAGES_PER_COLUMN, PAGES_PER_COLUMN_ID,
        PAGES_PER_ROW_ID, clamp_pages_per_column,
    },
    continuous_view_control::{
        CONTINUOUS_PRIMARY_ID, CONTINUOUS_SPLIT_ID, ContinuousViewControl, WheelBehavior,
    },
    page_view_control::{
        PageViewControl, PageViewMode, SINGLE_PAGE_PRIMARY_ID, SINGLE_PAGE_SPLIT_ID,
    },
    viewer_toolbar_strip::{
        FIT_BUTTON_GROUP_ID, FIT_PAGE_ID, FIT_WIDTH_ID, FitPreset, VIEWER_TOOLBAR_CONTENT_ID,
        VIEWER_TOOLBAR_ID, VIEWER_TOOLBAR_SCROLL_ID, ViewerToolbarStrip,
    },
    zoom_control::{
        DEFAULT_VIEWER_ZOOM, MAX_VIEWER_ZOOM, MIN_VIEWER_ZOOM, ZOOM_GROUP_ID, ZOOM_IN_ID,
        ZOOM_MENU_ID, ZOOM_OUT_ID, ZOOM_PRESETS, ZoomControl, clamp_viewer_zoom,
        format_zoom_percent,
    },
};
use gpui::{
    AppContext as _, Bounds, Context, Entity, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, ParentElement as _, Pixels, Render,
    ScrollDelta, ScrollWheelEvent, Styled as _, TestAppContext, Window, point, px,
};
use gpui_component::Root;

// The Electron shell has a 900 px minimum window with 180 px and 220 px
// minimum sidebars. The expanded center toolbar intentionally scrolls at the
// 480 px center-strip allowance and at the more constrained 320 px fixture.
const NORMAL_WIDTH: f32 = 720.;
const MINIMUM_CENTER_STRIP_WIDTH: f32 = 480.;
const CONSTRAINED_OVERFLOW_WIDTH: f32 = 320.;
const EXPANDED_TOOLBAR_REQUIRED_WIDTH: f32 = 607.;
const CAD_TOOLBAR_REQUIRED_WIDTH: f32 = 667.;
const EXPECTED_ZOOM_OUT_ID: &str = "viewer-zoom-out";
const EXPECTED_ZOOM_IN_ID: &str = "viewer-zoom-in";
const EXPECTED_ZOOM_MENU_ID: &str = "viewer-zoom-menu";
const EXPECTED_CAD_PRIMARY_ID: &str = "viewer-cad-view";
const EXPECTED_CAD_SETTINGS_ID: &str = "viewer-cad-view-settings";
const EXPECTED_CAD_ORGANISATION_COLUMNS_ID: &str = "viewer-cad-organisation-columns";
const EXPECTED_CAD_ORGANISATION_ROWS_ID: &str = "viewer-cad-organisation-rows";
const EXPECTED_PAGES_PER_COLUMN_ID: &str = "viewer-pages-per-column";

struct ToolbarHarness {
    width: Pixels,
    toolbar: Entity<ViewerToolbarStrip>,
}

impl Render for ToolbarHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .w(self.width)
            .h(px(80.))
            .child(self.toolbar.clone())
    }
}

struct HarnessEntities {
    harness: Entity<ToolbarHarness>,
    toolbar: Entity<ViewerToolbarStrip>,
    continuous: Entity<ContinuousViewControl>,
    single_page: Entity<PageViewControl>,
    zoom: Entity<ZoomControl>,
}

fn open_harness(cx: &mut TestAppContext) -> (&mut gpui::VisualTestContext, HarnessEntities) {
    cx.update(gpui_component::init);
    let entities = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let entities = entities.clone();
        move |window, cx| {
            let continuous = cx.new(|_| ContinuousViewControl::new());
            let single_page = cx.new(|_| PageViewControl::single_page());
            let zoom = cx.new(|_| ZoomControl::new());
            let toolbar = cx.new(|cx| {
                ViewerToolbarStrip::new_with_zoom(
                    continuous.clone(),
                    single_page.clone(),
                    zoom.clone(),
                    cx,
                )
            });
            let harness = cx.new(|_| ToolbarHarness {
                width: px(NORMAL_WIDTH),
                toolbar: toolbar.clone(),
            });
            entities.replace(Some(HarnessEntities {
                harness: harness.clone(),
                toolbar,
                continuous,
                single_page,
                zoom,
            }));
            Root::new(harness, window, cx)
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let entities = entities
        .borrow_mut()
        .take()
        .expect("the toolbar harness entities must be retained");
    (cx, entities)
}

struct CadHarnessEntities {
    harness: Entity<ToolbarHarness>,
    toolbar: Entity<ViewerToolbarStrip>,
    continuous: Entity<ContinuousViewControl>,
    single_page: Entity<PageViewControl>,
    zoom: Entity<ZoomControl>,
    cad_view: Entity<CadViewControl>,
}

fn open_cad_harness(cx: &mut TestAppContext) -> (&mut gpui::VisualTestContext, CadHarnessEntities) {
    cx.update(gpui_component::init);
    let entities = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let entities = entities.clone();
        move |window, cx| {
            let continuous = cx.new(|_| ContinuousViewControl::new());
            let single_page = cx.new(|_| PageViewControl::single_page());
            let zoom = cx.new(|_| ZoomControl::new());
            let cad_view = cx.new(|cx| CadViewControl::new(window, cx));
            let toolbar = cx.new(|cx| {
                ViewerToolbarStrip::new_with_cad_view(
                    continuous.clone(),
                    single_page.clone(),
                    zoom.clone(),
                    cad_view.clone(),
                    cx,
                )
            });
            let harness = cx.new(|_| ToolbarHarness {
                width: px(NORMAL_WIDTH),
                toolbar: toolbar.clone(),
            });
            entities.replace(Some(CadHarnessEntities {
                harness: harness.clone(),
                toolbar,
                continuous,
                single_page,
                zoom,
                cad_view,
            }));
            Root::new(harness, window, cx)
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let entities = entities
        .borrow_mut()
        .take()
        .expect("the CAD toolbar harness entities must be retained");
    (cx, entities)
}

fn bounds(cx: &mut gpui::VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must participate in rendered layout"))
}

fn set_width(cx: &mut gpui::VisualTestContext, harness: &Entity<ToolbarHarness>, width: f32) {
    harness.update(cx, |harness, cx| {
        harness.width = px(width);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn click_target(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let position = bounds(cx, selector).center();
    cx.simulate_click(position, Modifiers::default());
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

fn assert_fully_inside(outer: Bounds<Pixels>, inner: Bounds<Pixels>) {
    assert!(
        inner.left() >= outer.left(),
        "inner {inner:?} extends left of outer {outer:?}"
    );
    assert!(
        inner.right() <= outer.right(),
        "inner {inner:?} extends right of outer {outer:?}"
    );
    assert!(
        inner.top() >= outer.top(),
        "inner {inner:?} extends above outer {outer:?}"
    );
    assert!(
        inner.bottom() <= outer.bottom(),
        "inner {inner:?} extends below outer {outer:?}"
    );
}

fn assert_target_geometry_is_preserved(expected: &[Bounds<Pixels>], actual: &[Bounds<Pixels>]) {
    assert_eq!(expected.len(), actual.len());
    for (expected, actual) in expected.iter().zip(actual) {
        assert_eq!(actual.size, expected.size);
    }
}

fn interactive_bounds(cx: &mut gpui::VisualTestContext) -> [Bounds<Pixels>; 9] {
    [
        bounds(cx, ZOOM_OUT_ID),
        bounds(cx, ZOOM_IN_ID),
        bounds(cx, ZOOM_MENU_ID),
        bounds(cx, FIT_WIDTH_ID),
        bounds(cx, FIT_PAGE_ID),
        bounds(cx, CONTINUOUS_PRIMARY_ID),
        bounds(cx, CONTINUOUS_SPLIT_ID),
        bounds(cx, SINGLE_PAGE_PRIMARY_ID),
        bounds(cx, SINGLE_PAGE_SPLIT_ID),
    ]
}

fn cad_interactive_bounds(cx: &mut gpui::VisualTestContext) -> [Bounds<Pixels>; 11] {
    [
        bounds(cx, ZOOM_OUT_ID),
        bounds(cx, ZOOM_IN_ID),
        bounds(cx, ZOOM_MENU_ID),
        bounds(cx, FIT_WIDTH_ID),
        bounds(cx, FIT_PAGE_ID),
        bounds(cx, CONTINUOUS_PRIMARY_ID),
        bounds(cx, CONTINUOUS_SPLIT_ID),
        bounds(cx, SINGLE_PAGE_PRIMARY_ID),
        bounds(cx, SINGLE_PAGE_SPLIT_ID),
        bounds(cx, CAD_VIEW_PRIMARY_ID),
        bounds(cx, CAD_VIEW_SETTINGS_ID),
    ]
}

fn assert_cad_target_order(targets: &[Bounds<Pixels>; 11]) {
    assert!(targets[0].right() <= targets[1].left());
    assert!(targets[1].right() <= targets[2].left());
    assert!(targets[2].right() <= targets[3].left());
    assert!(targets[3].right() <= targets[4].left());
    assert!(targets[4].right() <= targets[6].left());
    assert_fully_inside(targets[6], targets[5]);
    assert!(targets[6].right() <= targets[8].left());
    assert_fully_inside(targets[8], targets[7]);
    assert!(targets[8].right() <= targets[9].left());
    assert!(targets[9].right() <= targets[10].left());
}

#[gpui::test]
fn zoom_group_starts_at_the_frozen_rendered_ids(cx: &mut TestAppContext) {
    let (cx, _) = open_harness(cx);

    for stable_id in [
        EXPECTED_ZOOM_OUT_ID,
        EXPECTED_ZOOM_IN_ID,
        EXPECTED_ZOOM_MENU_ID,
    ] {
        let target = bounds(cx, stable_id);
        assert!(target.size.width > px(0.));
        assert!(target.size.height > px(0.));
    }
}

#[gpui::test]
fn cad_view_starts_at_the_frozen_rendered_ids(cx: &mut TestAppContext) {
    let (cx, _) = open_cad_harness(cx);

    for stable_id in [EXPECTED_CAD_PRIMARY_ID, EXPECTED_CAD_SETTINGS_ID] {
        let target = bounds(cx, stable_id);
        assert!(target.size.width > px(0.));
        assert!(target.size.height > px(0.));
    }
}

#[gpui::test]
fn cad_view_popover_starts_with_the_frozen_configuration_targets(cx: &mut TestAppContext) {
    let (cx, entities) = open_cad_harness(cx);

    let toolbar_focus = entities
        .toolbar
        .read_with(cx, |toolbar, _| toolbar.focus_handle());
    cx.update(|window, cx| toolbar_focus.focus(window, cx));
    click_target(cx, EXPECTED_CAD_SETTINGS_ID);
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        entities
            .cad_view
            .read_with(cx, |cad_view, _| cad_view.is_settings_open())
    );
    for stable_id in [
        EXPECTED_CAD_ORGANISATION_COLUMNS_ID,
        EXPECTED_CAD_ORGANISATION_ROWS_ID,
        EXPECTED_PAGES_PER_COLUMN_ID,
    ] {
        let target = bounds(cx, stable_id);
        assert!(target.size.width > px(0.));
        assert!(target.size.height > px(0.));
    }
}

#[gpui::test]
fn cad_view_traces_primary_configuration_keyboard_escape_disabled_and_reset(
    cx: &mut TestAppContext,
) {
    let (cx, entities) = open_cad_harness(cx);

    assert_eq!(CAD_VIEW_GROUP_ID, "viewer-cad-view-controls");
    assert_eq!(CAD_VIEW_PRIMARY_ID, EXPECTED_CAD_PRIMARY_ID);
    assert_eq!(CAD_VIEW_SETTINGS_ID, EXPECTED_CAD_SETTINGS_ID);
    assert_eq!(CAD_VIEW_POPOVER_ID, "viewer-cad-settings");
    assert_eq!(CAD_VIEW_LABEL, "CAD View");
    assert_eq!(CAD_VIEW_SETTINGS_LABEL, "CAD View settings");
    assert_eq!(
        CAD_VIEW_DESCRIPTION,
        "Organise drawing sheets. Mousewheel always zooms in CAD View."
    );
    assert_eq!(
        CAD_ORGANISATION_COLUMNS_ID,
        EXPECTED_CAD_ORGANISATION_COLUMNS_ID
    );
    assert_eq!(CAD_ORGANISATION_ROWS_ID, EXPECTED_CAD_ORGANISATION_ROWS_ID);
    assert_eq!(PAGES_PER_COLUMN_ID, EXPECTED_PAGES_PER_COLUMN_ID);
    assert_eq!(PAGES_PER_ROW_ID, "viewer-pages-per-row");
    assert_eq!(DEFAULT_PAGES_PER_COLUMN, 10);
    assert_eq!(MIN_PAGES_PER_COLUMN, 1);
    assert_eq!(MAX_PAGES_PER_COLUMN, 100);
    assert_eq!(clamp_pages_per_column(0.), 1);
    assert_eq!(clamp_pages_per_column(500.), 100);
    assert_eq!(clamp_pages_per_column(f64::NAN), 10);

    assert!(!entities.cad_view.read_with(cx, |cad, _| cad.is_active()));
    assert_eq!(
        entities.cad_view.read_with(cx, |cad, _| cad.organisation()),
        CadViewOrganisation::Columns
    );
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.pages_per_column()),
        DEFAULT_PAGES_PER_COLUMN
    );

    let fit_before = entities
        .toolbar
        .read_with(cx, |toolbar, _| toolbar.fit_preset());
    let zoom_before = entities.zoom.read_with(cx, |zoom, _| zoom.zoom());
    let continuous_wheel_before = entities
        .continuous
        .read_with(cx, |control, _| control.wheel_behavior());
    let single_page_wheel_before = entities
        .single_page
        .read_with(cx, |control, _| control.wheel_behavior());

    click_target(cx, CAD_VIEW_PRIMARY_ID);
    assert!(entities.cad_view.read_with(cx, |cad, _| cad.is_active()));
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.primary_activations()),
        1
    );
    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.page_view_mode()),
        PageViewMode::Continuous
    );
    assert!(
        !entities
            .continuous
            .read_with(cx, |control, _| control.is_selected())
    );
    assert!(
        !entities
            .single_page
            .read_with(cx, |control, _| control.is_selected())
    );
    click_target(cx, CAD_VIEW_PRIMARY_ID);
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.primary_activations()),
        1,
        "the selected CAD primary cannot turn itself off"
    );

    click_target(cx, SINGLE_PAGE_PRIMARY_ID);
    assert!(!entities.cad_view.read_with(cx, |cad, _| cad.is_active()));
    let primary = bounds(cx, CAD_VIEW_PRIMARY_ID);
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    assert!(entities.cad_view.read_with(cx, |cad, _| cad.is_active()));
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.primary_activations()),
        2,
        "CAD defines no special double-click command"
    );

    let toolbar_focus = entities
        .toolbar
        .read_with(cx, |toolbar, _| toolbar.focus_handle());
    cx.update(|window, cx| toolbar_focus.focus(window, cx));
    click_target(cx, CAD_VIEW_SETTINGS_ID);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.is_settings_open())
    );
    assert_ne!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&toolbar_focus)
    );

    press_button_key(cx, "right");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        entities.cad_view.read_with(cx, |cad, _| cad.organisation()),
        CadViewOrganisation::Rows
    );
    bounds(cx, PAGES_PER_ROW_ID);
    press_button_key(cx, "left");
    press_button_key(cx, "end");
    press_button_key(cx, "home");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        entities.cad_view.read_with(cx, |cad, _| cad.organisation()),
        CadViewOrganisation::Columns
    );
    click_target(cx, CAD_ORGANISATION_ROWS_ID);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        entities.cad_view.read_with(cx, |cad, _| cad.organisation()),
        CadViewOrganisation::Rows
    );
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.organisation_changes()),
        5
    );

    let count = bounds(cx, PAGES_PER_ROW_ID);
    cx.simulate_click(
        point(count.right() - px(12.), count.center().y),
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.pages_per_column()),
        11
    );
    cx.simulate_click(
        point(count.left() + px(12.), count.center().y),
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.pages_per_column()),
        10
    );
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.page_count_changes()),
        2
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        !entities
            .cad_view
            .read_with(cx, |cad, _| cad.is_settings_open())
    );
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&toolbar_focus)
    );
    cx.simulate_keystrokes("right end");
    assert_eq!(
        entities.cad_view.read_with(cx, |cad, _| cad.organisation()),
        CadViewOrganisation::Rows,
        "keys after Escape must not reach the dismissed CAD Popover"
    );

    entities.toolbar.update(cx, |toolbar, cx| {
        toolbar.set_disabled(true, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(entities.cad_view.read_with(cx, |cad, _| cad.is_disabled()));
    click_target(cx, CAD_VIEW_PRIMARY_ID);
    click_target(cx, CAD_VIEW_SETTINGS_ID);
    let primary = bounds(cx, CAD_VIEW_PRIMARY_ID);
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.primary_activations()),
        2
    );
    assert!(
        !entities
            .cad_view
            .read_with(cx, |cad, _| cad.is_settings_open())
    );

    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.fit_preset()),
        fit_before
    );
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        zoom_before
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        continuous_wheel_before
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.wheel_behavior()),
        single_page_wheel_before
    );

    cx.update(|window, cx| {
        entities.cad_view.update(cx, |cad, cx| {
            cad.reset_for_document(window, cx);
        });
        window.draw(cx).clear(cx);
    });
    assert!(!entities.cad_view.read_with(cx, |cad, _| cad.is_active()));
    assert_eq!(
        entities.cad_view.read_with(cx, |cad, _| cad.organisation()),
        CadViewOrganisation::Columns
    );
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.pages_per_column()),
        DEFAULT_PAGES_PER_COLUMN
    );
}

#[gpui::test]
fn cad_toolbar_preserves_non_wrapping_targets_at_fixed_widths(cx: &mut TestAppContext) {
    let (cx, entities) = open_cad_harness(cx);

    let normal_toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
    assert_eq!(normal_toolbar.size.width, px(NORMAL_WIDTH));
    let normal_content = bounds(cx, VIEWER_TOOLBAR_CONTENT_ID);
    let normal_targets = cad_interactive_bounds(cx);
    for target in normal_targets {
        assert_fully_inside(normal_toolbar, target);
        assert_fully_inside(normal_content, target);
    }
    assert_cad_target_order(&normal_targets);

    set_width(cx, &entities.harness, MINIMUM_CENTER_STRIP_WIDTH);
    let minimum_toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
    let minimum_content = bounds(cx, VIEWER_TOOLBAR_CONTENT_ID);
    let required_width: f32 = minimum_content.size.width.into();
    assert_eq!(required_width, CAD_TOOLBAR_REQUIRED_WIDTH);
    let minimum_targets = cad_interactive_bounds(cx);
    assert_target_geometry_is_preserved(&normal_targets, &minimum_targets);
    for target in minimum_targets {
        assert_fully_inside(minimum_content, target);
    }
    assert_cad_target_order(&minimum_targets);
    assert!(minimum_targets[10].right() > minimum_toolbar.right());

    set_width(cx, &entities.harness, CONSTRAINED_OVERFLOW_WIDTH);
    let constrained_toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
    let constrained_content = bounds(cx, VIEWER_TOOLBAR_CONTENT_ID);
    assert_eq!(
        constrained_content.size.width,
        px(CAD_TOOLBAR_REQUIRED_WIDTH)
    );
    let constrained_targets = cad_interactive_bounds(cx);
    assert_target_geometry_is_preserved(&normal_targets, &constrained_targets);
    for target in constrained_targets {
        assert_fully_inside(constrained_content, target);
    }
    assert_cad_target_order(&constrained_targets);
    assert!(constrained_targets[10].right() > constrained_toolbar.right());

    cx.simulate_event(ScrollWheelEvent {
        position: constrained_toolbar.center(),
        delta: ScrollDelta::Pixels(point(px(-700.), px(0.))),
        ..Default::default()
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let scrolled_toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
    let scrolled_targets = cad_interactive_bounds(cx);
    assert_target_geometry_is_preserved(&normal_targets, &scrolled_targets);
    assert_fully_inside(scrolled_toolbar, scrolled_targets[9]);
    assert_fully_inside(scrolled_toolbar, scrolled_targets[10]);
}

#[gpui::test]
fn toolbar_composition_preserves_targets_across_representative_widths(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);

    assert_eq!(VIEWER_TOOLBAR_ID, "viewer-toolbar");
    assert_eq!(VIEWER_TOOLBAR_SCROLL_ID, "viewer-toolbar-scroll");
    assert_eq!(VIEWER_TOOLBAR_CONTENT_ID, "viewer-toolbar-content");
    assert_eq!(FIT_BUTTON_GROUP_ID, "viewer-fit-controls");
    assert_eq!(FIT_WIDTH_ID, "viewer-fit-width");
    assert_eq!(FIT_PAGE_ID, "viewer-fit-page");
    assert_eq!(ZOOM_GROUP_ID, "viewer-zoom-controls");
    assert_eq!(ZOOM_OUT_ID, EXPECTED_ZOOM_OUT_ID);
    assert_eq!(ZOOM_IN_ID, EXPECTED_ZOOM_IN_ID);
    assert_eq!(ZOOM_MENU_ID, EXPECTED_ZOOM_MENU_ID);
    assert_eq!(CONTINUOUS_SPLIT_ID, "continuous-view-split");
    assert_eq!(CONTINUOUS_PRIMARY_ID, "continuous-view-primary");
    assert_eq!(SINGLE_PAGE_SPLIT_ID, "single-page-view-split");
    assert_eq!(SINGLE_PAGE_PRIMARY_ID, "single-page-view-primary");

    let normal_toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
    assert_eq!(normal_toolbar.size.width, px(NORMAL_WIDTH));
    let normal_targets = interactive_bounds(cx);
    for target in normal_targets {
        assert_fully_inside(normal_toolbar, target);
    }
    assert!(normal_targets[0].right() <= normal_targets[1].left());
    assert!(normal_targets[1].right() <= normal_targets[2].left());
    assert!(normal_targets[2].right() <= normal_targets[3].left());
    assert!(normal_targets[3].right() <= normal_targets[4].left());
    assert!(normal_targets[4].right() <= normal_targets[6].left());
    assert_fully_inside(normal_targets[6], normal_targets[5]);
    assert!(normal_targets[6].right() <= normal_targets[8].left());
    assert_fully_inside(normal_targets[8], normal_targets[7]);

    set_width(cx, &entities.harness, MINIMUM_CENTER_STRIP_WIDTH);
    let minimum_toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
    assert_eq!(minimum_toolbar.size.width, px(MINIMUM_CENTER_STRIP_WIDTH));
    let minimum_content = bounds(cx, VIEWER_TOOLBAR_CONTENT_ID);
    let required_width: f32 = minimum_content.size.width.into();
    assert_eq!(required_width, EXPANDED_TOOLBAR_REQUIRED_WIDTH);
    let minimum_targets = interactive_bounds(cx);
    assert_target_geometry_is_preserved(&normal_targets, &minimum_targets);
    assert!(minimum_targets[0].right() <= minimum_targets[1].left());
    assert!(minimum_targets[1].right() <= minimum_targets[2].left());
    assert!(minimum_targets[2].right() <= minimum_targets[3].left());
    assert!(minimum_targets[3].right() <= minimum_targets[4].left());
    assert!(minimum_targets[4].right() <= minimum_targets[6].left());
    assert!(minimum_targets[6].right() <= minimum_targets[8].left());
    assert_fully_inside(minimum_targets[6], minimum_targets[5]);
    assert_fully_inside(minimum_targets[8], minimum_targets[7]);
    assert!(minimum_targets[8].right() > minimum_toolbar.right());

    set_width(cx, &entities.harness, CONSTRAINED_OVERFLOW_WIDTH);
    let constrained_toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
    assert_eq!(
        constrained_toolbar.size.width,
        px(CONSTRAINED_OVERFLOW_WIDTH)
    );
    let constrained_targets = interactive_bounds(cx);
    assert_target_geometry_is_preserved(&normal_targets, &constrained_targets);
    assert!(constrained_targets[0].right() <= constrained_targets[1].left());
    assert!(constrained_targets[1].right() <= constrained_targets[2].left());
    assert!(constrained_targets[2].right() <= constrained_targets[3].left());
    assert!(constrained_targets[3].right() <= constrained_targets[4].left());
    assert!(constrained_targets[4].right() <= constrained_targets[6].left());
    assert!(constrained_targets[6].right() <= constrained_targets[8].left());
    assert_fully_inside(constrained_targets[6], constrained_targets[5]);
    assert_fully_inside(constrained_targets[8], constrained_targets[7]);
    assert!(
        constrained_targets[8].right() > constrained_toolbar.right(),
        "the constrained fixture must exercise horizontal overflow"
    );

    cx.simulate_event(ScrollWheelEvent {
        position: constrained_toolbar.center(),
        delta: ScrollDelta::Pixels(point(px(-500.), px(0.))),
        ..Default::default()
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let scrolled_toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
    let scrolled_targets = interactive_bounds(cx);
    assert_target_geometry_is_preserved(&normal_targets, &scrolled_targets);
    assert_fully_inside(scrolled_toolbar, scrolled_targets[8]);
    assert_fully_inside(scrolled_targets[8], scrolled_targets[7]);
    assert!(scrolled_targets[6].right() <= scrolled_targets[8].left());
}

#[gpui::test]
fn zoom_group_traces_steps_presets_bounds_keyboard_escape_and_disabled(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);

    assert_eq!(MIN_VIEWER_ZOOM, 0.0625);
    assert_eq!(MAX_VIEWER_ZOOM, 64.);
    assert_eq!(ZOOM_PRESETS.len(), 14);
    assert_eq!(format_zoom_percent(MIN_VIEWER_ZOOM), "6.25%");
    assert_eq!(format_zoom_percent(DEFAULT_VIEWER_ZOOM), "100%");
    assert_eq!(format_zoom_percent(1.125), "113%");
    assert_eq!(clamp_viewer_zoom(f64::INFINITY), MIN_VIEWER_ZOOM);
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        DEFAULT_VIEWER_ZOOM
    );
    assert_eq!(
        entities
            .zoom
            .read_with(cx, |zoom, _| zoom.selected_preset()),
        Some(DEFAULT_VIEWER_ZOOM)
    );

    let initial_fit = entities
        .toolbar
        .read_with(cx, |toolbar, _| toolbar.fit_preset());
    let initial_page_mode = entities
        .toolbar
        .read_with(cx, |toolbar, _| toolbar.page_view_mode());
    let initial_continuous_wheel = entities
        .continuous
        .read_with(cx, |control, _| control.wheel_behavior());
    let initial_single_page_wheel = entities
        .single_page
        .read_with(cx, |control, _| control.wheel_behavior());

    click_target(cx, ZOOM_OUT_ID);
    assert_eq!(entities.zoom.read_with(cx, |zoom, _| zoom.zoom()), 0.909);
    click_target(cx, ZOOM_IN_ID);
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        DEFAULT_VIEWER_ZOOM
    );

    let zoom_focus = entities.zoom.read_with(cx, |zoom, _| zoom.focus_handle());
    cx.update(|window, cx| {
        zoom_focus.focus(window, cx);
        window.focus_next(cx);
        assert!(window.focused(cx).is_some());
        window.draw(cx).clear(cx);
    });
    press_button_key(cx, "enter");
    cx.run_until_parked();
    assert_eq!(entities.zoom.read_with(cx, |zoom, _| zoom.zoom()), 0.909);
    cx.update(|window, cx| {
        window.focus_next(cx);
        assert!(window.focused(cx).is_some());
        window.draw(cx).clear(cx);
    });
    press_button_key(cx, "space");
    cx.run_until_parked();
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        DEFAULT_VIEWER_ZOOM
    );

    let toolbar_focus = entities
        .toolbar
        .read_with(cx, |toolbar, _| toolbar.focus_handle());
    cx.update(|window, cx| toolbar_focus.focus(window, cx));
    click_target(cx, ZOOM_MENU_ID);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(entities.zoom.read_with(cx, |zoom, _| zoom.is_menu_open()));
    assert_ne!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&toolbar_focus)
    );
    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(entities.zoom.read_with(cx, |zoom, _| zoom.zoom()), 0.1);
    assert_eq!(
        entities
            .zoom
            .read_with(cx, |zoom, _| zoom.selected_preset()),
        Some(0.1)
    );
    assert_eq!(
        entities
            .zoom
            .read_with(cx, |zoom, _| zoom.displayed_percentage()),
        "10%"
    );

    entities
        .zoom
        .update(cx, |zoom, cx| zoom.set_zoom(1.234, cx));
    assert_eq!(
        entities
            .zoom
            .read_with(cx, |zoom, _| zoom.selected_preset()),
        None
    );
    assert_eq!(
        entities
            .zoom
            .read_with(cx, |zoom, _| zoom.displayed_percentage()),
        "123%"
    );
    entities.zoom.update(cx, |zoom, cx| zoom.set_zoom(1.25, cx));
    assert_eq!(
        entities
            .zoom
            .read_with(cx, |zoom, _| zoom.selected_preset()),
        Some(1.25)
    );

    cx.update(|window, cx| toolbar_focus.focus(window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click_target(cx, ZOOM_MENU_ID);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!entities.zoom.read_with(cx, |zoom, _| zoom.is_menu_open()));
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&toolbar_focus)
    );

    entities.zoom.update(cx, |zoom, cx| zoom.set_zoom(2., cx));
    click_target(cx, ZOOM_MENU_ID);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click_target(cx, ZOOM_MENU_ID);
    let menu = bounds(cx, ZOOM_MENU_ID);
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: menu.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: menu.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        DEFAULT_VIEWER_ZOOM
    );
    assert_eq!(
        entities
            .zoom
            .read_with(cx, |zoom, _| zoom.reset_activations()),
        1
    );
    assert!(!entities.zoom.read_with(cx, |zoom, _| zoom.is_menu_open()));

    entities
        .zoom
        .update(cx, |zoom, cx| zoom.set_zoom(MIN_VIEWER_ZOOM, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click_target(cx, ZOOM_OUT_ID);
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        MIN_VIEWER_ZOOM
    );
    entities
        .zoom
        .update(cx, |zoom, cx| zoom.set_zoom(MAX_VIEWER_ZOOM, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click_target(cx, ZOOM_IN_ID);
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        MAX_VIEWER_ZOOM
    );

    entities.toolbar.update(cx, |toolbar, cx| {
        toolbar.set_disabled(true, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(entities.zoom.read_with(cx, |zoom, _| zoom.is_disabled()));
    click_target(cx, ZOOM_OUT_ID);
    click_target(cx, ZOOM_IN_ID);
    click_target(cx, ZOOM_MENU_ID);
    let menu = bounds(cx, ZOOM_MENU_ID);
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: menu.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: menu.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    assert_eq!(
        entities.zoom.read_with(cx, |zoom, _| zoom.zoom()),
        MAX_VIEWER_ZOOM
    );
    assert!(!entities.zoom.read_with(cx, |zoom, _| zoom.is_menu_open()));
    assert_eq!(
        entities
            .zoom
            .read_with(cx, |zoom, _| zoom.reset_activations()),
        1,
        "disabled state must suppress the percentage-trigger double click"
    );

    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.fit_preset()),
        initial_fit
    );
    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.page_view_mode()),
        initial_page_mode
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        initial_continuous_wheel
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.wheel_behavior()),
        initial_single_page_wheel
    );
}

#[gpui::test]
fn toolbar_traces_pointer_menu_escape_disabled_and_double_click(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);

    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.page_view_mode()),
        PageViewMode::Continuous
    );
    assert!(
        entities
            .continuous
            .read_with(cx, |control, _| control.is_selected())
    );
    assert!(
        !entities
            .single_page
            .read_with(cx, |control, _| control.is_selected())
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Scroll
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Zoom
    );

    let fit_page = bounds(cx, FIT_PAGE_ID);
    cx.simulate_click(fit_page.center(), Modifiers::default());
    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.fit_preset()),
        FitPreset::Page
    );
    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.fit_changes()),
        1
    );

    let primary = bounds(cx, CONTINUOUS_PRIMARY_ID);
    cx.simulate_click(primary.center(), Modifiers::default());
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.primary_activations()),
        1
    );

    let toolbar_focus = entities
        .toolbar
        .read_with(cx, |toolbar, _| toolbar.focus_handle());
    cx.update(|window, cx| toolbar_focus.focus(window, cx));
    let split = bounds(cx, CONTINUOUS_SPLIT_ID);
    let caret = point(split.right() - px(10.), split.center().y);
    cx.simulate_click(caret, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let menu_focus = cx.update(|window, cx| window.focused(cx));
    assert!(menu_focus.is_some());
    assert_ne!(menu_focus.as_ref(), Some(&toolbar_focus));
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Zoom
    );

    cx.update(|window, cx| toolbar_focus.focus(window, cx));
    let split = bounds(cx, CONTINUOUS_SPLIT_ID);
    cx.simulate_click(
        point(split.right() - px(10.), split.center().y),
        Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_ne!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&toolbar_focus)
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&toolbar_focus)
    );
    cx.simulate_keystrokes("down down enter");
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Zoom,
        "keyboard input after Escape must not reach the dismissed menu"
    );

    let single_page_primary = bounds(cx, SINGLE_PAGE_PRIMARY_ID);
    cx.simulate_click(single_page_primary.center(), Modifiers::default());
    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.page_view_mode()),
        PageViewMode::SinglePage
    );
    assert!(
        !entities
            .continuous
            .read_with(cx, |control, _| control.is_selected())
    );
    assert!(
        entities
            .single_page
            .read_with(cx, |control, _| control.is_selected())
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.primary_activations()),
        1
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Zoom,
        "Single Page activation must not change Continuous wheel behavior"
    );

    cx.update(|window, cx| toolbar_focus.focus(window, cx));
    let single_page_split = bounds(cx, SINGLE_PAGE_SPLIT_ID);
    cx.simulate_click(
        point(
            single_page_split.right() - px(10.),
            single_page_split.center().y,
        ),
        Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_ne!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&toolbar_focus)
    );
    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Scroll
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Zoom,
        "the two page-view menus must retain independent state"
    );

    cx.update(|window, cx| toolbar_focus.focus(window, cx));
    let single_page_split = bounds(cx, SINGLE_PAGE_SPLIT_ID);
    cx.simulate_click(
        point(
            single_page_split.right() - px(10.),
            single_page_split.center().y,
        ),
        Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&toolbar_focus)
    );
    cx.simulate_keystrokes("down enter");
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Scroll,
        "keyboard input after Escape must not reach the Single Page menu"
    );

    let single_page_primary = bounds(cx, SINGLE_PAGE_PRIMARY_ID);
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: single_page_primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: single_page_primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.primary_activations()),
        1,
        "the double click is Fit page, not a second view-mode activation"
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.fit_page_activations()),
        1
    );

    let primary = bounds(cx, CONTINUOUS_PRIMARY_ID);
    cx.simulate_click(primary.center(), Modifiers::default());
    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.page_view_mode()),
        PageViewMode::Continuous
    );
    assert!(
        entities
            .continuous
            .read_with(cx, |control, _| control.is_selected())
    );
    assert!(
        !entities
            .single_page
            .read_with(cx, |control, _| control.is_selected())
    );

    let primary = bounds(cx, CONTINUOUS_PRIMARY_ID);
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.primary_activations()),
        2,
        "the double click is Fit width, not a second view-mode activation"
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.fit_width_activations()),
        1
    );

    entities.toolbar.update(cx, |toolbar, cx| {
        toolbar.set_disabled(true, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.is_disabled())
    );
    assert!(
        entities
            .continuous
            .read_with(cx, |control, _| control.is_disabled())
    );
    assert!(
        entities
            .single_page
            .read_with(cx, |control, _| control.is_disabled())
    );
    assert!(
        !entities
            .continuous
            .read_with(cx, |control, _| control.is_selected())
    );
    assert!(
        !entities
            .single_page
            .read_with(cx, |control, _| control.is_selected())
    );

    let fit_width = bounds(cx, FIT_WIDTH_ID);
    let primary = bounds(cx, CONTINUOUS_PRIMARY_ID);
    let split = bounds(cx, CONTINUOUS_SPLIT_ID);
    let single_page_primary = bounds(cx, SINGLE_PAGE_PRIMARY_ID);
    let single_page_split = bounds(cx, SINGLE_PAGE_SPLIT_ID);
    cx.simulate_click(fit_width.center(), Modifiers::default());
    cx.simulate_click(primary.center(), Modifiers::default());
    cx.simulate_click(single_page_primary.center(), Modifiers::default());
    cx.simulate_click(
        point(split.right() - px(10.), split.center().y),
        Modifiers::default(),
    );
    cx.simulate_click(
        point(
            single_page_split.right() - px(10.),
            single_page_split.center().y,
        ),
        Modifiers::default(),
    );
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.fit_preset()),
        FitPreset::Width,
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let single_page_primary = cx.debug_bounds(SINGLE_PAGE_PRIMARY_ID).unwrap();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: single_page_primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: single_page_primary.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.fit_preset()),
        FitPreset::Width,
        "a disabled Single Page control must not change the retained fit preset"
    );
    assert_eq!(
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.fit_changes()),
        3
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.primary_activations()),
        2
    );
    assert_eq!(
        entities
            .continuous
            .read_with(cx, |control, _| control.fit_width_activations()),
        1,
        "a disabled Continuous control must suppress double-click Fit width"
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.primary_activations()),
        1
    );
    assert_eq!(
        entities
            .single_page
            .read_with(cx, |control, _| control.fit_page_activations()),
        1,
        "a disabled Single Page control must suppress double-click Fit page"
    );
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)).as_ref(),
        Some(&toolbar_focus),
        "the disabled caret must not open or take focus"
    );
}
