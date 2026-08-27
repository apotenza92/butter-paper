use butter_paper_gpui_gallery::annotation_adapter::AnnotationAdapter;

const DENSITY_FIXTURE: &str =
    include_str!("../../performance/fixtures/bp-annotation-density-v1.fixture.json");

#[test]
fn frozen_density_fixture_loads_into_the_normal_document_model() {
    let mut adapter = AnnotationAdapter::default();

    let imported = adapter
        .load_density_fixture(42, DENSITY_FIXTURE)
        .expect("the frozen density fixture must load");

    assert_eq!(imported.fixture_id, "bp-annotation-density-v1");
    assert_eq!(imported.page_count, 100);
    assert_eq!(imported.annotation_count, 1_000);
    assert_eq!(
        adapter
            .snapshot(42)
            .expect("the imported document must exist")
            .rectangles
            .len(),
        1_000
    );
    assert_eq!(adapter.document_scene(42, 1).rectangles.len(), 100);
    assert_eq!(adapter.thumbnail_scene(42, 1).rectangles.len(), 100);
    let snapshot = adapter
        .snapshot(42)
        .expect("the imported document must remain available");
    assert_eq!(
        snapshot
            .rectangles
            .first()
            .expect("first rectangle")
            .id
            .as_str(),
        "bp-annotation-density-v1:p002:rectangle:0001"
    );
    assert_eq!(
        snapshot
            .rectangles
            .last()
            .expect("last rectangle")
            .id
            .as_str(),
        "bp-annotation-density-v1:p100:rectangle:0009"
    );
    assert_eq!(adapter.history_depths(42), (0, 0));
    assert!(!adapter.is_dirty(42));
}

#[test]
fn density_fixture_loader_rejects_recipe_drift_without_replacing_the_document() {
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_density_fixture(42, DENSITY_FIXTURE)
        .expect("the frozen density fixture must load");
    let drifted = DENSITY_FIXTURE.replacen(
        "\"total_annotation_count\": 1000",
        "\"total_annotation_count\": 999",
        1,
    );

    let error = adapter
        .load_density_fixture(42, &drifted)
        .expect_err("fixture recipe drift must be rejected");

    assert!(error.to_string().contains("frozen v1 recipe"));
    assert_eq!(
        adapter
            .snapshot(42)
            .expect("a rejected load must preserve the current document")
            .rectangles
            .len(),
        1_000
    );
}
