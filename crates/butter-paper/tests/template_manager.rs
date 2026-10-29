use butter_paper::document_resource::{
    NativeDocumentOpener, OpenDocumentRequest, OpenedNativeDocument, RasterSurface,
};
use butter_paper::document_workspace::{
    CloseRequestDisposition, DirtyCloseResolution, DocumentId, DocumentWorkspace,
    DocumentWorkspaceTemplateCommand,
};
use butter_paper::generated_document::{
    GeneratedDocumentRequest, GeneratedDocumentStore, GeneratedPattern,
};
use butter_paper::pdf_engine::PdfPersistenceSession;
use butter_paper::template_manager::{
    PersistentTemplateManager, TEMPLATE_MANAGER_BROWSE_PAGE_ID, TEMPLATE_MANAGER_CANCEL_ID,
    TEMPLATE_MANAGER_CLOSE_ID, TEMPLATE_MANAGER_COLOR_IDS, TEMPLATE_MANAGER_COLOR_INPUT_ID,
    TEMPLATE_MANAGER_CREATE_DOCUMENT_ID, TEMPLATE_MANAGER_CREATE_ID, TEMPLATE_MANAGER_DONE_ID,
    TEMPLATE_MANAGER_HEIGHT_INPUT_ID, TEMPLATE_MANAGER_ID, TEMPLATE_MANAGER_IMPORT_ID,
    TEMPLATE_MANAGER_IMPORTED_PREVIEW_ID, TEMPLATE_MANAGER_LIST_ID, TEMPLATE_MANAGER_NAME_FIELD_ID,
    TEMPLATE_MANAGER_NAME_INPUT_ID, TEMPLATE_MANAGER_ORIENTATION_IDS, TEMPLATE_MANAGER_PAPER_IDS,
    TEMPLATE_MANAGER_PATTERN_IDS, TEMPLATE_MANAGER_PREVIEW_ID, TEMPLATE_MANAGER_PREVIEW_PAGE_ID,
    TEMPLATE_MANAGER_SAVE_ID, TEMPLATE_MANAGER_SCROLL_ID, TEMPLATE_MANAGER_SPACING_IDS,
    TEMPLATE_MANAGER_SPACING_INPUT_ID, TEMPLATE_MANAGER_STATUS_ID, TEMPLATE_MANAGER_WIDTH_INPUT_ID,
    TemplateManagerEvent, TemplateManagerMode, TemplateManagerModel, TemplateManagerRecord,
    TemplateManagerView, draft_preview_svg, legacy_blank_request_from_json,
    next_custom_template_id, next_imported_template_id, route_workspace_template_command,
    template_manager_dialog_width, template_manager_section_id,
    template_manager_uses_stacked_layout,
};
use butter_paper::template_preview::TEMPLATE_PREVIEW_FIRST_PAGE_ID;
use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, Modifiers,
    Render, ScrollDelta, ScrollWheelEvent, Styled as _, TestAppContext, Window, div, point, px,
    size,
};
use gpui_component::{Root, WindowExt as _};
use std::sync::Arc;
use std::{cell::RefCell, rc::Rc};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn square_grid_request() -> GeneratedDocumentRequest {
    GeneratedDocumentRequest {
        title: "Untitled".into(),
        width_mm: 420.,
        height_mm: 297.,
        pattern: Some(GeneratedPattern::SquareGrid {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
    }
}

struct TemplateDialogHarness {
    manager: Entity<TemplateManagerView>,
    return_focus: FocusHandle,
}

impl Render for TemplateDialogHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let _keep_manager_alive = &self.manager;
        div()
            .id("template-dialog-return-owner")
            .size_full()
            .track_focus(&self.return_focus)
    }
}

#[test]
fn dialog_geometry_is_bounded_and_stacks_at_constrained_widths() {
    assert_eq!(template_manager_dialog_width(1200.), 880.);
    assert_eq!(template_manager_dialog_width(720.), 688.);
    assert_eq!(template_manager_dialog_width(320.), 288.);
    assert!(!template_manager_uses_stacked_layout(1200.));
    assert!(!template_manager_uses_stacked_layout(720.));
    assert!(template_manager_uses_stacked_layout(719.));
    assert!(template_manager_uses_stacked_layout(320.));
}

#[test]
fn legacy_json_and_preview_preserve_complete_pattern_identity_and_spacing() {
    let request = legacy_blank_request_from_json(
        r##"{"preset":"custom","orientation":"portrait","customWidth":"210","customHeight":"330","patternType":"isometric","patternSpacingPreset":"custom","customPatternSpacing":"7.5","patternColorPreset":"custom","customPatternColor":"#4E95CC"}"##,
    )
    .unwrap();
    assert_eq!((request.width_mm, request.height_mm), (210., 330.));
    let svg = draft_preview_svg(&request);
    assert!(svg.contains("data-pattern=\"isometric\""));
    assert!(svg.contains("data-spacing-mm=\"7.5\""));
    assert!(svg.contains("#4e95cc"));

    for (pattern, marker) in [
        ("dots", "dots"),
        ("grid", "grid"),
        ("lined", "lined"),
        ("isometric", "isometric"),
        ("triangle", "triangle"),
    ] {
        let json = format!(
            "{{\"patternType\":\"{pattern}\",\"patternSpacingPreset\":\"10\",\"patternColorPreset\":\"grey\"}}"
        );
        assert!(
            draft_preview_svg(&legacy_blank_request_from_json(&json).unwrap())
                .contains(&format!("data-pattern=\"{marker}\""))
        );
    }
}

#[gpui::test]
fn real_dialog_resets_state_and_returns_exact_focus(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let manager_slot = Rc::new(RefCell::new(None));
    let focus_slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let manager_slot = manager_slot.clone();
        let focus_slot = focus_slot.clone();
        move |window, cx| {
            let mut model = TemplateManagerModel::new(
                vec![TemplateManagerRecord::built_in(
                    "built-in-blank",
                    "Blank Paper",
                    "A3 · Landscape",
                )],
                "built-in-blank",
            )
            .unwrap();
            model.begin_create();
            let manager = cx.new(|cx| TemplateManagerView::new(model, window, cx));
            let return_focus = cx.focus_handle();
            manager_slot.replace(Some(manager.clone()));
            focus_slot.replace(Some(return_focus.clone()));
            let harness = cx.new(|_| TemplateDialogHarness {
                manager,
                return_focus,
            });
            Root::new(harness, window, cx)
        }
    });
    let manager = manager_slot.borrow().clone().unwrap();
    let return_focus = focus_slot.borrow().clone().unwrap();
    cx.update(|window, cx| {
        return_focus.focus(window, cx);
        TemplateManagerView::open_dialog(&manager, window, cx);
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert_eq!(
        manager.read_with(cx, |manager, _| manager.model().mode()),
        TemplateManagerMode::Browse,
        "opening the retained dialog must reset stale create mode"
    );
    cx.update(|window, cx| window.close_dialog(cx));
    cx.run_until_parked();
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| return_focus.is_focused(window)));
}

