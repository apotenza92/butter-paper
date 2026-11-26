//! A Cloud or Cloud+ whose outline is a rectangle resizes from rectangle
//! handles, and the result is saved as the same `/Vertices` outline.

use std::path::{Path, PathBuf};

use butter_paper::annotation_adapter::{AnnotationAdapter, AnnotationTool};
use butter_paper::annotation_model::{
    Annotation, PdfPoint, RectangleResizeHandle, cloud_rectangle,
};
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
        .join(format!("cloud-rectangle-{label}-{}", std::process::id()));
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

/// Presses `from` and releases at `to`, as one drag of the selected markup.
fn drag(adapter: &mut AnnotationAdapter, pointer: u64, from: PdfPoint, to: PdfPoint) {
    adapter
        .pointer_down(DOCUMENT, 0, pointer, from, 4.)
        .unwrap();
    adapter.pointer_move(pointer, to).unwrap();
    adapter.pointer_up(pointer, to).unwrap();
}

fn draw(adapter: &mut AnnotationAdapter, tool: AnnotationTool, from: PdfPoint, to: PdfPoint) {
    adapter.set_semantic_snap_page_size(DOCUMENT, 0, 612., 792.);
    adapter.set_tool(tool).unwrap();
    drag(adapter, 1, from, to);
}

#[test]
fn a_cloud_edge_drag_is_saved_as_one_rectangle_outline_and_reopens() {
    let directory = scratch_dir("cloud");
    let source = directory.join("source.pdf");
    let target = directory.join("resized.pdf");
    blank_letter_pdf(&source);

    let mut adapter = AnnotationAdapter::default();
    draw(
        &mut adapter,
        AnnotationTool::Cloud,
        point(100., 100.),
        point(220., 180.),
    );
    let created = adapter.snapshot(DOCUMENT).unwrap().clouds[0].clone();
    let rect = created.rectangle().expect("a drawn Cloud is a rectangle");

    // The east edge's middle: that whole side moves, the others stay.
    let east = RectangleResizeHandle::East.point(rect);
    drag(&mut adapter, 2, east, point(east.x + 60., east.y + 25.));
    let resized = adapter.snapshot(DOCUMENT).unwrap().clouds[0].clone();
    let grown = resized.rectangle().unwrap();
    assert_eq!(
        (grown.x, grown.y, grown.width, grown.height),
        (rect.x, rect.y, rect.width + 60., rect.height)
    );
    assert_eq!(resized.points().len(), 4);

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_cloud(resized.clone()).unwrap();
    save_as(&session, &source, &target);
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert!(reopened.untouched_annotations().is_empty());
    let saved = reopened.clouds()[0].clone();
    assert!(saved.same_persisted_state_as(&resized));
    assert_eq!(saved.rectangle(), resized.rectangle());

    // The reopened cloud resizes from rectangle handles again, one more
    // edge at a time, and saves again.
    adapter
        .load_imported_annotations(DOCUMENT, vec![Annotation::Cloud(saved.clone())])
        .unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    let place = RectangleResizeHandle::North.point(saved.rectangle().unwrap());
    // Select it with a click on its body, then drag its north edge.
    let body = saved
        .scallop_path()
        .into_iter()
        .max_by(|left, right| {
            let clearance = |candidate: PdfPoint| {
                saved
                    .points()
                    .iter()
                    .map(|vertex| (vertex.x - candidate.x).hypot(vertex.y - candidate.y))
                    .fold(f64::INFINITY, f64::min)
            };
            clearance(*left).total_cmp(&clearance(*right))
        })
        .unwrap();
    adapter.pointer_down(DOCUMENT, 0, 3, body, 4.).unwrap();
    adapter.pointer_up(3, body).unwrap();
    drag(&mut adapter, 4, place, point(place.x, place.y - 30.));
    let second = adapter.snapshot(DOCUMENT).unwrap().clouds[0].clone();
    let second_rect = second.rectangle().unwrap();
    let saved_rect = saved.rectangle().unwrap();
    assert_eq!(second_rect.y, saved_rect.y);
    assert_eq!(second_rect.height, saved_rect.height - 30.);
    assert_eq!(second_rect.x, saved_rect.x);
    assert_eq!(second_rect.width, saved_rect.width);
}

#[test]
fn a_cloud_plus_edge_drag_is_saved_with_its_leader_on_the_cloud() {
    let directory = scratch_dir("cloud-plus");
    let source = directory.join("source.pdf");
    let target = directory.join("resized.pdf");
    blank_letter_pdf(&source);

    let mut adapter = AnnotationAdapter::default();
    draw(
        &mut adapter,
        AnnotationTool::CloudPlus,
        point(120., 420.),
        point(260., 520.),
    );
    let created = adapter.snapshot(DOCUMENT).unwrap().cloud_pluses[0].clone();
    let rect = created.cloud_rectangle().unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    // A click on the cloud selects the Cloud+ again after its text opens.
    let south = RectangleResizeHandle::South.point(rect);
    drag(&mut adapter, 2, south, point(south.x, south.y - 20.));
    let resized = adapter.snapshot(DOCUMENT).unwrap().cloud_pluses[0].clone();
    let grown = resized.cloud_rectangle().unwrap();
    assert_eq!(
        (grown.x, grown.y, grown.width, grown.height),
        (rect.x, rect.y - 20., rect.width, rect.height + 20.)
    );
    assert_eq!(resized.text_box, created.text_box);

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_cloud_plus(resized.clone()).unwrap();
    save_as(&session, &source, &target);
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert!(reopened.untouched_annotations().is_empty());
    let saved = reopened.cloud_pluses()[0].clone();
    assert!(saved.cloud_rectangle().is_some());
    assert_eq!(saved.cloud_rectangle().unwrap().height, grown.height);
    assert_eq!(saved.leader_points().len(), 3);
    for (saved, resized) in saved.leader_points().iter().zip(resized.leader_points()) {
        assert!((saved.x - resized.x).abs() < 0.001 && (saved.y - resized.y).abs() < 0.001);
    }
}

#[test]
fn a_rectangular_cloud_from_revu_gets_rectangle_handles() {
    // Revu stores a rectangular cloud as four /Vertices.
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bluebeam/revu-shapes-styled.pdf");
    let session = PdfPersistenceSession::open(&source).unwrap();
    let cloud = &session.clouds()[0];
    assert_eq!(cloud.points().len(), 4);
    assert!(cloud.rectangle().is_some(), "{:?}", cloud.points());
    assert!(cloud_rectangle(cloud.points()).is_some());
}
