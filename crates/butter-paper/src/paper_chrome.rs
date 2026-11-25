//! Dark pages on the canvas: the colours the app paints over a page follow
//! the paper appearance, never the shell theme.
//!
//! - [`PaperChrome`] holds the screen-only colours drawn on the page
//!   (selection, handles, guides, caret, form editor, page ground). It has a
//!   light-paper set and a dark-paper set.
//! - [`markup_colour`] maps a markup's paint colour with the dark-page map
//!   ([`DarkPaper::map_colour`]) when the paper is dark. Stored colours,
//!   colour pickers, Properties and saved files keep the real colours: only
//!   the paint path calls it.
//!
//! The paper a frame paints with is set once for each frame
//! ([`set_paint_paper`]) and read by the paint functions, as the selection
//! fade is. Paint code runs on the main thread only.

use crate::paper_appearance::{DarkPaper, PaperAppearance, dark_highlight_colour};
use gpui::{Hsla, Rgba};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// Screen-only colours drawn over a page. They never become PDF data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaperChrome {
    /// The page ground under the page image (the paper itself).
    pub paper: u32,
    /// The ink of the paper: the Edit PDF caret and the form field editor's
    /// text.
    pub ink: u32,
    /// Selection outlines, handles and the window marquee.
    pub selection: u32,
    /// The halo around handles and selection marks.
    pub halo: u32,
    /// Text on a chip filled with [`Self::selection`] (the calibration
    /// label).
    pub on_selection: u32,
    pub hover: u32,
    pub focus: u32,
    pub locked: u32,
    pub draft: u32,
    /// The crossing marquee.
    pub crossing: u32,
    /// Snap, angle and spacing guides.
    pub guide: u32,
    /// The halo of guide marks.
    pub guide_halo: u32,
    /// The form field editor's ground.
    pub field: u32,
    /// A one-pixel ring outside the page, so a dark page stands apart from
    /// the viewport. `None` on light paper, which needs none.
    pub edge: Option<u32>,
    /// The fill of each Find match, yellow (the theme's `warning` look)
    /// so that it reads apart from the blue text selection, and how much of
    /// it shows over the paper, in percent. On dark paper the fill is less
    /// than on white, so that the light text under it stays readable.
    pub search_match: u32,
    pub search_match_percent: u8,
    /// The fill of the current Find match, stronger than the others.
    pub search_current: u32,
    pub search_current_percent: u8,
    /// The outline round the current Find match.
    pub search_current_edge: u32,
    /// The outline round a markup that matches, which has no fill.
    pub search_outline: u32,
}

impl PaperChrome {
    /// The colours on white paper (the document's own colours).
    pub const LIGHT: Self = Self {
        paper: 0xffffff,
        ink: 0x000000,
        selection: 0x2563eb,
        halo: 0xffffff,
        on_selection: 0xffffff,
        hover: 0x93c5fd,
        focus: 0x1d4ed8,
        locked: 0x94a3b8,
        draft: 0x0f766e,
        crossing: 0x22c55e,
        guide: 0x16a34a,
        guide_halo: 0xffffff,
        field: 0xffffff,
        edge: None,
        search_match: 0xfacc15,
        search_match_percent: 40,
        search_current: 0xf59e0b,
        search_current_percent: 70,
        search_current_edge: 0xb45309,
        search_outline: 0xa16207,
    };

    /// The colours on dark paper (`#1E1E1E`): lighter variants with 3:1 or
    /// more against the paper; the caret and field text 4.5:1 or more.
    pub const DARK: Self = Self {
        paper: 0x1e1e1e,
        ink: 0xe6e6e6,
        selection: 0x60a5fa,
        halo: 0x1e1e1e,
        on_selection: 0x1e1e1e,
        hover: 0x93c5fd,
        focus: 0x93c5fd,
        locked: 0x94a3b8,
        draft: 0x2dd4bf,
        crossing: 0x4ade80,
        guide: 0x4ade80,
        guide_halo: 0x1e1e1e,
        field: 0x1e1e1e,
        // About 2:1 against the dark viewport (`#262626`) and 2.8:1 against
        // the paper: seen, but quiet.
        edge: Some(0x5a5a5a),
        // Yellow that tints the paper, with the light text on it still 4.5:1
        // or more, and a ring of 3:1 or more round the current match.
        search_match: 0xeab308,
        search_match_percent: 30,
        search_current: 0xfbbf24,
        search_current_percent: 40,
        search_current_edge: 0xfef08a,
        search_outline: 0xeab308,
    };

