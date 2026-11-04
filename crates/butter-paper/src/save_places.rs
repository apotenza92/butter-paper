//! Where a PDF can't, or shouldn't, be saved back to: another app's copy of
//! it (a mail attachment, a temporary file, a file opened from inside a zip),
//! a read-only file, or a read-only disk. Their changes go somewhere else,
//! the system's Downloads folder unless the person chooses another.

use std::path::{Path, PathBuf};

/// Why changes to a PDF need saving somewhere other than where it opened.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourcePlace {
    /// Another app's copy, which that app or the system can remove at any
    /// time.
    AppCopy(CopyOwner),
    /// The file doesn't let Butter Paper change it.
    ReadOnlyFile,
    /// A disk image, disc or share that can't be written to.
    ReadOnlyDisk,
}

/// Whose copy a PDF is, as far as its path shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CopyOwner {
    Mail,
    Outlook,
    Messages,
    /// Taken out of a zip file to open it.
    Zip,
    /// A temporary or app-private file of no app it can name.
    Other,
}

impl SourcePlace {
    /// What the PDF is, as the banner's title.
    pub fn title(self) -> &'static str {
        match self {
            Self::AppCopy(CopyOwner::Mail) => "This PDF is Mail’s copy of an attachment",
            Self::AppCopy(CopyOwner::Outlook) => "This PDF is Outlook’s copy of an attachment",
            Self::AppCopy(CopyOwner::Messages) => "This PDF is Messages’ copy of an attachment",
            Self::AppCopy(CopyOwner::Zip) => "This PDF is a copy from inside a zip file",
            Self::AppCopy(CopyOwner::Other) => "This PDF is a temporary copy",
            Self::ReadOnlyFile => "This PDF is read-only",
            Self::ReadOnlyDisk => "This PDF is on a read-only disk",
        }
    }

    /// Why its changes need saving somewhere else, and that they are kept
    /// meanwhile.
    pub fn message(self) -> &'static str {
        match self {
            Self::AppCopy(CopyOwner::Zip) => {
                "Edits here don't change the zip file, so save the PDF somewhere else. Your \
                 changes stay here until you do."
            }
            Self::AppCopy(_) => {
                "It can be removed at any time, so save it somewhere else. Your changes stay \
                 here until you do."
            }
            Self::ReadOnlyFile | Self::ReadOnlyDisk => {
                "Save it as a new file. Your changes stay here until you do."
            }
        }
    }

    /// Whether a new file beside it (such as its Word document) belongs
    /// somewhere else too.
    pub fn moves_new_files(self) -> bool {
        !matches!(self, Self::ReadOnlyFile)
    }
}

/// Where this system keeps other apps' copies and the person's Downloads.
#[derive(Clone, Debug)]
pub struct SavePlaces {
    pub copies: CopyRoots,
    pub downloads: Option<PathBuf>,
}

impl SavePlaces {
    pub fn system() -> Self {
        Self {
            copies: CopyRoots::system(),
            downloads: downloads_folder(),
        }
    }

    /// Why changes to the PDF at `pdf` can't go back to it, if they can't.
    pub fn source_place(&self, pdf: &Path) -> Option<SourcePlace> {
        source_place_with(pdf, &self.copies)
    }

    pub fn is_app_copy(&self, pdf: &Path) -> bool {
        app_copy_owner(pdf, &self.copies).is_some()
    }
}

/// Why changes to the PDF at `pdf` can't go back to it, if they can't.
pub fn source_place(pdf: &Path) -> Option<SourcePlace> {
    source_place_with(pdf, &CopyRoots::system())
}

fn source_place_with(pdf: &Path, copies: &CopyRoots) -> Option<SourcePlace> {
    if let Some(owner) = app_copy_owner(pdf, copies) {
        return Some(SourcePlace::AppCopy(owner));
    }
    let folder = pdf.parent()?;
    if read_only_disk(folder) {
        return Some(SourcePlace::ReadOnlyDisk);
    }
    read_only_file(pdf).then_some(SourcePlace::ReadOnlyFile)
}

/// Whether `pdf` is another app's copy, judged by its path alone.
pub fn is_app_copy(pdf: &Path) -> bool {
    app_copy_owner(pdf, &CopyRoots::system()).is_some()
}

