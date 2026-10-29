//! Removing LimeKit from the system: everything it writes outside its own
//! folder, so that deleting the folder afterwards leaves nothing behind. The
//! settings screen starts `LimeKit.exe --cleanup` and closes itself, and an
//! uninstaller can run the same command.
//!
//! What goes: the running daemon, which also puts the Windows wallpaper back;
//! every tweak and theme change still in effect, undone through the journal;
//! the autostart entry; our part of the `.limekit` / `.wpk` and `.limetheme`
//! associations. With `--remove-data` also the wallpaper and theme libraries,
//! settings and log in `%APPDATA%\LimeKit` and the window's WebView2 data in
//! `%LOCALAPPDATA%`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{assoc, daemon_client, tweak_panel};

/// `identifier` in tauri.conf.json, which names the folder WebView2 keeps the
/// window's data in.
const WEBVIEW_DATA_DIR: &str = "com.twofame.limekit";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub remove_data: bool,
    /// Process to wait for first: the window that asked for the cleanup still
    /// holds its WebView2 data open, and registers the association on start.
    pub after: Option<u32>,
    /// Report the outcome in a message box. An uninstaller runs without one.
    pub report: bool,
}

/// `Some` when the command line asks for a cleanup instead of the window.
pub fn from_args(args: impl IntoIterator<Item = OsString>) -> Option<Options> {
    let mut args = args.into_iter().skip(1);
    let mut cleanup = false;
    let mut options = Options::default();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--cleanup") => cleanup = true,
            Some("--remove-data") => options.remove_data = true,
            Some("--report") => options.report = true,
            Some("--after") => {
                options.after = args.next().and_then(|pid| pid.to_str()?.parse().ok());
            }
            _ => {}
        }
    }
    cleanup.then_some(options)
}

/// Runs the cleanup and returns the exit code: 0 when everything went.
pub fn run(options: &Options) -> i32 {
    if let Some(pid) = options.after {
        platform::wait_for_exit(pid, Duration::from_secs(30));
    }

    let mut failures = Vec::new();
    // First, so it neither restarts anything nor keeps its log open.
    match ipc::stop_daemon(&daemon_client::endpoint(), Duration::from_secs(10)) {
        ipc::DaemonStop::NotRunning | ipc::DaemonStop::Stopped => {}
        ipc::DaemonStop::Unresponsive => {
            failures.push("фоновый плеер не завершился за 10 секунд".to_owned());
        }
    }
    // Before the data goes, since the journal is part of it.
    let mut tweaks_undone = 0;
    match tweak_panel::undo_all_for_cleanup() {
        Ok(undone) => {
            tweaks_undone = undone.reverted;
            if !undone.changed_since.is_empty() {
                failures.push(format!(
                    "эти параметры Windows поменяли уже после LimeKit, их не трогали: {}",
                    undone.changed_since.join(", ")
                ));
            }
        }
        Err(error) => failures.push(format!("отмена твиков и тем: {error}")),
    }
    if let Err(error) = platform::set_autostart(platform::AUTOSTART_APP, None) {
        failures.push(format!("автозапуск: {error}"));
    }
    if let Err(error) = assoc::unregister() {
        failures.push(format!("ассоциации файлов: {error}"));
    }
    if options.remove_data {
        for dir in data_dirs() {
            if let Err(error) = remove_dir(&dir) {
                failures.push(format!("{}: {error}", dir.display()));
            }
        }
    }

    if options.report {
        report(options, &failures, tweaks_undone);
    }
    for failure in &failures {
        eprintln!("cleanup: {failure}");
    }
    i32::from(!failures.is_empty())
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.extend(platform::appdata::user_dir());
    dirs.extend(dirs::data_local_dir().map(|local| local.join(WEBVIEW_DATA_DIR)));
    dirs
}

/// Removes `dir` if it exists. Retries for a moment: the processes that just
/// exited (the daemon, WebView2) can take a little while to let go of files.
fn remove_dir(dir: &Path) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) if attempt < 10 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(300));
            }
            Err(error) => return Err(error),
        }
    }
}

fn report(options: &Options, failures: &[String], tweaks_undone: usize) {
    if failures.is_empty() {
        let tweaks = if tweaks_undone > 0 {
            ", изменения твиков и тем отменены"
        } else {
            ""
        };
        let data = if options.remove_data {
            ", библиотеки обоев и тем и настройки удалены"
        } else {
            ""
        };
        let folder = std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.display().to_string()))
            .unwrap_or_default();
        platform::show_message(
            "LimeKit",
            &format!(
                "LimeKit убран из системы: обои остановлены, автозапуск и ассоциации \
                 файлов сняты{tweaks}{data}.\n\nТеперь можно удалить папку с программой:\n{folder}"
            ),
            false,
        );
    } else {
        platform::show_message(
            "LimeKit",
            &format!(
                "Убрать получилось не всё:\n\n- {}\n\nЗакройте LimeKit в трее и попробуйте ещё раз.",
                failures.join("\n- ")
            ),
            true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        std::iter::once("LimeKit.exe")
            .chain(list.iter().copied())
            .map(OsString::from)
            .collect()
    }

    #[test]
    fn a_normal_start_is_not_a_cleanup() {
        assert_eq!(from_args(args(&[])), None);
        assert_eq!(from_args(args(&[r"C:\Users\me\aurora.limekit"])), None);
    }

    #[test]
    fn reads_every_cleanup_option() {
        assert_eq!(
            from_args(args(&[
                "--cleanup",
                "--remove-data",
                "--after",
                "4242",
                "--report"
            ])),
            Some(Options {
                remove_data: true,
                after: Some(4242),
                report: true,
            })
        );
        assert_eq!(from_args(args(&["--cleanup"])), Some(Options::default()));
    }

    #[test]
    fn webview_data_folder_follows_the_app_identifier() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(config["identifier"], WEBVIEW_DATA_DIR);
    }
}
