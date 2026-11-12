//! Page content lifted off its page. A clip is a one-page PDF drawing only
//! the content selected in Edit PDF mode, in its own colours, line widths
//! and fonts, with nothing else on the page: it is the picture a drag
//! shows, and what Paste writes into another page or PDF.
//!
//! The page's content is kept in order, its state operators and all, and
//! only what paints something not selected is dropped, so what is left
//! draws exactly as the page drew it. Clips stay, as they clip what is
//! kept. A paste writes the clip's content after the target page's own, its
//! resources joining the page's, so what it draws is page content to
//! select, move and edit, as the parts of an exploded block are.

use super::*;
use crate::pdf_content_moves::{ContentExplode, ContentKind, ContentMove, Placement};

/// How far past the content's bounds a clip's page reaches, in points, so
/// that wide strokes and mitred corners are not cut off.
const CLIP_MARGIN: f64 = 24.;

/// Selected content as a one-page PDF.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentClip {
    pub pdf: Vec<u8>,
    /// The bounds of the content, in the coordinates of its page and of
    /// the PDF's page, without the margin.
    pub bounds: [f64; 4],
}

impl ContentClip {
    /// The bounds of the PDF's page, in the coordinates of the content's.
    pub fn page_rect(&self) -> [f64; 4] {
        let [left, bottom, right, top] = self.bounds;
        [
            left - CLIP_MARGIN,
            bottom - CLIP_MARGIN,
            right + CLIP_MARGIN,
            top + CLIP_MARGIN,
        ]
    }
}

/// What a kept content unit draws: each placement it is drawn at (a copy
/// is drawn again).
type Placements = Vec<(Placement, [f64; 4])>;

/// Which content of the page is kept, and where.
#[derive(Default)]
struct Kept {
    /// By paint: whole paths, by `None`, and pieces of stroked ones.
    paints: HashMap<usize, Vec<(Option<usize>, Placement, [f64; 4])>>,
    draws: HashMap<usize, Placements>,
    /// By the `q` opening the block, the `Q` closing it and placements.
    groups: BTreeMap<usize, (usize, Matrix, Placements)>,
    /// By the operation of the show operator.
    shows: HashMap<usize, (usize, Placements)>,
}

/// Which of the page's content `items` names.
fn kept_content(
    page: &ExtractedPage,
    page_box: Option<[f64; 4]>,
    items: &[ContentMove],
) -> Result<(Kept, [f64; 4]), TextEditError> {
    let units = page.units(page_box);
    let ordinals = unit_ordinals(&units);
    let mut kept = Kept::default();
    let mut bounds: Option<[f64; 4]> = None;
    for item in items {
        let placement = item.placement();
        let mut found = false;
        if item.kind == ContentKind::Text {
            for line in &page.lines {
                let Some(rect) = crate::pdf_tables::line_core_box(&line.public) else {
                    continue;
                };
                if !item.names(ContentKind::Text, rect) {
                    continue;
                }
                found = true;
                for &show in &line.ops {
                    let op_index = page.shows[show].op_index;
                    if matches!(
                        page.owners[page.operations[op_index].stream],
                        StreamOwner::Page
                    ) {
                        kept.shows
                            .entry(op_index)
                            .or_insert_with(|| (show, Vec::new()))
                            .1
                            .push((placement, item.rect));
                    }
                }
                break;
            }
        } else if let Some(unit) =
            units
                .iter()
                .zip(&ordinals)
                .find_map(|((unit, rect), (_, nth))| {
                    (unit.kind() == item.kind && item.names_nth(item.kind, *rect, *nth))
                        .then_some(*unit)
                })
        {
            found = true;
            match unit {
                ContentUnit::Paint(index) => kept
                    .paints
                    .entry(index)
                    .or_default()
                    .push((None, placement, item.rect)),
                ContentUnit::Piece(index, part) => {
                    kept.paints
                        .entry(index)
                        .or_default()
                        .push((Some(part), placement, item.rect))
                }
                ContentUnit::Draw(index) => kept
                    .draws
                    .entry(index)
                    .or_default()
                    .push((placement, item.rect)),
                ContentUnit::Group(clip) => {
                    let (start, end) =
                        enclosing_block(page, page.paints[clip].paint).ok_or_else(|| {
                            TextEditError::ReadOnly("the drawing cannot be copied".into())
                        })?;
                    let outer = page.saves.get(&start).copied().unwrap_or(IDENTITY);
                    kept.groups
                        .entry(start)
                        .or_insert_with(|| (end, outer, Vec::new()))
                        .2
                        .push((placement, item.rect));
                }
            }
        }
        if found {
            let shown = placement.shown(item.rect);
            bounds = Some(match bounds {
                Some(bounds) => [
                    bounds[0].min(shown[0]),
                    bounds[1].min(shown[1]),
                    bounds[2].max(shown[2]),
                    bounds[3].max(shown[3]),
                ],
                None => shown,
            });
        }
    }
    let bounds = bounds.ok_or_else(|| {
        TextEditError::Stale("the selected content is no longer on the page".into())
    })?;
    Ok((kept, bounds))
}

