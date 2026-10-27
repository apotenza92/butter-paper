//! A generated PDF for timing table editing: pages of ruled tables over a
//! drawing's worth of line work. Shared by tests through `#[path]`.

/// A PDF of `pages` pages, each a ruled table of `rows` rows and four
/// columns with text in every cell, over `paths` short line segments
/// (a drawing's worth of content).
pub fn table_pages_pdf(pages: usize, rows: usize, paths: usize) -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let mut kids = Vec::new();
    for page in 0..pages {
        let mut content = String::new();
        // The drawing: deterministic scattered segments in grey.
        content.push_str("q 0.8 G 0.3 w\n");
        let mut seed = 12345_u64 + page as u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) % 10_000) as f64 / 10_000.
        };
        for _ in 0..paths {
            let (x, y) = (20. + 570. * next(), 20. + 750. * next());
            content.push_str(&format!(
                "{x:.2} {y:.2} m {:.2} {:.2} l S\n",
                x + 8. * next(),
                y + 8. * next()
            ));
        }
        content.push_str("Q\n");
        let (left, right, top, row_height) = (60., 552., 740., 18.);
        let columns = [left, 200., 320., 440., right];
        let bottom = top - row_height * rows as f64;
        content.push_str("0 G 0.75 w\n");
        for row in 0..=rows {
            let y = top - row_height * row as f64;
            content.push_str(&format!("{left} {y} m {right} {y} l S\n"));
        }
        for x in columns {
            content.push_str(&format!("{x} {top} m {x} {bottom} l S\n"));
        }
        for row in 0..rows {
            let y = top - row_height * row as f64 - 13.;
            for (column, x) in columns[..4].iter().enumerate() {
                content.push_str(&format!(
                    "BT /F1 10 Tf {} {y} Td (Row {row} cell {column}) Tj ET\n",
                    x + 4.
                ));
            }
        }
        let mut stream = Stream::new(dictionary! {}, content.into_bytes());
        stream.compress().unwrap();
        let content_id = document.add_object(stream);
        kids.push(Object::from(document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        })));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => pages as i64,
            "Kids" => kids,
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}
