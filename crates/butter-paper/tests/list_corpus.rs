//! Lists found in real producer output. The committed fixtures
//! (`tests/fixtures/text-edit/lists`) check the lists each page has. The
//! corpus report finds the lists on every page of every PDF under
//! `BP_LIST_CORPUS` (default `target/list-corpus` in the workspace) and
//! writes what it found to `report.tsv`, `runs.tsv` and `report.md` there,
//! for a person to check against the pages. That corpus is not in the
//! repository:
//!
//! `cargo test --release --test list_corpus -- --ignored --nocapture`

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use butter_paper::pdf_text_blocks::{list_item_marker, list_runs_report, paragraph_blocks};
use butter_paper::pdf_text_edit::{TextEditLimits, extract_page_text};
use butter_paper::pdf_text_lists::read_marker;

/// Pages read from each document, at most.
const MAX_PAGES: u32 = 40;

fn pdfs(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries = entries.flatten().map(|e| e.path()).collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            pdfs(&path, found);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        {
            found.push(path);
        }
    }
}

fn short(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.chars().take(70).collect()
}

#[test]
#[ignore = "reads the local list corpus"]
fn list_corpus_report() {
    let root = std::env::var_os("BP_LIST_CORPUS").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/list-corpus"),
        PathBuf::from,
    );
    let mut files = Vec::new();
    pdfs(&root, &mut files);
    assert!(!files.is_empty(), "no PDFs under {}", root.display());
    let mut tsv = String::from("file\tpage\tkind\tmarker\tlevels\titems\tlines\tsample\n");
    let mut summary = String::new();
    let mut runs = String::from("file\tpage\tconfidence\treason\titems\tfirst line\n");
    let (mut total_lists, mut total_items, mut total_plain) = (0, 0, 0);
    for file in &files {
        let name = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .display()
            .to_string();
        let Ok(document) = lopdf::Document::load(file) else {
            let _ = writeln!(summary, "| {name} | unreadable | | |");
            continue;
        };
        let pages = (document.get_pages().len() as u32).min(MAX_PAGES);
        let (mut lists, mut items, mut plain) = (0, 0, 0);
        for page in 0..pages {
            let Ok(layer) = extract_page_text(&document, page, &TextEditLimits::default()) else {
                continue;
            };
            let blocks = paragraph_blocks(&layer);
            if std::env::var("BP_LIST_DUMP")
                .is_ok_and(|dump| dump == format!("{name}:{}", page + 1))
            {
                for block in &blocks {
                    println!("BLOCK list={:?}", block.list.map(|list| list.marker));
                    for anchor in &block.lines {
                        let line = layer.line(anchor).unwrap();
                        println!(
                            "   {:?} {:?} {:?}",
                            line.origin(),
                            line.paragraph,
                            line.text
                        );
                    }
                }
            }
            for run in list_runs_report(&layer) {
                let _ = writeln!(runs, "{name}\t{}\t{run}", page + 1);
            }
            for block in &blocks {
                let texts = block
                    .lines
                    .iter()
                    .filter_map(|anchor| layer.line(anchor))
                    .map(|line| line.text.trim().to_owned())
                    .collect::<Vec<_>>();
                match &block.list {
                    Some(list) => {
                        let count = texts
                            .iter()
                            .filter(|text| {
                                list_item_marker(
                                    &text
                                        .chars()
                                        .map(|c| if c.is_whitespace() { ' ' } else { c })
                                        .collect::<Vec<_>>(),
                                    Some(list),
                                )
                                .is_some()
                            })
                            .count();
                        lists += 1;
                        items += count;
                        let _ = writeln!(
                            tsv,
                            "{name}\t{}\tlist\t{:?}\t{}\t{count}\t{}\t{}",
                            page + 1,
                            list.marker,
                            list.levels().len(),
                            texts.len(),
                            short(&texts.join(" | "))
                        );
                    }
                    None => {
                        // Lines starting as an item would, left as plain text.
                        for text in &texts {
                            let chars = text.chars().collect::<Vec<_>>();
                            if let Some(reading) = read_marker(&chars)
                                && chars.get(reading.len).is_some_and(|c| c.is_whitespace())
                            {
                                plain += 1;
                                let _ = writeln!(
                                    tsv,
                                    "{name}\t{}\tplain\t{:?}\t\t\t\t{}",
                                    page + 1,
                                    reading.marker,
                                    short(text)
                                );
                            }
                        }
                    }
                }
            }
        }
        total_lists += lists;
        total_items += items;
        total_plain += plain;
        let _ = writeln!(
            summary,
            "| {name} | {pages} | {lists} | {items} | {plain} |"
        );
    }
    let report = format!(
        "# List corpus report\n\n{} documents, {total_lists} lists, {total_items} items, \
         {total_plain} marker-like lines left plain.\n\n\
         | Document | Pages read | Lists | Items | Plain marker lines |\n| --- | --- | --- | --- | --- |\n{summary}",
        files.len()
    );
    std::fs::write(root.join("report.tsv"), tsv).unwrap();
    std::fs::write(root.join("report.md"), &report).unwrap();
    std::fs::write(root.join("runs.tsv"), runs).unwrap();
    println!("{report}");
}

