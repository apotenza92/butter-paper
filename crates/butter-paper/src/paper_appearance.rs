//! Dark pages: the colour map that draws a page's ink and paper dark for
//! the view only.
//!
//! The map has three steps:
//!
//! 1. Paper unblend. PDFium mixes anti-aliased edges and transparency in
//!    gamma-encoded sRGB over white paper. Each pixel `c` is split into an
//!    ink colour `p` over white with coverage `α = 1 − min(c)`.
//! 2. Ink map. In OKLab, dark ink colours become light colours of the same
//!    hue. Colours that are already light (yellow, cyan) stay. Chroma is
//!    reduced only as far as the sRGB gamut needs.
//! 3. Dark paper. The mapped ink is put back over the dark paper colour
//!    with the same coverage `α`.
//!
//! The map is pure and never touches a document. Real-colour paths (save,
//! export, print, copies) do not use it. Ideas come from Zathura's
//! `recolor-keephue` (zlib) and Dark Reader (MIT). No code is copied. OKLab
//! is Björn Ottosson's colour space (public domain matrices).

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// How a view draws page paper and ink. Every path that must give the
/// document's own colours (save, export, print, copies, previews) uses
/// [`PaperAppearance::Real`] and does not take a paper argument.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperAppearance {
    /// The document's own colours.
    #[default]
    Real,
    /// Dark pages: ink and paper go through [`DarkPaperMap`]. Pictures keep
    /// their real colours.
    Dark,
}

/// The Page Appearance setting: View ▸ Page Appearance and Settings ▸
/// Appearance. One global choice, kept across launches. Original is the
/// default. It shows the PDF as it is, so it is not a light mode.
///
/// It is independent of the app theme (App Appearance) except for Match App,
/// which follows the theme the app shows now.
///
/// Profiles from 0.6.0 stored `light`, `dark` and `match_system`: they load
/// as Original, Force Dark and Match App.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PageAppearance {
    /// The document's own colours.
    #[default]
    #[serde(alias = "light")]
    Original,
    /// Dark pages, whatever the app shows.
    #[serde(alias = "dark")]
    ForceDark,
    /// Dark pages while the app is dark, original pages while it is light.
    #[serde(alias = "match_system", alias = "system")]
    MatchApp,
}

impl PageAppearance {
    pub const ALL: [Self; 3] = [Self::Original, Self::ForceDark, Self::MatchApp];

    /// The name in the menus and in Settings.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::ForceDark => "Force Dark",
            Self::MatchApp => "Match App",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Original => "Pages show the PDF as it is.",
            Self::ForceDark => {
                "Pages always show dark paper and light ink. Pictures keep their colours."
            }
            Self::MatchApp => {
                "Pages are dark while the app is dark, and show the PDF as it is while the \
                 app is light."
            }
        }
    }

    /// The paper a view draws for this setting, given whether the app theme
    /// is dark now (App Appearance, which follows the system for Match
    /// System).
    pub const fn resolve(self, app_is_dark: bool) -> PaperAppearance {
        match self {
            Self::Original => PaperAppearance::Real,
            Self::ForceDark => PaperAppearance::Dark,
            Self::MatchApp => {
                if app_is_dark {
                    PaperAppearance::Dark
                } else {
                    PaperAppearance::Real
                }
            }
        }
    }
}

/// A page is "mostly pictures" from this fraction of its area.
pub const PICTURE_PAGE_COVERAGE: f32 = 0.5;

/// The status bar line for a page that is mostly pictures and shows on
/// dark paper: its pictures (a scan) keep their real colours. `coverage` is
/// the fraction the worker reported for the paper the page image was drawn
/// with (`drawn_with`).
pub fn pictures_keep_colours_note(
    paper: PaperAppearance,
    drawn_with: PaperAppearance,
    coverage: Option<f32>,
) -> Option<&'static str> {
    (paper == PaperAppearance::Dark
        && drawn_with == PaperAppearance::Dark
        && coverage.is_some_and(|coverage| coverage >= PICTURE_PAGE_COVERAGE))
    .then_some("Pictures keep their real colours in Dark pages")
}

