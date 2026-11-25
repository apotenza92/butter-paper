use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use gpui::{App, KeyBinding, Pixels, px};
use gpui_component::ActiveTheme as _;
use gpui_component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use serde::{Deserialize, Serialize};

use crate::mouse_wheel::{PageTurning, ZoomSpeed};
use crate::paper_appearance::PageAppearance;

const APPLICATION_DATA_DIRECTORY_ENV: &str = "BP_GPUI_DATA_DIR";
pub(crate) const PREFERENCES_FILE_NAME: &str = "application-shell.json";
const DEFAULT_FONT_SIZE: Pixels = px(16.);
const UI_ZOOM_STEP: f32 = 1.2;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub const APPLICATION_UI_ZOOM_MIN_LEVEL: i8 = -3;
pub const APPLICATION_UI_ZOOM_MAX_LEVEL: i8 = 3;
pub const APPLICATION_UI_ZOOM_DEFAULT_LEVEL: i8 = 0;
pub const APPLICATION_RELEASES_URL: &str = "https://github.com/apotenza92/butter-paper/releases";
/// Opens Butter Paper's page in the Microsoft Store app.
pub const MICROSOFT_STORE_URL: &str = "ms-windows-store://pdp/?ProductId=9N4KN0XNRNHC";

/// Establishes the document command target before the first menu or dialog opens.
/// Stock menus restore their previous focus; without an initial target, Edit
/// actions can remain focused in the menu instead of reaching the workspace.
pub fn focus_initial_command_context(focus: &gpui::FocusHandle, window: &mut gpui::Window) {
    let focus = focus.clone();
    // Install Root and its command tree before notifying focus observers.
    window.on_next_frame(move |window, cx| focus.focus(window, cx));
}

/// Where focus goes when the focused element goes away (a closed popover,
/// panel or tab) and GPUI leaves nothing focused: back to the document, so its
/// single-key shortcuts keep working. While a dialog is open focus stays in
/// it: on the nearest part still shown (a removed row's button gives way to
/// its list), or else on the dialog itself (a closed confirmation's button
/// that was removed). Sent to the document behind, the dialog's Cancel,
/// Escape and Tab would no longer reach it.
pub fn restore_lost_focus(
    document: &gpui::FocusHandle,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    use gpui_component::WindowExt as _;
    if window.has_active_dialog(cx) {
        match window.focus_lost_restore_target(cx) {
            Some(target) if !target.contains(document, window) => target.focus(window, cx),
            _ => {
                window.focus_active_dialog(cx);
            }
        }
        return;
    }
    document.focus(window, cx);
}

gpui::actions!(
    application_shell,
    [
        ToggleApplicationMenuBar,
        ToggleReverseScrollZoom,
        ToggleLockImageProportions,
        OpenDocumentsInNewTab,
        OpenDocumentsInNewWindow,
        SetZoomSpeedFine,
        SetZoomSpeedStandard,
        SetZoomSpeedCoarse,
        SetPageTurningDeliberate,
        SetPageTurningBalanced,
        SetPageTurningResponsive,
        SetPageAppearanceLight,
        SetPageAppearanceDark,
        SetPageAppearanceMatchSystem,
        TogglePageAppearance,
        NewWindow,
        MoveDocumentToNewWindow,
        MergeAllWindows,
        MinimiseWindow,
        ZoomWindow,
        ToggleApplicationFullScreen,
        SetAsDefaultPdfApp,
        AboutButterPaper,
        OpenSettings,
        OpenUpdateSettings,
        DownloadUpdate,
        CheckForUpdates,
        RestartToUpdate,
        OpenReleasePage,
        SetUpdateFrequencyNever,
        SetUpdateFrequencyAtStartup,
        SetUpdateFrequencyHourly,
        SetUpdateFrequencyEverySixHours,
        SetUpdateFrequencyEveryTwelveHours,
        SetUpdateFrequencyDaily,
        SetUpdateFrequencyWeekly,
        SetUpdateFrequencyMonthly,
    ]
);

