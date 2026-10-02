use super::{
    Dictionary, Document, LineKind, Object, PdfPersistenceError, StraightLineAnnotation, Stream,
    color_array, color_components, inflate_rect, markup_border_style, page_space_matrix,
    pdf_literal, pdf_rect, preserve_annotation_metadata, preserve_markup_comment,
    rectangle_dash_pattern, set_markup_opacity, union_rect,
};
use crate::annotation_model::{
    PdfRect, straight_line_arrowhead_points, straight_line_painted_bounds,
};
use lopdf::dictionary;

const ANTIALIAS_ALLOWANCE_PT: f64 = 1.;
/// Revu pads a Line's `/Rect` by 5 pt plus half the stroke around `/L`.
const REVU_LINE_PADDING_PT: f64 = 5.;

pub(super) fn rebuild_managed(
    document: &mut Document,
    annotation: &StraightLineAnnotation,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let painted =
        straight_line_painted_bounds(annotation, ANTIALIAS_ALLOWANCE_PT).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "straight line {} has invalid painted geometry",
                annotation.id,
            ))
        })?;
    let appearance = &annotation.appearance;
    let endpoints = PdfRect::new(
        annotation.start.x.min(annotation.end.x),
        annotation.start.y.min(annotation.end.y),
        (annotation.start.x - annotation.end.x).abs(),
        (annotation.start.y - annotation.end.y).abs(),
    )?;
    let bounds = union_rect(
        inflate_rect(
            endpoints,
            REVU_LINE_PADDING_PT + appearance.stroke_width_pt() / 2.,
        ),
        painted,
    );
    let (red, green, blue) = color_components(appearance.stroke_color());
    let dash = rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
        .map_or_else(String::new, |(dash, gap)| {
            format!("[{dash:.6} {gap:.6}] 0 d\n")
        });
    let fill_color = (annotation.kind == LineKind::Arrow)
        .then(|| format!("{red:.6} {green:.6} {blue:.6} rg\n"))
        .unwrap_or_default();
    let translucent = appearance.opacity() < 1.;
    let graphics_state = if translucent { "/GS0 gs\n" } else { "" };
    let mut content = format!(
        "q\n{graphics_state}1 J 1 j\n{red:.6} {green:.6} {blue:.6} RG\n{fill_color}{dash}{:.6} w\n{:.6} {:.6} m {:.6} {:.6} l S\n",
        appearance.stroke_width_pt(),
        annotation.start.x,
        annotation.start.y,
        annotation.end.x,
        annotation.end.y,
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
        })?;
        content.push_str(&format!(
            "[] 0 d\n{:.6} {:.6} m {:.6} {:.6} l {:.6} {:.6} l h B\n",
            points[0].x, points[0].y, points[1].x, points[1].y, points[2].x, points[2].y,
        ));
    }
    content.push_str("Q\n");
    let mut resources = dictionary! { "ProcSet" => vec![Object::Name(b"PDF".to_vec())] };
    if translucent {
        resources.set(
            "ExtGState",
            dictionary! {
                "GS0" => dictionary! {
                    "Type" => "ExtGState",
                    "CA" => Object::Real(appearance.opacity() as f32),
                    "ca" => Object::Real(appearance.opacity() as f32),
                },
            },
        );
    }
    let appearance_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => pdf_rect(bounds),
            "Matrix" => page_space_matrix(bounds),
            "Resources" => resources,
        },
        content.into_bytes(),
    ));

    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "Rect" => pdf_rect(bounds),
        "NM" => pdf_literal(annotation.id.as_str()),
        "Subj" => pdf_literal(match annotation.kind { LineKind::Line => "Line", LineKind::Arrow => "Arrow" }),
        "L" => vec![
            Object::Real(annotation.start.x as f32),
            Object::Real(annotation.start.y as f32),
            Object::Real(annotation.end.x as f32),
            Object::Real(annotation.end.y as f32),
        ],
        "BS" => markup_border_style(appearance.stroke_width_pt(), appearance.stroke_style()),
        "C" => color_array(appearance.stroke_color()),
        "PitchRun" => 12,
        "SlopeType" => 0,
        "AP" => dictionary! { "N" => appearance_id },
    };
    if annotation.kind == LineKind::Arrow {
        replacement.set("IT", Object::Name(b"LineArrow".to_vec()));
        replacement.set(
            "LE",
            vec![
                Object::Name(b"None".to_vec()),
                Object::Name(b"ClosedArrow".to_vec()),
            ],
        );
        replacement.set("IC", color_array(appearance.stroke_color()));
    }
    set_markup_opacity(&mut replacement, appearance.opacity());
    preserve_markup_comment(&mut replacement, original);
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}