/// The matrix making page-space `change` in the user space of `ctm`.
fn user_change(ctm: Matrix, change: Matrix) -> Result<Matrix, TextEditError> {
    let inverse =
        invert(ctm).ok_or_else(|| TextEditError::ReadOnly("the copy cannot be placed".into()))?;
    Ok(multiply(multiply(ctm, change), inverse))
}

/// The bytes of `op` as the page has them.
fn op_bytes(page: &ExtractedPage, op: usize) -> Result<&[u8], TextEditError> {
    let operation = &page.operations[op];
    page.streams[operation.stream]
        .get(operation.start..operation.end)
        .filter(|_| !operation.spans_streams)
        .ok_or_else(|| TextEditError::ReadOnly("the content runs across streams".into()))
}

/// `Tj` for the string a show operator draws, in place of `'` and `"`,
/// which also move to the next line: the show's own matrix has done that.
fn show_operator(page: &ExtractedPage, show: &ShowOp) -> Result<Vec<u8>, TextEditError> {
    let operation = &page.operations[show.op_index];
    let bytes = op_bytes(page, show.op_index)?;
    let mut out = Vec::new();
    match show.kind {
        ShowKind::Show | ShowKind::ShowArray => out.extend_from_slice(bytes),
        ShowKind::NextLineShow => {
            out.extend_from_slice(&bytes[..bytes.len().saturating_sub(1)]);
            out.extend_from_slice(b" Tj");
        }
        ShowKind::SpacedNextLineShow { word, character } => {
            out.extend_from_slice(
                format!(
                    "{} Tw {} Tc ",
                    format_number(word),
                    format_number(character)
                )
                .as_bytes(),
            );
            let Some(Operand::Str(text)) = page.operations.operands_of(operation).last() else {
                return Err(TextEditError::ReadOnly("the text cannot be copied".into()));
            };
            out.push(b'<');
            for byte in text {
                out.extend_from_slice(format!("{byte:02x}").as_bytes());
            }
            out.extend_from_slice(b"> Tj");
        }
    }
    Ok(out)
}

