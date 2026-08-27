use std::{ffi::OsString, path::PathBuf};

use gpui::{App, Entity, Menu, MenuItem};
use gpui_component::{GlobalState, menu::AppMenuBar};

use crate::{
    application_close_workspace::RequestApplicationClose,
    document_workspace::{
        DocumentOpenBatchRequest, DocumentOpenOrigin, NewFromTemplate, OpenPdf, Save, SaveAs,
        SaveDocumentAsTemplate,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeApplicationMenuState {
    pub has_active_document: bool,
    pub save_busy: bool,
}

pub fn build_native_application_menus(state: NativeApplicationMenuState) -> Vec<Menu> {
    let save_disabled = !state.has_active_document || state.save_busy;
    vec![
        Menu::new("Butter Paper").items([MenuItem::action(
            "Quit Butter Paper",
            RequestApplicationClose,
        )]),
        Menu::new("File").items([
            MenuItem::action("Open…", OpenPdf),
            MenuItem::separator(),
            MenuItem::action("New from Template…", NewFromTemplate),
            MenuItem::action("Save Document as Template…", SaveDocumentAsTemplate)
                .disabled(save_disabled),
            MenuItem::separator(),
            MenuItem::action("Save", Save).disabled(save_disabled),
            MenuItem::action("Save As…", SaveAs).disabled(save_disabled),
            MenuItem::separator(),
            MenuItem::action("Close Window", RequestApplicationClose),
        ]),
        Menu::new("Edit").items([
            MenuItem::action("Cut", gpui_component::input::Cut),
            MenuItem::action("Copy", gpui_component::input::Copy),
            MenuItem::action("Paste", gpui_component::input::Paste),
            MenuItem::separator(),
            MenuItem::action("Select All", gpui_component::input::SelectAll),
        ]),
        Menu::new("Window").items([MenuItem::action("Close Window", RequestApplicationClose)]),
    ]
}

pub fn install_native_application_menus(
    state: NativeApplicationMenuState,
    app_menu_bar: &Entity<AppMenuBar>,
    cx: &mut App,
) {
    cx.set_menus(build_native_application_menus(state));
    GlobalState::global_mut(cx).set_app_menus(
        build_native_application_menus(state)
            .into_iter()
            .map(Menu::owned)
            .collect(),
    );
    app_menu_bar.update(cx, |menu_bar, cx| menu_bar.reload(cx));
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
