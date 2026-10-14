//! Copies installed from the Microsoft Store run as an MSIX package. The Store
//! installs and updates them, so they never update themselves, and Windows
//! lists them as PDF apps from the package manifest rather than the registry.

/// The application id in the package manifest (`<Application Id=…>`).
pub const APPLICATION_ID: &str = "ButterPaper";

/// The running copy's package family name, or `None` outside a Store package.
pub fn family_name() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use std::sync::OnceLock;
        static FAMILY: OnceLock<Option<String>> = OnceLock::new();
        FAMILY.get_or_init(windows::family_name).clone()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Default Apps, opened at this package's PDF association.
pub fn default_apps_url(family_name: &str) -> String {
    format!("ms-settings:defaultapps?registeredAUMID={family_name}!{APPLICATION_ID}")
}

#[cfg(target_os = "windows")]
mod windows {
    use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;

    pub fn family_name() -> Option<String> {
        let mut length = 0u32;
        // Ordinary programs fail with APPMODEL_ERROR_NO_PACKAGE; packaged
        // ones ask for room for the name.
        let status = unsafe { GetCurrentPackageFamilyName(&mut length, std::ptr::null_mut()) };
        if status != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        let mut name = vec![0u16; length as usize];
        let status = unsafe { GetCurrentPackageFamilyName(&mut length, name.as_mut_ptr()) };
        if status != 0 {
            return None;
        }
        name.truncate(length.saturating_sub(1) as usize);
        String::from_utf16(&name).ok()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tests_do_not_run_as_a_store_package() {
        assert_eq!(super::family_name(), None);
    }

    #[test]
    fn default_apps_opens_the_package_application() {
        assert_eq!(
            super::default_apps_url("ButterPaper_abc123"),
            "ms-settings:defaultapps?registeredAUMID=ButterPaper_abc123!ButterPaper"
        );
    }
}