/// The page's content with only what `kept` keeps painted, each where its
/// placements put it, and the resource names it uses.
fn kept_stream(
    page: &ExtractedPage,
    page_box: Option<[f64; 4]>,
    kept: &Kept,
) -> Result<(Vec<u8>, BTreeSet<Vec<u8>>), TextEditError> {
    let mut path_owner = HashMap::new();
    let mut paint_at = HashMap::new();
    for (index, paint) in page.paints.iter().enumerate() {
        paint_at.insert(paint.paint, index);
        for &op in &paint.ops {
            path_owner.insert(op, index);
        }
    }
    let draw_at = page
        .draws
        .iter()
        .enumerate()
        .map(|(index, draw)| (draw.op, index))
        .collect::<HashMap<_, _>>();
    let mut out = Vec::new();
    let mut names = BTreeSet::new();
    let note_names = |names: &mut BTreeSet<Vec<u8>>, op: usize| {
        for operand in page.operations.operands(op) {
            if let Operand::Name(name) = operand {
                names.insert(name.clone());
            }
        }
    };
    let mut depth = 0_i64;
    let mut lowest = 0_i64;
    let mut op = 0;
    while op < page.operations.len() {
        let operation = &page.operations[op];
        if !matches!(page.owners[operation.stream], StreamOwner::Page) {
            op += 1;
            continue;
        }
        // A small clip and all it clips, whole.
        if let Some((end, outer, placements)) = kept.groups.get(&op) {
            let (first, last) = (&page.operations[op], &page.operations[*end]);
            let block = page.streams[first.stream]
                .get(first.start..last.end)
                .ok_or_else(|| TextEditError::ReadOnly("the drawing cannot be copied".into()))?;
            for index in op..=*end {
                note_names(&mut names, index);
            }
            for (placement, rect) in placements {
                let change = placement.matrix(*rect);
                out.extend_from_slice(
                    format!(" q {} cm ", format_matrix(user_change(*outer, change)?)).as_bytes(),
                );
                out.extend_from_slice(block);
                out.extend_from_slice(b" Q\n");
            }
            op = end + 1;
            continue;
        }
        let operator = operation.operator.as_slice();
        // A paint: what is kept, or a clip it sets, or nothing.
        if let Some(&index) = paint_at.get(&op) {
            let paint = &page.paints[index];
            if let Some(pieces) = kept.paints.get(&index) {
                let parts = pieces
                    .iter()
                    .any(|(part, _, _)| part.is_some())
                    .then(|| page.path_parts(paint, page_box))
                    .flatten();
                for (part, placement, rect) in pieces {
                    let (ops, paint_operator): (&[usize], &[u8]) = match part {
                        Some(part) => {
                            let Some(piece) = parts.as_ref().and_then(|parts| parts.get(*part))
                            else {
                                continue;
                            };
                            (&piece.ops, b"S")
                        }
                        None => (&paint.ops, op_bytes(page, op)?),
                    };
                    out.extend_from_slice(
                        format!(
                            " q {} cm ",
                            format_matrix(user_change(paint.ctm, placement.matrix(*rect))?)
                        )
                        .as_bytes(),
                    );
                    for &path_op in ops {
                        out.extend_from_slice(op_bytes(page, path_op)?);
                        out.push(b' ');
                    }
                    out.extend_from_slice(paint_operator);
                    out.extend_from_slice(b" Q\n");
                }
            }
            if paint.clips {
                out.extend_from_slice(b" n\n");
            }
            op += 1;
            continue;
        }
        // A path's construction: only a clip's stays.
        if let Some(&index) = path_owner.get(&op) {
            if page.paints[index].clips {
                out.extend_from_slice(op_bytes(page, op)?);
                out.push(b'\n');
            }
            op += 1;
            continue;
        }
        if let Some(&index) = draw_at.get(&op) {
            if let Some(placements) = kept.draws.get(&index) {
                note_names(&mut names, op);
                let draw = &page.draws[index];
                for (placement, rect) in placements {
                    out.extend_from_slice(
                        format!(
                            " q {} cm ",
                            format_matrix(user_change(draw.ctm, placement.matrix(*rect))?)
                        )
                        .as_bytes(),
                    );
                    out.extend_from_slice(op_bytes(page, op)?);
                    out.extend_from_slice(b" Q\n");
                }
            }
            op += 1;
            continue;
        }
        match operator {
            b"Tj" | b"TJ" | b"'" | b"\"" => {
                if let Some((show, placements)) = kept.shows.get(&op) {
                    note_names(&mut names, op);
                    let show = &page.shows[*show];
                    let drawn = show_operator(page, show)?;
                    for (placement, rect) in placements {
                        if *placement == Placement::UNMOVED {
                            out.extend_from_slice(
                                format!(" {} Tm ", format_matrix(show.text_matrix)).as_bytes(),
                            );
                            out.extend_from_slice(&drawn);
                            out.push(b'\n');
                        } else {
                            let change = user_change(show.state.ctm, placement.matrix(*rect))?;
                            out.extend_from_slice(
                                format!(
                                    " ET q {} cm BT {} Tm ",
                                    format_matrix(change),
                                    format_matrix(show.text_matrix)
                                )
                                .as_bytes(),
                            );
                            out.extend_from_slice(&drawn);
                            out.extend_from_slice(
                                format!(" ET Q BT {} Tm\n", format_matrix(show.line_matrix))
                                    .as_bytes(),
                            );
                        }
                    }
                } else if operator == b"\"" {
                    // Its spacing lasts, as kept text after it is spaced so.
                    if let [Operand::Number(word), Operand::Number(character), ..] =
                        page.operations.operands_of(operation)
                    {
                        out.extend_from_slice(
                            format!(
                                " {} Tw {} Tc\n",
                                format_number(*word),
                                format_number(*character)
                            )
                            .as_bytes(),
                        );
                    }
                }
            }
            // What paints and is not kept.
            b"Do" | b"sh" | b"BI" | b"n" | b"f" | b"F" | b"f*" | b"S" | b"s" | b"B" | b"B*"
            | b"b" | b"b*" | b"m" | b"l" | b"c" | b"v" | b"y" | b"h" | b"re" => {}
            _ => {
                match operator {
                    b"q" => depth += 1,
                    b"Q" => {
                        depth -= 1;
                        lowest = lowest.min(depth);
                    }
                    _ => {}
                }
                note_names(&mut names, op);
                out.extend_from_slice(op_bytes(page, op)?);
                out.push(b'\n');
            }
        }
        op += 1;
    }
    // Balanced, so what follows it is not changed by what it left open.
    let mut balanced = b"q\n".repeat(usize::try_from(-lowest).unwrap_or(0));
    balanced.append(&mut out);
    balanced.extend_from_slice(&b"Q\n".repeat(usize::try_from(depth - lowest).unwrap_or(0)));
    Ok((balanced, names))
}