/// The system's Downloads folder: XDG's on Linux, the Downloads known folder
/// on Windows (wherever it was moved), `~/Downloads` on macOS.
pub fn downloads_folder() -> Option<PathBuf> {
    // A scripted review saves into its own folder, not the reviewer's
    // Downloads, and so meets no privacy prompt.
    #[cfg(feature = "review-driver")]
    if let Some(folder) = std::env::var_os("BP_REVIEW_DOWNLOADS_DIR") {
        return Some(PathBuf::from(folder)).filter(|folder| folder.is_dir());
    }
    dirs::download_dir().filter(|folder| folder.is_dir())
}

/// The name a PDF's saved copy takes: its own, without the `[11854]`
/// Outlook adds to tell its attachments apart.
pub fn saved_copy_name(pdf: &Path) -> String {
    let name = pdf
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document.pdf".to_owned());
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name.as_str(), ""),
    };
    match stem
        .strip_suffix(']')
        .and_then(|rest| rest.rsplit_once('['))
    {
        Some((base, number))
            if !base.is_empty()
                && !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit()) =>
        {
            format!("{base}{extension}")
        }
        _ => name,
    }
}

/// `name` in `folder` without replacing a file: `name` while that is free,
/// then the first free `Stem 2.ext`, `Stem 3.ext`, ….
pub fn unused_path_in(folder: &Path, name: &str) -> PathBuf {
    let name = Path::new(name);
    let stem = name
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".to_owned());
    let extension = name
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    std::iter::once(folder.join(name))
        .chain((2..).map(|n| folder.join(format!("{stem} {n}{extension}"))))
        .find(|path| !path.exists())
        .expect("an unused name exists")
}

/// The system setting that lets Butter Paper save in a folder the system
/// refused, and where to find the switch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderPermission {
    /// The button that opens that setting.
    pub button: &'static str,
    pub url: &'static str,
    /// Where the switch is, as a clause: "open … and turn on …".
    pub steps: String,
    /// Whether it takes effect only once Butter Paper reopens.
    pub needs_reopen: bool,
}

impl FolderPermission {
    /// How to allow it, then `then` ("retry", "convert again").
    pub fn sentence(&self, then: &str) -> String {
        if self.needs_reopen {
            format!(
                "To save there in future, {}. It takes effect once Butter Paper reopens.",
                self.steps
            )
        } else {
            format!("To save there, {}, then {then}.", self.steps)
        }
    }
}

/// Which macOS privacy switch covers a folder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MacFolderSwitch {
    /// An item under Butter Paper in Files & Folders.
    FilesAndFolders(&'static str),
    /// Butter Paper in Full Disk Access, the switch for everywhere else.
    FullDiskAccess,
}

/// The macOS switch for `folder`, given the home folder and whether the
/// folder is on a disk of this Mac (rather than a network share).
pub fn mac_folder_switch(folder: &Path, home: &Path, local_disk: bool) -> MacFolderSwitch {
    for (name, item) in [
        ("Desktop", "Desktop Folder"),
        ("Documents", "Documents Folder"),
        ("Downloads", "Downloads Folder"),
    ] {
        if folder.starts_with(home.join(name)) {
            return MacFolderSwitch::FilesAndFolders(item);
        }
    }
    // A disk or share mounted under /Volumes (the Mac's own disk is "/").
    if folder.starts_with("/Volumes") && folder.components().count() > 2 {
        return MacFolderSwitch::FilesAndFolders(if local_disk {
            "Removable Volumes"
        } else {
            "Network Volumes"
        });
    }
    MacFolderSwitch::FullDiskAccess
}

impl MacFolderSwitch {
    pub fn permission(self) -> FolderPermission {
        match self {
            Self::FilesAndFolders(item) => FolderPermission {
                button: "Open Files & Folders",
                url: "x-apple.systempreferences:com.apple.preference.security?\
                      Privacy_FilesAndFolders",
                steps: format!(
                    "open System Settings › Privacy & Security › Files & Folders and, under \
                     Butter Paper, turn on {item}"
                ),
                needs_reopen: false,
            },
            Self::FullDiskAccess => FolderPermission {
                button: "Open Full Disk Access",
                url: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
                steps: "open System Settings › Privacy & Security › Full Disk Access and turn \
                        on Butter Paper"
                    .to_owned(),
                needs_reopen: true,
            },
        }
    }
}

