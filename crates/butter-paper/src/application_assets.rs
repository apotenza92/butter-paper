use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub const PAGE_THUMBNAILS_ICON: &str = "icons/page-thumbnails.svg";
pub const DOWNLOAD_ICON: &str = "icons/download.svg";
pub const SIGNATURE_VALID_ICON: &str = "icons/shield-check.svg";
pub const SIGNATURE_UNVERIFIED_ICON: &str = "icons/shield-alert.svg";
pub const SIGNATURE_INVALID_ICON: &str = "icons/rail/shield-x.svg";
pub const EDIT_PDF_ICON: &str = "icons/file-pen-line.svg";
pub const NEW_DOCUMENT_ICON: &str = "icons/file-plus.svg";
/// The template library: New from template's second half opens it.
pub const TEMPLATE_LIBRARY_ICON: &str = "icons/book-open.svg";
pub const LOCK_ICON: &str = "icons/lock.svg";
pub const LOCK_OPEN_ICON: &str = "icons/lock-open.svg";
/// Settings ▸ Mouse wheel.
pub const MOUSE_WHEEL_ICON: &str = "icons/mouse.svg";
/// The find bar's Match whole word toggle.
pub const WHOLE_WORD_ICON: &str = "icons/whole-word.svg";
/// The search panel's Collapse results toggle.
pub const LIST_COLLAPSE_ICON: &str = "icons/list-collapse.svg";
/// The search panel's More context toggle.
pub const MORE_CONTEXT_ICON: &str = "icons/unfold-vertical.svg";
/// A markup in the search panel's results.
pub const MARKUP_RESULT_ICON: &str = "icons/message-square.svg";
/// The Edit PDF format bar's Bold, Italic and alignment buttons.
pub const FORMAT_BOLD_ICON: &str = "icons/format/bold.svg";
pub const FORMAT_ITALIC_ICON: &str = "icons/format/italic.svg";
pub const FORMAT_ALIGN_LEFT_ICON: &str = "icons/format/text-align-start.svg";
pub const FORMAT_ALIGN_CENTRE_ICON: &str = "icons/format/text-align-center.svg";
pub const FORMAT_ALIGN_RIGHT_ICON: &str = "icons/format/text-align-end.svg";
pub const FORMAT_ALIGN_JUSTIFY_ICON: &str = "icons/format/text-align-justify.svg";
const FORMAT_ICONS: [(&str, &[u8]); 6] = [
    (
        FORMAT_BOLD_ICON,
        include_bytes!("../assets/icons/format/bold.svg"),
    ),
    (
        FORMAT_ITALIC_ICON,
        include_bytes!("../assets/icons/format/italic.svg"),
    ),
    (
        FORMAT_ALIGN_LEFT_ICON,
        include_bytes!("../assets/icons/format/text-align-start.svg"),
    ),
    (
        FORMAT_ALIGN_CENTRE_ICON,
        include_bytes!("../assets/icons/format/text-align-center.svg"),
    ),
    (
        FORMAT_ALIGN_RIGHT_ICON,
        include_bytes!("../assets/icons/format/text-align-end.svg"),
    ),
    (
        FORMAT_ALIGN_JUSTIFY_ICON,
        include_bytes!("../assets/icons/format/text-align-justify.svg"),
    ),
];

/// Product icons supplement, rather than modify, the stock component assets.
pub struct ApplicationAssets;

