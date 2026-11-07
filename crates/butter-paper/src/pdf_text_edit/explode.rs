//! Explode, in Edit PDF mode: a Form XObject drawn on a page (a CAD block,
//! or a whole sheet wrapped in one) is written into the page's own content
//! where the page drew it, so what it draws is page content to select,
//! move and edit, as AutoCAD's Explode turns a block into its parts.
//!
//! Its `Do` becomes what drawing the form does: `q`, the form's matrix,
//! a clip to its bounding box, its content, `Q`, inside its optional
//! content and wherever its change had placed it. Its resources join the
//! page's; one whose name the page uses for something else is renamed, in
//! the operators that name it, and every other byte of its content is
//! kept. Forms it draws are left whole: they explode in turn.
//!
//! A form drawn once covering most of the page is a flattened sheet: it is
//! opened by itself, whenever the page is read for editing, so the sheet's
//! drawings are each their own. Nothing changes on the page until it is
//! edited: then the opened sheet is written with the other edits.

use super::*;
use crate::pdf_content_moves::{ContentExplode, Placement};

/// A page's content with forms written in: the content and resources read
/// in place of the page's own.
#[derive(Clone, Debug, PartialEq)]
pub struct PageOverride {
    pub content: Vec<u8>,
    pub resources: Dictionary,
}

/// How many forms deep a flattened sheet is opened.
const MAX_SHEET_DEPTH: usize = 4;

/// The resource categories a form's content names.
const CATEGORIES: [&[u8]; 7] = [
    b"Font",
    b"XObject",
    b"ExtGState",
    b"ColorSpace",
    b"Pattern",
    b"Shading",
    b"Properties",
];

/// Page `page_id` read with its flattened sheet opened and the forms
/// `explodes` names written in, in order (each names a form of the page as
/// the ones before leave it); and the content and resources that does it,
/// when anything is written in. An explode whose form is no longer on the
/// page is passed over.
pub(super) fn extract_exploded(
    document: &Document,
    page_id: ObjectId,
    explodes: &[&ContentExplode],
    limits: &TextEditLimits,
    fonts: Option<&FontCache>,
) -> Result<(ExtractedPage, Option<PageOverride>), TextEditError> {
    let page_box = visible_box_of_page(document, page_id);
    let mut current: Option<PageOverride> = None;
    let mut page = extract_page_at_with(document, page_id, limits, fonts, None)?;
    for _ in 0..MAX_SHEET_DEPTH {
        let Some(draw) = sheet_form(&page, page_box) else {
            break;
        };
        (page, current) = exploded(
            document,
            page_id,
            &page,
            current.as_ref(),
            draw,
            Placement::UNMOVED,
            limits,
            fonts,
        )?;
    }
    for explode in explodes {
        let units = page.units(page_box);
        let ordinals = unit_ordinals(&units);
        let found = units
            .iter()
            .zip(&ordinals)
            .find_map(|((unit, rect), (_, nth))| match unit {
                ContentUnit::Draw(index)
                    if page.draws[*index].form.is_some() && explode.names(*rect, *nth) =>
                {
                    Some(*index)
                }
                _ => None,
            });
        let Some(draw) = found else {
            continue;
        };
        (page, current) = exploded(
            document,
            page_id,
            &page,
            current.as_ref(),
            draw,
            explode.placement(),
            limits,
            fonts,
        )?;
    }
    Ok((page, current))
}