/// Each list on `page` of fixture `name`: its marker, its levels and its
/// items.
fn fixture_lists(name: &str, page: u32) -> Vec<(String, usize, usize)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/text-edit/lists")
        .join(name);
    let document = lopdf::Document::load(&path).unwrap();
    let layer = extract_page_text(&document, page, &TextEditLimits::default()).unwrap();
    paragraph_blocks(&layer)
        .into_iter()
        .filter_map(|block| {
            let list = block.list?;
            let items = block
                .lines
                .iter()
                .filter_map(|anchor| layer.line(anchor))
                .filter(|line| {
                    let text = line
                        .text
                        .trim()
                        .chars()
                        .map(|c| if c.is_whitespace() { ' ' } else { c })
                        .collect::<Vec<_>>();
                    list_item_marker(&text, Some(&list)).is_some()
                })
                .count();
            Some((list.marker.text(1), list.levels().len(), items))
        })
        .collect()
}

#[test]
fn committed_fixtures_find_their_lists_and_leave_the_rest_plain() {
    let lists = |name, page| {
        fixture_lists(name, page)
            .into_iter()
            .map(|(marker, levels, items)| format!("{marker} {levels} {items}"))
            .collect::<Vec<_>>()
    };
    // Three lists; the numbered headings, the dash in prose and the dates
    // stay plain.
    assert_eq!(
        lists("lo-mixed.pdf", 0),
        ["\u{2022} 1 3", "1. 1 4", "\u{2013} 1 3"]
    );
    // Nested lists are one paragraph each, with their levels.
    assert_eq!(lists("lo-nested.pdf", 0), ["1. 5 21", "1. 3 16"]);
    assert_eq!(lists("lo-nested.pdf", 1), ["a. 2 5"]);
    // Word's Symbol and Wingdings bullets, roman numerals right-aligned.
    assert_eq!(
        lists("lo-docx-wordstyle.pdf", 0),
        [
            "\u{f0b7} 3 4",
            "1. 3 21",
            "1. 3 4",
            "\u{f0d8} 1 3",
            "\u{f0fc} 1 3"
        ]
    );
    assert_eq!(
        lists("lo-docx-wordstyle.pdf", 1),
        ["1) 1 4", "I. 1 4", "(1) 1 4"]
    );
    assert_eq!(
        lists("firefox-numbering-styles.pdf", 0),
        ["1) 1 4", "(1) 1 4", "a. 1 4", "a) 1 4", "A. 1 4", "i. 1 3"]
    );
}

/// A list edited, finished and opened again before a save reads back as it
/// was typed: items that moved to other lines keep their order.
#[test]
fn an_edited_list_reopens_with_its_items_in_order() {
    use butter_paper::page_text_editor::PageTextEditState;
    use butter_paper::pdf_text_reflow::Paragraph;
    use std::sync::Arc;
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-edit/lists/lo-mixed.pdf");
    let document = lopdf::Document::load(&path).unwrap();
    let layer = Arc::new(extract_page_text(&document, 0, &TextEditLimits::default()).unwrap());
    let block = paragraph_blocks(&layer)
        .into_iter()
        .find(|block| block.list.is_some_and(|list| list.marker.is_number()))
        .unwrap();
    let paragraph = Paragraph::new(layer.clone(), &block).unwrap();
    let mut state = PageTextEditState::new(paragraph.clone(), &[], &[], &[], 0).unwrap();
    let end = state.text().find("\n3.").unwrap();
    state.move_to(state.text()[..end].chars().count(), false);
    assert!(state.insert_newline());
    assert!(state.insert("Check the earth bond"));
    assert!(state.insert_newline());
    let text = state.text();
    let edits = state.edits();
    let edit_of = |anchor| edits.iter().find(|edit| edit.anchor == anchor);
    let current = block
        .lines
        .iter()
        .map(|anchor| {
            edit_of(*anchor).map_or_else(
                || layer.line(anchor).unwrap().original_runs(),
                |edit| edit.runs.clone(),
            )
        })
        .collect::<Vec<_>>();
    let breaks = block
        .lines
        .iter()
        .map(|anchor| edit_of(*anchor).map(|edit| edit.layout.hard_break))
        .collect::<Vec<_>>();
    let added = block
        .lines
        .last()
        .and_then(|anchor| edit_of(*anchor))
        .map(|edit| edit.layout.extra_lines.clone())
        .unwrap_or_default();
    let reopened = PageTextEditState::new(paragraph, &current, &breaks, &added, 0).unwrap();
    assert_eq!(reopened.text(), text);
}
