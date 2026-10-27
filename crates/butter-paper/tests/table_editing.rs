//! Tables with Edit PDF in the workspace, on a table Chrome wrote
//! (`tests/fixtures/text-edit/chrome-table.pdf`): typing in a cell grows its
//! row as one undoable edit, an empty cell takes typing, Tab moves between
//! cells, and a table grown into the text below it is labelled.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use butter_paper::document_resource::{
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DocumentId, DocumentWorkspace, document_annotation_layer_id, init_document_workspace_actions,
};
use butter_paper::page_view_control::PageViewMode;
use butter_paper::pdf_tables::{BandSource, page_tables};
use butter_paper::pdf_text_edit::{PageTextLayer, TextEditLimits, extract_page_text};
use butter_paper::pdf_worker::DocumentEdits;
use gpui::{AppContext as _, Modifiers, TestAppContext, point, px};
use gpui_component::Root;

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

fn chrome_table() -> PageTextLayer {
    let path = format!(
        "{}/tests/fixtures/text-edit/chrome-table.pdf",
        env!("CARGO_MANIFEST_DIR")
    );
    let document = lopdf::Document::load(&path).unwrap();
    extract_page_text(&document, 0, &TextEditLimits::default()).unwrap()
}

struct TableResource {
    layers: Vec<PageTextLayer>,
    shown: Arc<Mutex<Vec<DocumentEdits>>>,
    released: AtomicBool,
}

