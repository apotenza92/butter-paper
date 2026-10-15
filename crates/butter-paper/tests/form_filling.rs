//! Filling form fields with the Select tool: a press in a text field opens
//! its editor, Tab moves to the next field and Escape finishes, each value
//! is one undoable change, check boxes and radio buttons toggle on a click,
//! and the worker is told what to render.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use butter_paper::annotation_adapter::AnnotationTool;
use butter_paper::document_resource::{
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DOCUMENT_FORM_CHOICE_MENU_ID, DocumentId, DocumentWorkspace, document_annotation_layer_id,
    init_document_workspace_actions,
};
use butter_paper::page_view_control::PageViewMode;
use butter_paper::pdf_forms::{FormFieldEdit, FormLayer, FormValue, extract_form_layer};
use butter_paper::pdf_worker::DocumentEdits;
use gpui::{AppContext as _, Modifiers, Pixels, Point, TestAppContext, point, px};
use gpui_component::Root;

const PAGE: (f32, f32) = (595.3, 841.9);

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

fn fixture_form() -> FormLayer {
    extract_form_layer(
        &lopdf::Document::load_mem(include_bytes!("fixtures/forms/libreoffice.pdf")).unwrap(),
    )
}

struct FormResource {
    form: Arc<FormLayer>,
    shown: Arc<Mutex<Vec<Vec<FormFieldEdit>>>>,
    released: AtomicBool,
}

impl NativeDocumentResource for FormResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4400)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_tile(
        &self,
        request: butter_paper::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        Ok(raster(
            request.crop.width as u32,
            request.crop.height as u32,
        ))
    }

    fn form_layer(&self) -> Arc<FormLayer> {
        self.form.clone()
    }

    fn set_document_edits(&self, edits: &DocumentEdits) -> Result<(), String> {
        self.shown.lock().unwrap().push(edits.forms.clone());
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

/// Opens the fixture form and returns the workspace, its document and the
/// form edits the worker was asked to show.
fn form_workspace(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<DocumentWorkspace>,
    DocumentId,
    Arc<Mutex<Vec<Vec<FormFieldEdit>>>>,
    &mut gpui::VisualTestContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let shown = Arc::new(Mutex::new(Vec::new()));
    let resource = Arc::new(FormResource {
        form: Arc::new(fixture_form()),
        shown: shown.clone(),
        released: AtomicBool::new(false),
    });
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("form.pdf"), cx)
    });
    let document_id = request.document_id;
    workspace.update(cx, |workspace, cx| {
        let opened = OpenedNativeDocument::new(
            "form.pdf",
            vec![PAGE],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            resource,
        )
        .unwrap();
        workspace.apply_open_result(&request, Ok(opened), cx);
        workspace.set_view_configuration(document_id, PageViewMode::SinglePage, 100., cx);
        workspace
            .refresh_viewport_async(document_id, 800., 1000., 1., cx)
            .unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select)
    );
    (workspace, document_id, shown, cx)
}

/// The window point at the centre of a field's widget.
fn widget_centre(
    cx: &mut gpui::VisualTestContext,
    document_id: DocumentId,
    field: &str,
    widget: usize,
) -> Point<Pixels> {
    let rect = fixture_form().field(field).unwrap().widgets[widget].rect;
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    // The page is centred in its layer at a uniform scale.
    let scale = (f32::from(layer.size.width) / PAGE.0).min(f32::from(layer.size.height) / PAGE.1);
    let origin = point(
        layer.origin.x + (layer.size.width - px(PAGE.0 * scale)) / 2.,
        layer.origin.y + (layer.size.height - px(PAGE.1 * scale)) / 2.,
    );
    let x = (rect[0] + rect[2]) as f32 / 2.;
    let y = (rect[1] + rect[3]) as f32 / 2.;
    point(
        origin.x + px(x * scale),
        origin.y + px((PAGE.1 - y) * scale),
    )
}

