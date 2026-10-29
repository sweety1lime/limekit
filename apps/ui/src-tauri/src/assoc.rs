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
//!
//! `.limetheme` has a ProgID of its own and is written separately, see
//! [`register_theme`].

pub const PROG_ID: &str = "LimeKit.Package";

/// Theme packages have a ProgID of their own. Their association is written
/// only once a theme has been imported, so nobody who just plays wallpapers
/// gets it.
pub const THEME_PROG_ID: &str = "LimeKit.Theme";

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

/// Registers `.limetheme` the way [`register`] registers `.limekit`. Runs after
/// the first theme import, and at every UI start once a theme has been
/// imported, so the path does not go stale when the folder moves.
#[cfg(windows)]
pub fn register_theme() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(map)?;
    if register_theme_under(r"Software\Classes", &exe)? {
        platform::notify_associations_changed();
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn register_theme() -> Result<(), String> {
    Err("file association is Windows-only for now".into())
}

/// Writes the `.limetheme` association under `classes` and reports whether
/// anything changed. The extension is ours alone, as `.limekit` is.
#[cfg(windows)]
fn register_theme_under(classes: &str, exe: &std::path::Path) -> Result<bool, String> {
    let mut changed = false;
    let progid = format!(r"{classes}\{THEME_PROG_ID}");
    changed |= set_if_different(&progid, "", "LimeKit theme")?;
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
    let key = format!(r"{classes}\.{}", themes::EXTENSION);
    changed |= set_if_different(&key, "", THEME_PROG_ID)?;
    changed |= set_if_different(&format!(r"{key}\OpenWithProgids"), THEME_PROG_ID, "")?;
    Ok(changed)
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

/// Removes what [`register`] wrote, for `LimeKit.exe --cleanup`. Only our parts
/// go: another program that owns `.wpk` keeps it, and our entry leaves its
/// "Open with" list.
#[cfg(windows)]
pub fn unregister() -> Result<(), String> {
    let wallpapers = unregister_under(r"Software\Classes")?;
    let themes = unregister_theme_under(r"Software\Classes")?;
    if wallpapers || themes {
        platform::notify_associations_changed();
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn unregister() -> Result<(), String> {
    Ok(())
}

/// Undoes [`register_under`] and reports whether anything changed.
#[cfg(windows)]
fn unregister_under(classes: &str) -> Result<bool, String> {
    let root = windows_registry::CURRENT_USER;
    let mut changed = false;

    for ext in wpk::ACCEPTED_EXTS {
        let path = format!(r"{classes}\.{ext}");
        let Ok(key) = root.options().read().write().open(&path) else {
            continue;
        };
        if key.get_string("").is_ok_and(|current| current == PROG_ID) {
            key.remove_value("").map_err(map)?;
            changed = true;
        }
        let open_with = format!(r"{path}\OpenWithProgids");
        if let Ok(list) = root.options().read().write().open(&open_with)
            && list.get_string(PROG_ID).is_ok()
        {
            list.remove_value(PROG_ID).map_err(map)?;
            changed = true;
            remove_if_empty(&open_with)?;
        }
        remove_if_empty(&path)?;
    }

    let progid = format!(r"{classes}\{PROG_ID}");
    if root.open(&progid).is_ok() {
        root.remove_tree(&progid).map_err(map)?;
        changed = true;
    }
    Ok(changed)
}

/// Undoes [`register_theme_under`] and reports whether anything changed.
#[cfg(windows)]
fn unregister_theme_under(classes: &str) -> Result<bool, String> {
    let root = windows_registry::CURRENT_USER;
    let mut changed = false;

    let path = format!(r"{classes}\.{}", themes::EXTENSION);
    if let Ok(key) = root.options().read().write().open(&path) {
        if key
            .get_string("")
            .is_ok_and(|current| current == THEME_PROG_ID)
        {
            key.remove_value("").map_err(map)?;
            changed = true;
        }
        let open_with = format!(r"{path}\OpenWithProgids");
        if let Ok(list) = root.options().read().write().open(&open_with)
            && list.get_string(THEME_PROG_ID).is_ok()
        {
            list.remove_value(THEME_PROG_ID).map_err(map)?;
            changed = true;
            remove_if_empty(&open_with)?;
        }
        remove_if_empty(&path)?;
    }

    let progid = format!(r"{classes}\{THEME_PROG_ID}");
    if root.open(&progid).is_ok() {
        root.remove_tree(&progid).map_err(map)?;
        changed = true;
    }
    Ok(changed)
}

/// Deletes the key at `path` (inside HKCU) when it has no values and no
/// subkeys left, so a cleaned-up extension does not linger as an empty key.
#[cfg(windows)]
fn remove_if_empty(path: &str) -> Result<(), String> {
    let root = windows_registry::CURRENT_USER;
    let Ok(key) = root.open(path) else {
        return Ok(());
    };
    let empty =
        key.values().map_err(map)?.next().is_none() && key.keys().map_err(map)?.next().is_none();
    if empty {
        root.remove_tree(path).map_err(map)?;
    }
    Ok(())
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
    fn unregister_removes_everything_register_wrote() {
        let sandbox = Sandbox::new("cleanup");
        register_under(&sandbox.0, &exe()).unwrap();

        assert!(unregister_under(&sandbox.0).unwrap());

        for ext in wpk::ACCEPTED_EXTS {
            assert!(
                windows_registry::CURRENT_USER
                    .open(format!(r"{}\.{ext}", sandbox.0))
                    .is_err(),
                ".{ext} should be gone"
            );
        }
        assert!(
            windows_registry::CURRENT_USER
                .open(format!(r"{}\{PROG_ID}", sandbox.0))
                .is_err()
        );
        // A second cleanup finds nothing left to do.
        assert!(!unregister_under(&sandbox.0).unwrap());
    }

    #[test]
    fn unregister_leaves_another_programs_wpk_alone() {
        let sandbox = Sandbox::new("cleanup-foreign");
        sandbox.set(".wpk", "", "Other.Program");
        sandbox.set(r".wpk\OpenWithProgids", "Other.Program", "");
        register_under(&sandbox.0, &exe()).unwrap();

        unregister_under(&sandbox.0).unwrap();

        assert_eq!(sandbox.get(".wpk", "").as_deref(), Some("Other.Program"));
        assert_eq!(
            sandbox
                .get(r".wpk\OpenWithProgids", "Other.Program")
                .as_deref(),
            Some("")
        );
        assert_eq!(sandbox.get(r".wpk\OpenWithProgids", PROG_ID), None);
    }

    #[test]
    fn registers_the_theme_extension_with_icon_and_command_and_only_once() {
        let sandbox = Sandbox::new("theme");
        assert!(register_theme_under(&sandbox.0, &exe()).unwrap());

        assert_eq!(
            sandbox.get(".limetheme", "").as_deref(),
            Some(THEME_PROG_ID)
        );
        assert_eq!(
            sandbox
                .get(r".limetheme\OpenWithProgids", THEME_PROG_ID)
                .as_deref(),
            Some("")
        );
        assert_eq!(
            sandbox.get(&format!(r"{THEME_PROG_ID}\shell\open\command"), ""),
            Some(r#""C:\LimeKit\LimeKit.exe" "%1""#.to_owned())
        );
        assert_eq!(
            sandbox.get(&format!(r"{THEME_PROG_ID}\DefaultIcon"), ""),
            Some(r#""C:\LimeKit\LimeKit.exe",0"#.to_owned())
        );
        // Every UI start after the first import registers again; nothing
        // changes, so Explorer is not made to refresh its icons for nothing.
        assert!(!register_theme_under(&sandbox.0, &exe()).unwrap());
    }

    #[test]
    fn the_wallpaper_association_does_not_carry_the_theme_one() {
        let sandbox = Sandbox::new("theme-apart");

        register_under(&sandbox.0, &exe()).unwrap();

        assert_eq!(sandbox.get(".limetheme", ""), None);
        assert_eq!(
            sandbox.get(&format!(r"{THEME_PROG_ID}\shell\open\command"), ""),
            None
        );
    }

    #[test]
    fn unregister_removes_the_theme_association_and_leaves_the_wallpaper_one() {
        let sandbox = Sandbox::new("theme-cleanup");
        register_under(&sandbox.0, &exe()).unwrap();
        register_theme_under(&sandbox.0, &exe()).unwrap();

        assert!(unregister_theme_under(&sandbox.0).unwrap());

        assert!(
            windows_registry::CURRENT_USER
                .open(format!(r"{}\.limetheme", sandbox.0))
                .is_err()
        );
        assert!(
            windows_registry::CURRENT_USER
                .open(format!(r"{}\{THEME_PROG_ID}", sandbox.0))
                .is_err()
        );
        assert_eq!(sandbox.get(".limekit", "").as_deref(), Some(PROG_ID));
        assert!(!unregister_theme_under(&sandbox.0).unwrap());
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
