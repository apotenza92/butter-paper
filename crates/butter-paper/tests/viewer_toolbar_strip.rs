use std::{cell::RefCell, rc::Rc};

use butter_paper::{
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
        EDIT_PDF_ID, FIT_BUTTON_GROUP_ID, FIT_PAGE_ID, FIT_WIDTH_ID, FitPreset, ScaleToolbarState,
        VIEWER_SCALE_ID, VIEWER_TOOLBAR_CONTENT_ID, VIEWER_TOOLBAR_CONTROLS_ID, VIEWER_TOOLBAR_ID,
        VIEWER_TOOLBAR_SCROLL_ID, ViewerToolbarFit, ViewerToolbarStrip, viewer_toolbar_rems,
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
    Styled as _, TestAppContext, Window, point, px,
};
use gpui_component::Root;

// The toolbar keeps the GPUI Component controls at their intrinsic size. It
// fills ordinary widths without distributing the groups across the strip and
// scrolls only once that intrinsic content no longer fits.
const NORMAL_WIDTH: f32 = 720.;
/// The centre controls' strip, with Undo and Redo at the leading edge (72
/// px with the edge inset and gap) and the page field's room for
/// four-digit page counts.
const MINIMUM_CENTER_STRIP_WIDTH: f32 = 480.
    + 72.
    + 16.
        * butter_paper::page_navigator::indicator_room_rems(
            butter_paper::page_navigator::PAGE_NAVIGATOR_MAX_DIGITS,
        );
