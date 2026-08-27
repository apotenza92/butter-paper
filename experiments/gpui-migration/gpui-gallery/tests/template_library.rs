use std::{fs, path::PathBuf};

use butter_paper_gpui_gallery::{
    generated_document::{GeneratedDocumentRequest, GeneratedDocumentStore, GeneratedPattern},
    pdf_engine::PdfPersistenceSession,
    template_library::{BUILT_IN_BLANK_ID, TemplateLibrary},
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn template_library_custom_and_imported_records_survive_restart_and_materialize_owned_documents() {
    let root = tempdir().unwrap();
    let library_root = root.path().join("template-library");
    let generated_root = root.path().join("generated-documents");
    let original_import = root.path().join("Site Form.pdf");
    let fixture_bytes = fs::read(fixture_path()).unwrap();
    fs::write(&original_import, &fixture_bytes).unwrap();

    let mut library = TemplateLibrary::open(library_root.clone()).unwrap();
    assert_eq!(library.last_template_id(), BUILT_IN_BLANK_ID);
    library
        .add_generated(
            "custom-fixed-grid",
            "  My   Grid  ",
            GeneratedDocumentRequest {
                title: "Untitled".into(),
                width_mm: 420.,
                height_mm: 297.,
                pattern: Some(GeneratedPattern::SquareGrid {
                    spacing_mm: 10.,
                    color: "#d1d5db".into(),
                }),
            },
        )
        .unwrap();
    let imported_id = "imported-00000000-0000-4000-8000-000000000000";
    let imported = library
        .import_pdf(
            imported_id,
            "  Site   Form.pdf  ",
            "2026-08-27T00:00:00.000Z",
            &original_import,
        )
        .unwrap();
    assert_eq!(imported.name(), "Site Form");
    assert_eq!(imported.page_count(), Some(100));
    assert_eq!(imported.sha256(), Some(sha256(&fixture_bytes).as_str()));
    library.select(imported_id).unwrap();

    fs::write(
        &original_import,
        b"the original import is no longer authoritative",
    )
    .unwrap();
    drop(library);

    let mut library = TemplateLibrary::open(library_root.clone()).unwrap();
    assert_eq!(library.record_ids(), ["custom-fixed-grid", imported_id]);
    assert_eq!(library.last_template_id(), imported_id);
    let managed_source = library.managed_source_path(imported_id).unwrap();
    assert_eq!(
        sha256(&fs::read(&managed_source).unwrap()),
        sha256(&fixture_bytes)
    );
    fs::write(&managed_source, b"checksum drift").unwrap();
    assert!(TemplateLibrary::open(library_root.clone()).is_err());
    fs::write(&managed_source, &fixture_bytes).unwrap();
    library = TemplateLibrary::open(library_root.clone()).unwrap();

    let generated_store = GeneratedDocumentStore::new(generated_root).unwrap();
    let custom = library
        .materialize("custom-fixed-grid", "custom-session", &generated_store)
        .unwrap();
    let imported = library
        .materialize(imported_id, "imported-session", &generated_store)
        .unwrap();
    assert_ne!(
        imported.path(),
        library.managed_source_path(imported_id).unwrap()
    );
    assert_eq!(
        PdfPersistenceSession::open(custom.path())
            .unwrap()
            .page_count(),
        1
    );
    assert_eq!(
        PdfPersistenceSession::open(imported.path())
            .unwrap()
            .page_count(),
        100
    );

    generated_store.release(&custom).unwrap();
    generated_store.release(&imported).unwrap();
    assert!(library.managed_source_path(imported_id).unwrap().is_file());

    let before = library
        .record_ids()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let invalid = root.path().join("invalid.pdf");
    fs::write(&invalid, b"not a pdf").unwrap();
    assert!(
        library
            .import_pdf(
                "imported-11111111-1111-4111-8111-111111111111",
                "Invalid",
                "2026-08-27T00:00:01.000Z",
                &invalid,
            )
            .is_err()
    );
    assert_eq!(
        library.record_ids(),
        before.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(library.last_template_id(), imported_id);

    library.remove(imported_id).unwrap();
    assert_eq!(library.last_template_id(), BUILT_IN_BLANK_ID);
    assert!(library.managed_source_path(imported_id).is_err());
    assert_eq!(
        TemplateLibrary::open(library_root).unwrap().record_ids(),
        ["custom-fixed-grid"]
    );
}