/// The Windows setting behind a refused folder: Controlled folder access,
/// the one that stops apps saving in protected folders.
pub fn windows_folder_permission() -> FolderPermission {
    FolderPermission {
        button: "Open Ransomware Protection",
        url: "windowsdefender://ransomwareprotection",
        steps: "open Windows Security › Virus & threat protection › Ransomware protection, \
                choose Allow an app through Controlled folder access and add Butter Paper"
            .to_owned(),
        needs_reopen: false,
    }
}

/// Whether the error that refused a folder came from the system's privacy
/// protection (macOS's EPERM, Windows' Controlled folder access, which looks
/// like any other "access is denied"), not the folder's own permissions.
pub fn refused_by_privacy(error: &str) -> bool {
    if cfg!(target_os = "macos") {
        error
            .to_ascii_lowercase()
            .contains("operation not permitted (os error 1)")
    } else {
        cfg!(windows)
    }
}

/// How to let Butter Paper save in `folder` after the system's privacy
/// protection refused it, where the system has a setting for it (macOS and
/// Windows).
pub fn folder_permission(folder: &Path) -> Option<FolderPermission> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir()?;
        Some(mac_folder_switch(folder, &home, local_disk(folder)).permission())
    }
    #[cfg(windows)]
    {
        let _ = folder;
        Some(windows_folder_permission())
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = folder;
        None
    }
}

#[cfg(target_os = "macos")]
fn local_disk(folder: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(folder.as_os_str().as_bytes()) else {
        return true;
    };
    let mut stats = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `stats` is written before it's read.
    unsafe {
        libc::statfs(path.as_ptr(), stats.as_mut_ptr()) != 0
            || stats.assume_init().f_flags & libc::MNT_LOCAL as u32 != 0
    }
}

/// The folders whose files are other apps' copies, and the folders inside
/// them that hold people's own files.
#[derive(Clone, Debug, Default)]
pub struct CopyRoots {
    pub copies: Vec<PathBuf>,
    pub own_files: Vec<PathBuf>,
}

impl CopyRoots {
    /// This system's.
    pub fn system() -> Self {
        let mut roots = Self::default();
        let temp = std::env::temp_dir();
        roots.copies.push(temp);
        let home = dirs::home_dir();
        if cfg!(target_os = "macos") {
            roots.copies.extend(
                [
                    "/private/var/folders",
                    "/var/folders",
                    "/private/tmp",
                    "/tmp",
                ]
                .map(PathBuf::from),
            );
            if let Some(home) = &home {
                let library = home.join("Library");
                roots.copies.push(library.clone());
                // iCloud Drive and the file providers' folders (Dropbox,
                // Google Drive, OneDrive, …) hold people's own files.
                roots.own_files.push(library.join("Mobile Documents"));
                roots.own_files.push(library.join("CloudStorage"));
            }
        } else if cfg!(windows) {
            // Outlook's attachments (INetCache\Content.Outlook), the Mail and
            // Teams apps' and %TEMP%, where Explorer opens a zip's files.
            roots.copies.extend(dirs::cache_dir());
        } else {
            roots.copies.extend(["/tmp", "/var/tmp"].map(PathBuf::from));
            roots.copies.extend(dirs::cache_dir());
        }
        // Compared as the paths of opened documents are: resolved.
        for list in [&mut roots.copies, &mut roots.own_files] {
            let resolved: Vec<_> = list
                .iter()
                .filter_map(|path| std::fs::canonicalize(path).ok())
                .map(|path| dunce::simplified(&path).to_path_buf())
                .collect();
            list.extend(resolved);
        }
        roots
    }
}

