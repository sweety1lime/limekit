//! The cursor schemes Windows ships. They sit under HKLM, which any user may
//! read, and name a cursor file for each role.

/// Every scheme as `(name, paths)`. The paths are one comma-separated string as
/// stored: `%SystemRoot%` and the like are not expanded, and the list ends with
/// a reference to the scheme's display name. Empty when the key cannot be read.
pub fn system_schemes() -> Vec<(String, String)> {
    #[cfg(windows)]
    {
        let Ok(key) = windows_registry::LOCAL_MACHINE
            .open(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Control Panel\Cursors\Schemes")
        else {
            return Vec::new();
        };
        let Ok(values) = key.values() else {
            return Vec::new();
        };
        values
            .filter_map(|(name, value)| Some((name, String::try_from(value).ok()?)))
            .collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    #[test]
    fn windows_ships_schemes_with_a_path_for_every_role() {
        let schemes = system_schemes();
        assert!(!schemes.is_empty());
        for (name, paths) in schemes {
            assert!(paths.split(',').count() >= 15, "{name}: {paths}");
        }
    }
}
