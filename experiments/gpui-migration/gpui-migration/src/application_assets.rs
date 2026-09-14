use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub const PAGE_THUMBNAILS_ICON: &str = "icons/page-thumbnails.svg";

/// Product icons supplement, rather than modify, the stock component assets.
pub struct ApplicationAssets;

impl AssetSource for ApplicationAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let thumbnail: Option<&'static [u8]> = match path {
            "icons/thumbnail-scale.svg" => Some(include_bytes!("../assets/icons/thumbnail-scale.svg")),
            "icons/thumbnail-rotate-left.svg" => Some(include_bytes!("../assets/icons/thumbnail-rotate-left.svg")),
            "icons/thumbnail-rotate-right.svg" => Some(include_bytes!("../assets/icons/thumbnail-rotate-right.svg")),
            "icons/rail/mouse-pointer-2.svg" => Some(include_bytes!("../assets/icons/rail/mouse-pointer-2.svg")),
            "icons/rail/hand.svg" => Some(include_bytes!("../assets/icons/rail/hand.svg")),
            "icons/rail/type.svg" => Some(include_bytes!("../assets/icons/rail/type.svg")),
            "icons/rail/arrow-right.svg" => Some(include_bytes!("../assets/icons/rail/arrow-right.svg")),
            "icons/rail/highlighter.svg" => Some(include_bytes!("../assets/icons/rail/highlighter.svg")),
            "icons/rail/cloud.svg" => Some(include_bytes!("../assets/icons/rail/cloud.svg")),
            "icons/rail/shield-x.svg" => Some(include_bytes!("../assets/icons/rail/shield-x.svg")),
            "icons/rail/signature.svg" => Some(include_bytes!("../assets/icons/rail/signature.svg")),
            "icons/rail/image.svg" => Some(include_bytes!("../assets/icons/rail/image.svg")),
            "icons/rail/scan-search.svg" => Some(include_bytes!("../assets/icons/rail/scan-search.svg")),
            "icons/rail/square.svg" => Some(include_bytes!("../assets/icons/rail/square.svg")),
            "icons/rail/circle.svg" => Some(include_bytes!("../assets/icons/rail/circle.svg")),
            "icons/rail/minus.svg" => Some(include_bytes!("../assets/icons/rail/minus.svg")),
            "icons/rail/waypoints.svg" => Some(include_bytes!("../assets/icons/rail/waypoints.svg")),
            "icons/rail/pen-line.svg" => Some(include_bytes!("../assets/icons/rail/pen-line.svg")),
            "icons/rail/spline.svg" => Some(include_bytes!("../assets/icons/rail/spline.svg")),
            "icons/rail/pentagon.svg" => Some(include_bytes!("../assets/icons/rail/pentagon.svg")),
            "icons/rail/ruler.svg" => Some(include_bytes!("../assets/icons/rail/ruler.svg")),
            "icons/rail/scan-line.svg" => Some(include_bytes!("../assets/icons/rail/scan-line.svg")),
            "icons/rail/ruler-dimension-line.svg" => Some(include_bytes!("../assets/icons/rail/ruler-dimension-line.svg")),
            "icons/rail/route.svg" => Some(include_bytes!("../assets/icons/rail/route.svg")),
            "icons/rail/chart-area.svg" => Some(include_bytes!("../assets/icons/rail/chart-area.svg")),
            "icons/rail/magnet.svg" => Some(include_bytes!("../assets/icons/rail/magnet.svg")),
            "icons/rail/sliders-horizontal.svg" => Some(include_bytes!("../assets/icons/rail/sliders-horizontal.svg")),
            "icons/rail/cloud-plus.svg" => Some(include_bytes!("../assets/icons/rail/cloud-plus.svg")),
            "icons/rail/callout.svg" => Some(include_bytes!("../assets/icons/rail/callout.svg")),
            _ => None,
        };
        if let Some(bytes) = thumbnail { return Ok(Some(Cow::Borrowed(bytes))); }
        if path == PAGE_THUMBNAILS_ICON {
            return Ok(Some(Cow::Borrowed(include_bytes!("../assets/icons/page-thumbnails.svg"))));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut entries = gpui_component_assets::Assets.list(path)?;
        for icon in ["icons/thumbnail-scale.svg", "icons/thumbnail-rotate-left.svg", "icons/thumbnail-rotate-right.svg"] {
            if icon.starts_with(path) { entries.push(icon.into()); }
        }
        if PAGE_THUMBNAILS_ICON.starts_with(path) {
            entries.push(PAGE_THUMBNAILS_ICON.into());
        }
        if "icons/rail/mouse-pointer-2.svg".starts_with(path) { entries.push("icons/rail/mouse-pointer-2.svg".into()); }
        if "icons/rail/hand.svg".starts_with(path) { entries.push("icons/rail/hand.svg".into()); }
        if "icons/rail/type.svg".starts_with(path) { entries.push("icons/rail/type.svg".into()); }
        if "icons/rail/arrow-right.svg".starts_with(path) { entries.push("icons/rail/arrow-right.svg".into()); }
        if "icons/rail/highlighter.svg".starts_with(path) { entries.push("icons/rail/highlighter.svg".into()); }
        if "icons/rail/cloud.svg".starts_with(path) { entries.push("icons/rail/cloud.svg".into()); }
        if "icons/rail/shield-x.svg".starts_with(path) { entries.push("icons/rail/shield-x.svg".into()); }
        if "icons/rail/signature.svg".starts_with(path) { entries.push("icons/rail/signature.svg".into()); }
        if "icons/rail/image.svg".starts_with(path) { entries.push("icons/rail/image.svg".into()); }
        if "icons/rail/scan-search.svg".starts_with(path) { entries.push("icons/rail/scan-search.svg".into()); }
        if "icons/rail/square.svg".starts_with(path) { entries.push("icons/rail/square.svg".into()); }
        if "icons/rail/circle.svg".starts_with(path) { entries.push("icons/rail/circle.svg".into()); }
        if "icons/rail/minus.svg".starts_with(path) { entries.push("icons/rail/minus.svg".into()); }
        if "icons/rail/waypoints.svg".starts_with(path) { entries.push("icons/rail/waypoints.svg".into()); }
        if "icons/rail/pen-line.svg".starts_with(path) { entries.push("icons/rail/pen-line.svg".into()); }
        if "icons/rail/spline.svg".starts_with(path) { entries.push("icons/rail/spline.svg".into()); }
        if "icons/rail/pentagon.svg".starts_with(path) { entries.push("icons/rail/pentagon.svg".into()); }
        if "icons/rail/ruler.svg".starts_with(path) { entries.push("icons/rail/ruler.svg".into()); }
        if "icons/rail/scan-line.svg".starts_with(path) { entries.push("icons/rail/scan-line.svg".into()); }
        if "icons/rail/ruler-dimension-line.svg".starts_with(path) { entries.push("icons/rail/ruler-dimension-line.svg".into()); }
        if "icons/rail/route.svg".starts_with(path) { entries.push("icons/rail/route.svg".into()); }
        if "icons/rail/chart-area.svg".starts_with(path) { entries.push("icons/rail/chart-area.svg".into()); }
        if "icons/rail/magnet.svg".starts_with(path) { entries.push("icons/rail/magnet.svg".into()); }
        if "icons/rail/sliders-horizontal.svg".starts_with(path) { entries.push("icons/rail/sliders-horizontal.svg".into()); }
        if "icons/rail/cloud-plus.svg".starts_with(path) { entries.push("icons/rail/cloud-plus.svg".into()); }
        if "icons/rail/callout.svg".starts_with(path) { entries.push("icons/rail/callout.svg".into()); }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_page_glyph_and_preserves_stock_assets() {
        let bytes = ApplicationAssets.load(PAGE_THUMBNAILS_ICON).unwrap().unwrap();
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains("M16.706 2.706"), "Files must retain the folded page corner");
        assert!(svg.contains("stroke-width=\"2.25\""));
        assert!(ApplicationAssets.load("icons/copy.svg").unwrap().is_some());
        assert!(ApplicationAssets.list("icons/").unwrap().iter().any(|p| p == PAGE_THUMBNAILS_ICON));
    }

    #[test]
    fn every_rail_icon_is_embedded_and_loadable() {
        let icons = ApplicationAssets.list("icons/rail/").unwrap();
        assert_eq!(icons.len(), 26);
        for path in icons {
            let bytes = ApplicationAssets.load(&path).unwrap().expect("embedded rail icon");
            let svg = std::str::from_utf8(&bytes).unwrap();
            let view_box = if path == "icons/rail/cloud-plus.svg" { "-6 -6 36 36" } else { "0 0 24 24" };
            assert!(svg.contains(&format!("viewBox=\"{view_box}\"")), "{path}");
            assert!(svg.contains("</svg>"), "{path}");
        }
    }

    #[test]
    fn composite_rail_glyphs_preserve_reference_size_and_absolute_stroke() {
        // Electron centres a 6px Type in Callout and positions a 7px Type at
        // right/top -2px in Cloud+. Compensate stroke before scaling the SVG group.
        for (path, transform, stroke, scale) in [
            ("icons/rail/callout.svg", "translate(7.5 7.5) scale(.375)", "6", 0.375_f64),
            ("icons/rail/cloud-plus.svg", "translate(16.5 -3) scale(.4375)", "5.142857", 0.4375_f64),
        ] {
            let bytes = ApplicationAssets.load(path).unwrap().unwrap();
            let svg = std::str::from_utf8(&bytes).unwrap();
            assert!(svg.contains(&format!("transform=\"{transform}\" stroke-width=\"{stroke}\"")));
            assert!((stroke.parse::<f64>().unwrap() * scale * 16. / 24. - 1.5).abs() < 0.000001);
        }
        // Cloud+ overscan contains the full Type stroke, not just its centreline.
        let stroke_radius = 5.142857_f64 * 0.4375 / 2.;
        assert!(-3. + 4. * 0.4375 - stroke_radius >= -6.);
        assert!(16.5 + 20. * 0.4375 + stroke_radius <= 30.);
        assert_eq!(24. / 36., 16. / 24.);
    }
}
