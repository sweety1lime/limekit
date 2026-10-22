//! Which Windows this is, for deciding which tweaks apply here.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsVersion {
    /// 19045 for Windows 10 22H2, 26100 for Windows 11 24H2.
    pub build: u32,
    /// The update build revision, the part after the dot in 26100.4061.
    pub ubr: u32,
    /// `EditionID` as Windows reports it: Core, Professional, Enterprise...
    pub edition_id: String,
    /// 22H2, 24H2 and so on; empty before 20H2.
    pub display_version: String,
}

/// Reads the version from `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`,
/// which any user may read. `ProductName` there still says "Windows 10" on
/// Windows 11, so only the build tells them apart.
pub fn windows_version() -> Option<WindowsVersion> {
    #[cfg(windows)]
    {
        let key = windows_registry::LOCAL_MACHINE
            .open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
            .ok()?;
        Some(WindowsVersion {
            build: key.get_string("CurrentBuildNumber").ok()?.parse().ok()?,
            ubr: key.get_u32("UBR").unwrap_or(0),
            edition_id: key.get_string("EditionID").unwrap_or_default(),
            display_version: key
                .get_string("DisplayVersion")
                .or_else(|_| key.get_string("ReleaseId"))
                .unwrap_or_default(),
        })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    #[test]
    fn reads_a_windows_10_or_later_build() {
        let version = windows_version().expect("version readable");
        assert!(version.build >= 10240, "{version:?}");
    }

    #[test]
    fn reads_an_edition() {
        let version = windows_version().expect("version readable");
        assert!(!version.edition_id.is_empty(), "{version:?}");
    }
}