/// The one page of `document` as a document of its own, so that pastes can
/// be written into it to read it without copying the whole file.
fn single_page(document: &Document, page_index: u32) -> Result<Document, TextEditError> {
    crate::pdf_page_base::page_as_document(document, page_index).map_err(TextEditError::Malformed)
}

/// `document`'s page `page_index`, or a one-page copy of it with the
/// pastes of `explodes` written in, and the page it is there.
pub(super) fn with_pastes<'a>(
    document: &'a Document,
    page_index: u32,
    explodes: &[&ContentExplode],
    limits: &TextEditLimits,
) -> Result<(std::borrow::Cow<'a, Document>, u32), TextEditError> {
    if explodes.iter().all(|explode| explode.paste.is_none()) {
        return Ok((std::borrow::Cow::Borrowed(document), page_index));
    }
    let mut single = single_page(document, page_index)?;
    let page = page_id(&single, 0)?;
    write_pastes(&mut single, page, explodes, limits)?;
    Ok((std::borrow::Cow::Owned(single), 0))
}

/// The content `items` names on page `page_index` as a one-page PDF,
/// placed as they place it. It is read from the page as `explodes` leaves
/// it, which is what the items name.
pub fn content_clip(
    document: &Document,
    page_index: u32,
    items: &[ContentMove],
    explodes: &[ContentExplode],
    limits: &TextEditLimits,
) -> Result<ContentClip, TextEditError> {
    let explodes = explodes
        .iter()
        .filter(|explode| explode.page_index == page_index)
        .collect::<Vec<_>>();
    let (document, page_index) = with_pastes(document, page_index, &explodes, limits)?;
    let document = document.as_ref();
    let page_id = super::page_id(document, page_index)?;
    let page_box = visible_box_of_page(document, page_id);
    let (page, current) = explode::extract_exploded(document, page_id, &explodes, limits, None)?;
    let (kept, bounds) = kept_content(&page, page_box, items)?;
    let (content, names) = kept_stream(&page, page_box, &kept)?;

    let resources = match &current {
        Some(current) => current.resources.clone(),
        None => page_resources(document, page_id).unwrap_or_default(),
    };
    let mut used = Dictionary::new();
    for category in explode::CATEGORIES {
        let Some(entries) = get_dict(document, &resources, category) else {
            continue;
        };
        let mut chosen = Dictionary::new();
        for (name, value) in entries.iter() {
            if names.contains(name) {
                chosen.set(name.clone(), value.clone());
            }
        }
        if !chosen.is_empty() {
            used.set(category, Object::Dictionary(chosen));
        }
    }
    let mut clip = Document::with_version("1.7");
    let pages_id = clip.new_object_id();
    let mut copier = crate::pdf_page_base::Copier::new(document);
    let resources = copier.copy(&mut clip, &Object::Dictionary(used), 0);
    let mut stream = Stream::new(Dictionary::new(), content);
    stream.compress()?;
    let contents = clip.add_object(stream);
    let [left, bottom, right, top] = bounds;
    let page_ref = clip.add_object(lopdf::dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![
            Object::Real((left - CLIP_MARGIN) as f32),
            Object::Real((bottom - CLIP_MARGIN) as f32),
            Object::Real((right + CLIP_MARGIN) as f32),
            Object::Real((top + CLIP_MARGIN) as f32),
        ],
        "Resources" => resources,
        "Contents" => contents,
    });
    clip.objects.insert(
        pages_id,
        Object::Dictionary(lopdf::dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_ref.into()],
            "Count" => 1,
        }),
    );
    let catalog = clip.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    clip.trailer.set("Root", catalog);
    let mut pdf = Vec::new();
    clip.save_to(&mut pdf).map_err(|error| {
        TextEditError::Malformed(format!("the copy cannot be written: {error}"))
    })?;
    Ok(ContentClip { pdf, bounds })
}

