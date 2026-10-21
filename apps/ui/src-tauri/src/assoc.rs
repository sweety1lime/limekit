//! Per-user package file association (HKCU\Software\Classes, no elevation):
//! double-clicking a package opens this UI with the file as an argument.
//!
//! `.limekit` is ours alone, so it points straight at our ProgID and shows the
//! LimeKit icon. `.wpk` is kept only for packages exported by 0.1, and a
//! three-letter extension is rarely anyone's alone: we take its default only
//! when it is unset or already ours, and otherwise just join its "Open with"
//! list, so a program that owns `.wpk` keeps it.
//!
//! LimeWall 0.1 registered `LimeWall.Package`. Once `.wpk` points at the new
//! ProgID nothing refers to it any more, so it is removed.

pub const PROG_ID: &str = "LimeKit.Package";

/// ProgID LimeWall 0.1 registered.
#[cfg(windows)]
const LEGACY_PROG_ID: &str = "LimeWall.Package";

#[cfg(windows)]
fn map<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

/// Whether we should be the default handler of `.{ext}`, given the ProgID
/// that holds that default now, if any.
#[cfg(windows)]
fn takes_default(ext: &str, current: Option<&str>) -> bool {
    ext == wpk::PACK_EXT
        || matches!(
            current,
            None | Some("") | Some(PROG_ID) | Some(LEGACY_PROG_ID)
        )
}

