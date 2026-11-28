//! List markers and the rule that decides when lines are a list.
//!
//! A list item's line begins with a marker: a bullet (`•`, `◦`, `▪`, a dash,
//! a symbol font's private-use bullet) or an ordinal (`1.`, `1)`, `(1)`,
//! `a.`, `a)`, `i.`, `A.`, `I.`). Edit PDF treats lines as a list only when
//! the layout makes that certain ([`list_confidence`]): a "1." that starts a
//! sentence, a numbered heading or a dash in prose stays plain text.

use serde::{Deserialize, Serialize};

/// Characters that begin a list item. A dash or an asterisk is a weak
/// bullet: prose can start a line with one.
const STRONG_BULLETS: [char; 21] = [
    '•', '◦', '‣', '▪', '▫', '●', '○', '■', '□', '◆', '◇', '►', '▸', '➢', '➤', '✓', '✔', '∗', '·',
    '⁃', '❖',
];
const WEAK_BULLETS: [char; 4] = ['–', '—', '-', '*'];

/// The most digits a decimal marker has.
const MAX_DIGITS: usize = 3;
/// The longest roman numeral read as a marker (`xxxviii`).
const MAX_ROMAN: usize = 7;

/// Between an item's marker and its text: at least this gap, in ems.
pub const MIN_GAP_EM: f64 = 0.15;
/// A tab after a marker: a gap at least this wide, in ems. A word space is
/// about 0.25 em.
pub const TAB_GAP_EM: f64 = 0.5;
/// At most this gap, in ems.
pub const MAX_GAP_EM: f64 = 5.0;
/// Items' markers and texts start within this distance of each other, in
/// ems (at least a point).
pub const ALIGN_EM: f64 = 0.3;
/// From an item's last line to the next item's first, at most this many
/// ems between baselines.
pub const MAX_ITEM_PITCH_EM: f64 = 3.5;
/// A marker drawn smaller than this share of its text's size is a
/// footnote's or a superscript's.
pub const SMALL_MARKER: f64 = 0.85;

/// How an ordinal counts.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum NumberStyle {
    #[default]
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

/// What begins each item of a list.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ListMarker {
    /// The same character before every item.
    Bullet(char),
    /// Ordinals counting up in `style`, each followed by `suffix` (`.` or
    /// `)`) and, when `open`, preceded by `(`.
    Number {
        #[serde(default)]
        style: NumberStyle,
        #[serde(default)]
        open: bool,
        suffix: char,
    },
}

impl ListMarker {
    /// Decimal numbers followed by `suffix`: `1.` or `1)`.
    pub const fn decimal(suffix: char) -> Self {
        Self::Number {
            style: NumberStyle::Decimal,
            open: false,
            suffix,
        }
    }

    pub fn is_number(&self) -> bool {
        matches!(self, Self::Number { .. })
    }

    /// The marker of item number `value` (from 1): the bullet, or the
    /// ordinal with its punctuation.
    pub fn text(&self, value: u32) -> String {
        match *self {
            Self::Bullet(bullet) => bullet.to_string(),
            Self::Number {
                style,
                open,
                suffix,
            } => {
                let ordinal = format_ordinal(style, value);
                if open {
                    format!("({ordinal}{suffix}")
                } else {
                    format!("{ordinal}{suffix}")
                }
            }
        }
    }
}

/// `value` written in `style`. Letters run on past z as aa, ab, as word
/// processors number.
pub fn format_ordinal(style: NumberStyle, value: u32) -> String {
    let value = value.max(1);
    match style {
        NumberStyle::Decimal => value.to_string(),
        NumberStyle::LowerAlpha | NumberStyle::UpperAlpha => {
            let base = if style == NumberStyle::LowerAlpha {
                b'a'
            } else {
                b'A'
            };
            let mut letters = Vec::new();
            let mut rest = value;
            while rest > 0 {
                rest -= 1;
                letters.push((base + (rest % 26) as u8) as char);
                rest /= 26;
            }
            letters.iter().rev().collect()
        }
        NumberStyle::LowerRoman => roman(value),
        NumberStyle::UpperRoman => roman(value).to_uppercase(),
    }
}

