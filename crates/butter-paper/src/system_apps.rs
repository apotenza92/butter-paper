//! What the system calls the apps it shows files with, so buttons can say
//! where a file will open: the default app for Word documents, and the
//! file manager.

/// The name of the app the system opens Word (`.docx`) documents with, if
/// it has one (`Microsoft Word`, `Pages`, `LibreOffice Writer`).
pub fn word_document_app() -> Option<String> {
    platform::word_document_app().filter(|name| !name.trim().is_empty())
}

/// What the file manager is called: `Finder`, `File Explorer`, or the
/// default folder app's name on Linux (`Files`, `Dolphin`), else `Folder`
/// ("Show in Folder").
pub fn file_manager() -> String {
    platform::file_manager()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Folder".to_owned())
}

/// Shows `path` selected in Windows' File Explorer, returning whether it
/// did. GPUI's `reveal_path` does this through a shell call on a thread
/// without COM set up, which fails silently on Windows; `explorer.exe
/// /select` does not. Elsewhere this does nothing and returns `false`.
pub fn reveal_in_file_explorer(path: &std::path::Path) -> bool {
    platform::reveal(path)
}

/// The `Name` of a desktop entry file's `[Desktop Entry]` group: its
/// unlocalised name, as `Name[xx]` keys are translations.
pub fn desktop_entry_name(contents: &str) -> Option<String> {
    let mut in_entry = false;
    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if in_entry
            && let Some(name) = line.strip_prefix("Name")
            && let Some(value) = name.trim_start().strip_prefix('=')
        {
            return Some(value.trim().to_owned()).filter(|value| !value.is_empty());
        }
    }
    None
}

#[cfg(target_os = "macos")]
mod platform {
    use core_foundation::base::TCFType as _;
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation::url::CFURL;
    use core_foundation_sys::error::CFErrorRef;
    use core_foundation_sys::url::CFURLRef;

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultApplicationURLForContentType(
            content_type: CFStringRef,
            role: u32,
            error: *mut CFErrorRef,
        ) -> CFURLRef;
    }

    const ROLES_ALL: u32 = 0xFFFF_FFFF;

    pub fn word_document_app() -> Option<String> {
        let content_type = CFString::new("org.openxmlformats.wordprocessingml.document");
        let url = unsafe {
            LSCopyDefaultApplicationURLForContentType(
                content_type.as_concrete_TypeRef(),
                ROLES_ALL,
                std::ptr::null_mut(),
            )
        };
        if url.is_null() {
            return None;
        }
        let url = unsafe { CFURL::wrap_under_create_rule(url) };
        // `/Applications/Microsoft Word.app` is Microsoft Word.
        Some(url.to_path()?.file_stem()?.to_string_lossy().into_owned())
    }

    pub fn file_manager() -> Option<String> {
        Some("Finder".to_owned())
    }

    pub fn reveal(_path: &std::path::Path) -> bool {
        false
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::UI::Shell::{
        ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_FRIENDLYAPPNAME, AssocQueryStringW,
    };

    /// Without an app for `.docx`, Windows would name its "Pick an app"
    /// chooser; `ASSOCF_INIT_IGNOREUNKNOWN` makes the lookup fail instead.
    pub fn word_document_app() -> Option<String> {
        let extension = ".docx".encode_utf16().chain([0]).collect::<Vec<u16>>();
        let mut length = 0u32;
        // Asked without a buffer, it says how long the name is.
        unsafe {
            AssocQueryStringW(
                ASSOCF_INIT_IGNOREUNKNOWN,
                ASSOCSTR_FRIENDLYAPPNAME,
                extension.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut length,
            )
        };
        if length <= 1 {
            return None;
        }
        let mut name = vec![0u16; length as usize];
        let result = unsafe {
            AssocQueryStringW(
                ASSOCF_INIT_IGNOREUNKNOWN,
                ASSOCSTR_FRIENDLYAPPNAME,
                extension.as_ptr(),
                std::ptr::null(),
                name.as_mut_ptr(),
                &mut length,
            )
        };
        if result != 0 {
            return None;
        }
        let end = name
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(name.len());
        Some(String::from_utf16_lossy(&name[..end]))
    }

    pub fn file_manager() -> Option<String> {
        Some("File Explorer".to_owned())
    }

    pub fn reveal(path: &std::path::Path) -> bool {
        use std::os::windows::process::CommandExt as _;
        // Explorer reads `/select,` and a quoted path as one argument; it
        // exits with 1 even when it shows the folder.
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .is_ok()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use std::path::PathBuf;
    use std::process::Command;

    /// The name of the desktop entry `xdg-mime` says opens `mime` files.
    fn default_app(mime: &str) -> Option<String> {
        let output = Command::new("xdg-mime")
            .args(["query", "default", mime])
            .output()
            .ok()?;
        let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if id.is_empty() || id.contains(['/', '\\']) {
            return None;
        }
        let home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            });
        let system = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|dirs| !dirs.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
        home.into_iter()
            .chain(system.split(':').map(PathBuf::from))
            .map(|root| root.join("applications").join(&id))
            .find_map(|path| super::desktop_entry_name(&std::fs::read_to_string(path).ok()?))
    }

    pub fn word_document_app() -> Option<String> {
        default_app("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
    }

    pub fn file_manager() -> Option<String> {
        default_app("inode/directory")
    }

    pub fn reveal(_path: &std::path::Path) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_entry_names_its_app() {
        let entry = "[Desktop Entry]\nName[de]=Dateien\nName=Files\nExec=nautilus\n\
                     [Desktop Action new-window]\nName=New Window\n";
        assert_eq!(desktop_entry_name(entry).as_deref(), Some("Files"));
        assert_eq!(desktop_entry_name("[Other]\nName=X\n"), None);
        assert_eq!(
            desktop_entry_name("[Desktop Entry]\nName =  LibreOffice Writer \n").as_deref(),
            Some("LibreOffice Writer")
        );
    }
}