impl AssetSource for ApplicationAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let thumbnail: Option<&'static [u8]> = match path {
            DOWNLOAD_ICON => Some(include_bytes!("../assets/icons/download.svg")),
            SIGNATURE_VALID_ICON => Some(include_bytes!("../assets/icons/shield-check.svg")),
            SIGNATURE_UNVERIFIED_ICON => Some(include_bytes!("../assets/icons/shield-alert.svg")),
            EDIT_PDF_ICON => Some(include_bytes!("../assets/icons/file-pen-line.svg")),
            NEW_DOCUMENT_ICON => Some(include_bytes!("../assets/icons/file-plus.svg")),
            TEMPLATE_LIBRARY_ICON => Some(include_bytes!("../assets/icons/book-open.svg")),
            LOCK_ICON => Some(include_bytes!("../assets/icons/lock.svg")),
            WHOLE_WORD_ICON => Some(include_bytes!("../assets/icons/whole-word.svg")),
            LIST_COLLAPSE_ICON => Some(include_bytes!("../assets/icons/list-collapse.svg")),
            MORE_CONTEXT_ICON => Some(include_bytes!("../assets/icons/unfold-vertical.svg")),
            MARKUP_RESULT_ICON => Some(include_bytes!("../assets/icons/message-square.svg")),
            LOCK_OPEN_ICON => Some(include_bytes!("../assets/icons/lock-open.svg")),
            MOUSE_WHEEL_ICON => Some(include_bytes!("../assets/icons/mouse.svg")),
            "icons/grip-vertical.svg" => Some(include_bytes!("../assets/icons/grip-vertical.svg")),
            "icons/thumbnail-rotate-left.svg" => {
                Some(include_bytes!("../assets/icons/thumbnail-rotate-left.svg"))
            }
            "icons/thumbnail-rotate-right.svg" => {
                Some(include_bytes!("../assets/icons/thumbnail-rotate-right.svg"))
            }
            "icons/rail/mouse-pointer-2.svg" => {
                Some(include_bytes!("../assets/icons/rail/mouse-pointer-2.svg"))
            }
            "icons/rail/hand.svg" => Some(include_bytes!("../assets/icons/rail/hand.svg")),
            "icons/rail/type.svg" => Some(include_bytes!("../assets/icons/rail/type.svg")),
            "icons/rail/text-cursor-input.svg" => {
                Some(include_bytes!("../assets/icons/rail/text-cursor-input.svg"))
            }
            "icons/rail/arrow-right.svg" => {
                Some(include_bytes!("../assets/icons/rail/arrow-right.svg"))
            }
            "icons/rail/highlighter.svg" => {
                Some(include_bytes!("../assets/icons/rail/highlighter.svg"))
            }
            "icons/rail/cloud.svg" => Some(include_bytes!("../assets/icons/rail/cloud.svg")),
            "icons/rail/shield-x.svg" => Some(include_bytes!("../assets/icons/rail/shield-x.svg")),
            "icons/rail/signature.svg" => {
                Some(include_bytes!("../assets/icons/rail/signature.svg"))
            }
            "icons/rail/image.svg" => Some(include_bytes!("../assets/icons/rail/image.svg")),
            "icons/rail/scan-search.svg" => {
                Some(include_bytes!("../assets/icons/rail/scan-search.svg"))
            }
            "icons/rail/square.svg" => Some(include_bytes!("../assets/icons/rail/square.svg")),
            "icons/rail/circle.svg" => Some(include_bytes!("../assets/icons/rail/circle.svg")),
            "icons/rail/minus.svg" => Some(include_bytes!("../assets/icons/rail/minus.svg")),
            "icons/rail/waypoints.svg" => {
                Some(include_bytes!("../assets/icons/rail/waypoints.svg"))
            }
            "icons/rail/pen-line.svg" => Some(include_bytes!("../assets/icons/rail/pen-line.svg")),
            "icons/rail/spline.svg" => Some(include_bytes!("../assets/icons/rail/spline.svg")),
            "icons/rail/pentagon.svg" => Some(include_bytes!("../assets/icons/rail/pentagon.svg")),
            "icons/rail/dimension.svg" => {
                Some(include_bytes!("../assets/icons/rail/dimension.svg"))
            }
            "icons/rail/scan-line.svg" => {
                Some(include_bytes!("../assets/icons/rail/scan-line.svg"))
            }
            "icons/rail/ruler-dimension-line.svg" => Some(include_bytes!(
                "../assets/icons/rail/ruler-dimension-line.svg"
            )),
            "icons/rail/magnet.svg" => Some(include_bytes!("../assets/icons/rail/magnet.svg")),
            "icons/rail/sliders-horizontal.svg" => Some(include_bytes!(
                "../assets/icons/rail/sliders-horizontal.svg"
            )),
            "icons/rail/cloud-plus.svg" => {
                Some(include_bytes!("../assets/icons/rail/cloud-plus.svg"))
            }
            "icons/rail/callout.svg" => Some(include_bytes!("../assets/icons/rail/callout.svg")),
            "icons/rail/ellipsis.svg" => Some(include_bytes!("../assets/icons/rail/ellipsis.svg")),
            "icons/snap/square.svg" => Some(include_bytes!("../assets/icons/snap/square.svg")),
            "icons/snap/triangle.svg" => Some(include_bytes!("../assets/icons/snap/triangle.svg")),
            "icons/snap/circle.svg" => Some(include_bytes!("../assets/icons/snap/circle.svg")),
            "icons/snap/x.svg" => Some(include_bytes!("../assets/icons/snap/x.svg")),
            "icons/snap/diamond.svg" => Some(include_bytes!("../assets/icons/snap/diamond.svg")),
            _ => None,
        };
        if let Some(bytes) = thumbnail {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if let Some((_, bytes)) = FORMAT_ICONS.iter().find(|(icon, _)| *icon == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if path == PAGE_THUMBNAILS_ICON {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/page-thumbnails.svg"
            ))));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut entries = gpui_component_assets::Assets.list(path)?;
        for (icon, _) in FORMAT_ICONS {
            if icon.starts_with(path) {
                entries.push(icon.into());
            }
        }
        for icon in [
            EDIT_PDF_ICON,
            MOUSE_WHEEL_ICON,
            WHOLE_WORD_ICON,
            "icons/grip-vertical.svg",
            "icons/thumbnail-rotate-left.svg",
            "icons/thumbnail-rotate-right.svg",
        ] {
            if icon.starts_with(path) {
                entries.push(icon.into());
            }
        }
        if PAGE_THUMBNAILS_ICON.starts_with(path) {
            entries.push(PAGE_THUMBNAILS_ICON.into());
        }
        if DOWNLOAD_ICON.starts_with(path) {
            entries.push(DOWNLOAD_ICON.into());
        }
        if "icons/rail/mouse-pointer-2.svg".starts_with(path) {
            entries.push("icons/rail/mouse-pointer-2.svg".into());
        }
        if "icons/rail/hand.svg".starts_with(path) {
            entries.push("icons/rail/hand.svg".into());
        }
        if "icons/rail/type.svg".starts_with(path) {
            entries.push("icons/rail/type.svg".into());
        }
        if "icons/rail/arrow-right.svg".starts_with(path) {
            entries.push("icons/rail/arrow-right.svg".into());
        }
        if "icons/rail/highlighter.svg".starts_with(path) {
            entries.push("icons/rail/highlighter.svg".into());
        }
        if "icons/rail/cloud.svg".starts_with(path) {
            entries.push("icons/rail/cloud.svg".into());
        }
        if "icons/rail/shield-x.svg".starts_with(path) {
            entries.push("icons/rail/shield-x.svg".into());
        }
        if "icons/rail/signature.svg".starts_with(path) {
            entries.push("icons/rail/signature.svg".into());
        }
        if "icons/rail/image.svg".starts_with(path) {
            entries.push("icons/rail/image.svg".into());
        }
        if "icons/rail/scan-search.svg".starts_with(path) {
            entries.push("icons/rail/scan-search.svg".into());
        }
        if "icons/rail/square.svg".starts_with(path) {
            entries.push("icons/rail/square.svg".into());
        }
        if "icons/rail/circle.svg".starts_with(path) {
            entries.push("icons/rail/circle.svg".into());
        }
        if "icons/rail/minus.svg".starts_with(path) {
            entries.push("icons/rail/minus.svg".into());
        }
        if "icons/rail/waypoints.svg".starts_with(path) {
            entries.push("icons/rail/waypoints.svg".into());
        }
        if "icons/rail/pen-line.svg".starts_with(path) {
            entries.push("icons/rail/pen-line.svg".into());
        }
        if "icons/rail/spline.svg".starts_with(path) {
            entries.push("icons/rail/spline.svg".into());
        }
        if "icons/rail/pentagon.svg".starts_with(path) {
            entries.push("icons/rail/pentagon.svg".into());
        }
        if "icons/rail/dimension.svg".starts_with(path) {
            entries.push("icons/rail/dimension.svg".into());
        }
        if "icons/rail/scan-line.svg".starts_with(path) {
            entries.push("icons/rail/scan-line.svg".into());
        }
        if "icons/rail/ruler-dimension-line.svg".starts_with(path) {
            entries.push("icons/rail/ruler-dimension-line.svg".into());
        }
        if "icons/rail/magnet.svg".starts_with(path) {
            entries.push("icons/rail/magnet.svg".into());
        }
        if "icons/rail/sliders-horizontal.svg".starts_with(path) {
            entries.push("icons/rail/sliders-horizontal.svg".into());
        }
        if "icons/rail/cloud-plus.svg".starts_with(path) {
            entries.push("icons/rail/cloud-plus.svg".into());
        }
        if "icons/rail/callout.svg".starts_with(path) {
            entries.push("icons/rail/callout.svg".into());
        }
        if "icons/rail/ellipsis.svg".starts_with(path) {
            entries.push("icons/rail/ellipsis.svg".into());
        }
        if "icons/snap/square.svg".starts_with(path) {
            entries.push("icons/snap/square.svg".into());
        }
        if "icons/snap/triangle.svg".starts_with(path) {
            entries.push("icons/snap/triangle.svg".into());
        }
        if "icons/snap/circle.svg".starts_with(path) {
            entries.push("icons/snap/circle.svg".into());
        }
        if "icons/snap/x.svg".starts_with(path) {
            entries.push("icons/snap/x.svg".into());
        }
        if "icons/snap/diamond.svg".starts_with(path) {
            entries.push("icons/snap/diamond.svg".into());
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_page_glyph_and_preserves_stock_assets() {
        let bytes = ApplicationAssets
            .load(PAGE_THUMBNAILS_ICON)
            .unwrap()
            .unwrap();
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(
            svg.contains("M16.706 2.706"),
            "Files must retain the folded page corner"
        );
        assert!(svg.contains("stroke-width=\"2.25\""));
        assert!(ApplicationAssets.load("icons/copy.svg").unwrap().is_some());
        assert!(
            ApplicationAssets
                .list("icons/")
                .unwrap()
                .iter()
                .any(|p| p == PAGE_THUMBNAILS_ICON)
        );
    }

    #[test]
    fn every_rail_icon_is_embedded_and_loadable() {
        let icons = ApplicationAssets.list("icons/rail/").unwrap();
        assert_eq!(icons.len(), 25);
        for path in icons {
            let bytes = ApplicationAssets
                .load(&path)
                .unwrap()
                .expect("embedded rail icon");
            let svg = std::str::from_utf8(&bytes).unwrap();
            let view_box = if path == "icons/rail/cloud-plus.svg" {
                "-6 -6 36 36"
            } else {
                "0 0 24 24"
            };
            assert!(svg.contains(&format!("viewBox=\"{view_box}\"")), "{path}");
            assert!(svg.contains("</svg>"), "{path}");
        }
    }

    #[test]
    fn composite_rail_glyphs_preserve_reference_size_and_absolute_stroke() {
        // Electron centres a 6px Type in Callout. Cloud+ is a larger Cloud
        // glyph at lower left, the Callout glyph at upper right and a
        // straight line from the Callout's tail tip to the Cloud. Compensate
        // stroke before scaling each SVG group, so every stroke is 1.5px
        // wide at the 16px icon size (the 36 unit box is drawn at 24px).
        for (path, transform, stroke, scale) in [
            (
                "icons/rail/callout.svg",
                "translate(7.5 7.5) scale(.375)",
                "6",
                0.375_f64,
            ),
            (
                "icons/rail/cloud-plus.svg",
                "translate(-7.125 7.5) scale(1.125)",
                "2",
                1.125_f64,
            ),
            (
                "icons/rail/cloud-plus.svg",
                "translate(9.625 -7.5) scale(.875)",
                "2.571429",
                0.875_f64,
            ),
        ] {
            let bytes = ApplicationAssets.load(path).unwrap().unwrap();
            let svg = std::str::from_utf8(&bytes).unwrap();
            assert!(
                svg.contains(&format!(
                    "transform=\"{transform}\" stroke-width=\"{stroke}\""
                )),
                "{path} {transform}"
            );
            assert!((stroke.parse::<f64>().unwrap() * scale * 16. / 24. - 1.5).abs() < 0.00001);
        }
        let bytes = ApplicationAssets
            .load("icons/rail/cloud-plus.svg")
            .unwrap()
            .unwrap();
        let svg = std::str::from_utf8(&bytes).unwrap();
        // The Callout's Type keeps callout.svg's weight against its outline
        // (both 2.25 once scaled), inside the scaled Callout group.
        assert!(
            svg.contains(
                r#"<g transform="translate(7.5 7.5) scale(.375)" stroke-width="6.857143">"#
            )
        );
        assert!((6.857143_f64 * 0.375 * 0.875 * 16. / 24. - 1.5).abs() < 0.00001);
        // The link has no scale group: it keeps the default 2.25 stroke,
        // which is 1.5px.
        assert!(svg.contains(r#"<path d="M12.436 11.564 8.568 15.432"/></svg>"#));
        assert!(svg.contains("stroke-width=\"2.25\""));
        assert!(
            svg.contains("stroke=\"currentColor\"") && !svg.contains('#'),
            "Cloud+ draws in the theme's colour only"
        );
        assert!(
            svg.contains(r#"<path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"/>"#),
            "Cloud+ holds the Cloud glyph as cloud.svg draws it"
        );
        let callout = ApplicationAssets
            .load("icons/rail/callout.svg")
            .unwrap()
            .unwrap();
        let callout = std::str::from_utf8(&callout).unwrap();
        let callout_path = callout.split("<path d=\"").nth(1).unwrap();
        let callout_path = &callout_path[..callout_path.find('"').unwrap()];
        assert!(
            svg.contains(&format!("<path d=\"{callout_path}\"/>")),
            "Cloud+ holds the Callout glyph as callout.svg draws it"
        );
        // The overscan holds every stroke whole, not just its centreline.
        // Cloud: x 2..22, y 5..19 scaled 1.125 and moved by (-7.125, 7.5).
        // Callout: x 2..22, y 3..21.788 scaled .875 and moved by (9.625, -7.5).
        let half = 2.25_f64 / 2.;
        let cloud = (
            2. * 1.125 - 7.125,
            22. * 1.125 - 7.125,
            5. * 1.125 + 7.5,
            19. * 1.125 + 7.5,
        );
        let callout = (
            2. * 0.875 + 9.625,
            22. * 0.875 + 9.625,
            3. * 0.875 - 7.5,
            21.788 * 0.875 - 7.5,
        );
        assert!(cloud.0 - half >= -6. && cloud.3 + half <= 30.);
        assert!(callout.1 + half <= 30. && callout.2 - half >= -6.);
        // The Cloud is larger than the 20 unit Cloud glyph that came before,
        // and the Callout stays near its 18.75 unit size.
        assert!(cloud.1 - cloud.0 > 22. && callout.1 - callout.0 > 17.);
        // The link runs from the Callout's tail tip, down and to the left at
        // 45 degrees, to the Cloud's outline, and is long enough to read.
        let tail = (3.212 * 0.875 + 9.625, 21.788 * 0.875 - 7.5);
        let (start, end) = ((12.436_f64, 11.564_f64), (8.568_f64, 15.432_f64));
        assert!(
            (start.0 - tail.0).hypot(start.1 - tail.1) < 0.01,
            "{tail:?}"
        );
        assert!(((start.0 - end.0) - (end.1 - start.1)).abs() < 0.001);
        let length = (start.0 - end.0).hypot(end.1 - start.1);
        assert!((5. ..=6.).contains(&length), "{length}");
        // The end lies on the Cloud's big bulge: centre (9, 12), radius 7.
        let bulge = (9. * 1.125 - 7.125, 12. * 1.125 + 7.5, 7. * 1.125);
        assert!(((end.0 - bulge.0).hypot(end.1 - bulge.1) - bulge.2).abs() < 0.05);
        // Away from the tail the two outlines stand a stroke's width apart
        // or more. The Callout's bottom edge (y = 19) clears the Cloud's
        // right bump, whose top is y = 10 and whose centre is (17.5, 14.5).
        let bump = (17.5 * 1.125 - 7.125, 14.5 * 1.125 + 7.5, 4.5 * 1.125);
        let bottom = 19. * 0.875 - 7.5;
        assert!(bump.1 - bump.2 - bottom - 2. * half >= 2.25);
        assert_eq!(24. / 36., 16. / 24.);
    }
}
