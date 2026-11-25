//! The discrete mouse wheel: Settings ▸ Mouse wheel's Zoom speed and Page
//! turning, and how notches turn pages in Single Page view.
//!
//! GPUI gives a discrete wheel's movement in lines, which the workspace
//! converts at [`WHEEL_LINE`] pixels each. One notch is [`notch_lines`]
//! lines. A smaller movement (a high-resolution or free-spinning wheel)
//! counts only when the movements of one direction add up to a notch.
//! Trackpads and other precise devices do not come here.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The pixels the workspace gives one wheel line.
pub const WHEEL_LINE: f32 = 16.;

/// The lines one wheel notch moves by default. macOS gives one line for a
/// notch. Linux gives three (GPUI's fixed `SCROLL_LINES`). Windows gives the
/// system's lines per notch, three by default: [`notch_lines`] reads the
/// value that the user set.
#[cfg(any(target_os = "linux", windows))]
pub const NOTCH_LINES: f32 = 3.;
#[cfg(not(any(target_os = "linux", windows)))]
pub const NOTCH_LINES: f32 = 1.;

/// The lines one wheel notch moves now. On Windows, GPUI multiplies each
/// notch by the system's lines per notch, so one notch is that many lines.
pub fn notch_lines() -> f32 {
    #[cfg(windows)]
    {
        system_notch_lines().unwrap_or(NOTCH_LINES)
    }
    #[cfg(not(windows))]
    {
        NOTCH_LINES
    }
}

/// Windows' lines per wheel notch (Settings ▸ Mouse ▸ "Lines to scroll at a
/// time"), as GPUI reads it. "One screen at a time" gives `u32::MAX` lines,
/// which GPUI also multiplies by.
#[cfg(windows)]
fn system_notch_lines() -> Option<f32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETWHEELSCROLLLINES, SystemParametersInfoW,
    };
    let mut lines: u32 = 0;
    // SAFETY: SPI_GETWHEELSCROLLLINES writes one u32 to the pointer.
    let read =
        unsafe { SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, (&raw mut lines).cast(), 0) };
    (read != 0 && lines != 0).then_some(lines as f32)
}

/// Responsive: notches that come less than this time after a page turn are
/// dropped, as they are while a page turns.
pub const RESPONSIVE_TURN_HOLD: Duration = Duration::from_millis(40);

/// Balanced: notches with no gap longer than this are one spin.
pub const BALANCED_SPIN_GAP: Duration = Duration::from_millis(180);

/// Deliberate: notches with no gap longer than this are one spin, so a new
/// spin needs a clear pause.
pub const DELIBERATE_SPIN_GAP: Duration = Duration::from_millis(450);

/// Part of a notch is forgotten after a pause this long.
pub const PARTIAL_NOTCH_IDLE: Duration = Duration::from_millis(300);

/// The most notches one wheel event zooms by. An accelerated wheel can give
/// many lines in one event.
const MAX_ZOOM_NOTCHES: f32 = 2.;

/// Settings ▸ Mouse wheel ▸ Zoom speed: how much one notch zooms.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoomSpeed {
    Fine,
    #[default]
    Standard,
    Coarse,
}

impl ZoomSpeed {
    pub const ALL: [Self; 3] = [Self::Fine, Self::Standard, Self::Coarse];

    /// The zoom ratio of one notch.
    pub const fn notch_factor(self) -> f32 {
        match self {
            Self::Fine => 1.10,
            Self::Standard => 1.20,
            Self::Coarse => 1.35,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Fine => "Fine",
            Self::Standard => "Standard",
            Self::Coarse => "Coarse",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Fine => "Each click zooms by 10%.",
            Self::Standard => "Each click zooms by 20%.",
            Self::Coarse => "Each click zooms by 35%.",
        }
    }