/// The two end colours of the dark-page map, as sRGB.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DarkPaper {
    /// The colour white paper becomes.
    pub paper: [u8; 3],
    /// The colour black ink becomes.
    pub ink: [u8; 3],
}

impl DarkPaper {
    /// `#1E1E1E` paper with `#E6E6E6` ink: 13.4:1, without the halation
    /// of pure white on pure black.
    pub const DEFAULT: Self = Self {
        paper: [0x1E, 0x1E, 0x1E],
        ink: [0xE6, 0xE6, 0xE6],
    };

    /// The map of one sRGB colour, computed directly in floating point.
    /// Use it for a paint colour (one call for each colour). Use
    /// [`DarkPaperMap`] for pixels.
    pub fn map_colour(self, colour: [u8; 3]) -> [u8; 3] {
        let c = colour.map(unit);
        let m = c[0].min(c[1]).min(c[2]);
        let alpha = 1. - m;
        if alpha <= 0. {
            return self.paper;
        }
        let p = c.map(|channel| (channel - m) / alpha);
        let ink = self.ink_map(p);
        let paper = self.paper.map(unit);
        [0, 1, 2].map(|i| to_byte(alpha * ink[i] + (1. - alpha) * paper[i]))
    }

    /// Step 2: the mapped colour of ink `p` (sRGB, 0 to 1, one channel 0).
    fn ink_map(self, p: [f64; 3]) -> [f64; 3] {
        let [lightness, a, b] = oklab(p);
        let chroma = a.hypot(b);
        let weight = smoothstep(0.02, 0.10, chroma);
        let neutral_lightness = oklab(self.ink.map(unit))[0];
        let high = mix(neutral_lightness, 0.80, weight);
        let low = mix(1.0, 0.65, weight);
        let mapped = if lightness < low {
            high - (high - low) * lightness / low
        } else {
            lightness
        };
        let hue = b.atan2(a);
        in_gamut_srgb(mapped, chroma, hue)
    }
}

impl Default for DarkPaper {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The dark-page map as a lookup table, for pixels.
///
/// The ink colour `p` of step 1 always has one channel at 0. So a table of
/// 3 × 256 × 256 ink colours (about 590 KB) holds step 2 for every 8-bit
/// `p`. Steps 1 and 3 are integer operations.
pub struct DarkPaperMap {
    colours: DarkPaper,
    /// The mapped ink for each `p`: index `zero × 65536 + u × 256 + v`,
    /// where `zero` is the channel at 0 and `u`, `v` are the other two
    /// channels in RGB order.
    ink: Box<[[u8; 3]]>,
    /// `255 × 65536 / α`, rounded, for each coverage `α`.
    reciprocal: [u32; 256],
}

impl DarkPaperMap {
    /// Builds the table for `colours`. This takes tens of milliseconds in
    /// an optimised build. Keep the result.
    pub fn new(colours: DarkPaper) -> Self {
        let mut ink = Vec::with_capacity(3 * 256 * 256);
        for zero in 0..3 {
            for u in 0..=255u8 {
                for v in 0..=255u8 {
                    let p = ink_from_index(zero, u, v).map(unit);
                    ink.push(colours.ink_map(p).map(to_byte));
                }
            }
        }
        let mut reciprocal = [0u32; 256];
        for (alpha, value) in reciprocal.iter_mut().enumerate().skip(1) {
            *value = ((255u32 << 16) + alpha as u32 / 2) / alpha as u32;
        }
        Self {
            colours,
            ink: ink.into_boxed_slice(),
            reciprocal,
        }
    }

    /// The table for [`DarkPaper::DEFAULT`], built once for each process.
    pub fn shared() -> &'static Self {
        static SHARED: OnceLock<DarkPaperMap> = OnceLock::new();
        SHARED.get_or_init(|| Self::new(DarkPaper::DEFAULT))
    }

    pub fn colours(&self) -> DarkPaper {
        self.colours
    }

