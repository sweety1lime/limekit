//! Wallpaper playback and the daemon behind the two renderer executables:
//! the `renderer` CLI and the windowless `limekit-daemon` used by autostart.

pub mod daemon;
pub mod playback;
pub mod playlist;

use std::time::Duration;

/// How long a still-running LimeWall 0.1 daemon gets to quit before the data
/// move goes ahead anyway.
const LEGACY_STOP_TIMEOUT: Duration = Duration::from_secs(3);

/// Readies the per-user data directory for this run, for upgrades from
/// LimeWall 0.1: when the old data is still waiting to move, asks the old
/// daemon to quit first — it holds its log open, which would make the move
/// fail — then moves it. Must run before anything opens a file inside the
/// data directory. Returns a line worth logging, or `None` if there was
/// nothing to do.
pub fn take_over_legacy_data() -> Option<String> {
    let stopped = platform::appdata::migration_pending()
        .then(|| ipc::stop_legacy_daemon(LEGACY_STOP_TIMEOUT))
        .filter(|outcome| *outcome != ipc::LegacyDaemon::NotRunning);
    let migration = platform::appdata::migrate_user_data();
    match (stopped, migration) {
        (None, platform::appdata::Migration::NothingToDo) => None,
        (Some(stopped), migration) => {
            Some(format!("old LimeWall daemon: {stopped:?}; data directory: {migration}"))
        }
        (None, migration) => Some(format!("data directory: {migration}")),
    }
}
