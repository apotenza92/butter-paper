//! The resource script that build.rs embeds in the Windows app.

#[allow(dead_code)]
mod windows_resource {
    include!("../build/windows_resource.rs");
}

use windows_resource::{windows_resource_script, windows_version_numbers};

#[test]
fn windows_app_names_itself_butter_paper() {
    let script = windows_resource_script(r"C:\src\assets\app\icon.ico", "0.6.5");
    assert!(script.starts_with("1 ICON \"C:\\\\src\\\\assets\\\\app\\\\icon.ico\"\n"));
    assert!(script.contains("FILEVERSION 0,6,5,0\n"));
    assert!(script.contains("VALUE \"FileDescription\", \"Butter Paper\"\n"));
    assert!(script.contains("VALUE \"ProductName\", \"Butter Paper\"\n"));
    assert!(script.contains("VALUE \"ProductVersion\", \"0.6.5\"\n"));
}

#[test]
fn windows_file_version_reads_release_and_beta_versions() {
    assert_eq!(windows_version_numbers("0.6.5"), [0, 6, 5, 0]);
    assert_eq!(windows_version_numbers("0.7.0-beta.2"), [0, 7, 0, 2]);
    assert_eq!(windows_version_numbers("1.0"), [1, 0, 0, 0]);
}
