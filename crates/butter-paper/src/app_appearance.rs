//! The App Appearance setting: whether the application's own theme (toolbars,
//! panels, dialogs, menus) is light, dark or follows the system.
//!
//! It is independent of Page Appearance ([`crate::paper_appearance`]), which
//! chooses how the PDF pages draw. A forced app theme never changes the
//! pages, and Match System in both settings follows one system switch.

use gpui::WindowAppearance;
use serde::{Deserialize, Serialize};

/// The App Appearance setting: View ▸ App Appearance and Settings ▸
/// Appearance. One global choice, kept across launches.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppAppearance {
    /// The light theme, whatever the system shows.
    Light,
    /// The dark theme, whatever the system shows.
    Dark,
    /// The theme follows the system's appearance.
    #[default]
    MatchSystem,
}

impl AppAppearance {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::MatchSystem];

    /// The name in the menus and in Settings.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::MatchSystem => "Match System",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Light => "The app shows its light theme.",
            Self::Dark => "The app shows its dark theme.",
            Self::MatchSystem => "The app follows the system appearance.",
        }
    }

    /// The appearance the theme takes, given the system's. Light and Dark
    /// ignore the system.
    pub const fn resolve(self, system: WindowAppearance) -> WindowAppearance {
        match self {
            Self::Light => WindowAppearance::Light,
            Self::Dark => WindowAppearance::Dark,
            Self::MatchSystem => system,
        }
    }
}

/// Application-wide copy of the App Appearance preference.
#[derive(Clone, Copy, Debug, Default)]
pub struct AppAppearancePreference(pub AppAppearance);

impl gpui::Global for AppAppearancePreference {}

impl AppAppearancePreference {
    /// The preference as the application holds it. Match System until set.
    pub fn current(cx: &gpui::App) -> AppAppearance {
        cx.try_global::<Self>().copied().unwrap_or_default().0
    }

    /// Sets the preference and the native appearance of every window to
    /// match it. On macOS, Light and Dark set the application's appearance,
    /// so the window frames, the menu bar menus and native panels take that
    /// appearance too; Match System clears it, so they follow the system
    /// again. Other platforms have no native override.
    pub fn set(appearance: AppAppearance, cx: &mut gpui::App) {
        cx.set_global(Self(appearance));
        cx.set_window_appearance(appearance.native_override());
    }
}

impl AppAppearance {
    /// The native window appearance this setting forces: none for Match
    /// System.
    pub const fn native_override(self) -> Option<WindowAppearance> {
        match self {
            Self::Light => Some(WindowAppearance::Light),
            Self::Dark => Some(WindowAppearance::Dark),
            Self::MatchSystem => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_appearance_resolves_to_the_theme_each_setting_shows() {
        use AppAppearance::{Dark, Light, MatchSystem};
        for system in [
            WindowAppearance::Light,
            WindowAppearance::VibrantLight,
            WindowAppearance::Dark,
            WindowAppearance::VibrantDark,
        ] {
            assert_eq!(Light.resolve(system), WindowAppearance::Light);
            assert_eq!(Dark.resolve(system), WindowAppearance::Dark);
            assert_eq!(MatchSystem.resolve(system), system);
        }
        assert_eq!(AppAppearance::default(), MatchSystem);
    }

    #[test]
    fn only_match_system_leaves_the_native_appearance_to_the_system() {
        use AppAppearance::{Dark, Light, MatchSystem};
        assert_eq!(Light.native_override(), Some(WindowAppearance::Light));
        assert_eq!(Dark.native_override(), Some(WindowAppearance::Dark));
        assert_eq!(MatchSystem.native_override(), None);
    }

    #[test]
    fn app_appearance_is_stored_as_snake_case_words() {
        for (setting, text) in [
            (AppAppearance::Light, "\"light\""),
            (AppAppearance::Dark, "\"dark\""),
            (AppAppearance::MatchSystem, "\"match_system\""),
        ] {
            assert_eq!(serde_json::to_string(&setting).unwrap(), text);
            assert_eq!(
                serde_json::from_str::<AppAppearance>(text).unwrap(),
                setting
            );
        }
        assert_eq!(
            AppAppearance::ALL.map(AppAppearance::label),
            ["Light", "Dark", "Match System"]
        );
    }
}