fn click(cx: &mut gpui::VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_click(at, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn value_of(
    workspace: &gpui::Entity<DocumentWorkspace>,
    cx: &mut gpui::VisualTestContext,
    document_id: DocumentId,
    name: &str,
) -> FormValue {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .form_field_values(document_id, cx)
            .into_iter()
            .find(|(field, _)| field == name)
            .unwrap()
            .1
    })
}

#[gpui::test]
fn text_fields_fill_in_tab_order_and_commit_one_undoable_change_each(cx: &mut TestAppContext) {
    let (workspace, document_id, shown, cx) = form_workspace(cx);
    let at = widget_centre(cx, document_id, "ApplicantName", 0);
    click(cx, at);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.form_field_focus_summary(cx)),
        Some(("ApplicantName".into(), String::new()))
    );
    // Tool shortcut letters type into the field.
    cx.simulate_input("Ada Lovelace");
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.form_field_focus_summary(cx)),
        Some(("Email".into(), String::new())),
        "Tab moves to the next field"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
        "typed letters must not switch tools"
    );
    cx.simulate_input("ada@example.com");
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.form_field_focus_summary(cx)),
        None,
        "Escape finishes filling"
    );
    assert_eq!(
        value_of(&workspace, cx, document_id, "ApplicantName"),
        FormValue::Text("Ada Lovelace".into())
    );
    assert_eq!(
        value_of(&workspace, cx, document_id, "Email"),
        FormValue::Text("ada@example.com".into())
    );
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert!(snapshot.dirty);
    assert_eq!(snapshot.form_field_edits.len(), 2);
    assert_eq!(
        shown.lock().unwrap().last().unwrap(),
        &snapshot.form_field_edits,
        "the worker renders the values"
    );

    // A field's maximum length holds, and Tab leaves a multi-line field
    // rather than indenting it.
    let postcode = widget_centre(cx, document_id, "Postcode", 0);
    click(cx, postcode);
    cx.simulate_input("30519");
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.form_field_focus_summary(cx)),
        Some(("AccessNotes".into(), String::new()))
    );
    cx.simulate_input("Gate code 4410");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Dog in the yard");
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace.form_field_focus_summary(cx))
            .map(|(name, _)| name),
        Some("InspectionType".into())
    );
    assert_eq!(
        value_of(&workspace, cx, document_id, "Postcode"),
        FormValue::Text("3051".into())
    );
    assert_eq!(
        value_of(&workspace, cx, document_id, "AccessNotes"),
        FormValue::Text("Gate code 4410\nDog in the yard".into())
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    for _ in 0..2 {
        workspace
            .update(cx, |workspace, cx| {
                workspace.undo_annotations(document_id, cx)
            })
            .unwrap();
    }

    // Each field is its own change.    // Each field is its own change.
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(
        value_of(&workspace, cx, document_id, "Email"),
        FormValue::Text(String::new())
    );
    assert_eq!(
        value_of(&workspace, cx, document_id, "ApplicantName"),
        FormValue::Text("Ada Lovelace".into())
    );
    assert_eq!(shown.lock().unwrap().last().unwrap().len(), 1);
}

#[gpui::test]
fn check_boxes_and_radio_buttons_toggle_on_a_click(cx: &mut TestAppContext) {
    let (workspace, document_id, _, cx) = form_workspace(cx);
    let on = fixture_form().field("SiteReady").unwrap().widgets[0]
        .on_state
        .clone();
    let check = widget_centre(cx, document_id, "SiteReady", 0);
    click(cx, check);
    assert_eq!(
        value_of(&workspace, cx, document_id, "SiteReady"),
        FormValue::Button(on)
    );
    // Space toggles the focused check box back.
    cx.simulate_keystrokes("space");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(
        value_of(&workspace, cx, document_id, "SiteReady"),
        FormValue::Button(None)
    );

    let final_option = widget_centre(cx, document_id, "InspectionType", 2);
    click(cx, final_option);
    assert_eq!(
        value_of(&workspace, cx, document_id, "InspectionType"),
        FormValue::Button(Some("Final".into()))
    );
    // A radio group that cannot be turned off stays on its choice.
    click(cx, final_option);
    assert_eq!(
        value_of(&workspace, cx, document_id, "InspectionType"),
        FormValue::Button(Some("Final".into()))
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        value_of(&workspace, cx, document_id, "InspectionType"),
        FormValue::Button(Some("Frame".into()))
    );
}

