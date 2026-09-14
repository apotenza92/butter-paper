use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use gpui::{App, KeyBinding, Pixels, px};
use gpui_component::ActiveTheme as _;
use gpui_component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use serde::{Deserialize, Serialize};

const APPLICATION_DATA_DIRECTORY_ENV: &str = "BP_GPUI_DATA_DIR";
const PREFERENCES_FILE_NAME: &str = "application-shell.json";
const DEFAULT_FONT_SIZE: Pixels = px(16.);
const UI_ZOOM_STEP: f32 = 1.2;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub const APPLICATION_UI_ZOOM_MIN_LEVEL: i8 = -3;
pub const APPLICATION_UI_ZOOM_MAX_LEVEL: i8 = 3;
pub const APPLICATION_UI_ZOOM_DEFAULT_LEVEL: i8 = 0;
pub const APPLICATION_RELEASES_URL: &str = "https://github.com/apotenza92/butter-paper/releases";

/// Establishes the document command target before the first menu or dialog opens.
/// Stock menus restore their previous focus; without an initial target, Edit
/// actions can remain focused in the menu instead of reaching the workspace.
pub fn focus_initial_command_context(focus: &gpui::FocusHandle, window: &mut gpui::Window) {
    let focus = focus.clone();
    // Install Root and its command tree before notifying focus observers.
    window.on_next_frame(move |window, cx| focus.focus(window, cx));
}

gpui::actions!(
    application_shell,
    [
        ToggleApplicationMenuBar,
        MakeInterfaceBigger,
        MakeInterfaceSmaller,
        ResetInterfaceSize,
        ToggleApplicationFullScreen,
        SetAsDefaultPdfApp,
        CheckForUpdates,
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
    ui_zoom_level: i8,
}

impl Default for ApplicationShellPreferences {
    fn default() -> Self {
        Self {
            menu_bar_visible: true,
            ui_zoom_level: APPLICATION_UI_ZOOM_DEFAULT_LEVEL,
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

    pub fn set_menu_bar_visible(&mut self, visible: bool) {
        self.menu_bar_visible = visible;
    }

    pub fn set_ui_zoom_level(&mut self, level: i8) {
        self.ui_zoom_level =
            level.clamp(APPLICATION_UI_ZOOM_MIN_LEVEL, APPLICATION_UI_ZOOM_MAX_LEVEL);
    }

    fn normalise(mut self) -> Self {
        self.set_ui_zoom_level(self.ui_zoom_level);
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
            File::open(&self.directory)?.sync_all()?;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationUiZoomAction {
    In,
    Out,
    Reset,
}

pub fn resolve_application_ui_zoom_level(current_level: i8, action: ApplicationUiZoomAction) -> i8 {
    match action {
        ApplicationUiZoomAction::Reset => APPLICATION_UI_ZOOM_DEFAULT_LEVEL,
        ApplicationUiZoomAction::In => current_level
            .clamp(APPLICATION_UI_ZOOM_MIN_LEVEL, APPLICATION_UI_ZOOM_MAX_LEVEL)
            .saturating_add(1)
            .min(APPLICATION_UI_ZOOM_MAX_LEVEL),
        ApplicationUiZoomAction::Out => current_level
            .clamp(APPLICATION_UI_ZOOM_MIN_LEVEL, APPLICATION_UI_ZOOM_MAX_LEVEL)
            .saturating_sub(1)
            .max(APPLICATION_UI_ZOOM_MIN_LEVEL),
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
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-q",
            crate::application_close_workspace::RequestApplicationQuit,
            None,
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-=", MakeInterfaceBigger, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-=", MakeInterfaceBigger, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift--", MakeInterfaceSmaller, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift--", MakeInterfaceSmaller, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-0", ResetInterfaceSize, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-0", ResetInterfaceSize, None),
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
    fn ui_zoom_uses_reference_bounds_steps_and_reset() {
        assert_eq!(
            resolve_application_ui_zoom_level(
                APPLICATION_UI_ZOOM_MIN_LEVEL,
                ApplicationUiZoomAction::Out
            ),
            APPLICATION_UI_ZOOM_MIN_LEVEL
        );
        assert_eq!(
            resolve_application_ui_zoom_level(
                APPLICATION_UI_ZOOM_MAX_LEVEL,
                ApplicationUiZoomAction::In
            ),
            APPLICATION_UI_ZOOM_MAX_LEVEL
        );
        assert_eq!(
            resolve_application_ui_zoom_level(2, ApplicationUiZoomAction::Reset),
            APPLICATION_UI_ZOOM_DEFAULT_LEVEL
        );
        assert_eq!(
            resolve_application_ui_zoom_level(0, ApplicationUiZoomAction::In),
            1
        );
        assert_eq!(
            resolve_application_ui_zoom_level(0, ApplicationUiZoomAction::Out),
            -1
        );
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
        preferences.set_ui_zoom_level(APPLICATION_UI_ZOOM_MAX_LEVEL);
        store.save(preferences).unwrap();

        assert_eq!(store.load().unwrap(), preferences);
    }

    #[test]
    fn persisted_zoom_is_clamped_before_use() {
        let directory = ScratchDirectory::new();
        let store = ApplicationShellPreferencesStore::new(&directory.0);
        fs::write(
            store.path(),
            br#"{"menu_bar_visible":false,"ui_zoom_level":99}"#,
        )
        .unwrap();

        let preferences = store.load().unwrap();
        assert!(!preferences.menu_bar_visible());
        assert_eq!(preferences.ui_zoom_level(), APPLICATION_UI_ZOOM_MAX_LEVEL);
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