impl NativeDocumentResource for TableResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4301)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_thumbnail(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
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

    fn page_text(&self, page_index: u32) -> Result<PageTextLayer, String> {
        Ok(self.layers[page_index as usize].clone())
    }

    fn set_document_edits(&self, edits: &DocumentEdits) -> Result<(), String> {
        self.shown.lock().unwrap().push(edits.clone());
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

fn table_workspace(
    cx: &mut TestAppContext,
    layer: PageTextLayer,
) -> (
    gpui::Entity<DocumentWorkspace>,
    DocumentId,
    Arc<Mutex<Vec<DocumentEdits>>>,
    &mut gpui::VisualTestContext,
) {
    pages_workspace(cx, vec![layer], PageViewMode::SinglePage, 100.)
}

/// A workspace showing a document of `layers`' pages in Edit PDF mode.
fn pages_workspace(
    cx: &mut TestAppContext,
    layers: Vec<PageTextLayer>,
    mode: PageViewMode,
    zoom: f32,
) -> (
    gpui::Entity<DocumentWorkspace>,
    DocumentId,
    Arc<Mutex<Vec<DocumentEdits>>>,
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
    let sizes = layers
        .iter()
        .map(|layer| {
            let [left, bottom, right, top] = layer.page_box.unwrap_or([0., 0., 612., 792.]);
            ((right - left) as f32, (top - bottom) as f32)
        })
        .collect::<Vec<_>>();
    let pages = layers.len() as u32;
    let resource = Arc::new(TableResource {
        layers,
        shown: shown.clone(),
        released: AtomicBool::new(false),
    });
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("table.pdf"), cx)
    });
    let document_id = request.document_id;
    workspace.update(cx, |workspace, cx| {
        let opened = OpenedNativeDocument::new(
            "table.pdf",
            sizes,
            raster(32, 40),
            (0..pages)
                .map(|page| ThumbnailSurface::new(page, raster(8, 10)))
                .collect(),
            resource,
        )
        .unwrap();
        workspace.apply_open_result(&request, Ok(opened), cx);
        workspace.set_view_configuration(document_id, mode, zoom, cx);
        workspace
            .refresh_viewport_async(document_id, 900., 1000., 1., cx)
            .unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    workspace.update(cx, |workspace, cx| {
        workspace.set_content_edit_mode(document_id, true, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    (workspace, document_id, shown, cx)
}

/// The window position of page point (`x`, `y`) of an upright 612 x 792
/// page.
fn window_point(
    cx: &mut gpui::VisualTestContext,
    document_id: DocumentId,
    at: (f64, f64),
) -> gpui::Point<gpui::Pixels> {
    page_window_point(cx, document_id, 0, (612., 792.), at)
}

/// The window position of point (`x`, `y`) of upright page `page`, of
/// `size`.
fn page_window_point(
    cx: &mut gpui::VisualTestContext,
    document_id: DocumentId,
    page: u32,
    (width, height): (f64, f64),
    (x, y): (f64, f64),
) -> gpui::Point<gpui::Pixels> {
    let layer_id = Box::leak(document_annotation_layer_id(document_id, page).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / width as f32)
        .min(f32::from(layer.size.height) / height as f32);
    let origin = point(
        layer.origin.x + (layer.size.width - px(width as f32 * scale)) / 2.,
        layer.origin.y + (layer.size.height - px(height as f32 * scale)) / 2.,
    );
    point(
        origin.x + px(x as f32 * scale),
        origin.y + px((height - y) as f32 * scale),
    )
}

/// Clicks page point (`x`, `y`) of an upright 612 x 792 page.
fn click_page_point(cx: &mut gpui::VisualTestContext, document_id: DocumentId, at: (f64, f64)) {
    let at = window_point(cx, document_id, at);
    click_at(cx, at);
}

fn click_at(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.run_until_parked();
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_click(at, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// Types `text` key by key.
fn type_text(cx: &mut gpui::VisualTestContext, text: &str) {
    let keys = text
        .chars()
        .map(|character| match character {
            ' ' => "space".to_owned(),
            ',' => "comma".to_owned(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&keys);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn line_end(layer: &PageTextLayer, text: &str) -> (f64, f64) {
    let line = layer
        .lines
        .iter()
        .find(|line| line.text.trim() == text)
        .unwrap_or_else(|| panic!("no line {text:?}"));
    let last = line.chars.last().unwrap();
    let (x, y) = line.origin();
    (x + (last.x + last.advance) * line.matrix[0] - 0.5, y + 3.)
}

#[gpui::test]
fn typing_in_a_cell_grows_its_row_as_one_undoable_edit(cx: &mut TestAppContext) {
    let layer = chrome_table();
    let table = page_tables(&layer).remove(0);
    let base_height = table.rows[2] - table.rows[3];
    let washers = line_end(&layer, "Washers");
    let (workspace, document_id, shown, cx) = table_workspace(cx, layer);
    click_page_point(cx, document_id, washers);
    type_text(
        cx,
        " and spring washers in assorted sizes for the site works",
    );
    let live = shown.lock().unwrap().last().cloned().unwrap();
    assert_eq!(live.tables.len(), 1, "{live:#?}");
    let row = live.tables[0].rows[2];
    assert_eq!(row.source, BandSource::Base(2));
    assert!(row.size > base_height + 5., "{} {}", row.size, base_height);
    assert!(
        live.text
            .iter()
            .any(|edit| !edit.layout.extra_lines.is_empty()),
        "the cell's text wraps onto a new line: {:#?}",
        live.text
    );

    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert_eq!(snapshot.page_table_edits, live.tables);
    assert_eq!(snapshot.page_text_edits, live.text);
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert!(snapshot.page_table_edits.is_empty());
    assert!(snapshot.page_text_edits.is_empty());
    let shown_now = shown.lock().unwrap().last().cloned().unwrap();
    assert!(shown_now.tables.is_empty() && shown_now.text.is_empty());
}

#[gpui::test]
fn an_empty_cell_takes_typing(cx: &mut TestAppContext) {
    let mut layer = chrome_table();
    // Without its "200", the first item's quantity cell is empty.
    layer.lines.retain(|line| line.text.trim() != "200");
    let table = page_tables(&layer).remove(0);
    let (workspace, document_id, _shown, cx) = table_workspace(cx, layer);
    let (left, right) = (table.columns[1], table.columns[2]);
    let (top, bottom) = (table.rows[1], table.rows[2]);
    click_page_point(cx, document_id, ((left + right) / 2., (top + bottom) / 2.));
    type_text(cx, "Nuts");
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert_eq!(snapshot.page_table_edits.len(), 1);
    let texts = &snapshot.page_table_edits[0].cell_texts;
    assert_eq!(texts.len(), 1, "{texts:#?}");
    assert_eq!((texts[0].row, texts[0].column), (1, 1));
    assert_eq!(texts[0].text(), "Nuts");
    // In the table's own size on the page: Chrome draws 14.66 point text
    // through a 0.75 matrix.
    let size = texts[0].lines[0].runs[0].style.font_size;
    assert!((size - 14.66 * 0.75).abs() < 0.01, "{size}");
    assert!(snapshot.page_text_edits.is_empty());
    // Clicking it again reopens its text.
    let place = texts[0].lines[0].clone();
    click_page_point(
        cx,
        document_id,
        (left + place.x + 1., top - place.baseline + 2.),
    );
    type_text(cx, "!");
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    let text = snapshot.page_table_edits[0].cell_texts[0].text();
    assert!(text == "Nuts!" || text == "!Nuts", "{text}");
}

#[gpui::test]
fn tab_moves_between_cells_and_stays_in_the_last(cx: &mut TestAppContext) {
    let layer = chrome_table();
    let item = line_end(&layer, "Item");
    let last = line_end(&layer, "$88.00");
    let (workspace, document_id, shown, cx) = table_workspace(cx, layer);
    // Tab from "Item" selects "Qty": typing replaces it.
    click_page_point(cx, document_id, item);
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    type_text(cx, "Count");
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    let qty = snapshot
        .page_text_edits
        .iter()
        .find(|edit| edit.original_text() == "Qty")
        .unwrap_or_else(|| panic!("{:#?}", snapshot.page_text_edits));
    assert_eq!(qty.text(), "Count");
    // Tab in the last cell keeps editing it: the table gains no row.
    click_page_point(cx, document_id, last);
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    type_text(cx, "!");
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let edits = shown.lock().unwrap().last().cloned().unwrap();
    assert!(edits.tables.iter().all(|table| table.rows.len() == 4));
    assert!(edits.tables.iter().all(|table| table.cell_texts.is_empty()));
    let price = edits
        .text
        .iter()
        .find(|edit| edit.original_text() == "$88.00")
        .unwrap_or_else(|| panic!("{:#?}", edits.text));
    assert_eq!(price.text(), "$88.00!");
}

/// A table grown into the paragraph below it says so in a label, rather
/// than being outlined; the paragraph stays where it is.
#[gpui::test]
fn a_table_grown_into_the_text_below_is_labelled(cx: &mut TestAppContext) {
    let layer = chrome_table();
    let brackets = line_end(&layer, "Brackets");
    let delivery = line_end(
        &layer,
        "Delivery is expected within two weeks of the order date.",
    );
    let (workspace, document_id, shown, cx) = table_workspace(cx, layer);
    // Laid out as pages, which the labels are drawn over.
    workspace.update(cx, |workspace, cx| {
        workspace
            .refresh_viewport_async(document_id, 900., 1000., 1., cx)
            .unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    // Whether the label shows, under the table's bottom as shown now.
    let label = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx.debug_bounds("content-warning-Overlaps")?;
        let live = shown.lock().unwrap().last().cloned().unwrap();
        let rows = live.tables.first()?.output_rows();
        let bottom = window_point(cx, document_id, (100., *rows.last()?));
        assert!(
            bounds.top() >= bottom.y,
            "the label {bounds:?} is under the table's bottom {bottom:?}"
        );
        Some(bounds)
    };
    click_page_point(cx, document_id, brackets);
    assert!(label(cx).is_none());
    for _ in 0..4 {
        cx.simulate_keystrokes("enter");
        type_text(cx, "and more brackets");
    }
    cx.run_until_parked();
    assert!(
        label(cx).is_some(),
        "editing, the table runs into the text below"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(label(cx).is_some(), "and still does once the edit is done");
    let edits = shown.lock().unwrap().last().cloned().unwrap();
    assert!(edits.tables[0].growth() > 20., "{:#?}", edits.tables);

    // The covered line's edge, inside the table, still drags it: past its
    // end, where the table's cell would otherwise take the press.
    let (end_x, end_y) = delivery;
    let start = window_point(cx, document_id, (end_x, end_y - 3.));
    let start = point(start.x + px(7.), start.y);
    let end = point(start.x, start.y + px(200.));
    cx.simulate_mouse_move(start, None, Modifiers::default());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
    for step in 1..=20 {
        let at = point(start.x, start.y + px(10. * step as f32));
        cx.simulate_mouse_move(at, Some(gpui::MouseButton::Left), Modifiers::default());
    }
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert_eq!(snapshot.page_content_moves.len(), 1, "the line moved");
    assert!(
        label(cx).is_none(),
        "moved out of the way, nothing is overlapped"
    );
}

#[path = "support/table_pages.rs"]
mod table_pages;

/// Times the app's side of typing in a growing table cell on a heavy page
/// (a ruled table over 40,000 line segments): the cell laid out, its row
/// fitted and the page's outlines and controls drawn, per keystroke. The
/// worker's side is `table_typing_latency` in the worker. Prints the times;
/// run with `--nocapture`, in release.
#[gpui::test]
#[ignore = "a measurement, not a check"]
fn table_typing_latency_in_the_app(cx: &mut TestAppContext) {
    let bytes = table_pages::table_pages_pdf(1, 38, 40_000);
    let document = lopdf::Document::load_mem(&bytes).unwrap();
    let started = std::time::Instant::now();
    let layer = extract_page_text(&document, 0, &TextEditLimits::default()).unwrap();
    let read = started.elapsed();
    let started = std::time::Instant::now();
    let tables = page_tables(&layer);
    let found = started.elapsed();
    assert_eq!(tables.len(), 1);
    let cell = line_end(&layer, "Row 36 cell 1");
    let (_, document_id, shown, cx) = table_workspace(cx, layer);
    click_page_point(cx, document_id, cell);
    let mut times = Vec::new();
    for keystroke in 0..36 {
        let started = std::time::Instant::now();
        type_text(cx, if keystroke % 6 == 5 { " " } else { "x" });
        times.push(started.elapsed());
    }
    let live = shown.lock().unwrap().last().cloned().unwrap();
    assert_eq!(live.tables.len(), 1, "the row grows");
    times.sort();
    eprintln!(
        "page text read in {:.1} ms, tables found in {:.1} ms; per keystroke: median {:.1} ms, worst {:.1} ms",
        read.as_secs_f64() * 1000.,
        found.as_secs_f64() * 1000.,
        times[times.len() / 2].as_secs_f64() * 1000.,
        times[times.len() - 1].as_secs_f64() * 1000.
    );
}