/// Registers the association, and tells Explorer when it changed so package
/// files pick up the LimeKit icon without a sign-out. Runs at every UI start;
/// when nothing changed it writes nothing and leaves Explorer alone.
#[cfg(windows)]
pub fn register() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(map)?;
    if register_under(r"Software\Classes", &exe)? {
        platform::notify_associations_changed();
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn register() -> Result<(), String> {
    Err("file association is Windows-only for now".into())
}

/// Writes the association under `classes` (a path inside HKCU) and reports
/// whether anything changed. Tests point it at a private key.
#[cfg(windows)]
fn register_under(classes: &str, exe: &std::path::Path) -> Result<bool, String> {
    let root = windows_registry::CURRENT_USER;
    let mut changed = false;

    let progid = format!(r"{classes}\{PROG_ID}");
    changed |= set_if_different(&progid, "", "LimeKit package")?;
    changed |= set_if_different(
        &format!(r"{progid}\DefaultIcon"),
        "",
        &format!("\"{}\",0", exe.display()),
    )?;
    changed |= set_if_different(
        &format!(r"{progid}\shell\open\command"),
        "",
        &format!("\"{}\" \"%1\"", exe.display()),
    )?;

    for ext in wpk::ACCEPTED_EXTS {
        let key = format!(r"{classes}\.{ext}");
        let current = root.open(&key).and_then(|key| key.get_string("")).ok();
        if takes_default(ext, current.as_deref()) {
            changed |= set_if_different(&key, "", PROG_ID)?;
        }
        // On the "Open with" list whoever holds the default.
        changed |= set_if_different(&format!(r"{key}\OpenWithProgids"), PROG_ID, "")?;
    }

    let legacy = format!(r"{classes}\{LEGACY_PROG_ID}");
    if root.open(&legacy).is_ok() {
        root.remove_tree(&legacy).map_err(map)?;
        changed = true;
    }
    Ok(changed)
}

/// Sets value `name` of the key at `path` (inside HKCU) to `value` unless it
/// already holds exactly that. Returns whether it wrote.
#[cfg(windows)]
fn set_if_different(path: &str, name: &str, value: &str) -> Result<bool, String> {
    let key = windows_registry::CURRENT_USER.create(path).map_err(map)?;
    if key.get_string(name).is_ok_and(|current| current == value) {
        return Ok(false);
    }
    key.set_string(name, value).map_err(map)?;
    Ok(true)
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    /// A private stand-in for HKCU\Software\Classes, deleted on drop, so the
    /// tests never touch the associations of the machine they run on.
    struct Sandbox(String);

    impl Sandbox {
        fn new(name: &str) -> Self {
            Self(format!(
                r"Software\LimeKitTest-{}-{name}",
                std::process::id()
            ))
        }

        fn get(&self, path: &str, name: &str) -> Option<String> {
            windows_registry::CURRENT_USER
                .open(format!(r"{}\{path}", self.0))
                .and_then(|key| key.get_string(name))
                .ok()
        }

        fn set(&self, path: &str, name: &str, value: &str) {
            windows_registry::CURRENT_USER
                .create(format!(r"{}\{path}", self.0))
                .expect("create sandbox key")
                .set_string(name, value)
                .expect("write sandbox value");
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = windows_registry::CURRENT_USER.remove_tree(&self.0);
        }
    }

    fn exe() -> std::path::PathBuf {
        std::path::PathBuf::from(r"C:\LimeKit\LimeKit.exe")
    }

    #[test]
    fn registers_every_extension_with_icon_and_command() {
        let sandbox = Sandbox::new("fresh");
        assert!(register_under(&sandbox.0, &exe()).unwrap());

        for ext in wpk::ACCEPTED_EXTS {
            assert_eq!(
                sandbox.get(&format!(".{ext}"), "").as_deref(),
                Some(PROG_ID)
            );
            assert_eq!(
                sandbox
                    .get(&format!(r".{ext}\OpenWithProgids"), PROG_ID)
                    .as_deref(),
                Some("")
            );
        }
        assert_eq!(
            sandbox.get(&format!(r"{PROG_ID}\shell\open\command"), ""),
            Some(r#""C:\LimeKit\LimeKit.exe" "%1""#.to_owned())
        );
        assert_eq!(
            sandbox.get(&format!(r"{PROG_ID}\DefaultIcon"), ""),
            Some(r#""C:\LimeKit\LimeKit.exe",0"#.to_owned())
        );

        // Every UI start registers again; the second time nothing changes, so
        // Explorer is not made to refresh its icons for nothing.
        assert!(!register_under(&sandbox.0, &exe()).unwrap());
    }

    #[test]
    fn leaves_wpk_to_another_program_that_owns_it() {
        let sandbox = Sandbox::new("foreign");
        sandbox.set(".wpk", "", "Other.Program");

        register_under(&sandbox.0, &exe()).unwrap();

        assert_eq!(sandbox.get(".wpk", "").as_deref(), Some("Other.Program"));
        assert_eq!(
            sandbox.get(r".wpk\OpenWithProgids", PROG_ID).as_deref(),
            Some("")
        );
        assert_eq!(sandbox.get(".limekit", "").as_deref(), Some(PROG_ID));
    }

    #[test]
    fn takes_over_and_removes_the_limewall_registration() {
        let sandbox = Sandbox::new("upgrade");
        sandbox.set(".wpk", "", LEGACY_PROG_ID);
        sandbox.set(
            &format!(r"{LEGACY_PROG_ID}\shell\open\command"),
            "",
            r#""C:\Old\LimeWall.exe" "%1""#,
        );

        register_under(&sandbox.0, &exe()).unwrap();

        assert_eq!(sandbox.get(".wpk", "").as_deref(), Some(PROG_ID));
        assert_eq!(sandbox.get(LEGACY_PROG_ID, ""), None);
        assert!(
            windows_registry::CURRENT_USER
                .open(format!(r"{}\{LEGACY_PROG_ID}", sandbox.0))
                .is_err()
        );
    }

    #[test]
    fn claims_wpk_only_when_it_is_unset_or_already_ours() {
        assert!(takes_default("wpk", None));
        assert!(takes_default("wpk", Some("")));
        assert!(takes_default("wpk", Some(PROG_ID)));
        assert!(takes_default("wpk", Some(LEGACY_PROG_ID)));
        assert!(!takes_default("wpk", Some("Other.Program")));
        // Our own extension is ours whatever is there.
        assert!(takes_default(wpk::PACK_EXT, Some("Other.Program")));
    }
}