    pub const fn of(paper: PaperAppearance) -> &'static Self {
        match paper {
            PaperAppearance::Real => &Self::LIGHT,
            PaperAppearance::Dark => &Self::DARK,
        }
    }
}

thread_local! {
    /// The paper this frame paints on. Set by the workspace for each frame.
    static PAINT_PAPER: Cell<PaperAppearance> = const { Cell::new(PaperAppearance::Real) };
    /// Mapped paint colours: a page has few distinct markup colours.
    static MAPPED: RefCell<HashMap<[u8; 3], [u8; 3]>> = RefCell::new(HashMap::new());
}

/// Sets the paper the paint functions draw for, until the next call.
pub fn set_paint_paper(paper: PaperAppearance) {
    PAINT_PAPER.with(|cell| cell.set(paper));
}

/// The paper the paint functions draw for now.
pub fn paint_paper() -> PaperAppearance {
    PAINT_PAPER.with(Cell::get)
}

/// The canvas chrome of the paper painted now.
pub fn chrome() -> &'static PaperChrome {
    PaperChrome::of(paint_paper())
}

/// The ring around a page on the paper painted now, if it has one.
pub fn page_edge() -> Option<Hsla> {
    chrome().edge.map(|edge| gpui::rgb(edge).into())
}

/// A chrome colour as GPUI paints it.
pub fn chrome_colour(pick: impl Fn(&PaperChrome) -> u32) -> Hsla {
    gpui::rgb(pick(chrome())).into()
}

/// A markup's paint colour on the paper painted now: the real colour on
/// light paper, the dark-page map of it on dark paper. Alpha stays.
pub fn markup_colour(colour: Hsla) -> Hsla {
    paper_markup_colour(paint_paper(), colour)
}

/// [`markup_colour`] for an explicit paper.
pub fn paper_markup_colour(paper: PaperAppearance, colour: Hsla) -> Hsla {
    match paper {
        PaperAppearance::Real => colour,
        PaperAppearance::Dark => with_rgb(colour, mapped_rgb(rgb_of(colour))),
    }
}

/// A markup colour from its stored text (`#rrggbb` and the other forms
/// gpui-component reads), mapped for the paper painted now.
pub fn parse_markup_colour(colour: &str) -> Result<Hsla, String> {
    gpui_component::try_parse_color(colour)
        .map(markup_colour)
        .map_err(|error| error.to_string())
}

/// A highlight's body as the canvas paints it while it is drawn (before
/// the page composes it): the real colour on light paper; on dark paper
/// the dark highlight colour at 60% (design 5.3).
pub fn live_highlight_colour(colour: Hsla) -> Hsla {
    match paint_paper() {
        PaperAppearance::Real => colour,
        PaperAppearance::Dark => {
            let dark = with_rgb(colour, dark_highlight_colour(rgb_of(colour)));
            dark.opacity(colour.a * 0.6)
        }
    }
}

fn mapped_rgb(rgb: [u8; 3]) -> [u8; 3] {
    MAPPED.with(|cache| {
        *cache
            .borrow_mut()
            .entry(rgb)
            .or_insert_with(|| DarkPaper::DEFAULT.map_colour(rgb))
    })
}

fn rgb_of(colour: Hsla) -> [u8; 3] {
    let rgba = colour.to_rgb();
    [rgba.r, rgba.g, rgba.b].map(|channel| (channel.clamp(0., 1.) * 255.).round() as u8)
}