/// The application-shell preferences that survive a normal relaunch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApplicationShellPreferences {
    menu_bar_visible: bool,
    /// The interface size. Users cannot change it for now (decisions.md,
    /// "Interface size"): load ignores a stored level and save writes the
    /// default, so a profile from an earlier version opens at the default
    /// size. The key stays so that an earlier version can read the file.
    #[serde(default)]
    ui_zoom_level: i8,
    /// Flips wheel zoom for mice whose scroll direction is reversed by a
    /// utility such as LinearMouse, which apps cannot detect.
    #[serde(default)]
    reverse_scroll_zoom: bool,
    /// New images keep their proportions as they are resized.
    #[serde(default = "locked_by_default")]
    lock_image_proportions: bool,
    /// Where a PDF opened from outside the app goes.
    #[serde(default)]
    open_documents_in: OpenDocumentsIn,
    /// How much one wheel notch zooms: Settings ▸ Mouse wheel.
    #[serde(default)]
    zoom_speed: ZoomSpeed,
    /// How the wheel turns pages in Single Page view: Settings ▸ Mouse wheel.
    #[serde(default)]
    page_turning: PageTurning,
    /// Whether pages draw dark: View ▸ Page Appearance and Settings ▸ View.
    /// Light until set.
    #[serde(default)]
    page_appearance: PageAppearance,
}

/// Application-wide copy of the Zoom speed preference.
#[derive(Clone, Copy, Debug, Default)]
pub struct ZoomSpeedPreference(pub ZoomSpeed);

impl gpui::Global for ZoomSpeedPreference {}

impl ZoomSpeedPreference {
    /// The preference as the application holds it.
    pub fn current(cx: &gpui::App) -> ZoomSpeed {
        cx.try_global::<Self>().copied().unwrap_or_default().0
    }
}

/// Application-wide copy of the Page turning preference.
#[derive(Clone, Copy, Debug, Default)]
pub struct PageTurningPreference(pub PageTurning);

impl gpui::Global for PageTurningPreference {}

impl PageTurningPreference {
    /// The preference as the application holds it.
    pub fn current(cx: &gpui::App) -> PageTurning {
        cx.try_global::<Self>().copied().unwrap_or_default().0
    }
}

/// Application-wide copy of the Page Appearance preference.
#[derive(Clone, Copy, Debug, Default)]
pub struct PageAppearancePreference(pub PageAppearance);

impl gpui::Global for PageAppearancePreference {}

impl PageAppearancePreference {
    /// The preference as the application holds it.
    pub fn current(cx: &gpui::App) -> PageAppearance {
        cx.try_global::<Self>().copied().unwrap_or_default().0
    }
}

/// The action that chooses `appearance` in View ▸ Page Appearance and
/// Settings ▸ View.
pub fn page_appearance_action(appearance: PageAppearance) -> Box<dyn gpui::Action> {
    match appearance {
        PageAppearance::Light => Box::new(SetPageAppearanceLight),
        PageAppearance::Dark => Box::new(SetPageAppearanceDark),
        PageAppearance::MatchSystem => Box::new(SetPageAppearanceMatchSystem),
    }
}

/// The shortcut that toggles Light and Dark pages, as the platform writes
/// it. It follows the platform's command key, like the other shortcuts.
pub const fn toggle_page_appearance_shortcut(macos: bool) -> &'static str {
    if macos { "cmd-shift-d" } else { "ctrl-shift-d" }
}

/// The action that chooses `speed` in Settings ▸ Mouse wheel.
pub fn zoom_speed_action(speed: ZoomSpeed) -> Box<dyn gpui::Action> {
    match speed {
        ZoomSpeed::Fine => Box::new(SetZoomSpeedFine),
        ZoomSpeed::Standard => Box::new(SetZoomSpeedStandard),
        ZoomSpeed::Coarse => Box::new(SetZoomSpeedCoarse),
    }
}