    /// The zoom ratio of a wheel movement of `delta_px` pixels: a notch's
    /// ratio for each notch, a part of it for part of a notch, and at most
    /// [`MAX_ZOOM_NOTCHES`] notches. A positive delta (a scroll up) zooms in.
    pub fn zoom_ratio(self, delta_px: f32) -> f32 {
        self.zoom_ratio_for(delta_px, notch_lines())
    }

    /// [`Self::zoom_ratio`] for a notch of `notch_lines` lines.
    pub fn zoom_ratio_for(self, delta_px: f32, notch_lines: f32) -> f32 {
        if !delta_px.is_finite() {
            return 1.;
        }
        let notches =
            (delta_px / (WHEEL_LINE * notch_lines)).clamp(-MAX_ZOOM_NOTCHES, MAX_ZOOM_NOTCHES);
        self.notch_factor().powf(notches)
    }
}

/// Settings ▸ Mouse wheel ▸ Page turning: how the wheel turns pages in
/// Single Page view.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PageTurning {
    Deliberate,
    Balanced,
    #[default]
    Responsive,
}

impl PageTurning {
    pub const ALL: [Self; 3] = [Self::Deliberate, Self::Balanced, Self::Responsive];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Deliberate => "Deliberate",
            Self::Balanced => "Balanced",
            Self::Responsive => "Responsive",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Deliberate => {
                "Pages turn only after a clear pause. The wheel stops at each page edge."
            }
            Self::Balanced => {
                "One page for each spin of the wheel. The wheel stops at each page edge."
            }
            Self::Responsive => "Every click turns a page.",
        }
    }

    /// The longest gap between the notches of one spin, for the choices
    /// that turn one page a spin.
    const fn spin_gap(self) -> Option<Duration> {
        match self {
            Self::Deliberate => Some(DELIBERATE_SPIN_GAP),
            Self::Balanced => Some(BALANCED_SPIN_GAP),
            Self::Responsive => None,
        }
    }
}

/// What one wheel event does in Single Page view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotchAction {
    /// Nothing: part of a notch, or a notch the choice drops.
    Ignore,
    /// Scroll the page, which goes on past the view that way.
    Scroll,
    /// Turn to the next page (1) or the previous page (-1).
    Turn(isize),
}

/// The wheel's progress through notches and spins in Single Page view.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelTurns {
    /// Lines of a notch not yet complete, signed as the delta.
    partial: f32,
    partial_at: Option<Instant>,
    /// The last wheel event, which a spin's gaps are measured from.
    last_event: Option<Instant>,
    last_turn: Option<Instant>,
    /// The way this spin goes: 1 to the next page, -1 to the previous.
    spin_direction: isize,
    /// This spin scrolled the page.
    spin_scrolled: bool,
    /// This spin turned a page or stopped at an edge: the rest is ignored.
    spin_spent: bool,
}

impl WheelTurns {
    /// Forgets the notches so far, after the view changes another way.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Forgets part of a notch, after the wheel scrolled the page.
    pub fn clear_partial(&mut self) {
        self.partial = 0.;
        self.partial_at = None;
    }

    /// One wheel event of `delta_px` pixels along its main axis, at `at`.
    /// A negative delta (a scroll down) goes to the next page.
    /// `page_continues` says that the page goes on past the view that way.
    pub fn event(
        &mut self,
        delta_px: f32,
        at: Instant,
        page_continues: bool,
        turning: PageTurning,
    ) -> NotchAction {
        self.event_for(delta_px, at, page_continues, turning, notch_lines())
    }

