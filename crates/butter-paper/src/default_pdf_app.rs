//! Makes Butter Paper the default app for PDF files.
//!
//! macOS and Linux change the default directly and verify it. Windows does not
//! let apps choose their own defaults, so Butter Paper registers itself as a
//! PDF-capable app for the current user and opens Default Apps to confirm.

use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultPdfAppOutcome {
    Changed,
    RequiresConfirmation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultPdfAppResult {
    pub outcome: DefaultPdfAppOutcome,
    pub message: String,
}

pub const PRODUCT_NAME: &str = "Butter Paper";
pub const WINDOWS_PROG_ID: &str = "ButterPaper.PDF";
pub const LINUX_DESKTOP_FILE: &str = "butter-paper.desktop";

/// Change (or, on Windows, begin changing) the default PDF application.
pub fn set_as_default_pdf_app() -> Result<DefaultPdfAppResult, String> {
    #[cfg(target_os = "macos")]
    {
        macos::set_default()?;
        Ok(changed())
    }
    #[cfg(target_os = "windows")]
    {
        let executable = std::env::current_exe()
            .map_err(|error| format!("Butter Paper could not find its own program: {error}"))?;
        windows::register_and_open_settings(&executable)?;
        Ok(DefaultPdfAppResult {
            outcome: DefaultPdfAppOutcome::RequiresConfirmation,
            message: format!(
                "Choose {PRODUCT_NAME} for .pdf files in Windows Default Apps, which is now open."
            ),
        })
    }
    #[cfg(target_os = "linux")]
    {
        linux::set_default()?;
        Ok(changed())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err("Setting the default PDF app is not supported on this platform.".into())
    }
}

#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
fn changed() -> DefaultPdfAppResult {
    DefaultPdfAppResult {
        outcome: DefaultPdfAppOutcome::Changed,
        message: format!("{PRODUCT_NAME} is now the default app for PDF files."),
    }
}

/// `reg.exe` invocations that register Butter Paper as a PDF handler for the
/// current user, so Windows lists it in Default Apps.
pub fn windows_registry_commands(executable: &Path) -> Vec<Vec<String>> {
    let executable = executable.display().to_string();
    let prog_id = format!(r"HKCU\Software\Classes\{WINDOWS_PROG_ID}");
    let capabilities = format!(r"Software\{PRODUCT_NAME}\Capabilities");
    let add = |key: &str, value: Option<&str>, data: &str| {
        let mut command = vec!["add".to_owned(), key.to_owned()];
        match value {
            Some(value) => command.extend(["/v".to_owned(), value.to_owned()]),
            None => command.push("/ve".to_owned()),
        }
        command.extend([
            "/t".to_owned(),
            "REG_SZ".to_owned(),
            "/d".to_owned(),
            data.to_owned(),
            "/f".to_owned(),
        ]);
        command
    };
    vec![
        add(&prog_id, None, "PDF Document"),
        add(&format!(r"{prog_id}\DefaultIcon"), None, &format!("\"{executable}\",0")),
        add(
            &format!(r"{prog_id}\shell\open\command"),
            None,
            &format!("\"{executable}\" \"%1\""),
        ),
        add(r"HKCU\Software\Classes\.pdf\OpenWithProgids", Some(WINDOWS_PROG_ID), ""),
        add(&format!(r"HKCU\{capabilities}"), Some("ApplicationName"), PRODUCT_NAME),
        add(
            &format!(r"HKCU\{capabilities}"),
            Some("ApplicationDescription"),
            "PDF review and markup",
        ),
        add(&format!(r"HKCU\{capabilities}\FileAssociations"), Some(".pdf"), WINDOWS_PROG_ID),
        add(r"HKCU\Software\RegisteredApplications", Some(PRODUCT_NAME), &capabilities),
    ]
}

pub fn windows_default_apps_url() -> String {
    format!(
        "ms-settings:defaultapps?registeredAppUser={}",
        PRODUCT_NAME.replace(' ', "%20")
    )
}

/// Whether `xdg-mime query default application/pdf` reports Butter Paper.
pub fn linux_default_is_butter_paper(query_output: &str) -> bool {
    query_output.trim() == LINUX_DESKTOP_FILE
}

#[cfg(target_os = "macos")]
mod macos {
    use core_foundation::base::TCFType as _;
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation_sys::bundle::{CFBundleGetIdentifier, CFBundleGetMainBundle};

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSSetDefaultRoleHandlerForContentType(
            content_type: CFStringRef,
            role: u32,
            handler_bundle_id: CFStringRef,
        ) -> i32;
        fn LSCopyDefaultRoleHandlerForContentType(content_type: CFStringRef, role: u32) -> CFStringRef;
    }

    const ROLES_ALL: u32 = 0xFFFF_FFFF;

    pub fn set_default() -> Result<(), String> {
        let bundle_id = unsafe {
            let bundle = CFBundleGetMainBundle();
            let identifier = if bundle.is_null() {
                std::ptr::null()
            } else {
                CFBundleGetIdentifier(bundle)
            };
            if identifier.is_null() {
                return Err("Butter Paper must run from its installed application bundle.".into());
            }
            CFString::wrap_under_get_rule(identifier).to_string()
        };
        let pdf = CFString::new("com.adobe.pdf");
        let handler = CFString::new(&bundle_id);
        let status = unsafe {
            LSSetDefaultRoleHandlerForContentType(
                pdf.as_concrete_TypeRef(),
                ROLES_ALL,
                handler.as_concrete_TypeRef(),
            )
        };
        if status != 0 {
            return Err(format!("macOS did not change the default PDF app (error {status})."));
        }
        let current = unsafe {
            let current = LSCopyDefaultRoleHandlerForContentType(pdf.as_concrete_TypeRef(), ROLES_ALL);
            (!current.is_null()).then(|| CFString::wrap_under_create_rule(current).to_string())
        };
        if current.is_some_and(|current| current.eq_ignore_ascii_case(&bundle_id)) {
            Ok(())
        } else {
            Err("macOS did not keep Butter Paper as the default PDF app.".into())
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::os::windows::process::CommandExt as _;
    use std::path::Path;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn register_and_open_settings(executable: &Path) -> Result<(), String> {
        for arguments in super::windows_registry_commands(executable) {
            let status = Command::new("reg.exe")
                .args(&arguments)
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .map_err(|error| format!("Butter Paper could not register for PDFs: {error}"))?;
            if !status.success() {
                return Err("Butter Paper could not register itself for PDF files.".into());
            }
        }
        Command::new("explorer.exe")
            .arg(super::windows_default_apps_url())
            .spawn()
            .map_err(|error| format!("Windows Default Apps could not be opened: {error}"))?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::process::Command;

    pub fn set_default() -> Result<(), String> {
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".local/share")));
        let installed = data_home
            .into_iter()
            .chain([
                std::path::PathBuf::from("/usr/local/share"),
                std::path::PathBuf::from("/usr/share"),
            ])
            .any(|root| root.join("applications").join(super::LINUX_DESKTOP_FILE).is_file());
        if !installed {
            return Err(
                "Install Butter Paper with install-user.sh first, then try again.".into(),
            );
        }
        let status = Command::new("xdg-mime")
            .args(["default", super::LINUX_DESKTOP_FILE, "application/pdf"])
            .status()
            .map_err(|error| format!("xdg-mime could not be run: {error}"))?;
        if !status.success() {
            return Err("The desktop environment did not accept the change.".into());
        }
        let output = Command::new("xdg-mime")
            .args(["query", "default", "application/pdf"])
            .output()
            .map_err(|error| format!("xdg-mime could not be run: {error}"))?;
        if super::linux_default_is_butter_paper(&String::from_utf8_lossy(&output.stdout)) {
            Ok(())
        } else {
            Err("The desktop environment did not keep Butter Paper as the default PDF app.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn windows_registration_lists_butter_paper_for_pdfs_under_the_current_user() {
        let commands = windows_registry_commands(&PathBuf::from(r"C:\Apps\Butter Paper\butter-paper.exe"));
        assert!(commands.iter().all(|command| command[0] == "add" && command[1].starts_with("HKCU\\")));
        assert!(commands.contains(&vec![
            "add".into(),
            r"HKCU\Software\Classes\ButterPaper.PDF\shell\open\command".into(),
            "/ve".into(),
            "/t".into(),
            "REG_SZ".into(),
            "/d".into(),
            r#""C:\Apps\Butter Paper\butter-paper.exe" "%1""#.into(),
            "/f".into(),
        ]));
        assert!(commands.contains(&vec![
            "add".into(),
            r"HKCU\Software\RegisteredApplications".into(),
            "/v".into(),
            "Butter Paper".into(),
            "/t".into(),
            "REG_SZ".into(),
            "/d".into(),
            r"Software\Butter Paper\Capabilities".into(),
            "/f".into(),
        ]));
        assert_eq!(
            windows_default_apps_url(),
            "ms-settings:defaultapps?registeredAppUser=Butter%20Paper"
        );
    }

    #[test]
    fn linux_default_is_verified_from_the_xdg_query() {
        assert!(linux_default_is_butter_paper("butter-paper.desktop\n"));
        assert!(!linux_default_is_butter_paper("org.gnome.Evince.desktop\n"));
        assert!(!linux_default_is_butter_paper(""));
    }
}