/// `page` with draw `index`'s form written in, read again. Its content is
/// written unclipped, as AutoCAD explodes a block, so its parts are each
/// their own rather than one clipped drawing; when it draws beyond its
/// bounding box (which clipped it), it is written clipped instead.
#[allow(clippy::too_many_arguments)]
fn exploded(
    document: &Document,
    page_id: ObjectId,
    page: &ExtractedPage,
    current: Option<&PageOverride>,
    index: usize,
    placement: Placement,
    limits: &TextEditLimits,
    fonts: Option<&FontCache>,
) -> Result<(ExtractedPage, Option<PageOverride>), TextEditError> {
    let bounds = page.draws[index].bounds;
    let (open, written) = inline_form(document, page_id, page, current, index, placement, false)?;
    let read = extract_page_at_with(document, page_id, limits, fonts, Some(&open))?;
    // Where the form drew is shown as the change placed it.
    let shown = placement.shown(bounds);
    let reach = 0.5;
    let inside = |rect: [f64; 4]| {
        rect[0] >= shown[0] - reach
            && rect[1] >= shown[1] - reach
            && rect[2] <= shown[2] + reach
            && rect[3] <= shown[3] + reach
    };
    let written_op = |op: usize| {
        let operation = &read.operations[op];
        operation.stream == 0 && written.contains(&operation.start)
    };
    let overflows = read
        .paints
        .iter()
        .filter(|paint| !paint.clips && (paint.stroked || paint.filled) && written_op(paint.paint))
        .filter_map(|paint| read.paint_bounds(paint))
        .chain(
            read.draws
                .iter()
                .filter(|draw| written_op(draw.op))
                .map(|draw| draw.bounds),
        )
        .any(|rect| !inside(rect));
    if !overflows {
        return Ok((read, Some(open)));
    }
    let (clipped, _) = inline_form(document, page_id, page, current, index, placement, true)?;
    let read = extract_page_at_with(document, page_id, limits, fonts, Some(&clipped))?;
    Ok((read, Some(clipped)))
}

/// Writes `with` as page `page_id`'s content and resources.
pub(super) fn write(
    document: &mut Document,
    page_id: ObjectId,
    with: PageOverride,
    limits: &TextEditLimits,
) -> Result<(), TextEditError> {
    let mut stream = Stream::new(Dictionary::new(), with.content);
    if limits.compress_streams {
        stream.compress()?;
    }
    let content = document.add_object(stream);
    let page = document.get_object_mut(page_id)?.as_dict_mut()?;
    page.set("Contents", Object::Reference(content));
    page.set("Resources", Object::Dictionary(with.resources));
    Ok(())
}

/// The form a flattened sheet is: drawn once by the page's own content,
/// outside a small clip, covering most of the page.
fn sheet_form(page: &ExtractedPage, page_box: Option<[f64; 4]>) -> Option<usize> {
    let area = |rect: [f64; 4]| (rect[2] - rect[0]).max(0.) * (rect[3] - rect[1]).max(0.);
    let page_area = area(page_box?);
    let mut uses: HashMap<ObjectId, usize> = HashMap::new();
    for form in page.draws.iter().filter_map(|draw| draw.form) {
        *uses.entry(form).or_insert(0) += 1;
    }
    page.draws.iter().position(|draw| {
        draw.form.is_some_and(|form| uses[&form] == 1)
            && page.local_clip(&draw.clip_chain, page_box).is_none()
            && area(draw.bounds) >= crate::pdf_content_moves::MAX_OBJECT_SHARE * page_area
    })
}

