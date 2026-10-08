//! Per-user data directory, and the one-time move from the pre-rename layout.
//!
//! The app shipped as LimeWall up to 0.1.0-beta and kept the wallpaper
//! library, the applied-wallpaper state and the daemon log under
//! `%APPDATA%\LimeWall`. Renaming the product to LimeKit would have orphaned
//! all of it, so the first run of a renamed build moves the whole directory
//! across instead.
//!
//! Both the daemon and the UI call [`migrate_legacy`] on startup. The move is
//! a single directory rename, so whichever process gets there first wins and
//! the other one finds the new directory already in place — no locking needed.

use std::path::{Path, PathBuf};

/// Directory name under the per-user data root (`%APPDATA%` on Windows).
pub const APP_DIR: &str = "LimeKit";

/// Directory names earlier releases used, newest first.
const LEGACY_APP_DIRS: &[&str] = &["LimeWall"];

/// What [`migrate_legacy`] did, so the caller can log it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Migration {
    /// No directory from an earlier release name was found: a fresh install,
    /// or a build that already migrated.
    NothingToDo,
    /// The directory was moved over from `from`.
    Moved { from: PathBuf },
    /// Both directories existed, so `from` was left alone. Happens when an old
    /// build is run again after migrating; the current directory stays
    /// authoritative and the stale one is the user's to delete.
    Skipped { from: PathBuf },
    /// The move failed — most likely an old daemon still holding the log file
    /// open. The caller keeps going with an empty current directory rather
    /// than refusing to start.
    Failed { from: PathBuf, error: String },
}

impl std::fmt::Display for Migration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Migration::NothingToDo => write!(f, "no data directory to migrate"),
            Migration::Moved { from } => {
                write!(f, "moved {} to {APP_DIR}", from.display())
            }
            Migration::Skipped { from } => write!(
                f,
                "{APP_DIR} already exists, left {} alone — it can be deleted",
                from.display()
            ),
            Migration::Failed { from, error } => write!(
                f,
                "could not move {} to {APP_DIR}: {error} \
                 (quit an older build from the tray and restart)",
                from.display()
            ),
        }
    }
}

/// Current per-user data directory under an explicit root. See [`user_dir`]
/// for the one callers normally want.
pub fn data_dir(base: &Path) -> PathBuf {
    base.join(APP_DIR)
}

/// Current per-user data directory, e.g. `%APPDATA%\LimeKit`.
pub fn user_dir() -> Option<PathBuf> {
    Some(data_dir(&dirs::data_dir()?))
}

/// Runs [`migrate_legacy`] against the per-user data root.
///
/// Call this before touching anything inside [`user_dir`] — creating the log
/// file or the library directory first would make the current directory exist
/// and the move would be skipped, silently orphaning the user's library.
pub fn migrate_user_data() -> Migration {
    match dirs::data_dir() {
        Some(base) => migrate_legacy(&base),
        None => Migration::NothingToDo,
    }
}

/// Moves the data directory of an earlier release name into [`APP_DIR`], once.
///
/// Safe to call on every startup: it returns [`Migration::NothingToDo`] as
/// soon as the current directory exists.
pub fn migrate_legacy(base: &Path) -> Migration {
    let current = data_dir(base);
    if current.exists() {
        // Already migrated, or a fresh install that got there first. Report a
        // leftover old directory so the caller can mention it in the log.
        for name in LEGACY_APP_DIRS {
            let legacy = base.join(name);
            if legacy.exists() {
                return Migration::Skipped { from: legacy };
            }
        }
        return Migration::NothingToDo;
    }

    for name in LEGACY_APP_DIRS {
        let legacy = base.join(name);
        if !legacy.is_dir() {
            continue;
        }
        return match std::fs::rename(&legacy, &current) {
            Ok(()) => Migration::Moved { from: legacy },
            // Another process may have migrated between the check above and
            // the rename; that is a success from this caller's point of view.
            Err(_) if current.exists() => Migration::NothingToDo,
            Err(error) => Migration::Failed {
                from: legacy,
                error: error.to_string(),
            },
        };
    }

    Migration::NothingToDo
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn moves_the_legacy_directory_with_its_contents() {
        let base = tempfile::tempdir().unwrap();
        let base = base.path();
        write(&base.join("LimeWall").join("library").join("index.json"), "[]");

        let outcome = migrate_legacy(base);

        assert_eq!(
            outcome,
            Migration::Moved {
                from: base.join("LimeWall")
            }
        );
        assert!(!base.join("LimeWall").exists());
        assert_eq!(
            std::fs::read_to_string(data_dir(base).join("library").join("index.json")).unwrap(),
            "[]"
        );
    }

    #[test]
    fn does_nothing_on_a_fresh_install() {
        let base = tempfile::tempdir().unwrap();

        assert_eq!(migrate_legacy(base.path()), Migration::NothingToDo);
        assert!(!data_dir(base.path()).exists());
    }

    #[test]
    fn is_idempotent() {
        let base = tempfile::tempdir().unwrap();
        let base = base.path();
        write(&base.join("LimeWall").join("wallpapers.json"), "{}");

        assert!(matches!(migrate_legacy(base), Migration::Moved { .. }));
        assert_eq!(migrate_legacy(base), Migration::NothingToDo);
    }

    #[test]
    fn keeps_the_current_directory_when_both_exist() {
        let base = tempfile::tempdir().unwrap();
        let base = base.path();
        write(&base.join("LimeWall").join("wallpapers.json"), "old");
        write(&data_dir(base).join("wallpapers.json"), "current");

        assert_eq!(
            migrate_legacy(base),
            Migration::Skipped {
                from: base.join("LimeWall")
            }
        );
        assert_eq!(
            std::fs::read_to_string(data_dir(base).join("wallpapers.json")).unwrap(),
            "current"
        );
        assert!(base.join("LimeWall").exists());
    }
}