#[gpui::test]
fn rendered_browse_and_create_layouts_stay_horizontally_bounded_and_scrollable(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        Root::new(
            cx.new(|cx| {
                TemplateManagerView::new(
                    TemplateManagerModel::new(
                        vec![TemplateManagerRecord::built_in(
                            "built-in-blank",
                            "Blank Paper",
                            "A3 · Landscape",
                        )],
                        "built-in-blank",
                    )
                    .unwrap(),
                    window,
                    cx,
                )
            }),
            window,
            cx,
        )
    });

    let assert_bounded = |cx: &mut gpui::VisualTestContext, width: f32, ids: &[&'static str]| {
        for id in ids {
            let bounds = cx
                .debug_bounds(id)
                .unwrap_or_else(|| panic!("missing {id}"));
            assert!(
                bounds.origin.x >= px(0.),
                "{id} starts outside the viewport"
            );
            assert!(
                bounds.origin.x + bounds.size.width <= px(width),
                "{id} grows horizontally past {width}px"
            );
            assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
        }
    };

    for (width, height) in [(1200., 800.), (736., 816.), (720., 720.), (320., 480.)] {
        cx.simulate_resize(size(px(width), px(height)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_bounded(
            cx,
            width,
            &[
                TEMPLATE_MANAGER_ID,
                TEMPLATE_MANAGER_LIST_ID,
                TEMPLATE_MANAGER_PREVIEW_ID,
                TEMPLATE_MANAGER_CREATE_ID,
                TEMPLATE_MANAGER_IMPORT_ID,
                TEMPLATE_MANAGER_DONE_ID,
            ],
        );
    }
    let list = cx.debug_bounds(TEMPLATE_MANAGER_LIST_ID).unwrap();
    let preview = cx.debug_bounds(TEMPLATE_MANAGER_PREVIEW_ID).unwrap();
    assert!(
        list.bottom() <= preview.top(),
        "stacked browse regions overlap"
    );

    cx.simulate_resize(size(px(1200.), px(800.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let list = cx.debug_bounds(TEMPLATE_MANAGER_LIST_ID).unwrap();
    let preview = cx.debug_bounds(TEMPLATE_MANAGER_PREVIEW_ID).unwrap();
    assert_eq!(
        list.size.width, preview.size.width,
        "side-by-side list and preview split 50/50"
    );
    // The stock dialog body clips its content; an outset close button lost
    // hover over its clipped half.
    let content = cx.debug_bounds(TEMPLATE_MANAGER_ID).unwrap();
    let close = cx.debug_bounds(TEMPLATE_MANAGER_CLOSE_ID).unwrap();
    assert!(
        close.top() >= content.top() && close.right() <= content.right(),
        "close must sit wholly inside the dialog content: {close:?} in {content:?}"
    );
    cx.simulate_resize(size(px(320.), px(480.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let scroll_into_view = |cx: &mut gpui::VisualTestContext, target_id: &'static str| {
        for _ in 0..12 {
            let scroll = cx.debug_bounds(TEMPLATE_MANAGER_SCROLL_ID).unwrap();
            let target = cx.debug_bounds(target_id).unwrap();
            let delta_y = if target.bottom() > scroll.bottom() {
                -(f32::from(target.bottom() - scroll.bottom()) + 8.)
            } else if target.top() < scroll.top() {
                f32::from(scroll.top() - target.top()) + 8.
            } else {
                break;
            };
            cx.simulate_event(ScrollWheelEvent {
                position: scroll.center(),
                delta: ScrollDelta::Pixels(point(px(0.), px(delta_y))),
                ..Default::default()
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        let scroll = cx.debug_bounds(TEMPLATE_MANAGER_SCROLL_ID).unwrap();
        let target = cx.debug_bounds(target_id).unwrap();
        assert!(
            target.top() >= scroll.top() && target.bottom() <= scroll.bottom(),
            "{target_id} must be fully reachable after scrolling"
        );
        target
    };

    let create = scroll_into_view(cx, TEMPLATE_MANAGER_CREATE_ID);
    let import = cx.debug_bounds(TEMPLATE_MANAGER_IMPORT_ID).unwrap();
    let done = cx.debug_bounds(TEMPLATE_MANAGER_DONE_ID).unwrap();
    assert!(create.bottom() <= import.top());
    assert!(import.bottom() <= done.top());
    cx.simulate_click(create.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(TEMPLATE_MANAGER_NAME_INPUT_ID).is_some(),
        "the scrolled constrained footer action must activate create mode"
    );

    assert_bounded(
        cx,
        320.,
        &[
            TEMPLATE_MANAGER_ID,
            TEMPLATE_MANAGER_NAME_INPUT_ID,
            TEMPLATE_MANAGER_PREVIEW_PAGE_ID,
            TEMPLATE_MANAGER_CANCEL_ID,
            TEMPLATE_MANAGER_SAVE_ID,
        ],
    );
    let cancel = scroll_into_view(cx, TEMPLATE_MANAGER_CANCEL_ID);
    let save = cx.debug_bounds(TEMPLATE_MANAGER_SAVE_ID).unwrap();
    assert!(cancel.bottom() <= save.top());
    cx.simulate_click(cancel.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(TEMPLATE_MANAGER_LIST_ID).is_some());
}

#[test]
fn draft_settings_cover_electron_presets_custom_bounds_and_restart_safe_ids() {
    let mut model = TemplateManagerModel::new(
        vec![TemplateManagerRecord::built_in(
            "built-in-blank",
            "Blank Paper",
            "A3 · Landscape",
        )],
        "built-in-blank",
    )
    .unwrap();
    model.begin_create();
    model.set_draft_paper_preset("a5").unwrap();
    assert_eq!(
        (
            model.draft_request().width_mm,
            model.draft_request().height_mm
        ),
        (210., 148.)
    );
    model.set_draft_orientation("portrait").unwrap();
    assert_eq!(
        (
            model.draft_request().width_mm,
            model.draft_request().height_mm
        ),
        (148., 210.)
    );
    model.set_draft_custom_dimensions("320.5", "450").unwrap();
    model.set_draft_pattern("built-in-grid").unwrap();
    model.set_draft_spacing("custom", Some("7.5")).unwrap();
    model.set_draft_color("custom", Some("#4E95CC")).unwrap();
    assert_eq!(
        (
            model.draft_request().width_mm,
            model.draft_request().height_mm
        ),
        (320.5, 450.)
    );
    assert_eq!(
        model.draft_request().pattern,
        Some(GeneratedPattern::SquareGrid {
            spacing_mm: 7.5,
            color: "#4e95cc".into(),
        })
    );
    assert!(model.set_draft_custom_dimensions("9", "450").is_err());
    assert!(model.set_draft_spacing("custom", Some("501")).is_err());
    assert!(model.set_draft_color("custom", Some("blue")).is_err());

    let records = vec![
        TemplateManagerRecord::generated(
            "custom-native-9",
            "Nine",
            "summary",
            square_grid_request(),
        ),
        TemplateManagerRecord::imported(
            "imported-00000000-0000-4000-8000-00000000000f",
            "Fifteen",
            1,
        ),
    ];
    assert_eq!(next_custom_template_id(&records), "custom-native-10");
    assert_eq!(
        next_imported_template_id(&records),
        "imported-00000000-0000-4000-8000-000000000010"
    );
}

#[test]
fn template_manager_retains_stable_dynamic_records_and_transactional_drafts() {
    let records = vec![
        TemplateManagerRecord::built_in("built-in-blank", "Blank Paper", "A3 · Landscape"),
        TemplateManagerRecord::imported(
            "imported-00000000-0000-4000-8000-000000000000",
            "Site Form",
            100,
        ),
    ];
    let mut manager = TemplateManagerModel::new(records, "built-in-blank").unwrap();

    manager
        .select("imported-00000000-0000-4000-8000-000000000000")
        .unwrap();
    assert_eq!(
        manager.selected_id(),
        "imported-00000000-0000-4000-8000-000000000000"
    );
    assert_eq!(
        manager.last_used_id(),
        "built-in-blank",
        "preview selection is transient"
    );

    manager.begin_create();
    assert_eq!(manager.mode(), TemplateManagerMode::Create);
    manager.reset_for_open();
    assert_eq!(manager.mode(), TemplateManagerMode::Browse);
    assert_eq!(manager.selected_id(), manager.last_used_id());
    assert_eq!(manager.error(), None);
    manager.begin_create();
    manager.set_draft_pattern("built-in-triangle").unwrap();
    assert!(matches!(
        manager.draft_request().pattern,
        Some(GeneratedPattern::Triangle { .. })
    ));
    assert!(
        manager
            .save_generated("custom-grid", "   ", square_grid_request())
            .unwrap_err()
            .to_string()
            .contains("required")
    );
    assert_eq!(manager.mode(), TemplateManagerMode::Create);
    manager.cancel_create();
    assert_eq!(manager.mode(), TemplateManagerMode::Browse);
    assert_eq!(manager.selected_id(), "built-in-blank");

    manager.begin_create();
    manager
        .save_generated("custom-grid", "  My   Grid  ", square_grid_request())
        .unwrap();
    assert_eq!(manager.selected_id(), "custom-grid");
    assert_eq!(manager.last_used_id(), "custom-grid");
    assert_eq!(manager.records()[2].name(), "My Grid");
    assert_eq!(
        manager.events().last(),
        Some(&TemplateManagerEvent::GeneratedSaved {
            template_id: "custom-grid".into(),
        })
    );

    manager.request_create_selected().unwrap();
    assert_eq!(
        manager.events().last(),
        Some(&TemplateManagerEvent::CreateRequested {
            template_id: "custom-grid".into(),
        })
    );
    manager.record_failure("Unable to create a PDF from the template.");
    assert_eq!(manager.last_used_id(), "custom-grid");
    assert_eq!(
        manager.error(),
        Some("Unable to create a PDF from the template.")
    );

    assert!(manager.remove("built-in-blank").is_err());
    manager.remove("custom-grid").unwrap();
    assert_eq!(manager.selected_id(), "built-in-blank");
    assert_eq!(manager.last_used_id(), "built-in-blank");
    assert!(
        manager
            .records()
            .iter()
            .all(|record| record.id() != "custom-grid")
    );
}

#[test]
fn imported_completion_is_stale_safe_and_preserves_the_last_good_library() {
    let mut manager = TemplateManagerModel::new(
        vec![TemplateManagerRecord::built_in(
            "built-in-blank",
            "Blank Paper",
            "A3 · Landscape",
        )],
        "built-in-blank",
    )
    .unwrap();

    let first = manager.begin_import().unwrap();
    assert!(
        manager.begin_import().is_none(),
        "a second import is suppressed while busy"
    );
    manager.cancel_import(first);
    assert_eq!(manager.records().len(), 1);

    let second = manager.begin_import().unwrap();
    assert!(!manager.complete_import(
        first,
        TemplateManagerRecord::imported(
            "imported-11111111-1111-4111-8111-111111111111",
            "Stale",
            1,
        ),
    ));
    assert!(manager.complete_import(
        second,
        TemplateManagerRecord::imported(
            "imported-22222222-2222-4222-8222-222222222222",
            "Current",
            2,
        ),
    ));
    assert_eq!(
        manager.last_used_id(),
        "imported-22222222-2222-4222-8222-222222222222"
    );
    assert_eq!(manager.records().len(), 2);
}

fn owned_test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "butter-paper-template-manager-{label}-{}-{nonce}",
        std::process::id()
    ))
}

#[cfg(unix)]
#[test]
fn generated_sources_canonicalise_aliased_ancestors_and_preserve_ownership() {
    let root = owned_test_root("canonical-generated");
    fs::create_dir_all(root.join("real")).unwrap();
    std::os::unix::fs::symlink(root.join("real"), root.join("alias")).unwrap();
    let store = GeneratedDocumentStore::new(root.join("alias/store")).unwrap();
    let source = store
        .create_from_pdf_bytes("document", b"test owned bytes")
        .unwrap();
    assert_eq!(source.path(), fs::canonicalize(source.path()).unwrap());
    store.release(&source).unwrap();
    assert!(!source.path().exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn persistent_manager_restarts_imports_removes_and_keeps_one_storage_authority() {
    let root = owned_test_root("persistence");
    let library_root = root.join("library");
    let source = root.join("Site Form.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bp-multi-page-v1.pdf"),
        &source,
    )
    .unwrap();
    let mut store = PersistentTemplateManager::open(library_root.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().records().len(), 12);
    assert_eq!(
        store.select("built-in-grid").unwrap().last_used_id(),
        "built-in-grid"
    );
    store
        .save_generated("custom-grid", "  My   Grid ", square_grid_request())
        .unwrap();
    let imported_id = "imported-00000000-0000-4000-8000-000000000000";
    let imported = store
        .import_pdf(
            imported_id,
            "Site Form.pdf",
            "2026-08-27T00:00:00Z",
            &source,
        )
        .unwrap();
    assert_eq!(imported.last_used_id(), imported_id);
    assert_eq!(imported.records().len(), 14);
    drop(store);

    fs::write(&source, b"the original source is no longer authoritative").unwrap();
    let mut store = PersistentTemplateManager::open(library_root).unwrap();
    let restarted = store.snapshot().unwrap();
    assert_eq!(restarted.last_used_id(), imported_id);
    assert_eq!(restarted.records().len(), 14);
    assert_eq!(
        restarted
            .records()
            .iter()
            .find(|record| record.id() == imported_id)
            .unwrap()
            .summary(),
        "100 pages · Imported PDF · Page grid not defined"
    );
    assert!(store.remove("built-in-grid").is_err());
    let after_remove = store.remove(imported_id).unwrap();
    assert_eq!(after_remove.last_used_id(), "built-in-blank");
    assert!(
        after_remove
            .records()
            .iter()
            .all(|record| record.id() != imported_id)
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn library_groups_templates_by_paper_size_with_a4_built_ins() {
    let root = owned_test_root("paper-size-groups");
    let library_root = root.join("library");
    let source = root.join("Site Form.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bp-multi-page-v1.pdf"),
        &source,
    )
    .unwrap();
    let mut store = PersistentTemplateManager::open(library_root).unwrap();
    let imported_id = "imported-00000000-0000-4000-8000-0000000000aa";
    store
        .import_pdf(
            imported_id,
            "Site Form.pdf",
            "2026-10-28T00:00:00Z",
            &source,
        )
        .unwrap();
    store
        .save_generated(
            "custom-wide",
            "Wide",
            GeneratedDocumentRequest {
                width_mm: 500.,
                height_mm: 300.,
                ..square_grid_request()
            },
        )
        .unwrap();
    let model = store
        .save_generated(
            "custom-a4-landscape",
            "A4 Landscape Grid",
            GeneratedDocumentRequest {
                width_mm: 297.,
                height_mm: 210.,
                ..square_grid_request()
            },
        )
        .unwrap();

    let summary = |id: &str| {
        let record = model.records().iter().find(|record| record.id() == id);
        let record = record.unwrap();
        format!("{} · {}", record.summary(), record.preview().grid_summary())
    };
    assert_eq!(
        summary("built-in-dots"),
        "A3 · Landscape · Page grid · 10 mm"
    );
    assert_eq!(
        summary("built-in-dots-a4"),
        "A4 · Portrait · Page grid · 10 mm"
    );
    assert_eq!(summary("built-in-blank-a4"), "A4 · Portrait · No page grid");
    assert_eq!(
        summary("custom-a4-landscape"),
        "A4 · Landscape · Page grid · 10 mm"
    );
    assert_eq!(
        summary("custom-wide"),
        "500 × 300 mm · Landscape · Page grid · 10 mm"
    );

    assert_eq!(
        model.records_by_paper_size(),
        vec![
            (
                "A3".to_owned(),
                vec![
                    "built-in-blank",
                    "built-in-dots",
                    "built-in-grid",
                    "built-in-lined",
                    "built-in-isometric",
                    "built-in-triangle",
                ],
            ),
            (
                "A4".to_owned(),
                vec![
                    "built-in-blank-a4",
                    "built-in-dots-a4",
                    "built-in-grid-a4",
                    "built-in-lined-a4",
                    "built-in-isometric-a4",
                    "built-in-triangle-a4",
                    "custom-a4-landscape",
                ],
            ),
            ("500 × 300 mm".to_owned(), vec!["custom-wide"]),
            ("Imported PDFs".to_owned(), vec![imported_id]),
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
fn template_library_renders_a_heading_above_each_paper_size(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let preview_record = |id: &str, name: &str| TemplateManagerRecord::built_in(id, name, "");
    let model = TemplateManagerModel::new(
        vec![
            preview_record("built-in-blank", "Blank Paper"),
            preview_record("built-in-dots", "Dot Grid"),
            preview_record("built-in-blank-a4", "Blank Paper"),
            TemplateManagerRecord::imported(
                "imported-00000000-0000-4000-8000-000000000000",
                "Site Form",
                1,
            ),
        ],
        "built-in-blank-a4",
    )
    .unwrap();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let manager = cx.new(|cx| TemplateManagerView::new(model, window, cx));
        Root::new(manager, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // Debug selectors are `&'static str`; leaking a few test strings is fine.
    let selector = |id: String| -> &'static str { Box::leak(id.into_boxed_str()) };
    let top = |cx: &mut gpui::VisualTestContext, id: &'static str| {
        cx.debug_bounds(id)
            .unwrap_or_else(|| panic!("missing {id}"))
            .top()
    };
    let a3 = top(cx, selector(template_manager_section_id(0)));
    let blank = top(cx, "template-manager-item-built-in-blank");
    let dots = top(cx, "template-manager-item-built-in-dots");
    let a4 = top(cx, selector(template_manager_section_id(1)));
    let blank_a4 = top(cx, "template-manager-item-built-in-blank-a4");
    let imported = top(cx, selector(template_manager_section_id(2)));
    let site_form = top(
        cx,
        "template-manager-item-imported-00000000-0000-4000-8000-000000000000",
    );
    assert!(a3 < blank && blank < dots && dots < a4 && a4 < blank_a4);
    assert!(blank_a4 < imported && imported < site_form);
    assert!(
        cx.debug_bounds(selector(template_manager_section_id(3)))
            .is_none()
    );

    // Selecting across a heading still selects the template.
    let site_form_bounds = cx
        .debug_bounds("template-manager-item-imported-00000000-0000-4000-8000-000000000000")
        .unwrap();
    cx.simulate_click(site_form_bounds.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(TEMPLATE_MANAGER_IMPORTED_PREVIEW_ID)
            .is_some()
    );
}

#[gpui::test]
fn a_long_template_name_truncates_beside_its_remove_control(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let imported_id = "imported-00000000-0000-4000-8000-000000000000";
    let long_name = "Imported site inspection form with a very long descriptive name for the job";
    let model = TemplateManagerModel::new(
        vec![
            TemplateManagerRecord::built_in("built-in-blank", "Blank Paper", ""),
            TemplateManagerRecord::imported(imported_id, long_name, 3),
        ],
        imported_id,
    )
    .unwrap();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let manager = cx.new(|cx| TemplateManagerView::new(model, window, cx));
        Root::new(manager, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx
        .debug_bounds("template-manager-item-imported-00000000-0000-4000-8000-000000000000")
        .unwrap();
    let remove = cx
        .debug_bounds("template-manager-remove-imported-00000000-0000-4000-8000-000000000000")
        .unwrap();
    assert!(
        remove.left() >= row.left() && remove.right() <= row.right(),
        "the Remove control {remove:?} stays inside its row {row:?}"
    );
    assert!(
        remove.size.width <= px(32.),
        "Remove is a compact icon button, not the template's name: {remove:?}"
    );
    if let Some(tree) = cx.update(|window, _| window.debug_a11y_tree_json()) {
        assert!(tree.contains(&format!("Remove {long_name}")), "{tree}");
    }
}

struct FirstPageRenderer(Result<(u32, u32), String>);

impl NativeDocumentOpener for FirstPageRenderer {
    fn open(&self, _: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        Err("previews only".into())
    }

    fn render_first_page(&self, path: &std::path::Path, _: u32) -> Result<RasterSurface, String> {
        assert!(path.ends_with("source.pdf"), "{path:?}");
        let (width, height) = self.0.clone()?;
        RasterSurface::new(width, height, vec![255; (width * height * 4) as usize])
    }
}

fn imported_preview_state(
    cx: &mut TestAppContext,
    renderer: FirstPageRenderer,
    label: &str,
) -> (bool, bool) {
    cx.update(gpui_component::init);
    let root = owned_test_root(label);
    let library_root = root.join("library");
    let source = root.join("Site Form.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bp-multi-page-v1.pdf"),
        &source,
    )
    .unwrap();
    PersistentTemplateManager::open(library_root.clone())
        .unwrap()
        .import_pdf(
            "imported-00000000-0000-4000-8000-0000000000bb",
            "Site Form.pdf",
            "2026-10-28T00:00:00Z",
            &source,
        )
        .unwrap();
    let manager_slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let manager_slot = manager_slot.clone();
        move |window, cx| {
            let manager = cx
                .new(|cx| TemplateManagerView::open_persistent(library_root, window, cx).unwrap());
            manager_slot.replace(Some(manager.clone()));
            Root::new(manager, window, cx)
        }
    });
    let manager = manager_slot.borrow().clone().unwrap();
    manager.update(cx, |manager, cx| {
        manager.bind_preview_renderer(Arc::new(renderer), cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let state = (
        cx.debug_bounds(TEMPLATE_PREVIEW_FIRST_PAGE_ID).is_some(),
        cx.debug_bounds(TEMPLATE_MANAGER_IMPORTED_PREVIEW_ID)
            .is_some(),
    );
    fs::remove_dir_all(root).unwrap();
    state
}

#[gpui::test]
fn an_imported_template_previews_its_first_page(cx: &mut TestAppContext) {
    assert_eq!(
        imported_preview_state(cx, FirstPageRenderer(Ok((60, 85))), "first-page"),
        (true, true),
        "the preview page shows the rendered first page"
    );
}

#[gpui::test]
fn an_imported_template_keeps_the_placeholder_when_its_page_cannot_be_drawn(
    cx: &mut TestAppContext,
) {
    assert_eq!(
        imported_preview_state(
            cx,
            FirstPageRenderer(Err("broken".into())),
            "first-page-fails"
        ),
        (false, true),
        "the placeholder page remains"
    );
}

#[test]
fn legacy_request_migrates_once_and_materialized_templates_keep_independent_sources() {
    let root = owned_test_root("legacy-and-materialize");
    let library_root = root.join("library");
    let generated_root = root.join("generated");
    let source = root.join("Imported.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bp-multi-page-v1.pdf"),
        &source,
    )
    .unwrap();

    let mut manager = PersistentTemplateManager::open(library_root.clone()).unwrap();
    let migrated = manager
        .migrate_legacy_blank_request(Some(square_grid_request()))
        .unwrap();
    assert_eq!(migrated.last_used_id(), "built-in-grid");
    assert_eq!(migrated.records().len(), 12);
    drop(manager);
    let mut manager = PersistentTemplateManager::open(library_root.clone()).unwrap();
    let second = manager
        .migrate_legacy_blank_request(Some(GeneratedDocumentRequest {
            title: "Untitled".into(),
            width_mm: 300.,
            height_mm: 200.,
            pattern: None,
        }))
        .unwrap();
    assert_eq!(
        second.last_used_id(),
        "built-in-grid",
        "migration is one-shot"
    );

    manager
        .save_generated("custom-grid", "My Grid", square_grid_request())
        .unwrap();
    let imported_id = "imported-33333333-3333-4333-8333-333333333333";
    manager
        .import_pdf(imported_id, "Imported.pdf", "2026-08-27T00:00:00Z", &source)
        .unwrap();
    let store = GeneratedDocumentStore::new(generated_root).unwrap();
    let custom = manager
        .materialize("custom-grid", "custom-session", &store)
        .unwrap();
    let imported = manager
        .materialize(imported_id, "imported-session", &store)
        .unwrap();
    assert_ne!(custom.path(), imported.path());
    assert_eq!(
        PdfPersistenceSession::open(custom.path())
            .unwrap()
            .page_count(),
        1
    );
    assert_eq!(
        PdfPersistenceSession::open(imported.path())
            .unwrap()
            .page_count(),
        100
    );
    store.release(&custom).unwrap();
    store.release(&imported).unwrap();
    assert!(!custom.path().exists());
    assert!(!imported.path().exists());

    drop(manager);
    assert_eq!(
        PersistentTemplateManager::open(library_root)
            .unwrap()
            .snapshot()
            .unwrap()
            .last_used_id(),
        imported_id,
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_legacy_data_falls_back_and_snapshot_keeps_generated_before_imported() {
    let root = owned_test_root("legacy-fallback-order");
    let library_root = root.join("library");
    let source = root.join("Imported.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bp-multi-page-v1.pdf"),
        &source,
    )
    .unwrap();
    let mut manager = PersistentTemplateManager::open(library_root.clone()).unwrap();
    let fallback = manager
        .migrate_legacy_blank_request(Some(GeneratedDocumentRequest {
            title: "Untitled".into(),
            width_mm: 0.,
            height_mm: 0.,
            pattern: None,
        }))
        .unwrap();
    assert_eq!(fallback.last_used_id(), "built-in-blank");
    manager
        .import_pdf(
            "imported-00000000-0000-4000-8000-000000000001",
            "Imported",
            "2026-08-27T00:00:00Z",
            &source,
        )
        .unwrap();
    manager
        .save_generated("custom-after-import", "Custom", square_grid_request())
        .unwrap();
    let ids = manager
        .snapshot()
        .unwrap()
        .records()
        .iter()
        .map(|record| record.id().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        &ids[12..],
        &[
            "custom-after-import",
            "imported-00000000-0000-4000-8000-000000000001"
        ]
    );
    drop(manager);
    assert_eq!(
        PersistentTemplateManager::open(library_root)
            .unwrap()
            .migrate_legacy_blank_request(Some(square_grid_request()))
            .unwrap()
            .last_used_id(),
        "custom-after-import",
        "legacy migration must remain one-shot after the fallback"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_document_as_template_copies_only_the_authorized_source_and_recovers_from_failure() {
    let root = owned_test_root("save-document-template");
    let library_root = root.join("library");
    let authorized_source = root.join("Current Drawing.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bp-multi-page-v1.pdf"),
        &authorized_source,
    )
    .unwrap();
    let expected = fs::read(&authorized_source).unwrap();
    let mut manager = PersistentTemplateManager::open(library_root).unwrap();
    let before = manager.snapshot().unwrap();
    assert!(
        manager
            .save_document_as_template(
                "imported-44444444-4444-4444-8444-444444444444",
                "Current Drawing.pdf",
                "2026-08-27T00:00:00Z",
                &root.join("missing.pdf"),
            )
            .is_err()
    );
    assert_eq!(manager.snapshot().unwrap().records(), before.records());

    let id = "imported-55555555-5555-4555-8555-555555555555";
    let snapshot = manager
        .save_document_as_template(
            id,
            "Current Drawing.pdf",
            "2026-08-27T00:00:01Z",
            &authorized_source,
        )
        .unwrap();
    assert_eq!(snapshot.last_used_id(), id);
    assert_eq!(
        fs::read(manager.managed_source_path(id).unwrap()).unwrap(),
        expected
    );
    fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
fn workspace_save_command_routes_once_through_the_retained_manager_and_recovers(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let root = owned_test_root("workspace-save-route");
    let library_root = root.join("library");
    let source = root.join("Current Drawing.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bp-multi-page-v1.pdf"),
        &source,
    )
    .unwrap();
    let manager_slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let manager_slot = manager_slot.clone();
        let library_root = library_root.clone();
        move |window, cx| {
            let manager = cx
                .new(|cx| TemplateManagerView::open_persistent(library_root, window, cx).unwrap());
            manager_slot.replace(Some(manager.clone()));
            Root::new(manager, window, cx)
        }
    });
    let manager = manager_slot.borrow().clone().unwrap();
    let command = DocumentWorkspaceTemplateCommand::SaveDocumentAsTemplate {
        document_id: DocumentId::new(7),
        document_name: "Current Drawing.pdf".into(),
        authorized_source: source,
    };
    cx.update(|window, cx| {
        route_workspace_template_command(&manager, &command, window, cx);
        route_workspace_template_command(&manager, &command, window, cx);
    });
    assert!(manager.read_with(cx, |manager, _| manager.is_storage_busy()));
    cx.run_until_parked();
    assert_eq!(
        manager.read_with(cx, |manager, _| manager.model().records().len()),
        13,
        "a duplicate command while storage is busy must be suppressed"
    );
    assert!(!manager.read_with(cx, |manager, _| manager.is_storage_busy()));

    let missing = DocumentWorkspaceTemplateCommand::SaveDocumentAsTemplate {
        document_id: DocumentId::new(7),
        document_name: "Missing.pdf".into(),
        authorized_source: root.join("missing.pdf"),
    };
    cx.update(|window, cx| route_workspace_template_command(&manager, &missing, window, cx));
    cx.run_until_parked();
    assert_eq!(
        manager.read_with(cx, |manager, _| manager.model().records().len()),
        13
    );
    assert!(manager.read_with(cx, |manager, _| manager.model().error().is_some()));
    fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
fn injected_native_picker_completion_imports_persistently_and_recovers_from_failure(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let root = owned_test_root("native-picker");
    let library_root = root.join("library");
    let source = root.join("Site Form.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bp-multi-page-v1.pdf"),
        &source,
    )
    .unwrap();
    PersistentTemplateManager::open(library_root.clone())
        .unwrap()
        .select("built-in-grid")
        .unwrap();
    let manager_slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let manager_slot = manager_slot.clone();
        let library_root = library_root.clone();
        move |window, cx| {
            let manager = cx
                .new(|cx| TemplateManagerView::open_persistent(library_root, window, cx).unwrap());
            manager_slot.replace(Some(manager.clone()));
            Root::new(manager, window, cx)
        }
    });
    let manager = manager_slot.borrow().clone().unwrap();

    let failed_generation =
        manager.update(cx, |manager, _| manager.begin_import_request().unwrap());
    assert!(cx.update(|window, cx| manager.update(cx, |manager, cx| {
        manager.dispatch_native_import(failed_generation, root.join("missing.pdf"), window, cx)
    })));
    cx.run_until_parked();
    assert!(manager.read_with(cx, |manager, _| manager.model().error().is_some()));

    let stale_generation = manager.update(cx, |manager, _| manager.begin_import_request().unwrap());
    assert!(cx.update(|window, cx| manager.update(cx, |manager, cx| {
        manager.dispatch_native_import(stale_generation, source.clone(), window, cx)
    })));
    assert!(manager.update(cx, |manager, _| {
        manager.cancel_import_request(stale_generation)
    }));
    cx.run_until_parked();
    assert_eq!(
        manager.read_with(cx, |manager, _| manager.model().records().len()),
        12,
        "a cancelled generation must not apply its completed storage result"
    );
    assert_eq!(
        PersistentTemplateManager::open(library_root.clone())
            .unwrap()
            .snapshot()
            .unwrap()
            .last_used_id(),
        "built-in-grid",
        "stale-result cleanup must preserve the prior durable selection"
    );

    let generation = manager.update(cx, |manager, _| manager.begin_import_request().unwrap());
    assert!(cx.update(|window, cx| manager.update(cx, |manager, cx| {
        manager.dispatch_native_import(generation, source.clone(), window, cx)
    })));
    cx.run_until_parked();
    let imported_id = manager.read_with(cx, |manager, _| manager.model().last_used_id().to_owned());
    assert!(imported_id.starts_with("imported-00000000-0000-4000-8000-"));
    assert_eq!(
        PersistentTemplateManager::open(library_root)
            .unwrap()
            .snapshot()
            .unwrap()
            .last_used_id(),
        imported_id
    );
    fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
fn real_list_keyboard_selection_tracks_stable_template_identity(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let model = TemplateManagerModel::new(
        vec![
            TemplateManagerRecord::built_in("built-in-blank", "Blank Paper", "A3 · Landscape"),
            TemplateManagerRecord::imported(
                "imported-00000000-0000-4000-8000-000000000000",
                "Site Form",
                100,
            ),
        ],
        "built-in-blank",
    )
    .unwrap();
    let manager_slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let manager_slot = manager_slot.clone();
        move |window, cx| {
            let manager = cx.new(|cx| TemplateManagerView::new(model, window, cx));
            manager_slot.replace(Some(manager.clone()));
            Root::new(manager, window, cx)
        }
    });
    let manager = manager_slot.borrow().clone().unwrap();

    cx.update(|window, cx| {
        manager.update(cx, |manager, cx| manager.focus_template_list(window, cx));
        window.draw(cx).clear(cx);
    });
    cx.simulate_keystrokes("down");
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert_eq!(
        manager.read_with(cx, |manager, _| manager.model().selected_id().to_owned()),
        "imported-00000000-0000-4000-8000-000000000000"
    );
    assert!(
        cx.debug_bounds("template-manager-item-imported-00000000-0000-4000-8000-000000000000")
            .is_some(),
        "keyboard selection must retain the domain-derived row identity"
    );
}

#[gpui::test]
fn real_manager_primary_action_creates_an_independent_dirty_workspace_session(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let root = owned_test_root("manager-create-document");
    let library_root = root.join("library");
    let store_root = root.join("documents");
    let mut persistent = PersistentTemplateManager::open(library_root.clone()).unwrap();
    persistent
        .save_generated("custom-grid", "My Grid", square_grid_request())
        .unwrap();
    drop(persistent);
    let store = GeneratedDocumentStore::new(store_root.clone()).unwrap();
    let workspace = cx.new(DocumentWorkspace::new);
    let manager_slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let manager_slot = manager_slot.clone();
        let workspace = workspace.clone();
        let store = store.clone();
        move |window, cx| {
            let manager = cx.new(|cx| {
                let mut manager =
                    TemplateManagerView::open_persistent(library_root, window, cx).unwrap();
                manager.bind_document_workspace(workspace.downgrade(), store);
                manager
            });
            manager_slot.replace(Some(manager.clone()));
            Root::new(manager, window, cx)
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let create = cx
        .debug_bounds(TEMPLATE_MANAGER_CREATE_DOCUMENT_ID)
        .unwrap();
    cx.simulate_click(create.center(), Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        1
    );
    let document_id =
        workspace.read_with(cx, |workspace, cx| workspace.sessions()[0].read(cx).id());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.document_dirty_revision(document_id, cx)
        }),
        Some(0)
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .resolve_dirty_close_discard(cx)),
        DirtyCloseResolution::Discarded
    );
    store.remove_if_empty().unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
fn removable_template_requires_confirmation_before_library_mutation(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let manager_slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let manager_slot = manager_slot.clone();
        move |window, cx| {
            let manager = cx.new(|cx| {
                TemplateManagerView::new(
                    TemplateManagerModel::new(
                        vec![
                            TemplateManagerRecord::built_in(
                                "built-in-blank",
                                "Blank Paper",
                                "A3 · Landscape",
                            ),
                            TemplateManagerRecord::generated(
                                "custom-grid",
                                "Site Grid",
                                "A3 · Landscape · Square grid",
                                square_grid_request(),
                            ),
                        ],
                        "custom-grid",
                    )
                    .unwrap(),
                    window,
                    cx,
                )
            });
            manager_slot.replace(Some(manager.clone()));
            Root::new(manager, window, cx)
        }
    });
    let manager = manager_slot.borrow().clone().unwrap();
    let remove_id = "template-manager-remove-custom-grid";

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let remove = cx
        .debug_bounds(remove_id)
        .expect("remove shortcut must render");
    cx.simulate_click(remove.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(manager.read_with(cx, |manager, _| {
        manager
            .model()
            .records()
            .iter()
            .any(|record| record.id() == "custom-grid")
    }));

    if let Some(tree) = cx.update(|window, _| window.debug_a11y_tree_json()) {
        assert!(tree.contains("Remove “Site Grid”?"));
        assert!(tree.contains("This cannot be undone."));
    }
    cx.update(|window, cx| window.close_dialog(cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(manager.read_with(cx, |manager, _| {
        manager
            .model()
            .records()
            .iter()
            .any(|record| record.id() == "custom-grid")
    }));
}

#[gpui::test]
fn real_component_manager_renders_dynamic_rows_validation_and_create_flow(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let model = TemplateManagerModel::new(
        vec![
            TemplateManagerRecord::built_in(
                "built-in-blank",
                "Blank Paper",
                "420 × 297 mm · Landscape",
            ),
            TemplateManagerRecord::imported(
                "imported-00000000-0000-4000-8000-000000000000",
                "Site Form",
                100,
            ),
        ],
        "built-in-blank",
    )
    .unwrap();
    let manager_slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let manager_slot = manager_slot.clone();
        move |window, cx| {
            let manager = cx.new(|cx| TemplateManagerView::new(model, window, cx));
            manager_slot.replace(Some(manager.clone()));
            Root::new(manager, window, cx)
        }
    });
    let manager = manager_slot.borrow().clone().unwrap();

    cx.update(|window, cx| window.draw(cx).clear(cx));
    for stable_id in [
        TEMPLATE_MANAGER_ID,
        TEMPLATE_MANAGER_LIST_ID,
        TEMPLATE_MANAGER_PREVIEW_ID,
        TEMPLATE_MANAGER_CREATE_ID,
        TEMPLATE_MANAGER_IMPORT_ID,
        TEMPLATE_MANAGER_DONE_ID,
        "template-manager-item-built-in-blank",
        "template-manager-item-imported-00000000-0000-4000-8000-000000000000",
        "template-manager-remove-imported-00000000-0000-4000-8000-000000000000",
    ] {
        assert!(
            cx.debug_bounds(stable_id).is_some(),
            "missing real component {stable_id}"
        );
    }
    assert!(cx.debug_bounds(TEMPLATE_MANAGER_BROWSE_PAGE_ID).is_some());
    let imported = cx
        .debug_bounds("template-manager-item-imported-00000000-0000-4000-8000-000000000000")
        .unwrap();
    cx.simulate_click(imported.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(TEMPLATE_MANAGER_IMPORTED_PREVIEW_ID)
            .is_some()
    );

    let create = cx.debug_bounds(TEMPLATE_MANAGER_CREATE_ID).unwrap();

    cx.simulate_click(create.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for stable_id in [
        TEMPLATE_MANAGER_NAME_FIELD_ID,
        TEMPLATE_MANAGER_NAME_INPUT_ID,
        TEMPLATE_MANAGER_PREVIEW_ID,
        TEMPLATE_MANAGER_PREVIEW_PAGE_ID,
        TEMPLATE_MANAGER_CANCEL_ID,
        TEMPLATE_MANAGER_SAVE_ID,
    ] {
        assert!(
            cx.debug_bounds(stable_id).is_some(),
            "missing create control {stable_id}"
        );
    }
    for stable_id in TEMPLATE_MANAGER_PATTERN_IDS {
        assert!(
            cx.debug_bounds(stable_id).is_some(),
            "missing real pattern control {stable_id}"
        );
    }
    for stable_id in TEMPLATE_MANAGER_PAPER_IDS
        .into_iter()
        .chain(TEMPLATE_MANAGER_ORIENTATION_IDS)
    {
        assert!(
            cx.debug_bounds(stable_id).is_some(),
            "missing setting control {stable_id}"
        );
    }

    let custom_paper = cx.debug_bounds(TEMPLATE_MANAGER_PAPER_IDS[6]).unwrap();
    cx.simulate_click(custom_paper.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(TEMPLATE_MANAGER_WIDTH_INPUT_ID).is_some());
    assert!(cx.debug_bounds(TEMPLATE_MANAGER_HEIGHT_INPUT_ID).is_some());
    let [width_input, height_input, spacing_input, color_input] =
        manager.read_with(cx, |manager, _| manager.draft_inputs());
    cx.update(|window, cx| {
        width_input.update(cx, |input, cx| input.set_value("333", window, cx));
        height_input.update(cx, |input, cx| input.set_value("444", window, cx));
    });
    cx.run_until_parked();
    assert!(manager.update(cx, |manager, cx| manager.refresh_draft_preview(cx)));
    assert_eq!(
        manager.read_with(cx, |manager, _| (
            manager.model().draft_request().width_mm,
            manager.model().draft_request().height_mm,
        )),
        (333., 444.),
        "valid custom input must update the retained preview model before Save"
    );

    let triangle = cx.debug_bounds(TEMPLATE_MANAGER_PATTERN_IDS[5]).unwrap();
    cx.simulate_click(triangle.center(), Modifiers::default());
    assert!(manager.read_with(cx, |manager, _| matches!(
        manager.model().draft_request().pattern,
        Some(GeneratedPattern::Triangle { .. })
    )));
    for stable_id in TEMPLATE_MANAGER_SPACING_IDS
        .into_iter()
        .chain(TEMPLATE_MANAGER_COLOR_IDS)
    {
        assert!(
            cx.debug_bounds(stable_id).is_some(),
            "missing pattern setting {stable_id}"
        );
    }
    let custom_spacing = cx.debug_bounds(TEMPLATE_MANAGER_SPACING_IDS[3]).unwrap();
    cx.simulate_click(custom_spacing.center(), Modifiers::default());
    let custom_color = cx.debug_bounds(TEMPLATE_MANAGER_COLOR_IDS[3]).unwrap();
    cx.simulate_click(custom_color.center(), Modifiers::default());
    cx.update(|window, cx| {
        spacing_input.update(cx, |input, cx| input.set_value("12.5", window, cx));
        color_input.update(cx, |input, cx| input.set_value("#123456", window, cx));
    });
    cx.run_until_parked();
    assert!(manager.update(cx, |manager, cx| manager.refresh_draft_preview(cx)));
    assert_eq!(
        manager.read_with(cx, |manager, _| manager
            .model()
            .draft_request()
            .pattern
            .clone()),
        Some(GeneratedPattern::Triangle {
            spacing_mm: 12.5,
            color: "#123456".into(),
        }),
        "valid spacing and colour input must update the live preview model"
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(TEMPLATE_MANAGER_SPACING_INPUT_ID).is_some());
    assert!(cx.debug_bounds(TEMPLATE_MANAGER_COLOR_INPUT_ID).is_some());

    let save = cx.debug_bounds(TEMPLATE_MANAGER_SAVE_ID).unwrap();
    cx.simulate_click(save.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(TEMPLATE_MANAGER_STATUS_ID).is_some());
    assert_eq!(
        manager.read_with(cx, |manager, _| manager.model().mode()),
        TemplateManagerMode::Create,
    );

    let name_input = manager.read_with(cx, |manager, _| manager.name_input());
    cx.update(|window, cx| {
        name_input.update(cx, |input, cx| {
            input.set_value("My Native Grid", window, cx)
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save_after_validation = cx.debug_bounds(TEMPLATE_MANAGER_SAVE_ID).unwrap();
    cx.simulate_click(save_after_validation.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("template-manager-item-custom-native-1")
            .is_some()
    );
    assert_eq!(
        manager.read_with(cx, |manager, _| manager.model().last_used_id().to_owned()),
        "custom-native-1",
    );
}

// The application window's composition: Root's dialog layer over the document,
// and lost focus restored as the application restores it.
struct TemplateFocusShell {
    manager: Entity<TemplateManagerView>,
    document: FocusHandle,
    _focus_lost: gpui::Subscription,
}

impl Render for TemplateFocusShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui::ParentElement as _;
        let _keep_manager_alive = &self.manager;
        let dialogs = Root::render_dialog_layer(window, cx);
        div()
            .id("template-focus-document")
            .size_full()
            .track_focus(&self.document)
            .children(dialogs)
    }
}

#[gpui::test]
fn removing_a_template_by_keyboard_keeps_focus_in_the_manager(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let slot = slot.clone();
        move |window, cx| {
            let manager = cx.new(|cx| {
                TemplateManagerView::new(
                    TemplateManagerModel::new(
                        vec![
                            TemplateManagerRecord::built_in(
                                "built-in-blank",
                                "Blank Paper",
                                "A3 · Landscape",
                            ),
                            TemplateManagerRecord::generated(
                                "custom-grid",
                                "Site Grid",
                                "A3 · Landscape · Square grid",
                                square_grid_request(),
                            ),
                        ],
                        "custom-grid",
                    )
                    .unwrap(),
                    window,
                    cx,
                )
            });
            let document = cx.focus_handle();
            slot.replace(Some((manager.clone(), document.clone())));
            let shell = cx.new(|cx| TemplateFocusShell {
                manager,
                document,
                _focus_lost: cx.on_focus_lost(
                    window,
                    |shell: &mut TemplateFocusShell, window, cx| {
                        butter_paper::application_shell::restore_lost_focus(
                            &shell.document,
                            window,
                            cx,
                        );
                    },
                ),
            });
            Root::new(shell, window, cx)
        }
    });
    let (manager, document) = slot.borrow().clone().unwrap();
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.executor().advance_clock(Duration::from_millis(300));
            cx.run_until_parked();
        }
    };
    let press_space = |cx: &mut gpui::VisualTestContext| {
        let space = gpui::Keystroke::parse("space").unwrap();
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: space.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(gpui::KeyUpEvent { keystroke: space });
    };
    cx.update(|window, cx| {
        document.focus(window, cx);
        TemplateManagerView::open_dialog(&manager, window, cx);
    });
    settle(cx);
    // Tab reaches Site Grid's Remove button; Space presses it and Enter
    // confirms the removal, which takes the focused button away with its row.
    cx.simulate_keystrokes("tab tab");
    settle(cx);
    press_space(cx);
    settle(cx);
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert!(manager.read_with(cx, |manager, _| {
        !manager
            .model()
            .records()
            .iter()
            .any(|record| record.id() == "custom-grid")
    }));
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the manager stays open"
    );
    assert!(
        !cx.update(|window, _| document.is_focused(window)),
        "focus stays in the manager, not the document behind it"
    );
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Escape still closes the manager"
    );
}

#[gpui::test]
fn a_template_saved_in_another_window_appears_in_the_list(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let root = owned_test_root("shared-reload");
    let library_root = root.join("library");
    let manager_slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let manager_slot = manager_slot.clone();
        let library_root = library_root.clone();
        move |window, cx| {
            let manager = cx
                .new(|cx| TemplateManagerView::open_persistent(library_root, window, cx).unwrap());
            manager_slot.replace(Some(manager.clone()));
            Root::new(manager, window, cx)
        }
    });
    let manager = manager_slot.borrow().clone().unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("template-manager-remove-custom-grid")
            .is_none()
    );
    // Another window saves a template into the same library.
    let shared = manager.read_with(cx, |manager, _| manager.shared_persistent().unwrap());
    shared
        .lock()
        .unwrap()
        .save_generated("custom-grid", "Site Grid", square_grid_request())
        .unwrap();
    assert!(manager.update(cx, |manager, cx| manager.reload_from_shared(cx)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("template-manager-remove-custom-grid")
            .is_some(),
        "the list shows the new template, not the records it had before"
    );
    fs::remove_dir_all(root).unwrap();
}
