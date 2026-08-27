use butter_paper_gpui_gallery::pdf_worker::{PageGeometry, Rotation};

#[test]
fn page_geometry_protocol_carries_user_unit_without_pdfium_types() {
    let geometry = PageGeometry {
        media_box: [0.0, 0.0, 612.0, 792.0],
        crop_box: [36.0, 72.0, 576.0, 792.0],
        rotation: Rotation::Degrees90,
        display_width_points: 1_440.0,
        display_height_points: 1_080.0,
        user_unit: 2.0,
    };
    let encoded = serde_json::to_string(&geometry).unwrap();
    assert!(encoded.contains("\"user_unit\":2.0"));
    assert_eq!(serde_json::from_str::<PageGeometry>(&encoded).unwrap(), geometry);
}

#[test]
fn older_geometry_payloads_default_to_one_user_unit() {
    let encoded = r#"{
        "media_box":[0.0,0.0,612.0,792.0],
        "crop_box":[0.0,0.0,612.0,792.0],
        "rotation":"degrees0",
        "display_width_points":612.0,
        "display_height_points":792.0
    }"#;
    assert_eq!(serde_json::from_str::<PageGeometry>(encoded).unwrap().user_unit, 1.0);
}
