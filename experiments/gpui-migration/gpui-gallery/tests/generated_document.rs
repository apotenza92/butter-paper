use std::{fs, path::PathBuf};

use butter_paper_gpui_gallery::generated_document::{
    GeneratedDocumentRequest, GeneratedDocumentStore, GeneratedPattern, millimetres_to_pdf_points,
};
use lopdf::{Document, Object};

fn temporary_store_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "butter-paper-generated-document-{name}-{}",
        std::process::id()
    ))
}

fn numeric(object: &Object) -> f64 {
    match object {
        Object::Integer(value) => *value as f64,
        Object::Real(value) => f64::from(*value),
        other => panic!("expected PDF number, got {other:?}"),
    }
}

#[test]
fn a3_landscape_square_grid_matches_the_frozen_electron_pdf_contract() {
    let request = GeneratedDocumentRequest {
        title: "Untitled".into(),
        width_mm: 420.,
        height_mm: 297.,
        pattern: Some(GeneratedPattern::SquareGrid {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
    };
    let bytes = request.to_pdf_bytes().unwrap();
    let document = Document::load_mem(&bytes).unwrap();
    let pages = document.get_pages();
    assert_eq!(pages.len(), 1);
    let page = document
        .get_object(*pages.get(&1).unwrap())
        .unwrap()
        .as_dict()
        .unwrap();
    let media_box = page.get(b"MediaBox").unwrap().as_array().unwrap();
    assert!((numeric(&media_box[2]) - millimetres_to_pdf_points(420.)).abs() < 0.01);
    assert!((numeric(&media_box[3]) - millimetres_to_pdf_points(297.)).abs() < 0.01);

    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Title (Untitled)"));
    assert!(text.contains("/Creator (Butter Paper)"));
    assert!(text.contains("/Producer (Butter Paper)"));
    assert!(text.contains("butter-paper:page-grid:"));
    assert!(text.contains("/Artifact BMC"));
    assert_eq!(text.matches(" m\n").count(), 70);
    assert_eq!(text.matches(" l\n").count(), 70);
}

#[test]
fn every_frozen_builtin_pattern_generates_tagged_bounded_vector_artwork() {
    let cases = [
        (
            GeneratedPattern::Dots {
                spacing_mm: 10.,
                color: "#d1d5db".into(),
            },
            "rectangular",
        ),
        (
            GeneratedPattern::Ruled {
                spacing_mm: 10.,
                color: "#d1d5db".into(),
            },
            "ruled",
        ),
        (
            GeneratedPattern::Isometric {
                spacing_mm: 10.,
                color: "#d1d5db".into(),
            },
            "isometric",
        ),
        (
            GeneratedPattern::Triangle {
                spacing_mm: 10.,
                color: "#d1d5db".into(),
            },
            "triangle",
        ),
    ];
    for (pattern, subject_type) in cases {
        let bytes = GeneratedDocumentRequest {
            title: "Untitled".into(),
            width_mm: 420.,
            height_mm: 297.,
            pattern: Some(pattern),
        }
        .to_pdf_bytes()
        .unwrap();
        assert!(Document::load_mem(&bytes).is_ok());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Artifact BMC"));
        assert!(text.contains(&format!("\"type\":\"{subject_type}\"")));
        assert!(text.contains("EMC"));
        assert!(bytes.len() < 1_000_000);
    }
}

#[test]
fn validation_and_owned_temporary_cleanup_fail_closed() {
    let mut invalid = GeneratedDocumentRequest::a3_landscape_blank();
    invalid.width_mm = 9.99;
    assert_eq!(
        invalid.to_pdf_bytes().unwrap_err().to_string(),
        "width_mm must be between 10 and 5000 millimetres"
    );

    let root = temporary_store_root("owned-cleanup");
    let _ = fs::remove_dir_all(&root);
    let store = GeneratedDocumentStore::new(root.clone()).unwrap();
    let source = store
        .create(
            "document-1",
            &GeneratedDocumentRequest::a3_landscape_blank(),
        )
        .unwrap();
    assert_eq!(source.path().file_name().unwrap(), "Untitled.pdf");
    assert!(source.path().is_file());
    assert!(source.path().starts_with(&root));

    let outside = root.parent().unwrap().join("not-owned.pdf");
    assert!(store.release_path(&outside).is_err());
    assert!(source.path().is_file());
    store.release(&source).unwrap();
    assert!(!root.join("document-1").exists());
    store.remove_if_empty().unwrap();
    assert!(!root.exists());
}