    /// [`Self::event`] for a notch of `notch_lines` lines.
    pub fn event_for(
        &mut self,
        delta_px: f32,
        at: Instant,
        page_continues: bool,
        turning: PageTurning,
        notch_lines: f32,
    ) -> NotchAction {
        if !delta_px.is_finite() || delta_px.abs() <= f32::EPSILON {
            return NotchAction::Ignore;
        }
        let direction: isize = if delta_px < 0. { 1 } else { -1 };
        let gap = self
            .last_event
            .map(|last| at.saturating_duration_since(last));
        self.last_event = Some(at);
        let new_spin = direction != self.spin_direction
            || match (turning.spin_gap(), gap) {
                (_, None) => true,
                (Some(spin_gap), Some(gap)) => gap > spin_gap,
                (None, Some(_)) => false,
            };
        if new_spin {
            self.spin_direction = direction;
            self.spin_scrolled = false;
            self.spin_spent = false;
        }

        match turning {
            PageTurning::Responsive => {
                if self
                    .last_turn
                    .is_some_and(|turn| at.saturating_duration_since(turn) < RESPONSIVE_TURN_HOLD)
                {
                    self.clear_partial();
                    return NotchAction::Ignore;
                }
                if page_continues {
                    self.clear_partial();
                    return NotchAction::Scroll;
                }
            }
            PageTurning::Balanced | PageTurning::Deliberate => {
                if self.spin_spent {
                    self.clear_partial();
                    return NotchAction::Ignore;
                }
                if page_continues {
                    self.spin_scrolled = true;
                    self.clear_partial();
                    return NotchAction::Scroll;
                }
                if self.spin_scrolled {
                    // The spin scrolled the page to its edge: it stops there.
                    self.spin_spent = true;
                    self.clear_partial();
                    return NotchAction::Ignore;
                }
            }
        }

        if !self.add_to_notch(delta_px, at, notch_lines) {
            return NotchAction::Ignore;
        }
        self.last_turn = Some(at);
        self.spin_spent = true;
        NotchAction::Turn(direction)
    }

    /// Adds a movement to the notch so far. Returns true when it completes a
    /// notch, which then starts afresh.
    fn add_to_notch(&mut self, delta_px: f32, at: Instant, notch_lines: f32) -> bool {
        let lines = delta_px / WHEEL_LINE;
        let stale = self
            .partial_at
            .is_none_or(|last| at.saturating_duration_since(last) > PARTIAL_NOTCH_IDLE);
        if stale || self.partial.signum() != lines.signum() {
            self.partial = 0.;
        }
        self.partial += lines;
        self.partial_at = Some(at);
        // A float sum of parts can fall just short of a whole notch.
        if self.partial.abs() + 0.001 < notch_lines {
            return false;
        }
        self.clear_partial();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTCH: f32 = WHEEL_LINE * NOTCH_LINES;
    /// One notch down, toward the next page.
    const DOWN: f32 = -NOTCH;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    /// Notches down at the offsets given, on a page that fits the view.
    fn turns(turning: PageTurning, offsets: &[u64]) -> usize {
        let start = Instant::now();
        let mut wheel = WheelTurns::default();
        offsets
            .iter()
            .filter(|offset| {
                wheel.event(DOWN, start + ms(**offset), false, turning) == NotchAction::Turn(1)
            })
            .count()
    }

    /// A fast spin: ten notches 10 ms apart.
    const FAST_BURST: [u64; 10] = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90];
    /// Two fast spins 600 ms apart.
    const TWO_BURSTS: [u64; 8] = [0, 10, 20, 30, 630, 640, 650, 660];
    /// A slow scroll: a notch each 250 ms.
    const SLOW: [u64; 4] = [0, 250, 500, 750];

    #[test]
    fn responsive_turns_a_page_a_notch_but_drops_notches_during_a_turn() {
        use PageTurning::Responsive;
        // Notches 10 ms apart: one each 40 ms turns.
        assert_eq!(turns(Responsive, &FAST_BURST), 3);
        assert_eq!(turns(Responsive, &[0, 0, 0, 0]), 1);
        assert_eq!(turns(Responsive, &TWO_BURSTS), 2);
        assert_eq!(turns(Responsive, &SLOW), 4);
        // Notches 50 ms apart all turn.
        assert_eq!(turns(Responsive, &[0, 50, 100, 150]), 4);
    }