/// The action that chooses `turning` in Settings ▸ Mouse wheel.
pub fn page_turning_action(turning: PageTurning) -> Box<dyn gpui::Action> {
    match turning {
        PageTurning::Deliberate => Box::new(SetPageTurningDeliberate),
        PageTurning::Balanced => Box::new(SetPageTurningBalanced),
        PageTurning::Responsive => Box::new(SetPageTurningResponsive),
    }
}

/// Where a PDF opened from outside the app (Finder or Explorer, the Dock or
/// taskbar, the app icon, the command line) goes: Settings ▸ Documents.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenDocumentsIn {
    /// A new tab in the most recently active document window.
    #[default]
    NewTab,
    /// A window of its own.
    NewWindow,
}

/// Application-wide copy of the Open Documents In preference.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenDocumentsInPreference(pub OpenDocumentsIn);

impl gpui::Global for OpenDocumentsInPreference {}

impl OpenDocumentsInPreference {
    /// The preference as the application holds it.
    pub fn current(cx: &gpui::App) -> OpenDocumentsIn {
        cx.try_global::<Self>().copied().unwrap_or_default().0
    }
}

/// The window a PDF opened from outside the app goes to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutsideOpenTarget<W> {
    /// This existing window, as a new tab.
    Window(W),
    /// A new window.
    NewWindow,
}

/// Where an outside open goes, given the preference and the most recently
/// active document window (`recent`), and whether that window holds no
/// documents. A window with no documents takes the open under either
/// preference, so a cold launch from a PDF opens one window, not two.
pub fn outside_open_target<W>(
    preference: OpenDocumentsIn,
    recent: Option<W>,
    recent_is_empty: bool,
) -> OutsideOpenTarget<W> {
    match (preference, recent) {
        (_, None) => OutsideOpenTarget::NewWindow,
        (OpenDocumentsIn::NewTab, Some(window)) => OutsideOpenTarget::Window(window),
        (OpenDocumentsIn::NewWindow, Some(window)) if recent_is_empty => {
            OutsideOpenTarget::Window(window)
        }
        (OpenDocumentsIn::NewWindow, Some(_)) => OutsideOpenTarget::NewWindow,
    }
}

fn locked_by_default() -> bool {
    true
}

/// Application-wide copy of the Reverse Scroll Zoom preference.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReverseScrollZoom(pub bool);

impl gpui::Global for ReverseScrollZoom {}

/// Application-wide copy of the Lock Image Proportions preference: whether
/// new images keep their proportions. Locked when unset.
#[derive(Clone, Copy, Debug)]
pub struct LockImageProportions(pub bool);

impl Default for LockImageProportions {
    fn default() -> Self {
        Self(true)
    }
}

impl gpui::Global for LockImageProportions {}

impl LockImageProportions {
    /// The preference as the application holds it.
    pub fn current(cx: &gpui::App) -> bool {
        cx.try_global::<Self>().copied().unwrap_or_default().0
    }
}

impl Default for ApplicationShellPreferences {
    fn default() -> Self {
        Self {
            menu_bar_visible: true,
            ui_zoom_level: APPLICATION_UI_ZOOM_DEFAULT_LEVEL,
            reverse_scroll_zoom: false,
            lock_image_proportions: true,
            open_documents_in: OpenDocumentsIn::NewTab,
            zoom_speed: ZoomSpeed::Standard,
            page_turning: PageTurning::Responsive,
            page_appearance: PageAppearance::Light,
        }
    }
}

impl ApplicationShellPreferences {
    pub fn menu_bar_visible(&self) -> bool {
        self.menu_bar_visible
    }

    pub fn ui_zoom_level(&self) -> i8 {
        self.ui_zoom_level
    }

    pub fn reverse_scroll_zoom(&self) -> bool {
        self.reverse_scroll_zoom
    }

    pub fn set_reverse_scroll_zoom(&mut self, reverse: bool) {
        self.reverse_scroll_zoom = reverse;
    }

    pub fn lock_image_proportions(&self) -> bool {
        self.lock_image_proportions
    }

    pub fn set_lock_image_proportions(&mut self, locked: bool) {
        self.lock_image_proportions = locked;
    }