    /// The map of one opaque sRGB pixel.
    #[inline]
    pub fn map(&self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        let m = r.min(g).min(b);
        if m == 255 {
            return self.colours.paper;
        }
        let alpha = u32::from(255 - m);
        let (zero, u, v) = if r == m {
            (0, g, b)
        } else if g == m {
            (1, r, b)
        } else {
            (2, r, g)
        };
        let unblend = |channel: u8| {
            let value = (u32::from(channel - m) * self.reciprocal[alpha as usize] + 32_768) >> 16;
            value.min(255) as usize
        };
        let ink = self.ink[zero * 65_536 + unblend(u) * 256 + unblend(v)];
        let paper = self.colours.paper;
        [0, 1, 2]
            .map(|i| div255(alpha * u32::from(ink[i]) + (255 - alpha) * u32::from(paper[i])) as u8)
    }

    /// Maps premultiplied BGRA pixels in place. A pixel with alpha below
    /// 255 (a vector Snapshot on a transparent page) is mapped as an opaque
    /// ink colour and keeps its alpha.
    pub fn map_bgra_premultiplied(&self, pixels: &mut [u8]) {
        let mut cache = PixelCache::new(self);
        let (pixels, _) = pixels.as_chunks_mut::<4>();
        for pixel in pixels {
            *pixel = cache.map(self, u32::from_le_bytes(*pixel)).to_le_bytes();
        }
    }

    /// Maps premultiplied BGRA pixels in place where `coverage` (one byte
    /// for each pixel) is not 255: `out = mix(map(c), c, coverage)`. Pixels
    /// under full picture coverage keep their real colours.
    pub fn map_bgra_premultiplied_outside(&self, pixels: &mut [u8], coverage: &[u8]) {
        assert_eq!(
            pixels.len(),
            coverage.len() * 4,
            "one coverage value for each pixel"
        );
        let mut cache = PixelCache::new(self);
        let (pixels, _) = pixels.as_chunks_mut::<4>();
        for (pixel, &covered) in pixels.iter_mut().zip(coverage) {
            if covered == 255 {
                continue;
            }
            let mapped = cache.map(self, u32::from_le_bytes(*pixel)).to_le_bytes();
            if covered == 0 {
                *pixel = mapped;
            } else {
                let keep = u32::from(covered);
                for i in 0..3 {
                    pixel[i] =
                        div255(u32::from(mapped[i]) * (255 - keep) + u32::from(pixel[i]) * keep)
                            as u8;
                }
            }
        }
    }

    /// The map of one premultiplied BGRA pixel as a little-endian word.
    fn map_word(&self, word: u32) -> u32 {
        let [b, g, r, a] = word.to_le_bytes();
        let [b, g, r] = match a {
            0 => [b, g, r],
            255 => {
                let [r, g, b] = self.map([r, g, b]);
                [b, g, r]
            }
            _ => {
                let alpha = u32::from(a);
                let straight =
                    |channel: u8| ((u32::from(channel) * 255 + alpha / 2) / alpha).min(255) as u8;
                let [r, g, b] = self.map([straight(r), straight(g), straight(b)]);
                let premultiply = |channel: u8| div255(u32::from(channel) * alpha) as u8;
                [premultiply(b), premultiply(g), premultiply(r)]
            }
        };
        u32::from_le_bytes([b, g, r, a])
    }
}

/// Recent pixel maps, by colour. A page has few distinct colours (paper,
/// ink and their anti-aliased edges), so most pixels are found here.
struct PixelCache {
    /// The previous pixel: runs of one colour are common.
    last: (u32, u32),
    inputs: Box<[u32; PIXEL_CACHE_SLOTS]>,
    outputs: Box<[u32; PIXEL_CACHE_SLOTS]>,
}

/// 32 KB of cache: it fits a core's level-1 or level-2 cache.
const PIXEL_CACHE_SLOTS: usize = 4_096;

impl PixelCache {
    /// Every slot starts as opaque white, which is a valid entry.
    fn new(map: &DarkPaperMap) -> Self {
        let white = (u32::MAX, map.map_word(u32::MAX));
        Self {
            last: white,
            inputs: Box::new([white.0; PIXEL_CACHE_SLOTS]),
            outputs: Box::new([white.1; PIXEL_CACHE_SLOTS]),
        }
    }