fn roman(mut value: u32) -> String {
    const NUMERALS: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut text = String::new();
    for (amount, numeral) in NUMERALS {
        while value >= amount {
            text.push_str(numeral);
            value -= amount;
        }
    }
    text
}

/// The value of roman numeral `text` in lower case, when it is one written
/// the usual way.
fn roman_value(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > MAX_ROMAN || !text.chars().all(|c| "ivxlc".contains(c)) {
        return None;
    }
    let digit = |c: char| match c {
        'i' => 1,
        'v' => 5,
        'x' => 10,
        'l' => 50,
        _ => 100,
    };
    let chars = text.chars().map(digit).collect::<Vec<u32>>();
    let mut value = 0i64;
    for (index, digit) in chars.iter().enumerate() {
        if chars.get(index + 1).is_some_and(|next| next > digit) {
            value -= i64::from(*digit);
        } else {
            value += i64::from(*digit);
        }
    }
    let value = u32::try_from(value).ok().filter(|value| *value > 0)?;
    (roman(value) == text).then_some(value)
}

/// Whether two ordinals' markers line up: at one edge (left or right
/// aligned), or anywhere within an em and a half of each other while their
/// texts line up (numbers a word processor right-aligns, some of them
/// redrawn left-aligned).
pub fn ordinals_aligned(a: (f64, f64), b: (f64, f64), em: f64) -> bool {
    let tolerance = (ALIGN_EM * em).max(1.);
    (a.0 - b.0).abs() <= tolerance
        || (a.1 - b.1).abs() <= tolerance
        || (a.0 - b.0).abs() <= 1.5 * em
}

/// Whether an item's text starting at `text_left` lines up with a list's
/// text at `list_text_left`: at the same place, or, when its marker is too
/// wide for the list's indent (`iii.` where `i.` fits), a word space after
/// the marker, as word processors set it.
pub fn text_aligned(list_text_left: f64, marker_right: f64, text_left: f64, em: f64) -> bool {
    let tolerance = (ALIGN_EM * em).max(1.);
    (text_left - list_text_left).abs() <= tolerance
        || marker_right > list_text_left - MIN_GAP_EM * em
            && text_left > list_text_left
            && text_left - marker_right <= TAB_GAP_EM * em
}

/// Whether `text` has a dot leader, as a table of contents entry does.
pub fn has_leader(text: &str) -> bool {
    text.contains("....") || text.contains("\u{2026}\u{2026}") || text.contains(". . . .")
}

/// Whether `character` is a bullet that only a list draws: not a dash or
/// an asterisk. A symbol font's private-use character (Word's Symbol and
/// Wingdings bullets) counts.
pub fn strong_bullet(character: char) -> bool {
    STRONG_BULLETS.contains(&character) || ('\u{e000}'..='\u{f8ff}').contains(&character)
}

pub fn is_bullet(character: char) -> bool {
    strong_bullet(character) || WEAK_BULLETS.contains(&character)
}

/// A marker read from the start of a line's text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerReading {
    pub marker: ListMarker,
    /// The ordinal's value (from 1); 0 for a bullet.
    pub value: u32,
    /// Its characters, with punctuation but not the gap after it.
    pub len: usize,
    /// Another reading of the same characters: `i.` is the ninth letter or
    /// the first roman numeral; a list's sequence decides.
    pub alternative: Option<(ListMarker, u32)>,
}

