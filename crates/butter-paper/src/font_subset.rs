//! Subsets a TrueType (`glyf`) font program for embedding in a PDF, keeping
//! glyph ids: unused glyphs become empty, so a `CIDToGIDMap` written for the
//! whole font stays valid. Tables PDF viewers do not use are dropped.

use std::collections::BTreeSet;

/// Tables kept in a subset, in tag order. A CIDFontType2 program is reached
/// through `CIDToGIDMap`, so `cmap` and the layout tables are not needed.
const KEPT_TABLES: [&[u8; 4]; 11] = [
    b"OS/2", b"cvt ", b"fpgm", b"gasp", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp",
    b"prep",
];

struct Table<'a> {
    tag: [u8; 4],
    data: &'a [u8],
}

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

/// The tables of the face whose table directory starts at `directory`
/// (0 for a single font; the face's offset in a collection). Table offsets
/// are from the start of the file either way.
fn tables(data: &[u8], directory: usize) -> Option<Vec<Table<'_>>> {
    let count = usize::from(u16_at(data, directory + 4)?);
    (0..count)
        .map(|index| {
            let record = directory + 12 + index * 16;
            let tag = data.get(record..record + 4)?.try_into().ok()?;
            let offset = u32_at(data, record + 8)? as usize;
            let length = u32_at(data, record + 12)? as usize;
            Some(Table {
                tag,
                data: data.get(offset..offset.checked_add(length)?)?,
            })
        })
        .collect()
}

/// Each glyph's byte range in `glyf`.
fn glyph_ranges(head: &[u8], loca: &[u8], glyph_count: usize) -> Option<Vec<(usize, usize)>> {
    let long = u16_at(head, 50)? == 1;
    let offset = |index: usize| -> Option<usize> {
        if long {
            u32_at(loca, index * 4).map(|value| value as usize)
        } else {
            u16_at(loca, index * 2).map(|value| usize::from(value) * 2)
        }
    };
    (0..glyph_count)
        .map(|glyph| Some((offset(glyph)?, offset(glyph + 1)?)))
        .collect()
}

/// The glyphs a composite glyph is built from.
fn components(glyph: &[u8]) -> Option<Vec<u16>> {
    const ARGS_ARE_WORDS: u16 = 0x0001;
    const HAS_SCALE: u16 = 0x0008;
    const MORE_COMPONENTS: u16 = 0x0020;
    const HAS_XY_SCALE: u16 = 0x0040;
    const HAS_TWO_BY_TWO: u16 = 0x0080;
    if glyph.len() < 10 || i16::from_be_bytes([glyph[0], glyph[1]]) >= 0 {
        return Some(Vec::new());
    }
    let mut found = Vec::new();
    let mut offset = 10;
    loop {
        let flags = u16_at(glyph, offset)?;
        found.push(u16_at(glyph, offset + 2)?);
        offset += 4 + if flags & ARGS_ARE_WORDS != 0 { 4 } else { 2 };
        offset += if flags & HAS_SCALE != 0 {
            2
        } else if flags & HAS_XY_SCALE != 0 {
            4
        } else if flags & HAS_TWO_BY_TWO != 0 {
            8
        } else {
            0
        };
        if flags & MORE_COMPONENTS == 0 {
            return Some(found);
        }
    }
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// A subset of face `face` of `data` (a font collection, `ttcf`, or with
/// face 0 a single font) that draws `glyphs` (and glyph 0) with their
/// original ids, as a single font; `None` when it is not a TrueType font
/// this can subset.
pub(crate) fn subset_truetype_face(
    data: &[u8],
    face: u32,
    glyphs: &BTreeSet<u16>,
) -> Option<Vec<u8>> {
    let directory = if data.get(..4)? == b"ttcf" {
        if face >= u32_at(data, 8)? {
            return None;
        }
        u32_at(data, 12 + face as usize * 4)? as usize
    } else if face == 0 {
        0
    } else {
        return None;
    };
    if u32_at(data, directory)? != 0x0001_0000 {
        return None;
    }
    let tables = tables(data, directory)?;
    let table = |tag: &[u8; 4]| {
        tables
            .iter()
            .find(|table| &table.tag == tag)
            .map(|table| table.data)
    };
    let (head, loca, glyf) = (table(b"head")?, table(b"loca")?, table(b"glyf")?);
    let glyph_count = usize::from(u16_at(table(b"maxp")?, 4)?);
    let ranges = glyph_ranges(head, loca, glyph_count)?;
    let glyph_data = |glyph: u16| {
        let (start, end) = *ranges.get(usize::from(glyph))?;
        glyf.get(start..end)
    };

    // Keep the requested glyphs, .notdef and every component they use.
    let mut kept = BTreeSet::from([0u16]);
    let mut pending = glyphs.iter().copied().collect::<Vec<_>>();
    while let Some(glyph) = pending.pop() {
        if usize::from(glyph) >= glyph_count || !kept.insert(glyph) && glyph != 0 {
            continue;
        }
        pending.extend(components(glyph_data(glyph)?)?);
    }

    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::with_capacity((glyph_count + 1) * 4);
    for glyph in 0..glyph_count {
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        if kept.contains(&(glyph as u16)) {
            new_glyf.extend_from_slice(glyph_data(glyph as u16)?);
            new_glyf.resize(new_glyf.len().next_multiple_of(4), 0);
        }
    }
    new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
    let mut new_head = head.to_vec();
    // Long offsets, and the whole-font checksum is set below.
    new_head
        .get_mut(50..52)?
        .copy_from_slice(&1u16.to_be_bytes());
    new_head
        .get_mut(8..12)?
        .copy_from_slice(&0u32.to_be_bytes());

    let mut output_tables = KEPT_TABLES
        .iter()
        .filter_map(|tag| {
            let data = match *tag {
                b"glyf" => new_glyf.clone(),
                b"loca" => new_loca.clone(),
                b"head" => new_head.clone(),
                _ => table(tag)?.to_vec(),
            };
            Some((**tag, data))
        })
        .collect::<Vec<_>>();
    output_tables.sort_by_key(|(tag, _)| *tag);

    let count = output_tables.len() as u16;
    let entry_selector = 15 - count.leading_zeros() as u16;
    let search_range = (1u16 << entry_selector) * 16;
    let mut font = Vec::new();
    font.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    for value in [
        count,
        search_range,
        entry_selector,
        count * 16 - search_range,
    ] {
        font.extend_from_slice(&value.to_be_bytes());
    }
    let mut offset = 12 + 16 * output_tables.len();
    let mut head_offset = 0;
    for (tag, data) in &output_tables {
        if tag == b"head" {
            head_offset = offset;
        }
        font.extend_from_slice(tag);
        font.extend_from_slice(&checksum(data).to_be_bytes());
        font.extend_from_slice(&(offset as u32).to_be_bytes());
        font.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += data.len().next_multiple_of(4);
    }
    for (_, data) in &output_tables {
        font.extend_from_slice(data);
        font.resize(font.len().next_multiple_of(4), 0);
    }
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&font));
    font.get_mut(head_offset + 8..head_offset + 12)?
        .copy_from_slice(&adjustment.to_be_bytes());
    Some(font)
}

