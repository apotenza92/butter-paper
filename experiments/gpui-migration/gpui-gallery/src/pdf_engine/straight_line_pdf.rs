use super::{
    Dictionary, Document, LineKind, Object, PdfPersistenceError, Stream,
    StraightLineAnnotation, canonical_native_annotation_name, color_array, color_components,
    pdf_literal, pdf_rect, preserve_annotation_metadata, rectangle_dash_pattern, rect_bbox,
};
use crate::annotation_model::{straight_line_arrowhead_points, straight_line_painted_bounds};
use lopdf::dictionary;

const ANTIALIAS_ALLOWANCE_PT: f64 = 1.;

pub(super) fn rebuild_managed(
    document: &mut Document,
    annotation: &StraightLineAnnotation,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let bounds = straight_line_painted_bounds(annotation, ANTIALIAS_ALLOWANCE_PT).ok_or_else(|| {
        PdfPersistenceError::InvalidDocument(format!(
            "straight line {} has invalid painted geometry",
            annotation.id,
        ))
    })?;
    let local = |point: crate::annotation_model::PdfPoint| {
        (point.x - bounds.x, point.y - bounds.y)
    };
    let (start_x, start_y) = local(annotation.start);
    let (end_x, end_y) = local(annotation.end);
    let appearance = &annotation.appearance;
    let (red, green, blue) = color_components(appearance.stroke_color());
    let dash = rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
        .map_or_else(String::new, |(dash, gap)| {
            format!("[{dash:.6} {gap:.6}] 0 d\n")
        });
    let fill_color = (annotation.kind == LineKind::Arrow)
        .then(|| format!("{red:.6} {green:.6} {blue:.6} rg\n"))
        .unwrap_or_default();
    let mut content = format!(
        "q\n/GS0 gs\n1 J 1 j\n{red:.6} {green:.6} {blue:.6} RG\n{fill_color}{dash}{:.6} w\n{start_x:.6} {start_y:.6} m {end_x:.6} {end_y:.6} l S\n",
        appearance.stroke_width_pt(),
    );
    if annotation.kind == LineKind::Arrow {
        let points = straight_line_arrowhead_points(
            annotation.start,
            annotation.end,
            appearance.stroke_width_pt(),
        )
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "straight line {} has invalid Arrow geometry",
                annotation.id,
            ))
        })?
        .map(local);
        content.push_str(&format!(
            "[] 0 d\n{:.6} {:.6} m {:.6} {:.6} l {:.6} {:.6} l h B\n",
            points[0].0,
            points[0].1,
            points[1].0,
            points[1].1,
            points[2].0,
            points[2].1,
        ));
    }
    content.push_str("Q\n");
    let appearance_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Matrix" => vec![1.into(), 0.into(), 0.into(), 1.into(), 0.into(), 0.into()],
            "Resources" => dictionary! {
                "ExtGState" => dictionary! {
                    "GS0" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(appearance.opacity() as f32),
                        "ca" => Object::Real(appearance.opacity() as f32),
                    },
                },
            },
        },
        content.into_bytes(),
    ));

    let mut border_style = dictionary! {
        "Type" => "Border",
        "W" => Object::Real(appearance.stroke_width_pt() as f32),
        "S" => "S",
    };
    if let Some((dash, gap)) =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
    {
        border_style.set("S", Object::Name(b"D".to_vec()));
        border_style.set(
            "D",
            vec![Object::Real(dash as f32), Object::Real(gap as f32)],
        );
    }
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "Rect" => pdf_rect(bounds),
        "NM" => pdf_literal(&canonical_native_annotation_name(&annotation.id)),
        "Subj" => pdf_literal(match annotation.kind { LineKind::Line => "Line", LineKind::Arrow => "Arrow" }),
        "Contents" => pdf_literal(""),
        "F" => 4,
        "L" => vec![
            Object::Real(annotation.start.x as f32),
            Object::Real(annotation.start.y as f32),
            Object::Real(annotation.end.x as f32),
            Object::Real(annotation.end.y as f32),
        ],
        "Border" => vec![0.into(), 0.into(), Object::Real(appearance.stroke_width_pt() as f32)],
        "BS" => border_style,
        "C" => color_array(appearance.stroke_color()),
        "CA" => Object::Real(appearance.opacity() as f32),
        "ca" => Object::Real(appearance.opacity() as f32),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if annotation.kind == LineKind::Arrow {
        replacement.set("IT", Object::Name(b"LineArrow".to_vec()));
        replacement.set(
            "LE",
            vec![Object::Name(b"None".to_vec()), Object::Name(b"ClosedArrow".to_vec())],
        );
        replacement.set("IC", color_array(appearance.stroke_color()));
    }
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    for key in [b"Subj".as_slice(), b"Contents".as_slice()] {
        if let Ok(value) = original.get(key) {
            replacement.set(key, value.clone());
        }
    }
    Ok(replacement)
}
