//! Finds a file again after it moves, by the identity the file system gives
//! it rather than its path: a bookmark on macOS and the volume's file ID on
//! Windows. Both resolve in one lookup, without searching folders, and only
//! within the original volume. Other platforms have no such lookup.
use std::path::{Path, PathBuf};

/// An opaque record of `path`'s file identity, or `None` where the platform
/// or file system has none.
pub fn locate(path: &Path) -> Option<Vec<u8>> {
    platform::locate(path)
}

/// Where the file recorded by `locator` is now. A file moved to the Trash or
/// Recycle Bin counts as gone.
pub fn resolve(locator: &[u8]) -> Option<PathBuf> {
    platform::resolve(locator).filter(|path| path.is_file() && !in_trash(path))
}

fn in_trash(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        [".Trash", ".Trashes", "$Recycle.Bin"]
            .iter()
            .any(|trash| name.eq_ignore_ascii_case(trash))
    })
}

#[cfg(target_os = "macos")]
mod platform {
    use core_foundation::{
        base::{TCFType, kCFAllocatorDefault},
        data::CFData,
        url::CFURL,
    };
    use core_foundation_sys::url::{
        CFURLCreateBookmarkData, CFURLCreateByResolvingBookmarkData,
        kCFURLBookmarkResolutionWithoutMountingMask, kCFURLBookmarkResolutionWithoutUIMask,
    };
    use std::path::{Path, PathBuf};

    pub fn locate(path: &Path) -> Option<Vec<u8>> {
        let url = CFURL::from_path(path, false)?;
        let data = unsafe {
            CFURLCreateBookmarkData(
                kCFAllocatorDefault,
                url.as_concrete_TypeRef(),
                0,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        };
        (!data.is_null()).then(|| unsafe { CFData::wrap_under_create_rule(data) }.to_vec())
    }

    pub fn resolve(locator: &[u8]) -> Option<PathBuf> {
        let data = CFData::from_buffer(locator);
        let mut stale = 0;
        let url = unsafe {
            CFURLCreateByResolvingBookmarkData(
                kCFAllocatorDefault,
                data.as_concrete_TypeRef(),
                kCFURLBookmarkResolutionWithoutUIMask | kCFURLBookmarkResolutionWithoutMountingMask,
                std::ptr::null(),
                std::ptr::null(),
                &mut stale,
                std::ptr::null_mut(),
            )
        };
        if url.is_null() {
            return None;
        }
        unsafe { CFURL::wrap_under_create_rule(url) }.to_path()
    }
}

#[cfg(windows)]
mod platform {
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt as _, OsStringExt as _},
        path::{Path, PathBuf},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, ExtendedFileIdType, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_DESCRIPTOR,
            FILE_ID_DESCRIPTOR_0, FILE_ID_INFO, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdInfo,
            GetFileInformationByHandleEx, GetFinalPathNameByHandleW, GetVolumePathNameW,
            OPEN_EXISTING, OpenFileById, VOLUME_NAME_DOS,
        },
    };

    const SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

    struct Handle(HANDLE);

    impl Handle {
        fn new(handle: HANDLE) -> Option<Self> {
            (handle != INVALID_HANDLE_VALUE && !handle.is_null()).then_some(Self(handle))
        }

        fn identity(&self) -> Option<FILE_ID_INFO> {
            let mut info: FILE_ID_INFO = unsafe { std::mem::zeroed() };
            let ok = unsafe {
                GetFileInformationByHandleEx(
                    self.0,
                    FileIdInfo,
                    (&raw mut info).cast(),
                    size_of::<FILE_ID_INFO>() as u32,
                )
            };
            (ok != 0).then_some(info)
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain([0]).collect()
    }

    fn open(path: &Path) -> Option<Handle> {
        let path = wide(path);
        Handle::new(unsafe {
            CreateFileW(
                path.as_ptr(),
                FILE_READ_ATTRIBUTES,
                SHARE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        })
    }

    /// Volume serial (8 bytes), 128-bit file ID (16 bytes), then the
    /// volume's root path as UTF-16.
    pub fn locate(path: &Path) -> Option<Vec<u8>> {
        let info = open(path)?.identity()?;
        let mut volume = [0u16; 261];
        let ok = unsafe {
            GetVolumePathNameW(
                wide(path).as_ptr(),
                volume.as_mut_ptr(),
                volume.len() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        let volume = &volume[..volume.iter().position(|unit| *unit == 0)?];
        let mut locator = info.VolumeSerialNumber.to_le_bytes().to_vec();
        locator.extend_from_slice(&info.FileId.Identifier);
        locator.extend(volume.iter().flat_map(|unit| unit.to_le_bytes()));
        Some(locator)
    }

    pub fn resolve(locator: &[u8]) -> Option<PathBuf> {
        if locator.len() < 24 || !locator.len().is_multiple_of(2) {
            return None;
        }
        let serial = u64::from_le_bytes(locator[..8].try_into().ok()?);
        let identifier: [u8; 16] = locator[8..24].try_into().ok()?;
        let volume: Vec<u16> = locator[24..]
            .chunks_exact(2)
            .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
            .collect();
        let volume = open(&PathBuf::from(OsString::from_wide(&volume)))?;
        let mut descriptor = FILE_ID_DESCRIPTOR {
            dwSize: size_of::<FILE_ID_DESCRIPTOR>() as u32,
            Type: ExtendedFileIdType,
            Anonymous: FILE_ID_DESCRIPTOR_0 { FileId: 0 },
        };
        descriptor.Anonymous.ExtendedFileId.Identifier = identifier;
        let file = Handle::new(unsafe {
            OpenFileById(
                volume.0,
                &descriptor,
                FILE_READ_ATTRIBUTES,
                SHARE,
                std::ptr::null(),
                FILE_FLAG_BACKUP_SEMANTICS,
            )
        })?;
        // A different volume mounted at the same root has its own IDs.
        if file.identity()?.VolumeSerialNumber != serial {
            return None;
        }
        let mut path = vec![0u16; 32_768];
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.0,
                path.as_mut_ptr(),
                path.len() as u32,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        } as usize;
        (length > 0 && length < path.len())
            .then(|| PathBuf::from(OsString::from_wide(&path[..length])))
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use std::path::{Path, PathBuf};

    pub fn locate(_: &Path) -> Option<Vec<u8>> {
        None
    }

    pub fn resolve(_: &[u8]) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_folders_do_not_count_as_a_new_location() {
        assert!(in_trash(Path::new("/Users/a/.Trash/plan.pdf")));
        assert!(in_trash(Path::new("/Volumes/Drive/.Trashes/501/plan.pdf")));
        #[cfg(windows)]
        assert!(in_trash(Path::new(r"C:\$Recycle.Bin\S-1-5-21\$R1.pdf")));
        assert!(!in_trash(Path::new("/Users/a/Documents/plan.pdf")));
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn moved_files_resolve_to_their_new_path() {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-scratch/file-locator");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("moved")).unwrap();
        let before = root.join("plan.pdf");
        let after = root.join("moved/renamed plan.pdf");
        std::fs::write(&before, b"%PDF-1.7").unwrap();
        let locator = locate(&before).unwrap();
        std::fs::rename(&before, &after).unwrap();
        let found = resolve(&locator).unwrap();
        assert_eq!(found.canonicalize().unwrap(), after.canonicalize().unwrap());
        std::fs::remove_file(&after).unwrap();
        assert_eq!(resolve(&locator), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
