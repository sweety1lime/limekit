//! Per-user data directory, and the one-time move from the pre-rename layout.
//!
//! The app shipped as LimeWall up to 0.1.0-beta and kept the wallpaper
//! library, the applied-wallpaper state and the daemon log under
//! `%APPDATA%\LimeWall`. Renaming the product to LimeKit would have orphaned
//! all of it, so the first run of a renamed build moves it across instead.
//!
//! Both the daemon and the UI call [`migrate_user_data`] on startup, before
//! anything can create the new directory. The common case is a single
//! directory rename: whichever process gets there first wins and the other
//! finds the work done.
//!
//! The move can fail — an old daemon still running holds its log open — and
//! by the time anyone notices, the new run has created a log and an empty
//! library of its own. A directory holding nothing but that is treated as not
//! there yet ([`is_disposable`]), so the next start merges the old data in
//! instead of concluding the move already happened.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Directory name under the per-user data root (`%APPDATA%` on Windows).
pub const APP_DIR: &str = "LimeKit";

/// Directory names earlier releases used, newest first.
const LEGACY_APP_DIRS: &[&str] = &["LimeWall"];

/// A move that fails is retried this many times in all, [`RETRY_DELAY`] apart:
/// an old daemon that was just asked to quit may still be closing its log.
const MOVE_ATTEMPTS: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_millis(250);

/// What [`migrate_legacy`] did, so the caller can log it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Migration {
    /// No directory from an earlier release name was found: a fresh install,
    /// or one that already migrated.
    NothingToDo,
    /// The data was moved over from `from`.
    Moved { from: PathBuf },
    /// `from` still exists, but the current directory already holds real data,
    /// so neither was touched. Happens when an old build is run again after
    /// migrating.
    Skipped { from: PathBuf },
    /// The move failed — most likely an old daemon still holding its log
    /// open. The caller keeps going; the next start tries again.
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
                "{APP_DIR} already holds data, so {} was left as it is",
                from.display()
            ),
            Migration::Failed { from, error } => write!(
                f,
                "could not move {} to {APP_DIR}: {error} \
                 (quit the old version from the tray, then restart)",
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

/// Whether data from an earlier release is still waiting to be moved.
///
/// Callers use it to decide whether asking an old daemon to quit is worth the
/// round trip, and the UI to hold off seeding a fresh library with its sample
/// wallpaper — which would make the half-created directory look real.
pub fn migration_pending() -> bool {
    dirs::data_dir().is_some_and(|base| pending_in(&base))
}

/// Moves an earlier release's data into [`APP_DIR`] under the per-user data
/// root, retrying a failed move a few times.
///
/// Call this before touching anything inside [`user_dir`].
pub fn migrate_user_data() -> Migration {
    let Some(base) = dirs::data_dir() else {
        return Migration::NothingToDo;
    };
    let mut outcome = migrate_legacy(&base);
    for _ in 1..MOVE_ATTEMPTS {
        if !matches!(outcome, Migration::Failed { .. }) {
            break;
        }
        std::thread::sleep(RETRY_DELAY);
        outcome = migrate_legacy(&base);
    }
    outcome
}

/// Moves the data directory of an earlier release name into [`APP_DIR`].
///
/// Safe to call on every startup: once the move is done it returns
/// [`Migration::NothingToDo`] without touching anything.
pub fn migrate_legacy(base: &Path) -> Migration {
    let Some(legacy) = legacy_dir(base) else {
        return Migration::NothingToDo;
    };
    let current = data_dir(base);

    if !current.exists() {
        return match std::fs::rename(&legacy, &current) {
            Ok(()) => Migration::Moved { from: legacy },
            // Another process moved it between our check and the rename.
            Err(_) if current.exists() && !legacy.exists() => Migration::NothingToDo,
            Err(error) => Migration::Failed {
                from: legacy,
                error: error.to_string(),
            },
        };
    }

    if is_disposable(&current) {
        return match merge_into(&legacy, &current) {
            Ok(()) => Migration::Moved { from: legacy },
            Err(error) => Migration::Failed {
                from: legacy,
                error: error.to_string(),
            },
        };
    }

    Migration::Skipped { from: legacy }
}

fn legacy_dir(base: &Path) -> Option<PathBuf> {
    LEGACY_APP_DIRS
        .iter()
        .map(|name| base.join(name))
        .find(|path| path.is_dir())
}

fn pending_in(base: &Path) -> bool {
    let current = data_dir(base);
    legacy_dir(base).is_some() && (!current.exists() || is_disposable(&current))
}

/// Whether `dir` holds nothing a user would miss: only log files and empty
/// directories. That is all a run leaves behind when its migration failed.
/// Symlinks count as content, so nothing is followed.
fn is_disposable(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().all(|entry| match entry.file_type() {
        Ok(kind) if kind.is_dir() => is_disposable(&entry.path()),
        Ok(kind) if kind.is_file() => entry
            .path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("log")),
        _ => false,
    })
}

/// Moves everything from `legacy` into `current`, which holds only logs and
/// empty directories (see [`is_disposable`]). An empty directory the failed
/// run created gives way to the real one; where both have a log, the newer
/// run's log is kept and the old one dropped. `legacy` is removed once empty.
fn merge_into(legacy: &Path, current: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(legacy)? {
        let entry = entry?;
        let target = current.join(entry.file_name());
        let is_dir = entry.file_type()?.is_dir();
        if is_dir && target.is_dir() && is_disposable(&target) {
            std::fs::remove_dir_all(&target)?;
        }
        if !target.exists() {
            std::fs::rename(entry.path(), &target)?;
        } else if !is_dir
            && entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
        {
            std::fs::remove_file(entry.path())?;
        }
    }
    // Only succeeds when empty, which is the point: anything that could not
    // be placed stays where the user can find it.
    let _ = std::fs::remove_dir(legacy);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
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
            read(&data_dir(base).join("library").join("index.json")),
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
    fn keeps_the_current_directory_when_it_holds_real_data() {
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
        assert_eq!(read(&data_dir(base).join("wallpapers.json")), "current");
        assert_eq!(read(&base.join("LimeWall").join("wallpapers.json")), "old");
    }

    #[test]
    fn merges_into_what_a_failed_run_left_behind() {
        // The first run could not move the directory, then opened its log and
        // created an empty library before anyone noticed.
        let base = tempfile::tempdir().unwrap();
        let base = base.path();
        let legacy = base.join("LimeWall");
        write(&legacy.join("library").join("index.json"), "[\"item\"]");
        write(&legacy.join("wallpapers.json"), "state");
        write(&legacy.join("daemon.log"), "old log");
        let current = data_dir(base);
        write(&current.join("daemon.log"), "new log");
        std::fs::create_dir_all(current.join("library")).unwrap();

        let outcome = migrate_legacy(base);

        assert_eq!(outcome, Migration::Moved { from: legacy.clone() });
        assert_eq!(read(&current.join("library").join("index.json")), "[\"item\"]");
        assert_eq!(read(&current.join("wallpapers.json")), "state");
        assert_eq!(read(&current.join("daemon.log")), "new log");
        assert!(!legacy.exists());
    }

    #[test]
    fn pending_until_the_data_has_moved() {
        let base = tempfile::tempdir().unwrap();
        let base = base.path();
        assert!(!pending_in(base), "nothing to move on a fresh install");

        write(&base.join("LimeWall").join("wallpapers.json"), "state");
        assert!(pending_in(base));

        // A failed run's leftovers do not count as having moved.
        write(&data_dir(base).join("daemon.log"), "log");
        assert!(pending_in(base));

        migrate_legacy(base);
        assert!(!pending_in(base));
    }
}
