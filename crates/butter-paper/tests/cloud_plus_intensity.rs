//! A Cloud+'s Bump size (`/BE /I`) sets the size of its bumps: a change makes
//! the cloud outline again from the control points, as a Cloud's does, and
//! drops any appearance path stored by another app. A Bump size left as it
//! is keeps that stored path.

use std::path::{Path, PathBuf};

use butter_paper::annotation_adapter::AnnotationAdapter;
use butter_paper::annotation_model::{Annotation, AnnotationSelectionSupplement, CloudAnnotation};
use butter_paper::cloud_plus_routing::cloud_plus_landing_pt;
use butter_paper::pdf_engine::PdfPersistenceSession;
use butter_paper::pdf_file_authority::SaveAsTargetAuthority;

const DOCUMENT: u64 = 7;

fn scratch_dir(label: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-scratch")
        .join(format!(
            "cloud-plus-intensity-{label}-{}",
            std::process::id()
        ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn save_as(session: &PdfPersistenceSession, source: &Path, target: &Path) {
    let authority = SaveAsTargetAuthority::bind(target.to_path_buf(), source).unwrap();
    session
        .prepare_save_authorized(&authority)
        .unwrap()
        .publish()
        .unwrap();
}

fn generated_bumps(
    cloud_plus: &butter_paper::annotation_model::CloudPlusAnnotation,
    intensity: f64,
) -> Vec<butter_paper::annotation_model::PdfPoint> {
    CloudAnnotation::new(
        cloud_plus.id.clone(),
        cloud_plus.page_index,
        cloud_plus.cloud_points().to_vec(),
        intensity,
        cloud_plus.appearance.cloud().clone(),
    )
    .unwrap()
    .scallop_path()
}

#[test]
fn an_intensity_change_on_a_cloud_plus_from_another_app_makes_new_bumps_and_saves_them() {
    let directory = scratch_dir("revu");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bluebeam/revu-measure-text-media.pdf");
    let target = directory.join("intensity.pdf");

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    let original = session.cloud_pluses()[0].clone();
    let intensity = original.border_effect_intensity();
    assert!(
        original.cloud_appearance_path().is_some(),
        "the fixture stores its cloud's appearance path"
    );

    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(DOCUMENT, vec![Annotation::CloudPlus(original.clone())])
        .unwrap();
    assert!(adapter.select_id(DOCUMENT, &original.id));
    let landing = cloud_plus_landing_pt(original.leader_points()).unwrap();
    let restyle = |adapter: &mut AnnotationAdapter, intensity: f64| {
        adapter
            .set_cloud_plus_style_with_routing_supplement(
                DOCUMENT,
                &original.id,
                original.appearance.clone(),
                intensity,
                landing,
                &AnnotationSelectionSupplement::new(),
            )
            .unwrap();
        adapter.snapshot(DOCUMENT).unwrap().cloud_pluses[0].clone()
    };

    // The same bump size keeps what the file stored.
    let untouched = restyle(&mut adapter, intensity);
    assert_eq!(
        untouched.cloud_appearance_path(),
        original.cloud_appearance_path()
    );
    assert_eq!(untouched.scallop_path(), original.scallop_path());

    // Another one makes the bumps again.
    let next = if intensity == 4. { 1. } else { 4. };
    let changed = restyle(&mut adapter, next);
    assert_eq!(changed.cloud_appearance_path(), None);
    assert_eq!(changed.scallop_path(), generated_bumps(&changed, next));
    assert_ne!(changed.scallop_path(), generated_bumps(&changed, intensity));

    session.replace_cloud_plus(changed.clone()).unwrap();
    save_as(&session, &source, &target);
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert!(reopened.untouched_annotations().is_empty());
    let saved = reopened.cloud_pluses()[0].clone();
    assert_eq!(saved.border_effect_intensity(), next);
    assert!(saved.same_persisted_state_as(&changed));
    let (saved_bumps, wanted) = (saved.scallop_path(), changed.scallop_path());
    assert_eq!(saved_bumps.len(), wanted.len());
    assert!(
        saved_bumps
            .iter()
            .zip(&wanted)
            .all(|(a, b)| (a.x - b.x).abs() < 1e-2 && (a.y - b.y).abs() < 1e-2)
    );
}

#[test]
fn an_untouched_cloud_plus_from_another_app_keeps_its_stored_path_through_a_save() {
    let directory = scratch_dir("untouched");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bluebeam/revu-measure-text-media.pdf");
    let target = directory.join("untouched.pdf");
    let session = PdfPersistenceSession::open(&source).unwrap();
    let original = session.cloud_pluses()[0].clone();
    save_as(&session, &source, &target);
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    let saved = &reopened.cloud_pluses()[0];
    assert_eq!(
        saved.cloud_appearance_path(),
        original.cloud_appearance_path()
    );
    assert!(saved.same_persisted_state_as(&original));
}
