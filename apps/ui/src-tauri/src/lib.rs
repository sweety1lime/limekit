mod assoc;
mod cleanup;
mod daemon_client;
mod diagnostics;
mod gallery;
mod library;
mod theme_panel;
mod tweak_panel;

use std::path::{Path, PathBuf};

use base64::Engine;

fn parse_quality(quality: &str) -> Result<ipc::Quality, String> {
    match quality {
        "eco" => Ok(ipc::Quality::Eco),
        "balanced" => Ok(ipc::Quality::Balanced),
        "max" => Ok(ipc::Quality::Max),
        other => Err(format!("unknown quality profile: {other}")),
    }
}

/// Connects to the daemon, starting it when needed; returns its version.
#[tauri::command]
fn daemon_connect() -> Result<String, String> {
    daemon_client::ensure_daemon(&daemon_client::endpoint())
}

#[tauri::command]
fn list_monitors() -> Result<Vec<ipc::Monitor>, String> {
    match daemon_client::request(&daemon_client::endpoint(), ipc::Command::ListMonitors)? {
        ipc::ResponseData::Monitors { monitors } => Ok(monitors),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

/// Session list plus the wallpaper stack's live CPU% and playlists, for the panel.
#[derive(serde::Serialize)]
struct DaemonStatus {
    sessions: Vec<ipc::SessionStatus>,
    stack_cpu_percent: Option<f32>,
    playlists: Vec<ipc::PlaylistSummary>,
}

#[tauri::command]
fn daemon_status() -> Result<DaemonStatus, String> {
    match daemon_client::request(&daemon_client::endpoint(), ipc::Command::Status)? {
        ipc::ResponseData::Status {
            sessions,
            stack_cpu_percent,
            playlists,
        } => Ok(DaemonStatus {
            sessions,
            stack_cpu_percent,
            playlists,
        }),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[tauri::command]
fn set_playlist(
    monitor: usize,
    items: Vec<String>,
    interval_minutes: u32,
    shuffle: bool,
    quality: String,
    volume: u8,
    anime4k: bool,
) -> Result<String, String> {
    let items = items
        .iter()
        .map(|s| {
            PathBuf::from(s)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(s))
        })
        .collect();
    acknowledged(ipc::Command::SetPlaylist {
        monitor,
        items,
        interval_minutes,
        shuffle,
        quality: parse_quality(&quality)?,
        volume,
        anime4k,
    })
}

#[tauri::command]
fn clear_playlist(monitor: Option<usize>) -> Result<String, String> {
    acknowledged(ipc::Command::ClearPlaylist { monitor })
}

#[tauri::command]
fn playlist_next(monitor: Option<usize>) -> Result<String, String> {
    acknowledged(ipc::Command::PlaylistNext { monitor })
}

#[tauri::command]
fn play(
    path: String,
    monitor: usize,
    quality: String,
    volume: u8,
    anime4k: bool,
) -> Result<String, String> {
    let path = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| format!("file not found: {error}"))?;
    let command = ipc::Command::Play {
        monitor,
        path,
        quality: parse_quality(&quality)?,
        volume,
        anime4k,
    };
    acknowledged(command)
}

#[tauri::command]
fn stop(monitor: Option<usize>) -> Result<String, String> {
    acknowledged(ipc::Command::Stop { monitor })
}

#[tauri::command]
fn pause(monitor: Option<usize>) -> Result<String, String> {
    acknowledged(ipc::Command::Pause { monitor })
}

#[tauri::command]
fn resume(monitor: Option<usize>) -> Result<String, String> {
    acknowledged(ipc::Command::Resume { monitor })
}

#[tauri::command]
fn set_volume(monitor: usize, volume: u8) -> Result<String, String> {
    acknowledged(ipc::Command::SetVolume { monitor, volume })
}

#[tauri::command]
fn set_quality(monitor: usize, quality: String, anime4k: bool) -> Result<String, String> {
    let command = ipc::Command::SetQuality {
        monitor,
        quality: parse_quality(&quality)?,
        anime4k,
    };
    acknowledged(command)
}

fn acknowledged(command: ipc::Command) -> Result<String, String> {
    match daemon_client::request(&daemon_client::endpoint(), command)? {
        ipc::ResponseData::Acknowledged { status } => Ok(status),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[tauri::command]
fn get_autostart() -> Result<bool, String> {
    match daemon_client::request(&daemon_client::endpoint(), ipc::Command::GetAutostart)? {
        ipc::ResponseData::Autostart { enabled } => Ok(enabled),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<String, String> {
    acknowledged(ipc::Command::SetAutostart { enabled })
}

/// Hands over to `LimeKit.exe --cleanup` and closes the window. The cleanup
/// waits for this process to exit first: it holds the WebView2 data open and
/// would register the file association again on its way.
#[tauri::command]
fn remove_from_system(app: tauri::AppHandle, remove_data: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = std::process::Command::new(exe);
    command
        .args(["--cleanup", "--report", "--after"])
        .arg(std::process::id().to_string());
    if remove_data {
        command.arg("--remove-data");
    }
    command
        .spawn()
        .map_err(|error| format!("failed to start the cleanup: {error}"))?;
    app.exit(0);
    Ok(())
}

fn parse_battery_policy(policy: &str) -> Result<ipc::BatteryPolicy, String> {
    match policy {
        "pause" => Ok(ipc::BatteryPolicy::Pause),
        "eco" => Ok(ipc::BatteryPolicy::Eco),
        "keep" => Ok(ipc::BatteryPolicy::Keep),
        other => Err(format!("unknown battery policy: {other}")),
    }
}

#[tauri::command]
fn get_battery_policy() -> Result<String, String> {
    match daemon_client::request(&daemon_client::endpoint(), ipc::Command::GetBatteryPolicy)? {
        ipc::ResponseData::BatteryPolicy { policy } => Ok(match policy {
            ipc::BatteryPolicy::Pause => "pause".into(),
            ipc::BatteryPolicy::Eco => "eco".into(),
            ipc::BatteryPolicy::Keep => "keep".into(),
        }),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[tauri::command]
fn set_battery_policy(policy: String) -> Result<String, String> {
    acknowledged(ipc::Command::SetBatteryPolicy {
        policy: parse_battery_policy(&policy)?,
    })
}

// ---------------------------------------------------------------------------
// library commands (import runs ffmpeg — keep it off the UI thread)
// ---------------------------------------------------------------------------

pub(crate) async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_list() -> Result<Vec<library::LibraryItem>, String> {
    blocking(|| library::Library::default_location()?.list()).await
}

#[tauri::command]
async fn library_import(path: String) -> Result<library::LibraryItem, String> {
    blocking(move || library::Library::default_location()?.import(Path::new(&path))).await
}

#[tauri::command]
async fn library_remove(id: String) -> Result<(), String> {
    blocking(move || library::Library::default_location()?.remove(&id)).await
}

/// Preview as base64 jpeg; small enough to travel over invoke.
#[tauri::command]
async fn library_preview(id: String) -> Result<String, String> {
    let bytes = blocking(move || library::Library::default_location()?.preview_jpeg(&id)).await?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Packs a library item into a package file at the chosen location.
#[tauri::command]
async fn library_export(id: String, target: String) -> Result<(), String> {
    blocking(move || library::Library::default_location()?.export(&id, Path::new(&target))).await
}

#[derive(Clone, serde::Serialize)]
struct PackageInfo {
    name: String,
    kind: String,
}

fn kind_str(kind: wpk::MediaType) -> &'static str {
    match kind {
        wpk::MediaType::Video => "video",
        wpk::MediaType::Image => "image",
        wpk::MediaType::Web => "web",
        wpk::MediaType::Model3d => "model3d",
    }
}

/// web / 3D packages ship code that runs on the desktop, so they must never be
/// installed without the user's explicit consent (see docs/research/security-model.md).
fn needs_consent(kind: wpk::MediaType) -> bool {
    matches!(kind, wpk::MediaType::Web | wpk::MediaType::Model3d)
}

/// Reads a package's manifest so the UI can decide whether to warn before import.
#[tauri::command]
async fn inspect_package(path: String) -> Result<PackageInfo, String> {
    blocking(move || {
        let manifest = wpk::read_manifest(Path::new(&path)).map_err(|error| error.to_string())?;
        Ok(PackageInfo {
            name: manifest.name,
            kind: kind_str(manifest.media_type).to_owned(),
        })
    })
    .await
}

/// Locates the bundled first-run sample web wallpaper: portable/installed
/// layout first (`web/demo` next to the exe), then the dev tree. Mirrors
/// library's two-tier asset lookup.
fn bundled_sample_entry() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("web").join("demo").join("index.html"));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../assets/web/demo/index.html"),
    );
    candidates.into_iter().find(|path| path.is_file())
}

/// What an action that would create the data directory says while the LimeWall
/// one has not been moved over yet.
const MIGRATION_PENDING: &str = "библиотека LimeWall ещё не перенесена: закройте старую версию через трей и перезапустите LimeKit";

/// Imports the bundled first-party sample wallpaper so a fresh library is not
/// empty. It is our own code shipped inside the install, so it skips the
/// code-wallpaper consent that foreign `.html`/`.wpk` imports require.
#[tauri::command]
async fn import_bundled_sample() -> Result<library::LibraryItem, String> {
    blocking(|| {
        // The library looks empty because the LimeWall one has not moved yet.
        // Seeding it now would make the half-created directory look real, and
        // the old library would never be brought over.
        if platform::appdata::migration_pending() {
            return Err(MIGRATION_PENDING.to_owned());
        }
        let entry = bundled_sample_entry().ok_or("bundled sample wallpaper not found")?;
        library::Library::default_location()?.import_web_folder_meta(
            &entry,
            Some("Аврора — пример".to_owned()),
            Some("2fame".to_owned()),
        )
    })
    .await
}

/// Package files among command line arguments (double-click / "open with").
fn packages_in(args: impl Iterator<Item = std::ffi::OsString>) -> Vec<std::path::PathBuf> {
    args.skip(1)
        .map(std::path::PathBuf::from)
        .filter(|path| wpk::has_package_extension(path) && path.is_file())
        .collect()
}

/// Theme packages among command line arguments (double-click / "open with").
fn theme_packages_in(args: impl Iterator<Item = std::ffi::OsString>) -> Vec<std::path::PathBuf> {
    args.skip(1)
        .map(std::path::PathBuf::from)
        .filter(|path| themes::has_theme_extension(path) && path.is_file())
        .collect()
}

/// Imports theme packages off the UI thread and tells the panel what came of
/// it. A theme is data, so unlike a code wallpaper it needs no consent: nothing
/// changes in Windows until the theme is applied.
fn import_theme_packages(app: tauri::AppHandle, packages: Vec<std::path::PathBuf>) {
    use tauri::Emitter;
    if packages.is_empty() {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        for package in packages {
            match theme_panel::import_path(&package) {
                Ok(imported) => {
                    println!(
                        "imported theme {} from {}",
                        imported.name,
                        package.display()
                    );
                    let _ = app.emit("themes-changed", imported);
                }
                Err(message) => {
                    eprintln!("failed to import {}: {message}", package.display());
                    let _ = app.emit("themes-import-error", message);
                }
            }
        }
    });
}

#[derive(Clone, serde::Serialize)]
struct ConsentRequest {
    path: String,
    name: String,
    kind: String,
}

/// Imports packages off the UI thread and tells the panel to refresh.
///
/// Plain media (video / image) is imported directly — double-clicking a video
/// `.wpk` just works. Code-bearing packages (web / 3D) and any package whose
/// manifest cannot be read are never imported silently: the UI is asked for
/// explicit consent first (fail-safe — a missed event means no import, not a
/// silent one).
fn import_packages(app: tauri::AppHandle, packages: Vec<std::path::PathBuf>) {
    use tauri::Emitter;
    if packages.is_empty() {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let library = match library::Library::default_location() {
            Ok(library) => library,
            Err(error) => {
                eprintln!("library unavailable: {error}");
                return;
            }
        };
        let mut imported = false;
        for package in packages {
            let consent = match wpk::read_manifest(&package) {
                Ok(manifest) if needs_consent(manifest.media_type) => Some(ConsentRequest {
                    path: package.display().to_string(),
                    name: manifest.name,
                    kind: kind_str(manifest.media_type).to_owned(),
                }),
                Ok(_) => None,
                Err(error) => {
                    eprintln!("cannot classify {}: {error}", package.display());
                    Some(ConsentRequest {
                        path: package.display().to_string(),
                        name: package
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("пакет")
                            .to_owned(),
                        kind: "unknown".to_owned(),
                    })
                }
            };
            if let Some(request) = consent {
                let _ = app.emit("wpk-consent", request);
                continue;
            }
            match library.import(&package) {
                Ok(item) => {
                    println!("imported {} from {}", item.name, package.display());
                    imported = true;
                }
                Err(error) => eprintln!("failed to import {}: {error}", package.display()),
            }
        }
        if imported {
            let _ = app.emit("library-changed", ());
        }
    });
}

/// Shrinks the window to the work area of its monitor and centers it there.
/// 1200×800 is taller than a 1080p laptop screen at 150% scale, 720 logical
/// pixels less the taskbar, and Windows would put the window's top, with the
/// navigation, under a taskbar at the top of the screen.
fn fit_to_work_area(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let outer = window.outer_size()?;
    let inner = window.inner_size()?;
    let width = outer.width.min(area.size.width);
    let height = outer.height.min(area.size.height);
    // set_size sizes the client area. The frame comes on top of it: invisible
    // without decorations, but still there for resizing.
    window.set_size(tauri::PhysicalSize::new(
        width - (outer.width - inner.width),
        height - (outer.height - inner.height),
    ))?;
    window.set_position(tauri::PhysicalPosition::new(
        area.position.x + ((area.size.width - width) / 2) as i32,
        area.position.y + ((area.size.height - height) / 2) as i32,
    ))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // `LimeKit.exe --cleanup` runs without a window. It has to branch off
    // before the migration below, and before the single-instance plugin, which
    // would hand it to the window that started it and exit.
    if let Some(options) = cleanup::from_args(std::env::args_os()) {
        std::process::exit(cleanup::run(&options));
    }
    // Before the library, the log or the daemon can create the data directory:
    // see platform::appdata. An upgrading tester usually still has the LimeWall
    // daemon in the tray, holding its log open inside the directory to be
    // moved, so ask it to quit first. Whichever of the UI and the daemon starts
    // first does the move; the other finds it done.
    if platform::appdata::migration_pending() {
        let stopped = ipc::stop_legacy_daemon(std::time::Duration::from_secs(3));
        if stopped != ipc::DaemonStop::NotRunning {
            eprintln!("old LimeWall daemon: {stopped:?}");
        }
    }
    let migration = platform::appdata::migrate_user_data();
    if !matches!(migration, platform::appdata::Migration::NothingToDo) {
        eprintln!("data directory: {migration}");
    }
    tauri::Builder::default()
        // Must come first: a second launch (e.g. double-clicking a .wpk)
        // hands its arguments to the running instance and exits.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            let argv: Vec<std::ffi::OsString> =
                argv.into_iter().map(std::ffi::OsString::from).collect();
            import_packages(app.clone(), packages_in(argv.iter().cloned()));
            import_theme_packages(app.clone(), theme_packages_in(argv.into_iter()));
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(tweak_panel::TweakEngine::default())
        .invoke_handler(tauri::generate_handler![
            daemon_connect,
            list_monitors,
            daemon_status,
            play,
            stop,
            pause,
            resume,
            set_volume,
            set_quality,
            get_autostart,
            set_autostart,
            remove_from_system,
            get_battery_policy,
            set_battery_policy,
            library_list,
            library_import,
            library_remove,
            library_preview,
            library_export,
            inspect_package,
            import_bundled_sample,
            diagnostics::run_diagnostics,
            set_playlist,
            clear_playlist,
            playlist_next,
            gallery::gallery_fetch_catalog,
            gallery::gallery_download,
            gallery::gallery_apply_revocations,
            tweak_panel::tweaks_view,
            tweak_panel::tweak_preview,
            tweak_panel::tweak_switch,
            tweak_panel::tweaks_undo_all,
            tweak_panel::tweaks_export_undo,
            tweak_panel::tweak_open_settings,
            tweak_panel::restart_explorer,
            theme_panel::themes_view,
            theme_panel::theme_cursor_image,
            theme_panel::theme_preview,
            theme_panel::theme_switch,
            theme_panel::theme_import,
            theme_panel::theme_remove,
        ])
        .setup(|app| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = fit_to_work_area(&window) {
                    eprintln!("window not fitted to the screen: {error}");
                }
                window.show()?;
            }
            // Double-clicking a .wpk should land here (per-user, no admin).
            if let Err(error) = assoc::register() {
                eprintln!("wpk association not registered: {error}");
            }
            // The theme association exists only once a theme has been imported.
            if theme_panel::library_exists()
                && let Err(error) = assoc::register_theme()
            {
                eprintln!("theme association not registered: {error}");
            }
            import_packages(app.handle().clone(), packages_in(std::env::args_os()));
            import_theme_packages(app.handle().clone(), theme_packages_in(std::env::args_os()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_code_bearing_packages_require_consent() {
        // web / 3D run code on the desktop -> must ask the user first.
        assert!(needs_consent(wpk::MediaType::Web));
        assert!(needs_consent(wpk::MediaType::Model3d));
        // plain media is inert -> import silently.
        assert!(!needs_consent(wpk::MediaType::Video));
        assert!(!needs_consent(wpk::MediaType::Image));
    }
}
