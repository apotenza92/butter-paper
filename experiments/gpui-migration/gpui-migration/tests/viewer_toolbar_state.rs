use butter_paper_gpui_migration::{
    cad_view_control::{
        CadViewControl, CadViewControlEvent, CadViewOrganisation, MAX_PAGES_PER_COLUMN,
        MIN_PAGES_PER_COLUMN, clamp_pages_per_column,
    },
    page_view_control::{PageViewControl, PageViewControlEvent, PageViewMode, WheelBehavior},
    viewer_toolbar_strip::{FitPreset, ViewerToolbarStrip},
    zoom_control::{
        DEFAULT_VIEWER_ZOOM, MAX_VIEWER_ZOOM, MIN_VIEWER_ZOOM, ZoomControl, ZoomControlEvent,
        clamp_viewer_zoom, format_zoom_percent,
    },
};
use gpui::{AppContext as _, TestAppContext};
use std::{cell::RefCell, rc::Rc};

#[test]
fn zoom_and_cad_inputs_use_the_same_clamped_values_as_the_controls() {
    assert_eq!(clamp_viewer_zoom(f64::NAN), MIN_VIEWER_ZOOM);
    assert_eq!(clamp_viewer_zoom(f64::INFINITY), MIN_VIEWER_ZOOM);
    assert_eq!(clamp_viewer_zoom(-1.), MIN_VIEWER_ZOOM);
    assert_eq!(clamp_viewer_zoom(100.), MAX_VIEWER_ZOOM);
    assert_eq!(clamp_viewer_zoom(1.23456), 1.235);
    assert_eq!(format_zoom_percent(DEFAULT_VIEWER_ZOOM), "100%");
    assert_eq!(format_zoom_percent(MIN_VIEWER_ZOOM), "6.25%");

    assert_eq!(clamp_pages_per_column(f64::NAN), 10);
    assert_eq!(clamp_pages_per_column(0.), MIN_PAGES_PER_COLUMN);
    assert_eq!(clamp_pages_per_column(1000.), MAX_PAGES_PER_COLUMN);
    assert_eq!(clamp_pages_per_column(4.6), 5);
}

#[gpui::test]
fn zoom_actions_change_state_and_respect_disabled_state(cx: &mut TestAppContext) {
    let zoom = cx.new(|_| ZoomControl::new());
    let events = Rc::new(RefCell::new(Vec::new()));
    let captured_events = events.clone();
    cx.update(|app| {
        app.subscribe(&zoom, move |_, event: &ZoomControlEvent, _| {
            captured_events.borrow_mut().push(*event)
        })
        .detach();
    });

    zoom.update(cx, |zoom, cx| zoom.zoom_in(cx));
    assert_eq!(
        zoom.read_with(cx, |zoom, _| zoom.displayed_percentage()),
        "110%"
    );
    assert_eq!(*events.borrow(), vec![ZoomControlEvent::Changed(1.1)]);
    zoom.update(cx, |zoom, cx| zoom.zoom_out(cx));
    assert_eq!(
        zoom.read_with(cx, |zoom, _| zoom.zoom()),
        DEFAULT_VIEWER_ZOOM
    );
    events.borrow_mut().clear();
    zoom.update(cx, |zoom, cx| {
        zoom.set_zoom(2.5, cx);
        zoom.reset(cx);
    });
    assert_eq!(
        zoom.read_with(cx, |zoom, _| zoom.zoom()),
        DEFAULT_VIEWER_ZOOM
    );
    assert_eq!(
        *events.borrow(),
        vec![ZoomControlEvent::Changed(DEFAULT_VIEWER_ZOOM)]
    );
    assert_eq!(zoom.read_with(cx, |zoom, _| zoom.reset_activations()), 1);

    zoom.update(cx, |zoom, cx| zoom.reset(cx));
    assert_eq!(
        *events.borrow(),
        vec![ZoomControlEvent::Changed(DEFAULT_VIEWER_ZOOM)]
    );
    assert_eq!(zoom.read_with(cx, |zoom, _| zoom.reset_activations()), 2);

    zoom.update(cx, |zoom, cx| {
        zoom.set_disabled(true, cx);
        zoom.zoom_in(cx);
        zoom.reset(cx);
    });
    assert_eq!(
        zoom.read_with(cx, |zoom, _| zoom.zoom()),
        DEFAULT_VIEWER_ZOOM
    );
    assert_eq!(zoom.read_with(cx, |zoom, _| zoom.reset_activations()), 2);
}

#[gpui::test]
fn page_view_actions_are_selected_and_disabled_safe(cx: &mut TestAppContext) {
    let page = cx.new(|_| PageViewControl::single_page());
    let events = Rc::new(RefCell::new(Vec::new()));
    let captured_events = events.clone();
    cx.update(|app| {
        app.subscribe(&page, move |_, event: &PageViewControlEvent, _| {
            captured_events.borrow_mut().push(*event)
        })
        .detach();
    });

    page.update(cx, |page, cx| page.activate_fit(cx));
    assert_eq!(
        page.read_with(cx, |page, _| page.mode()),
        PageViewMode::SinglePage
    );
    assert!(page.read_with(cx, |page, _| page.is_selected()));
    assert_eq!(page.read_with(cx, |page, _| page.fit_page_activations()), 1);
    assert_eq!(
        *events.borrow(),
        vec![PageViewControlEvent::FitActivated(PageViewMode::SinglePage)]
    );

    page.update(cx, |page, cx| {
        page.set_disabled(true, cx);
        page.activate(cx);
        page.activate_fit(cx);
    });
    assert!(!page.read_with(cx, |page, _| page.is_selected()));
    assert_eq!(page.read_with(cx, |page, _| page.primary_activations()), 0);
    assert_eq!(page.read_with(cx, |page, _| page.fit_page_activations()), 1);

    page.update(cx, |page, cx| {
        page.set_wheel_behavior(WheelBehavior::Scroll, cx)
    });
    assert_eq!(
        *events.borrow(),
        vec![PageViewControlEvent::FitActivated(PageViewMode::SinglePage)]
    );
}