/// Whose copy `pdf` is when it is inside `roots`' copies and not their own
/// files.
pub fn app_copy_owner(pdf: &Path, roots: &CopyRoots) -> Option<CopyOwner> {
    let pdf = dunce::simplified(pdf);
    let inside = |root: &PathBuf| pdf.starts_with(root) && pdf != root;
    if !roots.copies.iter().any(inside) || roots.own_files.iter().any(inside) {
        return None;
    }
    let folders: Vec<String> = pdf
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let has = |name: &str| folders.iter().any(|folder| folder == name);
    let starts = |prefix: &str| folders.iter().any(|folder| folder.starts_with(prefix));
    Some(
        if has("com.apple.mail")
            || has("mail downloads")
            || starts("microsoft.windowscommunicationsapps")
        {
            CopyOwner::Mail
        } else if has("com.microsoft.outlook")
            || has("content.outlook")
            || has("olk")
            || (has("ubf8t346g9.office") && has("outlook"))
        {
            CopyOwner::Outlook
        } else if has("messages") && has("attachments") {
            CopyOwner::Messages
        } else if folders.iter().any(|folder| {
            // Explorer's `Temp1_Name.zip`, Archive Manager's `.fr-XXXXXX`
            // and 7-Zip's `7zO…`.
            (folder.starts_with("temp")
                && folder.ends_with(".zip")
                && folder[4..].starts_with(|c: char| c.is_ascii_digit()))
                || folder.starts_with(".fr-")
                || folder.starts_with("7zo")
        }) {
            CopyOwner::Zip
        } else {
            CopyOwner::Other
        },
    )
}

#[cfg(unix)]
fn read_only_disk(folder: &Path) -> bool {
    rustix::fs::statvfs(folder)
        .is_ok_and(|stats| stats.f_flag.contains(rustix::fs::StatVfsMountFlags::RDONLY))
}

#[cfg(windows)]
fn read_only_disk(folder: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW};
    // winnt.h's flag, which windows-sys keeps behind another feature.
    const FILE_READ_ONLY_VOLUME: u32 = 0x0008_0000;
    let wide: Vec<u16> = folder.as_os_str().encode_wide().chain([0]).collect();
    let mut volume = [0u16; 1024];
    let mut flags = 0u32;
    // SAFETY: both buffers outlive the calls and their lengths are given.
    unsafe {
        GetVolumePathNameW(wide.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) != 0
            && GetVolumeInformationW(
                volume.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut flags,
                std::ptr::null_mut(),
                0,
            ) != 0
            && flags & FILE_READ_ONLY_VOLUME != 0
    }
}

#[cfg(unix)]
fn read_only_file(pdf: &Path) -> bool {
    // For this user, not the file's mode for anyone.
    rustix::fs::access(pdf, rustix::fs::Access::WRITE_OK).is_err() && pdf.exists()
}