    pub fn open_documents_in(&self) -> OpenDocumentsIn {
        self.open_documents_in
    }

    pub fn set_open_documents_in(&mut self, open_documents_in: OpenDocumentsIn) {
        self.open_documents_in = open_documents_in;
    }

    pub fn zoom_speed(&self) -> ZoomSpeed {
        self.zoom_speed
    }

    pub fn set_zoom_speed(&mut self, zoom_speed: ZoomSpeed) {
        self.zoom_speed = zoom_speed;
    }

    pub fn page_turning(&self) -> PageTurning {
        self.page_turning
    }

    pub fn set_page_turning(&mut self, page_turning: PageTurning) {
        self.page_turning = page_turning;
    }

    pub fn page_appearance(&self) -> PageAppearance {
        self.page_appearance
    }

    pub fn set_page_appearance(&mut self, page_appearance: PageAppearance) {
        self.page_appearance = page_appearance;
    }

    pub fn set_menu_bar_visible(&mut self, visible: bool) {
        self.menu_bar_visible = visible;
    }

    fn normalise(mut self) -> Self {
        self.ui_zoom_level = APPLICATION_UI_ZOOM_DEFAULT_LEVEL;
        self
    }
}

/// File-backed storage for application-shell preferences.
pub struct ApplicationShellPreferencesStore {
    directory: PathBuf,
}

impl ApplicationShellPreferencesStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.directory.join(PREFERENCES_FILE_NAME)
    }

    pub fn load(&self) -> io::Result<ApplicationShellPreferences> {
        match fs::read(self.path()) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(ApplicationShellPreferences::normalise)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Ok(ApplicationShellPreferences::default())
            }
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, preferences: ApplicationShellPreferences) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;
        let path = self.path();
        let temporary_path = self.directory.join(format!(
            ".{PREFERENCES_FILE_NAME}.{}.{}.tmp",
            std::process::id(),
            temporary_file_id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut temporary_created = false;
        let result = (|| {
            let mut file = options.open(&temporary_path)?;
            temporary_created = true;
            let json = serde_json::to_vec_pretty(&preferences.normalise())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            file.write_all(&json)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            drop(file);
            replace_file(&temporary_path, &path)?;
            #[cfg(unix)]
            fs::File::open(&self.directory)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() && temporary_created {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }
}

fn temporary_file_id() -> u64 {
    TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

fn replace_file(temporary_path: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary_path, path)
}

/// Resolves the development application's durable data directory.
///
/// `BP_GPUI_DATA_DIR` is an explicit seam for disposable tests and development
/// harnesses. In its absence, the platform's conventional per-user location is
/// used without creating it until preferences are first saved.
pub fn application_data_directory() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(APPLICATION_DATA_DIRECTORY_ENV) {
        let path = PathBuf::from(path);
        return path.is_absolute().then_some(path);
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("GPUI Migration"))
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|path| path.join("AppData/Roaming/GPUI Migration"))
            })
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join("Library/Application Support/GPUI Migration"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join(".local/share/GPUI Migration"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join("GPUI Migration"))
    }
}

pub fn application_ui_zoom_font_size(base_font_size: Pixels, level: i8) -> Pixels {
    let level = level.clamp(APPLICATION_UI_ZOOM_MIN_LEVEL, APPLICATION_UI_ZOOM_MAX_LEVEL);
    px(f32::from(base_font_size) * UI_ZOOM_STEP.powi(i32::from(level)))
}

pub fn apply_application_ui_zoom(
    level: i8,
    base_font_size: Pixels,
    window: &mut gpui::Window,
    cx: &mut App,
) {
    let font_size = application_ui_zoom_font_size(base_font_size, level);
    if cx.theme().font_size == font_size {
        return;
    }
    gpui_component::Theme::global_mut(cx).font_size = font_size;
    gpui_component::Theme::sync_base(cx);
    window.refresh();
}

pub fn default_application_ui_font_size() -> Pixels {
    DEFAULT_FONT_SIZE
}