/// The marker `text` begins with, if its characters are one: a bullet, or
/// an ordinal and its punctuation. What follows is not checked.
pub fn read_marker(text: &[char]) -> Option<MarkerReading> {
    let first = *text.first()?;
    if is_bullet(first) {
        return Some(MarkerReading {
            marker: ListMarker::Bullet(first),
            value: 0,
            len: 1,
            alternative: None,
        });
    }
    let open = first == '(';
    let start = usize::from(open);
    let body_len = text[start..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric())
        .count();
    if body_len == 0 {
        return None;
    }
    let body = text[start..start + body_len].iter().collect::<String>();
    let suffix = *text.get(start + body_len)?;
    if !(suffix == ')' || suffix == '.' && !open) {
        return None;
    }
    let len = start + body_len + 1;
    let number = |style: NumberStyle| ListMarker::Number {
        style,
        open,
        suffix,
    };
    if body.chars().all(|c| c.is_ascii_digit()) {
        return (body_len <= MAX_DIGITS).then(|| MarkerReading {
            marker: number(NumberStyle::Decimal),
            value: body.parse().unwrap_or(0),
            len,
            alternative: None,
        });
    }
    let lower = body.to_ascii_lowercase();
    let upper = body.chars().all(|c| c.is_ascii_uppercase());
    if !upper && !body.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    let (alpha_style, roman_style) = if upper {
        (NumberStyle::UpperAlpha, NumberStyle::UpperRoman)
    } else {
        (NumberStyle::LowerAlpha, NumberStyle::LowerRoman)
    };
    let alpha = (body_len == 1).then(|| {
        (
            number(alpha_style),
            u32::from(lower.as_bytes()[0] - b'a') + 1,
        )
    });
    let roman = roman_value(&lower).map(|value| (number(roman_style), value));
    // A single letter reads as a letter first, except `i` and `I`, which
    // begin roman lists far more often than they are the ninth item.
    let (primary, alternative) = match (alpha, roman) {
        (Some(alpha), Some(roman)) if lower == "i" => (roman, Some(alpha)),
        (Some(alpha), roman) => (alpha, roman),
        (None, Some(roman)) => (roman, None),
        (None, None) => return None,
    };
    Some(MarkerReading {
        marker: primary.0,
        value: primary.1,
        len,
        alternative,
    })
}

/// The marker a list item begins with in plain `text` (an edited
/// paragraph's): one of `kinds` when given, else a bullet or a decimal
/// number; and its length with the spaces after it. Letters and roman
/// numerals count only as a known list's kind: in plain text "a." or
/// "I." can be a word.
pub fn text_marker(text: &[char], kinds: &[ListMarker]) -> Option<(ListMarker, u32, usize)> {
    let reading = read_marker(text)?;
    let gap = text[reading.len..]
        .iter()
        .take_while(|c| **c == ' ' || **c == '\u{a0}')
        .count();
    if gap == 0 {
        return None;
    }
    let readings = std::iter::once((reading.marker, reading.value)).chain(reading.alternative);
    for (marker, value) in readings {
        let known = kinds.contains(&marker);
        let plain = matches!(
            marker,
            ListMarker::Bullet(_)
                | ListMarker::Number {
                    style: NumberStyle::Decimal,
                    open: false,
                    ..
                }
        );
        if known || kinds.is_empty() && plain {
            return Some((marker, value, reading.len + gap));
        }
    }
    None
}