/// The page's content (as `page` read it, from `current` or its own) with
/// draw `index`'s form written in its place, placed by `placement` (how
/// its change showed it), clipped to its bounding box with `clip`, and the
/// page's resources with the form's; and where in the content the form was
/// written.
fn inline_form(
    document: &Document,
    page_id: ObjectId,
    page: &ExtractedPage,
    current: Option<&PageOverride>,
    index: usize,
    placement: Placement,
    clip: bool,
) -> Result<(PageOverride, std::ops::Range<usize>), TextEditError> {
    let cannot =
        |why: &str| TextEditError::ReadOnly(format!("the block cannot be exploded: {why}"));
    let draw = &page.draws[index];
    let operation = &page.operations[draw.op];
    if !matches!(page.owners[operation.stream], StreamOwner::Page) || operation.spans_streams {
        return Err(cannot("it is drawn across content streams"));
    }
    let form_id = draw.form.ok_or_else(|| cannot("it is not a form"))?;
    let form = document.get_object(form_id)?.as_stream()?;
    let mut decoded = 0;
    let content = decode_streams(
        document,
        &[form_id],
        &mut decoded,
        &TextEditLimits::default(),
    )?
    .pop()
    .unwrap_or_default();
    let mut resources = match current {
        Some(current) => current.resources.clone(),
        None => page_resources(document, page_id).unwrap_or_default(),
    };
    // Its resources join the page's, renamed where the page uses the name
    // for something else. A form without resources uses the page's.
    let mut renames: HashMap<&[u8], HashMap<Vec<u8>, Vec<u8>>> = HashMap::new();
    if let Some(own) = get_dict(document, &form.dict, b"Resources") {
        for category in CATEGORIES {
            let Some(entries) = get_dict(document, own, category) else {
                continue;
            };
            let mut merged = get_dict(document, &resources, category)
                .cloned()
                .unwrap_or_default();
            for (name, value) in entries.iter() {
                let taken = merged.get(name).ok();
                if taken.is_some_and(|existing| existing == value) {
                    continue;
                }
                let mut renamed = name.clone();
                if taken.is_some() {
                    let mut suffix = 0;
                    while merged.has(&renamed) {
                        suffix += 1;
                        renamed = format!(
                            "{}_x{}{}",
                            String::from_utf8_lossy(name),
                            form_id.0,
                            if suffix > 1 {
                                format!("_{suffix}")
                            } else {
                                String::new()
                            }
                        )
                        .into_bytes();
                    }
                    renames
                        .entry(category)
                        .or_default()
                        .insert(name.clone(), renamed.clone());
                }
                merged.set(renamed, value.clone());
            }
            resources.set(category, Object::Dictionary(merged));
        }
    }
    // The page no longer draws a form exploded where it drew it once: it
    // leaves the page's resources, and the file, rather than travel unused.
    let drawn_name = match operation.operands.last() {
        Some(Operand::Name(name)) => Some(name.clone()),
        _ => None,
    };
    if let Some(name) = drawn_name {
        let drawn_elsewhere = page.operations.iter().enumerate().any(|(op, other)| {
            op != draw.op
                && other.operator.as_slice() == b"Do"
                && matches!(page.owners[other.stream], StreamOwner::Page)
                && matches!(other.operands.last(), Some(Operand::Name(other)) if *other == name)
        });
        if !drawn_elsewhere
            && let Some(mut xobjects) = get_dict(document, &resources, b"XObject").cloned()
        {
            xobjects.remove(&name);
            resources.set("XObject", Object::Dictionary(xobjects));
        }
    }
    let content = renamed_content(&content, &renames).map_err(|error| match error {
        TextEditError::ReadOnly(why) => cannot(&why),
        other => other,
    })?;
    // What drawing the form does, where its change placed it.
    let numbers = |key: &[u8]| -> Option<Vec<f64>> {
        resolve(document, form.dict.get(key).ok()?)
            .ok()?
            .as_array()
            .ok()?
            .iter()
            .map(object_number)
            .collect()
    };
    let matrix = numbers(b"Matrix")
        .and_then(|values| <[f64; 6]>::try_from(values).ok())
        .unwrap_or(IDENTITY);
    let [x0, y0, x1, y1] = numbers(b"BBox")
        .and_then(|values| <[f64; 4]>::try_from(values).ok())
        .ok_or_else(|| cannot("it has no bounding box"))?;
    let mut text = String::new();
    let placed = placement != Placement::UNMOVED;
    if placed {
        let inverse = invert(draw.ctm).ok_or_else(|| cannot("it cannot be placed"))?;
        let page_change = placement.matrix(draw.bounds);
        text.push_str(&format!(
            "q {} cm ",
            format_matrix(multiply(multiply(draw.ctm, page_change), inverse))
        ));
    }
    // Its optional content still shows or hides it.
    let optional = form.dict.get(b"OC").ok().cloned();
    if let Some(optional) = &optional {
        let mut properties = get_dict(document, &resources, b"Properties")
            .cloned()
            .unwrap_or_default();
        let mut name = format!("oc_x{}", form_id.0).into_bytes();
        let mut suffix = 1;
        while properties
            .get(&name)
            .is_ok_and(|existing| existing != optional)
        {
            suffix += 1;
            name = format!("oc_x{}_{suffix}", form_id.0).into_bytes();
        }
        properties.set(name.clone(), optional.clone());
        resources.set("Properties", Object::Dictionary(properties));
        text.push_str(&format!("/OC {} BDC ", format_name(&name)));
    }
    text.push_str(&format!("q {} cm ", format_matrix(matrix)));
    if clip {
        text.push_str(&format!(
            "{} {} {} {} re W n",
            format_number(x0.min(x1)),
            format_number(y0.min(y1)),
            format_number((x1 - x0).abs()),
            format_number((y1 - y0).abs()),
        ));
    }
    text.push('\n');
    let mut replacement = text.into_bytes();
    replacement.extend_from_slice(&content);
    replacement.extend_from_slice(b"\nQ");
    if optional.is_some() {
        replacement.extend_from_slice(b" EMC");
    }
    if placed {
        replacement.extend_from_slice(b" Q");
    }
    // The page's own content, as one stream, with the form in its `Do`'s
    // place.
    let mut joined = Vec::new();
    let mut written = 0..0;
    for (stream, bytes) in page.streams.iter().enumerate() {
        if !matches!(page.owners[stream], StreamOwner::Page) {
            continue;
        }
        if stream == operation.stream {
            joined.extend_from_slice(&bytes[..operation.start]);
            joined.push(b'\n');
            written.start = joined.len();
            joined.extend_from_slice(&replacement);
            written.end = joined.len();
            joined.push(b'\n');
            joined.extend_from_slice(&bytes[operation.end..]);
        } else {
            joined.extend_from_slice(bytes);
        }
        joined.push(b'\n');
    }
    Ok((
        PageOverride {
            content: joined,
            resources,
        },
        written,
    ))
}