    #[inline(always)]
    fn map(&mut self, map: &DarkPaperMap, word: u32) -> u32 {
        if word == self.last.0 {
            return self.last.1;
        }
        let slot = (word.wrapping_mul(0x9E37_79B1) >> 20) as usize & (PIXEL_CACHE_SLOTS - 1);
        if self.inputs[slot] != word {
            self.inputs[slot] = word;
            self.outputs[slot] = map.map_word(word);
        }
        self.last = (word, self.outputs[slot]);
        self.last.1
    }
}

/// The highlight colour for dark pages: OKLab lightness 0.45, the same hue,
/// chroma at most 0.12, in gamut. It is drawn with [`screen`] after the
/// page map. `#FFFF00` becomes `#595900`.
pub fn dark_highlight_colour(colour: [u8; 3]) -> [u8; 3] {
    let [_, a, b] = oklab(colour.map(unit));
    in_gamut_srgb(0.45, a.hypot(b).min(0.12), b.atan2(a)).map(to_byte)
}

/// Screen blend, `1 − (1 − a)(1 − b)`: the dark dual of Multiply.
pub fn screen(a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    [0, 1, 2].map(|i| 255 - div255(u32::from(255 - a[i]) * u32::from(255 - b[i])) as u8)
}

/// The WCAG 2 contrast ratio of two sRGB colours (1 to 21).
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let luminance = |colour: [u8; 3]| {
        let [r, g, b] = colour.map(|channel| srgb_to_linear(unit(channel)));
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The OKLab lightness, a and b of an sRGB colour (channels 0 to 255).
pub fn oklab_of(colour: [u8; 3]) -> [f64; 3] {
    oklab(colour.map(unit))
}

fn ink_from_index(zero: usize, u: u8, v: u8) -> [u8; 3] {
    match zero {
        0 => [0, u, v],
        1 => [u, 0, v],
        _ => [u, v, 0],
    }
}

/// `round(x / 255)` for `x` in `0..=65535`.
#[inline]
fn div255(x: u32) -> u32 {
    let x = x + 128;
    (x + (x >> 8)) >> 8
}

fn unit(channel: u8) -> f64 {
    f64::from(channel) / 255.
}

fn to_byte(value: f64) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    }
}

fn oklab(srgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = srgb.map(srgb_to_linear);
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    [
        0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    ]
}

fn oklab_to_linear([lightness, a, b]: [f64; 3]) -> [f64; 3] {
    let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
    ]
}