/// The six-letter tag a subset's PostScript name begins with ("ABCDEF+"),
/// derived from the glyphs it keeps.
pub(crate) fn subset_tag(glyphs: &BTreeSet<u16>) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for glyph in glyphs {
        for byte in glyph.to_be_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    (0..6)
        .map(|_| {
            let letter = char::from(b'A' + (hash % 26) as u8);
            hash /= 26;
            letter
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARIMO: &[u8] = include_bytes!("../assets/fonts/Arimo-Regular.ttf");
    const TINOS_BOLD: &[u8] = include_bytes!("../assets/fonts/Tinos-Bold.ttf");
    const NOTO_SC: &[u8] = include_bytes!("../assets/fonts/NotoSansSC-Regular.ttf");

    fn glyph_ids(font: &[u8], text: &str) -> BTreeSet<u16> {
        let face = ttf_parser::Face::parse(font, 0).unwrap();
        text.chars()
            .map(|character| face.glyph_index(character).unwrap().0)
            .collect()
    }

    struct Outline(usize);

    impl ttf_parser::OutlineBuilder for Outline {
        fn move_to(&mut self, _: f32, _: f32) {
            self.0 += 1;
        }
        fn line_to(&mut self, _: f32, _: f32) {
            self.0 += 1;
        }
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
            self.0 += 1;
        }
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
            self.0 += 1;
        }
        fn close(&mut self) {}
    }

    fn outline(font: &[u8], glyph: u16) -> Option<ttf_parser::Rect> {
        let face = ttf_parser::Face::parse(font, 0).unwrap();
        face.outline_glyph(ttf_parser::GlyphId(glyph), &mut Outline(0))
    }

    #[test]
    fn subsets_keep_the_used_glyphs_with_their_ids_and_metrics() {
        for (font, text) in [
            (ARIMO, "Hello, wörld Ж"),
            (TINOS_BOLD, "Structural steel Ω"),
            (NOTO_SC, "钢结构 steel"),
        ] {
            let glyphs = glyph_ids(font, text);
            let subset = subset_truetype_face(font, 0, &glyphs).unwrap();
            assert!(
                subset.len() * 10 < font.len(),
                "{} of {} bytes",
                subset.len(),
                font.len()
            );
            let original = ttf_parser::Face::parse(font, 0).unwrap();
            let face = ttf_parser::Face::parse(&subset, 0).unwrap();
            assert_eq!(face.number_of_glyphs(), original.number_of_glyphs());
            assert_eq!(face.units_per_em(), original.units_per_em());
            for glyph in &glyphs {
                let id = ttf_parser::GlyphId(*glyph);
                assert_eq!(
                    outline(&subset, *glyph),
                    outline(font, *glyph),
                    "glyph {glyph}"
                );
                assert_eq!(face.glyph_hor_advance(id), original.glyph_hor_advance(id));
            }
            // Glyphs not asked for are empty.
            let unused = (1..original.number_of_glyphs())
                .find(|glyph| !glyphs.contains(glyph) && outline(font, *glyph).is_some())
                .unwrap();
            assert_eq!(outline(&subset, unused), None);
            // The whole-font checksum is valid.
            assert_eq!(checksum(&subset), 0xB1B0_AFBA);
        }
    }

    #[test]
    fn composite_glyphs_keep_their_components() {
        // Accented letters in Arimo are composites of a base and a mark.
        let glyphs = glyph_ids(ARIMO, "ö");
        let subset = subset_truetype_face(ARIMO, 0, &glyphs).unwrap();
        let glyph = *glyphs.first().unwrap();
        assert!(outline(&subset, glyph).is_some());
        assert_eq!(outline(&subset, glyph), outline(ARIMO, glyph));
    }

    #[test]
    fn subset_tags_are_stable_letters() {
        let glyphs = BTreeSet::from([3, 40, 41]);
        let tag = subset_tag(&glyphs);
        assert_eq!(tag.len(), 6);
        assert!(tag.chars().all(|letter| letter.is_ascii_uppercase()));
        assert_eq!(tag, subset_tag(&glyphs));
        assert_ne!(tag, subset_tag(&BTreeSet::from([3, 40])));
    }
}
