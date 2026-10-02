//! Native PDF output for every markup family, drawn at the same geometry as
//! the Bluebeam Revu reference specimens so dictionaries can be compared key
//! by key.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use butter_paper::annotation_model::{
    ArcAnnotation, CalloutAnnotation, CalloutAppearance, CloudAnnotation, CloudPlusAnnotation,
    CloudPlusAppearance, DecodedRgbaAsset, DimensionAnnotation, DimensionAppearance,
    EllipseAnnotation, ImageAnnotation, LengthAnnotation, LengthCalibration, LineKind, MarkupId,
    MeasurementPathAnnotation, MeasurementPathKind, PageScale, PdfPoint, PdfRect, PenAnnotation,
    PenAppearance, RectangleAnnotation, RectangleAppearance, ScalePrecision, ScaleSource,
    ScaleUnit, SnapshotAnnotation, StraightLineAnnotation, StraightLineAppearance, StrokeStyle,
    TextAlignment, TextBoxAnnotation, TextBoxStyle, VertexPathAnnotation, VertexPathKind,
};
use butter_paper::pdf_engine::PdfPersistenceSession;
use butter_paper::pdf_file_authority::SaveAsTargetAuthority;
use lopdf::dictionary;

fn id(value: &str) -> MarkupId {
    MarkupId::new(value).unwrap()
}

fn point(x: f64, y: f64) -> PdfPoint {
    PdfPoint::new(x, y).unwrap()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PdfRect {
    PdfRect::new(x0, y0, x1 - x0, y1 - y0).unwrap()
}

fn blank_letter_pdf(path: &Path) {
    let mut document = lopdf::Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let content_id = document.add_object(lopdf::Stream::new(dictionary! {}, Vec::new()));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
    });
    document.objects.insert(
        pages_id,
        lopdf::Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.save(path).unwrap();
}

