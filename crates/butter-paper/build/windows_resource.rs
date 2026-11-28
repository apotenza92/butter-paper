// Shared by build.rs and tests/windows_resource.rs, so the resource script
// the Windows build embeds has a deterministic test.

/// The resource script for the Windows app: icon 1, which GPUI loads for
/// its windows and the taskbar, and the version information. Windows takes
/// the app's name in Open with, Default Apps and Task Manager from
/// `FileDescription`; without it, Windows shows the file name `butter-paper`.
pub fn windows_resource_script(icon_path: &str, version: &str) -> String {
    let icon = icon_path.replace('\\', "\\\\");
    let numbers = windows_version_numbers(version);
    let [major, minor, patch, build] = numbers;
    format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},{build}
PRODUCTVERSION {major},{minor},{patch},{build}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Alex Potenza"
      VALUE "FileDescription", "Butter Paper"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "butter-paper"
      VALUE "OriginalFilename", "butter-paper.exe"
      VALUE "ProductName", "Butter Paper"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}

/// The four numbers of a Windows file version: `0.6.5` is 0.6.5.0 and
/// `0.7.0-beta.2` is 0.7.0.2. A missing or unreadable part is 0.
pub fn windows_version_numbers(version: &str) -> [u16; 4] {
    let (release, prerelease) = version.split_once('-').unwrap_or((version, ""));
    let mut numbers = [0u16; 4];
    for (number, part) in numbers.iter_mut().zip(release.split('.')) {
        *number = part.parse().unwrap_or(0);
    }
    numbers[3] = prerelease
        .rsplit('.')
        .next()
        .and_then(|part| part.parse().ok())
        .unwrap_or(0);
    numbers
}