pub fn init_application_shell_actions(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(
            "escape",
            crate::document_workspace::CloseDocument,
            Some("Settings"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-w",
            crate::document_workspace::CloseDocument,
            Some("Settings"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-w",
            crate::document_workspace::CloseDocument,
            Some("Settings"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-,", OpenSettings, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-,", OpenSettings, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-q",
            crate::application_close_workspace::RequestApplicationQuit,
            None,
        ),
        // Windows and Linux have no quit shortcut: closing the last window
        // (Alt+F4 or the close button) ends the app, as platform apps do.
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-n", NewWindow, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-n", NewWindow, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-m", MinimiseWindow, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-z", Undo, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-z", Undo, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-z", Redo, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-y", Redo, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-x", Cut, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-x", Cut, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", Copy, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", Copy, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-v", Paste, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-v", Paste, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-a", SelectAll, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-a", SelectAll, None),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn preferences_saved_before_the_image_setting_lock_images() {
        let saved = br#"{"menu_bar_visible":true,"ui_zoom_level":0,"reverse_scroll_zoom":false}"#;
        let preferences: ApplicationShellPreferences = serde_json::from_slice(saved).unwrap();
        assert!(preferences.lock_image_proportions());
        assert!(ApplicationShellPreferences::default().lock_image_proportions());
        let mut free = preferences;
        free.set_lock_image_proportions(false);
        let round_trip: ApplicationShellPreferences =
            serde_json::from_slice(&serde_json::to_vec(&free).unwrap()).unwrap();
        assert!(!round_trip.lock_image_proportions());
    }

    #[test]
    fn open_documents_in_is_a_new_tab_until_set_and_survives_a_save() {
        // A profile from before the setting opens documents in a new tab.
        let saved = br#"{"menu_bar_visible":true,"ui_zoom_level":0,"reverse_scroll_zoom":false}"#;
        let preferences: ApplicationShellPreferences = serde_json::from_slice(saved).unwrap();
        assert_eq!(preferences.open_documents_in(), OpenDocumentsIn::NewTab);
        let directory = std::env::temp_dir().join(format!(
            "bp-open-documents-in-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let store = ApplicationShellPreferencesStore::new(&directory);
        for setting in [OpenDocumentsIn::NewWindow, OpenDocumentsIn::NewTab] {
            let mut preferences = preferences;
            preferences.set_open_documents_in(setting);
            store.save(preferences).unwrap();
            assert_eq!(store.load().unwrap().open_documents_in(), setting);
        }
        let mut window = preferences;
        window.set_open_documents_in(OpenDocumentsIn::NewWindow);
        let text = String::from_utf8(serde_json::to_vec(&window).unwrap()).unwrap();
        assert!(
            text.contains(r#""open_documents_in":"new_window""#),
            "{text}"
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn page_appearance_is_light_until_set_and_survives_a_save() {
        // A profile from before the setting shows the document's own colours.
        let saved = br#"{"menu_bar_visible":true,"ui_zoom_level":0,"reverse_scroll_zoom":false}"#;
        let preferences: ApplicationShellPreferences = serde_json::from_slice(saved).unwrap();
        assert_eq!(preferences.page_appearance(), PageAppearance::Light);
        assert_eq!(
            ApplicationShellPreferences::default().page_appearance(),
            PageAppearance::Light
        );
        let directory = std::env::temp_dir().join(format!(
            "bp-page-appearance-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let store = ApplicationShellPreferencesStore::new(&directory);
        for setting in [
            PageAppearance::Dark,
            PageAppearance::MatchSystem,
            PageAppearance::Light,
        ] {
            let mut preferences = preferences;
            preferences.set_page_appearance(setting);
            store.save(preferences).unwrap();
            let loaded = store.load().unwrap();
            assert_eq!(loaded.page_appearance(), setting);
            // The other preferences are untouched.
            assert!(loaded.menu_bar_visible());
        }
        let mut dark = preferences;
        dark.set_page_appearance(PageAppearance::MatchSystem);
        let text = String::from_utf8(serde_json::to_vec(&dark).unwrap()).unwrap();
        assert!(
            text.contains(r#""page_appearance":"match_system""#),
            "{text}"
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    /// Cmd+Shift+D on macOS, Ctrl+Shift+D on Windows and Linux: the same
    /// keys, with the platform's command key.
    #[test]
    fn the_page_appearance_shortcut_follows_the_platform_command_key() {
        assert_eq!(toggle_page_appearance_shortcut(true), "cmd-shift-d");
        assert_eq!(toggle_page_appearance_shortcut(false), "ctrl-shift-d");
        // The same keys show in Settings, the platform's way.
        let mac = cfg!(target_os = "macos");
        assert_eq!(
            crate::shortcut_label::shortcut_label(if mac { "Cmd+Shift+D" } else { "Ctrl+Shift+D" }),
            if mac { "⇧⌘D" } else { "Ctrl+Shift+D" }
        );
    }

    #[test]
    fn mouse_wheel_settings_default_and_survive_a_save() {
        // A profile from before the settings: Standard zoom, Responsive turns.
        let saved = br#"{"menu_bar_visible":true,"ui_zoom_level":0,"reverse_scroll_zoom":true}"#;
        let preferences: ApplicationShellPreferences = serde_json::from_slice(saved).unwrap();
        assert_eq!(preferences.zoom_speed(), ZoomSpeed::Standard);
        assert_eq!(preferences.page_turning(), PageTurning::Responsive);
        assert!(preferences.reverse_scroll_zoom());
        let directory = std::env::temp_dir().join(format!(
            "bp-mouse-wheel-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let store = ApplicationShellPreferencesStore::new(&directory);
        for (speed, turning) in ZoomSpeed::ALL.into_iter().zip(PageTurning::ALL) {
            let mut preferences = preferences;
            preferences.set_zoom_speed(speed);
            preferences.set_page_turning(turning);
            store.save(preferences).unwrap();
            let loaded = store.load().unwrap();
            assert_eq!(loaded.zoom_speed(), speed);
            assert_eq!(loaded.page_turning(), turning);
            assert!(loaded.reverse_scroll_zoom());
        }
        let mut coarse = preferences;
        coarse.set_zoom_speed(ZoomSpeed::Coarse);
        coarse.set_page_turning(PageTurning::Deliberate);
        let text = String::from_utf8(serde_json::to_vec(&coarse).unwrap()).unwrap();
        assert!(text.contains(r#""zoom_speed":"coarse""#), "{text}");
        assert!(text.contains(r#""page_turning":"deliberate""#), "{text}");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn outside_opens_go_to_the_recent_window_or_a_new_one_as_set() {
        use OpenDocumentsIn::{NewTab, NewWindow};
        use OutsideOpenTarget::{NewWindow as Open, Window};
        // No window: one opens, whatever the setting.
        assert_eq!(outside_open_target::<u8>(NewTab, None, true), Open);
        assert_eq!(outside_open_target::<u8>(NewWindow, None, true), Open);
        // New tab: the most recently active window, empty or not.
        assert_eq!(outside_open_target(NewTab, Some(7), false), Window(7));
        assert_eq!(outside_open_target(NewTab, Some(7), true), Window(7));
        // New window: a window of its own, unless the recent window is
        // empty (a cold launch from a PDF opens one window, not two).
        assert_eq!(outside_open_target(NewWindow, Some(7), false), Open);
        assert_eq!(outside_open_target(NewWindow, Some(7), true), Window(7));
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    fn quit_shortcut_is_distinct_from_close_window(cx: &mut gpui::TestAppContext) {
        use crate::application_close_workspace::{RequestApplicationClose, RequestApplicationQuit};
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new(
                "cmd-shift-w",
                RequestApplicationClose,
                None,
            )]);
            init_application_shell_actions(cx);
            let expected = KeyBinding::new("cmd-q", RequestApplicationQuit, None);
            let bindings = cx.key_bindings();
            let bindings = bindings.borrow();
            let quit_bindings = bindings
                .bindings_for_action(&RequestApplicationQuit)
                .collect::<Vec<_>>();
            assert_eq!(quit_bindings.len(), 1);
            assert_eq!(quit_bindings[0].keystrokes(), expected.keystrokes());
        });
    }

    #[cfg(not(target_os = "macos"))]
    #[gpui::test]
    fn windows_and_linux_have_no_quit_shortcut(cx: &mut gpui::TestAppContext) {
        use crate::application_close_workspace::RequestApplicationQuit;

        cx.update(|cx| {
            init_application_shell_actions(cx);
            let bindings = cx.key_bindings();
            let bindings = bindings.borrow();
            assert_eq!(
                bindings
                    .bindings_for_action(&RequestApplicationQuit)
                    .count(),
                0
            );
        });
    }

    struct ScratchDirectory(PathBuf);

    impl ScratchDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "butter-paper-application-shell-{}-{}",
                std::process::id(),
                TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for ScratchDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn preferences_roundtrip_isolated_from_the_real_application_directory() {
        let directory = ScratchDirectory::new();
        let store = ApplicationShellPreferencesStore::new(&directory.0);
        assert_eq!(
            store.load().unwrap(),
            ApplicationShellPreferences::default()
        );

        let mut preferences = ApplicationShellPreferences::default();
        preferences.set_menu_bar_visible(false);
        store.save(preferences).unwrap();

        assert_eq!(store.load().unwrap(), preferences);
    }

    /// Users cannot change the interface size for now: a profile that an
    /// earlier version saved at a larger size opens at the default size, and
    /// the next save writes the default.
    #[test]
    fn a_stored_interface_size_opens_and_saves_at_the_default_size() {
        for stored in [
            APPLICATION_UI_ZOOM_MAX_LEVEL,
            1,
            APPLICATION_UI_ZOOM_MIN_LEVEL,
            99,
        ] {
            let directory = ScratchDirectory::new();
            let store = ApplicationShellPreferencesStore::new(&directory.0);
            fs::write(
                store.path(),
                format!(r#"{{"menu_bar_visible":false,"ui_zoom_level":{stored}}}"#),
            )
            .unwrap();

            let preferences = store.load().unwrap();
            assert!(!preferences.menu_bar_visible());
            assert_eq!(
                preferences.ui_zoom_level(),
                APPLICATION_UI_ZOOM_DEFAULT_LEVEL
            );
            assert_eq!(
                application_ui_zoom_font_size(
                    default_application_ui_font_size(),
                    preferences.ui_zoom_level()
                ),
                default_application_ui_font_size()
            );

            store.save(preferences).unwrap();
            let saved: serde_json::Value =
                serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
            assert_eq!(saved["ui_zoom_level"], 0, "stored level {stored}");
            assert_eq!(saved["menu_bar_visible"], false);
        }
    }

    #[test]
    fn preferences_without_an_interface_size_load() {
        let preferences: ApplicationShellPreferences =
            serde_json::from_slice(br#"{"menu_bar_visible":true}"#).unwrap();
        assert_eq!(
            preferences.ui_zoom_level(),
            APPLICATION_UI_ZOOM_DEFAULT_LEVEL
        );
    }

    #[test]
    fn malformed_preferences_fail_without_replacing_the_existing_bytes() {
        let directory = ScratchDirectory::new();
        let store = ApplicationShellPreferencesStore::new(&directory.0);
        let malformed = br#"{"menu_bar_visible":false,"ui_zoom_level":"broken"}"#;
        fs::write(store.path(), malformed).unwrap();

        let error = store.load().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(store.path()).unwrap(), malformed);
    }

    #[test]
    fn zoom_font_size_is_relative_to_the_active_theme_base() {
        assert_eq!(
            application_ui_zoom_font_size(default_application_ui_font_size(), 0),
            default_application_ui_font_size()
        );
        assert!(
            application_ui_zoom_font_size(px(20.), 1) > application_ui_zoom_font_size(px(20.), 0)
        );
        assert!(
            application_ui_zoom_font_size(px(20.), -1) < application_ui_zoom_font_size(px(20.), 0)
        );
    }
}
