//! The six release targets and the names each one's package uses.

use crate::Result;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Os {
    Macos,
    Windows,
    Linux,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Arch {
    Arm64,
    X64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Target {
    pub os: Os,
    pub arch: Arch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    Stable,
    Beta,
}

impl Channel {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "stable" => Ok(Self::Stable),
            "beta" => Ok(Self::Beta),
            _ => Err(format!("channel must be stable or beta, not {value}")),
        }
    }

    pub fn product_name(self) -> &'static str {
        match self {
            Self::Stable => "Butter Paper",
            Self::Beta => "Butter Paper Beta",
        }
    }

    pub fn bundle_identifier(self) -> &'static str {
        match self {
            Self::Stable => "com.butterpaper.desktop",
            Self::Beta => "com.butterpaper.desktop.beta",
        }
    }
}

pub const ALL: [Target; 6] = [
    Target { os: Os::Macos, arch: Arch::Arm64 },
    Target { os: Os::Macos, arch: Arch::X64 },
    Target { os: Os::Windows, arch: Arch::Arm64 },
    Target { os: Os::Windows, arch: Arch::X64 },
    Target { os: Os::Linux, arch: Arch::Arm64 },
    Target { os: Os::Linux, arch: Arch::X64 },
];

impl Target {
    pub fn parse(triple: &str) -> Result<Self> {
        ALL.into_iter()
            .find(|target| target.triple() == triple)
            .ok_or_else(|| format!("unsupported target {triple}"))
    }

    pub fn host() -> Result<Self> {
        let os = match std::env::consts::OS {
            "macos" => Os::Macos,
            "windows" => Os::Windows,
            "linux" => Os::Linux,
            other => return Err(format!("unsupported host OS {other}")),
        };
        let arch = match std::env::consts::ARCH {
            "aarch64" => Arch::Arm64,
            "x86_64" => Arch::X64,
            other => return Err(format!("unsupported host architecture {other}")),
        };
        Ok(Self { os, arch })
    }

    pub fn triple(self) -> &'static str {
        match (self.os, self.arch) {
            (Os::Macos, Arch::Arm64) => "aarch64-apple-darwin",
            (Os::Macos, Arch::X64) => "x86_64-apple-darwin",
            (Os::Windows, Arch::Arm64) => "aarch64-pc-windows-msvc",
            (Os::Windows, Arch::X64) => "x86_64-pc-windows-msvc",
            (Os::Linux, Arch::Arm64) => "aarch64-unknown-linux-gnu",
            (Os::Linux, Arch::X64) => "x86_64-unknown-linux-gnu",
        }
    }

    /// The release asset name the in-app updater and Homebrew look for.
    pub fn asset_name(self, channel: Channel) -> String {
        let platform = match (self.os, self.arch) {
            (Os::Macos, Arch::Arm64) => "macOS-arm64.zip",
            (Os::Macos, Arch::X64) => "macOS-x64.zip",
            (Os::Windows, Arch::Arm64) => "Windows-arm64.zip",
            (Os::Windows, Arch::X64) => "Windows-x64.zip",
            (Os::Linux, Arch::Arm64) => "Linux-arm64.tar.xz",
            (Os::Linux, Arch::X64) => "Linux-x64.tar.xz",
        };
        match channel {
            Channel::Stable => format!("Butter-Paper-{platform}"),
            Channel::Beta => format!("Butter-Paper-Beta-{platform}"),
        }
    }

    /// The architecture in Windows and Linux install paths
    /// (`native_updater::UpdateTarget::package_architecture`).
    pub fn package_architecture(self) -> &'static str {
        match self.arch {
            Arch::Arm64 => "arm64",
            Arch::X64 => "x86_64",
        }
    }

    pub fn exe(self, name: &str) -> String {
        match self.os {
            Os::Windows => format!("{name}.exe"),
            _ => name.to_string(),
        }
    }

    pub fn pdfium_library(self) -> &'static str {
        match self.os {
            Os::Macos => "libpdfium.dylib",
            Os::Windows => "pdfium.dll",
            Os::Linux => "libpdfium.so",
        }
    }

    pub fn go_os(self) -> &'static str {
        match self.os {
            Os::Macos => "darwin",
            Os::Windows => "windows",
            Os::Linux => "linux",
        }
    }

    pub fn go_arch(self) -> &'static str {
        match self.arch {
            Arch::Arm64 => "arm64",
            Arch::X64 => "amd64",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names_match_the_updater_and_homebrew() {
        let mac = Target::parse("aarch64-apple-darwin").unwrap();
        assert_eq!(mac.asset_name(Channel::Stable), "Butter-Paper-macOS-arm64.zip");
        assert_eq!(mac.asset_name(Channel::Beta), "Butter-Paper-Beta-macOS-arm64.zip");
        let linux = Target::parse("x86_64-unknown-linux-gnu").unwrap();
        assert_eq!(linux.asset_name(Channel::Stable), "Butter-Paper-Linux-x64.tar.xz");
        assert_eq!(linux.package_architecture(), "x86_64");
        let windows = Target::parse("aarch64-pc-windows-msvc").unwrap();
        assert_eq!(windows.asset_name(Channel::Stable), "Butter-Paper-Windows-arm64.zip");
        assert_eq!(windows.exe("butter-paper"), "butter-paper.exe");
        assert!(Target::parse("riscv64gc-unknown-linux-gnu").is_err());
    }
}