    /// Windows sends one notch as the system's lines per notch: three by
    /// default, but any count the user sets. One notch zooms one step and
    /// turns one page whatever that count is.
    #[test]
    fn one_notch_of_any_line_count_zooms_one_step_and_turns_one_page() {
        let start = Instant::now();
        for lines in [1., 3., 5.] {
            let notch = WHEEL_LINE * lines;
            let ratio = ZoomSpeed::Standard.zoom_ratio_for(notch, lines);
            assert!((ratio - 1.2).abs() < 1e-4, "{lines} lines: {ratio}");
            let ratio = ZoomSpeed::Standard.zoom_ratio_for(-notch, lines);
            assert!((ratio - 1. / 1.2).abs() < 1e-4, "{lines} lines: {ratio}");
            let mut wheel = WheelTurns::default();
            assert_eq!(
                wheel.event_for(-notch, start, false, PageTurning::Responsive, lines),
                NotchAction::Turn(1),
                "{lines} lines"
            );
            // A high-resolution wheel's quarter notches add up to one turn.
            let mut wheel = WheelTurns::default();
            let turned = (0..4)
                .filter(|index| {
                    wheel.event_for(
                        -notch / 4.,
                        start + ms(index * 5),
                        false,
                        PageTurning::Responsive,
                        lines,
                    ) == NotchAction::Turn(1)
                })
                .count();
            assert_eq!(turned, 1, "{lines} lines");
        }
    }

    /// The notch is the system's own setting on Windows.
    #[cfg(windows)]
    #[test]
    fn a_windows_notch_is_the_system_lines_per_notch() {
        let lines = system_notch_lines().expect("Windows gives its lines per notch");
        assert_eq!(notch_lines(), lines);
        let ratio = ZoomSpeed::Standard.zoom_ratio(WHEEL_LINE * lines);
        assert!((ratio - 1.2).abs() < 1e-4, "{lines} lines: {ratio}");
    }

    #[test]
    fn balanced_turns_one_page_a_spin() {
        use PageTurning::Balanced;
        assert_eq!(turns(Balanced, &FAST_BURST), 1);
        assert_eq!(turns(Balanced, &TWO_BURSTS), 2);
        assert_eq!(turns(Balanced, &SLOW), 4);
        // A long spin with no gap above the spin gap is one spin.
        let long: Vec<u64> = (0..20).map(|index| index * 150).collect();
        assert_eq!(turns(Balanced, &long), 1);
    }

    #[test]
    fn deliberate_needs_a_clear_pause_between_spins() {
        use PageTurning::Deliberate;
        assert_eq!(turns(Deliberate, &FAST_BURST), 1);
        assert_eq!(turns(Deliberate, &TWO_BURSTS), 2);
        // A notch each 250 ms is one spin to Deliberate, four to Balanced.
        assert_eq!(turns(Deliberate, &SLOW), 1);
        assert_eq!(turns(Deliberate, &[0, 500, 1_000]), 3);
    }

    #[test]
    fn a_turn_the_other_way_is_a_new_spin() {
        let start = Instant::now();
        for turning in PageTurning::ALL {
            let mut wheel = WheelTurns::default();
            assert_eq!(
                wheel.event(DOWN, start, false, turning),
                NotchAction::Turn(1)
            );
            assert_eq!(
                wheel.event(-DOWN, start + ms(60), false, turning),
                NotchAction::Turn(-1),
                "{turning:?}"
            );
        }
    }

