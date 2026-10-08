//! Windowless entry point for the wallpaper daemon, used by autostart.
//!
//! `renderer.exe` is a console application, so launching it from the Run key at
//! logon gives it a console window: it pops up in the user's face, and closing
//! it kills the daemon. This binary runs the very same daemon in the windows
//! subsystem — no console, nothing to close — and leaves `renderer.exe` a
//! proper CLI with working stdout and stdin.
#![windows_subsystem = "windows"]

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "limekit-daemon",
    about = "LimeKit wallpaper daemon (no console window)",
    version
)]
struct Cli {
    /// IPC endpoint to listen on (default: the per-user LimeKit endpoint).
    #[arg(long)]
    endpoint: Option<String>,
    /// Wallpaper state file (default: %APPDATA%/LimeKit/wallpapers.json).
    #[arg(long)]
    state: Option<PathBuf>,
    /// Log file (default: %APPDATA%/LimeKit/daemon.log).
    #[arg(long)]
    log: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Before anything creates the data directory: opening the log below would
    // make %APPDATA%/LimeKit exist and the pre-rename library would be left
    // behind. Keep the outcome and report it once there is somewhere to write.
    let migration = platform::appdata::migrate_user_data();
    // Without a console there is nowhere for the daemon's output to go, and a
    // silent daemon cannot be diagnosed from a bug report. Redirect first, so
    // even a failing start leaves a trace. A fresh file per start, matching the
    // log the UI keeps when it spawns the daemon itself.
    if let Some(log) = cli.log.or_else(default_log_path)
        && let Err(error) = platform::redirect_output_to_file(&log)
    {
        eprintln!("logging to file disabled: {error}");
    }
    if !matches!(migration, platform::appdata::Migration::NothingToDo) {
        println!("data directory: {migration}");
    }
    renderer::daemon::run(cli.endpoint.as_deref(), cli.state.as_deref())
}

fn default_log_path() -> Option<PathBuf> {
    Some(platform::appdata::user_dir()?.join("daemon.log"))
}