/// One item of a possible list, as the page lays it out. Positions are
/// along the lines (`*_left`, `*_right`) and across them (baselines,
/// larger is higher), in page units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemShape {
    pub reading: MarkerReading,
    /// A bullet that only a list draws, or a letter drawn as a bullet in a
    /// symbol font of its own.
    pub strong: bool,
    pub marker_left: f64,
    pub marker_right: f64,
    pub text_left: f64,
    pub em: f64,
    pub first_baseline: f64,
    pub last_baseline: f64,
    /// Whether the item's wrapped lines start at its text (a hanging
    /// indent); `None` for a one-line item.
    pub hanging: Option<bool>,
    /// The line below a one-line item starts at the marker, at the item's
    /// line pitch: it may be the item's text wrapped without an indent.
    pub followed_at_marker: bool,
    /// The marker is drawn smaller than its text (a footnote's number).
    pub small: bool,
    /// The line has a dot leader (a table of contents entry).
    pub leader: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Confidence {
    /// Certainly not a list.
    None,
    /// Could be a list, but plain text reads the same: a single "1." line,
    /// or a dash at the start of a line.
    Low,
    /// A list.
    High,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListVerdict {
    pub confidence: Confidence,
    /// The items' marker, its ordinals read so that they count up.
    pub marker: Option<ListMarker>,
    /// Why, for the corpus report and tests.
    pub reason: &'static str,
}

impl ListVerdict {
    fn not(confidence: Confidence, reason: &'static str) -> Self {
        Self {
            confidence,
            marker: None,
            reason,
        }
    }
}

/// The marker `readings` share when their ordinals count up by one (each
/// read as itself or its alternative), or their bullets are the same.
pub fn counted_marker(readings: &[MarkerReading]) -> Option<ListMarker> {
    let first = readings.first()?;
    let candidates = std::iter::once((first.marker, first.value)).chain(first.alternative);
    'candidate: for (marker, start) in candidates {
        let mut expected = start;
        for reading in &readings[1..] {
            let value = std::iter::once((reading.marker, reading.value))
                .chain(reading.alternative)
                .find(|(other, _)| *other == marker)
                .map(|(_, value)| value);
            match (marker, value) {
                (ListMarker::Bullet(_), Some(_)) => {}
                (ListMarker::Number { .. }, Some(value)) if value == expected + 1 => {
                    expected = value;
                }
                _ => continue 'candidate,
            }
        }
        return Some(marker);
    }
    None
}

fn shared_marker(items: &[ItemShape]) -> Option<ListMarker> {
    counted_marker(&items.iter().map(|item| item.reading).collect::<Vec<_>>())
}