/// `content` with the resource names `renames` changes (by category)
/// changed in the operators that name them; every other byte is kept.
fn renamed_content(
    content: &[u8],
    renames: &HashMap<&[u8], HashMap<Vec<u8>, Vec<u8>>>,
) -> Result<Vec<u8>, TextEditError> {
    if renames.is_empty() {
        return Ok(content.to_vec());
    }
    let streams = vec![content.to_vec()];
    let operations = parse_operations(&streams, &TextEditLimits::default())?;
    let renamed = |category: &[u8], name: &[u8]| {
        renames
            .get(category)
            .and_then(|names| names.get(name))
            .cloned()
    };
    let mut splices = Vec::new();
    for operation in &operations {
        let operator = operation.operator.as_slice();
        // Which operand names a resource, of which category.
        let named = match operator {
            b"Tf" => Some((0, b"Font".as_slice())),
            b"Do" => Some((0, b"XObject".as_slice())),
            b"gs" => Some((0, b"ExtGState".as_slice())),
            b"cs" | b"CS" => Some((0, b"ColorSpace".as_slice())),
            b"sh" => Some((0, b"Shading".as_slice())),
            b"scn" | b"SCN" => operation
                .operands
                .len()
                .checked_sub(1)
                .map(|last| (last, b"Pattern".as_slice())),
            b"BDC" | b"DP" => Some((1, b"Properties".as_slice())),
            b"BI" => {
                // An inline image naming a renamed colour space cannot be
                // rewritten byte for byte.
                let names_renamed = operation.operands.iter().any(|operand| match operand {
                    Operand::Name(name) => renamed(b"ColorSpace", name).is_some(),
                    _ => false,
                });
                if names_renamed {
                    return Err(TextEditError::ReadOnly(
                        "an inline image uses a colour space the page names otherwise".into(),
                    ));
                }
                None
            }
            _ => None,
        };
        let Some((slot, category)) = named else {
            continue;
        };
        let Some(Operand::Name(name)) = operation.operands.get(slot) else {
            continue;
        };
        let Some(new_name) = renamed(category, name) else {
            continue;
        };
        // The operator rewritten: its names and numbers.
        let mut text = String::new();
        for (index, operand) in operation.operands.iter().enumerate() {
            match operand {
                Operand::Name(name) if index == slot => text.push_str(&format_name(&new_name)),
                Operand::Name(name) => text.push_str(&format_name(name)),
                Operand::Number(value) => push_number(&mut text, *value),
                _ => {
                    return Err(TextEditError::ReadOnly(
                        "an operator naming a renamed resource cannot be rewritten".into(),
                    ));
                }
            }
            text.push(' ');
        }
        text.push_str(&String::from_utf8_lossy(operator));
        splices.push((operation.start, operation.end, text));
    }
    let mut bytes = Vec::with_capacity(content.len() + 64);
    let mut cursor = 0;
    for (start, end, text) in splices {
        bytes.extend_from_slice(&content[cursor..start]);
        bytes.extend_from_slice(text.as_bytes());
        cursor = end;
    }
    bytes.extend_from_slice(&content[cursor..]);
    Ok(bytes)
}
