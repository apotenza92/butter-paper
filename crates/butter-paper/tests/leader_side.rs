//! The side of its text box that a Cloud+ or Callout leader meets is the
//! user's choice: a knee dragged across the text box's centre line moves it,
//! and it is kept through a save and reopen, derived from `/CL`.

use std::path::{Path, PathBuf};

use butter_paper::annotation_adapter::{AnnotationAdapter, AnnotationTool};
use butter_paper::annotation_model::{Annotation, PdfPoint, PdfRect};
use butter_paper::pdf_engine::PdfPersistenceSession;
use butter_paper::pdf_file_authority::SaveAsTargetAuthority;
use lopdf::dictionary;

const DOCUMENT: u64 = 7;

fn point(x: f64, y: f64) -> PdfPoint {
    PdfPoint::new(x, y).unwrap()
}

fn scratch_dir(label: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-scratch")
        .join(format!("leader-side-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
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

fn save_as(session: &PdfPersistenceSession, source: &Path, target: &Path) {
    let authority = SaveAsTargetAuthority::bind(target.to_path_buf(), source).unwrap();
    session
        .prepare_save_authorized(&authority)
        .unwrap()
        .publish()
        .unwrap();
}

fn drag(adapter: &mut AnnotationAdapter, pointer: u64, from: PdfPoint, to: PdfPoint) {
    adapter
        .pointer_down(DOCUMENT, 0, pointer, from, 4.)
        .unwrap();
    adapter.pointer_move(pointer, to).unwrap();
    adapter.pointer_up(pointer, to).unwrap();
}

fn centre_x(text_box: PdfRect) -> f64 {
    text_box.x + text_box.width * 0.5
}

#[test]
fn a_cloud_plus_leader_side_chosen_by_a_knee_drag_survives_save_and_reopen() {
    let directory = scratch_dir("cloud-plus");
    let source = directory.join("source.pdf");
    let target = directory.join("flipped.pdf");
    blank_letter_pdf(&source);

    let mut adapter = AnnotationAdapter::default();
    adapter.set_semantic_snap_page_size(DOCUMENT, 0, 612., 792.);
    adapter.set_tool(AnnotationTool::CloudPlus).unwrap();
    drag(&mut adapter, 1, point(120., 420.), point(260., 520.));
    let created = adapter.snapshot(DOCUMENT).unwrap().cloud_pluses[0].clone();
    // A new Cloud+ picks its side itself: the leader meets the left side of
    // the text beside the cloud.
    assert_eq!(created.leader_points()[2].x, created.text_box.x);

    adapter.set_tool(AnnotationTool::Select).unwrap();
    let knee = created.leader_points()[1];
    let across = point(
        centre_x(created.text_box) + 4.,
        created.text_box.y + created.text_box.height + 15.,
    );
    drag(&mut adapter, 2, knee, across);
    let flipped = adapter.snapshot(DOCUMENT).unwrap().cloud_pluses[0].clone();
    let [_, knee, connection] = *flipped.leader_points() else {
        panic!("a Cloud+ leader");
    };
    assert_eq!(connection.x, flipped.text_box.x + flipped.text_box.width);
    assert!(knee.x > connection.x && knee.y == connection.y);

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_cloud_plus(flipped.clone()).unwrap();
    save_as(&session, &source, &target);
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert!(reopened.untouched_annotations().is_empty());
    let saved = reopened.cloud_pluses()[0].clone();
    assert!(saved.same_persisted_state_as(&flipped));
    // The side comes from `/CL`: the connection lies on the text's right.
    let [_, saved_knee, saved_connection] = *saved.leader_points() else {
        panic!("a Cloud+ leader");
    };
    assert!((saved_connection.x - (saved.text_box.x + saved.text_box.width)).abs() < 0.01);
    assert!(saved_knee.x > saved_connection.x);

    // Reopened, moving the text box to the left of the cloud keeps the side.
    adapter
        .load_imported_annotations(DOCUMENT, vec![Annotation::CloudPlus(saved.clone())])
        .unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    let body = saved.text_box;
    let inside = point(body.x + 6., body.y + body.height - 3.);
    adapter.pointer_down(DOCUMENT, 0, 3, inside, 4.).unwrap();
    adapter.pointer_up(3, inside).unwrap();
    adapter
        .translate_selected_cloud_plus_text_box(DOCUMENT, -400., 0.)
        .unwrap();
    let moved = adapter.snapshot(DOCUMENT).unwrap().cloud_pluses[0].clone();
    assert!(moved.text_box.x < created.text_box.x - 300.);
    assert!(
        (moved.leader_points()[2].x - (moved.text_box.x + moved.text_box.width)).abs() < 0.01,
        "the leader still meets the text's right side"
    );
}

#[test]
fn a_callout_leader_side_chosen_by_a_knee_drag_survives_save_and_reopen() {
    let directory = scratch_dir("callout");
    let source = directory.join("source.pdf");
    let target = directory.join("flipped.pdf");
    blank_letter_pdf(&source);

    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Callout).unwrap();
    drag(&mut adapter, 1, point(120., 420.), point(260., 520.));
    let created = adapter.snapshot(DOCUMENT).unwrap().callouts[0].clone();
    assert!((created.leader_points()[2].x - created.text_box.x).abs() < 0.001);
    adapter.set_tool(AnnotationTool::Select).unwrap();

    let knee = created.leader_points()[1];
    let across = point(
        centre_x(created.text_box) + 4.,
        created.text_box.y + created.text_box.height + 25.,
    );
    drag(&mut adapter, 2, knee, across);
    let flipped = adapter.snapshot(DOCUMENT).unwrap().callouts[0].clone();
    let connection = flipped.leader_points()[2];
    // The disk geometry is f32.
    assert!((connection.x - (flipped.text_box.x + flipped.text_box.width)).abs() < 0.001);
    assert!((connection.y - (flipped.text_box.y + flipped.text_box.height * 0.5)).abs() < 0.001);

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_callout(flipped.clone()).unwrap();
    save_as(&session, &source, &target);
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert!(reopened.untouched_annotations().is_empty());
    let saved = reopened.callouts()[0].clone();
    assert!(
        (saved.leader_points()[2].x - (saved.text_box.x + saved.text_box.width)).abs() < 0.01,
        "the saved connection lies on the text's right side"
    );

    adapter
        .load_imported_annotations(DOCUMENT, vec![Annotation::Callout(saved.clone())])
        .unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    let body = saved.text_box;
    let inside = point(body.x + 6., body.y + body.height - 3.);
    adapter.pointer_down(DOCUMENT, 0, 3, inside, 4.).unwrap();
    adapter.pointer_up(3, inside).unwrap();
    adapter
        .translate_selected_callout_text_box(DOCUMENT, -400., 0.)
        .unwrap();
    let moved = adapter.snapshot(DOCUMENT).unwrap().callouts[0].clone();
    assert!((moved.leader_points()[2].x - (moved.text_box.x + moved.text_box.width)).abs() < 0.01);
}