/// The sRGB colour (0 to 1) at OKLab `lightness` and `hue` with the most
/// chroma up to `chroma` that is in the sRGB gamut.
fn in_gamut_srgb(lightness: f64, chroma: f64, hue: f64) -> [f64; 3] {
    let at = |chroma: f64| oklab_to_linear([lightness, chroma * hue.cos(), chroma * hue.sin()]);
    let inside = |linear: [f64; 3]| linear.iter().all(|c| (-1e-6..=1. + 1e-6).contains(c));
    let mut linear = at(chroma);
    if !inside(linear) {
        let (mut low, mut high) = (0., chroma);
        for _ in 0..30 {
            let middle = (low + high) / 2.;
            if inside(at(middle)) {
                low = middle;
            } else {
                high = middle;
            }
        }
        linear = at(low);
    }
    linear.map(|c| linear_to_srgb(c.clamp(0., 1.)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_appearance_resolves_to_the_paper_each_setting_shows() {
        use PageAppearance::{ForceDark, MatchApp, Original};
        for app_is_dark in [false, true] {
            assert_eq!(Original.resolve(app_is_dark), PaperAppearance::Real);
            assert_eq!(ForceDark.resolve(app_is_dark), PaperAppearance::Dark);
        }
        // Match App follows the app's theme, both ways.
        assert_eq!(MatchApp.resolve(false), PaperAppearance::Real);
        assert_eq!(MatchApp.resolve(true), PaperAppearance::Dark);
        assert_eq!(
            PageAppearance::default(),
            Original,
            "Original until it is set"
        );
    }

    #[test]
    fn page_appearance_is_stored_as_snake_case_words() {
        for (setting, text) in [
            (PageAppearance::Original, "\"original\""),
            (PageAppearance::ForceDark, "\"force_dark\""),
            (PageAppearance::MatchApp, "\"match_app\""),
        ] {
            assert_eq!(serde_json::to_string(&setting).unwrap(), text);
            assert_eq!(
                serde_json::from_str::<PageAppearance>(text).unwrap(),
                setting
            );
        }
        // A profile from 0.6.0 stored the choices under their old names.
        for (old, setting) in [
            ("light", PageAppearance::Original),
            ("dark", PageAppearance::ForceDark),
            ("match_system", PageAppearance::MatchApp),
            ("system", PageAppearance::MatchApp),
        ] {
            assert_eq!(
                serde_json::from_str::<PageAppearance>(&format!("\"{old}\"")).unwrap(),
                setting,
                "{old}"
            );
        }
        // The names in the menus and in Settings.
        assert_eq!(
            PageAppearance::ALL.map(PageAppearance::label),
            ["Original", "Force Dark", "Match App"]
        );
    }

    /// The status line shows for a mostly-picture page on dark paper only,
    /// and only from a render made with the paper that shows now.
    #[test]
    fn the_picture_note_shows_for_a_mostly_picture_page_on_dark_paper_only() {
        use PaperAppearance::{Dark, Real};
        let note = Some("Pictures keep their real colours in Dark pages");
        assert_eq!(pictures_keep_colours_note(Dark, Dark, Some(0.95)), note);
        assert_eq!(pictures_keep_colours_note(Dark, Dark, Some(0.5)), note);
        assert_eq!(pictures_keep_colours_note(Dark, Dark, Some(0.49)), None);
        assert_eq!(pictures_keep_colours_note(Dark, Dark, Some(0.)), None);
        assert_eq!(pictures_keep_colours_note(Dark, Dark, None), None);
        // Original pages: nothing to say. A page image still drawn light (the
        // switch has not arrived) says nothing for dark paper.
        assert_eq!(pictures_keep_colours_note(Real, Real, Some(1.)), None);
        assert_eq!(pictures_keep_colours_note(Real, Dark, Some(1.)), None);
        assert_eq!(pictures_keep_colours_note(Dark, Real, Some(1.)), None);
    }

    fn hex(value: u32) -> [u8; 3] {
        [(value >> 16) as u8, (value >> 8) as u8, value as u8]
    }

    fn close(a: [u8; 3], b: [u8; 3], tolerance: u8) -> bool {
        a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= tolerance)
    }

    /// The example colours of the Dark pages design: input, output.
    const EXAMPLES: [(&str, u32, u32); 14] = [
        ("black", 0x000000, 0xE6E6E6),
        ("white", 0xFFFFFF, 0x1E1E1E),
        ("red", 0xFF0000, 0xFF3C2E),
        ("dark red", 0x8B0000, 0xF67362),
        ("navy", 0x000080, 0x7EA8FF),
        ("blue", 0x0000FF, 0x6A99FF),
        ("dark green", 0x006400, 0x62B55C),
        ("yellow", 0xFFFF00, 0xFFFF00),
        ("orange", 0xFF8000, 0xFF8000),
        ("cyan", 0x00FFFF, 0x00FFFF),
        ("light grey", 0xD3D3D3, 0x414141),
        ("50% grey hatch", 0x808080, 0x828282),
        ("pale blue fill", 0xDDEEFF, 0x222D3C),
        ("red edge, 50% coverage", 0xFF8080, 0x8E2D26),
    ];

    #[test]
    fn the_design_examples_map_within_one_step() {
        let table = DarkPaperMap::shared();
        for (name, input, output) in EXAMPLES {
            let direct = DarkPaper::DEFAULT.map_colour(hex(input));
            let looked_up = table.map(hex(input));
            assert!(
                close(direct, hex(output), 1),
                "{name}: direct {direct:02X?}, expected {output:06X}"
            );
            assert!(
                close(looked_up, hex(output), 1),
                "{name}: table {looked_up:02X?}, expected {output:06X}"
            );
        }
        assert_eq!(table.map([255; 3]), DarkPaper::DEFAULT.paper);
        assert_eq!(table.map([0; 3]), DarkPaper::DEFAULT.ink);
    }

    #[test]
    fn saturated_colours_stay_readable_on_dark_paper() {
        let paper = DarkPaper::DEFAULT.paper;
        for colour in [
            0xFF0000, 0x8B0000, 0x000080, 0x0000FF, 0x006400, 0xFFFF00, 0xFF8000, 0x00FFFF,
        ] {
            let mapped = DarkPaper::DEFAULT.map_colour(hex(colour));
            assert!(
                contrast_ratio(mapped, paper) >= 4.5,
                "{colour:06X} -> {mapped:02X?}"
            );
        }
        let black = DarkPaper::DEFAULT.map_colour([0; 3]);
        assert!((contrast_ratio(black, paper) - 13.4).abs() < 0.1);
    }

    #[test]
    fn greys_invert_monotonically_between_ink_and_paper() {
        let table = DarkPaperMap::shared();
        let mut previous = table.map([0; 3]);
        for grey in 1..=255u8 {
            let mapped = table.map([grey; 3]);
            assert!(mapped[0] == mapped[1] && mapped[1] == mapped[2], "{grey}");
            assert!(mapped[0] <= previous[0], "grey {grey} is not monotonic");
            previous = mapped;
        }
    }

    #[test]
    fn hue_is_kept() {
        let hue = |colour: [u8; 3]| {
            let [_, a, b] = oklab_of(colour);
            b.atan2(a).to_degrees()
        };
        for colour in [0xFF0000, 0x8B0000, 0x0000FF, 0x000080, 0x00A000, 0x006400] {
            let mapped = DarkPaperMap::shared().map(hex(colour));
            let difference = (hue(hex(colour)) - hue(mapped) + 540.) % 360. - 180.;
            assert!(
                difference.abs() < 2.,
                "{colour:06X} -> {mapped:02X?}: hue moved {difference:.2} degrees"
            );
        }
    }

    /// A small deterministic generator (xorshift), so the tests need no
    /// random-number crate and give the same colours on each run.
    fn colours(seed: u64, count: usize) -> Vec<([u8; 3], u8)> {
        let mut state = seed;
        (0..count)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let bytes = state.to_le_bytes();
                ([bytes[0], bytes[1], bytes[2]], bytes[3])
            })
            .collect()
    }

    fn over(colour: [u8; 3], background: [u8; 3], opacity: u8) -> [u8; 3] {
        [0, 1, 2].map(|i| {
            div255(
                u32::from(colour[i]) * u32::from(opacity)
                    + u32::from(background[i]) * (255 - u32::from(opacity)),
            ) as u8
        })
    }

    /// `map(mix(c, white, a)) = mix(map(c), P, a)`, so the app can map a
    /// markup colour and draw it with its own opacity.
    #[test]
    fn the_map_commutes_with_opacity_over_paper() {
        let table = DarkPaperMap::shared();
        let paper = table.colours().paper;
        let mut worst = 0;
        let mut differences = 0;
        let samples = colours(0x5EED_CAFE, 20_000);
        for &(colour, opacity) in &samples {
            // map(mix(c, white, a)) = mix(map(c), P, a)
            let left = table.map(over(colour, [255; 3], opacity));
            let right = over(table.map(colour), paper, opacity);
            let difference = (0..3).map(|i| left[i].abs_diff(right[i])).max().unwrap();
            worst = worst.max(difference);
            differences += usize::from(difference > 1);
        }
        // The identity is exact for real numbers. With 8-bit pixels the
        // mixed input is rounded before the unblend, so the ink colour it
        // gives back can move by a step. The ink map can enlarge that.
        assert!(worst <= 4, "worst difference {worst}");
        assert!(
            differences * 20 < samples.len(),
            "{differences} of {} differ by more than 1",
            samples.len()
        );
    }

    #[test]
    fn an_anti_aliased_red_edge_has_no_bright_halo() {
        let table = DarkPaperMap::shared();
        let line = table.map([255, 0, 0]);
        let mut previous = oklab_of(table.colours().paper)[0];
        for coverage in 1..=255u8 {
            let edge = table.map(over([255, 0, 0], [255; 3], coverage));
            let lightness = oklab_of(edge)[0];
            assert!(
                lightness >= previous - 0.005,
                "edge at {coverage} is darker than the one before"
            );
            assert!(
                lightness <= oklab_of(line)[0] + 0.005,
                "edge at {coverage} is lighter than the line: a halo"
            );
            previous = lightness;
        }
    }

    #[test]
    fn the_table_equals_the_direct_map_on_a_grid() {
        let table = DarkPaperMap::shared();
        let mut worst = 0;
        for r in (0..=255u8).step_by(15) {
            for g in (0..=255u8).step_by(17) {
                for b in (0..=255u8).step_by(13) {
                    let direct = DarkPaper::DEFAULT.map_colour([r, g, b]);
                    let looked_up = table.map([r, g, b]);
                    for i in 0..3 {
                        worst = worst.max(direct[i].abs_diff(looked_up[i]));
                    }
                }
            }
        }
        assert!(worst <= 2, "table differs from the direct map by {worst}");
    }

    #[test]
    fn coverage_keeps_pictures_real_and_mixes_partial_coverage() {
        let table = DarkPaperMap::shared();
        let real = [0x00, 0x00, 0xFF, 0xFF]; // opaque red, BGRA
        let mapped = table.map([255, 0, 0]);
        let mut pixels = [real, real, real].concat();
        table.map_bgra_premultiplied_outside(&mut pixels, &[0, 255, 128]);
        assert_eq!(&pixels[0..4], &[mapped[2], mapped[1], mapped[0], 255]);
        assert_eq!(&pixels[4..8], &real);
        let half = [0, 1, 2]
            .map(|i| div255(u32::from(mapped[2 - i]) * 127 + u32::from(real[i]) * 128) as u8);
        assert_eq!(&pixels[8..11], &half);
        assert_eq!(pixels[11], 255);
    }

    #[test]
    fn transparent_pixels_keep_their_alpha() {
        let table = DarkPaperMap::shared();
        // Black at alpha 128 (premultiplied 0), white at alpha 64
        // (premultiplied 64), fully transparent.
        let mut pixels = vec![0, 0, 0, 128, 64, 64, 64, 64, 0, 0, 0, 0];
        table.map_bgra_premultiplied(&mut pixels);
        let ink = DarkPaper::DEFAULT.ink[0];
        let paper = DarkPaper::DEFAULT.paper[0];
        let premultiplied = |value: u8, alpha: u32| div255(u32::from(value) * alpha) as u8;
        assert_eq!(
            pixels[..4],
            [premultiplied(ink, 128); 3]
                .into_iter()
                .chain([128])
                .collect::<Vec<_>>()[..]
        );
        assert_eq!(
            pixels[4..8],
            [premultiplied(paper, 64); 3]
                .into_iter()
                .chain([64])
                .collect::<Vec<_>>()[..]
        );
        assert_eq!(pixels[8..], [0, 0, 0, 0]);
    }

    #[test]
    fn dark_highlights_screen_over_dark_paper() {
        let highlight = dark_highlight_colour([255, 255, 0]);
        assert!(close(highlight, hex(0x595900), 1), "{highlight:02X?}");
        let paper = screen(highlight, DarkPaper::DEFAULT.paper);
        assert!(close(paper, hex(0x6D6D1E), 1), "{paper:02X?}");
        let text = screen(highlight, DarkPaper::DEFAULT.ink);
        assert!(close(text, hex(0xEFEFE6), 1), "{text:02X?}");
        assert!(contrast_ratio(text, paper) >= 4.5);
        // Screen is commutative, as Multiply is.
        assert_eq!(screen(highlight, paper), screen(paper, highlight));
    }

    #[test]
    fn paper_appearance_is_real_by_default_and_serialises_in_snake_case() {
        assert_eq!(PaperAppearance::default(), PaperAppearance::Real);
        assert_eq!(
            serde_json::to_string(&PaperAppearance::Dark).unwrap(),
            "\"dark\""
        );
    }
}