/// Writes the pastes of `explodes` after page `page_id`'s own content.
pub(super) fn write_pastes(
    document: &mut Document,
    page_id: ObjectId,
    explodes: &[&ContentExplode],
    limits: &TextEditLimits,
) -> Result<(), TextEditError> {
    let pastes = explodes
        .iter()
        .filter_map(|explode| explode.paste.as_ref())
        .collect::<Vec<_>>();
    if pastes.is_empty() {
        return Ok(());
    }
    let cannot = |why: String| TextEditError::Malformed(format!("pasted content: {why}"));
    let mut decoded = 0;
    let streams = decode_streams(
        document,
        &page_content_ids(document, page_id)?,
        &mut decoded,
        limits,
    )?;
    // The page's own content, closed, so that nothing it leaves set
    // changes what is pasted.
    let mut content = b"q\n".to_vec();
    for stream in &streams {
        content.extend_from_slice(stream);
        content.push(b'\n');
    }
    let operations = parse_operations(&streams, limits)?;
    let (mut depth, mut lowest) = (0_i64, 0_i64);
    for operation in operations.iter() {
        match operation.operator.as_slice() {
            b"q" => depth += 1,
            b"Q" => {
                depth -= 1;
                lowest = lowest.min(depth);
            }
            _ => {}
        }
    }
    if lowest < 0 {
        let mut opened = b"q\n".repeat(usize::try_from(-lowest).unwrap_or(0));
        opened.append(&mut content);
        content = opened;
    }
    content.extend_from_slice(&b"Q\n".repeat(usize::try_from(depth - lowest + 1).unwrap_or(1)));

    let mut resources = page_resources(document, page_id).unwrap_or_default();
    // A clip pasted twice shares its images and fonts in the file.
    let mut copied: HashMap<&str, (Dictionary, Vec<u8>)> = HashMap::new();
    for (index, paste) in pastes.iter().enumerate() {
        if !copied.contains_key(paste.id.as_str()) {
            let pdf = paste
                .pdf
                .as_ref()
                .ok_or_else(|| cannot("its PDF is missing".into()))?;
            let clip = Document::load_mem(pdf).map_err(|error| cannot(error.to_string()))?;
            let clip_page = super::page_id(&clip, 0)?;
            let mut clip_decoded = 0;
            let mut clip_content = Vec::new();
            for stream in decode_streams(
                &clip,
                &page_content_ids(&clip, clip_page)?,
                &mut clip_decoded,
                limits,
            )? {
                clip_content.extend_from_slice(&stream);
                clip_content.push(b'\n');
            }
            let clip_resources = page_resources(&clip, clip_page).unwrap_or_default();
            let mut copier = crate::pdf_page_base::Copier::new(&clip);
            let Object::Dictionary(own) =
                copier.copy(document, &Object::Dictionary(clip_resources), 0)
            else {
                return Err(cannot("its resources cannot be copied".into()));
            };
            copied.insert(paste.id.as_str(), (own, clip_content));
        }
        let (own, clip_content) = &copied[paste.id.as_str()];
        let renames = explode::merge_resources(document, &mut resources, own, index as u32 + 1);
        let clip_content = explode::renamed_content(clip_content, &renames)?;
        content.extend_from_slice(format!("q {} cm\n", format_matrix(paste.matrix)).as_bytes());
        content.extend_from_slice(&clip_content);
        content.extend_from_slice(b"\nQ\n");
    }
    explode::write(
        document,
        page_id,
        PageOverride { content, resources },
        limits,
    )
}