#[gpui::test]
fn a_choice_field_opens_its_menu_and_escape_leaves_it_unchanged(cx: &mut TestAppContext) {
    let (workspace, document_id, _, cx) = form_workspace(cx);
    let combo = widget_centre(cx, document_id, "BuildingClass", 0);
    click(cx, combo);
    assert!(
        cx.debug_bounds(DOCUMENT_FORM_CHOICE_MENU_ID).is_some(),
        "the options open below the field"
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds(DOCUMENT_FORM_CHOICE_MENU_ID).is_none());
    assert_eq!(
        value_of(&workspace, cx, document_id, "BuildingClass"),
        FormValue::Choices(vec!["Class 1a".into()])
    );
}

/// The worker process renders filled fields into the page, as the app's
/// views request it.
#[test]
#[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
fn the_worker_process_renders_filled_fields() {
    use butter_paper::document_resource::{
        NativeDocumentOpener, OpenDocumentRequest, PdfiumWorkerBackend,
    };
    let worker = std::env::current_exe()
        .unwrap()
        .parent()
        .and_then(|deps| deps.parent())
        .unwrap()
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = PathBuf::from(std::env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-scratch")
        .join(format!("form-worker-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("form.pdf");
    std::fs::write(&path, include_bytes!("fixtures/forms/libreoffice.pdf")).unwrap();
    let backend = PdfiumWorkerBackend::new(worker, library, root.join("surfaces"));
    let opened = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(1),
            generation: 1,
            path,
        })
        .unwrap();
    let resource = opened;
    let form = resource.form_layer();
    assert_eq!(form.fields.len(), 9, "the opened document carries its form");
    let width = 595;
    let dark = |raster: &RasterSurface, name: &str| {
        let rect = form.field(name).unwrap().widgets[0].rect;
        let scale = raster.width() as f64 / PAGE.0 as f64;
        let mut count = 0;
        let (x0, x1) = ((rect[0] * scale) as u32, (rect[2] * scale) as u32);
        let (y0, y1) = (
            ((PAGE.1 as f64 - rect[3]) * scale) as u32,
            ((PAGE.1 as f64 - rect[1]) * scale) as u32,
        );
        for y in y0..y1 {
            for x in x0..x1 {
                let pixel = &raster.pixels_bgra()[((y * raster.width() + x) * 4) as usize..][..4];
                if pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100 {
                    count += 1;
                }
            }
        }
        count
    };
    let before = resource.render_page(0, width).unwrap();
    let check = form.field("SiteReady").unwrap();
    resource
        .set_document_edits(&DocumentEdits {
            text: Vec::new(),
            forms: vec![
                FormFieldEdit {
                    name: "ApplicantName".into(),
                    original: FormValue::Text(String::new()),
                    value: FormValue::Text("Ada Lovelace".into()),
                },
                FormFieldEdit {
                    name: "SiteReady".into(),
                    original: check.value.clone(),
                    value: FormValue::Button(check.widgets[0].on_state.clone()),
                },
            ],
        })
        .unwrap();
    let after = resource.render_page(0, width).unwrap();
    assert!(
        dark(&after, "ApplicantName") > dark(&before, "ApplicantName") + 30,
        "the filled name renders"
    );
    assert!(
        dark(&after, "SiteReady") > dark(&before, "SiteReady"),
        "the check box renders checked"
    );
    resource.close().unwrap();
    std::fs::remove_dir_all(root).ok();
}