/// Whether `items` (in reading order, each the next below the last) are a
/// list. Confident only when the layout leaves no doubt:
///
/// - Two or more items with the same bullet, or ordinals counting up by
///   one, their markers starting (or, for ordinals, ending) at one edge and
///   their text at another, each within [`MAX_ITEM_PITCH_EM`] of the last.
/// - One item whose marker is a bullet glyph (not a dash), set off by a
///   tab-like gap, with any wrapped lines at its text (a hanging indent);
///   or one ordinal with a tab-like gap and a hanging indent shown by a
///   wrapped line.
///
/// Never: a marker drawn smaller than its text (a footnote), a line with a
/// dot leader (a table of contents), an item whose lines wrap back under
/// its marker, or a marker run into its text.
pub fn list_confidence(items: &[ItemShape]) -> ListVerdict {
    let Some(first) = items.first() else {
        return ListVerdict::not(Confidence::None, "no items");
    };
    let em = first.em.max(1e-3);
    if items.iter().any(|item| item.small) {
        return ListVerdict::not(Confidence::None, "marker smaller than its text");
    }
    if items.iter().any(|item| item.leader) {
        return ListVerdict::not(Confidence::None, "a table of contents entry");
    }
    if items.iter().any(|item| item.hanging == Some(false)) {
        return ListVerdict::not(Confidence::None, "wrapped lines start under the marker");
    }
    let gap = |item: &ItemShape| (item.text_left - item.marker_right) / item.em.max(1e-3);
    if items
        .iter()
        .any(|item| !(MIN_GAP_EM..=MAX_GAP_EM).contains(&gap(item)))
    {
        return ListVerdict::not(Confidence::None, "no gap after the marker");
    }
    if items.iter().any(|item| (item.em - em).abs() > 0.1 * em) {
        return ListVerdict::not(Confidence::None, "items of different sizes");
    }
    let Some(marker) = shared_marker(items) else {
        return ListVerdict::not(Confidence::Low, "markers differ or do not count up");
    };
    let tolerance = (ALIGN_EM * em).max(1.);
    let aligned = |a: f64, b: f64| (a - b).abs() <= tolerance;
    for pair in items.windows(2) {
        let (above, below) = (&pair[0], &pair[1]);
        let markers_aligned = aligned(above.marker_left, below.marker_left)
            || marker.is_number()
                && ordinals_aligned(
                    (above.marker_left, above.marker_right),
                    (below.marker_left, below.marker_right),
                    em,
                );
        let first = &items[0];
        let texts_aligned = |item: &ItemShape| {
            text_aligned(first.text_left, item.marker_right, item.text_left, em)
                || text_aligned(item.text_left, first.marker_right, first.text_left, em)
        };
        if !markers_aligned || !texts_aligned(below) {
            return ListVerdict::not(Confidence::Low, "items do not share their edges");
        }
        let pitch = above.last_baseline - below.first_baseline;
        if !(0.8 * em..=MAX_ITEM_PITCH_EM * em).contains(&pitch) {
            return ListVerdict::not(Confidence::Low, "items too far apart");
        }
    }
    let verdict = |confidence, reason| ListVerdict {
        confidence,
        marker: Some(marker),
        reason,
    };
    if items.len() >= 2 {
        return verdict(Confidence::High, "items in sequence");
    }
    let tab = gap(first) >= TAB_GAP_EM;
    match marker {
        ListMarker::Bullet(_)
            if first.strong && tab && !(first.hanging.is_none() && first.followed_at_marker) =>
        {
            verdict(Confidence::High, "a bullet glyph and a hanging indent")
        }
        ListMarker::Number { .. } if tab && first.hanging == Some(true) => {
            verdict(Confidence::High, "an ordinal and a hanging indent")
        }
        _ => verdict(Confidence::Low, "a single item"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn reading(text: &str) -> MarkerReading {
        read_marker(&chars(text)).unwrap_or_else(|| panic!("{text}"))
    }

    #[test]
    fn markers_read_bullets_and_every_ordinal_form() {
        for (text, marker, value) in [
            ("• x", ListMarker::Bullet('•'), 0),
            ("\u{f0b7} x", ListMarker::Bullet('\u{f0b7}'), 0),
            ("12. x", ListMarker::decimal('.'), 12),
            ("3) x", ListMarker::decimal(')'), 3),
            (
                "(4) x",
                ListMarker::Number {
                    style: NumberStyle::Decimal,
                    open: true,
                    suffix: ')',
                },
                4,
            ),
            (
                "b. x",
                ListMarker::Number {
                    style: NumberStyle::LowerAlpha,
                    open: false,
                    suffix: '.',
                },
                2,
            ),
            (
                "(c) x",
                ListMarker::Number {
                    style: NumberStyle::LowerAlpha,
                    open: true,
                    suffix: ')',
                },
                3,
            ),
            (
                "iv. x",
                ListMarker::Number {
                    style: NumberStyle::LowerRoman,
                    open: false,
                    suffix: '.',
                },
                4,
            ),
            (
                "II) x",
                ListMarker::Number {
                    style: NumberStyle::UpperRoman,
                    open: false,
                    suffix: ')',
                },
                2,
            ),
            (
                "D. x",
                ListMarker::Number {
                    style: NumberStyle::UpperAlpha,
                    open: false,
                    suffix: '.',
                },
                4,
            ),
        ] {
            let read = reading(text);
            assert_eq!((read.marker, read.value), (marker, value), "{text}");
        }
        // "i." is the first roman numeral, or the ninth letter.
        let i = reading("i. x");
        assert_eq!(i.value, 1);
        assert_eq!(i.alternative.map(|(_, value)| value), Some(9));
        for text in ["e.g. x", "1.1 x", "2020. x", "(1. x", "ab. x", "Mr. x", "x"] {
            let read = read_marker(&chars(text));
            assert!(
                read.is_none() || text_marker(&chars(text), &[]).is_none(),
                "{text}: {read:?}"
            );
        }
    }

    #[test]
    fn ordinals_write_in_every_style() {
        let style = |style| ListMarker::Number {
            style,
            open: false,
            suffix: '.',
        };
        assert_eq!(style(NumberStyle::Decimal).text(10), "10.");
        assert_eq!(style(NumberStyle::LowerAlpha).text(28), "ab.");
        assert_eq!(style(NumberStyle::UpperAlpha).text(3), "C.");
        assert_eq!(style(NumberStyle::LowerRoman).text(14), "xiv.");
        assert_eq!(style(NumberStyle::UpperRoman).text(9), "IX.");
        assert_eq!(
            ListMarker::Number {
                style: NumberStyle::Decimal,
                open: true,
                suffix: ')'
            }
            .text(2),
            "(2)"
        );
        for value in 1..=60 {
            assert_eq!(roman_value(&roman(value)), Some(value));
        }
    }

    #[test]
    fn plain_text_reads_letters_only_as_a_known_kind() {
        let alpha = ListMarker::Number {
            style: NumberStyle::LowerAlpha,
            open: false,
            suffix: '.',
        };
        assert_eq!(text_marker(&chars("a. Text"), &[]), None);
        assert_eq!(
            text_marker(&chars("b.  Text"), &[alpha]),
            Some((alpha, 2, 4))
        );
        assert_eq!(
            text_marker(&chars("3. Text"), &[]),
            Some((ListMarker::decimal('.'), 3, 3))
        );
        assert_eq!(text_marker(&chars("3.Text"), &[]), None);
        // "i." in a lettered list is the ninth letter.
        assert_eq!(
            text_marker(&chars("i. Text"), &[alpha]),
            Some((alpha, 9, 3))
        );
    }

    /// An item `index` lines down a list at 12 pt: marker at 72, text at
    /// `text_left`.
    fn item(marker: &str, index: usize, text_left: f64) -> ItemShape {
        let reading = reading(&format!("{marker} "));
        let width = marker.chars().count() as f64 * 6.;
        ItemShape {
            reading,
            strong: matches!(reading.marker, ListMarker::Bullet(c) if strong_bullet(c)),
            marker_left: 72.,
            marker_right: 72. + width,
            text_left,
            em: 12.,
            first_baseline: 700. - 16. * index as f64,
            last_baseline: 700. - 16. * index as f64,
            hanging: None,
            followed_at_marker: false,
            small: false,
            leader: false,
        }
    }

    #[test]
    fn items_in_sequence_with_shared_edges_are_a_list() {
        let bullets = [item("•", 0, 90.), item("•", 1, 90.), item("•", 2, 90.)];
        let verdict = list_confidence(&bullets);
        assert_eq!(verdict.confidence, Confidence::High, "{verdict:?}");
        assert_eq!(verdict.marker, Some(ListMarker::Bullet('•')));

        let numbers = [item("1.", 0, 90.), item("2.", 1, 90.), item("3.", 2, 90.)];
        assert_eq!(list_confidence(&numbers).confidence, Confidence::High);

        // i. ii. iii. counts in roman numerals; h. i. j. in letters.
        let roman = [
            item("i.", 0, 100.),
            item("ii.", 1, 100.),
            item("iii.", 2, 100.),
        ];
        assert!(matches!(
            list_confidence(&roman).marker,
            Some(ListMarker::Number {
                style: NumberStyle::LowerRoman,
                ..
            })
        ));
        let letters = [item("h.", 0, 90.), item("i.", 1, 90.), item("j.", 2, 90.)];
        assert!(matches!(
            list_confidence(&letters).marker,
            Some(ListMarker::Number {
                style: NumberStyle::LowerAlpha,
                ..
            })
        ));

        // Numbers ending at one edge (9. and 10.), as LaTeX and browsers set
        // them.
        let mut nine = item("9.", 0, 90.);
        nine.marker_left = 78.;
        nine.marker_right = 90. - 6.;
        let mut ten = item("10.", 1, 90.);
        ten.marker_right = 90. - 6.;
        assert_eq!(list_confidence(&[nine, ten]).confidence, Confidence::High);
    }

    #[test]
    fn one_bullet_glyph_with_a_tab_and_hanging_indent_is_a_list() {
        let mut bullet = item("•", 0, 90.);
        assert_eq!(list_confidence(&[bullet]).confidence, Confidence::High);
        bullet.hanging = Some(true);
        assert_eq!(list_confidence(&[bullet]).confidence, Confidence::High);
        // A word space is no tab.
        assert_eq!(
            list_confidence(&[item("•", 0, 72. + 6. + 3.)]).confidence,
            Confidence::Low
        );
        // A dash is no bullet glyph.
        assert_eq!(
            list_confidence(&[item("-", 0, 90.)]).confidence,
            Confidence::Low
        );
        // A line below at the marker's edge may be its text, not indented.
        let mut followed = item("•", 0, 90.);
        followed.followed_at_marker = true;
        assert_eq!(list_confidence(&[followed]).confidence, Confidence::Low);
        // One number shows a hanging indent only with a wrapped line.
        let mut number = item("1.", 0, 90.);
        assert_eq!(list_confidence(&[number]).confidence, Confidence::Low);
        number.hanging = Some(true);
        number.last_baseline -= 14.;
        assert_eq!(list_confidence(&[number]).confidence, Confidence::High);
    }

    #[test]
    fn sentences_headings_dates_dashes_and_footnotes_are_not_lists() {
        // "1. The first sentence…" wrapped back to the margin.
        let mut sentence = item("1.", 0, 72. + 12. + 3.);
        sentence.hanging = Some(false);
        assert_eq!(list_confidence(&[sentence]).confidence, Confidence::None);
        // A numbered heading alone.
        assert_eq!(
            list_confidence(&[item("1.", 0, 90.)]).confidence,
            Confidence::Low
        );
        // Headings far apart.
        let mut second = item("2.", 1, 90.);
        second.first_baseline = 500.;
        second.last_baseline = 500.;
        assert_eq!(
            list_confidence(&[item("1.", 0, 90.), second]).confidence,
            Confidence::Low
        );
        // Numbers that do not count up: a date, then another.
        assert_eq!(
            list_confidence(&[item("12.", 0, 100.), item("3.", 1, 100.)]).confidence,
            Confidence::Low
        );
        // Text that starts at a different place on each line.
        assert_eq!(
            list_confidence(&[item("1.", 0, 90.), item("2.", 1, 110.)]).confidence,
            Confidence::Low
        );
        // A table of contents.
        let mut entry = item("1.", 0, 90.);
        entry.leader = true;
        assert_eq!(
            list_confidence(&[entry, item("2.", 1, 90.)]).confidence,
            Confidence::None
        );
        // A marker too wide for the indent pushes its text on a word space.
        let mut third = item("iii.", 2, 100.);
        third.marker_right = 99.5;
        third.text_left = 102.5;
        assert_eq!(
            list_confidence(&[item("i.", 0, 100.), item("ii.", 1, 100.), third]).confidence,
            Confidence::High
        );
        // A footnote's small number.
        let mut footnote = item("1.", 0, 90.);
        footnote.small = true;
        assert_eq!(
            list_confidence(&[footnote, item("2.", 1, 90.)]).confidence,
            Confidence::None
        );
        // A dash run into the word after it.
        assert_eq!(
            list_confidence(&[item("-", 0, 72. + 6. + 0.5), item("-", 1, 72. + 6. + 0.5)])
                .confidence,
            Confidence::None
        );
    }
}