/// Writes one markup of every family. Geometry mirrors the Revu specimens.
pub fn write_reference(source: &Path, target: &Path) {
    let red = "#ff0000";
    let shape = RectangleAppearance::new(red, 1., None::<String>, 1.).unwrap();
    let line = StraightLineAppearance::new(red, 1., 1., StrokeStyle::Solid).unwrap();
    // The app's text tools use Revu's 3 pt margin; captions are centred.
    let text = TextBoxStyle::new("Helvetica", 12., red, 1.)
        .unwrap()
        .with_layout_metrics(13.8, 3.)
        .unwrap();
    let caption = TextBoxStyle::new("Helvetica", 12., red, 1.)
        .unwrap()
        .with_weight_and_alignment(400, TextAlignment::Center)
        .unwrap();
    // Revu "1:100" metric preset: 1 cm on paper is 1 m, reported in mm.
    let calibration = LengthCalibration::from_scale(72. / 2.54, 1000., "mm", 2, true).unwrap();
    let asset = DecodedRgbaAsset::new(2, 2, vec![70, 130, 180, 255].repeat(4)).unwrap();

    let mut session = PdfPersistenceSession::open(source).unwrap();
    session
        .replace_page_scales(&[PageScale::from_factors(
            0,
            ScaleSource::Preset,
            "1:100",
            ScaleUnit::Cm,
            ScaleUnit::M,
            1. / (72. / 2.54),
            1. / (72. / 2.54),
            ScalePrecision::decimal(0.01).unwrap(),
        )
        .unwrap()])
        .unwrap();
    session
        .add_rectangle(RectangleAnnotation {
            id: id("rectangle"),
            page_index: 0,
            rect: rect(58.6721, 708.6731, 135.1331, 766.0969),
            rotation_degrees: 0.,
            appearance: shape.clone(),
            locked: false,
        })
        .unwrap();
    session
        .add_ellipse(
            EllipseAnnotation::new(
                id("ellipse"),
                0,
                rect(173.5196, 708.6731, 249.9806, 766.0969),
                shape.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_straight_line(
            StraightLineAnnotation::new(
                id("line"),
                0,
                point(288.3672, 766.0969),
                point(364.8282, 708.6731),
                LineKind::Line,
                line.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_straight_line(
            StraightLineAnnotation::new(
                id("arrow"),
                0,
                point(403.2147, 766.0969),
                point(479.6757, 708.6731),
                LineKind::Arrow,
                line.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_vertex_path(
            VertexPathAnnotation::new(
                id("polyline"),
                0,
                vec![
                    point(58.6721, 612.8628),
                    point(85.5115, 670.2866),
                    point(116.72, 612.8628),
                    point(135.1331, 670.2866),
                ],
                VertexPathKind::Polyline,
                shape.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_vertex_path(
            VertexPathAnnotation::new(
                id("polygon"),
                0,
                vec![
                    point(173.5196, 612.8628),
                    point(210.3457, 670.2866),
                    point(249.9806, 612.8628),
                ],
                VertexPathKind::Polygon,
                shape.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_cloud(
            CloudAnnotation::new(
                id("cloud"),
                0,
                vec![
                    point(288.3672, 670.2866),
                    point(364.8282, 670.2866),
                    point(364.8282, 612.8628),
                    point(288.3672, 612.8628),
                ],
                2.,
                shape.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_pen(
            PenAnnotation::new_paths(
                id("pen"),
                0,
                vec![vec![point(58.6665, 574.4732), point(135.1358, 517.0535)]],
                PenAppearance::new(red, 1., 1.).unwrap(),
                false,
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_pen(
            PenAnnotation::new_highlight(
                id("highlight"),
                0,
                vec![point(173.5181, 574.4732), point(249.9756, 517.0535)],
                PenAppearance::new("#ffff00", 12., 1.).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_text_box(
            TextBoxAnnotation::new(
                id("text-box"),
                0,
                rect(58.6721, 300., 147.9286, 336.2019),
                "Text box",
                text.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_arc(
            ArcAnnotation::new(
                id("arc"),
                0,
                point(403.7147, 517.0525),
                point(441.2891, 573.9763),
                point(414.72, 557.3037),
                shape.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_callout(
            CalloutAnnotation::new(
                id("callout"),
                0,
                vec![point(288.3672, 450.), point(335.18, 482.6304)],
                rect(335.18, 470., 403.68, 500.),
                "Callout",
                CalloutAppearance::new(line.clone(), text.clone()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_dimension(
            DimensionAnnotation::new(
                id("dimension"),
                0,
                point(173.5196, 400.),
                point(249.9806, 400.),
                10.,
                "",
                DimensionAppearance::new(line.clone(), caption.clone()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_length(
            LengthAnnotation::new_with_appearance(
                id("length"),
                0,
                point(58.6721, 400.),
                point(135.1331, 400.),
                calibration.clone(),
                DimensionAppearance::new(line.clone(), caption.clone()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_measurement_path(
            MeasurementPathAnnotation::new_with_text_style(
                id("polylength"),
                0,
                vec![
                    point(173.5196, 200.),
                    point(210.3457, 257.4238),
                    point(249.9806, 200.),
                ],
                MeasurementPathKind::Polylength,
                calibration.clone(),
                shape.clone(),
                caption.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_measurement_path(
            MeasurementPathAnnotation::new_with_text_style(
                id("area"),
                0,
                vec![
                    point(403.2147, 257.4238),
                    point(479.6757, 257.4238),
                    point(479.6757, 200.),
                    point(403.2147, 200.),
                ],
                MeasurementPathKind::Area,
                calibration,
                shape.clone(),
                caption.clone(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_cloud_plus(
            CloudPlusAnnotation::new(
                id("cloud-plus"),
                0,
                vec![
                    point(58.6721, 150.),
                    point(116.72, 150.),
                    point(116.72, 109.4289),
                    point(58.6721, 109.4289),
                ],
                2.,
                vec![
                    point(124.7705, 130.1),
                    point(159.3372, 130.),
                    point(179.1372, 130.),
                ],
                rect(179.1372, 115., 310.6372, 146.),
                "CloudPlus",
                CloudPlusAppearance::new(shape.clone(), line.clone(), text.clone()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_image(
            ImageAnnotation::new(
                id("image"),
                0,
                rect(272.7629, 614.4233 - 300., 335.18, 661.2361 - 300.),
                asset.clone(),
                false,
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_snapshot(
            SnapshotAnnotation::new(
                id("snapshot"),
                0,
                rect(335.18, 50., 422.564, 112.4172),
                asset,
                1.,
            )
            .unwrap(),
        )
        .unwrap();
    let authority = SaveAsTargetAuthority::bind(target.to_path_buf(), source).unwrap();
    session
        .prepare_save_authorized(&authority)
        .unwrap()
        .publish()
        .unwrap();
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bluebeam")
        .join(name)
}

const REVU_FIXTURES: [&str; 3] = [
    "revu-shapes.pdf",
    "revu-measure-text-media.pdf",
    "revu-text-box.pdf",
];

fn scratch_dir(label: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("bp-bluebeam-format-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn name_of(object: &lopdf::Object) -> String {
    object
        .as_name()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .unwrap_or_default()
}

fn string_of(object: &lopdf::Object) -> String {
    object
        .as_str()
        .map(|value| String::from_utf8_lossy(value).into_owned())
        .unwrap_or_default()
}

/// Every markup in a PDF as (Subtype, IT, ITEx, Subj) and its key set.
fn markup_key_sets(path: &Path) -> Vec<((String, String, String, String), BTreeSet<String>)> {
    let document = lopdf::Document::load(path).unwrap();
    let mut markups = Vec::new();
    for page_id in document.get_pages().into_values() {
        let page = document.get_object(page_id).unwrap().as_dict().unwrap();
        let Ok(annotations) = page.get(b"Annots") else {
            continue;
        };
        let annotations = match annotations {
            lopdf::Object::Reference(id) => document.get_object(*id).unwrap().as_array().unwrap(),
            other => other.as_array().unwrap(),
        };
        for annotation in annotations {
            let dictionary = document
                .get_object(annotation.as_reference().unwrap())
                .unwrap()
                .as_dict()
                .unwrap();
            let field = |key: &[u8]| dictionary.get(key).ok();
            let identity = (
                field(b"Subtype").map(name_of).unwrap_or_default(),
                field(b"IT").map(name_of).unwrap_or_default(),
                field(b"ITEx").map(name_of).unwrap_or_default(),
                field(b"Subj").map(string_of).unwrap_or_default(),
            );
            let keys = dictionary
                .iter()
                .map(|(key, _)| String::from_utf8_lossy(key).into_owned())
                .collect();
            markups.push((identity, keys));
        }
    }
    markups
}

/// Keys that hold optional values: opacity below one, rotation, fill.
const OPTIONAL_VALUE_KEYS: [&str; 5] = ["CA", "FillOpacity", "Rotation", "IC", "BS"];

#[test]
fn every_native_markup_family_writes_revu_key_sets() {
    let directory = scratch_dir("keys");
    let source = directory.join("source.pdf");
    let target = directory.join("native.pdf");
    blank_letter_pdf(&source);
    write_reference(&source, &target);
    let revu = REVU_FIXTURES
        .iter()
        .flat_map(|name| markup_key_sets(&fixture(name)))
        .collect::<Vec<_>>();
    let native = markup_key_sets(&target);
    assert_eq!(native.len(), 20, "one markup per family, Cloud+ as a pair");
    for (identity, keys) in native {
        let candidates = revu
            .iter()
            .filter(|(revu_identity, _)| revu_identity == &identity)
            .map(|(_, keys)| keys)
            .collect::<Vec<_>>();
        assert!(!candidates.is_empty(), "Revu has no {identity:?} specimen");
        let comparable = |set: &BTreeSet<String>| {
            set.iter()
                .filter(|key| !OPTIONAL_VALUE_KEYS.contains(&key.as_str()))
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        assert!(
            candidates
                .iter()
                .any(|candidate| comparable(candidate) == comparable(&keys)),
            "{identity:?} keys differ from Revu:\n native {keys:?}\n revu   {candidates:?}"
        );
    }
}

fn collect_bp_keys(object: &lopdf::Object, found: &mut Vec<String>) {
    match object {
        lopdf::Object::Dictionary(dictionary) => {
            for (key, value) in dictionary.iter() {
                if key.starts_with(b"BP") {
                    found.push(String::from_utf8_lossy(key).into_owned());
                }
                collect_bp_keys(value, found);
            }
        }
        lopdf::Object::Stream(stream) => {
            collect_bp_keys(&lopdf::Object::Dictionary(stream.dict.clone()), found)
        }
        lopdf::Object::Array(values) => values
            .iter()
            .for_each(|value| collect_bp_keys(value, found)),
        _ => {}
    }
}

#[test]
fn native_output_contains_no_private_keys() {
    let directory = scratch_dir("private");
    let source = directory.join("source.pdf");
    let target = directory.join("native.pdf");
    blank_letter_pdf(&source);
    write_reference(&source, &target);
    let document = lopdf::Document::load(&target).unwrap();
    let mut found = Vec::new();
    for object in document.objects.values() {
        collect_bp_keys(object, &mut found);
    }
    assert!(found.is_empty(), "private keys written: {found:?}");
    let session = PdfPersistenceSession::open(&target).unwrap();
    assert!(session.untouched_annotations().is_empty());
    assert_eq!(session.annotation_order().len(), 19);
}

#[test]
fn revu_authored_markups_import_as_their_native_families() {
    let shapes = PdfPersistenceSession::open(fixture("revu-shapes.pdf")).unwrap();
    assert!(
        shapes.untouched_annotations().is_empty(),
        "{:?}",
        shapes.untouched_annotations()
    );
    assert_eq!(shapes.rectangles().len(), 1);
    assert_eq!(shapes.ellipses().len(), 1);
    assert_eq!(shapes.arcs().len(), 1);
    assert_eq!(shapes.straight_lines().len(), 2);
    assert_eq!(shapes.vertex_paths().len(), 2);
    assert_eq!(shapes.clouds().len(), 1);
    assert_eq!(shapes.pens().len(), 2);
    assert_eq!(shapes.text_boxes().len(), 1);
    assert_eq!(shapes.rectangles()[0].id.as_str(), "MSYMAPZFINTDPPUL");

    let measured = PdfPersistenceSession::open(fixture("revu-measure-text-media.pdf")).unwrap();
    assert!(measured.untouched_annotations().is_empty(), "{:?}", measured.untouched_annotations());
    // Revu's vector Snapshot is an editable Snapshot that keeps its Form.
    assert_eq!(measured.vector_snapshot_ids(), [id("RIJSPWSIYBGOWHST")]);
    // A callout whose text was deleted in Revu is still a callout.
    assert_eq!(measured.callouts().len(), 2);
    assert!(measured.callouts().iter().any(|callout| callout.content().is_empty()));
    assert_eq!(measured.dimensions().len(), 1);
    assert_eq!(measured.lengths().len(), 2);
    assert_eq!(measured.measurement_paths().len(), 2);
    assert_eq!(measured.cloud_pluses().len(), 1);
    assert_eq!(measured.images().len(), 1);
    assert_eq!(measured.snapshots().len(), 1);
    // Revu's 1:100 page viewport and its caption values come through.
    let scale = &measured.page_scales()[0];
    assert_eq!(scale.name, "1:100");
    assert!(
        (measured.lengths()[0].measured_value() - 2697.37).abs() < 0.01,
        "{:?} {:?}",
        measured.lengths()[0],
        measured.page_scales()
    );
    assert_eq!(measured.lengths()[0].caption(), "2,697.37 mm");
    let area = measured
        .measurement_paths()
        .iter()
        .find(|path| path.kind == MeasurementPathKind::Area)
        .unwrap();
    assert_eq!(area.caption(), "5.46 sq m");

    let styled = PdfPersistenceSession::open(fixture("revu-text-box-bold-centre.pdf")).unwrap();
    let style = styled.text_boxes()[0].style();
    assert_eq!(style.weight(), 700);
    assert_eq!(style.alignment(), TextAlignment::Center);
    assert_eq!(style.color(), "#ff0000");
    assert_eq!(style.inset_pt(), 3.);
}

#[test]
fn untouched_revu_markups_are_saved_byte_for_byte() {
    let directory = scratch_dir("untouched");
    for name in REVU_FIXTURES {
        let source = fixture(name);
        let target = directory.join(name);
        let session = PdfPersistenceSession::open(&source).unwrap();
        let authority = SaveAsTargetAuthority::bind(target.clone(), &source).unwrap();
        session
            .prepare_save_authorized(&authority)
            .unwrap()
            .publish()
            .unwrap();
        let before = lopdf::Document::load(&source).unwrap();
        let after = lopdf::Document::load(&target).unwrap();
        for page_id in before.get_pages().into_values() {
            let annotations = match before
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Annots")
                .unwrap()
            {
                lopdf::Object::Reference(id) => before.get_object(*id).unwrap().as_array().unwrap(),
                other => other.as_array().unwrap(),
            };
            for annotation in annotations {
                let id = annotation.as_reference().unwrap();
                assert_eq!(
                    after.get_object(id).unwrap(),
                    before.get_object(id).unwrap(),
                    "{name}: untouched markup {id:?} changed"
                );
            }
        }
    }
}

#[test]
fn edited_revu_markup_keeps_revu_keys_and_metadata() {
    let directory = scratch_dir("edited");
    let source = fixture("revu-shapes-styled.pdf");
    let target = directory.join("edited.pdf");
    let mut session = PdfPersistenceSession::open(&source).unwrap();
    let mut rectangle = session.rectangles()[0].clone();
    assert_eq!(rectangle.rotation_degrees, 15.);
    assert_eq!(rectangle.appearance.stroke_width_pt(), 2., "{rectangle:?}");
    assert_eq!(rectangle.appearance.fill_opacity(), 0.5);
    assert_eq!(rectangle.appearance.opacity(), 0.8);
    rectangle.rect.x += 10.;
    session.replace_rectangle(rectangle.clone()).unwrap();
    let authority = SaveAsTargetAuthority::bind(target.clone(), &source).unwrap();
    session
        .prepare_save_authorized(&authority)
        .unwrap()
        .publish()
        .unwrap();
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert!(reopened.rectangles()[0].same_persisted_state_as(&rectangle));
    let before = markup_key_sets(&source)
        .into_iter()
        .find(|(identity, _)| identity.0 == "Square")
        .unwrap()
        .1;
    let after = markup_key_sets(&target)
        .into_iter()
        .find(|(identity, _)| identity.0 == "Square")
        .unwrap()
        .1;
    assert_eq!(after, before, "an edit must keep Revu's key set");
    let edited = lopdf::Document::load(&target).unwrap();
    let original = lopdf::Document::load(&source).unwrap();
    let square = |document: &lopdf::Document| {
        document
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .find(|dictionary| {
                dictionary.get(b"Subtype").ok().map(name_of).as_deref() == Some("Square")
            })
            .unwrap()
            .clone()
    };
    // Revu's dash array survives an edit that keeps the line style.
    let dash = |document: &lopdf::Document| {
        let border = square(document).get(b"BS").unwrap().clone();
        let border = match border {
            lopdf::Object::Reference(id) => document.get_object(id).unwrap().clone(),
            other => other,
        };
        border.as_dict().unwrap().get(b"D").unwrap().clone()
    };
    assert_eq!(
        dash(&edited)
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_float().unwrap())
            .collect::<Vec<_>>(),
        [2., 2.]
    );
    assert_eq!(dash(&edited), dash(&original));
    for key in [b"T".as_slice(), b"CreationDate", b"NM", b"Subj", b"P"] {
        assert_eq!(
            square(&edited).get(key).unwrap(),
            square(&original).get(key).unwrap()
        );
    }
}

/// Run with `BP_BLUEBEAM_REFERENCE_OUT=/abs/out.pdf cargo test --test
/// bluebeam_format -- --ignored` to export the comparison specimen.
#[test]
#[ignore = "exports a comparison specimen for manual Revu review"]
fn export_bluebeam_reference_specimen() {
    let Some(target) = std::env::var_os("BP_BLUEBEAM_REFERENCE_OUT").map(PathBuf::from) else {
        eprintln!("set BP_BLUEBEAM_REFERENCE_OUT to export the specimen");
        return;
    };
    let source = target.with_extension("source.pdf");
    blank_letter_pdf(&source);
    let _ = std::fs::remove_file(&target);
    write_reference(&source, &target);
}

/// Opens `BP_INSPECT_PDF` (for example a native specimen Revu has edited) and
/// requires every markup to import as a typed family.
#[test]
#[ignore = "inspects a Revu-edited specimen supplied by BP_INSPECT_PDF"]
fn inspect_revu_edited_specimen() {
    let Some(path) = std::env::var_os("BP_INSPECT_PDF").map(PathBuf::from) else {
        eprintln!("set BP_INSPECT_PDF to inspect a specimen");
        return;
    };
    let session = PdfPersistenceSession::open(&path).unwrap();
    eprintln!("rectangles {:#?}", session.rectangles());
    eprintln!("ellipses {:#?}", session.ellipses());
    eprintln!("lines {:#?}", session.straight_lines());
    eprintln!("polygons {:#?}", session.vertex_paths());
    eprintln!("pens {:#?}", session.pens());
    eprintln!("text {:#?}", session.text_boxes());
    eprintln!("callouts {:#?}", session.callouts());
    eprintln!("lengths {:#?}", session.lengths());
    eprintln!("areas {:#?}", session.measurement_paths());
    eprintln!(
        "images {:?}",
        session
            .images()
            .iter()
            .map(|image| (image.rect, image.rotation_degrees()))
            .collect::<Vec<_>>()
    );
    assert!(
        session.untouched_annotations().is_empty(),
        "{:?}",
        session.untouched_annotations()
    );
    assert_eq!(session.annotation_order().len(), 19);
}

/// A native specimen edited by Revu's ScriptEngine (`MarkupSet`) and saved
/// incrementally with a predictor-encoded cross-reference stream.
#[test]
fn revu_edits_to_native_markups_reopen_typed() {
    let session = PdfPersistenceSession::open(fixture("revu-edited-native.pdf")).unwrap();
    assert!(
        session.untouched_annotations().is_empty(),
        "{:?}",
        session.untouched_annotations()
    );
    assert_eq!(session.annotation_order().len(), 19);
    let rectangle = &session.rectangles()[0];
    assert_eq!(rectangle.rotation_degrees, 20.);
    assert_eq!(rectangle.appearance.stroke_color(), "#0000ff");
    assert_eq!(rectangle.appearance.stroke_width_pt(), 3.);
    assert_eq!(rectangle.appearance.fill_color(), Some("#00ff00"));
    assert!((rectangle.appearance.fill_opacity() - 0.4).abs() < 1e-6);
    assert!((session.ellipses()[0].appearance.opacity() - 0.5).abs() < 1e-6);
    assert_eq!(session.text_boxes()[0].style().color(), "#0000ff");
    assert_eq!(
        session.callouts()[0].appearance.line().stroke_color(),
        "#0000ff",
        "Revu's callout colour is the leader colour"
    );
    assert_eq!(
        session.lengths()[0].appearance.line().stroke_color(),
        "#0000ff"
    );
    assert!(
        session
            .pens()
            .iter()
            .any(|pen| pen.appearance.color() == "#00aa00")
    );
}

fn save_as(session: &PdfPersistenceSession, source: &Path, target: &Path) {
    let authority = SaveAsTargetAuthority::bind(target.to_path_buf(), source).unwrap();
    session
        .prepare_save_authorized(&authority)
        .unwrap()
        .publish()
        .unwrap();
}

fn annotation_dictionaries(path: &Path) -> (lopdf::Document, Vec<lopdf::Dictionary>) {
    let document = lopdf::Document::load(path).unwrap();
    let dictionaries = document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dictionary| dictionary.get(b"NM").is_ok())
        .cloned()
        .collect();
    (document, dictionaries)
}

fn appearance_content(document: &lopdf::Document, annotation: &lopdf::Dictionary) -> String {
    let appearance = annotation.get(b"AP").unwrap().as_dict().unwrap();
    let stream_id = appearance.get(b"N").unwrap().as_reference().unwrap();
    let stream = document.get_object(stream_id).unwrap().as_stream().unwrap();
    String::from_utf8_lossy(
        &stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()),
    )
    .into_owned()
}

#[test]
fn revu_cloud_fill_survives_an_edit() {
    let directory = scratch_dir("cloud-fill");
    let source = fixture("revu-shapes-styled.pdf");
    let target = directory.join("edited.pdf");
    let mut session = PdfPersistenceSession::open(&source).unwrap();
    let cloud = session.clouds()[0].clone();
    assert_eq!(cloud.appearance.fill_color(), Some("#0000ff"));
    let moved = CloudAnnotation::new(
        cloud.id.clone(),
        cloud.page_index,
        cloud
            .points()
            .iter()
            .map(|vertex| point(vertex.x + 5., vertex.y))
            .collect(),
        cloud.border_effect_intensity(),
        cloud.appearance.clone(),
    )
    .unwrap();
    session.replace_cloud(moved.clone()).unwrap();
    save_as(&session, &source, &target);
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert!(reopened.clouds()[0].same_persisted_state_as(&moved));
    let (document, dictionaries) = annotation_dictionaries(&target);
    let saved = dictionaries
        .iter()
        .find(|dictionary| string_of(dictionary.get(b"NM").unwrap()) == cloud.id.as_str())
        .unwrap();
    assert_eq!(
        saved.get(b"IC").unwrap().as_array().unwrap().len(),
        3,
        "Revu stores the cloud fill as /IC"
    );
    assert!(appearance_content(&document, saved).contains("h B"));
}

#[test]
fn unlabelled_dimension_and_bordered_callout_match_revu() {
    let directory = scratch_dir("dimension-callout");
    let source = directory.join("source.pdf");
    let target = directory.join("native.pdf");
    blank_letter_pdf(&source);
    let mut session = PdfPersistenceSession::open(&source).unwrap();
    let text = TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.)
        .unwrap()
        .with_layout_metrics(13.8, 3.)
        .unwrap();
    let thin = StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap();
    let wide = StraightLineAppearance::new("#ff0000", 2., 1., StrokeStyle::Solid).unwrap();
    session
        .add_dimension(
            DimensionAnnotation::new(
                id("ABCDEFGHIJKLMNOP"),
                0,
                point(173.5196, 729.895),
                point(249.9806, 729.895),
                10.,
                "",
                DimensionAppearance::new(thin.clone(), text.clone()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    for (name, line) in [("PLAINCALLOUTAAAA", thin), ("BORDERCALLOUTAAA", wide)] {
        session
            .add_callout(
                CalloutAnnotation::new(
                    id(name),
                    0,
                    vec![point(288.3672, 450.), point(335.18, 482.6304)],
                    rect(335.18, 470., 403.68, 500.),
                    "",
                    CalloutAppearance::new(line, text.clone()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
    }
    save_as(&session, &source, &target);
    let (document, dictionaries) = annotation_dictionaries(&target);
    let by_name = |name: &str| {
        dictionaries
            .iter()
            .find(|dictionary| string_of(dictionary.get(b"NM").unwrap()) == name)
            .unwrap()
    };

    // Revu's Dimension: no label keys, `LLE 2`, arrows inside a continuous
    // dimension line.
    let dimension = by_name("ABCDEFGHIJKLMNOP");
    assert!(dimension.get(b"Contents").is_err());
    assert!(dimension.get(b"RC").is_err());
    assert!(dimension.get(b"DS").is_ok());
    assert_eq!(dimension.get(b"LLE").unwrap().as_float().unwrap(), 2.);
    let content = appearance_content(&document, dimension);
    assert!(!content.contains("BT"), "an unlabelled Dimension draws no text");
    assert_eq!(content.matches(" b\n").count(), 2, "two closed arrowheads");

    // `W 0` is Revu's default 1 pt leader; any other width also strokes the box.
    let plain = by_name("PLAINCALLOUTAAAA");
    let bordered = by_name("BORDERCALLOUTAAA");
    let border_width = |dictionary: &lopdf::Dictionary| {
        dictionary
            .get(b"BS")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"W")
            .unwrap()
            .as_float()
            .unwrap()
    };
    assert_eq!(border_width(plain), 0.);
    assert_eq!(border_width(bordered), 2.);
    assert!(!appearance_content(&document, plain).contains(" re\n"));
    assert!(
        !appearance_content(&document, plain).contains("BT"),
        "a callout without text draws no text object"
    );
    assert!(appearance_content(&document, bordered).contains(" re\nS\n"));

    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert_eq!(reopened.dimensions()[0].content(), "");
    assert_eq!(reopened.callouts().len(), 2);
    assert!(reopened.untouched_annotations().is_empty());
}

#[test]
fn filled_cloud_plus_saves_and_reopens_its_fill() {
    let directory = scratch_dir("cloud-plus-fill");
    let source = directory.join("source.pdf");
    let target = directory.join("native.pdf");
    blank_letter_pdf(&source);
    let mut session = PdfPersistenceSession::open(&source).unwrap();
    let filled = RectangleAppearance::new("#ff0000", 1., Some("#0000ff"), 1.)
        .unwrap()
        .with_fill_opacity(0.5)
        .unwrap();
    let line = StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap();
    let text = TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.)
        .unwrap()
        .with_layout_metrics(13.8, 3.)
        .unwrap();
    session
        .add_cloud_plus(
            CloudPlusAnnotation::new(
                id("FILLEDCLOUDPLUSA"),
                0,
                vec![
                    point(58.6721, 150.),
                    point(116.72, 150.),
                    point(116.72, 109.4289),
                    point(58.6721, 109.4289),
                ],
                2.,
                vec![point(124.7705, 130.1), point(159.3372, 130.), point(179.1372, 130.)],
                rect(179.1372, 115., 310.6372, 146.),
                "Filled",
                CloudPlusAppearance::new(filled, line, text).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    save_as(&session, &source, &target);
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert!(reopened.untouched_annotations().is_empty(), "{:?}", reopened.untouched_annotations());
    let cloud = reopened.cloud_pluses()[0].appearance.cloud();
    assert_eq!(cloud.fill_color(), Some("#0000ff"));
    assert_eq!(cloud.fill_opacity(), 0.5);
    let (document, dictionaries) = annotation_dictionaries(&target);
    let polygon = dictionaries
        .iter()
        .find(|dictionary| dictionary.get(b"Subtype").ok().map(name_of).as_deref() == Some("Polygon"))
        .unwrap();
    assert!(polygon.get(b"IC").is_ok());
    assert!(appearance_content(&document, polygon).contains("h B"));
}

#[test]
fn edited_revu_vector_snapshot_keeps_its_form() {
    let directory = scratch_dir("vector-snapshot");
    let source = fixture("revu-measure-text-media.pdf");
    let target = directory.join("edited.pdf");
    let original_document = lopdf::Document::load(&source).unwrap();
    let snapshot_dictionary = |document: &lopdf::Document| {
        document
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .find(|dictionary| {
                dictionary.get(b"IT").ok().map(name_of).as_deref() == Some("StampSnapshot")
            })
            .unwrap()
            .clone()
    };
    let original_form = snapshot_dictionary(&original_document)
        .get(b"AP")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"N")
        .unwrap()
        .as_reference()
        .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    let mut snapshot = session.snapshots()[0].clone();
    snapshot.rect.x += 20.;
    let snapshot = snapshot.with_rotation_degrees(30.).unwrap();
    session.replace_snapshot(snapshot.clone()).unwrap();
    save_as(&session, &source, &target);

    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert!(reopened.untouched_annotations().is_empty());
    assert_eq!(reopened.vector_snapshot_ids(), [snapshot.id.clone()]);
    let moved = &reopened.snapshots()[0];
    assert!((moved.rect.x - snapshot.rect.x).abs() < 0.01, "{moved:?}");
    assert!((moved.rotation_degrees() - 30.).abs() < 0.01);

    let edited = lopdf::Document::load(&target).unwrap();
    let dictionary = snapshot_dictionary(&edited);
    assert_eq!(dictionary.get(b"Rotation").unwrap().as_float().unwrap(), 30.);
    let wrapper_id = dictionary
        .get(b"AP")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"N")
        .unwrap()
        .as_reference()
        .unwrap();
    let wrapper = edited.get_object(wrapper_id).unwrap().as_stream().unwrap();
    let drawn = wrapper
        .dict
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"XObject")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Snapshot")
        .unwrap()
        .as_reference()
        .unwrap();
    // Revu's own Form, unchanged, is what the new appearance draws.
    assert_eq!(drawn, original_form);
    assert_eq!(
        edited.get_object(drawn).unwrap(),
        original_document.get_object(original_form).unwrap()
    );
}

/// Run with `BP_REVIEW_SPECIMEN_OUT=/abs/out.pdf cargo test --test
/// bluebeam_format -- --ignored` to export the visual review specimen: sloped
/// measurements, a bordered callout and filled Cloud+.
#[test]
#[ignore = "exports a visual review specimen"]
fn export_visual_review_specimen() {
    let Some(target) = std::env::var_os("BP_REVIEW_SPECIMEN_OUT").map(PathBuf::from) else {
        eprintln!("set BP_REVIEW_SPECIMEN_OUT to export the specimen");
        return;
    };
    let source = target.with_extension("source.pdf");
    blank_letter_pdf(&source);
    let _ = std::fs::remove_file(&target);
    let mut session = PdfPersistenceSession::open(&source).unwrap();
    let text = TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.)
        .unwrap()
        .with_layout_metrics(13.8, 3.)
        .unwrap();
    let line = StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap();
    let wide = StraightLineAppearance::new("#ff0000", 2., 1., StrokeStyle::Solid).unwrap();
    let calibration = LengthCalibration::from_scale(1., 100., "m", 2, true).unwrap();
    session
        .add_length(
            LengthAnnotation::new_with_appearance(
                id("REVIEWSLOPEDLENG"),
                0,
                point(80., 600.),
                point(260., 700.),
                calibration,
                DimensionAppearance::new(line.clone(), text.clone()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_dimension(
            DimensionAnnotation::new(
                id("REVIEWSLOPEDDIME"),
                0,
                point(320., 700.),
                point(480., 600.),
                12.,
                "door",
                DimensionAppearance::new(line.clone(), text.clone()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_callout(
            CalloutAnnotation::new(
                id("REVIEWBORDERCALL"),
                0,
                vec![point(100., 450.), point(160., 500.)],
                rect(160., 480., 280., 520.),
                "Bordered",
                CalloutAppearance::new(wide, text.clone()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let filled = RectangleAppearance::new("#ff0000", 1., Some("#9999ff"), 1.)
        .unwrap()
        .with_fill_opacity(0.6)
        .unwrap();
    session
        .add_cloud_plus(
            CloudPlusAnnotation::new(
                id("REVIEWFILLEDCLOU"),
                0,
                vec![point(330., 470.), point(450., 470.), point(450., 380.), point(330., 380.)],
                2.,
                vec![point(458., 425.), point(480., 425.), point(500., 425.)],
                rect(500., 410., 570., 440.),
                "Filled",
                CloudPlusAppearance::new(filled, line, text).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    save_as(&session, &source, &target);
}
