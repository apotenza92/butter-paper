//! Edit PDF Content on an untagged report laid out as Word lays one out
//! (`tests/fixtures/text-edit/word-table.pdf`): a heading block, then a
//! ruled two-column table whose right-hand cells hold wrapped paragraphs.
//! Its lines join into paragraphs where they wrap, never across a cell's
//! rules, and an edited cell paragraph rewraps inside its cell without
//! moving or covering other text.

use butter_paper::pdf_tables::line_box;
use butter_paper::pdf_text_blocks::{PageTextBlock, paragraph_blocks};
use butter_paper::pdf_text_edit::{
    PageTextLayer, TextEditLimits, apply_text_edits, extract_page_text,
};
use butter_paper::pdf_text_reflow::Paragraph;
use lopdf::Document;
use std::sync::Arc;

fn report() -> Document {
    let path = format!(
        "{}/tests/fixtures/text-edit/word-table.pdf",
        env!("CARGO_MANIFEST_DIR")
    );
    Document::load(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn layer(document: &Document) -> PageTextLayer {
    extract_page_text(document, 0, &TextEditLimits::default()).expect("extract")
}

fn texts(layer: &PageTextLayer, block: &PageTextBlock) -> Vec<String> {
    block
        .lines
        .iter()
        .map(|anchor| layer.line(anchor).unwrap().text.trim().to_owned())
        .collect()
}

/// Words separated by single spaces.
fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn untagged_lines_join_only_where_they_wrap() {
    let document = report();
    let layer = layer(&document);
    assert!(
        layer.lines.iter().all(|line| line.paragraph.is_none()),
        "the report is untagged"
    );
    let blocks = paragraph_blocks(&layer)
        .iter()
        .map(|block| texts(&layer, block))
        .map(|lines| normalise(&lines.join(" ")))
        .collect::<Vec<_>>();
    assert_eq!(
        blocks,
        [
            "Home Visit Assessment:",
            "Mr Sample Person",
            "12 Example Way, Testville Heights 3999",
            "Thank you for referring Sam for physiotherapy treatment at home.",
            "Home assessment",
            "Sam lives alone in a double storey house with level pathways to all rooms. \
             There are 3 steps between the living area and the shower. There are no \
             modifications to the home. There is a rail at the back door for access.",
            "Steps",
            "There are 2 steps at the front entrance and a staircase to the second level \
             where the bedroom is. Sam climbs them twice a day, morning and night. We \
             reviewed gait and use of the rail.",
            "Mobility and transfers",
            // Two paragraphs of one cell: the first ends short of the cell's
            // edge, where the next one's first word would have fitted.
            "Sam uses a single point stick to walk around the house and uses it out in \
             the community.",
            "Sam reports no falls in the past year.",
            "Please contact the clinic with any questions about this report.",
        ]
    );
    // A cell's paragraph wraps within the rules round it, inset as the text
    // is from the left rule.
    let cell = paragraph_blocks(&layer)
        .into_iter()
        .find(|block| texts(&layer, block)[0].starts_with("Sam lives alone"))
        .unwrap();
    let (left, right) = cell.ruled.expect("ruled round");
    assert!(
        (left - 189.75).abs() < 0.1 && (right - 533.15).abs() < 0.1,
        "{left} {right}"
    );
}

/// A user's reported edit: a word in the cell's third line changed and words
/// added. The paragraph rewraps across its lines inside the cell; every
/// other line keeps its text and place, and none is covered.
#[test]
fn an_edited_cell_paragraph_rewraps_in_its_cell_and_moves_nothing_else() {
    let document = report();
    let before = Arc::new(layer(&document));
    let block = paragraph_blocks(&before)
        .into_iter()
        .find(|block| texts(&before, block)[0].starts_with("Sam lives alone"))
        .unwrap();
    let (_, cell_right) = block.ruled.unwrap();
    let paragraph = Paragraph::new(before.clone(), &block).unwrap();
    let base = paragraph.text(&[], &[]);
    let joined = base
        .iter()
        .map(|(character, _)| *character)
        .collect::<String>();
    let from = joined.find("There are no").unwrap();
    let style = base[from].1.clone();
    let mut text = base[..from].to_vec();
    text.extend(
        "These are no recent"
            .chars()
            .map(|character| (character, style.clone())),
    );
    text.extend_from_slice(&base[from + "There are no".len()..]);
    let expected = normalise(
        &text
            .iter()
            .map(|(character, _)| *character)
            .collect::<String>(),
    );

    let wrapped = paragraph.wrap(&text).unwrap();
    let edits = paragraph.edits(&text, &wrapped);
    let mut edited = document.clone();
    apply_text_edits(&mut edited, &edits, &TextEditLimits::default()).unwrap();
    let mut bytes = Vec::new();
    edited.save_to(&mut bytes).unwrap();
    let after = layer(&Document::load_mem(&bytes).unwrap());

    // The cell's lines: those starting at its text edge between its rules.
    let in_cell = |line: &&butter_paper::pdf_text_edit::PageTextLine| {
        let (x, y) = line.origin();
        (x - 189.75).abs() < 0.5 && y < 672. && y > 616.2
    };
    let cell_lines = after.lines.iter().filter(in_cell).collect::<Vec<_>>();
    assert_eq!(
        normalise(
            &cell_lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        expected
    );
    assert_eq!(cell_lines.len(), 4, "the text still fits its four lines");
    for line in &cell_lines {
        let [_, _, right, _] = line_box(line).unwrap();
        assert!(right <= cell_right + 0.5, "{:?} ends at {right}", line.text);
    }

    // Every other line is where it was, as it was.
    let others = |layer: &PageTextLayer| {
        layer
            .lines
            .iter()
            .filter(|line| !in_cell(line) && !line.text.trim().is_empty())
            .map(|line| {
                let (x, y) = line.origin();
                (
                    line.text.trim().to_owned(),
                    (x * 100.).round(),
                    (y * 100.).round(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(others(&after), others(&before));

    // No edited line covers another line.
    for line in &cell_lines {
        let a = line_box(line).unwrap();
        for other in after.lines.iter().filter(|other| !in_cell(other)) {
            let Some(b) = line_box(other) else { continue };
            let overlaps = a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
            assert!(!overlaps, "{:?} covers {:?}", line.text, other.text);
        }
    }
}
