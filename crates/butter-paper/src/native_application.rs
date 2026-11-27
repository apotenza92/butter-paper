use std::path::PathBuf;

use gpui::{App, Entity, Menu, MenuItem};
use gpui_component::{GlobalState, menu::AppMenuBar};

use crate::app_appearance::AppAppearance;
use crate::paper_appearance::PageAppearance;

use crate::{
    application_close_workspace::{RequestApplicationClose, RequestApplicationQuit},
    application_shell::{
        AboutButterPaper, MergeAllWindows, MinimiseWindow, MoveDocumentToNewWindow, NewWindow,
        OpenSettings, SetAppAppearanceDark, SetAppAppearanceLight, SetAppAppearanceMatchSystem,
        SetAsDefaultPdfApp, SetPageAppearanceForceDark, SetPageAppearanceMatchApp,
        SetPageAppearanceOriginal, ToggleApplicationFullScreen, ToggleApplicationMenuBar,
        ToggleReverseScrollZoom, ZoomWindow,
    },
    document_workspace::{
        ActualSize, CloseDocument, ContinuousView, ConvertToWord, DocumentOpenBatchRequest,
        DocumentOpenOrigin, ExplodeBlock, FindInDocument, FindInDocuments, FitPage, FitWidth,
        MergeDocuments, NavigateNextPage, NavigatePreviousPage, NewFromTemplate, OpenPdf,
        RotateAllPagesLeft, RotateAllPagesRight, RotatePageLeft, RotatePageRight, Save, SaveAs,
        SaveDocumentAsTemplate, SignWithCertificate, SinglePageView, ToggleCoverPageAlone,
        ToggleSplitView, TwoPagesView, ZoomIn, ZoomOut,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeApplicationMenuState {
    pub has_active_document: bool,
    pub save_busy: bool,
    pub has_focused_input: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub can_cut: bool,
    pub can_copy: bool,
    pub can_paste: bool,
    pub can_select_all: bool,
    pub can_delete: bool,
    pub can_explode_block: bool,
    pub can_close_document: bool,
    pub document_ready: bool,
    pub can_previous_page: bool,
    pub can_next_page: bool,
    pub rotation_busy: bool,
    pub can_zoom_out: bool,
    pub can_zoom_in: bool,
    pub actual_size_checked: bool,
    pub fit_width_checked: bool,
    pub fit_page_checked: bool,
    pub continuous_view_checked: bool,
    pub single_page_view_checked: bool,
    pub two_pages_view_checked: bool,
    pub cover_page_alone_checked: bool,
    pub split_view_checked: bool,
    pub can_move_document_to_new_window: bool,
    /// More than one document window is open.
    pub can_merge_all_windows: bool,
}

pub fn build_native_application_menus(state: NativeApplicationMenuState) -> Vec<Menu> {
    build_native_application_menus_with_optional_shell(state, None)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationMenuShellState {
    pub menu_bar_visible: bool,
    pub menu_bar_visibility_supported: bool,
    pub reverse_scroll_zoom: bool,
    /// The App Appearance setting: the checked item of its submenu.
    pub app_appearance: AppAppearance,
    /// The Page Appearance setting: the checked item of its submenu.
    pub page_appearance: PageAppearance,
    pub updates: UpdateMenuState,
}

/// What the update items in the application menu show.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UpdateMenuStatus {
    /// Development builds and copies run outside an install update manually.
    #[default]
    Unavailable,
    /// Microsoft Store copies, which the Store keeps up to date.
    Store,
    Idle,
    Checking,
    Available(crate::native_updater::ReleaseVersion),
    Downloading,
    Ready(crate::native_updater::ReleaseVersion),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UpdateMenuState {
    pub status: UpdateMenuStatus,
    pub frequency: Option<crate::native_update_policy::UpdateFrequency>,
}

impl Default for ApplicationMenuShellState {
    fn default() -> Self {
        Self {
            menu_bar_visible: true,
            menu_bar_visibility_supported: false,
            reverse_scroll_zoom: false,
            app_appearance: AppAppearance::MatchSystem,
            page_appearance: PageAppearance::Original,
            updates: UpdateMenuState::default(),
        }
    }
}

pub fn build_native_application_menus_with_shell(
    state: NativeApplicationMenuState,
    shell: ApplicationMenuShellState,
) -> Vec<Menu> {
    build_native_application_menus_with_optional_shell(state, Some(shell))
}

fn build_native_application_menus_with_optional_shell(
    state: NativeApplicationMenuState,
    shell: Option<ApplicationMenuShellState>,
) -> Vec<Menu> {
    let save_disabled = !state.has_active_document || !state.document_ready || state.save_busy;
    let rotate_disabled = !state.document_ready || state.save_busy || state.rotation_busy;
    vec![
        build_product_menu(),
        build_file_menu(save_disabled, true, &state),
        build_edit_menu(state, true),
        Menu::new("Document").items([
            MenuItem::action("Previous Page", NavigatePreviousPage)
                .disabled(!state.document_ready || !state.can_previous_page),
            MenuItem::action("Next Page", NavigateNextPage)
                .disabled(!state.document_ready || !state.can_next_page),
            MenuItem::separator(),
            MenuItem::action("Rotate Left", RotatePageLeft).disabled(rotate_disabled),
            MenuItem::action("Rotate Right", RotatePageRight).disabled(rotate_disabled),
            MenuItem::action("Rotate All Pages Left", RotateAllPagesLeft).disabled(rotate_disabled),
            MenuItem::action("Rotate All Pages Right", RotateAllPagesRight)
                .disabled(rotate_disabled),
            MenuItem::separator(),
            MenuItem::action("Move Document to New Window", MoveDocumentToNewWindow)
                .disabled(!state.can_move_document_to_new_window),
        ]),
        build_native_view_menu(state, shell),
        build_window_menu(state),
    ]
}

pub fn build_in_window_application_menus(
    state: NativeApplicationMenuState,
    shell: ApplicationMenuShellState,
) -> Vec<Menu> {
    let save_disabled = !state.has_active_document || !state.document_ready || state.save_busy;
    vec![
        build_product_menu(),
        build_file_menu(save_disabled, false, &state),
        build_edit_menu(state, false),
        build_in_window_view_menu(state, shell),
    ]
}

/// On macOS the menu named "Window" is AppKit's windows menu: it lists open
/// windows. Document windows have no tabbing identifier, so AppKit adds no
/// window-tab commands; Merge All Windows is Butter Paper's own and moves
/// every document into the front window's tab strip.
fn build_window_menu(state: NativeApplicationMenuState) -> Menu {
    let mut items = Vec::new();
    if cfg!(target_os = "macos") {
        items.extend([
            MenuItem::action("Minimise", MinimiseWindow),
            MenuItem::action("Zoom", ZoomWindow),
            MenuItem::separator(),
            merge_all_windows_item(state),
            MenuItem::separator(),
        ]);
    }
    items.push(MenuItem::action("Close Window", RequestApplicationClose));
    Menu::new("Window").items(items)
}

fn build_product_menu() -> Menu {
    // Check for Updates does what fits the copy: a Microsoft Store copy
    // opens its Store page, an installed release checks and shows the
    // progress in Settings, and other copies open the releases page.
    Menu::new("Butter Paper").items([
        MenuItem::action("About Butter Paper", AboutButterPaper),
        MenuItem::action(
            "Check for Updates…",
            crate::application_shell::CheckForUpdates,
        ),
        MenuItem::separator(),
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::action("Set as Default PDF App…", SetAsDefaultPdfApp),
        MenuItem::separator(),
        MenuItem::action("Quit Butter Paper", RequestApplicationQuit),
    ])
}

fn build_file_menu(
    save_disabled: bool,
    include_close_document: bool,
    state: &NativeApplicationMenuState,
) -> Menu {
    let mut items = vec![
        MenuItem::action("New Window", NewWindow),
        MenuItem::action("New from Template…", NewFromTemplate),
        MenuItem::action("Open…", OpenPdf),
        MenuItem::separator(),
        MenuItem::action("Save", Save).disabled(save_disabled),
        MenuItem::action("Save As…", SaveAs).disabled(save_disabled),
        MenuItem::action("Save Document as Template…", SaveDocumentAsTemplate)
            .disabled(save_disabled),
        MenuItem::separator(),
        MenuItem::action("Merge Documents…", MergeDocuments),
        MenuItem::action("Convert to Word Document…", ConvertToWord).disabled(save_disabled),
        MenuItem::action("Sign with Certificate…", SignWithCertificate).disabled(save_disabled),
    ];
    if include_close_document {
        items.extend([
            MenuItem::separator(),
            MenuItem::action("Close Document", CloseDocument).disabled(!state.can_close_document),
        ]);
    }
    Menu::new("File").items(items)
}

fn build_edit_menu(state: NativeApplicationMenuState, include_delete: bool) -> Menu {
    let mut items = vec![
        MenuItem::action("Undo", gpui_component::input::Undo)
            .disabled(!state.has_focused_input && !state.can_undo),
        MenuItem::action("Redo", gpui_component::input::Redo)
            .disabled(!state.has_focused_input && !state.can_redo),
        MenuItem::separator(),
        MenuItem::action("Cut", gpui_component::input::Cut)
            .disabled(!state.has_focused_input && !state.can_cut),
        MenuItem::action("Copy", gpui_component::input::Copy)
            .disabled(!state.has_focused_input && !state.can_copy),
        MenuItem::action("Paste", gpui_component::input::Paste)
            .disabled(!state.has_focused_input && !state.can_paste),
    ];
    if include_delete {
        items.push(
            MenuItem::action("Delete", gpui_component::input::Delete)
                .disabled(!state.has_focused_input && !state.can_delete),
        );
    }
    // A block selected in Edit PDF breaks into its parts.
    items.extend([
        MenuItem::separator(),
        MenuItem::action("Explode Block", ExplodeBlock).disabled(!state.can_explode_block),
    ]);
    items.extend([
        MenuItem::separator(),
        MenuItem::action("Select All", gpui_component::input::SelectAll)
            .disabled(!state.has_focused_input && !state.can_select_all),
    ]);
    // Find opens the bar over the page; Find in Documents, the search panel.
    items.extend([
        MenuItem::separator(),
        MenuItem::action("Find", FindInDocument).disabled(!state.document_ready),
        MenuItem::action("Find in Documents", FindInDocuments).disabled(!state.document_ready),
    ]);
    Menu::new("Edit").items(items)
}

/// Merge All Windows: in the Window menu on macOS, in the View menu
/// elsewhere.
fn merge_all_windows_item(state: NativeApplicationMenuState) -> MenuItem {
    MenuItem::action("Merge All Windows", MergeAllWindows).disabled(!state.can_merge_all_windows)
}

fn build_native_view_menu(
    state: NativeApplicationMenuState,
    shell: Option<ApplicationMenuShellState>,
) -> Menu {
    let mut items = shell
        .map(|shell| build_in_window_view_items(state, shell))
        .unwrap_or_default();
    if !items.is_empty() {
        items.push(MenuItem::separator());
    }
    items.extend([
        MenuItem::action("Zoom In", ZoomIn).disabled(!state.document_ready || !state.can_zoom_in),
        MenuItem::action("Zoom Out", ZoomOut)
            .disabled(!state.document_ready || !state.can_zoom_out),
        MenuItem::action("Actual Size", ActualSize)
            .checked(state.actual_size_checked)
            .disabled(!state.document_ready),
        MenuItem::separator(),
        MenuItem::action("Fit Width", FitWidth)
            .checked(state.fit_width_checked)
            .disabled(!state.document_ready),
        MenuItem::action("Fit Page", FitPage)
            .checked(state.fit_page_checked)
            .disabled(!state.document_ready),
        MenuItem::separator(),
    ]);
    items.extend(page_view_items(state));
    Menu::new("View").items(items)
}

/// The page views, Cover Page Alone and Split View: the same items in the
/// native menu bar and the in-window menu.
pub fn page_view_items(state: NativeApplicationMenuState) -> Vec<MenuItem> {
    vec![
        MenuItem::action("Continuous View", ContinuousView)
            .checked(state.continuous_view_checked)
            .disabled(!state.document_ready),
        MenuItem::action("Single Page View", SinglePageView)
            .checked(state.single_page_view_checked)
            .disabled(!state.document_ready),
        MenuItem::action("Two Pages View", TwoPagesView)
            .checked(state.two_pages_view_checked)
            .disabled(!state.document_ready),
        MenuItem::action("Cover Page Alone", ToggleCoverPageAlone)
            .checked(state.cover_page_alone_checked)
            .disabled(!state.document_ready),
        MenuItem::separator(),
        MenuItem::action("Split View", ToggleSplitView)
            .checked(state.split_view_checked)
            .disabled(!state.document_ready),
    ]
}

/// The in-window View menu: the page views and Split View first, as the
/// toolbar's View button lists them, then the window's own settings.
fn build_in_window_view_menu(
    state: NativeApplicationMenuState,
    shell: ApplicationMenuShellState,
) -> Menu {
    let mut items = page_view_items(state);
    items.push(MenuItem::separator());
    items.extend(build_in_window_view_items(state, shell));
    Menu::new("View").items(items)
}

fn build_in_window_view_items(
    state: NativeApplicationMenuState,
    shell: ApplicationMenuShellState,
) -> Vec<MenuItem> {
    let mut items = Vec::new();
    if shell.menu_bar_visibility_supported {
        items.push(
            MenuItem::action("Show Menu Bar in App Windows", ToggleApplicationMenuBar)
                .checked(shell.menu_bar_visible),
        );
        items.push(MenuItem::separator());
    }
    items.extend([
        MenuItem::action("Reverse Scroll Zoom", ToggleReverseScrollZoom)
            .checked(shell.reverse_scroll_zoom),
        MenuItem::separator(),
        app_appearance_submenu(shell.app_appearance),
        page_appearance_submenu(shell.page_appearance),
        MenuItem::separator(),
        MenuItem::action("Toggle Full Screen", ToggleApplicationFullScreen),
    ]);
    if !cfg!(target_os = "macos") {
        items.extend([MenuItem::separator(), merge_all_windows_item(state)]);
    }
    items
}

/// View ▸ App Appearance: Light, Dark and Match System, the current
/// setting checked (the title-case names of the native menu bar).
pub fn app_appearance_submenu(current: AppAppearance) -> MenuItem {
    let item = |appearance: AppAppearance, item: MenuItem| item.checked(current == appearance);
    MenuItem::submenu(Menu::new("App Appearance").items([
        item(
            AppAppearance::Light,
            MenuItem::action(AppAppearance::Light.label(), SetAppAppearanceLight),
        ),
        item(
            AppAppearance::Dark,
            MenuItem::action(AppAppearance::Dark.label(), SetAppAppearanceDark),
        ),
        item(
            AppAppearance::MatchSystem,
            MenuItem::action(
                AppAppearance::MatchSystem.label(),
                SetAppAppearanceMatchSystem,
            ),
        ),
    ]))
}

/// View ▸ Page Appearance: Original, Force Dark and Match App, the current
/// setting checked (the title-case names of the native menu bar).
pub fn page_appearance_submenu(current: PageAppearance) -> MenuItem {
    let item = |appearance: PageAppearance, item: MenuItem| item.checked(current == appearance);
    MenuItem::submenu(Menu::new("Page Appearance").items([
        item(
            PageAppearance::Original,
            MenuItem::action(PageAppearance::Original.label(), SetPageAppearanceOriginal),
        ),
        item(
            PageAppearance::ForceDark,
            MenuItem::action(
                PageAppearance::ForceDark.label(),
                SetPageAppearanceForceDark,
            ),
        ),
        item(
            PageAppearance::MatchApp,
            MenuItem::action(PageAppearance::MatchApp.label(), SetPageAppearanceMatchApp),
        ),
    ]))
}

pub fn install_native_application_menus(
    state: NativeApplicationMenuState,
    app_menu_bar: &Entity<AppMenuBar>,
    cx: &mut App,
) {
    install_native_application_menus_with_shell(
        state,
        ApplicationMenuShellState::default(),
        app_menu_bar,
        cx,
    );
}

pub fn install_native_application_menus_with_shell(
    state: NativeApplicationMenuState,
    shell: ApplicationMenuShellState,
    app_menu_bar: &Entity<AppMenuBar>,
    cx: &mut App,
) {
    cx.set_menus(build_native_application_menus_with_shell(state, shell));
    GlobalState::global_mut(cx).set_app_menus(
        build_in_window_application_menus(state, shell)
            .into_iter()
            .map(Menu::owned)
            .collect(),
    );
    app_menu_bar.update(cx, |menu_bar, cx| menu_bar.reload(cx));
}

pub fn install_native_platform_menus(
    state: NativeApplicationMenuState,
    shell: ApplicationMenuShellState,
    cx: &mut App,
) {
    cx.set_menus(build_native_application_menus_with_shell(state, shell));
}

#[derive(Clone)]
pub struct NativeDocumentIngress {
    sender: async_channel::Sender<DocumentOpenBatchRequest>,
    receiver: async_channel::Receiver<DocumentOpenBatchRequest>,
}

impl Default for NativeDocumentIngress {
    fn default() -> Self {
        let (sender, receiver) = async_channel::unbounded();
        Self { sender, receiver }
    }
}

impl NativeDocumentIngress {
    pub fn enqueue_request(&self, request: DocumentOpenBatchRequest) -> bool {
        self.sender.try_send(request).is_ok()
    }

    pub fn enqueue_file_urls(&self, urls: impl IntoIterator<Item = impl AsRef<str>>) -> usize {
        let paths = stable_pdf_paths(
            urls.into_iter()
                .filter_map(|url| local_file_url_path(url.as_ref())),
        );
        let accepted = paths.len();
        if accepted > 0 {
            self.enqueue_request(DocumentOpenBatchRequest::new(
                DocumentOpenOrigin::System,
                paths,
            ));
        }
        accepted
    }

    pub fn enqueue_drop_paths(&self, paths: impl IntoIterator<Item = PathBuf>) -> usize {
        let paths = stable_pdf_paths(paths);
        let accepted = paths.len();
        if accepted > 0 {
            self.enqueue_request(DocumentOpenBatchRequest::new(
                DocumentOpenOrigin::Drop,
                paths,
            ));
        }
        accepted
    }

    pub fn take_requests(&self) -> Vec<DocumentOpenBatchRequest> {
        let mut requests = Vec::new();
        while let Ok(request) = self.receiver.try_recv() {
            requests.push(request);
        }
        requests
    }

    pub async fn next_request(&self) -> Option<DocumentOpenBatchRequest> {
        self.receiver.recv().await.ok()
    }
}

fn stable_pdf_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut accepted = Vec::new();
    for path in paths {
        if !path
            .extension()
            .is_some_and(|extension| extension.as_encoded_bytes().eq_ignore_ascii_case(b"pdf"))
            || accepted.contains(&path)
        {
            continue;
        }
        accepted.push(path);
    }
    accepted
}

fn local_file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let path = if let Some(path) = rest.strip_prefix("localhost/") {
        format!("/{path}")
    } else if rest.starts_with('/') {
        rest.to_owned()
    } else {
        return None;
    };
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let bytes = percent_decode(path.as_bytes())?;
    if bytes.contains(&0) {
        return None;
    }
    path_from_url_bytes(bytes)
}

fn percent_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] != b'%' {
            output.push(input[index]);
            index += 1;
            continue;
        }
        let high = *input.get(index + 1)?;
        let low = *input.get(index + 2)?;
        output.push((hex_value(high)? << 4) | hex_value(low)?);
        index += 3;
    }
    Some(output)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(unix)]
fn path_from_url_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt as _;
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

#[cfg(windows)]
fn path_from_url_bytes(mut bytes: Vec<u8>) -> Option<PathBuf> {
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[2] == b':' {
        bytes.remove(0);
    }
    String::from_utf8(bytes).ok().map(PathBuf::from)
}

#[cfg(not(any(unix, windows)))]
fn path_from_url_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    String::from_utf8(bytes).ok().map(PathBuf::from)
}