#[cfg(windows)]
fn read_only_file(pdf: &Path) -> bool {
    std::fs::metadata(pdf).is_ok_and(|metadata| metadata.permissions().readonly())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> CopyRoots {
        CopyRoots {
            copies: vec![
                PathBuf::from("/Users/me/Library"),
                PathBuf::from("/tmp"),
                PathBuf::from("/Users/me/AppData/Local"),
            ],
            own_files: vec![
                PathBuf::from("/Users/me/Library/Mobile Documents"),
                PathBuf::from("/Users/me/Library/CloudStorage"),
            ],
        }
    }

    #[test]
    fn other_apps_copies_are_named_by_their_folders() {
        let roots = roots();
        let owner = |path: &str| app_copy_owner(Path::new(path), &roots);
        assert_eq!(
            owner(
                "/Users/me/Library/Containers/com.apple.mail/Data/Library/Mail Downloads/\
                 415D1A6D-BBB5-41C0-8F26-432ED18F9FF9/CV.docx.pdf"
            ),
            Some(CopyOwner::Mail)
        );
        assert_eq!(
            owner(
                "/Users/me/Library/Group Containers/UBF8T346G9.Office/Outlook/Outlook 15 \
                 Profiles/Main Identity/Files/S0/1/Attachments/0/CV[11854].pdf"
            ),
            Some(CopyOwner::Outlook)
        );
        assert_eq!(
            owner(
                "/Users/me/AppData/Local/Microsoft/Windows/INetCache/Content.Outlook/AB12CD34/a.pdf"
            ),
            Some(CopyOwner::Outlook)
        );
        assert_eq!(
            owner("/Users/me/Library/Messages/Attachments/3f/15/AB-CD/a.pdf"),
            Some(CopyOwner::Messages)
        );
        assert_eq!(
            owner("/Users/me/AppData/Local/Temp/Temp1_Drawings.zip/Drawings/A101.pdf"),
            Some(CopyOwner::Zip)
        );
        assert_eq!(owner("/tmp/.fr-Ab12Cd/A101.pdf"), Some(CopyOwner::Zip));
        assert_eq!(owner("/tmp/mozilla_me0/a.pdf"), Some(CopyOwner::Other));
    }

    #[test]
    fn peoples_own_folders_are_not_copies() {
        let roots = roots();
        let owner = |path: &str| app_copy_owner(Path::new(path), &roots);
        assert_eq!(owner("/Users/me/Downloads/CV.docx.pdf"), None);
        assert_eq!(owner("/Users/me/Documents/Library/a.pdf"), None);
        assert_eq!(
            owner("/Users/me/Library/Mobile Documents/com~apple~CloudDocs/a.pdf"),
            None
        );
        assert_eq!(owner("/Users/me/Library/CloudStorage/Dropbox/a.pdf"), None);
        // Beside the folder, not in it.
        assert_eq!(owner("/tmpfiles/a.pdf"), None);
    }

    #[test]
    fn a_saved_copy_drops_outlooks_number() {
        assert_eq!(
            saved_copy_name(Path::new("/a/DeanneHealCV_Sep2026.docx[11854].pdf")),
            "DeanneHealCV_Sep2026.docx.pdf"
        );
        assert_eq!(
            saved_copy_name(Path::new("/a/Plan [A].pdf")),
            "Plan [A].pdf"
        );
        assert_eq!(saved_copy_name(Path::new("/a/[12].pdf")), "[12].pdf");
    }

    #[test]
    fn a_name_in_use_gains_a_number() {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-scratch")
            .join(format!("save-places-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(unused_path_in(&folder, "CV.pdf"), folder.join("CV.pdf"));
        std::fs::write(folder.join("CV.pdf"), b"").unwrap();
        assert_eq!(unused_path_in(&folder, "CV.pdf"), folder.join("CV 2.pdf"));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_file_this_user_cannot_change_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-scratch")
            .join(format!("save-places-read-only-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let pdf = folder.join("a.pdf");
        std::fs::write(&pdf, b"%PDF").unwrap();
        assert_eq!(source_place(&pdf), None);
        std::fs::set_permissions(&pdf, std::fs::Permissions::from_mode(0o444)).unwrap();
        // Root may write to anything.
        if unsafe { libc::geteuid() } != 0 {
            assert_eq!(source_place(&pdf), Some(SourcePlace::ReadOnlyFile));
        }
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_refused_mac_folder_names_its_switch() {
        let home = Path::new("/Users/me");
        let switch = |folder: &str, local| mac_folder_switch(Path::new(folder), home, local);
        assert_eq!(
            switch("/Users/me/Downloads", true),
            MacFolderSwitch::FilesAndFolders("Downloads Folder")
        );
        assert_eq!(
            switch("/Users/me/Documents/Plans", true),
            MacFolderSwitch::FilesAndFolders("Documents Folder")
        );
        assert_eq!(
            switch("/Users/me/Desktop", true),
            MacFolderSwitch::FilesAndFolders("Desktop Folder")
        );
        assert_eq!(
            switch("/Volumes/USB/Scans", true),
            MacFolderSwitch::FilesAndFolders("Removable Volumes")
        );
        assert_eq!(
            switch("/Volumes/Office", false),
            MacFolderSwitch::FilesAndFolders("Network Volumes")
        );
        assert_eq!(
            switch(
                "/Users/me/Library/Mobile Documents/com~apple~CloudDocs",
                true
            ),
            MacFolderSwitch::FullDiskAccess
        );
        let downloads = MacFolderSwitch::FilesAndFolders("Downloads Folder").permission();
        assert_eq!(
            downloads.sentence("retry"),
            "To save there, open System Settings › Privacy & Security › Files & Folders and, \
             under Butter Paper, turn on Downloads Folder, then retry."
        );
        assert!(
            MacFolderSwitch::FullDiskAccess
                .permission()
                .sentence("retry")
                .contains("once Butter Paper reopens")
        );
    }
}