const CONSTRAINED_OVERFLOW_WIDTH: f32 = 320.;
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
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let entities = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let entities = entities.clone();
        move |window, cx| {
            let continuous = cx.new(|_| ContinuousViewControl::new());
            let single_page = cx.new(|_| PageViewControl::single_page());
            let zoom = cx.new(|_| ZoomControl::new());
            let toolbar = cx.new(|cx| {
                let mut toolbar = ViewerToolbarStrip::new_with_zoom(
                    continuous.clone(),
                    single_page.clone(),
                    zoom.clone(),
                    cx,
                );
                // The widest page field a document is budgeted for.
                let navigator = cx.new(|_| butter_paper::page_navigator::PageNavigator::new());
                toolbar.set_page_navigator(navigator, cx);
                toolbar.sync_pages(0, 9_999, cx);
                toolbar
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
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
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

#[gpui::test]
fn cad_settings_reopens_with_the_active_document_count(cx: &mut TestAppContext) {
    let (cx, entities) = open_cad_harness(cx);
    // The input already exists with 10. Switching documents restores only the
    // retained model; opening settings must refresh the input before editing.
    entities.cad_view.update(cx, |cad, cx| {
        cad.sync_retained_state(false, CadViewOrganisation::Columns, 42, cx);
    });
    click_target(cx, CAD_VIEW_SETTINGS_ID);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let count = bounds(cx, PAGES_PER_COLUMN_ID);
    cx.simulate_click(
        point(count.right() - px(12.), count.center().y),
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        entities
            .cad_view
            .read_with(cx, |cad, _| cad.pages_per_column()),
        43
    );
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

#[gpui::test]
fn fit_control_hover_and_dismissal_do_not_shift_toolbar_layout(cx: &mut TestAppContext) {
    let (cx, _) = open_harness(cx);
    let target_before = bounds(cx, FIT_WIDTH_ID);
    cx.simulate_mouse_move(point(px(1.), px(200.)), None, Modifiers::default());
    cx.simulate_mouse_move(target_before.center(), None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(bounds(cx, FIT_WIDTH_ID), target_before);

    cx.simulate_mouse_move(point(px(1.), px(200.)), None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(bounds(cx, FIT_WIDTH_ID), target_before);
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
        "Organise drawing sheets. Mouse wheel zooms; trackpad scrolls."
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
    // The controls fit the minimum strip at their intrinsic width, so it
    // does not scroll.
    assert_eq!(minimum_content.size.width, minimum_toolbar.size.width);
    assert!(bounds(cx, VIEWER_TOOLBAR_CONTROLS_ID).size.width <= minimum_toolbar.size.width);
    let minimum_targets = cad_interactive_bounds(cx);
    assert_target_geometry_is_preserved(&normal_targets, &minimum_targets);
    for target in minimum_targets {
        assert_fully_inside(minimum_content, target);
    }
    assert_cad_target_order(&minimum_targets);
    for target in minimum_targets {
        assert_fully_inside(minimum_toolbar, target);
    }
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
    assert_eq!(normal_toolbar.size.height, px(48.));
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
    // The controls fit a 480 px strip at their intrinsic width, so it does
    // not scroll.
    assert_eq!(minimum_content.size.width, minimum_toolbar.size.width);
    assert!(bounds(cx, VIEWER_TOOLBAR_CONTROLS_ID).size.width <= minimum_toolbar.size.width);
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
    for target in minimum_targets {
        assert_fully_inside(minimum_toolbar, target);
    }

    // Narrow, the toolbar never scrolls: its controls step into tighter
    // forms instead. Zoom Out and In and the fit presets move into the zoom
    // menu, and the two page views merge into one menu button.
    set_width(cx, &entities.harness, CONSTRAINED_OVERFLOW_WIDTH);
    let constrained_toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
    assert_eq!(
        constrained_toolbar.size.width,
        px(CONSTRAINED_OVERFLOW_WIDTH)
    );
    let constrained_content = bounds(cx, VIEWER_TOOLBAR_CONTENT_ID);
    assert_eq!(
        constrained_content.size.width,
        constrained_toolbar.size.width
    );
    for id in [
        ZOOM_OUT_ID,
        ZOOM_IN_ID,
        FIT_WIDTH_ID,
        FIT_PAGE_ID,
        CONTINUOUS_SPLIT_ID,
        SINGLE_PAGE_SPLIT_ID,
    ] {
        assert!(cx.debug_bounds(id).is_none(), "{id} at 320px");
    }
    for id in [
        ZOOM_MENU_ID,
        butter_paper::viewer_toolbar_strip::VIEWER_PAGE_VIEW_MENU_ID,
    ] {
        assert_fully_inside(constrained_toolbar, bounds(cx, id));
    }
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
    assert_eq!(entities.zoom.read_with(cx, |zoom, _| zoom.zoom()), 0.9);
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
    assert_eq!(entities.zoom.read_with(cx, |zoom, _| zoom.zoom()), 0.9);
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
    cx.simulate_keystrokes("down down enter");
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
    cx.simulate_keystrokes("down down down enter");
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

/// The form a laid-out toolbar shows, read from the controls it shows.
fn form_shown(cx: &mut gpui::VisualTestContext) -> ViewerToolbarFit {
    use butter_paper::viewer_toolbar_strip::{VIEWER_PAGE_VIEW_MENU_ID, VIEWER_SCALE_VALUE_ID};
    let shown = |cx: &mut gpui::VisualTestContext, id: &'static str| cx.debug_bounds(id).is_some();
    if shown(cx, butter_paper::page_navigator::PAGE_FIRST_ID) {
        ViewerToolbarFit::Full
    } else if shown(cx, ZOOM_OUT_ID) {
        ViewerToolbarFit::Middle
    } else if shown(cx, FIT_WIDTH_ID) {
        ViewerToolbarFit::StepsInMenu
    } else if !shown(cx, VIEWER_PAGE_VIEW_MENU_ID) {
        ViewerToolbarFit::FitInMenu
    } else if shown(cx, VIEWER_SCALE_VALUE_ID) {
        ViewerToolbarFit::ViewMenu
    } else if bounds(cx, ZOOM_MENU_ID).size.width >= px(80.) {
        ViewerToolbarFit::Compact
    } else {
        ViewerToolbarFit::Narrowest
    }
}

/// A harness toolbar for a four-digit page count with Edit PDF and the
/// longest scale, signed or not.
fn worst_case_toolbar(
    cx: &mut TestAppContext,
    signed: bool,
) -> (&mut gpui::VisualTestContext, HarnessEntities) {
    let (cx, entities) = open_harness(cx);
    let navigator = cx.new(|_| butter_paper::page_navigator::PageNavigator::new());
    entities.toolbar.update(cx, |toolbar, cx| {
        toolbar.set_page_navigator(navigator, cx);
        toolbar.sync_pages(0, 9_999, cx);
        toolbar.set_edit_pdf(Some(false), cx);
        toolbar.set_scale(
            Some(ScaleToolbarState {
                page_number: 9_999,
                on: true,
                label: Some("1/32\" = 1'-0\"".into()),
                menu_open: false,
            }),
            cx,
        );
        if signed {
            toolbar.set_signatures(
                Some(butter_paper::pdf_signatures::SignatureStatus::Checking),
                cx,
            );
        }
    });
    (cx, entities)
}

fn lay_out_at(cx: &mut gpui::VisualTestContext, entities: &HarnessEntities, width: f32) {
    set_width(cx, &entities.harness, width);
    // The form follows the width laid out last frame.
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn every_toolbar_group_shows_whole_at_every_width_from_the_narrowest(cx: &mut TestAppContext) {
    use butter_paper::viewer_toolbar_strip::{
        VIEWER_HISTORY_GROUP_ID, VIEWER_PAGE_LAYOUT_ID, VIEWER_PAGES_GROUP_ID,
        VIEWER_SCALE_MENU_ID, VIEWER_SCALE_TOGGLE_ID, VIEWER_SIGNATURES_ID,
    };
    for signed in [false, true] {
        let mut app = cx.clone();
        let (cx, entities) = worst_case_toolbar(&mut app, signed);
        let narrowest = 16. * viewer_toolbar_rems(ViewerToolbarFit::Narrowest, signed, 4);
        let mut width = narrowest;
        while width <= 1400. {
            lay_out_at(cx, &entities, width);
            let toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
            let content = bounds(cx, VIEWER_TOOLBAR_CONTENT_ID);
            assert_eq!(
                content.size.width, toolbar.size.width,
                "the toolbar scrolls at {width} (signed {signed})"
            );
            // Every group, whole, in order, inside the toolbar.
            let mut groups = vec![
                bounds(cx, VIEWER_HISTORY_GROUP_ID),
                bounds(cx, VIEWER_PAGES_GROUP_ID),
                bounds(cx, ZOOM_MENU_ID),
                bounds(cx, VIEWER_PAGE_LAYOUT_ID),
                bounds(cx, EDIT_PDF_ID),
                bounds(cx, VIEWER_SCALE_ID),
            ];
            if signed {
                groups.push(bounds(cx, VIEWER_SIGNATURES_ID));
            }
            for pair in groups.windows(2) {
                assert!(
                    pair[0].right() <= pair[1].left(),
                    "overlap at {width} (signed {signed}): {groups:?}"
                );
            }
            for group in &groups {
                assert!(
                    group.left() >= toolbar.left() && group.right() <= toolbar.right(),
                    "{group:?} outside {toolbar:?} at {width}"
                );
            }
            // The Scale always shows its ruler and its caret.
            for id in [VIEWER_SCALE_TOGGLE_ID, VIEWER_SCALE_MENU_ID] {
                assert!(cx.debug_bounds(id).is_some(), "{id} at {width}");
            }
            width += 8.;
        }
    }
}

#[gpui::test]
fn each_toolbar_form_starts_at_its_planned_width(cx: &mut TestAppContext) {
    for signed in [false, true] {
        let mut app = cx.clone();
        let (cx, entities) = worst_case_toolbar(&mut app, signed);
        for (index, fit) in ViewerToolbarFit::ALL.into_iter().enumerate() {
            let at = 16. * viewer_toolbar_rems(fit, signed, 4);
            lay_out_at(cx, &entities, at);
            assert_eq!(form_shown(cx), fit, "at {at} (signed {signed})");
            let toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
            assert_eq!(
                bounds(cx, VIEWER_TOOLBAR_CONTENT_ID).size.width,
                toolbar.size.width,
                "{fit:?} does not fit its own width {at} (signed {signed})"
            );
            if let Some(next) = ViewerToolbarFit::ALL.get(index + 1) {
                lay_out_at(cx, &entities, at - 1.);
                assert_eq!(form_shown(cx), *next, "at {} (signed {signed})", at - 1.);
            }
        }
    }
}

#[gpui::test]
fn toolbar_centres_controls_when_it_can_and_never_scrolls_down_to_its_minimum(
    cx: &mut TestAppContext,
) {
    use butter_paper::viewer_toolbar_strip::{
        VIEWER_HISTORY_GROUP_ID, VIEWER_PAGE_LAYOUT_ID, VIEWER_PAGES_GROUP_ID,
    };
    let (cx, entities) = worst_case_toolbar(cx, false);
    let narrowest = 16. * viewer_toolbar_rems(ViewerToolbarFit::Narrowest, false, 4);
    let at = |fit| 16. * viewer_toolbar_rems(fit, false, 4);
    let widths =
        ViewerToolbarFit::ALL
            .into_iter()
            .map(at)
            .chain([1400., 1100., 900., narrowest + 40.]);
    for width in widths {
        lay_out_at(cx, &entities, width);
        let toolbar = bounds(cx, VIEWER_TOOLBAR_ID);
        let history = bounds(cx, VIEWER_HISTORY_GROUP_ID);
        let pages_group = bounds(cx, VIEWER_PAGES_GROUP_ID);
        let layout = bounds(cx, VIEWER_PAGE_LAYOUT_ID);
        let edit_pdf = bounds(cx, EDIT_PDF_ID);
        let scale = bounds(cx, VIEWER_SCALE_ID);
        // The group gap and edge inset: 8px, halved in the narrowest form.
        let unit = if form_shown(cx) == ViewerToolbarFit::Narrowest {
            px(4.)
        } else {
            px(8.)
        };
        // Every form pins Undo and Redo left and Edit PDF with the Scale
        // right, and keeps the pages, zoom and layout centred in the
        // toolbar unless a wider side would reach them there; then they
        // meet that side.
        assert_eq!(
            history.left(),
            toolbar.left() + unit,
            "Undo and Redo at the left edge at {width}"
        );
        assert_eq!(
            scale.right(),
            toolbar.right() - unit,
            "Edit PDF and the Scale pinned right at {width}"
        );
        let controls = bounds(cx, VIEWER_TOOLBAR_CONTROLS_ID);
        let centre = toolbar.center().x;
        let half = controls.size.width / 2.;
        if centre - half >= history.right() && centre + half <= edit_pdf.left() {
            assert!(
                (controls.center().x - centre).abs() <= px(1.),
                "the centre controls not centred at {width}: {controls:?} in {toolbar:?}"
            );
        } else {
            assert!(
                (controls.left() - history.right()).abs() <= px(1.)
                    || (edit_pdf.left() - controls.right()).abs() <= px(1.),
                "the centre controls not against the wider side at {width}: {controls:?}"
            );
        }
        // The groups never come nearer than the group gap; at each form's
        // least width they meet at it, give or take the few pixels the
        // planned widths keep in hand.
        let gaps = [
            pages_group.left() - history.right(),
            edit_pdf.left() - layout.right(),
        ];
        assert!(gaps.iter().all(|gap| *gap >= unit), "{gaps:?} at {width}");
        if ViewerToolbarFit::ALL
            .into_iter()
            .map(at)
            .any(|least| least == width)
        {
            assert!(
                gaps[0] + gaps[1] <= unit * 2. + px(16.),
                "the groups do not meet at {width}: {gaps:?}"
            );
        }
    }
}

#[gpui::test]
fn the_zoom_percentage_keeps_one_width_at_every_zoom(cx: &mut TestAppContext) {
    let (cx, entities) = open_harness(cx);
    let mut widths = Vec::new();
    for zoom in [0.0625, 0.25, 1., 1.29, 64.] {
        entities
            .zoom
            .update(cx, |control, cx| control.set_zoom(zoom, cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        widths.push((
            zoom,
            bounds(cx, ZOOM_MENU_ID).size.width,
            bounds(cx, VIEWER_TOOLBAR_CONTROLS_ID).left(),
        ));
    }
    let (_, width, left) = widths[0];
    for (zoom, w, l) in &widths {
        assert_eq!(*w, width, "the percentage resized at {zoom}: {widths:?}");
        assert_eq!(*l, left, "the toolbar shifted at {zoom}: {widths:?}");
    }
}

/// The stock buttons draw their focus ring 3px outside their border; the
/// toolbar leaves that room round each of Undo, Redo, Edit PDF and the
/// Scale's halves in the widest and narrowest forms, inside the toolbar,
/// which clips.
#[gpui::test]
fn toolbar_icon_buttons_keep_room_for_their_focus_ring(cx: &mut TestAppContext) {
    use butter_paper::viewer_toolbar_strip::{
        VIEWER_REDO_ID, VIEWER_SCALE_MENU_ID, VIEWER_SCALE_TOGGLE_ID, VIEWER_UNDO_ID,
    };
    for fit in [ViewerToolbarFit::Full, ViewerToolbarFit::Narrowest] {
        let mut app = cx.clone();
        let (cx, entities) = worst_case_toolbar(&mut app, true);
        lay_out_at(cx, &entities, 16. * viewer_toolbar_rems(fit, true, 4));
        let toolbar = bounds(cx, VIEWER_TOOLBAR_SCROLL_ID);
        for id in [
            VIEWER_UNDO_ID,
            VIEWER_REDO_ID,
            EDIT_PDF_ID,
            VIEWER_SCALE_TOGGLE_ID,
            VIEWER_SCALE_MENU_ID,
        ] {
            let ring = bounds(cx, id).dilate(px(3.));
            assert!(
                ring.left() >= toolbar.left()
                    && ring.right() <= toolbar.right()
                    && ring.top() >= toolbar.top()
                    && ring.bottom() <= toolbar.bottom(),
                "{fit:?}: the ring round {id} {ring:?} is cut by {toolbar:?}"
            );
        }
    }
}

/// Undo and Redo send the document's own actions when enabled; disabled,
/// they send nothing. The action goes from the toolbar, so the document
/// behind it takes it, never a focused text field.
#[gpui::test]
fn undo_and_redo_send_the_document_actions_when_enabled(cx: &mut TestAppContext) {
    use butter_paper::viewer_toolbar_strip::{
        HistoryState, RedoDocument, UndoDocument, VIEWER_REDO_ID, VIEWER_UNDO_ID,
    };
    let (cx, entities) = open_harness(cx);
    let sent = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    cx.update(|_, cx| {
        let undo = sent.clone();
        cx.on_action(move |_: &UndoDocument, _| undo.borrow_mut().push("undo"));
        let redo = sent.clone();
        cx.on_action(move |_: &RedoDocument, _| redo.borrow_mut().push("redo"));
    });
    let history = |cx: &mut gpui::VisualTestContext| {
        entities
            .toolbar
            .read_with(cx, |toolbar, _| toolbar.history())
    };
    assert_eq!(history(cx), HistoryState::default(), "both start disabled");
    // Disabled, a press sends nothing.
    click_target(cx, VIEWER_UNDO_ID);
    click_target(cx, VIEWER_REDO_ID);
    cx.run_until_parked();
    assert!(sent.borrow().is_empty(), "{:?}", sent.borrow());
    // Icon buttons, side by side at the leading edge.
    let undo = bounds(cx, VIEWER_UNDO_ID);
    let redo = bounds(cx, VIEWER_REDO_ID);
    assert_eq!(undo.size, gpui::size(px(32.), px(32.)));
    assert_eq!(redo.size, undo.size);
    assert!(undo.right() <= redo.left());
    assert_eq!(undo.left(), bounds(cx, VIEWER_TOOLBAR_ID).left() + px(8.));

    entities.toolbar.update(cx, |toolbar, cx| {
        toolbar.set_history(
            HistoryState {
                can_undo: true,
                can_redo: false,
            },
            cx,
        );
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click_target(cx, VIEWER_UNDO_ID);
    click_target(cx, VIEWER_REDO_ID);
    cx.run_until_parked();
    assert_eq!(*sent.borrow(), ["undo"]);
    entities.toolbar.update(cx, |toolbar, cx| {
        toolbar.set_history(
            HistoryState {
                can_undo: true,
                can_redo: true,
            },
            cx,
        );
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click_target(cx, VIEWER_REDO_ID);
    cx.run_until_parked();
    assert_eq!(*sent.borrow(), ["undo", "redo"]);
    if let Some(tree) = cx.update(|window, _| window.debug_a11y_tree_json()) {
        assert!(tree.contains("Undo") && tree.contains("Redo"));
    }
}

/// Edit PDF is an icon toggle as Fit Width is: one square icon button that
/// asks for Edit PDF on or off, named with its state.
#[gpui::test]
fn edit_pdf_is_an_icon_toggle_named_with_its_state(cx: &mut TestAppContext) {
    use butter_paper::viewer_toolbar_strip::ViewerToolbarStripEvent;
    let (cx, entities) = worst_case_toolbar(cx, false);
    lay_out_at(cx, &entities, 1400.);
    let events = Rc::new(RefCell::new(Vec::new()));
    cx.update(|_, cx| {
        let events = events.clone();
        cx.subscribe(
            &entities.toolbar,
            move |_, event: &ViewerToolbarStripEvent, _| {
                events.borrow_mut().push(*event);
            },
        )
        .detach();
    });
    for on in [false, true] {
        entities
            .toolbar
            .update(cx, |toolbar, cx| toolbar.set_edit_pdf(Some(on), cx));
        lay_out_at(cx, &entities, 1400.);
        let edit = bounds(cx, EDIT_PDF_ID);
        assert_eq!(edit.size, gpui::size(px(32.), px(32.)), "icon-only, {on}");
        if let Some(tree) = cx.update(|window, _| window.debug_a11y_tree_json()) {
            let name = if on { "Edit PDF, on" } else { "Edit PDF, off" };
            assert!(tree.contains(name), "{name}");
        }
        click_target(cx, EDIT_PDF_ID);
        cx.run_until_parked();
    }
    assert_eq!(
        *events.borrow(),
        [
            ViewerToolbarStripEvent::EditPdfToggled,
            ViewerToolbarStripEvent::EditPdfToggled
        ]
    );
    // Hidden without a document.
    entities
        .toolbar
        .update(cx, |toolbar, cx| toolbar.set_edit_pdf(None, cx));
    lay_out_at(cx, &entities, 1400.);
    assert!(cx.debug_bounds(EDIT_PDF_ID).is_none());
}

/// The Scale is one split button: the ruler half turns the page's scale on
/// and off, the caret asks for the Page Scale panel. The scale shows beside
/// the ruler while the page has one, cut short when long, and not in the
/// two narrowest forms.
#[gpui::test]
fn the_scale_is_one_icon_split_button_with_the_scale_beside_it(cx: &mut TestAppContext) {
    use butter_paper::viewer_toolbar_strip::{
        ScaleRequest, VIEWER_SCALE_MENU_ID, VIEWER_SCALE_TOGGLE_ID, VIEWER_SCALE_VALUE_ID,
    };
    let (cx, entities) = worst_case_toolbar(cx, false);
    let requests = Rc::new(RefCell::new(Vec::new()));
    entities.toolbar.update(cx, |toolbar, _| {
        let requests = requests.clone();
        toolbar.set_scale_handler(Rc::new(move |request, _, _| {
            requests.borrow_mut().push(request);
        }));
    });
    let set = |cx: &mut gpui::VisualTestContext, on: bool, label: Option<&str>| {
        entities.toolbar.update(cx, |toolbar, cx| {
            toolbar.set_scale(
                Some(ScaleToolbarState {
                    page_number: 3,
                    on,
                    label: label.map(Into::into),
                    menu_open: false,
                }),
                cx,
            );
        });
        lay_out_at(cx, &entities, 1400.);
    };
    // Off: the ruler and the caret alone, even with a scale to bring back.
    for label in [None, Some("1:100")] {
        set(cx, false, label);
        assert!(
            cx.debug_bounds(VIEWER_SCALE_VALUE_ID).is_none(),
            "{label:?}"
        );
        assert_eq!(
            bounds(cx, VIEWER_SCALE_TOGGLE_ID).size,
            gpui::size(px(32.), px(32.))
        );
    }
    // On: the scale beside the ruler; a short one takes its own width.
    set(cx, true, Some("1:100"));
    let short = bounds(cx, VIEWER_SCALE_VALUE_ID);
    let toggle = bounds(cx, VIEWER_SCALE_TOGGLE_ID);
    assert!(short.left() > toggle.left() + px(16.) && short.right() <= toggle.right());
    assert!(short.size.width < px(60.), "{short:?}");
    if let Some(tree) = cx.update(|window, _| window.debug_a11y_tree_json()) {
        assert!(tree.contains("Page Scale, on, 1:100"));
    }
    // A long one is cut short at 92px.
    set(cx, true, Some("1/32\" = 1'-0\""));
    assert_eq!(bounds(cx, VIEWER_SCALE_VALUE_ID).size.width, px(92.));
    // The halves are joined, the caret last; both ask for their own.
    let toggle = bounds(cx, VIEWER_SCALE_TOGGLE_ID);
    let caret = bounds(cx, VIEWER_SCALE_MENU_ID);
    assert_eq!(toggle.right(), caret.left());
    assert_eq!(caret.size, gpui::size(px(32.), px(32.)));
    click_target(cx, VIEWER_SCALE_TOGGLE_ID);
    click_target(cx, VIEWER_SCALE_MENU_ID);
    cx.run_until_parked();
    assert_eq!(
        *requests.borrow(),
        [ScaleRequest::Toggle, ScaleRequest::Menu]
    );
    // The two narrowest forms show the ruler alone; the tooltip and status
    // bar name the scale.
    for fit in [ViewerToolbarFit::Compact, ViewerToolbarFit::Narrowest] {
        lay_out_at(cx, &entities, 16. * viewer_toolbar_rems(fit, false, 4));
        assert!(cx.debug_bounds(VIEWER_SCALE_VALUE_ID).is_none(), "{fit:?}");
        assert_eq!(
            bounds(cx, VIEWER_SCALE_TOGGLE_ID).size,
            gpui::size(px(32.), px(32.))
        );
    }
    lay_out_at(
        cx,
        &entities,
        16. * viewer_toolbar_rems(ViewerToolbarFit::ViewMenu, false, 4),
    );
    assert!(cx.debug_bounds(VIEWER_SCALE_VALUE_ID).is_some());
}

/// The page field fits the page count's digits (two at least) inside its
/// padding, with room for its focus ring inside the pages group, which
/// clips: a page typed or selected is never cut. Opening it for typing
/// leaves the toolbar where it was.
#[gpui::test]
fn the_page_field_fits_its_digits_and_focus_ring_for_one_to_four_digits(cx: &mut TestAppContext) {
    use butter_paper::page_navigator::{PAGE_INDICATOR_ID, PAGE_INPUT_FIELD_ID, PAGE_INPUT_ID};
    use butter_paper::viewer_toolbar_strip::VIEWER_PAGES_GROUP_ID;
    for (page_count, fit) in [
        (2, ViewerToolbarFit::Middle),
        (9, ViewerToolbarFit::Full),
        (99, ViewerToolbarFit::Middle),
        (999, ViewerToolbarFit::Middle),
        (999, ViewerToolbarFit::Narrowest),
        (9_999, ViewerToolbarFit::Full),
        (9_999, ViewerToolbarFit::Narrowest),
    ] {
        let mut app = cx.clone();
        let (cx, entities) = worst_case_toolbar(&mut app, false);
        let navigator = cx.new(|_| butter_paper::page_navigator::PageNavigator::new());
        entities.toolbar.update(cx, |toolbar, cx| {
            toolbar.set_page_navigator(navigator.clone(), cx);
            toolbar.sync_pages(page_count - 1, page_count, cx);
        });
        let digits = butter_paper::page_navigator::page_count_digits(page_count);
        lay_out_at(cx, &entities, 16. * viewer_toolbar_rems(fit, false, digits));
        let case = format!("{page_count} pages, {fit:?}");
        let indicator = bounds(cx, PAGE_INDICATOR_ID);
        let controls = bounds(cx, VIEWER_TOOLBAR_CONTROLS_ID);
        click_target(cx, PAGE_INDICATOR_ID);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            navigator.read_with(cx, |navigator, _| navigator.is_editing()),
            "{case}"
        );
        let entry = bounds(cx, PAGE_INPUT_ID);
        let field = bounds(cx, PAGE_INPUT_FIELD_ID);
        let group = bounds(cx, VIEWER_PAGES_GROUP_ID);
        // The same room as the indicator, so nothing moves.
        assert_eq!(entry.size.width, indicator.size.width, "{case}");
        assert_eq!(bounds(cx, VIEWER_TOOLBAR_CONTROLS_ID), controls, "{case}");
        // The focus ring, 3px outside the field, inside the group's frame.
        let ring = field.dilate(px(3.));
        assert!(
            ring.left() > group.left()
                && ring.right() < group.right()
                && ring.top() > group.top()
                && ring.bottom() < group.bottom(),
            "{case}: ring {ring:?} cut by {group:?}"
        );
        // The widest number of that many digits fits inside the field's 8px
        // padding and 1px border, with room for the caret.
        let widest = "8".repeat(digits.max(2) as usize);
        let text = cx.update(|window, _| {
            let font_size = window.rem_size() * 0.875;
            let run = gpui::TextRun {
                len: widest.len(),
                font: window.text_style().font(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(widest.clone().into(), font_size, &[run], None)
                .width
        });
        assert!(
            text + px(2.) <= field.size.width - px(18.),
            "{case}: {widest} is {text:?} wide in a {field:?} field"
        );
    }
}