    /// Zoomed in: the page scrolls until its edge, then the choice decides.
    #[test]
    fn zoomed_in_the_wheel_scrolls_to_the_edge_then_stops_or_turns() {
        let start = Instant::now();
        // Responsive: one notch past the edge turns the page, in the same
        // spin.
        let mut wheel = WheelTurns::default();
        let responsive = PageTurning::Responsive;
        assert_eq!(
            wheel.event(DOWN, start, true, responsive),
            NotchAction::Scroll
        );
        assert_eq!(
            wheel.event(DOWN, start + ms(20), true, responsive),
            NotchAction::Scroll
        );
        assert_eq!(
            wheel.event(DOWN, start + ms(40), false, responsive),
            NotchAction::Turn(1)
        );

        for turning in [PageTurning::Balanced, PageTurning::Deliberate] {
            let mut wheel = WheelTurns::default();
            assert_eq!(wheel.event(DOWN, start, true, turning), NotchAction::Scroll);
            // The spin reaches the edge: it stops there.
            for offset in [20, 40, 60, 80] {
                assert_eq!(
                    wheel.event(DOWN, start + ms(offset), false, turning),
                    NotchAction::Ignore,
                    "{turning:?}"
                );
            }
            // A new spin, after a pause, turns the page.
            let later = start + ms(80) + turning.spin_gap().unwrap() + ms(1);
            assert_eq!(
                wheel.event(DOWN, later, false, turning),
                NotchAction::Turn(1)
            );
            // The rest of that spin is ignored, on the new page too.
            assert_eq!(
                wheel.event(DOWN, later + ms(20), true, turning),
                NotchAction::Ignore
            );
        }
    }

    #[test]
    fn small_movements_add_up_to_a_notch_before_they_turn() {
        let start = Instant::now();
        for turning in PageTurning::ALL {
            let mut wheel = WheelTurns::default();
            let quarter = DOWN / 4.;
            for index in 0..3 {
                assert_eq!(
                    wheel.event(quarter, start + ms(index * 5), false, turning),
                    NotchAction::Ignore
                );
            }
            assert_eq!(
                wheel.event(quarter, start + ms(15), false, turning),
                NotchAction::Turn(1),
                "{turning:?}"
            );
        }
        // Part of a notch is forgotten after a pause.
        let mut wheel = WheelTurns::default();
        let half = DOWN / 2.;
        let responsive = PageTurning::Responsive;
        assert_eq!(
            wheel.event(half, start, false, responsive),
            NotchAction::Ignore
        );
        let later = start + PARTIAL_NOTCH_IDLE + ms(1);
        assert_eq!(
            wheel.event(half, later, false, responsive),
            NotchAction::Ignore
        );
        assert_eq!(
            wheel.event(half, later + ms(5), false, responsive),
            NotchAction::Turn(1)
        );
    }

    #[test]
    fn the_spin_and_hold_gaps_keep_their_order() {
        assert!(RESPONSIVE_TURN_HOLD < BALANCED_SPIN_GAP);
        assert!(BALANCED_SPIN_GAP < DELIBERATE_SPIN_GAP);
        assert_eq!(RESPONSIVE_TURN_HOLD, ms(40));
    }

    #[test]
    fn each_zoom_speed_zooms_its_ratio_a_notch() {
        for (speed, factor) in [
            (ZoomSpeed::Fine, 1.10),
            (ZoomSpeed::Standard, 1.20),
            (ZoomSpeed::Coarse, 1.35),
        ] {
            assert!((speed.zoom_ratio(NOTCH) - factor).abs() < 1e-5);
            assert!((speed.zoom_ratio(-NOTCH) - 1. / factor).abs() < 1e-5);
            // Half a notch zooms half as far, by ratio.
            assert!((speed.zoom_ratio(NOTCH / 2.) - factor.sqrt()).abs() < 1e-5);
            // An accelerated event zooms two notches at most.
            assert!((speed.zoom_ratio(NOTCH * 10.) - factor * factor).abs() < 1e-4);
            assert_eq!(speed.zoom_ratio(f32::NAN), 1.);
        }
        assert_eq!(ZoomSpeed::default(), ZoomSpeed::Standard);
        assert_eq!(PageTurning::default(), PageTurning::Responsive);
    }
}
