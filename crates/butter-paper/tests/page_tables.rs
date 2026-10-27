//! Tables in real producer output (`tests/fixtures/text-edit/*-table.pdf`):
//! they are found with their text in cells, and table edits move rows and
//! columns as the edit says, read back from the written page.

use butter_paper::pdf_content_moves::{ContentKind, ContentMove};
use butter_paper::pdf_tables::{BandSource, PageTable, PageTableEdit, TableBand, page_tables};
use butter_paper::pdf_text_edit::{
    PageTextLayer, TextEditLimits, apply_content_edits, extract_page_text,
};
use lopdf::Document;

const PRODUCERS: [&str; 2] = ["chrome-table", "libreoffice-table"];

fn producer(name: &str) -> Document {
    let path = format!(
        "{}/tests/fixtures/text-edit/{name}.pdf",
        env!("CARGO_MANIFEST_DIR")
    );
    Document::load(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn layer(document: &Document) -> PageTextLayer {
    extract_page_text(document, 0, &TextEditLimits::default()).expect("extract")
}

fn reload(document: &mut Document) -> Document {
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("save");
    Document::load_mem(&bytes).expect("reload")
}

fn cell_text(layer: &PageTextLayer, table: &PageTable, cell: usize) -> String {
    table.cells[cell]
        .lines
        .iter()
        .filter_map(|anchor| layer.line(anchor))
        .map(|line| line.text.trim().to_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn texts(layer: &PageTextLayer, table: &PageTable) -> Vec<String> {
    (0..table.cells.len())
        .map(|cell| cell_text(layer, table, cell))
        .collect()
}

const CELLS: [&str; 12] = [
    "Item",
    "Qty",
    "Price",
    "Bolts, M8 zinc-plated",
    "200",
    "$14.00",
    "Washers",
    "500",
    "$6.50",
    "Brackets",
    "40",
    "$88.00",
];

#[test]
fn producer_tables_are_found_with_their_text_in_cells() {
    for name in PRODUCERS {
        let document = producer(name);
        let layer = layer(&document);
        let tables = page_tables(&layer);
        assert_eq!(tables.len(), 1, "{name}: {tables:?}");
        let table = &tables[0];
        assert_eq!(
            (table.row_count(), table.column_count()),
            (4, 3),
            "{name}: {table:?}"
        );
        assert_eq!(texts(&layer, table), CELLS, "{name}");
        assert!(table.read_only.is_none(), "{name}: {:?}", table.read_only);
    }
}

/// The page's lines, by text, with their origins.
fn origins(layer: &PageTextLayer) -> Vec<(String, f64, f64)> {
    layer
        .lines
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .map(|line| {
            let (x, y) = line.origin();
            (line.text.trim().to_owned(), x, y)
        })
        .collect()
}

fn origin_of(origins: &[(String, f64, f64)], text: &str) -> (f64, f64) {
    origins
        .iter()
        .find(|(line, _, _)| line == text)
        .map(|(_, x, y)| (*x, *y))
        .unwrap_or_else(|| panic!("no line {text:?} in {origins:?}"))
}

/// Moves of the text below `table` down by `by`, as a user clears room for
/// a growing table: content below a table never moves by itself.
fn moved_below(layer: &PageTextLayer, table: &PageTable, by: f64) -> Vec<ContentMove> {
    let bottom = table.rows[table.rows.len() - 1];
    layer
        .lines
        .iter()
        .filter_map(butter_paper::pdf_tables::line_core_box)
        .filter(|core| core[3] < bottom)
        .map(|rect| ContentMove {
            page_index: 0,
            kind: ContentKind::Text,
            rect,
            offset: [0., -by],
            scale: [1., 1.],
            removed: false,
            copy: None,
        })
        .collect()
}

#[test]
fn a_grown_row_moves_the_rows_below_and_their_borders() {
    for name in PRODUCERS {
        let mut document = producer(name);
        let before = layer(&document);
        let table = page_tables(&before).remove(0);
        let mut edit = PageTableEdit::unchanged(0, &table);
        edit.rows[1].size += 15.;
        // The text after the table, moved down out of its way.
        apply_content_edits(
            &mut document,
            &[],
            &[edit.clone()],
            &moved_below(&before, &table, 15.),
            &TextEditLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        let mut document = reload(&mut document);
        let after = layer(&document);
        let (old, new) = (origins(&before), origins(&after));
        for text in ["Item", "Bolts, M8 zinc-plated", "$14.00"] {
            assert_eq!(
                origin_of(&old, text),
                origin_of(&new, text),
                "{name}: {text}"
            );
        }
        let after_table = "Delivery is expected within two weeks of the order date.";
        for text in ["Washers", "500", "Brackets", "$88.00", after_table] {
            let (x0, y0) = origin_of(&old, text);
            let (x1, y1) = origin_of(&new, text);
            assert!((x1 - x0).abs() < 0.01, "{name}: {text} {x0} {x1}");
            assert!((y0 - y1 - 15.).abs() < 0.01, "{name}: {text} {y0} {y1}");
        }
        // Text before the table stays where it was.
        let before_table = "Order summary for the site works.";
        assert_eq!(
            origin_of(&old, before_table),
            origin_of(&new, before_table),
            "{name}"
        );
        // The written page's table has the grown row, its text still in cells.
        let tables = page_tables(&after);
        assert_eq!(tables.len(), 1, "{name}");
        let grown = &tables[0];
        assert_eq!(texts(&after, grown), CELLS, "{name}");
        let expected = edit.output_rows();
        for (row, (found, wanted)) in grown.rows.iter().zip(&expected).enumerate() {
            assert!(
                (found - wanted).abs() < 0.6,
                "{name}: row {row} {found} {wanted}"
            );
        }
        // Editing again from the written page keeps the text where it is.
        let unchanged = PageTableEdit::unchanged(0, grown);
        apply_content_edits(
            &mut document,
            &[],
            &[unchanged],
            &[],
            &TextEditLimits::default(),
        )
        .unwrap();
        assert_eq!(origins(&layer(&document)), new, "{name}");
    }
}

#[test]
fn deleted_and_new_rows_and_columns_read_back() {
    for name in PRODUCERS {
        let mut document = producer(name);
        let before = layer(&document);
        let table = page_tables(&before).remove(0);
        let mut edit = PageTableEdit::unchanged(0, &table);
        // Delete "Washers"; add a row like "Bolts" at the end; swap the
        // last two columns; add a column like "Qty" at the end.
        let washers = edit.rows.remove(2);
        edit.rows.push(TableBand {
            source: BandSource::New(1),
            size: washers.size,
            set_size: None,
        });
        edit.columns.swap(1, 2);
        let qty = edit.columns[2];
        edit.columns.push(TableBand {
            source: BandSource::New(1),
            size: qty.size,
            set_size: None,
        });
        apply_content_edits(
            &mut document,
            &[],
            &[edit.clone()],
            &[],
            &TextEditLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        let document = reload(&mut document);
        let after = layer(&document);
        let tables = page_tables(&after);
        assert_eq!(tables.len(), 1, "{name}: {:?}", after.rules.len());
        let edited = &tables[0];
        assert_eq!(
            (edited.row_count(), edited.column_count()),
            (4, 4),
            "{name}: {edited:?}"
        );
        assert_eq!(
            texts(&after, edited),
            [
                "Item",
                "Price",
                "Qty",
                "",
                "Bolts, M8 zinc-plated",
                "$14.00",
                "200",
                "",
                "Brackets",
                "$88.00",
                "40",
                "",
                "",
                "",
                "",
                ""
            ],
            "{name}"
        );
        assert!(
            !origins(&after).iter().any(|(text, _, _)| text == "Washers"),
            "{name}"
        );
        for (found, wanted) in edited.columns.iter().zip(edit.output_columns()) {
            assert!((found - wanted).abs() < 0.6, "{name}: {found} {wanted}");
        }
    }
}

/// The operands and operators of the first clip a page's content sets.
fn first_clip(document: &Document) -> Vec<String> {
    let page = document.get_pages()[&1];
    let content = String::from_utf8_lossy(&document.get_page_content(page)).into_owned();
    let tokens = content.split_whitespace().collect::<Vec<_>>();
    let clip = tokens
        .iter()
        .position(|token| *token == "W" || *token == "W*")
        .expect("a clip");
    tokens[clip.saturating_sub(5)..=clip]
        .iter()
        .map(|token| token.to_string())
        .collect()
}

#[test]
fn deleting_the_first_row_keeps_the_tables_top_border_and_the_page_clip() {
    for name in PRODUCERS {
        let mut document = producer(name);
        let clip = first_clip(&document);
        let before = layer(&document);
        let table = page_tables(&before).remove(0);
        let mut edit = PageTableEdit::unchanged(0, &table);
        edit.rows.remove(0);
        apply_content_edits(
            &mut document,
            &[],
            &[edit.clone()],
            &[],
            &TextEditLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        let document = reload(&mut document);
        assert_eq!(first_clip(&document), clip, "{name}");
        let after = layer(&document);
        let tables = page_tables(&after);
        assert_eq!(tables.len(), 1, "{name}");
        let edited = &tables[0];
        // The rows below move up into the first row's place, under the
        // table's top border.
        assert_eq!(edited.row_count(), 3, "{name}: {edited:?}");
        assert!(
            (edited.rows[0] - table.rows[0]).abs() < 0.6,
            "{name}: {} {}",
            edited.rows[0],
            table.rows[0]
        );
        assert_eq!(texts(&after, edited), CELLS[3..], "{name}");
        let before_table = "Order summary for the site works.";
        assert_eq!(
            origin_of(&origins(&before), before_table),
            origin_of(&origins(&after), before_table),
            "{name}"
        );
    }
}

/// A table's part carried onto a new page with nothing in it (a "ghost"
/// row a page break left) can be deleted outright: its borders and text go,
/// and nothing of it is drawn.
#[test]
fn new_cell_text_is_drawn_in_its_cell() {
    use butter_paper::pdf_tables::{TableCellLine, TableCellText};
    use butter_paper::pdf_text_edit::TextEditRun;
    for name in PRODUCERS {
        let mut document = producer(name);
        let before = layer(&document);
        let table = page_tables(&before).remove(0);
        let model = before
            .line(&table.cells[9].lines[0])
            .expect("the Brackets line")
            .clone();
        let mut edit = PageTableEdit::unchanged(0, &table);
        let size = edit.rows[3].size;
        edit.rows.push(TableBand {
            source: BandSource::New(3),
            size,
            set_size: None,
        });
        // In the new row's first cell, in the table's font; "Ω" is drawn by
        // a bundled font.
        edit.cell_texts.push(TableCellText {
            row: 4,
            column: 0,
            lines: vec![TableCellLine {
                x: 4.,
                baseline: 12.,
                runs: vec![TextEditRun {
                    style: model.styles[0].clone(),
                    text: "Nuts Ω".into(),
                }],
                hard_break: false,
            }],
        });
        apply_content_edits(
            &mut document,
            &[],
            &[edit.clone()],
            &moved_below(&before, &table, size),
            &TextEditLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        let document = reload(&mut document);
        let after = layer(&document);
        let nuts = after
            .lines
            .iter()
            .find(|line| line.text.trim() == "Nuts Ω")
            .unwrap_or_else(|| panic!("{name}: {:?}", origins(&after)));
        let (x, y) = nuts.origin();
        let rows = edit.output_rows();
        assert!((x - (table.columns[0] + 4.)).abs() < 0.01, "{name}: {x}");
        assert!((y - (rows[4] - 12.)).abs() < 0.01, "{name}: {y}");
        // The written table has the new row, with the text in its cell.
        let tables = page_tables(&after);
        assert_eq!(tables.len(), 1, "{name}");
        assert_eq!(tables[0].row_count(), 5, "{name}");
        assert_eq!(cell_text(&after, &tables[0], 12), "Nuts Ω", "{name}");
    }
}

#[test]
fn selectable_text_follows_table_edits() {
    use butter_paper::pdf_tables::{TableCellLine, TableCellText};
    use butter_paper::pdf_text_blocks::paragraph_blocks;
    use butter_paper::pdf_text_edit::TextEditRun;
    use butter_paper::pdf_text_selection::selectable_lines_with_tables;
    let document = producer("chrome-table");
    let layer = layer(&document);
    let table = page_tables(&layer).remove(0);
    let blocks = paragraph_blocks(&layer);
    let mut edit = PageTableEdit::unchanged(0, &table);
    edit.rows[1].size += 15.;
    edit.rows.remove(3);
    edit.cell_texts.push(TableCellText {
        row: 0,
        column: 1,
        lines: vec![TableCellLine {
            x: 4.,
            baseline: 12.,
            runs: vec![TextEditRun {
                style: layer.lines[1].styles[0].clone(),
                text: "Count".into(),
            }],
            hard_break: false,
        }],
    });
    let lines = selectable_lines_with_tables(&layer, &blocks, &[], &[edit], &[]);
    let text = |line: &butter_paper::pdf_text_selection::SelectableLine| {
        line.chars
            .iter()
            .map(|(character, _, _)| *character)
            .collect::<String>()
    };
    let origin = |wanted: &str| {
        lines
            .iter()
            .find(|line| text(line).trim() == wanted)
            .map(|line| (line.line.matrix[4], line.line.matrix[5]))
    };
    let base = |wanted: &str| {
        layer
            .lines
            .iter()
            .find(|line| line.text.trim() == wanted)
            .map(|line| line.origin())
            .unwrap()
    };
    // A row below the grown one is selected where it is shown.
    let washers = origin("Washers").unwrap();
    assert!((base("Washers").1 - washers.1 - 15.).abs() < 0.01);
    // The deleted row's text is gone; the new cell text is selectable.
    assert!(origin("Brackets").is_none());
    assert!(origin("Count").is_some());
}

#[test]
fn undo_past_a_save_shows_the_page_as_the_earlier_file_had_it() {
    use butter_paper::pdf_page_base::{PageBase, restore_pages, snapshot_page};
    use std::sync::Arc;
    for name in PRODUCERS {
        let original = producer(name);
        let table = page_tables(&layer(&original)).remove(0);
        // Two states: the second row grown by 10, then by 25.
        let grown = |by: f64| {
            let mut edit = PageTableEdit::unchanged(0, &table);
            edit.rows[1].size += by;
            edit
        };
        let edited = |by: f64| {
            let mut document = original.clone();
            apply_content_edits(
                &mut document,
                &[],
                &[grown(by)],
                &[],
                &TextEditLimits::default(),
            )
            .unwrap();
            origins(&layer(&reload(&mut document)))
        };
        // Saving the second state keeps the page as the file had it.
        let kept = PageBase::new(0, Arc::new(snapshot_page(&original, 0).unwrap()));
        let mut saved = original.clone();
        apply_content_edits(
            &mut saved,
            &[],
            &[grown(25.)],
            &[],
            &TextEditLimits::default(),
        )
        .unwrap();
        let saved = reload(&mut saved);
        assert_eq!(origins(&layer(&saved)), edited(25.), "{name}");

        // Undo: the first state's edit applies to the kept page.
        let mut undone = saved.clone();
        restore_pages(&mut undone, std::slice::from_ref(&kept)).unwrap();
        assert_eq!(
            origins(&layer(&undone)),
            origins(&layer(&original)),
            "{name}"
        );
        apply_content_edits(
            &mut undone,
            &[],
            &[grown(10.)],
            &[],
            &TextEditLimits::default(),
        )
        .unwrap();
        let undone = reload(&mut undone);
        assert_eq!(origins(&layer(&undone)), edited(10.), "{name}");
        // And back to the file as it was opened.
        let mut first = saved.clone();
        restore_pages(&mut first, std::slice::from_ref(&kept)).unwrap();
        let first = reload(&mut first);
        assert_eq!(
            origins(&layer(&first)),
            origins(&layer(&original)),
            "{name}"
        );
        assert_eq!(page_tables(&layer(&first)).len(), 1, "{name}");
    }
}

#[test]
fn a_table_among_a_drawings_line_work_is_found() {
    // 40,000 short strokes, hundreds of them axis-aligned and touching the
    // table's borders.
    let bytes = table_pages::table_pages_pdf(1, 38, 40_000);
    let document = Document::load_mem(&bytes).unwrap();
    let layer = layer(&document);
    let tables = page_tables(&layer);
    assert_eq!(tables.len(), 1);
    let table = &tables[0];
    assert_eq!((table.row_count(), table.column_count()), (38, 4));
    assert_eq!(cell_text(&layer, table, 0), "Row 0 cell 0");
    assert_eq!(cell_text(&layer, table, 38 * 4 - 1), "Row 37 cell 3");
}

#[path = "support/table_pages.rs"]
mod table_pages;