#[gpui::test]
fn toolbar_keeps_page_modes_exclusive_and_syncs_document_state(cx: &mut TestAppContext) {
    let continuous = cx.new(|_| PageViewControl::continuous());
    let single_page = cx.new(|_| PageViewControl::single_page());
    let zoom = cx.new(|_| ZoomControl::new());
    let toolbar = cx.new(|cx| {
        ViewerToolbarStrip::new_with_zoom(continuous.clone(), single_page.clone(), zoom.clone(), cx)
    });

    single_page.update(cx, |page, cx| page.activate(cx));
    cx.run_until_parked();
    assert_eq!(
        toolbar.read_with(cx, |toolbar, _| toolbar.page_view_mode()),
        PageViewMode::SinglePage
    );
    assert!(!continuous.read_with(cx, |page, _| page.is_selected()));
    assert!(single_page.read_with(cx, |page, _| page.is_selected()));

    continuous.update(cx, |page, cx| page.activate_fit(cx));
    cx.run_until_parked();
    assert_eq!(
        toolbar.read_with(cx, |toolbar, _| toolbar.page_view_mode()),
        PageViewMode::Continuous
    );
    assert_eq!(
        toolbar.read_with(cx, |toolbar, _| toolbar.fit_preset()),
        FitPreset::Width
    );
    assert_eq!(toolbar.read_with(cx, |toolbar, _| toolbar.fit_changes()), 1);
    assert!(!single_page.read_with(cx, |page, _| page.is_selected()));

    toolbar.update(cx, |toolbar, cx| {
        toolbar.sync_document_state(
            PageViewMode::SinglePage,
            Some(FitPreset::Page),
            WheelBehavior::Zoom,
            WheelBehavior::Scroll,
            2.5,
            true,
            cx,
        );
    });
    assert!(toolbar.read_with(cx, |toolbar, _| toolbar.is_disabled()));
    assert_eq!(
        toolbar.read_with(cx, |toolbar, _| toolbar.page_view_mode()),
        PageViewMode::SinglePage
    );
    assert_eq!(
        toolbar.read_with(cx, |toolbar, _| toolbar.fit_preset()),
        FitPreset::Page
    );
    assert_eq!(
        continuous.read_with(cx, |page, _| page.wheel_behavior()),
        WheelBehavior::Zoom
    );
    assert_eq!(
        single_page.read_with(cx, |page, _| page.wheel_behavior()),
        WheelBehavior::Scroll
    );
    assert_eq!(zoom.read_with(cx, |zoom, _| zoom.zoom()), 2.5);
    assert!(!continuous.read_with(cx, |page, _| page.is_selected()));
    assert!(!single_page.read_with(cx, |page, _| page.is_selected()));

    toolbar.update(cx, |toolbar, cx| {
        toolbar.sync_document_state(
            PageViewMode::Continuous,
            None,
            WheelBehavior::Scroll,
            WheelBehavior::Zoom,
            1.,
            false,
            cx,
        );
    });
    assert!(!toolbar.read_with(cx, |toolbar, _| toolbar.is_disabled()));
    assert!(continuous.read_with(cx, |page, _| page.is_selected()));
    assert!(!single_page.read_with(cx, |page, _| page.is_selected()));
}

#[gpui::test]
fn cad_activation_and_retained_sync_are_disabled_safe(cx: &mut TestAppContext) {
    let cad = cx.new(|cx| CadViewControl::new_deferred(cx));
    let events = Rc::new(RefCell::new(Vec::new()));
    let captured_events = events.clone();
    cx.update(|app| {
        app.subscribe(&cad, move |_, event: &CadViewControlEvent, _| {
            captured_events.borrow_mut().push(*event)
        })
        .detach();
    });

    cad.update(cx, |cad, cx| {
        cad.activate(cx);
        cad.set_organisation(CadViewOrganisation::Rows, cx);
    });
    assert!(cad.read_with(cx, |cad, _| cad.is_active()));
    assert_eq!(
        cad.read_with(cx, |cad, _| cad.organisation()),
        CadViewOrganisation::Rows
    );
    assert_eq!(cad.read_with(cx, |cad, _| cad.organisation_changes()), 1);
    assert_eq!(
        *events.borrow(),
        vec![
            CadViewControlEvent::Activated,
            CadViewControlEvent::OrganisationChanged(CadViewOrganisation::Rows)
        ]
    );

    cad.update(cx, |cad, cx| cad.set_disabled(true, cx));
    assert!(!cad.read_with(cx, |cad, _| cad.is_active()));
    cad.update(cx, |cad, cx| {
        cad.set_active(true, cx);
        cad.set_organisation(CadViewOrganisation::Columns, cx);
        cad.sync_retained_state(true, CadViewOrganisation::Columns, 0, cx);
    });
    assert!(!cad.read_with(cx, |cad, _| cad.is_active()));
    assert_eq!(
        cad.read_with(cx, |cad, _| cad.organisation()),
        CadViewOrganisation::Columns
    );
    assert_eq!(
        cad.read_with(cx, |cad, _| cad.pages_per_column()),
        MIN_PAGES_PER_COLUMN
    );
    assert_eq!(cad.read_with(cx, |cad, _| cad.organisation_changes()), 1);
    assert_eq!(
        *events.borrow(),
        vec![
            CadViewControlEvent::Activated,
            CadViewControlEvent::OrganisationChanged(CadViewOrganisation::Rows)
        ]
    );
}
