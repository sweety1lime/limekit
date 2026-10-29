//! The Windows shell side of a changed setting: telling open windows about it,
//! making Windows read cursors again, and restarting Explorer for the settings
//! it reads only when it starts.

/// Broadcasts `WM_SETTINGCHANGE` for `area`, such as `ImmersiveColorSet`, the
/// way Windows does after its own settings change. Windows that hang are
/// skipped rather than waited for.
pub fn announce_setting_change(area: &str) {
    #[cfg(windows)]
    win32::announce_setting_change(area);
    #[cfg(not(windows))]
    let _ = area;
}

/// Makes Windows read the cursors from the registry again, so a changed cursor
/// shows at once and not at the next sign-in.
pub fn reload_cursors() -> Result<(), String> {
    #[cfg(windows)]
    {
        win32::reload_cursors()
    }
    #[cfg(not(windows))]
    {
        Err("cursors exist only on Windows".into())
    }
}

/// Resolves a string that points into a resource file, such as
/// `@%SystemRoot%\system32\main.cpl,-1020`, to the text Windows shows in the
/// current language. `None` for anything else, or when Windows cannot find it.
pub fn indirect_string(reference: &str) -> Option<String> {
    #[cfg(windows)]
    {
        win32::indirect_string(reference)
    }
    #[cfg(not(windows))]
    {
        let _ = reference;
        None
    }
}

/// Shuts Explorer down and starts it again through the Restart Manager, which
/// lets it save its state on the way out, the way installers restart it.
/// Starts Explorer by hand if the Restart Manager did not bring it back.
pub fn restart_explorer() -> Result<(), String> {
    #[cfg(windows)]
    {
        win32::restart_explorer()
    }
    #[cfg(not(windows))]
    {
        Err("Explorer exists only on Windows".into())
    }
}

#[cfg(windows)]
mod win32 {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{CloseHandle, FILETIME, HWND, LPARAM, WPARAM};
    use windows::Win32::System::RestartManager::{
        CCH_RM_SESSION_KEY, RM_UNIQUE_PROCESS, RmEndSession, RmForceShutdown, RmRegisterResources,
        RmRestart, RmShutdown, RmStartSession,
    };
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Shell::SHLoadIndirectString;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetWindowThreadProcessId, HWND_BROADCAST, SMTO_ABORTIFHUNG, SPI_SETCURSORS,
        SPIF_SENDCHANGE, SendMessageTimeoutW, SystemParametersInfoW, WM_SETTINGCHANGE,
    };
    use windows::core::{HSTRING, PCWSTR, PWSTR, w};

    /// How long Explorer gets to show its taskbar again before LimeKit starts
    /// it itself.
    const COMEBACK: Duration = Duration::from_secs(10);

    pub fn announce_setting_change(area: &str) {
        let area = HSTRING::from(area);
        // SAFETY: `area` stays alive until every window has had the message
        // or timed out; the call returns only then.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(area.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                1000,
                None,
            );
        }
    }

    pub fn reload_cursors() -> Result<(), String> {
        // SAFETY: SPI_SETCURSORS takes neither a value nor a pointer.
        unsafe { SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_SENDCHANGE) }
            .map_err(|error| format!("cursors were not reloaded: {error}"))
    }

    pub fn indirect_string(reference: &str) -> Option<String> {
        if !reference.starts_with('@') {
            return None;
        }
        let source = HSTRING::from(reference);
        let mut buffer = [0u16; 512];
        // SAFETY: `source` is NUL-terminated and outlives the call, and the
        // buffer is writable for its whole length.
        unsafe { SHLoadIndirectString(&source, &mut buffer, None) }.ok()?;
        let end = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..end]);
        (!text.is_empty()).then_some(text)
    }

    /// The taskbar window, which belongs to the Explorer process that runs the
    /// shell. Folder windows may live in other Explorer processes; they close
    /// with it and are not what needs restarting.
    fn taskbar() -> Option<HWND> {
        // SAFETY: plain lookup by class name.
        unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.ok()
    }

    fn shell_process() -> Result<RM_UNIQUE_PROCESS, String> {
        let window = taskbar().ok_or("Explorer is not running")?;
        let mut pid = 0;
        // SAFETY: `pid` outlives the call.
        unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
        // SAFETY: the handle is closed below whatever GetProcessTimes says.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map_err(|error| format!("cannot open Explorer: {error}"))?;
        let mut created = FILETIME::default();
        let mut unused = [FILETIME::default(); 3];
        let [exited, kernel, user] = &mut unused;
        // SAFETY: all four pointers are to live FILETIMEs.
        let times = unsafe { GetProcessTimes(process, &mut created, exited, kernel, user) };
        // SAFETY: `process` came from OpenProcess and is not used afterwards.
        let _ = unsafe { CloseHandle(process) };
        times.map_err(|error| format!("cannot read Explorer's start time: {error}"))?;
        // The start time keeps the Restart Manager off a process that reused
        // the id after Explorer exited.
        Ok(RM_UNIQUE_PROCESS {
            dwProcessId: pid,
            ProcessStartTime: created,
        })
    }

    pub fn restart_explorer() -> Result<(), String> {
        let shell = shell_process()?;
        let mut session = 0;
        let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
        // SAFETY: `key` has room for the session key and its NUL.
        unsafe { RmStartSession(&mut session, None, PWSTR(key.as_mut_ptr())) }
            .ok()
            .map_err(|error| format!("Restart Manager: {error}"))?;
        let restarted = restart_in(session, shell);
        // SAFETY: the session was started above and is not used afterwards.
        let _ = unsafe { RmEndSession(session) };

        // Whatever the Restart Manager managed, the shell must not stay down.
        let deadline = Instant::now() + COMEBACK;
        while Instant::now() < deadline {
            if taskbar().is_some() {
                return restarted;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let windows_dir = std::env::var_os("SystemRoot").ok_or("SystemRoot is not set")?;
        std::process::Command::new(PathBuf::from(windows_dir).join("explorer.exe"))
            .spawn()
            .map(drop)
            .map_err(|error| format!("Explorer did not come back and cannot be started: {error}"))
    }

    fn restart_in(session: u32, shell: RM_UNIQUE_PROCESS) -> Result<(), String> {
        let error = |step: &str, code: windows::Win32::Foundation::WIN32_ERROR| {
            format!(
                "Restart Manager, {step}: {}",
                windows::core::Error::from(code)
            )
        };
        // SAFETY: the session is open and the slice outlives the call.
        let code = unsafe { RmRegisterResources(session, None, Some(&[shell]), None) };
        code.ok().map_err(|_| error("register", code))?;
        // SAFETY: as above; no progress callback.
        let code = unsafe { RmShutdown(session, RmForceShutdown.0 as u32, None) };
        code.ok().map_err(|_| error("shut down", code))?;
        // SAFETY: as above.
        let code = unsafe { RmRestart(session, None, None) };
        code.ok().map_err(|_| error("restart", code))
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    #[test]
    fn resolves_a_reference_into_a_resource_file() {
        let name = indirect_string(r"@%SystemRoot%\system32\main.cpl,-1020")
            .expect("main.cpl holds the name of the default cursor scheme");
        assert!(!name.is_empty() && !name.starts_with('@'), "{name}");
    }

    #[test]
    fn plain_text_is_not_a_reference() {
        assert_eq!(indirect_string("Windows Aero"), None);
    }
}