fn with_rgb(colour: Hsla, [r, g, b]: [u8; 3]) -> Hsla {
    Hsla::from(Rgba {
        r: f32::from(r) / 255.,
        g: f32::from(g) / 255.,
        b: f32::from(b) / 255.,
        a: colour.a,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper_appearance::contrast_ratio;

    fn bytes(colour: u32) -> [u8; 3] {
        [(colour >> 16) as u8, (colour >> 8) as u8, colour as u8]
    }

    #[test]
    fn light_chrome_is_the_chrome_before_dark_pages() {
        let light = PaperChrome::of(PaperAppearance::Real);
        assert_eq!(light.selection, 0x2563eb);
        assert_eq!(light.halo, 0xffffff);
        assert_eq!(light.paper, 0xffffff);
        assert_eq!(light.ink, 0x000000);
    }

    /// Design 6.2: chrome on dark paper has 3:1 or more against the paper
    /// (WCAG 1.4.11); the caret and the form editor's text 4.5:1 or more.
    #[test]
    fn dark_chrome_meets_its_contrast_targets() {
        let dark = PaperChrome::DARK;
        let paper = bytes(dark.paper);
        assert_eq!(paper, DarkPaper::DEFAULT.paper);
        assert_eq!(bytes(dark.ink), DarkPaper::DEFAULT.ink);
        for (name, colour) in [
            ("selection", dark.selection),
            ("hover", dark.hover),
            ("focus", dark.focus),
            ("locked", dark.locked),
            ("draft", dark.draft),
            ("crossing", dark.crossing),
            ("guide", dark.guide),
        ] {
            let ratio = contrast_ratio(bytes(colour), paper);
            assert!(ratio >= 3., "{name} {colour:06x}: {ratio:.2}:1");
        }
        for (name, colour) in [("caret", dark.ink), ("field text", dark.ink)] {
            let ratio = contrast_ratio(bytes(colour), bytes(dark.field));
            assert!(ratio >= 4.5, "{name}: {ratio:.2}:1");
        }
        let label = contrast_ratio(bytes(dark.on_selection), bytes(dark.selection));
        assert!(label >= 4.5, "calibration label text: {label:.2}:1");
        // The light set keeps its targets on white paper too.
        let light = PaperChrome::LIGHT;
        for colour in [light.selection, light.draft, light.guide] {
            assert!(contrast_ratio(bytes(colour), bytes(light.paper)) >= 3.);
        }
    }

    /// Find's yellow reads apart from the selection's blue on both papers,
    /// and the text under a match stays readable (4.5:1) on each.
    #[test]
    fn find_matches_are_yellow_and_keep_the_text_under_them_readable() {
        let blend = |colour: u32, percent: u8, paper: u32| {
            let (top, under) = (bytes(colour), bytes(paper));
            let mix = |a: u8, b: u8| {
                ((u32::from(a) * u32::from(percent) + u32::from(b) * (100 - u32::from(percent)))
                    / 100) as u8
            };
            [
                mix(top[0], under[0]),
                mix(top[1], under[1]),
                mix(top[2], under[2]),
            ]
        };
        for (name, chrome) in [("light", PaperChrome::LIGHT), ("dark", PaperChrome::DARK)] {
            for (what, colour, percent) in [
                ("match", chrome.search_match, chrome.search_match_percent),
                (
                    "current match",
                    chrome.search_current,
                    chrome.search_current_percent,
                ),
            ] {
                let ground = blend(colour, percent, chrome.paper);
                let ratio = contrast_ratio(bytes(chrome.ink), ground);
                assert!(ratio >= 4.5, "{name} {what}: text {ratio:.2}:1");
                // Yellow: red and green well above blue.
                let rgb = bytes(colour);
                assert!(rgb[0] > 200 && rgb[1] > 150 && rgb[2] < 80, "{name} {what}");
                assert_ne!(colour, chrome.selection);
            }
            // The current match is stronger than the others.
            assert!(chrome.search_current_percent > chrome.search_match_percent);
        }
        for (name, chrome) in [("light", PaperChrome::LIGHT), ("dark", PaperChrome::DARK)] {
            for (what, colour) in [
                ("ring", chrome.search_current_edge),
                ("outline", chrome.search_outline),
            ] {
                let ratio = contrast_ratio(bytes(colour), bytes(chrome.paper));
                assert!(ratio >= 3., "{name} {what}: {ratio:.2}:1");
            }
        }
    }

    /// The page edge exists on dark paper only, and is visible but quiet
    /// against the dark viewport (`#262626`, the dark theme's background).
    #[test]
    fn dark_paper_has_a_quiet_page_edge_and_light_paper_has_none() {
        assert_eq!(PaperChrome::LIGHT.edge, None);
        let edge = bytes(PaperChrome::DARK.edge.expect("dark paper has an edge"));
        let viewport = [0x26, 0x26, 0x26];
        let against_viewport = contrast_ratio(edge, viewport);
        let against_paper = contrast_ratio(edge, bytes(PaperChrome::DARK.paper));
        assert!(
            (1.8..3.0).contains(&against_viewport),
            "edge vs viewport {against_viewport:.2}:1"
        );
        assert!(against_paper > against_viewport, "{against_paper:.2}:1");
        // Without the edge the paper is nearly the viewport's colour.
        assert!(contrast_ratio(bytes(PaperChrome::DARK.paper), viewport) < 1.2);
        set_paint_paper(PaperAppearance::Dark);
        assert!(page_edge().is_some());
        set_paint_paper(PaperAppearance::Real);
        assert!(page_edge().is_none());
    }

    #[test]
    fn markup_colours_map_only_on_dark_paper_and_keep_alpha() {
        let navy = with_rgb(gpui::black().opacity(0.4), [0x00, 0x00, 0x80]);
        assert_eq!(paper_markup_colour(PaperAppearance::Real, navy), navy);
        let mapped = paper_markup_colour(PaperAppearance::Dark, navy);
        assert_eq!(rgb_of(mapped), DarkPaper::DEFAULT.map_colour([0, 0, 0x80]));
        assert!((mapped.a - 0.4).abs() < 1e-6, "opacity stays");
        // Black ink becomes the dark ink; white fill becomes the paper.
        let black = paper_markup_colour(PaperAppearance::Dark, gpui::black());
        assert_eq!(rgb_of(black), DarkPaper::DEFAULT.ink);
        let white = paper_markup_colour(PaperAppearance::Dark, gpui::white());
        assert_eq!(rgb_of(white), DarkPaper::DEFAULT.paper);
        // Yellow stays yellow (design 5.2).
        let yellow = with_rgb(gpui::black(), [0xff, 0xff, 0x00]);
        assert_eq!(
            rgb_of(paper_markup_colour(PaperAppearance::Dark, yellow)),
            [0xff, 0xff, 0x00]
        );
    }

    #[test]
    fn the_paint_paper_follows_the_frame_setting() {
        set_paint_paper(PaperAppearance::Dark);
        assert_eq!(chrome(), &PaperChrome::DARK);
        let red = parse_markup_colour("#8b0000").unwrap();
        assert_eq!(rgb_of(red), DarkPaper::DEFAULT.map_colour([0x8b, 0, 0]));
        let live = live_highlight_colour(with_rgb(gpui::black(), [0xff, 0xff, 0]));
        assert_eq!(rgb_of(live), [0x59, 0x59, 0x00]);
        assert!((live.a - 0.6).abs() < 1e-6);
        set_paint_paper(PaperAppearance::Real);
        assert_eq!(chrome(), &PaperChrome::LIGHT);
        let red = parse_markup_colour("#8b0000").unwrap();
        assert_eq!(rgb_of(red), [0x8b, 0, 0]);
    }
}
