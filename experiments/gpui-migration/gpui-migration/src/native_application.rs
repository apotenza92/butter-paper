use std::{ffi::OsString, path::PathBuf};

use gpui::{App, Entity, Menu, MenuItem};
use gpui_component::{GlobalState, menu::AppMenuBar};

use crate::{
    application_close_workspace::{RequestApplicationClose, RequestApplicationQuit},
    application_shell::{
        CheckForUpdates, MakeInterfaceBigger, MakeInterfaceSmaller, OpenReleasePage,
        ResetInterfaceSize, SetAsDefaultPdfApp, SetUpdateFrequencyAtStartup,
        SetUpdateFrequencyDaily, SetUpdateFrequencyEverySixHours,
        SetUpdateFrequencyEveryTwelveHours, SetUpdateFrequencyHourly, SetUpdateFrequencyMonthly,
        SetUpdateFrequencyNever, SetUpdateFrequencyWeekly, ToggleApplicationFullScreen,
        ToggleApplicationMenuBar,
    },
    document_workspace::{
        ActualSize, CloseDocument, ContinuousView, DocumentOpenBatchRequest, DocumentOpenOrigin,
        FitPage, FitWidth, NavigateNextPage, NavigatePreviousPage, NewFromTemplate, OpenPdf,
        RotatePageLeft, RotatePageRight, Save, SaveAs, SaveDocumentAsTemplate, SinglePageView,
        ZoomIn, ZoomOut,
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
}

pub fn build_native_application_menus(state: NativeApplicationMenuState) -> Vec<Menu> {
    build_native_application_menus_with_optional_shell(state, None)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationMenuShellState {
    pub menu_bar_visible: bool,
    pub menu_bar_visibility_supported: bool,
}

impl Default for ApplicationMenuShellState {
    fn default() -> Self {
        Self {
            menu_bar_visible: true,
            menu_bar_visibility_supported: false,
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
            MenuItem::action("Rotate Left", RotatePageLeft)
                .disabled(!state.document_ready || state.save_busy || state.rotation_busy),
            MenuItem::action("Rotate Right", RotatePageRight)
                .disabled(!state.document_ready || state.save_busy || state.rotation_busy),
        ]),
        build_native_view_menu(state, shell),
        Menu::new("Window").items([MenuItem::action("Close Window", RequestApplicationClose)]),
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
        build_in_window_view_menu(shell),
    ]
}

fn build_product_menu() -> Menu {
    Menu::new("GPUI Migration").items([
        MenuItem::action("Set as Default PDF App…", SetAsDefaultPdfApp).disabled(true),
        MenuItem::action(
            "Default PDF registration is not yet available in GPUI Migration",
            SetAsDefaultPdfApp,
        )
        .disabled(true),
        MenuItem::separator(),
        MenuItem::action("Check for Updates…", CheckForUpdates).disabled(true),
        MenuItem::action(
            "Updates are not yet available in GPUI Migration",
            CheckForUpdates,
        )
        .disabled(true),
        MenuItem::submenu(
            Menu::new("Check Automatically")
                .disabled(true)
                .items(update_frequency_items()),
        ),
        MenuItem::action("View Releases…", OpenReleasePage),
        MenuItem::separator(),
        MenuItem::action("Quit GPUI Migration", RequestApplicationQuit),
    ])
}

fn update_frequency_items() -> [MenuItem; 8] {
    [
        MenuItem::action("Never", SetUpdateFrequencyNever).disabled(true),
        MenuItem::action("At startup", SetUpdateFrequencyAtStartup).disabled(true),
        MenuItem::action("Hourly", SetUpdateFrequencyHourly).disabled(true),
        MenuItem::action("Every 6 hours", SetUpdateFrequencyEverySixHours).disabled(true),
        MenuItem::action("Every 12 hours", SetUpdateFrequencyEveryTwelveHours).disabled(true),
        MenuItem::action("Daily", SetUpdateFrequencyDaily).disabled(true),
        MenuItem::action("Weekly", SetUpdateFrequencyWeekly).disabled(true),
        MenuItem::action("Monthly", SetUpdateFrequencyMonthly).disabled(true),
    ]
}

fn build_file_menu(
    save_disabled: bool,
    include_close_document: bool,
    state: &NativeApplicationMenuState,
) -> Menu {
    let mut items = vec![
        MenuItem::action("New from Template…", NewFromTemplate),
        MenuItem::action("Open…", OpenPdf),
        MenuItem::separator(),
        MenuItem::action("Save", Save).disabled(save_disabled),
        MenuItem::action("Save As…", SaveAs).disabled(save_disabled),
        MenuItem::action("Save Document as Template…", SaveDocumentAsTemplate)
            .disabled(save_disabled),
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
    items.extend([
        MenuItem::separator(),
        MenuItem::action("Select All", gpui_component::input::SelectAll)
            .disabled(!state.has_focused_input && !state.can_select_all),
    ]);
    Menu::new("Edit").items(items)
}

fn build_native_view_menu(
    state: NativeApplicationMenuState,
    shell: Option<ApplicationMenuShellState>,
) -> Menu {
    let mut items = shell.map(build_in_window_view_items).unwrap_or_default();
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
        MenuItem::action("Continuous View", ContinuousView)
            .checked(state.continuous_view_checked)
            .disabled(!state.document_ready),
        MenuItem::action("Single Page View", SinglePageView)
            .checked(state.single_page_view_checked)
            .disabled(!state.document_ready),
    ]);
    Menu::new("View").items(items)
}

fn build_in_window_view_menu(shell: ApplicationMenuShellState) -> Menu {
    Menu::new("View").items(build_in_window_view_items(shell))
}

fn build_in_window_view_items(shell: ApplicationMenuShellState) -> Vec<MenuItem> {
    let mut items = Vec::new();
    if shell.menu_bar_visibility_supported {
        items.push(
            MenuItem::action("Show Menu Bar in App Windows", ToggleApplicationMenuBar)
                .checked(shell.menu_bar_visible),
        );
        items.push(MenuItem::separator());
    }
    items.extend([
        MenuItem::action("Make Interface Bigger", MakeInterfaceBigger),
        MenuItem::action("Make Interface Smaller", MakeInterfaceSmaller),
        MenuItem::action("Reset Interface Size", ResetInterfaceSize),
        MenuItem::separator(),
        MenuItem::action("Toggle Full Screen", ToggleApplicationFullScreen),
    ]);
    items
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
    Some(PathBuf::from(OsString::from_vec(bytes)))
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
